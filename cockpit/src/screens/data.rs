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

use cockpit::engine::{Engine, EngineError, EngineInput, EngineOutput};

#[cfg(feature = "real-engine")]
use cockpit_adapter::synergy_drafthouse as eng;

use crate::clock::now_ms;

/// The frame budget for lazy engine work, in ms.
const BUDGET_MS: f64 = 14.0;

// ======================================================================================== stubs

/// STUB: three field-test readings for the Rate screen, as fractions/offsets of the design duty. A real
/// acceptance test (CTI ATC-105 style) brings its own readings; nothing in the fixture records one.
/// (water-flow fraction, dry-air fraction, hot water C, cold water C, wet bulb C, dry bulb C)
pub const STUB_TEST_POINTS: [(f64, f64, f64, f64, f64, f64); 3] = [
    (0.97, 0.99, 41.6, 31.9, 26.6, 32.4),
    (1.02, 1.00, 42.3, 32.6, 27.1, 33.0),
    (0.94, 0.97, 40.8, 31.0, 25.9, 31.6),
];
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
/// Compare's three variants: what each edits on a copy of the draft (the engine then runs each copy).
/// These are design choices of the screen, not stubs - the results are engine output.
pub const CMP_FILL_DEEPER_M: f64 = 0.3;
pub const CMP_FAN_FASTER: f64 = 0.10;

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

#[derive(Clone, Debug, Default)]
pub struct XfGrid {
    pub cand: Cand,
    pub cold_c: f64,
    pub hot_c: f64,
    pub wb_c: f64,
    pub water: Vec<Vec<f64>>,
    pub air_h: Vec<Vec<f64>>,
    pub outlet_db: f64,
    pub ms: f64,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub key: &'static str,
    pub label: String,
    pub change: String,
    pub input: EngineInput,
    pub out: Result<EngineOutput, String>,
}

/// The lazy cache. Keyed by the duty it was computed for, so an edited duty recomputes.
#[derive(Default)]
pub struct Cache {
    pub key: String,
    pub size: Option<SizeData>,
    size_next: usize,
    size_acc: Vec<TowerRun>,
    pub rate: Option<RateData>,
    pub curves: Option<CurveData>,
    curves_next: usize,
    curves_acc: Vec<CurveRec>,
    curves_failed: usize,
    curves_fail_why: Vec<(String, usize)>,
    curves_runs: usize,
    curves_ms: f64,
    pub xf: Option<Result<XfGrid, String>>,
    pub variants: Option<Vec<Variant>>,
    pub engine_ms_last: f64,
    #[cfg(feature = "real-engine")]
    catalog: Option<Result<eng::SelectionCatalog, String>>,
}

fn duty_key(d: &EngineInput) -> String {
    format!(
        "{:.3}|{:.2}|{:.2}|{:.2}|{:.2}|{}|{}|{:.3}|{}",
        d.duty.water_flow_m3_hr,
        d.duty.hot_water_c,
        d.duty.target_cold_water_c,
        d.duty.wet_bulb_c,
        d.duty.dry_bulb_c,
        d.tower.id,
        d.fan.id,
        d.speed_ratio,
        d.fill_layers
            .iter()
            .map(|l| format!("{}@{:.2}", l.fill_id, l.depth_m))
            .collect::<Vec<_>>()
            .join("+")
    )
}

impl Cache {
    pub fn sync(&mut self, draft: &EngineInput) {
        let k = duty_key(draft);
        if k != self.key {
            let keep_cat;
            #[cfg(feature = "real-engine")]
            {
                keep_cat = self.catalog.take();
            }
            #[cfg(not(feature = "real-engine"))]
            {
                keep_cat = ();
            }
            *self = Cache::default();
            #[cfg(feature = "real-engine")]
            {
                self.catalog = keep_cat;
            }
            #[cfg(not(feature = "real-engine"))]
            let _ = keep_cat;
            self.key = k;
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
        power_kw: c.electrical_input_kw,
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
        engine: Option<&dyn Engine>,
    ) -> bool {
        self.sync(draft);
        let t0 = now_ms();
        let busy = match want {
            Want::Size => self.step_size(fixture_text, draft, t0),
            Want::Crossflow => {
                if self.size.is_none() {
                    self.step_size(fixture_text, draft, t0)
                } else {
                    if self.xf.is_none() {
                        self.xf = Some(self.solve_xf(draft));
                    }
                    false
                }
            }
            Want::Rate => {
                if self.rate.is_none() {
                    if let Some(o) = out {
                        self.rate = Some(rate(draft, o));
                    }
                }
                false
            }
            Want::Curves => match engine {
                Some(e) => self.step_curves(draft, e, t0),
                None => false,
            },
            Want::Compare => {
                if self.variants.is_none() {
                    if let Some(e) = engine {
                        self.variants = Some(variants(draft, e));
                    }
                }
                false
            }
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
                self.size = Some(SizeData::default());
                return false;
            };
            let req = requirements(draft);
            while self.size_next < cat.towers.len() {
                let tower = cat.towers[self.size_next].clone();
                let crossflow = tower.tower_type == eng::TowerType::Crossflow;
                let mut one = cat.clone();
                one.towers = vec![tower.clone()];
                let s = now_ms();
                let run = eng::run_selection(
                    &eng::SelectionInput::new(&one).with_requirements(req.clone()),
                );
                let ms = now_ms() - s;
                let tr = match run {
                    Ok(run) => TowerRun {
                        id: tower.id.clone(),
                        crossflow,
                        footprint_m2: tower.footprint_m2,
                        max_flow_kg_s: tower.max_water_mass_flow_kg_s,
                        feasible: run.feasible_candidate_count(),
                        rejections: run.rejection_summary.clone(),
                        cands: run
                            .candidates
                            .iter()
                            .map(|c| cand_of(c, crossflow, tower.footprint_m2))
                            .collect(),
                        ms,
                    },
                    Err(e) => TowerRun {
                        id: tower.id.clone(),
                        crossflow,
                        footprint_m2: tower.footprint_m2,
                        max_flow_kg_s: tower.max_water_mass_flow_kg_s,
                        rejections: vec![(e.to_string(), 1)],
                        ms,
                        ..TowerRun::default()
                    },
                };
                self.size_acc.push(tr);
                self.size_next += 1;
                if now_ms() - t0 > BUDGET_MS {
                    break;
                }
            }
            if self.size_next >= cat.towers.len() {
                let towers = std::mem::take(&mut self.size_acc);
                let ms = towers.iter().map(|t| t.ms).sum();
                self.size = Some(SizeData {
                    water_kg_s: req.water_mass_flow_kg_s,
                    towers,
                    ms,
                });
                return false;
            }
            true
        }
        #[cfg(not(feature = "real-engine"))]
        {
            self.size = Some(SizeData::default());
            false
        }
    }

    fn step_curves(&mut self, draft: &EngineInput, e: &dyn Engine, t0: f64) -> bool {
        let total = CURVE_WB.len() * CURVE_RANGE.len() * CURVE_FLOW_PCT.len();
        if self.curves_next >= total && self.curves.is_some() {
            return false;
        }
        let design = water_kg_s(draft);
        while self.curves_next < total {
            let i = self.curves_next;
            // range-major, the middle range (the one the screen opens on) first, so its chart appears
            // after a third of the work; the others fill in behind it
            let per_range = CURVE_WB.len() * CURVE_FLOW_PCT.len();
            let range = CURVE_RANGE[CURVE_RANGE_ORDER[i / per_range]];
            let wb = CURVE_WB[i % CURVE_WB.len()];
            let fpct = CURVE_FLOW_PCT[(i % per_range) / CURVE_WB.len()];
            let s = now_ms();
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
            let (mut cold, seeded) = match same.as_slice() {
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
            let mut ok = None;
            let mut why = String::new();
            // The solved cold water does not depend on the target the run states (the engine's
            // `thermal_model` never reads it); the target only gates the "thermal duty" check. So the
            // first run states a loose target (just under the hot water) to find the cold water, and
            // later runs a tight one (cold + 0.3 K), which keeps the mean water temperature - and so the
            // density the mass flow is converted at - within 0.15 K of the record's own.
            let mut loose = !seeded;
            for _ in 0..6 {
                inp.duty.hot_water_c = cold + range;
                inp.duty.target_cold_water_c = if loose {
                    cold + range - 0.1
                } else {
                    cold + 0.3
                };
                self.curves_runs += 1;
                match e.run(&inp) {
                    Ok(o) if o.cold_water_c.is_finite() && o.cold_water_c > 0.0 => {
                        let moved = (o.cold_water_c - cold).abs();
                        cold = o.cold_water_c;
                        ok = Some(cold);
                        // CURVE_TOL: the record's range is then within this of the stated one
                        if moved < CURVE_TOL && !loose {
                            break;
                        }
                        loose = false;
                    }
                    Ok(o) if !loose => {
                        // the tight target refused: the cold water moved past it; go loose again
                        why = o
                            .validation
                            .first()
                            .map(|l| l.message.clone())
                            .unwrap_or_default();
                        loose = true;
                    }
                    Ok(o) => {
                        why = o
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
                        ok = None;
                        break;
                    }
                    Err(e) => {
                        why = match e {
                            EngineError::Unavailable(m) | EngineError::Schema(m) => m,
                        };
                        ok = None;
                        break;
                    }
                }
            }
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
                    flow_kg_s: design * fpct / 100.0,
                    cold: c,
                }),
                None => self.curves_failed += 1,
            }
            self.curves_ms += now_ms() - s;
            self.curves_next += 1;
            if self
                .curves_next
                .is_multiple_of(CURVE_WB.len() * CURVE_FLOW_PCT.len())
                && self.curves_next < total
            {
                // a range is complete: show what there is
                self.curves = Some(CurveData {
                    design_flow_kg_s: design,
                    recs: self.curves_acc.clone(),
                    failed: self.curves_failed,
                    fail_why: self.curves_fail_why.clone(),
                    total,
                    runs: self.curves_runs,
                    ms: self.curves_ms,
                });
            }
            if now_ms() - t0 > BUDGET_MS {
                break;
            }
        }
        if self.curves_next >= total {
            self.curves = Some(CurveData {
                design_flow_kg_s: design,
                recs: std::mem::take(&mut self.curves_acc),
                failed: self.curves_failed,
                fail_why: std::mem::take(&mut self.curves_fail_why),
                total,
                runs: self.curves_runs,
                ms: self.curves_ms,
            });
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
            .with_cells(14, 14)
            .with_richardson(false);
            let r = eng::solve_crossflow_grid(&input).map_err(|e| e.to_string())?;
            Ok(XfGrid {
                cand: best,
                cold_c: r.cold_water_c,
                hot_c: draft.duty.hot_water_c,
                wb_c: draft.duty.wet_bulb_c,
                water: r.water_temperature_grid_c,
                air_h: r.air_enthalpy_grid_kj_kg_dry_air,
                outlet_db: r.outlet_air_state.dry_bulb_c,
                ms: now_ms() - s,
            })
        }
        #[cfg(not(feature = "real-engine"))]
        Err("the real engine is not in this build".into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Want {
    Nothing,
    Size,
    Rate,
    Curves,
    Compare,
    Crossflow,
    /// Report's Charts page: the selection first, then the curves grid.
    Report,
}

// ======================================================================================= rate

#[allow(unused_variables)]
fn rate(d: &EngineInput, o: &EngineOutput) -> RateData {
    let m_w = water_kg_s(d);
    let m_a = step_value(o, "Dry-air mass flow").unwrap_or(0.0);
    let mut data = RateData {
        points: Vec::new(),
        dry_air_kg_s: m_a,
        water_kg_s: m_w,
    };
    #[cfg(feature = "real-engine")]
    {
        let design = eng::CapabilityCondition::new(
            m_w,
            m_a,
            d.duty.hot_water_c,
            d.duty.target_cold_water_c,
            d.duty.wet_bulb_c,
            d.duty.dry_bulb_c,
        )
        .with_pressure(d.duty.pressure_pa);
        for (i, (fw, fa, hot, cold, wb, db)) in STUB_TEST_POINTS.iter().enumerate() {
            let test = eng::CapabilityCondition::new(m_w * fw, m_a * fa, *hot, *cold, *wb, *db)
                .with_pressure(d.duty.pressure_pa);
            let base = eng::CharacteristicCapabilityInput::new(design, test).with_curve_points(48);
            let r = eng::evaluate_characteristic_capability(&base);
            let p = match r {
                Ok(r) => {
                    let mut p = RatePoint {
                        capability_pct: r.capability_pct,
                        design_lg: r.design_water_to_dry_air_ratio,
                        test_lg: r.test_water_to_dry_air_ratio,
                        cap_lg: r.capability_water_to_dry_air_ratio,
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
                    // The Monte Carlo band only for the first point at start; the others on demand would
                    // cost the same, so all three run once (300 samples each).
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
                    let mc = eng::MonteCarloInput::new(base.with_curve_points(8), unc)
                        .with_samples(STUB_SIGMA_SAMPLES)
                        .with_seed(20261002.0 + i as f64);
                    if let Ok(m) = eng::monte_carlo_characteristic_capability(&mc) {
                        p.mc_mean = Some(m.mean_capability_pct);
                        let u = m.expanded_uncertainty_approx_pct_points;
                        p.band = Some((r.capability_pct - u, r.capability_pct + u));
                    }
                    Ok(p)
                }
                Err(e) => Err(e.to_string()),
            };
            data.points.push(p);
        }
    }
    data
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
    Err("the real engine is not in this build".into())
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
            refused: Some("the engine returned no run for this draft".into()),
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

// ===================================================================================== compare

fn variants(d: &EngineInput, e: &dyn Engine) -> Vec<Variant> {
    let mut out = Vec::new();
    let a = d.clone();
    out.push(Variant {
        key: "A",
        label: "Current".into(),
        change: "as drafted".into(),
        out: e.run(&a).map_err(|x| format!("{x:?}")),
        input: a,
    });
    // B: the lower fill layer 0.3 m deeper (the same records, a taller stack).
    let mut b = d.clone();
    if let Some(l) = b.fill_layers.last_mut() {
        l.depth_m = (l.depth_m + CMP_FILL_DEEPER_M).min(2.4);
    }
    let bl = b.fill_layers.last().map(|l| l.depth_m).unwrap_or(0.0);
    let al = d.fill_layers.last().map(|l| l.depth_m).unwrap_or(0.0);
    out.push(Variant {
        key: "B",
        label: "Deeper fill".into(),
        change: format!("lower layer {al:.2} → {bl:.2} m"),
        out: e.run(&b).map_err(|x| format!("{x:?}")),
        input: b,
    });
    // C: the fan 0.10 faster, inside its own validity band.
    let mut c = d.clone();
    let [lo, hi] = c.fan.allowed_speed_ratio;
    c.speed_ratio = (c.speed_ratio + CMP_FAN_FASTER).clamp(lo, hi);
    let cs = c.speed_ratio;
    out.push(Variant {
        key: "C",
        label: "Faster fan".into(),
        change: format!("fan speed {:.2} → {cs:.2}×", d.speed_ratio),
        out: e.run(&c).map_err(|x| format!("{x:?}")),
        input: c,
    });
    out
}
