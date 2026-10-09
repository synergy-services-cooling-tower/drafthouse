//! The `.drafthouse` project file: the **one** reader/writer, and the one place its schema is stated.
//!
//! Owner decision (2026-09-23, the private decision record D26): native and web share **one persistence model -
//! file-based first, optional connect later**. A project is a file; a catalog revision is an immutable
//! file object ([`crate::revision`]); nothing in this module (or in this crate) talks to a network.
//!
//! # What the file carries
//!
//! * `duty` - every field of the working thermal duty (all nine the engine's `Duty` contract holds), and
//!   `site` - the site rows the DUTY & SITE panel shows that are *recorded* rather than edited (the
//!   derived ISA altitude, the display-only water-quality rows, the recorded requirement limits);
//! * `tower` / `fitted` / `fillLayers` - the machine: the tower record's **id**, the fitted fan / drift /
//!   nozzle **ids**, and the ordered fill stack (id, depth, both multipliers);
//! * `customParts` - the records the user authored, as **full records** marked `custom` with a provenance
//!   note (they live in the file, not in a session: D26 replaced the session-scoped store);
//! * `catalogRevisionId` - the revision the run used, and `engine` - the engine's own name, this crate's
//!   version and the **adapter contract hash** the file was written against;
//! * `speedRatio`, and `results` - a snapshot of the 11 headline metrics and the per-layer rows, marked
//!   *"as calculated on save - recomputed on open"* ([`RESULTS_NOTE`]).
//!
//! # The two rules that keep the file usable
//!
//! 1. **Forward compatibility: unknown fields are preserved.** Every block carries
//!    `#[serde(flatten)] rest`: a key this version does not know is kept verbatim and written back in its
//!    original position in the object, so a file written by a newer build survives a round trip through
//!    this one - and a build that learns a key later does not have to drop what it used to ignore. The
//!    *known* keys are the schema; everything else is a passenger.
//! 2. **Byte-stable writes.** [`Project::write`] serialises deterministically: fixed field order, sorted
//!    keys for every map, `ryu`'s shortest round-trip form for numbers. `write -> read -> write` is
//!    therefore byte-equal, which is what the round-trip test asserts rather than trusting.
//!
//! # The schema version
//!
//! `formatVersion` is checked on read, not ignored: a file without it, or one written by a newer
//! `formatVersion`, is refused by name (a silent partial read of a future file is the one failure mode a
//! persistence format cannot afford).
//!
//! # Why this module has no Bevy and no `bevy_*` type in it
//!
//! It compiles on both planes unchanged (native has the file dialogs, the web-internal host has the page's
//! download/upload through the existing bridge), and it is testable without building the renderer - the
//! dependency set is `serde`, `serde_json`, the engine contract and the seams crate.

use std::collections::BTreeMap;

use cockpit::engine::{Duty, EngineInput, EngineOutput, FillLayer, HEADLINES};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use drafthouse_cockpit_seams::custom::{CustomPart, Entry, Value as PartValueRaw, Values};

/// The schema version this build writes and is willing to read.
pub const FORMAT_VERSION: u32 = 1;
/// The file's own `kind` marker, so a stray JSON file is refused by name rather than half-read.
pub const KIND: &str = "drafthouse.project";
/// The application's own name, as `savedBy.app` records it. It is the product, not the document's `kind`.
pub const APP: &str = "drafthouse";
/// The file extension the native dialogs filter on.
pub const EXTENSION: &str = "drafthouse";
/// The snapshot's own label. Recomputed on open - never trusted as the current answer.
pub const RESULTS_NOTE: &str = "as calculated on save - recomputed on open";
/// The provenance note every record that lives in the file carries.
pub const CUSTOM_PROVENANCE: &str =
    "authored in this project file - user-entered values, not vendor data";
/// This crate's version, from the manifest (never typed twice).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The digest of the **compiled** engine contract copy (`cockpit/contract/src/engine.rs`).
///
/// The adapter carries a byte-identical copy of the same file behind its own drift gate; recording this
/// digest in a project says which contract the numbers were produced against, so a later contract change
/// is visible in a project file instead of being guessed at.
pub fn contract_sha256() -> &'static str {
    use std::sync::LazyLock;
    static HASH: LazyLock<String> =
        LazyLock::new(|| crate::sha256::hex(include_bytes!("../contract/src/engine.rs")));
    &HASH
}

// --------------------------------------------------------------------------------------------- blocks

/// Who wrote the file. `host` is the plane (`native` or `web-internal`); the public host has no writer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedBy {
    pub host: String,
    pub app: String,
    pub version: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The engine a project's numbers came from, and the contract they were produced against.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineMark {
    /// The engine's own name (`Engine::name`), as the read-out and the provenance line carry it.
    pub name: String,
    pub version: String,
    pub adapter_contract_sha256: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// Every field of the working duty - the nine the engine's contract holds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DutyBlock {
    pub water_flow_m3_hr: f64,
    pub hot_water_c: f64,
    pub target_cold_water_c: f64,
    pub wet_bulb_c: f64,
    pub dry_bulb_c: f64,
    pub pressure_pa: f64,
    pub salinity_g_kg: f64,
    pub water_quality_class: String,
    pub cycles_of_concentration: f64,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl DutyBlock {
    pub fn of(duty: &Duty) -> Self {
        Self {
            water_flow_m3_hr: duty.water_flow_m3_hr,
            hot_water_c: duty.hot_water_c,
            target_cold_water_c: duty.target_cold_water_c,
            wet_bulb_c: duty.wet_bulb_c,
            dry_bulb_c: duty.dry_bulb_c,
            pressure_pa: duty.pressure_pa,
            salinity_g_kg: duty.salinity_g_kg,
            water_quality_class: duty.water_quality_class.clone(),
            cycles_of_concentration: duty.cycles_of_concentration,
            rest: Map::new(),
        }
    }

    /// Write the nine fields back into a working duty. The fields this file does not carry are left
    /// exactly as the caller had them (an older file cannot zero a newer field).
    pub fn write_into(&self, duty: &mut Duty) {
        duty.water_flow_m3_hr = self.water_flow_m3_hr;
        duty.hot_water_c = self.hot_water_c;
        duty.target_cold_water_c = self.target_cold_water_c;
        duty.wet_bulb_c = self.wet_bulb_c;
        duty.dry_bulb_c = self.dry_bulb_c;
        duty.pressure_pa = self.pressure_pa;
        duty.salinity_g_kg = self.salinity_g_kg;
        duty.water_quality_class = self.water_quality_class.clone();
        duty.cycles_of_concentration = self.cycles_of_concentration;
    }

    /// The nine field names this block carries, in schema order - the list the docs and the tests read.
    pub const FIELDS: [&'static str; 9] = [
        "waterFlowM3Hr",
        "hotWaterC",
        "targetColdWaterC",
        "wetBulbC",
        "dryBulbC",
        "pressurePa",
        "salinityGKg",
        "waterQualityClass",
        "cyclesOfConcentration",
    ];
}

/// The site rows the DUTY & SITE panel shows that are recorded rather than edited.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteBlock {
    /// Derived, never measured: the ISA standard-atmosphere altitude for `duty.pressurePa`.
    pub altitude_m: f64,
    /// What that number is, in one sentence, so no reader takes it for a survey.
    pub altitude_from: String,
    /// The display-only water-quality rows (`duty-fields.json` -> `displayOnly`), keyed by their own
    /// spec keys, `null` where the descriptor records no value. Recorded, not used by the engine yet.
    pub water_quality_recorded: BTreeMap<String, Option<f64>>,
    /// The recorded requirement rows (`duty-fields.json` -> `limits`), keyed by their own spec keys.
    pub limits_recorded: BTreeMap<String, f64>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// One fill layer of the stack: id, depth, and both multipliers (the recorded correction factors).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LayerBlock {
    pub fill_id: String,
    pub depth_m: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The fitted records, by id. The records themselves live in the pinned catalog revision.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FittedBlock {
    pub fan: String,
    pub drift: String,
    pub nozzle: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A value the user typed, in the file. The shape mirrors the seams crate's own value model one for one
/// (`drafthouse_cockpit_seams::custom::Value`), with a JSON form a human can read: an audit's attribution
/// list stays an ordered array of `[key, value]` pairs, never a re-sorted map.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum PartValue {
    Text(String),
    Number(f64),
    Range2([f64; 2]),
    List(Vec<String>),
    Points(Vec<Vec<f64>>),
    Object(Vec<(String, f64)>),
}

impl Default for PartValue {
    fn default() -> Self {
        PartValue::Text(String::new())
    }
}

/// One record that lives in the file rather than in the pinned revision.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PartBlock {
    /// Always `custom` - the marker that says where this record came from.
    pub record: String,
    /// The provenance note ([`CUSTOM_PROVENANCE`]), written into every file.
    pub provenance: String,
    /// `fan` / `drift` / `fill` / `nozzle`.
    pub class: String,
    /// The record's own fields, in the class's field order.
    pub values: Vec<PartEntry>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PartEntry {
    pub key: String,
    pub value: PartValue,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl PartBlock {
    pub fn of(part: &CustomPart) -> Self {
        Self {
            record: "custom".to_string(),
            provenance: CUSTOM_PROVENANCE.to_string(),
            class: part.class.clone(),
            values: part
                .values
                .iter()
                .map(|e| PartEntry {
                    key: e.key.clone(),
                    value: out(&e.value),
                    rest: Map::new(),
                })
                .collect(),
            rest: Map::new(),
        }
    }

    /// Is this block a custom record this build can hand back to the form's own value model?
    pub fn is_custom(&self) -> bool {
        self.record == "custom"
    }

    pub fn to_custom(&self) -> Result<CustomPart, String> {
        if !self.is_custom() {
            return Err(format!(
                "record `{}` is not `custom` - a record that lives in the pinned revision is a reference, not a copy",
                self.record
            ));
        }
        let values: Values = self
            .values
            .iter()
            .map(|e| Entry {
                key: e.key.clone(),
                value: back(&e.value),
            })
            .collect();
        Ok(CustomPart::new(self.class.clone(), values))
    }
}

/// The seams value model out of a record, for the file.
fn out(value: &PartValueRaw) -> PartValue {
    match value {
        PartValueRaw::Text(s) => PartValue::Text(s.clone()),
        PartValueRaw::Number(n) => PartValue::Number(*n),
        PartValueRaw::Range2(a, b) => PartValue::Range2([*a, *b]),
        PartValueRaw::TextList(l) => PartValue::List(l.clone()),
        PartValueRaw::Points(p) => PartValue::Points(p.clone()),
        PartValueRaw::Object(o) => PartValue::Object(o.clone()),
    }
}

/// The file's value model back into the record the form and the engine read.
fn back(value: &PartValue) -> PartValueRaw {
    match value {
        PartValue::Text(s) => PartValueRaw::Text(s.clone()),
        PartValue::Number(n) => PartValueRaw::Number(*n),
        PartValue::Range2([a, b]) => PartValueRaw::Range2(*a, *b),
        PartValue::List(l) => PartValueRaw::TextList(l.clone()),
        PartValue::Points(p) => PartValueRaw::Points(p.clone()),
        PartValue::Object(o) => PartValueRaw::Object(o.clone()),
    }
}

/// One headline metric, named exactly as the contract's own table names it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HeadlineRow {
    pub metric: String,
    pub unit: String,
    pub value: f64,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// One fill layer's own results, as the run reported them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LayerRow {
    pub index: usize,
    pub fill_id: String,
    pub depth_m: f64,
    pub kavl: f64,
    pub pressure_pa: f64,
    pub cooling_share_pct: f64,
    pub inside_envelope: bool,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The results snapshot: what the engine said when the file was written.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResultsBlock {
    /// [`RESULTS_NOTE`], verbatim, so the file itself says what these numbers are.
    pub note: String,
    /// The engine that produced them (`Engine::name`).
    pub engine: String,
    /// The 11 headline metrics, in the contract's `HEADLINES` order.
    pub headlines: Vec<HeadlineRow>,
    /// One row per declared fill layer, in stack order.
    pub layers: Vec<LayerRow>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl ResultsBlock {
    /// Snapshot an output. The headline rows are built from the contract's own `HEADLINES` table, so the
    /// snapshot cannot drift from the names and units the first screen shows.
    pub fn of(output: &EngineOutput) -> Self {
        let values = output.headline_values();
        Self {
            note: RESULTS_NOTE.to_string(),
            engine: output.provenance.engine.clone(),
            headlines: HEADLINES
                .iter()
                .zip(values)
                .map(|(headline, value)| HeadlineRow {
                    metric: headline.name.to_string(),
                    unit: headline.unit.to_string(),
                    value,
                    rest: Map::new(),
                })
                .collect(),
            layers: output
                .kavl_per_layer
                .iter()
                .map(|layer| LayerRow {
                    index: layer.index,
                    fill_id: layer.fill_id.clone(),
                    depth_m: layer.depth_m,
                    kavl: layer.kavl,
                    pressure_pa: layer.pressure_pa,
                    cooling_share_pct: layer.cooling_share_pct,
                    inside_envelope: layer.inside_envelope,
                    rest: Map::new(),
                })
                .collect(),
            rest: Map::new(),
        }
    }

    /// How far a *live* run is from this snapshot, metric by metric: the largest relative difference and
    /// the metric that produced it. Zero-length when either side is empty.
    ///
    /// This is what the app prints when a project is opened - `as calculated on save - recomputed on
    /// open` is a claim about these numbers, and the app makes the claim measurable instead of asserting
    /// it. A difference is reported, never smoothed over: a snapshot from a different engine (the fixture
    /// replay against the real one) legitimately disagrees, and the line says which engine it was.
    pub fn delta(&self, output: &EngineOutput) -> SnapshotDelta {
        let live = output.headline_values();
        let mut worst = SnapshotDelta {
            metrics: self.headlines.len().min(live.len()),
            max_rel: 0.0,
            metric: String::new(),
            snapshot_engine: self.engine.clone(),
            live_engine: output.provenance.engine.clone(),
        };
        for (row, value) in self.headlines.iter().zip(live) {
            let scale = row.value.abs().max(value.abs()).max(1e-12);
            let rel = (row.value - value).abs() / scale;
            if rel > worst.max_rel {
                worst.max_rel = rel;
                worst.metric = row.metric.clone();
            }
        }
        worst
    }
}

/// The answer to "does the live run still agree with the snapshot?".
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotDelta {
    /// How many headline metrics were compared.
    pub metrics: usize,
    /// The largest relative difference over those metrics.
    pub max_rel: f64,
    /// The metric that produced it (empty when every metric matched exactly).
    pub metric: String,
    pub snapshot_engine: String,
    pub live_engine: String,
}

impl SnapshotDelta {
    /// The tolerance a *recomputation* has to land inside: the engines are the same code, so anything
    /// above rounding is a real difference and is reported as one.
    pub const TOLERANCE: f64 = 1e-9;

    pub fn agrees(&self) -> bool {
        self.metrics > 0 && self.max_rel <= Self::TOLERANCE
    }

    /// The one line the app shows (and the frames quote).
    pub fn line(&self) -> String {
        if self.metrics == 0 {
            return "no saved results in this project".to_string();
        }
        if self.agrees() {
            return format!(
                "recomputed on open: the {0} headline metrics match the saved snapshot",
                self.metrics
            );
        }
        format!(
            "recomputed on open: `{0}` differs by {1:.3e} relative (snapshot: {2}; this build: {3})",
            self.metric, self.max_rel, self.snapshot_engine, self.live_engine
        )
    }
}

// -------------------------------------------------------------------------------------------- project

/// A `.drafthouse` project file, in memory.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub format_version: u32,
    pub kind: String,
    pub saved_by: SavedBy,
    pub catalog_revision_id: String,
    pub engine: EngineMark,
    pub duty: DutyBlock,
    pub site: SiteBlock,
    pub tower: String,
    pub fill_layers: Vec<LayerBlock>,
    pub fitted: FittedBlock,
    pub speed_ratio: f64,
    pub custom_parts: Vec<PartBlock>,
    pub results: ResultsBlock,
    /// Unknown top-level keys, kept verbatim (the forward-compatibility rule).
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// Everything one project is built from. A struct rather than eight arguments, so a caller cannot silently
/// pass the revision id where the engine name goes.
pub struct Snapshot<'a> {
    /// `native` or `web-internal` - the plane that wrote the file.
    pub host: &'a str,
    pub input: &'a EngineInput,
    pub catalog_revision_id: &'a str,
    pub engine_name: &'a str,
    pub custom_parts: &'a [CustomPart],
    pub water_quality_recorded: BTreeMap<String, Option<f64>>,
    pub limits_recorded: BTreeMap<String, f64>,
    /// The run's own output, when there is one. `None` writes a file with an empty snapshot.
    pub output: Option<&'a EngineOutput>,
}

impl Project {
    /// Build a project from the working state (see [`Snapshot`]).
    pub fn build(snapshot: Snapshot<'_>) -> Self {
        let input = snapshot.input;
        Self {
            format_version: FORMAT_VERSION,
            kind: KIND.to_string(),
            saved_by: SavedBy {
                host: snapshot.host.to_string(),
                app: APP.to_string(),
                version: VERSION.to_string(),
                rest: Map::new(),
            },
            catalog_revision_id: snapshot.catalog_revision_id.to_string(),
            engine: EngineMark {
                name: snapshot.engine_name.to_string(),
                version: VERSION.to_string(),
                adapter_contract_sha256: contract_sha256().to_string(),
                rest: Map::new(),
            },
            duty: DutyBlock::of(&input.duty),
            site: SiteBlock {
                altitude_m: drafthouse_cockpit_seams::mapping::altitude_m_from_pressure_pa(
                    input.duty.pressure_pa,
                ),
                altitude_from:
                    "derived from duty.pressurePa through the ISA standard atmosphere - a definition, \
                     not a measurement"
                        .to_string(),
                water_quality_recorded: snapshot.water_quality_recorded,
                limits_recorded: snapshot.limits_recorded,
                rest: Map::new(),
            },
            tower: input.tower.id.clone(),
            fill_layers: input
                .fill_layers
                .iter()
                .map(|layer| LayerBlock {
                    fill_id: layer.fill_id.clone(),
                    depth_m: layer.depth_m,
                    thermal_multiplier: layer.thermal_multiplier,
                    pressure_multiplier: layer.pressure_multiplier,
                    rest: Map::new(),
                })
                .collect(),
            fitted: FittedBlock {
                fan: input.fan.id.clone(),
                drift: input.drift.id.clone(),
                nozzle: input.nozzle.id.clone(),
                rest: Map::new(),
            },
            speed_ratio: input.speed_ratio,
            custom_parts: snapshot.custom_parts.iter().map(PartBlock::of).collect(),
            results: snapshot
                .output
                .map(ResultsBlock::of)
                .unwrap_or_else(|| ResultsBlock {
                    note: RESULTS_NOTE.to_string(),
                    ..ResultsBlock::default()
                }),
            rest: Map::new(),
        }
    }

    /// The file's bytes. Deterministic: see the module docs.
    pub fn write(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a project is serialisable");
        text.push('\n');
        text
    }

    /// Parse a file's text, refusing a version this build cannot read.
    pub fn read(text: &str) -> Result<Project, String> {
        let project: Project = serde_json::from_str(text).map_err(|e| format!("`{KIND}`: {e}"))?;
        if project.format_version == 0 {
            return Err(format!(
                "`{KIND}`: no `formatVersion` - this is not a project file this build can read"
            ));
        }
        if project.format_version > FORMAT_VERSION {
            return Err(format!(
                "`{KIND}`: formatVersion {} is newer than this build's {FORMAT_VERSION} - open it with a \
                 newer build rather than reading part of it",
                project.format_version
            ));
        }
        Ok(project)
    }

    /// The ordered fill stack this file declares, as the engine's own type. Pure: no catalog needed.
    pub fn stack(&self) -> Vec<FillLayer> {
        self.fill_layers
            .iter()
            .map(|layer| FillLayer {
                fill_id: layer.fill_id.clone(),
                depth_m: layer.depth_m,
                thermal_multiplier: layer.thermal_multiplier,
                pressure_multiplier: layer.pressure_multiplier,
            })
            .collect()
    }

    /// Write the duty, the stack and the speed ratio into a working input. The tower and the fitted
    /// records are **ids**: resolving them is a catalog lookup and belongs to the caller.
    pub fn write_into(&self, input: &mut EngineInput) {
        self.duty.write_into(&mut input.duty);
        if !self.fill_layers.is_empty() {
            input.fill_layers = self.stack();
        }
        input.speed_ratio = self.speed_ratio;
    }

    /// The custom records this file carries, in order.
    pub fn custom_parts(&self) -> Result<Vec<CustomPart>, String> {
        self.custom_parts.iter().map(PartBlock::to_custom).collect()
    }

    /// Carry the unknown keys of an opened project onto a freshly built one.
    ///
    /// The app's save path re-serialises the working state ([`crate::files::snapshot_text`]), so without
    /// this it would drop exactly the keys the format's forward-compatibility rule keeps (issue #79 F2).
    /// Blocks a file has one of are carried one-to-one; blocks held in lists are matched by their own
    /// identity - a fill layer by `fillId`, a custom record by its class and `id` entry (and its entries
    /// by key), a headline by `metric`, a results layer by `index`.
    pub fn carry_unknowns(&mut self, opened: &Project) {
        fn take(dst: &mut Map<String, Value>, src: &Map<String, Value>) {
            for (key, value) in src {
                dst.insert(key.clone(), value.clone());
            }
        }
        take(&mut self.rest, &opened.rest);
        take(&mut self.saved_by.rest, &opened.saved_by.rest);
        take(&mut self.engine.rest, &opened.engine.rest);
        take(&mut self.duty.rest, &opened.duty.rest);
        take(&mut self.site.rest, &opened.site.rest);
        take(&mut self.fitted.rest, &opened.fitted.rest);
        take(&mut self.results.rest, &opened.results.rest);
        for layer in &mut self.fill_layers {
            if let Some(old) = opened
                .fill_layers
                .iter()
                .find(|o| o.fill_id == layer.fill_id)
            {
                take(&mut layer.rest, &old.rest);
            }
        }
        for part in &mut self.custom_parts {
            let id = part_identity(part);
            if let Some(old) = opened
                .custom_parts
                .iter()
                .find(|o| o.class == part.class && part_identity(o) == id)
            {
                take(&mut part.rest, &old.rest);
                for entry in &mut part.values {
                    if let Some(old_entry) = old.values.iter().find(|oe| oe.key == entry.key) {
                        take(&mut entry.rest, &old_entry.rest);
                    }
                }
            }
        }
        for row in &mut self.results.headlines {
            if let Some(old) = opened
                .results
                .headlines
                .iter()
                .find(|o| o.metric == row.metric)
            {
                take(&mut row.rest, &old.rest);
            }
        }
        for row in &mut self.results.layers {
            if let Some(old) = opened.results.layers.iter().find(|o| o.index == row.index) {
                take(&mut row.rest, &old.rest);
            }
        }
    }
}

/// The `id` a custom record carries in its value list - the identity the carry-over above matches on.
/// `None` when the record has no `id` entry, or one this build cannot read as text.
fn part_identity(part: &PartBlock) -> Option<&str> {
    part.values
        .iter()
        .find(|e| e.key == "id")
        .and_then(|e| match &e.value {
            PartValue::Text(s) => Some(s.as_str()),
            _ => None,
        })
}

// ------------------------------------------------------------------------------- results export text

/// The results export as JSON: the same snapshot shape the file carries, plus the pressure-by-zone rows.
pub fn results_json(output: &EngineOutput) -> String {
    let mut root = Map::new();
    root.insert("note".into(), Value::String(RESULTS_NOTE.into()));
    root.insert(
        "engine".into(),
        Value::String(output.provenance.engine.clone()),
    );
    root.insert(
        "catalogId".into(),
        Value::String(output.provenance.catalog_id.clone()),
    );
    root.insert(
        "catalogRevision".into(),
        Value::String(output.provenance.catalog_revision.clone()),
    );
    root.insert(
        "headlines".into(),
        serde_json::to_value(ResultsBlock::of(output).headlines)
            .expect("headline rows are serialisable"),
    );
    root.insert(
        "layers".into(),
        serde_json::to_value(ResultsBlock::of(output).layers).expect("layer rows are serialisable"),
    );
    root.insert(
        "byZone".into(),
        serde_json::to_value(&output.pressure_by_zone).expect("zone rows are serialisable"),
    );
    let mut text =
        serde_json::to_string_pretty(&Value::Object(root)).expect("the export is serialisable");
    text.push('\n');
    text
}

/// The results export as CSV: one row per quantity, `section,key,unit,value`.
pub fn results_csv(output: &EngineOutput) -> String {
    let values = output.headline_values();
    let mut text = String::from("section,key,unit,value\n");
    for (headline, value) in HEADLINES.iter().zip(values) {
        text.push_str(&format!(
            "headline,{},{},{}\n",
            csv_field(headline.name),
            csv_field(headline.unit),
            num(value)
        ));
    }
    for layer in &output.kavl_per_layer {
        let section = format!("layer[{}]", layer.index);
        let rows = [
            ("fillId".to_string(), String::new(), layer.fill_id.clone()),
            ("depthM".to_string(), "m".to_string(), num(layer.depth_m)),
            ("kavl".to_string(), "-".to_string(), num(layer.kavl)),
            (
                "pressurePa".to_string(),
                "Pa".to_string(),
                num(layer.pressure_pa),
            ),
            (
                "coolingSharePct".to_string(),
                "%".to_string(),
                num(layer.cooling_share_pct),
            ),
            (
                "insideEnvelope".to_string(),
                "-".to_string(),
                layer.inside_envelope.to_string(),
            ),
        ];
        for (key, unit, value) in rows {
            text.push_str(&format!(
                "{},{},{},{}\n",
                csv_field(&section),
                csv_field(&key),
                csv_field(&unit),
                csv_field(&value)
            ));
        }
    }
    text
}

/// A CSV field: quoted only when it has to be (a custom id is user text and may hold anything).
fn csv_field(text: &str) -> String {
    if text.contains(',') || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// Numbers in the export as the shortest round-trip form - the same rule the file writes with.
fn num(value: f64) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::engine::Engine;
    use cockpit::fixture_engine::FixtureEngine;
    use drafthouse_cockpit_seams::duty::DutySpec;
    use serde_json::json;

    const FIXTURE: &str = include_str!("../assets/fixture.json");
    const DUTY_SPEC: &str = include_str!("../assets/duty-fields.json");

    fn fixture() -> FixtureEngine {
        FixtureEngine::from_json(FIXTURE).expect("the fixture parses")
    }

    fn spec() -> DutySpec {
        DutySpec::from_json(DUTY_SPEC).expect("the duty & site descriptor parses")
    }

    /// The recorded (not edited) rows the panel shows, as the project file carries them.
    fn recorded(spec: &DutySpec) -> BTreeMap<String, Option<f64>> {
        spec.display_only
            .iter()
            .map(|row| (row.key.clone(), row.value))
            .collect()
    }

    fn limits(spec: &DutySpec) -> BTreeMap<String, f64> {
        spec.limits
            .iter()
            .map(|row| (row.key.clone(), row.value.unwrap_or_default()))
            .collect()
    }

    /// One custom record carrying **every** shape the value model has, in one list.
    fn every_shape() -> CustomPart {
        CustomPart::new(
            "fan",
            vec![
                Entry {
                    key: "id".into(),
                    value: PartValueRaw::Text("CUSTOM-FAN-1".into()),
                },
                Entry {
                    key: "stackAreaM2".into(),
                    value: PartValueRaw::Number(12.5),
                },
                Entry {
                    key: "allowedSpeedRatio".into(),
                    value: PartValueRaw::Range2(0.6, 1.1),
                },
                Entry {
                    key: "compatibleTowerTypes".into(),
                    value: PartValueRaw::TextList(vec!["induced-draft".into(), "crossflow".into()]),
                },
                Entry {
                    key: "curve".into(),
                    value: PartValueRaw::Points(vec![
                        vec![1.0, 100.0, 0.7],
                        vec![2.0, 220.0, 0.75],
                    ]),
                },
                Entry {
                    key: "audit".into(),
                    value: PartValueRaw::Object(vec![
                        ("legal, with a comma".into(), 0.6),
                        ("fan".into(), 0.4),
                    ]),
                },
            ],
        )
    }

    /// A project built from the recorded fixture run - the shape both planes write.
    fn base_project() -> Project {
        let engine = fixture();
        let input = engine.default_input();
        let output = engine.run(&input).expect("the recorded duty runs");
        Project::build(Snapshot {
            host: "native",
            input: &input,
            catalog_revision_id: crate::revision::SHIPPED_ID,
            engine_name: engine.name(),
            custom_parts: std::slice::from_ref(&every_shape()),
            water_quality_recorded: recorded(&spec()),
            limits_recorded: limits(&spec()),
            output: Some(&output),
        })
    }

    /// The acceptance criterion, as a test: `write -> read -> write` is byte-equal.
    #[test]
    fn the_project_round_trips_byte_for_byte() {
        let written = base_project().write();
        let read = Project::read(&written).expect("what this build wrote, this build reads");
        let rewritten = read.write();
        assert_eq!(
            written, rewritten,
            "write -> read -> write has to be byte-equal, or a file moves every time it is opened"
        );
        // And a second pass is stable too (the first write normalises unknown-key positions).
        let again = Project::read(&rewritten).expect("reads").write();
        assert_eq!(rewritten, again);
        // The file says what it is, before anything else in it.
        assert!(
            written.starts_with("{\n  \"formatVersion\": 1,"),
            "{written:?}"
        );
        assert!(written.contains(&format!("\"kind\": \"{KIND}\"")));
        assert!(written.ends_with("\n"));
    }

    /// Issue #79 F2, at the level of the carry itself: every level of an opened file - the custom
    /// records and their own entries included - is handed to a freshly built project. The app's save
    /// path calls this (`files::snapshot_text`); that module's open -> save test proves the wiring.
    #[test]
    fn carry_unknowns_keeps_every_level_the_opened_file_carried() {
        let mut value: Value = serde_json::from_str(&base_project().write()).expect("parses");
        let plant = |value: &mut Value, path: &[&str], key: &str| {
            let mut at = value;
            for step in path {
                at = match step.parse::<usize>() {
                    Ok(index) => at.get_mut(index).expect("the path exists"),
                    Err(_) => at.get_mut(*step).expect("the path exists"),
                };
            }
            at.as_object_mut()
                .expect("the path lands on an object")
                .insert(key.to_string(), json!({ "kept": true, "n": 7 }));
        };
        let levels: &[&[&str]] = &[
            &[],
            &["savedBy"],
            &["engine"],
            &["duty"],
            &["site"],
            &["fillLayers", "0"],
            &["fitted"],
            &["customParts", "0"],
            &["customParts", "0", "values", "0"],
            &["results"],
            &["results", "headlines", "0"],
            &["results", "layers", "0"],
        ];
        for (index, path) in levels.iter().enumerate() {
            plant(&mut value, path, &format!("fromTheFuture{index}"));
        }
        let text = serde_json::to_string_pretty(&value).expect("serialises") + "\n";

        let opened = Project::read(&text).expect("a file with unknown keys still reads");
        let mut fresh = base_project();
        fresh.carry_unknowns(&opened);
        let written = fresh.write();

        let at: Value = serde_json::from_str(&written).expect("the carry writes JSON");
        for (index, path) in levels.iter().enumerate() {
            let mut step_at = &at;
            for step in *path {
                step_at = match step.parse::<usize>() {
                    Ok(index) => step_at.get(index).expect("the path still exists"),
                    Err(_) => step_at.get(*step).expect("the path still exists"),
                };
            }
            assert_eq!(
                step_at.get(format!("fromTheFuture{index}")),
                Some(&json!({ "kept": true, "n": 7 })),
                "`fromTheFuture{index}` at {path:?} was dropped by carry_unknowns"
            );
        }
    }

    /// The forward-compatibility rule, proved at every level of the file rather than asserted in prose.
    #[test]
    fn unknown_fields_survive_a_round_trip_at_every_level() {
        let mut value: Value = serde_json::from_str(&base_project().write()).expect("parses");
        let plant = |value: &mut Value, path: &[&str], key: &str, planted: Value| {
            let mut at = value;
            for step in path {
                at = match step.parse::<usize>() {
                    Ok(index) => at.get_mut(index).expect("the path exists"),
                    Err(_) => at.get_mut(*step).expect("the path exists"),
                };
            }
            at.as_object_mut()
                .expect("the path lands on an object")
                .insert(key.to_string(), planted);
        };
        let levels: &[(&[&str], &str)] = &[
            (&[], "fromTheFuture"),
            (&["savedBy"], "fromTheFuture"),
            (&["engine"], "fromTheFuture"),
            (&["duty"], "fromTheFuture"),
            (&["site"], "fromTheFuture"),
            (&["fillLayers", "0"], "fromTheFuture"),
            (&["fitted"], "fromTheFuture"),
            (&["customParts", "0"], "fromTheFuture"),
            (&["customParts", "0", "values", "0"], "fromTheFuture"),
            (&["results"], "fromTheFuture"),
            (&["results", "headlines", "0"], "fromTheFuture"),
            (&["results", "layers", "0"], "fromTheFuture"),
        ];
        for (path, key) in levels {
            plant(&mut value, path, key, json!({"kept": true, "n": 7}));
        }
        let text = serde_json::to_string_pretty(&value).expect("serialises") + "\n";

        let project = Project::read(&text).expect("a file with unknown keys still reads");
        let written = project.write();
        let reread = Project::read(&written).expect("and reads again");
        assert_eq!(
            written,
            reread.write(),
            "unknown keys do not move on a re-write"
        );

        // Every planted key is still there, with its value intact.
        let at: &Value = &serde_json::from_str(&written).expect("parses");
        let mut seen = Vec::new();
        for (path, key) in levels {
            let mut step_at = at;
            for step in *path {
                step_at = match step.parse::<usize>() {
                    Ok(index) => step_at.get(index).expect("the path still exists"),
                    Err(_) => step_at.get(*step).expect("the path still exists"),
                };
            }
            let kept = step_at.get(*key).unwrap_or_else(|| {
                panic!("`{key}` at {path:?} was dropped by the round trip");
            });
            assert_eq!(kept, &json!({"kept": true, "n": 7}), "`{key}` at {path:?}");
            seen.push(format!("{path:?}.{key}"));
        }
        assert_eq!(seen.len(), levels.len());
        let _ = at;
        let reparsed: Value = serde_json::from_str(&written).expect("the re-write is still JSON");
        assert!(reparsed.is_object());
    }

    /// The snapshot is the engine's own eleven metrics (names, units and values), not a re-typed list.
    #[test]
    fn the_snapshot_carries_the_engine_s_own_eleven_headlines() {
        let engine = fixture();
        let input = engine.default_input();
        let output = engine.run(&input).expect("runs");
        let project = base_project();
        assert_eq!(project.results.note, RESULTS_NOTE);
        assert_eq!(project.results.engine, output.provenance.engine);
        assert_eq!(project.results.headlines.len(), 11);
        for (row, (headline, value)) in project
            .results
            .headlines
            .iter()
            .zip(HEADLINES.iter().zip(output.headline_values()))
        {
            assert_eq!(row.metric, headline.name);
            assert_eq!(row.unit, headline.unit);
            assert_eq!(row.value, value, "`{}`", row.metric);
        }
        assert_eq!(
            project.results.layers.len(),
            input.fill_layers.len(),
            "one results row per declared layer"
        );
        for (row, layer) in project.results.layers.iter().zip(&output.kavl_per_layer) {
            assert_eq!(row.index, layer.index);
            assert_eq!(row.fill_id, layer.fill_id);
            assert_eq!(row.depth_m, layer.depth_m);
            assert_eq!(row.kavl, layer.kavl);
            assert_eq!(row.inside_envelope, layer.inside_envelope);
        }
        // The file, as text, carries the label the owner asked for - verbatim.
        let written = project.write();
        assert!(
            written.contains("as calculated on save - recomputed on open"),
            "the snapshot is labelled in the file itself"
        );
    }

    /// "Recomputed on open" is a measurable claim: the same run agrees, a moved metric does not.
    #[test]
    fn the_snapshot_is_measured_against_the_live_run() {
        let engine = fixture();
        let input = engine.default_input();
        let output = engine.run(&input).expect("runs");
        let project = base_project();

        let same = project.results.delta(&output);
        assert!(same.agrees(), "{}", same.line());
        assert_eq!(same.metrics, 11);
        assert_eq!(same.max_rel, 0.0);
        assert!(
            same.line().contains("match the saved snapshot"),
            "{}",
            same.line()
        );

        // Move one headline metric and the delta lands on it, by name.
        let mut moved = output.clone();
        moved.capability_pct += 0.5;
        let delta = project.results.delta(&moved);
        assert!(!delta.agrees());
        assert_eq!(delta.metric, "capability");
        assert!(delta.max_rel > 0.0);
        assert!(delta.line().contains("capability"), "{}", delta.line());
        assert!(delta.line().contains("differs by"), "{}", delta.line());
    }

    /// A project hands its duty, its stack, its ratio and its fitted ids back to a working input.
    #[test]
    fn a_project_carries_the_machine_back_into_an_input() {
        let engine = fixture();
        let mut input = engine.default_input();
        input.duty.water_flow_m3_hr = 640.0;
        input.duty.hot_water_c = 40.0;
        input.duty.wet_bulb_c = 24.0;
        input.duty.water_quality_class = "clean".to_string();
        input.speed_ratio = 0.71;
        input.fill_layers = vec![
            FillLayer {
                fill_id: "FILM-VF38".into(),
                depth_m: 1.5,
                thermal_multiplier: 1.02,
                pressure_multiplier: 0.98,
            },
            FillLayer {
                fill_id: "FILM-WF25".into(),
                depth_m: 0.45,
                thermal_multiplier: 1.0,
                pressure_multiplier: 1.0,
            },
        ];
        input.fan.id = "AX-500".into();
        input.drift.id = "DE-150".into();
        input.nozzle.id = "NZ-24".into();
        let output = engine.run(&input).expect("runs");
        let project = Project::build(Snapshot {
            host: "web-internal",
            input: &input,
            catalog_revision_id: crate::revision::SHIPPED_ID,
            engine_name: engine.name(),
            custom_parts: &[],
            water_quality_recorded: recorded(&spec()),
            limits_recorded: limits(&spec()),
            output: Some(&output),
        });

        // Every duty field the file carries is written back, and the ids come back as ids.
        assert_eq!(project.duty.water_flow_m3_hr, 640.0);
        assert_eq!(project.duty.water_quality_class, "clean");
        assert_eq!(project.fitted.fan, "AX-500");
        assert_eq!(project.fitted.drift, "DE-150");
        assert_eq!(project.fitted.nozzle, "NZ-24");
        assert_eq!(project.tower, input.tower.id);
        assert_eq!(project.catalog_revision_id, crate::revision::SHIPPED_ID);
        assert_eq!(project.saved_by.host, "web-internal");
        assert_eq!(project.format_version, FORMAT_VERSION);
        assert_eq!(project.engine.version, VERSION);
        assert_eq!(project.engine.adapter_contract_sha256, contract_sha256());

        let mut fresh = engine.default_input();
        let read = Project::read(&project.write()).expect("reads");
        read.write_into(&mut fresh);
        assert_eq!(fresh.duty, input.duty, "the nine duty fields, all of them");
        assert_eq!(fresh.fill_layers, input.fill_layers, "the ordered stack");
        assert_eq!(fresh.speed_ratio, input.speed_ratio);
        assert_eq!(
            stack_ids(&read),
            vec!["FILM-VF38", "FILM-WF25"],
            "stack order"
        );

        // The site block names the rows that are recorded rather than edited, and marks the derived one.
        assert!(read.site.altitude_from.contains("ISA"));
        assert!(read.site.altitude_from.contains("derived"));
        assert!(read.site.water_quality_recorded.contains_key("tds"));
        assert_eq!(read.site.limits_recorded.len(), 5);
    }

    fn stack_ids(project: &Project) -> Vec<&str> {
        project
            .fill_layers
            .iter()
            .map(|layer| layer.fill_id.as_str())
            .collect()
    }

    /// A custom record is a **full record** in the file: marked `custom`, with its provenance note, and it
    /// comes back through the form's own value model with every value shape intact.
    #[test]
    fn a_custom_part_is_a_full_record_in_the_file() {
        let part = every_shape();
        let project = base_project();
        assert_eq!(project.custom_parts.len(), 1);
        let block = &project.custom_parts[0];
        assert_eq!(block.record, "custom");
        assert_eq!(block.provenance, CUSTOM_PROVENANCE);
        assert!(block.provenance.contains("not vendor data"));
        assert_eq!(block.class, "fan");

        let written = project.write();
        assert!(written.contains("\"record\": \"custom\""), "{written}");
        assert!(written.contains("provenance"), "{written}");
        let read = Project::read(&written).expect("reads");
        let back = read.custom_parts().expect("the custom records come back");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0], part, "every value shape survives the file");

        // A record that is not marked custom is a reference, not a copy, and is refused as one.
        let mut not_custom = block.clone();
        not_custom.record = "catalog".into();
        let error = not_custom.to_custom().expect_err("refused");
        assert!(error.contains("catalog"), "{error}");
        assert!(error.contains("custom"), "{error}");
    }

    /// The schema version is checked, not ignored.
    #[test]
    fn a_file_from_a_newer_build_is_refused_by_name() {
        let text = base_project()
            .write()
            .replace("\"formatVersion\": 1", "\"formatVersion\": 99");
        let error = Project::read(&text).expect_err("a newer version is refused");
        assert!(error.contains("formatVersion 99"), "{error}");
        assert!(error.contains("newer"), "{error}");
        let error = Project::read(&text.replace("\"formatVersion\": 99,", ""))
            .expect_err("a file with no version is refused");
        assert!(error.contains("formatVersion"), "{error}");
    }

    /// The two exports: JSON for a machine, CSV for a spreadsheet, both carrying all eleven metrics and
    /// every layer row.
    #[test]
    fn the_exports_carry_every_headline_and_every_layer() {
        let engine = fixture();
        let input = engine.default_input();
        let output = engine.run(&input).expect("runs");

        let json_text = results_json(&output);
        let parsed: Value = serde_json::from_str(&json_text).expect("the JSON export parses");
        let headlines = parsed["headlines"].as_array().expect("headlines");
        assert_eq!(headlines.len(), 11);
        assert_eq!(headlines[0]["metric"], "capability");
        assert_eq!(
            parsed["layers"].as_array().expect("layers").len(),
            input.fill_layers.len()
        );
        assert_eq!(parsed["note"], RESULTS_NOTE);
        assert!(parsed["byZone"].as_array().is_some_and(|z| !z.is_empty()));

        let csv_text = results_csv(&output);
        let lines: Vec<&str> = csv_text.lines().collect();
        assert_eq!(lines[0], "section,key,unit,value");
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("headline,")).count(),
            11
        );
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("layer[0],")).count(),
            6
        );
        assert!(csv_text.contains("headline,capability,%,"));
        assert!(csv_text.contains("insideEnvelope,-,"));

        // A user-typed id with a comma in it is quoted, not split into two columns.
        let mut custom = output.clone();
        custom.provenance.engine = "engine, with a comma".into();
        assert!(results_json(&custom).contains("engine, with a comma"));
        let csv = results_csv(&custom);
        assert!(
            lines.iter().all(|l| l.split(',').count() == 4),
            "{csv_text}"
        );
        assert!(csv.lines().all(|l| l.split(',').count() == 4), "{csv}");
    }
}
