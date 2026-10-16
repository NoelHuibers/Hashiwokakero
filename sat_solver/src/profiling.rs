use std::sync::atomic::{AtomicBool, Ordering};

// flame keeps every recorded span in memory until it is dumped. The solvers open spans in their
// hot loops, so recording unconditionally grows without bound on hard instances (GBs in seconds).
// Spans are therefore only recorded once profiling has been enabled, i.e. with --flamegraph.
// flame doesn't build for wasm, so it's behind the (default) flamegraph feature.
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable() {
    if cfg!(feature = "flamegraph") {
        ENABLED.store(true, Ordering::Relaxed);
    } else {
        eprintln!("Built without the flamegraph feature, no flamegraph will be created.");
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

#[inline]
pub fn start(name: &'static str) {
    #[cfg(feature = "flamegraph")]
    if is_enabled() {
        flame::start(name);
    }
    #[cfg(not(feature = "flamegraph"))]
    let _ = name;
}

#[inline]
pub fn end(name: &'static str) {
    #[cfg(feature = "flamegraph")]
    if is_enabled() {
        flame::end(name);
    }
    #[cfg(not(feature = "flamegraph"))]
    let _ = name;
}

pub fn dump_html(path: &str) -> std::io::Result<()> {
    #[cfg(feature = "flamegraph")]
    if is_enabled() {
        flame::dump_html(std::fs::File::create(path)?)?;
    }
    #[cfg(not(feature = "flamegraph"))]
    let _ = path;
    Ok(())
}
