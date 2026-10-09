//! The engine selection (issue #58): which `Engine` the instrument runs on.
//!
//! The cockpit asks for an engine, not for a type: [`build`] returns a `Box<dyn CockpitEngine>` (the
//! contract's trait, plus the screens' detached runs - issue #138) and every system in this crate
//! calls it through that trait. Two engines are compiled in here, and a build carries the one its
//! features select:
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

use cockpit::engine::{Engine, EngineError, EngineInput, EngineOutput};

/// Issue #138: the engine the cockpit holds - the contract's [`Engine`] plus the two calls its
/// screens make for an input that is **not** the live draft (a Curves record, a comparison's
/// variant). The contract's trait is unchanged (it is pinned byte for byte to the adapter's copy);
/// this is the cockpit's own seam over it.
///
/// Why the calls are not plain [`Engine::run`]: the slot's engine sits behind the draft's throttle
/// (`bootstrap::InstrumentedEngine`), which holds one cached input and, inside its window, answers a
/// *different* input with the last run's output. A grid record or a variant asked through it either
/// got another input's answer or evicted the draft's run, which the next frame then paid for again
/// in full. These calls go past it to the engine itself.
pub trait CockpitEngine: Engine {
    /// The engine's whole run of `input`, exactly [`Engine::run`]'s answer for it, without touching
    /// the draft's throttle (a comparison's variant).
    fn run_detached(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        self.run(input)
    }

    /// [`Self::run_detached`] for a caller that reads the run's point and never its performance
    /// chart (the Curves records). An engine that can leave the chart out does; every other field is
    /// the whole run's. By default it is the whole run.
    fn run_point(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        self.run_detached(input)
    }
}

/// The recorded replay is arithmetic on its recording: it has no chart to leave out.
impl CockpitEngine for cockpit::fixture_engine::FixtureEngine {}

/// Build the engine for this build's features, or the one the page asked for.
pub fn build(
    fixture_text: &str,
    requested: Option<&str>,
) -> Result<Box<dyn CockpitEngine>, String> {
    match requested {
        Some("fixture") => fixture(fixture_text),
        Some("real") => real(fixture_text),
        _ => default_engine(fixture_text),
    }
}

/// The real engine, when this build carries it.
#[cfg(feature = "real-engine")]
fn real(fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    let catalog = crate::engine_catalog::catalog_from_fixture(fixture_text)?;
    let fill_depths = crate::engine_catalog::fill_depths_from_fixture(fixture_text)?;
    Ok(Box::new(
        crate::adapter_bridge::AdapterEngine::new(catalog).with_fill_depths(fill_depths),
    ))
}

#[cfg(not(feature = "real-engine"))]
fn real(_fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    Err("`?engine=real` needs the `real-engine` feature, which this build does not carry".into())
}

/// The recorded replay, when this build carries it.
#[cfg(feature = "fixture-engine")]
fn fixture(fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    Ok(Box::new(cockpit::fixture_engine::FixtureEngine::from_json(
        fixture_text,
    )?))
}

#[cfg(not(feature = "fixture-engine"))]
fn fixture(_fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    Err(
        "`?engine=fixture` needs the `fixture-engine` feature, which this build does not carry"
            .into(),
    )
}

#[cfg(feature = "real-engine")]
fn default_engine(fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    real(fixture_text)
}

#[cfg(all(not(feature = "real-engine"), feature = "fixture-engine"))]
fn default_engine(fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    fixture(fixture_text)
}

#[cfg(not(any(feature = "real-engine", feature = "fixture-engine")))]
fn default_engine(_fixture_text: &str) -> Result<Box<dyn CockpitEngine>, String> {
    Err(
        "no engine is compiled into this build: enable `real-engine` (default) or `fixture-engine`"
            .into(),
    )
}
