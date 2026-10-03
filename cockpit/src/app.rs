//! The Bevy application: resources, the fixture load, the staging path, the clock and the keyboard.
//!
//! Systems are split: [`crate::ui`] (the egui instrument), [`crate::scene`] (the Bevy tower section),
//! [`crate::bridge`] (wasm-bindgen + the HTML mirror; **wasm builds only**, issue #71 - on native the
//! mirror's live text has no surface, the known gap in `docs/COCKPIT_SEAMS.md`). The engine is the
//! baseline's `FixtureEngine`, reached through the path dependency - this crate adds no physics.

use std::sync::Mutex;

use bevy::asset::{io::Reader, AssetLoader, LoadContext, LoadState};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use cockpit::engine::{Engine, EngineInput};
use cockpit::fixture_engine::FixtureEngine;

use crate::state::{
    self, add_layer, apply_drop, move_layer, remove_layer, Anchor, Catalog, Class, Drag, Flash,
    Focus, PartRef, Picker, Run, Slot, StartOptions, View, Visual,
};
#[cfg(feature = "three-d")]
use crate::state::{MAX_CELLS, MIN_CELLS};

/// The clock that drives every animation, in seconds. `frozen` holds it at a constant so two evidence
/// frames of the same state are byte-comparable (`?frozen=1`).
pub const FROZEN_T: f32 = 2.5;

#[derive(Resource, Clone)]
pub struct Draft(pub EngineInput);

/// Where the Bevy scene draws (egui points, screen space). `window` is the full screen in the same units.
#[derive(Resource, Default, Clone, Copy)]
pub struct SceneRect {
    pub min: Vec2,
    pub max: Vec2,
    pub phone: bool,
    pub window: Vec2,
}

impl SceneRect {
    pub fn rect(&self) -> bevy_egui::egui::Rect {
        bevy_egui::egui::Rect::from_min_max(
            bevy_egui::egui::pos2(self.min.x, self.min.y),
            bevy_egui::egui::pos2(self.max.x, self.max.y),
        )
    }
    pub fn set(&mut self, r: bevy_egui::egui::Rect, window: bevy_egui::egui::Vec2, phone: bool) {
        self.min = Vec2::new(r.min.x, r.min.y);
        self.max = Vec2::new(r.max.x, r.max.y);
        self.window = Vec2::new(window.x, window.y);
        self.phone = phone;
    }
}

/// The engine the instrument runs on, whichever build selected it (issue #58).
///
/// The slot holds the **contract's** trait object, never a concrete engine: the recorded
/// `FixtureEngine` and the real engine (`cockpit-adapter`) are both reached through [`Engine`], and
/// nothing in this crate branches on which one it got. [`crate::engine_select::build`] decides
/// which, and `?engine=real|fixture` overrides it in a build that carries both.
#[derive(Resource, Default)]
pub struct EngineSlot(pub Option<Box<dyn Engine>>);

#[derive(Resource, Default)]
pub struct Load {
    pub started: f64,
    pub failure: Option<String>,
    pub ready: bool,
}

/// The raw fixture text, kept so the pass can read the recorded inlet air (`anchor.air.inlet`), which the
/// baseline's fixture schema does not expose. Nothing else reads it.
#[derive(Resource, Default, Clone)]
pub struct FixtureText(pub String);

#[derive(Resource, Default)]
pub struct FirstFrame {
    pub first: Option<f64>,
    pub ready: Option<f64>,
}

#[derive(Resource)]
pub struct AnimClock {
    pub t: f32,
    pub frozen: bool,
    /// The clock value a frozen frame holds (`?t=<seconds>`, default [`FROZEN_T`]).
    pub frozen_at: f32,
}

impl Default for AnimClock {
    fn default() -> Self {
        Self {
            t: 0.0,
            frozen: false,
            frozen_at: FROZEN_T,
        }
    }
}

/// What a `?…` URL staged, in words. Shown in the frame so an evidence screenshot says what was staged.
#[derive(Resource, Default)]
pub struct StagedLog(pub Vec<String>);

/// True while egui owns the keyboard, so the app's own shortcuts stay quiet.
#[derive(Resource, Default)]
pub struct WantsKeyboard(pub bool);

/// Where the interactive parts of the instrument are, in canvas pixels, published as `data-hits`.
///
/// This is the same kind of seam as the baseline's `data-stage`: the app reports what it drew, so a headless
/// harness can aim a *real* pointer drag at a tray card and a bay instead of guessing coordinates.
#[derive(Resource, Default)]
pub struct HitMap(pub Vec<(String, [f32; 4])>);

#[derive(Asset, TypePath, Debug)]
pub struct FixtureAsset(pub String);

#[derive(Default, TypePath)]
pub struct FixtureLoader;

impl AssetLoader for FixtureLoader {
    type Asset = FixtureAsset;
    type Settings = ();
    type Error = std::io::Error;
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<FixtureAsset, std::io::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        String::from_utf8(bytes)
            .map(FixtureAsset)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
    fn extensions(&self) -> &[&str] {
        &["json"]
    }
}

#[derive(Resource)]
pub struct FixtureHandle {
    pub handle: Handle<FixtureAsset>,
}

/// Commands from the page (`window.__viz.dispatch("view:curves")`), drained once per frame.
pub static COMMANDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub struct VizPlugin {
    pub options: StartOptions,
}

impl Plugin for VizPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb_u8(0x0f, 0x17, 0x1c)))
            .insert_resource(self.options.clone())
            .insert_resource(AnimClock {
                t: 0.0,
                frozen: self.options.frozen.unwrap_or(false) || self.options.t.is_some(),
                frozen_at: self.options.t.unwrap_or(FROZEN_T),
            })
            .init_resource::<EngineSlot>()
            .init_resource::<Load>()
            .init_resource::<FixtureText>()
            .init_resource::<FirstFrame>()
            .init_resource::<StagedLog>()
            .init_resource::<WantsKeyboard>()
            .init_resource::<HitMap>()
            .init_resource::<Catalog>()
            .init_resource::<Anchor>()
            .init_resource::<Run>()
            .init_resource::<Visual>()
            .init_resource::<SceneRect>()
            .init_resource::<crate::perf::PerfGridRes>()
            .init_resource::<crate::state::ChartStats>()
            // Round 5: the layout measurement surface (the clip probe and the layout counters).
            .init_resource::<crate::clip::ClipProbe>()
            .init_resource::<crate::clip::LayoutInfo>()
            .init_resource::<crate::clip::ScreenText>()
            // Round 4: the generated descriptors (embedded), the form, the parameter card and the duty
            // & site row set. All four are session state - none of them is an engine input of its own.
            .insert_resource(crate::state::FieldsRes::load())
            .insert_resource(crate::duty_panel::DutyRes {
                spec: crate::state::DutySpecRes::load().spec,
                error: crate::state::DutySpecRes::load().error,
            })
            .init_resource::<crate::form::CustomForm>()
            .init_resource::<crate::hover::HoverState>()
            .init_asset::<FixtureAsset>()
            .register_asset_loader(FixtureLoader)
            .add_plugins(EguiPlugin::default())
            .add_systems(Startup, (setup_camera, start_loading))
            .add_systems(
                PreUpdate,
                crate::ui::init_fonts.before(bevy_egui::EguiPreUpdateSet::BeginPass),
            )
            .add_systems(Update, (poll_loading, anim_clock, drain_commands, keyboard))
            .add_systems(EguiPrimaryContextPass, crate::ui::viz_ui)
            .add_plugins(crate::scene::ScenePlugin);
        // Issue #71: the HTML mirror is the web host's, and it writes to the page's DOM - so it is
        // registered on wasm builds only. The native binary runs the same UI without a mirror (the
        // accessibility gap recorded in `docs/COCKPIT_SEAMS.md`).
        #[cfg(target_arch = "wasm32")]
        {
            app.add_systems(PostUpdate, crate::bridge::mirror_system);
        }
        {
            // Round 4, item 1: the 3D module (its plugin, its camera and `bevy_pbr`) is registered only
            // when the `three-d` feature is on. The default build has no 3D view at all.
            #[cfg(feature = "three-d")]
            add_three_plugin(app);
        }
        let mut settings = app.world_mut().resource_mut::<EguiGlobalSettings>();
        settings.enable_absorb_bevy_input_system = false;
        // Round 3: this app attaches `PrimaryEguiContext` to its own 2D camera in `setup_camera`, but
        // bevy_egui's auto-create path attaches a *second* context to whichever camera it sees first
        // (`setup_primary_egui_context_system`) - and on this build the 3D camera, spawned by the same
        // `Startup` schedule, won that race. Two contexts then share `EguiPrimaryContextPass`, and
        // `run_egui_contexts` panics with "Each Egui context must have a unique schedule", which kills the
        // app on its first frame. With auto-create off there is exactly one context and the race is gone.
        settings.auto_create_primary_context = false;
    }
}

/// Round 4, item 1: the 3D module (and its plugin, its camera and its `bevy_pbr` dependency) is registered
/// only when the `three-d` feature is on. The default build has no 3D view at all.
#[cfg(feature = "three-d")]
fn add_three_plugin(app: &mut App) {
    app.add_plugins(crate::three::ThreePlugin);
}

fn setup_camera(mut commands: Commands) {
    // Order 1: the 2D camera renders *after* the 3D one (three.rs spawns it at order 0), so when the 3D
    // view is up it draws egui and the (hidden) sprites over the 3D picture instead of clearing it.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            ..default()
        },
        Tonemapping::None,
        bevy_egui::PrimaryEguiContext,
    ));
}

fn start_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    options: Res<StartOptions>,
    mut load: ResMut<Load>,
) {
    let path = if options.engine.as_deref() == Some("unavailable") {
        "engine-missing.json"
    } else {
        "fixture.json"
    };
    let handle: Handle<FixtureAsset> = asset_server.load(path);
    load.started = crate::clock::now_ms();
    commands.insert_resource(FixtureHandle { handle });
}

/// Load the fixture, build the engine, the catalog and the draft, then apply the URL's staging.
///
/// Staging goes through the *same* functions the pointer calls ([`apply_drop`], [`move_layer`],
/// [`add_layer`], [`remove_layer`]) - a staged frame is a real state, reached without a mouse.
#[allow(clippy::too_many_arguments)]
fn poll_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    assets: Res<Assets<FixtureAsset>>,
    fixture: Option<Res<FixtureHandle>>,
    mut load: ResMut<Load>,
    mut engine: ResMut<EngineSlot>,
    mut anchor: ResMut<Anchor>,
    mut vis: ResMut<Visual>,
    mut staged: ResMut<StagedLog>,
    options: Res<StartOptions>,
    mut form: ResMut<crate::form::CustomForm>,
    fields_res: Res<crate::state::FieldsRes>,
    mut hover: ResMut<crate::hover::HoverState>,
) {
    if load.ready || load.failure.is_some() {
        return;
    }
    let Some(fixture) = fixture else { return };
    match asset_server.get_load_state(fixture.handle.id()) {
        Some(LoadState::Failed(err)) => load.failure = Some(format!("{err}")),
        Some(LoadState::Loaded) => {
            let Some(asset) = assets.get(&fixture.handle) else {
                return;
            };
            // The fixture text is **data** here, not an engine: the catalog the view lists, the
            // default input the draft starts from, the recorded performance grid. Which engine runs
            // on that input is the build's choice (issue #58), and it is asked for by name.
            let file = match FixtureEngine::from_json(&asset.0) {
                Err(e) => {
                    load.failure = Some(format!("fixture rejected: {e}"));
                    return;
                }
                Ok(file) => file,
            };
            let mut selected =
                match crate::engine_select::build(&asset.0, options.engine.as_deref()) {
                    Err(e) => {
                        load.failure = Some(format!("engine: {e}"));
                        return;
                    }
                    Ok(engine) => engine,
                };
            let mut catalog = Catalog::from_fixture(&file);
            catalog.ambient = state::Ambient::from_json(&asset.0);
            let input = file.default_input();
            // The recorded run: the fixture default input, run once **by the selected engine**.
            // Every delta the instrument shows is against this, and it is always labelled as the
            // anchor - which engine answered is carried in the output's own provenance line.
            anchor.output = selected.run(&input).ok();
            anchor.ratio = input.speed_ratio;
            let mut draft = Draft(input);
            vis.reset_ratio = draft.0.speed_ratio;
            let log = apply_staging(
                &mut vis,
                &mut draft,
                &mut catalog,
                &options,
                &mut form,
                fields_res.fields.as_ref(),
                &mut hover,
                Some(&mut *selected),
            );
            staged.0 = log;
            // Round 3: the recorded performance grid the two CTI-style charts draw. Read once,
            // here, from the same fixture text the engine was built from - never per frame.
            let grid = crate::perf::PerfGrid::from_json(&asset.0);
            commands.insert_resource(crate::perf::PerfGridRes(grid));
            commands.insert_resource(catalog);
            commands.insert_resource(draft);
            commands.insert_resource(FixtureText(asset.0.clone()));
            engine.0 = Some(selected);
            load.ready = true;
        }
        _ => {}
    }
}

/// `?…` staging. Every entry that a URL can set, applied in a fixed order, in words.
#[allow(clippy::too_many_arguments)]
pub fn apply_staging(
    vis: &mut Visual,
    draft: &mut Draft,
    cat: &mut Catalog,
    o: &StartOptions,
    form: &mut crate::form::CustomForm,
    fields: Option<&drafthouse_cockpit_seams::custom::Fields>,
    hover: &mut crate::hover::HoverState,
    engine: Option<&mut (dyn Engine + '_)>,
) -> Vec<String> {
    let mut log: Vec<String> = Vec::new();
    // ---- round 4, item 2: the custom-part form. Applied FIRST, so a URL can author a record and then use
    // it in the same frame (the evidence does exactly that for the "custom chip dropped into a bay" frame).
    if let Some(class) = o.form.as_deref().and_then(Class::from_slug) {
        if let Some(fields) = fields {
            form.open(class, fields, true);
            log.push(format!("form={} open", class.slug()));
            if let Some(pairs) = o.form_fields.as_deref() {
                // `;` between fields, `,` inside a value (a range pair or a text list).
                for pair in pairs.split(';') {
                    if let Some((k, v)) = pair.split_once('=') {
                        form.set_text(k.trim(), v.trim());
                        log.push(format!("form field {k}={}", v.trim()));
                    }
                }
            }
            if let Some(rows) = o.form_rows.as_deref() {
                // `v,v,v;v,v,v` -> the first point table of the class
                let parsed: Vec<Vec<String>> = rows
                    .split(';')
                    .map(|r| r.split(',').map(|c| c.trim().to_string()).collect())
                    .collect();
                if let Some(f) = fields.class(class.slug()).and_then(|c| {
                    c.fields
                        .iter()
                        .find(|f| f.kind == drafthouse_cockpit_seams::custom::Kind::Points)
                }) {
                    let key = f.key.clone();
                    form.set_rows(&key, &parsed, fields, class);
                    log.push(format!("form rows into {key}: {} row(s)", parsed.len()));
                }
            }
            if let Some(c) = o.form_companion.as_deref() {
                if let Some(f) = fields
                    .class(class.slug())
                    .and_then(|c| c.fields.iter().find(|f| f.companion.is_some()))
                {
                    if let Some(comp) = f.companion.as_ref() {
                        let key = comp.key.clone();
                        let field_key = f.key.clone();
                        // The companion rides in the point table's own input (that is where the editor
                        // writes it), so the staged value goes to the same place the pointer's would.
                        form.set_companion(&field_key, c);
                        log.push(format!("form companion {key}={c}"));
                    }
                }
            }
            if o.form_save.unwrap_or(false) {
                match form.save(fields, cat, engine) {
                    Ok(line) => log.push(format!("form save: {line}")),
                    Err(e) => log.push(format!(
                        "form save refused: {}",
                        e.first().cloned().unwrap_or_default()
                    )),
                }
            }
        } else {
            log.push("form refused: the field list did not load".to_string());
        }
    }
    // ---- round 4, item 4: the duty, in the engine's own field names (`waterMassFlowKgS=200`).
    if let Some(pairs) = o.duty.as_deref() {
        // `;` between fields, the engine's own names on the left.
        for pair in pairs.split(';') {
            let Some((k, v)) = pair.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "waterMassFlowKgS" => {
                    if let Ok(x) = v.parse::<f64>() {
                        draft.0.duty.water_flow_m3_hr =
                            drafthouse_cockpit_seams::mapping::m3_hr_from_kg_s(x);
                        log.push(format!(
                            "duty waterMassFlowKgS={x} -> {:.0} m3/hr at 1000 kg/m3",
                            draft.0.duty.water_flow_m3_hr
                        ));
                    }
                }
                "waterFlowM3Hr" => {
                    if let Ok(x) = v.parse::<f64>() {
                        draft.0.duty.water_flow_m3_hr = x;
                        log.push(format!("duty waterFlowM3Hr={x}"));
                    }
                }
                "hotWaterC"
                | "targetColdWaterC"
                | "wetBulbC"
                | "dryBulbC"
                | "pressurePa"
                | "salinityGKg"
                | "cyclesOfConcentration" => {
                    if let Ok(x) = v.parse::<f64>() {
                        match k {
                            "hotWaterC" => draft.0.duty.hot_water_c = x,
                            "targetColdWaterC" => draft.0.duty.target_cold_water_c = x,
                            "wetBulbC" => draft.0.duty.wet_bulb_c = x,
                            "dryBulbC" => draft.0.duty.dry_bulb_c = x,
                            "pressurePa" => draft.0.duty.pressure_pa = x,
                            "salinityGKg" => draft.0.duty.salinity_g_kg = x,
                            _ => draft.0.duty.cycles_of_concentration = x,
                        }
                        log.push(format!("duty {k}={x}"));
                    }
                }
                "waterQualityClass" => {
                    draft.0.duty.water_quality_class = v.to_string();
                    log.push(format!("duty waterQualityClass={v}"));
                }
                _ => log.push(format!("duty: {k} is not an editable duty field")),
            }
        }
    }
    if let Some(v) = o.view.as_deref().and_then(View::from_slug) {
        vis.view = v;
        log.push(format!("view={}", v.slug()));
    }
    if let Some(f) = o.focus.as_deref().and_then(Focus::from_slug) {
        vis.focus = f;
        log.push(format!("focus={}", f.slug()));
    }
    if let Some(g) = o.grid {
        vis.grid = g;
    }
    if let Some(p) = o.pattern.as_deref().and_then(state::Pattern::from_slug) {
        vis.nozzle_pattern = p;
        log.push(format!("pattern={}", p.slug()));
    }
    if let Some(s) = o.spacing {
        vis.nozzle_spacing_m = s.clamp(0.2, 2.5);
        log.push(format!("nozzle spacing={:.2} m", vis.nozzle_spacing_m));
    }
    if let Some(l) = o.layer {
        vis.selected_layer = l;
        vis.selected_slot = Slot::Fill;
    }
    if let Some(rpm) = o.rpm {
        // Issue #59: the staged rpm is mapped through the record's own rated speed and handed to the
        // engine **as asked**. A ratio outside the record's band is not clamped into it here: the
        // engine refuses it by name and the dock shows that limit - never a clamped number.
        match draft.0.fan.nominal_rpm {
            Some(nominal) => {
                let ratio = drafthouse_cockpit_seams::mapping::ratio_from_rpm(rpm, nominal);
                let [lo, hi] = draft.0.fan.allowed_speed_ratio;
                draft.0.speed_ratio = ratio;
                log.push(format!(
                    "rpm={:.0} -> speed ratio {:.3} (record range {:.2}-{:.2}){}",
                    rpm,
                    ratio,
                    lo,
                    hi,
                    if ratio < lo || ratio > hi {
                        " outside the record's band: the engine names the limit"
                    } else {
                        ""
                    }
                ));
            }
            None => log.push(format!(
                "rpm={:.0} ignored: {} states no rated speed",
                rpm, draft.0.fan.id
            )),
        }
    }
    if let Some(a) = o.add.as_deref() {
        let part = PartRef::parse(a);
        match add_layer(&*cat, &mut draft.0, part.as_ref()) {
            Ok(msg) => log.push(format!("add: {msg}")),
            Err(e) => log.push(format!("add refused: {e}")),
        }
    }
    if let Some(s) = o.shift.as_deref() {
        if let Some((dir, idx)) = s.split_once(':') {
            if let Ok(i) = idx.parse::<usize>() {
                if let Some(msg) = move_layer(&mut draft.0, i, dir == "up") {
                    vis.selected_layer = if dir == "up" {
                        i.saturating_sub(1)
                    } else {
                        i + 1
                    };
                    log.push(format!("move: {msg}"));
                }
            }
        }
    }
    if let Some(i) = o.remove {
        if let Some(msg) = remove_layer(&mut draft.0, i) {
            vis.selected_layer = i.min(draft.0.fill_layers.len().saturating_sub(1));
            log.push(format!("remove: {msg}"));
        }
    }
    if let Some(d) = o.drop.as_deref() {
        // `<class>:<id>@<slot>`
        if let Some((part_s, slot_s)) = d.split_once('@') {
            if let (Some(part), Some(slot)) = (
                PartRef::parse(part_s),
                Slot::ALL.iter().find(|s| s.slug() == slot_s),
            ) {
                let layer = vis.selected_layer;
                // Staging mirrors the pointer path: a drop also dismisses the open picker (change A).
                vis.picker = None;
                vis.picker_rect = [0.0; 4];
                match apply_drop(&*cat, &mut draft.0, *slot, &part, layer) {
                    Ok(msg) => {
                        vis.selected_part = Some(part.clone());
                        vis.selected_slot = *slot;
                        log.push(format!("drop {} -> {} bay: {msg}", part.id, slot.name()));
                    }
                    Err(e) => log.push(format!("drop {} refused: {e}", part.id)),
                }
            }
        }
    }
    if let Some(d) = o.drag.as_deref() {
        if let Some(part) = PartRef::parse(d) {
            let over = o
                .over
                .as_deref()
                .and_then(|s| Slot::ALL.iter().find(|x| x.slug() == s).copied());
            let verdict = match over {
                Some(slot) => state::check_drop(&*cat, &draft.0, slot, &part),
                None => Ok(()),
            };
            vis.selected_part = Some(part.clone());
            vis.drag = Some(Drag {
                part,
                over,
                verdict,
                staged: true,
            });
            log.push(format!(
                "staged drag over {}",
                over.map(|s| s.slug()).unwrap_or("nothing")
            ));
        }
    }
    // ---- round 2: the bay focus, the picker, the 3D configuration ------------------------------------
    if let Some(slot) = o.bay.as_deref().and_then(slot_of) {
        vis.bay_focus = Some(slot);
        log.push(format!("bay focus={}", slot.slug()));
    }
    // A staged drop dismisses the picker (change A), so it must not be re-opened by a staged picker on the
    // same frame: the drop is the later event.
    if let Some(slot) = o
        .picker
        .as_deref()
        .and_then(slot_of)
        .filter(|_| o.drop.is_none())
    {
        let layer = vis
            .selected_layer
            .min(draft.0.fill_layers.len().saturating_sub(1));
        let mut p = Picker::new(slot, layer, [0.0; 4], false);
        if let Some(r) = o.picker_row {
            p.row = r;
        }
        vis.picker = Some(p);
        vis.selected_slot = slot;
        vis.selected_layer = layer;
        log.push(format!("picker={} (layer {})", slot.slug(), layer + 1));
    }
    #[cfg(feature = "three-d")]
    if let Some(c) = o.cells {
        vis.cells = c.clamp(MIN_CELLS, MAX_CELLS);
        log.push(format!("cells={}", vis.cells));
    }
    #[cfg(feature = "three-d")]
    if let Some(c) = o.cutaway {
        vis.cutaway = c;
        log.push(format!("cutaway={}", if c { "open" } else { "closed" }));
    }
    #[cfg(feature = "three-d")]
    if let Some(i) = o.cell {
        vis.focus_cell = i;
        log.push(format!("focus cell={}", vis.focus_cell + 1));
    }
    #[cfg(feature = "three-d")]
    if let Some(cam) = o.cam.as_deref() {
        let v: Vec<f32> = cam
            .split(',')
            .filter_map(|x| x.trim().parse::<f32>().ok())
            .collect();
        if v.len() == 3 {
            vis.cam_yaw = v[0];
            vis.cam_pitch = v[1];
            vis.cam_dist = v[2].clamp(1.0, 12.0);
            log.push(format!(
                "cam yaw {:.0} pitch {:.0} dist {:.2}",
                vis.cam_yaw, vis.cam_pitch, vis.cam_dist
            ));
        }
    }
    // ---- round 3: the parts rail and the section's honesty legend ------------------------------------
    if let Some(r) = o.rail {
        vis.rail_open = r;
        log.push(format!("rail={}", if r { "open" } else { "collapsed" }));
    }
    if let Some(l) = o.legend {
        vis.legend_open = l;
        log.push(format!("legend={}", if l { "shown" } else { "hidden" }));
    }
    // ---- issue #91: the two drawers and reduced motion ------------------------------------------------
    if let Some(n) = o.notes {
        vis.notes_open = n;
        log.push(format!("notes={}", if n { "open" } else { "closed" }));
    }
    if let Some(p) = o.panel {
        vis.panel_open = p;
        log.push(format!("panel={}", if p { "open" } else { "closed" }));
    }
    // #91 round 2: the tap-detail card, staged.
    if let Some(d) = o
        .detail
        .as_deref()
        .and_then(crate::state::Detail::from_slug)
    {
        vis.detail = Some(d);
        log.push(format!("detail={}", d.slug()));
    }
    if o.reduced_motion {
        vis.reduced_motion = true;
        log.push("reduced-motion".to_string());
    }
    // ---- round 5: the duty panel's three collapsible sections ---------------------------------------
    if let Some(v) = o.duty_open {
        vis.duty_open = v;
        log.push(format!(
            "duty-open={}",
            if v { "expanded" } else { "collapsed" }
        ));
    }
    if let Some(v) = o.water_open {
        vis.water_open = v;
        log.push(format!(
            "water-open={}",
            if v { "expanded" } else { "collapsed" }
        ));
    }
    if let Some(v) = o.limits_open {
        vis.limits_open = v;
        log.push(format!(
            "limits-open={}",
            if v { "expanded" } else { "collapsed" }
        ));
    }
    // ---- round 4, item 3: the parameter card, staged (the phone long-press path).
    if let Some(spec) = o.hover.as_deref() {
        if let Some(part) = PartRef::parse(spec) {
            hover.stage(
                part.clone(),
                o.hover_long.unwrap_or(false),
                o.hover_bay.unwrap_or(false),
            );
            log.push(format!(
                "hover={} ({} card)",
                part.slug(),
                if o.hover_long.unwrap_or(false) {
                    "long-press"
                } else {
                    "hover"
                }
            ));
        }
    }
    log
}

/// A `Slot` from its slug, for the URL and the page's commands.
pub fn slot_of(s: &str) -> Option<Slot> {
    Slot::ALL.iter().find(|x| x.slug() == s).copied()
}

/// The picker's own commands, shared by the page's control bar and the app's keyboard so the two cannot
/// drift. Returns true when it handled something (so the frame can say what happened).
pub fn picker_command(
    vis: &mut Visual,
    cat: Option<&Catalog>,
    draft: Option<&mut Draft>,
    cmd: &str,
) -> bool {
    match cmd {
        "close" => {
            let had = vis.picker.take().is_some();
            if had {
                vis.flash = Some(Flash {
                    ok: true,
                    text: "picker closed".into(),
                });
            }
            return had;
        }
        "open" | "toggle" => {
            if vis.picker.is_some() {
                vis.picker = None;
                vis.flash = Some(Flash {
                    ok: true,
                    text: "picker closed".into(),
                });
                return true;
            }
            return open_picker(vis, vis.selected_slot, false);
        }
        "next" | "prev" => {
            let Some(p) = vis.picker.as_mut() else {
                return false;
            };
            let rows = cat
                .map(|c| state::picker_rows(c, p.slot))
                .unwrap_or_default();
            if rows.is_empty() {
                return false;
            }
            let n = rows.len();
            p.row = if cmd == "next" {
                (p.row + 1) % n
            } else {
                (p.row + n - 1) % n
            };
            return true;
        }
        "pick" => {
            let Some(p) = vis.picker.clone() else {
                return false;
            };
            let Some(cat) = cat else { return false };
            let rows = state::picker_rows(cat, p.slot);
            let Some(part) = rows.get(p.row.min(rows.len().saturating_sub(1))).cloned() else {
                return false;
            };
            pick_part(cat, draft, vis, p.slot, p.layer, &part);
            return true;
        }
        _ => {}
    }
    // `open:<slot>` / `row:<n>`
    if let Some(slug) = cmd.strip_prefix("open:") {
        if let Some(slot) = slot_of(slug) {
            return open_picker(vis, slot, false);
        }
    }
    if let Some(n) = cmd
        .strip_prefix("row:")
        .and_then(|x| x.parse::<usize>().ok())
    {
        if let Some(p) = vis.picker.as_mut() {
            p.row = n;
            return true;
        }
    }
    false
}

/// Open a bay's picker (one at a time - this replaces whatever was open).
pub fn open_picker(vis: &mut Visual, slot: Slot, from_keyboard: bool) -> bool {
    let layer = vis.selected_layer;
    vis.picker = Some(Picker::new(slot, layer, [0.0; 4], from_keyboard));
    vis.selected_slot = slot;
    vis.flash = Some(Flash {
        ok: true,
        text: format!(
            "{} picker open{}",
            slot.name(),
            if from_keyboard { " (keyboard)" } else { "" }
        ),
    });
    true
}

/// Pick a row: the same replace a drop does, through the same `apply_drop`, then dismiss.
fn pick_part(
    cat: &Catalog,
    draft: Option<&mut Draft>,
    vis: &mut Visual,
    slot: Slot,
    layer: usize,
    part: &PartRef,
) {
    let Some(d) = draft else {
        vis.flash = Some(Flash {
            ok: false,
            text: "the engine is not loaded".into(),
        });
        return;
    };
    match apply_drop(cat, &mut d.0, slot, part, layer) {
        Ok(msg) => {
            vis.flash = Some(Flash {
                ok: true,
                text: msg,
            });
            vis.selected_part = Some(part.clone());
            vis.selected_slot = slot;
            vis.picker = None;
            vis.drag = None;
            if slot == Slot::Fill {
                vis.selected_layer = layer.min(d.0.fill_layers.len().saturating_sub(1));
            }
        }
        Err(e) => {
            vis.flash = Some(Flash {
                ok: false,
                text: format!("refused: {e}"),
            })
        }
    }
}

/// The animation clock. Frozen time is a constant, so `?frozen=1` frames are reproducible.
fn anim_clock(time: Res<Time>, mut clock: ResMut<AnimClock>, vis: Option<Res<Visual>>) {
    if clock.frozen {
        clock.t = clock.frozen_at;
    } else if vis.map(|v| v.reduced_motion).unwrap_or(false) {
        // "reduced motion" holds the clock: the flow, the droplets and the wheel stand still while the
        // engine's numbers still update (the button's own promise). Nothing snaps - the pose it holds is
        // whichever pose it was in when the switch was flipped.
        clock.t = FROZEN_T;
    } else {
        clock.t += time.delta_secs();
    }
}

/// Commands from the page (the capture harness and the HTML controls both come through here).
fn drain_commands(
    mut vis: ResMut<Visual>,
    mut draft: Option<ResMut<Draft>>,
    cat: Option<Res<Catalog>>,
    mut clock: ResMut<AnimClock>,
    mut staged: ResMut<StagedLog>,
) {
    let mut inbox = COMMANDS.lock().unwrap();
    if inbox.is_empty() {
        return;
    }
    let cmds: Vec<String> = inbox.drain(..).collect();
    drop(inbox);
    for cmd in cmds {
        let (head, arg) = match cmd.split_once(':') {
            Some((h, a)) => (h, Some(a)),
            None => (cmd.as_str(), None),
        };
        match head {
            "view" => {
                if let Some(v) = arg.and_then(View::from_slug) {
                    vis.view = v;
                }
            }
            "focus" => {
                if let Some(f) = arg.and_then(Focus::from_slug) {
                    vis.focus = f;
                }
            }
            "next-view" => {
                let i = View::ALL.iter().position(|v| *v == vis.view).unwrap_or(0);
                vis.view = View::ALL[(i + 1) % View::ALL.len()];
            }
            "grid" => vis.grid = arg != Some("0"),
            "freeze" => clock.frozen = arg == Some("1"),
            "spacing" => {
                if let Some(v) = arg.and_then(|a| a.parse::<f64>().ok()) {
                    vis.nozzle_spacing_m = v.clamp(0.2, 2.5);
                }
            }
            "pattern" => {
                if let Some(p) = arg.and_then(state::Pattern::from_slug) {
                    vis.nozzle_pattern = p;
                }
            }
            "slot" => {
                if let Some(s) = arg.and_then(|a| Slot::ALL.iter().find(|x| x.slug() == a).copied())
                {
                    vis.selected_slot = s;
                }
            }
            "layer" => {
                if let Some(i) = arg.and_then(|a| a.parse::<usize>().ok()) {
                    vis.selected_layer = i;
                    vis.selected_slot = Slot::Fill;
                }
            }
            "nudge" => {
                if let Some(draft) = draft.as_deref_mut() {
                    let [lo, hi] = draft.0.fan.allowed_speed_ratio;
                    let dir = arg.and_then(|a| a.parse::<f64>().ok()).unwrap_or(1.0);
                    draft.0.speed_ratio = (draft.0.speed_ratio + 0.02 * dir).clamp(lo, hi);
                }
            }
            "focus-next" => {
                let i = Focus::ALL_SLUGS
                    .iter()
                    .position(|s| *s == vis.focus.slug())
                    .unwrap_or(0);
                vis.focus = Focus::from_slug(Focus::ALL_SLUGS[(i + 1) % Focus::ALL_SLUGS.len()])
                    .unwrap_or(Focus::All);
            }
            "spacing-step" => {
                let dir = arg.and_then(|a| a.parse::<f64>().ok()).unwrap_or(1.0);
                vis.nozzle_spacing_m = (vis.nozzle_spacing_m + 0.1 * dir).clamp(0.3, 2.0);
            }
            "freeze-toggle" => clock.frozen = !clock.frozen,
            "grid-toggle" => vis.grid = !vis.grid,
            "reset" => {
                if let Some(draft) = draft.as_deref_mut() {
                    draft.0.speed_ratio = vis.reset_ratio;
                }
            }
            // ---- round 2: the picker and the 3D configuration ------------------------------------------
            "picker" => {
                let cmd = arg.unwrap_or("toggle").to_string();
                let cat = cat.as_deref();
                let _ = picker_command(&mut vis, cat, draft.as_deref_mut(), &cmd);
            }
            "bay" => {
                let next = match arg {
                    Some("next") | None => {
                        let i = vis
                            .bay_focus
                            .and_then(|s| Slot::ALL.iter().position(|x| *x == s))
                            .map(|i| (i + 1) % Slot::ALL.len())
                            .unwrap_or(0);
                        Some(Slot::ALL[i])
                    }
                    Some(slug) => slot_of(slug),
                };
                if let Some(s) = next {
                    vis.bay_focus = Some(s);
                }
            }
            "bay-open" => {
                if let Some(s) = vis.bay_focus {
                    open_picker(&mut vis, s, true);
                }
            }
            #[cfg(feature = "three-d")]
            "cells" => match arg {
                Some("up") => vis.cells = (vis.cells + 1).min(MAX_CELLS),
                Some("down") => vis.cells = vis.cells.saturating_sub(1).max(MIN_CELLS),
                Some(a) => {
                    if let Ok(n) = a.parse::<u32>() {
                        vis.cells = n.clamp(MIN_CELLS, MAX_CELLS);
                    }
                }
                None => {}
            },
            #[cfg(feature = "three-d")]
            "cutaway" => {
                vis.cutaway = match arg {
                    Some("1") => true,
                    Some("0") => false,
                    _ => !vis.cutaway,
                };
            }
            // ---- round 3: the parts rail and the legend ------------------------------------------
            "rail" => {
                vis.rail_open = match arg {
                    Some("1") => true,
                    Some("0") => false,
                    _ => !vis.rail_open,
                };
            }
            // ---- issue #91: the drawers and reduced motion --------------------------------------------
            "notes" => {
                vis.notes_open = match arg {
                    Some("1") => true,
                    Some("0") => false,
                    _ => !vis.notes_open,
                };
                if vis.notes_open {
                    vis.panel_open = false;
                }
            }
            "panel" => {
                vis.panel_open = match arg {
                    Some("1") => true,
                    Some("0") => false,
                    _ => !vis.panel_open,
                };
                if vis.panel_open {
                    vis.notes_open = false;
                }
            }
            "motion" => {
                vis.reduced_motion = match arg {
                    Some("reduced") | Some("0") => true,
                    Some("full") | Some("1") => false,
                    _ => !vis.reduced_motion,
                };
            }
            // #91 round 2: the tap-detail card (`detail:fan`, `detail:op`, `detail:close`).
            "detail" => {
                vis.detail = arg.and_then(crate::state::Detail::from_slug);
            }
            "legend" => {
                vis.legend_open = match arg {
                    Some("1") => true,
                    Some("0") => false,
                    _ => !vis.legend_open,
                };
            }
            #[cfg(feature = "three-d")]
            "cell" => {
                vis.focus_cell = match arg {
                    Some("next") | None => (vis.focus_cell + 1) % vis.cells.max(1) as usize,
                    Some(a) => a.parse::<usize>().unwrap_or(0),
                };
            }
            #[cfg(feature = "three-d")]
            "cam" => {
                let reset = drafthouse_cockpit_seams::mapping::CAM_DEFAULT;
                match arg {
                    Some("reset") | None => {
                        vis.cam_yaw = reset.0;
                        vis.cam_pitch = reset.1;
                        vis.cam_dist = reset.2;
                    }
                    Some("left") => vis.cam_yaw -= 12.0,
                    Some("right") => vis.cam_yaw += 12.0,
                    Some("up") => vis.cam_pitch = (vis.cam_pitch + 8.0).min(80.0),
                    Some("down") => vis.cam_pitch = (vis.cam_pitch - 8.0).max(2.0),
                    Some("in") => vis.cam_dist = (vis.cam_dist - 0.25).max(1.0),
                    Some("out") => vis.cam_dist = (vis.cam_dist + 0.25).min(12.0),
                    Some(a) => {
                        let v: Vec<f32> = a
                            .split(',')
                            .filter_map(|x| x.trim().parse::<f32>().ok())
                            .collect();
                        if v.len() == 3 {
                            vis.cam_yaw = v[0];
                            vis.cam_pitch = v[1].clamp(2.0, 80.0);
                            vis.cam_dist = v[2].clamp(1.0, 12.0);
                        }
                    }
                }
            }
            // drafthouse#91 Part B: every head this match does not know belongs to the new screens' shell.
            _ => crate::screens::push_command(&cmd),
        }
    }
    let _ = (&cat, &mut staged);
}

fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    wants: Res<WantsKeyboard>,
    mut vis: ResMut<Visual>,
    mut draft: Option<ResMut<Draft>>,
    mut clock: ResMut<AnimClock>,
) {
    if wants.0 {
        return;
    }
    // Round 3: L shows/hides the section's label legend, K collapses the parts rail. Both mirror the
    // on-screen controls, and both states are published for the evidence frames.
    if keys.just_pressed(KeyCode::KeyL) {
        vis.legend_open = !vis.legend_open;
    }
    if keys.just_pressed(KeyCode::KeyK) {
        vis.rail_open = !vis.rail_open;
    }
    // Issue #91: I (notes), M (reduced motion) and P (setup drawer) are the page's keys only (index.html),
    // like B and E - one path per key, so a focused canvas never toggles them twice.
    if keys.just_pressed(KeyCode::Digit1) {
        vis.view = View::Cockpit;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        vis.view = View::Curves;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        vis.grid = !vis.grid;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        clock.frozen = !clock.frozen;
    }
    if keys.just_pressed(KeyCode::Tab) {
        let i = Focus::ALL_SLUGS
            .iter()
            .position(|s| *s == vis.focus.slug())
            .unwrap_or(0);
        vis.focus = Focus::from_slug(Focus::ALL_SLUGS[(i + 1) % Focus::ALL_SLUGS.len()])
            .unwrap_or(Focus::All);
    }
    if let Some(draft) = draft.as_deref_mut() {
        let [lo, hi] = draft.0.fan.allowed_speed_ratio;
        let step = 0.02;
        let mut ratio = draft.0.speed_ratio;
        if keys.just_pressed(KeyCode::BracketRight) {
            ratio = (ratio + step).clamp(lo, hi);
        }
        if keys.just_pressed(KeyCode::BracketLeft) {
            ratio = (ratio - step).clamp(lo, hi);
        }
        if keys.just_pressed(KeyCode::KeyR) {
            ratio = vis.reset_ratio;
        }
        draft.0.speed_ratio = ratio;
    }
}
