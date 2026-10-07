//! The app's millisecond clock, in one place for both hosts (issue #71).
//!
//! The wasm build reads `performance.now()`; the native binary has no `performance`, so it reads a
//! monotonic `Instant` taken at the first call. Both are relative to their own process starting, which
//! is what every reader of this clock wants: the first-frame and interactive-load timestamps
//! ([`crate::app::FirstFrame`]) are published as *deltas*, never as wall-clock times.

/// Milliseconds since this process started (wasm: since the page's time origin).
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// Milliseconds since this process started.
#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}
