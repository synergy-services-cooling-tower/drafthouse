//! Issue #139: the tabs read one project. Every assertion here goes through a screen's own `ui` (a
//! real egui pass, the strings and controls it drew), the session's own file writer and reader, the
//! PDF writer or the comparison's reader - never a value recomputed for the test.
//! Test-only, real-engine-only (mounted from `screens/mod.rs`).

use bevy_egui::egui::{self, pos2, vec2, Event, PointerButton, RawInput, Rect};
use cockpit::engine::{Engine, EngineInput, EngineOutput};
use cockpit::fixture_engine::FixtureEngine;

use super::{crossflow, curves, kit, rate, report, report_pdf, water, Env, State};
use crate::engine_select::CockpitEngine;
use crate::theme as t;

const FIXTURE: &str = include_str!("../../assets/fixture.json");

fn engine() -> Box<dyn CockpitEngine> {
    crate::engine_select::build(FIXTURE, None).expect("the build's engine")
}

fn default_draft() -> EngineInput {
    FixtureEngine::from_json(FIXTURE)
        .expect("the fixture parses")
        .default_input()
}

fn catalog() -> crate::state::Catalog {
    crate::state::Catalog::from_fixture(&FixtureEngine::from_json(FIXTURE).expect("parses"))
}

/// The desktop viewport, and the rect the shell hands a screen in it.
fn screen() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))
}
fn area() -> Rect {
    Rect::from_min_size(pos2(76.0, 0.0), vec2(1364.0, 842.0))
}

/// A font context carrying the app's own fonts (one pass activates them).
fn font_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(t::fonts());
    let mut out = ctx.run_ui(raw(Vec::new()), |_| {});
    out.textures_delta.clear();
    ctx
}

fn raw(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(screen()),
        events,
        ..RawInput::default()
    }
}

/// One frame of a screen in a real egui pass: the strings it painted and the controls it drew.
type Drawn = (Vec<String>, Vec<(String, [f32; 4])>);
fn frame(ctx: &egui::Context, input: RawInput, f: impl FnMut(&mut egui::Ui)) -> Drawn {
    let mut f = f;
    kit::strings_begin();
    kit::hits_begin();
    let mut out = ctx.run_ui(input, |ui| f(ui));
    out.textures_delta.clear();
    (kit::strings_take(), kit::hits_take())
}

/// One want's heavy work, stepped to completion the way the shell would across frames: each call is
/// one frame's budget, so a screen is only painted with the work it owes done.
fn settle(
    cache: &mut super::data::Cache,
    want: super::data::Want,
    draft: &EngineInput,
    out: Option<&EngineOutput>,
    engine: Option<&dyn CockpitEngine>,
) {
    let mut steps = 0;
    while cache.step(want, FIXTURE, draft, out, engine) {
        steps += 1;
        assert!(steps < 10_000, "the {want:?} work never finished");
    }
}

fn env<'a>(out: &'a EngineOutput, engine: &'a dyn CockpitEngine) -> Env<'a> {
    Env {
        out: Some(out),
        engine: Some(engine),
        t: 0.0,
        motion: false,
        phone: false,
        document: None,
        catalog: None,
    }
}

/// `label` is painted, and the string painted right after it is `value` (the rows paint the label,
/// then the value).
fn shows(strings: &[String], label: &str, value: &str) -> bool {
    strings.windows(2).any(|w| w[0] == label && w[1] == value)
}

fn files_ctx(out: Option<&EngineOutput>) -> crate::files::Ctx<'_> {
    crate::files::Ctx {
        options: Box::leak(Box::new(crate::state::StartOptions::default())),
        host_label: crate::files::HOST_WEB_INTERNAL,
        spec: None,
        output: out,
        fixture_text: FIXTURE,
    }
}

fn session() -> crate::files::FileSession {
    crate::files::FileSession {
        revision_id: crate::revision::SHIPPED_ID.to_string(),
        ..crate::files::FileSession::default()
    }
}

/// Issue #139 AC 1 / Gap 1: cycles of concentration is the project's value. The Water tab's `+`
/// (pressed and released on the control it drew) edits the draft's duty itself - there is no
/// screen-local copy - and the edited value is what the save writes, what an open restores, and what
/// the Report screen, the PDF and Compare all show. The `cycles:<n>` command lands in the same field.
#[test]
fn cycles_is_the_projects_and_every_surface_reads_it() {
    let engine = engine();
    let mut draft = default_draft();
    let before = draft.duty.cycles_of_concentration;
    let out = engine.run(&draft).expect("the recorded duty runs");
    let ctx = font_ctx();
    let mut st = State::default();

    // ---- the Water tab edits the project
    let (strings, hits) = frame(&ctx, raw(Vec::new()), |ui| {
        water::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    assert!(
        strings.contains(&format!("{before:.1}")),
        "Water shows the project's cycles {before:.1}: {strings:?}"
    );
    let plus = hits
        .iter()
        .find(|(id, _)| id == "cyc.plus")
        .map(|(_, r)| pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0))
        .expect("Water draws its + control");
    let press = vec![
        Event::PointerMoved(plus),
        Event::PointerButton {
            pos: plus,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ];
    let release = vec![Event::PointerButton {
        pos: plus,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }];
    for events in [press, release] {
        frame(&ctx, raw(events), |ui| {
            water::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
        });
    }
    let edited = ((before * 2.0).round() / 2.0 + 0.5).min(10.0);
    assert_ne!(edited, before, "the edit moves the value");
    assert_eq!(
        draft.duty.cycles_of_concentration, edited,
        "the Water tab's + edits the project's own cycles"
    );
    // ...so the edit is unsaved work: the document's fingerprint moves with it
    assert_ne!(
        crate::files::signature(&draft, &catalog(), crate::revision::SHIPPED_ID),
        crate::files::signature(&default_draft(), &catalog(), crate::revision::SHIPPED_ID),
        "a cycles edit makes the project dirty"
    );
    // the app re-runs the draft every frame: the surfaces below read that run
    let out = engine.run(&draft).expect("the edited duty runs");
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        water::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    assert!(
        strings.contains(&format!("{edited:.1}")),
        "Water shows the edited cycles: {strings:?}"
    );

    // ---- the command path writes the same field
    let mut via_command = default_draft();
    let mut cst = State::default();
    super::apply(&mut cst, "cycles:6.5");
    super::take_cycles(&mut cst, &mut via_command);
    assert_eq!(via_command.duty.cycles_of_concentration, 6.5);
    assert_eq!(cst.cycles, None, "a command lands once");

    // ---- save -> open
    let cat = catalog();
    let text = crate::files::snapshot_text(
        &files_ctx(Some(&out)),
        &session(),
        &draft,
        &cat,
        Some(engine.as_ref() as &dyn Engine),
    );
    let path = std::env::temp_dir().join(format!(
        "drafthouse-139-cycles-{}.drafthouse",
        std::process::id()
    ));
    std::fs::write(&path, &text).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);
    let mut opened = default_draft();
    assert_eq!(opened.duty.cycles_of_concentration, before);
    let (mut cat2, mut slot, mut sess) = (catalog(), crate::app::EngineSlot::default(), session());
    crate::files::open_text(
        &saved,
        &files_ctx(None),
        &mut opened,
        &mut cat2,
        &mut slot,
        &mut sess,
    )
    .expect("the saved project opens");
    assert_eq!(
        opened.duty.cycles_of_concentration, edited,
        "cycles survives save -> open"
    );

    // ---- the Report screen (Inputs page) and the PDF
    st.page = 1;
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        report::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    assert!(
        shows(&strings, "cycles", &format!("{edited:.1}")),
        "the Report's inputs show cycles {edited:.1}: {strings:?}"
    );
    let meta = report_pdf::meta_for_export(true, &draft);
    let pdf = report_pdf::document(&draft, &out, &st.cache, &meta);
    let lines = crate::pdf::extract_text(&pdf);
    // the PDF prints its inputs at 3 significant figures, value and unit in one run ("4.50 ×")
    let pdf_cycles = format!("{} ×", t::num::sig(edited, 3));
    assert!(
        shows(&lines, "cycles", &pdf_cycles),
        "the PDF's inputs show cycles {pdf_cycles}: {lines:?}"
    );

    // ---- Compare: a file's cycles is a row, read from that file's own duty
    let base_text =
        crate::files::snapshot_text(&files_ctx(None), &session(), &default_draft(), &cat, None);
    let a = crate::compare::open(&base_text, "a.drafthouse", &cat, engine.as_ref()).expect("A");
    let b = crate::compare::open(&saved, "b.drafthouse", &cat, engine.as_ref()).expect("B");
    let m = crate::compare::Metric::Cycles;
    assert_eq!(crate::compare::value(m, &a), Some(before));
    assert_eq!(crate::compare::value(m, &b), Some(edited));
    assert!(
        crate::compare::changed(m.decimals(), Some(before), Some(edited)),
        "the edited file's cycles row is lit against the baseline"
    );
    st.compare.variants = std::rc::Rc::new(vec![a, b]);
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        super::compare::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    for want in [
        "cycles".to_string(),
        format!("{before:.1}"),
        format!("{edited:.1}"),
    ] {
        assert!(
            strings.contains(&want),
            "Compare shows {want:?}: {strings:?}"
        );
    }
}

/// Every text shape a pass painted (egui's own output shapes), whitespace-normalised - what a panel
/// drawn with plain `ui.label`s showed, where the screens' own string inventory does not reach.
fn painted(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(ts) => {
                let text = ts.galley.text().split_whitespace().collect::<Vec<_>>();
                if !text.is_empty() {
                    out.push(text.join(" "));
                }
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}

/// One frame of the duty & site panel (open, the build's descriptor) on `draft`, beside `run`.
fn duty_panel_strings(
    ctx: &egui::Context,
    draft: &EngineInput,
    run: &crate::state::Run,
) -> Vec<String> {
    let res = crate::duty_panel::DutyRes {
        spec: crate::state::DutySpecRes::load().spec,
        error: None,
    };
    let mut d = crate::app::Draft(draft.clone());
    let mut vis = crate::state::Visual {
        duty_open: true,
        ..Default::default()
    };
    let (mut hits, mut clip, mut info) = (
        crate::app::HitMap::default(),
        crate::clip::ClipProbe::default(),
        crate::clip::LayoutInfo::default(),
    );
    let mut out = ctx.run_ui(raw(Vec::new()), |ui| {
        crate::duty_panel::panel(
            ui,
            Some(&mut d),
            &res,
            run,
            &mut vis,
            &mut hits,
            &mut clip,
            &mut info,
            false,
        )
    });
    out.textures_delta.clear();
    painted(&out.shapes)
}

/// Issue #139 labels (Gap 5 + Gap 6): the duty's DESIGN range (hot water − target) and the run's
/// ACHIEVED range (hot water − the calculated cold water) are two numbers, so they carry two names on
/// every surface - the duty panel, the calculation card / Report sheet / PDF worked step, and the
/// results - and neither name is the bare "range". And there is one water mass flow: the engine's,
/// at the water's real density (200.0 kg/s at the default duty), never the panel's old fixed-1000
/// kg/m³ conversion (201.3).
#[test]
fn design_and_achieved_range_are_named_apart_and_one_mass_flow_shows() {
    let engine = engine();
    let mut draft = default_draft();
    let out = engine.run(&draft).expect("the recorded duty runs");
    let design = draft.duty.hot_water_c - draft.duty.target_cold_water_c;
    assert!(
        (out.range_c - design).abs() > 0.05,
        "the default duty's achieved {} and design {design} ranges differ - the case the names exist for",
        out.range_c
    );
    let ctx = font_ctx();

    // ---- the worked step: the engine's `Range` key, shown as "Achieved range"
    let step = out
        .worked_steps
        .iter()
        .find(|s| s.label == "Range")
        .expect("the engine's range step");
    assert_eq!(
        step.value,
        Some(out.range_c),
        "the step is the achieved range"
    );
    assert_eq!(kit::step_title(step), "Achieved range");

    // ---- the duty panel: "design range", one mass flow - the engine's
    let run = crate::state::Run {
        output: Some(out.clone()),
        error: None,
        ratio: draft.speed_ratio,
    };
    let panel = duty_panel_strings(&ctx, &draft, &run);
    assert!(
        panel.iter().any(|s| s == "DESIGN RANGE"),
        "the duty panel names its hot − target cell the design range: {panel:?}"
    );
    let engine_kg_s = crate::duty_panel::engine_mass_flow_kg_s(&run).expect("the run's mass flow");
    let at_1000 = drafthouse_cockpit_seams::mapping::kg_s_from_m3_hr(draft.duty.water_flow_m3_hr);
    assert!(
        (engine_kg_s - 200.0).abs() < 0.05,
        "the calculated mass flow at the default duty is 200.0 kg/s, got {engine_kg_s}"
    );
    let kg_s_lines: Vec<&String> = panel.iter().filter(|s| s.contains("kg/s")).collect();
    assert!(
        kg_s_lines
            .iter()
            .any(|s| s.contains(&format!("{} kg/s", t::fmt(engine_kg_s)))),
        "the panel shows the calculated {} kg/s: {kg_s_lines:?}",
        t::fmt(engine_kg_s)
    );
    assert!(
        !panel.iter().any(|s| s.contains(&t::fmt(at_1000))),
        "the fixed-1000 kg/m³ figure {} is gone from the panel: {panel:?}",
        t::fmt(at_1000)
    );
    // no run yet: no mass flow at all, never a stand-in
    let none = duty_panel_strings(&ctx, &draft, &crate::state::Run::default());
    assert!(
        !none.iter().any(|s| s.contains("kg/s")),
        "with no run the panel prints no mass flow: {none:?}"
    );

    // ---- the calculation card and the Report's steps + results pages
    let mut st = State::default();
    let achieved = t::num::kelvin(out.range_c);
    st.page = 3;
    let (results, _) = frame(&ctx, raw(Vec::new()), |ui| {
        report::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    assert!(
        shows(&results, "achieved range", &achieved),
        "the Report's results name the run's range the achieved range ({achieved}): {results:?}"
    );
    st.page = 2;
    let (steps, _) = frame(&ctx, raw(Vec::new()), |ui| {
        report::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    assert!(
        steps.iter().any(|s| s == "Achieved range"),
        "the Report's worked steps title the range step: {steps:?}"
    );
    for strings in [&results, &steps] {
        assert!(
            !strings.iter().any(|s| s == "range" || s == "Range"),
            "no bare \"range\" beside a design range: {strings:?}"
        );
    }

    // ---- the PDF: the results row and the worked step carry the same name
    let meta = report_pdf::meta_for_export(true, &draft);
    let pdf = report_pdf::document(&draft, &out, &st.cache, &meta);
    let lines = crate::pdf::extract_text(&pdf);
    assert!(
        lines.iter().any(|l| l == "Achieved range"),
        "the PDF's worked step is titled the achieved range: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "achieved range"),
        "the PDF's results row names the achieved range: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l == "range" || l == "Range"),
        "the PDF prints no bare \"range\": {lines:?}"
    );
}

/// The shipped catalog revision with one tower removed, re-sealed the way the generator seals (the
/// digest of the file's own bytes with `sha256` blanked) - an import a user could make.
fn revision_without(tower: &str) -> String {
    let mut v: serde_json::Value =
        serde_json::from_str(crate::revision::SHIPPED_TEXT).expect("the shipped revision is JSON");
    v["records"]["towers"]
        .as_array_mut()
        .expect("the revision's towers")
        .retain(|t| t["id"] != tower);
    v["id"] = serde_json::json!("illustrative-catalog-v0.1-139");
    v["sha256"] = serde_json::json!("");
    let unsealed = serde_json::to_string_pretty(&v).expect("serialises");
    let digest = crate::revision::digest_of(&unsealed).expect("digests");
    unsealed.replacen("\"sha256\": \"\"", &format!("\"sha256\": \"{digest}\""), 1)
}

/// Issue #139 Gap 3: a catalog import reaches every tab that runs the catalog. The heavy work (Size,
/// the crossflow grid, Curves) is handed the imported records, not the fixture's, so the selection
/// re-runs on them; and every file the comparison opened is opened and run again against the new
/// catalog - here the duty's own tower is gone from it, so the column becomes that catalog's named
/// refusal instead of keeping the old catalog's run.
#[test]
fn a_catalog_import_re_runs_size_and_the_comparison() {
    let draft = default_draft();
    let mut cat = catalog();
    let mut slot = crate::app::EngineSlot(Some(engine()));
    let mut session = session();

    // ---- before: the shipped catalog
    let shipped = super::catalog_text(FIXTURE, Some(&cat));
    assert!(
        std::ptr::eq(shipped, FIXTURE),
        "with no import the shipped catalog text is the one the heavy work reads"
    );
    let mut st = State::default();
    let before = super::data::text_key(shipped);
    super::follow_catalog(&mut st, before);
    let base_text = crate::files::snapshot_text(&files_ctx(None), &session, &draft, &cat, None);
    st.compare.pending = vec![("a.drafthouse".into(), base_text)];
    let (loaded, refused) = super::open_compare_step(&mut st, &cat, slot.0.as_deref())
        .expect("one file: the open is done in one step");
    assert_eq!((loaded, refused), (1, None), "{:?}", st.compare.problems);
    assert!(
        st.compare.variants[0].output().is_some(),
        "A runs on the shipped catalog"
    );

    let mut cache = super::data::Cache::default();
    let settle = |cache: &mut super::data::Cache, text: &str| {
        let mut n = 0;
        while cache.step(super::data::Want::Size, text, &draft, None, None) {
            n += 1;
            assert!(n < 10_000, "the size workspace never finished");
        }
        let size = cache.size.clone().expect("the size workspace has data");
        size.towers.iter().map(|t| t.id.clone()).collect::<Vec<_>>()
    };
    let towers = settle(&mut cache, shipped);
    assert!(towers.iter().any(|t| t == "IDCF-064"), "{towers:?}");

    // ---- the import: the duty's own tower is not in the new catalog
    crate::files::import_text(
        &revision_without("IDCF-064"),
        &mut cat,
        &mut slot,
        &mut session,
    )
    .expect("the revision imports");
    let imported = super::catalog_text(FIXTURE, Some(&cat));
    assert!(
        imported != FIXTURE,
        "the heavy work is handed the imported records, not the shipped ones"
    );
    let after = super::data::text_key(imported);
    assert_ne!(after, before);

    // Size re-runs on the imported records: the removed tower has no run
    let towers = settle(&mut cache, imported);
    assert!(!towers.is_empty(), "the imported catalog's towers run");
    assert!(
        !towers.iter().any(|t| t == "IDCF-064"),
        "Size re-ran on the import, not the shipped catalog: {towers:?}"
    );

    // the comparison runs its file again, against the new catalog
    super::follow_catalog(&mut st, after);
    assert!(
        st.compare.variants.is_empty(),
        "the old catalog's run is dropped"
    );
    assert_eq!(st.compare.pending.len(), 1, "the file is queued again");
    let (loaded, refused) = super::open_compare_step(&mut st, &cat, slot.0.as_deref())
        .expect("one file: the open is done in one step");
    assert_eq!(loaded, 0, "A cannot run on a catalog without its tower");
    assert_eq!(refused.as_deref(), Some("a.drafthouse"));
    assert!(
        st.compare
            .problems
            .iter()
            .any(|p| p.starts_with("a.drafthouse: ") && p.contains("IDCF-064")),
        "the refusal names the file and the missing tower: {:?}",
        st.compare.problems
    );
    // and a later frame on the same catalog does not run it a third time
    super::follow_catalog(&mut st, after);
    assert!(st.compare.pending.is_empty());
}

/// Issue #139 Gap 2: Curves' range, probe wet bulb and target and the picked Size candidate are set
/// relative to the project, so they follow it. The first frame keeps what a URL set; a duty edit (or a
/// file open / catalog import - the catalog key) resets them from the new project; a frame with nothing
/// moved leaves a user's choice alone.
#[test]
fn the_tabs_duty_relative_settings_follow_the_project() {
    let mut draft = default_draft();
    let key = super::data::text_key(FIXTURE);
    // a URL's choices, before the first frame
    let mut st = State {
        range_c: 12.0,
        target_cold: Some(30.0),
        cand: Some(3),
        ..State::default()
    };
    super::follow_project(&mut st, &draft, key);
    assert_eq!(
        st.probe_wb, draft.duty.wet_bulb_c,
        "the probe is seeded from the duty"
    );
    assert_eq!(
        (st.range_c, st.target_cold, st.cand),
        (12.0, Some(30.0), Some(3)),
        "the first frame keeps the URL's settings"
    );
    // the user's own choice stands while the project does not move
    st.probe_wb = 24.0;
    super::follow_project(&mut st, &draft, key);
    assert_eq!(
        (st.range_c, st.probe_wb, st.target_cold, st.cand),
        (12.0, 24.0, Some(30.0), Some(3))
    );

    // a duty edit: hot water 42 -> 40 °C, wet bulb 27 -> 26 °C (design range 8 K)
    draft.duty.hot_water_c = 40.0;
    draft.duty.wet_bulb_c = 26.0;
    super::follow_project(&mut st, &draft, key);
    assert_eq!(
        st.range_c, 8.0,
        "Curves' range follows the duty's design range"
    );
    assert_eq!(st.probe_wb, 26.0, "the probe follows the duty's wet bulb");
    assert_eq!(st.target_cold, None, "the target is the duty's own again");
    assert_eq!(st.cand, None, "the old selection's pick names nothing now");

    // a catalog import (the same duty): the pick is cleared again
    st.cand = Some(1);
    super::follow_project(&mut st, &draft, key ^ 1);
    assert_eq!(st.cand, None);
}

/// Issue #139 Gap 4: Rate's sample test readings sit around the duty, so an edited duty moves them -
/// and at the recorded duty they are exactly the readings the table always carried.
#[test]
fn rate_test_readings_follow_the_duty() {
    let mut draft = default_draft();
    let recorded = super::data::test_points(&draft.duty);
    let carried = [
        (0.97, 0.99, 41.6, 31.9, 26.6, 32.4),
        (1.02, 1.00, 42.3, 32.6, 27.1, 33.0),
        (0.94, 0.97, 40.8, 31.0, 25.9, 31.6),
    ];
    for (got, want) in recorded.iter().zip(carried) {
        let g = [got.0, got.1, got.2, got.3, got.4, got.5];
        let w = [want.0, want.1, want.2, want.3, want.4, want.5];
        for (a, b) in g.iter().zip(w) {
            assert!(
                (a - b).abs() < 1e-9,
                "at the recorded duty {got:?} == {want:?}"
            );
        }
    }
    draft.duty.hot_water_c += 2.0;
    draft.duty.target_cold_water_c += 1.0;
    draft.duty.wet_bulb_c -= 1.0;
    draft.duty.dry_bulb_c += 0.5;
    for (moved, was) in super::data::test_points(&draft.duty).iter().zip(recorded) {
        assert!(
            (moved.2 - was.2 - 2.0).abs() < 1e-9,
            "hot water follows: {moved:?}"
        );
        assert!(
            (moved.3 - was.3 - 1.0).abs() < 1e-9,
            "cold water follows: {moved:?}"
        );
        assert!(
            (moved.4 - was.4 + 1.0).abs() < 1e-9,
            "wet bulb follows: {moved:?}"
        );
        assert!(
            (moved.5 - was.5 - 0.5).abs() < 1e-9,
            "dry bulb follows: {moved:?}"
        );
        assert_eq!(
            (moved.0, moved.1),
            (was.0, was.1),
            "the flow fractions are the duty's"
        );
    }
}

/// Issue #139 AC 2: one duty edit - hot water, flow and wet bulb - reaches **every** tab. The tab
/// surfaces are painted through their own `ui` with a settled `State`, following the shell's own
/// per-frame discipline (`follow_project`, then one budgeted want step at a time), and the file the
/// comparison re-opens is a real save of the edited project. A cache that keeps a stale grid, a
/// follower that misses the edit, or a surface that paints the previous run fails here; the fix
/// round's red half shows the cache-key mutation that turns it RED.
#[test]
fn a_duty_edit_reaches_every_tab_and_no_cache_keeps_the_old_run() {
    let engine = engine();
    let cat = catalog();
    let catalog_key = super::data::text_key(FIXTURE);
    let mut draft = default_draft();
    let recorded = draft.clone();
    let out1 = engine.run(&draft).expect("the recorded duty runs");
    let ctx = font_ctx();
    let mut st = State::default();

    // ---- the recorded duty: the settings follow the project, then the Instrument's crossflow work
    //      steps to completion (size, then the grid) - so a later edit finds a full cache to drop
    super::follow_project(&mut st, &draft, catalog_key);
    settle(
        &mut st.cache,
        super::data::Want::Crossflow,
        &draft,
        Some(&out1),
        Some(engine.as_ref()),
    );
    let grid1 = match st.cache.xf.as_ref().expect("the recorded crossflow run") {
        Ok(g) => std::sync::Arc::clone(g),
        Err(e) => panic!("the recorded duty's crossflow run refused: {e}"),
    };
    let (was, _) = frame(&ctx, raw(Vec::new()), |ui| {
        crossflow::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out1, engine.as_ref()),
            area(),
        )
    });
    assert!(
        was.iter()
            .any(|s| s == &format!("hot {:.1} °C", recorded.duty.hot_water_c)),
        "the Instrument paints the recorded duty's hot water: {was:?}"
    );

    // ---- the edit: hot water, flow and wet bulb, then the engine re-runs
    draft.duty.hot_water_c = 40.0;
    draft.duty.water_flow_m3_hr = 650.0;
    draft.duty.wet_bulb_c = 25.5;
    let out2 = engine.run(&draft).expect("the edited duty runs");
    assert!(
        out2.validation.is_empty(),
        "the edited duty runs clean: {:?}",
        out2.validation
    );
    super::follow_project(&mut st, &draft, catalog_key);
    assert_eq!(
        st.probe_wb, draft.duty.wet_bulb_c,
        "the tab-relative settings follow the edit"
    );

    // ---- Instrument (crossflow::ui): the cache drops the recorded grid and re-runs on the edit
    let xf_runs = st.cache.computed.xf;
    settle(
        &mut st.cache,
        super::data::Want::Crossflow,
        &draft,
        Some(&out2),
        Some(engine.as_ref()),
    );
    assert_eq!(
        st.cache.computed.xf - xf_runs,
        1,
        "the edit re-runs the crossflow grid exactly once"
    );
    let grid2 = match st.cache.xf.as_ref().expect("the re-run crossflow grid") {
        Ok(g) => std::sync::Arc::clone(g),
        Err(e) => panic!("the edited duty's crossflow run refused: {e}"),
    };
    assert_ne!(
        (grid1.hot_c, grid1.cold_c),
        (grid2.hot_c, grid2.cold_c),
        "the grid moved with the duty"
    );
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        crossflow::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    assert!(
        strings
            .iter()
            .any(|s| s == &format!("hot {:.1} °C", draft.duty.hot_water_c)),
        "the Instrument paints the edited duty's hot water: {strings:?}"
    );
    assert!(
        strings
            .iter()
            .any(|s| s == &format!("cold {} °C", t::num::temp(grid2.cold_c))),
        "the Instrument paints the re-run grid's cold water: {strings:?}"
    );
    assert!(
        !strings
            .iter()
            .any(|s| s == &format!("hot {:.1} °C", recorded.duty.hot_water_c)),
        "the recorded run's hot water is gone: {strings:?}"
    );

    // ---- Curves (curves::ui): a fresh grid on the edited flow, the probe at the edited wet bulb
    settle(
        &mut st.cache,
        super::data::Want::Curves,
        &draft,
        Some(&out2),
        Some(engine.as_ref()),
    );
    let curves2 = std::sync::Arc::clone(st.cache.curves.as_ref().expect("the curves grid"));
    assert_eq!(
        curves2.design_flow_kg_s.to_bits(),
        super::data::water_kg_s(&draft).to_bits(),
        "the curves grid was built on the edited flow"
    );
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        curves::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    assert!(
        strings
            .iter()
            .any(|s| s == &format!("At {:.1} °C wet bulb", draft.duty.wet_bulb_c)),
        "Curves paints the edited wet bulb: {strings:?}"
    );
    // the read-off card is the fresh grid's own prediction at the probe, painted
    let probe = st.probe_wb.clamp(
        super::data::CURVE_WB[0],
        *super::data::CURVE_WB.last().unwrap(),
    );
    let ri = super::data::CURVE_RANGE
        .iter()
        .position(|r| (r - st.range_c).abs() < 0.01)
        .unwrap_or(1);
    let (cold_at_probe, _) = super::data::predict_cold(
        &curves2,
        probe,
        super::data::CURVE_RANGE[ri],
        curves2.design_flow_kg_s,
    )
    .expect("the probe read-off");
    assert!(
        strings.contains(&t::num::temp(cold_at_probe)),
        "Curves paints its read-off {} °C: {strings:?}",
        t::num::temp(cold_at_probe)
    );
    assert!(
        !strings
            .iter()
            .any(|s| s == &format!("At {:.1} °C wet bulb", recorded.duty.wet_bulb_c)),
        "the recorded probe's paint is gone: {strings:?}"
    );

    // ---- Water: the balance re-derived at the edited flow
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        water::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    let w2 = super::data::water(&draft, &out2, draft.duty.cycles_of_concentration)
        .expect("the edited balance");
    let m3h = |density: f64, kg_s: f64| kg_s / density * 3600.0;
    assert!(
        shows(
            &strings,
            "make-up",
            &t::num::flow(m3h(w2.density, w2.makeup_kg_s))
        ),
        "Water paints the edited make-up: {strings:?}"
    );
    assert!(
        strings.contains(&format!(
            "{} m³/h circulating",
            t::num::flow(m3h(w2.density, w2.circulating_kg_s))
        )),
        "Water paints the edited circulating flow: {strings:?}"
    );
    let w1 = super::data::water(&recorded, &out1, recorded.duty.cycles_of_concentration)
        .expect("the recorded balance");
    let before = t::num::flow(m3h(w1.density, w1.makeup_kg_s));
    let after = t::num::flow(m3h(w2.density, w2.makeup_kg_s));
    assert!(
        before != after,
        "the edit moves the make-up: {before} -> {after}"
    );
    assert!(
        !strings.contains(&before),
        "the recorded run's make-up {before} is gone: {strings:?}"
    );

    // ---- Rate (rate::ui): the chips follow the duty; the read-out is the re-run's dry air
    settle(
        &mut st.cache,
        super::data::Want::Rate,
        &draft,
        Some(&out2),
        None,
    );
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        rate::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    for p in super::data::test_points(&draft.duty) {
        let chip = format!("{} → {} °C", t::num::temp(p.2), t::num::temp(p.3));
        assert!(
            strings.contains(&chip),
            "Rate paints the edited duty's reading {chip}: {strings:?}"
        );
    }
    let dry2 = super::data::dry_air_kg_s(&out2).expect("the run's air flow");
    assert!(
        shows(&strings, "dry air", &t::num::flow(dry2)),
        "Rate paints the re-run's dry air: {strings:?}"
    );

    // ---- Compare: a real save of the edited project, re-opened as a variant
    let text2 = crate::files::snapshot_text(
        &files_ctx(Some(&out2)),
        &session(),
        &draft,
        &cat,
        Some(engine.as_ref() as &dyn Engine),
    );
    let variant = crate::compare::open(&text2, "edited.drafthouse", &cat, engine.as_ref())
        .expect("the edited file opens");
    st.compare.variants = std::rc::Rc::new(vec![variant]);
    st.base = 0;
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        super::compare::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    let value = crate::compare::value(crate::compare::Metric::ColdWater, &st.compare.variants[0])
        .expect("the re-opened file's run");
    assert!(
        (value - out2.cold_water_c).abs() < 0.005,
        "the re-opened file runs the edited duty: {value} vs {}",
        out2.cold_water_c
    );
    let shown = crate::compare::display(crate::compare::Metric::ColdWater, value);
    assert!(
        strings.contains(&shown),
        "Compare paints the edited cold water {shown}: {strings:?}"
    );
    let gone = crate::compare::display(crate::compare::Metric::ColdWater, out1.cold_water_c);
    assert!(
        gone != shown && !strings.contains(&gone),
        "the recorded run's cold water {gone} is gone: {strings:?}"
    );

    // ---- Report: the inputs the edit typed, and the results the re-run produced
    st.page = 1;
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        report::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    assert!(
        shows(
            &strings,
            "water flow",
            &t::num::flow(draft.duty.water_flow_m3_hr)
        ),
        "the Report's inputs paint the edited flow: {strings:?}"
    );
    assert!(
        shows(&strings, "hot water", &t::num::temp(draft.duty.hot_water_c)),
        "the Report's inputs paint the edited hot water: {strings:?}"
    );
    assert!(
        shows(&strings, "wet bulb", &t::num::temp(draft.duty.wet_bulb_c)),
        "the Report's inputs paint the edited wet bulb: {strings:?}"
    );
    assert!(
        !shows(
            &strings,
            "hot water",
            &t::num::temp(recorded.duty.hot_water_c)
        ),
        "the Report's inputs no longer paint the recorded run: {strings:?}"
    );
    st.page = 3;
    let (strings, _) = frame(&ctx, raw(Vec::new()), |ui| {
        report::ui(
            ui,
            &mut st,
            &mut draft,
            &env(&out2, engine.as_ref()),
            area(),
        )
    });
    assert!(
        shows(&strings, "approach", &t::num::kelvin(out2.approach_c)),
        "the Report's results paint the re-run's approach: {strings:?}"
    );
    assert!(
        shows(&strings, "achieved range", &t::num::kelvin(out2.range_c)),
        "the Report's results paint the re-run's achieved range: {strings:?}"
    );
    assert!(
        shows(&strings, "make-up", &t::num::flow(out2.makeup_m3_hr)),
        "the Report's results paint the re-run's make-up: {strings:?}"
    );
    assert!(
        !shows(&strings, "approach", &t::num::kelvin(out1.approach_c)),
        "the Report's results no longer paint the recorded run: {strings:?}"
    );

    // ---- the PDF: the same duty and the same results, in the sheet's own extracted text
    let meta = report_pdf::meta_for_export(true, &draft);
    let pdf = report_pdf::document(&draft, &out2, &st.cache, &meta);
    let lines = crate::pdf::extract_text(&pdf);
    for (label, value) in [
        (
            "water flow",
            format!("{} m³/h", t::num::flow(draft.duty.water_flow_m3_hr)),
        ),
        (
            "hot water",
            format!("{} °C", t::num::temp(draft.duty.hot_water_c)),
        ),
        (
            "wet bulb",
            format!("{} °C", t::num::temp(draft.duty.wet_bulb_c)),
        ),
        ("approach", format!("{} K", t::num::kelvin(out2.approach_c))),
        (
            "achieved range",
            format!("{} K", t::num::kelvin(out2.range_c)),
        ),
        (
            "make-up",
            format!("{} m³/h", t::num::flow(out2.makeup_m3_hr)),
        ),
    ] {
        assert!(
            shows(&lines, label, &value),
            "the PDF shows {label} = {value}: {lines:?}"
        );
    }
    assert!(
        !shows(
            &lines,
            "hot water",
            &format!("{} °C", t::num::temp(recorded.duty.hot_water_c))
        ),
        "the PDF no longer shows the recorded run: {lines:?}"
    );
}

/// Issue #139 AC 6: the default duty's hand check. Every value below is computed from the run this
/// build's engine makes - its own fields, its heat step and the water balance - and checked against
/// the audit's numbers (approach 4.6546 K, achieved range 10.3454 K, heat 8655.8 kW, evaporation
/// 1.56 %, make-up 15.0 m³/h) at the precision the surfaces print them, then read back off the
/// painted surfaces. The audit's numbers are targets here; the run is the only source.
#[test]
fn the_default_duty_hand_check_reproduces_the_audited_numbers() {
    let engine = engine();
    let mut draft = default_draft();
    let out = engine.run(&draft).expect("the recorded duty runs");
    assert!(
        out.validation.is_empty(),
        "the recorded duty runs clean: {:?}",
        out.validation
    );
    let m_w = super::data::water_kg_s(&draft);

    // the run's own fields, and the identities the hand check leans on
    let approach = out.cold_water_c - draft.duty.wet_bulb_c;
    let range = draft.duty.hot_water_c - out.cold_water_c;
    assert!(
        (approach - out.approach_c).abs() < 1e-9,
        "the run's approach is cold − wet bulb"
    );
    assert!(
        (range - out.range_c).abs() < 1e-9,
        "the run's range is hot − cold"
    );
    let heat = super::data::step_value(&out, "Heat load").expect("the run's heat-load step");
    let w = super::data::water(&draft, &out, draft.duty.cycles_of_concentration)
        .expect("the water balance");
    let evap_kg_s = out.evaporation_pct / 100.0 * m_w;

    // heat = m_w · cp · range: the audit's cp 4.18339 kJ/kg·K, solved out of the run's numbers
    let cp = heat / (m_w * range);
    assert!(
        (cp - 4.18339).abs() <= 0.0005,
        "the run's own cp is the audit's: {cp} kJ/kg·K"
    );

    // the audit's numbers, at the displayed precision
    assert!((approach - 4.6546).abs() <= 0.05, "approach {approach} K");
    assert!((range - 10.3454).abs() <= 0.05, "achieved range {range} K");
    assert!((heat - 8655.8).abs() <= 0.05, "heat {heat} kW");
    assert!(
        (out.evaporation_pct - 1.56).abs() <= 0.005,
        "evaporation {} %",
        out.evaporation_pct
    );
    assert!(
        (out.makeup_m3_hr - 15.0).abs() <= 0.05,
        "make-up {} m³/h",
        out.makeup_m3_hr
    );
    // ...and the audit's kg/s pair, out of the same run
    assert!(
        (evap_kg_s - 3.1113).abs() <= 0.0005,
        "evaporation {evap_kg_s} kg/s"
    );
    assert!(
        (w.makeup_kg_s - 4.1484).abs() <= 0.0005,
        "make-up {} kg/s",
        w.makeup_kg_s
    );

    // the strings the surfaces print are those numbers, in the house style
    assert_eq!(t::num::kelvin(out.approach_c), "4.7");
    assert_eq!(t::num::kelvin(out.range_c), "10.3");
    assert_eq!(t::num::power(heat), "8655.8");
    assert_eq!(t::num::sig(out.evaporation_pct, 3), "1.56");
    assert_eq!(t::num::flow(out.makeup_m3_hr), "15.0");

    // ---- painted: the answer card's sheet, the Report's results page, the PDF's rows
    let ctx = font_ctx();
    let one = super::data::answer(&draft, Some(&out), super::data::recorded_limits(FIXTURE));
    let mut st = State {
        answer: Some(one),
        answer_open: true,
        ..State::default()
    };
    let (bar, _) = frame(&ctx, raw(Vec::new()), |_ui| {
        super::answer_bar(&ctx, &mut st, screen(), true)
    });
    assert!(!bar.is_empty(), "the answer bar painted something: {bar:?}");
    assert!(
        shows(&bar, "approach", &t::num::kelvin(out.approach_c)),
        "the answer card paints the approach {}: {bar:?}",
        t::num::kelvin(out.approach_c)
    );
    assert!(
        shows(&bar, "heat load", &t::num::power(heat)),
        "the answer card paints the heat load {}: {bar:?}",
        t::num::power(heat)
    );
    assert!(
        shows(&bar, "make-up", &t::num::flow(out.makeup_m3_hr)),
        "the answer card paints the make-up {}: {bar:?}",
        t::num::flow(out.makeup_m3_hr)
    );

    let mut st = State {
        page: 3,
        ..State::default()
    };
    let (results, _) = frame(&ctx, raw(Vec::new()), |ui| {
        report::ui(ui, &mut st, &mut draft, &env(&out, engine.as_ref()), area())
    });
    assert!(
        shows(&results, "approach", &t::num::kelvin(out.approach_c)),
        "the Report's results paint the approach: {results:?}"
    );
    assert!(
        shows(&results, "achieved range", &t::num::kelvin(out.range_c)),
        "the Report's results paint the achieved range: {results:?}"
    );
    assert!(
        shows(&results, "make-up", &t::num::flow(out.makeup_m3_hr)),
        "the Report's results paint the make-up: {results:?}"
    );

    let meta = report_pdf::meta_for_export(true, &draft);
    let pdf = report_pdf::document(&draft, &out, &st.cache, &meta);
    let lines = crate::pdf::extract_text(&pdf);
    assert!(
        shows(
            &lines,
            "approach",
            &format!("{} K", t::num::kelvin(out.approach_c))
        ),
        "the PDF shows approach = {} K: {lines:?}",
        t::num::kelvin(out.approach_c)
    );
    assert!(
        shows(
            &lines,
            "make-up",
            &format!("{} m³/h", t::num::flow(out.makeup_m3_hr))
        ),
        "the PDF shows make-up = {} m³/h: {lines:?}",
        t::num::flow(out.makeup_m3_hr)
    );
}
