//! The one `App` composition both entry points share (issue #71).
//!
//! The wasm entry ([`crate::bridge::viz_start`]) and the native binary (`src/bin/drafthouse.rs`)
//! both call [`assemble`]: same `DefaultPlugins` configuration, same [`crate::app::VizPlugin`],
//! same asset plugin. Only the *host* differs - the web page hands in a canvas window and the
//! binary a desktop window (or none, for the `--no-window` smoke) - so the instrument itself is
//! never forked for the native build.
//!
//! #82: this file also carries what the lag work is *measured* by and (below) what governs it -
//! [`EnginePolicy`], the counted [`InstrumentedEngine`] the engine is wrapped in, and the native
//! [`harness`] that runs a re-runnable before/after pass on this host's own GPU.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButtonInput, MouseWheel};
use bevy::input::touch::TouchInput;
use bevy::prelude::*;
use bevy::window::{CursorMoved, RequestRedraw, Window, WindowPlugin, WindowResized};
use bevy_egui::EguiPostUpdateSet;
use cockpit::engine::{Engine, EngineError, EngineInput, EngineOutput};

use crate::app::VizPlugin;
use crate::state::StartOptions;

/// #82: the run loop an interactive session runs under, named so the report and the docs quote the
/// same string the code does. This is what bevy's `WinitSettings::default()` (`game()`) *was*: a
/// focused window updating on every `AboutToWait` (a frame forever), an unfocused one at 1/60 s.
/// [`session_settings`] replaces it with the reactive family, and the report hashes this string so a
/// before/after pair cannot silently compare two different loops. Only the native harness reads it;
/// the lint is silenced on wasm so the page's build stays warning-free.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const SESSION_LOOP: &str =
    "reactive(3600 s): a still, loaded pose stops redrawing; an event wakes it";

/// #82: how long between real engine runs while an input keeps moving. The brief's family is
/// "≤10 Hz"; 100 ms is that family's own window, and the measured cost of one real run (317-477 ms
/// on the lane's M4, the issue's recorded before-run) is one to five windows by itself - the
/// window is a floor under the rate, not a promise the engine can always keep it.
pub const ENGINE_THROTTLE_WINDOW_MS: f64 = 100.0;

/// #82: how long an idle window must have seen no input before it may stop asking for redraws - the
/// settling window. Long enough that the frames right after a click (hover, focus, the click's own
/// repaint) still happen, short enough that a recorder waiting for `data-frames` to stand still sees
/// it inside a second.
pub const IDLE_BEFORE_SLEEP_MS: f64 = 400.0;

/// Build the app the host is about to run.
///
/// * `window`: `Some` for a real window (the page's canvas / the desktop window), `None` to build
///   the same app without one - the smoke path, which disables the winit plugin because there is
///   no event loop to create.
/// * `frames`: `Some(n)` asks the app to exit cleanly once `n` updates have run (the smoke's own
///   bound); `None` runs until the host's normal exit (the window closes).
pub fn assemble(options: StartOptions, window: Option<Window>, frames: Option<u32>) -> App {
    let windowless = window.is_none();
    let mut plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: window,
            ..default()
        })
        .set(bevy::log::LogPlugin {
            level: bevy::log::Level::WARN,
            ..default()
        })
        // The fixture ships as a plain file, so no `.meta` side-car is written for it; without this
        // the asset reader probes `<file>.meta` and logs a 404 on every load (harmless but wrong in
        // a console an evidence frame is read from). The path is the host's: the page's own
        // directory on wasm, the executable's own `assets/` on native ([`assets_root`]).
        .set(bevy::asset::AssetPlugin {
            meta_check: bevy::asset::AssetMetaCheck::Never,
            file_path: assets_root(),
            ..default()
        });
    if windowless {
        plugins = plugins.disable::<bevy::winit::WinitPlugin>();
    }

    let mut app = App::new();
    app.add_plugins(plugins);
    app.add_plugins(VizPlugin { options });
    // #82: the redraw policy and the engine policy, installed here because this file is the one
    // composition both entry points share (the fenced `app.rs`/`ui.rs`/`scene.rs` are another
    // lane's while this one runs). Ordering matters and is stated where each system is added:
    //   - `instrument_engine` wraps the engine the frame `poll_loading` selected, before the pass
    //     that will call it;
    //   - `keep_loading` keeps a quiet loop stepping until the load has actually finished;
    //   - `track_activity` records the last frame that saw input, and `sleep_when_static` drops the
    //     frame's redraw requests *after* the pass asked for them, which is the only place that
    //     knows the pose is still.
    app.insert_resource(EnginePolicy::default());
    app.insert_resource(Idle::default());
    app.insert_resource(Slept::default());
    app.add_systems(Update, keep_loading);
    app.add_systems(
        PostUpdate,
        instrument_engine.before(EguiPostUpdateSet::EndPass),
    );
    // `Last`, not after the pass: `bevy_egui`'s own `process_output_system` writes `RequestRedraw`
    // from the later `EguiPostUpdateSet::ProcessOutput` (it forwards the context's own repaint
    // request - `ui.rs` animates the flow map and the fan, so it asks on every frame), and a drop
    // that ran before it measured 54.7 fps in a held pose (perf/z-fixed.json, first run at
    // 05310e1). `Last` is the last word before the frame presents and the runner reads the queue.
    app.add_systems(Last, (track_activity, sleep_when_static).chain());
    match frames {
        // A bounded run is a smoke: its frame count must not depend on whether the window has focus.
        // Bevy's default winit settings update a focused window continuously and an unfocused one
        // only on events, so `--frames N` would take unbounded wall time behind another window.
        Some(frames) => {
            app.insert_resource(bevy::winit::WinitSettings::continuous());
            app.add_systems(Update, exit_after_frames(frames));
        }
        // #82: an interactive session is reactive instead. Events still update and draw the frame
        // they arrive in; a frame that has nothing to do draws nothing, and an idle window stops
        // repainting altogether once `sleep_when_static` says it is quiet. The wait is
        // an hour rather than `Duration::MAX` on purpose: bevy computes `None` for an infinite wait
        // and leaves winit's own default in place - a platform-dependent frame source this lane will
        // not rely on either way.
        None => {
            app.insert_resource(session_settings());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    harness::attach(&mut app);
    app
}

/// `--frames N`: raise `AppExit` once `N` updates have run, so a smoke can start the real app and
/// still stop by itself (the windowed form closes the window loop the same way a close click does).
fn exit_after_frames(frames: u32) -> impl FnMut(Local<u32>, MessageWriter<AppExit>) {
    move |mut tick: Local<u32>, mut exit: MessageWriter<AppExit>| {
        *tick += 1;
        if *tick >= frames {
            exit.write(AppExit::Success);
        }
    }
}

// ------------------------------------------------------------------------------- the engine policy

/// #82: what the counted wrapper knows about the engine. One per app, shared with the wrapper.
#[derive(Default)]
struct PolicyCounters {
    /// Times the instrument asked the engine for a run (the wrapper was called).
    calls: AtomicU64,
    /// Times the engine's own `run` actually ran.
    runs: AtomicU64,
    /// Nanoseconds spent inside those runs.
    run_nanos: AtomicU64,
    /// True when an input is waiting for its window to elapse (the throttle's own state).
    pending: AtomicBool,
}

/// #82: the engine policy resource - the counters the measurement reads. A resource, so any
/// read-out (or the harness) can take a snapshot without borrowing the engine slot.
#[derive(Resource, Default, Clone)]
pub struct EnginePolicy(Arc<PolicyCounters>);

/// A snapshot of [`EnginePolicy`]'s counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub calls: u64,
    pub runs: u64,
    pub run_nanos: u64,
    pub pending: bool,
}

impl EnginePolicy {
    pub fn counters(&self) -> Counters {
        Counters {
            calls: self.0.calls.load(Ordering::Relaxed),
            runs: self.0.runs.load(Ordering::Relaxed),
            run_nanos: self.0.run_nanos.load(Ordering::Relaxed),
            pending: self.0.pending.load(Ordering::Relaxed),
        }
    }
}

/// The throttle's cache: the last real run's input, its answer, when it *finished*, and whether the
/// last call is still owed a run of its own.
#[derive(Default)]
struct EngineCache {
    input: Option<EngineInput>,
    result: Option<Result<EngineOutput, EngineError>>,
    /// The end of the last real run on the caller's clock (ms). The window is measured from here,
    /// so a run that itself takes longer than the window does not simply re-run on the next frame.
    ran_at: f64,
    /// A call was answered from the cache while its input had not settled.
    pending: bool,
}

/// The engine the app runs against: the selected engine behind the policy. It answers every call
/// itself, and the *only* thing it changes about the engine's answers is how often they are asked
/// for: a real run happens when the input changed and the window since the last finished run has
/// elapsed, and an unchanged input is answered from the cache forever.
struct InstrumentedEngine {
    inner: Box<dyn Engine>,
    policy: EnginePolicy,
    window_ms: f64,
    cache: Mutex<EngineCache>,
}

impl InstrumentedEngine {
    /// The throttle, with the caller's clock handed in so it can be tested without waiting. A drag
    /// therefore re-runs at most once per window (the brief's "≤10 Hz" at a 100 ms window) and an
    /// unchanged input never re-runs at all - which is what a held pose is.
    ///
    /// The answer for an input inside its window is the previous run's output, deliberately: the
    /// engine's own read-outs are the last settled answer, not half of a new one. The
    /// `test` module pins the two properties the brief asks for - a bounded run rate and the
    /// settled input getting its own fresh run on release.
    pub fn run_at(&self, input: &EngineInput, now_ms: f64) -> Result<EngineOutput, EngineError> {
        let counters = &self.policy.0;
        counters.calls.fetch_add(1, Ordering::Relaxed);
        let mut cache = self
            .cache
            .lock()
            .expect("the engine cache is never poisoned");
        if cache.input.as_ref() == Some(input) {
            if let Some(result) = cache.result.clone() {
                return result;
            }
        }
        if cache.result.is_some() && now_ms - cache.ran_at < self.window_ms {
            cache.pending = true;
            counters.pending.store(true, Ordering::Relaxed);
            return cache
                .result
                .clone()
                .expect("checked above: a cached result exists");
        }
        // The app's own clock, not `std::time`: on wasm32-unknown-unknown `Instant::now()` panics
        // ("time not implemented on this platform") and takes the whole page down with it - the wasm
        // trap left the canvas on whatever frame was last painted, which is how it was measured in
        // the headless page (the lane's committed web probe, capture and all), not guessed.
        let started = crate::clock::now_ms();
        let out = self.inner.run(input);
        let run_ms = crate::clock::now_ms() - started;
        counters
            .run_nanos
            .fetch_add((run_ms * 1_000_000.0) as u64, Ordering::Relaxed);
        counters.runs.fetch_add(1, Ordering::Relaxed);
        cache.input = Some(input.clone());
        cache.result = Some(out.clone());
        cache.ran_at = now_ms + run_ms;
        cache.pending = false;
        counters.pending.store(false, Ordering::Relaxed);
        out
    }
}

impl Engine for InstrumentedEngine {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn run(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        self.run_at(input, crate::clock::now_ms())
    }

    fn fixture_catalog_mut(&mut self) -> Option<&mut cockpit::fixture_engine::FixtureFile> {
        // A catalog edit is a new engine in every way that matters: whatever is cached was
        // computed from the old one, and nothing is owed against it.
        if let Ok(mut cache) = self.cache.lock() {
            *cache = EngineCache::default();
            self.policy.0.pending.store(false, Ordering::Relaxed);
        }
        self.inner.fixture_catalog_mut()
    }
}

/// Wrap the selected engine in [`InstrumentedEngine`], once, the frame `poll_loading` put it in the
/// slot. Ordering: after the egui pass, so the wrap cannot race the pass that reads the slot, and
/// before the next frame's pass runs.
fn instrument_engine(
    mut slot: ResMut<crate::app::EngineSlot>,
    policy: Res<EnginePolicy>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    if let Some(inner) = slot.0.take() {
        slot.0 = Some(Box::new(InstrumentedEngine {
            inner,
            policy: policy.clone(),
            window_ms: ENGINE_THROTTLE_WINDOW_MS,
            cache: Mutex::default(),
        }));
        *done = true;
    }
}

// ------------------------------------------------------------------------------- the redraw policy

/// #82: when the app last saw input, on its own clock (ms). An idle window gets a moment to settle
/// after the last event before it is allowed to stop drawing (see [`may_sleep`]).
#[derive(Resource, Default)]
pub struct Idle(pub f64);

/// #82: what the policy did, as counters rather than just state, so a measurement can tell "the
/// pose is not still" from "the pose is still and the drops are not enough" - the difference
/// between a wrong condition and a wrong place to look.
#[derive(Resource, Default)]
pub struct Slept {
    /// Frames on which the policy considered the pose still.
    pub frames: u64,
    /// Redraw requests dropped across those frames (they may not all have been there to drop).
    pub dropped: u64,
}

/// #82: the settings an interactive session runs under: reactive, with the longest wait winit will
/// take. Every real event wakes the loop immediately, so the wait is a floor under the sleep rather
/// than a frame rate - and when it does elapse, it is one frame an hour.
pub fn session_settings() -> bevy::winit::WinitSettings {
    const IDLE_WAIT: Duration = Duration::from_secs(3600);
    bevy::winit::WinitSettings {
        focused_mode: bevy::winit::UpdateMode::reactive(IDLE_WAIT),
        unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(IDLE_WAIT),
    }
}

/// #82: whether a frame may be the last one for a while - the brief's "an idle window does not
/// repaint, on web AND desktop". Three conditions, and every one of them is the point: the load has
/// to be in (a loading pose cannot wake the loop from its own thread), an owed engine run has to be
/// allowed to happen (its answer is a frame's content), and the window has to have been quiet since
/// the last input for the settling window (a click, a key, a hover, and the frames they move).
fn may_sleep(loaded: bool, pending: bool, quiet_ms: f64) -> bool {
    loaded && !pending && quiet_ms >= IDLE_BEFORE_SLEEP_MS
}

/// #82: keep the loop stepping through the whole load: while the fixture is loading, and until a
/// frame with the scene in it has been drawn. The load completes on its own thread and `Load` only
/// flips when a frame runs; a reactive loop that has gone quiet would never notice - and once the
/// load is in, the frame that paints it is the one that must not be skipped.
fn keep_loading(load: Res<crate::app::Load>, mut redraws: MessageWriter<RequestRedraw>) {
    if keep_stepping(load.failure.is_some(), load.ready, crate::scene::frames()) {
        redraws.write(RequestRedraw);
    }
}

/// The predicate [`keep_loading`] applies, split out so the trap it closes can be tested without a
/// world. `scene::frames()` counts only frames past the point where the draft is visible, so a load
/// that completes while nothing is moving used to leave the page sitting on the loading card until
/// the first input event - measured in the headless page (the lane's committed web probe),
/// not guessed. A failed load is a state to show once, not a reason to spin.
fn keep_stepping(failed: bool, ready: bool, scene_frames: u32) -> bool {
    !failed && (!ready || scene_frames == 0)
}

/// #82: remember the last frame that saw input. Only *input* counts (the load flipping ready, the
/// engine answering, are not activity), which is why this is separate from `keep_loading`.
#[allow(clippy::too_many_arguments)]
fn track_activity(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: MessageReader<CursorMoved>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut wheel: MessageReader<MouseWheel>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut touch: MessageReader<TouchInput>,
    mut resized: MessageReader<WindowResized>,
    mut idle: ResMut<Idle>,
) {
    let active = mouse.get_just_pressed().next().is_some()
        || mouse.get_just_released().next().is_some()
        || keys.get_just_pressed().next().is_some()
        || keys.get_just_released().next().is_some()
        || cursor.read().next().is_some()
        || buttons.read().next().is_some()
        || wheel.read().next().is_some()
        || keyboard.read().next().is_some()
        || touch.read().next().is_some()
        || resized.read().next().is_some();
    if active {
        idle.0 = crate::clock::now_ms();
    }
}

/// #82: the last place in the frame that knows a still pose is still. The instrument's own pass asks
/// for a repaint unconditionally (the flow map and the fan are always animating, `ui.rs`), so the
/// requests are collected and then dropped here - after the pass has drawn, before the winit runner
/// reads them next. The frame in progress still presents; the next one comes when something actually
/// happens: an event, the load finishing, or the wait elapsing.
fn sleep_when_static(
    mut redraws: ResMut<Messages<RequestRedraw>>,
    load: Res<crate::app::Load>,
    policy: Res<EnginePolicy>,
    idle: Res<Idle>,
    mut slept: ResMut<Slept>,
    mut state: Local<bool>,
) {
    // A load in progress owns the loop - [`keep_loading`]'s requests are the only thing that can
    // finish it, and the frame that first paints the scene has to have happened before a pose can be
    // called still. Without this guard the drop starves the load on a page that is already quiet:
    // the headless page sat on its loading card with the fixture ready and zero frames drawn
    // (the lane's committed web probe, run against the tree before the guard).
    if keep_stepping(load.failure.is_some(), load.ready, crate::scene::frames()) {
        return;
    }
    let quiet_ms = crate::clock::now_ms() - idle.0;
    let can_sleep = may_sleep(load.ready, policy.counters().pending, quiet_ms);
    // One line when the answer changes, so a page or a window can be asked why it is (not) quiet
    // without a profiler: the inputs to the decision, on the transition.
    if *state != can_sleep {
        *state = can_sleep;
        bevy::log::warn!(
            "#82: loop {} - loaded={} pending_run={} quiet={:.0}ms",
            if can_sleep { "can park" } else { "stays awake" },
            load.ready,
            policy.counters().pending,
            quiet_ms
        );
    }
    if can_sleep {
        slept.frames += 1;
        slept.dropped += redraws.len() as u64;
        redraws.clear();
    }
}

// ------------------------------------------------------------------------------------- the harness

/// #82: the native measurement pass. Absent unless `DRAFTHOUSE_PERF` names a plan, so an ordinary
/// run pays nothing for it.
///
/// ```text
/// DRAFTHOUSE_PERF="out:<path>;label:<text>;phases:<name:seconds,...>"
/// ```
///
/// The phases (all optional, default `warmup:2,idle_motion:4,drag:6,settle:1.5,idle_frozen:6`) are
/// driven by the app itself, so the before and after runs are the same commands on the same
/// instrument:
///
/// * `warmup` - the instrument loads and settles.
/// * `idle_motion` - nothing moves the draft: the instrument asks for the *same* input every frame.
/// * `drag` - the harness drags the rpm slider's own value (`speed_ratio`), as the pointer does.
/// * `settle` - the drag stops; the last exact value has a window to land in.
/// * `idle_frozen` - the pose is held (`AnimClock::frozen`, what `?frozen=1` stages): a still
///   picture must stop redrawing.
///
/// One JSON report goes to `out` and to stdout; each phase also prints `PERF PHASE_START`/`PHASE_END`
/// lines with a unix-epoch stamp, so an external sampler can line CPU samples up with the phases.
#[cfg(not(target_arch = "wasm32"))]
mod harness {
    use std::path::Path;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use bevy::winit::EventLoopProxyWrapper;
    use serde::Serialize;

    use super::*;
    use crate::app::{AnimClock, Draft, FixtureText, Load};
    use crate::state::Run;

    /// The default plan: a short warm-up, then ~17.5 s of sampled phases.
    const DEFAULT_PHASES: &str = "warmup:2,idle_motion:4,drag:6,settle:1.5,idle_frozen:6";

    /// How long the frozen pose must hold before `idle_motion`'s number is trustworthy: any longer
    /// than the settle window, so the number can never be a leftover of the drag.
    pub(super) fn attach(app: &mut App) {
        let Ok(raw) = std::env::var("DRAFTHOUSE_PERF") else {
            return;
        };
        let spec = match Spec::parse(&raw) {
            Ok(spec) => spec,
            Err(why) => {
                eprintln!("DRAFTHOUSE_PERF rejected: {why}");
                std::process::exit(2);
            }
        };
        eprintln!(
            "PERF plan label={} out={} phases={}",
            spec.label,
            spec.out,
            spec.phases
                .iter()
                .map(|phase| format!("{}:{}", phase.name, phase.seconds))
                .collect::<Vec<_>>()
                .join(",")
        );
        app.insert_resource(Perf::new(spec));
        app.add_systems(Last, perf_tick);
    }

    struct Spec {
        out: String,
        label: String,
        phases: Vec<Phase>,
    }

    impl Spec {
        /// The wake schedule: when each phase's boundary is due, counted from the plan's start. The
        /// closing update must observe `elapsed >= expected`, so each nudge is late by a slack - an
        /// early nudge would find the phase a hair short and the app asleep again for a whole phase.
        fn wake_schedule(&self) -> Vec<Duration> {
            let mut at = 0.0;
            self.phases
                .iter()
                .map(|phase| {
                    at += phase.seconds + 0.1;
                    Duration::from_secs_f64(at)
                })
                .collect()
        }
    }

    struct Phase {
        kind: Kind,
        name: String,
        seconds: f64,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Warmup,
        IdleMotion,
        Drag,
        Settle,
        IdleFrozen,
    }

    impl Spec {
        fn parse(raw: &str) -> Result<Self, String> {
            let mut spec = Spec {
                out: "perf.json".to_string(),
                label: "run".to_string(),
                phases: parse_phases(DEFAULT_PHASES)?,
            };
            for part in raw.split(';') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                let (key, value) = part
                    .split_once('=')
                    .ok_or_else(|| format!("`{part}` is not `key=value`"))?;
                match key.trim() {
                    "out" => spec.out = value.trim().to_string(),
                    "label" => spec.label = value.trim().to_string(),
                    "phases" => spec.phases = parse_phases(value)?,
                    other => return Err(format!("unknown key `{other}`")),
                }
            }
            Ok(spec)
        }
    }

    fn parse_phases(raw: &str) -> Result<Vec<Phase>, String> {
        raw.split(',')
            .map(|entry| {
                let (name, seconds) = entry
                    .split_once(':')
                    .ok_or_else(|| format!("phase `{entry}` is not `name:seconds`"))?;
                let name = name.trim();
                let kind = match name {
                    "warmup" => Kind::Warmup,
                    "idle_motion" => Kind::IdleMotion,
                    "drag" => Kind::Drag,
                    "settle" => Kind::Settle,
                    "idle_frozen" => Kind::IdleFrozen,
                    other => return Err(format!("unknown phase `{other}`")),
                };
                let seconds: f64 = seconds
                    .trim()
                    .parse()
                    .map_err(|e| format!("phase `{entry}`: {e}"))?;
                if seconds <= 0.0 {
                    return Err(format!("phase `{entry}`: seconds must be positive"));
                }
                Ok(Phase {
                    kind,
                    name: name.to_string(),
                    seconds,
                })
            })
            .collect()
    }

    /// The report one phase contributes. All rates are measured over the phase's own wall time.
    #[derive(Serialize, Clone)]
    struct PhaseReport {
        name: String,
        seconds: f64,
        /// `scene::frames()` advanced during the phase - the same counter the page mirrors.
        frames: u32,
        fps: f64,
        samples: usize,
        frame_ms_p50: Option<f64>,
        frame_ms_p95: Option<f64>,
        engine_calls: u64,
        engine_runs: u64,
        engine_runs_per_s: f64,
        engine_run_ms_mean: Option<f64>,
        engine_ms_per_s: f64,
        frozen: bool,
        dragging: bool,
        /// Whether the redraw-drop's four conditions held at the phase's closing frame.
        sleep_ready: bool,
        /// Frames this phase on which the redraw queue was actually dropped.
        sleeps: u64,
        /// Redraw requests dropped across those frames.
        dropped: u64,
    }

    /// The post-release check deliverable 2 asks for, done live: the value on screen at the end of
    /// the drag must be a fresh `Engine::run` of the settled input, not a throttled leftover.
    #[derive(Serialize, Clone)]
    struct PostRelease {
        /// No leaf differed by more than `TOLERANCE` (relative).
        equal: bool,
        /// Every leaf was bit-identical.
        exact: bool,
        worst_rel: f64,
        first_mismatch: Option<String>,
        engine: String,
        checked_at_ms: f64,
    }

    const TOLERANCE: f64 = 1e-9;

    #[derive(Serialize)]
    struct Report<'a> {
        label: &'a str,
        run_loop: &'a str,
        /// The `WinitSettings` read back out of the world the frame the plan started.
        settings: &'a str,
        profile: &'a str,
        target: &'a str,
        /// Milliseconds from process start to the fixture being loaded (the plan starts here).
        load_ready_ms: Option<f64>,
        unix_start_ms: u128,
        phases: &'a [PhaseReport],
        post_release: &'a Option<PostRelease>,
        note: &'a str,
    }

    #[derive(Resource)]
    struct Perf {
        spec: Spec,
        phase: usize,
        started: bool,
        phase_start_ms: f64,
        last_frame_ms: f64,
        samples: Vec<f64>,
        reports: Vec<PhaseReport>,
        start_frames: u32,
        start_slept: u64,
        start_slept_dropped: u64,
        start_counters: Counters,
        drag: Option<(f64, f64)>,
        post_release_done: bool,
        post_release: Option<PostRelease>,
        load_ready_ms: Option<f64>,
        unix_start_ms: u128,
    }

    impl Perf {
        fn new(spec: Spec) -> Self {
            Self {
                spec,
                phase: 0,
                started: false,
                phase_start_ms: 0.0,
                last_frame_ms: 0.0,
                samples: Vec::new(),
                reports: Vec::new(),
                start_frames: 0,
                start_slept: 0,
                start_slept_dropped: 0,
                start_counters: Counters::default(),
                drag: None,
                post_release_done: false,
                post_release: None,
                load_ready_ms: None,
                unix_start_ms: unix_ms(),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn perf_tick(
        mut perf: ResMut<Perf>,
        mut draft: Option<ResMut<Draft>>,
        mut clock: Option<ResMut<AnimClock>>,
        fixture: Option<Res<FixtureText>>,
        options: Res<StartOptions>,
        load: Res<Load>,
        run: Res<Run>,
        policy: Res<EnginePolicy>,
        idle: Res<Idle>,
        slept: Res<Slept>,
        settings: Option<Res<bevy::winit::WinitSettings>>,
        event_loop_proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
        mut exit: MessageWriter<AppExit>,
    ) {
        if let Some(failure) = &load.failure {
            eprintln!("PERF stopped: the fixture did not load ({failure})");
            exit.write(AppExit::error());
            return;
        }
        if !load.ready || perf.phase >= perf.spec.phases.len() {
            return;
        }
        let now = crate::clock::now_ms();
        let frames = crate::scene::frames();
        let counters = policy.counters();

        if !perf.started {
            perf.started = true;
            perf.load_ready_ms = Some(now);
            spawn_wakes(perf.spec.wake_schedule(), event_loop_proxy.as_deref());
            begin_phase(&mut perf, now, frames, counters, &slept, &mut clock);
            return;
        }

        let (kind, expected_ms) = {
            let phase = &perf.spec.phases[perf.phase];
            (phase.kind, phase.seconds * 1000.0)
        };
        let elapsed = now - perf.phase_start_ms;
        if elapsed >= expected_ms {
            let sleep_ready = may_sleep(load.ready, counters.pending, now - idle.0);
            finish_phase(
                &mut perf,
                now,
                frames,
                counters,
                &slept,
                sleep_ready,
                elapsed,
            );
            if perf.phase >= perf.spec.phases.len() {
                let settings = settings
                    .as_deref()
                    .map(|settings| {
                        format!(
                            "focused {:?}, unfocused {:?}",
                            settings.focused_mode, settings.unfocused_mode
                        )
                    })
                    .unwrap_or_else(|| "no winit plugin".to_string());
                write_and_exit(&perf, &settings, &mut exit);
                return;
            }
            begin_phase(&mut perf, now, frames, counters, &slept, &mut clock);
            return;
        }

        if perf.last_frame_ms > 0.0 {
            let delta = now - perf.last_frame_ms;
            perf.samples.push(delta);
        }
        perf.last_frame_ms = now;

        match kind {
            Kind::Drag => {
                if let Some(draft) = draft.as_mut() {
                    let (lo, hi) = *perf
                        .drag
                        .get_or_insert_with(|| draft.0.fan.allowed_speed_ratio.into());
                    if hi > lo {
                        // A 1 s triangle: down to the floor and back up, the way a hand drags.
                        let u = (elapsed / 1000.0).fract();
                        let frac = if u < 0.5 { u * 2.0 } else { 2.0 - u * 2.0 };
                        draft.0.speed_ratio = lo + (hi - lo) * frac;
                    }
                }
            }
            Kind::IdleFrozen => {
                // The settled value, once: +700 ms is past the last release run's own duration
                // (measured: ~320-480 ms), so what the read-outs hold is what a fresh run of the
                // settled draft would answer.
                let settled_due = !perf.post_release_done && elapsed >= 700.0;
                if settled_due {
                    perf.post_release_done = true;
                    let settled = draft.as_deref().map(|draft| draft.0.clone());
                    let checked = post_release_check(
                        settled.as_ref(),
                        fixture.as_deref(),
                        &run,
                        &options,
                        &mut exit,
                    );
                    perf.post_release = Some(checked);
                }
            }
            _ => {}
        }
    }

    /// The plan's wake-ups. An app that has gone quiet does not advance its own clock, so every phase
    /// boundary is nudged from a thread: bevy's `WinitUserEvent::WakeUp` sets `redraw_requested`,
    /// which is one update. Without this the *after* run would stop at the first frozen, quiet frame
    /// and never write its report - the very behavior being measured cannot also be the thing the
    /// measurement waits on.
    fn spawn_wakes(schedule: Vec<Duration>, proxy: Option<&EventLoopProxyWrapper>) {
        let Some(proxy) = proxy else {
            // No winit plugin (the `--no-window` smoke): there is no loop to nudge and no window to
            // draw, so a `DRAFTHOUSE_PERF` plan cannot run - say so rather than stall wordlessly.
            eprintln!("PERF: no winit event loop; the plan needs a window");
            return;
        };
        let proxy = (**proxy).clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            for boundary in schedule {
                let target = start + boundary;
                if let Some(rest) = target.checked_duration_since(Instant::now()) {
                    std::thread::sleep(rest);
                }
                if proxy
                    .send_event(bevy::winit::WinitUserEvent::WakeUp)
                    .is_err()
                {
                    return; // the loop is closed; nothing to wake
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_phase(
        perf: &mut Perf,
        now: f64,
        frames: u32,
        counters: Counters,
        slept: &Slept,
        clock: &mut Option<ResMut<AnimClock>>,
    ) {
        let phase = &perf.spec.phases[perf.phase];
        if let Some(clock) = clock.as_deref_mut() {
            clock.frozen = phase.kind == Kind::IdleFrozen;
        }
        perf.phase_start_ms = now;
        perf.last_frame_ms = now;
        perf.samples.clear();
        perf.start_frames = frames;
        perf.start_slept = slept.frames;
        perf.start_slept_dropped = slept.dropped;
        perf.start_counters = counters;
        if phase.kind == Kind::Drag {
            perf.drag = None;
        }
        eprintln!(
            "PERF PHASE_START name={} expected_s={} at_ms={:.1} at_unix_ms={} frames={} calls={} runs={} frozen={}",
            phase.name,
            phase.seconds,
            now,
            unix_ms(),
            frames,
            counters.calls,
            counters.runs,
            phase.kind == Kind::IdleFrozen
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_phase(
        perf: &mut Perf,
        now: f64,
        frames: u32,
        counters: Counters,
        slept: &Slept,
        sleep_ready: bool,
        elapsed_ms: f64,
    ) {
        let phase = &perf.spec.phases[perf.phase];
        let seconds = elapsed_ms / 1000.0;
        let mut sorted = perf.samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let frames_delta = frames.saturating_sub(perf.start_frames).saturating_sub(1);
        // The `- 1`: the frame that closes a phase is the harness observing the boundary, not a
        // redraw *during* the phase - counting it out is what makes a sleeping pose's count a clean
        // zero instead of one (and the same rule applies to every phase, so the two runs are
        // comparable).
        let calls = counters.calls.saturating_sub(perf.start_counters.calls);
        let runs = counters.runs.saturating_sub(perf.start_counters.runs);
        let nanos = counters
            .run_nanos
            .saturating_sub(perf.start_counters.run_nanos);
        let report = PhaseReport {
            name: phase.name.clone(),
            seconds,
            frames: frames_delta,
            fps: if seconds > 0.0 {
                f64::from(frames_delta) / seconds
            } else {
                0.0
            },
            samples: sorted.len(),
            frame_ms_p50: percentile(&sorted, 50.0),
            frame_ms_p95: percentile(&sorted, 95.0),
            engine_calls: calls,
            engine_runs: runs,
            engine_runs_per_s: if seconds > 0.0 {
                runs as f64 / seconds
            } else {
                0.0
            },
            engine_run_ms_mean: if runs > 0 {
                Some((nanos as f64 / runs as f64) / 1e6)
            } else {
                None
            },
            engine_ms_per_s: if seconds > 0.0 {
                (nanos as f64 / 1e6) / seconds
            } else {
                0.0
            },
            frozen: phase.kind == Kind::IdleFrozen,
            dragging: phase.kind == Kind::Drag,
            sleep_ready,
            sleeps: slept.frames.saturating_sub(perf.start_slept),
            dropped: slept.dropped.saturating_sub(perf.start_slept_dropped),
        };
        eprintln!(
            "PERF PHASE_END name={} at_ms={:.1} at_unix_ms={} frames={} fps={:.1} samples={} p50_ms={} p95_ms={} calls={} runs={} runs_per_s={:.1} engine_ms_per_s={:.2} sleeps={} dropped={}",
            report.name,
            now,
            unix_ms(),
            frames,
            report.fps,
            report.samples,
            fmt_opt(report.frame_ms_p50),
            fmt_opt(report.frame_ms_p95),
            report.engine_calls,
            report.engine_runs,
            report.engine_runs_per_s,
            report.engine_ms_per_s,
            report.sleeps,
            report.dropped
        );
        perf.reports.push(report);
        perf.phase += 1;
    }

    fn post_release_check(
        settled: Option<&EngineInput>,
        fixture: Option<&FixtureText>,
        run: &Run,
        options: &StartOptions,
        exit: &mut MessageWriter<AppExit>,
    ) -> PostRelease {
        let engine_name = run
            .output
            .as_ref()
            .map(|output| output.provenance.engine.clone())
            .unwrap_or_default();
        let checked_at_ms = crate::clock::now_ms();
        let Some(settled) = settled else {
            return PostRelease {
                equal: false,
                exact: false,
                worst_rel: f64::INFINITY,
                first_mismatch: Some("the draft".to_string()),
                engine: engine_name,
                checked_at_ms,
            };
        };
        let Some(displayed) = run.output.as_ref() else {
            eprintln!("PERF post-release: no output on screen to check");
            exit.write(AppExit::error());
            return PostRelease {
                equal: false,
                exact: false,
                worst_rel: f64::INFINITY,
                first_mismatch: Some("no output on screen".to_string()),
                engine: engine_name,
                checked_at_ms,
            };
        };
        let fresh = match crate::engine_select::build(
            fixture.map(|text| text.0.as_str()).unwrap_or_default(),
            options.engine.as_deref(),
        ) {
            Ok(engine) => engine.run(settled),
            Err(why) => {
                eprintln!("PERF post-release: the fresh engine could not be built ({why})");
                exit.write(AppExit::error());
                return PostRelease {
                    equal: false,
                    exact: false,
                    worst_rel: f64::INFINITY,
                    first_mismatch: Some(format!("fresh engine: {why}")),
                    engine: engine_name,
                    checked_at_ms,
                };
            }
        };
        let fresh = match fresh {
            Ok(output) => output,
            Err(why) => {
                eprintln!("PERF post-release: the fresh run failed ({why:?})");
                exit.write(AppExit::error());
                return PostRelease {
                    equal: false,
                    exact: false,
                    worst_rel: f64::INFINITY,
                    first_mismatch: Some(format!("fresh run: {why:?}")),
                    engine: engine_name,
                    checked_at_ms,
                };
            }
        };
        let (Ok(left), Ok(right)) = (
            serde_json::to_value(displayed),
            serde_json::to_value(&fresh),
        ) else {
            return PostRelease {
                equal: false,
                exact: false,
                worst_rel: f64::INFINITY,
                first_mismatch: Some("outputs are not serialisable".to_string()),
                engine: engine_name,
                checked_at_ms,
            };
        };
        let mut exact = true;
        let mut worst_rel: f64 = 0.0;
        let mut first_mismatch: Option<String> = None;
        walk(
            &left,
            &right,
            "",
            &mut exact,
            &mut worst_rel,
            &mut first_mismatch,
        );
        eprintln!(
            "PERF post-release engine={} exact={} equal_within_{TOLERANCE:e}={} worst_rel={:.3e} first_mismatch={:?}",
            engine_name,
            exact,
            first_mismatch.is_none(),
            worst_rel,
            first_mismatch
        );
        PostRelease {
            equal: first_mismatch.is_none(),
            exact,
            worst_rel,
            first_mismatch,
            engine: engine_name,
            checked_at_ms,
        }
    }

    /// Walk two serialized outputs, recording the first leaf that differs by more than
    /// [`TOLERANCE`] relative and the worst relative difference seen over all of them.
    fn walk(
        left: &serde_json::Value,
        right: &serde_json::Value,
        path: &str,
        exact: &mut bool,
        worst_rel: &mut f64,
        first: &mut Option<String>,
    ) {
        use serde_json::Value;
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let (a, b) = (
                    a.as_f64().unwrap_or(f64::NAN),
                    b.as_f64().unwrap_or(f64::NAN),
                );
                if a != b {
                    *exact = false;
                    let rel = if a == 0.0 {
                        if b == 0.0 {
                            0.0
                        } else {
                            f64::INFINITY
                        }
                    } else {
                        ((b - a) / a).abs()
                    };
                    if rel > *worst_rel {
                        *worst_rel = rel;
                    }
                    if rel > TOLERANCE && first.is_none() {
                        *first = Some(path.to_string());
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    *exact = false;
                    if first.is_none() {
                        *first = Some(format!("{path}.len"));
                    }
                    return;
                }
                for (index, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                    walk(a, b, &format!("{path}[{index}]"), exact, worst_rel, first);
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                for (key, a) in a {
                    let leaf = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    match b.get(key) {
                        Some(b) => walk(a, b, &leaf, exact, worst_rel, first),
                        None => {
                            *exact = false;
                            if first.is_none() {
                                *first = Some(leaf);
                            }
                        }
                    }
                }
                for key in b.keys() {
                    if !a.contains_key(key) {
                        *exact = false;
                        if first.is_none() {
                            *first = Some(path.to_string());
                        }
                    }
                }
            }
            (a, b) => {
                if a != b {
                    *exact = false;
                    if first.is_none() {
                        *first = Some(path.to_string());
                    }
                }
            }
        }
    }

    fn write_and_exit(perf: &Perf, settings: &str, exit: &mut MessageWriter<AppExit>) {
        let report = Report {
            label: &perf.spec.label,
            run_loop: SESSION_LOOP,
            settings,
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            target: "native",
            load_ready_ms: perf.load_ready_ms,
            unix_start_ms: perf.unix_start_ms,
            phases: &perf.reports,
            post_release: &perf.post_release,
            note: "frame time is the app's own update loop on this host's GPU; engine runs are the wrapper's count of Engine::run",
        };
        let json = match serde_json::to_string_pretty(&report) {
            Ok(json) => json,
            Err(why) => {
                eprintln!("PERF report could not be written: {why}");
                exit.write(AppExit::error());
                return;
            }
        };
        if let Some(dir) = Path::new(&perf.spec.out).parent() {
            if !dir.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(dir);
            }
        }
        match std::fs::write(&perf.spec.out, &json) {
            Ok(()) => eprintln!("PERF report written to {}", perf.spec.out),
            Err(why) => eprintln!(
                "PERF report could not be written to {}: {why}",
                perf.spec.out
            ),
        }
        println!("{json}");
        exit.write(AppExit::Success);
    }

    /// Nearest-rank percentile over an already sorted slice.
    fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
        if sorted.is_empty() {
            return None;
        }
        let rank = (p / 100.0) * (sorted.len() as f64 - 1.0);
        Some(sorted[rank.round() as usize])
    }

    fn fmt_opt(value: Option<f64>) -> String {
        value
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string())
    }

    fn unix_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }
}

/// Where the asset server reads from.
///
/// * **native** - `<the executable's own directory>/assets`. A desktop binary must find its assets
///   next to itself, never in whatever directory it was launched from; `build.rs` seeds that folder
///   beside every binary cargo builds, and a shipped app is the binary with `assets/` beside it.
/// * **wasm** - `assets`, relative to the page, exactly as before (the served plane serves it).
#[cfg(not(target_arch = "wasm32"))]
pub fn assets_root() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("assets")))
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_else(|| "assets".to_string())
}

/// Where the asset server reads from: the page's own directory, exactly as before issue #71.
#[cfg(target_arch = "wasm32")]
pub fn assets_root() -> String {
    "assets".to_string()
}

// ------------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::fixture_engine::FixtureEngine;

    /// The committed fixture - the engine the app actually ships with when no recording is named:
    /// the throttle's properties must hold for a real [`Engine`], not a mock.
    fn recorded() -> FixtureEngine {
        FixtureEngine::from_json(include_str!("../assets/fixture.json"))
            .expect("the committed fixture loads")
    }

    fn input(speed_ratio: f64) -> EngineInput {
        let mut input = recorded().default_input();
        input.speed_ratio = speed_ratio;
        input
    }

    fn throttle() -> (InstrumentedEngine, EnginePolicy) {
        let policy = EnginePolicy::default();
        let engine = InstrumentedEngine {
            inner: Box::new(recorded()),
            policy: policy.clone(),
            window_ms: ENGINE_THROTTLE_WINDOW_MS,
            cache: Mutex::default(),
        };
        (engine, policy)
    }

    /// The drag case: a frame every 16.7 ms with the slider moving on every one of them.
    #[test]
    fn a_drag_re_runs_at_most_once_per_window() {
        let (engine, policy) = throttle();
        let mut at = 0.0;
        let mut answers = Vec::new();
        for frame in 0..=60 {
            let speed_ratio = 0.4 + f64::from(frame) * 0.005;
            answers.push(
                engine
                    .run_at(&input(speed_ratio), at)
                    .expect("the recorded engine answers every input"),
            );
            at += 1000.0 / 60.0;
        }
        let counters = policy.counters();
        assert_eq!(
            counters.calls, 61,
            "every frame of the drag asked for an answer"
        );
        assert!(
            counters.runs <= 10,
            "one second of 60 fps dragging must be at most 10 runs, got {}",
            counters.runs
        );
        assert!(
            counters.runs >= 8,
            "the window must still re-run as it elapses, got {}",
            counters.runs
        );
        let held = answers.windows(2).filter(|pair| pair[0] == pair[1]).count();
        assert!(
            held >= 45,
            "a frame inside a window answers the last run: expected most of the drag to be held, \
             got {held}"
        );
    }

    /// The release case - the property that keeps the throttle from being a plain "answer from the
    /// cache": once the drag stops, the settled input must get a run of its own. That is what makes
    /// the value on screen after a release exact, and `perf` checks it on every run
    /// (`post_release.exact`).
    #[test]
    fn the_settled_input_gets_its_own_run_and_then_never_runs_again() {
        let (engine, policy) = throttle();
        let mut at = 0.0;
        for frame in 0..=20 {
            let speed_ratio = 0.4 + f64::from(frame) * 0.005;
            engine
                .run_at(&input(speed_ratio), at)
                .expect("the recorded engine answers every input");
            at += 1000.0 / 60.0;
        }
        let settled = input(0.52);
        // Force a real run of the last dragged input so the release lands just after one.
        let dragged = input(0.51);
        engine
            .run_at(&dragged, at + 500.0)
            .expect("the recorded engine answers every input");
        // The release lands inside that window: the answer stands, and the input is owed a run.
        let stale = engine
            .run_at(&settled, at + 501.0)
            .expect("the recorded engine answers every input");
        assert_eq!(
            stale,
            engine.run_at(&settled, at + 502.0).expect("answered"),
            "inside the window the answer is the last run's"
        );
        assert!(
            policy.counters().pending,
            "the settled input is owed a run of its own"
        );
        // Past the window it runs, and what it answers is a fresh run of the settled input.
        let fresh = engine
            .run_at(&settled, at + 700.0)
            .expect("the recorded engine answers every input");
        assert_eq!(
            fresh,
            recorded()
                .run(&settled)
                .expect("the recorded engine answers"),
            "the value left on screen after a release must be a fresh run of the settled input"
        );
        assert!(
            !policy.counters().pending,
            "nothing is owed once the settled input has run"
        );
        // And from here on the same input is a cache hit, however long it is held.
        let before = policy.counters().runs;
        engine
            .run_at(&settled, at + 900.0)
            .expect("the recorded engine answers every input");
        assert_eq!(
            policy.counters().runs,
            before,
            "an unchanged input must never re-run"
        );
    }

    /// The redraw policy's three conditions, as a table: a window may only stop repainting when it
    /// is loaded, owed nothing, and has been quiet since the last input - so the same rule covers a
    /// staged still pose, an idle desktop window and an idle page.
    #[test]
    fn only_a_loaded_settled_window_may_stop_redrawing() {
        assert!(
            !may_sleep(false, false, 10_000.0),
            "a loading window must keep stepping"
        );
        assert!(
            !may_sleep(true, true, 10_000.0),
            "an owed engine run must be allowed to happen"
        );
        assert!(
            !may_sleep(true, false, IDLE_BEFORE_SLEEP_MS - 1.0),
            "a window that has just moved must settle first"
        );
        assert!(
            may_sleep(true, false, IDLE_BEFORE_SLEEP_MS),
            "a loaded, quiet window stops repainting"
        );
        assert!(may_sleep(true, false, 10_000.0));
    }

    /// The report's `run_loop` line is a claim about the settings the app runs under. Pin it to
    /// them, or the before/after pair could compare two different loops under one name.
    #[test]
    fn the_session_loop_is_reactive_and_the_report_says_so() {
        assert!(SESSION_LOOP.starts_with("reactive(3600 s)"));
        match session_settings().focused_mode {
            bevy::winit::UpdateMode::Reactive {
                wait,
                react_to_window_events,
                ..
            } => {
                assert_eq!(wait, Duration::from_secs(3600));
                assert!(
                    react_to_window_events,
                    "a reactive session that ignores window events could never be woken by input"
                );
            }
            other => panic!("a focused session must be reactive, got {other:?}"),
        }
        assert!(matches!(
            session_settings().unfocused_mode,
            bevy::winit::UpdateMode::Reactive {
                react_to_device_events: false,
                ..
            }
        ));
    }

    /// A catalog edit is a new engine as far as the cache is concerned.
    #[test]
    fn editing_the_catalog_drops_the_cached_answer() {
        let (mut engine, policy) = throttle();
        let held = input(0.4);
        let first = engine
            .run_at(&held, 0.0)
            .expect("the recorded engine answers");
        assert_eq!(
            engine
                .run_at(&held, 10.0)
                .expect("the recorded engine answers"),
            first,
            "a live control asking every frame for the same input is a cache hit"
        );
        assert_eq!(policy.counters().runs, 1);
        assert!(
            engine.fixture_catalog_mut().is_some(),
            "the recorded engine exposes its catalog"
        );
        assert_eq!(
            engine
                .run_at(&held, 20.0)
                .expect("the recorded engine answers"),
            first,
            "the answer is unchanged, but it must have been computed again"
        );
        assert_eq!(
            policy.counters().runs,
            2,
            "the catalog edit must have dropped the cache"
        );
    }

    #[test]
    fn a_loading_page_keeps_drawing_until_the_scene_is_in() {
        assert!(
            keep_stepping(false, false, 0),
            "the fixture is still loading: step"
        );
        assert!(
            keep_stepping(false, true, 0),
            "the fixture is in but no frame has the scene in it yet: step, or the page sits on the \
             loading card until the first input"
        );
        assert!(
            !keep_stepping(false, true, 1),
            "the scene has drawn: the loop may park"
        );
        assert!(
            !keep_stepping(true, false, 0),
            "a failed load is a state to show once, not a reason to spin"
        );
    }
}
