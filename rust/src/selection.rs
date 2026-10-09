//! Component selection — port of `src/core/selection.js`, with the ranking replaced by the
//! engineering-objective semantics owner-routed from issue #1 into this port (issue #1, 18
//! September 2026: "this should land in the Rust port (#5), not in the JavaScript engine").
//!
//! What is a faithful port, line for line: the candidate generation (towers → fills → fill
//! depths → drift eliminators → fans → speed ratios), every feasibility constraint and its
//! rejection label, the per-candidate thermal/air-side/fan/nozzle physics, and the inlet-air
//! state. What deliberately diverges from the reference — and from nothing else:
//!
//! * **No economics.** The reference computes `candidateEconomics()` and sorts by it first;
//!   this port has no commercial field anywhere, reads none of the catalog's rate fields and
//!   produces no economics. The superseded reference ranking is documented in
//!   `rust/README.md`, not silently overwritten.
//! * **The ranking is the run's declared objective.** Duty and thermal margin are
//!   constraints, never sort keys. The four objectives are [`Objective`]; the default is
//!   least over-capacity — the smallest unit that meets the duty with the required margin,
//!   i.e. the smallest [`SelectionCandidate::capacity_kg_s`] (the water flow the unit holds
//!   at the required cold-water temperature). See `rust/README.md` for why the sort key is
//!   the capacity rather than the reported capability ratio.
//! * **Two quantities are reported per candidate that the reference never computed**:
//!   `capacityKgS` (an inverse solve at fixed cold-water temperature on the candidate's own
//!   physics) and `capabilityRatio` (available KaV/L over required KaV/L).
//!
//! Three functions of `src/core/water.js` and one of `src/core/merkel.js` that the reviewed
//! slices left out ([`water_density_kg_m3`], [`heat_rejection_kw`],
//! [`volumetric_flow_m3_s_from_mass_flow`], [`estimate_outlet_air_state`]) are ported in this
//! file, because this lane's file fence keeps the already-reviewed modules
//! byte-identical. They are marked as such below; no physics is invented.

use std::collections::HashMap;

use crate::airside::{
    layered_system_pressure_breakdown, DriftEliminatorRecord, FillRecord, LayeredBreakdown,
    LayeredBreakdownInput, SystemPressureBreakdown, TowerRecord,
};
use crate::crossflow::{solve_crossflow_grid, CrossflowGridInput, OutletAirState, CROSSFLOW_MODEL};
use crate::fan::{
    choose_standard_motor, solve_fan_system_intersection, FanRecord, FanSystemIntersection,
    FanSystemIntersectionInput, MotorSelection,
};
use crate::layers::{check_stack_operating_envelope, FillLayerResult, FillStack, LayerTerms};
use crate::merkel::{
    merkel_demand, solve_cold_water_temperature, ColdWaterTemperatureInput, Integration,
    MerkelInput,
};
use crate::nozzle::{
    select_nozzle_arrangement, NozzleArrangement, NozzleArrangementInput, NozzleRecord,
};
use crate::numeric::{assert_positive, solve_bracketed_root, BracketedRootOptions, DomainError};
use crate::psychrometrics::{psychrometric_state, PsychrometricState, PsychrometricStateInput};
use crate::water::water_specific_heat_kj_kg_k;
use crate::water_balance::{
    cooling_tower_water_balance, drift_loss_kg_s, evaporation_from_air_mass_balance, WaterBalance,
    WaterBalanceInput,
};

/// The warning the reference returns, minus its economics clause: this port reads no rate
/// data, so it cannot promise anything about it. Documented in `rust/README.md`.
pub const SELECTION_WARNING: &str = "Selection output is only as valid as the fill, drift, fan, tower-loss and material data supplied. The bundled catalog is synthetic.";

/// Reference's `Math.max(a, b)`: NaN-propagating, unlike `f64::max`.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// Reference's `Math.min(a, b)`: NaN-propagating, unlike `f64::min`.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// The reference's sort comparators are sign-of-difference; NaN compares as equal, which keeps
/// the sort stable exactly as the JavaScript `Array.prototype.sort` does.
fn js_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/* ---------------- the unported remainder of `water.js` / `merkel.js` ---------------- */

/// Port of `waterDensityKgM3` from `src/core/water.js` (see the module header for why it
/// lives here). The reference's 0–100 °C refusal and its `Math.max(0, salinity)` floor are
/// kept.
pub fn water_density_kg_m3(temperature_c: f64, salinity_g_kg: f64) -> Result<f64, DomainError> {
    let t = temperature_c;
    // Spelled as two comparisons rather than `(0.0..=100.0).contains(&t)`: for a NaN input both
    // comparisons are false and the value flows on, exactly as `t < 0 || t > 100` does in the
    // reference, whereas `contains` would refuse NaN.
    let below_range = t < 0.0;
    let above_range = t > 100.0;
    if below_range || above_range {
        return Err(DomainError::new(
            "Prototype water-density correlation is limited to 0–100 °C.",
        ));
    }
    let pure =
        1000.0 * (1.0 - ((t + 288.9414) / (508929.2 * (t + 68.12963))) * (t - 3.9863).powi(2));
    Ok(pure + 0.75 * js_max(0.0, salinity_g_kg))
}

/// Port of `heatRejectionKW` from `src/core/water.js`.
pub fn heat_rejection_kw(
    water_mass_flow_kg_s: f64,
    hot_water_c: f64,
    cold_water_c: f64,
    salinity_g_kg: f64,
) -> Result<f64, DomainError> {
    assert_positive(water_mass_flow_kg_s, "waterMassFlowKgS")?;
    let mean_temperature_c = (hot_water_c + cold_water_c) / 2.0;
    Ok(water_mass_flow_kg_s
        * water_specific_heat_kj_kg_k(mean_temperature_c, salinity_g_kg)
        * (hot_water_c - cold_water_c))
}

/// Port of `volumetricFlowM3SFromMassFlow` from `src/core/water.js`.
pub fn volumetric_flow_m3_s_from_mass_flow(
    water_mass_flow_kg_s: f64,
    temperature_c: f64,
    salinity_g_kg: f64,
) -> Result<f64, DomainError> {
    Ok(water_mass_flow_kg_s / water_density_kg_m3(temperature_c, salinity_g_kg)?)
}

/// Port of `estimateOutletAirState` from `src/core/merkel.js`. The reference's `cpWaterKJkgK`
/// and `pressurePa` defaults are not ported: the selector always passes both explicitly.
pub fn estimate_outlet_air_state(
    inlet_air_state: &PsychrometricState,
    water_to_dry_air_ratio: f64,
    hot_water_c: f64,
    cold_water_c: f64,
    cp_water_kj_kg_k: f64,
    pressure_pa: f64,
) -> Result<OutletAirState, DomainError> {
    let outlet_enthalpy_kj_kg_dry_air = inlet_air_state.enthalpy_kj_kg_dry_air
        + water_to_dry_air_ratio * cp_water_kj_kg_k * (hot_water_c - cold_water_c);
    let upper_bound = js_min(
        120.0,
        js_max(hot_water_c + 35.0, inlet_air_state.dry_bulb_c + 10.0),
    );
    let outlet_dry_bulb_c = crate::crossflow::saturated_temperature_from_enthalpy(
        outlet_enthalpy_kj_kg_dry_air,
        pressure_pa,
        [-50.0, upper_bound],
    )?;
    let outlet_humidity_ratio = crate::psychrometrics::saturation_humidity_ratio(
        outlet_dry_bulb_c,
        pressure_pa,
        crate::psychrometrics::PsychrometricOptions::default(),
    )?;
    Ok(OutletAirState {
        dry_bulb_c: outlet_dry_bulb_c,
        wet_bulb_c: outlet_dry_bulb_c,
        relative_humidity: 1.0,
        enthalpy_kj_kg_dry_air: outlet_enthalpy_kj_kg_dry_air,
        humidity_ratio: outlet_humidity_ratio,
        pressure_pa,
    })
}

/* ---------------- the selector's catalog records ---------------- */

/// `tower.type` — the reference compares it as a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TowerType {
    Counterflow,
    Crossflow,
}

impl TowerType {
    /// The JavaScript spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TowerType::Counterflow => "counterflow",
            TowerType::Crossflow => "crossflow",
        }
    }

    /// Parse the JavaScript spelling; an unknown string is the caller's usage error.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "counterflow" => Some(TowerType::Counterflow),
            "crossflow" => Some(TowerType::Crossflow),
            _ => None,
        }
    }
}

/// Catalog metadata — `id`, `revision` and `status` only. The reference metadata also carries
/// a commercial field; this port does not read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogMetadata {
    pub id: String,
    pub revision: String,
    pub status: String,
}

/// `catalog.waterQualityFactors[name]` — the reference record also carries a third field that
/// exists only for its economics; this port reads only the two physics multipliers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterQualityFactor {
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

/// A tower record plus the selection-only fields `selection.js` reads.
///
/// `fill_stacks` is the ordered-layer contract (issue #54): the tower's **stack variants**, each
/// a complete ordered list of layers that names its own fill ids, depths and multipliers. A
/// tower that declares variants is selected over them — compare mode over complete stacks —
/// instead of over `fill_depth_options_m` × the catalog's fills; `fill_depth_options_m` stays
/// the single-fill contract's own enumeration (one layer per fill × depth option).
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionTower {
    pub id: String,
    pub tower_type: TowerType,
    pub physics: TowerRecord,
    pub footprint_m2: f64,
    pub max_water_mass_flow_kg_s: f64,
    pub fill_depth_options_m: Vec<f64>,
    pub fill_stacks: Vec<FillStack>,
    pub compatible_fan_ids: Vec<String>,
}

/// A fill record plus its selection-only compatibility fields.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionFill {
    pub physics: FillRecord,
    pub compatible_tower_types: Vec<TowerType>,
}

/// A drift-eliminator record plus its selection-only temperature limit.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionDriftEliminator {
    pub physics: DriftEliminatorRecord,
    pub max_water_temperature_c: f64,
}

/// A fan record plus its selection-only speed window and efficiencies.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionFan {
    pub physics: FanRecord,
    pub allowed_speed_ratio: [f64; 2],
    /// The speed at which the recorded curve is published — speed ratio 1.0 — in rpm. The rpm a
    /// read-out shows at a ratio is `speed_ratio × nominal_rpm`; `None` when the record states
    /// none, because an unstated datum is never invented (see [`SelectionFan::rpm_at_speed_ratio`]).
    pub nominal_rpm: Option<f64>,
    pub drive_efficiency: f64,
    pub motor_efficiency: f64,
}

/// The catalog the selector runs against — the records `selection.js` reads, never fetched.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionCatalog {
    pub metadata: CatalogMetadata,
    pub water_quality_factors: HashMap<String, WaterQualityFactor>,
    pub towers: Vec<SelectionTower>,
    pub fills: Vec<SelectionFill>,
    pub drift_eliminators: Vec<SelectionDriftEliminator>,
    pub fans: Vec<SelectionFan>,
    pub nozzles: Vec<NozzleRecord>,
}

/* ---------------- requirements and the run's objective ---------------- */

/// The run's declared ranking objective. Duty and thermal margin are constraints under every
/// one of these; they are never sort keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Objective {
    /// The default: the smallest unit that meets the duty with the required margin, i.e. the
    /// smallest `capacityKgS` among the feasible candidates.
    #[default]
    LeastOverCapacity,
    /// Smallest `electricalInputKW`.
    LowestElectricalInput,
    /// Smallest `makeupKgS`.
    LowestMakeupWater,
    /// Smallest `airside.totalPa`.
    LowestTotalAirSidePressure,
}

impl Objective {
    /// The four objectives, in the order the README documents them.
    pub const ALL: [Objective; 4] = [
        Objective::LeastOverCapacity,
        Objective::LowestElectricalInput,
        Objective::LowestMakeupWater,
        Objective::LowestTotalAirSidePressure,
    ];

    /// The reference-facing spelling (`--objective` accepts exactly this).
    pub fn as_str(self) -> &'static str {
        match self {
            Objective::LeastOverCapacity => "least-over-capacity",
            Objective::LowestElectricalInput => "lowest-electrical-input",
            Objective::LowestMakeupWater => "lowest-makeup-water",
            Objective::LowestTotalAirSidePressure => "lowest-total-air-side-pressure",
        }
    }

    /// Parse the spelling; an unknown string is the caller's usage error.
    pub fn parse(raw: &str) -> Option<Self> {
        Objective::ALL
            .into_iter()
            .find(|objective| objective.as_str() == raw)
    }
}

/// The reference's `defaultSelectionRequirements()`, minus the six inputs that only fed its
/// economics (the energy and water rates, annual maintenance, analysis period and present-value
/// rate, and the operating hours); `rust/README.md` names them.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionRequirements {
    /// Default 200 kg/s.
    pub water_mass_flow_kg_s: f64,
    /// Default 42 °C.
    pub hot_water_c: f64,
    /// Default 32 °C.
    pub target_cold_water_c: f64,
    /// Default 27 °C.
    pub wet_bulb_c: f64,
    /// Default 33 °C.
    pub dry_bulb_c: f64,
    /// Default 101 325 Pa.
    pub pressure_pa: f64,
    /// Default 0 g/kg.
    pub salinity_g_kg: f64,
    /// Default `"moderate"`.
    pub water_quality_class: String,
    /// Default 4.
    pub cycles_of_concentration: f64,
    /// Default 30 ppm.
    pub max_drift_ppm: f64,
    /// Default 75 kW.
    pub max_electrical_input_kw: f64,
    /// Default 130 m².
    pub max_footprint_m2: f64,
    /// Default 0.3 °C — a real margin, because the default objective must not settle on a
    /// unit sitting exactly on the thermal constraint (the reference's own note).
    pub minimum_thermal_margin_c: f64,
    /// Default 65 000 Pa.
    pub nozzle_pressure_drop_pa: f64,
    /// Default `[0.78, 0.88, 0.98, 1.08]`.
    pub speed_ratios: Vec<f64>,
}

impl Default for SelectionRequirements {
    fn default() -> Self {
        Self {
            water_mass_flow_kg_s: 200.0,
            hot_water_c: 42.0,
            target_cold_water_c: 32.0,
            wet_bulb_c: 27.0,
            dry_bulb_c: 33.0,
            pressure_pa: 101_325.0,
            salinity_g_kg: 0.0,
            water_quality_class: "moderate".to_string(),
            cycles_of_concentration: 4.0,
            max_drift_ppm: 30.0,
            max_electrical_input_kw: 75.0,
            max_footprint_m2: 130.0,
            minimum_thermal_margin_c: 0.3,
            nozzle_pressure_drop_pa: 65_000.0,
            speed_ratios: vec![0.78, 0.88, 0.98, 1.08],
        }
    }
}

/// Inputs for [`run_selection`] / [`select_cooling_tower_components`].
#[derive(Clone, Debug)]
pub struct SelectionInput<'a> {
    pub requirements: SelectionRequirements,
    pub catalog: &'a SelectionCatalog,
    pub objective: Objective,
    /// The reference's `maxResults`, default 30.
    pub max_results: usize,
}

impl<'a> SelectionInput<'a> {
    /// The reference defaults: `defaultSelectionRequirements()`, the default objective
    /// (least over-capacity) and `maxResults` 30.
    pub fn new(catalog: &'a SelectionCatalog) -> Self {
        Self {
            requirements: SelectionRequirements::default(),
            catalog,
            objective: Objective::default(),
            max_results: 30,
        }
    }

    /// Override the requirements wholesale, as the reference's spread does.
    pub fn with_requirements(mut self, requirements: SelectionRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    /// The run's declared objective.
    pub fn with_objective(mut self, objective: Objective) -> Self {
        self.objective = objective;
        self
    }

    /// The reference's `maxResults` argument.
    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results;
        self
    }
}

/* ---------------- results ---------------- */

/// The per-candidate thermal model, flattened to the fields the selector reports. The two
/// branches keep their reference `model` string.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalModel {
    pub model: &'static str,
    pub cold_water_c: f64,
    pub approach_c: f64,
    pub range_c: f64,
    pub heat_transfer_kw: f64,
    pub cp_water_kj_kg_k: f64,
    pub outlet_air_state: OutletAirState,
}

/// One feasible candidate — everything the reference pushed into `feasible`, plus the two new
/// reported quantities and minus the economics object it also pushed.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionCandidate {
    /// 1-based position in the declared objective's order (assigned to the reported slice).
    pub rank: usize,
    pub tower_id: String,
    /// The fill stack's identity: the fill id for a one-layer stack (the single-fill
    /// candidate's identity, byte for byte), or the stack's own name — `A@0.45+B@0.9` — for a
    /// mixed stack.
    pub fill_id: String,
    pub drift_eliminator_id: String,
    pub fan_id: String,
    /// The stack's total depth, top to bottom.
    pub fill_depth_m: f64,
    /// The stack's layers, in physical order (top first), with the results each layer
    /// contributed.
    pub fill_layers: Vec<FillLayerResult>,
    pub speed_ratio: f64,
    pub fan_operating_point: FanSystemIntersection,
    pub airside: SystemPressureBreakdown,
    pub thermal: ThermalModel,
    pub thermal_margin_c: f64,
    pub motor: MotorSelection,
    pub electrical_input_kw: f64,
    pub water_balance: WaterBalance,
    pub nozzle: NozzleArrangement,
    pub nozzle_option_count: usize,
    pub water_volumetric_flow_m3_s: f64,
    /// The water flow this unit holds at the required cold-water temperature and wet bulb.
    /// `None` only if the inverse solve refuses; the bundled catalog resolves all of them.
    pub capacity_kg_s: Option<f64>,
    /// Available KaV/L over required KaV/L at the design duty.
    pub capability_ratio: Option<f64>,
    pub provenance_status: String,
}

impl SelectionCandidate {
    /// The metric the objective ranks by.
    pub fn objective_metric(&self, objective: Objective) -> Option<f64> {
        match objective {
            Objective::LeastOverCapacity => self.capacity_kg_s,
            Objective::LowestElectricalInput => Some(self.electrical_input_kw),
            Objective::LowestMakeupWater => Some(self.water_balance.makeup_kg_s),
            Objective::LowestTotalAirSidePressure => Some(self.airside.total_pa),
        }
    }

    /// The candidate's identity as matched between the engines: tower, fill, drift
    /// eliminator, fan, fill depth, speed ratio.
    pub fn identity(&self) -> (&str, &str, &str, &str, f64, f64) {
        (
            &self.tower_id,
            &self.fill_id,
            &self.drift_eliminator_id,
            &self.fan_id,
            self.fill_depth_m,
            self.speed_ratio,
        )
    }
}

/// The full feasible set, ranked under the declared objective.
#[derive(Clone, Debug)]
pub struct SelectionRun {
    pub objective: Objective,
    pub requirements: SelectionRequirements,
    pub catalog_metadata: CatalogMetadata,
    pub inlet_air_state: PsychrometricState,
    pub candidates: Vec<SelectionCandidate>,
    pub rejection_summary: Vec<(String, usize)>,
    pub warning: String,
}

impl SelectionRun {
    /// The number of feasible candidates, as the reference counts it.
    pub fn feasible_candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// The whole feasible set's order under one objective, as indices into
    /// [`SelectionRun::candidates`].
    pub fn order_for(&self, objective: Objective) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.candidates.len()).collect();
        indices.sort_by(|&a, &b| {
            compare_candidates(&self.candidates[a], &self.candidates[b], objective)
        });
        indices
    }
}

/// The reference-shaped result: the declared objective's first `maxResults` candidates, each
/// with its 1-based rank.
#[derive(Clone, Debug)]
pub struct SelectionResult {
    pub objective: Objective,
    pub requirements: SelectionRequirements,
    pub catalog_metadata: CatalogMetadata,
    pub inlet_air_state: PsychrometricState,
    pub results: Vec<SelectionCandidate>,
    pub feasible_candidate_count: usize,
    pub rejection_summary: Vec<(String, usize)>,
    pub warning: String,
}

/* ---------------- the ranking ---------------- */

/// Rank the feasible set under `objective`.
///
/// The primary key is the objective's own metric — never the duty and never the thermal
/// margin, which are constraints. Ties are broken by the remaining metrics in a fixed,
/// documented order (electrical input, make-up water, total air-side pressure, capability
/// ratio, capacity) and finally by the candidate identity, so every order is total and
/// reproducible from the objective alone.
pub fn rank_candidates(candidates: &mut [SelectionCandidate], objective: Objective) {
    candidates.sort_by(|a, b| compare_candidates(a, b, objective));
}

fn compare_candidates(
    a: &SelectionCandidate,
    b: &SelectionCandidate,
    objective: Objective,
) -> std::cmp::Ordering {
    compare_optional(a.objective_metric(objective), b.objective_metric(objective))
        .then_with(|| js_cmp(a.electrical_input_kw, b.electrical_input_kw))
        .then_with(|| js_cmp(a.water_balance.makeup_kg_s, b.water_balance.makeup_kg_s))
        .then_with(|| js_cmp(a.airside.total_pa, b.airside.total_pa))
        .then_with(|| compare_optional(a.capability_ratio, b.capability_ratio))
        .then_with(|| compare_optional(a.capacity_kg_s, b.capacity_kg_s))
        .then_with(|| {
            let (a_tower, a_fill, a_drift, a_fan, a_depth, a_speed) = a.identity();
            let (b_tower, b_fill, b_drift, b_fan, b_depth, b_speed) = b.identity();
            a_tower
                .cmp(b_tower)
                .then_with(|| a_fill.cmp(b_fill))
                .then_with(|| a_drift.cmp(b_drift))
                .then_with(|| a_fan.cmp(b_fan))
                .then_with(|| a_depth.total_cmp(&b_depth))
                .then_with(|| a_speed.total_cmp(&b_speed))
        })
}

/// A metric that may be unresolved: `None` sorts after every `Some`.
fn compare_optional(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => js_cmp(a, b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/* ---------------- the per-candidate physics ---------------- */

/// Everything one candidate's physics needs from the run.
struct CandidateContext<'a> {
    requirements: &'a SelectionRequirements,
    tower: &'a SelectionTower,
    /// The candidate's fill stack, and its layers resolved against the catalog. A one-layer
    /// stack is what the single-fill contract enumerates.
    stack: &'a FillStack,
    layers: &'a [LayerTerms<'a>],
    drift_eliminator: &'a SelectionDriftEliminator,
    fan: &'a SelectionFan,
    speed_ratio: f64,
    quality: &'a WaterQualityFactor,
    inlet_air: &'a PsychrometricState,
}

impl CandidateContext<'_> {
    /// The reference's `systemPressureFn`, now the layered breakdown: the whole stack is
    /// evaluated at one flow and one water flow, and its fill terms are the layers' totals.
    fn breakdown(
        &self,
        volumetric_air_flow_m3_s: f64,
        water_mass_flow_kg_s: f64,
    ) -> Result<LayeredBreakdown, DomainError> {
        layered_system_pressure_breakdown(LayeredBreakdownInput {
            tower: &self.tower.physics,
            layers: self.layers,
            drift_eliminator: &self.drift_eliminator.physics,
            fan: Some(&self.fan.physics),
            volumetric_air_flow_m3_s,
            dry_air_density_kg_m3: self.inlet_air.dry_air_density_kg_m3,
            moist_air_density_kg_m3: self.inlet_air.moist_air_density_kg_m3,
            water_mass_flow_kg_s,
            thermal_multiplier: self.quality.thermal_multiplier,
            pressure_multiplier: self.quality.pressure_multiplier,
        })
    }

    /// `systemPressureFn` — the reference's per-flow breakdown closure.
    fn system_pressure_pa(
        &self,
        volumetric_air_flow_m3_s: f64,
        water_mass_flow_kg_s: f64,
    ) -> Result<f64, DomainError> {
        Ok(self
            .breakdown(volumetric_air_flow_m3_s, water_mass_flow_kg_s)?
            .breakdown
            .total_pa)
    }

    /// The fan operating point and the air-side breakdown at one water flow — the two calls
    /// the reference makes before the thermal model.
    fn unit_airside(
        &self,
        water_mass_flow_kg_s: f64,
    ) -> Result<(FanSystemIntersection, LayeredBreakdown), DomainError> {
        let fan_operating_point = solve_fan_system_intersection(
            &FanSystemIntersectionInput::new(
                &self.fan.physics,
                self.inlet_air.moist_air_density_kg_m3,
            )
            .with_speed_ratio(self.speed_ratio),
            |flow_m3_s| self.system_pressure_pa(flow_m3_s, water_mass_flow_kg_s),
        )?;
        let airside = self.breakdown(fan_operating_point.flow_m3_s, water_mass_flow_kg_s)?;
        Ok((fan_operating_point, airside))
    }

    /// The whole unit at one water flow: air-side, fan balance, then the thermal model. The
    /// capacity inverse solve walks this closure.
    fn unit_cold_water_c(&self, water_mass_flow_kg_s: f64) -> Result<f64, DomainError> {
        let (_, airside) = self.unit_airside(water_mass_flow_kg_s)?;
        Ok(thermal_model(
            self.tower.tower_type,
            self.requirements,
            water_mass_flow_kg_s,
            airside.breakdown.dry_air_mass_flow_kg_s,
            airside.breakdown.available_merkel_number,
        )?
        .cold_water_c)
    }

    /// `capacity` — the water flow the unit holds at the required cold-water temperature,
    /// by bracketed root on the unit's own physics. The duty flow is the lower bracket: it is
    /// feasible, so its cold water is at or below the target. The upper bracket expands by
    /// 1.5× until the unit's cold water crosses the target. `None` if the expansion or the
    /// solve refuses (which the bundled catalog never triggers).
    fn capacity_kg_s(&self) -> Option<f64> {
        let target_c = self.requirements.target_cold_water_c;
        let duty_kg_s = self.requirements.water_mass_flow_kg_s;
        let residual = |water_mass_flow_kg_s: f64| -> Result<f64, DomainError> {
            Ok(self.unit_cold_water_c(water_mass_flow_kg_s)? - target_c)
        };
        // The duty flow is the lower bracket: every candidate here already cleared the
        // thermal-margin constraint, so its cold water sits at or below the target.
        let f_lower = residual(duty_kg_s).ok()?;
        if f_lower >= 0.0 {
            return Some(duty_kg_s);
        }
        let mut upper = duty_kg_s;
        let mut f_upper = f_lower;
        for _ in 0..40 {
            if f_upper >= 0.0 {
                break;
            }
            upper *= 1.5;
            f_upper = residual(upper).ok()?;
        }
        if f_upper < 0.0 {
            return None;
        }
        solve_bracketed_root(
            residual,
            duty_kg_s,
            upper,
            BracketedRootOptions {
                tolerance: 1e-9,
                x_tolerance: Some(1e-6),
                max_iterations: 160,
            },
        )
        .ok()
    }

    /// `capabilityRatio` — available KaV/L over required KaV/L at the design duty.
    ///
    /// Counterflow: the required KaV/L is the Merkel demand evaluated at the required
    /// cold-water temperature, which is the model's own inverse (`solveColdWaterTemperature`
    /// solves demand(cold) = available), so no new solve is needed. Crossflow: the grid has no
    /// scalar demand form, so the same quantity is bracketed and bisected on `availableMerkelNumber`
    /// with the already-ported root machinery.
    fn capability_ratio(&self, airside: &SystemPressureBreakdown) -> Option<f64> {
        let available = airside.available_merkel_number;
        let requirements = self.requirements;
        let target_c = requirements.target_cold_water_c;
        match self.tower.tower_type {
            TowerType::Counterflow => {
                let water_to_dry_air_ratio =
                    requirements.water_mass_flow_kg_s / airside.dry_air_mass_flow_kg_s;
                let demand = merkel_demand(&MerkelInput {
                    pressure_pa: requirements.pressure_pa,
                    salinity_g_kg: requirements.salinity_g_kg,
                    integration: Integration::Chebyshev4,
                    ..MerkelInput::new(
                        requirements.hot_water_c,
                        target_c,
                        requirements.wet_bulb_c,
                        requirements.dry_bulb_c,
                        water_to_dry_air_ratio,
                    )
                })
                .ok()?;
                (demand.merkel_number > 0.0).then(|| available / demand.merkel_number)
            }
            TowerType::Crossflow => {
                let cold_at = |available_merkel_number: f64| -> Result<f64, DomainError> {
                    Ok(solve_crossflow_grid(
                        &CrossflowGridInput::new(
                            requirements.hot_water_c,
                            requirements.dry_bulb_c,
                            requirements.wet_bulb_c,
                            requirements.water_mass_flow_kg_s,
                            airside.dry_air_mass_flow_kg_s,
                            available_merkel_number,
                        )
                        .with_pressure(requirements.pressure_pa)
                        .with_salinity(requirements.salinity_g_kg),
                    )?
                    .cold_water_c)
                };
                let residual = |available_merkel_number: f64| -> Result<f64, DomainError> {
                    Ok(cold_at(available_merkel_number)? - target_c)
                };
                let f_available = residual(available).ok()?;
                if f_available >= 0.0 {
                    return Some(1.0);
                }
                let mut lower = available;
                let mut f_lower = f_available;
                for _ in 0..60 {
                    if f_lower >= 0.0 {
                        break;
                    }
                    lower *= 0.5;
                    f_lower = residual(lower).ok()?;
                }
                if f_lower < 0.0 {
                    return None;
                }
                let required_merkel_number = solve_bracketed_root(
                    residual,
                    lower,
                    available,
                    BracketedRootOptions {
                        tolerance: 1e-9,
                        x_tolerance: Some(1e-9),
                        max_iterations: 160,
                    },
                )
                .ok()?;
                (required_merkel_number > 0.0).then(|| available / required_merkel_number)
            }
        }
    }
}

/// The candidate thermal model — the reference's `thermalModelForTower`, where the crossflow
/// branch is the grid solve and everything else is the counterflow Merkel solve plus the
/// saturated outlet-air estimate.
fn thermal_model(
    tower_type: TowerType,
    requirements: &SelectionRequirements,
    water_mass_flow_kg_s: f64,
    dry_air_mass_flow_kg_s: f64,
    available_merkel_number: f64,
) -> Result<ThermalModel, DomainError> {
    if tower_type == TowerType::Crossflow {
        let result = solve_crossflow_grid(
            &CrossflowGridInput::new(
                requirements.hot_water_c,
                requirements.dry_bulb_c,
                requirements.wet_bulb_c,
                water_mass_flow_kg_s,
                dry_air_mass_flow_kg_s,
                available_merkel_number,
            )
            .with_pressure(requirements.pressure_pa)
            .with_salinity(requirements.salinity_g_kg),
        )?;
        return Ok(ThermalModel {
            model: CROSSFLOW_MODEL,
            cold_water_c: result.cold_water_c,
            approach_c: result.approach_c,
            range_c: result.range_c,
            heat_transfer_kw: result.heat_transfer_kw,
            cp_water_kj_kg_k: result.cp_water_kj_kg_k,
            outlet_air_state: result.outlet_air_state,
        });
    }

    let water_to_dry_air_ratio = water_mass_flow_kg_s / dry_air_mass_flow_kg_s;
    let thermal = solve_cold_water_temperature(&ColdWaterTemperatureInput {
        pressure_pa: requirements.pressure_pa,
        salinity_g_kg: requirements.salinity_g_kg,
        integration: Integration::Chebyshev4,
        ..ColdWaterTemperatureInput::new(
            requirements.hot_water_c,
            requirements.wet_bulb_c,
            requirements.dry_bulb_c,
            water_to_dry_air_ratio,
            available_merkel_number,
        )
    })?;
    let outlet_air_state = estimate_outlet_air_state(
        &thermal.inlet_air_state,
        water_to_dry_air_ratio,
        requirements.hot_water_c,
        thermal.cold_water_c,
        thermal.cp_water_kj_kg_k,
        requirements.pressure_pa,
    )?;
    let heat_transfer_kw = heat_rejection_kw(
        water_mass_flow_kg_s,
        requirements.hot_water_c,
        thermal.cold_water_c,
        requirements.salinity_g_kg,
    )?;
    Ok(ThermalModel {
        model: "counterflow Merkel",
        cold_water_c: thermal.cold_water_c,
        approach_c: thermal.approach_c,
        range_c: thermal.range_c,
        heat_transfer_kw,
        cp_water_kj_kg_k: thermal.cp_water_kj_kg_k,
        outlet_air_state,
    })
}

/// Why a candidate is not feasible: a labelled constraint failure, or a physics refusal the
/// reference's `catch` maps onto `'numerical or curve domain'` (the reference discards the
/// error object there too).
enum CandidateFailure {
    Constraint(&'static str),
    Domain,
}

/// The reference's per-candidate body — everything inside its `try`, plus the whole build.
fn evaluate_candidate(
    context: &CandidateContext<'_>,
    catalog: &SelectionCatalog,
    drift_velocity_envelope_ms: [f64; 2],
) -> Result<SelectionCandidate, CandidateFailure> {
    let requirements = context.requirements;
    let (fan_operating_point, airside) = context
        .unit_airside(requirements.water_mass_flow_kg_s)
        .map_err(|_| CandidateFailure::Domain)?;

    if airside.breakdown.drift_velocity_ms < drift_velocity_envelope_ms[0]
        || airside.breakdown.drift_velocity_ms > drift_velocity_envelope_ms[1]
    {
        return Err(CandidateFailure::Constraint("drift velocity range"));
    }
    // Every layer of the stack is checked against its OWN fill's limits at the loadings the
    // stack sees; a one-layer stack is exactly the single-fill envelope the reference checks.
    let envelope = check_stack_operating_envelope(
        context.layers,
        airside.breakdown.water_loading_kg_m2_s,
        airside.breakdown.dry_air_loading_kg_m2_s,
        Some(requirements.hot_water_c),
        Some(&requirements.water_quality_class),
    );
    if !envelope.is_empty() {
        return Err(CandidateFailure::Constraint("fill operating envelope"));
    }

    let thermal = thermal_model(
        context.tower.tower_type,
        requirements,
        requirements.water_mass_flow_kg_s,
        airside.breakdown.dry_air_mass_flow_kg_s,
        airside.breakdown.available_merkel_number,
    )
    .map_err(|_| CandidateFailure::Domain)?;
    let thermal_margin_c = requirements.target_cold_water_c - thermal.cold_water_c;
    if thermal_margin_c < requirements.minimum_thermal_margin_c {
        return Err(CandidateFailure::Constraint("thermal duty"));
    }
    if airside.breakdown.drift_ppm > requirements.max_drift_ppm {
        return Err(CandidateFailure::Constraint("drift limit"));
    }

    let motor = choose_standard_motor(
        fan_operating_point.shaft_power_kw,
        context.fan.drive_efficiency,
        1.1,
    )
    .map_err(|_| CandidateFailure::Domain)?;
    if motor.selected_motor_kw.is_none() {
        return Err(CandidateFailure::Constraint("motor size unavailable"));
    }
    let electrical_input_kw = fan_operating_point.shaft_power_kw
        / context.fan.drive_efficiency
        / context.fan.motor_efficiency;
    if electrical_input_kw > requirements.max_electrical_input_kw {
        return Err(CandidateFailure::Constraint("power limit"));
    }

    let outlet_air_state = thermal.outlet_air_state;
    let evaporation_kg_s = evaporation_from_air_mass_balance(
        airside.breakdown.dry_air_mass_flow_kg_s,
        context.inlet_air.humidity_ratio,
        outlet_air_state.humidity_ratio,
    )
    .map_err(|_| CandidateFailure::Domain)?;
    let drift_kg_s = drift_loss_kg_s(
        requirements.water_mass_flow_kg_s,
        airside.breakdown.drift_ppm,
    )
    .map_err(|_| CandidateFailure::Domain)?;
    let water_balance = cooling_tower_water_balance(
        &WaterBalanceInput::new(evaporation_kg_s)
            .with_drift_kg_s(drift_kg_s)
            .with_cycles_of_concentration(requirements.cycles_of_concentration),
    )
    .map_err(|_| CandidateFailure::Domain)?;
    let water_volumetric_flow_m3_s = volumetric_flow_m3_s_from_mass_flow(
        requirements.water_mass_flow_kg_s,
        (requirements.hot_water_c + requirements.target_cold_water_c) / 2.0,
        requirements.salinity_g_kg,
    )
    .map_err(|_| CandidateFailure::Domain)?;
    let nozzle_options = select_nozzle_arrangement(&NozzleArrangementInput::new(
        water_volumetric_flow_m3_s,
        &catalog.nozzles,
        requirements.nozzle_pressure_drop_pa,
    ))
    .map_err(|_| CandidateFailure::Domain)?;
    let nozzle = nozzle_options
        .first()
        .expect("the selector refuses an empty arrangement list")
        .clone();

    // The two new reported quantities. A refusal here must never remove the candidate from
    // the feasible set — the reference has no such step — so both degrade to `None`.
    let capacity_kg_s = context.capacity_kg_s();
    let capability_ratio = context.capability_ratio(&airside.breakdown);
    let (fill_id, fill_depth_m) = context.stack.identity();

    Ok(SelectionCandidate {
        rank: 0,
        tower_id: context.tower.id.clone(),
        fill_id,
        drift_eliminator_id: context.drift_eliminator.physics.id.clone(),
        fan_id: context.fan.id().to_string(),
        fill_depth_m,
        fill_layers: airside.fill_layers,
        speed_ratio: context.speed_ratio,
        fan_operating_point,
        airside: airside.breakdown,
        thermal,
        thermal_margin_c,
        motor,
        electrical_input_kw,
        water_balance,
        nozzle,
        nozzle_option_count: nozzle_options.len(),
        water_volumetric_flow_m3_s,
        capacity_kg_s,
        capability_ratio,
        provenance_status: String::new(),
    })
}

impl SelectionFan {
    /// The fan's identity, as `fan.id` in the reference.
    pub fn id(&self) -> &str {
        &self.physics.id
    }

    /// The fan's speed in rpm at a speed ratio: `nominal_rpm × speed_ratio`.
    ///
    /// `None` when the record states no rated speed: the ratio is still the engine's own input, but
    /// there is no datum to turn it into an rpm, and nothing here invents one.
    pub fn rpm_at_speed_ratio(&self, speed_ratio: f64) -> Option<f64> {
        self.nominal_rpm.map(|nominal| nominal * speed_ratio)
    }

    /// `None` inside the fan's validity band; the named [`FanSpeedRatioLimit`] outside it.
    pub fn speed_ratio_limit(&self, speed_ratio: f64) -> Option<FanSpeedRatioLimit> {
        fan_speed_ratio_limit(&self.physics.id, self.allowed_speed_ratio, speed_ratio)
    }
}

/// The fan record's validity band, refused **by name** rather than clamped: the ratio a run asked
/// for against the band the record allows (`allowed_speed_ratio`).
///
/// A caller can tell "outside the fan's validity band" (this) from a computed value; nothing on
/// this path rounds a ratio into the band and reports it as if the engine had evaluated it.
#[derive(Clone, Debug, PartialEq)]
pub struct FanSpeedRatioLimit {
    pub fan_id: String,
    pub speed_ratio: f64,
    pub min: f64,
    pub max: f64,
}

impl FanSpeedRatioLimit {
    /// The rejection label the selector counts this refusal under (the reference's own spelling).
    pub const LABEL: &'static str = "fan speed range";
}

/// `None` inside the band `[min, max]`; the named [`FanSpeedRatioLimit`] outside it. The band-level
/// form of [`SelectionFan::speed_ratio_limit`], so a caller that carries the record's own band (the
/// adapter, checking its input) names the same outcome without rebuilding a record.
pub fn fan_speed_ratio_limit(
    fan_id: &str,
    allowed_speed_ratio: [f64; 2],
    speed_ratio: f64,
) -> Option<FanSpeedRatioLimit> {
    let [min, max] = allowed_speed_ratio;
    if speed_ratio < min || speed_ratio > max {
        return Some(FanSpeedRatioLimit {
            fan_id: fan_id.to_string(),
            speed_ratio,
            min,
            max,
        });
    }
    None
}

/* ---------------- the run ---------------- */

/// Count one rejection under its label, keeping the reference's insertion order for the ties
/// its summary sorts (`counts.sort_by_key(Reverse(count))` is stable).
fn count_rejection(counts: &mut Vec<(String, usize)>, reason: &str) {
    if let Some(entry) = counts.iter_mut().find(|(label, _)| label == reason) {
        entry.1 += 1;
    } else {
        counts.push((reason.to_string(), 1));
    }
}

/// Everything one tower's stack sweep needs: the catalog, the tower, the resolved stack and the
/// run's own inputs.
struct SweepContext<'a> {
    catalog: &'a SelectionCatalog,
    tower: &'a SelectionTower,
    stack: &'a FillStack,
    layers: &'a [LayerTerms<'a>],
    requirements: &'a SelectionRequirements,
    inlet_air: &'a PsychrometricState,
    quality: &'a WaterQualityFactor,
}

/// One resolved stack on one tower: sweep every drift eliminator, compatible fan and speed
/// ratio the requirements allow, appending the candidates the physics accepts and counting
/// every rejection under the reference's own label — in the reference's nesting order, so the
/// recorded rejection summary keeps its insertion order.
fn sweep_stack(
    sweep: &SweepContext<'_>,
    compatible_fans: &[&SelectionFan],
    candidates: &mut Vec<SelectionCandidate>,
    counts: &mut Vec<(String, usize)>,
) {
    for drift_eliminator in &sweep.catalog.drift_eliminators {
        if sweep.requirements.hot_water_c > drift_eliminator.max_water_temperature_c {
            count_rejection(counts, "drift material temperature");
            continue;
        }
        let drift_velocity_envelope_ms = [
            drift_eliminator
                .physics
                .curve
                .iter()
                .fold(f64::INFINITY, |acc, point| {
                    js_min(acc, point.face_velocity_ms)
                }),
            drift_eliminator
                .physics
                .curve
                .iter()
                .fold(f64::NEG_INFINITY, |acc, point| {
                    js_max(acc, point.face_velocity_ms)
                }),
        ];

        for fan in compatible_fans {
            for speed_ratio in &sweep.requirements.speed_ratios {
                if fan.speed_ratio_limit(*speed_ratio).is_some() {
                    count_rejection(counts, FanSpeedRatioLimit::LABEL);
                    continue;
                }
                let context = CandidateContext {
                    requirements: sweep.requirements,
                    tower: sweep.tower,
                    stack: sweep.stack,
                    layers: sweep.layers,
                    drift_eliminator,
                    fan,
                    speed_ratio: *speed_ratio,
                    quality: sweep.quality,
                    inlet_air: sweep.inlet_air,
                };
                match evaluate_candidate(&context, sweep.catalog, drift_velocity_envelope_ms) {
                    Ok(candidate) => candidates.push(candidate),
                    Err(CandidateFailure::Constraint(reason)) => count_rejection(counts, reason),
                    Err(CandidateFailure::Domain) => {
                        count_rejection(counts, "numerical or curve domain");
                    }
                }
            }
        }
    }
}

/// Resolve one declared stack variant against the catalog.
///
/// Every layer's fill must be in the catalog, usable on this tower's type and approved for the
/// run's water-quality class; a variant that fails any of those is not built, and the refusal
/// is counted under a named label. The returned terms borrow the catalog's own records, so the
/// sweep walks the declared layers in order without copying a record.
fn declared_stack_layers<'a>(
    stack: &FillStack,
    catalog: &'a SelectionCatalog,
    tower: &SelectionTower,
    water_quality_class: &str,
    counts: &mut Vec<(String, usize)>,
) -> Option<Vec<LayerTerms<'a>>> {
    let mut layers = Vec::with_capacity(stack.layers.len());
    for layer in &stack.layers {
        let Some(fill) = catalog
            .fills
            .iter()
            .find(|fill| fill.physics.id == layer.fill_id)
        else {
            count_rejection(counts, "fill not in the catalog");
            return None;
        };
        if !fill.compatible_tower_types.contains(&tower.tower_type) {
            count_rejection(counts, "fill tower-type compatibility");
            return None;
        }
        if !fill
            .physics
            .allowed_water_quality_classes
            .iter()
            .any(|class| class == water_quality_class)
        {
            count_rejection(counts, "fill water-quality compatibility");
            return None;
        }
        layers.push(LayerTerms {
            fill: &fill.physics,
            depth_m: layer.depth_m,
            thermal_multiplier: layer.thermal_multiplier,
            pressure_multiplier: layer.pressure_multiplier,
        });
    }
    Some(layers)
}

/// Run the selector and rank the whole feasible set under the declared objective.
pub fn run_selection(input: &SelectionInput<'_>) -> Result<SelectionRun, DomainError> {
    let requirements = input.requirements.clone();
    let catalog = input.catalog;
    if !(requirements.hot_water_c > requirements.target_cold_water_c
        && requirements.target_cold_water_c > requirements.wet_bulb_c)
    {
        return Err(DomainError::new(
            "Selection temperatures must satisfy hot water > target cold water > wet bulb.",
        ));
    }
    let inlet_air = psychrometric_state(
        PsychrometricStateInput::from_wet_bulb(requirements.dry_bulb_c, requirements.wet_bulb_c)
            .with_pressure(requirements.pressure_pa),
    )?;
    let quality = catalog
        .water_quality_factors
        .get(&requirements.water_quality_class)
        .ok_or_else(|| DomainError::new("Unknown water-quality class."))?;

    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut candidates: Vec<SelectionCandidate> = Vec::new();
    for tower in &catalog.towers {
        if tower.footprint_m2 > requirements.max_footprint_m2 {
            count_rejection(&mut counts, "footprint");
            continue;
        }
        if requirements.water_mass_flow_kg_s > tower.max_water_mass_flow_kg_s {
            count_rejection(&mut counts, "tower water-flow limit");
            continue;
        }
        let compatible_fans: Vec<&SelectionFan> = catalog
            .fans
            .iter()
            .filter(|fan| tower.compatible_fan_ids.contains(&fan.physics.id))
            .collect();
        let compatible_fills: Vec<&SelectionFill> = catalog
            .fills
            .iter()
            .filter(|fill| fill.compatible_tower_types.contains(&tower.tower_type))
            .collect();

        if tower.fill_stacks.is_empty() {
            // The single-fill contract, unchanged: one layer per compatible fill × depth
            // option, in the reference's own nesting order.
            for fill in &compatible_fills {
                if !fill
                    .physics
                    .allowed_water_quality_classes
                    .contains(&requirements.water_quality_class)
                {
                    count_rejection(&mut counts, "fill water-quality compatibility");
                    continue;
                }
                for fill_depth_m in &tower.fill_depth_options_m {
                    let stack = FillStack::single(fill.physics.id.clone(), *fill_depth_m);
                    let layers = [LayerTerms::of_fill(&fill.physics, *fill_depth_m)];
                    sweep_stack(
                        &SweepContext {
                            catalog,
                            tower,
                            stack: &stack,
                            layers: &layers,
                            requirements: &requirements,
                            inlet_air: &inlet_air,
                            quality,
                        },
                        &compatible_fans,
                        &mut candidates,
                        &mut counts,
                    );
                }
            }
        } else {
            // Compare mode over COMPLETE stacks: each declared variant is one candidate
            // identity, and its layers may name different fill types, depths and multipliers.
            // A variant whose layers cannot be resolved is not built, and the reason is
            // counted — never a silent skip.
            for stack in &tower.fill_stacks {
                let Some(layers) = declared_stack_layers(
                    stack,
                    catalog,
                    tower,
                    &requirements.water_quality_class,
                    &mut counts,
                ) else {
                    continue;
                };
                sweep_stack(
                    &SweepContext {
                        catalog,
                        tower,
                        stack,
                        layers: &layers,
                        requirements: &requirements,
                        inlet_air: &inlet_air,
                        quality,
                    },
                    &compatible_fans,
                    &mut candidates,
                    &mut counts,
                );
            }
        }
    }

    rank_candidates(&mut candidates, input.objective);
    for (index, candidate) in candidates.iter_mut().enumerate() {
        candidate.rank = index + 1;
    }
    for candidate in &mut candidates {
        candidate.provenance_status = catalog.metadata.status.clone();
    }
    // The reference sorts its rejection map by count, descending, keeping insertion order for
    // ties (a stable sort over the insertion-ordered map).
    counts.sort_by_key(|entry| std::cmp::Reverse(entry.1));

    Ok(SelectionRun {
        objective: input.objective,
        requirements,
        catalog_metadata: catalog.metadata.clone(),
        inlet_air_state: inlet_air,
        candidates,
        rejection_summary: counts,
        warning: SELECTION_WARNING.to_string(),
    })
}

/// The reference-shaped selection: [`run_selection`], truncated to the first `maxResults`
/// candidates of the declared objective and ranked from 1.
pub fn select_cooling_tower_components(
    input: &SelectionInput<'_>,
) -> Result<SelectionResult, DomainError> {
    let mut run = run_selection(input)?;
    let feasible_candidate_count = run.feasible_candidate_count();
    run.candidates.truncate(input.max_results);
    Ok(SelectionResult {
        objective: run.objective,
        requirements: run.requirements,
        catalog_metadata: run.catalog_metadata,
        inlet_air_state: run.inlet_air_state,
        results: run.candidates,
        feasible_candidate_count,
        rejection_summary: run.rejection_summary,
        warning: run.warning,
    })
}

/// Reference default requirements, exposed like `defaultSelectionRequirements()`.
pub fn default_selection_requirements() -> SelectionRequirements {
    SelectionRequirements::default()
}
