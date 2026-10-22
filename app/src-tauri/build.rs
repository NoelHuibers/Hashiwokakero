//! Tauri's build step: reads `tauri.conf.json` and the capabilities, and generates the
//! permission schemas in `gen/schemas`.

fn main() {
    tauri_build::build();
}
