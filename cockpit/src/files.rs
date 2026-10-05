//! The project session: the file the instrument is editing, and what each plane may do with it.
//!
//! Owner decision (D26): **one persistence model** for both hosts. This module is the one place that
//! turns the working state into a `.drafthouse` file and back, so the native menu and the page's
//! download/upload are the *same* two calls ([`snapshot_text`], [`open_text`]) with different plumbing.
//!
//! | | native binary | web-internal host | public host |
//! |---|---|---|---|
//! | save / export | `File > Save / Save As / Export…`, dialogs via `rfd` | *Download project* through the existing bridge | **nothing** |
//! | open | `File > Open`, recent files, dialogs via `rfd` | *Upload project* through the existing bridge | **nothing** |
//! | catalog revision | `File > Import catalog revision…` | *Import revision* through the existing bridge | **nothing** |
//!
//! The public host keeps the rule it always had: it can author, and it cannot save or export. That is
//! enforced *twice* - the page draws no such button, and [`handle_command`] refuses the commands on a
//! public host by name, so a hand-dispatched bridge call cannot talk its way past the page.
//!
//! # What "unsaved" means here
//!
//! The session does not guess whether the user *meant* to change something: it compares the machine's own
//! state ([`signature`]) against the state the file was written from. A drop, a nudge, a duty edit or a
//! new custom record all move that signature; a save or an open resets it. The File menu's dot, the
//! page's marker and the unsaved-changes prompt all read that one comparison.
//!
//! # No network, anywhere
//!
//! Every path in this module is a local file path or an in-memory string. The web-internal host's
//! "upload" is the page handing over the *text* of a file it read through its own `<input type="file">`;
//! the Rust side never fetches, posts or opens a socket.

use bevy::prelude::*;
/// The File menu is drawn with egui; the wasm planes have no menu at all.
#[cfg(not(target_arch = "wasm32"))]
use bevy_egui::egui;
use cockpit::engine::{Engine, EngineInput, EngineOutput};
use cockpit::fixture_engine::FixtureEngine;
use drafthouse_cockpit_seams::custom::{CustomPart, Fields};
use drafthouse_cockpit_seams::duty::DutySpec;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::app::EngineSlot;
use crate::project::{self, Project, Snapshot};
use crate::revision::{self, Revision};
use crate::state::{Catalog, StartOptions};

/// The host labels a project file can be written with. The public host writes none: it cannot save.
pub const HOST_NATIVE: &str = "native";
pub const HOST_WEB_INTERNAL: &str = "web-internal";

/// Which plane this binary is: the native one, or the page's wasm module. The public host is not a plane -
/// it reaches no write path at all (see [`handle_command`]).
pub fn plane_label() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        HOST_WEB_INTERNAL
    } else {
        HOST_NATIVE
    }
}

/// How many recent files the menu keeps. Session state, not a config file: this issue ships no settings
/// store (a later connect issue owns persistence of preferences, per D26).
pub const RECENT_LIMIT: usize = 8;

/// What the session is holding.
#[derive(Resource, Clone, Default)]
pub struct FileSession {
    /// Where the project came from / went to. `None` = never saved.
    pub path: Option<PathBuf>,
    /// The machine state the file was written from (see [`signature`]).
    pub saved_signature: String,
    /// Most recent first, de-duplicated.
    pub recent: Vec<PathBuf>,
    /// The last line the user should read: what happened, or why it did not.
    pub status: String,
    /// The project text the page may download (written on request; empty = nothing to hand out).
    pub outbox: String,
    /// The file name the page should offer for the outbox.
    pub outbox_name: String,
    /// The question the session is waiting on (the unsaved-changes prompt), and what it blocks.
    pub prompt: Option<Prompt>,
    /// Set when a file operation this session was *asked* to do failed. The native CLI's headless run
    /// reads this to decide its exit code: a refused import must not look like a run that worked.
    pub refused: bool,
    /// The catalog revision the session's runs are pinned to.
    pub revision_id: String,
    pub revision_status: String,
    /// The project last opened or saved here - what "recomputed on open" is measured against.
    pub loaded: Option<Box<Project>>,
    /// Startup work that has to wait for the engine (the native CLI's flags, and nothing else).
    pub pending_open: Option<PathBuf>,
    pub pending_revision: Option<PathBuf>,
    pub pending_save: Option<PathBuf>,
    pub pending_json: Option<PathBuf>,
    pub pending_csv: Option<PathBuf>,
    /// `--compare <p1,p2[,p3]>` (issue #89): the saved project files the comparison screen opens.
    /// Each is read here (where the file system is) and handed to the comparison as a
    /// `compare:open:<name>:<text>` command; the screen opens and runs them.
    pub pending_compare: Vec<PathBuf>,
}

/// The unsaved-changes question, and what it is standing in the way of.
#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    pub then: Request,
}

/// A request from the File menu (or a recent-files entry). Kept as data so the menu can be drawn from
/// borrowed state and applied afterwards, where the mutable resources are.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    New,
    Open,
    Save,
    SaveAs,
    ImportRevision,
    ExportJson,
    ExportCsv,
    OpenPath(PathBuf),
    ConfirmDiscard,
    Cancel,
}

/// Everything a file operation reads that is not the session, the draft or the catalog.
pub struct Ctx<'a> {
    pub options: &'a StartOptions,
    /// The plane writing the file: [`plane_label`]. It is a property of the *build*, not of the host
    /// configuration - the native binary defaults to the public host config and still writes native files.
    pub host_label: &'a str,
    pub spec: Option<&'a DutySpec>,
    /// The current run's output - what a snapshot is written from.
    pub output: Option<&'a EngineOutput>,
    /// The fixture text (the recorded default input, for "New").
    pub fixture_text: &'a str,
}

impl FileSession {
    pub fn note(&mut self, line: impl Into<String>) {
        self.status = line.into();
    }

    /// Is there work that is not in a file?
    pub fn is_dirty(&self, input: &EngineInput, cat: &Catalog) -> bool {
        !self.saved_signature.is_empty()
            && signature(input, cat, &self.revision_id) != self.saved_signature
    }

    /// Is there native file work still outstanding? The CLI's headless loop waits on this.
    pub fn has_pending(&self) -> bool {
        self.pending_open.is_some()
            || self.pending_revision.is_some()
            || self.pending_save.is_some()
            || self.pending_json.is_some()
            || self.pending_csv.is_some()
            || !self.pending_compare.is_empty()
    }

    pub fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("untitled.{}", project::EXTENSION))
    }

    /// The recent list is native state only: the web-internal host has no paths to remember, so on wasm
    /// this method is compiled out rather than left as an unreachable body (issue #79 F3).
    #[cfg(not(target_arch = "wasm32"))]
    fn remember(&mut self, path: &PathBuf) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(RECENT_LIMIT);
    }

    /// The line the app shows (and the page publishes) about the loaded project.
    pub fn recompute_line(&self, output: Option<&EngineOutput>) -> String {
        match (self.loaded.as_deref(), output) {
            (Some(project), Some(output)) => project.results.delta(output).line(),
            (Some(_), None) => "no run yet".to_string(),
            (None, _) => "nothing loaded from a file yet".to_string(),
        }
    }
}

/// The fingerprint of the document: everything a project file carries that is not the results snapshot.
/// Two states with the same fingerprint are the same document.
pub fn signature(input: &EngineInput, cat: &Catalog, revision_id: &str) -> String {
    let mut out = format!(
        "rev={revision_id}|tower={}|fan={}|drift={}|nozzle={}|ratio={:.17}|duty={:.17},{:.17},{:.17},\
         {:.17},{:.17},{:.17},{:.17},{}|layers=",
        input.tower.id,
        input.fan.id,
        input.drift.id,
        input.nozzle.id,
        input.speed_ratio,
        input.duty.water_flow_m3_hr,
        input.duty.hot_water_c,
        input.duty.target_cold_water_c,
        input.duty.wet_bulb_c,
        input.duty.dry_bulb_c,
        input.duty.pressure_pa,
        input.duty.salinity_g_kg,
        input.duty.water_quality_class,
    );
    for layer in &input.fill_layers {
        out.push_str(&format!(
            "{}:{:.17}:{:.17}:{:.17};",
            layer.fill_id, layer.depth_m, layer.thermal_multiplier, layer.pressure_multiplier
        ));
    }
    out.push_str("|custom=");
    for part in &cat.custom {
        out.push_str(&format!("{}:{};", part.class, part.id()));
    }
    out
}

/// The recorded rows the project file carries: the display-only water-quality rows and the recorded
/// requirement limits, read from the shipped descriptor so a new row appears without a schema change.
pub fn recorded_rows(
    spec: Option<&DutySpec>,
) -> (BTreeMap<String, Option<f64>>, BTreeMap<String, f64>) {
    let Some(spec) = spec else {
        return (BTreeMap::new(), BTreeMap::new());
    };
    let recorded = spec
        .display_only
        .iter()
        .map(|row| (row.key.clone(), row.value))
        .collect();
    let limits = spec
        .limits
        .iter()
        .map(|row| (row.key.clone(), row.value.unwrap_or_default()))
        .collect();
    (recorded, limits)
}

/// The working state as a project file's text. The one writer, for both planes.
pub fn snapshot_text(
    ctx: &Ctx<'_>,
    session: &FileSession,
    input: &EngineInput,
    cat: &Catalog,
    engine: Option<&dyn Engine>,
) -> String {
    let (recorded, limits) = recorded_rows(ctx.spec);
    let revision_id = if session.revision_id.is_empty() {
        &cat.catalog_id
    } else {
        &session.revision_id
    };
    let mut project = Project::build(Snapshot {
        host: ctx.host_label,
        input,
        catalog_revision_id: revision_id,
        engine_name: engine.map(Engine::name).unwrap_or("not loaded"),
        custom_parts: &cat.custom,
        water_quality_recorded: recorded,
        limits_recorded: limits,
        output: ctx.output,
    });
    // Issue #79 F2: the app's own save path keeps the format's forward-compatibility promise. The file
    // just opened (or last written) is the source of what this build does not recognise, and every one
    // of those keys rides along into the fresh snapshot - so a file another build wrote survives
    // open -> save here exactly as it survives `read -> write` in [`crate::project`].
    if let Some(opened) = session.loaded.as_deref() {
        project.carry_unknowns(opened);
    }
    project.write()
}

// ------------------------------------------------------------------------------------------- commands

/// The bridge commands this module owns, in the form the page dispatches them (a payload may follow the
/// second colon). Anything else belongs to another module.
pub const COMMANDS: [&str; 4] = [
    "project:download",
    "project:upload",
    "revision:import",
    "project:new",
];

/// Apply one bridge command. `Err` is the refusal line: the caller shows it, and the page publishes it.
///
/// The web-internal host reaches this through the page's own buttons; the public host reaches a refusal,
/// which is the rule it has always had - it can author, it cannot save or export.
pub fn handle_command(
    command: &str,
    ctx: &Ctx<'_>,
    session: &mut FileSession,
    input: &mut EngineInput,
    cat: &mut Catalog,
    engine: &mut EngineSlot,
) -> Result<String, String> {
    if ctx.options.host.public {
        return Err(format!(
            "the public host cannot save or export: `{command}` is refused here (the rule is unchanged - \
             the public host authors, and the work it is doing lives in the page)"
        ));
    }
    let (head, arg) = match command.split_once(':') {
        Some((head, arg)) => (head, arg),
        None => (command, ""),
    };
    let (verb, payload) = match arg.split_once(':') {
        Some((verb, payload)) => (verb, Some(payload)),
        None => (arg, None),
    };
    let line = match (head, verb) {
        ("project", "new") => {
            reset_document(ctx.fixture_text, input, cat, session);
            "a new project: the working state is back to the recorded default".to_string()
        }
        ("project", "download") => download(ctx, session, input, cat, engine),
        ("project", "upload") => {
            let text = payload.ok_or_else(|| {
                "`project:upload` expects the file's text after the command".to_string()
            })?;
            open_text(text, ctx, input, cat, engine, session)?
        }
        ("revision", "import") => {
            let text = payload.ok_or_else(|| {
                "`revision:import` expects the file's text after the command".to_string()
            })?;
            import_text(text, cat, engine, session)?
        }
        _ => return Err(format!("`{command}` is not a file command")),
    };
    session.note(line.clone());
    Ok(line)
}

/// The web-internal *Download project*: the text goes into the session's outbox and the mirror publishes
/// it for the page to hand to the user. No dialog, no file system, no network.
pub fn download(
    ctx: &Ctx<'_>,
    session: &mut FileSession,
    input: &EngineInput,
    cat: &Catalog,
    engine: &EngineSlot,
) -> String {
    let text = snapshot_text(ctx, session, input, cat, engine.0.as_deref());
    let name = session.file_name();
    session.outbox = text;
    session.outbox_name = name.clone();
    format!(
        "the project file is ready to download ({name}, {} bytes) - the page hands it to you; nothing is \
         sent anywhere",
        session.outbox.len()
    )
}

// ------------------------------------------------------------------------------------------- the state

/// Back to a fresh document: the fixture's recorded default input, no custom records, no file.
pub fn reset_document(
    fixture_text: &str,
    input: &mut EngineInput,
    cat: &mut Catalog,
    session: &mut FileSession,
) {
    if let Ok(file) = FixtureEngine::from_json(fixture_text) {
        *input = file.default_input();
    }
    cat.custom.clear();
    session.path = None;
    session.loaded = None;
    session.outbox.clear();
    session.saved_signature = signature(input, cat, &session.revision_id);
}

/// Apply a project file's text: the duty, the stack, the ratio, the fitted records (by id, refused by
/// name when the catalog does not carry one), and the custom records the file carries.
pub fn open_text(
    text: &str,
    ctx: &Ctx<'_>,
    input: &mut EngineInput,
    cat: &mut Catalog,
    engine: &mut EngineSlot,
    session: &mut FileSession,
) -> Result<String, String> {
    let project = Project::read(text)?;
    if !project.catalog_revision_id.is_empty() && project.catalog_revision_id != session.revision_id
    {
        return Err(format!(
            "this project was written against catalog revision `{}`; this session is pinned to `{}` - \
             import that revision (`File > Import catalog revision…`) and open the project again",
            project.catalog_revision_id, session.revision_id
        ));
    }
    let tower = cat
        .tower(&project.tower)
        .ok_or_else(|| format!("tower `{}` is not in the catalog revision", project.tower))?;
    let fan = cat.fan(&project.fitted.fan).ok_or_else(|| {
        format!(
            "fan `{}` is not in the catalog revision",
            project.fitted.fan
        )
    })?;
    let drift = cat.drift(&project.fitted.drift).ok_or_else(|| {
        format!(
            "drift `{}` is not in the catalog revision",
            project.fitted.drift
        )
    })?;
    let nozzle = cat.nozzle(&project.fitted.nozzle).ok_or_else(|| {
        format!(
            "nozzle `{}` is not in the catalog revision",
            project.fitted.nozzle
        )
    })?;

    input.tower = tower.clone();
    input.fan = fan.clone();
    input.drift = drift.clone();
    input.nozzle = nozzle.clone();
    project.write_into(input);

    // Custom records live in the file, so an open replaces the session's own set instead of adding to it:
    // otherwise opening two projects would accumulate one project's records inside the other.
    let parts = project.custom_parts()?;
    let fields = crate::state::FieldsRes::load().fields;
    cat.custom.clear();
    for part in &parts {
        let Some(fields) = fields.as_ref() else {
            return Err(
                "the custom-part field list did not load, so this file's own records cannot be rebuilt"
                    .to_string(),
            );
        };
        add_custom(cat, engine, part, fields).map_err(|e| {
            format!(
                "custom record `{}` (this file's own record): {e}",
                part.id()
            )
        })?;
    }
    let _ = ctx;
    session.path = None;
    session.outbox.clear();
    session.loaded = Some(Box::new(project));
    session.saved_signature = signature(input, cat, &session.revision_id);
    Ok(format!(
        "opened the project: {} custom record(s) restored - the saved results are recomputed on open, \
         never trusted",
        parts.len()
    ))
}

/// The one way a custom record enters the catalog: the engine's own path (`Catalog::add_custom`), so a
/// restored record takes exactly the route a freshly authored one takes.
pub fn add_custom(
    cat: &mut Catalog,
    engine: &mut EngineSlot,
    part: &CustomPart,
    fields: &Fields,
) -> Result<(), String> {
    cat.add_custom(part, fields, engine.0.as_deref_mut())
        .map(|_| ())
}

/// Verify and install a catalog revision. The revision's own digest decides: a file that does not hash to
/// its declared `sha256` is refused by [`Revision::read`], naming both digests.
pub fn import_text(
    text: &str,
    cat: &mut Catalog,
    engine: &mut EngineSlot,
    session: &mut FileSession,
) -> Result<String, String> {
    let revision = Revision::read(text)?;
    install(&revision, cat, engine)?;
    session.revision_id = revision.id.clone();
    session.revision_status = revision.status.clone();
    // The document just changed underneath the draft: everything is "unsaved" until it is saved again.
    session.saved_signature.clear();
    session.loaded = None;
    Ok(format!(
        "imported catalog revision `{}` ({}): every record now comes from that file, and the digest \
         verified",
        revision.id, revision.status
    ))
}

/// Swap the catalog the whole app reads: the view's records and the engine's own catalog.
pub fn install(
    revision: &Revision,
    cat: &mut Catalog,
    engine: &mut EngineSlot,
) -> Result<(), String> {
    let records = revision.records();
    let block: cockpit::fixture_engine::FixtureCatalog = serde_json::from_value(records.clone())
        .map_err(|e| format!("catalog revision `{}`: record block: {e}", revision.id))?;
    let ambient = cat.ambient;
    *cat = Catalog::from_catalog_block(&block);
    cat.ambient = ambient;
    cat.custom.clear();

    #[cfg(feature = "real-engine")]
    {
        let catalog = crate::engine_catalog::catalog_from_records(records)?;
        engine.0 = Some(Box::new(crate::adapter_bridge::AdapterEngine::new(catalog)));
        let _ = &block;
    }
    #[cfg(not(feature = "real-engine"))]
    {
        let Some(file) = engine
            .0
            .as_deref_mut()
            .and_then(Engine::fixture_catalog_mut)
        else {
            return Err(
                "this build carries the recorded replay, and its catalog could not be swapped"
                    .to_string(),
            );
        };
        file.catalog = block;
        file.provenance.catalog = cockpit::fixture_engine::CatalogMeta {
            id: revision.id.clone(),
            revision: revision.published_at.clone(),
            status: revision.status.clone(),
        };
    }
    Ok(())
}

/// The revision shipped in this build, as the session's starting pin.
pub fn shipped_revision() -> Option<&'static Revision> {
    revision::shipped().as_ref().ok()
}

/// The exports: `(json, csv)`, from the run's own output.
pub fn export_texts(output: &EngineOutput) -> (String, String) {
    (project::results_json(output), project::results_csv(output))
}

// ------------------------------------------------------------------------------ native: the dialogs

/// The native half: `rfd` dialogs and the file system. Not compiled into the wasm module at all - the
/// web-internal host goes through the page's download/upload instead.
#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// The project file filter both dialogs share.
    fn filter() -> rfd::FileDialog {
        rfd::FileDialog::new()
            .add_filter("Drafthouse project", &[project::EXTENSION])
            .add_filter("JSON", &["json"])
    }

    /// `File > Open…` and `File > Import catalog revision…` - the platform's own dialog.
    pub fn pick_open(title: &str) -> Option<PathBuf> {
        filter().set_title(title).pick_file()
    }

    /// `File > Save As…`, and the first save of a document that has no path yet.
    pub fn pick_save(title: &str, default_name: &str, directory: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = filter().set_title(title).set_file_name(default_name);
        if let Some(directory) = directory {
            dialog = dialog.set_directory(directory);
        }
        dialog.save_file()
    }

    /// `File > Export results…`: where the export goes.
    pub fn pick_export(title: &str, default_name: &str, extension: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(default_name)
            .add_filter(extension.to_uppercase(), &[extension])
            .save_file()
    }

    pub fn read_text(path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

// ------------------------------------------------------------------------- the File menu (native)

/// The native `File` menu, drawn in the header's third row (desktop only - the region already has the
/// room, so no other row of the layout moves). Returns what was picked; the caller applies it.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_menu(ui: &mut egui::Ui, session: &FileSession, dirty: bool) -> Option<Request> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.menu_button("File", |ui| {
            let items: [(&str, Request); 7] = [
                ("New", Request::New),
                ("Open…", Request::Open),
                ("Save", Request::Save),
                ("Save As…", Request::SaveAs),
                ("Import catalog revision…", Request::ImportRevision),
                ("Export results JSON…", Request::ExportJson),
                ("Export results CSV…", Request::ExportCsv),
            ];
            for (label, request) in items {
                if ui.button(label).clicked() {
                    picked = Some(request);
                    ui.close();
                }
            }
            if !session.recent.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("recent").small());
                for path in &session.recent {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    if ui
                        .button(name)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        picked = Some(Request::OpenPath(path.clone()));
                        ui.close();
                    }
                }
            }
        });
        let name = session.file_name();
        ui.label(
            egui::RichText::new(format!("{name}{}", if dirty { " •" } else { "" }))
                .size(11.0)
                .color(egui::Color32::from_rgb(0x7f, 0x91, 0x9c)),
        );
        if !session.status.is_empty() {
            ui.label(
                egui::RichText::new(&session.status)
                    .size(11.0)
                    .color(egui::Color32::from_rgb(0xb4, 0xc2, 0xcb)),
            );
        }
    });
    picked
}

/// The header's File row: the menu, the prompt, and the request they produce - the one call the UI makes.
#[cfg(not(target_arch = "wasm32"))]
pub fn menu_row(
    ui: &mut egui::Ui,
    ctx: &Ctx<'_>,
    session: &mut FileSession,
    input: Option<&mut EngineInput>,
    cat: Option<&mut Catalog>,
    engine: &mut EngineSlot,
) {
    let dirty = match (input.as_deref(), cat.as_deref()) {
        (Some(input), Some(cat)) => session.is_dirty(input, cat),
        _ => false,
    };
    // One answer per frame: either the menu was used, or the prompt it raised was answered.
    if let Some(request) = file_menu(ui, session, dirty).or_else(|| unsaved_prompt(ui, session)) {
        apply_request(request, ctx, session, input, cat, engine);
    }
}

/// The unsaved-changes prompt: shown when New / Open / Import would throw work away. Returns the answer.
#[cfg(not(target_arch = "wasm32"))]
pub fn unsaved_prompt(ui: &mut egui::Ui, session: &FileSession) -> Option<Request> {
    let prompt = session.prompt.as_ref()?;
    let what = match prompt.then {
        Request::New => "a new project",
        Request::Open | Request::OpenPath(_) => "another project",
        Request::ImportRevision => "another catalog revision",
        _ => "that",
    };
    let mut answer = None;
    egui::Window::new("unsaved changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            ui.label(format!(
                "`{}` has changes that are not in a file. Opening {what} discards them.",
                session.file_name()
            ));
            ui.horizontal(|ui| {
                if ui.button("Discard and continue").clicked() {
                    answer = Some(Request::ConfirmDiscard);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Request::Cancel);
                }
            });
        });
    answer
}

/// Apply a File-menu request. Native only: every branch ends in a dialog or a file write.
#[cfg(not(target_arch = "wasm32"))]
pub fn apply_request(
    request: Request,
    ctx: &Ctx<'_>,
    session: &mut FileSession,
    input: Option<&mut EngineInput>,
    cat: Option<&mut Catalog>,
    engine: &mut EngineSlot,
) {
    let (Some(input), Some(cat)) = (input, cat) else {
        session.note("the engine is still loading - nothing to do with a file yet");
        return;
    };
    // A request that would throw work away asks first - once, then it is honoured.
    match &request {
        Request::New | Request::Open | Request::ImportRevision | Request::OpenPath(_) => {
            if session.is_dirty(input, cat) {
                session.prompt = Some(Prompt {
                    then: request.clone(),
                });
                return;
            }
        }
        Request::Cancel => {
            session.prompt = None;
            session.note("kept the working state");
            return;
        }
        Request::ConfirmDiscard => {
            let Some(prompt) = session.prompt.take() else {
                return;
            };
            // The prompt was asked, and answered: run the request with nothing left to protect.
            session.saved_signature.clear();
            apply_request(prompt.then, ctx, session, Some(input), Some(cat), engine);
            return;
        }
        _ => {}
    }

    let done = match request {
        Request::New => {
            reset_document(ctx.fixture_text, input, cat, session);
            session.note("a new project: the recorded default, nothing in a file yet");
            Ok(())
        }
        Request::Open => match native::pick_open("Open a project") {
            Some(path) => open_path(&path, ctx, input, cat, engine, session),
            None => {
                session.note("open cancelled");
                Ok(())
            }
        },
        Request::OpenPath(path) => open_path(&path, ctx, input, cat, engine, session),
        Request::Save => save(ctx, session, input, cat, engine, false),
        Request::SaveAs => save(ctx, session, input, cat, engine, true),
        Request::ImportRevision => match native::pick_open("Import a catalog revision") {
            Some(path) => import_path(&path, cat, engine, session),
            None => {
                session.note("import cancelled");
                Ok(())
            }
        },
        Request::ExportJson => export(ctx, session, true),
        Request::ExportCsv => export(ctx, session, false),
        Request::Cancel | Request::ConfirmDiscard => Ok(()),
    };
    if let Err(error) = done {
        session.note(format!("refused: {error}"));
        eprintln!("drafthouse: refused: {error}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_path(
    path: &std::path::Path,
    ctx: &Ctx<'_>,
    input: &mut EngineInput,
    cat: &mut Catalog,
    engine: &mut EngineSlot,
    session: &mut FileSession,
) -> Result<(), String> {
    let text = native::read_text(path)?;
    let line = open_text(&text, ctx, input, cat, engine, session)?;
    session.path = Some(path.to_path_buf());
    session.remember(&path.to_path_buf());
    session.note(format!("{line} ({})", path.display()));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn import_path(
    path: &std::path::Path,
    cat: &mut Catalog,
    engine: &mut EngineSlot,
    session: &mut FileSession,
) -> Result<(), String> {
    // One reader for a revision: the text goes to `import_text`, which verifies the declared digest
    // before anything is installed. Nothing here reads a revision a second time or trusts it early.
    let text = native::read_text(path)?;
    let line = import_text(&text, cat, engine, session)?;
    session.note(format!("{line} ({})", path.display()));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn save(
    ctx: &Ctx<'_>,
    session: &mut FileSession,
    input: &EngineInput,
    cat: &Catalog,
    engine: &EngineSlot,
    force_pick: bool,
) -> Result<(), String> {
    let path = match (&session.path, force_pick) {
        (Some(path), false) => path.clone(),
        _ => {
            let directory = session
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf());
            match native::pick_save(
                "Save the project",
                &session.file_name(),
                directory.as_deref(),
            ) {
                Some(path) => path,
                None => {
                    session.note("save cancelled");
                    return Ok(());
                }
            }
        }
    };
    let text = snapshot_text(ctx, session, input, cat, engine.0.as_deref());
    native::write_text(&path, &text)?;
    session.path = Some(path.clone());
    session.remember(&path);
    session.saved_signature = signature(input, cat, &session.revision_id);
    session.loaded = Project::read(&text).ok().map(Box::new);
    session.note(format!(
        "saved {} ({} bytes, snapshot {})",
        path.display(),
        text.len(),
        if ctx.output.is_some() {
            "as calculated on save"
        } else {
            "empty - no run yet"
        }
    ));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn export(ctx: &Ctx<'_>, session: &mut FileSession, json: bool) -> Result<(), String> {
    let Some(output) = ctx.output else {
        return Err("there is no run to export yet".to_string());
    };
    let (json_text, csv_text) = export_texts(output);
    let (extension, text) = if json {
        ("json", json_text)
    } else {
        ("csv", csv_text)
    };
    let stem = session
        .path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "drafthouse-results".to_string());
    let default = format!("{stem}-results.{extension}");
    let Some(path) = native::pick_export(
        if json {
            "Export the results as JSON"
        } else {
            "Export the results as CSV"
        },
        &default,
        extension,
    ) else {
        session.note("export cancelled");
        return Ok(());
    };
    native::write_text(&path, &text)?;
    session.note(format!(
        "exported {} ({} bytes) - the snapshot's own caption travels with it",
        path.display(),
        text.len()
    ));
    Ok(())
}

// ------------------------------------------------------------------ the systems (shared, then native)

/// The bridge commands this session owns, split out of the shared queue so the page's file buttons work
/// whether they are drained before or after the staging commands: each system takes only its own heads.
#[allow(clippy::too_many_arguments)]
pub fn file_commands(
    mut session: ResMut<FileSession>,
    mut draft: Option<ResMut<crate::app::Draft>>,
    mut catalog: Option<ResMut<Catalog>>,
    mut engine: ResMut<EngineSlot>,
    options: Res<StartOptions>,
    duty: Res<crate::duty_panel::DutyRes>,
    run: Res<crate::state::Run>,
    fixture_text: Res<crate::app::FixtureText>,
) {
    // Nothing is drained while the app is still loading: a command that arrived early has to wait for the
    // engine, calendar and draft it needs, not be dropped because it was early.
    if draft.is_none() || catalog.is_none() {
        return;
    }
    let mut mine: Vec<String> = Vec::new();
    {
        let mut inbox = crate::app::COMMANDS.lock().unwrap();
        if inbox.is_empty() {
            return;
        }
        let taken: Vec<String> = inbox.drain(..).collect();
        for command in taken {
            let head = command
                .split_once(':')
                .map(|(head, _)| head)
                .unwrap_or(command.as_str());
            if head == "project" || head == "revision" {
                mine.push(command);
            } else {
                inbox.push(command);
            }
        }
    }
    if mine.is_empty() {
        return;
    }
    let (Some(draft), Some(catalog)) = (draft.as_deref_mut(), catalog.as_deref_mut()) else {
        return;
    };
    let ctx = Ctx {
        options: &options,
        host_label: plane_label(),
        spec: duty.spec.as_ref(),
        output: run.output.as_ref(),
        fixture_text: &fixture_text.0,
    };
    for command in mine {
        match handle_command(
            &command,
            &ctx,
            &mut session,
            &mut draft.0,
            catalog,
            &mut engine,
        ) {
            Ok(line) => session.note(line),
            Err(refusal) => {
                session.note(refusal.clone());
                eprintln!("drafthouse: {refusal}");
            }
        }
    }
}

/// Issue #117: one saved project file as the comparison's own command - the native `--compare` route
/// reads the file here (where the file system is) and hands the comparison screen
/// `compare:open:<name>:<text>`; the web plane's page hands over the same command, built from the
/// text its own file picker read. The native route and the parity test that pins the web route's
/// export against it both build the command through this function, so the two cannot drift apart.
///
/// A `:` inside the *name* is written as `_` - the name is the column's label, not its identity
/// (the digest is) - because the command's name field ends at the first `:`; the text keeps every
/// colon it carries. Returns `(name, command)` so the caller can also print the name.
#[cfg(not(target_arch = "wasm32"))]
pub fn compare_file_command(path: &std::path::Path) -> Result<(String, String), String> {
    let text = native::read_text(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
        .replace(':', "_");
    Ok((name.clone(), format!("compare:open:{name}:{text}")))
}

/// The native command line's file work: open / import before the run, then the save and the exports after
/// it. This is also the **headless** path an evidence run uses - it runs the engine itself when the UI is
/// not going to (there is no window, so no UI pass, so nothing else would produce the run the snapshot
/// needs). Every line it prints goes to stdout for the log.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub fn file_work(
    mut session: ResMut<FileSession>,
    mut draft: Option<ResMut<crate::app::Draft>>,
    mut catalog: Option<ResMut<Catalog>>,
    mut engine: ResMut<EngineSlot>,
    mut run: ResMut<crate::state::Run>,
    load: Res<crate::app::Load>,
    options: Res<StartOptions>,
    duty: Res<crate::duty_panel::DutyRes>,
    fixture_text: Res<crate::app::FixtureText>,
) {
    if !load.ready {
        return;
    }
    let pending = session.pending_revision.is_some()
        || session.pending_open.is_some()
        || session.pending_save.is_some()
        || session.pending_json.is_some()
        || session.pending_csv.is_some()
        || !session.pending_compare.is_empty();
    if !pending {
        return;
    }
    let (Some(draft), Some(catalog)) = (draft.as_deref_mut(), catalog.as_deref_mut()) else {
        return;
    };

    // Issue #89: `--compare` - read each saved project file and hand it to the comparison screen as
    // one `compare:open:<name>:<text>` command per file; the screen opens it through the reader
    // (against `catalog` there) and the engine runs it. The text is everything after the name's
    // first `:`, so a file's own colons travel untouched; a `:` inside the *name* is written as `_`
    // (the name is the column's label, not its identity - the digest is).
    if !session.pending_compare.is_empty() {
        let files: Vec<PathBuf> = session.pending_compare.drain(..).collect();
        if let Ok(mut q) = crate::app::COMMANDS.lock() {
            q.push("screen:compare".to_string());
            for path in files {
                match compare_file_command(&path) {
                    Ok((name, command)) => {
                        println!("drafthouse: comparison: {name} ({})", path.display());
                        q.push(command);
                    }
                    Err(error) => {
                        session.refused = true;
                        eprintln!(
                            "drafthouse: refused: comparison file {}: {error}",
                            path.display()
                        );
                    }
                }
            }
        }
    }

    if let Some(path) = session.pending_revision.take() {
        match native::read_text(&path)
            .and_then(|text| import_text(&text, catalog, &mut engine, &mut session))
        {
            Ok(line) => println!("drafthouse: {line}"),
            Err(error) => {
                session.refused = true;
                eprintln!("drafthouse: refused: {error}");
            }
        }
    }
    if let Some(path) = session.pending_open.take() {
        let ctx = Ctx {
            options: &options,
            host_label: plane_label(),
            spec: duty.spec.as_ref(),
            output: None,
            fixture_text: &fixture_text.0,
        };
        match native::read_text(&path).and_then(|text| {
            open_text(
                &text,
                &ctx,
                &mut draft.0,
                catalog,
                &mut engine,
                &mut session,
            )
        }) {
            Ok(line) => {
                session.path = Some(path.clone());
                session.remember(&path);
                println!("drafthouse: {line} ({})", path.display());
            }
            Err(error) => {
                session.refused = true;
                eprintln!("drafthouse: refused: {error}");
            }
        }
    }

    // The run the snapshot needs, when no UI pass will produce it.
    let wants_run = session.pending_save.is_some()
        || session.pending_json.is_some()
        || session.pending_csv.is_some();
    if wants_run && run.output.is_none() {
        if let Some(engine) = engine.0.as_ref() {
            match engine.run(&draft.0) {
                Ok(output) => {
                    run.output = Some(output);
                    run.error = None;
                }
                Err(error) => {
                    run.output = None;
                    run.error = Some(error);
                    eprintln!("drafthouse: the engine refused the loaded state");
                }
            }
            run.ratio = draft.0.speed_ratio;
        }
    }

    let ctx = Ctx {
        options: &options,
        host_label: plane_label(),
        spec: duty.spec.as_ref(),
        output: run.output.as_ref(),
        fixture_text: &fixture_text.0,
    };
    if let Some(path) = session.pending_save.take() {
        // `--project-save <path>` is the menu's *Save As* with the path supplied, so it wins over whatever
        // path a `--project-open` set a moment ago.
        session.path = Some(path);
        match save(&ctx, &mut session, &draft.0, catalog, &engine, false) {
            Ok(()) => println!("drafthouse: {}", session.status),
            Err(error) => {
                session.refused = true;
                eprintln!("drafthouse: refused: {error}");
            }
        }
        if let Some(output) = run.output.as_ref() {
            println!("drafthouse: engine {}", output.provenance.engine);
            for (headline, value) in cockpit::engine::HEADLINES
                .iter()
                .zip(output.headline_values())
            {
                println!(
                    "drafthouse: headline {},{}={}",
                    headline.name, headline.unit, value
                );
            }
            let delta = session.recompute_line(Some(output));
            println!("drafthouse: {delta}");
        }
    }
    for json in [true, false] {
        let pending = if json {
            session.pending_json.is_some()
        } else {
            session.pending_csv.is_some()
        };
        if !pending {
            continue;
        }
        let Some(output) = run.output.as_ref() else {
            continue;
        };
        let (json_text, csv_text) = export_texts(output);
        let (extension, text) = if json {
            ("json", json_text)
        } else {
            ("csv", csv_text)
        };
        let path = if json {
            session.pending_json.take()
        } else {
            session.pending_csv.take()
        };
        if let Some(path) = path {
            match native::write_text(&path, &text) {
                Ok(()) => println!(
                    "drafthouse: exported {} ({} bytes, {extension})",
                    path.display(),
                    text.len()
                ),
                Err(error) => {
                    session.refused = true;
                    eprintln!("drafthouse: refused: {error}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::fixture_engine::FixtureEngine;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    fn fixture() -> FixtureEngine {
        FixtureEngine::from_json(FIXTURE).expect("the fixture parses")
    }

    fn internal_ctx() -> Ctx<'static> {
        // The two &'static references are the two embedded assets; `output` is supplied per test.
        Ctx {
            options: Box::leak(Box::new(StartOptions {
                host: cockpit::host::HostConfig {
                    public: false,
                    branding: None,
                    catalog_label: None,
                },
                ..StartOptions::default()
            })),
            host_label: HOST_WEB_INTERNAL,
            spec: None,
            output: None,
            fixture_text: FIXTURE,
        }
    }

    /// A session's signature moves when the machine moves, and only then.
    #[test]
    fn the_signature_is_the_document() {
        let engine = fixture();
        let input = engine.default_input();
        let cat = Catalog::from_fixture(&engine);
        let base = signature(&input, &cat, "illustrative-catalog-v0.1");

        let mut nudged = input.clone();
        nudged.speed_ratio += 0.01;
        assert_ne!(base, signature(&nudged, &cat, "illustrative-catalog-v0.1"));

        let mut edited = input.clone();
        edited.duty.hot_water_c = 41.0;
        assert_ne!(base, signature(&edited, &cat, "illustrative-catalog-v0.1"));

        let mut restacked = input.clone();
        restacked.fill_layers[0].depth_m += 0.05;
        assert_ne!(
            base,
            signature(&restacked, &cat, "illustrative-catalog-v0.1")
        );

        let mut refitted = input.clone();
        refitted.fan.id = "AX-420".to_string();
        assert_ne!(
            base,
            signature(&refitted, &cat, "illustrative-catalog-v0.1")
        );

        assert_ne!(
            base,
            signature(&input, &cat, "some-other-revision"),
            "the pinned revision is part of the document"
        );
        assert_eq!(base, signature(&input, &cat, "illustrative-catalog-v0.1"));
    }

    /// The public host refuses every file command by name - the page's own buttons are not the only gate.
    #[test]
    fn the_public_host_refuses_every_file_command() {
        let engine = fixture();
        let mut input = engine.default_input();
        let mut cat = Catalog::from_fixture(&engine);
        let mut session = FileSession::default();
        let options = StartOptions {
            host: cockpit::host::HostConfig {
                public: true,
                branding: None,
                catalog_label: None,
            },
            ..StartOptions::default()
        };
        let ctx = Ctx {
            options: &options,
            host_label: HOST_WEB_INTERNAL,
            spec: None,
            output: None,
            fixture_text: FIXTURE,
        };
        let mut slot = EngineSlot::default();
        for command in COMMANDS {
            let error = handle_command(
                &format!("{command}:{{}}"),
                &ctx,
                &mut session,
                &mut input,
                &mut cat,
                &mut slot,
            )
            .expect_err("the public host refuses");
            assert!(error.contains("public host"), "{command}: {error}");
            assert!(
                error.contains("cannot save or export"),
                "{command}: {error}"
            );
        }
    }

    /// The web-internal host reaches the same functions the native menu does.
    #[test]
    fn the_internal_host_reaches_the_file_functions() {
        let engine = fixture();
        let mut input = engine.default_input();
        let mut cat = Catalog::from_fixture(&engine);
        let mut session = FileSession {
            revision_id: revision::SHIPPED_ID.to_string(),
            ..FileSession::default()
        };
        let mut slot = EngineSlot::default();
        let ctx = internal_ctx();

        // Download: a project file's text, handed to the page, and nothing else happens.
        let line = handle_command(
            "project:download",
            &ctx,
            &mut session,
            &mut input,
            &mut cat,
            &mut slot,
        )
        .expect("the internal host may download a project");
        assert!(line.contains("ready to download"), "{line}");
        let project =
            Project::read(&session.outbox).expect("what the page downloads is a project file");
        assert_eq!(project.saved_by.host, HOST_WEB_INTERNAL);
        assert_eq!(project.catalog_revision_id, revision::SHIPPED_ID);
        assert_eq!(project.format_version, project::FORMAT_VERSION);
        assert!(session.outbox_name.ends_with(".drafthouse"));

        // Upload: the page hands back the same text, and the state matches the file afterwards.
        let mut other = engine.default_input();
        other.speed_ratio = 0.9;
        other.duty.hot_water_c = 39.0;
        let moved = snapshot_text(&ctx, &session, &other, &cat, None);
        let line = handle_command(
            &format!("project:upload:{moved}"),
            &ctx,
            &mut session,
            &mut input,
            &mut cat,
            &mut slot,
        )
        .expect("the internal host may upload a project");
        assert!(line.starts_with("opened the project"), "{line}");
        assert_eq!(input.speed_ratio, 0.9);
        assert_eq!(input.duty.hot_water_c, 39.0);
        assert!(
            session.loaded.is_some(),
            "the loaded project is kept, for the recompute line"
        );
        assert!(!session.is_dirty(&input, &cat));

        // Import: a revision whose bytes do not match its declared digest is refused by name.
        let tampered = revision::SHIPPED_TEXT.replace("FILM-MF20", "FILM-MF21");
        let error = handle_command(
            &format!("revision:import:{tampered}"),
            &ctx,
            &mut session,
            &mut input,
            &mut cat,
            &mut slot,
        )
        .expect_err("the tampered revision is refused");
        assert!(error.contains("sha256 mismatch"), "{error}");
        assert!(error.contains(revision::SHIPPED_ID), "{error}");
        assert!(
            error.contains(&revision::shipped().as_ref().expect("verifies").sha256),
            "{error}"
        );

        // And the shipped revision itself imports cleanly (no engine in this test, so the engine slot
        // stays empty and the catalog is what moves).
        let line = handle_command(
            &format!("revision:import:{}", revision::SHIPPED_TEXT),
            &ctx,
            &mut session,
            &mut input,
            &mut cat,
            &mut slot,
        )
        .expect("the shipped revision imports");
        assert!(line.contains("digest verified"), "{line}");
        assert_eq!(session.revision_id, revision::SHIPPED_ID);
        assert_eq!(cat.catalog_id, revision::SHIPPED_ID);
    }

    /// Issue #79 F2: the app's save path keeps the format's forward-compatibility promise - the same
    /// guarantee `project::tests::unknown_fields_survive_a_round_trip_at_every_level` pins for
    /// `read -> write`, held by open -> save (the path `File > Save` and *download project* both take).
    #[test]
    fn the_app_save_path_keeps_the_unknown_keys_of_the_opened_file() {
        let engine = fixture();
        let input = engine.default_input();
        let output = engine.run(&input).expect("the recorded duty runs");
        let cat = Catalog::from_fixture(&engine);
        let ctx = Ctx {
            options: internal_ctx().options,
            host_label: HOST_WEB_INTERNAL,
            spec: None,
            output: Some(&output),
            fixture_text: FIXTURE,
        };
        let mut session = FileSession {
            revision_id: revision::SHIPPED_ID.to_string(),
            ..FileSession::default()
        };

        // A file "another build wrote": an unknown key at every level this file has.
        let mut value: serde_json::Value =
            serde_json::from_str(&snapshot_text(&ctx, &session, &input, &cat, Some(&engine)))
                .expect("the snapshot parses");
        let plant = |value: &mut serde_json::Value, path: &[&str], key: &str| {
            let mut at = value;
            for step in path {
                at = match step.parse::<usize>() {
                    Ok(index) => at.get_mut(index).expect("the path exists"),
                    Err(_) => at.get_mut(*step).expect("the path exists"),
                };
            }
            at.as_object_mut()
                .expect("the path lands on an object")
                .insert(key.to_string(), serde_json::json!({ "kept": true, "n": 7 }));
        };
        let levels: &[&[&str]] = &[
            &[],
            &["savedBy"],
            &["engine"],
            &["duty"],
            &["site"],
            &["fillLayers", "0"],
            &["fitted"],
            &["results"],
            &["results", "headlines", "0"],
            &["results", "layers", "0"],
        ];
        for (index, path) in levels.iter().enumerate() {
            plant(&mut value, path, &format!("fromTheFuture{index}"));
        }
        let text = serde_json::to_string_pretty(&value).expect("serialises") + "\n";

        // Open it (the state comes back), then save without touching anything.
        let mut opened_input = engine.default_input();
        let mut opened_cat = Catalog::from_fixture(&engine);
        let mut slot = EngineSlot::default();
        open_text(
            &text,
            &ctx,
            &mut opened_input,
            &mut opened_cat,
            &mut slot,
            &mut session,
        )
        .expect("the app opens the file");
        let saved = snapshot_text(&ctx, &session, &opened_input, &opened_cat, None);

        // Every unknown key is still there, at its own path, with its value intact.
        let at: serde_json::Value = serde_json::from_str(&saved).expect("the save is JSON");
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
                Some(&serde_json::json!({ "kept": true, "n": 7 })),
                "`fromTheFuture{index}` at {path:?} was dropped by open -> save"
            );
        }
    }

    /// A project that names a different revision is refused by name rather than half-applied.
    #[test]
    fn a_project_from_another_revision_is_refused() {
        let engine = fixture();
        let mut input = engine.default_input();
        let mut cat = Catalog::from_fixture(&engine);
        let mut session = FileSession {
            revision_id: revision::SHIPPED_ID.to_string(),
            ..FileSession::default()
        };
        let mut slot = EngineSlot::default();
        let ctx = internal_ctx();
        let text = snapshot_text(&ctx, &session, &input, &cat, None)
            .replace(revision::SHIPPED_ID, "some-other-catalog-v9");
        let error = open_text(&text, &ctx, &mut input, &mut cat, &mut slot, &mut session)
            .expect_err("a different revision is refused");
        assert!(error.contains("some-other-catalog-v9"), "{error}");
        assert!(error.contains("Import catalog revision"), "{error}");
    }

    /// "New" really is the recorded default again, and it leaves the session clean.
    #[test]
    fn a_new_document_is_the_recorded_default() {
        let engine = fixture();
        let recorded = engine.default_input();
        let mut input = engine.default_input();
        input.speed_ratio = 0.95;
        input.duty.hot_water_c = 38.0;
        let mut cat = Catalog::from_fixture(&engine);
        let mut session = FileSession {
            revision_id: revision::SHIPPED_ID.to_string(),
            path: Some(PathBuf::from("/tmp/whatever.drafthouse")),
            ..FileSession::default()
        };
        session.saved_signature = signature(&recorded, &cat, &session.revision_id);
        assert!(session.is_dirty(&input, &cat));
        reset_document(FIXTURE, &mut input, &mut cat, &mut session);
        assert_eq!(input.speed_ratio, recorded.speed_ratio);
        assert_eq!(input.duty.hot_water_c, recorded.duty.hot_water_c);
        assert!(session.path.is_none());
        assert!(!session.is_dirty(&input, &cat));
    }

    /// The recorded rows the file carries come from the shipped descriptor, not from a second list.
    #[test]
    fn the_recorded_rows_are_the_descriptor_s_own() {
        let spec = DutySpec::from_json(include_str!("../assets/duty-fields.json")).expect("parses");
        let (recorded, limits) = recorded_rows(Some(&spec));
        assert_eq!(recorded.len(), spec.display_only.len());
        assert!(recorded.contains_key("tds"));
        assert_eq!(limits.len(), spec.limits.len());
        assert_eq!(limits["maxDriftPpm"], 30.0);
        let (empty, none) = recorded_rows(None);
        assert!(empty.is_empty() && none.is_empty());
    }

    /// The recompute line is the snapshot's own measurement, and it says which engine disagreed.
    #[test]
    fn the_recompute_line_measures_the_snapshot() {
        let engine = fixture();
        let input = engine.default_input();
        let output = engine.run(&input).expect("the recorded duty runs");
        let cat = Catalog::from_fixture(&engine);
        let ctx = Ctx {
            options: internal_ctx().options,
            host_label: HOST_WEB_INTERNAL,
            spec: None,
            output: Some(&output),
            fixture_text: FIXTURE,
        };
        let mut session = FileSession {
            revision_id: revision::SHIPPED_ID.to_string(),
            ..FileSession::default()
        };
        let text = snapshot_text(&ctx, &session, &input, &cat, Some(&engine));
        let project = Project::read(&text).expect("reads");
        session.loaded = Some(Box::new(project));
        let line = session.recompute_line(Some(&output));
        assert!(line.contains("match the saved snapshot"), "{line}");

        let mut moved = output.clone();
        moved.fan_power_kw += 1.0;
        let line = session.recompute_line(Some(&moved));
        assert!(line.contains("fan power"), "{line}");
        assert!(line.contains("differs"), "{line}");
        assert_eq!(session.recompute_line(None), "no run yet");
    }

    /// Every command this module owns is a file command, and the picker's are not claimed here.
    #[test]
    fn the_command_heads_are_the_file_commands() {
        for command in COMMANDS {
            assert!(
                command.starts_with("project:") || command.starts_with("revision:"),
                "{command}"
            );
        }
        assert!(!COMMANDS.contains(&"picker:open"));
    }
}
