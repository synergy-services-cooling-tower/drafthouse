//! `FixtureEngine` - the one shipped [`Engine`] implementation.
//!
//! It does NOT implement the physics. It returns the real engine's recorded run (`assets/fixture.json`,
//! built from `fixtures/engine-run.json` by `tools/build-cockpit-fixture.py`) and re-expresses ONLY the
//! parts the UI can edit as bounded arithmetic on that record:
//!
//! - per-layer KaV/L and pressure: linear interpolation of each fill's sampled characteristic table
//!   (the engine's own sampled points) at the anchor loadings, times depth, times the layer multipliers;
//! - drift pressure: linear interpolation of the drift record's curve at the anchor face velocity;
//! - the fan curve: the fan record's points scaled by the affinity laws at the chosen speed ratio
//!   (the same scaling the engine applied to produce `fanCurveAtSpeed`);
//! - the operating point: the crossing of that fan curve with the recorded system curve;
//! - validation: the fill envelope test the engine itself applies (water loading and dry-air loading
//!   against each fill's `limits`), the fan speed-ratio range, the tower water-flow ceiling.
//!
//! Everything else (Merkel integral, psychrometrics, root solves, capacity) stays the recorded value;
//! headline numbers are rescaled from the anchor only where an edited input moves them monotonically
//! (documented per field in [`FixtureEngine::run`]). The real engine replaces this file without a UI change.

use std::collections::HashMap;

use serde::Deserialize;

use crate::engine::*;

const MMWG_PER_PA: f64 = 1.0 / 9.80665;

// ------------------------------------------------------------------------------- fixture file schema

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureFile {
    pub provenance: FixtureProvenance,
    pub catalog: FixtureCatalog,
    pub default_input: FixtureDefaultInput,
    pub anchor: FixtureAnchor,
    pub refusals: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureProvenance {
    pub engine: String,
    pub generated_by: String,
    pub catalog: CatalogMeta,
    pub anchor_run: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogMeta {
    pub id: String,
    pub revision: String,
    pub status: String,
}

/// The catalog block. Read through [`RawCatalog`] so that every fill record's height spec (issue #136)
/// is required and checked on read: a catalog fill without one is refused by name.
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "RawCatalog")]
pub struct FixtureCatalog {
    pub metadata: CatalogMeta,
    pub water_quality_factors: HashMap<String, QualityFactor>,
    pub towers: Vec<TowerRecord>,
    pub fills: Vec<FixtureFill>,
    pub drift_eliminators: Vec<DriftRecord>,
    pub fans: Vec<FanRecord>,
    pub nozzles: Vec<NozzleRecord>,
    /// Issue #136: each catalog fill's own height spec (`fills[].depth`), by fill id. A fill added
    /// later in a session (a custom record) has none.
    pub fill_depths: HashMap<String, FillDepth>,
}

/// Issue #136: a fill record's own height spec - the layer depths the fill is made in. The allowed
/// depths are the whole multiples of `moduleM` from `minM` to `maxM` (film fill is sold in stacked
/// pack heights, so the module is the natural unit). The tower's `fillDepthOptionsM` is the tower's
/// single-fill total-depth list and says nothing about a layer.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FillDepth {
    pub module_m: f64,
    pub min_m: f64,
    pub max_m: f64,
    /// Where the numbers come from (`illustrative` in the shipped catalog).
    #[serde(default)]
    pub source: String,
}

/// Float slack for "a whole number of modules" and the range ends.
const DEPTH_EPS: f64 = 1e-6;

/// The label a depth refusal starts with: the named reason a layer outside its fill's spec is not run.
pub const FILL_HEIGHT_SPEC: &str = "fill height spec";

impl FillDepth {
    fn on_module(&self, depth_m: f64) -> bool {
        let k = depth_m / self.module_m;
        (k - k.round()).abs() < DEPTH_EPS
    }

    /// The spec is usable: a positive module, a positive minimum at or below the maximum, both ends
    /// whole modules. Refused naming the record otherwise.
    pub fn check(&self, fill_id: &str) -> Result<(), String> {
        let ok = self.module_m > 0.0
            && self.min_m > 0.0
            && self.min_m <= self.max_m
            && self.on_module(self.min_m)
            && self.on_module(self.max_m);
        if ok {
            Ok(())
        } else {
            Err(format!(
                "fill record {fill_id}: height spec moduleM {} / minM {} / maxM {} is not a positive module with both ends whole modules, min at or below max",
                self.module_m, self.min_m, self.max_m
            ))
        }
    }

    /// Is `depth_m` one of this fill's heights?
    pub fn admits(&self, depth_m: f64) -> bool {
        depth_m >= self.min_m - DEPTH_EPS && depth_m <= self.max_m + DEPTH_EPS && self.on_module(depth_m)
    }

    /// Every height this fill is made in, smallest first (rounded to the micrometre, so 3 x 0.15 is 0.45).
    pub fn options(&self) -> Vec<f64> {
        let first = (self.min_m / self.module_m).round() as i64;
        let last = (self.max_m / self.module_m).round() as i64;
        (first..=last)
            .map(|k| (k as f64 * self.module_m * 1e6).round() / 1e6)
            .collect()
    }

    /// The valid height nearest `depth_m` (the smaller one on a tie).
    pub fn nearest(&self, depth_m: f64) -> f64 {
        self.options()
            .into_iter()
            .fold(None::<f64>, |best, o| match best {
                Some(b) if (b - depth_m).abs() <= (o - depth_m).abs() => Some(b),
                _ => Some(o),
            })
            .unwrap_or(self.min_m)
    }

    /// One line: `0.30-2.10 m in 0.15 m modules`.
    pub fn describe(&self) -> String {
        format!("{:.2}-{:.2} m in {:.2} m modules", self.min_m, self.max_m, self.module_m)
    }
}

/// Issue #136: the named refusal for the first layer whose depth is not one of its fill's heights, or
/// `None`. A fill with no spec in `depths` (a session-authored custom fill) is not judged here.
pub fn depth_refusal(depths: &HashMap<String, FillDepth>, layers: &[FillLayer]) -> Option<String> {
    layers.iter().enumerate().find_map(|(i, l)| {
        let spec = depths.get(&l.fill_id)?;
        (!spec.admits(l.depth_m)).then(|| {
            format!(
                "{FILL_HEIGHT_SPEC}: layer {} ({}@{}) is {:.2} m, which is not a {} height ({}) - not run",
                i + 1,
                l.fill_id,
                l.depth_m,
                l.depth_m,
                l.fill_id,
                spec.describe()
            )
        })
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    metadata: CatalogMeta,
    water_quality_factors: HashMap<String, QualityFactor>,
    towers: Vec<TowerRecord>,
    fills: Vec<RawFill>,
    drift_eliminators: Vec<DriftRecord>,
    fans: Vec<FanRecord>,
    nozzles: Vec<NozzleRecord>,
}

#[derive(Deserialize)]
struct RawFill {
    #[serde(default)]
    depth: Option<FillDepth>,
    #[serde(flatten)]
    fill: FixtureFill,
}

impl TryFrom<RawCatalog> for FixtureCatalog {
    type Error = String;

    fn try_from(raw: RawCatalog) -> Result<Self, String> {
        let mut fills = Vec::with_capacity(raw.fills.len());
        let mut fill_depths = HashMap::new();
        for RawFill { depth, fill } in raw.fills {
            let id = fill.record.id.clone();
            let depth = depth.ok_or_else(|| {
                format!(
                    "fill record {id}: no height spec - every catalog fill states the layer depths it is made in as `depth: {{ moduleM, minM, maxM }}`"
                )
            })?;
            depth.check(&id)?;
            fill_depths.insert(id, depth);
            fills.push(fill);
        }
        Ok(Self {
            metadata: raw.metadata,
            water_quality_factors: raw.water_quality_factors,
            towers: raw.towers,
            fills,
            drift_eliminators: raw.drift_eliminators,
            fans: raw.fans,
            nozzles: raw.nozzles,
            fill_depths,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityFactor {
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

/// A fill record plus its sampled characteristic (what the curve editor edits).
#[derive(Clone, Debug, Deserialize)]
pub struct FixtureFill {
    #[serde(flatten)]
    pub record: FillRecord,
    pub characteristic: Characteristic,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Characteristic {
    pub at_dry_air_loading_kg_m2_s: f64,
    pub points: Vec<CharPoint>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CharPoint {
    pub water_loading_kg_m2_s: f64,
    pub kavl_per_m: f64,
    pub pa_per_m: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureDefaultInput {
    pub duty: Duty,
    pub tower_id: String,
    pub fill_layers: Vec<FillLayer>,
    pub drift_id: String,
    pub fan_id: String,
    pub speed_ratio: f64,
    pub nozzle_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureAnchor {
    pub worked: Worked,
    pub warning: String,
    pub system_curve: Vec<SystemPoint>,
    pub sweeps: Sweeps,
    /// The fill the engine actually ran (its sweep series is the performance curve).
    pub anchor_fill_id: String,
    pub zones: Zones,
    pub transfer: Transfer,
    pub water_flow: WaterFlow,
    pub headline: AnchorHeadline,
    pub candidate: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Worked {
    pub title: String,
    pub purpose: String,
    pub steps: Vec<WorkedStep>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPoint {
    pub flow_m3_s: f64,
    pub total_pa: f64,
    pub fill_pa: f64,
    pub drift_pa: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sweeps {
    pub water_mass_flow_kg_s: Sweep,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Sweep {
    pub series: HashMap<String, Vec<SweepPoint>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepPoint {
    pub x: f64,
    pub cold_water_c: Option<f64>,
    pub capability_ratio: Option<f64>,
    #[serde(default)]
    pub infeasible: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zones {
    pub inlet_pa: f64,
    pub support_pa: f64,
    pub distribution_pa: f64,
    pub drift_pa: f64,
    pub plenum_pa: f64,
    pub fan_stack_pa: f64,
    pub fixed_pa: f64,
    pub fill_pa: f64,
    pub total_pa: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub fill_merkel_number: f64,
    pub spray_zone_merkel_number: f64,
    pub rain_zone_merkel_number: f64,
    pub available_merkel_number: f64,
    pub water_loading_kg_m2_s: f64,
    pub dry_air_loading_kg_m2_s: f64,
    /// Moist-air density at the inlet - the engine corrects the fan record's pressure (given at its
    /// reference density) by the ratio.
    pub air_density_kg_m3: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterFlow {
    pub kg_s: f64,
    pub m3_hr: f64,
    /// The density the engine used to convert mass flow to volume flow (at its mean water temperature).
    pub water_density_kg_m3: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AnchorHeadline {
    pub capability_pct: f64,
    pub water_flow_m3_hr: f64,
    pub cold_water_c: f64,
    pub range_c: f64,
    pub approach_c: f64,
    pub airflow_m3_s: f64,
    pub fan_power_kw: f64,
    pub total_pressure_pa: f64,
    pub evaporation_pct: f64,
    pub makeup_m3_hr: f64,
    pub kavl_total: f64,
}

// ------------------------------------------------------------------------------------- helpers

/// Linear interpolation with linear extrapolation beyond the ends (the drift curve behaviour the engine
/// shows in `systemCurve`: 9.74 Pa at 0.98 m/s from the (1.0, 10) - (1.5, 18) segment).
pub fn interp(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let n = xs.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return ys[0];
    }
    let mut i = 0;
    while i + 2 < n && x > xs[i + 1] {
        i += 1;
    }
    let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[i], ys[i + 1]);
    if (x1 - x0).abs() < 1e-12 {
        return y0;
    }
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

/// First crossing of two piecewise-linear curves sampled on a common x-grid (bisection on the difference).
fn crossing(fan: &[XY], system: &[XY]) -> Option<XY> {
    let fx: Vec<f64> = fan.iter().map(|p| p.x).collect();
    let fy: Vec<f64> = fan.iter().map(|p| p.y).collect();
    let sx: Vec<f64> = system.iter().map(|p| p.x).collect();
    let sy: Vec<f64> = system.iter().map(|p| p.y).collect();
    let lo = fx.first()?.max(*sx.first()?);
    let hi = fx.last()?.min(*sx.last()?);
    if hi <= lo {
        return None;
    }
    let d = |x: f64| interp(&fx, &fy, x) - interp(&sx, &sy, x);
    let steps = 400;
    let mut prev_x = lo;
    let mut prev_d = d(lo);
    for k in 1..=steps {
        let x = lo + (hi - lo) * k as f64 / steps as f64;
        let dx = d(x);
        if prev_d == 0.0 {
            return Some(XY { x: prev_x, y: interp(&fx, &fy, prev_x) });
        }
        if prev_d.signum() != dx.signum() {
            let (mut a, mut b) = (prev_x, x);
            for _ in 0..60 {
                let m = 0.5 * (a + b);
                if d(a).signum() == d(m).signum() {
                    a = m;
                } else {
                    b = m;
                }
            }
            let x = 0.5 * (a + b);
            return Some(XY { x, y: interp(&fx, &fy, x) });
        }
        prev_x = x;
        prev_d = dx;
    }
    None
}

// -------------------------------------------------------------------------------------- engine

pub struct FixtureEngine {
    pub fixture: FixtureFile,
}

impl FixtureEngine {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let fixture: FixtureFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self { fixture })
    }

    pub fn fill(&self, id: &str) -> Option<&FixtureFill> {
        self.fixture.catalog.fills.iter().find(|f| f.record.id == id)
    }

    /// The default input: the engine's anchor duty on the mixed stack (upper FILM-MF20 0.45 m over lower
    /// FILM-WF25 0.90 m).
    pub fn default_input(&self) -> EngineInput {
        let d = &self.fixture.default_input;
        let c = &self.fixture.catalog;
        EngineInput {
            duty: d.duty.clone(),
            tower: c.towers.iter().find(|t| t.id == d.tower_id).cloned().expect("default tower"),
            fill_layers: d.fill_layers.clone(),
            drift: c.drift_eliminators.iter().find(|x| x.id == d.drift_id).cloned().expect("default drift"),
            fan: c.fans.iter().find(|x| x.id == d.fan_id).cloned().expect("default fan"),
            speed_ratio: d.speed_ratio,
            nozzle: c.nozzles.iter().find(|x| x.id == d.nozzle_id).cloned().expect("default nozzle"),
        }
    }

    fn provenance(&self) -> Provenance {
        Provenance {
            engine: self.fixture.provenance.engine.clone(),
            catalog_id: self.fixture.catalog.metadata.id.clone(),
            catalog_revision: self.fixture.catalog.metadata.revision.clone(),
            catalog_status: self.fixture.catalog.metadata.status.clone(),
            warning: self.fixture.anchor.warning.clone(),
        }
    }
}

impl Engine for FixtureEngine {
    fn name(&self) -> &str {
        "FixtureEngine (recorded run of the real engine; editable parts interpolated from its tables)"
    }

    /// The curve editor retunes the sampled characteristic tables in place, so an edited fill curve
    /// really moves that layer's KaV/L (and therefore the headline answer).
    fn fixture_catalog_mut(&mut self) -> Option<&mut FixtureFile> {
        Some(&mut self.fixture)
    }

    fn run(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        let a = &self.fixture.anchor;
        let t = a.transfer;
        let z = a.zones;

        // ---- schema-level checks (the real engine refuses these before any physics) ----
        if input.fill_layers.is_empty() {
            return Err(EngineError::Schema("fill stack is empty: at least one fill layer is required".into()));
        }
        for l in &input.fill_layers {
            if !(l.depth_m > 0.0) {
                return Err(EngineError::Schema(format!("fill layer {}: depth_m must be positive, got {}", l.fill_id, l.depth_m)));
            }
        }
        // Issue #136: a layer outside its own fill's height spec is refused, named, and not run.
        if let Some(reason) = depth_refusal(&self.fixture.catalog.fill_depths, &input.fill_layers) {
            return Err(EngineError::Schema(reason));
        }
        for p in &input.fan.curve {
            if !(p.pressure_pa >= 0.0) || !(p.flow_m3_s >= 0.0) {
                return Err(EngineError::Schema(format!("fan record {}: curve points must be non-negative", input.fan.id)));
            }
        }

        // ---- loadings: water loading follows the edited flow and the tower's fill area ----
        let rho = a.water_flow.water_density_kg_m3;
        let water_kg_s = input.duty.water_flow_m3_hr / 3600.0 * rho;
        let water_loading = water_kg_s / input.tower.fill_area_m2;
        let air_loading = t.dry_air_loading_kg_m2_s; // recorded: the air side is the engine's solve

        // ---- validation: name the limit ----
        let mut validation = Vec::new();
        for l in &input.fill_layers {
            let Some(f) = self.fill(&l.fill_id) else {
                return Err(EngineError::Schema(format!("fill record \"{}\" is not in the catalog", l.fill_id)));
            };
            let lim = &f.record.limits;
            if water_loading < lim.min_water_loading_kg_m2_s || water_loading > lim.max_water_loading_kg_m2_s {
                validation.push(Limit {
                    field: format!("fill_layers[{}].fill_id = {}", index_of(input, l), l.fill_id),
                    message: format!(
                        "water loading {:.2} kg/(m2 s) outside {:.1}-{:.1} for {}",
                        water_loading, lim.min_water_loading_kg_m2_s, lim.max_water_loading_kg_m2_s, l.fill_id
                    ),
                    value: water_loading,
                    unit: "kg/(m2 s)".into(),
                    min: Some(lim.min_water_loading_kg_m2_s),
                    max: Some(lim.max_water_loading_kg_m2_s),
                });
            }
            if air_loading < lim.min_dry_air_loading_kg_m2_s || air_loading > lim.max_dry_air_loading_kg_m2_s {
                validation.push(Limit {
                    field: format!("fill_layers[{}].fill_id = {}", index_of(input, l), l.fill_id),
                    message: format!(
                        "dry-air loading {:.2} kg/(m2 s) outside {:.1}-{:.1} for {}",
                        air_loading, lim.min_dry_air_loading_kg_m2_s, lim.max_dry_air_loading_kg_m2_s, l.fill_id
                    ),
                    value: air_loading,
                    unit: "kg/(m2 s)".into(),
                    min: Some(lim.min_dry_air_loading_kg_m2_s),
                    max: Some(lim.max_dry_air_loading_kg_m2_s),
                });
            }
            if input.duty.hot_water_c > lim.max_water_temperature_c {
                validation.push(Limit {
                    field: "duty.hot_water_c".into(),
                    message: format!(
                        "hot water {:.1} C above the {:.0} C limit of {}",
                        input.duty.hot_water_c, lim.max_water_temperature_c, l.fill_id
                    ),
                    value: input.duty.hot_water_c,
                    unit: "C".into(),
                    min: None,
                    max: Some(lim.max_water_temperature_c),
                });
            }
        }
        if input.duty.hot_water_c > input.drift.max_water_temperature_c {
            validation.push(Limit {
                field: "duty.hot_water_c".into(),
                message: format!(
                    "hot water {:.1} C above the {:.0} C limit of drift eliminator {}",
                    input.duty.hot_water_c, input.drift.max_water_temperature_c, input.drift.id
                ),
                value: input.duty.hot_water_c,
                unit: "C".into(),
                min: None,
                max: Some(input.drift.max_water_temperature_c),
            });
        }
        if water_kg_s > input.tower.max_water_mass_flow_kg_s {
            validation.push(Limit {
                field: "duty.water_flow_m3_hr".into(),
                message: format!(
                    "water flow {:.0} kg/s above the {:.0} kg/s ceiling of tower {}",
                    water_kg_s, input.tower.max_water_mass_flow_kg_s, input.tower.id
                ),
                value: water_kg_s,
                unit: "kg/s".into(),
                min: None,
                max: Some(input.tower.max_water_mass_flow_kg_s),
            });
        }
        let [smin, smax] = input.fan.allowed_speed_ratio;
        if input.speed_ratio < smin || input.speed_ratio > smax {
            validation.push(Limit {
                field: "speed_ratio".into(),
                message: format!("fan speed ratio {:.2} outside {:.2}-{:.2} for {}", input.speed_ratio, smin, smax, input.fan.id),
                value: input.speed_ratio,
                unit: "-".into(),
                min: Some(smin),
                max: Some(smax),
            });
        }
        if input.duty.target_cold_water_c <= input.duty.wet_bulb_c {
            validation.push(Limit {
                field: "duty.target_cold_water_c".into(),
                message: format!(
                    "target cold water {:.1} C is not above the wet bulb {:.1} C - the approach must be positive",
                    input.duty.target_cold_water_c, input.duty.wet_bulb_c
                ),
                value: input.duty.target_cold_water_c,
                unit: "C".into(),
                min: Some(input.duty.wet_bulb_c),
                max: None,
            });
        }
        if input.duty.hot_water_c <= input.duty.target_cold_water_c {
            validation.push(Limit {
                field: "duty.hot_water_c".into(),
                message: format!(
                    "hot water {:.1} C is not above the target cold water {:.1} C - the range must be positive",
                    input.duty.hot_water_c, input.duty.target_cold_water_c
                ),
                value: input.duty.hot_water_c,
                unit: "C".into(),
                min: Some(input.duty.target_cold_water_c),
                max: None,
            });
        }

        // ---- per-layer transfer and pressure from the sampled characteristics ----
        let mut layers = Vec::new();
        let mut fill_kavl = 0.0;
        let mut fill_pa = 0.0;
        for (i, l) in input.fill_layers.iter().enumerate() {
            let f = self.fill(&l.fill_id).expect("checked above");
            let xs: Vec<f64> = f.characteristic.points.iter().map(|p| p.water_loading_kg_m2_s).collect();
            let k: Vec<f64> = f.characteristic.points.iter().map(|p| p.kavl_per_m).collect();
            let p: Vec<f64> = f.characteristic.points.iter().map(|p| p.pa_per_m).collect();
            let kavl = interp(&xs, &k, water_loading) * l.depth_m * l.thermal_multiplier;
            let pa = interp(&xs, &p, water_loading) * l.depth_m * l.pressure_multiplier;
            let lim = &f.record.limits;
            fill_kavl += kavl;
            fill_pa += pa;
            layers.push(LayerResult {
                index: i,
                fill_id: l.fill_id.clone(),
                depth_m: l.depth_m,
                kavl,
                pressure_pa: pa,
                cooling_share_pct: 0.0,
                inside_envelope: water_loading >= lim.min_water_loading_kg_m2_s && water_loading <= lim.max_water_loading_kg_m2_s,
            });
        }
        // Spray and rain zones: recorded values, scaled by the water-loading exponent of the tower's own
        // zone correlations would need the record; the fixture keeps them at the anchor (they are 17 % of
        // the total and do not depend on the fill stack).
        let kavl_total = fill_kavl + t.spray_zone_merkel_number + t.rain_zone_merkel_number;
        for l in &mut layers {
            l.cooling_share_pct = 100.0 * l.kavl / kavl_total;
        }

        // ---- drift: the record's curve at the anchor face velocity ----
        let face_velocity = a.headline.airflow_m3_s / input.tower.drift_area_m2;
        let dv: Vec<f64> = input.drift.curve.iter().map(|p| p.face_velocity_m_s).collect();
        let dp: Vec<f64> = input.drift.curve.iter().map(|p| p.pressure_drop_pa).collect();
        let drift_pa = interp(&dv, &dp, face_velocity);

        // ---- pressure by zone (air-path order) ----
        let fixed_terms = z.inlet_pa + z.support_pa + z.distribution_pa + z.plenum_pa + z.fan_stack_pa + z.fixed_pa;
        let total_pa = fixed_terms + fill_pa + drift_pa;
        let mut zones = vec![
            PressureZone { zone: ZoneId::Inlet, layer: None, label: "inlet louvres".into(), pressure_pa: z.inlet_pa, share_pct: 0.0 },
            PressureZone { zone: ZoneId::Rain, layer: None, label: "rain zone + supports".into(), pressure_pa: z.support_pa, share_pct: 0.0 },
        ];
        // Air rises bottom -> top: the LAST layer in the ordered stack is met first.
        for l in layers.iter().rev() {
            zones.push(PressureZone {
                zone: ZoneId::Fill,
                layer: Some(l.index),
                label: format!("fill {} ({} m)", l.fill_id, trim(l.depth_m)),
                pressure_pa: l.pressure_pa,
                share_pct: 0.0,
            });
        }
        zones.push(PressureZone { zone: ZoneId::Spray, layer: None, label: "spray / distribution".into(), pressure_pa: z.distribution_pa, share_pct: 0.0 });
        zones.push(PressureZone { zone: ZoneId::Drift, layer: None, label: format!("drift eliminator {}", input.drift.id), pressure_pa: drift_pa, share_pct: 0.0 });
        zones.push(PressureZone { zone: ZoneId::Plenum, layer: None, label: "plenum".into(), pressure_pa: z.plenum_pa, share_pct: 0.0 });
        zones.push(PressureZone { zone: ZoneId::Stack, layer: None, label: "fan stack".into(), pressure_pa: z.fan_stack_pa, share_pct: 0.0 });
        zones.push(PressureZone { zone: ZoneId::Fixed, layer: None, label: "fixed losses".into(), pressure_pa: z.fixed_pa, share_pct: 0.0 });
        for zn in &mut zones {
            zn.share_pct = 100.0 * zn.pressure_pa / total_pa;
        }

        // ---- fan/system curve: fan record at the speed ratio (affinity laws), recorded system curve
        //      shifted by the change in fill + drift pressure ----
        let r = input.speed_ratio;
        let density_ratio = t.air_density_kg_m3 / input.fan.reference_density_kg_m3;
        let fan_pts: Vec<XY> = input.fan.curve.iter().map(|p| XY { x: p.flow_m3_s * r, y: p.pressure_pa * r * r * density_ratio }).collect();
        let delta = (fill_pa + drift_pa) - (z.fill_pa + z.drift_pa);
        let sys_pts: Vec<XY> = a
            .system_curve
            .iter()
            .map(|p| {
                let scale = if z.fill_pa + z.drift_pa > 0.0 { (p.fill_pa + p.drift_pa) / (z.fill_pa + z.drift_pa) } else { 1.0 };
                XY { x: p.flow_m3_s, y: p.total_pa + delta * scale }
            })
            .collect();
        let op = crossing(&fan_pts, &sys_pts);
        if op.is_none() {
            validation.push(Limit {
                field: "fan.curve".into(),
                message: format!("fan {} curve at speed ratio {:.2} does not cross the system curve - no operating point", input.fan.id, r),
                value: r,
                unit: "-".into(),
                min: None,
                max: None,
            });
        }
        let op = op.unwrap_or(XY { x: a.headline.airflow_m3_s, y: total_pa });
        // Efficiency at the operating point from the fan record; shaft power = Q dp / eta.
        let ex: Vec<f64> = fan_pts.iter().map(|p| p.x).collect();
        let ee: Vec<f64> = input.fan.curve.iter().map(|p| p.efficiency).collect();
        let eta = interp(&ex, &ee, op.x).clamp(0.2, 0.95);
        let fan_power_kw = op.x * op.y / eta / 1000.0;

        // ---- thermal curve: the engine's recorded water-flow sweep (cold water vs flow) ----
        let sweep = a.sweeps.water_mass_flow_kg_s.series.get(&a.anchor_fill_id).cloned().unwrap_or_default();
        let perf: Vec<XY> = sweep
            .iter()
            .filter(|p| !p.infeasible)
            .filter_map(|p| p.cold_water_c.map(|c| XY { x: p.x / rho * 3600.0, y: c }))
            .collect();
        let px: Vec<f64> = perf.iter().map(|p| p.x).collect();
        let py: Vec<f64> = perf.iter().map(|p| p.y).collect();
        let (xmin, xmax) = (px.first().copied().unwrap_or(0.0), px.last().copied().unwrap_or(1.0));

        // ---- headline: the recorded run, moved only by the edited inputs ----
        // Cold water: recorded sweep at the edited flow, shifted by the transfer the edited stack adds or
        // removes (dt/d(KaV/L) is taken from the recorded fill-depth sweep: 1.5 m -> 1.8 m moved cold
        // water by -0.333 C for +0.2653 KaV/L, i.e. -1.255 C per unit KaV/L).
        const DT_PER_KAVL: f64 = -1.2553;
        let cold_from_flow = if perf.len() >= 2 { interp(&px, &py, input.duty.water_flow_m3_hr) } else { a.headline.cold_water_c };
        let cold_water_c = cold_from_flow + DT_PER_KAVL * (kavl_total - t.available_merkel_number)
            + (input.duty.wet_bulb_c - 27.0) * 0.55
            + (input.duty.hot_water_c - 42.0) * 0.12;
        let range_c = input.duty.hot_water_c - cold_water_c;
        let approach_c = cold_water_c - input.duty.wet_bulb_c;
        let capability_pct = a.headline.capability_pct
            * (kavl_total / t.available_merkel_number)
            * (a.headline.water_flow_m3_hr / input.duty.water_flow_m3_hr).powf(0.35);
        let evaporation_pct = a.headline.evaporation_pct * (range_c / a.headline.range_c);
        let n = input.duty.cycles_of_concentration.max(1.01);
        let evaporation_m3_hr = input.duty.water_flow_m3_hr * evaporation_pct / 100.0;
        let makeup_m3_hr = evaporation_m3_hr * n / (n - 1.0) + a.headline.makeup_m3_hr * 0.0005;

        let thermal_curve = ThermalCurve {
            x_label: "water flow, m³/h".into(),
            y_label: "cold water, °C".into(),
            performance: Series { label: "performance - cold water the tower reaches".into(), points: perf },
            demand: Series {
                label: "demand - target cold water".into(),
                points: vec![XY { x: xmin, y: input.duty.target_cold_water_c }, XY { x: xmax, y: input.duty.target_cold_water_c }],
            },
            operating_point: XY { x: input.duty.water_flow_m3_hr, y: cold_water_c },
        };
        let fan_system_curve = FanSystemCurve {
            x_label: "air flow, m³/s".into(),
            y_label: "pressure, Pa".into(),
            fan: Series { label: format!("fan {} at speed ratio {:.2}", input.fan.id, r), points: fan_pts },
            system: Series { label: "system - tower resistance".into(), points: sys_pts },
            operating_point: op,
        };

        // ---- worked steps: the engine's sheet, with the stack-dependent lines re-substituted ----
        let mut steps = a.worked.steps.clone();
        for s in &mut steps {
            match s.label.as_str() {
                "Fill transfer demand" => {
                    s.formula = Some("KaV/L_fill = sum over layers of (KaV/L per m at L, G) * depth * f_quality".into());
                    s.substitution = Some(
                        layers.iter().map(|l| format!("{} {} m -> {:.4}", l.fill_id, trim(l.depth_m), l.kavl)).collect::<Vec<_>>().join(" + "),
                    );
                    s.value = Some(fill_kavl);
                }
                "Total demand" | "Available transfer" => {
                    s.substitution = Some(format!("{:.4} + {:.4} + {:.4}", fill_kavl, t.spray_zone_merkel_number, t.rain_zone_merkel_number));
                    s.value = Some(kavl_total);
                }
                "Air-side pressure demand" => {
                    s.substitution = Some(format!(
                        "{:.4} + {:.4} + {:.4} + {:.4} + {:.4} + {:.4} + {:.4} + {:.4}",
                        fill_pa, drift_pa, z.inlet_pa, z.distribution_pa, z.support_pa, z.plenum_pa, z.fan_stack_pa, z.fixed_pa
                    ));
                    s.value = Some(total_pa);
                }
                "Fan operating point" => {
                    s.substitution = Some(format!("fan {:.4} Pa vs system {:.4} Pa at {:.4} m3/s", op.y, op.y, op.x));
                    s.value = Some(op.x);
                }
                "Fan shaft power" => {
                    s.substitution = Some(format!("{:.4} * {:.4} / {:.4}", op.x, op.y, eta));
                    s.value = Some(fan_power_kw);
                }
                "Cold-water temperature, solved" => {
                    s.substitution = Some(format!("target {:.4}, margin {:.4}", input.duty.target_cold_water_c, input.duty.target_cold_water_c - cold_water_c));
                    s.value = Some(cold_water_c);
                }
                "Capability ratio" => s.value = Some(capability_pct / 100.0),
                "Range" => {
                    s.substitution = Some(format!("{:.4} - {:.4}", input.duty.hot_water_c, input.duty.target_cold_water_c));
                    s.value = Some(input.duty.hot_water_c - input.duty.target_cold_water_c);
                }
                "Duty water flow" => {
                    s.substitution = Some(format!("{:.4} kg/s (given)", water_kg_s));
                    s.value = Some(water_kg_s);
                }
                _ => {}
            }
        }
        steps.insert(
            0,
            WorkedStep {
                label: "Fixture engine".into(),
                why: "This baseline runs the recorded result of the real engine; only the fill stack, drift, fan and duty edits are re-expressed by interpolating the engine's own sampled tables. The real engine replaces this step.".into(),
                formula: None,
                substitution: None,
                value: None,
                unit: String::new(),
                reference: Some(self.fixture.provenance.engine.clone()),
                kind: "note".into(),
            },
        );

        Ok(EngineOutput {
            capability_pct,
            water_flow_m3_hr: input.duty.water_flow_m3_hr,
            cold_water_c,
            range_c,
            approach_c,
            airflow_m3_s: op.x,
            fan_power_kw,
            total_pressure_pa: total_pa,
            total_pressure_mmwg: total_pa * MMWG_PER_PA,
            evaporation_pct,
            makeup_m3_hr,
            kavl_total,
            kavl_per_layer: layers,
            pressure_by_zone: zones,
            worked_steps: steps,
            thermal_curve,
            fan_system_curve,
            validation,
            provenance: self.provenance(),
        })
    }
}

fn index_of(input: &EngineInput, l: &FillLayer) -> usize {
    input.fill_layers.iter().position(|x| std::ptr::eq(x, l)).unwrap_or(0)
}

pub fn trim(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Issue #58: the layout moved this crate to `cockpit/contract/`, so the test reads the
    // application's own fixture asset (the same file the UI ships) - the one path adaptation in
    // this otherwise byte-identical copy of the baseline's file.
    const FIXTURE: &str = include_str!("../../assets/fixture.json");

    #[test]
    fn default_run_reproduces_the_anchor_headline() {
        let eng = FixtureEngine::from_json(FIXTURE).unwrap();
        let out = eng.run(&eng.default_input()).unwrap();
        let h = eng.fixture.anchor.headline;
        assert!(out.validation.is_empty(), "{:?}", out.validation);
        assert!((out.total_pressure_pa - h.total_pressure_pa).abs() < 0.2, "{} vs {}", out.total_pressure_pa, h.total_pressure_pa);
        assert!((out.kavl_total - h.kavl_total).abs() < 1e-3, "{} vs {}", out.kavl_total, h.kavl_total);
        assert!((out.airflow_m3_s - h.airflow_m3_s).abs() < 0.5, "{} vs {}", out.airflow_m3_s, h.airflow_m3_s);
        assert!((out.fan_power_kw - h.fan_power_kw).abs() < 0.3, "{} vs {}", out.fan_power_kw, h.fan_power_kw);
        assert!((out.cold_water_c - h.cold_water_c).abs() < 0.05, "{} vs {}", out.cold_water_c, h.cold_water_c);
        assert!((out.capability_pct - h.capability_pct).abs() < 0.2);
        assert_eq!(out.kavl_per_layer.len(), 2);
        assert_eq!(out.kavl_per_layer[0].fill_id, "FILM-MF20");
        assert_eq!(out.kavl_per_layer[1].fill_id, "FILM-WF25");
    }

    #[test]
    fn envelope_refusal_names_the_limit() {
        let eng = FixtureEngine::from_json(FIXTURE).unwrap();
        let mut input = eng.default_input();
        // The engine's own recorded refusal: fillAreaM2 400 -> water loading 0.50 outside 1.1-6.2.
        input.tower.fill_area_m2 = 400.0;
        input.fill_layers = vec![FillLayer { fill_id: "FILM-VF38".into(), depth_m: 1.5, thermal_multiplier: 0.92, pressure_multiplier: 1.12 }];
        let out = eng.run(&input).unwrap();
        assert!(out.validation.iter().any(|l| l.message.starts_with("water loading 0.50 kg/(m2 s) outside 1.1-6.2")), "{:?}", out.validation);
    }

    /// Every shipped catalog copy, by the path the tree carries it at.
    const COPIES: [(&str, &str); 3] = [
        ("cockpit/assets/fixture.json", FIXTURE),
        (
            "rust/cockpit-adapter/tests/data/cockpit-fixture.json",
            include_str!("../../../rust/cockpit-adapter/tests/data/cockpit-fixture.json"),
        ),
        (
            "cockpit/assets/revision-illustrative-catalog-v0.1.json",
            include_str!("../../assets/revision-illustrative-catalog-v0.1.json"),
        ),
    ];

    /// A copy's catalog block (`catalog` in a fixture, `records` in a revision), read through the schema.
    fn catalog_of(text: &str) -> Result<FixtureCatalog, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let block = v.get("catalog").or_else(|| v.get("records")).cloned().ok_or("no catalog block")?;
        serde_json::from_value(block).map_err(|e| e.to_string())
    }

    /// Issue #136 schema: every fill record in every shipped catalog states its own height spec, all
    /// copies state the same one, and each is usable (positive module, whole-module ends).
    #[test]
    fn every_shipped_fill_record_has_a_height_spec() {
        let mut seen: Option<HashMap<String, FillDepth>> = None;
        for (path, text) in COPIES {
            let cat = catalog_of(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(cat.fills.len(), 7, "{path}: the 7 illustrative fills");
            for f in &cat.fills {
                let spec = cat.fill_depths.get(&f.record.id).unwrap_or_else(|| panic!("{path}: {} has no height spec", f.record.id));
                spec.check(&f.record.id).unwrap_or_else(|e| panic!("{path}: {e}"));
                assert_eq!(spec.source, "illustrative", "{path}: {} - the spec says where it comes from", f.record.id);
            }
            match &seen {
                None => seen = Some(cat.fill_depths),
                Some(first) => assert_eq!(first, &cat.fill_depths, "{path}: the copies' height specs are in step"),
            }
        }
    }

    /// Issue #136 schema: a catalog fill with no height spec is refused, naming the record - for
    /// each fill of each shipped copy, so no record can lose its spec quietly.
    #[test]
    fn a_fill_without_a_height_spec_is_refused_by_name() {
        for (path, text) in COPIES {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let key = if v.get("catalog").is_some() { "catalog" } else { "records" };
            for i in 0..7 {
                let mut block = v[key].clone();
                let fill = block["fills"][i].as_object_mut().unwrap();
                let id = fill["id"].as_str().unwrap().to_string();
                assert!(fill.remove("depth").is_some(), "{path}: {id} ships a `depth`");
                let err = serde_json::from_value::<FixtureCatalog>(block).expect_err("refused");
                assert!(err.to_string().contains(&format!("fill record {id}: no height spec")), "{path}: {err}");
            }
        }
        // And a spec that is not one (ends off the module, min above max) is refused too.
        let v: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        for bad in [
            serde_json::json!({"moduleM": 0.15, "minM": 0.4, "maxM": 2.1}),
            serde_json::json!({"moduleM": 0.15, "minM": 2.1, "maxM": 0.3}),
            serde_json::json!({"moduleM": 0.0, "minM": 0.3, "maxM": 2.1}),
        ] {
            let mut block = v["catalog"].clone();
            block["fills"][0]["depth"] = bad;
            let err = serde_json::from_value::<FixtureCatalog>(block).expect_err("refused");
            assert!(err.to_string().contains("fill record FILM-CF19: height spec"), "{err}");
        }
    }

    /// The default stack's own fills list their own heights: FILM-MF20 offers 0.45 m, FILM-WF25 0.90 m.
    #[test]
    fn the_default_stack_s_depths_are_their_fills_own_options() {
        let eng = FixtureEngine::from_json(FIXTURE).unwrap();
        let depths = &eng.fixture.catalog.fill_depths;
        for l in &eng.default_input().fill_layers {
            let spec = &depths[&l.fill_id];
            assert!(spec.options().contains(&l.depth_m), "{} {} in {:?}", l.fill_id, l.depth_m, spec.options());
        }
        assert!(depths["FILM-MF20"].options().contains(&0.45));
        assert!(depths["FILM-WF25"].options().contains(&0.9));
        assert_eq!(depths["FILM-MF20"].nearest(0.5), 0.45);
        assert_eq!(depths["FILM-VF38"].nearest(0.45), 0.6, "below the range snaps to its minimum");
    }

    /// Issue #136, AC 4 (recorded replay): a layer outside its own fill's height spec is refused with the
    /// named reason before anything runs; the default stack (0.45 / 0.90) and on-spec swaps still run.
    #[test]
    fn a_layer_outside_its_fill_s_spec_is_refused_by_name() {
        use crate::engine::Engine;
        let eng = FixtureEngine::from_json(FIXTURE).unwrap();
        let ok = eng.default_input();
        assert!(eng.run(&ok).is_ok(), "the default stack runs");
        for (layer, depth, want) in [
            (0, 0.5, "fill height spec: layer 1 (FILM-MF20@0.5) is 0.50 m, which is not a FILM-MF20 height (0.30-2.10 m in 0.15 m modules) - not run"),
            (1, 2.25, "fill height spec: layer 2 (FILM-WF25@2.25) is 2.25 m, which is not a FILM-WF25 height (0.30-2.10 m in 0.15 m modules) - not run"),
            (0, 0.15, "fill height spec: layer 1 (FILM-MF20@0.15) is 0.15 m, which is not a FILM-MF20 height (0.30-2.10 m in 0.15 m modules) - not run"),
        ] {
            let mut bad = ok.clone();
            bad.fill_layers[layer].depth_m = depth;
            match eng.run(&bad) {
                Err(EngineError::Schema(m)) => assert_eq!(m, want),
                other => panic!("{depth} m on layer {layer}: expected the named refusal, got {other:?}"),
            }
        }
    }

    /// Hand-checked option lists for the three shipped spec shapes, written out by hand (not derived):
    /// film 0.15 m modules 0.30-2.10 (13 heights), VF38/trickle 0.30 m 0.60-2.40 (7), splash 0.30 m
    /// 0.90-3.00 (8); and `nearest` on the in-between and out-of-range cases the UI snap meets.
    #[test]
    fn the_shipped_height_lists_by_hand() {
        let eng = FixtureEngine::from_json(FIXTURE).unwrap();
        let d = &eng.fixture.catalog.fill_depths;
        let film = [0.3, 0.45, 0.6, 0.75, 0.9, 1.05, 1.2, 1.35, 1.5, 1.65, 1.8, 1.95, 2.1];
        for id in ["FILM-CF19", "FILM-OF25", "FILM-MF20", "FILM-WF25"] {
            assert_eq!(d[id].options(), film, "{id}");
        }
        for id in ["FILM-VF38", "TRICKLE-50"] {
            assert_eq!(d[id].options(), [0.6, 0.9, 1.2, 1.5, 1.8, 2.1, 2.4], "{id}");
        }
        assert_eq!(d["SPLASH-GRID"].options(), [0.9, 1.2, 1.5, 1.8, 2.1, 2.4, 2.7, 3.0]);
        // nearest: inside (0.5 -> 0.45), a tie (0.75 between 0.6 and 0.9 -> the smaller), below and
        // above the range (-> its ends).
        assert_eq!(d["FILM-MF20"].nearest(0.5), 0.45);
        assert_eq!(d["FILM-VF38"].nearest(0.75), 0.6);
        assert_eq!(d["SPLASH-GRID"].nearest(0.45), 0.9);
        assert_eq!(d["FILM-WF25"].nearest(3.0), 2.1);
        assert!(d["FILM-MF20"].admits(0.45) && d["FILM-WF25"].admits(0.9));
        assert!(!d["FILM-MF20"].admits(0.5) && !d["SPLASH-GRID"].admits(0.6));
    }

    #[test]
    fn interp_extrapolates_linearly() {
        assert!((interp(&[1.0, 1.5], &[10.0, 18.0], 60.0 / 61.0) - 9.7377).abs() < 1e-3);
    }
}
