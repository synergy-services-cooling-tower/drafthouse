//! The parity gate (#57): on the recorded duty, the adapter's numbers against the recorded run.
//!
//! **Data.** `tests/data/cockpit-fixture.json` is a byte copy of the cockpit's own fixture file
//! (`<cockpit>/assets/fixture.json` of the round-55 design pass at commit `815d832`,
//! plus issue #59's `nominalRpm` field on the four fan records and issue #73's rename of the two
//! synthetic layer fills; sha256
//! `2302f119606337ec9ede690c89410c3fcd1aa676592f7be392e3463f88ca16f2`, 96763 bytes; see
//! `tests/data/README.md`, which carries the full provenance). Its `catalog` block carries the engine's own records (the cockpit's curve
//! data rides along as `characteristic` and is not part of the engine's intake), and its `anchor`
//! block records the real engine's run (`fixtures/engine-run.json`, generated from the shipped wasm
//! build) exactly as the cockpit renders it: the 11 headline metrics, the zone breakdown, the
//! transfer numbers, the per-layer split, the water flow, the recorded sweeps and the two curves.
//!
//! **Two recorded cases.** `anchor.anchorFillId`/`anchor.candidate` name the recorded single-fill
//! run - FILM-VF38 at 1.5 m - and `recorded_duty_matches_the_recorded_run` drives that case through
//! the same contract the cockpit uses and compares every number against the record. The cockpit's
//! own default input is the **mixed stack** FILM-MF20 0.45 m over FILM-WF25 0.90 m, whose two
//! synthetic fills the fixture's generator solved so that the stack carries the anchor run's fill
//! terms; `anchor.layers` records that split per layer, and
//! `the_recorded_default_input_matches_the_recorded_layers_per_layer` compares the adapter's
//! per-layer results against those recorded numbers.
//!
//! **What agreement means here.** The record's provenance is the shipped *wasm* build of this crate
//! and this test runs the *native* crate, so directly carried quantities are compared at `REL`/`ABS`
//! rather than bitwise; the test prints the count and the worst relative deviation it saw. Agreement
//! is a measurement, not a statement that either side is right - the engine is ported and
//! drift-guarded, not validated.

use std::collections::HashMap;

use cockpit_adapter::engine::{Duty, Engine, EngineError, EngineInput, FillLayer, ZoneId};
use cockpit_adapter::RealEngine;
use serde::Deserialize;
use serde_json::Value;
use synergy_drafthouse::{
    fan_pressure_pa_at_flow, CatalogMetadata, DriftCurvePoint, FanCurvePoint, FanPressureBasis,
    FillLimits, FillRecord, SelectionCatalog, SelectionDriftEliminator, SelectionFan,
    SelectionFill, SelectionTower, TowerRecord as EngineTowerRecord, TowerType, WaterQualityFactor,
    ZoneCorrelation,
};

const FIXTURE: &str = include_str!("data/cockpit-fixture.json");

/// Relative tolerance for a quantity the record carries directly (the record came from the wasm build
/// of this crate; the engine's own cross-build measurement is ~1e-13 relative).
const REL: f64 = 1e-12;
/// Absolute floor, for quantities that are near zero (a pressure share, a residual).
const ABS: f64 = 1e-9;
/// Relative tolerance for the recorded fan-curve trace: those points came from the engine's own solve
/// for a constant system pressure, so they carry that solve's tolerance.
const TRACE_REL: f64 = 1e-6;
/// Relative tolerance for the recorded mixed stack's own rows (`anchor.layers`, the shares derived
/// from it and the totals they make up). Those numbers were computed by the fixture's generator at
/// the **anchor run's** loadings and its synthetic coefficients were rounded (6 dp thermal, 5 dp
/// pressure), while the adapter's mixed stack resolves its own operating point: the two airflows
/// differ by 3.8e-9 relative (measured), which the fills' exponents amplify to 6.6e-9 on a
/// per-layer pressure and 2.9e-8 on a share - so this is a 30x margin over a measured, explained
/// artifact, not a widened `REL` (a moved depth moves these rows by 20 %+).
const RECORDED_SPLIT_REL: f64 = 1e-6;
/// The duty edit the `duty-edited` frame uses, and the test below drives through the same input: the
/// recorded water flow is 724.81 m3/hr (200 kg/s).
const EDITED_DUTY_WATER_FLOW_M3_HR: f64 = 600.0;

// ------------------------------------------------------------------------------------ the fixture

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    catalog: Catalog,
    default_input: DefaultInput,
    anchor: Anchor,
    refusals: Refusals,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    metadata: Meta,
    water_quality_factors: HashMap<String, QualityFactor>,
    towers: Vec<Value>,
    fills: Vec<Value>,
    drift_eliminators: Vec<Value>,
    fans: Vec<Value>,
    nozzles: Vec<Value>,
}

#[derive(Deserialize)]
struct Meta {
    id: String,
    revision: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QualityFactor {
    thermal_multiplier: f64,
    pressure_multiplier: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefaultInput {
    duty: Duty,
    tower_id: String,
    fill_layers: Vec<FillLayer>,
    drift_id: String,
    fan_id: String,
    speed_ratio: f64,
    nozzle_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Anchor {
    headline: Headline,
    zones: Zones,
    transfer: Transfer,
    water_flow: WaterFlow,
    candidate: Candidate,
    sweeps: Sweeps,
    system_curve: Vec<SystemPoint>,
    fan_curve_at_speed: FanCurveAtSpeed,
    worked: Worked,
    warning: String,
    anchor_fill_id: String,
    layers: Vec<LayerJson>,
}

#[derive(Deserialize)]
struct Headline {
    capability_pct: f64,
    water_flow_m3_hr: f64,
    cold_water_c: f64,
    range_c: f64,
    approach_c: f64,
    airflow_m3_s: f64,
    fan_power_kw: f64,
    total_pressure_pa: f64,
    total_pressure_mmwg: f64,
    evaporation_pct: f64,
    makeup_m3_hr: f64,
    kavl_total: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Zones {
    inlet_pa: f64,
    support_pa: f64,
    distribution_pa: f64,
    drift_pa: f64,
    plenum_pa: f64,
    fan_stack_pa: f64,
    fixed_pa: f64,
    fill_pa: f64,
    total_pa: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transfer {
    fill_merkel_number: f64,
    spray_zone_merkel_number: f64,
    rain_zone_merkel_number: f64,
    available_merkel_number: f64,
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
    air_density_kg_m3: f64,
    fan_efficiency_at_anchor: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaterFlow {
    kg_s: f64,
    m3_hr: f64,
    water_density_kg_m3: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    fill_depth_m: f64,
    speed_ratio: f64,
    water_volumetric_flow_m3_s: f64,
    fan_operating_point: FanOperatingPoint,
    water_balance: WaterBalance,
    airside: Airside,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FanOperatingPoint {
    flow_m3_s: f64,
    fan_pressure_pa: f64,
    efficiency: f64,
    #[serde(rename = "shaftPowerKW")]
    shaft_power_kw: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaterBalance {
    evaporation_kg_s: f64,
    makeup_kg_s: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Airside {
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
}

#[derive(Deserialize)]
struct Sweeps {
    #[serde(rename = "waterMassFlowKgS")]
    water_mass_flow_kg_s: Sweep,
}

#[derive(Deserialize)]
struct Sweep {
    values: Vec<f64>,
    series: HashMap<String, Vec<SweepPoint>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SweepPoint {
    x: f64,
    cold_water_c: Option<f64>,
    #[serde(default)]
    infeasible: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemPoint {
    flow_m3_s: f64,
    total_pa: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FanCurveAtSpeed {
    speed_ratio: f64,
    air_density_kg_m3: f64,
    points: Vec<TracePoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TracePoint {
    flow_m3_s: f64,
    pressure_pa: f64,
}

#[derive(Deserialize)]
struct Worked {
    steps: Vec<WorkedStepJson>,
}

#[derive(Deserialize)]
struct WorkedStepJson {
    label: String,
    value: Option<f64>,
}

/// One recorded per-layer entry of the cockpit's default (mixed) stack. `position` is its place in
/// the stack ("upper"/"lower"); the numbers are the engine's correlation form evaluated at the
/// anchor loadings with the layer's declared multipliers.
#[derive(Deserialize)]
struct LayerJson {
    fill_id: String,
    depth_m: f64,
    thermal_multiplier: f64,
    pressure_multiplier: f64,
    kavl: f64,
    pressure_pa: f64,
    cooling_share_pct: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Refusals {
    envelope: EnvelopeRefusal,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvelopeRefusal {
    value: f64,
    run: RefusedRun,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefusedRun {
    feasible_candidate_count: usize,
    rejection_summary: HashMap<String, usize>,
}

// ------------------------------------------------------------- engine-side record mirrors
//
// The engine's own record types carry no serde derives (its library is dependency-free), so the
// records are read through mirrors that use the engine's field names and built into the engine's
// types explicitly. A wrong mirror cannot pass quietly: every number below is compared against the
// record afterwards.

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonTower {
    id: String,
    #[serde(rename = "type")]
    tower_type: String,
    fill_area_m2: f64,
    air_free_area_m2: Option<f64>,
    drift_area_m2: Option<f64>,
    inlet_area_m2: Option<f64>,
    plenum_area_m2: Option<f64>,
    fan_stack_area_m2: Option<f64>,
    stack_recovery_factor: Option<f64>,
    inlet_loss_coefficient: Option<f64>,
    distribution_loss_coefficient: Option<f64>,
    support_loss_coefficient: Option<f64>,
    plenum_loss_coefficient: Option<f64>,
    fixed_pressure_loss_pa: Option<f64>,
    spray_zone_height_m: Option<f64>,
    spray_zone: Option<JsonZone>,
    rain_zone_height_m: Option<f64>,
    rain_zone: Option<JsonZone>,
    footprint_m2: f64,
    max_water_mass_flow_kg_s: f64,
    compatible_fan_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonZone {
    coefficient_per_m: f64,
    reference_water_loading_kg_m2_s: f64,
    reference_dry_air_loading_kg_m2_s: f64,
    water_exponent: f64,
    air_exponent: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFill {
    id: String,
    thermal: Option<JsonZone>,
    pressure: Option<JsonPressure>,
    limits: JsonLimits,
    allowed_water_quality_classes: Vec<String>,
    compatible_tower_types: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonPressure {
    coefficient_pa_per_m: f64,
    reference_water_loading_kg_m2_s: f64,
    reference_dry_air_loading_kg_m2_s: f64,
    water_exponent: f64,
    air_exponent: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonLimits {
    min_water_loading_kg_m2_s: f64,
    max_water_loading_kg_m2_s: f64,
    min_dry_air_loading_kg_m2_s: f64,
    max_dry_air_loading_kg_m2_s: f64,
    max_water_temperature_c: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonDrift {
    id: String,
    max_water_temperature_c: f64,
    curve: Vec<JsonDriftPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonDriftPoint {
    #[serde(rename = "faceVelocityMS")]
    face_velocity_ms: f64,
    drift_ppm: f64,
    pressure_drop_pa: f64,
}

// ---------------------------------------------------------------------------------- the checker

/// Counts the comparisons and remembers the worst relative deviation, so a passing run reports how
/// close it was instead of just "ok".
struct Parity {
    checks: usize,
    worst_rel: f64,
    worst_at: String,
    trace_checks: usize,
    worst_trace_rel: f64,
}

impl Parity {
    fn new() -> Self {
        Self {
            checks: 0,
            worst_rel: 0.0,
            worst_at: String::new(),
            trace_checks: 0,
            worst_trace_rel: 0.0,
        }
    }

    fn close(&mut self, what: &str, actual: f64, expected: f64) {
        self.close_with(REL, what, actual, expected);
    }

    /// The recorded fan trace: those points came from the engine's own solve for a constant system
    /// pressure, so they carry that solve's tolerance rather than `REL`.
    fn trace(&mut self, what: &str, actual: f64, expected: f64) {
        self.trace_checks += 1;
        let scale = expected.abs().max(actual.abs());
        let rel = (actual - expected).abs() / scale.max(f64::MIN_POSITIVE);
        if rel > self.worst_trace_rel {
            self.worst_trace_rel = rel;
        }
        assert!(
            rel <= TRACE_REL,
            "{what}: adapter {actual} vs recorded {expected} (rel {rel:e})"
        );
    }

    fn close_with(&mut self, rel_tolerance: f64, what: &str, actual: f64, expected: f64) {
        self.checks += 1;
        let scale = expected.abs().max(actual.abs());
        let delta = (actual - expected).abs();
        if scale > 0.0 {
            let rel = delta / scale;
            if rel > self.worst_rel {
                self.worst_rel = rel;
                self.worst_at = what.to_string();
            }
        }
        assert!(
            delta <= ABS || delta / scale.max(f64::MIN_POSITIVE) <= rel_tolerance,
            "{what}: adapter {actual} vs recorded {expected} (delta {delta:e}, rel {})",
            delta / scale.max(f64::MIN_POSITIVE)
        );
    }

    fn same_count(&mut self, what: &str, actual: usize, expected: usize) {
        self.checks += 1;
        assert_eq!(
            actual, expected,
            "{what}: adapter {actual} vs recorded {expected}"
        );
    }

    fn report(&self) {
        eprintln!(
            "parity: {} comparison(s), worst relative deviation {:.3e} at {}",
            self.checks, self.worst_rel, self.worst_at
        );
        eprintln!(
            "parity: {} recorded fan-trace point(s), worst relative deviation {:.3e} (tolerance {:.0e})",
            self.trace_checks, self.worst_trace_rel, TRACE_REL
        );
    }
}

// ------------------------------------------------------------------------------------- helpers

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("the recorded cockpit fixture parses")
}

fn by_id(records: &[Value], id: &str) -> Value {
    records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("record \"{id}\" is not in the fixture's catalog"))
        .clone()
}

fn from_value<T: for<'de> Deserialize<'de>>(value: &Value) -> T {
    serde_json::from_value(value.clone()).expect("a recorded record parses")
}

fn zone_correlation(zone: &JsonZone) -> ZoneCorrelation {
    ZoneCorrelation {
        coefficient_per_m: zone.coefficient_per_m,
        reference_water_loading_kg_m2_s: zone.reference_water_loading_kg_m2_s,
        reference_dry_air_loading_kg_m2_s: zone.reference_dry_air_loading_kg_m2_s,
        water_exponent: zone.water_exponent,
        air_exponent: zone.air_exponent,
    }
}

/// The engine-side catalog the adapter reads: the fixture's towers (the only records the cockpit's
/// contract cannot carry in the input) plus its metadata and quality factors.
fn engine_catalog(f: &Fixture) -> SelectionCatalog {
    let catalog = &f.catalog;
    let mut water_quality_factors: HashMap<String, WaterQualityFactor> = HashMap::new();
    for (class, factor) in &catalog.water_quality_factors {
        water_quality_factors.insert(
            class.clone(),
            WaterQualityFactor {
                thermal_multiplier: factor.thermal_multiplier,
                pressure_multiplier: factor.pressure_multiplier,
            },
        );
    }
    SelectionCatalog {
        metadata: CatalogMetadata {
            id: catalog.metadata.id.clone(),
            revision: catalog.metadata.revision.clone(),
            status: catalog.metadata.status.clone(),
        },
        water_quality_factors,
        towers: catalog
            .towers
            .iter()
            .map(|value| {
                let tower: JsonTower = from_value(value);
                SelectionTower {
                    id: tower.id,
                    tower_type: TowerType::parse(&tower.tower_type).expect("a known tower type"),
                    physics: EngineTowerRecord {
                        fill_area_m2: tower.fill_area_m2,
                        air_free_area_m2: tower.air_free_area_m2,
                        drift_area_m2: tower.drift_area_m2,
                        inlet_area_m2: tower.inlet_area_m2,
                        plenum_area_m2: tower.plenum_area_m2,
                        fan_stack_area_m2: tower.fan_stack_area_m2,
                        stack_recovery_factor: tower.stack_recovery_factor,
                        inlet_loss_coefficient: tower.inlet_loss_coefficient,
                        distribution_loss_coefficient: tower.distribution_loss_coefficient,
                        support_loss_coefficient: tower.support_loss_coefficient,
                        plenum_loss_coefficient: tower.plenum_loss_coefficient,
                        fixed_pressure_loss_pa: tower.fixed_pressure_loss_pa,
                        spray_zone_height_m: tower.spray_zone_height_m,
                        spray_zone: tower.spray_zone.as_ref().map(zone_correlation),
                        rain_zone_height_m: tower.rain_zone_height_m,
                        rain_zone: tower.rain_zone.as_ref().map(zone_correlation),
                    },
                    footprint_m2: tower.footprint_m2,
                    max_water_mass_flow_kg_s: tower.max_water_mass_flow_kg_s,
                    // Overwritten per run with the layer's single depth; the value here is unused.
                    fill_depth_options_m: vec![tower.fill_area_m2],
                    // The fixture's towers declare no stack variants: the adapter's own per-run
                    // catalog carries the input's stack, and this catalog only supplies the
                    // spray/rain correlations and the metadata.
                    fill_stacks: Vec::new(),
                    compatible_fan_ids: tower.compatible_fan_ids,
                }
            })
            .collect(),
        fills: catalog
            .fills
            .iter()
            .map(|value| {
                let fill: JsonFill = from_value(value);
                SelectionFill {
                    compatible_tower_types: fill
                        .compatible_tower_types
                        .iter()
                        .map(|raw| TowerType::parse(raw).expect("a known tower type"))
                        .collect(),
                    physics: FillRecord {
                        id: fill.id,
                        thermal: fill.thermal.as_ref().map(zone_correlation),
                        pressure: fill.pressure.as_ref().map(|pressure| {
                            synergy_drafthouse::FillPressureCorrelation {
                                coefficient_pa_per_m: pressure.coefficient_pa_per_m,
                                reference_water_loading_kg_m2_s: pressure
                                    .reference_water_loading_kg_m2_s,
                                reference_dry_air_loading_kg_m2_s: pressure
                                    .reference_dry_air_loading_kg_m2_s,
                                water_exponent: pressure.water_exponent,
                                air_exponent: pressure.air_exponent,
                            }
                        }),
                        limits: FillLimits {
                            min_water_loading_kg_m2_s: fill.limits.min_water_loading_kg_m2_s,
                            max_water_loading_kg_m2_s: fill.limits.max_water_loading_kg_m2_s,
                            min_dry_air_loading_kg_m2_s: fill.limits.min_dry_air_loading_kg_m2_s,
                            max_dry_air_loading_kg_m2_s: fill.limits.max_dry_air_loading_kg_m2_s,
                            max_water_temperature_c: fill.limits.max_water_temperature_c,
                        },
                        allowed_water_quality_classes: fill.allowed_water_quality_classes,
                    },
                }
            })
            .collect(),
        drift_eliminators: catalog
            .drift_eliminators
            .iter()
            .map(|value| {
                let drift: JsonDrift = from_value(value);
                SelectionDriftEliminator {
                    physics: synergy_drafthouse::DriftEliminatorRecord {
                        id: drift.id,
                        curve: drift
                            .curve
                            .iter()
                            .map(|point| DriftCurvePoint {
                                face_velocity_ms: point.face_velocity_ms,
                                drift_ppm: point.drift_ppm,
                                pressure_drop_pa: point.pressure_drop_pa,
                            })
                            .collect(),
                    },
                    max_water_temperature_c: drift.max_water_temperature_c,
                }
            })
            .collect(),
        fans: catalog
            .fans
            .iter()
            .map(|value| {
                let fan: JsonFan = from_value(value);
                SelectionFan {
                    physics: synergy_drafthouse::FanRecord {
                        id: fan.id,
                        stack_area_m2: fan.stack_area_m2,
                        pressure_basis: Some(match fan.pressure_basis.as_str() {
                            "total" => FanPressureBasis::Total,
                            "static" => FanPressureBasis::Static,
                            other => panic!("unknown fan pressure basis {other}"),
                        }),
                        reference_density_kg_m3: fan.reference_density_kg_m3,
                        stack_recovery_factor: fan.stack_recovery_factor,
                        curve: fan
                            .curve
                            .iter()
                            .map(|point| FanCurvePoint {
                                flow_m3_s: point.flow_m3_s,
                                pressure_pa: point.pressure_pa,
                                efficiency: point.efficiency,
                            })
                            .collect(),
                    },
                    allowed_speed_ratio: fan.allowed_speed_ratio,
                    nominal_rpm: fan.nominal_rpm,
                    drive_efficiency: fan.drive_efficiency,
                    motor_efficiency: fan.motor_efficiency,
                }
            })
            .collect(),
        // The adapter takes the nozzle from the input (the cockpit's nozzle record is complete), so
        // this catalog carries none - and the adapter never reads this list.
        nozzles: Vec::new(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFan {
    id: String,
    stack_area_m2: Option<f64>,
    pressure_basis: String,
    reference_density_kg_m3: Option<f64>,
    stack_recovery_factor: Option<f64>,
    allowed_speed_ratio: [f64; 2],
    /// Issue #59's fan-record field; absent in a record that states no rated speed.
    #[serde(default)]
    nominal_rpm: Option<f64>,
    drive_efficiency: f64,
    motor_efficiency: f64,
    curve: Vec<JsonFanPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFanPoint {
    flow_m3_s: f64,
    pressure_pa: f64,
    efficiency: f64,
}

/// The cockpit's own default input as the contract states it: the tower, duty, drift, fan, speed
/// ratio and nozzle the fixture records, with its **ordered fill layers** - the recorded mixed
/// stack, FILM-MF20 0.45 m over FILM-WF25 0.90 m - as declared.
fn default_input(f: &Fixture) -> EngineInput {
    let catalog = &f.catalog;
    assert_eq!(f.default_input.tower_id, "IDCF-064");
    assert_eq!(f.default_input.drift_id, "DE-3P-10");
    assert_eq!(f.default_input.fan_id, "AX-500");
    assert_eq!(f.default_input.nozzle_id, "NZ-20");
    EngineInput {
        duty: f.default_input.duty.clone(),
        tower: from_value(&by_id(&catalog.towers, &f.default_input.tower_id)),
        fill_layers: f.default_input.fill_layers.clone(),
        drift: from_value(&by_id(
            &catalog.drift_eliminators,
            &f.default_input.drift_id,
        )),
        fan: from_value(&by_id(&catalog.fans, &f.default_input.fan_id)),
        speed_ratio: f.default_input.speed_ratio,
        nozzle: from_value(&by_id(&catalog.nozzles, &f.default_input.nozzle_id)),
    }
}

/// The recorded single-fill run's own selection: [`default_input`] with the anchor's one layer.
/// `anchor.anchorFillId`/`anchor.candidate` name it: FILM-VF38 at 1.5 m, the moderate quality
/// factors as the layer's multipliers - not the two synthetic layers the cockpit's default input
/// stacks (those re-express this run's fill terms per layer).
fn recorded_input(f: &Fixture) -> EngineInput {
    let quality = &f.catalog.water_quality_factors[&f.default_input.duty.water_quality_class];
    assert_eq!(
        f.anchor.anchor_fill_id, "FILM-VF38",
        "the recorded anchor fill moved: the parity case assumes the recorded single fill"
    );
    let mut input = default_input(f);
    input.fill_layers = vec![FillLayer {
        fill_id: f.anchor.anchor_fill_id.clone(),
        depth_m: f.anchor.candidate.fill_depth_m,
        thermal_multiplier: quality.thermal_multiplier,
        pressure_multiplier: quality.pressure_multiplier,
    }];
    input
}

// --------------------------------------------------------------------------------------- tests

/// The parity gate: every number the adapter produces for the recorded duty against the record.
#[test]
fn recorded_duty_matches_the_recorded_run() {
    let f = fixture();
    let mut p = Parity::new();
    let mut engine = RealEngine::new(engine_catalog(&f));
    let input = recorded_input(&f);
    let out = engine
        .run(&input)
        .expect("the recorded duty runs through the adapter");

    assert!(
        out.validation.is_empty(),
        "the recorded run reported limits: {:?}",
        out.validation
    );

    // The 11 headline metrics, plus the second total-pressure unit the first screen shows.
    let h = &f.anchor.headline;
    p.close(
        "headline.capability_pct",
        out.capability_pct,
        h.capability_pct,
    );
    p.close(
        "headline.water_flow_m3_hr",
        out.water_flow_m3_hr,
        h.water_flow_m3_hr,
    );
    p.close("headline.cold_water_c", out.cold_water_c, h.cold_water_c);
    p.close("headline.range_c", out.range_c, h.range_c);
    p.close("headline.approach_c", out.approach_c, h.approach_c);
    p.close("headline.airflow_m3_s", out.airflow_m3_s, h.airflow_m3_s);
    p.close("headline.fan_power_kw", out.fan_power_kw, h.fan_power_kw);
    p.close(
        "headline.total_pressure_pa",
        out.total_pressure_pa,
        h.total_pressure_pa,
    );
    p.close(
        "headline.total_pressure_mmwg",
        out.total_pressure_mmwg,
        h.total_pressure_mmwg,
    );
    p.close(
        "headline.evaporation_pct",
        out.evaporation_pct,
        h.evaporation_pct,
    );
    p.close("headline.makeup_m3_hr", out.makeup_m3_hr, h.makeup_m3_hr);
    p.close("headline.kavl_total", out.kavl_total, h.kavl_total);

    // Pressure by zone: the engine's breakdown, in air-path order, against the recorded zones.
    p.same_count("zones", out.pressure_by_zone.len(), 8);
    let zone = |id: ZoneId, layer: Option<usize>| {
        out.pressure_by_zone
            .iter()
            .find(|zone| zone.zone == id && zone.layer == layer)
            .unwrap_or_else(|| panic!("no {id:?} zone with layer {layer:?}"))
            .pressure_pa
    };
    let z = &f.anchor.zones;
    p.close("zone inlet", zone(ZoneId::Inlet, None), z.inlet_pa);
    p.close(
        "zone rain (support)",
        zone(ZoneId::Rain, None),
        z.support_pa,
    );
    p.close("zone fill[0]", zone(ZoneId::Fill, Some(0)), z.fill_pa);
    p.close(
        "zone spray (distribution)",
        zone(ZoneId::Spray, None),
        z.distribution_pa,
    );
    p.close("zone drift", zone(ZoneId::Drift, None), z.drift_pa);
    p.close("zone plenum", zone(ZoneId::Plenum, None), z.plenum_pa);
    p.close("zone stack", zone(ZoneId::Stack, None), z.fan_stack_pa);
    p.close("zone fixed", zone(ZoneId::Fixed, None), z.fixed_pa);
    let zone_total: f64 = out
        .pressure_by_zone
        .iter()
        .map(|zone| zone.pressure_pa)
        .sum();
    p.close("zones sum to the total", zone_total, z.total_pa);
    let share_sum: f64 = out.pressure_by_zone.iter().map(|zone| zone.share_pct).sum();
    p.close("zone shares sum to 100", share_sum, 100.0);

    // KaV/L per layer: the recorded case is one fill, so the layer is the engine's own fill numbers.
    p.same_count("layers", out.kavl_per_layer.len(), 1);
    let layer = &out.kavl_per_layer[0];
    assert_eq!(layer.index, 0);
    assert_eq!(layer.fill_id, f.anchor.anchor_fill_id);
    assert!(
        layer.inside_envelope,
        "the recorded fill is inside its envelope"
    );
    let t = &f.anchor.transfer;
    p.close(
        "layer.depth_m",
        layer.depth_m,
        f.anchor.candidate.fill_depth_m,
    );
    p.close("layer.kavl", layer.kavl, t.fill_merkel_number);
    p.close("layer.pressure_pa", layer.pressure_pa, z.fill_pa);
    p.close(
        "layer.cooling_share_pct",
        layer.cooling_share_pct,
        100.0 * t.fill_merkel_number / t.available_merkel_number,
    );

    // The cockpit's synthetic two-layer split of that same fill, added up: the adapter's single-fill
    // numbers are the recorded stack total. The split's coefficients were rounded to 6 dp by its
    // generator, which allowed itself 0.05 %.
    let split_kavl: f64 = f.anchor.layers.iter().map(|layer| layer.kavl).sum();
    let split_pa: f64 = f.anchor.layers.iter().map(|layer| layer.pressure_pa).sum();
    assert!(
        (split_kavl - layer.kavl).abs() / split_kavl < 5e-4,
        "the recorded split totals {split_kavl} against the adapter's {0}",
        layer.kavl
    );
    assert!(
        (split_pa - layer.pressure_pa).abs() / split_pa < 5e-4,
        "the recorded split totals {split_pa} Pa against the adapter's {0} Pa",
        layer.pressure_pa
    );

    // The transfer numbers and the water flow the run resolved.
    p.close(
        "transfer.fill_merkel_number",
        layer.kavl,
        t.fill_merkel_number,
    );
    p.close(
        "transfer.spray_zone_merkel_number + rain",
        t.spray_zone_merkel_number + t.rain_zone_merkel_number,
        t.available_merkel_number - t.fill_merkel_number,
    );
    p.close(
        "transfer.available_merkel_number",
        out.kavl_total,
        t.available_merkel_number,
    );
    p.close(
        "transfer.water_loading_kg_m2_s",
        f.anchor.candidate.airside.water_loading_kg_m2_s,
        t.water_loading_kg_m2_s,
    );
    p.close(
        "transfer.dry_air_loading_kg_m2_s",
        f.anchor.candidate.airside.dry_air_loading_kg_m2_s,
        t.dry_air_loading_kg_m2_s,
    );
    p.close(
        "water_flow.m3_hr",
        out.water_flow_m3_hr,
        f.anchor.water_flow.m3_hr,
    );
    p.close(
        "candidate.water_volumetric_flow_m3_s (echoed by the duty)",
        input.duty.water_flow_m3_hr / 3600.0,
        f.anchor.candidate.water_volumetric_flow_m3_s,
    );
    p.close(
        "candidate.speedRatio",
        f.anchor.candidate.speed_ratio,
        input.speed_ratio,
    );
    // The two water-balance readings the first screen derives, from the record's own kg/s at the
    // density the run used: the identities hold on both sides.
    p.close(
        "water_flow.kg_s (recorded run's mass flow)",
        f.anchor.water_flow.kg_s,
        f.anchor.water_flow.m3_hr / 3600.0 * f.anchor.water_flow.water_density_kg_m3,
    );
    p.close(
        "headline.evaporation_pct (from the record's own kg/s)",
        100.0 * f.anchor.candidate.water_balance.evaporation_kg_s / f.anchor.water_flow.kg_s,
        h.evaporation_pct,
    );
    p.close(
        "headline.makeup_m3_hr (from the record's own kg/s)",
        f.anchor.candidate.water_balance.makeup_kg_s / f.anchor.water_flow.water_density_kg_m3
            * 3600.0,
        h.makeup_m3_hr,
    );

    // The fan/system chart: the operating point and the tower's resistance on the recorded grid.
    let op = &f.anchor.candidate.fan_operating_point;
    p.close(
        "fan_system_curve.operating_point.x",
        out.fan_system_curve.operating_point.x,
        op.flow_m3_s,
    );
    p.close(
        "fan_system_curve.operating_point.y",
        out.fan_system_curve.operating_point.y,
        op.fan_pressure_pa,
    );
    p.close(
        "headline.airflow_m3_s (operating point)",
        out.airflow_m3_s,
        op.flow_m3_s,
    );
    p.close(
        "headline.fan_power_kw (shaft)",
        out.fan_power_kw,
        op.shaft_power_kw,
    );
    p.close(
        "transfer.fan_efficiency_at_anchor",
        op.efficiency,
        t.fan_efficiency_at_anchor,
    );
    p.same_count(
        "system series points",
        out.fan_system_curve.system.points.len(),
        f.anchor.system_curve.len(),
    );
    for (index, (point, recorded)) in out
        .fan_system_curve
        .system
        .points
        .iter()
        .zip(&f.anchor.system_curve)
        .enumerate()
    {
        p.close(&format!("system[{index}].x"), point.x, recorded.flow_m3_s);
        p.close(&format!("system[{index}].y"), point.y, recorded.total_pa);
    }
    p.same_count(
        "fan series points (one per fan-curve point)",
        out.fan_system_curve.fan.points.len(),
        input.fan.curve.len(),
    );
    for (index, (point, recorded)) in out
        .fan_system_curve
        .fan
        .points
        .iter()
        .zip(&input.fan.curve)
        .enumerate()
    {
        p.close(
            &format!("fan[{index}].x = record flow * speed ratio"),
            point.x,
            recorded.flow_m3_s * input.speed_ratio,
        );
    }

    // The recorded fan curve: a pressure sweep through the engine's own fan solve. Every traced
    // point must lie on the speed-scaled curve this adapter samples for the chart - one curve, two
    // engine entry points.
    let trace = &f.anchor.fan_curve_at_speed;
    p.close("fan trace speedRatio", trace.speed_ratio, input.speed_ratio);
    p.close(
        "fan trace airDensityKgM3",
        trace.air_density_kg_m3,
        t.air_density_kg_m3,
    );
    let engine_fan = engine
        .catalog()
        .fans
        .iter()
        .find(|fan| fan.physics.id == input.fan.id)
        .expect("the fixture carries the fan the run resolved")
        .physics
        .clone();
    for (index, point) in trace.points.iter().enumerate() {
        let sampled = fan_pressure_pa_at_flow(
            &engine_fan,
            point.flow_m3_s,
            trace.speed_ratio,
            trace.air_density_kg_m3,
        )
        .expect("the fan record evaluates");
        p.trace(&format!("fan trace[{index}]"), sampled, point.pressure_pa);
    }

    // The performance chart against the recorded water-flow sweep.
    let sweep = &f.anchor.sweeps.water_mass_flow_kg_s.series[&f.anchor.anchor_fill_id];
    let feasible: Vec<&SweepPoint> = sweep
        .iter()
        .filter(|point| !point.infeasible && point.cold_water_c.is_some())
        .collect();
    p.same_count(
        "performance series points",
        out.thermal_curve.performance.points.len(),
        feasible.len(),
    );
    // The chart's axis is the recorded one: 13 values, of which the engine refuses 5 at this duty -
    // the adapter drops exactly those (the record marks the same condition `infeasible`).
    let infeasible = f.anchor.sweeps.water_mass_flow_kg_s.values.len() - feasible.len();
    p.same_count(
        "recorded axis values",
        f.anchor.sweeps.water_mass_flow_kg_s.values.len(),
        13,
    );
    p.same_count(
        "sweep points the engine refuses (recorded axis minus feasible)",
        infeasible,
        sweep
            .iter()
            .filter(|point| point.infeasible || point.cold_water_c.is_none())
            .count(),
    );
    for (index, (point, recorded)) in out
        .thermal_curve
        .performance
        .points
        .iter()
        .zip(&feasible)
        .enumerate()
    {
        p.close(
            &format!("performance[{index}].x (kg/s at the run's density)"),
            point.x,
            recorded.x / f.anchor.water_flow.water_density_kg_m3 * 3600.0,
        );
        p.close(
            &format!("performance[{index}].y (cold water)"),
            point.y,
            recorded.cold_water_c.expect("filtered to recorded points"),
        );
    }
    p.close(
        "thermal_curve.operating_point.y",
        out.thermal_curve.operating_point.y,
        h.cold_water_c,
    );

    // The worked sheet: the values the engine's own sheet carries, by label.
    let recorded_steps: HashMap<&str, f64> = f
        .anchor
        .worked
        .steps
        .iter()
        .filter_map(|step| step.value.map(|value| (step.label.as_str(), value)))
        .collect();
    let adapter_steps: Vec<(&str, f64)> = out
        .worked_steps
        .iter()
        .filter_map(|step| step.value.map(|value| (step.label.as_str(), value)))
        .collect();
    p.same_count(
        "worked value steps",
        adapter_steps.len(),
        recorded_steps.len(),
    );
    for (label, value) in adapter_steps {
        let recorded = recorded_steps.get(label).unwrap_or_else(|| {
            panic!("the adapter's step \"{label}\" is not in the recorded sheet")
        });
        p.close(&format!("step \"{label}\""), value, *recorded);
    }

    // Provenance: the catalog the run resolved against, and the engine's own warning.
    p.checks += 1;
    assert_eq!(out.provenance.catalog_id, f.catalog.metadata.id);
    assert_eq!(out.provenance.catalog_revision, f.catalog.metadata.revision);
    assert_eq!(out.provenance.catalog_status, f.catalog.metadata.status);
    assert_eq!(
        out.provenance.warning, f.anchor.warning,
        "the engine's own selection warning reached the contract unchanged"
    );

    // The contract's own intent for the third method: the real engine has no in-place catalog
    // authoring, so the UI hides the fill-curve controls.
    assert!(
        engine.fixture_catalog_mut().is_none(),
        "the real engine must not hand out an authoring surface"
    );

    p.report();
}

/// The recorded envelope refusal: `refusals.envelope` records fillAreaM2 400 m2 on a 1.5 m FILM-VF38
/// stack, which starves the water loading; the engine refuses every candidate and the contract gets
/// the refusal as `validation`, naming the engine's own reason.
#[test]
fn the_recorded_refusal_is_reported_with_the_engines_reason() {
    let f = fixture();
    let engine = RealEngine::new(engine_catalog(&f));
    let mut input = recorded_input(&f);
    input.tower.fill_area_m2 = f.refusals.envelope.value;
    let out = engine
        .run(&input)
        .expect("a refusal is an output carrying limits, not an engine error");

    assert!(
        !out.validation.is_empty(),
        "the starved water loading must be refused"
    );
    let reasons: Vec<&str> = out
        .validation
        .iter()
        .map(|limit| limit.message.as_str())
        .collect();
    let expected_count = f
        .refusals
        .envelope
        .run
        .rejection_summary
        .get("fill operating envelope")
        .copied()
        .expect("the record names the envelope as the reason");
    assert_eq!(f.refusals.envelope.run.feasible_candidate_count, 0);
    assert_eq!(reasons.len(), 1, "one combination, one reason: {reasons:?}");
    assert!(
        reasons[0].contains("fill operating envelope"),
        "the engine's own reason word: {}",
        reasons[0]
    );
    assert!(
        reasons[0].contains(&expected_count.to_string()),
        "the record's refusal count: {}",
        reasons[0]
    );
}

/// The cockpit's own default input - the recorded mixed stack FILM-MF20 0.45 m over FILM-WF25
/// 0.90 m - through the adapter, per layer against the fixture's recorded `anchor.layers`.
///
/// **Where the expected values come from.** `anchor.layers` is the fixture's own record of the
/// default scene's per-layer split: its generator solved the two synthetic fills' correlation
/// coefficients so that this stack, at the recorded loadings, carries the fill KaV/L and the fill
/// pressure the recorded single-fill run produced, and recorded each layer's `kavl`, `pressure_pa`
/// and `cooling_share_pct` from the engine's correlation form at those loadings, with the
/// multipliers the layer declares. The comparison below is the adapter's output for the recorded
/// default input against those recorded numbers, in the recorded order.
#[test]
fn the_recorded_default_input_matches_the_recorded_layers_per_layer() {
    let f = fixture();
    let mut p = Parity::new();
    let engine = RealEngine::new(engine_catalog(&f));
    let input = default_input(&f);
    assert_eq!(
        input.fill_layers.len(),
        2,
        "the cockpit's default input is the recorded mixed stack"
    );
    let out = engine
        .run(&input)
        .expect("the recorded default input runs through the adapter");
    assert!(
        out.validation.is_empty(),
        "the recorded default input is feasible: {:?}",
        out.validation
    );

    // One contract layer per declared layer, in the declared order, each against its own recorded
    // entry. The recorded entries sit at the anchor run's loadings and the mixed stack resolves its
    // own operating point ([`RECORDED_SPLIT_REL`] carries the measured difference, 6.6e-9 at the
    // worst); the depth echo stays at `REL`, and it is compared last so a moved input reports the
    // number it moved rather than the echo.
    p.same_count("layers", out.kavl_per_layer.len(), f.anchor.layers.len());
    for (index, (layer, recorded)) in out.kavl_per_layer.iter().zip(&f.anchor.layers).enumerate() {
        assert_eq!(
            layer.index, index,
            "the contract layer index is the stack's own"
        );
        assert_eq!(
            layer.fill_id, recorded.fill_id,
            "layer[{index}]: the recorded order is the declared order (index 0 = top)"
        );
        assert!(
            layer.inside_envelope,
            "layer[{index}] is inside its own fill's recorded limits"
        );
        // The recorded split carries the pair the input declares for that layer.
        let declared = &input.fill_layers[index];
        assert_eq!(
            recorded.thermal_multiplier, declared.thermal_multiplier,
            "layer[{index}]: the recorded thermal multiplier is the declared one"
        );
        assert_eq!(
            recorded.pressure_multiplier, declared.pressure_multiplier,
            "layer[{index}]: the recorded pressure multiplier is the declared one"
        );
        p.close_with(
            RECORDED_SPLIT_REL,
            &format!("layer[{index}].kavl"),
            layer.kavl,
            recorded.kavl,
        );
        p.close_with(
            RECORDED_SPLIT_REL,
            &format!("layer[{index}].pressure_pa"),
            layer.pressure_pa,
            recorded.pressure_pa,
        );
        p.close_with(
            RECORDED_SPLIT_REL,
            &format!("layer[{index}].cooling_share_pct"),
            layer.cooling_share_pct,
            recorded.cooling_share_pct,
        );
        p.close(
            &format!("layer[{index}].depth_m"),
            layer.depth_m,
            recorded.depth_m,
        );
    }

    // The stack's own totals: the adapter's per-layer sums are the recorded split's sums, and the
    // totals they make up are the recorded anchor run's within the same measured difference.
    let recorded_kavl: f64 = f.anchor.layers.iter().map(|layer| layer.kavl).sum();
    let recorded_pa: f64 = f.anchor.layers.iter().map(|layer| layer.pressure_pa).sum();
    let adapter_kavl: f64 = out.kavl_per_layer.iter().map(|layer| layer.kavl).sum();
    let adapter_pa: f64 = out
        .kavl_per_layer
        .iter()
        .map(|layer| layer.pressure_pa)
        .sum();
    p.close_with(
        RECORDED_SPLIT_REL,
        "the layers' kavl sum",
        adapter_kavl,
        recorded_kavl,
    );
    p.close_with(
        RECORDED_SPLIT_REL,
        "the layers' pressure sum",
        adapter_pa,
        recorded_pa,
    );
    p.close_with(
        RECORDED_SPLIT_REL,
        "kavl_total against the recorded available transfer",
        out.kavl_total,
        f.anchor.transfer.available_merkel_number,
    );
    p.close_with(
        RECORDED_SPLIT_REL,
        "total_pressure_pa against the recorded total",
        out.total_pressure_pa,
        f.anchor.zones.total_pa,
    );

    // The zones: one Fill zone per layer, lowest first (the air rises), each carrying its layer's
    // own drop from the engine's layered result.
    p.same_count("zones", out.pressure_by_zone.len(), 9);
    let fill_zones: Vec<_> = out
        .pressure_by_zone
        .iter()
        .filter(|zone| zone.zone == ZoneId::Fill)
        .collect();
    p.same_count("fill zones", fill_zones.len(), 2);
    assert_eq!(
        fill_zones[0].layer,
        Some(1),
        "the lowest layer is met first on the air path"
    );
    assert_eq!(fill_zones[1].layer, Some(0));
    p.close_with(
        RECORDED_SPLIT_REL,
        "zone fill[1] (the lower layer's own drop)",
        fill_zones[0].pressure_pa,
        f.anchor.layers[1].pressure_pa,
    );
    p.close_with(
        RECORDED_SPLIT_REL,
        "zone fill[0] (the upper layer's own drop)",
        fill_zones[1].pressure_pa,
        f.anchor.layers[0].pressure_pa,
    );
    let zone_total: f64 = out
        .pressure_by_zone
        .iter()
        .map(|zone| zone.pressure_pa)
        .sum();
    p.close(
        "the zones sum to the headline total",
        zone_total,
        out.total_pressure_pa,
    );

    p.report();
}

/// The stack shapes the adapter accepts: two layers of different fills (the recorded default input
/// above), two layers of the **same** fill, one layer, and a stack whose layers declare different
/// multiplier pairs - each answered with one contract layer result per declared layer, in the
/// declared order. A single layer too short to carry the duty is answered by the **engine's** own
/// refusal (a `validation`), not by the adapter; an empty stack stays the baseline's own schema
/// refusal.
#[test]
fn mixed_same_type_and_single_layer_stacks_are_all_accepted() {
    let f = fixture();
    let engine = RealEngine::new(engine_catalog(&f));

    // Two layers, one fill: two results, two zones, and two equal layers carry equal numbers.
    let mut same_type = default_input(&f);
    let upper = same_type.fill_layers[0].clone();
    same_type.fill_layers = vec![upper.clone(), upper];
    let out = engine.run(&same_type).expect("a same-fill stack runs");
    assert!(
        out.validation.is_empty(),
        "same-fill stack: {:?}",
        out.validation
    );
    assert_eq!(
        out.kavl_per_layer.len(),
        2,
        "one contract layer per declared layer"
    );
    assert_eq!(out.kavl_per_layer[0].index, 0);
    assert_eq!(out.kavl_per_layer[1].index, 1);
    assert_eq!(out.kavl_per_layer[0].fill_id, "FILM-MF20");
    assert_eq!(out.kavl_per_layer[1].fill_id, "FILM-MF20");
    assert_eq!(
        out.kavl_per_layer[0].kavl, out.kavl_per_layer[1].kavl,
        "two identical layers see the same loadings and carry the same transfer"
    );
    assert_eq!(
        out.pressure_by_zone
            .iter()
            .filter(|zone| zone.zone == ZoneId::Fill)
            .count(),
        2
    );

    // One layer: the recorded single-fill input - one result, index 0, the fill's own identity.
    let single = recorded_input(&f);
    let single_out = engine.run(&single).expect("a single layer runs");
    assert!(
        single_out.validation.is_empty(),
        "single layer: {:?}",
        single_out.validation
    );
    assert_eq!(single_out.kavl_per_layer.len(), 1);
    assert_eq!(single_out.kavl_per_layer[0].index, 0);
    assert_eq!(single_out.kavl_per_layer[0].fill_id, "FILM-VF38");

    // One film layer alone is **accepted** as a stack and then refused by the engine's own duty
    // check: a `validation` carrying the engine's reason, not a schema refusal - the adapter no
    // longer has an opinion about how many layers a stack may have.
    let mut solo_film = default_input(&f);
    solo_film.fill_layers = vec![solo_film.fill_layers[0].clone()];
    let solo = engine
        .run(&solo_film)
        .expect("a single film layer is answered by the engine");
    assert!(
        !solo.validation.is_empty(),
        "the one 0.45 m film layer cannot carry the recorded duty by itself"
    );

    // A stack whose layers declare different pairs: each layer's declared pair reaches that layer's
    // own fill terms exactly once - the engine's one run-level pair is the top layer's, and each
    // other layer's own pair is carried relative to it (see `run_level_multipliers`).
    let uniform = engine
        .run(&default_input(&f))
        .expect("the recorded default input runs");
    let mut non_uniform = default_input(&f);
    non_uniform.fill_layers[1].thermal_multiplier = 0.80;
    let moved = engine
        .run(&non_uniform)
        .expect("a stack whose layers declare different pairs runs");
    assert!(
        (moved.kavl_per_layer[1].kavl / uniform.kavl_per_layer[1].kavl - 0.80 / 0.92).abs() < 1e-12,
        "layer[1]'s declared pair moved its own transfer by the declared ratio: {} against {}",
        moved.kavl_per_layer[1].kavl / uniform.kavl_per_layer[1].kavl,
        0.80 / 0.92
    );
    assert_eq!(
        moved.kavl_per_layer[0].kavl, uniform.kavl_per_layer[0].kavl,
        "the other layer's numbers did not move"
    );

    // An empty stack is the baseline's own refusal, kept.
    let mut empty = default_input(&f);
    empty.fill_layers.clear();
    assert!(matches!(engine.run(&empty), Err(EngineError::Schema(_))));
}

/// An edited duty moves the numbers: the estimate the `duty-edited` frame relies on, driven here
/// through the same input the frame's run uses. Only the water flow changes.
#[test]
fn an_edited_duty_moves_the_numbers() {
    let f = fixture();
    let engine = RealEngine::new(engine_catalog(&f));
    let recorded = engine
        .run(&recorded_input(&f))
        .expect("the recorded duty runs");

    let mut edited_input = recorded_input(&f);
    edited_input.duty.water_flow_m3_hr = EDITED_DUTY_WATER_FLOW_M3_HR;
    let edited = engine.run(&edited_input).expect("the edited duty runs");

    assert!(
        edited.validation.is_empty(),
        "the edited duty must stay feasible: {:?}",
        edited.validation
    );
    assert!(
        (edited.cold_water_c - recorded.cold_water_c).abs() > 0.05,
        "cold water did not move: {} vs {}",
        edited.cold_water_c,
        recorded.cold_water_c
    );
    assert!(
        (edited.total_pressure_pa - recorded.total_pressure_pa).abs() > 0.5,
        "the air-side total did not move: {} vs {}",
        edited.total_pressure_pa,
        recorded.total_pressure_pa
    );
    assert!(
        (edited.water_flow_m3_hr - EDITED_DUTY_WATER_FLOW_M3_HR).abs() < 1e-6,
        "the edited duty's water flow is what the run reports: {}",
        edited.water_flow_m3_hr
    );
    eprintln!(
        "duty edit: water {} -> {} m3/hr; cold water {} -> {} C; total {} -> {} Pa",
        recorded.water_flow_m3_hr,
        edited.water_flow_m3_hr,
        recorded.cold_water_c,
        edited.cold_water_c,
        recorded.total_pressure_pa,
        edited.total_pressure_pa
    );
}

/// Issue #59: an out-of-band speed ratio comes back as a **named** `Limit`, never as a number the
/// engine clamped into the fan record's band, and the record's own rated speed reaches the engine's
/// fan record (the datum the cockpit's rpm read-out multiplies).
#[test]
fn an_out_of_band_speed_ratio_is_a_named_limit_and_the_rated_speed_reaches_the_engine() {
    let f = fixture();
    let mut engine = RealEngine::new(engine_catalog(&f));
    let mut input = recorded_input(&f);
    // The fixture's AX-500 record: the band and the rated speed of issue #59.
    assert_eq!(input.fan.allowed_speed_ratio, [0.70, 1.13]);
    assert_eq!(input.fan.nominal_rpm, Some(233.0));
    let ax_500 = engine
        .catalog()
        .fans
        .iter()
        .find(|fan| fan.id() == "AX-500")
        .expect("the fixture's AX-500 is in the catalog");
    assert_eq!(ax_500.nominal_rpm, Some(233.0));
    assert_eq!(
        ax_500.rpm_at_speed_ratio(input.speed_ratio),
        Some(233.0 * 0.78)
    );

    input.speed_ratio = 1.50;
    let out = engine
        .run(&input)
        .expect("the adapter answers with limits rather than an error");
    assert!(
        !out.validation.is_empty(),
        "an out-of-band ratio must not return clean numbers"
    );
    let limit = out
        .validation
        .iter()
        .find(|limit| limit.field == "fan.speedRatio")
        .expect("the named fan-speed limit");
    assert!(limit.message.contains("AX-500"), "{}", limit.message);
    assert!(limit.message.contains("0.70-1.13"), "{}", limit.message);
    assert!(limit.message.contains("clamped"), "{}", limit.message);
    // The ratio the limit names is the one asked for - 1.50, not the band's edge.
    assert_eq!(limit.value, 1.50);
    assert_eq!(limit.min, Some(0.70));
    assert_eq!(limit.max, Some(1.13));

    // ... and the in-band recorded ratio stays clean: the limit is about the band, not the run.
    input.speed_ratio = 0.78;
    let recorded = engine.run(&input).expect("the recorded ratio runs");
    assert!(recorded.validation.is_empty(), "{:?}", recorded.validation);
}
