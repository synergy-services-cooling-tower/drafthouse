//! The real engine behind **this** contract (issue #58).
//!
//! `rust/cockpit-adapter` implements the cockpit's `Engine` over the repository's engine, and it
//! carries its **own copy** of the contract file: `rust/cockpit-adapter/src/engine.rs` is a byte
//! copy of the design pass's `cockpit/src/engine.rs`, pinned by that crate's `contract-drift` test.
//! This crate compiles the design's copy (`cockpit/contract/src/engine.rs`). The two files hold the
//! same bytes, so the two type families are structurally identical — but they are *nominally*
//! distinct, and `RealEngine`'s `Engine` is the adapter's trait, not this crate's.
//!
//! This module is the one place that difference is handled: [`AdapterEngine`] implements this
//! crate's [`Engine`] by converting the input to the adapter's copy and the output back, through
//! serde. That is an identity, not a mapping — every field of both families carries the same name,
//! type and `#[serde]` attributes, because the definitions are the same file — and the crate's own
//! tests check that identity on the fixture's own input (`the_contract_bridge_is_the_identity`)
//! rather than asserting it in prose.
//!
//! Why not share the trait directly: the adapter's copy is pinned for a reason (the drift gate),
//! and editing it to depend on this crate is exactly what the import lane is fenced from doing.

use serde::de::DeserializeOwned;
use serde::Serialize;

// The adapter's trait, aliased: it is the one `RealEngine` implements, and a trait has to be in
// scope for its methods to be callable. The contract's `Engine` below is the one this crate exposes.
use cockpit::engine::{Engine, EngineError, EngineInput, EngineOutput};
use cockpit_adapter::engine::Engine as AdapterContract;

/// `cockpit_adapter::RealEngine` behind this crate's [`Engine`].
pub struct AdapterEngine(cockpit_adapter::RealEngine);

impl AdapterEngine {
    pub fn new(catalog: cockpit_adapter::synergy_drafthouse::SelectionCatalog) -> Self {
        Self(cockpit_adapter::RealEngine::new(catalog))
    }

    /// The engine's own name, as the adapter states it (`data-source`, the provenance line).
    pub fn engine_name(&self) -> &str {
        self.0.name()
    }
}

/// One value of either contract as the other's type: serialise, deserialise. A failure is a
/// schema error naming the field, never a defaulted value.
fn convert<From: Serialize, To: DeserializeOwned>(value: &From) -> Result<To, EngineError> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|e| {
            EngineError::Schema(format!(
                "the contract's copy and the adapter's copy disagreed: {e}"
            ))
        })
}

/// The adapter's two error variants are the contract's, carried across by hand: the error type
/// itself does not derive serde (the contract pins that), so a new variant must be spelled here.
fn engine_error(error: cockpit_adapter::engine::EngineError) -> EngineError {
    match error {
        cockpit_adapter::engine::EngineError::Unavailable(message) => {
            EngineError::Unavailable(message)
        }
        cockpit_adapter::engine::EngineError::Schema(message) => EngineError::Schema(message),
    }
}

impl Engine for AdapterEngine {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn run(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        let input: cockpit_adapter::engine::EngineInput = convert(input)?;
        let output = self.0.run(&input).map_err(engine_error)?;
        convert(&output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::fixture_engine::FixtureEngine;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    /// The bridge is the identity: the fixture's own input crosses to the adapter's copy and comes
    /// back byte-equal in every field. A field added to one copy and not the other fails here.
    #[test]
    fn the_contract_bridge_is_the_identity() {
        let engine = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        let input = engine.default_input();
        let over: cockpit_adapter::engine::EngineInput =
            convert(&input).expect("the input crosses to the adapter's copy");
        let back: EngineInput = convert(&over).expect("the input comes back");
        assert_eq!(back, input, "the two contract copies are the same fields");
    }

    /// The wrapper runs the real engine through that bridge: the output comes back as this crate's
    /// type, and its provenance names the adapter's engine.
    #[test]
    fn the_real_engine_runs_through_the_bridge() {
        let catalog =
            crate::engine_catalog::catalog_from_fixture(FIXTURE).expect("the catalog builds");
        let engine = AdapterEngine::new(catalog);
        let fixture = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        let output = engine
            .run(&fixture.default_input())
            .expect("the recorded duty runs");
        assert!(
            output.provenance.engine.contains("RealEngine"),
            "the output's provenance names the engine that answered: {}",
            output.provenance.engine
        );
        assert!(
            output.airflow_m3_s > 0.0,
            "the run carries the engine's own numbers"
        );
        // The engine reports the air path once per fill layer: the fixture's default input is a
        // two-layer stack, so the eight zones carry two Fill entries.
        let fill_zones = output
            .pressure_by_zone
            .iter()
            .filter(|zone| zone.zone == cockpit::engine::ZoneId::Fill)
            .count();
        assert_eq!(
            fill_zones,
            fixture.default_input().fill_layers.len(),
            "one fill zone per declared layer"
        );
    }
}
