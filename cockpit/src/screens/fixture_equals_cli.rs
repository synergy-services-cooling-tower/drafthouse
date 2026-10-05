//! The load-bearing test of issue #83: for each of the four workspaces, the **same input** takes
//! the UI's engine path and the engine's own native CLI path, and the numbers must be equal.
//!
//! The UI path is the screen's real one - [`data::Cache`] stepped to completion (the same lazy
//! `run_selection`/`RealEngine::run` calls the wasm app makes), or the same `rust/src` function
//! the screen calls. The CLI path is `rust/src/cli.rs`'s [`eng::cli::run`], the function
//! `rust/src/bin/ct-engine.rs` is a shell over, driven with the fixture's own records spelled as
//! `select`/`capability`/`curve-*`/`drift`/`balance` arguments.
//!
//! Equality is exact (`f64` bits). Both paths run the same engine functions on the same records,
//! and both the argument decimals going in and the answer coming back are read exactly: the
//! arguments are Rust's shortest round-trip formatting, and the answers are read out of the bytes
//! the CLI printed by [`Cli`]'s walker (`serde_json`'s default float path can move a decimal by an
//! ulp, and this crate's dependencies are fenced, so the printed bytes - parsed by Rust's own
//! correctly-rounded parser - are the authority). A difference is a real difference, not a
//! tolerance to widen. Each test names what it pins; the module also pins the two design-round
//! defects the issue names.
//!
//! One spelling difference is deliberate and documented: the cockpit's glue converts the water
//! volume flow with `m3/hr * rho / 3600` ([`data::water_kg_s`]), the adapter's `run` with
//! `m3/hr / 3600 * rho`; the two differ in the last ulp, so each path is driven with its own
//! spelling, and the one place the two numbers meet (the water workspace's evaporation) is pinned
//! to the ulp it actually is.

use std::collections::BTreeMap;

use cockpit::engine::{Engine, EngineInput, EngineOutput};
use cockpit::fixture_engine::FixtureEngine;
use cockpit_adapter::synergy_drafthouse as eng;
use serde_json::{json, Value};

use super::data;
use crate::adapter_bridge::AdapterEngine;

const FIXTURE: &str = include_str!("../../assets/fixture.json");

// ------------------------------------------------------------------- the native CLI, in-process

/// One step into the CLI's JSON: an object key or an array index.
#[derive(Clone, Copy, Debug)]
pub(super) enum Step<'a> {
    Key(&'a str),
    Index(usize),
}
use Step::{Index, Key};

/// The CLI's answer, kept as the bytes it printed and read path by path. Numbers are handed back
/// as the exact text the CLI wrote (parsed by Rust's own round-trip parser) - see the module doc.
pub(super) struct Cli {
    raw: String,
}

impl Cli {
    fn skip_ws(&self, mut at: usize) -> usize {
        let bytes = self.raw.as_bytes();
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        at
    }

    /// One past the closing quote of the string that opens at `at`.
    fn string_end(&self, at: usize) -> usize {
        let bytes = self.raw.as_bytes();
        assert_eq!(bytes[at], b'"', "a string at {at}");
        let mut i = at + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => return i + 1,
                _ => i += 1,
            }
        }
        panic!("the CLI's JSON has an unterminated string at {at}");
    }

    /// The string's contents. The CLI's own writer escapes nothing this walker needs, so an
    /// escape is asserted away rather than silently unescaped.
    fn string(&self, at: usize) -> &str {
        let token = &self.raw[at..self.string_end(at)];
        assert!(
            !token.contains('\\'),
            "an escaped string the walker does not unescape: {token}"
        );
        &token[1..token.len() - 1]
    }

    /// One past the value that starts at `at`.
    fn value_end(&self, at: usize) -> usize {
        let bytes = self.raw.as_bytes();
        match bytes[at] {
            b'"' => self.string_end(at),
            open @ (b'{' | b'[') => {
                let close = if open == b'{' { b'}' } else { b']' };
                let mut depth = 0usize;
                let mut i = at;
                while i < bytes.len() {
                    match bytes[i] {
                        b'"' => {
                            i = self.string_end(i);
                            continue;
                        }
                        byte if byte == open => depth += 1,
                        byte if byte == close => {
                            depth -= 1;
                            if depth == 0 {
                                return i + 1;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                panic!("the CLI's JSON is unbalanced at {at}");
            }
            _ => {
                let mut i = at;
                while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b']') {
                    i += 1;
                }
                let mut end = i;
                while end > at && bytes[end - 1].is_ascii_whitespace() {
                    end -= 1;
                }
                end
            }
        }
    }

    /// An object's `(key, value start)` pairs, in the order the CLI wrote them.
    fn members(&self, at: usize) -> Vec<(&str, usize)> {
        let bytes = self.raw.as_bytes();
        assert_eq!(bytes[at], b'{', "an object at {at}");
        let mut out = Vec::new();
        let mut i = self.skip_ws(at + 1);
        if bytes[i] == b'}' {
            return out;
        }
        loop {
            let key = self.string(i);
            i = self.skip_ws(self.string_end(i));
            assert_eq!(bytes[i], b':', "a colon at {i}");
            i = self.skip_ws(i + 1);
            out.push((key, i));
            i = self.skip_ws(self.value_end(i));
            match bytes[i] {
                b',' => i = self.skip_ws(i + 1),
                b'}' => return out,
                other => panic!("unexpected {:?} in an object at {i}", other as char),
            }
        }
    }

    /// An array's value starts, in order.
    fn items(&self, at: usize) -> Vec<usize> {
        let bytes = self.raw.as_bytes();
        assert_eq!(bytes[at], b'[', "an array at {at}");
        let mut out = Vec::new();
        let mut i = self.skip_ws(at + 1);
        if bytes[i] == b']' {
            return out;
        }
        loop {
            out.push(i);
            i = self.skip_ws(self.value_end(i));
            match bytes[i] {
                b',' => i = self.skip_ws(i + 1),
                b']' => return out,
                other => panic!("unexpected {:?} in an array at {i}", other as char),
            }
        }
    }

    fn reach(&self, path: &[Step]) -> usize {
        let mut at = self.skip_ws(0);
        for step in path {
            at = match *step {
                Key(key) => self
                    .members(at)
                    .into_iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| value)
                    .unwrap_or_else(|| panic!("the CLI's answer has no {key:?} on {path:?}")),
                Index(index) => *self
                    .items(at)
                    .get(index)
                    .unwrap_or_else(|| panic!("the CLI's answer has no [{index}] on {path:?}")),
            };
        }
        at
    }

    /// The value at `path`, as the CLI printed it.
    pub(super) fn raw_at(&self, path: &[Step]) -> &str {
        let at = self.reach(path);
        &self.raw[at..self.value_end(at)]
    }

    pub(super) fn number(&self, path: &[Step]) -> f64 {
        let token = self.raw_at(path);
        token
            .parse()
            .unwrap_or_else(|_| panic!("the CLI printed {token:?} as a number on {path:?}"))
    }

    fn text(&self, path: &[Step]) -> &str {
        self.string(self.reach(path))
    }

    fn flag(&self, path: &[Step]) -> bool {
        self.raw_at(path) == "true"
    }

    fn array_len(&self, path: &[Step]) -> usize {
        self.items(self.reach(path)).len()
    }

    /// An object's entries, each value as the CLI printed it.
    fn entries(&self, path: &[Step]) -> Vec<(&str, &str)> {
        let at = self.reach(path);
        self.members(at)
            .into_iter()
            .map(|(key, value)| (key, &self.raw[value..self.value_end(value)]))
            .collect()
    }
}

/// `cli::run` - the function `rust/src/bin/ct-engine.rs` is a shell over: the shipped binary's own
/// path, called in-process. A refusal or a non-JSON answer is the test's failure, named.
pub(super) fn cli(args: &[String]) -> Cli {
    cli_opt(args).unwrap_or_else(|| panic!("the CLI refused {args:?}"))
}

/// The same, for the one place a refusal is a legitimate answer (a curve run the iteration expects
/// to be able to retry).
fn cli_opt(args: &[String]) -> Option<Cli> {
    let raw = eng::cli::run(args)
        .map_err(|failure| {
            // The replay treats a refusal as data; print it so a genuine usage error is visible.
            eprintln!("cli::run refused {args:?}: {failure:?}");
        })
        .ok()?;
    let cli = Cli { raw };
    // the answer's outside shape must at least be JSON; the exact reading is the walker's
    serde_json::from_str::<Value>(&cli.raw)
        .unwrap_or_else(|error| panic!("the CLI printed no JSON for {args:?}: {error}"));
    Some(cli)
}

// ------------------------------------------------------------------------- the fixture's records

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("the fixture parses")
}

fn records<'a>(fx: &'a Value, list: &str) -> &'a Vec<Value> {
    fx["catalog"][list]
        .as_array()
        .unwrap_or_else(|| panic!("the fixture carries catalog.{list}"))
}

fn record<'a>(fx: &'a Value, list: &str, id: &str) -> &'a Value {
    records(fx, list)
        .iter()
        .find(|r| r["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("catalog.{list} carries {id}"))
}

/// The number as the CLI reads it back: Rust's shortest round-trip decimal.
pub(super) fn n(v: f64) -> String {
    format!("{v}")
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Number(x) => n(x.as_f64().unwrap_or_else(|| panic!("{x} is not an f64"))),
        Value::String(s) => s.clone(),
        other => panic!("a scalar was expected, got {other}"),
    }
}

/// The engine's field order of the five-number correlations (`select-fields`' blocks): the zone
/// fields, the fill's `thermal`/`pressure` blocks, its `limits`, the two curve families.
const ZONE_ORDER: [&str; 6] = [
    "coefficientPerM",
    "coefficientPaPerM",
    "referenceWaterLoadingKgM2S",
    "referenceDryAirLoadingKgM2S",
    "waterExponent",
    "airExponent",
];
const LIMITS_ORDER: [&str; 5] = [
    "minWaterLoadingKgM2S",
    "maxWaterLoadingKgM2S",
    "minDryAirLoadingKgM2S",
    "maxDryAirLoadingKgM2S",
    "maxWaterTemperatureC",
];
const FAN_CURVE_ORDER: [&str; 3] = ["flowM3S", "pressurePa", "efficiency"];
const DRIFT_CURVE_ORDER: [&str; 3] = ["faceVelocityMS", "driftPpm", "pressureDropPa"];

/// A block of correlated numbers: the engine's `|`-joined spelling, in the field order its
/// validator states.
fn block(v: &Value) -> String {
    let order: &[&str] = if v.get("minWaterLoadingKgM2S").is_some() {
        &LIMITS_ORDER[..]
    } else {
        &ZONE_ORDER[..]
    };
    order
        .iter()
        .filter_map(|key| v.get(*key))
        .map(scalar)
        .collect::<Vec<_>>()
        .join("|")
}

/// One curve point: the engine's `x:y:z` spelling.
fn point(v: &Value) -> String {
    let order: &[&str] = if v.get("faceVelocityMS").is_some() {
        &DRIFT_CURVE_ORDER[..]
    } else {
        &FAN_CURVE_ORDER[..]
    };
    order
        .iter()
        .filter_map(|key| v.get(*key))
        .map(scalar)
        .collect::<Vec<_>>()
        .join(":")
}

/// One `select` record: `key:value` pairs joined with commas, only the fields `select-fields`
/// accepts (the fixture carries the design pass's own extras, which the CLI's schema does not
/// declare).
fn spec(record: &Value, accepted: &[String]) -> String {
    let fields = record
        .as_object()
        .unwrap_or_else(|| panic!("a record object was expected, got {record}"));
    let mut parts: Vec<String> = Vec::new();
    for key in accepted {
        let Some(value) = fields.get(key) else {
            continue;
        };
        let spelled = match value {
            Value::Array(items) => {
                if items.is_empty() {
                    continue;
                }
                items
                    .iter()
                    .map(|item| match item {
                        Value::Object(_) => point(item),
                        _ => scalar(item),
                    })
                    .collect::<Vec<_>>()
                    .join("|")
            }
            Value::Object(_) => block(value),
            _ => scalar(value),
        };
        parts.push(format!("{key}:{spelled}"));
    }
    parts.join(",")
}

/// `select-fields`, the CLI's own record-field contract.
struct Schema(Value);

impl Schema {
    fn new() -> Self {
        // Field names only - and the structure check `cli_opt` already runs.
        let answer = cli(&["select-fields".to_string()]);
        Self(serde_json::from_str(&answer.raw).expect("select-fields prints JSON"))
    }

    fn accepted(&self, list: &str) -> Vec<String> {
        self.0[list]
            .as_array()
            .unwrap_or_else(|| panic!("select-fields carries {list}"))
            .iter()
            .map(|v| v.as_str().expect("a field name").to_string())
            .collect()
    }

    fn spec(&self, list: &str, record: &Value) -> String {
        spec(record, &self.accepted(list))
    }
}

/// The fixture's quality factors as the CLI's `name:thermal:pressure;...` list.
fn quality_factors(fx: &Value) -> String {
    fx["catalog"]["waterQualityFactors"]
        .as_object()
        .expect("the fixture carries quality factors")
        .iter()
        .map(|(name, pair)| {
            format!(
                "{name}:{}:{}",
                scalar(&pair["thermalMultiplier"]),
                scalar(&pair["pressureMultiplier"])
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The `select` duty arguments, from the draft's own duty.
fn duty_args(draft: &EngineInput) -> Vec<String> {
    let d = &draft.duty;
    vec![
        "--hot".into(),
        n(d.hot_water_c),
        "--target-cold".into(),
        n(d.target_cold_water_c),
        "--wb".into(),
        n(d.wet_bulb_c),
        "--db".into(),
        n(d.dry_bulb_c),
        "--p".into(),
        n(d.pressure_pa),
        "--salinity".into(),
        n(d.salinity_g_kg),
        "--quality-class".into(),
        d.water_quality_class.clone(),
        "--cycles".into(),
        n(d.cycles_of_concentration),
    ]
}

/// The draft's declared stack in the CLI's spelling (`<fillId>@<depthM>[@<t>[@<p>]]`, layers
/// joined with `+`, top first). The multipliers are relative to the run-level pair - the adapter's
/// `own_multipliers` - so a stack whose layers share one pair spells without multipliers.
fn stack_spelling(draft: &EngineInput) -> String {
    let (run_t, run_p) = draft
        .fill_layers
        .first()
        .map(|layer| (layer.thermal_multiplier, layer.pressure_multiplier))
        .expect("the draft declares a stack");
    draft
        .fill_layers
        .iter()
        .map(|layer| {
            let thermal = layer.thermal_multiplier / run_t;
            let pressure = layer.pressure_multiplier / run_p;
            let mut spelled = format!("{}@{}", layer.fill_id, n(layer.depth_m));
            if thermal != 1.0 || pressure != 1.0 {
                spelled.push_str(&format!("@{thermal}@{pressure}"));
            }
            spelled
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The water mass flow the adapter's `run` computes: `m3/hr / 3600 * rho(mean, salinity)`. The
/// multiplication order matters (see the module doc); the per-tower selection runs take the
/// cockpit's own [`data::water_kg_s`] instead.
fn adapter_water(draft: &EngineInput) -> f64 {
    let d = &draft.duty;
    let mean = (d.hot_water_c + d.target_cold_water_c) / 2.0;
    let rho = eng::water_density_kg_m3(mean, d.salinity_g_kg).expect("the engine's density");
    d.water_flow_m3_hr / 3600.0 * rho
}

/// The draft's own configuration as `select` arguments: the input's tower (with the declared
/// stack), the input's fills, fan, eliminator and nozzle, the run's own quality pair - what
/// `RealEngine::run` builds in `run_catalog`, so the CLI's answer is the answer the app's engine
/// path gives. The duty can be overridden the way the screens override it.
#[allow(clippy::too_many_arguments)]
fn draft_mirror_args(
    fx: &Value,
    schema: &Schema,
    draft: &EngineInput,
    water: f64,
    hot: f64,
    target: f64,
    wb: f64,
    db: f64,
) -> Vec<String> {
    let mut tower = record(fx, "towers", &draft.tower.id).clone();
    tower["fillStacks"] = json!([stack_spelling(draft)]);
    let mut args: Vec<String> = vec!["select".into()];
    args.extend(["--towers".into(), schema.spec("towers", &tower)]);
    args.extend([
        "--fills".into(),
        records(fx, "fills")
            .iter()
            .map(|r| schema.spec("fills", r))
            .collect::<Vec<_>>()
            .join(";"),
    ]);
    args.extend([
        "--drift-eliminators".into(),
        schema.spec(
            "driftEliminators",
            record(fx, "driftEliminators", &draft.drift.id),
        ),
    ]);
    args.extend([
        "--fans".into(),
        schema.spec("fans", record(fx, "fans", &draft.fan.id)),
    ]);
    args.extend([
        "--nozzles".into(),
        schema.spec("nozzles", record(fx, "nozzles", &draft.nozzle.id)),
    ]);
    let (run_t, run_p) = draft
        .fill_layers
        .first()
        .map(|layer| (layer.thermal_multiplier, layer.pressure_multiplier))
        .expect("the draft declares a stack");
    args.extend([
        "--quality-factors".into(),
        format!(
            "{}:{}:{}",
            draft.duty.water_quality_class,
            n(run_t),
            n(run_p)
        ),
    ]);
    args.extend(["--water".into(), n(water)]);
    args.extend([
        "--hot".into(),
        n(hot),
        "--target-cold".into(),
        n(target),
        "--wb".into(),
        n(wb),
        "--db".into(),
        n(db),
        "--p".into(),
        n(draft.duty.pressure_pa),
        "--salinity".into(),
        n(draft.duty.salinity_g_kg),
        "--quality-class".into(),
        draft.duty.water_quality_class.clone(),
        "--cycles".into(),
        n(draft.duty.cycles_of_concentration),
    ]);
    // The adapter's run: no extra thermal margin, exactly the input's one speed ratio.
    args.extend([
        "--min-margin".into(),
        "0".into(),
        "--speed-ratios".into(),
        n(draft.speed_ratio),
        "--all-orders".into(),
        "--max-results".into(),
        "100000".into(),
    ]);
    args
}

/// One tower as the screen gives `run_selection`: the whole fixture catalog minus the other
/// towers, the screen's requirements (its own defaults - no `--min-margin`/`--speed-ratios`
/// override), no declared stack: the engine enumerates the tower's fill depth options.
fn tower_args(
    fx: &Value,
    schema: &Schema,
    draft: &EngineInput,
    tower: &Value,
    water: f64,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["select".into()];
    args.extend(["--towers".into(), schema.spec("towers", tower)]);
    for (flag, list) in [
        ("--fills", "fills"),
        ("--drift-eliminators", "driftEliminators"),
        ("--fans", "fans"),
        ("--nozzles", "nozzles"),
    ] {
        args.push(flag.into());
        args.push(
            records(fx, list)
                .iter()
                .map(|r| schema.spec(list, r))
                .collect::<Vec<_>>()
                .join(";"),
        );
    }
    args.extend(["--quality-factors".into(), quality_factors(fx)]);
    args.extend(["--water".into(), n(water)]);
    args.extend(duty_args(draft));
    args.extend([
        "--all-orders".into(),
        "--max-results".into(),
        "100000".into(),
    ]);
    args
}

/// The draft's run both ways: the app's engine path (`AdapterEngine::run`, the answer bar's
/// source) and the CLI's mirror of it.
fn draft_run(draft: &EngineInput) -> (EngineOutput, Cli) {
    let fx = fixture();
    let schema = Schema::new();
    let catalog =
        crate::engine_catalog::catalog_from_fixture(FIXTURE).expect("the fixture's catalog builds");
    let engine = AdapterEngine::new(catalog);
    let out = engine.run(draft).expect("the recorded duty runs");
    let mirror = cli(&draft_mirror_args(
        &fx,
        &schema,
        draft,
        adapter_water(draft),
        draft.duty.hot_water_c,
        draft.duty.target_cold_water_c,
        draft.duty.wet_bulb_c,
        draft.duty.dry_bulb_c,
    ));
    (out, mirror)
}

/// The ranked-first candidate's `coldWaterC` of a `select` run, or `None` when nothing was
/// feasible.
fn first_cold(cli: &Cli) -> Option<f64> {
    (cli.array_len(&[Key("results")]) > 0)
        .then(|| cli.number(&[Key("results"), Index(0), Key("coldWaterC")]))
}

/// The ranked-first candidate's fan power (the engine's `fanPowerKW` for it).
fn engine_fan_kw_from(cli: &Cli, index: usize) -> f64 {
    cli.number(&[
        Key("results"),
        Index(index),
        Key("fanOperatingPoint"),
        Key("shaftPowerKW"),
    ])
}

/// One candidate, every screen field against the CLI's own candidate.
fn assert_candidate(ui: &data::Cand, cli: &Cli, index: usize, tower: &str) {
    let where_ = format!("{tower} {}/{}@{}", ui.fill_id, ui.fan_id, ui.depth_m);
    let number = |key: &str| cli.number(&[Key("results"), Index(index), Key(key)]);
    let nested =
        |outer: &str, key: &str| cli.number(&[Key("results"), Index(index), Key(outer), Key(key)]);
    let text = |key: &str| cli.text(&[Key("results"), Index(index), Key(key)]);
    assert_eq!(ui.tower_id, text("towerId"), "{where_}: tower");
    assert_eq!(ui.fill_id, text("fillId"), "{where_}: fill");
    assert_eq!(ui.depth_m, number("fillDepthM"), "{where_}: depth");
    assert_eq!(ui.fan_id, text("fanId"), "{where_}: fan");
    assert_eq!(ui.speed, number("speedRatio"), "{where_}: speed");
    assert_eq!(ui.cold_c, number("coldWaterC"), "{where_}: cold water");
    assert_eq!(ui.margin_c, number("thermalMarginC"), "{where_}: margin");
    assert_eq!(
        ui.capacity_kg_s,
        Some(number("capacityKgS")),
        "{where_}: capacity"
    );
    assert_eq!(
        ui.cap_ratio,
        Some(number("capabilityRatio")),
        "{where_}: capability ratio"
    );
    assert_eq!(
        ui.makeup_kg_s,
        nested("waterBalance", "makeupKgS"),
        "{where_}: make-up"
    );
    assert_eq!(
        ui.drift_ppm,
        nested("airside", "driftPpm"),
        "{where_}: drift"
    );
    assert_eq!(
        ui.pressure_pa,
        nested("airside", "totalPa"),
        "{where_}: total pressure"
    );
    assert_eq!(
        ui.airflow_m3_s,
        nested("fanOperatingPoint", "flowM3S"),
        "{where_}: airflow"
    );
    assert_eq!(
        ui.dry_air_kg_s,
        nested("airside", "dryAirMassFlowKgS"),
        "{where_}: dry air"
    );
    assert_eq!(
        ui.available_merkel,
        nested("airside", "availableMerkelNumber"),
        "{where_}: KaV/L"
    );
    assert_eq!(ui.nozzles, nested("nozzle", "count"), "{where_}: nozzles");
    assert_eq!(
        ui.nozzle_id,
        cli.text(&[Key("results"), Index(index), Key("nozzle"), Key("nozzleId")]),
        "{where_}: nozzle"
    );
    // The field the design round got wrong, pinned here: the card's fan power is the engine's fan
    // power, and the electrical input is a different number the card must not show.
    assert_eq!(
        ui.power_kw,
        nested("fanOperatingPoint", "shaftPowerKW"),
        "{where_}: fan power"
    );
}

// -------------------------------------------------------------------------------------- Size

/// The screen's per-tower selection runs against one `select` per tower: the same feasible counts,
/// the same rejection reasons, the same ranked candidates number for number - and the ranked
/// winner's fan power equals what the engine reports for it (the design-round defect).
#[test]
fn size_workspace_equals_the_native_cli() {
    let fx = fixture();
    let schema = Schema::new();
    let draft = FixtureEngine::from_json(FIXTURE)
        .expect("the fixture parses")
        .default_input();
    let water = data::water_kg_s(&draft);

    let mut cache = data::Cache::default();
    let mut steps = 0;
    while cache.step(data::Want::Size, FIXTURE, &draft, None, None) {
        steps += 1;
        assert!(steps < 10_000, "the size workspace never finished");
    }
    let size = cache.size.as_ref().expect("the size workspace has data");
    assert_eq!(
        size.towers.len(),
        records(&fx, "towers").len(),
        "every tower has a run"
    );
    assert_eq!(size.water_kg_s, water, "the screen's own requirement");

    // The winner: the card the screens paint, plus the engine's two powers for it - stashed from
    // the same CLI run, so nothing is recomputed and no number can drift between the checks.
    let mut winner: Option<(data::Cand, f64, f64, f64)> = None;
    for tower_run in &size.towers {
        let tower = record(&fx, "towers", &tower_run.id);
        let run = cli(&tower_args(&fx, &schema, &draft, tower, water));
        assert_eq!(
            tower_run.feasible as u64,
            run.number(&[Key("feasibleCandidateCount")]) as u64,
            "tower {}: the feasible count",
            tower_run.id
        );
        let cli_rejections: BTreeMap<String, u64> = run
            .entries(&[Key("rejectionSummary")])
            .into_iter()
            .map(|(reason, count)| {
                (
                    reason.to_string(),
                    count.parse::<u64>().expect("a rejection count"),
                )
            })
            .collect();
        let ui_rejections: BTreeMap<String, u64> = tower_run
            .rejections
            .iter()
            .map(|(reason, count)| (reason.clone(), *count as u64))
            .collect();
        assert_eq!(
            ui_rejections, cli_rejections,
            "tower {}: the rejection reasons the screen paints",
            tower_run.id
        );
        assert_eq!(
            tower_run.cands.len(),
            run.array_len(&[Key("results")]),
            "tower {}: the ranked candidates",
            tower_run.id
        );
        for (index, ui) in tower_run.cands.iter().enumerate() {
            assert_candidate(ui, &run, index, &tower_run.id);
            let capacity = run.number(&[Key("results"), Index(index), Key("capacityKgS")]);
            if winner
                .as_ref()
                .is_none_or(|(_, best_capacity, _, _)| capacity < *best_capacity)
            {
                winner = Some((
                    ui.clone(),
                    capacity,
                    engine_fan_kw_from(&run, index),
                    run.number(&[Key("results"), Index(index), Key("electricalInputKW")]),
                ));
            }
        }
    }

    // The regression the design round shipped: the winner card's fan power must be the engine's
    // fan power for that candidate - the CLI prints fan and electrical input side by side, so the
    // card showing the wrong one fails the assert above; this pins that the two are different
    // numbers for the winner, or the equality would prove nothing.
    let (card, _, fan_kw, electrical_kw) = winner.expect("the fixture's duty has a feasible tower");
    // ...the two numbers are genuinely different for the winner, or the card showing the wrong one
    // would be invisible to the equality above.
    assert_ne!(
        fan_kw, electrical_kw,
        "the winner's fan power and electrical input are two numbers to choose between \
         ({}/{}@{} ~ {fan_kw} vs {electrical_kw})",
        card.tower_id, card.fill_id, card.depth_m
    );
    // Issue #107: the electrical input's **value**, not only its difference from the fan power.
    // The `assert_ne!` above kept its comparison discriminating, but nothing pinned the number: a
    // negation of the field the CLI printed survived the whole suite. The engine computes it as
    // `shaft_power_kw / drive_efficiency / motor_efficiency` (`rust/src/selection.rs`), so the
    // printed field is compared, exact f64 bits like every other number in this module, against
    // that definition over the winner's own fan record.
    let fan = record(&fx, "fans", &card.fan_id);
    let drive = fan["driveEfficiency"]
        .as_f64()
        .expect("the winner's drive efficiency");
    let motor = fan["motorEfficiency"]
        .as_f64()
        .expect("the winner's motor efficiency");
    let expected_kw = fan_kw / drive / motor;
    assert_eq!(
        electrical_kw,
        expected_kw,
        "{}: electricalInputKW - the CLI's printed {electrical_kw} is not the engine's \
         shaftPowerKW/driveEfficiency/motorEfficiency ({fan_kw}/{drive}/{motor} = {expected_kw}); \
         delta {}",
        card.tower_id,
        electrical_kw - expected_kw
    );
    let (_, mirror) = draft_run(&draft);

    // The answer bar's own reading of the same field: the engine's run of the draft (its
    // `EngineOutput.fan_power_kw`) is the CLI's answer for the draft's configuration, and the
    // answer card paints that field.
    let out = {
        let catalog = crate::engine_catalog::catalog_from_fixture(FIXTURE)
            .expect("the fixture's catalog builds");
        AdapterEngine::new(catalog)
            .run(&draft)
            .expect("the recorded duty runs")
    };
    assert_eq!(
        out.cold_water_c,
        mirror.number(&[Key("results"), Index(0), Key("coldWaterC")]),
        "the answer bar's cold water is the CLI's"
    );
    assert_eq!(
        out.fan_power_kw,
        engine_fan_kw_from(&mirror, 0),
        "the answer bar's fan power is the CLI's"
    );
    let answer = data::answer(&draft, Some(&out), data::recorded_limits(FIXTURE));
    assert_eq!(
        answer.fan_kw,
        engine_fan_kw_from(&mirror, 0),
        "the answer card paints the engine's own fan power"
    );
}

// -------------------------------------------------------------------------------------- Rate

/// The fitted characteristic: `capability.rs`'s `evaluate_characteristic_capability` on the
/// screen's conditions (the STUB test readings included) against the CLI's `capability` command
/// with the same conditions - capability %, both L/G ratios, the fitted C and m.
#[test]
fn rate_workspace_equals_the_native_cli() {
    let draft = FixtureEngine::from_json(FIXTURE)
        .expect("the fixture parses")
        .default_input();
    let (out, _) = draft_run(&draft);

    let mut cache = data::Cache::default();
    cache.step(data::Want::Rate, FIXTURE, &draft, Some(&out), None);
    let rate = cache.rate.as_ref().expect("the rate workspace has data");
    let point = match &rate.points[0] {
        Ok(point) => point,
        Err(error) => panic!("the fixture's first test point evaluates: {error}"),
    };

    let m_w = data::water_kg_s(&draft);
    let m_a = data::step_value(&out, "Dry-air mass flow").expect("the run's dry-air step");
    let d = &draft.duty;
    let (fw, fa, hot, cold, wb, db) = data::STUB_TEST_POINTS[0];
    let condition = |water: f64, air: f64, hot: f64, cold: f64, wb: f64, db: f64| {
        format!(
            "waterMassFlowKgS:{},dryAirMassFlowKgS:{},hotWaterC:{},coldWaterC:{},wetBulbC:{},dryBulbC:{},pressurePa:{}",
            n(water),
            n(air),
            n(hot),
            n(cold),
            n(wb),
            n(db),
            n(d.pressure_pa)
        )
    };
    let run = cli(&[
        "capability".into(),
        "--design".into(),
        condition(
            m_w,
            m_a,
            d.hot_water_c,
            d.target_cold_water_c,
            d.wet_bulb_c,
            d.dry_bulb_c,
        ),
        "--test".into(),
        condition(m_w * fw, m_a * fa, hot, cold, wb, db),
        "--curve-points".into(),
        "48".into(),
    ]);

    assert_eq!(
        point.capability_pct,
        run.number(&[Key("capabilityPct")]),
        "the capability % against design"
    );
    assert_eq!(
        point.design_lg,
        run.number(&[Key("designWaterToDryAirRatio")]),
        "the design L/G"
    );
    assert_eq!(
        point.test_lg,
        run.number(&[Key("testWaterToDryAirRatio")]),
        "the test L/G"
    );
    assert_eq!(
        point.cap_lg,
        run.number(&[Key("capabilityWaterToDryAirRatio")]),
        "the capability L/G"
    );
    assert_eq!(
        point.test_c,
        run.number(&[Key("testCharacteristicCoefficient")]),
        "the fitted characteristic's C"
    );
    assert_eq!(
        point.exponent_m,
        run.number(&[Key("characteristicExponent")]),
        "the fitted characteristic's m"
    );
    assert!(
        point.test_c > 0.0 && point.exponent_m < 0.0,
        "a sane fit: KaV/L falls with L/G"
    );
}

// ------------------------------------------------------------------------------------ Curves

/// The grid's records against the CLI: the first record's whole hot-water iteration replayed
/// through `select` (the seed is the screen's closed form, so the replay is call for call), and
/// the read-off plus the inverse lookup on the screen's own records through `curve-predict` and
/// `curve-flow`.
#[test]
fn curves_workspace_equals_the_native_cli() {
    let fx = fixture();
    let schema = Schema::new();
    let draft = FixtureEngine::from_json(FIXTURE)
        .expect("the fixture parses")
        .default_input();
    let catalog =
        crate::engine_catalog::catalog_from_fixture(FIXTURE).expect("the fixture's catalog builds");
    let engine = AdapterEngine::new(catalog);
    let (out, _) = draft_run(&draft);

    let mut cache = data::Cache::default();
    let mut steps = 0;
    while cache.step(
        data::Want::Curves,
        FIXTURE,
        &draft,
        Some(&out),
        Some(&engine),
    ) {
        steps += 1;
        assert!(steps < 10_000, "the curves grid never finished");
    }
    let curves = cache.curves.as_ref().expect("the curves grid has data");
    assert_eq!(
        curves.failed, 0,
        "the fixture's grid computes whole: {:?}",
        curves.fail_why
    );

    // At least three flow lines, each with at least three points (the issue's AC, pinned).
    let mut lines: BTreeMap<String, usize> = BTreeMap::new();
    for rec in &curves.recs {
        *lines.entry(n(rec.flow_kg_s)).or_default() += 1;
    }
    assert!(
        lines.len() >= 3,
        "the grid carries >= 3 flow lines, has {}",
        lines.len()
    );
    for (flow, count) in &lines {
        assert!(
            *count >= 3,
            "the {flow} kg/s line carries >= 3 points, has {count}"
        );
    }

    // The first grid record - (wb 20, range 10, flow 80%) - replayed through the native CLI.
    let rec = curves.recs.first().expect("the grid has records");
    assert_eq!(
        (rec.wb, rec.range),
        (20.0, 10.0),
        "the first record is the screen's first grid point"
    );
    let design = curves.design_flow_kg_s;
    assert!(
        (rec.flow_kg_s - design * 0.8).abs() < 1e-9,
        "the first record is the 80% flow line"
    );
    let replayed = replay_first_record(&fx, &schema, &draft);
    assert_eq!(
        replayed, rec.cold,
        "the CLI replay of the first grid record lands on the screen's own cold water"
    );

    // The read-off at the probe (wb 27, range 10, the design flow) and the inverse lookup.
    let (ui_cold, ui_extrapolated) =
        data::predict_cold(curves, 27.0, 10.0, design).expect("the screen's read-off");
    let (ui_flow, ui_at_flow) =
        data::predict_flow(curves, 27.0, 10.0, ui_cold).expect("the screen's inverse lookup");
    let record_list = curves
        .recs
        .iter()
        .map(|r| {
            format!(
                "wetBulbC:{},rangeC:{},waterFlowKgS:{},coldWaterC:{}",
                n(r.wb),
                n(r.range),
                n(r.flow_kg_s),
                n(r.cold)
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let predicted = cli(&[
        "curve-predict".into(),
        "--records".into(),
        record_list.clone(),
        "--wb".into(),
        "27".into(),
        "--range".into(),
        "10".into(),
        "--water".into(),
        n(design),
    ]);
    assert_eq!(
        ui_cold,
        predicted.number(&[Key("coldWaterC")]),
        "the read-off's cold water is the CLI's"
    );
    assert_eq!(
        ui_extrapolated,
        predicted.flag(&[Key("extrapolated")]),
        "the read-off's extrapolated flag is the CLI's"
    );
    let looked_up = cli(&[
        "curve-flow".into(),
        "--records".into(),
        record_list,
        "--wb".into(),
        "27".into(),
        "--range".into(),
        "10".into(),
        "--cold".into(),
        n(ui_cold),
    ]);
    assert_eq!(
        ui_flow,
        looked_up.number(&[Key("waterFlowKgS")]),
        "the inverse lookup's water flow is the CLI's"
    );
    assert_eq!(
        ui_at_flow,
        looked_up.number(&[Key("predictedColdWaterC")]),
        "the inverse lookup's interpolated cold water is the CLI's"
    );
}

/// The screen's iteration for the grid's first record, call for call: the first record of the
/// first range (wb 20, range 10, 80% flow) has no earlier record to seed from, so the seed is the
/// screen's closed form `wb + (target - wb)` and the whole loop is reproducible. Every engine call
/// goes through the native CLI (`select` on the draft's configuration, duty overridden the way the
/// screen overrides it).
fn replay_first_record(fx: &Value, schema: &Schema, draft: &EngineInput) -> f64 {
    use data::CURVE_TOL;
    let range = 10.0;
    let wb = 20.0;
    let db = wb + (draft.duty.dry_bulb_c - draft.duty.wet_bulb_c);
    let m3_hr = draft.duty.water_flow_m3_hr * 0.8;
    let mut cold = wb + (draft.duty.target_cold_water_c - draft.duty.wet_bulb_c);
    let mut loose = true;
    for _ in 0..6 {
        let hot = cold + range;
        let target = if loose {
            cold + range - 0.1
        } else {
            cold + 0.3
        };
        let mean = (hot + target) / 2.0;
        let rho =
            eng::water_density_kg_m3(mean, draft.duty.salinity_g_kg).expect("the engine's density");
        let water = m3_hr / 3600.0 * rho;
        let run = cli_opt(&draft_mirror_args(
            fx, schema, draft, water, hot, target, wb, db,
        ));
        let solved = run.as_ref().and_then(first_cold);
        let Some(solved) = solved else {
            // The tight run refused (the screen's `Ok(o) if !loose` branch): go loose and retry;
            // a loose run that refuses is a real failure (the record would not exist).
            assert!(!loose, "the CLI refused the record's loose run");
            loose = true;
            continue;
        };
        let moved = (solved - cold).abs();
        cold = solved;
        if moved < CURVE_TOL && !loose {
            break;
        }
        loose = false;
    }
    cold
}

// ------------------------------------------------------------------------------------- Water

/// The water balance: the screen's `water_balance.rs` numbers against the CLI's `balance` and
/// `drift` commands driven with the screen's own inputs, and the run's own numbers against the
/// CLI's `select` for the draft.
#[test]
fn water_workspace_equals_the_native_cli() {
    let draft = FixtureEngine::from_json(FIXTURE)
        .expect("the fixture parses")
        .default_input();
    let (out, mirror) = draft_run(&draft);
    let cycles = draft.duty.cycles_of_concentration;
    let ui = data::water(&draft, &out, cycles).expect("the fixture's water balance");

    let balance = cli(&[
        "balance".into(),
        "--evaporation".into(),
        n(ui.evaporation_kg_s),
        "--drift".into(),
        n(ui.drift_kg_s),
        "--cycles".into(),
        n(cycles),
    ]);
    assert_eq!(
        balance.number(&[Key("evaporationKgS")]),
        ui.evaporation_kg_s,
        "the balance carries back the evaporation it was given"
    );
    assert_eq!(
        balance.number(&[Key("blowdownKgS")]),
        ui.blowdown_kg_s,
        "the screen's blowdown is the engine's balance"
    );
    assert_eq!(
        balance.number(&[Key("makeupKgS")]),
        ui.makeup_kg_s,
        "the screen's make-up is the engine's balance"
    );
    assert_eq!(
        balance.number(&[Key("cyclesOfConcentration")]),
        ui.cycles,
        "the cycles the balance ran at"
    );

    let drift = cli(&[
        "drift".into(),
        "--circulating-water".into(),
        n(data::water_kg_s(&draft)),
        "--drift-ppm".into(),
        n(ui.drift_ppm),
    ]);
    assert_eq!(
        drift.number(&[Key("driftKgS")]),
        ui.drift_kg_s,
        "the screen's drift is the engine's drift loss"
    );

    // The run's own numbers: the screen's ppm is the eliminator's curve at the run's face
    // velocity, and its evaporation is the engine's, re-weighted through the cockpit's water
    // mass flow - the one place the two spellings of that flow meet (see the module doc).
    assert_eq!(
        ui.drift_ppm,
        mirror.number(&[Key("results"), Index(0), Key("airside"), Key("driftPpm")]),
        "the eliminator's ppm at the run's face velocity"
    );
    let engine_evaporation = mirror.number(&[
        Key("results"),
        Index(0),
        Key("waterBalance"),
        Key("evaporationKgS"),
    ]);
    let ulps = (ui.evaporation_kg_s - engine_evaporation).abs()
        / (engine_evaporation.abs() * f64::EPSILON);
    assert!(
        ulps < 4.0,
        "the screen's evaporation is the engine's to the last ulps of the water mass flow \
         (differ by {ulps} ulps: {} vs {engine_evaporation})",
        ui.evaporation_kg_s
    );
}

// --------------------------------------------------------------------------------- the header

/// The four workspaces are the app's header tabs: the shell's own tab list (the `Screen` enum the
/// header and the dock iterate) carries all four, each with its own slug and label - and each
/// tab's data comes from the engine path the tests above pin.
#[test]
fn the_four_workspaces_are_header_tabs() {
    use super::Screen;
    let slugs: Vec<&str> = Screen::ALL.iter().map(|screen| screen.slug()).collect();
    for screen in [Screen::Size, Screen::Rate, Screen::Curves, Screen::Water] {
        assert!(
            slugs.contains(&screen.slug()),
            "the header's tab list carries {}",
            screen.name()
        );
        assert!(!screen.name().is_empty(), "{} has a label", screen.slug());
    }
}
