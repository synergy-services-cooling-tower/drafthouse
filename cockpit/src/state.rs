//! What the instrument edits, what a drop does, and how a URL stages a frame.
//!
//! The **draft** ([`cockpit::engine::EngineInput`]) is the single source of truth: the rpm control writes
//! `speed_ratio`, a fan drop replaces `fan`, a fill drop replaces a `fill_layers` entry. Nothing in this
//! module computes a cooling-tower quantity - a drop either finds the record in the fixture catalog or is
//! refused with the catalog's own reason.

use bevy::prelude::*;
use bevy_egui::egui;
use cockpit::engine::{
    DriftRecord, Engine, EngineError, EngineInput, EngineOutput, FanRecord, FillLayer,
    NozzleRecord, TowerRecord,
};
use cockpit::fixture_engine::{FixtureEngine, FixtureFill};
use serde::Deserialize;

use drafthouse_cockpit_seams::custom::{CustomPart, Fields};
use drafthouse_cockpit_seams::duty::DutySpec;

use crate::theme as t;

/// The generated custom-part field list (`tools/gen-custom-fields.mjs`, from `../fixtures-fields.json` +
/// the bundled catalog). Embedded, not fetched: it is a build-time artefact and the seams crate's tests
/// pin it, so an app shipped with a stale copy fails `cargo test` rather than the user's browser.
pub const CUSTOM_FIELDS_JSON: &str = include_str!("../assets/custom-fields.json");
/// The generated duty & site descriptor (`tools/gen-duty.mjs`, from `../fixtures/engine-run.json`).
pub const DUTY_FIELDS_JSON: &str = include_str!("../assets/duty-fields.json");

/// The custom-part field list, or the reason it could not be read. A parse failure is shown in the rail
/// (the `+ custom` chip goes quiet) instead of panicking the wasm module.
#[derive(Resource, Clone, Default)]
pub struct FieldsRes {
    pub fields: Option<Fields>,
    pub error: Option<String>,
}

impl FieldsRes {
    pub fn load() -> Self {
        match Fields::from_json(CUSTOM_FIELDS_JSON) {
            Ok(fields) => Self {
                fields: Some(fields),
                error: None,
            },
            Err(e) => Self {
                fields: None,
                error: Some(e),
            },
        }
    }
    pub fn count(&self) -> usize {
        self.fields.as_ref().map(|f| f.field_count()).unwrap_or(0)
    }
}

/// The duty & site descriptor (`viz::duty_panel` owns the panel; this is where it is parsed).
#[derive(Resource, Clone, Default)]
pub struct DutySpecRes {
    pub spec: Option<DutySpec>,
    pub error: Option<String>,
}

impl DutySpecRes {
    pub fn load() -> Self {
        match DutySpec::from_json(DUTY_FIELDS_JSON) {
            Ok(spec) => Self {
                spec: Some(spec),
                error: None,
            },
            Err(e) => Self {
                spec: None,
                error: Some(e),
            },
        }
    }
}

/// Required copy, verbatim (from the seam registry so the doc and the app cannot drift).
pub const REQUIRED_COPY: &str = drafthouse_cockpit_seams::REQUIRED_COPY;

/// How many fill layers the stack editor will hold. The tower record's depth options decide the domains
/// (this is a UI cap, not a physical limit).
pub const MAX_LAYERS: usize = 4;
/// How many nozzle symbols the scene draws (the label carries the true count).
pub const MAX_NOZZLES_DRAWN: usize = 8;

/// Round 2, change C: how many cells the 3D view can be configured with (`1..=8`, default 2). This is a
/// **UI count** - the fixture records describe one cell, and nothing here is a thermodynamic input.
pub const MIN_CELLS: u32 = 1;
pub const MAX_CELLS: u32 = 8;
pub const DEFAULT_CELLS: u32 = 2;

// ------------------------------------------------------------------------------------------- views

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    /// The instrument: the 2D section + rail + dock (round 1 called this the cockpit).
    #[default]
    Cockpit,
    /// The operating point on the fan/system curve and the performance curve.
    Curves,
    /// The data seams the pass binds to.
    Seams,
    /// Round 2's 3D build/orientation view. Round 4, item 1 took it out of the UI and put the whole module
    /// behind the `three-d` cargo feature (off by default, so the default build links no `bevy_pbr`); with
    /// the feature on, the tab, the CELLS controls and the `4` / `C` keys come back with it.
    #[cfg(feature = "three-d")]
    Three,
}

impl View {
    pub fn name(self) -> &'static str {
        match self {
            View::Cockpit => "Instrument",
            View::Curves => "Operating point",
            View::Seams => "Data seams",
            #[cfg(feature = "three-d")]
            View::Three => "3D tower",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            View::Cockpit => "cockpit",
            View::Curves => "curves",
            View::Seams => "seams",
            #[cfg(feature = "three-d")]
            View::Three => "3d",
        }
    }
    pub fn from_slug(s: &str) -> Option<View> {
        View::ALL.iter().copied().find(|v| v.slug() == s)
    }
    /// The views the header offers, in tab order. Three of them by default.
    pub const ALL: &'static [View] = &[
        View::Cockpit,
        View::Curves,
        View::Seams,
        #[cfg(feature = "three-d")]
        View::Three,
    ];
}

/// Which overlay the frame emphasises. Used by the evidence staging and by the "focus" buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Focus {
    #[default]
    All,
    Airflow,
    Pressure,
    Nozzle,
    Fan,
}

impl Focus {
    pub fn slug(self) -> &'static str {
        match self {
            Focus::All => "all",
            Focus::Airflow => "airflow",
            Focus::Pressure => "pressure",
            Focus::Nozzle => "nozzle",
            Focus::Fan => "fan",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Focus::All => "all overlays",
            Focus::Airflow => "flow map",
            Focus::Pressure => "pressure zones",
            Focus::Nozzle => "spray coverage",
            Focus::Fan => "fan / operating point",
        }
    }
    pub const ALL_SLUGS: [&'static str; 5] = ["all", "airflow", "pressure", "nozzle", "fan"];

    pub fn from_slug(s: &str) -> Option<Focus> {
        [
            Focus::All,
            Focus::Airflow,
            Focus::Pressure,
            Focus::Nozzle,
            Focus::Fan,
        ]
        .into_iter()
        .find(|f| f.slug() == s)
    }
}

// ------------------------------------------------------------------------------------------- parts

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Fan,
    Drift,
    Fill,
    Nozzle,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Fan => "fan",
            Class::Drift => "drift eliminator",
            Class::Fill => "fill",
            Class::Nozzle => "nozzle",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Class::Fan => "fan",
            Class::Drift => "drift",
            Class::Fill => "fill",
            Class::Nozzle => "nozzle",
        }
    }
    pub fn from_slug(s: &str) -> Option<Class> {
        [Class::Fan, Class::Drift, Class::Fill, Class::Nozzle]
            .into_iter()
            .find(|c| c.slug() == s)
    }
    pub fn tray_title(self) -> &'static str {
        match self {
            Class::Fan => "Fans",
            Class::Drift => "Drift eliminators",
            Class::Fill => "Fill",
            Class::Nozzle => "Nozzle bank",
        }
    }
    /// The slot this class belongs in. One class, one bay.
    pub fn slot(self) -> Slot {
        match self {
            Class::Fan => Slot::Fan,
            Class::Drift => Slot::Drift,
            Class::Fill => Slot::Fill,
            Class::Nozzle => Slot::Nozzle,
        }
    }
    pub const ALL: [Class; 4] = [Class::Fan, Class::Drift, Class::Fill, Class::Nozzle];
}

/// A bay in the section a part can be dropped into.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Slot {
    #[default]
    Fan,
    Drift,
    Fill,
    Nozzle,
}

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Fan => "fan",
            Slot::Drift => "drift eliminator",
            Slot::Fill => "fill stack",
            Slot::Nozzle => "nozzle bank",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Slot::Fan => "fan",
            Slot::Drift => "drift",
            Slot::Fill => "fill",
            Slot::Nozzle => "nozzle",
        }
    }
    pub fn class(self) -> Class {
        match self {
            Slot::Fan => Class::Fan,
            Slot::Drift => Class::Drift,
            Slot::Fill => Class::Fill,
            Slot::Nozzle => Class::Nozzle,
        }
    }
    pub const ALL: [Slot; 4] = [Slot::Fan, Slot::Drift, Slot::Fill, Slot::Nozzle];
}

/// The payload carried by a drag (or by a click-select, which places by tap).
/// Round 3: what the charts drew, so an evidence frame can assert the charts' own numbers instead of
/// reading the read-outs beside them. Written by the chart code, published by the bridge.
#[derive(Resource, Default)]
pub struct ChartStats {
    /// The flow-family lines drawn in the wet-bulb chart (0 when the fixture carries no such family).
    pub wb_lines: usize,
    /// The recorded sweep points drawn in the wet-bulb chart.
    pub wb_pts: usize,
    /// The points drawn in the KaV/L vs L/G chart.
    pub kavl_pts: usize,
    /// The fan/system operating point the chart *marked* (`operating_point.y`), in Pa.
    pub fan_pa: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartRef {
    pub class: Class,
    pub id: String,
}

impl PartRef {
    pub fn new(class: Class, id: impl Into<String>) -> Self {
        Self {
            class,
            id: id.into(),
        }
    }
    /// The `class:id` form used by `?drag=` / `?drop=`.
    pub fn slug(&self) -> String {
        format!("{}:{}", self.class.slug(), self.id)
    }
    pub fn parse(s: &str) -> Option<PartRef> {
        let (c, id) = s.split_once(':')?;
        Some(PartRef::new(Class::from_slug(c)?, id))
    }
}

/// A live drag: the payload, the bay it is over, and what the catalog says about that pairing.
#[derive(Clone, Debug)]
pub struct Drag {
    pub part: PartRef,
    pub over: Option<Slot>,
    /// `Ok(())` = the bay would accept it; `Err(reason)` = the catalog refuses, and the reason is shown.
    pub verdict: Result<(), String>,
    /// True when a URL staged this drag rather than a pointer (evidence frames say which).
    pub staged: bool,
}

#[derive(Clone, Debug)]
pub struct Flash {
    pub ok: bool,
    pub text: String,
}

/// Round 2, change A: the bay-tap picker. One per app, anchored beside the bay it belongs to (a bottom
/// sheet on a phone). It lists *only* that bay's catalog cards, and picking one is a replace - the same
/// `apply_drop` the tray drag used to reach.
#[derive(Clone, Debug, PartialEq)]
pub struct Picker {
    pub slot: Slot,
    /// The fill layer a fill picker would replace (ignored for the other bays).
    pub layer: usize,
    /// The row the keyboard is on.
    pub row: usize,
    /// The bay's rect in canvas points, so the picker can sit beside it.
    pub anchor: [f32; 4],
    /// True when it was opened from the keyboard (the frame proves the affordance path).
    pub from_keyboard: bool,
}

impl Picker {
    pub fn new(slot: Slot, layer: usize, anchor: [f32; 4], from_keyboard: bool) -> Self {
        Self {
            slot,
            layer,
            row: 0,
            anchor,
            from_keyboard,
        }
    }
}

// --------------------------------------------------------------------------------------- the state

/// UI state that is *not* an engine input: selection, drag, nozzle arrangement (not modelled yet), grid.
#[derive(Resource, Clone, Debug)]
pub struct Visual {
    pub view: View,
    pub focus: Focus,
    pub selected_slot: Slot,
    pub selected_layer: usize,
    /// The fixture's own default speed ratio - what the "fixture 0.78" button restores.
    pub reset_ratio: f64,
    /// The part the detail strip describes and a bay click would place.
    pub selected_part: Option<PartRef>,
    /// A record the rail should bring into view once (a custom record the user just saved, which is at the
    /// far end of its own section and may be under the fold). One-shot: the chip clears it when it draws.
    pub scroll_to: Option<String>,
    pub drag: Option<Drag>,
    pub flash: Option<Flash>,
    /// Nozzle arrangement: an editor value, explicitly not an engine input (see the seams panel).
    pub nozzle_spacing_m: f64,
    pub nozzle_pattern: Pattern,
    pub grid: bool,
    pub seams_detail: bool,
    /// Round 2, change A: the open bay-tap picker (at most one).
    pub picker: Option<Picker>,
    /// The picker's last drawn rect in canvas points (`[x, y, w, h]`), so a click inside it is never read as
    /// a click on the bay behind it. Zero when the picker is closed.
    pub picker_rect: [f32; 4],
    /// True when the picker is drawn as the phone bottom sheet (published as `data-picker-sheet`).
    pub picker_sheet: bool,
    /// Round 2, change A: the bay the keyboard is on (`B` cycles, `Enter` opens its picker).
    pub bay_focus: Option<Slot>,
    /// Round 2, change C: the 3D view's cell count (a UI count - see [`DEFAULT_CELLS`]).
    pub cells: u32,
    /// Round 2, change C: is the focus cell cut away?
    pub cutaway: bool,
    /// Round 2, change C: which cell is cut (0-based, clamped to `cells`).
    pub focus_cell: usize,
    /// Round 2, change C: the orbit camera - yaw/pitch in degrees, distance in tower-widths.
    pub cam_yaw: f32,
    pub cam_pitch: f32,
    pub cam_dist: f32,
    /// Round 2, change C: a tap in the 3D viewport, in viewport-local points (`[x, y, w, h]`), for the 3D
    /// crate to resolve into a cell or a bay. Taken (set to `None`) by the resolver.
    pub three_click: Option<[f32; 4]>,
    /// Round 3, item 1: the parts rail - open (chips) or collapsed to the 28 px icon strip. Desktop only;
    /// a phone reaches parts through the bay-tap picker's bottom sheet, exactly as in round 2.
    pub rail_open: bool,
    /// Round 3, item 7: the section's honesty legend. Moved out of the fan bay's corner, dismissible, and
    /// the state is remembered for the session (`?legend=0` proves it).
    pub legend_open: bool,
    /// Round 5, item 1: the duty & site panel's three collapsible sections. DUTY is open by default; WATER
    /// QUALITY and LIMITS are collapsed, each showing its own one-line summary in the header row. The panel
    /// is a ~20-row form, and at 1440x900 the read-out below it has to stay on screen.
    pub duty_open: bool,
    pub water_open: bool,
    pub limits_open: bool,
}

/// The default three-quarter camera. The number lives in the seam mapping, where every invented value the
/// pass uses is unit-tested (`seams/src/mapping.rs`); this is the alias the UI reads.
pub const CAM_DEFAULT: (f32, f32, f32) = drafthouse_cockpit_seams::mapping::CAM_DEFAULT;

impl Default for Visual {
    fn default() -> Self {
        Self {
            view: View::Cockpit,
            focus: Focus::All,
            selected_slot: Slot::Fan,
            selected_layer: 0,
            scroll_to: None,
            selected_part: None,
            reset_ratio: 0.78,
            drag: None,
            flash: None,
            nozzle_spacing_m: 1.0,
            nozzle_pattern: Pattern::SingleRow,
            grid: true,
            seams_detail: false,
            picker: None,
            picker_rect: [0.0; 4],
            picker_sheet: false,
            bay_focus: None,
            cells: DEFAULT_CELLS,
            cutaway: false,
            focus_cell: 0,
            cam_yaw: CAM_DEFAULT.0,
            cam_pitch: CAM_DEFAULT.1,
            cam_dist: CAM_DEFAULT.2,
            three_click: None,
            rail_open: true,
            legend_open: true,
            // Round 5, item 1: the owner's default - DUTY open, WATER QUALITY and LIMITS collapsed.
            duty_open: true,
            water_open: false,
            limits_open: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pattern {
    #[default]
    SingleRow,
    Staggered,
}

impl Pattern {
    pub fn name(self) -> &'static str {
        match self {
            Pattern::SingleRow => "single row",
            Pattern::Staggered => "staggered",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Pattern::SingleRow => "single-row",
            Pattern::Staggered => "staggered",
        }
    }
    pub fn from_slug(s: &str) -> Option<Pattern> {
        [Pattern::SingleRow, Pattern::Staggered]
            .into_iter()
            .find(|p| p.slug() == s)
    }
}

// ----------------------------------------------------------------------------------------- catalog

/// The fixture catalog the tray and the drop rules read. Built from the same `FixtureFile` the engine was
/// constructed from - one fixture, one crate.
#[derive(Resource, Clone, Debug, Default)]
pub struct Catalog {
    pub towers: Vec<TowerRecord>,
    pub fills: Vec<FixtureFill>,
    pub drifts: Vec<DriftRecord>,
    pub fans: Vec<FanRecord>,
    pub nozzles: Vec<NozzleRecord>,
    /// Round 4, item 2: the records the user authored in **this session**, in save order. They are appended
    /// to the typed arrays above as well, which is what makes a custom record take exactly the same path as
    /// a catalog card (the rail, the picker, `check_drop`, `apply_drop`, `Engine::run`); this list is what
    /// marks them `custom` and what the parameter card reads.
    pub custom: Vec<CustomPart>,
    pub catalog_id: String,
    pub catalog_revision: String,
    pub catalog_status: String,
    /// `anchor.air.inlet` - the recorded inlet psychrometrics (the engine's output contract does not carry
    /// them yet; see the `ambient.inlet_air` seam).
    pub ambient: Option<Ambient>,
}

impl Catalog {
    pub fn from_fixture(fx: &FixtureEngine) -> Self {
        let c = &fx.fixture.catalog;
        Self {
            // Round 4: the session's own records start empty - the fixture catalog is the fixture's.
            custom: Vec::new(),
            towers: c.towers.clone(),
            fills: c.fills.clone(),
            drifts: c.drift_eliminators.clone(),
            fans: c.fans.clone(),
            nozzles: c.nozzles.clone(),
            catalog_id: c.metadata.id.clone(),
            catalog_revision: c.metadata.revision.clone(),
            catalog_status: c.metadata.status.clone(),
            ambient: None,
        }
    }

    pub fn fill(&self, id: &str) -> Option<&FixtureFill> {
        self.fills.iter().find(|f| f.record.id == id)
    }
    pub fn fan(&self, id: &str) -> Option<&FanRecord> {
        self.fans.iter().find(|f| f.id == id)
    }
    pub fn drift(&self, id: &str) -> Option<&DriftRecord> {
        self.drifts.iter().find(|d| d.id == id)
    }
    pub fn nozzle(&self, id: &str) -> Option<&NozzleRecord> {
        self.nozzles.iter().find(|n| n.id == id)
    }
    pub fn tower(&self, id: &str) -> Option<&TowerRecord> {
        self.towers.iter().find(|x| x.id == id)
    }
    pub fn fill_ids(&self) -> Vec<String> {
        self.fills.iter().map(|f| f.record.id.clone()).collect()
    }

    /// Is this record one the user authored here (rather than a fixture catalog record)?
    pub fn is_custom(&self, class: Class, id: &str) -> bool {
        self.custom
            .iter()
            .any(|c| c.class == class.slug() && c.id() == id)
    }

    /// The custom record behind an id, when there is one.
    pub fn custom_part(&self, class: Class, id: &str) -> Option<&CustomPart> {
        self.custom
            .iter()
            .find(|c| c.class == class.slug() && c.id() == id)
    }

    /// Append a saved custom record: build the engine's own record type out of the values and put it in
    /// **both** catalogs - this view's and the engine's own `FixtureFile.catalog` (through the
    /// `Engine::fixture_catalog_mut` hook the baseline exposes for exactly this) - so every existing path
    /// sees it: the rail, the picker, `check_drop`, `apply_drop` and `Engine::run` itself.
    ///
    /// Refusals: an empty id, or an id already in this session's catalog for that class. Everything else was
    /// already checked by the form (shape + range) on the way here.
    pub fn add_custom(
        &mut self,
        part: &CustomPart,
        fields: &Fields,
        engine: Option<&mut (dyn Engine + '_)>,
    ) -> Result<String, String> {
        let class = Class::from_slug(&part.class)
            .ok_or_else(|| format!("{} is not a part class", part.class))?;
        let id = part.id();
        if id.trim().is_empty() {
            return Err("id is required".into());
        }
        if tray_ids(self, class).iter().any(|x| x == &id) {
            return Err(format!(
                "{id} is already in this session's {} catalog - ids are unique per class",
                class.name()
            ));
        }
        // Both catalogs, one record: the engine runs on its own copy, the view lists and validates on this
        // one, and a drop must be visible to both or the run would refuse the id it was just given.
        match class {
            Class::Fan => {
                let rec = part.to_fan(fields)?;
                if let Some(file) = engine.and_then(|e| e.fixture_catalog_mut()) {
                    file.catalog.fans.push(rec.clone());
                }
                self.fans.push(rec);
            }
            Class::Drift => {
                let rec = part.to_drift(fields)?;
                if let Some(file) = engine.and_then(|e| e.fixture_catalog_mut()) {
                    file.catalog.drift_eliminators.push(rec.clone());
                }
                self.drifts.push(rec);
            }
            Class::Fill => {
                let rec = part.to_fill(fields)?;
                if let Some(file) = engine.and_then(|e| e.fixture_catalog_mut()) {
                    file.catalog.fills.push(rec.clone());
                }
                self.fills.push(rec);
            }
            Class::Nozzle => {
                let rec = part.to_nozzle(fields)?;
                if let Some(file) = engine.and_then(|e| e.fixture_catalog_mut()) {
                    file.catalog.nozzles.push(rec.clone());
                }
                self.nozzles.push(rec);
            }
        }
        self.custom.push(part.clone());
        Ok(format!(
            "custom {} {id} added to the rail - session only, drag it onto its bay",
            class.name()
        ))
    }
}

/// The recorded inlet air (`anchor.air.inlet`).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ambient {
    pub dry_bulb_c: f64,
    pub wet_bulb_c: f64,
    pub relative_humidity: f64,
    pub humidity_ratio: f64,
}

impl Ambient {
    /// Read the inlet block out of the raw fixture text. `FixtureAnchor` (the baseline's schema) does not
    /// expose `air`, so this reads the one field the pass needs and leaves the rest alone.
    pub fn from_json(text: &str) -> Option<Ambient> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let inlet = v.get("anchor")?.get("air")?.get("inlet")?;
        let get = |k: &str| inlet.get(k).and_then(|x| x.as_f64());
        Some(Ambient {
            dry_bulb_c: get("dryBulbC")?,
            wet_bulb_c: get("wetBulbC")?,
            relative_humidity: get("relativeHumidity")?,
            humidity_ratio: get("humidityRatio")?,
        })
    }
}

// ------------------------------------------------------------------------------------- drop rules

/// Does the catalog accept `part` in `slot`? This is a compatibility lookup on fixture records - no
/// estimate, no fallback, no guess. Every refusal names the field that refused it.
pub fn check_drop(
    cat: &Catalog,
    input: &EngineInput,
    slot: Slot,
    part: &PartRef,
) -> Result<(), String> {
    if part.class.slot() != slot {
        return Err(format!(
            "the {} bay takes a {}; {} is a {}",
            slot.name(),
            slot.class().name(),
            part.id,
            part.class.name()
        ));
    }
    match slot {
        Slot::Fan => {
            let Some(fan) = cat.fan(&part.id) else {
                return Err(format!("{} is not in the fixture fan catalog", part.id));
            };
            if !input
                .tower
                .compatible_fan_ids
                .iter()
                .any(|id| id == &fan.id)
            {
                return Err(format!(
                    "tower {} lists compatible fans [{}] - {} is not one of them",
                    input.tower.id,
                    input.tower.compatible_fan_ids.join(", "),
                    fan.id
                ));
            }
            if fan.stack_area_m2 > input.tower.fill_area_m2 {
                return Err(format!(
                    "{} stack area {:.2} m2 exceeds the tower's {:.2} m2 fill area",
                    fan.id, fan.stack_area_m2, input.tower.fill_area_m2
                ));
            }
            Ok(())
        }
        Slot::Drift => {
            let Some(drift) = cat.drift(&part.id) else {
                return Err(format!("{} is not in the fixture drift catalog", part.id));
            };
            if drift.max_water_temperature_c < input.duty.hot_water_c {
                return Err(format!(
                    "{} is rated to {:.0} C; this duty runs hot water at {:.0} C",
                    drift.id, drift.max_water_temperature_c, input.duty.hot_water_c
                ));
            }
            Ok(())
        }
        Slot::Fill => {
            let Some(fill) = cat.fill(&part.id) else {
                return Err(format!("{} is not in the fixture fill catalog", part.id));
            };
            let rec = &fill.record;
            if !rec
                .compatible_tower_types
                .iter()
                .any(|x| x == &input.tower.tower_type)
            {
                return Err(format!(
                    "{} is a {} fill; tower {} is {}",
                    rec.id,
                    rec.compatible_tower_types.join("/"),
                    input.tower.id,
                    input.tower.tower_type
                ));
            }
            if !rec
                .allowed_water_quality_classes
                .iter()
                .any(|x| x == &input.duty.water_quality_class)
            {
                return Err(format!(
                    "{} allows water quality [{}]; this duty is {}",
                    rec.id,
                    rec.allowed_water_quality_classes.join(", "),
                    input.duty.water_quality_class
                ));
            }
            if rec.limits.max_water_temperature_c < input.duty.hot_water_c {
                return Err(format!(
                    "{} is rated to {:.0} C water; this duty is {:.0} C",
                    rec.id, rec.limits.max_water_temperature_c, input.duty.hot_water_c
                ));
            }
            Ok(())
        }
        Slot::Nozzle => {
            let Some(n) = cat.nozzle(&part.id) else {
                return Err(format!("{} is not in the fixture nozzle catalog", part.id));
            };
            if !(n.discharge_coefficient > 0.0 && n.orifice_diameter_m > 0.0) {
                return Err(format!("{} has no usable orifice record", n.id));
            }
            Ok(())
        }
    }
}

/// Apply an accepted drop to the engine input. Returns the line the detail strip shows, or the refusal.
///
/// Note what a drop does **not** do: it never invents a `thermal_multiplier` / `pressure_multiplier` (those
/// are the fixture's recorded correction factors, and recomputing them is engine work), and it never
/// changes a depth unless the caller asks. The strip says so in the frame.
pub fn apply_drop(
    cat: &Catalog,
    input: &mut EngineInput,
    slot: Slot,
    part: &PartRef,
    layer: usize,
) -> Result<String, String> {
    check_drop(cat, input, slot, part)?;
    match slot {
        Slot::Fan => {
            let fan = cat.fan(&part.id).expect("checked");
            let old = input.fan.id.clone();
            input.fan = fan.clone();
            let [lo, hi] = fan.allowed_speed_ratio;
            let clamped = input.speed_ratio.clamp(lo, hi);
            let note = if (clamped - input.speed_ratio).abs() > 1e-12 {
                input.speed_ratio = clamped;
                format!(
                    "fan {old} -> {} (record replaced; speed ratio clamped to {clamped:.2})",
                    fan.id
                )
            } else {
                format!("fan {old} -> {} (record replaced)", fan.id)
            };
            Ok(note)
        }
        Slot::Drift => {
            let drift = cat.drift(&part.id).expect("checked");
            let old = input.drift.id.clone();
            input.drift = drift.clone();
            Ok(format!(
                "drift eliminator {old} -> {} (record replaced)",
                drift.id
            ))
        }
        Slot::Fill => {
            let Some(l) = input.fill_layers.get_mut(layer) else {
                return Err(format!("no layer {} in the stack", layer + 1));
            };
            let old = l.fill_id.clone();
            l.fill_id = part.id.clone();
            Ok(format!(
                "layer {} fill {old} -> {} at {:.2} m (identity replaced; the recorded multipliers stay {:.2}/{:.2} - not recomputed here)",
                layer + 1,
                l.fill_id,
                l.depth_m,
                l.thermal_multiplier,
                l.pressure_multiplier
            ))
        }
        Slot::Nozzle => {
            let n = cat.nozzle(&part.id).expect("checked");
            let old = input.nozzle.id.clone();
            input.nozzle = n.clone();
            Ok(format!("nozzle bank {old} -> {} (record replaced)", n.id))
        }
    }
}

/// Reorder the stack: `up`/`down` swap a layer with its neighbour. Order is the engine input's order
/// (index 0 = top, water enters here first), so this is a real edit, not a drawing change.
pub fn move_layer(input: &mut EngineInput, index: usize, up: bool) -> Option<String> {
    let n = input.fill_layers.len();
    if n < 2 || index >= n {
        return None;
    }
    let target = if up {
        if index == 0 {
            return None;
        }
        index - 1
    } else {
        if index + 1 >= n {
            return None;
        }
        index + 1
    };
    input.fill_layers.swap(index, target);
    Some(format!(
        "layer {} moved {}",
        index + 1,
        if up { "up" } else { "down" }
    ))
}

/// Remove a layer (the stack keeps at least one).
pub fn remove_layer(input: &mut EngineInput, index: usize) -> Option<String> {
    if input.fill_layers.len() < 2 || index >= input.fill_layers.len() {
        return None;
    }
    let gone = input.fill_layers.remove(index);
    Some(format!("layer {} ({}) removed", index + 1, gone.fill_id))
}

/// Add a layer: the new layer takes the fill from `part` (or the last layer's fill) at a depth the tower
/// record offers, and the stack stays inside the tower's depth domain.
pub fn add_layer(
    cat: &Catalog,
    input: &mut EngineInput,
    part: Option<&PartRef>,
) -> Result<String, String> {
    if input.fill_layers.len() >= MAX_LAYERS {
        return Err(format!("the stack holds {MAX_LAYERS} layers"));
    }
    let fill_id = match part {
        Some(p) if p.class == Class::Fill => p.id.clone(),
        _ => input
            .fill_layers
            .last()
            .map(|l| l.fill_id.clone())
            .unwrap_or_else(|| "FILM-MF20".into()),
    };
    if let Some(f) = cat.fill(&fill_id) {
        if !f
            .record
            .compatible_tower_types
            .iter()
            .any(|x| x == &input.tower.tower_type)
        {
            return Err(format!(
                "{fill_id} is not a {} fill",
                input.tower.tower_type
            ));
        }
    }
    // Depth: the smallest depth the tower record offers - a fixture field, not a preference.
    let depths = &input.tower.fill_depth_options_m;
    let depth = depths.iter().cloned().fold(f64::INFINITY, f64::min);
    let (tm, pm) = input
        .fill_layers
        .last()
        .map(|l| (l.thermal_multiplier, l.pressure_multiplier))
        .unwrap_or((1.0, 1.0));
    input.fill_layers.push(FillLayer {
        fill_id: fill_id.clone(),
        depth_m: depth,
        thermal_multiplier: tm,
        pressure_multiplier: pm,
    });
    Ok(format!(
        "layer {} added: {fill_id} at {depth:.2} m (tower {})",
        input.fill_layers.len(),
        input.tower.id
    ))
}

// -------------------------------------------------------------------------------------- the run

/// The current engine result. Written by the UI pass (the fixture engine's run is arithmetic on the
/// recorded run - cheap enough to call while a slider moves), read by the scene and the mirror.
#[derive(Resource, Default)]
pub struct Run {
    pub output: Option<EngineOutput>,
    pub error: Option<EngineError>,
    /// The ratio the output belongs to, so a read-out can never mix two ratios.
    pub ratio: f64,
}

/// The recorded run: the fixture default input, run once at load. Every delta the instrument shows is
/// against this, and it is labelled as the anchor, never as "current".
#[derive(Resource, Default, Clone)]
pub struct Anchor {
    pub output: Option<EngineOutput>,
    pub ratio: f64,
}

// ---------------------------------------------------------------------------------------- staging

/// `?…` staging, so an evidence frame is a URL rather than a hand-driven session. Every staged field is
/// applied by calling the *same* function the pointer calls (see [`crate::app`]).
#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct StartOptions {
    pub host: cockpit::host::HostConfig,
    pub view: Option<String>,
    pub focus: Option<String>,
    pub rpm: Option<f64>,
    pub spacing: Option<f64>,
    pub pattern: Option<String>,
    pub drag: Option<String>,
    pub over: Option<String>,
    pub drop: Option<String>,
    #[serde(rename = "move")]
    pub shift: Option<String>,
    pub remove: Option<usize>,
    pub add: Option<String>,
    pub layer: Option<usize>,
    pub grid: Option<bool>,
    pub engine: Option<String>,
    /// `1` freezes the animations so two frames are byte-comparable.
    pub frozen: Option<bool>,
    pub reduced_motion: bool,
    // ---- round 2 ---------------------------------------------------------------------------------
    /// `?bay=<slot>` - the bay the keyboard is focused on (the affordance proof).
    pub bay: Option<String>,
    /// `?picker=<slot>` - open that bay's picker.
    pub picker: Option<String>,
    /// `?picker-row=<n>` - which row the picker's keyboard cursor is on.
    pub picker_row: Option<usize>,
    /// `?cells=<1..8>` - the 3D view's cell count.
    pub cells: Option<u32>,
    /// `?cutaway=1` - open the focus cell's cutaway.
    pub cutaway: Option<bool>,
    /// `?cell=<n>` - which cell is cut (0-based).
    pub cell: Option<usize>,
    /// `?cam=<yaw>,<pitch>,<dist>` - the orbit camera, for a reproducible 3D frame.
    pub cam: Option<String>,
    // ---- round 3 ---------------------------------------------------------------------------------
    /// `?rail=0|1` - the parts rail collapsed / open.
    pub rail: Option<bool>,
    /// `?legend=0|1` - the section's honesty legend dismissed / shown.
    pub legend: Option<bool>,
    // ---- round 4 ---------------------------------------------------------------------------------
    /// `?form=<class>` - open the custom-part form for that class (fan / drift / fill / nozzle).
    pub form: Option<String>,
    /// `?form-fields=key=value,key=value` - the texts the form's fields are seeded with.
    pub form_fields: Option<String>,
    /// `?form-rows=v,v,v;v,v,v` - the first point table's rows (the class's curve / characteristic field).
    pub form_rows: Option<String>,
    /// `?form-companion=<value>` - the companion scalar of that point table, when it has one.
    pub form_companion: Option<String>,
    /// `?form-save=1` - press Save (which refuses or saves, through the same call the button makes).
    pub form_save: Option<bool>,
    /// `?duty=key=value,key=value` - edit the working duty before the first run (`waterMassFlowKgS=200`).
    pub duty: Option<String>,
    /// `?hover=<class>:<id>&hover-long=1` - show the parameter card (the phone long-press path).
    pub hover: Option<String>,
    /// `?hover-bay=1` - the staged card is the *fitted bay's* card (the same record plus the engine rows);
    /// the pointer path sets this when it is over a bay, and a staged frame has to say it explicitly.
    #[serde(default)]
    pub hover_bay: Option<bool>,
    pub hover_long: Option<bool>,
    // ---- round 5 ---------------------------------------------------------------------------------
    /// `?duty-open=0|1` - the DUTY section of the duty & site panel expanded (default) / collapsed.
    pub duty_open: Option<bool>,
    /// `?water-open=0|1` - the WATER QUALITY section collapsed (default) / expanded.
    pub water_open: Option<bool>,
    /// `?limits-open=0|1` - the LIMITS section collapsed (default) / expanded.
    pub limits_open: Option<bool>,
}

// --------------------------------------------------------------------------------------- helpers

/// A short, engineer-readable description of a part - the tray card's second line.
pub fn part_spec(cat: &Catalog, part: &PartRef) -> String {
    match part.class {
        Class::Fan => cat
            .fan(&part.id)
            .map(|f| {
                format!(
                    "stack {:.2} m2 · ratio {:.2}-{:.2} · eta {:.2}",
                    f.stack_area_m2,
                    f.allowed_speed_ratio[0],
                    f.allowed_speed_ratio[1],
                    f.drive_efficiency * f.motor_efficiency
                )
            })
            .unwrap_or_else(|| "not in the fixture catalog".into()),
        Class::Drift => cat
            .drift(&part.id)
            .map(|d| {
                format!(
                    "max water {:.0} C · 3-point curve through {:.0} Pa at {:.1} m/s",
                    d.max_water_temperature_c,
                    d.curve.last().map(|p| p.pressure_drop_pa).unwrap_or(0.0),
                    d.curve.last().map(|p| p.face_velocity_m_s).unwrap_or(0.0)
                )
            })
            .unwrap_or_else(|| "not in the fixture catalog".into()),
        Class::Fill => cat
            .fill(&part.id)
            .map(|f| {
                format!(
                    "{} · {} · loading {:.1}-{:.1} kg/m2 s · max {:.0} C",
                    f.record.geometry,
                    f.record.compatible_tower_types.join("/"),
                    f.record.limits.min_water_loading_kg_m2_s,
                    f.record.limits.max_water_loading_kg_m2_s,
                    f.record.limits.max_water_temperature_c
                )
            })
            .unwrap_or_else(|| "not in the fixture catalog".into()),
        Class::Nozzle => cat
            .nozzle(&part.id)
            .map(|n| {
                format!(
                    "orifice {:.0} mm · Cd {:.2}",
                    n.orifice_diameter_m * 1000.0,
                    n.discharge_coefficient
                )
            })
            .unwrap_or_else(|| "not in the fixture catalog".into()),
    }
}

/// A one-line spec for a tray card: short enough not to overrun the card. The full spec belongs in the
/// detail strip, which has the width for it.
pub fn part_spec_short(cat: &Catalog, part: &PartRef) -> String {
    match part.class {
        Class::Fan => cat.fan(&part.id).map(|f| {
            format!(
                "{:.1} m2 stack · ratio {:.2}-{:.2}",
                f.stack_area_m2, f.allowed_speed_ratio[0], f.allowed_speed_ratio[1]
            )
        }),
        Class::Drift => cat
            .drift(&part.id)
            .map(|d| format!("max water {:.0} C", d.max_water_temperature_c)),
        Class::Fill => cat.fill(&part.id).map(|f| {
            format!(
                "{} · max {:.0} C",
                f.record.compatible_tower_types.join("/"),
                f.record.limits.max_water_temperature_c
            )
        }),
        Class::Nozzle => cat.nozzle(&part.id).map(|n| {
            format!(
                "orifice {:.0} mm · Cd {:.2}",
                n.orifice_diameter_m * 1000.0,
                n.discharge_coefficient
            )
        }),
    }
    .unwrap_or_else(|| "not in the fixture catalog".into())
}

/// The tray: every fixture part of a class, in catalog order.
pub fn tray_ids(cat: &Catalog, class: Class) -> Vec<String> {
    match class {
        Class::Fan => cat.fans.iter().map(|x| x.id.clone()).collect(),
        Class::Drift => cat.drifts.iter().map(|x| x.id.clone()).collect(),
        Class::Fill => cat.fills.iter().map(|x| x.record.id.clone()).collect(),
        Class::Nozzle => cat.nozzles.iter().map(|x| x.id.clone()).collect(),
    }
}

/// Which ids are currently in the machine - the tray marked them; the picker marks them.
pub fn in_use(cat: &Catalog, input: &EngineInput) -> Vec<(Class, String, String)> {
    let mut v = vec![
        (
            Class::Fan,
            input.fan.id.clone(),
            part_spec(cat, &PartRef::new(Class::Fan, input.fan.id.clone())),
        ),
        (
            Class::Drift,
            input.drift.id.clone(),
            part_spec(cat, &PartRef::new(Class::Drift, input.drift.id.clone())),
        ),
        (
            Class::Nozzle,
            input.nozzle.id.clone(),
            part_spec(cat, &PartRef::new(Class::Nozzle, input.nozzle.id.clone())),
        ),
    ];
    for l in &input.fill_layers {
        v.push((
            Class::Fill,
            l.fill_id.clone(),
            format!("{:.2} m in the stack", l.depth_m),
        ));
    }
    v
}

/// Status colour for a drop verdict, used by the tray, the bays and the strip.
pub fn verdict_color(v: &Result<(), String>) -> egui::Color32 {
    match v {
        Ok(()) => t::VALID,
        Err(_) => t::INVALID,
    }
}

// ---------------------------------------------------------------------------- round 2: the picker

/// The rows a bay's picker lists: **only that bay's class**, in catalog order, straight off the fixture.
pub fn picker_rows(cat: &Catalog, slot: Slot) -> Vec<PartRef> {
    tray_ids(cat, slot.class())
        .into_iter()
        .map(|id| PartRef::new(slot.class(), id))
        .collect()
}

/// The picker's heading: which bay it belongs to, and what a pick would replace.
pub fn picker_title(draft: Option<&EngineInput>, vis: &Visual) -> String {
    let slot = match vis.picker.as_ref() {
        Some(p) => p.slot,
        None => return String::new(),
    };
    let layer = vis.picker.as_ref().map(|p| p.layer).unwrap_or(0);
    match (slot, draft) {
        (Slot::Fan, Some(d)) => format!("fan bay · now {}", d.fan.id),
        (Slot::Drift, Some(d)) => format!("drift bay · now {}", d.drift.id),
        (Slot::Nozzle, Some(d)) => format!("nozzle bay · now {}", d.nozzle.id),
        (Slot::Fill, Some(d)) => match d.fill_layers.get(layer) {
            Some(l) => format!(
                "fill layer {} · now {} {:.2} m",
                layer + 1,
                l.fill_id,
                l.depth_m
            ),
            None => format!("fill layer {}", layer + 1),
        },
        (s, _) => format!("{} bay", s.name()),
    }
}

/// Which texture family a drawn fill layer gets. Read from the fixture record's own `geometry` string -
/// the pass does not invent a material, it draws the one the catalog names, in two visual families.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FillTexture {
    /// Fine vertical channels (film families).
    Film,
    /// Coarser horizontal flute rows (the wide-flute, bottom-supported layer).
    WideFlute,
    /// A grid of criss-cross bars.
    Grid,
    /// A splash deck: scattered splash bars.
    Splash,
    Unknown,
}

pub fn fill_texture(cat: Option<&Catalog>, fill_id: &str) -> FillTexture {
    let g = cat
        .and_then(|c| c.fill(fill_id))
        .map(|f| f.record.geometry.to_lowercase())
        .unwrap_or_default();
    if g.contains("wide-flute") || g.contains("bottom-supported") {
        FillTexture::WideFlute
    } else if g.contains("trickle") || g.contains("grid") {
        FillTexture::Grid
    } else if g.contains("splash") {
        FillTexture::Splash
    } else if g.contains("film") || g.contains("fluted") {
        FillTexture::Film
    } else {
        FillTexture::Unknown
    }
}

/// Round 2, change C: the cell-plan size the tower record implies (a square-face reading of `fillAreaM2`) -
/// the same reading the 2D section's horizontal scale uses. The arithmetic lives in the seam mapping.
pub fn cell_plan_m(input: Option<&EngineInput>) -> f64 {
    drafthouse_cockpit_seams::mapping::cell_plan_m(
        input.map(|i| i.tower.fill_area_m2).unwrap_or(64.0),
    )
}

/// Round 2, change C: the fan stack's diameter from the fitted fan's stack area (`4A/pi`), in metres.
pub fn stack_diameter_m(input: Option<&EngineInput>) -> f64 {
    drafthouse_cockpit_seams::mapping::stack_diameter_m(
        input.map(|i| i.fan.stack_area_m2).unwrap_or(19.635),
    )
}
