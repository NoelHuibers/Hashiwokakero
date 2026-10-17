//! Checking a submission before it is compiled: valid WebAssembly without threads, 64-bit or
//! multiple memories, importing nothing but WASI preview 1, with a `_start` function.

use wasmparser::{Parser, Payload};

use crate::Rejection;

pub const WASI: &str = "wasi_snapshot_preview1";

/// Checks that a submission is a WASI command the sandbox can run.
pub fn validate(wasm: &[u8]) -> Result<(), Rejection> {
    if wasm.len() > crate::MAX_WASM_BYTES {
        return Err(Rejection::TooLarge);
    }
    let invalid = |e: wasmparser::BinaryReaderError| Rejection::Invalid(e.message().to_string());
    // Validated with the features the sandbox allows (no threads, no 64-bit memories, ...)
    let mut features = wasmparser::WasmFeatures::default();
    features.remove(wasmparser::WasmFeatures::THREADS | wasmparser::WasmFeatures::SHARED_EVERYTHING_THREADS);
    features.remove(wasmparser::WasmFeatures::MEMORY64 | wasmparser::WasmFeatures::MULTI_MEMORY);
    features.remove(wasmparser::WasmFeatures::COMPONENT_MODEL);
    wasmparser::Validator::new_with_features(features).validate_all(wasm).map_err(invalid)?;

    let (mut types, mut has_imports, mut has_start) = (0u32, false, false);
    for payload in Parser::new(0).parse_all(wasm) {
        match payload.map_err(invalid)? {
            Payload::TypeSection(section) => {
                for group in section {
                    types += group.map_err(invalid)?.types().len() as u32;
                }
            }
            Payload::ImportSection(section) => {
                has_imports = true;
                for import in section {
                    let import = import.map_err(invalid)?;
                    if import.module != WASI {
                        return Err(Rejection::Import(format!("{}.{}", import.module, import.name)));
                    }
                }
            }
            Payload::ExportSection(section) => {
                for export in section {
                    let export = export.map_err(invalid)?;
                    has_start |= export.name == "_start" && export.kind == wasmparser::ExternalKind::Func;
                }
            }
            _ => {}
        }
    }
    if !has_start {
        return Err(Rejection::NoStart);
    }
    if !has_imports || types == 0 {
        return Err(Rejection::Invalid("the module doesn't use WASI, so it can't print an answer".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = r#"(import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory 1) (export "memory" (memory 0))"#;

    #[test]
    fn accepts_a_wasi_command() {
        let wasm = wat::parse_str(format!(
            r#"(module {START} (func (export "_start") (call $exit (i32.const 0))))"#
        ))
        .unwrap();
        assert_eq!(validate(&wasm), Ok(()));
    }

    #[test]
    fn rejects_bad_submissions() {
        assert!(matches!(validate(b"not wasm"), Err(Rejection::Invalid(_))));
        let other = wat::parse_str(
            r#"(module (import "env" "f" (func)) (func (export "_start")))"#,
        )
        .unwrap();
        assert_eq!(validate(&other), Err(Rejection::Import("env.f".into())));
        let library = wat::parse_str(format!(r#"(module {START} (func (export "solve")))"#)).unwrap();
        assert_eq!(validate(&library), Err(Rejection::NoStart));
        let threads = wat::parse_str(
            r#"(module (import "wasi_snapshot_preview1" "proc_exit" (func (param i32)))
                (memory 1 1 shared) (func (export "_start")))"#,
        )
        .unwrap();
        assert!(matches!(validate(&threads), Err(Rejection::Invalid(_))));
    }
}
