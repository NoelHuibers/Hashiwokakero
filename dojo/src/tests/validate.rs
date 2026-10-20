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

#[test]
fn compilation_work_is_bounded_before_validation_and_codegen() {
    let body = "nop ".repeat(100_001);
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func (param i32)))
        (func (export "_start") {body}))"#
    ))
    .unwrap();
    let Err(Rejection::Invalid(message)) = validate(&wasm) else {
        panic!("oversized function accepted");
    };
    assert!(message.contains("complexity limit"));
    let locals = "i32 ".repeat(50_001);
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func (param i32)))
        (func (export "_start") (local {locals})))"#
    ))
    .unwrap();
    assert!(validate(&wasm).is_err());
}
