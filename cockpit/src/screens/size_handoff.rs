//! Issue #150: the Size screen's pick sheet hands its candidate to the instrument.
//!
//! The control is pressed the way the screen receives a pointer - egui events at the control's own
//! rect, read back from the pass that drew it - and the assertions are the issue's own: the shared
//! run carries the candidate's tower, stack, fan, speed, nozzle and drift eliminator; the screen is
//! the instrument; a computed number of the run is the candidate's own value; a pick that cannot be
//! loaded offers a locked control and a reason instead of a press that does nothing; the duty is the
//! project's and no press disturbs it; and the old apologetic line never comes back.
//!
//! Test-only, real-engine-only: the selection this presses the control on does not exist without the
//! engine, and the workspace runs through the same `data::Cache` step the screen itself uses.

use std::sync::Arc;

use bevy_egui::egui::{self, pos2, vec2, Event, PointerButton, RawInput, Rect};
use cockpit::engine::{EngineInput, EngineOutput};
use cockpit::fixture_engine::FixtureEngine;

use super::*;

const FIXTURE: &str = include_str!("../../assets/fixture.json");

fn desktop() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))
}

/// A font context carrying the app's own fonts (one pass activates them).
fn context(vp: Rect) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(t::fonts());
    let mut out = ctx.run_ui(
        RawInput {
            screen_rect: Some(vp),
            ..RawInput::default()
        },
        |_| {},
    );
    out.textures_delta.clear();
    ctx
}

/// One pass of the screens' own shell over `st`: the same `body` dispatch a frame makes, with the
/// engine, the run and the catalog a live frame carries.
fn pass(
    ctx: &egui::Context,
    st: &mut State,
    draft: &mut EngineInput,
    cat: &crate::state::Catalog,
    engine: &dyn crate::engine_select::CockpitEngine,
    run: &EngineOutput,
    events: Vec<Event>,
) -> Vec<String> {
    let screen = desktop();
    let content = layout(screen, false, st.nav, true);
    kit::strings_begin();
    kit::hits_begin();
    let input = RawInput {
        screen_rect: Some(screen),
        events,
        ..RawInput::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let mut c = Ctx {
            draft: Some(&mut *draft),
            engine: Some(engine),
            out: Some(run),
            fixture_text: FIXTURE,
            reduced_motion: false,
            t: 0.0,
            document: None,
            catalog: Some(cat),
        };
        body(ui, st, &mut c, content, false);
    });
    out.textures_delta.clear();
    let strings = kit::strings_take();
    let _ = kit::hits_take();
    strings
}

/// A press and its release, as the real events a frame receives.
fn tap(at: egui::Pos2) -> [Vec<Event>; 2] {
    [
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    ]
}

fn shows(strings: &[String], s: &str) -> bool {
    strings.iter().any(|x| x == s)
}

/// Press the pick sheet's control where it was drawn; returns the strings of the release pass.
fn press_the_control(
    ctx: &egui::Context,
    st: &mut State,
    ws: &mut Workspace,
    strings_out: &mut Vec<String>,
) {
    let at = ctx
        .read_response(egui::Id::new("size.use"))
        .expect("the pick sheet draws its control")
        .rect
        .center();
    let mut last = Vec::new();
    for events in tap(at) {
        last = pass(
            ctx,
            st,
            &mut ws.draft,
            &ws.cat,
            ws.engine.as_ref(),
            &ws.run,
            events,
        );
    }
    *strings_out = last;
}

/// The live fixture, the recorded duty **with an edit** (the project's own - the load must not
/// disturb it), the build's engine, and the draft's run - with the size workspace stepped the way
/// the screen steps it.
struct Workspace {
    cat: crate::state::Catalog,
    draft: EngineInput,
    engine: Box<dyn crate::engine_select::CockpitEngine>,
    run: EngineOutput,
}

fn workspace() -> (Workspace, data::Cache) {
    let fx = FixtureEngine::from_json(FIXTURE).expect("the recorded input parses");
    let mut draft = fx.default_input();
    let cat = crate::state::Catalog::from_fixture(&fx);
    let engine = crate::engine_select::build(FIXTURE, None).expect("the build's engine");
    draft.duty.cycles_of_concentration = 7.5;
    draft.duty.hot_water_c += 1.0;
    let run = engine.run(&draft).expect("the edited duty runs");
    let mut cache = data::Cache::default();
    let mut steps = 0;
    while cache.step(
        data::Want::Size,
        FIXTURE,
        &draft,
        Some(&run),
        Some(engine.as_ref()),
    ) {
        steps += 1;
        assert!(steps < 100_000, "the size workspace never finishes");
    }
    (
        Workspace {
            cat,
            draft,
            engine,
            run,
        },
        cache,
    )
}

#[test]
fn the_pick_sheet_hands_its_candidate_to_the_instrument() {
    let (mut ws, cache) = workspace();
    let size = cache.size.clone().expect("the size workspace landed");
    // the candidate: a counterflow tower other than the draft's own, so every field the load
    // writes is a change
    let i = size
        .by_capacity
        .iter()
        .position(|c| !c.crossflow && c.tower_id != ws.draft.tower.id)
        .expect("a counterflow candidate on another tower");
    let cand = size.by_capacity[i].clone();
    let duty_before = ws.draft.duty.clone();

    let mut st = State {
        screen: Screen::Size,
        nav: Nav::Rail,
        cand: Some(i),
        cache,
        ..State::default()
    };
    let ctx = context(desktop());
    let _ = pass(
        &ctx,
        &mut st,
        &mut ws.draft,
        &ws.cat,
        ws.engine.as_ref(),
        &ws.run,
        Vec::new(),
    );
    let mut strings = Vec::new();
    press_the_control(&ctx, &mut st, &mut ws, &mut strings);

    // AC 1 and 2: the instrument carries the picked size - the list entry's own records, and the
    // run the instrument paints is the candidate's own run (its drift eliminator is what its own
    // numbers pin, so a bit-exact number is the whole identity, eliminator included).
    assert_eq!(
        st.screen,
        Screen::Instrument,
        "the control opens the instrument: {strings:?}"
    );
    assert!(
        !st.crossflow,
        "the instrument is the section, not the crossflow grid"
    );
    assert_eq!(
        ws.draft.tower.id, cand.tower_id,
        "the instrument's tower is the list entry"
    );
    assert_eq!(ws.draft.fill_layers.len(), 1, "the candidate is one layer");
    assert_eq!(ws.draft.fill_layers[0].fill_id, cand.fill_id);
    assert_eq!(ws.draft.fill_layers[0].depth_m, cand.depth_m);
    assert_eq!(ws.draft.fan.id, cand.fan_id);
    assert_eq!(ws.draft.speed_ratio, cand.speed);
    assert_eq!(ws.draft.nozzle.id, cand.nozzle_id);
    let loaded = ws.engine.run(&ws.draft).expect("the loaded machine runs");
    assert_eq!(
        loaded.cold_water_c, cand.cold_c,
        "the instrument's cold water is the candidate's own"
    );
    assert_eq!(
        loaded.fan_power_kw, cand.power_kw,
        "the instrument's fan power is the candidate's own"
    );
    // AC 5: the duty is the project's - the edit survived the load
    assert_eq!(
        ws.draft.duty, duty_before,
        "the load carries the machinery, never the duty"
    );
    // the change is visible: a confirmation naming the tower, not the old apology
    let (line, _) = st.toast.clone().expect("the load raises its confirmation");
    assert!(
        line.contains(&cand.tower_id),
        "the confirmation names the tower: {line}"
    );

    // AC 4: the same sheet, a pick the catalog cannot resolve - the control is locked, the reason
    // is on screen, and a press does nothing at all
    st.screen = Screen::Size;
    st.toast = None;
    let j = {
        let size = Arc::make_mut(st.cache.size.as_mut().expect("the size workspace"));
        size.by_capacity[i].fill_id = "NO-SUCH-FILL".into();
        size.by_capacity
            .iter()
            .position(|c| c.crossflow)
            .expect("a crossflow candidate")
    };
    let cross = st
        .cache
        .size
        .as_ref()
        .expect("the size workspace")
        .by_capacity[j]
        .clone();
    let tower_before = ws.draft.tower.id.clone();
    let stack_before = ws.draft.fill_layers.clone();
    let _ = pass(
        &ctx,
        &mut st,
        &mut ws.draft,
        &ws.cat,
        ws.engine.as_ref(),
        &ws.run,
        Vec::new(),
    );
    let mut strings = Vec::new();
    press_the_control(&ctx, &mut st, &mut ws, &mut strings);
    assert!(
        shows(&strings, "NO-SUCH-FILL is not in this catalog"),
        "the locked control says why: {strings:?}"
    );
    assert_eq!(st.screen, Screen::Size, "a locked control opens nothing");
    assert!(
        st.toast.is_none(),
        "a locked control does not apologise: {:?}",
        st.toast
    );
    assert_eq!(ws.draft.tower.id, tower_before, "the run is untouched");
    assert_eq!(ws.draft.fill_layers, stack_before, "the stack is untouched");

    // and a crossflow size loads through the same control: the section's own run for a crossflow
    // tower is the candidate's own run too
    st.screen = Screen::Size;
    st.toast = None;
    st.cand = Some(j);
    let _ = pass(
        &ctx,
        &mut st,
        &mut ws.draft,
        &ws.cat,
        ws.engine.as_ref(),
        &ws.run,
        Vec::new(),
    );
    let mut strings = Vec::new();
    press_the_control(&ctx, &mut st, &mut ws, &mut strings);
    assert_eq!(
        st.screen,
        Screen::Instrument,
        "the crossflow control opens the instrument: {strings:?}"
    );
    assert!(
        !st.crossflow,
        "the instrument is the section, not the crossflow grid"
    );
    assert_eq!(ws.draft.tower.id, cross.tower_id);
    assert_eq!(ws.draft.fill_layers[0].fill_id, cross.fill_id);
    assert_eq!(ws.draft.fill_layers[0].depth_m, cross.depth_m);
    assert_eq!(ws.draft.fan.id, cross.fan_id);
    assert_eq!(ws.draft.speed_ratio, cross.speed);
    assert_eq!(ws.draft.nozzle.id, cross.nozzle_id);
    let loaded = ws
        .engine
        .run(&ws.draft)
        .expect("the crossflow machine runs");
    assert_eq!(
        loaded.cold_water_c, cross.cold_c,
        "the instrument's cold water is the crossflow candidate's own"
    );
}

/// AC 3: the toast string is gone - and this test fails if it returns, wherever it returns.
#[test]
fn the_old_apology_is_gone_from_the_sources() {
    const OLD: &str = "Design only - the candidate does not load yet";
    let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src_root, &mut files);
    let mut found = Vec::new();
    for f in &files {
        // the needle's own home is not a hit
        if f.file_stem().is_some_and(|n| n == "size_handoff") {
            continue;
        }
        if let Ok(src) = std::fs::read_to_string(f) {
            if src.contains(OLD) {
                found.push(f.display().to_string());
            }
        }
    }
    assert!(
        found.is_empty(),
        "the old apology is back in: {found:?} - the control loads the pick now, \
         and a refusal is a locked control with its reason"
    );
}

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}
