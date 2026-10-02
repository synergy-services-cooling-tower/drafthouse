//! Cooling-tower engine core, Rust port — slices 1 to 4.
//!
//! This crate mirrors the JavaScript reference engine in `../src` (barrel: `src/index.js`)
//! for the ported slices only:
//!
//! * [`psychrometrics`] — ASHRAE-style moist-air properties from dry bulb/wet bulb or dry
//!   bulb/relative humidity, including the water-vapour enhancement factor and the
//!   sub-zero (over-ice) saturation branch.
//! * [`merkel`] — counterflow Merkel demand by composite Simpson and four-point
//!   equal-weight Tchebycheff quadrature, with the selectable entering-air enthalpy
//!   convention, plus the cold-water-temperature solve from an available `KaV/L`.
//! * [`airside`] — the air-side pressure breakdown (fill, drift eliminator, minor losses,
//!   fan-stack discharge), the resolved flow areas, the per-zone Merkel contributions and the
//!   fill operating envelope.
//! * [`fan`] — fan operating limits, curve evaluation, the fan/system balance and standard
//!   motor selection.
//! * [`water`] — the water heat-capacity correlation the Merkel demand uses.
//! * [`water_balance`] — evaporation from the dry-air mass balance, drift loss, and the
//!   blowdown/makeup water balance.
//! * [`capability`] — the characteristic-curve capability projection (design demand against
//!   the test-anchored whole-tower characteristic) and its seeded Monte-Carlo propagation.
//! * [`natural_draft`] — the coupled natural-draft counterflow solve: buoyancy against the
//!   air-side resistance, with the fill's thermal demand solved at each trial flow.
//! * [`performance_curve`] — the rectangular wet-bulb × range curve set: bilinear cold-water
//!   prediction, the inverse water-flow solve, and the test capability ratio.
//! * [`crossflow`] — the simplified finite-volume crossflow grid, Richardson-extrapolated,
//!   plus the saturated temperature-from-enthalpy inversion the crossflow and selection
//!   slices both use.
//! * [`nozzle`] — nozzle flow and arrangement selection.
//! * [`selection`] — component selection: candidate generation, the feasibility constraints
//!   and the per-candidate physics ported from `selection.js`, ranked by the
//!   **engineering objectives** owner-routed from issue #1 (duty and margin as constraints,
//!   the objective declared per run, capacity and capability ratio reported) instead of the
//!   reference's superseded economics ranking. No commercial field exists anywhere in the
//!   port. This module also carries the unported-remainder helpers the selector needs
//!   (`waterDensityKgM3`, `heatRejectionKW`, `volumetricFlowM3SFromMassFlow`,
//!   `estimateOutletAirState`); see the module header for why they live there.
//! * [`validate`] — the fail-closed catalog-record validator (issue #5): the machine schema
//!   of the five catalog record types as data (`docs/CATALOG_SCHEMA.md` as code), and the
//!   refusals that keep a mistyped, missing or non-finite field out of a ranking.
//! * [`numeric`] — the quadrature and root-finding primitives the slices need.
//!
//! Not ported yet: `generateDemandCurve` from `merkel.js` and the worked-steps renderer.
//! The JavaScript engine is the specification, not a work item.
//!
//! The library has no dependencies outside the standard library; see `README.md` for the
//! JavaScript-to-Rust name mapping and for the parity harness
//! (`node scripts/parity/run.mjs`).
//!
//! Two more modules are not part of the physics: [`cli`] is the JSON command surface shared
//! by the `ct-engine` binary and the wasm export, and [`wasm`] (built only for
//! `wasm32-unknown-unknown`) is the C ABI over it. Both are documented in `README.md`.

pub mod airside;
pub mod capability;
pub mod cli;
pub mod crossflow;
pub mod fan;
pub mod layers;
pub mod merkel;
pub mod natural_draft;
pub mod nozzle;
pub mod numeric;
pub mod performance_curve;
pub mod psychrometrics;
pub mod selection;
pub mod validate;
pub mod water;
pub mod water_balance;

/// The wasm export surface (issue #24). Compiled only for the wasm target: it is a C ABI,
/// not a Rust API, and nothing else in the crate should reach through it.
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use airside::{
    check_fill_operating_envelope, drift_performance_at_velocity, envelope_failures,
    fan_stack_discharge_pressure_pa, fill_pressure_drop_pa, fill_thermal_merkel_number,
    layered_system_pressure_breakdown, minor_loss_pressure_pa, resolve_airside_areas,
    system_pressure_breakdown, velocity_pressure_pa, zone_merkel_number, AirsideAreas,
    DriftCurvePoint, DriftEliminatorRecord, DriftPerformance, FanStackDischarge,
    FanStackDischargeInput, FillLimits, FillOperatingEnvelope, FillOperatingEnvelopeInput,
    FillPressureCorrelation, FillRecord, LayeredBreakdown, LayeredBreakdownInput,
    SystemPressureBreakdown, SystemPressureBreakdownInput, TowerRecord, ZoneCorrelation,
};
pub use capability::{
    evaluate_characteristic_capability, monte_carlo_characteristic_capability, CapabilityCondition,
    CapabilityCurvePoint, CapabilityUncertainty, CharacteristicCapability,
    CharacteristicCapabilityInput, ConditionField, MonteCarloInput, MonteCarloResult,
};
pub use crossflow::{
    crossflow_convergence_study, saturated_temperature_from_enthalpy, solve_crossflow_grid,
    CrossflowConvergenceStudy, CrossflowGridConvergence, CrossflowGridInput, CrossflowGridResult,
    CrossflowStudyInput, OutletAirState, CROSSFLOW_MODEL,
};
pub use fan::{
    choose_standard_motor, estimate_airflow_from_fan_power, fan_efficiency_at_flow,
    fan_operating_limits, fan_pressure_pa_at_flow, fan_shaft_power_kw,
    solve_fan_system_intersection, AirflowEstimateInput, FanCurvePoint, FanOperatingLimits,
    FanPressureBasis, FanRecord, FanSystemIntersection, FanSystemIntersectionInput, MotorSelection,
    FLOW_AFFINITY_EXPONENT, PRESSURE_AFFINITY_EXPONENT, SHAFT_POWER_AFFINITY_EXPONENT,
    STANDARD_MOTOR_SIZES_KW,
};
pub use layers::{
    check_stack_operating_envelope, empty_stack_error, resolve_fill_layers, FillLayer,
    FillLayerResult, FillStack, LayerTerms,
};
pub use merkel::{
    air_enthalpy_operating_line_kj_kg, characteristic_coefficient,
    inlet_air_enthalpy_kj_kg_dry_air, merkel_demand, safe_merkel_demand,
    solve_cold_water_temperature, tower_characteristic, validate_thermal_temperatures,
    ColdWaterTemperatureInput, ColdWaterTemperatureResult, InletEnthalpyConvention, Integration,
    MerkelInput, MerkelResult,
};
pub use natural_draft::{
    natural_draft_pressure_pa, solve_natural_draft_counterflow, NaturalDraftFlowPoint,
    NaturalDraftInput, NaturalDraftResult,
};
pub use nozzle::{
    nozzle_flow_m3_s, select_nozzle_arrangement, NozzleArrangement, NozzleArrangementInput,
    NozzleRecord, NOZZLE_WATER_DENSITY_KG_M3,
};
pub use numeric::{
    assert_finite_number, assert_non_negative, assert_positive, bilinear, bracket_values, clamp,
    create_seeded_random, find_root_by_scan, gaussian_random, integrate_chebyshev4,
    integrate_simpson, interpolate_1d, js_max, js_min, percentile, range, solve_bracketed_root,
    BracketedRootOptions, DomainError, RootPreference, RootScanOptions, SeededRandom,
};
pub use performance_curve::{
    evaluate_performance_curve_capability, performance_curve_bounds,
    predict_cold_water_from_performance_curves, predict_water_flow_from_performance_curves,
    ColdWaterPrediction, PerformanceCurveBounds, PerformanceCurveCapability,
    PerformanceCurveRecord, WaterFlowPrediction,
};
pub use psychrometrics::{
    dew_point_from_humidity_ratio, dry_air_density_kg_m3, dry_air_specific_volume_m3_kg,
    effective_saturation_pressure_pa, humidity_ratio_from_relative_humidity,
    humidity_ratio_from_vapor_pressure, humidity_ratio_from_wet_bulb, moist_air_density_kg_m3,
    moist_air_enthalpy_kj_kg_dry_air, psychrometric_state, relative_humidity_from_humidity_ratio,
    saturated_air_enthalpy_kj_kg_dry_air, saturation_humidity_ratio, saturation_vapor_pressure_pa,
    vapor_pressure_from_humidity_ratio, water_vapor_enhancement_factor,
    wet_bulb_from_humidity_ratio, PsychrometricOptions, PsychrometricState,
    PsychrometricStateInput,
};
pub use selection::{
    default_selection_requirements, estimate_outlet_air_state, fan_speed_ratio_limit,
    heat_rejection_kw, rank_candidates, run_selection, select_cooling_tower_components,
    volumetric_flow_m3_s_from_mass_flow, water_density_kg_m3, CatalogMetadata, FanSpeedRatioLimit,
    Objective, SelectionCandidate, SelectionCatalog, SelectionDriftEliminator, SelectionFan,
    SelectionFill, SelectionInput, SelectionRequirements, SelectionResult, SelectionRun,
    SelectionTower, ThermalModel, TowerType, WaterQualityFactor, SELECTION_WARNING,
};
pub use validate::{
    schema, validate_record, Column, FieldKind, FieldRule, NumberDomain, RecordKind, Violation,
};
pub use water::water_specific_heat_kj_kg_k;
pub use water_balance::{
    cooling_tower_water_balance, drift_loss_kg_s, evaporation_from_air_mass_balance, WaterBalance,
    WaterBalanceInput,
};
