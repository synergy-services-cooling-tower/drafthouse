//! Issue #151: a cold path shows its progress, never a still window - and a comparison's file open
//! runs each file once, at its point (#138 N4).
//!
//! Every frame here is the screens' own [`frame`] - the per-frame entry `ui::viz_ui` calls, with the
//! commands the page sends (`screen:compare`, `compare:open:<name>:<text>`, ...) going through the same
//! inbox - over the build's engine, the fixture's catalog and the recorded duty. What a frame painted
//! is read back from what the frame published (the painted-text inventory, `data-strings` on the
//! page), the inventory the evidence counts.
//!
//! Test-only, real-engine-only: the sliced work these frames step is the engine's.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use bevy_egui::egui::{self, pos2, vec2, RawInput, Rect};
use cockpit::engine::{Engine, EngineError, EngineInput, EngineOutput};
use cockpit::fixture_engine::FixtureEngine;

use super::*;
use crate::engine_select::CockpitEngine;

const FIXTURE: &str = include_str!("../../assets/fixture.json");
const A: &str = include_str!("../../assets/variants/a-base.drafthouse");
const B: &str = include_str!("../../assets/variants/b-fan-faster.drafthouse");

/// The screens' state and their command inbox are per-thread and process-wide; the tests that drive
/// [`frame`] take turns.
static SERIAL: Mutex<()> = Mutex::new(());

fn desktop() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))
}

/// The build's engine, counting the calls a frame makes on it: the whole runs (`run`,
/// `run_detached`) and the point runs (`run_point`).
struct Counting {
    inner: Box<dyn CockpitEngine>,
    whole: AtomicUsize,
    point: AtomicUsize,
}

impl Counting {
    fn new() -> Self {
        Self {
            inner: crate::engine_select::build(FIXTURE, None).expect("the build's engine"),
            whole: AtomicUsize::new(0),
            point: AtomicUsize::new(0),
        }
    }
    fn calls(&self) -> (usize, usize) {
        (
            self.whole.load(Ordering::Relaxed),
            self.point.load(Ordering::Relaxed),
        )
    }
}

impl Engine for Counting {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn run(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        self.whole.fetch_add(1, Ordering::Relaxed);
        self.inner.run(input)
    }
}

impl CockpitEngine for Counting {
    fn run_detached(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        self.whole.fetch_add(1, Ordering::Relaxed);
        self.inner.run_detached(input)
    }
    fn run_point(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        self.point.fetch_add(1, Ordering::Relaxed);
        self.inner.run_point(input)
    }
}

/// The session a live frame carries: the fixture's catalog, the recorded duty and its run.
struct Session {
    cat: crate::state::Catalog,
    draft: EngineInput,
    run: EngineOutput,
    ctx: egui::Context,
}

fn session(engine: &dyn CockpitEngine) -> Session {
    let fx = FixtureEngine::from_json(FIXTURE).expect("the recorded input parses");
    let draft = fx.default_input();
    let run = engine.run(&draft).expect("the recorded duty runs");
    let ctx = egui::Context::default();
    ctx.set_fonts(t::fonts());
    let mut out = ctx.run_ui(
        RawInput {
            screen_rect: Some(desktop()),
            ..RawInput::default()
        },
        |_| {},
    );
    out.textures_delta.clear();
    // a fresh screens state for this test's thread: the URL is read, the first screen is set
    STATE.with(|cell| {
        *cell.borrow_mut() = State {
            url_read: true,
            ..State::default()
        }
    });
    Session {
        cat: crate::state::Catalog::from_fixture(&fx),
        draft,
        run,
        ctx,
    }
}

/// One frame: the commands the page sends, then the screens' own per-frame entry. Returns what the
/// frame painted and whether it ended with work still owed (`data-screen-busy`).
fn frame_with(
    s: &mut Session,
    engine: &dyn CockpitEngine,
    commands: &[&str],
) -> (Vec<String>, bool) {
    for c in commands {
        push_command(c);
    }
    let input = RawInput {
        screen_rect: Some(desktop()),
        ..RawInput::default()
    };
    let ctx = s.ctx.clone();
    let mut out = ctx.run_ui(input, |ui| {
        let _ = frame(
            ui.ctx(),
            Ctx {
                draft: Some(&mut s.draft),
                engine: Some(engine),
                out: Some(&s.run),
                fixture_text: FIXTURE,
                reduced_motion: true,
                t: 0.0,
                document: None,
                catalog: Some(&s.cat),
            },
        );
    });
    out.textures_delta.clear();
    let busy = STATE.with(|cell| cell.borrow().busy);
    (published_take(), busy)
}

/// The progress a painted string reports: the `done/n` word (n > 0, done <= n) and its counts.
fn fraction(s: &str) -> Option<(String, (usize, usize))> {
    s.split(|c: char| c.is_whitespace() || c == '·')
        .filter_map(|w| {
            let (a, b) = w.split_once('/')?;
            let p: (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
            (p.1 > 0 && p.0 <= p.1).then(|| (w.to_string(), p))
        })
        .next()
}

fn compare_cmd(name: &str, text: &str) -> String {
    format!("compare:open:{name}:{text}")
}

/// #138 N4: opening two saved files into Compare costs two point runs - one per file, no whole run
/// (the whole run's performance chart is the selection again at 13 flows, and no comparison row reads
/// it). And the columns are still the whole run's numbers: every field of each variant's output is
/// the whole run's own, to the bit, but the chart.
#[test]
fn a_compare_open_runs_each_file_once_at_its_point() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let engine = Counting::new();
    let mut s = session(&engine);
    let before = engine.calls();

    let a = compare_cmd("a-base.drafthouse", A);
    let b = compare_cmd("b-fan-faster.drafthouse", B);
    let mut frames = 0;
    let mut busy = true;
    let mut commands: Vec<&str> = vec!["screen:compare", &a, &b];
    while busy {
        frames += 1;
        assert!(frames < 20, "the comparison never finishes opening");
        busy = frame_with(&mut s, &engine, &commands).1;
        commands.clear();
    }
    let (whole, point) = engine.calls();
    assert_eq!(
        (whole - before.0, point - before.1),
        (0, 2),
        "two files opened: two point runs and no whole run (whole, point)"
    );

    let variants = STATE.with(|cell| cell.borrow().compare.variants.clone());
    assert_eq!(variants.len(), 2, "both files loaded");
    for v in variants.iter() {
        let shown = v.out.as_ref().expect("the variant ran");
        let whole = engine.inner.run_detached(&v.input).expect("the whole run");
        assert_eq!(
            &EngineOutput {
                thermal_curve: whole.thermal_curve.clone(),
                ..shown.clone()
            },
            &whole,
            "{}: every field but the performance chart is the whole run's",
            v.name
        );
        for m in crate::compare::Metric::ALL {
            assert_eq!(
                crate::compare::value(m, v).map(f64::to_bits),
                m.read(&v.input, &whole).map(f64::to_bits),
                "{}: the {m:?} row is the whole run's number",
                v.name
            );
        }
    }
}

/// Files handed to the comparison while another screen is up open in that frame: nothing there
/// paints their progress, and a frame that ends owing nothing may be the last for a while (an idle
/// loop parks, #82) - so none is left queued. Still one point run per file.
#[test]
fn a_compare_open_off_the_compare_screen_finishes_in_its_frame() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let engine = Counting::new();
    let mut s = session(&engine);
    frame_with(&mut s, &engine, &["screen:instrument"]);
    let before = engine.calls();
    let a = compare_cmd("a-base.drafthouse", A);
    let b = compare_cmd("b-fan-faster.drafthouse", B);
    let (_, busy) = frame_with(&mut s, &engine, &[&a, &b]);
    let (pending, variants) = STATE.with(|cell| {
        let st = cell.borrow();
        (st.compare.pending.len(), st.compare.variants.len())
    });
    assert_eq!(
        (pending, variants, busy),
        (0, 2, false),
        "(files still queued, variants, busy) after the frame"
    );
    let (whole, point) = engine.calls();
    assert_eq!(
        (whole - before.0, point - before.1),
        (0, 2),
        "(whole, point)"
    );
}

/// The point run must not admit a file the whole run refuses: a layer height outside its fill's
/// spec (issue #136) is a column whose run is refused - in the whole run's own words, without a
/// point run.
#[test]
fn a_compare_open_refuses_what_the_whole_run_refuses_in_its_words() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let engine = Counting::new();
    let mut s = session(&engine);
    // the first layer's height, 0.45 m (on its fill's 0.15 m module), made 0.5 m (off it)
    let off_spec = A.replacen(": 0.45,", ": 0.5,", 1);
    assert_ne!(off_spec, A, "the saved file carries the layer this edits");
    let whole = crate::compare::open(
        &off_spec,
        "off-spec.drafthouse",
        &s.cat,
        engine.inner.as_ref(),
    )
    .expect("the file reads");
    let Err(expected) = whole.out else {
        panic!("the whole run refuses the off-spec layer")
    };
    assert!(expected.contains("0.50 m"), "{expected}");
    let before = engine.calls();

    let cmd = compare_cmd("off-spec.drafthouse", &off_spec);
    let mut commands: Vec<&str> = vec!["screen:compare", &cmd];
    for _ in 0..4 {
        frame_with(&mut s, &engine, &commands);
        commands.clear();
    }
    let variants = STATE.with(|cell| cell.borrow().compare.variants.clone());
    assert_eq!(variants.len(), 1, "the off-spec file is a column");
    assert_eq!(
        variants[0].out.as_ref().err(),
        Some(&expected),
        "its run refused in the whole run's words"
    );
    assert_eq!(engine.calls(), before, "a refused run runs nothing");
}

/// Scope 4: every frame a cold path ends with work still owed paints that work's progress - what is
/// running and how far (`done/n`) - and the progress moves. Never a still screen while the main
/// thread is busy: the comparison's file open included (it opens a file a frame, the first frame
/// paints the files queued).
#[test]
fn every_busy_frame_of_a_cold_path_paints_its_progress() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let engine = Counting::new();
    let mut s = session(&engine);
    let a = compare_cmd("a-base.drafthouse", A);
    let b = compare_cmd("b-fan-faster.drafthouse", B);
    // (what the owner opens, the commands, what its progress names)
    let cases: [(&str, Vec<&str>, &str); 6] = [
        ("Size", vec!["screen:size"], "running the selection"),
        (
            "Crossflow",
            vec!["tower:crossflow", "screen:instrument"],
            "running the selection",
        ),
        (
            "Rate",
            vec!["tower:counterflow", "screen:rate"],
            "running the capability evaluation",
        ),
        ("Curves", vec!["screen:curves"], "calculating"),
        (
            "Report charts",
            vec!["page:4", "screen:report"],
            "computing",
        ),
        (
            "Compare",
            vec!["screen:compare", &a, &b],
            "opening the files",
        ),
    ];
    for (label, commands, names) in cases {
        // every case is cold: nothing a previous case computed is reused
        STATE.with(|cell| cell.borrow_mut().cache = data::Cache::default());
        let mut sent = commands.clone();
        let mut busy_frames = 0;
        let mut seen: Vec<(usize, usize)> = Vec::new();
        // watch until the work is done, or it has been sliced and its progress has moved
        let moved = |seen: &[(usize, usize)]| seen.windows(2).any(|w| w[1].0 > w[0].0);
        while !(busy_frames >= 2 && moved(&seen)) {
            assert!(busy_frames < 20_000, "{label}: the work never finishes");
            let (strings, busy) = frame_with(&mut s, &engine, &sent);
            sent.clear();
            if !busy {
                break;
            }
            busy_frames += 1;
            assert!(
                strings.iter().any(|x| x.contains(names)),
                "{label}: busy frame {busy_frames} paints no `{names}`: {strings:?}"
            );
            // the count is in the named line (Rate, Curves, Report) or is the progress ring's own bare
            // `done/n` beside it (Size, Crossflow, Compare)
            let progress = strings
                .iter()
                .filter(|x| x.contains(names))
                .chain(
                    strings
                        .iter()
                        .filter(|x| fraction(x).is_some_and(|(w, _)| w == x.trim())),
                )
                .find_map(|x| fraction(x).map(|(_, p)| p))
                .unwrap_or_else(|| {
                    panic!("{label}: busy frame {busy_frames} paints no done/n: {strings:?}")
                });
            seen.push(progress);
        }
        assert!(
            busy_frames >= 2,
            "{label}: the cold path is sliced across frames (busy frames: {busy_frames})"
        );
        assert!(
            moved(&seen),
            "{label}: the progress moves while the work runs: {:?}",
            &seen[seen.len().saturating_sub(4)..]
        );
    }
}
