//! The engine boundary.
//!
//! The UI never computes physics. It hands an [`EngineInput`] to something that implements [`Engine`]
//! and renders the [`EngineOutput`] it gets back. Field names mirror the real engine's accepted record
//! fields (`fixtures-fields.json`) so a real host engine (wasm) can replace
//! [`FixtureEngine`] without a UI change.
//!
//! There is no money field of any kind in this contract.

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------------ input

/// The thermal duty the tower has to hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Duty {
    pub water_flow_m3_hr: f64,
    pub hot_water_c: f64,
    pub target_cold_water_c: f64,
    pub wet_bulb_c: f64,
    pub dry_bulb_c: f64,
    pub pressure_pa: f64,
    pub salinity_g_kg: f64,
    pub water_quality_class: String,
    pub cycles_of_concentration: f64,
}

/// Tower record - field names are the engine's (`fields.towers`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TowerRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub tower_type: String,
    pub fill_area_m2: f64,
    pub air_free_area_m2: f64,
    pub drift_area_m2: f64,
    pub inlet_area_m2: f64,
    #[serde(default)]
    pub plenum_area_m2: Option<f64>,
    #[serde(default)]
    pub fan_stack_area_m2: Option<f64>,
    pub stack_recovery_factor: f64,
    pub inlet_loss_coefficient: f64,
    pub distribution_loss_coefficient: f64,
    pub support_loss_coefficient: f64,
    pub plenum_loss_coefficient: f64,
    pub fixed_pressure_loss_pa: f64,
    pub spray_zone_height_m: f64,
    pub rain_zone_height_m: f64,
    pub footprint_m2: f64,
    pub max_water_mass_flow_kg_s: f64,
    pub fill_depth_options_m: Vec<f64>,
    pub compatible_fan_ids: Vec<String>,
}

/// One fill layer in the ORDERED stack (index 0 = top, water enters here first).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FillLayer {
    pub fill_id: String,
    pub depth_m: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

/// Fill catalog record (`fields.fills`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub geometry: String,
    pub compatible_tower_types: Vec<String>,
    pub allowed_water_quality_classes: Vec<String>,
    pub thermal: FillThermal,
    pub pressure: FillPressure,
    pub limits: FillLimits,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillThermal {
    pub coefficient_per_m: f64,
    pub reference_water_loading_kg_m2_s: f64,
    pub reference_dry_air_loading_kg_m2_s: f64,
    pub water_exponent: f64,
    pub air_exponent: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillPressure {
    pub coefficient_pa_per_m: f64,
    pub reference_water_loading_kg_m2_s: f64,
    pub reference_dry_air_loading_kg_m2_s: f64,
    pub water_exponent: f64,
    pub air_exponent: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillLimits {
    pub min_water_loading_kg_m2_s: f64,
    pub max_water_loading_kg_m2_s: f64,
    pub min_dry_air_loading_kg_m2_s: f64,
    pub max_dry_air_loading_kg_m2_s: f64,
    pub max_water_temperature_c: f64,
}

/// Drift eliminator record (`fields.driftEliminators`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub max_water_temperature_c: f64,
    pub curve: Vec<DriftPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftPoint {
    pub face_velocity_m_s: f64,
    pub drift_ppm: f64,
    pub pressure_drop_pa: f64,
}

/// Fan record with its curve (`fields.fans`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FanRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub stack_area_m2: f64,
    pub pressure_basis: String,
    pub reference_density_kg_m3: f64,
    #[serde(default)]
    pub stack_recovery_factor: Option<f64>,
    pub allowed_speed_ratio: [f64; 2],
    /// The speed the fan's recorded curve is published at — speed ratio 1.0 — in rpm; the rpm a
    /// read-out shows at a ratio is `speedRatio × nominalRpm`. Absent when the record states no
    /// rated speed: nothing on this path invents one.
    #[serde(default)]
    pub nominal_rpm: Option<f64>,
    pub drive_efficiency: f64,
    pub motor_efficiency: f64,
    pub curve: Vec<FanPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FanPoint {
    pub flow_m3_s: f64,
    pub pressure_pa: f64,
    pub efficiency: f64,
}

/// Nozzle record (`fields.nozzles`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NozzleRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub discharge_coefficient: f64,
    pub orifice_diameter_m: f64,
    #[serde(default)]
    pub reference_water_density_kg_m3: Option<f64>,
}

/// Everything one run needs. The UI owns and edits this; the engine only reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineInput {
    pub duty: Duty,
    pub tower: TowerRecord,
    /// Ordered top -> bottom.
    pub fill_layers: Vec<FillLayer>,
    pub drift: DriftRecord,
    pub fan: FanRecord,
    pub speed_ratio: f64,
    pub nozzle: NozzleRecord,
}

// ----------------------------------------------------------------------------------------- output

/// One pressure zone on the air path, in air-path order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PressureZone {
    pub zone: ZoneId,
    /// Which fill layer, for `ZoneId::Fill`; index into `EngineInput::fill_layers`.
    pub layer: Option<usize>,
    pub label: String,
    pub pressure_pa: f64,
    pub share_pct: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneId {
    Inlet,
    Rain,
    Fill,
    Spray,
    Drift,
    Plenum,
    Stack,
    Fixed,
}

/// Per-layer analysis - shown in the analysis view, never on the first screen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerResult {
    pub index: usize,
    pub fill_id: String,
    pub depth_m: f64,
    pub kavl: f64,
    pub pressure_pa: f64,
    /// Share of the TOTAL available transfer (fill + spray + rain).
    pub cooling_share_pct: f64,
    pub inside_envelope: bool,
}

/// One worked step behind "How calculated".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkedStep {
    pub label: String,
    #[serde(default)]
    pub why: String,
    pub formula: Option<String>,
    pub substitution: Option<String>,
    pub value: Option<f64>,
    pub unit: String,
    pub reference: Option<String>,
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct XY {
    pub x: f64,
    pub y: f64,
}

/// A named series for the two charts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub label: String,
    pub points: Vec<XY>,
}

/// Thermal demand/performance curve: x = water flow (m3/hr), y = cold water (C).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermalCurve {
    pub x_label: String,
    pub y_label: String,
    /// Cold water the tower delivers vs water flow (the performance curve).
    pub performance: Series,
    /// The target cold water the duty demands (a horizontal line).
    pub demand: Series,
    pub operating_point: XY,
}

/// Fan/system curve: x = airflow (m3/s), y = pressure (Pa).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FanSystemCurve {
    pub x_label: String,
    pub y_label: String,
    pub fan: Series,
    pub system: Series,
    pub operating_point: XY,
}

/// A physical limit the run hit. The message names the limit; the field names what carries it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    pub field: String,
    pub message: String,
    pub value: f64,
    pub unit: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// What one run returns. The 11 headline metrics are the first screen; the rest lives behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineOutput {
    pub capability_pct: f64,
    pub water_flow_m3_hr: f64,
    pub cold_water_c: f64,
    pub range_c: f64,
    pub approach_c: f64,
    pub airflow_m3_s: f64,
    pub fan_power_kw: f64,
    pub total_pressure_pa: f64,
    pub total_pressure_mmwg: f64,
    pub evaporation_pct: f64,
    pub makeup_m3_hr: f64,
    pub kavl_total: f64,
    pub kavl_per_layer: Vec<LayerResult>,
    pub pressure_by_zone: Vec<PressureZone>,
    pub worked_steps: Vec<WorkedStep>,
    pub thermal_curve: ThermalCurve,
    pub fan_system_curve: FanSystemCurve,
    /// Empty when the run is clean. Non-empty means the engine refused: no headline numbers are valid.
    pub validation: Vec<Limit>,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub engine: String,
    pub catalog_id: String,
    pub catalog_revision: String,
    pub catalog_status: String,
    pub warning: String,
}

/// Why a run could not be attempted at all (as opposed to a run that returned `validation`).
#[derive(Clone, Debug, PartialEq)]
pub enum EngineError {
    /// The engine is not loaded / not reachable. The UI shows the EngineUnavailable state.
    Unavailable(String),
    /// The input failed schema-level checks before any physics ran.
    Schema(String),
}

/// The boundary. Implemented by [`crate::fixture_engine::FixtureEngine`] here; by a real
/// engine binding later.
pub trait Engine: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, input: &EngineInput) -> Result<EngineOutput, EngineError>;

    /// Optional authoring surface: the catalog whose sampled tables the curve editor may retune in
    /// place. [`crate::fixture_engine::FixtureEngine`] returns its loaded record; the real engine
    /// returns `None` (catalog authoring belongs to its server - decision 5) and the UI then hides
    /// the fill-curve controls instead of pretending they do something.
    fn fixture_catalog_mut(&mut self) -> Option<&mut crate::fixture_engine::FixtureFile> {
        None
    }
}

/// The 11 headline metrics with the exact names/units the first screen shows, plus one plain line each.
pub struct Headline {
    pub name: &'static str,
    pub unit: &'static str,
    pub plain: &'static str,
}

pub const HEADLINES: [Headline; 11] = [
    Headline { name: "capability", unit: "%", plain: "Share of the duty it holds; above 100 % there is headroom." },
    Headline { name: "water flow", unit: "m3/hr", plain: "The circulating water the tower cools at this duty." },
    Headline { name: "cold water", unit: "C", plain: "The water temperature the tower actually reaches." },
    Headline { name: "range", unit: "C", plain: "Hot water minus cold water: how far it is cooled." },
    Headline { name: "approach", unit: "C", plain: "How close the cold water gets to the wet bulb." },
    Headline { name: "airflow", unit: "m3/s", plain: "Air the fan actually moves where its curve meets the tower." },
    Headline { name: "fan power", unit: "kW", plain: "Shaft power at that operating point and efficiency." },
    Headline { name: "total pressure", unit: "Pa / mmWG", plain: "Every air-side loss, summed along the path." },
    Headline { name: "evaporation", unit: "%", plain: "Share of circulating water the air carries off as vapour." },
    Headline { name: "makeup", unit: "m3/hr", plain: "Water to add back: evaporation, bleed and drift." },
    Headline { name: "KaV/L", unit: "-", plain: "Transfer the fill, spray and rain zones supply together." },
];

impl EngineOutput {
    /// Values in `HEADLINES` order (total pressure carries Pa; mmWG is a second value on the same row).
    pub fn headline_values(&self) -> [f64; 11] {
        [
            self.capability_pct,
            self.water_flow_m3_hr,
            self.cold_water_c,
            self.range_c,
            self.approach_c,
            self.airflow_m3_s,
            self.fan_power_kw,
            self.total_pressure_pa,
            self.evaporation_pct,
            self.makeup_m3_hr,
            self.kavl_total,
        ]
    }
}
