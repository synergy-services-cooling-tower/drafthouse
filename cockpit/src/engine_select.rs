//! The engine selection (issue #58): which `Engine` the instrument runs on.
//!
//! The cockpit asks for an engine, not for a type: [`build`] returns a `Box<dyn Engine>` (the
//! contract's trait) and every system in this crate calls it through that trait. Two engines are
//! compiled in here, and a build carries the one its features select:
//!
//! * `real-engine` (**on by default**): `cockpit-adapter`'s `RealEngine` - the repository's Rust
//!   engine behind the same contract. Its catalog is built from the fixture asset's own `catalog`
//!   block ([`crate::engine_catalog`]), so the records the UI lists and the records the engine
//!   selects over are the same text.
//! * `fixture-engine`: the recorded replay, `FixtureEngine`, read from the fixture asset. Demos and
//!   tests that must run without the engine crate build with
//!   `--no-default-features --features fixture-engine`; a build that carries both picks either at
//!   run time with `?engine=real` / `?engine=fixture`, which is also how an evidence frame states
//!   which engine it ran on (the output's provenance line carries the engine's own name).
//!
//! `?engine=unavailable` is not an engine at all: the page stages a missing fixture asset, the load
//! fails and the app draws the labelled engine-unavailable state - the deployment-shaped tests' own
//! case, unchanged by this wiring.

use cockpit::engine::Engine;

/// Build the engine for this build's features, or the one the page asked for.
pub fn build(fixture_text: &str, requested: Option<&str>) -> Result<Box<dyn Engine>, String> {
    match requested {
        Some("fixture") => fixture(fixture_text),
        Some("real") => real(fixture_text),
        _ => default_engine(fixture_text),
    }
}

/// The real engine, when this build carries it.
#[cfg(feature = "real-engine")]
fn real(fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    let catalog = crate::engine_catalog::catalog_from_fixture(fixture_text)?;
    Ok(Box::new(crate::adapter_bridge::AdapterEngine::new(catalog)))
}

#[cfg(not(feature = "real-engine"))]
fn real(_fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    Err("`?engine=real` needs the `real-engine` feature, which this build does not carry".into())
}

/// The recorded replay, when this build carries it.
#[cfg(feature = "fixture-engine")]
fn fixture(fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    Ok(Box::new(cockpit::fixture_engine::FixtureEngine::from_json(
        fixture_text,
    )?))
}

#[cfg(not(feature = "fixture-engine"))]
fn fixture(_fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    Err(
        "`?engine=fixture` needs the `fixture-engine` feature, which this build does not carry"
            .into(),
    )
}

#[cfg(feature = "real-engine")]
fn default_engine(fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    real(fixture_text)
}

#[cfg(all(not(feature = "real-engine"), feature = "fixture-engine"))]
fn default_engine(fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    fixture(fixture_text)
}

#[cfg(not(any(feature = "real-engine", feature = "fixture-engine")))]
fn default_engine(_fixture_text: &str) -> Result<Box<dyn Engine>, String> {
    Err(
        "no engine is compiled into this build: enable `real-engine` (default) or `fixture-engine`"
            .into(),
    )
}
