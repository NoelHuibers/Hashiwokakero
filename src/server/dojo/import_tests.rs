//! Bound hostile imports before allocating their requested dictionary or reading to EOF.

use super::*;
use crate::server::dojo::benchmark_tests::compress;

#[test]
fn decoded_text_and_concatenated_streams_share_one_limit() {
    let exact = vec![b' '; community::MAX_BYTES];
    assert_eq!(decompress(&exact).unwrap().len(), community::MAX_BYTES);
    assert_eq!(
        decompress(&compress(&exact)).unwrap().len(),
        community::MAX_BYTES
    );
    let mut concatenated = compress(&exact);
    concatenated.extend(compress(b"x"));
    assert_eq!(decompress(&concatenated), Err(BenchmarkError::TooLarge));
    let bomb = compress(&vec![b'c'; 16 << 20]);
    assert!(bomb.len() < 4096);
    assert_eq!(decompress(&bomb), Err(BenchmarkError::TooLarge));
    assert_eq!(
        decompress(&vec![b' '; community::MAX_BYTES + 1]),
        Err(BenchmarkError::TooLarge)
    );
    assert_eq!(
        decompress(&vec![0; MAX_DOWNLOAD + 1]),
        Err(BenchmarkError::TooLarge)
    );
    let mut streams = compress(b"one");
    streams.extend(compress(b"two"));
    assert_eq!(decompress(&streams).unwrap(), "onetwo");
}

#[test]
fn oversized_dictionary_is_rejected_before_reading_block_data() {
    // Python lzma preset 0, with the LZMA2 dictionary property changed to 4 GiB
    // and the block header CRC32 recalculated. The payload itself is only 14 bytes.
    let xz = [
        253, 55, 122, 88, 90, 0, 0, 4, 230, 214, 180, 70, 2, 0, 33, 1, 40, 0, 0, 0, 230, 160, 17,
        179, 1, 0, 13, 112, 32, 99, 110, 102, 32, 49, 32, 49, 10, 49, 32, 48, 10, 0, 0, 0, 243, 13,
        183, 114, 17, 234, 214, 96, 0, 1, 38, 14, 8, 27, 224, 4, 31, 182, 243, 125, 1, 0, 0, 0, 0,
        4, 89, 90,
    ];
    let mut cursor = std::io::Cursor::new(xz);
    let mut reader = lzma_rust2::XzReader::new_mem_limit(&mut cursor, true, DECODER_MEMORY_KIB);
    assert_eq!(
        reader.read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::OutOfMemory
    );
    assert_eq!(
        cursor.position(),
        24,
        "only the stream and block headers were read"
    );
    assert_eq!(decompress(&xz), Err(BenchmarkError::TooLarge));
}

#[test]
fn corrupt_truncated_and_non_utf8_imports_are_invalid() {
    let valid = compress(b"p cnf 1 1\n1 0\n");
    assert_eq!(decompress(&valid).unwrap(), "p cnf 1 1\n1 0\n");
    let mut corrupted = valid.clone();
    // The block checksum is followed by the index and footer (24 bytes for this fixture).
    let checksum = corrupted.len() - 25;
    corrupted[checksum] ^= 1;
    assert_eq!(decompress(&corrupted), Err(BenchmarkError::Invalid));
    for length in [6, 12, valid.len() - 1] {
        assert_eq!(decompress(&valid[..length]), Err(BenchmarkError::Invalid));
    }
    assert_eq!(decompress(&compress(&[0xff])), Err(BenchmarkError::Invalid));
    assert_eq!(decompress(&[0xff]), Err(BenchmarkError::Invalid));
}

#[tokio::test]
async fn downloads_stop_at_the_limit_without_waiting_for_eof() {
    use axum::{body::Body, routing::get};
    use futures::{StreamExt as _, stream};

    let app = axum::Router::new().route(
        "/file/{instance}",
        get(
            |axum::extract::Path(hash): axum::extract::Path<String>| async move {
                let over = hash.starts_with('b');
                let declared = hash.starts_with('c');
                let chunks =
                    stream::iter((0..256).map(|_| Ok::<_, std::io::Error>(vec![b'x'; 64 << 10])));
                let tail = stream::once(async { Ok(vec![b'x']) }).chain(stream::pending());
                let body = if over {
                    Body::from_stream(chunks.chain(tail))
                } else if declared {
                    Body::from_stream(stream::pending::<Result<Vec<u8>, std::io::Error>>())
                } else {
                    Body::from_stream(chunks)
                };
                let mut response = body.into_response();
                if declared {
                    response.headers_mut().insert(
                        header::CONTENT_LENGTH,
                        (MAX_DOWNLOAD + 1).to_string().parse().unwrap(),
                    );
                }
                response
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let exact = fetch_gbd(&url, &"a".repeat(32)).await.unwrap();
    assert_eq!(exact.len(), MAX_DOWNLOAD);
    for hash in ['b', 'c'] {
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            fetch_gbd(&url, &hash.to_string().repeat(32)),
        )
        .await;
        assert_eq!(result.unwrap(), Err(BenchmarkError::TooLarge));
    }
    server.abort();
}
