//! The cockpit's engine boundary over the real cooling-tower engine (issue #57).
//!
//! The cockpit (the round-55 private design pass, taken from the designer's own checkout)
//! renders one [`engine::EngineOutput`] per run and never computes physics itself. Its baseline
//! implementation, `FixtureEngine`, replays a recorded run. This crate is the second implementation:
//! [`RealEngine`] answers the same contract out of the engine crate at this repository's `rust/`.
//!
//! # The contract is a byte copy of the cockpit's file
//!
//! `src/engine.rs` is the cockpit's `cockpit/contract/src/engine.rs` copied byte for byte
//! (imported from the private design pass at commit `815d832`, plus issue #59's `nominalRpm` field on the fan
//! record) - 13010 bytes, sha256
//! `c955238548fd51a62c7a33687d47bf05608e64062502df53d0d66b0c74bf27fe`, git blob
//! `418ecf8cb46fbf2cc8de51d066e146815c03904f`. It is an ordinary module; `#[rustfmt::skip]` on its
//! `mod` declaration in this file keeps `cargo fmt --check` from reformatting it, because the byte
//! identity is the point - a reformatted copy is no longer the contract the drift test pins.
//! `tests/contract_drift.rs` re-checks that identity and names any drift.
//!
//! # What the adapter does, and what it refuses
//!
//! [`RealEngine::run`] maps a cockpit [`engine::EngineInput`] onto the engine's own selection path
//! (`synergy_drafthouse::run_selection`) plus the engine's public layered air-side breakdown
//! and fan curve evaluation for the two charts. [`RealEngine::run`]'s own doc comment is the
//! field-by-field map; the one thing to know before reading it: the cockpit's `fill_layers` are the
//! engine's **ordered fill stack** - one layer per declared layer, top first - so a mixed stack is
//! selected over and its contract results are the engine's own layered results, not one layer
//! reported as if it were the stack.
//!
//! The engine stays data-only: nothing in `rust/cockpit-adapter` or `rust/src` names a UI toolkit.
//!
//! # Wording
//!
//! The engine is **ported and drift-guarded, not validated**. This crate's docs, comments and tests
//! say what they measure and nothing more: borrowed <-> recorded numbers agreeing at a tolerance is
//! a measured agreement, not a proof of correctness.
// The contract copy is byte-pinned (see the crate docs): `#[rustfmt::skip]` on the declaration keeps
// `cargo fmt --check` from rewriting it. Removing this attribute silently breaks the drift pin.
#[rustfmt::skip]
pub mod engine;
pub mod fixture_engine;
mod real_engine;

pub use real_engine::RealEngine;

/// The engine crate itself, re-exported: a caller that builds the [`RealEngine::new`] catalog needs
/// its record types (`SelectionCatalog`, `SelectionTower`, `ZoneCorrelation`, ...) and there is no
/// reason for it to take a second path dependency on the same crate.
pub use synergy_drafthouse;
