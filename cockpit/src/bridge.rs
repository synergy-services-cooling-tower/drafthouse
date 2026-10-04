//! wasm-bindgen entry point and the HTML mirror. **wasm builds only** (issue #71).
//!
//! The canvas is the interface; this module keeps a minimal HTML overlay in sync with live text (for a
//! screen reader and for a keyboard user) and publishes the `data-*` runtime markers the capture harness
//! reads back. Those markers are how an evidence frame proves what the app actually reached.
//!
//! The native binary has no DOM to mirror into and this module does not compile there; the mirror has
//! no native equivalent yet (the known gap in `docs/COCKPIT_SEAMS.md`, issue #71). The `App` itself is
//! built by [`crate::bootstrap::assemble`], which the native entry point calls too - the instrument is
//! not forked.

use bevy::prelude::*;
use bevy::window::{PresentMode, Window, WindowResolution};
use wasm_bindgen::prelude::*;

use crate::app::Draft;
use crate::app::{
    AnimClock, FirstFrame, FixtureText, HitMap, Load, SceneRect, StagedLog, COMMANDS,
};
use crate::clock::now_ms;
use crate::state::{self, Anchor, Catalog, Run, StartOptions, Visual};
#[cfg(feature = "three-d")]
use crate::three::ThreeInfo;

/// Called by index.html once the module is instantiated.
#[wasm_bindgen]
pub fn viz_start(canvas_selector: &str, options_json: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let options: StartOptions = serde_json::from_str(options_json)
        .map_err(|e| JsValue::from_str(&format!("options: {e}")))?;
    let window = Window {
        canvas: Some(canvas_selector.to_string()),
        fit_canvas_to_parent: true,
        prevent_default_event_handling: false,
        present_mode: PresentMode::AutoVsync,
        resolution: WindowResolution::new(1440, 900),
        ..default()
    };
    crate::bootstrap::assemble(options, Some(window), None).run();
    Ok(())
}

/// The page pushes commands here (keyboard, nav buttons, the capture harness).
///
/// Issue #74: the push also wakes a parked loop. #82's redraw policy parks an idle window (the
/// page included), and a command arriving while it waits would otherwise sit in [`COMMANDS`]
/// until the next *device* event - while the DOM bar sits above the canvas, a click on one of
/// its buttons never reaches winit at all. So the page's own channel owns its wake: the frame's
/// [`bevy::winit::WinitUserEvent::WakeUp`] is the same event `bootstrap`'s harness wakes with,
/// sent through the proxy the running loop provides ([`capture_wake_proxy`]).
#[wasm_bindgen]
pub fn viz_dispatch(command: &str) {
    COMMANDS.lock().unwrap().push(command.to_string());
    request_wake();
}

// The loop's wake handle, captured once the proxy exists. `thread_local` and not a `OnceLock`:
// winit's web proxy is main-thread-bound (its `Waker` holds `Rc`s), and this page runs it all
// on one thread anyway.
thread_local! {
    static WAKE_PROXY: std::cell::RefCell<Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>> =
        const { std::cell::RefCell::new(None) };
}

/// Capture the event-loop proxy the first update that has one. Registered at `Startup` on wasm
/// builds (`app.rs`), so every later `viz_dispatch` can wake the loop.
pub fn capture_wake_proxy(proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>) {
    if let Some(proxy) = proxy {
        let proxy = (**proxy).clone();
        WAKE_PROXY.with(|wake| {
            wake.replace(Some(proxy));
        });
    }
}

/// Ask a parked loop for one frame. Best-effort: before the loop exists (the page's first
/// dispatches) or after it closes, the queued command is still the record that matters.
fn request_wake() {
    WAKE_PROXY.with(|wake| {
        if let Some(proxy) = wake.borrow().as_ref() {
            let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    });
}

fn set_text(doc: &web_sys::Document, id: &str, text: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        if el.text_content().as_deref() != Some(text) {
            el.set_text_content(Some(text));
        }
    }
}

fn set_attr(doc: &web_sys::Document, id: &str, attr: &str, value: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        if el.get_attribute(attr).as_deref() != Some(value) {
            let _ = el.set_attribute(attr, value);
        }
    }
}

fn fmt2(v: f64) -> String {
    format!("{v:.2}")
}

/// Round 4's read-only resources for the mirror, bundled for the same reason [`crate::ui::VizAux`] is.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Round4Mirror<'w, 's> {
    pub form: Res<'w, crate::form::CustomForm>,
    pub fields: Res<'w, crate::state::FieldsRes>,
    pub card: Res<'w, crate::hover::HoverState>,
    pub duty: Res<'w, crate::duty_panel::DutyRes>,
    #[doc(hidden)]
    pub _phantom: std::marker::PhantomData<&'s ()>,
}

/// Round 5's read-only resources for the mirror: the clip probe (published to `#mirror-clip`) and the
/// layout counters (published as `data-*`). Issue #58 added the engine slot here rather than as a
/// parameter of its own: `mirror_system` is at the sixteen-parameter limit a system function has.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Round5Mirror<'w, 's> {
    pub clip: Res<'w, crate::clip::ClipProbe>,
    pub info: Res<'w, crate::clip::LayoutInfo>,
    pub engine: Res<'w, crate::app::EngineSlot>,
    /// Issue #74: the project session. Bundled here rather than as a parameter of its own, because
    /// `mirror_system` is at the sixteen-parameter limit a system may have.
    pub session: Res<'w, crate::files::FileSession>,
    /// Issue #91: the painted-text inventory, published to `#mirror-words`.
    pub words: Res<'w, crate::clip::ScreenText>,
    #[doc(hidden)]
    pub _phantom: std::marker::PhantomData<&'s ()>,
}

/// Mirrors the instrument into the HTML overlay every frame (writes only on change).
#[allow(clippy::too_many_arguments)]
pub fn mirror_system(
    vis: Res<Visual>,
    run: Res<Run>,
    anchor: Res<Anchor>,
    draft: Option<Res<Draft>>,
    catalog: Res<Catalog>,
    options: Res<StartOptions>,
    load: Res<Load>,
    staged: Res<StagedLog>,
    clock: Res<AnimClock>,
    fixture_text: Res<FixtureText>,
    hits: Res<HitMap>,
    #[cfg(feature = "three-d")] three: Res<ThreeInfo>,
    scene_rect: Res<crate::app::SceneRect>,
    charts: Res<crate::state::ChartStats>,
    round4: Round4Mirror,
    round5: Round5Mirror,
    mut first: ResMut<FirstFrame>,
) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let _ = &fixture_text;
    // Round 3: how much of the viewport the section's own column gets, so a frame can prove the parts rail
    // does not push the scene below the brief's ~60 % floor at 1440x900.
    // Issue #81: and the section's own height against the agreed minimum, plus which slot the answer card
    // took - the two layout promises a frame can prove without a picture.
    {
        let w = scene_rect.window.x.max(1.0);
        let frac = (scene_rect.max.x - scene_rect.min.x).max(0.0) / w;
        set_attr(&doc, "viz-root", "data-scene-frac", &format!("{frac:.3}"));
        let section_h = (scene_rect.max.y - scene_rect.min.y).max(0.0);
        let section_w = (scene_rect.max.x - scene_rect.min.x).max(0.0);
        set_attr(
            &doc,
            "viz-root",
            "data-scene-rect",
            &format!(
                "{:.0},{:.0},{section_w:.0},{section_h:.0}",
                scene_rect.min.x, scene_rect.min.y
            ),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-min-section-h",
            &format!("{:.0}", crate::scene::MIN_SECTION_H),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-card-slot",
            if crate::scene::hud_fits(section_w, scene_rect.phone) {
                "hud"
            } else {
                "above"
            },
        );
    }
    set_attr(
        &doc,
        "viz-root",
        "data-rail",
        if vis.rail_open { "open" } else { "collapsed" },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-legend",
        if vis.legend_open { "shown" } else { "hidden" },
    );
    // ---- round 5: the layout counters, and the clip probe the layout frames and `tools/clip-check.mjs`
    // assert on. Every value here is measured by the UI while it draws (see `crate::clip`), not asserted.
    {
        let info = &round5.info;
        let flag = |b: bool| if b { "1" } else { "0" };
        set_attr(
            &doc,
            "viz-root",
            "data-right-w",
            &format!("{:.0}", info.right_w),
        );
        set_attr(&doc, "viz-root", "data-duty-open", flag(info.duty_open));
        set_attr(&doc, "viz-root", "data-water-open", flag(info.water_open));
        set_attr(&doc, "viz-root", "data-limits-open", flag(info.limits_open));
        set_attr(&doc, "viz-root", "data-duty-summary", &info.duty_summary);
        set_attr(&doc, "viz-root", "data-water-summary", &info.water_summary);
        set_attr(
            &doc,
            "viz-root",
            "data-limits-summary",
            &info.limits_summary,
        );
        set_attr(&doc, "viz-root", "data-sections", &info.section_rects);
        set_attr(&doc, "viz-root", "data-hint-state", info.hint_state);
        set_attr(&doc, "viz-root", "data-hint", &info.hint);
        set_attr(
            &doc,
            "viz-root",
            "data-bay-labels",
            &info.bay_labels.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-bay-label-overlaps",
            &info.bay_label_overlaps.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-bay-label-outside",
            &info.bay_label_outside.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-bay-label-truncated",
            &info.bay_label_truncated.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-bay-label-dropped",
            &info.bay_label_dropped.to_string(),
        );
        if !info.bay_label_notes.is_empty() {
            set_attr(
                &doc,
                "viz-root",
                "data-bay-label-notes",
                &info.bay_label_notes,
            );
        }
        set_attr(&doc, "viz-root", "data-plates", &info.plates.to_string());
        set_attr(&doc, "viz-root", "data-plate-rects", &info.plate_rects);
        set_attr(
            &doc,
            "viz-root",
            "data-plate-overlaps",
            &info.plate_overlaps.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-readout-rows",
            &info.readout_rows.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-readout-units",
            &info.readout_units.to_string(),
        );
        set_attr(&doc, "viz-root", "data-staged-text", &info.staged_text);
        set_attr(
            &doc,
            "viz-root",
            "data-staged-truncated",
            flag(info.staged_truncated),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-footer-overlaps",
            &info.footer_overlaps.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-clip-entries",
            &round5.clip.entries.len().to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-clip-overflows",
            &round5.clip.overflow_count().to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-clip-below",
            &round5.clip.below_count().to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-clip-clips",
            &round5.clip.clip_count().to_string(),
        );
        // The probe itself, as JSON, for `tools/clip-check.mjs` to re-read per frame.
        set_text(&doc, "mirror-clip", &round5.clip.json());
        set_text(&doc, "mirror-words", &round5.words.json());
    }
    if first.first.is_none() {
        let ms = now_ms();
        first.first = Some(ms);
        set_attr(&doc, "viz-root", "data-first-frame-ms", &format!("{ms:.0}"));
    }
    if first.ready.is_none() && load.ready {
        let ms = now_ms();
        first.ready = Some(ms);
        set_attr(&doc, "viz-root", "data-interactive-ms", &format!("{ms:.0}"));
    }

    set_attr(&doc, "viz-root", "data-view", vis.view.slug());
    set_attr(&doc, "viz-root", "data-focus", vis.focus.slug());
    set_attr(&doc, "viz-root", "data-focus-name", vis.focus.name());
    set_attr(
        &doc,
        "viz-root",
        "data-host",
        if options.host.public {
            "public"
        } else {
            "internal"
        },
    );
    // Issue #58: which engine this build selected, in the engine's own words (`Engine::name`). The
    // attribute used to name the baseline's `FixtureEngine` unconditionally, which stopped being true
    // the moment the real engine became the default.
    set_attr(
        &doc,
        "viz-root",
        "data-source",
        round5
            .engine
            .0
            .as_deref()
            .map(|e| e.name())
            .unwrap_or("not loaded"),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-load",
        if load.failure.is_some() {
            "failed"
        } else if load.ready {
            "ready"
        } else {
            "loading"
        },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-frozen",
        if clock.frozen { "1" } else { "0" },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-grid",
        if vis.grid { "1" } else { "0" },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-nozzle-spacing",
        &fmt2(vis.nozzle_spacing_m),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-nozzle-pattern",
        vis.nozzle_pattern.slug(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-selected-slot",
        vis.selected_slot.slug(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-selected-layer",
        &vis.selected_layer.to_string(),
    );
    set_text(&doc, "mirror-view", vis.view.name());
    set_text(&doc, "mirror-focus", vis.focus.name());
    set_text(
        &doc,
        "mirror-staged",
        &if staged.0.is_empty() {
            "nothing staged".to_string()
        } else {
            staged.0.join(" · ")
        },
    );
    set_text(&doc, "mirror-error", load.failure.as_deref().unwrap_or(""));

    // The engine side.
    let (rpm, ratio, fan_id) = match draft.as_deref() {
        Some(d) => (
            // The record's own rated speed; empty when the record states none (never an invented
            // number). The attribute is a mirror of the read-out, so it spells the same absence.
            drafthouse_cockpit_seams::mapping::rpm(d.0.speed_ratio, d.0.fan.nominal_rpm)
                .map(|rpm| format!("{rpm:.1}"))
                .unwrap_or_default(),
            d.0.speed_ratio,
            d.0.fan.id.clone(),
        ),
        None => (String::new(), 0.0, "-".to_string()),
    };
    set_attr(&doc, "viz-root", "data-rpm", &rpm);
    set_attr(&doc, "viz-root", "data-ratio", &format!("{ratio:.4}"));
    set_attr(&doc, "viz-root", "data-fan", &fan_id);
    set_text(
        &doc,
        "mirror-rpm",
        &format!("{rpm:.0} rpm (speed ratio {ratio:.3})"),
    );

    match &run.output {
        Some(o) => {
            set_attr(&doc, "viz-root", "data-airflow", &fmt2(o.airflow_m3_s));
            set_attr(
                &doc,
                "viz-root",
                "data-pressure",
                &fmt2(o.total_pressure_pa),
            );
            set_attr(&doc, "viz-root", "data-fan-power", &fmt2(o.fan_power_kw));
            set_attr(
                &doc,
                "viz-root",
                "data-op-flow",
                &fmt2(o.fan_system_curve.operating_point.x),
            );
            set_attr(
                &doc,
                "viz-root",
                "data-op-pressure",
                &fmt2(o.fan_system_curve.operating_point.y),
            );
            set_attr(&doc, "viz-root", "data-capability", &fmt2(o.capability_pct));
            // Round 4, item 4: the two numbers an edited duty must move (the frame compares them with the
            // anchor's, so a panel edit that did not reach the engine cannot pass).
            set_attr(&doc, "viz-root", "data-cold-water", &fmt2(o.cold_water_c));
            set_attr(
                &doc,
                "viz-root",
                "data-engine-approach",
                &fmt2(o.approach_c),
            );
            set_text(
                &doc,
                "mirror-airflow",
                &format!("{} m3/s", crate::theme::fmt(o.airflow_m3_s)),
            );
            set_text(
                &doc,
                "mirror-pressure",
                &format!(
                    "{} Pa ({} mmWG)",
                    crate::theme::fmt(o.total_pressure_pa),
                    crate::theme::fmt(o.total_pressure_mmwg)
                ),
            );
            set_text(
                &doc,
                "mirror-fanpower",
                &format!("{} kW at {fan_id}", crate::theme::fmt(o.fan_power_kw)),
            );
            set_text(
                &doc,
                "mirror-op",
                &format!(
                    "operating point {} m3/s at {} Pa (engine output)",
                    crate::theme::fmt(o.fan_system_curve.operating_point.x),
                    crate::theme::fmt(o.fan_system_curve.operating_point.y)
                ),
            );
            // #91 round 2: the correctness gate's engine side. One JSON attribute carrying the numbers
            // the screen paints, straight out of this run - the same grouped, largest-remainder rows the
            // answer card, the tap-detail and the tower's call-outs read (`answer::air_path`). The gate
            // (`docs/design/usability-r2/tools/gate.mjs`) compares these against the painted strings, so a
            // painted figure that is not the engine's is a red gate, not a review note.
            {
                let rows = crate::answer::air_path(o);
                let zones: Vec<serde_json::Value> = rows
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "n": r.name,
                            "pa": r.pa,
                            "s": r.share_shown,
                        })
                    })
                    .collect();
                let gate = serde_json::json!({
                    "zones": zones,
                    "fill_pa": rows.iter().find(|r| r.zone == cockpit::engine::ZoneId::Fill).map(|r| r.pa),
                    "fill_kavl": crate::answer::fill_kavl(o),
                    "kavl_total": o.kavl_total,
                    "cold": o.cold_water_c,
                    "approach": o.approach_c,
                    "fan_kw": o.fan_power_kw,
                    "makeup": o.makeup_m3_hr,
                    "evap_pct": o.evaporation_pct,
                    "flow": o.airflow_m3_s,
                });
                set_attr(&doc, "viz-root", "data-gate", &gate.to_string());
            }
            set_text(
                &doc,
                "mirror-zones",
                &o.pressure_by_zone
                    .iter()
                    .map(|z| format!("{} {} Pa", z.label, crate::theme::fmt(z.pressure_pa)))
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
        }
        None => {
            for (k, id) in [
                ("data-airflow", "mirror-airflow"),
                ("data-pressure", "mirror-pressure"),
                ("data-fan-power", "mirror-fanpower"),
            ] {
                set_attr(&doc, "viz-root", k, "");
                set_text(&doc, id, "-");
            }
            set_text(&doc, "mirror-op", "no run yet");
            set_text(&doc, "mirror-zones", "no run yet");
            set_attr(&doc, "viz-root", "data-gate", "");
        }
    }

    // The slots and the stack, so a frame's evidence says what is in the machine.
    match draft.as_deref() {
        Some(d) => {
            set_attr(&doc, "viz-root", "data-slot-fan", &d.0.fan.id);
            set_attr(&doc, "viz-root", "data-slot-drift", &d.0.drift.id);
            set_attr(&doc, "viz-root", "data-slot-nozzle", &d.0.nozzle.id);
            let stack =
                d.0.fill_layers
                    .iter()
                    .map(|l| format!("{}:{:.2}", l.fill_id, l.depth_m))
                    .collect::<Vec<_>>()
                    .join(",");
            set_attr(&doc, "viz-root", "data-fill-stack", &stack);
            set_text(
                &doc,
                "mirror-stack",
                &d.0.fill_layers
                    .iter()
                    .map(|l| format!("{} {:.2} m", l.fill_id, l.depth_m))
                    .collect::<Vec<_>>()
                    .join(" over "),
            );
        }
        None => {
            for k in [
                "data-slot-fan",
                "data-slot-drift",
                "data-slot-nozzle",
                "data-fill-stack",
            ] {
                set_attr(&doc, "viz-root", k, "");
            }
            set_text(&doc, "mirror-stack", "-");
        }
    }

    // The drag, in one attribute, so a mid-drag frame is provable.
    match &vis.drag {
        Some(drag) => {
            set_attr(&doc, "viz-root", "data-drag", &drag.part.slug());
            set_attr(
                &doc,
                "viz-root",
                "data-drag-over",
                drag.over.map(|s| s.slug()).unwrap_or("none"),
            );
            set_attr(
                &doc,
                "viz-root",
                "data-drag-verdict",
                match &drag.verdict {
                    Ok(()) => "valid",
                    Err(_) => "invalid",
                },
            );
            set_attr(
                &doc,
                "viz-root",
                "data-drag-staged",
                if drag.staged { "1" } else { "0" },
            );
            let reason = match &drag.verdict {
                Ok(()) => "accepted by the catalog".to_string(),
                Err(e) => e.clone(),
            };
            set_text(
                &doc,
                "mirror-drag",
                &format!(
                    "dragging {} over {}: {reason}",
                    drag.part.id,
                    drag.over.map(|s| s.name()).unwrap_or("nothing")
                ),
            );
        }
        None => {
            for k in [
                "data-drag",
                "data-drag-over",
                "data-drag-verdict",
                "data-drag-staged",
            ] {
                set_attr(&doc, "viz-root", k, "");
            }
            set_text(&doc, "mirror-drag", "no drag");
        }
    }

    set_attr(
        &doc,
        "viz-root",
        "data-seams",
        &format!("{}", drafthouse_cockpit_seams::SEAMS.len()),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-action",
        vis.flash.as_ref().map(|f| f.text.as_str()).unwrap_or(""),
    );
    // `data-hover`: which published hit rect the pointer the app sees is inside. A debug seam, and the
    // harness's way to say "the pointer was over this control".
    let hover = hits
        .0
        .iter()
        .find(|(_, r)| {
            let (px, py) = hits
                .0
                .iter()
                .find(|(k, _)| k == "ctl:pointer")
                .map(|(_, p)| (p[0], p[1]))
                .unwrap_or((-1.0, -1.0));
            r[2] > 0.5
                && r[3] > 0.5
                && px >= r[0]
                && px <= r[0] + r[2]
                && py >= r[1]
                && py <= r[1] + r[3]
        })
        .map(|(k, _)| k.clone())
        .unwrap_or_default();
    set_attr(&doc, "viz-root", "data-hover", &hover);
    // The hit map: what the instrument drew, in canvas pixels, so a real pointer can be aimed at it.
    set_text(
        &doc,
        "mirror-spacing",
        &format!("{:.2} m pitch", vis.nozzle_spacing_m),
    );
    if !hits.0.is_empty() {
        let json = hits
            .0
            .iter()
            .map(|(k, r)| format!("\"{k}\":[{:.0},{:.0},{:.0},{:.0}]", r[0], r[1], r[2], r[3]))
            .collect::<Vec<_>>()
            .join(",");
        set_attr(&doc, "viz-root", "data-hits", &format!("{{{json}}}"));
    }
    set_text(
        &doc,
        "mirror-label",
        drafthouse_cockpit_seams::REQUIRED_COPY,
    );
    // ---- issue #91: the two drawers and the motion mode ----------------------------------------------
    set_attr(
        &doc,
        "viz-root",
        "data-notes",
        if vis.notes_open { "open" } else { "closed" },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-panel",
        if vis.panel_open { "open" } else { "closed" },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-motion",
        if vis.reduced_motion {
            "reduced"
        } else {
            "full"
        },
    );
    // ---- round 2: the picker, the bay focus and the 3D view ---------------------------------------
    match &vis.picker {
        Some(p) => {
            let rows = state::picker_rows(&catalog, p.slot);
            set_attr(&doc, "viz-root", "data-picker", p.slot.slug());
            set_attr(
                &doc,
                "viz-root",
                "data-picker-title",
                &state::picker_title(draft.as_deref().map(|d| &d.0), &vis),
            );
            set_attr(
                &doc,
                "viz-root",
                "data-picker-rows",
                &rows.len().to_string(),
            );
            set_attr(&doc, "viz-root", "data-picker-row", &p.row.to_string());
            set_attr(
                &doc,
                "viz-root",
                "data-picker-row-id",
                &rows
                    .get(p.row.min(rows.len().saturating_sub(1)))
                    .map(|r| r.id.clone())
                    .unwrap_or_default(),
            );
            set_attr(
                &doc,
                "viz-root",
                "data-picker-sheet",
                if vis.picker_sheet { "1" } else { "0" },
            );
            let list = rows
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
                .join(" · ");
            set_text(
                &doc,
                "mirror-picker",
                &format!("{} open: {}", p.slot.name(), list),
            );
        }
        None => {
            for k in [
                "data-picker",
                "data-picker-title",
                "data-picker-rows",
                "data-picker-row",
                "data-picker-row-id",
                "data-picker-sheet",
            ] {
                set_attr(&doc, "viz-root", k, "");
            }
            set_text(&doc, "mirror-picker", "no picker open");
        }
    }
    set_attr(
        &doc,
        "viz-root",
        "data-bay-focus",
        vis.bay_focus.map(|s| s.slug()).unwrap_or(""),
    );
    // Round 4, item 1: the 3D view's markers exist only with the `three-d` feature. Without it there is no
    // cell count, no cutaway, no camera and no mesh count to publish - the view is gone from the UI.
    #[cfg(feature = "three-d")]
    {
        set_attr(&doc, "viz-root", "data-cells", &vis.cells.to_string());
        set_attr(
            &doc,
            "viz-root",
            "data-cutaway",
            if vis.cutaway { "1" } else { "0" },
        );
        set_attr(&doc, "viz-root", "data-cell", &vis.focus_cell.to_string());
        // Round 3, item 6: what the cut removed and what it exposed, straight from the builder.
        set_attr(
            &doc,
            "viz-root",
            "data-cut-removed",
            &three.cut_removed.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-cut-faces",
            &three.cut_faces.to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-cam",
            &format!(
                "{:.1},{:.1},{:.2}",
                vis.cam_yaw, vis.cam_pitch, vis.cam_dist
            ),
        );
        set_attr(&doc, "viz-root", "data-meshes3d", &three.meshes.to_string());
        set_attr(&doc, "viz-root", "data-frames", &three.frames.to_string());
        set_attr(&doc, "viz-root", "data-pick3d", &three.last_pick);
        set_attr(
            &doc,
            "viz-root",
            "data-blade-phase",
            &format!("{:.4}", three.blade_phase),
        );
        set_text(
            &doc,
            "mirror-three",
            &format!(
                "3D tower: {} cell(s), cutaway {}, focus cell {}, camera yaw {:.0} pitch {:.0} dist {:.2}, {} meshes",
                three.cells,
                if three.cutaway { "open" } else { "closed" },
                three.focus_cell + 1,
                three.cam.0,
                three.cam.1,
                three.cam.2,
                three.meshes
            ),
        );
    }
    // Round 3, item 4/5: what the charts drew, and the pressure the fan/system chart marks.
    set_attr(
        &doc,
        "viz-root",
        "data-perf-wb-lines",
        &charts.wb_lines.to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-perf-wb-pts",
        &charts.wb_pts.to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-perf-kavl-pts",
        &charts.kavl_pts.to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-chart-fan-pa",
        &format!("{:.2}", charts.fan_pa),
    );
    // The two illustrations, published as counts so a still frame can prove what it shows.
    let flow = run
        .output
        .as_ref()
        .map(|o| o.airflow_m3_s)
        .unwrap_or(drafthouse_cockpit_seams::mapping::ANCHOR_AIRFLOW_M3_S);
    set_attr(
        &doc,
        "viz-root",
        "data-streamlines",
        &drafthouse_cockpit_seams::mapping::streamline_count(flow).to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-flow-factor",
        &format!(
            "{:.3}",
            drafthouse_cockpit_seams::mapping::flow_factor(flow)
        ),
    );
    let water = run
        .output
        .as_ref()
        .map(|o| o.water_flow_m3_hr)
        .unwrap_or(0.0);
    set_attr(&doc, "viz-root", "data-water-flow", &fmt2(water));
    // Issue #91: the frame counter (`scene::frames()`), so fps and idle redraws are measurable from the page.
    set_attr(
        &doc,
        "viz-root",
        "data-frames",
        &crate::scene::frames().to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-water-streaks",
        &drafthouse_cockpit_seams::mapping::water_streak_count(water).to_string(),
    );
    // ---- round 4, item 2: the custom-part form and the session's own records.
    let (form_class, form_field_count, form_errors, form_first_error) =
        crate::form::markers(&round4.form, round4.fields.fields.as_ref());
    set_attr(&doc, "viz-root", "data-form", &form_class);
    set_attr(&doc, "viz-root", "data-form-fields", &form_field_count);
    set_attr(&doc, "viz-root", "data-form-errors", &form_errors);
    set_attr(&doc, "viz-root", "data-form-error", &form_first_error);
    set_attr(
        &doc,
        "viz-root",
        "data-form-staged",
        if round4.form.staged { "1" } else { "0" },
    );
    let custom_ids: Vec<String> = catalog
        .custom
        .iter()
        .map(|c| format!("{}:{}", c.class, c.id()))
        .collect();
    set_attr(
        &doc,
        "viz-root",
        "data-custom-count",
        &catalog.custom.len().to_string(),
    );
    set_attr(&doc, "viz-root", "data-custom", &custom_ids.join(","));
    set_text(
        &doc,
        "mirror-custom",
        &if custom_ids.is_empty() {
            "no custom parts".to_string()
        } else {
            format!("custom parts (session only): {}", custom_ids.join(" · "))
        },
    );
    // ---- round 4, item 3: the parameter card.
    // The card's source: a record authored in this session says `custom`, a fixture record `catalog`.
    let hover_source = match round4.card.part.as_ref() {
        Some(p) => {
            if catalog.custom_part(p.class, &p.id).is_some() {
                "custom"
            } else {
                "catalog"
            }
        }
        None => "",
    };
    set_attr(&doc, "viz-root", "data-hover-source", hover_source);
    set_attr(
        &doc,
        "viz-root",
        "data-hover-curve-rows",
        &round4.card.curve_rows.to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-hover-engine-rows",
        &round4.card.engine_rows.to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-hover-card",
        &round4
            .card
            .part
            .as_ref()
            .map(|p| p.slug())
            .unwrap_or_default(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-hover-long",
        if round4.card.long_press { "1" } else { "0" },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-hover-bay",
        if round4.card.bay { "1" } else { "0" },
    );
    set_text(
        &doc,
        "mirror-hover",
        &round4
            .card
            .part
            .as_ref()
            .map(|p| {
                format!(
                    "parameter card: {} ({}{})",
                    p.slug(),
                    if round4.card.long_press {
                        "long press"
                    } else {
                        "hover"
                    },
                    if round4.card.bay { ", fitted bay" } else { "" }
                )
            })
            .unwrap_or_else(|| "no parameter card".to_string()),
    );
    // ---- round 4, item 4: the duty, its derived pair and the evidence gate.
    if let Some(d) = draft.as_deref() {
        let (evidence, out, domain) = crate::duty_panel::markers(&round4.duty, &d.0.duty);
        let (range, approach) = crate::duty_panel::derived(&d.0.duty);
        set_attr(&doc, "viz-root", "data-duty-evidence", &evidence);
        set_attr(&doc, "viz-root", "data-duty-out", &out);
        set_attr(&doc, "viz-root", "data-duty-evidence-domain", &domain);
        set_attr(
            &doc,
            "viz-root",
            "data-duty-flow",
            &fmt2(d.0.duty.water_flow_m3_hr),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-flow-kg-s",
            &fmt2(drafthouse_cockpit_seams::mapping::kg_s_from_m3_hr(
                d.0.duty.water_flow_m3_hr,
            )),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-hot",
            &fmt2(d.0.duty.hot_water_c),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-target",
            &fmt2(d.0.duty.target_cold_water_c),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-wetbulb",
            &fmt2(d.0.duty.wet_bulb_c),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-drybulb",
            &fmt2(d.0.duty.dry_bulb_c),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-pressure",
            &fmt2(d.0.duty.pressure_pa),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-class",
            &d.0.duty.water_quality_class,
        );
        let pre = crate::duty_panel::precheck(&d.0.duty);
        set_attr(
            &doc,
            "viz-root",
            "data-duty-precheck",
            &pre.len().to_string(),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-limit",
            pre.first().map(|s| s.as_str()).unwrap_or(""),
        );
        set_attr(&doc, "viz-root", "data-duty-range", &fmt2(range));
        set_attr(&doc, "viz-root", "data-duty-approach", &fmt2(approach));
        set_attr(
            &doc,
            "viz-root",
            "data-duty-cycles",
            &fmt2(d.0.duty.cycles_of_concentration),
        );
        set_attr(
            &doc,
            "viz-root",
            "data-duty-salinity",
            &fmt2(d.0.duty.salinity_g_kg),
        );
        set_text(
            &doc,
            "mirror-duty",
            &format!(
                "duty: water {} m3/hr ({} kg/s at 1000 kg/m3), hot {} C, target cold {} C, wet bulb {} C, dry bulb {} C, pressure {} Pa, class {}, range {} K, approach {} K, evidence {} · TDS / chloride / pH: recorded, not used by the engine yet",
                crate::theme::fmt(d.0.duty.water_flow_m3_hr),
                crate::theme::fmt(drafthouse_cockpit_seams::mapping::kg_s_from_m3_hr(
                    d.0.duty.water_flow_m3_hr
                )),
                crate::theme::fmt(d.0.duty.hot_water_c),
                crate::theme::fmt(d.0.duty.target_cold_water_c),
                crate::theme::fmt(d.0.duty.wet_bulb_c),
                crate::theme::fmt(d.0.duty.dry_bulb_c),
                crate::theme::fmt(d.0.duty.pressure_pa),
                d.0.duty.water_quality_class,
                crate::theme::fmt(range),
                crate::theme::fmt(approach),
                evidence.replace('-', " ")
            ),
        );
    }
    // ---- issue #74: the run's own eleven headline values, at the display precision, in the contract's
    // own order. One attribute, so a frame can state every number it shows and two frames can be compared
    // for equality without reading pixels.
    match run.output.as_ref() {
        Some(o) => {
            let values: Vec<String> = cockpit::engine::HEADLINES
                .iter()
                .zip(o.headline_values())
                .map(|(headline, value)| format!("{}={value:.2}", headline.name))
                .collect();
            set_attr(&doc, "viz-root", "data-headlines", &values.join("|"));
        }
        None => set_attr(&doc, "viz-root", "data-headlines", ""),
    }

    // ---- issue #74: the project session. `data-project*` is what the page's own buttons read, and
    // `#mirror-project` carries the project file's text for *Download project* - the page hands those
    // bytes to the user, and nothing is sent anywhere.
    set_attr(
        &doc,
        "viz-root",
        "data-project-name",
        &round5.session.file_name(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-project-dirty",
        if draft
            .as_deref()
            .map(|d| round5.session.is_dirty(&d.0, &catalog))
            .unwrap_or(false)
        {
            "1"
        } else {
            "0"
        },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-project-path-set",
        if round5.session.path.is_some() {
            "1"
        } else {
            "0"
        },
    );
    set_attr(
        &doc,
        "viz-root",
        "data-project-bytes",
        &round5.session.outbox.len().to_string(),
    );
    set_attr(
        &doc,
        "viz-root",
        "data-catalog-revision",
        &round5.session.revision_id,
    );
    set_text(&doc, "mirror-project", &round5.session.outbox);
    let session_line = if round5.session.status.is_empty() {
        round5.session.recompute_line(run.output.as_ref())
    } else {
        round5.session.status.clone()
    };
    set_text(&doc, "mirror-project-status", &session_line);
    let _ = &load;

    let provenance = run
        .output
        .as_ref()
        .map(|o| {
            format!(
                "{} · {} · {} · {}",
                o.provenance.engine,
                o.provenance.catalog_id,
                o.provenance.catalog_revision,
                o.provenance.catalog_status
            )
        })
        .unwrap_or_else(|| {
            format!(
                "{} - {} - {}",
                catalog.catalog_id, catalog.catalog_revision, catalog.catalog_status
            )
        });
    set_text(&doc, "mirror-provenance", &provenance);
    let _ = &anchor;
    let _ = &options.host;
    let _ = SceneRect::default();
}
