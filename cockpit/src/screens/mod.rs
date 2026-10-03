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

use cockpit::engine::{Engine, EngineInput, EngineOutput};

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
    /// Where the numbers come from, for the marks: `Engine::run` or `run_selection`.
    pub origin: &'static str,
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
    /// Size: the tower-type filter (0 all, 1 counterflow, 2 crossflow). Compare: the baseline column.
    pub filter: usize,
    pub base: usize,
    pub info: Option<String>,
    pub toast: Option<(String, f64)>,
    pub url_read: bool,
    pub cache: data::Cache,
    pub busy: bool,
    pub frame: u64,
    /// Round 2: the last screen open in each step (a step with two screens reopens the one you left).
    pub step_last: [Screen; 5],
    pub answer: Option<Answer>,
    /// Phone: the answer row expanded into its card.
    pub answer_open: bool,
    /// Curves on a phone: 0 = cold water vs wet bulb, 1 = fan vs system.
    pub chart: usize,
    /// The source mark whose detail is open: (class, origin, anchor).
    pub src_open: Option<(kit::Src, String, egui::Pos2)>,
    /// Recorded requirements from the fixture (`provenance.fixed`), read once.
    pub limits: Option<(Option<f64>, Option<f64>, f64)>,
    /// `src:<n>` (evidence): open the n-th source mark painted on the next frame.
    pub src_pending: Option<usize>,
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
            filter: 0,
            base: 0,
            info: None,
            toast: None,
            url_read: false,
            cache: data::Cache::default(),
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
            chart: 0,
            src_open: None,
            limits: None,
            src_pending: None,
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
        "filter" => st.filter = num.map(|v| v as usize).unwrap_or(0).min(2),
        "base" => st.base = num.map(|v| v as usize).unwrap_or(0).min(2),
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

pub struct Ctx<'a> {
    pub draft: Option<&'a mut EngineInput>,
    pub engine: Option<&'a dyn Engine>,
    pub out: Option<&'a EngineOutput>,
    pub fixture_text: &'a str,
    pub reduced_motion: bool,
    pub t: f32,
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
        let screen = ctx.viewport_rect();
        let phone = screen.width() < 760.0;
        kit::strings_begin();

        let show_new = st.screen != Screen::Instrument || st.crossflow;
        let want = match st.screen {
            Screen::Instrument if st.crossflow => data::Want::Crossflow,
            Screen::Size => data::Want::Size,
            Screen::Rate => data::Want::Rate,
            Screen::Curves => data::Want::Curves,
            Screen::Compare => data::Want::Compare,
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
                        origin: "run_selection",
                        ..Default::default()
                    },
                }
            } else {
                let mut a = data::answer(d, c.out, limits);
                a.subject = format!(
                    "the draft: {} · {} · {:.2}×",
                    d.tower.id, d.fan.id, d.speed_ratio
                );
                a.origin = "Engine::run";
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
            "Loading the engine…",
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
    pub engine: Option<&'a dyn Engine>,
    /// Seconds of animation; 0 when motion is off (every motion helper then paints its rest pose).
    pub t: f32,
    pub motion: bool,
    pub phone: bool,
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
                kit::glass(p, r, 19);
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
        // bottom cluster: open, save, motion
        let mut y = r.bottom() - 30.0;
        for (g, id, label) in [
            (
                if st.motion { "motion" } else { "still" },
                "motion",
                if st.motion { "Motion" } else { "Still" },
            ),
            ("save", "save", "Save"),
            ("open", "open", "Open"),
        ] {
            let br =
                Rect::from_center_size(pos2(r.center().x, y - 14.0), vec2(RAIL_W - 12.0, 52.0));
            let resp = kit::hit(ui, br, &format!("rail.{id}"));
            if resp.hovered() {
                p.rect_filled(br.shrink(4.0), kit::r(10), t::PANEL_RAISED);
            }
            glyph(
                &p,
                g,
                pos2(br.center().x, br.center().y - 7.0),
                17.0,
                if resp.hovered() { t::INK } else { t::MUTED },
            );
            text(
                &p,
                pos2(br.center().x, br.center().y + 14.0),
                Align2::CENTER_CENTER,
                label,
                kit::sans(10.0),
                t::MUTED,
            );
            if resp.clicked() {
                shell_action(st, id);
            }
            y -= 56.0;
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
        st.cache.xf = None;
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
            "vendor records (synergy-apps#1808).",
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
            "best crossflow: {} · {} · {:.2}×",
            c.tower_id, c.fan_id, c.speed
        ),
        origin: "run_selection",
    }
}

/// One answer figure: (label, value, unit, colour, source, origin).
type Fig = (
    &'static str,
    String,
    &'static str,
    Color32,
    kit::Src,
    String,
);

fn answer_figs(a: &Answer) -> Vec<Fig> {
    let o = a.origin;
    let verdict = if a.pass { t::OK } else { t::DANGER };
    let mut v: Vec<Fig> = vec![
        (
            "cold water",
            format!("{:.2}", a.cold_c),
            "°C",
            t::INK,
            kit::Src::Calc,
            format!("{o} · cold water, solved"),
        ),
        (
            "target",
            format!("{:.1}", a.target_c),
            "°C",
            t::INK_2,
            kit::Src::Catalog,
            "duty · target cold water (recorded)".into(),
        ),
        (
            "margin",
            format!("{:+.2}", a.margin_k),
            "K",
            verdict,
            kit::Src::Calc,
            format!("target − cold water ({o})"),
        ),
        (
            "approach",
            format!("{:.2}", a.approach_k),
            "K",
            t::INK,
            kit::Src::Calc,
            format!("{o} · cold water − wet bulb"),
        ),
        (
            "fan power",
            format!("{:.1}", a.fan_kw),
            "kW",
            t::INK,
            kit::Src::Calc,
            format!("{o} · fan power at the operating point"),
        ),
    ];
    if let Some(dp) = a.drift_ppm {
        v.push((
            "drift",
            format!("{dp:.2}"),
            "ppm",
            match a.drift_limit_ppm {
                Some(l) if dp > l => t::DANGER,
                _ => t::INK,
            },
            kit::Src::Calc,
            if o == "run_selection" {
                "run_selection · airside drift".into()
            } else {
                "drift_performance_at_velocity · eliminator curve at the run".into()
            },
        ));
    }
    v.push((
        "water use",
        format!("{:.2}", a.makeup_m3_h),
        "m³/h",
        t::INK,
        kit::Src::Calc,
        format!("{o} · make-up (evaporation + blowdown + drift)"),
    ));
    v
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
        let figs = answer_figs(&a);
        let room = r.right() - 16.0 - x;
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
fn answer_phone(ctx: &egui::Context, st: &mut State, a: &Answer, r: Rect, screen: Rect) {
    let figs = answer_figs(a);
    if st.answer_open && a.refused.is_none() {
        let rows = figs.len().div_ceil(2) as f32;
        let h = 52.0 + rows * 50.0;
        let card = Rect::from_min_max(
            pos2(r.left() + 8.0, r.top() - h - 6.0),
            pos2(r.right() - 8.0, r.top() - 6.0),
        );
        area(ctx, "screens.answer.card", card, Order::Foreground, |ui| {
            let p = ui.painter().clone();
            p.rect_filled(
                card.expand(1.0).translate(vec2(0.0, 3.0)),
                kit::r(14),
                t::with_alpha(Color32::BLACK, 90),
            );
            kit::glass(&p, card, 12);
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
            &format!("{:.2}", a.cold_c),
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
            &format!("{:+.2} K", a.margin_k),
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
        if let Some(pos) = pos {
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
    let Some((src, origin, at)) = st.src_open.clone() else {
        return;
    };
    let p = ctx.layer_painter(egui::LayerId::new(
        Order::Tooltip,
        egui::Id::new("screens.src"),
    ));
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
        &origin,
        kit::mono(11.0),
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

// ====================================================================================== publish

#[cfg(target_arch = "wasm32")]
fn publish(st: &State, screen: Rect, content: Rect) {
    let strings = kit::strings_take();
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
            let _ = el.set_attribute("class", "sr-only");
            let _ = el.set_attribute("aria-live", "off");
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
}
