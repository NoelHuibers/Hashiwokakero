//! Submissions refused before anything is compiled: too large, or not using WASI at all (it
//! couldn't print an answer, and a module without imports would just burn fuel).

use super::validate;
use crate::{MAX_WASM_BYTES, Rejection};

#[test]
fn oversized_modules_are_refused_unread() {
    // Not even WebAssembly: the size is checked first
    assert_eq!(
        validate(&vec![0; MAX_WASM_BYTES + 1]),
        Err(Rejection::TooLarge)
    );
}

#[test]
fn modules_without_wasi_are_refused() {
    let silent =
        wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "_start")))"#)
            .unwrap();
    assert_eq!(
        validate(&silent),
        Err(Rejection::Invalid(
            "the module doesn't use WASI, so it can't print an answer".into()
        ))
    );
}
