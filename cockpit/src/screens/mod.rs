//! drafthouse#91 Part B: the new screens and the app shell that navigates between them.
//!
//! The shell owns the frame whenever a new screen is open; on **Instrument** it only draws its chrome
//! (the rail or the dock, the top HUD) and hands the existing view the rect that is left, so that view is
//! unchanged. Hooks into existing files are kept to the minimum and listed in
//! `docs/design/screens-r1/README.md` (they are the merge points with the sibling lane).
//!
//! State lives here (a thread-local, not a Bevy resource): the UI system already uses every parameter slot.
//! Commands arrive through the page's existing `window.__viz.dispatch(...)` queue - `drain_commands` hands
//! every head it does not know to [`push_command`]:
//!
//!   screen:<instrument|size|rate|curves|water|compare|report>   nav:<rail|dock>   account:<demo|staff>
//!   tower:<counterflow|crossflow>   motion:<on|off>   drawer:<open|close>   cycles:<n>   test:<0..2>
//!   range:<8|10|12>   wb:<C>   target:<C>   cand:<n>   page:<n>   filter:<0..2>   base:<0..2>   info:<id>
//!
//! and the same names are read once from the URL (`?screen=size&nav=dock&account=staff...`).

pub mod compare;
pub mod crossflow;
pub mod curves;
pub mod data;
pub mod kit;
pub mod rate;
pub mod report;
/// drafthouse#85: the report export - the sheet as a real PDF (writer: [`crate::pdf`]).
pub mod report_pdf;
pub mod size;
pub mod water;

/// The fixture-equals-native-CLI tests (issue #83's load-bearing test): the four workspaces, the
/// settings both paths take, and the numbers both must print. Test-only, and real-engine-only
/// (there is no CLI without the engine).
#[cfg(all(test, feature = "real-engine"))]
mod fixture_equals_cli;

/// The crossflow fixture against the same native CLI (issue #84's load-bearing test): the UI's
/// crossflow run and `crossflow`/`convergence` on the same grid inputs, at exact f64 bits. Reuses
/// [`fixture_equals_cli`]'s walker over the CLI's printed bytes. Test-only, real-engine-only.
#[cfg(all(test, feature = "real-engine"))]
mod crossflow_equals_cli;

use std::cell::RefCell;
use std::sync::Mutex;

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Order, Rect, Sense, Stroke, StrokeKind};

use cockpit::engine::{EngineInput, EngineOutput};

use crate::theme as t;
use kit::{glyph, text, Btn};

// ======================================================================================== state

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Instrument,
    Size,
    Rate,
    Curves,
    Water,
    Compare,
    Report,
}

impl Screen {
    pub const ALL: [Screen; 7] = [
        Screen::Instrument,
        Screen::Size,
        Screen::Rate,
        Screen::Curves,
        Screen::Water,
        Screen::Compare,
        Screen::Report,
    ];
    pub fn slug(self) -> &'static str {
        match self {
            Screen::Instrument => "instrument",
            Screen::Size => "size",
            Screen::Rate => "rate",
            Screen::Curves => "curves",
            Screen::Water => "water",
            Screen::Compare => "compare",
            Screen::Report => "report",
        }
    }
    /// The dock's phone-cell label: the one that does not fit a 56 px cell is shortened.
    pub fn short_name(self) -> &'static str {
        match self {
            Screen::Instrument => "Instr",
            other => other.name(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Screen::Instrument => "Instrument",
            Screen::Size => "Size",
            Screen::Rate => "Rate",
            Screen::Curves => "Curves",
            Screen::Water => "Water",
            Screen::Compare => "Compare",
            Screen::Report => "Report",
        }
    }
    fn from_slug(s: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|x| x.slug() == s)
    }
    /// Round 2 (#91): the workflow step a screen belongs to.
    pub fn step(self) -> Step {
        match self {
            Screen::Size => Step::Duty,
            Screen::Instrument => Step::Tower,
            Screen::Curves | Screen::Water => Step::Result,
            Screen::Rate | Screen::Compare => Step::Check,
            Screen::Report => Step::Report,
        }
    }
}

/// Round 2 (#91 decisions, "Workflow"): the duty-first flow the nav is ordered by. Size takes the duty and
/// returns the towers that meet it; the Instrument is the chosen tower with its answer; Curves and Water
/// are the result in depth; Rate and Compare check it; Report closes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Duty,
    Tower,
    Result,
    Check,
    Report,
}

impl Step {
    pub const ALL: [Step; 5] = [
        Step::Duty,
        Step::Tower,
        Step::Result,
        Step::Check,
        Step::Report,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Step::Duty => "Duty",
            Step::Tower => "Tower",
            Step::Result => "Result",
            Step::Check => "Check",
            Step::Report => "Report",
        }
    }
    pub fn number(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0) + 1
    }
    pub fn screens(self) -> &'static [Screen] {
        match self {
            Step::Duty => &[Screen::Size],
            Step::Tower => &[Screen::Instrument],
            Step::Result => &[Screen::Curves, Screen::Water],
            Step::Check => &[Screen::Rate, Screen::Compare],
            Step::Report => &[Screen::Report],
        }
    }
}

/// The nav order: the steps' screens, in step order.
pub const NAV_ORDER: [Screen; 7] = [
    Screen::Size,
    Screen::Instrument,
    Screen::Curves,
    Screen::Water,
    Screen::Rate,
    Screen::Compare,
    Screen::Report,
];

/// Issue #137: where an answer figure comes from, in the words the source mark shows.
pub const ORIGIN_RUN: &str = "this duty, calculated";
pub const ORIGIN_SELECTION: &str = "the best crossflow candidate, calculated";

/// The always-visible answer (#91 decisions, "Workflow"): cold water vs target with PASS/FAIL and the
/// margin, approach, fan power, drift and water use - all from this frame's `Engine::run` of the draft,
/// drift from the draft eliminator's own curve at the run's face velocity.
#[derive(Clone, Debug, Default)]
pub struct Answer {
    pub cold_c: f64,
    pub target_c: f64,
    pub margin_k: f64,
    pub pass: bool,
    pub approach_k: f64,
    pub fan_kw: f64,
    pub drift_ppm: Option<f64>,
    pub drift_limit_ppm: Option<f64>,
    pub makeup_m3_h: f64,
    pub refused: Option<String>,
    /// What the answer is for: the draft, or (Crossflow) the selection's best crossflow candidate.
    pub subject: String,
    /// Where the numbers come from, in plain words, for the marks: the draft's own run or the
    /// selection's best candidate (issue #137: no function names on screen).
    pub origin: &'static str,
    /// Issue #137: the run's own worked calculation cards (`EngineOutput::worked_steps`), the same steps
    /// the Report and the PDF print. Empty when the answer is not the draft's run (the crossflow
    /// candidate), so no card claims arithmetic the screen did not run.
    pub steps: Vec<cockpit::engine::WorkedStep>,
    /// Range (K) and heat load (kW) of the draft's run, for the answer card. `None` off the draft's run.
    pub range_k: Option<f64>,
    pub heat_kw: Option<f64>,
    pub kavl: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    /// Variant 1: a persistent left rail (desktop); a top bar + slide-out rail (phone).
    Rail,
    /// Variant 2: a floating glass dock at the bottom (both sizes), HUD clusters at the top.
    Dock,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Account {
    Demo,
    Staff,
}

/// Issue #89: the comparison surface's state - the loaded variants (each a saved project file the
/// engine recomputed on open) and the files it could not open, named.
#[derive(Default)]
pub struct CompareState {
    /// Shared, so a frame's paint holds them with a pointer copy (issue #138); an open is the only write.
    pub variants: std::rc::Rc<Vec<crate::compare::Variant>>,
    /// `compare:open:<name>:<text>` commands, queued by [`apply`] and opened on the next frame -
    /// the frame is where the engine and the catalog are in hand.
    pub pending: Vec<(String, String)>,
    /// The loader's own refusals, shown by the surface: never silently dropped.
    pub problems: Vec<String>,
}

pub struct State {
    pub screen: Screen,
    pub nav: Nav,
    pub account: Account,
    pub crossflow: bool,
    pub motion: bool,
    pub drawer: bool,
    pub cycles: f64,
    pub test_point: usize,
    pub range_c: f64,
    pub probe_wb: f64,
    pub target_cold: Option<f64>,
    pub cand: Option<usize>,
    pub page: usize,
    /// Report: which sheet of the worked steps is shown (issue #137: they run over sheets).
    pub steps_sheet: usize,
    /// Size: the tower-type filter (0 all, 1 counterflow, 2 crossflow). Compare: the baseline column.
    pub filter: usize,
    pub base: usize,
    pub info: Option<String>,
    pub toast: Option<(String, f64)>,
    pub url_read: bool,
    pub cache: data::Cache,
    /// Issue #89: the compare screen's loaded variants.
    pub compare: CompareState,
    pub busy: bool,
    pub frame: u64,
    /// Round 2: the last screen open in each step (a step with two screens reopens the one you left).
    pub step_last: [Screen; 5],
    pub answer: Option<Answer>,
    /// Phone: the answer row expanded into its card.
    pub answer_open: bool,
    /// Issue #137: the open calculation card's rect, as last drawn. A tap on the card goes back (closes
    /// it) instead of reaching a source mark or a control underneath.
    pub calc_rect: Option<Rect>,
    /// The docked side panel the screen drew this frame ([`kit::side_panel`]); a desktop calculation
    /// card takes its column.
    pub side_panel: Option<Rect>,
    /// Curves on a phone: 0 = cold water vs wet bulb, 1 = fan vs system.
    pub chart: usize,
    /// The source mark whose detail is open: (class, origin, anchor).
    pub src_open: Option<(kit::Src, String, egui::Pos2)>,
    /// Recorded requirements from the fixture (`provenance.fixed`), read once.
    pub limits: Option<(Option<f64>, Option<f64>, f64)>,
    /// `src:<n>` (evidence): open the n-th source mark painted on the next frame.
    pub src_pending: Option<usize>,
    /// Issue #137: a calculation card asked for by name (`calc:range`), opened on the next frame at the
    /// mark that carries it.
    pub calc_pending: Option<&'static str>,
    /// Issue #137: the phone's open answer card, this frame. A calculation card opened from one of its
    /// marks is placed clear of it, so the card never covers the answer's own figures.
    /// The motion state last sent to the instrument's clock (`freeze:`), so the one Motion control drives
    /// both the new screens and the instrument.
    pub motion_sent: Option<bool>,
    /// Source marks painted last frame, by class (calculated, catalog, illustrative): the evidence count.
    pub mark_counts: [usize; 3],
}

impl Default for State {
    fn default() -> Self {
        Self {
            screen: Screen::Instrument,
            nav: Nav::Rail,
            account: Account::Demo,
            crossflow: false,
            motion: true,
            drawer: false,
            cycles: f64::NAN,
            test_point: 0,
            range_c: 10.0,
            probe_wb: f64::NAN,
            target_cold: None,
            cand: None,
            page: 0,
            steps_sheet: 0,
            filter: 0,
            base: 0,
            info: None,
            toast: None,
            url_read: false,
            cache: data::Cache::default(),
            compare: CompareState::default(),
            busy: false,
            frame: 0,
            step_last: [
                Screen::Size,
                Screen::Instrument,
                Screen::Curves,
                Screen::Rate,
                Screen::Report,
            ],
            answer: None,
            answer_open: false,
            calc_rect: None,
            side_panel: None,
            chart: 0,
            src_open: None,
            limits: None,
            src_pending: None,
            calc_pending: None,
            motion_sent: None,
            mark_counts: [0; 3],
        }
    }
}

impl State {
    /// Open a screen (the nav, the sibling tabs and the deep links all come through here).
    pub fn go(&mut self, s: Screen) {
        self.screen = s;
        self.step_last[s.step().number() - 1] = s;
        self.drawer = false;
        self.info = None;
        self.src_open = None;
        self.answer_open = false;
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}
static INBOX: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// `drain_commands` forwards every command head it does not handle.
pub fn push_command(cmd: &str) {
    INBOX.lock().unwrap().push(cmd.to_string());
}

fn apply(st: &mut State, cmd: &str) {
    let (head, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
    let num = arg.parse::<f64>().ok();
    match head {
        "screen" => {
            if let Some(s) = Screen::from_slug(arg) {
                st.go(s);
            }
        }
        "chart" => st.chart = if arg == "fan" || arg == "1" { 1 } else { 0 },
        "answer" => st.answer_open = arg == "open" || arg == "1",
        "src" => st.src_pending = num.map(|v| v as usize),
        "calc" => st.calc_pending = calc_label(arg),
        "nav" => {
            st.nav = if arg == "dock" { Nav::Dock } else { Nav::Rail };
        }
        "account" => {
            st.account = if arg == "staff" {
                Account::Staff
            } else {
                Account::Demo
            };
        }
        "tower" => st.crossflow = arg == "crossflow",
        "motion" => st.motion = arg != "off" && arg != "0",
        "drawer" => st.drawer = arg == "open",
        "cycles" => {
            if let Some(v) = num {
                st.cycles = v.clamp(1.5, 10.0);
            }
        }
        "test" => {
            if let Some(v) = num {
                st.test_point = (v as usize).min(2);
            }
        }
        "range" => {
            if let Some(v) = num {
                st.range_c = v;
            }
        }
        "wb" => {
            if let Some(v) = num {
                st.probe_wb = v;
            }
        }
        "target" => st.target_cold = num,
        "cand" => st.cand = num.map(|v| v as usize),
        "page" => {
            if let Some(v) = num {
                st.page = (v as usize).min(9);
            }
        }
        "sheet" => st.steps_sheet = num.map(|v| v as usize).unwrap_or(0).min(9),
        "filter" => st.filter = num.map(|v| v as usize).unwrap_or(0).min(2),
        "base" => st.base = num.map(|v| v as usize).unwrap_or(0).min(2),
        // issue #89: the page hands saved project files to the comparison - `compare:open:<name>:<text>`.
        // The open itself happens in `frame`, where the engine and the catalog are in hand.
        "compare" => {
            if arg == "clear" {
                st.compare = CompareState::default();
            } else if let Some(rest) = arg.strip_prefix("open:") {
                let (name, text) = rest.split_once(':').unwrap_or((rest, ""));
                if name.is_empty() {
                    st.compare
                        .problems
                        .push("compare:open needs a file name".to_string());
                } else if st.compare.variants.len() + st.compare.pending.len()
                    < crate::compare::MAX_VARIANTS
                {
                    st.compare
                        .pending
                        .push((name.to_string(), text.to_string()));
                } else {
                    st.compare.problems.push(format!(
                        "a comparison holds {} variants; `{name}` was not added",
                        crate::compare::MAX_VARIANTS
                    ));
                }
            }
        }
        "info" => {
            st.info = if arg.is_empty() || arg == "none" {
                None
            } else {
                Some(arg.to_string())
            }
        }
        _ => {}
    }
}

#[cfg(target_arch = "wasm32")]
fn read_url(st: &mut State) {
    let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) else {
        return;
    };
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return;
    };
    for key in [
        "screen", "nav", "account", "tower", "motion", "drawer", "cycles", "test", "range", "wb",
        "target", "cand", "page", "filter", "base", "info", "chart", "answer", "src",
    ] {
        if let Some(v) = params.get(key) {
            apply(st, &format!("{key}:{v}"));
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn read_url(_st: &mut State) {}

// ======================================================================================== frame

/// What the instrument gets back from the shell.
pub enum Frame {
    /// A new screen drew the whole frame: the existing view draws nothing.
    Took,
    /// Instrument is on show: lay it out in this rect (the shell's chrome sits around it).
    Instrument(Rect),
}

/// Issue #85 completion: the canonical bytes of the `.drafthouse` project document an export is
/// built from, as the shell hands them to the Report screen. `text` is the document the session's
/// own writer (`crate::files::snapshot_text`, the call the File menu's save makes) produces from the
/// current state; `unchanged` is the session's verdict that the working state still matches the
/// document it last saved or opened - so `text` is that file's own bytes, byte for byte, and the
/// report may carry its digest as the saved document's.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentBytes {
    pub text: String,
    pub unchanged: bool,
}

/// Issue #85 completion: how the shell offers the Report export its document, read at click time.
/// `None` from the callback means no project document backs the session (the export keeps its
/// labelled draft fallback).
pub type DocumentSource<'a> = &'a dyn Fn(&EngineInput) -> Option<DocumentBytes>;

pub struct Ctx<'a> {
    pub draft: Option<&'a mut EngineInput>,
    pub engine: Option<&'a dyn crate::engine_select::CockpitEngine>,
    pub out: Option<&'a EngineOutput>,
    pub fixture_text: &'a str,
    pub reduced_motion: bool,
    pub t: f32,
    /// Issue #85 completion: the project document behind the Report export, if the session holds
    /// one. Built by `ui::viz_ui` from the file session and the writer; called only when the
    /// export button is pressed.
    pub document: Option<DocumentSource<'a>>,
    /// Issue #89: the session's catalog, for the comparison's variant reader (the same one
    /// `files::open_text` resolves file records against).
    pub catalog: Option<&'a crate::state::Catalog>,
}

/// Per-frame entry, called by `ui::viz_ui` right after its engine run.
pub fn frame(ctx: &egui::Context, mut c: Ctx<'_>) -> Frame {
    STATE.with(|cell| {
        let mut st = cell.borrow_mut();
        st.frame += 1;
        if !st.url_read {
            st.url_read = true;
            if c.reduced_motion {
                st.motion = false;
            }
            read_url(&mut st);
            st.motion_sent = if st.motion { Some(true) } else { None };
            let s = st.screen;
            st.go(s);
        }
        let cmds: Vec<String> = INBOX.lock().unwrap().drain(..).collect();
        for cmd in &cmds {
            apply(&mut st, cmd);
        }
        // issue #137: the part names every screen prints next to a code
        if let Some(catalog) = c.catalog {
            kit::parts_set(catalog);
        }
        // issue #89: open the comparison's queued project files - the engine and the catalog, which
        // the reader needs, live here. Each open is one engine run of that file's own inputs.
        if !st.compare.pending.is_empty() {
            if let Some(catalog) = c.catalog {
                let pending: Vec<(String, String)> = st.compare.pending.drain(..).collect();
                let mut loaded = 0usize;
                let mut refused: Option<String> = None;
                for (name, text) in pending {
                    match c.engine {
                        Some(engine) => match crate::compare::open(&text, &name, catalog, engine) {
                            Ok(v) => {
                                std::rc::Rc::make_mut(&mut st.compare.variants).push(v);
                                loaded += 1;
                            }
                            Err(e) => {
                                st.compare.problems.push(format!("{name}: {e}"));
                                refused = Some(name);
                            }
                        },
                        None => {
                            st.compare
                                .problems
                                .push(format!("{name}: cannot be calculated in this build"));
                            refused = Some(name);
                        }
                    }
                }
                if loaded > 0 {
                    toast(
                        &mut st,
                        &format!("{loaded} variant(s) loaded for comparison"),
                    );
                }
                if let Some(name) = refused {
                    toast(&mut st, &format!("{name} could not be opened"));
                }
            } else {
                // no catalog yet: keep the queue for the next frame rather than dropping the files
                ctx.request_repaint();
            }
        }
        let screen = ctx.viewport_rect();
        let phone = screen.width() < 760.0;
        kit::strings_begin();
        // issue #117: the same per-frame discipline for the controls' hit rects - the screens report
        // what they drew, and `publish` hands the inventory to a harness.
        kit::hits_begin();

        let show_new = st.screen != Screen::Instrument || st.crossflow;
        let want = match st.screen {
            Screen::Instrument if st.crossflow => data::Want::Crossflow,
            Screen::Size => data::Want::Size,
            Screen::Rate => data::Want::Rate,
            Screen::Curves => data::Want::Curves,
            // the compare screen loads only what it was handed (saved project files), so it asks
            // the data cache for nothing (#89)
            Screen::Compare => data::Want::Nothing,
            // the Charts page draws the selection and the curves grid
            Screen::Report if st.page == 4 => data::Want::Report,
            _ => data::Want::Nothing,
        };
        if let Some(d) = c.draft.as_deref() {
            if st.cycles.is_nan() {
                st.cycles = d.duty.cycles_of_concentration;
            }
            if st.probe_wb.is_nan() {
                st.probe_wb = d.duty.wet_bulb_c;
            }
            let fixture = c.fixture_text;
            let busy = st.cache.step(want, fixture, d, c.out, c.engine);
            st.busy = busy;
            if busy {
                ctx.request_repaint();
            }
        }

        // the answer (#91 round 2): this frame's run of the draft, or the crossflow candidate on show
        if st.limits.is_none() {
            st.limits = Some(data::recorded_limits(c.fixture_text));
        }
        let limits = st.limits.unwrap_or((None, None, 0.0));
        st.answer = c.draft.as_deref().map(|d| {
            if st.screen == Screen::Instrument && st.crossflow {
                match st.cache.xf.as_ref() {
                    Some(Ok(g)) => xf_answer(d, &g.cand, limits),
                    _ => Answer {
                        target_c: d.duty.target_cold_water_c,
                        refused: Some("the crossflow run is still computing".into()),
                        subject: "Crossflow".into(),
                        origin: ORIGIN_SELECTION,
                        ..Default::default()
                    },
                }
            } else {
                let mut a = data::answer(d, c.out, limits);
                // issue #137: the run in the house units - fan speed as a % of rated, not a bare ratio
                a.subject = format!(
                    "this design: {} · {} at {} % speed",
                    d.tower.id,
                    d.fan.id,
                    t::num::pct(d.speed_ratio * 100.0)
                );
                a.origin = ORIGIN_RUN;
                a
            }
        });
        // one Motion control: the instrument's clock follows it
        if st.motion_sent != Some(st.motion) {
            if let Ok(mut q) = crate::app::COMMANDS.lock() {
                q.push(format!("freeze:{}", if st.motion { 0 } else { 1 }));
            }
            st.motion_sent = Some(st.motion);
        }

        let content = layout(screen, phone, st.nav, show_new);
        let result = if show_new {
            egui::Area::new(egui::Id::new("screens.body"))
                .fixed_pos(screen.min)
                .order(Order::Middle)
                .show(ctx, |ui| {
                    ui.set_clip_rect(screen);
                    ui.expand_to_include_rect(screen);
                    body(ui, &mut st, &mut c, content, phone);
                });
            Frame::Took
        } else {
            Frame::Instrument(content)
        };
        chrome(ctx, &mut st, screen, content, phone, c.out);
        if show_new {
            answer_bar(ctx, &mut st, screen, phone);
        }
        st.side_panel = kit::side_panel_take();
        let marks = kit::marks_take();
        st.mark_counts = [kit::Src::Calc, kit::Src::Catalog, kit::Src::Illus]
            .map(|k| marks.iter().filter(|m| m.1 == k).count());
        src_detail(ctx, &mut st, &marks, screen, phone);
        publish(&st, screen, content);
        result
    })
}

/// The rect the shell leaves for a screen (and for the instrument).
fn layout(screen: Rect, phone: bool, nav: Nav, answer: bool) -> Rect {
    let r = layout_nav(screen, phone, nav);
    if !answer {
        return r;
    }
    let h = if phone { ANSWER_PHONE_H } else { ANSWER_DESK_H };
    Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - h))
}

/// The answer's own strip: the bottom of the rect the nav leaves.
fn answer_rect(screen: Rect, phone: bool, nav: Nav) -> Rect {
    let r = layout_nav(screen, phone, nav);
    let h = if phone { ANSWER_PHONE_H } else { ANSWER_DESK_H };
    Rect::from_min_max(pos2(r.left(), r.bottom() - h), r.max)
}

fn layout_nav(screen: Rect, phone: bool, nav: Nav) -> Rect {
    match (nav, phone) {
        (Nav::Rail, false) => {
            Rect::from_min_max(pos2(screen.left() + RAIL_W, screen.top()), screen.max)
        }
        (Nav::Rail, true) => {
            Rect::from_min_max(pos2(screen.left(), screen.top() + TOP_PHONE), screen.max)
        }
        (Nav::Dock, false) => Rect::from_min_max(
            screen.min,
            pos2(screen.right(), screen.bottom() - DOCK_DESK_RESERVE),
        ),
        (Nav::Dock, true) => Rect::from_min_max(
            pos2(screen.left(), screen.top() + TOP_PHONE),
            pos2(screen.right(), screen.bottom() - DOCK_PHONE_H),
        ),
    }
}

const RAIL_W: f32 = 76.0;
const TOP_PHONE: f32 = 52.0;
const DOCK_PHONE_H: f32 = 66.0;
const DOCK_DESK_RESERVE: f32 = 96.0;
/// The answer strip under every new screen (round 2): desktop, and the phone's one-row version.
const ANSWER_DESK_H: f32 = 58.0;
const ANSWER_PHONE_H: f32 = 50.0;
/// The new screens' own top band (title + context controls) on desktop.
pub const TOP_DESK: f32 = 58.0;

fn body(ui: &mut egui::Ui, st: &mut State, c: &mut Ctx<'_>, area: Rect, phone: bool) {
    let p = ui.painter().clone();
    p.rect_filled(ui.clip_rect(), kit::r(0), t::BG);
    let Some(draft) = c.draft.as_deref_mut() else {
        text(
            &p,
            area.center(),
            Align2::CENTER_CENTER,
            "Loading…",
            kit::sans(13.0),
            t::MUTED,
        );
        return;
    };
    let env = Env {
        out: c.out,
        engine: c.engine,
        t: if st.motion { c.t } else { 0.0 },
        motion: st.motion,
        phone,
        document: c.document,
    };
    match st.screen {
        Screen::Instrument => crossflow::ui(ui, st, draft, &env, area),
        Screen::Size => size::ui(ui, st, draft, &env, area),
        Screen::Rate => rate::ui(ui, st, draft, &env, area),
        Screen::Curves => curves::ui(ui, st, draft, &env, area),
        Screen::Water => water::ui(ui, st, draft, &env, area),
        Screen::Compare => compare::ui(ui, st, draft, &env, area),
        Screen::Report => report::ui(ui, st, draft, &env, area),
    }
}

/// What a screen reads besides its own state.
pub struct Env<'a> {
    pub out: Option<&'a EngineOutput>,
    pub engine: Option<&'a dyn crate::engine_select::CockpitEngine>,
    /// Seconds of animation; 0 when motion is off (every motion helper then paints its rest pose).
    pub t: f32,
    pub motion: bool,
    pub phone: bool,
    /// Issue #85 completion: see [`Ctx::document`] - the project document the Report export may
    /// carry in its revision row, or `None` for the labelled draft fallback.
    pub document: Option<DocumentSource<'a>>,
}

/// A screen's title band (desktop): the screen name, a one-line context and a slot for its controls.
/// Returns the rect below it.
pub fn title_band(ui: &egui::Ui, area: Rect, title: &str, sub: &str, phone: bool) -> Rect {
    let p = ui.painter();
    if phone {
        // the phone's top bar already names the screen
        let _ = (title, sub);
        return area;
    }
    let band = Rect::from_min_size(area.min, vec2(area.width(), TOP_DESK));
    // round 2: where this screen sits in the duty-first flow
    let step = Screen::ALL
        .into_iter()
        .find(|s| s.name() == title)
        .map(|s| s.step())
        .unwrap_or(Step::Tower);
    let crumb = text(
        p,
        pos2(band.left() + 24.0, band.center().y),
        Align2::LEFT_CENTER,
        &format!("{} · {}", step.number(), step.name().to_uppercase()),
        kit::semi(10.5),
        t::PRIMARY,
    );
    let tx = crumb.right() + 12.0;
    text(
        p,
        pos2(tx, band.center().y - 1.0),
        Align2::LEFT_CENTER,
        title,
        kit::semi(19.0),
        t::INK,
    );
    let w = kit::measure(p, title, kit::semi(19.0)).x + (tx - band.left() - 24.0);
    text(
        p,
        pos2(band.left() + 24.0 + w + 14.0, band.center().y),
        Align2::LEFT_CENTER,
        sub,
        kit::sans(12.0),
        t::MUTED,
    );
    p.line_segment(
        [
            pos2(band.left(), band.bottom()),
            pos2(band.right(), band.bottom()),
        ],
        Stroke::new(1.0, t::LINE_SOFT),
    );
    Rect::from_min_max(pos2(area.left(), band.bottom()), area.max)
}

/// The ⓘ popover: a short glass card anchored near `at`. Tapping anywhere else closes it.
pub fn info_card(
    ui: &egui::Ui,
    st: &mut State,
    id: &str,
    at: egui::Pos2,
    lines: &[&str],
    bounds: Rect,
) {
    if st.info.as_deref() != Some(id) {
        return;
    }
    let p = ui.ctx().layer_painter(egui::LayerId::new(
        Order::Tooltip,
        egui::Id::new("screens.info"),
    ));
    let w = 260f32.min(bounds.width() - 24.0);
    let h = 18.0 + lines.len() as f32 * 18.0;
    let mut r = Rect::from_min_size(pos2(at.x - w + 16.0, at.y + 14.0), vec2(w, h));
    if r.left() < bounds.left() + 8.0 {
        r = r.translate(vec2(bounds.left() + 8.0 - r.left(), 0.0));
    }
    if r.bottom() > bounds.bottom() - 8.0 {
        r = r.translate(vec2(0.0, -(h + 28.0)));
    }
    p.rect_filled(r.expand(1.0), kit::r(9), t::with_alpha(Color32::BLACK, 90));
    p.rect_filled(r, kit::r(8), t::PANEL_RAISED);
    p.rect_stroke(
        r,
        kit::r(8),
        Stroke::new(1.0, t::PRIMARY_DEEP),
        StrokeKind::Inside,
    );
    for (i, l) in lines.iter().enumerate() {
        text(
            &p,
            pos2(r.left() + 12.0, r.top() + 10.0 + i as f32 * 18.0),
            Align2::LEFT_TOP,
            l,
            kit::sans(12.0),
            if i == 0 { t::INK } else { t::INK_2 },
        );
    }
}

/// Toggle an ⓘ id.
pub fn toggle_info(st: &mut State, id: &str) {
    if st.info.as_deref() == Some(id) {
        st.info = None;
    } else {
        st.info = Some(id.to_string());
    }
}

/// A short status line at the bottom of the frame.
pub fn toast(st: &mut State, s: &str) {
    st.toast = Some((s.to_string(), crate::clock::now_ms()));
}

// ======================================================================================= chrome

fn chrome(
    ctx: &egui::Context,
    st: &mut State,
    screen: Rect,
    content: Rect,
    phone: bool,
    out: Option<&EngineOutput>,
) {
    match (st.nav, phone) {
        (Nav::Rail, false) => rail_desktop(ctx, st, screen),
        (Nav::Rail, true) => {
            top_phone(ctx, st, screen, true);
            if st.drawer {
                drawer(ctx, st, screen);
            }
        }
        (Nav::Dock, false) => {
            dock(ctx, st, screen, false);
            hud_desktop(ctx, st, screen, content);
        }
        (Nav::Dock, true) => {
            top_phone(ctx, st, screen, false);
            dock(ctx, st, screen, true);
        }
    }
    if !phone && st.nav == Nav::Rail {
        hud_desktop(ctx, st, screen, content);
    }
    let _ = out;
    // the toast
    if let Some((msg, at)) = st.toast.clone() {
        let age = crate::clock::now_ms() - at;
        if age > 3200.0 {
            st.toast = None;
        } else {
            area(ctx, "screens.toast", screen, Order::Tooltip, |ui| {
                let p = ui.painter();
                let w = kit::measure(p, &msg, kit::sans(12.5)).x + 32.0;
                let show_new = st.screen != Screen::Instrument || st.crossflow;
                let y = if show_new {
                    answer_rect(screen, phone, st.nav).top() - 30.0
                } else if st.nav == Nav::Dock {
                    screen.bottom()
                        - (if phone {
                            DOCK_PHONE_H
                        } else {
                            DOCK_DESK_RESERVE
                        })
                        - 52.0
                } else {
                    screen.bottom() - 60.0
                };
                let r = Rect::from_center_size(pos2(content.center().x, y), vec2(w, 38.0));
                kit::sheet(p, r, 19);
                text(
                    p,
                    r.center(),
                    Align2::CENTER_CENTER,
                    &msg,
                    kit::sans(12.5),
                    t::INK,
                );
            });
        }
    }
}

fn area(ctx: &egui::Context, id: &str, rect: Rect, order: Order, add: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(rect.min)
        .order(order)
        .show(ctx, |ui| {
            ui.set_clip_rect(rect.expand(2.0));
            ui.expand_to_include_rect(rect);
            add(ui)
        });
}

fn nav_item(ui: &egui::Ui, st: &mut State, r: Rect, s: Screen, show_label: bool, id_suffix: &str) {
    let resp = kit::hit(ui, r, &format!("nav.{}.{id_suffix}", s.slug()));
    let p = ui.painter();
    let sel = st.screen == s;
    if sel {
        p.rect_filled(r.shrink(4.0), kit::r(10), t::PRIMARY_SOFT);
    } else if resp.hovered() {
        p.rect_filled(r.shrink(4.0), kit::r(10), t::PANEL_RAISED);
    }
    let col = if sel {
        t::PRIMARY
    } else if resp.hovered() {
        t::INK
    } else {
        t::INK_2
    };
    let g = if s == Screen::Instrument && st.crossflow {
        "crossflow"
    } else {
        s.slug()
    };
    let gy = if show_label {
        r.center().y - 8.0
    } else {
        r.center().y
    };
    glyph(p, g, pos2(r.center().x, gy), 20.0, col);
    if show_label {
        // a narrow cell (the phone dock at 390) cannot hold "Instrument" at a legible size
        let narrow = r.width() < 64.0;
        let name = if narrow { s.short_name() } else { s.name() };
        text(
            p,
            pos2(r.center().x, r.center().y + 14.0),
            Align2::CENTER_CENTER,
            name,
            kit::semi(if narrow { 11.0 } else { 10.5 }),
            if sel {
                t::INK
            } else if narrow {
                t::INK_2
            } else {
                t::MUTED
            },
        );
    }
    if resp.clicked() {
        st.go(s);
    }
}

fn rail_desktop(ctx: &egui::Context, st: &mut State, screen: Rect) {
    let r = Rect::from_min_size(screen.min, vec2(RAIL_W, screen.height()));
    area(ctx, "screens.rail", r, Order::Foreground, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(r, kit::r(0), t::RAIL_BG);
        p.line_segment(
            [r.right_top(), r.right_bottom()],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        // the mark: the host's teal tile with the house initial (type, not a logo asset)
        let m = Rect::from_center_size(pos2(r.center().x, r.top() + 30.0), vec2(34.0, 34.0));
        p.rect_filled(m, kit::r(8), t::PRIMARY_DEEP);
        text(
            &p,
            m.center(),
            Align2::CENTER_CENTER,
            "D",
            kit::semi(17.0),
            t::INK,
        );
        // round 2: the screens grouped by the duty-first flow, each group under its step
        let item_h = 54.0;
        let mut y = r.top() + 60.0;
        let cur = current_step(st);
        for step in Step::ALL {
            text(
                &p,
                pos2(r.center().x, y + 7.0),
                Align2::CENTER_CENTER,
                &format!("{} {}", step.number(), step.name().to_uppercase()),
                kit::semi(9.5),
                if step == cur { t::PRIMARY } else { t::MUTED },
            );
            y += 15.0;
            for s in step.screens() {
                let ir = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(RAIL_W - 12.0, item_h));
                nav_item(ui, st, ir, *s, true, "rail");
                if st.screen == *s {
                    p.rect_filled(
                        Rect::from_min_size(
                            pos2(r.left(), ir.top() + 12.0),
                            vec2(3.0, item_h - 24.0),
                        ),
                        kit::r(2),
                        t::PRIMARY,
                    );
                }
                y += item_h;
            }
            y += 5.0;
        }
        // Issue #137: the rail is one column, top to bottom, and nothing is anchored to its bottom edge.
        // The old bottom cluster (Open, Save, Motion) was laid out upward from the window's bottom, so
        // on a window shorter than the step list it was drawn over Rate, Compare and Report. Open and
        // Save already live in the Project menu (top right); Motion follows the last step, and goes
        // only where the column still has room for it.
        let item = 52.0;
        let motion_top = y + 6.0;
        if motion_top + item <= r.bottom() - 4.0 {
            p.line_segment(
                [
                    pos2(r.left() + 16.0, motion_top),
                    pos2(r.right() - 16.0, motion_top),
                ],
                Stroke::new(1.0, t::LINE_SOFT),
            );
            let br = Rect::from_min_size(
                pos2(r.left() + 6.0, motion_top + 4.0),
                vec2(RAIL_W - 12.0, item),
            );
            let resp = kit::hit(ui, br, "rail.motion");
            if resp.hovered() {
                p.rect_filled(br.shrink(4.0), kit::r(10), t::PANEL_RAISED);
            }
            glyph(
                &p,
                if st.motion { "motion" } else { "still" },
                pos2(br.center().x, br.center().y - 7.0),
                17.0,
                if resp.hovered() { t::INK } else { t::MUTED },
            );
            text(
                &p,
                pos2(br.center().x, br.center().y + 14.0),
                Align2::CENTER_CENTER,
                if st.motion { "Motion" } else { "Still" },
                kit::sans(10.0),
                t::MUTED,
            );
            if resp.clicked() {
                shell_action(st, "motion");
            }
        }
    });
}

fn shell_action(st: &mut State, id: &str) {
    match id {
        "motion" => {
            st.motion = !st.motion;
            toast(
                st,
                if st.motion {
                    "Motion on"
                } else {
                    "Motion off - still frames"
                },
            );
        }
        "save" => toast(st, "Save project.drafthouse - design only, no file written"),
        "open" => toast(st, "Open a .drafthouse file - design only"),
        "signin" => {
            st.account = Account::Staff;
            toast(st, "Signed in (stub identity)");
        }
        "signout" => {
            st.account = Account::Demo;
            toast(st, "Back to DEMO");
        }
        _ => {}
    }
}

/// The account + catalog cluster: DEMO badge and lock, or the signed-in identity.
fn account_cluster(ui: &egui::Ui, st: &mut State, right: egui::Pos2, compact: bool) -> f32 {
    let p = ui.painter().clone();
    let mut x = right.x;
    let cy = right.y;
    match st.account {
        Account::Demo => {
            let w = if compact { 72.0 } else { 84.0 };
            let br = Rect::from_min_max(pos2(x - w, cy - 18.0), pos2(x, cy + 18.0));
            if kit::button(ui, br, "acct.signin", "Sign in", Btn::Primary).clicked() {
                shell_action(st, "signin");
            }
            x = br.left() - 8.0;
            if !compact {
                // the real-catalog lock
                let lr = Rect::from_min_max(pos2(x - 158.0, cy - 18.0), pos2(x, cy + 18.0));
                let resp = kit::hit(ui, lr, "acct.lock");
                p.rect_filled(lr, kit::r(6), t::PANEL);
                p.rect_stroke(
                    lr,
                    kit::r(6),
                    Stroke::new(1.0, if resp.hovered() { t::AMBER } else { t::LINE }),
                    StrokeKind::Inside,
                );
                kit::lock_glyph(&p, pos2(lr.left() + 16.0, cy - 1.0), 5.0, t::AMBER);
                text(
                    &p,
                    pos2(lr.left() + 28.0, cy),
                    Align2::LEFT_CENTER,
                    data::STUB_CATALOG,
                    kit::sans(12.0),
                    t::INK_2,
                );
                if resp.clicked() {
                    toggle_info(st, "lock");
                }
                x = lr.left() - 8.0;
            }
            let b = kit::demo_badge(&p, pos2(x, cy), Align2::RIGHT_CENTER);
            x = b.left() - 8.0;
        }
        Account::Staff => {
            let ar = Rect::from_center_size(pos2(x - 18.0, cy), vec2(36.0, 36.0));
            let resp = kit::hit(ui, ar.expand(4.0), "acct.me");
            p.circle_filled(ar.center(), 17.0, t::PRIMARY_DEEP);
            p.circle_stroke(
                ar.center(),
                17.0,
                Stroke::new(1.0, if resp.hovered() { t::PRIMARY } else { t::LINE }),
            );
            text(
                &p,
                ar.center(),
                Align2::CENTER_CENTER,
                data::STUB_USER_INITIALS,
                kit::semi(12.0),
                t::INK,
            );
            if resp.clicked() {
                toggle_info(st, "me");
            }
            x = ar.left() - 10.0;
            if !compact {
                let c = kit::chip(
                    &p,
                    pos2(x, cy),
                    Align2::RIGHT_CENTER,
                    data::STUB_CATALOG,
                    t::INK,
                    t::OK_SOFT,
                    t::with_alpha(t::OK, 140),
                );
                let s = kit::stub_tag(&p, pos2(c.left() - 6.0, cy), Align2::RIGHT_CENTER);
                x = s.left() - 8.0;
            }
        }
    }
    x
}

fn tower_switch(ui: &egui::Ui, st: &mut State, r: Rect, compact: bool) {
    let opts: &[&str] = if compact {
        &["CF", "XF"]
    } else {
        &["Counterflow", "Crossflow"]
    };
    if let Some(i) = kit::segmented(
        ui,
        r,
        "tower",
        opts,
        st.crossflow as usize,
        if compact { 11.5 } else { 12.0 },
    ) {
        st.crossflow = i == 1;
        // issue #138: the grid's inputs (the selection and the duty) do not move with the view, so
        // the cached grid stays - flipping the section back is not a recompute
        if st.screen == Screen::Instrument {
            toast(
                st,
                if st.crossflow {
                    "Crossflow section"
                } else {
                    "Counterflow instrument"
                },
            );
        }
    }
}

/// The desktop top-right HUD: tower type, project, account. Floats over the screen's title band.
fn hud_desktop(ctx: &egui::Context, st: &mut State, screen: Rect, content: Rect) {
    let on_instrument = st.screen == Screen::Instrument && !st.crossflow;
    // On the instrument the existing header owns the top band; the HUD drops to a slim strip under the rail
    // variant's own space - it sits at the bottom-right corner instead so nothing of the view is covered.
    let h = 44.0;
    let r = if on_instrument {
        if st.nav == Nav::Dock {
            return;
        }
        return;
    } else {
        Rect::from_min_max(
            pos2(
                content.right() - 720.0,
                content.top() + (TOP_DESK - h) / 2.0,
            ),
            pos2(content.right() - 16.0, content.top() + (TOP_DESK + h) / 2.0),
        )
    };
    let _ = screen;
    area(ctx, "screens.hud", r, Order::Foreground, |ui| {
        let x = account_cluster(ui, st, pos2(r.right(), r.center().y), false);
        // project chip
        let pr = Rect::from_min_max(
            pos2(x - 120.0, r.center().y - 18.0),
            pos2(x, r.center().y + 18.0),
        );
        let resp = kit::hit(ui, pr, "hud.project");
        let p = ui.painter();
        p.rect_filled(
            pr,
            kit::r(6),
            if resp.hovered() {
                t::PANEL_RAISED
            } else {
                t::PANEL
            },
        );
        p.rect_stroke(pr, kit::r(6), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
        glyph(
            p,
            "open",
            pos2(pr.left() + 16.0, pr.center().y),
            14.0,
            t::MUTED,
        );
        text(
            p,
            pos2(pr.left() + 30.0, pr.center().y),
            Align2::LEFT_CENTER,
            "Project",
            kit::sans(12.0),
            t::INK_2,
        );
        glyph(
            p,
            "chevron",
            pos2(pr.right() - 14.0, pr.center().y),
            10.0,
            t::MUTED,
        );
        if resp.clicked() {
            toggle_info(st, "project");
        }
        if st.screen == Screen::Instrument {
            let sr = Rect::from_min_max(
                pos2(pr.left() - 232.0, pr.top()),
                pos2(pr.left() - 12.0, pr.bottom()),
            );
            tower_switch(ui, st, sr, false);
        }
        project_menu(ui, st, pos2(pr.right(), pr.bottom()), screen);
        account_cards(ui, st, pos2(r.right(), r.bottom()), screen);
    });
}

fn project_menu(ui: &egui::Ui, st: &mut State, anchor: egui::Pos2, bounds: Rect) {
    if st.info.as_deref() != Some("project") {
        return;
    }
    let p = ui.ctx().layer_painter(egui::LayerId::new(
        Order::Tooltip,
        egui::Id::new("screens.menu"),
    ));
    let w = 220.0;
    let r = Rect::from_min_size(
        pos2((anchor.x - w).max(bounds.left() + 8.0), anchor.y + 6.0),
        vec2(w, 132.0),
    );
    p.rect_filled(r, kit::r(8), t::PANEL_RAISED);
    p.rect_stroke(r, kit::r(8), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
    text(
        &p,
        pos2(r.left() + 14.0, r.top() + 12.0),
        Align2::LEFT_TOP,
        "untitled.drafthouse",
        kit::semi(12.5),
        t::INK,
    );
    text(
        &p,
        pos2(r.left() + 14.0, r.top() + 30.0),
        Align2::LEFT_TOP,
        "not saved",
        kit::sans(11.0),
        t::MUTED,
    );
    for (i, (g, id, label)) in [("open", "open", "Open…"), ("save", "save", "Save")]
        .into_iter()
        .enumerate()
    {
        let br = Rect::from_min_size(
            pos2(r.left() + 6.0, r.top() + 52.0 + i as f32 * 38.0),
            vec2(w - 12.0, 36.0),
        );
        let resp = ui.interact(br, egui::Id::new(("screens.menu", id)), Sense::click());
        if resp.hovered() {
            p.rect_filled(br, kit::r(6), t::PRIMARY_SOFT);
        }
        glyph(&p, g, pos2(br.left() + 18.0, br.center().y), 15.0, t::INK_2);
        text(
            &p,
            pos2(br.left() + 36.0, br.center().y),
            Align2::LEFT_CENTER,
            label,
            kit::sans(12.5),
            t::INK,
        );
        if resp.clicked() {
            st.info = None;
            shell_action(st, id);
        }
    }
}

fn account_cards(ui: &egui::Ui, st: &mut State, anchor: egui::Pos2, bounds: Rect) {
    let id = st.info.clone();
    let lines: &[&str] = match id.as_deref() {
        Some("lock") => &[
            "Real catalog - staff only",
            "DEMO runs the illustrative catalog.",
            "Sign in with Synergy to unlock",
            "vendor records.",
        ],
        Some("me") => &[
            data::STUB_USER,
            "Catalog: Synergy - not yet served.",
            "Tap Sign out in the menu to",
            "return to DEMO.",
        ],
        _ => return,
    };
    info_card(ui, st, id.as_deref().unwrap_or(""), anchor, lines, bounds);
    if id.as_deref() == Some("me") {
        // a sign-out row under the card
        let r = Rect::from_min_size(
            pos2(
                anchor.x - 244.0,
                anchor.y + 14.0 + 18.0 + lines.len() as f32 * 18.0 + 6.0,
            ),
            vec2(244.0, 40.0),
        );
        let r = Rect::from_min_max(pos2(r.left().max(bounds.left() + 8.0), r.top()), r.max);
        let p = ui.ctx().layer_painter(egui::LayerId::new(
            Order::Tooltip,
            egui::Id::new("screens.signout"),
        ));
        let resp = ui.interact(r, egui::Id::new("screens.signout"), Sense::click());
        p.rect_filled(
            r,
            kit::r(8),
            if resp.hovered() {
                t::PRIMARY_SOFT
            } else {
                t::PANEL_RAISED
            },
        );
        p.rect_stroke(r, kit::r(8), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
        text(
            &p,
            r.center(),
            Align2::CENTER_CENTER,
            "Sign out",
            kit::semi(12.0),
            t::INK,
        );
        if resp.clicked() {
            st.info = None;
            shell_action(st, "signout");
        }
    }
}

fn top_phone(ctx: &egui::Context, st: &mut State, screen: Rect, with_menu: bool) {
    let r = Rect::from_min_size(screen.min, vec2(screen.width(), TOP_PHONE));
    area(ctx, "screens.top", r, Order::Foreground, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(r, kit::r(0), t::RAIL_BG);
        p.line_segment(
            [r.left_bottom(), r.right_bottom()],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        let mut x = r.left() + 8.0;
        if with_menu {
            let mr = Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(44.0, 44.0));
            let resp = kit::hit(ui, mr, "top.menu");
            if resp.hovered() || st.drawer {
                p.rect_filled(mr.shrink(2.0), kit::r(8), t::PANEL_RAISED);
            }
            for k in 0..3 {
                let y = mr.center().y - 6.0 + k as f32 * 6.0;
                p.line_segment(
                    [pos2(mr.center().x - 9.0, y), pos2(mr.center().x + 9.0, y)],
                    Stroke::new(1.8, t::INK),
                );
            }
            if resp.clicked() {
                st.drawer = !st.drawer;
            }
            x = mr.right() + 4.0;
        } else {
            let m = Rect::from_min_size(pos2(x + 4.0, r.center().y - 15.0), vec2(30.0, 30.0));
            p.rect_filled(m, kit::r(7), t::PRIMARY_DEEP);
            text(
                &p,
                m.center(),
                Align2::CENTER_CENTER,
                "D",
                kit::semi(15.0),
                t::INK,
            );
            x = m.right() + 10.0;
        }
        let title = if st.screen == Screen::Instrument && st.crossflow {
            "Crossflow"
        } else {
            st.screen.name()
        };
        let step = current_step(st);
        text(
            &p,
            pos2(x, r.center().y - 9.0),
            Align2::LEFT_CENTER,
            &format!("{} · {}", step.number(), step.name().to_uppercase()),
            kit::semi(9.5),
            t::PRIMARY,
        );
        text(
            &p,
            pos2(x, r.center().y + 8.0),
            Align2::LEFT_CENTER,
            title,
            kit::semi(15.0),
            t::INK,
        );
        let right = account_cluster(ui, st, pos2(r.right() - 8.0, r.center().y), true);
        if st.screen == Screen::Instrument {
            let sr = Rect::from_min_max(
                pos2(right - 100.0, r.center().y - 22.0),
                pos2(right - 6.0, r.center().y + 22.0),
            );
            tower_switch(ui, st, sr, true);
        }
        account_cards(ui, st, pos2(r.right() - 8.0, r.bottom() - 6.0), screen);
    });
}

fn drawer(ctx: &egui::Context, st: &mut State, screen: Rect) {
    let scrim = Rect::from_min_max(pos2(screen.left(), screen.top() + TOP_PHONE), screen.max);
    area(ctx, "screens.drawer", scrim, Order::Foreground, |ui| {
        let p = ui.painter().clone();
        let w = 236.0f32.min(screen.width() - 64.0);
        let panel = Rect::from_min_size(scrim.min, vec2(w, scrim.height()));
        let rest = Rect::from_min_max(pos2(panel.right(), scrim.top()), scrim.max);
        p.rect_filled(rest, kit::r(0), t::with_alpha(Color32::BLACK, 120));
        if kit::hit(ui, rest, "drawer.scrim").clicked() {
            st.drawer = false;
        }
        p.rect_filled(panel, kit::r(0), t::RAIL_BG);
        p.line_segment(
            [panel.right_top(), panel.right_bottom()],
            Stroke::new(1.0, t::LINE),
        );
        let mut y = panel.top() + 8.0;
        let cur = current_step(st);
        for s in NAV_ORDER {
            if Some(s) == s.step().screens().first().copied() {
                let step = s.step();
                text(
                    &p,
                    pos2(panel.left() + 18.0, y + 9.0),
                    Align2::LEFT_CENTER,
                    &format!("{}  {}", step.number(), step.name().to_uppercase()),
                    kit::semi(10.0),
                    if step == cur { t::PRIMARY } else { t::MUTED },
                );
                y += 20.0;
            }
            let ir = Rect::from_min_size(pos2(panel.left() + 8.0, y), vec2(w - 16.0, 46.0));
            let resp = kit::hit(ui, ir, &format!("drawer.{}", s.slug()));
            let sel = st.screen == s;
            if sel {
                p.rect_filled(ir, kit::r(10), t::PRIMARY_SOFT);
            } else if resp.hovered() {
                p.rect_filled(ir, kit::r(10), t::PANEL_RAISED);
            }
            let g = if s == Screen::Instrument && st.crossflow {
                "crossflow"
            } else {
                s.slug()
            };
            glyph(
                &p,
                g,
                pos2(ir.left() + 26.0, ir.center().y),
                20.0,
                if sel { t::PRIMARY } else { t::INK_2 },
            );
            text(
                &p,
                pos2(ir.left() + 52.0, ir.center().y),
                Align2::LEFT_CENTER,
                s.name(),
                kit::semi(14.0),
                if sel { t::INK } else { t::INK_2 },
            );
            if resp.clicked() {
                st.go(s);
            }
            y += 47.0;
        }
        y += 8.0;
        p.line_segment(
            [pos2(panel.left() + 16.0, y), pos2(panel.right() - 16.0, y)],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        y += 12.0;
        let sr = Rect::from_min_size(pos2(panel.left() + 12.0, y), vec2(w - 24.0, 44.0));
        tower_switch(ui, st, sr, false);
        y += 56.0;
        for (g, id, label) in [
            ("open", "open", "Open project"),
            ("save", "save", "Save project"),
            (
                if st.motion { "motion" } else { "still" },
                "motion",
                if st.motion { "Motion on" } else { "Motion off" },
            ),
        ] {
            let ir = Rect::from_min_size(pos2(panel.left() + 8.0, y), vec2(w - 16.0, 46.0));
            let resp = kit::hit(ui, ir, &format!("drawer.{id}"));
            if resp.hovered() {
                p.rect_filled(ir, kit::r(10), t::PANEL_RAISED);
            }
            glyph(&p, g, pos2(ir.left() + 26.0, ir.center().y), 17.0, t::MUTED);
            text(
                &p,
                pos2(ir.left() + 52.0, ir.center().y),
                Align2::LEFT_CENTER,
                label,
                kit::sans(13.0),
                t::INK_2,
            );
            if resp.clicked() {
                shell_action(st, id);
            }
            y += 48.0;
        }
    });
}

fn dock(ctx: &egui::Context, st: &mut State, screen: Rect, phone: bool) {
    let n = Screen::ALL.len() as f32;
    let r = if phone {
        Rect::from_min_max(
            pos2(screen.left(), screen.bottom() - DOCK_PHONE_H),
            screen.max,
        )
    } else {
        let item_w = 76.0;
        let extra = 2.0 * 56.0 + 18.0;
        let w = item_w * n + extra + 20.0;
        Rect::from_center_size(
            pos2(screen.center().x, screen.bottom() - DOCK_DESK_RESERVE / 2.0),
            vec2(w, 64.0),
        )
    };
    area(ctx, "screens.dock", r, Order::Foreground, |ui| {
        let p = ui.painter().clone();
        if phone {
            p.rect_filled(r, kit::r(0), t::with_alpha(t::RAIL_BG, 246));
            p.line_segment(
                [r.left_top(), r.right_top()],
                Stroke::new(1.0, t::LINE_SOFT),
            );
            let w = r.width() / n;
            for (i, s) in NAV_ORDER.into_iter().enumerate() {
                let ir = Rect::from_min_size(
                    pos2(r.left() + i as f32 * w, r.top() + 2.0),
                    vec2(w, DOCK_PHONE_H - 4.0),
                );
                if i > 0 && NAV_ORDER[i - 1].step() != s.step() {
                    p.line_segment(
                        [
                            pos2(ir.left(), r.top() + 16.0),
                            pos2(ir.left(), r.bottom() - 16.0),
                        ],
                        Stroke::new(1.0, t::LINE),
                    );
                }
                nav_item(ui, st, ir, s, true, "dock");
            }
        } else {
            p.rect_filled(
                r.expand(1.0).translate(vec2(0.0, 3.0)),
                kit::r(20),
                t::with_alpha(Color32::BLACK, 70),
            );
            kit::glass(&p, r, 18);
            let mut x = r.left() + 10.0;
            for (i, s) in NAV_ORDER.into_iter().enumerate() {
                if i > 0 && NAV_ORDER[i - 1].step() != s.step() {
                    p.line_segment(
                        [pos2(x, r.top() + 16.0), pos2(x, r.bottom() - 16.0)],
                        Stroke::new(1.0, t::LINE_SOFT),
                    );
                }
                let ir = Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(76.0, 56.0));
                nav_item(ui, st, ir, s, true, "dock");
                x += 76.0;
            }
            x += 8.0;
            p.line_segment(
                [pos2(x, r.top() + 14.0), pos2(x, r.bottom() - 14.0)],
                Stroke::new(1.0, t::LINE),
            );
            x += 10.0;
            for (g, id) in [
                ("save", "save"),
                (if st.motion { "motion" } else { "still" }, "motion"),
            ] {
                let br = Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(56.0, 56.0));
                let resp = kit::hit(ui, br, &format!("dock.{id}"));
                if resp.hovered() {
                    p.rect_filled(br.shrink(4.0), kit::r(10), t::PANEL_RAISED);
                }
                glyph(
                    &p,
                    g,
                    br.center(),
                    18.0,
                    if resp.hovered() { t::INK } else { t::MUTED },
                );
                if resp.clicked() {
                    shell_action(st, id);
                }
                x += 56.0;
            }
        }
    });
}

// ======================================================================================= answer

fn current_step(st: &State) -> Step {
    st.screen.step()
}

/// The answer for the crossflow section: the selection's best crossflow candidate (what the section draws).
fn xf_answer(d: &EngineInput, c: &data::Cand, limits: (Option<f64>, Option<f64>, f64)) -> Answer {
    Answer {
        cold_c: c.cold_c,
        target_c: d.duty.target_cold_water_c,
        margin_k: c.margin_c,
        pass: c.margin_c >= limits.2,
        approach_k: c.cold_c - d.duty.wet_bulb_c,
        fan_kw: c.power_kw,
        drift_ppm: Some(c.drift_ppm),
        drift_limit_ppm: limits.0,
        makeup_m3_h: c.makeup_kg_s / data::density(d) * 3600.0,
        refused: None,
        subject: format!(
            "best crossflow: {} · {} at {} % speed",
            c.tower_id,
            c.fan_id,
            t::num::pct(c.speed * 100.0)
        ),
        origin: ORIGIN_SELECTION,
        ..Default::default()
    }
}

/// One answer figure: (label, value, unit, colour, source, origin). Issue #137: `origin` is the
/// figure's calculation card key - the worked step's own label - so a tap on the figure's mark opens
/// that card; a figure the run has no step for keeps a plain-words origin.
type Fig = (
    &'static str,
    String,
    &'static str,
    Color32,
    kit::Src,
    String,
);

/// Issue #137: the answer's figures, every number in the house style (`t::num`).
fn answer_figs(a: &Answer) -> Vec<Fig> {
    use t::num;
    let o = a.origin;
    let verdict = if a.pass { t::OK } else { t::DANGER };
    // the calculation card a figure opens: the run's step of that label, when the run carries one
    let card = |label: &str, fallback: String| -> String {
        if a.steps.iter().any(|s| s.label == label) {
            format!("{CALC_KEY}{label}")
        } else {
            fallback
        }
    };
    let mut v: Vec<Fig> = vec![
        (
            "cold water",
            num::temp(a.cold_c),
            "°C",
            t::INK,
            kit::Src::Calc,
            card("Cold water", format!("cold water · {o}")),
        ),
        (
            "target",
            num::temp(a.target_c),
            "°C",
            t::INK_2,
            kit::Src::Catalog,
            "target cold water · the duty".into(),
        ),
        (
            "margin",
            num::kelvin_signed(a.margin_k),
            "K",
            verdict,
            kit::Src::Calc,
            "target − cold water".into(),
        ),
        (
            "approach",
            num::kelvin(a.approach_k),
            "K",
            t::INK,
            kit::Src::Calc,
            card("Approach", format!("cold water − wet bulb · {o}")),
        ),
    ];
    if let Some(r) = a.range_k {
        v.push((
            "range",
            num::kelvin(r),
            "K",
            t::INK,
            kit::Src::Calc,
            card("Range", format!("hot water − cold water · {o}")),
        ));
    }
    if let Some(q) = a.heat_kw {
        v.push((
            "heat load",
            num::power(q),
            "kW",
            t::INK,
            kit::Src::Calc,
            card("Heat load", format!("heat load · {o}")),
        ));
    }
    v.push((
        "fan power",
        num::power(a.fan_kw),
        "kW",
        t::INK,
        kit::Src::Calc,
        card(
            "Fan power",
            format!("fan power at the operating point · {o}"),
        ),
    ));
    if let Some(k) = a.kavl {
        v.push((
            "KaV/L",
            num::kavl(k),
            "",
            t::INK,
            kit::Src::Calc,
            card("KaV/L", format!("transfer supplied · {o}")),
        ));
    }
    if let Some(dp) = a.drift_ppm {
        v.push((
            "drift",
            num::sig(dp, 3),
            "ppm",
            match a.drift_limit_ppm {
                Some(l) if dp > l => t::DANGER,
                _ => t::INK,
            },
            kit::Src::Calc,
            card(
                "Drift",
                "drift · the eliminator's curve at this air speed".into(),
            ),
        ));
    }
    v.push((
        "make-up",
        num::flow(a.makeup_m3_h),
        "m³/h",
        t::INK,
        kit::Src::Calc,
        card("Make-up", format!("evaporation + blowdown + drift · {o}")),
    ));
    v
}

/// Issue #137: a source mark's origin that names a calculation card starts with this key; the rest is
/// the worked step's label. Any other origin is a one-line source note.
const CALC_KEY: &str = "calc:";

/// Issue #137: the calculation card's short command names (`calc:range`, `?act=answer:open;calc:fan`)
/// -> the worked step's label. The card itself is the step the run returned; this only picks one.
fn calc_label(key: &str) -> Option<&'static str> {
    Some(match key {
        "flow" | "water" => "Water flow",
        "cold" => "Cold water",
        "range" => "Range",
        "approach" => "Approach",
        "heat" => "Heat load",
        "air" => "Air flow through the fill",
        "kavl" => "KaV/L",
        "capability" => "Capability",
        "pressure" => "Air-side pressure",
        "fan" => "Fan power",
        "evaporation" => "Evaporation",
        "drift" => "Drift",
        "blowdown" => "Blowdown",
        "makeup" => "Make-up",
        "capacity" => "Capacity",
        _ => return None,
    })
}

fn answer_bar(ctx: &egui::Context, st: &mut State, screen: Rect, phone: bool) {
    let Some(a) = st.answer.clone() else {
        return;
    };
    let r = answer_rect(screen, phone, st.nav);
    if phone {
        answer_phone(ctx, st, &a, r, screen);
        return;
    }
    area(ctx, "screens.answer", r, Order::Foreground, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(r, kit::r(0), t::RAIL_BG);
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t::LINE));
        let mut x = r.left() + 24.0;
        // "kind: what" -> the kind joins the ANSWER caption, the what gets the line (round-2 QA: the
        // whole subject on one line ellipsized away the speed that identifies the run)
        let (kind, what) = a
            .subject
            .split_once(": ")
            .unwrap_or(("", a.subject.as_str()));
        let e = text(
            &p,
            pos2(x, r.top() + 10.0),
            Align2::LEFT_TOP,
            &if kind.is_empty() {
                "ANSWER".to_string()
            } else {
                format!("ANSWER · {}", kind.to_uppercase())
            },
            kit::semi(10.0),
            t::MUTED,
        );
        kit::text_fit(
            &p,
            pos2(x, e.bottom() + 4.0),
            Align2::LEFT_TOP,
            what,
            kit::sans(11.5),
            t::INK_2,
            196.0,
        );
        x += 214.0;
        if let Some(why) = &a.refused {
            let c = kit::chip_sized(
                &p,
                pos2(x, r.center().y),
                Align2::LEFT_CENTER,
                "NO ANSWER",
                t::DANGER,
                t::DANGER_SOFT,
                t::DANGER,
                12.0,
            );
            kit::text_fit(
                &p,
                pos2(c.right() + 12.0, r.center().y),
                Align2::LEFT_CENTER,
                why,
                kit::sans(12.0),
                t::INK_2,
                r.right() - c.right() - 40.0,
            );
            return;
        }
        let (lab, fg, bg) = if a.pass {
            ("PASS", t::OK, t::OK_SOFT)
        } else {
            ("FAIL", t::DANGER, t::DANGER_SOFT)
        };
        let c = kit::chip_sized(
            &p,
            pos2(x, r.center().y),
            Align2::LEFT_CENTER,
            lab,
            fg,
            bg,
            fg,
            14.0,
        );
        x = c.right() + 22.0;
        let mut figs = answer_figs(&a);
        let room = r.right() - 16.0 - x;
        // issue #137: the desktop bar keeps its figure size; when the row cannot give every figure
        // 112 px, the three the phone card adds (range, heat load, KaV/L) stay in the Report and the
        // phone card rather than shrinking the rest
        if room / figs.len() as f32 > 0.0 && room / (figs.len() as f32) < 112.0 {
            figs.retain(|f| !matches!(f.0, "range" | "heat load" | "KaV/L"));
        }
        let step = (room / figs.len() as f32).min(150.0);
        for (l, v, u, col, src, origin) in figs.iter() {
            kit::metric_src(
                &p,
                pos2(x, r.top() + 11.0),
                l,
                v,
                u,
                17.0,
                *col,
                *src,
                origin,
            );
            x += step;
        }
    });
}

/// Phone: one row (verdict, cold water, margin); tap opens the full card above it.
/// The phone's answer sheet: the card that opens above the one-row answer bar. It covers the screen's
/// own control panel (Water's cycles, Rate's readings), never the screen's drawing; a calculation card
/// opened from one of its figures takes the same rect (see [`src_detail`]).
fn phone_sheet(r: Rect, figs: usize) -> Rect {
    let rows = figs.div_ceil(2) as f32;
    let h = 52.0 + rows * 50.0;
    Rect::from_min_max(
        pos2(r.left() + 8.0, r.top() - h - 6.0),
        pos2(r.right() - 8.0, r.top() - 6.0),
    )
}

fn answer_phone(ctx: &egui::Context, st: &mut State, a: &Answer, r: Rect, screen: Rect) {
    let figs = answer_figs(a);
    if st.answer_open && a.refused.is_none() {
        let card = phone_sheet(r, figs.len());
        area(ctx, "screens.answer.card", card, Order::Foreground, |ui| {
            let p = ui.painter().clone();
            p.rect_filled(
                card.expand(1.0).translate(vec2(0.0, 3.0)),
                kit::r(14),
                t::with_alpha(Color32::BLACK, 90),
            );
            // issue #137: opaque - the sheet floats over the screen's own controls
            kit::sheet(&p, card, 12);
            let e = text(
                &p,
                pos2(card.left() + 14.0, card.top() + 12.0),
                Align2::LEFT_TOP,
                "ANSWER",
                kit::semi(10.0),
                t::MUTED,
            );
            kit::text_fit(
                &p,
                pos2(card.left() + 14.0, e.bottom() + 3.0),
                Align2::LEFT_TOP,
                &a.subject,
                kit::sans(11.5),
                t::INK_2,
                card.width() - 28.0,
            );
            let cw = (card.width() - 28.0) / 2.0;
            for (i, (l, v, u, col, src, origin)) in figs.iter().enumerate() {
                kit::metric_src(
                    &p,
                    pos2(
                        card.left() + 14.0 + cw * (i % 2) as f32,
                        card.top() + 52.0 + 50.0 * (i / 2) as f32,
                    ),
                    l,
                    v,
                    u,
                    17.0,
                    *col,
                    *src,
                    origin,
                );
            }
        });
    }
    area(ctx, "screens.answer", r, Order::Foreground, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(r, kit::r(0), t::RAIL_BG);
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t::LINE));
        let resp = kit::hit(ui, r, "answer.row");
        let cy = r.center().y;
        let mut x = r.left() + 12.0;
        if let Some(why) = &a.refused {
            let c = kit::chip_sized(
                &p,
                pos2(x, cy),
                Align2::LEFT_CENTER,
                "NO ANSWER",
                t::DANGER,
                t::DANGER_SOFT,
                t::DANGER,
                11.5,
            );
            kit::text_fit(
                &p,
                pos2(c.right() + 10.0, cy),
                Align2::LEFT_CENTER,
                why,
                kit::sans(11.5),
                t::INK_2,
                r.right() - c.right() - 20.0,
            );
            return;
        }
        let (lab, fg, bg) = if a.pass {
            ("PASS", t::OK, t::OK_SOFT)
        } else {
            ("FAIL", t::DANGER, t::DANGER_SOFT)
        };
        let c = kit::chip_sized(&p, pos2(x, cy), Align2::LEFT_CENTER, lab, fg, bg, fg, 13.0);
        x = c.right() + 12.0;
        let v = text(
            &p,
            pos2(x, cy),
            Align2::LEFT_CENTER,
            &t::num::temp(a.cold_c),
            kit::num(19.0),
            t::INK,
        );
        let u = text(
            &p,
            pos2(v.right() + 3.0, cy + 1.0),
            Align2::LEFT_CENTER,
            "°C",
            kit::sans(11.0),
            t::INK_2,
        );
        let m1 = kit::src_mark(&p, pos2(u.right() + 4.0, cy), kit::Src::Calc, &figs[0].5);
        let mg = text(
            &p,
            pos2(m1.right() + 12.0, cy),
            Align2::LEFT_CENTER,
            &format!("{} K", t::num::kelvin_signed(a.margin_k)),
            kit::num(13.0),
            fg,
        );
        let m2 = kit::src_mark(&p, pos2(mg.right() + 4.0, cy), kit::Src::Calc, &figs[2].5);
        let _ = m2;
        let more = if st.answer_open { "less" } else { "more" };
        let mr = text(
            &p,
            pos2(r.right() - 30.0, cy),
            Align2::RIGHT_CENTER,
            more,
            kit::semi(11.5),
            t::PRIMARY,
        );
        kit::chevron(&p, pos2(mr.right() + 12.0, cy), 5.0, t::PRIMARY);
        if resp.clicked() {
            st.answer_open = !st.answer_open;
        }
        let _ = screen;
    });
}

/// Tap a source mark: a small card with its class and where the number comes from. Tap elsewhere closes.
fn src_detail(
    ctx: &egui::Context,
    st: &mut State,
    marks: &[(Rect, kit::Src, String)],
    screen: Rect,
    phone: bool,
) {
    if let Some(n) = st.src_pending {
        if let Some((r, s, o)) = marks.get(n) {
            st.src_open = Some((*s, o.clone(), r.center()));
            st.src_pending = None;
        } else if !marks.is_empty() {
            st.src_pending = None;
        }
    }
    if let Some(label) = st.calc_pending {
        let key = format!("{CALC_KEY}{label}");
        if let Some((r, s, o)) = marks.iter().find(|m| m.2 == key) {
            st.src_open = Some((*s, o.clone(), r.center()));
            st.calc_pending = None;
        } else if !marks.is_empty() {
            st.calc_pending = None;
        }
    }
    // the nearest mark within reach: 22 px on a phone (a 44 px target), 12 px with a pointer
    let reach = if phone { 22.0 } else { 12.0 };
    let nearest = |pos: egui::Pos2| {
        marks
            .iter()
            .map(|m| (m.0.center().distance(pos), m))
            .filter(|(d, _)| *d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, m)| m)
    };
    let (clicked, pos, hover) = ctx.input(|i| {
        (
            i.pointer.primary_clicked(),
            i.pointer.interact_pos(),
            i.pointer.hover_pos(),
        )
    });
    if let Some(h) = hover {
        if nearest(h).is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    if clicked {
        if let Some(pos) = pos.filter(|p| st.calc_rect.is_some_and(|r| r.contains(*p))) {
            // a tap on the open calculation card: back to where it was opened from
            let _ = pos;
            st.src_open = None;
        } else if let Some(pos) = pos {
            match nearest(pos) {
                Some((r, s, o)) => {
                    let same = st.src_open.as_ref().map(|x| &x.1) == Some(o);
                    st.src_open = if same {
                        None
                    } else {
                        Some((*s, o.clone(), r.center()))
                    };
                }
                None => st.src_open = None,
            }
        }
    }
    st.calc_rect = None;
    let Some((src, origin, at)) = st.src_open.clone() else {
        return;
    };
    let p = ctx.layer_painter(egui::LayerId::new(
        Order::Tooltip,
        egui::Id::new("screens.src"),
    ));
    if let Some(label) = origin.strip_prefix(CALC_KEY) {
        let answer = st.answer.as_ref();
        let step = answer.and_then(|a| a.steps.iter().find(|s| s.label == label).cloned());
        if let Some(step) = step {
            // issue #137 (conductor review): the card never floats over a screen's drawing. On a phone it
            // takes the answer sheet's own place - a drill-in that covers the sheet (the screen's control
            // panel lies under both); on a desktop it sits in the right-hand column every screen keeps for
            // its panel, above the answer strip.
            let bar = answer_rect(screen, phone, st.nav);
            let (slot, cover) = if phone {
                let figs = answer.map(|a| answer_figs(a).len()).unwrap_or(0);
                (phone_sheet(bar, figs), st.answer_open)
            } else if let Some(side) = st.side_panel {
                // the screen's docked panel, exactly its column: the drawing beside it stays whole
                (side, false)
            } else {
                // no docked panel (the instrument, the report): the content's right edge
                let w = 340f32.min(bar.width() - 16.0);
                (
                    Rect::from_min_max(
                        pos2(bar.right() - 8.0 - w, screen.top() + 8.0),
                        pos2(bar.right() - 8.0, bar.top() - 8.0),
                    ),
                    false,
                )
            };
            let r = calc_card(&p, &step, slot, cover);
            // the card's own surface takes the tap, so nothing under it is pressed through it
            area(ctx, "screens.calc", r, Order::Tooltip, |ui| {
                let _ = kit::hit(ui, r, "calc.back");
            });
            st.calc_rect = Some(r);
            return;
        }
    }
    let w = 300f32.min(screen.width() - 24.0);
    let h = 82.0;
    let mut r = Rect::from_min_size(pos2(at.x - 18.0, at.y + 14.0), vec2(w, h));
    if r.right() > screen.right() - 8.0 {
        r = r.translate(vec2(screen.right() - 8.0 - r.right(), 0.0));
    }
    if r.left() < screen.left() + 8.0 {
        r = r.translate(vec2(screen.left() + 8.0 - r.left(), 0.0));
    }
    if r.bottom() > screen.bottom() - 8.0 {
        r = r.translate(vec2(0.0, -(h + 28.0)));
    }
    p.rect_filled(r.expand(1.0), kit::r(9), t::with_alpha(Color32::BLACK, 90));
    p.rect_filled(r, kit::r(8), t::PANEL_RAISED);
    p.rect_stroke(
        r,
        kit::r(8),
        Stroke::new(1.0, src.color()),
        StrokeKind::Inside,
    );
    kit::src_glyph(&p, pos2(r.left() + 18.0, r.top() + 19.0), src, 9.0);
    text(
        &p,
        pos2(r.left() + 30.0, r.top() + 19.0),
        Align2::LEFT_CENTER,
        src.name(),
        kit::semi(12.5),
        src.color(),
    );
    kit::text_fit(
        &p,
        pos2(r.left() + 14.0, r.top() + 36.0),
        Align2::LEFT_TOP,
        origin.strip_prefix(CALC_KEY).unwrap_or(&origin),
        kit::sans(12.0),
        t::INK,
        w - 28.0,
    );
    text(
        &p,
        pos2(r.left() + 14.0, r.top() + 56.0),
        Align2::LEFT_TOP,
        src.meaning(),
        kit::sans(11.5),
        t::INK_2,
    );
}

/// Issue #137: **the calculation card** - one result, worked, in the order the issue sets: the result's
/// name, then its **inputs** (each named, with its value and unit), the **formula** in words, the formula
/// with the run's **numbers** substituted, and the **result**. It is the run's own
/// [`cockpit::engine::WorkedStep`] (inputs on its `why` line), the step the Report and the PDF print, so
/// the three cannot show different arithmetic.
///
/// It never floats over a screen's drawing (conductor review): `slot` is the place the caller gives it -
/// the phone's answer sheet (`cover`: the card replaces the sheet, a drill-in a tap on the card backs out
/// of) or the desktop's right-hand panel column. The card grows upward from the slot's bottom when its
/// content is taller; text is never shrunk to fit. Opaque, the theme's raised panel fill. Returns its rect.
fn calc_card(
    p: &egui::Painter,
    step: &cockpit::engine::WorkedStep,
    slot: Rect,
    cover: bool,
) -> Rect {
    let pad = 14.0;
    let cap_w = 74.0;
    let w = slot.width();
    let body_w = w - 2.0 * pad - cap_w;
    let inputs = step.why.as_str();
    let formula = step.formula.as_deref().unwrap_or("");
    let substitution = step.substitution.as_deref().unwrap_or("");
    let result = step
        .value
        .map(|v| t::num::with_unit(v, &step.unit))
        .unwrap_or_default();
    let i_font = kit::sans(12.5);
    let f_font = kit::sans(12.5);
    let s_font = kit::num(13.0);
    let r_font = kit::num(16.0);
    // one row per stage: caption on the left, the stage's text wrapped on the right
    let rows: [(&str, &str, egui::FontId, Color32); 4] = [
        ("INPUTS", inputs, i_font, t::INK_2),
        ("FORMULA", formula, f_font, kit::Src::Calc.color()),
        ("NUMBERS", substitution, s_font, t::INK),
        ("RESULT", result.as_str(), r_font, t::INK),
    ];
    let heights: Vec<f32> = rows
        .iter()
        .map(|(_, body, font, _)| {
            kit::wrap_height(p, &kit::bind_words(body), font.clone(), body_w).max(14.0)
        })
        .collect();
    let head = if cover { 16.0 } else { 0.0 } + 22.0 + 12.0;
    let rh = if step.reference.is_some() { 18.0 } else { 0.0 };
    let content = 12.0 + head + heights.iter().map(|h| h + 10.0).sum::<f32>() + rh + 4.0;
    let h = if cover {
        content.max(slot.height())
    } else {
        content
    };
    let r = Rect::from_min_max(pos2(slot.left(), slot.bottom() - h), slot.max);
    let src = kit::Src::Calc;
    p.rect_filled(
        r.expand(1.0).translate(vec2(0.0, 3.0)),
        kit::r(14),
        t::with_alpha(Color32::BLACK, 90),
    );
    p.rect_filled(r, kit::r(12), t::PANEL_RAISED);
    p.rect_stroke(
        r,
        kit::r(12),
        Stroke::new(1.0, src.color()),
        StrokeKind::Inside,
    );
    let mut y = r.top() + 12.0;
    if cover {
        // the sheet it replaced, named where the sheet's own caption was: a tap goes back to it (a drawn
        // chevron: the UI fonts have no `‹`)
        kit::chevron_left(p, pos2(r.left() + pad, y + 6.0), 8.0, t::MUTED);
        text(
            p,
            pos2(r.left() + pad + 9.0, y),
            Align2::LEFT_TOP,
            "ANSWER",
            kit::semi(10.0),
            t::MUTED,
        );
        y += 16.0;
    }
    // the result's name, with the calculated mark
    let y1 = y + 11.0;
    kit::src_glyph(p, pos2(r.left() + pad + 4.0, y1), src, 9.0);
    kit::text_fit(
        p,
        pos2(r.left() + pad + 16.0, y1),
        Align2::LEFT_CENTER,
        &step.label,
        kit::semi(13.5),
        t::INK,
        w - 2.0 * pad - 16.0,
    );
    y += 22.0 + 12.0;
    p.line_segment(
        [
            pos2(r.left() + pad, y - 6.0),
            pos2(r.right() - pad, y - 6.0),
        ],
        Stroke::new(1.0, t::LINE_SOFT),
    );
    for ((cap, body, font, col), bh) in rows.into_iter().zip(heights) {
        text(
            p,
            pos2(r.left() + pad, y + 2.0),
            Align2::LEFT_TOP,
            cap,
            kit::semi(10.0),
            t::MUTED,
        );
        kit::text_wrap_bound(p, pos2(r.left() + pad + cap_w, y), body, font, col, body_w);
        y += bh + 10.0;
    }
    if let Some(reference) = &step.reference {
        kit::text_fit(
            p,
            pos2(r.left() + pad + cap_w, y),
            Align2::LEFT_TOP,
            reference,
            kit::sans(11.0),
            t::MUTED,
            body_w,
        );
    }
    r
}

// ====================================================================================== publish

#[cfg(target_arch = "wasm32")]
fn publish(st: &State, screen: Rect, content: Rect) {
    let strings = kit::strings_take();
    // issue #117: the controls the frame drew, published the way the instrument publishes its own
    // (`data-hits`, `crate::app::HitMap`): a headless harness can aim a real pointer at a named
    // screen control - the comparison's export among them - instead of guessing coordinates.
    let hits = kit::hits_take();
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let set = |attr: &str, v: &str| {
        if let Some(el) = doc.get_element_by_id("viz-root") {
            if el.get_attribute(attr).as_deref() != Some(v) {
                let _ = el.set_attribute(attr, v);
            }
        }
    };
    let slug = if st.screen == Screen::Instrument && st.crossflow {
        "crossflow"
    } else {
        st.screen.slug()
    };
    set("data-screen", slug);
    set(
        "data-nav",
        if st.nav == Nav::Rail { "rail" } else { "dock" },
    );
    set(
        "data-account",
        if st.account == Account::Demo {
            "demo"
        } else {
            "staff"
        },
    );
    set(
        "data-tower-type",
        if st.crossflow {
            "crossflow"
        } else {
            "counterflow"
        },
    );
    set("data-motion", if st.motion { "on" } else { "off" });
    set("data-screen-busy", if st.busy { "1" } else { "0" });
    // issue #117: the controls this frame drew, by their ids - `{}` when the frame drew none.
    let mut hit_map = serde_json::Map::new();
    for (id, r) in hits {
        hit_map.insert(id, serde_json::json!(r));
    }
    set(
        "data-screen-hits",
        &serde_json::Value::Object(hit_map).to_string(),
    );
    // The instrument's HTML control bar belongs to the instrument: hidden while a new screen is open.
    if let Some(nav) = doc.get_element_by_id("viz-nav") {
        let want = if slug == "instrument" {
            ""
        } else {
            "display:none"
        };
        if nav.get_attribute("style").as_deref().unwrap_or("") != want {
            let _ = nav.set_attribute("style", want);
        }
    }
    if st.frame % 10 != 1 && !st.busy {
        // the inventory is cheap but not free: every 10th frame is plenty for the evidence reader
    }
    let el = match doc.get_element_by_id("mirror-screens") {
        Some(el) => el,
        None => {
            let Ok(el) = doc.create_element("pre") else {
                return;
            };
            el.set_id("mirror-screens");
            // Issue #137 item 5: this is the evidence channel (JSON for the capture harness and the
            // parity test, read by id), not text for a person. It used to be `sr-only`, which a screen
            // reader reads aloud - field names, `Engine::run`, raw floats. `hidden` + `aria-hidden`
            // keep it out of the accessibility tree; `textContent` still reads it, like
            // `#mirror-words` and `#mirror-clip`.
            el.set_attribute("hidden", "").ok();
            el.set_attribute("aria-hidden", "true").ok();
            if let Some(body) = doc.body() {
                let _ = body.append_child(&el);
            }
            el
        }
    };
    let json = serde_json::json!({
        "screen": slug,
        "nav": if st.nav == Nav::Rail { "rail" } else { "dock" },
        "account": if st.account == Account::Demo { "demo" } else { "staff" },
        "busy": st.busy,
        "viewport": [screen.width(), screen.height()],
        "content": [content.left(), content.top(), content.width(), content.height()],
        "engine_ms_last": st.cache.engine_ms_last,
        "step": st.screen.step().name(),
        "nav_order": NAV_ORDER.iter().map(|s| s.slug()).collect::<Vec<_>>(),
        "answer": st.answer.as_ref().map(|a| serde_json::json!({
            "subject": a.subject, "origin": a.origin, "refused": a.refused,
            "pass": a.pass, "cold_c": a.cold_c, "target_c": a.target_c, "margin_k": a.margin_k,
            "approach_k": a.approach_k, "fan_kw": a.fan_kw, "drift_ppm": a.drift_ppm,
            "makeup_m3_h": a.makeup_m3_h,
        })),
        "marks": { "calculated": st.mark_counts[0], "catalog": st.mark_counts[1], "illustrative": st.mark_counts[2] },
        "src_open": st.src_open.as_ref().map(|s| s.0.name()),
        "strings": strings,
    })
    .to_string();
    if el.text_content().as_deref() != Some(json.as_str()) {
        el.set_text_content(Some(&json));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish(_st: &State, _screen: Rect, _content: Rect) {
    let _ = kit::strings_take();
    let _ = kit::hits_take();
}
