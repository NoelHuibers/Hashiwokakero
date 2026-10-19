//! Record the actual locked engine, including patch updates, in every evaluation version.
fn main() {
    println!("cargo::rerun-if-changed=../Cargo.lock");
    let lock = include_str!("../Cargo.lock");
    let engine = lock
        .split("[[package]]")
        .find(|package| package.lines().any(|line| line == "name = \"wasmtime\""))
        .expect("wasmtime is locked in the workspace");
    let version = engine
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|v| v.strip_suffix('"'))
        })
        .expect("wasmtime has a version");
    println!("cargo::rustc-env=DOJO_ENGINE_VERSION={version}");
}
