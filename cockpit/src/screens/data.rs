//! The engine calls behind the new screens (drafthouse#91 Part B).
//!
//! Every number a screen shows comes from one of two places, and the screen says which:
//!
//! - **engine** - a function in `rust/src` called right here, through the `synergy_drafthouse` crate the
//!   adapter re-exports (`cockpit_adapter::synergy_drafthouse`), or the cockpit's own `Engine` (the adapter's
//!   `RealEngine`) run on an edited copy of the draft. The engine is not changed.
//! - **stub** - an input the engine has no source for yet (field test readings, measurement sigmas, a signed-in
//!   identity). Each stub is a named constant in this file and listed in `docs/design/screens-r1/README.md`;
//!   every screen that paints one also paints the amber STUB tag.
//!
//! Heavy work (the per-tower selection runs, the performance-curve grid) is computed lazily and in slices, so
//! a phone frame never stalls on it: [`Cache::step`] does at most a frame's budget of engine calls.

use cockpit::engine::{EngineError, EngineInput, EngineOutput};

use crate::engine_select::CockpitEngine;

#[cfg(feature = "real-engine")]
use cockpit_adapter::synergy_drafthouse as eng;

#[cfg(feature = "real-engine")]
use drafthouse_cockpit_seams::mapping as m;

use std::sync::Arc;

use crate::clock::now_ms;

/// The frame budget for lazy engine work, in ms.
const BUDGET_MS: f64 = 14.0;

// ======================================================================================== stubs

/// STUB: three field-test readings for the Rate screen, as fractions/offsets of the design duty. A real
/// acceptance test (CTI ATC-105 style) brings its own readings; nothing in the fixture records one.
/// (water-flow fraction, dry-air fraction, hot water C, cold water C, wet bulb C, dry bulb C)
///
/// Issue #139: the readings are written as offsets **from the duty** - water and air flow as fractions of
/// the design flows, hot water / cold water / wet bulb / dry bulb as kelvin off the duty's hot water,
/// target cold water, wet bulb and dry bulb - so an edited duty moves its test readings with it instead
/// of leaving fixed temperatures behind. At the recorded duty (42 / 32 / 27 / 33 °C) they are the
/// readings this table always carried (41.6 / 31.9 / 26.6 / 32.4 °C, ...). Read them via [`test_points`].
pub const STUB_TEST_OFFSETS: [(f64, f64, f64, f64, f64, f64); 3] = [
    (0.97, 0.99, -0.4, -0.1, -0.4, -0.6),
    (1.02, 1.00, 0.3, 0.6, 0.1, 0.0),
    (0.94, 0.97, -1.2, -1.0, -1.1, -1.4),
];

/// Issue #139: the STUB test points at `duty` - `(water fraction, air fraction, hot, cold, wet bulb, dry
/// bulb)`, the temperatures in °C - the one place the Rate screen, its Monte Carlo input and the
/// fixture-equals-CLI check read them from.
pub fn test_points(duty: &cockpit::engine::Duty) -> [(f64, f64, f64, f64, f64, f64); 3] {
    STUB_TEST_OFFSETS.map(|(fw, fa, dh, dc, dwb, ddb)| {
        (
            fw,
            fa,
            duty.hot_water_c + dh,
            duty.target_cold_water_c + dc,
            duty.wet_bulb_c + dwb,
            duty.dry_bulb_c + ddb,
        )
    })
}
/// STUB: the measurement sigmas the Monte Carlo band uses (flow as a fraction, temperatures in K).
pub const STUB_SIGMA: (f64, f64, f64, f64) = (0.02, 0.03, 0.10, 0.15);
pub const STUB_SIGMA_SAMPLES: usize = 300;
/// STUB: the Report cover's project name and sheet number. There is no project record yet (#74).
pub const STUB_PROJECT: &str = "Project name (no project record yet)";
pub const STUB_REPORT_NO: &str = "CS-0000";
/// STUB: the signed-in identity (initials, the menu line, the catalog it would unlock). Sign-in is a toggle;
/// no identity provider is wired (synergy-apps#1808), and the Synergy catalog is not served yet.
pub const STUB_USER_INITIALS: &str = "SS";
pub const STUB_USER: &str = "Synergy staff (stub identity)";
pub const STUB_CATALOG: &str = "Synergy catalog";

/// The performance-curve grid the Curves screen asks the engine for.
pub const CURVE_WB: [f64; 6] = [20.0, 22.0, 24.0, 26.0, 28.0, 30.0];
pub const CURVE_RANGE: [f64; 3] = [8.0, 10.0, 12.0];
/// Fixed-point tolerance (K) of the hot-water iteration: a record's actual range is within this of its
/// stated range. 0.05 K is under a pixel at the chart's scale and halves the engine runs (one run is
/// ~0.5 s in the wasm build).
pub const CURVE_TOL: f64 = 0.05;
pub const CURVE_FLOW_PCT: [f64; 3] = [80.0, 100.0, 120.0];
/// The order the ranges are computed in (indices into `CURVE_RANGE`): the one the screen opens on first.
const CURVE_RANGE_ORDER: [usize; 3] = [1, 0, 2];

/// The crossflow screen's own grid (issue #84): the cells the screen solves and the drawing paints.
/// The fixture-equals-CLI test spells it as `--cells {XF_CELLS}`, so both paths solve the same grid.
pub const XF_CELLS: usize = 14;

// ======================================================================================== types

#[derive(Clone, Debug, Default)]
pub struct Cand {
    pub tower_id: String,
    pub crossflow: bool,
    pub fill_id: String,
    pub depth_m: f64,
    pub fan_id: String,
    pub speed: f64,
    pub margin_c: f64,
    pub cold_c: f64,
    pub capacity_kg_s: Option<f64>,
    pub cap_ratio: Option<f64>,
    pub power_kw: f64,
    pub footprint_m2: f64,
    pub makeup_kg_s: f64,
    pub drift_ppm: f64,
    pub pressure_pa: f64,
    pub airflow_m3_s: f64,
    pub dry_air_kg_s: f64,
    pub available_merkel: f64,
    pub nozzles: f64,
    pub nozzle_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct TowerRun {
    pub id: String,
    pub crossflow: bool,
    pub footprint_m2: f64,
    pub max_flow_kg_s: f64,
    pub feasible: usize,
    pub rejections: Vec<(String, usize)>,
    pub cands: Vec<Cand>,
    pub ms: f64,
}

#[derive(Clone, Debug, Default)]
pub struct SizeData {
    pub water_kg_s: f64,
    pub towers: Vec<TowerRun>,
    pub ms: f64,
    /// [`SizeData::ranked`]`(None)`, owned and computed once when the selection lands - the Size
    /// screen reads it every frame (issue #138: it used to re-collect and clone every candidate).
    pub by_capacity: Vec<Cand>,
}

impl SizeData {
    /// Every feasible candidate across the per-tower runs, ordered by the engine's own objective metric
    /// (least over-capacity, `SelectionCandidate::objective_metric`).
    pub fn ranked(&self, crossflow: Option<bool>) -> Vec<&Cand> {
        let mut v: Vec<&Cand> = self
            .towers
            .iter()
            .filter(|t| crossflow.is_none_or(|x| t.crossflow == x))
            .flat_map(|t| t.cands.iter())
            .collect();
        v.sort_by(|a, b| {
            a.capacity_kg_s
                .unwrap_or(f64::INFINITY)
                .total_cmp(&b.capacity_kg_s.unwrap_or(f64::INFINITY))
        });
        v
    }
}

#[derive(Clone, Debug, Default)]
pub struct RatePoint {
    pub capability_pct: f64,
    pub design_lg: f64,
    pub test_lg: f64,
    pub cap_lg: f64,
    /// The fitted whole-tower characteristic's coefficient and exponent (`C` and `m`,
    /// `testCharacteristicCoefficient` / `characteristicExponent` from
    /// `capability::evaluate_characteristic_capability`): KaV/L = C · (L/G)^m through the test point.
    pub test_c: f64,
    pub exponent_m: f64,
    pub curve: Vec<(f64, f64, f64)>,
    pub band: Option<(f64, f64)>,
    pub mc_mean: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct RateData {
    pub points: Vec<Result<RatePoint, String>>,
    pub dry_air_kg_s: f64,
    pub water_kg_s: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CurveRec {
    pub wb: f64,
    pub range: f64,
    pub flow_kg_s: f64,
    pub cold: f64,
}

#[derive(Clone, Debug, Default)]
pub struct CurveData {
    pub design_flow_kg_s: f64,
    pub recs: Vec<CurveRec>,
    pub failed: usize,
    /// The engine's refusal reasons for the records that did not solve (digits masked), with counts.
    pub fail_why: Vec<(String, usize)>,
    pub total: usize,
    pub runs: usize,
    pub ms: f64,
}

#[derive(Clone, Debug, Default)]
pub struct WaterData {
    pub circulating_kg_s: f64,
    pub evaporation_kg_s: f64,
    pub drift_kg_s: f64,
    pub drift_ppm: f64,
    pub blowdown_kg_s: f64,
    pub makeup_kg_s: f64,
    pub cycles: f64,
    pub density: f64,
    /// |makeup recomputed at the duty's cycles - the engine run's own makeup|, m3/h: the self-check.
    pub check_m3_h: f64,
}

/// The engine's three-grid study ([`eng::crossflow_convergence_study`]): the same grid inputs solved
/// on an `n`/`2n`/`4n` ladder, its observed order and the extrapolated answer. Issue #84's second
/// acceptance criterion wants the solver's numerical quality on the surface, so the panel shows
/// this and the fixture-equals-CLI test pins every number against the CLI's `convergence` command.
#[derive(Clone, Debug, Default)]
pub struct XfStudy {
    pub cells: [usize; 3],
    pub cold: [f64; 3],
    pub order: Option<f64>,
    pub extrapolated: f64,
    pub fine_error: f64,
    pub gci_pct: f64,
    pub note: &'static str,
}

#[derive(Clone, Debug, Default)]
pub struct XfGrid {
    pub cand: Cand,
    pub hot_c: f64,
    pub wb_c: f64,
    /// The engine's own answer, carried whole ([`eng::solve_crossflow_grid`]'s scalars): the panel
    /// paints a few of them, the fixture-equals-CLI test (issue #84) reads all of them.
    pub cold_c: f64,
    pub range_c: f64,
    pub approach_c: f64,
    pub heat_kw: f64,
    pub water_kw: f64,
    pub cp_kj_kg_k: f64,
    pub outlet_db: f64,
    pub outlet_hr: f64,
    pub outlet_h: f64,
    pub water: Vec<Vec<f64>>,
    pub air_h: Vec<Vec<f64>>,
    /// The water loading the duty puts on this candidate's fill, kg/(m^2 s): the engine's own definition
    /// (water mass flow over the fill's plan area, `airside.rs`). Drives the drawn droplets' speed (issue
    /// #86 AC 1) - it is the number the loading is, not a look.
    pub water_loading_kg_m2_s: f64,
    /// The candidate's fan speed in rpm, from the record's own rated speed and this run's ratio. `None`
    /// when the record states no rated speed, and then nothing is drawn turning (the seams' `rpm` has
    /// nothing to say).
    pub rpm: Option<f64>,
    /// The convergence block: the grid actually solved, the doubled grid Richardson extrapolated
    /// from, the extrapolated answer, and the engine's own discretisation-error estimate with its
    /// stated meaning (AC 2's error estimate, shown with the result).
    pub cells: [usize; 2],
    pub fine_cells: Option<[usize; 2]>,
    pub coarse_cold_c: f64,
    pub fine_cold_c: Option<f64>,
    pub richardson_cold_c: f64,
    pub error_c: Option<f64>,
    pub error_meaning: &'static str,
    pub study: XfStudy,
    pub ms: f64,
}

/// How many times each heavy result has been computed (issue #138): a result is counted when it
/// lands, so a test can prove that nothing it depends on changed and nothing was recomputed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Computed {
    pub size: u32,
    pub rate: u32,
    pub curves: u32,
    pub xf: u32,
}

/// The lazy cache. Every heavy result is keyed by a hash of exactly the inputs it reads (issue #138),
/// so an edit recomputes only what it feeds - not on a tab switch, not every frame:
///
/// - **size** (and the crossflow grid built on its winner): the fixture's catalog and the duty
///   (`requirements` reads all of it);
/// - **rate**: the duty's flow and temperatures, pressure and salinity, and the run's dry-air flow;
/// - **curves**: the whole draft (each record runs the engine on an edited copy of it).
///
/// The results are `Arc`s: a screen holds one for its frame with a pointer copy, never a deep clone
/// of the result set (issue #138).
#[derive(Default)]
pub struct Cache {
    size_key: u64,
    rate_key: u64,
    curves_key: u64,
    /// The catalog text the heavy work was last synced to ([`text_key`]); readable by the screens'
    /// tests (issue #139: an import must move it).
    pub(super) fixture_key: u64,
    pub computed: Computed,
    #[cfg(feature = "real-engine")]
    rate_acc: Option<(RateData, Vec<Option<Mc>>)>,
    pub size: Option<Arc<SizeData>>,
    size_next: usize,
    size_acc: Vec<TowerRun>,
    #[cfg(feature = "real-engine")]
    size_sweep: Option<TowerSweep>,
    pub rate: Option<Arc<RateData>>,
    pub curves: Option<Arc<CurveData>>,
    curves_next: usize,
    curves_it: Option<CurveIter>,
    curves_acc: Vec<CurveRec>,
    curves_failed: usize,
    curves_fail_why: Vec<(String, usize)>,
    curves_runs: usize,
    curves_ms: f64,
    pub xf: Option<Result<Arc<XfGrid>, String>>,
    pub engine_ms_last: f64,
    #[cfg(feature = "real-engine")]
    catalog: Option<Result<eng::SelectionCatalog, String>>,
}

/// One hasher for the cache keys: every f64 by its exact bits, so a key moves exactly when a value does.
struct Key(std::collections::hash_map::DefaultHasher);

impl Key {
    fn new() -> Self {
        Key(std::collections::hash_map::DefaultHasher::new())
    }
    fn f(mut self, x: f64) -> Self {
        std::hash::Hasher::write_u64(&mut self.0, x.to_bits());
        self
    }
    fn s(mut self, x: &str) -> Self {
        std::hash::Hash::hash(x, &mut self.0);
        self
    }
    fn u(mut self, x: u64) -> Self {
        std::hash::Hasher::write_u64(&mut self.0, x);
        self
    }
    fn done(self) -> u64 {
        std::hash::Hasher::finish(&self.0)
    }
}

/// Everything the selection reads off the draft: the duty, whole (see [`requirements`]).
fn duty_key(d: &EngineInput) -> Key {
    let u = &d.duty;
    Key::new()
        .f(u.water_flow_m3_hr)
        .f(u.hot_water_c)
        .f(u.target_cold_water_c)
        .f(u.wet_bulb_c)
        .f(u.dry_bulb_c)
        .f(u.pressure_pa)
        .f(u.salinity_g_kg)
        .s(&u.water_quality_class)
        .f(u.cycles_of_concentration)
}

/// Issue #139: a catalog text's key - what the screens hold their duty-relative settings and the
/// comparison's runs against, so an imported revision is seen as a different catalog.
pub fn text_key(text: &str) -> u64 {
    Key::new().s(text).done()
}

/// Issue #139: the key the screens' duty-relative settings follow - the whole duty (the same fields the
/// selection reads) and the catalog the heavy work runs against. Curves' range, probe wet bulb and target,
/// and the picked Size candidate are reset from the project when it moves.
pub fn follow_key(d: &EngineInput, catalog_key: u64) -> u64 {
    duty_key(d).u(catalog_key).done()
}

impl Cache {
    /// Drop exactly the results whose inputs moved since they were computed (issue #138).
    pub fn sync(&mut self, fixture_text: &str, draft: &EngineInput, out: Option<&EngineOutput>) {
        let fixture = Key::new().s(fixture_text).done();
        if fixture != self.fixture_key {
            #[cfg(feature = "real-engine")]
            {
                self.catalog = None;
            }
            self.fixture_key = fixture;
        }
        let size = duty_key(draft).u(fixture).done();
        if size != self.size_key {
            self.size = None;
            self.size_next = 0;
            self.size_acc.clear();
            #[cfg(feature = "real-engine")]
            {
                self.size_sweep = None;
            }
            // the crossflow grid is solved on the selection's crossflow winner
            self.xf = None;
            self.size_key = size;
        }
        let u = &draft.duty;
        let rate = Key::new()
            .f(u.water_flow_m3_hr)
            .f(u.hot_water_c)
            .f(u.target_cold_water_c)
            .f(u.wet_bulb_c)
            .f(u.dry_bulb_c)
            .f(u.pressure_pa)
            .f(u.salinity_g_kg)
            // the air flow by either of its step names (issue #137 renamed it): a key that read only
            // the old name would hash NaN for every real run, and an air-flow change would keep a
            // stale rate result
            .f(out.and_then(dry_air_kg_s).unwrap_or(f64::NAN))
            .done();
        if rate != self.rate_key {
            self.rate = None;
            #[cfg(feature = "real-engine")]
            {
                self.rate_acc = None;
            }
            self.rate_key = rate;
        }
        // every curve record is an engine run on an edited copy of the whole draft
        let curves = Key::new()
            .s(&serde_json::to_string(draft).unwrap_or_default())
            .u(fixture)
            .done();
        if curves != self.curves_key {
            self.curves = None;
            self.curves_next = 0;
            self.curves_it = None;
            self.curves_acc.clear();
            self.curves_failed = 0;
            self.curves_fail_why.clear();
            self.curves_runs = 0;
            self.curves_ms = 0.0;
            self.curves_key = curves;
        }
    }

    pub fn size_progress(&self) -> (usize, usize) {
        #[cfg(feature = "real-engine")]
        {
            let n = self
                .catalog
                .as_ref()
                .and_then(|c| c.as_ref().ok())
                .map(|c| c.towers.len())
                .unwrap_or(0);
            (self.size_next.min(n), n)
        }
        #[cfg(not(feature = "real-engine"))]
        (0, 0)
    }

    /// The Rate screen's Monte Carlo samples drawn so far, of all (zero of zero before it starts).
    pub fn rate_progress(&self) -> (usize, usize) {
        #[cfg(feature = "real-engine")]
        {
            self.rate_acc
                .iter()
                .flat_map(|(_, mcs)| mcs.iter().flatten())
                .fold((0, 0), |(d, n), m| (d + m.drawn, n + m.samples))
        }
        #[cfg(not(feature = "real-engine"))]
        (0, 0)
    }

    pub fn curves_progress(&self) -> (usize, usize) {
        (
            self.curves_next,
            CURVE_WB.len() * CURVE_RANGE.len() * CURVE_FLOW_PCT.len(),
        )
    }
}

// ================================================================================ engine glue

pub fn water_kg_s(d: &EngineInput) -> f64 {
    #[cfg(feature = "real-engine")]
    {
        let t = (d.duty.hot_water_c + d.duty.target_cold_water_c) / 2.0;
        let rho = eng::water_density_kg_m3(t, d.duty.salinity_g_kg).unwrap_or(995.0);
        d.duty.water_flow_m3_hr * rho / 3600.0
    }
    #[cfg(not(feature = "real-engine"))]
    {
        d.duty.water_flow_m3_hr * 995.0 / 3600.0
    }
}

pub fn density(d: &EngineInput) -> f64 {
    #[cfg(feature = "real-engine")]
    {
        let t = (d.duty.hot_water_c + d.duty.target_cold_water_c) / 2.0;
        eng::water_density_kg_m3(t, d.duty.salinity_g_kg).unwrap_or(995.0)
    }
    #[cfg(not(feature = "real-engine"))]
    {
        let _ = d;
        995.0
    }
}

/// The value of one of the engine's own worked steps, by label.
pub fn step_value(o: &EngineOutput, label: &str) -> Option<f64> {
    o.worked_steps
        .iter()
        .find(|s| s.label == label)
        .and_then(|s| s.value)
}

/// The run's dry-air mass flow, kg/s. Issue #137 renamed the worked step ("Dry-air mass flow" -> "Air flow
/// through the fill"); a recorded replay still carries the old label, so both are read. Same value, same
/// step - only the name moved.
pub fn dry_air_kg_s(o: &EngineOutput) -> Option<f64> {
    step_value(o, "Air flow through the fill").or_else(|| step_value(o, "Dry-air mass flow"))
}

#[cfg(feature = "real-engine")]
fn build_catalog(fixture_text: &str) -> Result<eng::SelectionCatalog, String> {
    let mut cat = crate::engine_catalog::catalog_from_fixture(fixture_text)?;
    let v: serde_json::Value = serde_json::from_str(fixture_text).map_err(|e| e.to_string())?;
    let c = &v["catalog"];
    // `catalog_from_fixture` leaves two fields the adapter supplies per run: the towers' depth options and
    // the nozzle records. A whole-catalog `run_selection` needs both, so they are read from the same fixture.
    if let Some(towers) = c["towers"].as_array() {
        for t in towers {
            let id = t["id"].as_str().unwrap_or_default();
            let opts: Vec<f64> = t["fillDepthOptionsM"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default();
            if let Some(tw) = cat.towers.iter_mut().find(|x| x.id == id) {
                if !opts.is_empty() {
                    tw.fill_depth_options_m = opts;
                }
            }
        }
    }
    if let Some(nz) = c["nozzles"].as_array() {
        cat.nozzles = nz
            .iter()
            .filter_map(|n| {
                Some(eng::NozzleRecord {
                    id: n["id"].as_str()?.to_string(),
                    name: n["name"].as_str().unwrap_or_default().to_string(),
                    discharge_coefficient: n["dischargeCoefficient"].as_f64()?,
                    orifice_diameter_m: n["orificeDiameterM"].as_f64()?,
                    reference_water_density_kg_m3: n["referenceWaterDensityKgM3"].as_f64(),
                })
            })
            .collect();
    }
    Ok(cat)
}

#[cfg(feature = "real-engine")]
fn requirements(d: &EngineInput) -> eng::SelectionRequirements {
    eng::SelectionRequirements {
        water_mass_flow_kg_s: water_kg_s(d),
        hot_water_c: d.duty.hot_water_c,
        target_cold_water_c: d.duty.target_cold_water_c,
        wet_bulb_c: d.duty.wet_bulb_c,
        dry_bulb_c: d.duty.dry_bulb_c,
        pressure_pa: d.duty.pressure_pa,
        salinity_g_kg: d.duty.salinity_g_kg,
        water_quality_class: d.duty.water_quality_class.clone(),
        cycles_of_concentration: d.duty.cycles_of_concentration,
        ..eng::SelectionRequirements::default()
    }
}

#[cfg(feature = "real-engine")]
fn cand_of(c: &eng::SelectionCandidate, crossflow: bool, footprint: f64) -> Cand {
    Cand {
        tower_id: c.tower_id.clone(),
        crossflow,
        fill_id: c.fill_id.clone(),
        depth_m: c.fill_depth_m,
        fan_id: c.fan_id.clone(),
        speed: c.speed_ratio,
        margin_c: c.thermal_margin_c,
        cold_c: c.thermal.cold_water_c,
        capacity_kg_s: c.capacity_kg_s,
        cap_ratio: c.capability_ratio,
        // Fan power is the engine's own `fan_power_kw` for this candidate: the fan operating point's
        // shaft power - exactly the field the answer bar paints (`EngineOutput.fan_power_kw`, which
        // `RealEngine::run` fills from this same value). It used to be the electrical input, which
        // made the winner card's "fan power" disagree with the answer bar's for the same candidate
        // (issue #83's design-round defect).
        power_kw: c.fan_operating_point.shaft_power_kw,
        footprint_m2: footprint,
        makeup_kg_s: c.water_balance.makeup_kg_s,
        drift_ppm: c.airside.drift_ppm,
        pressure_pa: c.airside.total_pa,
        airflow_m3_s: c.fan_operating_point.flow_m3_s,
        dry_air_kg_s: c.airside.dry_air_mass_flow_kg_s,
        available_merkel: c.airside.available_merkel_number,
        nozzles: c.nozzle.count,
        nozzle_id: c.nozzle.nozzle_id.clone(),
    }
}

/// One curve record's hot-water iteration in flight (issue #138): its draft copy, the cold water so
/// far and the loose/tight state, carried from frame to frame between engine runs.
struct CurveIter {
    inp: EngineInput,
    wb: f64,
    range: f64,
    flow_kg_s: f64,
    cold: f64,
    loose: bool,
    ok: Option<f64>,
    why: String,
    tries: usize,
}

/// One part of one tower's selection, in the engine's own loop order (`run_selection`: tower checks,
/// then per fill its water-quality check, per depth and drift eliminator the drift's temperature check,
/// then per fan and speed ratio one candidate). Indices into the catalog's lists and the speed ratios.
#[cfg(feature = "real-engine")]
#[derive(Clone, Copy, Debug)]
enum Part {
    /// The tower's own checks (no fills): its footprint and water-flow limit.
    Tower,
    /// One fill's own check (no depths): water-quality compatibility.
    Fill(usize),
    /// One fill x depth x drift eliminator's own check (no fans): the drift's temperature limit.
    Drift(usize, usize, usize),
    /// One candidate: fill, depth, drift eliminator, fan, speed ratio.
    Cand(usize, usize, usize, usize, usize),
    /// A tower with declared fill stacks (compare mode): its whole run, unsliced.
    Whole,
}

/// Issue #138: one tower's selection, run a part at a time so no frame carries the whole sweep.
///
/// Every part is `run_selection` itself on a sub-catalog narrowed to that part (the engine is not
/// changed), and each part records at most one event - one rejection or one candidate - so the parts,
/// replayed in the engine's loop order, are the whole run's own event stream:
///
/// - a check part that rejects skips the parts nested under it, exactly where the engine `continue`s;
/// - the rejection counts are merged in first-seen order and then sorted by count with the engine's
///   own stable sort, so the summary (and its order on screen) is the whole run's;
/// - the candidates are ranked with the engine's own [`eng::rank_candidates`] under the same
///   (default) objective - a total order, so the merged ranking is the whole run's.
///
/// `size_workspace_equals_the_native_cli` pins the result against the CLI's whole-tower `select`.
#[cfg(feature = "real-engine")]
struct TowerSweep {
    parts: Vec<Part>,
    next: usize,
    cands: Vec<eng::SelectionCandidate>,
    counts: Vec<(String, usize)>,
    error: Option<String>,
    ms: f64,
}

#[cfg(feature = "real-engine")]
impl TowerSweep {
    fn new(
        cat: &eng::SelectionCatalog,
        tower: &eng::SelectionTower,
        req: &eng::SelectionRequirements,
    ) -> Self {
        let mut parts = vec![Part::Tower];
        if !tower.fill_stacks.is_empty() {
            parts = vec![Part::Whole];
        } else {
            // the engine's own two filters: a fill this tower type cannot take and a fan this tower
            // does not list are never iterated, so they have no part (and no event)
            let fans: Vec<usize> = (0..cat.fans.len())
                .filter(|&i| tower.compatible_fan_ids.contains(&cat.fans[i].physics.id))
                .collect();
            let fills = (0..cat.fills.len()).filter(|&i| {
                cat.fills[i]
                    .compatible_tower_types
                    .contains(&tower.tower_type)
            });
            for fi in fills {
                parts.push(Part::Fill(fi));
                for di in 0..tower.fill_depth_options_m.len() {
                    for xi in 0..cat.drift_eliminators.len() {
                        parts.push(Part::Drift(fi, di, xi));
                        for &ni in &fans {
                            for si in 0..req.speed_ratios.len() {
                                parts.push(Part::Cand(fi, di, xi, ni, si));
                            }
                        }
                    }
                }
            }
        }
        TowerSweep {
            parts,
            next: 0,
            cands: Vec::new(),
            counts: Vec::new(),
            error: None,
            ms: 0.0,
        }
    }

    fn done(&self) -> bool {
        self.error.is_some() || self.next >= self.parts.len()
    }

    /// Run the next part.
    fn step(
        &mut self,
        cat: &eng::SelectionCatalog,
        tower: &eng::SelectionTower,
        req: &eng::SelectionRequirements,
    ) {
        let Some(part) = self.parts.get(self.next).copied() else {
            return;
        };
        self.next += 1;
        // (fills, depths, drift eliminators, fans, speed ratios) the part's sub-catalog keeps
        let (fills, depths, drifts, fans, speeds) = match part {
            Part::Whole => (None, None, None, None, None),
            Part::Tower => (Some(vec![]), None, None, None, None),
            Part::Fill(f) => (Some(vec![f]), Some(vec![]), None, None, None),
            Part::Drift(f, d, x) => (
                Some(vec![f]),
                Some(vec![d]),
                Some(vec![x]),
                Some(vec![]),
                None,
            ),
            Part::Cand(f, d, x, n, r) => (
                Some(vec![f]),
                Some(vec![d]),
                Some(vec![x]),
                Some(vec![n]),
                Some(vec![r]),
            ),
        };
        let mut t = tower.clone();
        if let Some(d) = depths {
            t.fill_depth_options_m = d.iter().map(|&i| tower.fill_depth_options_m[i]).collect();
        }
        let sub = eng::SelectionCatalog {
            metadata: cat.metadata.clone(),
            water_quality_factors: cat.water_quality_factors.clone(),
            towers: vec![t],
            fills: fills
                .unwrap_or_else(|| (0..cat.fills.len()).collect())
                .iter()
                .map(|&i| cat.fills[i].clone())
                .collect(),
            drift_eliminators: drifts
                .unwrap_or_else(|| (0..cat.drift_eliminators.len()).collect())
                .iter()
                .map(|&i| cat.drift_eliminators[i].clone())
                .collect(),
            fans: fans
                .unwrap_or_else(|| (0..cat.fans.len()).collect())
                .iter()
                .map(|&i| cat.fans[i].clone())
                .collect(),
            nozzles: cat.nozzles.clone(),
        };
        let mut r = req.clone();
        if let Some(sp) = speeds {
            r.speed_ratios = sp.iter().map(|&i| req.speed_ratios[i]).collect();
        }
        let s = now_ms();
        let run = eng::run_selection(&eng::SelectionInput::new(&sub).with_requirements(r));
        self.ms += now_ms() - s;
        let run = match run {
            Ok(run) => run,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let rejected = !run.rejection_summary.is_empty();
        for (label, n) in run.rejection_summary {
            match self.counts.iter_mut().find(|(l, _)| *l == label) {
                Some(entry) => entry.1 += n,
                None => self.counts.push((label, n)),
            }
        }
        self.cands.extend(run.candidates);
        if rejected {
            // the engine `continue`s past everything nested under a check that rejects
            let nested = |p: &Part| match (part, *p) {
                (Part::Tower, _) => true,
                (Part::Fill(f), Part::Drift(g, ..) | Part::Cand(g, ..)) => f == g,
                (Part::Drift(f, d, x), Part::Cand(g, e, y, ..)) => (f, d, x) == (g, e, y),
                _ => false,
            };
            while self.parts.get(self.next).is_some_and(nested) {
                self.next += 1;
            }
        }
    }

    fn finish(mut self, tower: &eng::SelectionTower) -> TowerRun {
        let crossflow = tower.tower_type == eng::TowerType::Crossflow;
        let base = TowerRun {
            id: tower.id.clone(),
            crossflow,
            footprint_m2: tower.footprint_m2,
            max_flow_kg_s: tower.max_water_mass_flow_kg_s,
            ms: self.ms,
            ..TowerRun::default()
        };
        if let Some(e) = self.error {
            return TowerRun {
                rejections: vec![(e, 1)],
                ..base
            };
        }
        // the engine's own order: by count, descending, first-seen order for ties (a stable sort)
        self.counts.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        eng::rank_candidates(&mut self.cands, eng::Objective::default());
        TowerRun {
            feasible: self.cands.len(),
            rejections: self.counts,
            cands: self
                .cands
                .iter()
                .map(|c| cand_of(c, crossflow, tower.footprint_m2))
                .collect(),
            ..base
        }
    }
}

// ===================================================================================== stepping

impl Cache {
    /// Do at most one frame's budget of lazy engine work for `want` (the screen on show). Returns true while
    /// work remains (the screen paints its progress ring).
    pub fn step(
        &mut self,
        want: Want,
        fixture_text: &str,
        draft: &EngineInput,
        out: Option<&EngineOutput>,
        engine: Option<&dyn CockpitEngine>,
    ) -> bool {
        self.sync(fixture_text, draft, out);
        let t0 = now_ms();
        let busy = match want {
            Want::Size => self.step_size(fixture_text, draft, t0),
            Want::Crossflow => {
                if self.size.is_none() {
                    self.step_size(fixture_text, draft, t0);
                    // the grid (one solve and the convergence study's three) is still owed: it is
                    // the next frame's whole work, never the tail of a selection frame (issue #138)
                    true
                } else {
                    if self.xf.is_none() {
                        self.xf = Some(self.solve_xf(draft).map(Arc::new));
                        self.computed.xf += 1;
                    }
                    false
                }
            }
            Want::Rate => self.step_rate(draft, out, t0),
            Want::Curves => match engine {
                Some(e) => self.step_curves(draft, e, t0),
                None => false,
            },
            Want::Report => {
                // the Charts page needs both caches: stay busy across the hand-off, or a screenshot taken
                // between the two steps catches a "computing" cell (round-2 QA)
                let mut busy = self.size.is_none() && self.step_size(fixture_text, draft, t0);
                if !busy && self.curves.is_none() {
                    if let Some(e) = engine {
                        busy = self.step_curves(draft, e, t0);
                    }
                }
                busy
            }
            Want::Nothing => false,
        };
        let _ = fixture_text;
        self.engine_ms_last = now_ms() - t0;
        busy
    }

    /// The Rate screen's points (issue #138): the fitted characteristics at once (three cheap
    /// evaluations), then each point's Monte Carlo band a frame's budget of samples at a time. The
    /// result lands whole, when the last sample is drawn - the screen never paints half a band.
    #[allow(unused_variables)]
    fn step_rate(&mut self, draft: &EngineInput, out: Option<&EngineOutput>, t0: f64) -> bool {
        if self.rate.is_some() {
            return false;
        }
        let Some(o) = out else { return false };
        #[cfg(feature = "real-engine")]
        {
            let acc = self.rate_acc.get_or_insert_with(|| rate(draft, o));
            for m in acc.1.iter_mut().flatten() {
                while !m.done() {
                    m.sample();
                    if now_ms() - t0 > BUDGET_MS {
                        return true;
                    }
                }
            }
            let (mut data, mcs) = self.rate_acc.take().expect("the rate in progress");
            for (p, m) in data.points.iter_mut().zip(&mcs) {
                if let (Ok(p), Some(m)) = (p.as_mut(), m) {
                    if let Some((mean, u)) = m.result() {
                        p.mc_mean = Some(mean);
                        p.band = Some((p.capability_pct - u, p.capability_pct + u));
                    }
                }
            }
            self.rate = Some(Arc::new(data));
        }
        #[cfg(not(feature = "real-engine"))]
        {
            self.rate = Some(Arc::new(rate(draft, o)));
        }
        self.computed.rate += 1;
        false
    }

    #[allow(unused_variables)]
    fn step_size(&mut self, fixture_text: &str, draft: &EngineInput, t0: f64) -> bool {
        if self.size.is_some() {
            return false;
        }
        #[cfg(feature = "real-engine")]
        {
            if self.catalog.is_none() {
                self.catalog = Some(build_catalog(fixture_text));
            }
            let Some(Ok(cat)) = self.catalog.as_ref() else {
                self.size = Some(Arc::default());
                return false;
            };
            let req = requirements(draft);
            while self.size_next < cat.towers.len() {
                let tower = &cat.towers[self.size_next];
                let sweep = self
                    .size_sweep
                    .get_or_insert_with(|| TowerSweep::new(cat, tower, &req));
                // issue #138: one tower's selection is up to ~700 candidates (seconds of engine time),
                // so it runs one part per step - see [`TowerSweep`]
                sweep.step(cat, tower, &req);
                if sweep.done() {
                    let sweep = self.size_sweep.take().expect("the sweep in progress");
                    self.size_acc.push(sweep.finish(tower));
                    self.size_next += 1;
                }
                if now_ms() - t0 > BUDGET_MS {
                    break;
                }
            }
            if self.size_next >= cat.towers.len() {
                let towers = std::mem::take(&mut self.size_acc);
                let ms = towers.iter().map(|t| t.ms).sum();
                let mut size = SizeData {
                    water_kg_s: req.water_mass_flow_kg_s,
                    towers,
                    ms,
                    by_capacity: Vec::new(),
                };
                size.by_capacity = size.ranked(None).into_iter().cloned().collect();
                self.size = Some(Arc::new(size));
                self.computed.size += 1;
                return false;
            }
            true
        }
        #[cfg(not(feature = "real-engine"))]
        {
            self.size = Some(Arc::default());
            false
        }
    }

    fn step_curves(&mut self, draft: &EngineInput, e: &dyn CockpitEngine, t0: f64) -> bool {
        let total = CURVE_WB.len() * CURVE_RANGE.len() * CURVE_FLOW_PCT.len();
        if self.curves_next >= total && self.curves.is_some() {
            return false;
        }
        let design = water_kg_s(draft);
        while self.curves_next < total {
            // issue #138: one engine run is the unit of work - a record's iteration (up to six runs)
            // is carried across frames in `curves_it`, so no frame holds more than one run past budget
            let mut it = match self.curves_it.take() {
                Some(it) => it,
                None => {
                    let i = self.curves_next;
                    // range-major, the middle range (the one the screen opens on) first, so its chart appears
                    // after a third of the work; the others fill in behind it
                    let per_range = CURVE_WB.len() * CURVE_FLOW_PCT.len();
                    let range = CURVE_RANGE[CURVE_RANGE_ORDER[i / per_range]];
                    let wb = CURVE_WB[i % CURVE_WB.len()];
                    let fpct = CURVE_FLOW_PCT[(i % per_range) / CURVE_WB.len()];
                    // The engine takes the hot water and solves the cold; a performance-curve record fixes the range.
                    // So the hot water is iterated (hot = cold + range) until the solved range is the record's.
                    let mut inp = draft.clone();
                    inp.duty.wet_bulb_c = wb;
                    inp.duty.dry_bulb_c = wb + (draft.duty.dry_bulb_c - draft.duty.wet_bulb_c);
                    inp.duty.water_flow_m3_hr = draft.duty.water_flow_m3_hr * fpct / 100.0;
                    // seed: extrapolate along this curve's own records (same range + flow, the wet bulbs before
                    // this one) so the iteration starts within a few hundredths and usually settles in two runs
                    let flow_kg = design * fpct / 100.0;
                    let same: Vec<&CurveRec> = self
                        .curves_acc
                        .iter()
                        .rev()
                        .filter(|r| r.range == range && (r.flow_kg_s - flow_kg).abs() < 1e-9)
                        .take(2)
                        .collect();
                    let (cold, seeded) = match same.as_slice() {
                        [a, b] if (a.wb - b.wb).abs() > 1e-9 => (
                            a.cold + (wb - a.wb) * (a.cold - b.cold) / (a.wb - b.wb),
                            true,
                        ),
                        [a, ..] => (a.cold + (wb - a.wb) * 0.6, true),
                        _ => (
                            wb + (draft.duty.target_cold_water_c - draft.duty.wet_bulb_c),
                            false,
                        ),
                    };
                    CurveIter {
                        inp,
                        wb,
                        range,
                        flow_kg_s: flow_kg,
                        cold,
                        loose: !seeded,
                        ok: None,
                        why: String::new(),
                        tries: 0,
                    }
                }
            };
            let range = it.range;
            // The solved cold water does not depend on the target the run states (the engine's
            // `thermal_model` never reads it); the target only gates the "thermal duty" check. So the
            // first run states a loose target (just under the hot water) to find the cold water, and
            // later runs a tight one (cold + 0.3 K), which keeps the mean water temperature - and so the
            // density the mass flow is converted at - within 0.15 K of the record's own.
            let s = now_ms();
            it.inp.duty.hot_water_c = it.cold + range;
            it.inp.duty.target_cold_water_c = if it.loose {
                it.cold + range - 0.1
            } else {
                it.cold + 0.3
            };
            self.curves_runs += 1;
            it.tries += 1;
            // issue #138: the record's run goes past the draft's throttle and leaves out the
            // performance chart (13 selections) the grid never reads - its cold water is the whole
            // run's to the bit (`run_point_is_run_without_the_chart`)
            let settled = match e.run_point(&it.inp) {
                Ok(o) if o.cold_water_c.is_finite() && o.cold_water_c > 0.0 => {
                    let moved = (o.cold_water_c - it.cold).abs();
                    it.cold = o.cold_water_c;
                    it.ok = Some(it.cold);
                    // CURVE_TOL: the record's range is then within this of the stated one
                    let settled = moved < CURVE_TOL && !it.loose;
                    it.loose = false;
                    settled
                }
                Ok(o) if !it.loose => {
                    // the tight target refused: the cold water moved past it; go loose again
                    it.why = o
                        .validation
                        .first()
                        .map(|l| l.message.clone())
                        .unwrap_or_default();
                    it.loose = true;
                    false
                }
                Ok(o) => {
                    it.why = o
                        .validation
                        .first()
                        .map(|l| {
                            l.message
                                .rsplit(": ")
                                .next()
                                .unwrap_or(&l.message)
                                .to_string()
                        })
                        .unwrap_or_else(|| "refused".into());
                    it.ok = None;
                    true
                }
                Err(e) => {
                    it.why = match e {
                        EngineError::Unavailable(m) | EngineError::Schema(m) => m,
                    };
                    it.ok = None;
                    true
                }
            };
            self.curves_ms += now_ms() - s;
            if !settled && it.tries < 6 {
                self.curves_it = Some(it);
                if now_ms() - t0 > BUDGET_MS {
                    break;
                }
                continue;
            }
            let CurveIter {
                wb,
                flow_kg_s,
                ok,
                why,
                ..
            } = it;
            if ok.is_none() {
                // the engine's own refusal text, without the run-specific numbers, so equal causes count together
                let key: String = why
                    .chars()
                    .map(|c| {
                        if c.is_ascii_digit() || c == '.' || c == '-' {
                            '#'
                        } else {
                            c
                        }
                    })
                    .collect();
                let key = key.split("##").collect::<Vec<_>>().join("#");
                match self.curves_fail_why.iter_mut().find(|x| x.0 == key) {
                    Some(x) => x.1 += 1,
                    None => self.curves_fail_why.push((key, 1)),
                }
            }
            match ok {
                Some(c) => self.curves_acc.push(CurveRec {
                    wb,
                    range,
                    flow_kg_s,
                    cold: c,
                }),
                None => self.curves_failed += 1,
            }
            self.curves_next += 1;
            if self
                .curves_next
                .is_multiple_of(CURVE_WB.len() * CURVE_FLOW_PCT.len())
                && self.curves_next < total
            {
                // a range is complete: show what there is
                self.curves = Some(Arc::new(CurveData {
                    design_flow_kg_s: design,
                    recs: self.curves_acc.clone(),
                    failed: self.curves_failed,
                    fail_why: self.curves_fail_why.clone(),
                    total,
                    runs: self.curves_runs,
                    ms: self.curves_ms,
                }));
            }
            if now_ms() - t0 > BUDGET_MS {
                break;
            }
        }
        if self.curves_next >= total {
            self.curves = Some(Arc::new(CurveData {
                design_flow_kg_s: design,
                recs: std::mem::take(&mut self.curves_acc),
                failed: self.curves_failed,
                fail_why: std::mem::take(&mut self.curves_fail_why),
                total,
                runs: self.curves_runs,
                ms: self.curves_ms,
            }));
            self.computed.curves += 1;
            return false;
        }
        true
    }

    #[allow(unused_variables)]
    fn solve_xf(&self, draft: &EngineInput) -> Result<XfGrid, String> {
        #[cfg(feature = "real-engine")]
        {
            let size = self.size.as_ref().ok_or("selection not run")?;
            let best = size
                .ranked(Some(true))
                .first()
                .map(|c| (*c).clone())
                .ok_or("no feasible crossflow candidate at this duty")?;
            let s = now_ms();
            let input = eng::CrossflowGridInput::new(
                draft.duty.hot_water_c,
                draft.duty.dry_bulb_c,
                draft.duty.wet_bulb_c,
                water_kg_s(draft),
                best.dry_air_kg_s,
                best.available_merkel,
            )
            .with_pressure(draft.duty.pressure_pa)
            .with_salinity(draft.duty.salinity_g_kg)
            .with_cells(XF_CELLS, XF_CELLS);
            let r = eng::solve_crossflow_grid(&input).map_err(|e| e.to_string())?;
            // Issue #84 AC 2: the optional three-grid study runs with the result, so the panel can
            // show the solver's numerical quality next to its answer (and the CLI test can pin it).
            let study = eng::crossflow_convergence_study(&eng::CrossflowStudyInput::new(input))
                .map_err(|e| e.to_string())?;
            let conv = r.grid_convergence;
            // Issue #86 AC 1: the two rates the crossflow canvas animates are the engine's quantities, read
            // where the engine keeps them - the water loading of this duty over this candidate's fill, and the
            // candidate fan's rpm through its own record. Nothing here is a drawn constant.
            let catalog = self.catalog.as_ref().and_then(|c| c.as_ref().ok());
            let fill_area_m2 = catalog
                .and_then(|c| c.towers.iter().find(|t| t.id == best.tower_id))
                .map(|t| t.physics.fill_area_m2);
            let water_loading_kg_m2_s =
                m::water_loading_kg_m2_s(water_kg_s(draft), fill_area_m2.unwrap_or(0.0))
                    .unwrap_or(0.0);
            let rpm = catalog
                .and_then(|c| c.fans.iter().find(|f| f.physics.id == best.fan_id))
                .and_then(|f| m::rpm(best.speed, f.nominal_rpm));
            Ok(XfGrid {
                cand: best,
                hot_c: draft.duty.hot_water_c,
                wb_c: draft.duty.wet_bulb_c,
                cold_c: r.cold_water_c,
                range_c: r.range_c,
                approach_c: r.approach_c,
                heat_kw: r.heat_transfer_kw,
                water_kw: r.water_energy_kw,
                cp_kj_kg_k: r.cp_water_kj_kg_k,
                outlet_db: r.outlet_air_state.dry_bulb_c,
                outlet_hr: r.outlet_air_state.humidity_ratio,
                outlet_h: r.outlet_air_state.enthalpy_kj_kg_dry_air,
                water: r.water_temperature_grid_c,
                air_h: r.air_enthalpy_grid_kj_kg_dry_air,
                water_loading_kg_m2_s,
                rpm,
                cells: conv.coarse_cells,
                fine_cells: conv.fine_cells,
                coarse_cold_c: conv.coarse_cold_water_c,
                fine_cold_c: conv.fine_cold_water_c,
                richardson_cold_c: conv.richardson_cold_water_c,
                error_c: conv.estimated_discretization_error_c,
                error_meaning: conv.error_estimate_meaning,
                study: XfStudy {
                    cells: study.cells,
                    cold: study.cold_water_c,
                    order: study.observed_order,
                    extrapolated: study.extrapolated_cold_water_c,
                    fine_error: study.fine_grid_error_estimate_c,
                    gci_pct: study.grid_convergence_index_pct,
                    note: study.interpretation,
                },
                ms: now_ms() - s,
            })
        }
        #[cfg(not(feature = "real-engine"))]
        Err("not available in this build".into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Want {
    Nothing,
    Size,
    Rate,
    Curves,
    Crossflow,
    /// Report's Charts page: the selection first, then the curves grid.
    Report,
}

// ======================================================================================= rate

/// The Rate screen's points: the fitted characteristic of each STUB test point, and the Monte Carlo
/// work its band is still owed (one [`Mc`] per accepted point), which [`Cache::step`] draws a frame's
/// budget at a time.
#[cfg(feature = "real-engine")]
fn rate(d: &EngineInput, o: &EngineOutput) -> (RateData, Vec<Option<Mc>>) {
    let m_w = water_kg_s(d);
    let m_a = dry_air_kg_s(o).unwrap_or(0.0);
    let mut data = RateData {
        points: Vec::new(),
        dry_air_kg_s: m_a,
        water_kg_s: m_w,
    };
    let mut mcs = Vec::new();
    let design = eng::CapabilityCondition::new(
        m_w,
        m_a,
        d.duty.hot_water_c,
        d.duty.target_cold_water_c,
        d.duty.wet_bulb_c,
        d.duty.dry_bulb_c,
    )
    .with_pressure(d.duty.pressure_pa);
    for (i, (fw, fa, hot, cold, wb, db)) in test_points(&d.duty).iter().enumerate() {
        let test = eng::CapabilityCondition::new(m_w * fw, m_a * fa, *hot, *cold, *wb, *db)
            .with_pressure(d.duty.pressure_pa);
        let base = eng::CharacteristicCapabilityInput::new(design, test).with_curve_points(48);
        let r = eng::evaluate_characteristic_capability(&base);
        let (p, mc) = match r {
            Ok(r) => {
                let p = RatePoint {
                    capability_pct: r.capability_pct,
                    design_lg: r.design_water_to_dry_air_ratio,
                    test_lg: r.test_water_to_dry_air_ratio,
                    cap_lg: r.capability_water_to_dry_air_ratio,
                    test_c: r.test_characteristic_coefficient,
                    exponent_m: r.characteristic_exponent,
                    curve: r
                        .curves
                        .iter()
                        .map(|c| {
                            (
                                c.water_to_dry_air_ratio,
                                c.design_demand_merkel,
                                c.test_characteristic_merkel,
                            )
                        })
                        .collect(),
                    ..RatePoint::default()
                };
                // The Monte Carlo band for every point (300 samples each), drawn in slices.
                (Ok(p), Some(Mc::new(rate_mc_input(m_w, m_a, base, i))))
            }
            Err(e) => (Err(e.to_string()), None),
        };
        data.points.push(p);
        mcs.push(mc);
    }
    (data, mcs)
}

#[cfg(not(feature = "real-engine"))]
fn rate(d: &EngineInput, o: &EngineOutput) -> RateData {
    RateData {
        points: Vec::new(),
        dry_air_kg_s: dry_air_kg_s(o).unwrap_or(0.0),
        water_kg_s: water_kg_s(d),
    }
}

/// Test point `i`'s Monte Carlo input: the STUB sigmas on its test condition, the fitted base at
/// 8 curve points, [`STUB_SIGMA_SAMPLES`] samples and the point's own seed.
#[cfg(feature = "real-engine")]
fn rate_mc_input(
    m_w: f64,
    m_a: f64,
    base: eng::CharacteristicCapabilityInput,
    i: usize,
) -> eng::MonteCarloInput {
    let (fw, fa, ..) = STUB_TEST_OFFSETS[i];
    let (sf, sa, st, swb) = STUB_SIGMA;
    let unc = eng::CapabilityUncertainty {
        design: Vec::new(),
        test: vec![
            (eng::ConditionField::WaterMassFlowKgS, m_w * fw * sf),
            (eng::ConditionField::DryAirMassFlowKgS, m_a * fa * sa),
            (eng::ConditionField::HotWaterC, st),
            (eng::ConditionField::ColdWaterC, st),
            (eng::ConditionField::WetBulbC, swb),
        ],
        characteristic_exponent: None,
        salinity_g_kg: None,
    };
    eng::MonteCarloInput::new(base.with_curve_points(8), unc)
        .with_samples(STUB_SIGMA_SAMPLES)
        .with_seed(20261002.0 + i as f64)
}

/// Issue #138: `monte_carlo_characteristic_capability`, one sample per call, so a frame draws only
/// its budget of the 300. The loop is the engine's own (`rust/src/capability.rs`): the same seeded
/// stream (`create_seeded_random` / `gaussian_random`, the engine's exports), the same perturbation
/// order (the design fields, the test fields, then the exponent and salinity sigmas; a non-finite
/// sigma skipped, a non-finite target left as it is), the same acceptance rule and the same mean and
/// sample variance - so the band is the engine's to the bit (`rate_mc_slices_equal_the_engine_call`).
#[cfg(feature = "real-engine")]
struct Mc {
    input: eng::MonteCarloInput,
    random: eng::SeededRandom,
    caps: Vec<f64>,
    drawn: usize,
    samples: usize,
}

#[cfg(feature = "real-engine")]
impl Mc {
    fn new(input: eng::MonteCarloInput) -> Self {
        Mc {
            random: eng::create_seeded_random(input.seed),
            samples: input.samples,
            input,
            caps: Vec::new(),
            drawn: 0,
        }
    }

    fn done(&self) -> bool {
        self.drawn >= self.samples
    }

    fn sample(&mut self) {
        let mut sampled = self.input.base_input;
        let random = &mut self.random;
        let mut perturb = |target: &mut f64, sigma: f64| {
            if target.is_finite() {
                *target += sigma * eng::gaussian_random(&mut || random.next_value());
            }
        };
        let fields = |c: &mut eng::CapabilityCondition,
                      sigmas: &[(eng::ConditionField, f64)],
                      perturb: &mut dyn FnMut(&mut f64, f64)| {
            for (field, sigma) in sigmas {
                if !sigma.is_finite() {
                    continue;
                }
                let target = match field {
                    eng::ConditionField::WaterMassFlowKgS => &mut c.water_mass_flow_kg_s,
                    eng::ConditionField::DryAirMassFlowKgS => &mut c.dry_air_mass_flow_kg_s,
                    eng::ConditionField::HotWaterC => &mut c.hot_water_c,
                    eng::ConditionField::ColdWaterC => &mut c.cold_water_c,
                    eng::ConditionField::WetBulbC => &mut c.wet_bulb_c,
                    eng::ConditionField::DryBulbC => &mut c.dry_bulb_c,
                    eng::ConditionField::PressurePa => match c.pressure_pa.as_mut() {
                        Some(pressure) => pressure,
                        None => continue,
                    },
                };
                perturb(target, *sigma);
            }
        };
        let unc = &self.input.uncertainty;
        fields(&mut sampled.design, &unc.design, &mut perturb);
        fields(&mut sampled.test, &unc.test, &mut perturb);
        if let Some(sigma) = unc.characteristic_exponent {
            perturb(&mut sampled.characteristic_exponent, sigma);
        }
        if let Some(sigma) = unc.salinity_g_kg {
            perturb(&mut sampled.salinity_g_kg, sigma);
        }
        if let Ok(r) = eng::evaluate_characteristic_capability(&sampled) {
            if r.capability_pct.is_finite() {
                self.caps.push(r.capability_pct);
            }
        }
        self.drawn += 1;
    }

    /// (mean capability %, the expanded uncertainty 2 sigma) - `None` where the engine refuses the run
    /// (fewer than `max(20, samples / 2)` accepted draws).
    fn result(&self) -> Option<(f64, f64)> {
        let n = self.caps.len();
        if (n as f64) < eng::js_max(20.0, self.samples as f64 * 0.5) {
            return None;
        }
        let mean = self.caps.iter().sum::<f64>() / n as f64;
        let variance =
            self.caps.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1).max(1) as f64;
        Some((mean, 2.0 * variance.sqrt()))
    }
}

// ======================================================================================= water

/// The water balance at `cycles`, live: evaporation from the engine's run of the draft, drift from the
/// draft eliminator's own curve at the run's face velocity, and the balance from `water_balance.rs`.
#[allow(unused_variables)]
pub fn water(d: &EngineInput, o: &EngineOutput, cycles: f64) -> Result<WaterData, String> {
    let m_w = water_kg_s(d);
    let rho = density(d);
    #[cfg(feature = "real-engine")]
    {
        let evap = o.evaporation_pct / 100.0 * m_w;
        let v = o.airflow_m3_s / d.tower.drift_area_m2.max(1e-6);
        let rec = eng::DriftEliminatorRecord {
            id: d.drift.id.clone(),
            curve: d
                .drift
                .curve
                .iter()
                .map(|p| eng::DriftCurvePoint {
                    face_velocity_ms: p.face_velocity_m_s,
                    drift_ppm: p.drift_ppm,
                    pressure_drop_pa: p.pressure_drop_pa,
                })
                .collect(),
        };
        let ppm = eng::drift_performance_at_velocity(&rec, v)
            .map_err(|e| e.to_string())?
            .drift_ppm;
        let drift = eng::drift_loss_kg_s(m_w, ppm).map_err(|e| e.to_string())?;
        let bal = |c: f64| {
            eng::cooling_tower_water_balance(
                &eng::WaterBalanceInput::new(evap)
                    .with_drift_kg_s(drift)
                    .with_cycles_of_concentration(c),
            )
            .map_err(|e| e.to_string())
        };
        let b = bal(cycles)?;
        let at_duty = bal(d.duty.cycles_of_concentration)?;
        Ok(WaterData {
            circulating_kg_s: m_w,
            evaporation_kg_s: b.evaporation_kg_s,
            drift_kg_s: b.drift_kg_s,
            drift_ppm: ppm,
            blowdown_kg_s: b.blowdown_kg_s,
            makeup_kg_s: b.makeup_kg_s,
            cycles: b.cycles_of_concentration,
            density: rho,
            check_m3_h: (at_duty.makeup_kg_s / rho * 3600.0 - o.makeup_m3_hr).abs(),
        })
    }
    #[cfg(not(feature = "real-engine"))]
    Err("not available in this build".into())
}

// ===================================================================================== answer

/// Drift at this run: the draft eliminator's own curve (`drift_performance_at_velocity`) at the run's face
/// velocity (airflow / drift area). The same call Water uses; `None` without the real engine.
#[allow(unused_variables)]
pub fn drift_ppm_at(d: &EngineInput, o: &EngineOutput) -> Option<f64> {
    #[cfg(feature = "real-engine")]
    {
        let v = o.airflow_m3_s / d.tower.drift_area_m2.max(1e-6);
        let rec = eng::DriftEliminatorRecord {
            id: d.drift.id.clone(),
            curve: d
                .drift
                .curve
                .iter()
                .map(|p| eng::DriftCurvePoint {
                    face_velocity_ms: p.face_velocity_m_s,
                    drift_ppm: p.drift_ppm,
                    pressure_drop_pa: p.pressure_drop_pa,
                })
                .collect(),
        };
        eng::drift_performance_at_velocity(&rec, v)
            .ok()
            .map(|r| r.drift_ppm)
    }
    #[cfg(not(feature = "real-engine"))]
    None
}

/// The recorded requirements the fixture carries (`provenance.fixed`): max drift ppm, max electrical
/// input kW, and the minimum thermal margin (K). Read once per session.
pub fn recorded_limits(fixture_text: &str) -> (Option<f64>, Option<f64>, f64) {
    let v: serde_json::Value = serde_json::from_str(fixture_text).unwrap_or_default();
    let f = &v["provenance"]["fixed"];
    (
        f["maxDriftPpm"].as_f64(),
        f["maxElectricalInputKW"].as_f64(),
        f["minimumThermalMarginC"].as_f64().unwrap_or(0.0),
    )
}

/// The answer card's numbers: every one is this frame's run (or the eliminator curve at that run).
pub fn answer(
    d: &EngineInput,
    o: Option<&EngineOutput>,
    limits: (Option<f64>, Option<f64>, f64),
) -> super::Answer {
    let target = d.duty.target_cold_water_c;
    let Some(o) = o else {
        return super::Answer {
            target_c: target,
            refused: Some("no result for this duty".into()),
            ..Default::default()
        };
    };
    if !o.validation.is_empty() {
        return super::Answer {
            target_c: target,
            refused: Some(
                o.validation
                    .first()
                    .map(|l| l.message.clone())
                    .unwrap_or_default(),
            ),
            ..Default::default()
        };
    }
    let margin = target - o.cold_water_c;
    super::Answer {
        cold_c: o.cold_water_c,
        target_c: target,
        margin_k: margin,
        pass: margin >= limits.2,
        approach_k: o.approach_c,
        fan_kw: o.fan_power_kw,
        drift_ppm: drift_ppm_at(d, o),
        drift_limit_ppm: limits.0,
        makeup_m3_h: o.makeup_m3_hr,
        refused: None,
        steps: o.worked_steps.clone(),
        range_k: Some(o.range_c),
        heat_kw: o
            .worked_steps
            .iter()
            .find(|s| s.label == "Heat load")
            .and_then(|s| s.value),
        kavl: Some(o.kavl_total),
        ..Default::default()
    }
}

// ===================================================================================== curves

#[cfg(feature = "real-engine")]
fn recs(c: &CurveData) -> Vec<eng::PerformanceCurveRecord> {
    c.recs
        .iter()
        .map(|r| eng::PerformanceCurveRecord {
            wet_bulb_c: r.wb,
            range_c: r.range,
            water_flow_kg_s: r.flow_kg_s,
            cold_water_c: r.cold,
        })
        .collect()
}

/// `predict_cold_water_from_performance_curves` on the engine-built grid. (cold, extrapolated)
#[allow(unused_variables)]
pub fn predict_cold(c: &CurveData, wb: f64, range: f64, flow: f64) -> Option<(f64, bool)> {
    #[cfg(feature = "real-engine")]
    {
        eng::predict_cold_water_from_performance_curves(&recs(c), wb, range, flow)
            .ok()
            .map(|p| (p.cold_water_c, p.extrapolated))
    }
    #[cfg(not(feature = "real-engine"))]
    None
}

/// The inverse lookup: `predict_water_flow_from_performance_curves`. (flow kg/s, predicted cold)
#[allow(unused_variables)]
pub fn predict_flow(c: &CurveData, wb: f64, range: f64, cold: f64) -> Option<(f64, f64)> {
    #[cfg(feature = "real-engine")]
    {
        eng::predict_water_flow_from_performance_curves(&recs(c), wb, range, cold)
            .ok()
            .map(|p| (p.water_flow_kg_s, p.predicted_cold_water_c))
    }
    #[cfg(not(feature = "real-engine"))]
    None
}

/// Issue #138: a heavy result is computed once per change of what it reads - a tab switch, a repeat
/// frame or an edit it does not read is never a recompute, and an edit it does read always is.
#[cfg(all(test, feature = "real-engine"))]
mod tests {
    use super::*;
    use cockpit::engine::Engine;
    use cockpit::fixture_engine::FixtureEngine;

    const FIXTURE: &str = include_str!("../../assets/fixture.json");

    /// Step `want` until the cache says it has nothing left to do (the shell's frame loop).
    fn settle(c: &mut Cache, want: Want, d: &EngineInput, o: &EngineOutput, e: &dyn CockpitEngine) {
        let mut frames = 0;
        while c.step(want, FIXTURE, d, Some(o), Some(e)) {
            frames += 1;
            assert!(frames < 100_000, "{want:?} never settled");
        }
    }

    #[test]
    fn heavy_results_are_not_recomputed_when_their_inputs_did_not_change() {
        // The curves grid runs whatever engine the shell hands it; the recorded replay keeps the
        // 72-record grid cheap here (the real engine's grid is pinned by `fixture_equals_cli`).
        let e = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        let d = e.default_input();
        let o = e.run(&d).expect("the fixture duty runs");
        let mut c = Cache::default();
        let tabs = [
            Want::Size,
            Want::Crossflow,
            Want::Rate,
            Want::Curves,
            Want::Report,
        ];
        for want in tabs {
            settle(&mut c, want, &d, &o, &e);
        }
        let once = Computed {
            size: 1,
            rate: 1,
            curves: 1,
            xf: 1,
        };
        assert_eq!(c.computed, once, "every heavy result is computed once");

        // switching tabs back and forth, and the frames in between: nothing recomputes
        for _ in 0..3 {
            for want in tabs.iter().rev().chain([Want::Nothing].iter()) {
                settle(&mut c, *want, &d, &o, &e);
            }
        }
        assert_eq!(c.computed, once, "a tab switch is not a recompute");

        // an edit only the curves read (the draft's speed ratio, same run handed in): only the
        // curves recompute - the selection, the crossflow grid and the rate points stay
        let mut faster = d.clone();
        faster.speed_ratio += 0.05;
        for want in tabs {
            settle(&mut c, want, &faster, &o, &e);
        }
        assert_eq!(
            c.computed,
            Computed { curves: 2, ..once },
            "an edit the selection and the rate do not read recomputes only the curves"
        );

        // an edit everything reads (the wet bulb): every result is dropped, so nothing stale paints
        let mut wetter = faster.clone();
        wetter.duty.wet_bulb_c += 0.5;
        c.step(Want::Nothing, FIXTURE, &wetter, Some(&o), Some(&e));
        assert!(c.size.is_none() && c.xf.is_none() && c.rate.is_none() && c.curves.is_none());
        settle(&mut c, Want::Rate, &wetter, &o, &e);
        assert_eq!(
            c.computed.rate, 2,
            "the edited duty's rate points are computed again"
        );
    }

    /// Issue #137 x #138: the rate result's key reads the run's air flow under the step name the
    /// build's engine writes now ("Air flow through the fill"). The recorded replay above still carries
    /// the old name, so only the real run shows it: a key that read the old name alone hashed NaN for
    /// every real run, and an air-flow change kept the stale rate points.
    #[test]
    fn the_rate_key_reads_the_real_runs_air_flow() {
        let d = FixtureEngine::from_json(FIXTURE)
            .expect("the fixture parses")
            .default_input();
        let engine = crate::engine_select::build(FIXTURE, None).expect("the build's engine");
        let o = engine.run(&d).expect("the fixture duty runs");
        let m_a = dry_air_kg_s(&o).expect("the real run carries its air-flow step");
        assert!(m_a.is_finite() && m_a > 0.0, "air flow {m_a}");
        let mut c = Cache::default();
        c.sync(FIXTURE, &d, Some(&o));
        c.rate = Some(Arc::new(RateData::default()));
        c.sync(FIXTURE, &d, Some(&o));
        assert!(c.rate.is_some(), "the same run keeps its rate result");
        let mut more_air = o.clone();
        for s in &mut more_air.worked_steps {
            if s.label == "Air flow through the fill" {
                s.value = Some(m_a * 1.1);
            }
        }
        c.sync(FIXTURE, &d, Some(&more_air));
        assert!(
            c.rate.is_none(),
            "an air-flow change drops the rate result it was computed on"
        );
    }

    /// The Rate band drawn a sample per call is the engine's one-call Monte Carlo, to the bit.
    #[test]
    fn rate_mc_slices_equal_the_engine_call() {
        let e = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        let d = e.default_input();
        let o = e.run(&d).expect("the fixture duty runs");
        let (data, mcs) = rate(&d, &o);
        let mut checked = 0;
        for (p, mc) in data.points.iter().zip(mcs) {
            let (Ok(p), Some(mut mc)) = (p, mc) else {
                continue;
            };
            let whole = eng::monte_carlo_characteristic_capability(&mc.input)
                .expect("the engine's Monte Carlo runs");
            while !mc.done() {
                mc.sample();
            }
            let (mean, u) = mc.result().expect("the sliced Monte Carlo accepts the run");
            assert_eq!(mean.to_bits(), whole.mean_capability_pct.to_bits());
            assert_eq!(
                u.to_bits(),
                whole.expanded_uncertainty_approx_pct_points.to_bits()
            );
            assert_eq!(mc.caps.len(), whole.samples_accepted);
            assert!(p.capability_pct.is_finite());
            checked += 1;
        }
        assert_eq!(
            checked,
            STUB_TEST_OFFSETS.len(),
            "every test point has a band"
        );
    }
}
