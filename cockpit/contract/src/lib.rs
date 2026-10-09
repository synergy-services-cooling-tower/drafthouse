//! The cockpit's engine contract, its one recorded implementation and the host configuration.
//!
//! This crate is the approved baseline (the private design pass `2ef3eea`, `55-bevy-cockpit/cockpit/`)
//! **trimmed to the contract** on import (issue #58): `engine.rs` and `host.rs` are byte copies of
//! that commit's files, `fixture_engine.rs` is the same file with its one test-only `include_str!`
//! path adapted to this layout (the application's own `assets/fixture.json`), and the baseline's own
//! Bevy application (`app`, `bridge`, `scene`, `theme`, `ui`) is deliberately not part of it - the
//! product UI is `drafthouse-cockpit`, the owner-approved visual pass.
//!
//! Module map:
//! - `engine`         the data contract (trait `Engine`, input/output types) - no physics here
//! - `fixture_engine` the recorded run of the real engine (`FixtureEngine`) and the fixture file schema
//! - `host`           `HostConfig`, `Branding`, `ServerCommand` - what the host page injects / stubs

// The baseline's `fixture_engine.rs` refuses NaN by negating comparisons (`!(x > 0.0)` is true for
// NaN where `x <= 0.0` would be false), which is what `clippy::neg_cmp_op_on_partial_ord` flags. The
// file is a byte-pinned copy of the approved baseline (see above), so the lint is off for this crate
// rather than rewritten here - `#[rustfmt::skip]` below is the same trade for the same reason.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

// The three modules below are the baseline's files (see above). `#[rustfmt::skip]` keeps `cargo fmt
// --check` from rewriting them: the byte identity with the approved baseline is the point, exactly as
// `rust/cockpit-adapter` does for its copy of the same files.
#[rustfmt::skip]
pub mod engine;
#[rustfmt::skip]
pub mod fixture_engine;
#[rustfmt::skip]
pub mod host;
