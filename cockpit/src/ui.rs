//! The instrument: tray, section overlay, rpm dock, the read-out rail, the operating-point instruments
//! and the data-seams panel.
//!
//! Surface archetype: **OPERATE + COMMAND-INSPECT**. The tower section is the interface, the trays and bays
//! are the authoring surface, and every number carries the name of what produced it. No KPI wall.
//!
//! Labels are painted on the *same* [`crate::scene::layout`] the sprites use, so text and geometry cannot
//! drift apart.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Shape, Stroke, StrokeKind};

use cockpit::engine::{Engine, EngineInput, EngineOutput, ZoneId};
use drafthouse_cockpit_seams::mapping as m;

use crate::app::{
    open_picker, picker_command, AnimClock, Draft, EngineSlot, HitMap, Load, SceneRect, StagedLog,
    WantsKeyboard,
};
use crate::duty_panel::{self, DutyRes};
use crate::form;
use crate::hover::{self, HoverState};
use crate::scene;
use crate::state::{
    self, add_layer, apply_drop, check_drop, move_layer, picker_title, remove_layer, Anchor,
    Catalog, Class, Drag, FieldsRes, Flash, Focus, PartRef, Pattern, Picker, Run, Slot,
    StartOptions, View, Visual,
};
#[cfg(feature = "three-d")]
use crate::state::{CAM_DEFAULT, MAX_CELLS, MIN_CELLS};
use crate::theme as t;

const LEFT_W: f32 = 232.0;
/// Round 3, item 1: the narrow parts rail. Open it is 180 px of one-line chips; a caret collapses it to a
/// 28 px icon strip. A phone has no rail at all - it reaches parts through the bay-tap picker's bottom
/// sheet, which is the round-2 path unchanged.
const RAIL_W: f32 = 180.0;
const RAIL_STRIP_W: f32 = 28.0;
/// Round 5, item 1: the right column's width. Round 4's 344 px could not hold the duty form's two-value
/// rows: the widest row measured 377 px inside a 304 px content box, so the whole content grew and ran off
/// the right edge of the screen (the owner's report: a two-value row loses its second value and unit).
/// At 1280 px and above the column is 400 px, and the width comes from the centre canvas - the section is
/// a drawing, and a drawing can lose 56 px; a form row cannot lose a unit.
const RIGHT_W: f32 = 344.0;
const RIGHT_W_WIDE: f32 = 400.0;
const RIGHT_WIDE_ABOVE: f32 = 1280.0;
/// Round 3, item 1: the width below which the rail steps in, so the section keeps the majority of the
/// frame on a small laptop (the brief's floor is ~60 % of the viewport width at 1440x900).
const RAIL_NARROW_BELOW: f32 = 1240.0;
const RAIL_W_NARROW: f32 = 150.0;
/// Issue #91: below this width the parts rail is folded to its icon strip (the bay-tap picker stays).
const RAIL_FOLD_BELOW: f32 = 1100.0;
/// Issue #91: the Instrument view's bottom control bar (desktop): the rpm row and the nozzle/part row.
const ID_FONTS_INSTALLED: &str = "viz.fonts.installed";
const ID_FONTS_READY: &str = "viz.fonts.ready";
const ID_FONTS_PENDING: &str = "viz.fonts.pending";

/// Fonts are installed before the pass that draws text (see the baseline's note: `set_fonts` only queues,
/// and a named family used before it is bound panics epaint).
pub fn init_fonts(mut contexts: EguiContexts) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    if ctx
        .data(|d| d.get_temp::<bool>(egui::Id::new(ID_FONTS_INSTALLED)))
        .is_some()
    {
        return;
    }
    ctx.set_fonts(t::fonts());
    t::apply_style(ctx, false);
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::new(ID_FONTS_INSTALLED), true);
        d.insert_temp(egui::Id::new(ID_FONTS_PENDING), 1u32);
    });
}

fn fonts_bound(ctx: &egui::Context) -> bool {
    if ctx
        .data(|d| d.get_temp::<bool>(egui::Id::new(ID_FONTS_READY)))
        .is_some()
    {
        return true;
    }
    match ctx.data(|d| d.get_temp::<u32>(egui::Id::new(ID_FONTS_PENDING))) {
        Some(0) => {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(ID_FONTS_READY), true));
            true
        }
        Some(n) => {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(ID_FONTS_PENDING), n - 1));
            false
        }
        None => false,
    }
}

// ================================================================================================ ui

/// Round 4's resources, bundled: the UI system already used every one of Bevy's 16 parameter slots, and the
/// form, the parameter card, the duty panel and the charts need six more between them.
#[derive(bevy::ecs::system::SystemParam)]
pub struct VizAux<'w, 's> {
    pub perf: Res<'w, crate::perf::PerfGridRes>,
    pub charts: ResMut<'w, crate::state::ChartStats>,
    pub fields: Res<'w, FieldsRes>,
    pub duty: Res<'w, DutyRes>,
    pub form: ResMut<'w, form::CustomForm>,
    pub hover: ResMut<'w, HoverState>,
    /// Round 5: the layout measurement surface - the clip probe (one entry per text unit of the right
    /// column, published to `#mirror-clip`) and the counters the layout frames read.
    pub clip: ResMut<'w, crate::clip::ClipProbe>,
    pub info: ResMut<'w, crate::clip::LayoutInfo>,
    /// Issue #74: the project session (the file being edited, the recent list, the prompt) and the fixture
    /// text ("New" needs the recorded default input). Both live here rather than as parameters of their
    /// own: `viz_ui` is at the sixteen-parameter limit a Bevy system may have (see the note above).
    pub session: ResMut<'w, crate::files::FileSession>,
    /// Issue #91: the painted-text inventory (`#mirror-words`).
    pub words: ResMut<'w, crate::clip::ScreenText>,
    /// The fixture text: one field for both needs - #74's "New" reads the recorded default input from it,
    /// and the #91 screens select the whole catalog out of it.
    pub fixture_text: Res<'w, crate::app::FixtureText>,
    #[doc(hidden)]
    pub _phantom: std::marker::PhantomData<&'s ()>,
}

#[allow(clippy::too_many_arguments)]
pub fn viz_ui(
    mut contexts: EguiContexts,
    mut draft: Option<ResMut<Draft>>,
    mut catalog: Option<ResMut<Catalog>>,
    mut engine: ResMut<EngineSlot>,
    mut run: ResMut<Run>,
    anchor: Res<Anchor>,
    load: Res<Load>,
    options: Res<StartOptions>,
    mut vis: ResMut<Visual>,
    mut scene_rect: ResMut<SceneRect>,
    mut wants_kb: ResMut<WantsKeyboard>,
    staged: Res<StagedLog>,
    clock: Res<AnimClock>,
    mut hits: ResMut<HitMap>,
    mut aux: VizAux,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    if !fonts_bound(ctx) {
        return;
    }
    // Issue #86 AC 4: with reduced motion egui's own transitions (hover fades, pop-ups) run over zero time -
    // the mode turns animation **off**, it does not slow it down. Otherwise egui keeps its own default.
    if vis.reduced_motion {
        ctx.all_styles_mut(|s| s.animation_time = 0.0);
    }

    // The engine is called here, every frame, on the current draft: the fixture engine is arithmetic on the
    // recorded run (its own documented re-expression), so the read-outs never lag the slider.
    if let (Some(d), Some(e)) = (draft.as_deref(), engine.0.as_ref()) {
        match e.run(&d.0) {
            Ok(o) => {
                run.output = Some(o);
                run.error = None;
            }
            Err(err) => {
                run.output = None;
                run.error = Some(err);
            }
        }
        run.ratio = d.0.speed_ratio;
    }

    // drafthouse#91 Part B: the app shell and the new screens (`crate::screens`). On a new screen the shell
    // draws the whole frame; on Instrument it hands this view the rect its chrome leaves.
    let full = ctx.viewport_rect();
    // Issue #85 completion: the Report export's revision row carries the project document's own
    // digest. The document's canonical bytes are produced **at click time** through the same writer
    // the File menu's save uses (`files::snapshot_text`), so a project saved and exported unchanged
    // carries the saved file's own sha-256 (project writes are deterministic; issue #74), and a
    // sheet built after a modification carries the working document's. `None` - no saved or opened
    // document in the session - keeps the export's labelled state-hash fallback.
    let report_document = |input: &EngineInput| -> Option<crate::screens::DocumentBytes> {
        let backed = aux.session.path.is_some()
            || aux.session.loaded.is_some()
            || !aux.session.outbox.is_empty();
        if !backed {
            return None;
        }
        let catalog = catalog.as_deref()?;
        let fctx = crate::files::Ctx {
            options: &options,
            host_label: crate::files::plane_label(),
            spec: aux.duty.spec.as_ref(),
            output: run.output.as_ref(),
            fixture_text: &aux.fixture_text.0,
        };
        let text = crate::files::snapshot_text(&fctx, &aux.session, input, catalog, engine.get());
        // The session's own verdict on unsaved changes (the one the File menu's prompt uses);
        // on the web an unmodified download must match what was handed out, clause by clause.
        let unchanged = !aux.session.is_dirty(input, catalog)
            && (aux.session.outbox.is_empty() || aux.session.outbox == text);
        Some(crate::screens::DocumentBytes { text, unchanged })
    };
    let shell = crate::screens::frame(
        ctx,
        crate::screens::Ctx {
            draft: draft.as_deref_mut().map(|d| &mut d.0),
            engine: engine.0.as_deref(),
            out: run.output.as_ref(),
            fixture_text: &aux.fixture_text.0,
            // Issue #86 AC 4: the **live** flag, not the startup option - the toggle turns the screens' own
            // animation off too, and the seed only decides what the flag starts as.
            reduced_motion: vis.reduced_motion,
            t: clock.t,
            document: Some(&report_document),
            // Issue #89: the comparison's variant reader resolves file records against this catalog,
            // the same one `files::open_text` uses.
            catalog: catalog.as_deref(),
        },
    );
    let screen = match shell {
        crate::screens::Frame::Took => {
            scene_rect.set(Rect::NOTHING, full.size(), full.width() < 760.0);
            return;
        }
        crate::screens::Frame::Instrument(r) => r,
    };
    let phone = full.width() < 760.0;
    let style_key = egui::Id::new("viz.style.phone");
    if ctx.data(|d| d.get_temp::<bool>(style_key)) != Some(phone) {
        t::apply_style(ctx, phone);
        ctx.data_mut(|d| d.insert_temp(style_key, phone));
    }
    wants_kb.0 = ctx.egui_wants_keyboard_input();
    ctx.request_repaint(); // the flow map and the fan are always animating

    // Round 4, item 5: the phone header carries the one view bar, so it is one row taller than round 3's -
    // and tall enough that the tab row's own rect stays inside the header's clip rect, or egui would draw
    // the tabs and refuse the tap (found by a frame that clicked the tab on a phone and changed nothing).
    // Issue #91: one header row on a desktop; on a phone the name, the badge (wrapped, never cut) and the
    // 44 px tabs.
    let header_h = if phone { 150.0 } else { 50.0 };
    // Round 5, item 4: the desktop status strip is two rows (the notice + the staged log, then the engine
    // note + the mandated copy), so the staged string can never be drawn under another sentence.
    // Issue #91: the status strip is gone once the instrument is loaded - its notice is the header's
    // badge, its sentences are in the notes drawer. It stays for the loading state only.
    let footer_h = if load.ready {
        0.0
    } else if phone {
        60.0
    } else {
        46.0
    };
    let header_r = Rect::from_min_size(screen.min, egui::vec2(screen.width(), header_h));
    let footer_r = Rect::from_min_max(
        egui::pos2(screen.left(), screen.bottom() - footer_h),
        screen.max,
    );
    let body_r = Rect::from_min_max(
        egui::pos2(screen.left(), header_r.bottom()),
        egui::pos2(screen.right(), footer_r.top()),
    );
    // Issue #91: below RAIL_FOLD_BELOW the parts rail folds to its icon strip, so the section keeps the room.
    let rail_open = vis.rail_open && screen.width() >= RAIL_FOLD_BELOW;
    let left_w = if phone {
        0.0
    } else if rail_open {
        if screen.width() < RAIL_NARROW_BELOW {
            RAIL_W_NARROW
        } else {
            RAIL_W
        }
    } else {
        RAIL_STRIP_W
    };
    let _ = LEFT_W; // the round-1 tray column is gone: this is a chip rail, not a wall of cards
    let column_w = if screen.width() >= RIGHT_WIDE_ABOVE {
        // Round 5, item 1(a): at 1280 px and above the column takes the width the duty form needs.
        RIGHT_W_WIDE
    } else {
        RIGHT_W
    };
    // Issue #91: on the Instrument view the right column is a **drawer** - closed by default, opened from
    // the read-out HUD's `duty & site` button (or `P`), sliding in over 0.22 s (instant with reduced motion).
    // The Operating-point view keeps its column (out of this round's scope); the Data-seams view has none.
    let drawer_f = ctx.animate_bool_with_time(
        egui::Id::new("viz.drawer"),
        vis.panel_open,
        if vis.reduced_motion { 0.0 } else { 0.22 },
    );
    // Issue #81: on a phone the drawer has no column to slide in from. Below the fold the panel *is* the
    // screen: `duty & site` (or `?panel=1`, or the fill detail's `edit the stack …`) puts the whole panel
    // over the centre - its close control with it - instead of setting a flag nothing draws.
    let drawer_full = phone && vis.view == View::Cockpit && vis.panel_open;
    let right_w = if drawer_full {
        body_r.width()
    } else if phone {
        0.0
    } else if vis.view == View::Cockpit {
        (column_w * drawer_f).round()
    } else {
        column_w
    };
    let left_r = Rect::from_min_max(
        body_r.min,
        egui::pos2(body_r.left() + left_w, body_r.bottom()),
    );
    // The drawer is laid out at its full width and slides: its content never re-wraps mid-animation.
    let right_r = if drawer_full {
        body_r
    } else {
        Rect::from_min_max(
            egui::pos2(body_r.right() - right_w, body_r.top()),
            egui::pos2(body_r.right() - right_w + column_w, body_r.bottom()),
        )
    };
    let centre_r = Rect::from_min_max(
        egui::pos2(left_r.right(), body_r.top()),
        egui::pos2(body_r.right() - right_w, body_r.bottom()),
    );

    // ---- round 5: the layout measurement surface. Every text unit the right column draws is recorded with
    // the rect it was drawn in, the width it had to lay out in and the natural width of its text, and the
    // set is published to `#mirror-clip` (see `crate::clip` and `tools/clip-check.mjs`). A phone has no
    // right column, so the same panel is measured against the column the phone's stack scrolls in - the
    // brief's item 5 checks the duty panel and the read-out there.
    let clip_col = if right_w > 0.0 {
        right_r.shrink2(egui::vec2(12.0, 8.0))
    } else {
        centre_r.shrink2(egui::vec2(8.0, 4.0))
    };
    aux.clip.begin(clip_col);
    aux.info.reset(right_w);

    let mut draft_mut: Option<&mut Draft> = draft.as_deref_mut();
    hits.0.clear();
    if let Some(p) = ctx.pointer_latest_pos() {
        hits.0
            .push(("ctl:pointer".to_string(), [p.x, p.y, 0.0, 0.0]));
    }

    egui::Area::new(egui::Id::new("viz"))
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_clip_rect(screen);
            {
                let p = ui.painter();
                p.rect_filled(header_r, egui::CornerRadius::same(0), t::PANEL);
                p.rect_filled(footer_r, egui::CornerRadius::same(0), t::PANEL);
                if left_w > 0.0 {
                    p.rect_filled(left_r, egui::CornerRadius::same(0), t::PANEL);
                    p.line_segment(
                        [
                            egui::pos2(left_r.right(), left_r.top()),
                            egui::pos2(left_r.right(), left_r.bottom()),
                        ],
                        Stroke::new(1.0, t::LINE_SOFT),
                    );
                }
                if right_w > 0.0 {
                    p.rect_filled(right_r, egui::CornerRadius::same(0), t::PANEL);
                    p.line_segment(
                        [
                            egui::pos2(right_r.left(), right_r.top()),
                            egui::pos2(right_r.left(), right_r.bottom()),
                        ],
                        Stroke::new(1.0, t::LINE_SOFT),
                    );
                }
                p.line_segment(
                    [
                        egui::pos2(screen.left(), header_r.bottom()),
                        egui::pos2(screen.right(), header_r.bottom()),
                    ],
                    Stroke::new(1.0, t::LINE_SOFT),
                );
                p.line_segment(
                    [
                        egui::pos2(screen.left(), footer_r.top()),
                        egui::pos2(screen.right(), footer_r.top()),
                    ],
                    Stroke::new(1.0, t::LINE_SOFT),
                );
            }

            region(ui, header_r, egui::vec2(14.0, 8.0), |ui| {
                header(
                    ui,
                    &options,
                    &mut vis,
                    &load,
                    engine.get(),
                    phone,
                    &mut hits,
                );
                // Issue #74: the File row. The native binary draws the menu here; the web-internal host
                // reaches the same functions through the page's own buttons, and the public host reaches
                // nothing at all (it can author, and it cannot save or export).
                #[cfg(not(target_arch = "wasm32"))]
                if !phone {
                    let ctx = crate::files::Ctx {
                        options: &options,
                        host_label: crate::files::plane_label(),
                        spec: aux.duty.spec.as_ref(),
                        output: run.output.as_ref(),
                        fixture_text: &aux.fixture_text.0,
                    };
                    crate::files::menu_row(
                        ui,
                        &ctx,
                        &mut aux.session,
                        draft_mut.as_deref_mut().map(|d| &mut d.0),
                        catalog.as_deref_mut(),
                        &mut engine,
                    );
                }
            });

            if !load.ready {
                region(ui, centre_r, egui::vec2(16.0, 12.0), |ui| {
                    loading_state(ui, &load)
                });
                scene_rect.set(Rect::NOTHING, full.size(), phone);
                region(ui, footer_r, egui::vec2(14.0, 5.0), |ui| {
                    footer(
                        ui,
                        &options,
                        &load,
                        &staged,
                        engine.get(),
                        &mut aux.info,
                        phone,
                    )
                });
                return;
            }

            if right_w > 0.0 {
                region_exact(ui, right_r, egui::vec2(12.0, 8.0), |ui| {
                    // Issue #58, the owner's layout nit 1: the column's usable height is the window
                    // minus the header and the status strip - the strip's height is reserved in
                    // `body_r` above, so nothing is laid out *under* it. What was left was the
                    // read-out card's own last row ending a few pixels past that boundary and being
                    // cut at the strip's edge; one point off the column's vertical item spacing
                    // gives the card the room at 1440x900. `data-clip-below` counts any measured
                    // text unit that still passes the column's bottom.
                    ui.spacing_mut().item_spacing.y = 5.0;
                    // Issue #91: on the Instrument view this column is the setup drawer - a title row
                    // with its close control, then the form, the read-out in full, the fill stack, the
                    // seams summary and the internal host's commands.
                    if vis.view == View::Cockpit {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Duty & site")
                                    .size(14.0)
                                    .color(t::INK)
                                    .family(t::family_semi()),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let b = ui
                                        .add_sized(egui::vec2(30.0, 26.0), egui::Button::new("×"));
                                    if hit(&mut hits, "ctl:panel-close", b)
                                        .on_hover_text("close (P)")
                                        .clicked()
                                    {
                                        vis.panel_open = false;
                                    }
                                },
                            );
                        });
                    }
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            rail(
                                ui,
                                catalog.as_deref(),
                                draft_mut.as_deref_mut(),
                                &run,
                                &anchor,
                                &mut vis,
                                &options,
                                &mut hits,
                                &aux.duty,
                                &mut aux.clip,
                                &mut aux.info,
                                phone,
                            );
                            if vis.view == View::Cockpit {
                                ui.add_space(8.0);
                                host_actions(ui, &options);
                            }
                        });
                });
            }

            if left_w > 0.0 {
                region(
                    ui,
                    left_r,
                    egui::vec2(if vis.rail_open { 8.0 } else { 3.0 }, 8.0),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                parts_rail(
                                    ui,
                                    catalog.as_deref(),
                                    draft_mut.as_deref(),
                                    &mut vis,
                                    &mut hits,
                                    &mut aux.form,
                                    aux.fields.fields.as_ref(),
                                );
                            });
                    },
                );
            }

            region(
                ui,
                centre_r,
                egui::vec2(if phone { 8.0 } else { 12.0 }, 8.0),
                |ui| {
                    let avail = ui.max_rect();
                    if drawer_full {
                        // #81: the panel has the centre - the drawing is not underneath it, and no stale
                        // scene rect is left for the sprite pass to draw into.
                        scene_rect.set(Rect::NOTHING, full.size(), phone);
                        return;
                    }
                    match vis.view {
                        View::Curves => {
                            let scene_h = if phone {
                                (avail.height() * 0.32).clamp(150.0, 260.0)
                            } else {
                                (avail.height() * 0.42).clamp(190.0, 330.0)
                            };
                            let scene_r =
                                Rect::from_min_size(avail.min, egui::vec2(avail.width(), scene_h));
                            scene_rect.set(scene_r, full.size(), phone);
                            let _ = ui.allocate_exact_size(scene_r.size(), Sense::hover());
                            if let Some(d) = draft_mut.as_deref_mut() {
                                scene_overlay(
                                    ui,
                                    scene_r,
                                    catalog.as_deref(),
                                    d,
                                    &mut vis,
                                    &run,
                                    ctx,
                                    &mut hits,
                                    &mut aux.info,
                                    clock.t,
                                    false,
                                    phone,
                                );
                            }
                            let body = Rect::from_min_max(
                                egui::pos2(avail.left(), scene_r.bottom() + 8.0),
                                avail.max,
                            );
                            ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(
                                    ui,
                                    |ui| {
                                        // #91 round 2: the fan's speed has one implementation and one home
                                        // per surface - here, under the fan/system curve it moves, because
                                        // the Instrument reaches it through the fan bay's tap-detail.
                                        if let Some(d) = draft_mut.as_deref_mut() {
                                            let card_frame = t::card_flat();
                                            card_frame.show(ui, |ui| {
                                                ui.label(t::eyebrow("fan speed"));
                                                fan_speed_controls(
                                                    ui, d, &run, &mut vis, &mut hits, phone,
                                                );
                                            });
                                            ui.add_space(8.0);
                                        }
                                        curves_view(
                                            ui,
                                            draft_mut.as_deref(),
                                            catalog.as_deref(),
                                            aux.perf.0.as_ref(),
                                            &run,
                                            &anchor,
                                            phone,
                                            &mut aux.charts,
                                            &mut hits,
                                        )
                                    },
                                );
                            });
                        }
                        #[cfg(feature = "three-d")]
                        View::Three => {
                            // Round 2, change C: the 3D tower. The Bevy 3D camera renders behind this
                            // transparent region; here we own the controls, the honesty labels and the taps.
                            scene_rect.set(Rect::NOTHING, full.size(), phone);
                            let view_r = Rect::from_min_max(
                                avail.min,
                                egui::pos2(
                                    avail.right(),
                                    if phone {
                                        (avail.top() + avail.height() * 0.66).min(avail.bottom())
                                    } else {
                                        avail.bottom()
                                    },
                                ),
                            );
                            let resp = ui.interact(
                                view_r,
                                egui::Id::new("three.viewport"),
                                Sense::click_and_drag(),
                            );
                            hits.0.push((
                                "three:viewport".to_string(),
                                [view_r.min.x, view_r.min.y, view_r.width(), view_r.height()],
                            ));
                            three_viewport(ui, view_r, &mut vis, &resp, phone);
                            let panel_r = if phone {
                                Rect::from_min_max(
                                    egui::pos2(avail.left(), view_r.bottom() + 6.0),
                                    avail.max,
                                )
                            } else {
                                Rect::from_min_max(
                                    egui::pos2(avail.left() + 8.0, avail.top() + 8.0),
                                    egui::pos2(
                                        (avail.left() + 8.0 + 330.0).min(avail.right()),
                                        (avail.top() + 8.0 + 210.0).min(avail.bottom()),
                                    ),
                                )
                            };
                            ui.scope_builder(egui::UiBuilder::new().max_rect(panel_r), |ui| {
                                ui.set_clip_rect(panel_r.intersect(ui.clip_rect()));
                                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(
                                    ui,
                                    |ui| {
                                        three_panel(
                                            ui,
                                            draft_mut.as_deref_mut(),
                                            catalog.as_deref(),
                                            &mut vis,
                                            &mut hits,
                                            phone,
                                        )
                                    },
                                );
                            });
                        }
                        View::Cockpit => {
                            // #91 round 2: **duty-first, tower full-bleed.** The section is the whole
                            // centre - no control bar (the fan's speed lives in the fan's detail, the
                            // freeze/motion controls are the page's one nav row), the parts rail folded to
                            // its handle, the duty & site form a drawer. The answer card is the always-on
                            // overlay: in the section's own HUD slot where the section is wide enough for it,
                            // above the tower where it is not (a phone, and a window under
                            // `HUD_MIN_SECTION_W` - at 1024x768 the card used to be dropped altogether, and
                            // with it the duty inputs and the cold-water verdict).
                            let card_above = !scene::hud_fits(avail.width(), phone);
                            let mut card_bottom = avail.top();
                            if card_above {
                                if let Some(d) = draft_mut.as_deref_mut() {
                                    // Issue #81: the section keeps its agreed minimum height; the card takes
                                    // the room above it and scrolls inside what is left (the card is an
                                    // overlay, the section is the drawing).
                                    let max_h = scene::card_max_h(avail.height());
                                    let cr = Rect::from_min_size(
                                        avail.min,
                                        egui::vec2(avail.width(), max_h),
                                    );
                                    let mut used = cr;
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, true])
                                        .max_height(max_h)
                                        .show(ui, |ui| {
                                            used = crate::answer::answer_card(
                                                ui, cr, d, &run, &mut vis, &aux.duty, &mut hits,
                                                phone,
                                            );
                                        });
                                    card_bottom = used.bottom().min(cr.bottom()) + scene::CARD_GAP;
                                }
                            }
                            let scene_r = Rect::from_min_max(
                                egui::pos2(avail.left(), card_bottom),
                                avail.max,
                            );
                            scene_rect.set(scene_r, full.size(), phone);
                            let _ = ui.allocate_exact_size(scene_r.size(), Sense::hover());
                            let lay = draft_mut
                                .as_deref()
                                .map(|d| scene::layout(scene_r, Some(&d.0), phone));
                            // #91 round 2: on a phone the tap-detail is a bottom sheet over the tower's
                            // lower half - exactly where the plates are stacked. Nothing is painted beneath
                            // an opaque card, so with the sheet open the plates are not drawn. (The "no text
                            // sits on text" probe is what made this visible: the covered plates were still
                            // in the frame's string list, at the same y as the sheet's own rows.)
                            let gutter_plates = !(phone && vis.detail.is_some());
                            if let Some(d) = draft_mut.as_deref_mut() {
                                scene_overlay(
                                    ui,
                                    scene_r,
                                    catalog.as_deref(),
                                    d,
                                    &mut vis,
                                    &run,
                                    ctx,
                                    &mut hits,
                                    &mut aux.info,
                                    clock.t,
                                    gutter_plates,
                                    phone,
                                );
                            }
                            let hud_r = lay.as_ref().map(|l| l.hud).unwrap_or(Rect::NOTHING);
                            let mut answer_used = None;
                            if hud_r.is_positive() {
                                if let Some(d) = draft_mut.as_deref_mut() {
                                    answer_used = Some(crate::answer::answer_card(
                                        ui, hud_r, d, &run, &mut vis, &aux.duty, &mut hits, false,
                                    ));
                                }
                            }
                            // the tap-detail: a bay, the operating point or a zone, as a card under the
                            // answer card on a desktop and as a bottom sheet on a phone.
                            // the tap-detail hangs under the answer card's *used* rect (the card is only as
                            // tall as its rows), not under the full-height HUD slot.
                            let anchor = if let Some(used) = answer_used {
                                let below = used.bottom() + 8.0;
                                Rect::from_min_max(
                                    egui::pos2(used.left(), below),
                                    egui::pos2(
                                        used.right(),
                                        (screen.bottom() - 8.0).max(below + 140.0),
                                    ),
                                )
                            } else if hud_r.is_positive() {
                                let below = hud_r.top() + 240.0;
                                Rect::from_min_max(
                                    egui::pos2(hud_r.left(), below),
                                    egui::pos2(hud_r.right(), screen.bottom() - 8.0),
                                )
                            } else {
                                Rect::from_min_max(
                                    egui::pos2(
                                        scene_r.right() - 340.0,
                                        (card_bottom + 8.0).max(scene_r.top() + 8.0),
                                    ),
                                    scene_r.max,
                                )
                            };
                            crate::answer::detail_card(
                                ctx,
                                &mut vis,
                                draft_mut.as_deref_mut(),
                                &run,
                                &mut hits,
                                phone,
                                anchor,
                                screen,
                            );
                        }
                    }
                },
            );

            region(ui, footer_r, egui::vec2(14.0, 4.0), |ui| {
                footer(
                    ui,
                    &options,
                    &load,
                    &staged,
                    engine.get(),
                    &mut aux.info,
                    phone,
                )
            });
        });

    // ---- round 2, change A: the bay-tap picker. Drawn in its own foreground layer, one at a time,
    // dismissed by Esc, by a click outside, or by a drop.
    let centre_for_anchor = scene_rect.rect();
    picker_controls(
        ctx,
        &mut vis,
        catalog.as_deref(),
        draft_mut.as_deref_mut(),
        &mut hits,
        phone,
        screen,
        centre_for_anchor,
    );

    // ---- round 4, item 3: the parameter card. Its state is resolved from the hit rects this frame drew,
    // so a chip, a picker card and a fitted bay all work through one path.
    hover::update(ctx, &hits, draft_mut.as_deref(), &mut aux.hover, phone);
    if let Some(fields) = aux.fields.fields.as_ref() {
        hover::draw(
            ctx,
            &mut aux.hover,
            catalog.as_deref(),
            fields,
            &run,
            &anchor,
            &mut vis,
            &mut hits,
            draft_mut.as_deref(),
            screen,
        );
    }

    // ---- round 4, item 2: the custom-part form, drawn last (it is the authoring surface, so it sits over
    // everything), and only when a class is open.
    if let (Some(fields), Some(cat)) = (aux.fields.fields.as_ref(), catalog.as_deref_mut()) {
        form::draw(
            ctx,
            &mut aux.form,
            fields,
            cat,
            engine.get_mut(),
            &mut vis,
            &mut hits,
            screen,
            phone,
            options.host.public,
        );
    }
    // Issue #91: the notes drawer - every sentence the screen no longer paints.
    {
        let engine_name = engine
            .0
            .as_deref()
            .map(|e| crate::screens::kit::method(e.name()).to_string())
            .unwrap_or_else(|| "loading".to_string());
        crate::notes::draw(
            ctx,
            &mut vis,
            &mut hits,
            &options,
            &engine_name,
            &staged,
            &aux.duty,
            draft_mut.as_deref(),
            screen,
            header_r.bottom(),
            phone,
        );
    }
    // Issue #91: read back every text shape this pass painted (measurement only).
    aux.words.collect(ctx);
}

/// The picker's own keyboard, and its placement. Returns the bay rect it is anchored to (for the frames).
#[allow(clippy::too_many_arguments)]
fn picker_controls(
    ctx: &egui::Context,
    vis: &mut Visual,
    cat: Option<&Catalog>,
    mut draft: Option<&mut Draft>,
    hits: &mut HitMap,
    phone: bool,
    screen: Rect,
    centre: Rect,
) {
    let open_slot = vis.picker.as_ref().map(|p| p.slot);
    let rows_for = |cat: Option<&Catalog>, slot: Slot| -> Vec<PartRef> {
        cat.map(|c| state::picker_rows(c, slot)).unwrap_or_default()
    };

    // The bay's rect, so the picker can sit beside it (the layout is the same one the sprites use).
    let bay_rect = match (vis.view, draft.as_deref()) {
        // Issue #91: `centre` is the section's own rect this frame (the one the sprites were laid out in).
        (View::Cockpit, Some(d)) if centre.is_positive() => {
            let l = scene::layout(centre, Some(&d.0), phone);
            open_slot.and_then(|s| l.slots.iter().find(|(x, _)| *x == s).map(|(_, r)| *r))
        }
        _ => None,
    };

    // ---- the keyboard: Esc closes, arrows move, Enter picks; with no picker open, B focuses the next bay
    // and Enter opens its picker.
    ctx.input(|i| {
        let esc = i.key_pressed(egui::Key::Escape);
        let up = i.key_pressed(egui::Key::ArrowUp);
        let down = i.key_pressed(egui::Key::ArrowDown);
        let enter = i.key_pressed(egui::Key::Enter);
        if esc && vis.picker.is_some() {
            vis.picker = None;
            vis.flash = Some(Flash {
                ok: true,
                text: "picker closed (esc)".into(),
            });
        }
        if let Some(slot) = open_slot {
            let n = rows_for(cat, slot).len().max(1);
            if let Some(p) = vis.picker.as_mut() {
                if down {
                    p.row = (p.row + 1) % n;
                }
                if up {
                    p.row = (p.row + n - 1) % n;
                }
            }
            if enter {
                picker_command(vis, cat, draft.as_deref_mut(), "pick");
            }
        } else {
            if enter {
                if let Some(s) = vis.bay_focus {
                    open_picker(vis, s, true);
                }
            }
        }
    });

    let Some(slot) = vis.picker.as_ref().map(|p| p.slot) else {
        return;
    };
    let cat = match cat {
        Some(c) => c,
        None => return,
    };
    let rows = state::picker_rows(cat, slot);
    let layer = vis.picker.as_ref().map(|p| p.layer).unwrap_or(0);
    let title = picker_title(draft.as_deref().map(|d| &d.0), vis);

    // ---- placement
    vis.picker_sheet = phone;
    // Issue #135: on a phone the sheet is capped by the room the instrument actually leaves it -
    // `screen.top() + 60` down to `screen.bottom() - 6` - not by a 44 % share of the instrument. At
    // 390x844 the fraction gave the fill catalog (7 rows, ~500 px of cards) a 219 px viewport, so its
    // last four rows were laid out below the canvas: drawn nowhere, untappable, and the panel read as
    // clipped. The room is still a *cap* - a catalog that outgrows the screen scrolls inside it - and the
    // sheet hangs off the bottom edge, so a short list stays the small bottom sheet it was and a long one
    // grows upward instead of running off the screen.
    let room = (screen.bottom() - 6.0 - (screen.top() + 60.0)).max(120.0);
    let (pos, width, max_h) = if phone {
        (None, screen.width() - 12.0, room)
    } else {
        // No bay rect (the 3D view): anchor the card at the pointer, which is where the tap was.
        let bay = bay_rect.unwrap_or_else(|| {
            let p = ctx.pointer_latest_pos().unwrap_or(screen.center());
            Rect::from_min_size(p, egui::vec2(1.0, 1.0))
        });
        let w = 268.0_f32.min(screen.width() - 40.0);
        let x = if bay.right() + 10.0 + w < screen.right() - 8.0 {
            bay.right() + 10.0
        } else {
            (bay.left() - 10.0 - w).max(screen.left() + 8.0)
        };
        let y = bay
            .top()
            .max(screen.top() + 8.0)
            .min(screen.bottom() - 120.0);
        (
            Some(egui::pos2(x, y)),
            w,
            (screen.bottom() - y - 16.0).min(360.0),
        )
    };

    let mut picked: Option<PartRef> = None;
    let mut close = false;
    let area = match pos {
        Some(p) => egui::Area::new(egui::Id::new("viz.picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(p),
        // A phone: hang the sheet off the bottom of the viewport. An anchored area is sized by its
        // content, so the room cap above is what stops a long catalog - the sheet grows upward, never
        // off the canvas.
        None => egui::Area::new(egui::Id::new("viz.picker"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -6.0)),
    }
    .interactable(true)
    .show(ctx, |ui| {
        ui.set_max_width(width);
        ui.set_max_height(max_h);
        egui::Frame::new()
            .fill(t::PANEL_RAISED)
            .stroke(Stroke::new(1.0, t::PRIMARY))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} bay", slot.name()))
                            .size(12.0)
                            .color(t::INK)
                            .family(t::family_semi()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_sized(egui::vec2(22.0, 20.0), egui::Button::new("x"))
                            .on_hover_text("close (esc)")
                            .clicked()
                        {
                            close = true;
                        }
                        chip(ui, "picker", t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP);
                    });
                });
                ui.label(RichText::new(title.clone()).size(10.5).color(t::MUTED));
                ui.label(
                    RichText::new(
                        "pick = replace · drag a card into the bay · esc or click outside to close",
                    )
                    .size(9.5)
                    .color(t::MUTED),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height((max_h - 84.0).max(80.0))
                    .show(ui, |ui| {
                        for (i, part) in rows.iter().enumerate() {
                            if picker_row(
                                ui,
                                cat,
                                draft.as_deref(),
                                vis,
                                slot,
                                layer,
                                i,
                                part,
                                hits,
                            ) {
                                picked = Some(part.clone());
                            }
                        }
                    });
            });
    });
    let area_rect = area.response.rect;
    vis.picker_rect = [
        area_rect.min.x,
        area_rect.min.y,
        area_rect.width(),
        area_rect.height(),
    ];

    // A click outside dismisses (the pointer path, not only the keyboard).
    if ctx.input(|i| i.pointer.any_click())
        && !area_rect.contains(ctx.pointer_latest_pos().unwrap_or(egui::pos2(-99.0, -99.0)))
    {
        close = true;
    }
    if let Some(part) = picked {
        if let Some(p) = vis.picker.as_mut() {
            if let Some(idx) = rows.iter().position(|r| r.id == part.id) {
                p.row = idx;
            }
        }
        picker_command(vis, Some(cat), draft, "pick");
    }
    if close {
        vis.picker = None;
        vis.picker_rect = [0.0; 4];
        vis.flash = Some(Flash {
            ok: true,
            text: "picker closed".into(),
        });
    }
}

/// Is this pointer position inside the picker that is already open? A click there belongs to the picker,
/// never to the bay behind it.
fn picker_holds(vis: &Visual, pos: Option<egui::Pos2>) -> bool {
    let Some(p) = pos else { return false };
    let [x, y, w, h] = vis.picker_rect;
    w > 1.0 && h > 1.0 && p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
}

/// One row of the picker: the fixture card, the catalog's verdict, and `IN MACHINE` when it is the part
/// the bay already holds. Draggable into the bay (the drop path is the one the tray used).
#[allow(clippy::too_many_arguments)]
fn picker_row(
    ui: &mut egui::Ui,
    cat: &Catalog,
    draft: Option<&Draft>,
    vis: &mut Visual,
    slot: Slot,
    layer: usize,
    index: usize,
    part: &PartRef,
    hits: &mut HitMap,
) -> bool {
    let current = match (slot, draft) {
        (Slot::Fan, Some(d)) => d.0.fan.id == part.id,
        (Slot::Drift, Some(d)) => d.0.drift.id == part.id,
        (Slot::Nozzle, Some(d)) => d.0.nozzle.id == part.id,
        (Slot::Fill, Some(d)) => {
            d.0.fill_layers
                .get(layer)
                .map(|l| l.fill_id == part.id)
                .unwrap_or(false)
        }
        _ => false,
    };
    let verdict = draft
        .map(|d| check_drop(cat, &d.0, slot, part))
        .unwrap_or(Ok(()));
    let row_selected = vis.picker.as_ref().map(|p| p.row == index).unwrap_or(false);
    let mut clicked = false;
    let card_id = egui::Id::new(("picker", slot.slug(), part.id.as_str()));
    let inner = ui.dnd_drag_source(card_id, part.clone(), |ui| {
        let frame = if row_selected {
            t::chip_frame(t::PRIMARY_SOFT, t::PRIMARY)
        } else {
            t::chip_frame(t::PANEL, t::LINE_SOFT)
        };
        frame
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width().min(224.0));
                ui.horizontal(|ui| {
                    if cat.is_custom(part.class, &part.id) {
                        ui.label(
                            RichText::new("●")
                                .size(8.0)
                                .color(t::AMBER)
                                .family(t::family_semi()),
                        )
                        .on_hover_text("authored in this session");
                    }
                    ui.label(
                        RichText::new(&part.id)
                            .size(11.5)
                            .color(t::INK)
                            .family(t::family_mono_med()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if current {
                            chip(
                                ui,
                                "IN MACHINE",
                                t::OK,
                                t::OK_SOFT,
                                t::with_alpha(t::OK, 120),
                            );
                        }
                        match &verdict {
                            Ok(()) => {
                                chip(
                                    ui,
                                    "accepts",
                                    t::VALID,
                                    t::OK_SOFT,
                                    t::with_alpha(t::VALID, 120),
                                );
                            }
                            Err(_) => {
                                chip(
                                    ui,
                                    "refuses",
                                    t::INVALID,
                                    t::DANGER_SOFT,
                                    t::with_alpha(t::INVALID, 120),
                                );
                            }
                        }
                    });
                });
                ui.label(
                    RichText::new(state::part_spec_short(cat, part))
                        .size(9.5)
                        .color(t::MUTED),
                );
                if let Err(reason) = &verdict {
                    ui.label(RichText::new(reason).size(9.5).color(t::DANGER));
                }
            })
            .response
    });
    // Issue #135: `dnd_drag_source` senses **drag only**, so `inner.response.clicked()` below could never
    // be true and a tap on a card did nothing. The click sense goes on the drag source's own id, which the
    // widget table unions with the drag sense: a tap picks the row, a drag still carries the card.
    let resp = ui.interact(inner.response.rect, card_id, Sense::click_and_drag());
    if resp.clicked() {
        clicked = true;
    }
    let r = resp.rect;
    hits.0.push((
        format!("picker:{}:{}", slot.slug(), part.id),
        [r.min.x, r.min.y, r.width(), r.height()],
    ));
    // A hovered card becomes the selected part, so the strip describes the row under the pointer.
    if resp.hovered() {
        vis.selected_part = Some(part.clone());
        vis.selected_slot = slot;
    }
    // A card that has been *picked up* is carried: the app says so while the pointer is still on the card,
    // which is what the strip and the frame's `data-drag` show. The bay's own code overwrites this with the
    // bay it is over once the pointer gets there, so this only writes while the payload is not over a bay.
    if resp.dragged() && vis.drag.as_ref().map(|d| d.over.is_none()).unwrap_or(true) {
        vis.drag = Some(Drag {
            part: part.clone(),
            over: None,
            verdict: Ok(()),
            staged: false,
        });
        vis.selected_part = Some(part.clone());
        vis.selected_slot = slot;
    }
    clicked
}

// ==================================================================================== the parts rail
//
// Round 3, item 1. Round 1 had a wall of cards; round 2 removed it altogether; round 3 brings back a
// *narrow* one: four sections of one-line chips (`AX-500 · 19.6 m2`), the fitted record marked, the whole
// column collapsible to a 28 px strip of four icons. Dragging a chip onto its bay is the primary path; the
// round-2 bay-tap picker stays as the secondary path (and is the only path on a phone, as a bottom sheet).

/// One line of a rail chip: the record's own headline number, as short as it can honestly be.
fn rail_spec(cat: &Catalog, class: Class, id: &str) -> String {
    match class {
        Class::Fan => cat
            .fan(id)
            .map(|f| format!("{} m²", t::num::sig(f.stack_area_m2, 3)))
            .unwrap_or_default(),
        Class::Drift => cat
            .drift(id)
            .map(|d| format!("max {} °C", t::num::temp(d.max_water_temperature_c)))
            .unwrap_or_default(),
        Class::Fill => cat
            .fill(id)
            .map(|f| {
                let g = f.record.geometry.trim();
                let short = g.split_whitespace().next().unwrap_or(g);
                short.to_string()
            })
            .unwrap_or_default(),
        Class::Nozzle => cat
            .nozzle(id)
            .map(|n| format!("{:.0} mm", n.orifice_diameter_m * 1000.0))
            .unwrap_or_default(),
    }
}

/// Is this record the one fitted in the machine - and if so, which layer(s)?
fn fitted_in(draft: Option<&Draft>, class: Class, id: &str) -> Option<String> {
    let d = draft?;
    match class {
        Class::Fan => (d.0.fan.id == id).then(|| d.0.fan.id.clone()),
        Class::Drift => (d.0.drift.id == id).then(|| d.0.drift.id.clone()),
        Class::Nozzle => (d.0.nozzle.id == id).then(|| d.0.nozzle.id.clone()),
        Class::Fill => {
            let layers: Vec<String> =
                d.0.fill_layers
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.fill_id == id)
                    .map(|(i, _)| format!("{}", i + 1))
                    .collect();
            (!layers.is_empty()).then(|| format!("layer {}", layers.join("+")))
        }
    }
}

/// The rail: a caret row and four sections of chips (open), or four icons and an expand caret (collapsed).
#[allow(clippy::too_many_arguments)]
fn parts_rail(
    ui: &mut egui::Ui,
    cat: Option<&Catalog>,
    draft: Option<&Draft>,
    vis: &mut Visual,
    hits: &mut HitMap,
    form: &mut form::CustomForm,
    fields: Option<&drafthouse_cockpit_seams::custom::Fields>,
) {
    if !vis.rail_open {
        // ---- the collapsed strip: 28 px, four icons, one caret. The icons are the four part classes.
        let (cr, cresp) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), Sense::click());
        hits.0.push((
            "rail:caret".to_string(),
            [cr.min.x, cr.min.y, cr.width(), cr.height()],
        ));
        ui.painter_at(cr).text(
            cr.center(),
            Align2::CENTER_CENTER,
            "»",
            FontId::new(12.0, t::family_semi()),
            t::MUTED,
        );
        if cresp.clicked() {
            vis.rail_open = true;
            vis.flash = Some(Flash {
                ok: true,
                text: "parts rail open".into(),
            });
        }
        ui.add_space(2.0);
        for class in Class::ALL {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
            hits.0.push((
                format!("rail:icon:{}", class.slug()),
                [r.min.x, r.min.y, r.width(), r.height()],
            ));
            rail_icon(ui, r, class, vis.selected_slot == class.slot());
            if resp
                .on_hover_text(format!("{} — open the rail", class.tray_title()))
                .clicked()
            {
                vis.rail_open = true;
                vis.selected_slot = class.slot();
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!("parts rail open · {}", class.tray_title()),
                });
            }
            ui.add_space(2.0);
        }
        return;
    }

    // ---- the open rail
    ui.horizontal(|ui| {
        ui.label(t::eyebrow("parts"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), Sense::click());
            hits.0.push((
                "rail:caret".to_string(),
                [r.min.x, r.min.y, r.width(), r.height()],
            ));
            ui.painter_at(r).text(
                r.center(),
                Align2::CENTER_CENTER,
                "«",
                FontId::new(12.0, t::family_semi()),
                t::MUTED,
            );
            if resp.on_hover_text("collapse to the icon strip").clicked() {
                vis.rail_open = false;
                vis.flash = Some(Flash {
                    ok: true,
                    text: "parts rail collapsed".into(),
                });
            }
        });
    });
    ui.add_space(3.0);
    let Some(cat) = cat else {
        ui.label(RichText::new("catalog loading…").size(10.0).color(t::MUTED));
        return;
    };
    for class in Class::ALL {
        ui.label(t::eyebrow(class.tray_title()));
        for id in state::tray_ids(cat, class) {
            rail_chip(ui, cat, draft, vis, hits, class, &id);
        }
        // Round 4, item 2: the section's own authoring chip. It opens the form for THIS class, whose fields
        // are the class's field list - the user enters values and nothing else.
        form::add_chip(ui, class, vis, form, hits, fields);
        ui.add_space(5.0);
    }
}

/// One chip: the record id and its headline number on one line, draggable into its bay. The fitted record
/// carries the accent frame and a dot - the same "in machine" idea the picker's badge states in words.
fn rail_chip(
    ui: &mut egui::Ui,
    cat: &Catalog,
    draft: Option<&Draft>,
    vis: &mut Visual,
    hits: &mut HitMap,
    class: Class,
    id: &str,
) {
    let part = PartRef::new(class, id);
    let fitted = fitted_in(draft, class, id);
    let custom = cat.is_custom(class, id);
    // A record the form just saved: its chip is at the end of the section and may be under the fold, so it
    // asks to be brought into view exactly once (and then the request is consumed).
    let scroll_here = vis.scroll_to.as_deref() == Some(part.slug().as_str());
    let selected = vis
        .selected_part
        .as_ref()
        .map(|p| p.class == class && p.id == id)
        .unwrap_or(false);
    let frame = if fitted.is_some() {
        t::chip_frame(t::PRIMARY_SOFT, t::PRIMARY)
    } else if custom {
        t::chip_frame(t::PANEL, t::with_alpha(t::AMBER, 150))
    } else if selected {
        t::chip_frame(t::PANEL_RAISED, t::SLOT)
    } else {
        t::chip_frame(t::PANEL, t::LINE_SOFT)
    };
    let chip_id = egui::Id::new(("rail", class.slug(), id));
    let inner = ui.dnd_drag_source(chip_id, part.clone(), |ui| {
        frame
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width().max(64.0));
                ui.horizontal(|ui| {
                    if let Some(where_) = &fitted {
                        ui.label(
                            RichText::new("●")
                                .size(8.0)
                                .color(t::PRIMARY)
                                .family(t::family_semi()),
                        )
                        .on_hover_text(format!("{id} is fitted: {where_}"));
                    }
                    if custom {
                        // The `custom` mark: an amber dot, and the word, so the state is not colour-only.
                        ui.label(
                            RichText::new("●")
                                .size(8.0)
                                .color(t::AMBER)
                                .family(t::family_semi()),
                        )
                        .on_hover_text(format!(
                            "{id} was authored in this session - it lives in the app state only"
                        ));
                        ui.label(
                            RichText::new("custom")
                                .size(8.5)
                                .color(t::AMBER)
                                .family(t::family_semi()),
                        );
                    }
                    ui.label(
                        RichText::new(id)
                            .size(10.5)
                            .color(t::INK)
                            .family(t::family_mono_med()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(rail_spec(cat, class, id))
                                .size(9.0)
                                .color(t::MUTED)
                                .family(t::family_mono_med()),
                        );
                    });
                });
            })
            .response
    });
    // Issue #135: `dnd_drag_source` senses **drag only**, so `inner.response.clicked()` at the foot of this
    // function could never be true and a tap on a chip was ignored. The click sense is added on the drag
    // source's own id - the widget table unions it with the drag sense - so the tap selects the chip and the
    // drag-into-a-bay path below is untouched.
    let resp = ui.interact(inner.response.rect, chip_id, Sense::click_and_drag());
    let r = resp.rect;
    hits.0.push((
        format!("rail:chip:{}:{}", class.slug(), id),
        [r.min.x, r.min.y, r.width(), r.height()],
    ));
    if scroll_here {
        // The record the form just saved: bring its chip into view once, then forget the request.
        resp.scroll_to_me(Some(egui::Align::Center));
        vis.scroll_to = None;
    }
    if resp.hovered() {
        vis.selected_part = Some(part.clone());
        vis.selected_slot = class.slot();
    }
    // A held chip is a drag, exactly like a picker card: the bays light up before release (item 2).
    if resp.dragged() && vis.drag.as_ref().map(|d| d.over.is_none()).unwrap_or(true) {
        vis.drag = Some(Drag {
            part: part.clone(),
            over: None,
            verdict: Ok(()),
            staged: false,
        });
        vis.selected_part = Some(part.clone());
        vis.selected_slot = class.slot();
    }
    if resp.clicked() {
        vis.selected_part = Some(part.clone());
        vis.selected_slot = class.slot();
        vis.flash = Some(Flash {
            ok: true,
            text: format!(
                "{id} selected - drag it onto the {} bay, or tap the bay",
                class.name()
            ),
        });
    }
}

/// The four icons the collapsed rail shows. Drawn, not typed: a fan disc, a drift chevron bank, a fill
/// stack, a nozzle row. Each one is a miniature of the thing it holds.
fn rail_icon(ui: &mut egui::Ui, r: Rect, class: Class, active: bool) {
    let p = ui.painter_at(r);
    let ink = if active { t::PRIMARY } else { t::INK_2 };
    let c = r.center();
    let s = r.width().min(r.height()) * 0.5 - 4.0;
    match class {
        Class::Fan => {
            p.circle_stroke(c, s, Stroke::new(1.2, ink));
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::TAU / 3.0 + 0.4;
                p.line_segment(
                    [c, egui::pos2(c.x + s * a.cos(), c.y + s * a.sin())],
                    Stroke::new(1.1, ink),
                );
            }
            p.circle_filled(c, 1.6, ink);
        }
        Class::Drift => {
            for k in 0..3 {
                let y = c.y - s + k as f32 * (s * 0.8);
                p.line_segment(
                    [egui::pos2(c.x - s, y), egui::pos2(c.x, y + s * 0.5)],
                    Stroke::new(1.2, ink),
                );
                p.line_segment(
                    [egui::pos2(c.x, y + s * 0.5), egui::pos2(c.x + s, y)],
                    Stroke::new(1.2, ink),
                );
            }
        }
        Class::Fill => {
            for k in 0..3 {
                let y = c.y - s + k as f32 * (s * 0.75);
                p.line_segment(
                    [egui::pos2(c.x - s, y), egui::pos2(c.x + s, y)],
                    Stroke::new(1.6, ink),
                );
            }
        }
        Class::Nozzle => {
            p.line_segment(
                [
                    egui::pos2(c.x - s, c.y - s * 0.7),
                    egui::pos2(c.x + s, c.y - s * 0.7),
                ],
                Stroke::new(1.4, ink),
            );
            for k in 0..3 {
                let x = c.x - s * 0.7 + k as f32 * (s * 0.7);
                p.circle_filled(egui::pos2(x, c.y - s * 0.7), 1.5, ink);
                p.line_segment(
                    [
                        egui::pos2(x, c.y - s * 0.7),
                        egui::pos2(x - s * 0.35, c.y + s * 0.8),
                    ],
                    Stroke::new(1.0, ink),
                );
                p.line_segment(
                    [
                        egui::pos2(x, c.y - s * 0.7),
                        egui::pos2(x + s * 0.35, c.y + s * 0.8),
                    ],
                    Stroke::new(1.0, ink),
                );
            }
        }
    }
}

/// One region of the instrument: an explicitly placed child `Ui` with a margin and a clip rect.
#[allow(clippy::too_many_arguments)]
fn region<R>(
    ui: &mut egui::Ui,
    r: Rect,
    margin: egui::Vec2,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = r.shrink2(margin);
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.set_clip_rect(inner.intersect(ui.clip_rect()));
        ui.set_max_width(inner.width());
        add(ui)
    })
    .inner
}

/// `region`, with the width **fixed** to the rect: the round-5 measurement caught a single row (the water
/// quality class row, which wanted 418 px in a 376 px column) growing the layout for everything below it, so
/// every card under it was drawn 18 px wider and 30 px past the window's right edge. A row that cannot fit now
/// overflows *visibly* and is reported by the clip probe (`crate::clip`) instead of silently re-laying out the
/// column.
fn region_exact<R>(
    ui: &mut egui::Ui,
    r: Rect,
    margin: egui::Vec2,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = r.shrink2(margin);
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.set_clip_rect(inner.intersect(ui.clip_rect()));
        ui.set_min_width(inner.width());
        ui.set_max_width(inner.width());
        add(ui)
    })
    .inner
}

fn chip(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32, line: Color32) -> egui::Response {
    // A chip is a Frame around a label that does not wrap, so a long message (a drop verdict, a flash after
    // an edit, a carried part plus a verdict on a 390 px strip) grows the row it sits in - and every layout
    // below it follows it out of the column. Round 5's clip probe caught exactly that: with the picker sheet
    // open and a part carried, the phone's strip wanted 405 px, the stack grew to 549 px and eleven rows of
    // the read-out and the fill stack were drawn past the column. The *string* is cut here, before the frame.
    let room = (ui.available_width() - 14.0).max(24.0);
    let font = egui::FontId::new(10.0, t::family_semi());
    let shown = if measure_text(ui, text, 10.0, t::family_semi()) > room {
        let painter = ui.painter().clone();
        truncate_to(&painter, text, &font, room)
    } else {
        text.to_owned()
    };
    t::chip_frame(bg, line)
        .show(ui, |ui| {
            ui.label(
                RichText::new(shown)
                    .size(10.0)
                    .color(fg)
                    .family(t::family_semi())
                    .extra_letter_spacing(0.4),
            );
        })
        .response
}

/// A label + value row, right-aligned value in mono so columns line up while numbers move.
fn kv(ui: &mut egui::Ui, label: &str, value: &str, unit: &str, hot: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.5).color(t::MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !unit.is_empty() {
                ui.label(
                    RichText::new(unit)
                        .size(10.0)
                        .color(t::MUTED)
                        .family(t::family_mono_med()),
                );
            }
            ui.label(
                RichText::new(value)
                    .size(12.5)
                    .color(if hot { t::PRIMARY } else { t::INK })
                    .family(t::family_mono_med()),
            );
        });
    });
}

/// Record a control's screen rect for the harness, and hand the response back.
fn hit(hits: &mut HitMap, key: &str, resp: egui::Response) -> egui::Response {
    let r = resp.rect;
    hits.0
        .push((key.to_string(), [r.min.x, r.min.y, r.width(), r.height()]));
    resp
}

/// Issue #136: the heights a layer's depth control offers - its **own fill's** height spec, never the
/// tower's `fillDepthOptionsM` (that is the single-fill selector's total-stack list). A fill with no spec
/// (a session-authored custom record) offers only the depth the layer already has.
fn layer_depth_menu(cat: Option<&Catalog>, fill_id: &str, depth_m: f64) -> (Vec<f64>, String) {
    match cat.and_then(|c| c.depth_spec(fill_id)) {
        Some(spec) => (spec.options(), format!("{fill_id} heights")),
        None => (vec![depth_m], format!("{fill_id}: no height spec")),
    }
}

/// One layer's depth control: a menu of the layer's own fill's heights. Each offered height publishes
/// `layer:<i>:depth:<d>` to the hit map, the menu button `layer:<i>:depth`.
fn depth_row(
    ui: &mut egui::Ui,
    cat: Option<&Catalog>,
    input: &mut EngineInput,
    i: usize,
    hits: &mut HitMap,
) {
    let Some(layer) = input.fill_layers.get(i) else {
        return;
    };
    let (options_m, note) = layer_depth_menu(cat, &layer.fill_id, layer.depth_m);
    let mut depth_now = layer.depth_m;
    ui.horizontal(|ui| {
        ui.label(RichText::new("depth").size(10.0).color(t::MUTED));
        let combo = egui::ComboBox::from_id_salt(("depth", i))
            .selected_text(format!("{depth_now:.2} m"))
            .width(84.0)
            .show_ui(ui, |ui| {
                for o in options_m.iter() {
                    let resp = ui.selectable_value(&mut depth_now, *o, format!("{o:.2} m"));
                    hit(hits, &format!("layer:{i}:depth:{o:.2}"), resp);
                }
            });
        hit(hits, &format!("layer:{i}:depth"), combo.response);
        ui.label(RichText::new(note).size(9.5).color(t::MUTED));
    });
    input.fill_layers[i].depth_m = depth_now;
}

/// The engine's own provenance string is a path into the vendor engine repository; the instrument shows the
/// file it names, not the whole path.
fn engine_label(engine: &str) -> String {
    let f = engine.split_whitespace().next().unwrap_or(engine);
    f.rsplit('/').next().unwrap_or(f).to_string()
}

/// The engine-mode chip: what the header/strip/dock say about *which engine is running*, driven by the
/// engine the slot actually holds (or, before a run exists, the build's own feature). The round-5
/// strings were hardcoded to the fixture replay; issue #58's default build runs the real engine, so a
/// chip that said "preview" under it was a label contradicting the served state.
///
/// Returns the chip text and whether it wears the amber preview styling: the real engine's chip is
/// present tense (`real engine · computed`) in the primary colour, the fixture replay keeps its honest
/// `fixture-driven preview` amber, and an unloaded engine says so in muted. The mandated copy, the
/// synthetic/not-vendor labels and the PUBLIC/INTERNAL chips are not this chip and are untouched.
fn engine_mode(engine: Option<&dyn Engine>) -> (&'static str, bool) {
    match engine {
        Some(e) => {
            if engine_label(e.name()) == "RealEngine" {
                ("calculated", false)
            } else {
                ("recorded replay", true)
            }
        }
        // No engine in the slot: the build's default selection is the truth available (a frame is
        // never drawn before the slot is filled, but a default build must not claim "real" either
        // when its features exclude it).
        None => {
            if cfg!(feature = "real-engine") {
                ("calculated", false)
            } else if cfg!(feature = "fixture-engine") {
                ("recorded replay", true)
            } else {
                ("not available", false)
            }
        }
    }
}

fn engine_mode_chip(ui: &mut egui::Ui, engine: Option<&dyn Engine>) -> egui::Response {
    let (text, preview) = engine_mode(engine);
    if preview {
        chip(
            ui,
            text,
            t::AMBER,
            t::AMBER_SOFT,
            t::with_alpha(t::AMBER, 120),
        )
    } else {
        chip(ui, text, t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP)
    }
}

/// The engine's zone label, without a parenthesised suffix - the rail has one narrow column.
/// Round 5: measure a text unit with the same font the frame will draw it with, so a layout decision ("does
/// this row need two lines?") is made on the same shaping the painter uses instead of a character count.
fn measure_text(ui: &egui::Ui, text: &str, size: f32, family: egui::FontFamily) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::new(size, family), t::INK)
        .size()
        .x
}

/// Round 5: how many pairs of rects overlap (used by the status strip and the section's label plates - a
/// layout that overlaps is a layout that failed, and the frame reads the count from the app rather than from
/// a picture).
fn overlaps(rects: &[Rect]) -> u32 {
    let mut n = 0;
    for i in 0..rects.len() {
        for j in (i + 1)..rects.len() {
            let x = rects[i].intersect(rects[j]);
            if x.width() > 0.5 && x.height() > 0.5 {
                n += 1;
            }
        }
    }
    n
}

fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn accent_of(b: &cockpit::host::Branding) -> Color32 {
    b.accent
        .as_deref()
        .and_then(parse_hex)
        .unwrap_or(t::PRIMARY)
}

// ============================================================================================ header

/// Issue #91: **the validation badge** - the one place the screen states what these numbers are. It carries
/// the host's decision-12 label verbatim (`cockpit::host::PUBLIC_LABEL` / `INTERNAL_LABEL`, never shortened)
/// behind a painted (i): a click, the `I` key or the control bar's `notes` button opens the notes drawer,
/// where every sentence the screen used to paint now lives (`crate::notes`).
fn validation_badge(
    ui: &mut egui::Ui,
    options: &StartOptions,
    vis: &mut Visual,
    hits: &mut HitMap,
    wrap: bool,
) -> egui::Response {
    let notice = options.host.notice();
    let (fg, bg, line) = if notice.public {
        (t::INK_2, t::with_alpha(t::PANEL_RAISED, 235), t::LINE)
    } else {
        (t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP)
    };
    let open = vis.notes_open;
    let frame = egui::Frame::new()
        .fill(if open { t::PRIMARY_SOFT } else { bg })
        .stroke(Stroke::new(1.0, if open { t::PRIMARY } else { line }))
        .corner_radius(egui::CornerRadius::same(13))
        .inner_margin(egui::Margin::symmetric(9, 4));
    let shown = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (ir, _) = ui.allocate_exact_size(egui::vec2(15.0, 15.0), Sense::hover());
            ui.painter()
                .circle_stroke(ir.center(), 6.6, Stroke::new(1.3, t::PRIMARY));
            ui.painter().text(
                ir.center() + egui::vec2(0.0, 0.5),
                Align2::CENTER_CENTER,
                "i",
                FontId::new(10.5, t::family_semi()),
                t::PRIMARY,
            );
            ui.label(
                RichText::new(options.host.host_name())
                    .size(9.5)
                    .color(t::INK)
                    .family(t::family_semi()),
            );
            let label = egui::Label::new(
                RichText::new(&notice.text)
                    .size(10.0)
                    .color(fg)
                    .family(t::family_semi()),
            );
            ui.add(if wrap { label.wrap() } else { label });
        });
    });
    let resp = shown.response.interact(Sense::click());
    let resp = hit(hits, "ctl:notes", resp);
    if resp.clicked() {
        vis.notes_open = !vis.notes_open;
        if vis.notes_open {
            vis.panel_open = false;
        }
    }
    resp.on_hover_text("notes, validation and keyboard (I)")
}

/// Issue #91: the header is one row on a desktop - mark, name, the view tabs, and the validation badge -
/// and three short rows on a phone (name + notes button, the badge, the tabs at 44 px). The sub-line, the
/// required-copy row and the "no CFD ..." row are gone from the screen; they are in the notes drawer.
fn header(
    ui: &mut egui::Ui,
    options: &StartOptions,
    vis: &mut Visual,
    load: &Load,
    engine: Option<&(dyn Engine + '_)>,
    phone: bool,
    hits: &mut HitMap,
) {
    let host = &options.host;
    let b = host.branding.clone().unwrap_or_default();
    let accent = accent_of(&b);
    let mark = if b.mark.is_empty() {
        "SD".to_string()
    } else {
        b.mark.clone()
    };
    let name = if b.name.is_empty() {
        "Synergy Drafthouse".to_string()
    } else {
        b.name.clone()
    };
    let mark_box = |ui: &mut egui::Ui| {
        let (mr, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::hover());
        ui.painter()
            .rect_filled(mr, egui::CornerRadius::same(6), t::with_alpha(accent, 46));
        ui.painter().rect_stroke(
            mr,
            egui::CornerRadius::same(6),
            Stroke::new(1.0, accent),
            StrokeKind::Inside,
        );
        ui.painter().text(
            mr.center(),
            Align2::CENTER_CENTER,
            &mark,
            FontId::new(11.5, t::family_semi()),
            accent,
        );
    };
    let tabs = |ui: &mut egui::Ui, vis: &mut Visual, hits: &mut HitMap, h: f32| {
        for v in View::ALL.iter().copied() {
            let sel = vis.view == v;
            let text = RichText::new(v.name())
                .size(12.5)
                .color(if sel { t::INK } else { t::MUTED })
                .family(t::family_semi());
            let r = ui.add(
                egui::Button::selectable(sel, text)
                    .min_size(egui::vec2(0.0, h))
                    .corner_radius(egui::CornerRadius::same(6)),
            );
            if sel {
                // The active tab carries the accent underline - a selected state that reads in a still frame.
                ui.painter().line_segment(
                    [
                        egui::pos2(r.rect.left() + 6.0, r.rect.bottom() - 1.0),
                        egui::pos2(r.rect.right() - 6.0, r.rect.bottom() - 1.0),
                    ],
                    Stroke::new(2.0, t::PRIMARY),
                );
            }
            if hit(hits, &format!("ctl:view:{}", v.slug()), r).clicked() {
                vis.view = v;
            }
        }
    };

    if phone {
        // Row 1: the mark, the name, and a 44 px notes button (the badge below opens the same drawer).
        ui.horizontal(|ui| {
            mark_box(ui);
            ui.add(
                egui::Label::new(
                    RichText::new(&name)
                        .size(14.0)
                        .color(t::INK)
                        .family(t::family_semi()),
                )
                .truncate(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if load.failure.is_some() {
                    chip(
                        ui,
                        "DATA FAILED",
                        t::DANGER,
                        t::DANGER_SOFT,
                        t::with_alpha(t::DANGER, 140),
                    );
                }
            });
        });
        ui.add_space(2.0);
        // Row 2: the badge, wrapped - the decision-12 label is never truncated.
        validation_badge(ui, options, vis, hits, true);
        ui.add_space(2.0);
        // Row 3: the view tabs, 44 px tall.
        ui.horizontal(|ui| tabs(ui, vis, hits, 44.0));
        return;
    }

    ui.horizontal(|ui| {
        mark_box(ui);
        ui.label(t::strong(&name));
        ui.add_space(18.0);
        tabs(ui, vis, hits, 30.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            validation_badge(ui, options, vis, hits, false);
            // Issue #137: one badge. The method and the catalog id are notes-drawer facts; the header only
            // flags the one state an engineer must not miss - a recorded replay instead of a calculation.
            if engine_mode(engine).1 {
                engine_mode_chip(ui, engine);
            }
            let _ = &host.catalog_label;
            if load.failure.is_some() {
                chip(
                    ui,
                    "DATA FAILED",
                    t::DANGER,
                    t::DANGER_SOFT,
                    t::with_alpha(t::DANGER, 140),
                );
            }
        });
    });
}

fn footer(
    ui: &mut egui::Ui,
    options: &StartOptions,
    load: &Load,
    staged: &StagedLog,
    engine: Option<&(dyn Engine + '_)>,
    info: &mut crate::clip::LayoutInfo,
    phone: bool,
) {
    let notice = options.host.notice();
    if phone {
        // The mandated notice is a full sentence: it gets its own wrapped line rather than a chip that
        // runs off a 390px screen.
        let (color, bg) = if notice.public {
            (t::MUTED, t::BG)
        } else {
            (t::PRIMARY, t::PRIMARY_SOFT)
        };
        t::chip_frame(bg, t::with_alpha(color, 110)).show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(notice.text)
                        .size(10.0)
                        .color(color)
                        .family(t::family_semi()),
                )
                .wrap(),
            );
        });
        ui.horizontal(|ui| {
            engine_mode_chip(ui, engine);
        });
        return;
    }
    // Round 5, item 4: the status strip is one bounded row. The engine note and the mandated copy are
    // measured first and reserved; the `staged:` string - which grows with every staged key - gets exactly
    // what is left and is truncated with an ellipsis when it does not fit (`Label::truncate`). Round 4 drew
    // it at its natural width, so a long staged log ran under the two sentences that follow it. The row also
    // reports its own item rects, so a frame can read the overlap count instead of eyeballing a picture.
    //
    // Issue #58: the note names the engine this build actually selected, in the engine's own words
    // (`Engine::name`, shortened to its first word like the read-out's provenance line). It used to
    // name the baseline's `FixtureEngine` unconditionally, which the real engine made untrue.
    // Issue #137: the strip names the method in engineering words, not the implementation.
    let engine_note = match engine {
        Some(e) => format!("method: {}", crate::screens::kit::method(e.name())),
        None => "method: loading".to_string(),
    };
    let staged_text = if load.failure.is_none() && !staged.0.is_empty() {
        Some(format!("staged: {}", staged.0.join(" · ")))
    } else {
        None
    };
    info.staged_text = staged_text.clone().unwrap_or_default();
    info.staged_truncated = false;
    let mut items: Vec<Rect> = Vec::new();
    // ---- row 1: the mandated notice, the no-physics chip, and the staged log in the room that is left -
    // truncated with an ellipsis (`Label::truncate`) rather than drawn under the sentences on row 2.
    ui.horizontal(|ui| {
        let (color, bg) = if notice.public {
            (t::MUTED, t::BG)
        } else {
            (t::PRIMARY, t::PRIMARY_SOFT)
        };
        let notice_chip = t::chip_frame(bg, t::with_alpha(color, 110)).show(ui, |ui| {
            ui.label(
                RichText::new(notice.text)
                    .size(10.0)
                    .color(color)
                    .family(t::family_semi()),
            );
        });
        items.push(notice_chip.response.rect);

        if let Some(text) = staged_text.as_ref() {
            let room = (ui.available_width() - 12.0).max(72.0);
            let truncated = measure_text(ui, text, 10.0, egui::FontFamily::Monospace) > room;
            let r = ui
                .allocate_ui_with_layout(
                    egui::vec2(room, 17.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(text)
                                    .size(10.0)
                                    .color(t::AMBER)
                                    .family(egui::FontFamily::Monospace),
                            )
                            .truncate(),
                        );
                    },
                )
                .response
                .rect;
            items.push(r);
            info.staged_truncated = truncated;
        }
    });
    // ---- row 2: the engine note and the mandated copy, each with the row to itself if it needs it.
    ui.horizontal(|ui| {
        items.push(
            ui.label(
                RichText::new(engine_note.as_str())
                    .size(9.5)
                    .color(t::MUTED),
            )
            .rect,
        );
    });
    info.footer_overlaps = overlaps(&items);
}

fn loading_state(ui: &mut egui::Ui, load: &Load) {
    t::card().show(ui, |ui| {
        ui.label(t::heading(if load.failure.is_some() { "Calculation unavailable" } else { "Loading the design data" }));
        ui.label(t::body(if load.failure.is_some() {
            "The design data did not load, so the instrument shows nothing rather than something invented. Nothing is estimated in its place."
        } else {
            "Loading the design data and preparing the calculation. The tower appears when it is ready."
        }));
        if let Some(f) = &load.failure {
            ui.label(RichText::new(f).size(11.0).color(t::DANGER).family(egui::FontFamily::Monospace));
        }
    });
}

/// The 3D viewport's own interaction: orbit on drag, zoom on the wheel, and a tap that picks.
#[cfg(feature = "three-d")]
fn three_viewport(
    ui: &mut egui::Ui,
    rect: Rect,
    vis: &mut Visual,
    resp: &egui::Response,
    phone: bool,
) {
    // Orbit (drag) and zoom (wheel / pinch) - the camera is state, so a frame can be reproduced from a URL.
    if resp.dragged() {
        let d = resp.drag_delta();
        vis.cam_yaw = (vis.cam_yaw - d.x * 0.4).rem_euclid(360.0);
        vis.cam_pitch =
            (vis.cam_pitch + d.y * 0.25).clamp(m::CAM_PITCH_RANGE.0, m::CAM_PITCH_RANGE.1);
    }
    let zoom = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta() * 40.0 - 40.0);
    if resp.hovered() && zoom.abs() > 0.01 {
        vis.cam_dist =
            (vis.cam_dist - zoom * 0.004).clamp(m::CAM_DIST_RANGE.0, m::CAM_DIST_RANGE.1);
    }
    // A tap without a drag picks: the 3D crate resolves it (a cell selects the cut, an exposed component
    // opens its picker). Published as a canvas-space point for the frame to prove.
    if resp.clicked() {
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            // viewport-local points: the 3D crate turns them into a ray
            vis.three_click = Some([
                p.x - rect.left(),
                p.y - rect.top(),
                rect.width(),
                rect.height(),
            ]);
        }
    }
    let p = ui.painter_at(rect);
    p.text(
        egui::pos2(rect.left() + 8.0, rect.bottom() - 6.0),
        Align2::LEFT_BOTTOM,
        if phone {
            "orbit: drag · zoom: pinch".to_string()
        } else {
            "orbit: drag · zoom: wheel".to_string()
        },
        FontId::new(9.5, t::family_semi()),
        t::with_alpha(t::MUTED, 220),
    );
}

/// The 3D view's control panel: the cell count, the cutaway, the focus cell and the camera - plus the
/// numbers the parametric tower is built from, all read off the fixture records.
#[cfg(feature = "three-d")]
fn three_panel(
    ui: &mut egui::Ui,
    draft: Option<&mut Draft>,
    cat: Option<&Catalog>,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    let _ = cat;
    let plan_m = state::cell_plan_m(draft.as_deref().map(|d| &d.0));
    let stack_d = state::stack_diameter_m(draft.as_deref().map(|d| &d.0));
    let stack_h = m::stack_height_m(stack_d);
    let depths: Vec<String> = draft
        .as_deref()
        .map(|d| {
            d.0.fill_layers
                .iter()
                .map(|l| format!("{} {:.2} m", l.fill_id, l.depth_m))
                .collect()
        })
        .unwrap_or_default();

    t::card_flat().show(ui, |ui| {
        ui.label(t::eyebrow("3D tower — parametric build view"));
        if phone {
            ui.label(
                RichText::new("geometry is parametric and illustrative — not vendor CAD")
                    .size(10.0)
                    .color(t::AMBER),
            );
        }
        ui.horizontal(|ui| {
            ui.label(t::label("cells"));
            if hit(
                hits,
                "three:cells-down",
                ui.add_sized(egui::vec2(26.0, 22.0), egui::Button::new("−")),
            )
            .clicked()
            {
                vis.cells = (vis.cells - 1).clamp(MIN_CELLS, MAX_CELLS);
            }
            ui.label(RichText::new(format!("{}", vis.cells)).size(13.0).color(t::INK).family(t::family_mono_med()));
            if hit(
                hits,
                "three:cells-up",
                ui.add_sized(egui::vec2(26.0, 22.0), egui::Button::new("+")),
            )
            .clicked()
            {
                vis.cells = (vis.cells + 1).clamp(MIN_CELLS, MAX_CELLS);
            }
            ui.label(RichText::new(format!("1..{MAX_CELLS} · default 2")).size(9.5).color(t::MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if vis.cutaway { "cutaway: open" } else { "cutaway: closed" };
                if hit(
                    hits,
                    "three:cutaway",
                    ui.add_sized(egui::vec2(112.0, 22.0), egui::Button::new(label)),
                )
                .clicked()
                {
                    vis.cutaway = !vis.cutaway;
                    vis.flash = Some(Flash {
                        ok: true,
                        text: format!("cutaway {}", if vis.cutaway { "open" } else { "closed" }),
                    });
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(t::label("focus cell"));
            ui.label(
                RichText::new(format!("{} of {}", vis.focus_cell.min(vis.cells as usize - 1) + 1, vis.cells))
                    .size(12.0)
                    .color(t::INK)
                    .family(t::family_mono_med()),
            );
            if hit(
                hits,
                "three:cell-next",
                ui.add_sized(egui::vec2(46.0, 22.0), egui::Button::new("next")),
            )
            .clicked()
            {
                vis.focus_cell = (vis.focus_cell + 1) % vis.cells.max(1) as usize;
            }
            ui.label(RichText::new("tap a cell to cut it").size(9.5).color(t::MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if hit(
                    hits,
                    "three:cam-reset",
                    ui.add_sized(egui::vec2(78.0, 22.0), egui::Button::new("cam reset")),
                )
                .clicked()
                {
                    vis.cam_yaw = CAM_DEFAULT.0;
                    vis.cam_pitch = CAM_DEFAULT.1;
                    vis.cam_dist = CAM_DEFAULT.2;
                    vis.flash = Some(Flash {
                        ok: true,
                        text: "camera reset to the three-quarter view".into(),
                    });
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            for slot in Slot::ALL {
                if hit(
                    hits,
                    &format!("three:bay:{}", slot.slug()),
                    ui.add_sized(egui::vec2(78.0, 22.0), egui::Button::new(format!("{} bay", slot.slug()))),
                )
                .clicked()
                {
                    open_picker(vis, slot, false);
                }
            }
        });
        ui.label(
            RichText::new(format!("cell plan {} m · stack Ø {} m · stack height {} m", t::num::sig(plan_m, 3), t::num::sig(stack_d, 3), t::num::sig(stack_h, 3)))
                .size(9.5)
                .color(t::MUTED),
        );
        ui.label(
            RichText::new(format!(
                "{} cell(s) on a shared basin and casing - all of it drawn from the part records, none of it modelled by hand",
                vis.cells
            ))
            .size(9.5)
            .color(t::MUTED),
        );
        ui.label(
            RichText::new(format!(
                "fill layers from the stack: {}",
                if depths.is_empty() { "none".to_string() } else { depths.join(" over ") }
            ))
            .size(9.5)
            .color(t::MUTED),
        );
        ui.label(
            RichText::new("the cut cell exposes the drift bank, the nozzle header, both fill layers, the rain zone and the basin water - tap any of them for the same picker the 2D section uses; the other cells stay closed")
                .size(9.5)
                .color(t::MUTED),
        );
        ui.label(
            RichText::new("air and water inside the cut cell are the same illustration as the 2D view: not a CFD result, and no distribution model")
                .size(9.5)
                .color(t::AMBER),
        );
        ui.label(
            RichText::new("blades rotate with the speed control; the geometry is drawn from the part records, not modelled by hand")
                .size(9.5)
                .color(t::MUTED),
        );
    });
}

// ===================================================================================== the bay picker
//
// Round 2, change A: there is no always-visible parts tray. A bay is tapped (or focused with `B` and opened
// with Enter) and its **own** catalog cards appear beside it - the same fixture records, the same fields,
// the same accept/refuse rules and the same `IN MACHINE` badge the tray cards carried. Drawn by
// [`picker_controls`] / [`picker_row`] above.

// ==================================================================================== scene overlay

fn zone_pa(run: &Run, zone: ZoneId, layer: Option<usize>) -> Option<(f64, f64)> {
    let o = run.output.as_ref()?;
    o.pressure_by_zone
        .iter()
        .find(|z| z.zone == zone && z.layer == layer)
        .map(|z| (z.pressure_pa, z.share_pct))
}

/// #91 round 2, correctness (c): the fill's own transfer, the layers' KaV/L summed. The
/// engine's `kavl_total` is the tower's available transfer - fill + spray zone + rain zone.
pub fn fill_kavl(run: &Run) -> f64 {
    run.output
        .as_ref()
        .map(|o| o.kavl_per_layer.iter().map(|l| l.kavl).sum())
        .unwrap_or(0.0)
}

/// Round 3, item 5: **the pressure at the operating point**, from the engine's own fan/system solve.
///
/// Why this and not `total_pressure_pa`: the fixture engine re-expresses the edited inputs it can (the fan
/// curve follows the speed ratio by the affinity laws, and the run's airflow *is* the crossing's flow -
/// `EngineOutput.airflow_m3_s` is literally `operating_point.x`), while its `total_pressure_pa` stays the
/// recorded air-path loss sum of the anchor (none of its terms is a function of the speed ratio). At the
/// fixture ratio 0.78 the two agree to 0.02 Pa, which is why the round-2 frames looked consistent at
/// 234 rpm; at 0.70 the crossing reads 155.5 Pa while the anchor headline still reads 190.1 Pa, so the
/// chart (which marks the crossing) and the read-out (which read the headline) disagreed by 34.6 Pa.
/// Pairing the pressure with the flow it belongs to is the fix: one operating point, one solve, one number.
/// `data-op-pressure` / `data-pressure` publish both, and the frames state which is which.
pub fn op_pressure(o: &EngineOutput) -> f64 {
    o.fan_system_curve.operating_point.y
}

/// The air-path split the engine returns, summed. The engine's own zone rows; the fixture does not
/// re-split them when the speed ratio moves, so the frame labels them as the recorded run's.
pub fn zone_sum_pa(o: &EngineOutput) -> f64 {
    o.pressure_by_zone.iter().map(|z| z.pressure_pa).sum()
}

fn layer_result(run: &Run, i: usize) -> Option<&cockpit::engine::LayerResult> {
    run.output.as_ref()?.kavl_per_layer.get(i)
}

/// The bays, the section call-out labels and the required "illustrative" chips.
#[allow(clippy::too_many_arguments)]
/// Round 5, item 2: **the section's annotation layer.**
///
/// Every label drawn over the tower - the four bay blocks, the fill layers' own rows, the zone call-outs,
/// the operating point - is a [`Plate`]: a solid backing sized to its own text, positioned against the rect
/// it describes, and painted *after* `flow_overlay`, so the streamlines and the falling water pass under the
/// labels instead of through them. Round 4 painted the labels first and the flow second, which is why a
/// streamline crossed every call-out and why the fan bay's three labels could sit on top of one another.
///
/// The block's lines are merged until they fit the rect they belong to (a bay's label must stay inside the
/// bay), and a line that still cannot fit is truncated with an ellipsis - the section reports both facts
/// (`data-bay-label-outside`, `data-bay-label-truncated`) to its frames.
#[derive(Debug, Clone)]
struct Plate {
    rect: Rect,
    /// One line per row - text, font, colour, and where that line's number comes from. The mark beside a
    /// line is the same painted shape the answer card and the tap-detail use (calculated disc / catalog
    /// ring / illustrative diamond), so a reader learns the key once.
    lines: Vec<(String, FontId, Color32, crate::answer::Src)>,
    tag: Option<(String, FontId, Color32)>,
    pad: f32,
    tone: Color32,
}

/// Issue #91: a section label waiting for its slot in the gutter - the point it names, its lines.
struct Callout {
    target: egui::Pos2,
    lines: Vec<(String, FontId, Color32, crate::answer::Src)>,
    tone: Color32,
    bay: bool,
    key: String,
}

/// One line's height, from its own font size.
fn line_h(font: &FontId) -> f32 {
    (font.size * 1.36).round()
}

fn plate_size(
    painter: &egui::Painter,
    lines: &[(String, FontId, Color32, crate::answer::Src)],
    tag: Option<&(String, FontId, Color32)>,
    pad: f32,
) -> egui::Vec2 {
    let mut w: f32 = 0.0;
    let mut h = pad * 2.0;
    for (text, font, color, _src) in lines.iter() {
        // the mark's own gutter, reserved on every line
        w = w.max(
            painter
                .layout_no_wrap(text.clone(), font.clone(), *color)
                .size()
                .x
                + MARK_GUTTER,
        );
        h += line_h(font);
    }
    if let Some((text, font, color)) = tag {
        w = w.max(
            painter
                .layout_no_wrap(text.clone(), font.clone(), *color)
                .size()
                .x
                + 16.0,
        );
    }
    egui::vec2(w, h)
}

/// The width a line's source mark reserves at the plate's left edge.
const MARK_GUTTER: f32 = 9.5;

fn paint_plate(p: &egui::Painter, pl: &Plate) {
    p.rect_filled(
        pl.rect,
        egui::CornerRadius::same(3),
        t::with_alpha(t::BG, 234),
    );
    p.rect_stroke(
        pl.rect,
        egui::CornerRadius::same(3),
        Stroke::new(1.0, t::with_alpha(pl.tone, 150)),
        StrokeKind::Inside,
    );
    let mut y = pl.rect.top() + pl.pad;
    let mut last_top = y;
    for (text, font, color, src) in pl.lines.iter() {
        last_top = y;
        // #91 round 2: the source mark, one per line - so a number on the tower carries its provenance
        // exactly like a number on the answer card. Hover names it; the key is on the card.
        crate::answer::paint_mark(
            p,
            egui::pos2(pl.rect.left() + pl.pad + 3.4, y + line_h(font) * 0.5 - 1.0),
            3.2,
            *src,
        );
        p.text(
            egui::pos2(pl.rect.left() + pl.pad + MARK_GUTTER, y),
            Align2::LEFT_TOP,
            text,
            font.clone(),
            *color,
        );
        y += line_h(font);
    }
    if let Some((text, font, color)) = pl.tag.as_ref() {
        p.text(
            egui::pos2(pl.rect.right() - pl.pad, last_top),
            Align2::RIGHT_TOP,
            text,
            font.clone(),
            *color,
        );
    }
}

/// Cut a line to fit `max_w`, with an ellipsis (the honest fallback when a bay is smaller than its label).
fn truncate_to(painter: &egui::Painter, text: &str, font: &FontId, max_w: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    for take in (1..chars.len()).rev() {
        let s: String = chars[..take].iter().collect();
        let cand = format!("{s}…");
        if painter
            .layout_no_wrap(cand.clone(), font.clone(), t::INK)
            .size()
            .x
            <= max_w
        {
            return cand;
        }
    }
    "…".to_string()
}

/// Issue #81: does this stack of plates fit a column that tall? `scene_overlay` steps the type down to its
/// floor, then drops plates from the bottom of the band order, until this holds - a column that cannot hold
/// its plates is what put two labels on the same pixels at 390x844.
pub(crate) fn column_holds(heights: &[f32], gap: f32, budget: f32) -> bool {
    heights.iter().sum::<f32>() + gap * (heights.len().saturating_sub(1)) as f32 <= budget
}

/// Issue #81: stack plates down a column in band order - each as close to its target as it can be, never
/// above the one before it, then pulled back up from the bottom when the column ran out. Pure, so the
/// packer is testable against the sizes the frames measured: the caller has already made the stack fit
/// ([`column_holds`]), and then no two plates can share pixels.
pub(crate) fn pack_column(
    targets: &[f32],
    heights: &[f32],
    first_floor: f32,
    min_top: f32,
    bottom: f32,
    gap: f32,
) -> Vec<f32> {
    let mut ys = Vec::with_capacity(heights.len());
    let mut floor = first_floor;
    for (t, h) in targets.iter().zip(heights.iter()) {
        let y = (t - h * 0.5).max(floor);
        ys.push(y);
        floor = y + h + gap;
    }
    let mut ceil = bottom;
    for i in (0..heights.len()).rev() {
        if ys[i] + heights[i] > ceil {
            ys[i] = (ceil - heights[i]).max(min_top);
        }
        ceil = ys[i] - gap;
    }
    ys
}

#[allow(clippy::too_many_arguments)]
fn scene_overlay(
    ui: &mut egui::Ui,
    area: Rect,
    cat: Option<&Catalog>,
    draft: &mut Draft,
    vis: &mut Visual,
    run: &Run,
    ctx: &egui::Context,
    hits: &mut HitMap,
    info: &mut crate::clip::LayoutInfo,
    anim_t: f32,
    // Whether the gutter call-out plates are drawn. The Instrument's tower is the drawing the numbers live
    // on; the Curves view shows the same tower as a *reference section* beside its charts, where ten plates
    // cannot fit one column and every figure they carry is already on the Instrument and in the view's own
    // readout card. Round 2 passes `false` there.
    gutter_plates: bool,
    // Issue #81: the shell's own phone rule (screen width), so the overlay, the sprite pass and the layout
    // all read one breakpoint. It used to re-derive it from the section's width, so a 700 px window drew a
    // desktop rail beside a phone-styled section.
    phone: bool,
) {
    let l = scene::layout(area, Some(&draft.0), phone);
    let p = ui.painter_at(area);
    let mono = |size: f32| FontId::new(size, t::family_mono_med());
    let semi = |size: f32| FontId::new(size, t::family_semi());

    // ---- issue #81 / #91: the elevation ruler. The section has ONE scale, so one ruler measures all of it:
    // a tick per metre from the basin floor to the stack mouth, and a bracket for the fill depth.
    if !phone && l.px_per_m > 0.0 {
        let floor_y = l.basin.bottom();
        let top_y = l.stack.top();
        let x = l.ruler.right() - 4.0;
        p.line_segment(
            [egui::pos2(x, floor_y), egui::pos2(x, top_y)],
            Stroke::new(1.0, t::with_alpha(t::TICK, 200)),
        );
        let mut mtr = 0.0_f32;
        while floor_y - mtr * l.px_per_m >= top_y - 0.5 {
            let y = floor_y - mtr * l.px_per_m;
            let major = (mtr as i32) % 2 == 0;
            p.line_segment(
                [
                    egui::pos2(x - if major { 7.0 } else { 4.0 }, y),
                    egui::pos2(x, y),
                ],
                Stroke::new(1.0, t::with_alpha(t::TICK, 230)),
            );
            if major {
                p.text(
                    egui::pos2(x - 10.0, y),
                    Align2::RIGHT_CENTER,
                    format!("{mtr:.0}"),
                    mono(9.0),
                    t::MUTED,
                );
            }
            mtr += 1.0;
        }
        p.text(
            egui::pos2(x - 10.0, top_y - 12.0),
            Align2::RIGHT_BOTTOM,
            "m",
            mono(9.0),
            t::MUTED,
        );
        // the fill bracket: the authored stack's depth, at the same scale
        if let (Some(first), Some(last)) = (l.layers.first(), l.layers.last()) {
            let bx = l.tower.left() - if phone { 2.0 } else { 18.0 };
            let c = t::with_alpha(t::PRIMARY, 150);
            p.line_segment(
                [egui::pos2(bx, first.top()), egui::pos2(bx, last.bottom())],
                Stroke::new(1.0, c),
            );
            p.line_segment(
                [
                    egui::pos2(bx, first.top()),
                    egui::pos2(bx + 4.0, first.top()),
                ],
                Stroke::new(1.0, c),
            );
            p.line_segment(
                [
                    egui::pos2(bx, last.bottom()),
                    egui::pos2(bx + 4.0, last.bottom()),
                ],
                Stroke::new(1.0, c),
            );
        }
    }

    // ---- bays: dashed frames, their content, the drop state, tap-to-place
    //
    // Round 3, item 2: the bays advertise themselves at rest ("tap or drop here", a dashed frame and a
    // pointer cursor) and answer a carried part *before* release - every bay that accepts it lights up and
    // every bay that refuses it dims. The carried part is whichever one the user picked up: a live pointer
    // drag from the rail or a picker card, or a `?drag=` staged one (which is how the phone frames prove
    // the same drawing).
    // Round 5: the fan bay's own block carries the rpm (round 4 drew it as free text left of the stack,
    // where it ran off the drawing's left edge).
    let rpm_now = m::rpm_text(draft.0.speed_ratio, draft.0.fan.nominal_rpm);
    // issue #137: the plate stays short - "OP" was jargon and "operating point" pushed the plate to a third
    // line, so at 390 px the packer dropped the basin plate. The fan's operating point is air flow and
    // pressure; the plate names the air and the units name the rest.
    let op_text = |a: &EngineOutput| -> String {
        format!(
            "fan · {} m³/s · {} Pa",
            t::num::flow(a.airflow_m3_s),
            t::num::pa(op_pressure(a))
        )
    };
    // Issue #91: every section label is a **call-out in the gutter** right of the drawing, on a leader line
    // to the band it names. The gutter is one column and the call-outs are stacked in it in band order, so
    // two labels can never overlap and no label is drawn over the drawing (#81's collision list).
    let mut callouts: Vec<Callout> = Vec::new();
    let released = ctx.input(|i| i.pointer.any_released());
    let payload = egui::DragAndDrop::payload::<PartRef>(ctx);
    let carried: Option<PartRef> = payload
        .as_deref()
        .cloned()
        .or_else(|| vis.drag.as_ref().map(|d| d.part.clone()));
    let mut over_now: Option<Slot> = None;
    // Round 5, item 2: the annotation plates, built while the section is drawn and painted after the flow.
    let mut plates: Vec<Plate> = Vec::new();
    let mut bay_rects: Vec<Rect> = Vec::new();
    for (slot, r) in l.slots.iter() {
        let resp = ui
            .interact(*r, ui.id().with(("bay", slot.slug())), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!("{} bay · tap or drop a part here", slot.name()));
        hits.0.push((
            format!("bay:{}", slot.slug()),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
        // What the catalog says about this bay for the part being carried - computed for *every* bay, so an
        // accepting bay can light up while the pointer is still over the rail.
        let verdict = carried.as_ref().map(|part| {
            (
                part.clone(),
                check_drop(cat.unwrap_or(&Catalog::default()), &draft.0, *slot, part),
            )
        });
        let accepts = matches!(verdict.as_ref().map(|(_, v)| v), Some(Ok(())));
        let refuses = matches!(verdict.as_ref().map(|(_, v)| v), Some(Err(_)));
        let (color, width, wash) = bay_paint(vis, *slot, accepts, refuses);
        if let Some((tint, alpha)) = wash {
            p.rect_filled(*r, egui::CornerRadius::same(3), t::with_alpha(tint, alpha));
        }
        let pts = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        let mut dashes = Vec::new();
        Shape::dashed_line_many(&pts, Stroke::new(width, color), 7.0, 5.0, &mut dashes);
        p.extend(dashes);

        // ---- the bay's call-out: the record, the zone's own engine value - or, while a part is carried, the
        // bay's verdict on it. One block per bay (round 5), now in the gutter on a leader (issue #91).
        let mono_size = if phone { 9.0 } else { 10.5 };
        let mut lines: Vec<(String, FontId, Color32, crate::answer::Src)> = Vec::new();
        let tone = match verdict.as_ref() {
            Some((part, v)) => {
                let (txt, col) = carried_line(part, v);
                lines.push((
                    txt,
                    semi(if phone { 9.0 } else { 10.0 }),
                    col,
                    crate::answer::Src::Catalog,
                ));
                col
            }
            None => {
                // The bar carries the rpm (the primary action's value): the call-out names the part and the
                // stack's own pressure only, so no number is printed twice (issue #91).
                let _ = rpm_now;
                let identity = match slot {
                    Slot::Fan => draft.0.fan.id.clone(),
                    Slot::Drift => draft.0.drift.id.clone(),
                    Slot::Nozzle => draft.0.nozzle.id.clone(),
                    Slot::Fill => {
                        let depth: f64 = draft.0.fill_layers.iter().map(|x| x.depth_m).sum();
                        format!("fill · {:.2} m", depth)
                    }
                };
                lines.push((
                    identity,
                    mono(mono_size),
                    t::with_alpha(t::INK, 230),
                    crate::answer::Src::Catalog,
                ));
                // #91 round 2, correctness (a) and (b): every zone row reads the same grouped,
                // largest-remainder figures the answer card and the tap-detail print
                // (`answer::air_path`), so the fill finally carries its own Pa and share and the
                // shares a reader adds up across the bays come to exactly 100 %.
                let zone = match slot {
                    Slot::Fan => crate::answer::shown_share_f(run, ZoneId::Stack)
                        .map(|(pa, share)| format!("stack {pa:.1} Pa · {share}%")),
                    Slot::Drift => crate::answer::shown_share_f(run, ZoneId::Drift)
                        .map(|(pa, share)| format!("{pa:.1} Pa · {share}%")),
                    Slot::Nozzle => crate::answer::shown_share_f(run, ZoneId::Spray)
                        .map(|(pa, share)| format!("{pa:.1} Pa · {share}%")),
                    // (c): the fill's own KaV/L is its layers' sum; `kavl_total` is the tower's
                    // *available* transfer (fill + spray + rain) and is labelled as such on the answer
                    // card, never attributed to the fill.
                    Slot::Fill => {
                        crate::answer::shown_share_f(run, ZoneId::Fill).map(|(pa, share)| {
                            format!("{pa:.1} Pa · {share}% · KaV/L {:.3}", fill_kavl(run))
                        })
                    }
                };
                if let Some(z) = zone {
                    lines.push((z, mono(mono_size - 0.5), t::INK_2, crate::answer::Src::Calc));
                }
                if vis.selected_slot == *slot {
                    t::with_alpha(t::PRIMARY, 220)
                } else {
                    t::SLOT
                }
            }
        };
        // The leader lands on the bay's right edge, at the middle of what the bay draws (the fill bay: the
        // authored stack, not the spare depth above it).
        let target_y = match slot {
            Slot::Fan => l.stack_cyl.center().y,
            Slot::Fill => match (l.layers.first(), l.layers.last()) {
                (Some(a), Some(b)) => (a.top() + b.bottom()) * 0.5,
                _ => r.center().y,
            },
            _ => r.center().y,
        };
        let anchor_x = match slot {
            Slot::Fan => l.stack_cyl.right() - 2.0,
            _ => r.right() - 2.0,
        };
        callouts.push(Callout {
            target: egui::pos2(anchor_x, target_y),
            lines,
            tone,
            bay: true,
            key: format!("bay:{}", slot.slug()),
        });

        // What the catalog says about what is being carried. The pointer path and the staged path draw the
        // same thing; only the `staged` flag differs, and the frame publishes it.
        if let Some(payload) = &payload {
            if resp.contains_pointer() {
                over_now = Some(*slot);
                let part: &PartRef = payload;
                let verdict = check_drop(cat.unwrap_or(&Catalog::default()), &draft.0, *slot, part);
                if vis.drag.as_ref().map(|d| d.part != *part).unwrap_or(true) {
                    vis.selected_part = Some(part.clone());
                }
                vis.drag = Some(Drag {
                    part: part.clone(),
                    over: Some(*slot),
                    verdict,
                    staged: false,
                });
            }
        }

        // drop (pointer released over the bay): the replace, and the picker closes with it. A click/tap now
        // OPENS the bay's picker (round 2, change A) instead of placing whatever was selected.
        if resp.contains_pointer() && released {
            if let Some(payload) = egui::DragAndDrop::take_payload::<PartRef>(ctx) {
                let part: &PartRef = &payload;
                drop_part(cat, Some(draft), vis, *slot, part);
                vis.picker = None;
            }
        } else if resp.clicked()
            && vis.drag.is_none()
            && !picker_holds(vis, ctx.pointer_latest_pos())
        {
            let layer = vis
                .selected_layer
                .min(draft.0.fill_layers.len().saturating_sub(1));
            vis.picker = Some(Picker::new(
                *slot,
                layer,
                [r.min.x, r.min.y, r.width(), r.height()],
                false,
            ));
            vis.selected_slot = *slot;
            vis.flash = Some(Flash {
                ok: true,
                text: format!("{} picker open", slot.name()),
            });
        }
    }
    if released && over_now.is_none() && !ctx.input(|i| i.pointer.any_down()) {
        // A staged drag (?drag=…) is owned by the staging, not by the pointer: only a pointer drag ends here.
        if vis.drag.as_ref().map(|d| !d.staged).unwrap_or(false) {
            vis.drag = None;
        }
    }

    // ---- round 2, change B: air as streamlines and water as falling streaks, over the structure
    flow_overlay(&p, &l, draft, run, vis, anim_t, phone);

    // ---- the zone call-outs that are not a bay's own value (engine output at the current ratio), in the
    // same gutter, in band order: the operating point (plenum), the fill layers, the rain zone, the inlet
    // louvres, the basin.
    if let Some(o) = run.output.as_ref() {
        // The operating point, and the plenum's own share beside it: the plenum is the band the OP rail sits
        // in, so its Pa belongs in this call-out rather than in a second plate in the same place.
        let mut lines = vec![(
            op_text(o),
            mono(if phone { 9.0 } else { 10.0 }),
            t::PRIMARY,
            crate::answer::Src::Calc,
        )];
        if let Some((pa, share)) = crate::answer::shown_share_f(run, ZoneId::Plenum) {
            lines.push((
                format!("plenum {pa:.1} Pa · {share}%"),
                mono(if phone { 9.0 } else { 9.5 }),
                t::MUTED,
                crate::answer::Src::Calc,
            ));
        }
        callouts.push(Callout {
            target: egui::pos2(l.op_rail.right(), l.op_rail.center().y),
            lines,
            tone: t::with_alpha(t::PRIMARY, 200),
            bay: false,
            key: "callout:op".into(),
        });
    }
    if !phone {
        for (i, r) in l.layers.iter().enumerate() {
            let Some(lr) = layer_result(run, i) else {
                continue;
            };
            let share = zone_pa(run, ZoneId::Fill, Some(i))
                .map(|(_, s)| s)
                .unwrap_or(0.0);
            let mut lines = vec![(
                format!(
                    "L{} {} {:.2} m · {:.1} Pa · {:.0}% · KaV/L {:.3}",
                    i + 1,
                    lr.fill_id,
                    lr.depth_m,
                    lr.pressure_pa,
                    share,
                    lr.kavl
                ),
                mono(10.0),
                t::fill_color(&lr.fill_id),
                crate::answer::Src::Calc,
            )];
            if !lr.inside_envelope {
                lines.push((
                    "outside envelope".to_string(),
                    semi(9.5),
                    t::DANGER,
                    crate::answer::Src::Catalog,
                ));
            }
            callouts.push(Callout {
                target: egui::pos2(r.right() - 2.0, r.center().y),
                lines,
                tone: t::SLOT,
                bay: false,
                key: format!("callout:layer:{i}"),
            });
        }
    }
    if let Some((pa, share)) = crate::answer::shown_share_f(run, ZoneId::Rain) {
        callouts.push(Callout {
            target: egui::pos2(l.rain.right() - 2.0, l.rain.top() + l.rain.height() * 0.3),
            lines: vec![(
                format!("rain {pa:.1} Pa · {share}%"),
                mono(if phone { 9.0 } else { 10.0 }),
                t::INK_2,
                crate::answer::Src::Calc,
            )],
            tone: t::SLOT,
            bay: false,
            key: "callout:rain".into(),
        });
    }
    if let Some((pa, share)) = crate::answer::shown_share_f(run, ZoneId::Inlet) {
        let mut lines = vec![(
            format!("inlet {pa:.1} Pa · {share}%"),
            mono(if phone { 9.0 } else { 10.0 }),
            t::AIR,
            crate::answer::Src::Calc,
        )];
        if !phone {
            if let Some(a) = cat.and_then(|c| c.ambient) {
                lines.push((
                    format!(
                        "air in {} / {} °C · RH {} %",
                        t::num::temp(a.dry_bulb_c),
                        t::num::temp(a.wet_bulb_c),
                        t::num::pct(a.relative_humidity * 100.0)
                    ),
                    mono(9.5),
                    t::MUTED,
                    crate::answer::Src::Catalog,
                ));
            }
        }
        callouts.push(Callout {
            target: egui::pos2(l.inlet_r.right(), l.inlet_r.center().y),
            lines,
            tone: t::SLOT,
            bay: false,
            key: "callout:inlet".into(),
        });
    }
    if let Some(o) = run.output.as_ref() {
        // The basin, and the engine's lumped "fixed losses" row with it: the lumped row has no band of its
        // own, so it lives with the floor of the machine (rather than disappearing from the screen).
        let mut lines = vec![(
            format!("basin {} m³/h", t::num::flow(o.water_flow_m3_hr)),
            mono(if phone { 9.0 } else { 10.0 }),
            t::WATER,
            crate::answer::Src::Calc,
        )];
        if let Some((pa, share)) = crate::answer::shown_share_f(run, ZoneId::Fixed) {
            lines.push((
                format!("fixed {pa:.1} Pa · {share}%"),
                mono(if phone { 9.0 } else { 9.5 }),
                t::MUTED,
                crate::answer::Src::Calc,
            ));
        }
        callouts.push(Callout {
            target: egui::pos2(l.basin.right() - 2.0, l.water_surface_y + 4.0),
            lines,
            tone: t::SLOT,
            bay: false,
            key: "callout:basin".into(),
        });
    }

    // ---- place the call-outs: band order, each at its target's height where it can be, pushed down past the
    // one above it, then pulled back up from the bottom if the column ran out. One column, no overlaps.
    if !gutter_plates {
        // the Curves view's reference section: the plates are not drawn, so they are not placed and no
        // `label:callout:*` entry is published for a plate that is not on the screen.
        callouts.clear();
    }
    let gut = l.gutter;
    let mut placed: Vec<(Callout, Rect)> = Vec::new();
    {
        let gap = if phone { 2.0 } else { 6.0 };
        let pad = if phone { 2.5 } else { 4.0 };
        let mut sized: Vec<(Callout, egui::Vec2)> = callouts
            .into_iter()
            .map(|mut c| {
                // fit the gutter: step the type down, then drop the leading piece, then cut
                let room = gut.width() - pad * 2.0 - 2.0 - MARK_GUTTER;
                while plate_size(&p, &c.lines, None, 0.0).x > room && c.lines[0].1.size > 8.0 {
                    for ln in c.lines.iter_mut() {
                        ln.1 = FontId::new(ln.1.size - 0.5, ln.1.family.clone());
                    }
                }
                // #91 round 2, correctness (a): a line that does not fit the gutter is *reflowed* at its
                // last ` · ` into a second line, not shortened. Round 1 dropped the leading piece, which
                // is how the fill lost its Pa on a phone (its plate read "44 % · KaV/L 1.326" with no
                // pressure): dropping a figure is not an option on a screen whose whole point is that
                // every number is sourced. Only a piece that still cannot fit after reflowing is cut.
                let mut fitted: Vec<(String, FontId, Color32, crate::answer::Src)> = Vec::new();
                for ln in c.lines.drain(..) {
                    let (mut text, font, color, src) = ln;
                    let mut tails: Vec<String> = Vec::new();
                    loop {
                        if p.layout_no_wrap(text.clone(), font.clone(), color).size().x <= room {
                            break;
                        }
                        match text.rsplit_once(" · ") {
                            Some((head, tail)) if !head.is_empty() => {
                                tails.push(tail.to_string());
                                text = head.to_string();
                            }
                            _ => {
                                text = truncate_to(&p, &text, &font, room);
                                break;
                            }
                        }
                    }
                    fitted.push((text, font.clone(), color, src));
                    for tail in tails.iter().rev() {
                        fitted.push((tail.clone(), font.clone(), color, src));
                    }
                }
                c.lines = fitted;
                let s = plate_size(&p, &c.lines, None, pad);
                (c, egui::vec2((s.x + pad * 2.0).min(gut.width()), s.y))
            })
            .collect();
        sized.sort_by(|a, b| a.0.target.y.total_cmp(&b.0.target.y));
        // Issue #81: the column has a height as well as a width. At 390x844 the section is 234 px tall and
        // the eight plates wanted 242, so the packer pulled them back on top of one another (the fan plate
        // and the operating-point call-out shared eight rows of pixels). Step the type down to the floor -
        // the same floor the width fit uses - and only then drop plates from the bottom of the band order,
        // until the stack fits the column: what is dropped is still on the tap-detail.
        let col_top = l.area.top() + 2.0;
        let col_bottom = gut.bottom().max(l.area.bottom() - 4.0);
        let budget = col_bottom - col_top;
        let heights_of =
            |s: &[(Callout, egui::Vec2)]| -> Vec<f32> { s.iter().map(|(_, v)| v.y).collect() };
        while !column_holds(&heights_of(&sized), gap, budget) {
            let mut stepped = false;
            for (c, s) in sized.iter_mut() {
                if c.lines.iter().any(|ln| ln.1.size > 8.0) {
                    for ln in c.lines.iter_mut() {
                        ln.1 = FontId::new(ln.1.size - 0.5, ln.1.family.clone());
                    }
                    let sz = plate_size(&p, &c.lines, None, pad);
                    *s = egui::vec2((sz.x + pad * 2.0).min(gut.width()), sz.y);
                    stepped = true;
                }
            }
            if !stepped {
                break;
            }
        }
        while !column_holds(&heights_of(&sized), gap, budget) && sized.len() > 1 {
            sized.pop();
        }
        let targets: Vec<f32> = sized.iter().map(|(c, _)| c.target.y).collect();
        let heights: Vec<f32> = sized.iter().map(|(_, s)| s.y).collect();
        let ys = pack_column(&targets, &heights, gut.top(), col_top, col_bottom, gap);
        for (i, (c, s)) in sized.into_iter().enumerate() {
            let r = Rect::from_min_size(egui::pos2(gut.left(), ys[i]), s);
            placed.push((c, r));
        }
        let _ = pad;
    }
    // the leaders, under the plates: from the target, level to the drawing's edge, then to the plate
    for (c, r) in placed.iter() {
        let col = t::with_alpha(c.tone, 150);
        let elbow = egui::pos2(gut.left() - if phone { 4.0 } else { 10.0 }, r.center().y);
        let knee = egui::pos2(l.rail.left() - 4.0, c.target.y);
        p.circle_filled(c.target, 2.2, col);
        p.line_segment(
            [c.target, knee],
            Stroke::new(1.0, t::with_alpha(c.tone, 120)),
        );
        p.line_segment([knee, elbow], Stroke::new(1.0, t::with_alpha(c.tone, 120)));
        p.line_segment(
            [elbow, egui::pos2(r.left(), r.center().y)],
            Stroke::new(1.0, col),
        );
    }
    for (c, r) in placed.into_iter() {
        let pl = Plate {
            rect: r,
            lines: c.lines,
            tag: None,
            pad: if phone { 2.5 } else { 4.0 },
            tone: c.tone,
        };
        if c.bay {
            if !l.area.contains_rect(r) {
                info.bay_label_outside += 1;
            }
            bay_rects.push(r);
        }
        hits.0.push((
            format!("label:{}", c.key),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
        plates.push(pl);
    }

    // ---- the annotation layer, painted last (round 5, item 2): every plate is painted after the flow, so a
    // streamline passes under a label and never through it. The section reports what it drew.
    info.bay_labels = bay_rects.len() as u32;
    info.bay_label_overlaps = overlaps(&bay_rects);
    info.plates = plates.len() as u32;
    info.plate_overlaps = overlaps(&plates.iter().map(|pl| pl.rect).collect::<Vec<Rect>>());
    info.plate_rects = plates
        .iter()
        .map(|pl| {
            format!(
                "{:.0},{:.0},{:.0},{:.0}",
                pl.rect.min.x,
                pl.rect.min.y,
                pl.rect.width(),
                pl.rect.height()
            )
        })
        .collect::<Vec<String>>()
        .join(";");
    for pl in plates.iter() {
        paint_plate(&p, pl);
    }

    // ---- the required honesty chips, in place (the legend's own corner draws them now)
    let _chip_at = |p: &egui::Painter, pos: egui::Pos2, text: &str, fg: Color32, bg: Color32| {
        let galley = p.layout_no_wrap(text.to_string(), semi(9.5), fg);
        let r = Rect::from_min_size(pos, galley.size() + egui::vec2(10.0, 4.0));
        p.rect_filled(r, egui::CornerRadius::same(3), bg);
        p.rect_stroke(
            r,
            egui::CornerRadius::same(3),
            Stroke::new(1.0, t::with_alpha(fg, 140)),
            StrokeKind::Inside,
        );
        p.galley(r.min + egui::vec2(5.0, 2.0), galley, fg);
    };
    // ---- round 2, change B: the label legend. Round 3, item 7: it used to sit at the section's top-left
    // corner, where it covered the fan bay and clipped its label. It now sits in the plinth corner (the one
    // band no bay reaches), it is dismissible, and the state is remembered for the session.
    // Issue #91: the legend is a colour key - short labels only. Its honesty sentences ("streamlines are an
    // illustration, not a CFD result", "spray coverage: illustrative (no distribution model)") and the rpm
    // provenance line are in the notes drawer, behind the header's validation badge.
    let legend_lines: Vec<(String, Color32, bool)> = vec![
        ("air".to_string(), t::AIR, true),
        ("water".to_string(), t::WATER, false),
        ("spray".to_string(), t::AMBER, false),
    ];
    // Issue #91: the colour key sits in the section's top-left corner, beside the ruler's head, where the
    // headroom above the stack leaves the ground empty.
    let legend_anchor = egui::pos2(l.area.left() + 8.0, l.area.top() + 8.0);
    if !vis.legend_open {
        // A single restore chip in the same corner, so a dismissed legend is never lost.
        let txt = "labels hidden · L".to_string();
        let g = p.layout_no_wrap(txt.clone(), semi(9.5), t::MUTED);
        let r = Rect::from_min_size(legend_anchor, g.size() + egui::vec2(10.0, 4.0));
        p.rect_filled(r, egui::CornerRadius::same(3), t::with_alpha(t::BG, 226));
        p.rect_stroke(
            r,
            egui::CornerRadius::same(3),
            Stroke::new(1.0, t::with_alpha(t::SLOT, 200)),
            StrokeKind::Inside,
        );
        p.galley(r.min + egui::vec2(5.0, 2.0), g, t::MUTED);
        hits.0.push((
            "legend:show".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
        let resp = ui.interact(r, egui::Id::new("legend.show"), Sense::click());
        if resp.clicked() {
            vis.legend_open = true;
        }
    } else {
        let mut w: f32 = 0.0;
        let mut laid = Vec::new();
        for (text, color, strong) in legend_lines.iter() {
            let size = if *strong {
                if phone {
                    11.0
                } else {
                    10.0
                }
            } else if phone {
                10.0
            } else {
                9.0
            };
            let g = p.layout_no_wrap(
                text.clone(),
                if *strong { semi(size) } else { mono(size) },
                *color,
            );
            w = w.max(g.size().x);
            laid.push(g);
        }
        let pad = 7.0;
        let line_h = if phone { 15.0 } else { 13.0 };
        let box_r = Rect::from_min_size(
            legend_anchor,
            egui::vec2(
                w + pad * 2.0 + 12.0,
                line_h * laid.len() as f32 + pad * 2.0 - 3.0,
            ),
        );
        // The legend reports its own rect (round 3, item 7), so a frame can prove it clears every bay.
        hits.0.push((
            "legend:box".to_string(),
            [box_r.min.x, box_r.min.y, box_r.width(), box_r.height()],
        ));
        p.rect_filled(
            box_r,
            egui::CornerRadius::same(4),
            t::with_alpha(t::BG, 226),
        );
        p.rect_stroke(
            box_r,
            egui::CornerRadius::same(4),
            Stroke::new(1.0, t::with_alpha(t::SLOT, 220)),
            StrokeKind::Inside,
        );
        for (i, g) in laid.into_iter().enumerate() {
            p.galley(
                egui::pos2(box_r.left() + pad, box_r.top() + pad + line_h * i as f32),
                g,
                legend_lines[i].1,
            );
        }
        // The dismiss control: a small x in the box's top-right corner.
        let xr = Rect::from_min_size(
            egui::pos2(box_r.right() - 15.0, box_r.top() + 2.0),
            egui::vec2(13.0, 13.0),
        );
        p.text(
            xr.center(),
            Align2::CENTER_CENTER,
            "×",
            FontId::new(11.0, t::family_semi()),
            t::MUTED,
        );
        hits.0.push((
            "legend:hide".to_string(),
            [xr.min.x, xr.min.y, xr.width(), xr.height()],
        ));
        let resp = ui.interact(xr, egui::Id::new("legend.hide"), Sense::click());
        if resp.on_hover_text("hide these labels (L)").clicked() {
            vis.legend_open = false;
            vis.flash = Some(Flash {
                ok: true,
                text: "section labels hidden (L brings them back)".into(),
            });
        }
    }

    // ---- the pressure split rail's caption (issue #91: its per-zone values are the call-outs beside it,
    // in the same order, so the rail carries no text of its own except its total)
    if let Some(o) = run.output.as_ref() {
        p.text(
            egui::pos2(l.rail.center().x, l.rail.top() - 6.0),
            Align2::CENTER_BOTTOM,
            if phone {
                "Pa".to_string()
            } else {
                format!("total {:.0} Pa", zone_sum_pa(o))
            },
            semi(9.0),
            t::MUTED,
        );
    }
}

/// Round 2, change B: **air as streamlines, water as falling streaks**.
///
/// Density and speed follow the engine's airflow (and therefore the rpm slider); the colour of each piece of
/// a streamline follows *that zone's* own pressure share from `EngineOutput.pressure_by_zone`; the streak
/// count follows the engine's `water_flow_m3_hr`. Both are illustrations, and the frame says so where they
/// are drawn - `streamlines are an illustration, not a CFD result`, and the spray cones keep
/// `illustrative (no distribution model)`.
fn flow_overlay(
    p: &egui::Painter,
    l: &scene::Layout,
    draft: &Draft,
    run: &Run,
    vis: &Visual,
    anim_t: f32,
    phone: bool,
) {
    let flow = run
        .output
        .as_ref()
        .map(|o| o.airflow_m3_s)
        .unwrap_or(m::ANCHOR_AIRFLOW_M3_S);
    let flow_f = m::flow_factor(flow);
    // Issue #86 AC 1: the falling water answers to the **water** the engine solved - the duty's mass flow
    // over the tower's own fill area (the engine's own loading definition) - not to the airflow. The heads
    // on the air path move at the airflow's own rate.
    let water_loading = m::water_loading_kg_m2_s(
        m::kg_s_from_m3_hr(draft.0.duty.water_flow_m3_hr),
        draft.0.tower.fill_area_m2,
    )
    .unwrap_or(m::ANCHOR_WATER_LOADING_KG_M2_S);
    let air_rate = m::air_head_rate(flow);
    let water_rate = m::water_streak_rate(water_loading);
    let n = m::streamline_count(flow).min(if phone { 4 } else { m::MAX_STREAMLINES });
    let f_flow = if matches!(vis.focus, Focus::All | Focus::Airflow) {
        1.0
    } else {
        0.16
    };
    let f_water = if matches!(vis.focus, Focus::All | Focus::Airflow) {
        1.0
    } else {
        0.5
    };

    // A zone's own share -> the intensity of the line inside that zone (never a velocity).
    let share_of = |zone: ZoneId| -> f64 {
        run.output
            .as_ref()
            .and_then(|o| {
                o.pressure_by_zone
                    .iter()
                    .find(|z| z.zone == zone && z.layer.is_none())
                    .map(|z| z.share_pct)
            })
            .unwrap_or(10.0)
    };
    let air = t::AIR;
    let spray_c = t::PRIMARY;
    let drift_c = t::OK;
    let plenum_c = t::MUTED;
    let stack_c = t::INK_2;
    let water_c = t::WATER;
    // Which zone a screen y is in, top-down: the same bands the sprites use.
    let zone_at = |y: f32| -> (egui::Color32, f64) {
        if y <= l.deck.bottom() {
            (stack_c, share_of(ZoneId::Stack))
        } else if y <= l.plenum.bottom() {
            (plenum_c, share_of(ZoneId::Plenum))
        } else if y <= l.drift.bottom() {
            (drift_c, share_of(ZoneId::Drift))
        } else if y <= l.spray.bottom() {
            (spray_c, share_of(ZoneId::Spray))
        } else if y <= l.fill_band.bottom() {
            let id = draft.0.fill_layers.first().map(|x| x.fill_id.as_str());
            (
                id.map(t::fill_color).unwrap_or(t::PRIMARY),
                share_of(ZoneId::Fill),
            )
        } else if y <= l.rain.bottom() {
            (water_c, share_of(ZoneId::Rain))
        } else {
            (air, share_of(ZoneId::Inlet))
        }
    };

    // ---- the air: one bundle per side, in through the louvres, up through the fill, out of the stack
    let build = |side: f32| -> Vec<Vec<egui::Pos2>> {
        let in_rect = if side < 0.0 { l.inlet_l } else { l.inlet_r };
        let x_out = if side < 0.0 {
            in_rect.left() - if phone { 8.0 } else { 14.0 }
        } else {
            in_rect.right() + if phone { 8.0 } else { 14.0 }
        };
        let entry_x = if side < 0.0 {
            l.tower.left() + 4.0
        } else {
            l.tower.right() - 4.0
        };
        let stalk_x = l.tower.center().x + side * l.tower.width() * 0.30;
        (0..n)
            .map(|j| {
                let y = in_rect.top() + in_rect.height() * (j as f32 + 0.5) / n as f32;
                let anchors = [
                    egui::pos2(x_out, y),
                    egui::pos2(entry_x, y),
                    egui::pos2(stalk_x, l.rain.bottom() * 0.5 + l.rain.top() * 0.5),
                    egui::pos2(stalk_x, l.fill_band.center().y),
                    egui::pos2(
                        l.tower.center().x + side * l.tower.width() * 0.12,
                        l.drift.top(),
                    ),
                    egui::pos2(
                        l.stack_cyl.center().x + side * l.stack_cyl.width() * 0.18,
                        l.stack_rim.center().y,
                    ),
                    egui::pos2(
                        l.stack_cyl.center().x + side * l.stack_cyl.width() * 0.22,
                        l.stack_rim.top() - 12.0,
                    ),
                ];
                catmull(&anchors, if phone { 14 } else { 20 })
            })
            .collect()
    };
    let bundles = [build(-1.0), build(1.0)];
    let line_w = if phone { 1.2 } else { 1.5 };
    for (side_i, paths) in bundles.iter().enumerate() {
        let side = if side_i == 0 { -1.0 } else { 1.0 };
        for (j, pts) in paths.iter().enumerate() {
            // the faint continuous path, so the direction of travel is readable
            let mut drawn: Vec<egui::Pos2> = Vec::with_capacity(pts.len() * 2);
            for (i, pt) in pts.iter().enumerate() {
                let (c, share) = zone_at(pt.y);
                let a = ((30.0 + 90.0 * (share / 100.0) as f32) * f_flow) as u8;
                drawn.push(*pt);
                if i + 1 < pts.len() {
                    let (_c2, s2) = zone_at(pts[i + 1].y);
                    let a2 = ((30.0 + 90.0 * (s2 / 100.0) as f32) * f_flow) as u8;
                    p.line_segment(
                        [*pt, pts[i + 1]],
                        Stroke::new(line_w, t::with_alpha(c, a.max(a2).max(10))),
                    );
                }
                let _ = c;
            }
            // the travelling heads: speed from the airflow, count from the airflow
            let heads = (3.0 + 5.0 * flow_f).round() as usize;
            for k in 0..heads {
                let t =
                    (anim_t * air_rate + k as f32 / heads as f32 + j as f32 * 0.13 + side * 0.05)
                        % 1.0;
                let i = (t * (pts.len() - 2) as f32) as usize;
                let a = pts[i];
                let b = pts[i + 1];
                let (c, share) = zone_at(a.y);
                let alpha = ((120.0 + 110.0 * (share / 100.0) as f32) * f_flow) as u8;
                let mid = egui::pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt().max(4.0);
                let ang = (b.y - a.y).atan2(b.x - a.x);
                let head = Shape::line(
                    vec![
                        egui::pos2(mid.x - ang.cos() * len, mid.y - ang.sin() * len),
                        egui::pos2(mid.x + ang.cos() * len, mid.y + ang.sin() * len),
                    ],
                    Stroke::new(line_w + 0.9, t::with_alpha(c, alpha.max(24))),
                );
                p.add(head);
            }
        }
    }

    // ---- the water: streaks from the nozzle bank down through the fill, the rain zone and into the basin
    let water_flow = run
        .output
        .as_ref()
        .map(|o| o.water_flow_m3_hr)
        .unwrap_or(700.0);
    let streaks =
        m::water_streak_count(water_flow).min(if phone { 8 } else { m::MAX_WATER_STREAKS });
    let top_y = l
        .layers
        .first()
        .map(|r| r.top() - 2.0)
        .unwrap_or(l.fill_band.top());
    let bottom_y = l.water_surface_y;
    // Issue #91: the water carries its own temperature down the tower. It enters at `hot_water_c` (the spray
    // header, the top) and leaves at `cold_water_c` (the basin, the bottom), so a streak is tinted by where
    // it is: warm near the nozzles, cool by the water surface. The two ends are the run's own temperatures;
    // the walk between them is the pass's look (`water.temperature` in the seams table).
    // The engine returns the cold side and the range, so the hot end is the recorded sum (the duty panel
    // prints the pair from the duty inputs; here the *output* pair is what the drawn water is tinted from).
    let (hot_c, cold_c) = run
        .output
        .as_ref()
        .map(|o| (o.cold_water_c + o.range_c, o.cold_water_c))
        .unwrap_or((37.0, 32.0));
    let span = (bottom_y - top_y).max(1.0);
    // How far apart the run's own two water temperatures are sets how much of the warm end shows
    // (`water.temperature` in the seams table; the rule itself is in the seams crate).
    let ramp = m::water_ramp(hot_c, cold_c);
    // How far down the fall the run's heat reaches: a wide spread walks warm most of the way to the basin,
    // a narrow one shows a short hot band under the spray and cools almost at once.
    let hot_reach = 0.35 + 0.45 * ramp;
    let hue = |fall_frac: f32| -> f32 { ((1.0 - fall_frac) / hot_reach).clamp(0.0, 1.0) };
    // Issue #91: the *wash* - the temperature read at a glance, before the streaks. Sixteen bands from the
    // spray down to the water surface, each carrying its own height's colour: warm at the top, cool at the
    // basin. The structure and the drops are drawn over it; the band alpha decays down the fall, so the
    // gradient reads as "hot water entering, cooled water leaving" without shouting over the section.
    if !phone {
        let bands = 16;
        for i in 0..bands {
            let t0 = i as f32 / bands as f32;
            let t1 = (i as f32 + 1.0) / bands as f32;
            let y0 = top_y + span * t0;
            let y1 = top_y + span * t1;
            let mid_t = 1.0 - (t0 + t1) * 0.5; // 1.0 at the nozzle, 0.0 at the surface
            let u = hue(1.0 - mid_t); // 1.0 at the nozzle, 0.0 once the fall has cooled
            let c = t::water_tint(u);
            let a = 64.0 * f_water * (0.25 + 0.75 * u);
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(l.fill_band.left(), y0),
                    egui::pos2(l.fill_band.right(), y1),
                ),
                0.0,
                t::with_alpha(c, a as u8),
            );
        }
    }
    for k in 0..streaks {
        let x = l.fill_band.left() + l.fill_band.width() * (k as f32 + 0.5) / streaks as f32;
        let phase = (anim_t * water_rate + k as f32 / streaks as f32) % 1.0;
        let y = top_y + span * phase;
        let len = if phone { 14.0 } else { 22.0 };
        // The hue is the run's own walk down the fall: 1.0 at the nozzle (hot), 0.0 at the water surface.
        let c_head = t::water_tint(hue((y - top_y) / span));
        let c_tail = t::water_tint(hue(((y + len).min(bottom_y) - top_y) / span));
        for (i, tt) in [0.0_f32, 0.5, 1.0].into_iter().enumerate() {
            let a = c_head.lerp_to_gamma(c_tail, tt);
            let yy = y + len * tt;
            let seg = len / 3.0;
            p.line_segment(
                [egui::pos2(x, yy), egui::pos2(x, (yy + seg).min(bottom_y))],
                Stroke::new(
                    if phone { 1.4 } else { 2.0 },
                    t::with_alpha(a, (200.0 * f_water) as u8),
                ),
            );
            let _ = i;
        }
        // the faint thread the drops are falling along, its tint following the same ramp
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(x, bottom_y)],
            Stroke::new(
                0.8,
                t::with_alpha(
                    t::water_tint(hue((y - top_y) / span) * 0.9),
                    (40.0 * f_water) as u8,
                ),
            ),
        );
    }
    let _ = m::MIN_WATER_STREAKS;
    side_unused();
}

/// A Catmull-Rom sample through the anchors: the curve the streamlines travel along.
fn catmull(anchors: &[egui::Pos2], per_segment: usize) -> Vec<egui::Pos2> {
    if anchors.len() < 2 {
        return anchors.to_vec();
    }
    let mut out = Vec::with_capacity(per_segment * anchors.len());
    for i in 0..anchors.len() - 1 {
        let p0 = anchors[i.saturating_sub(1)];
        let p1 = anchors[i];
        let p2 = anchors[i + 1];
        let p3 = anchors[(i + 2).min(anchors.len() - 1)];
        for s in 0..per_segment {
            let t = s as f32 / per_segment as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);
            out.push(egui::pos2(x, y));
        }
    }
    out.push(*anchors.last().unwrap());
    out
}

fn side_unused() {}

/// What a bay's own frame paints (issue #86, defect (b)): the frame's colour and width, and the wash (a
/// fill) or `None`. A bay carries an accent only while something is on it - the part being carried is
/// accepted by it, the pointer is dragging over it, its picker or tap-detail card is open, or the keyboard
/// is on it ([`Visual::bay_in_focus`]). At rest every bay is the same dashed slate frame with no fill;
/// before this rule the *default* selected slot's accent and wash sat on the fan bay with nothing selected
/// anywhere in the app, reading as an orphaned selection marker (the stray highlight the conductor found on
/// live staging).
fn bay_paint(
    vis: &Visual,
    slot: Slot,
    accepts: bool,
    refuses: bool,
) -> (Color32, f32, Option<(Color32, u8)>) {
    let dragged = vis
        .drag
        .as_ref()
        .map(|d| d.over == Some(slot))
        .unwrap_or(false);
    let (color, width) = if dragged {
        let v = vis.drag.as_ref().map(|d| &d.verdict).unwrap_or(&Ok(()));
        (state::verdict_color(v), 2.4)
    } else if accepts {
        (t::VALID, 1.7)
    } else if refuses {
        (t::with_alpha(t::SLOT, 70), 1.0)
    } else if vis.bay_in_focus(slot) {
        (t::with_alpha(t::PRIMARY, 200), 1.4)
    } else {
        (t::SLOT, 1.1)
    };
    let wash = if accepts {
        Some((t::VALID, 30))
    } else if vis.bay_in_focus(slot) {
        Some((t::PRIMARY, 12))
    } else {
        None
    };
    (color, width, wash)
}

/// The line a bay shows while a part is carried over it: the catalog's own verdict, in its own words
/// (issue #86 AC 3). A refusal is the engine's sentence - `check_drop`'s own `Err` string, byte for byte -
/// never a paraphrase, and the tests below compare it against that string so a rewrite fails here.
fn carried_line(part: &PartRef, v: &Result<(), String>) -> (String, Color32) {
    match v {
        Ok(()) => (format!("drop · accepts {}", part.id), t::VALID),
        Err(e) => (e.clone(), t::with_alpha(t::MUTED, 220)),
    }
}

/// Apply a drop through the catalog, and record what happened for the strip and the frame.
fn drop_part(
    cat: Option<&Catalog>,
    draft: Option<&mut Draft>,
    vis: &mut Visual,
    slot: Slot,
    part: &PartRef,
) {
    let Some(cat) = cat else { return };
    match draft {
        Some(d) => match apply_drop(cat, &mut d.0, slot, part, vis.selected_layer) {
            Ok(msg) => {
                vis.flash = Some(Flash {
                    ok: true,
                    text: msg,
                });
                vis.drag = None;
                vis.selected_slot = slot;
                // Round 2, change A: a drop dismisses the picker it was dragged from. Without this the card
                // the user just dropped would stay on screen, offering to replace what it already replaced.
                vis.picker = None;
                vis.picker_rect = [0.0; 4];
                if slot == Slot::Fill {
                    vis.selected_layer = vis
                        .selected_layer
                        .min(d.0.fill_layers.len().saturating_sub(1));
                }
            }
            Err(e) => {
                // Issue #86 AC 3: the strip speaks the catalog's own sentence - `apply_drop` returns
                // `check_drop`'s `Err` unchanged (it calls it first) and this flash carries it verbatim,
                // with no prefix of its own. The test below compares this text against `check_drop`'s.
                vis.flash = Some(Flash { ok: false, text: e })
            }
        },
        None => {
            vis.flash = Some(Flash {
                ok: false,
                text: "the calculation is not available yet".into(),
            })
        }
    }
}

// ======================================================================================= control bar

// =================================================================================== the fan's speed

/// #91 round 2: **fan speed has one implementation and lives where the fan lives.** The Instrument reaches
/// it through the fan bay's tap-detail (`answer::detail_card`); the Curves view carries the same row under
/// its chart. Nothing else paints an rpm control - the old control bar is gone, and with it the duplicate
/// the round-2 brief called out.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fan_speed_controls(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    run: &Run,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    let btn_h = if phone { 44.0 } else { 28.0 };
    let [lo, hi] = draft.0.fan.allowed_speed_ratio;
    let nominal_rpm = draft.0.fan.nominal_rpm;
    let rpm_range = m::rpm_range(draft.0.fan.allowed_speed_ratio, nominal_rpm);
    // ---- the knob: tachometer, the rpm, the slider over the record's band
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = if phone { 6.0 } else { 10.0 };
        let (tr, _) = ui.allocate_exact_size(
            egui::vec2(
                if phone { 52.0 } else { 64.0 },
                if phone { 40.0 } else { 44.0 },
            ),
            Sense::hover(),
        );
        tacho(ui, tr, draft.0.speed_ratio, lo, hi);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(m::rpm_text(draft.0.speed_ratio, nominal_rpm))
                        .size(if phone { 22.0 } else { 24.0 })
                        .color(t::INK)
                        .family(t::family_mono_med()),
                );
                ui.label(RichText::new("rpm").size(11.0).color(t::MUTED));
            });
            ui.label(
                RichText::new(format!(
                    "{} % of rated speed",
                    t::num::pct(draft.0.speed_ratio * 100.0)
                ))
                .size(10.0)
                .color(t::MUTED)
                .family(t::family_mono_med()),
            );
        });
        if let (Some(nominal), Some((rpm_lo, rpm_hi))) = (nominal_rpm, rpm_range) {
            // The control's own grid is whole rpm and the value it is handed starts on that grid
            // (`.round()`): egui then has nothing to snap, so it never writes a ratio nobody authored.
            let shown = m::rpm(draft.0.speed_ratio, Some(nominal))
                .unwrap_or(rpm_lo)
                .round();
            let mut rpm = shown;
            let w = (ui.available_width() - 4.0).clamp(140.0, 460.0);
            ui.spacing_mut().slider_width = w;
            let sr = ui.add_sized(
                egui::vec2(w, btn_h),
                egui::Slider::new(&mut rpm, rpm_lo..=rpm_hi)
                    .step_by(1.0)
                    .fixed_decimals(0)
                    .show_value(false)
                    .trailing_fill(true),
            );
            let sr = hit(hits, "ctl:rpm-slider", sr);
            if sr.changed() && (rpm - shown).abs() > 0.5 {
                draft.0.speed_ratio = m::ratio_from_rpm(rpm, nominal);
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!(
                        "{rpm:.0} rpm ({} % of rated speed)",
                        t::num::pct(draft.0.speed_ratio * 100.0)
                    ),
                });
            }
        } else {
            ui.label(
                RichText::new("the record states no rated speed")
                    .size(10.5)
                    .color(t::MUTED),
            );
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let step_w = if phone { 44.0 } else { 30.0 };
        if hit(
            hits,
            "ctl:rpm-down",
            ui.add_sized(egui::vec2(step_w, btn_h), egui::Button::new("−")),
        )
        .on_hover_text("fan speed down ([)")
        .clicked()
        {
            draft.0.speed_ratio = (draft.0.speed_ratio - 0.02).clamp(lo, hi);
            vis.flash = Some(Flash {
                ok: true,
                text: format!(
                    "rpm {} (one step down)",
                    m::rpm_text(draft.0.speed_ratio, nominal_rpm)
                ),
            });
        }
        if hit(
            hits,
            "ctl:rpm-up",
            ui.add_sized(egui::vec2(step_w, btn_h), egui::Button::new("+")),
        )
        .on_hover_text("fan speed up (])")
        .clicked()
        {
            draft.0.speed_ratio = (draft.0.speed_ratio + 0.02).clamp(lo, hi);
            vis.flash = Some(Flash {
                ok: true,
                text: format!(
                    "rpm {} (one step up)",
                    m::rpm_text(draft.0.speed_ratio, nominal_rpm)
                ),
            });
        }
        let at_fixture = (draft.0.speed_ratio - vis.reset_ratio).abs() < 1e-9;
        if hit(
            hits,
            "ctl:reset",
            ui.add_enabled(
                !at_fixture,
                egui::Button::new(format!("rated {} %", t::num::pct(vis.reset_ratio * 100.0)))
                    .min_size(egui::vec2(if phone { 72.0 } else { 62.0 }, btn_h)),
            ),
        )
        .on_hover_text("back to the recorded fan speed (R)")
        .clicked()
        {
            draft.0.speed_ratio = vis.reset_ratio.clamp(lo, hi);
            vis.flash = Some(Flash {
                ok: true,
                text: format!(
                    "back to the recorded fan speed, {} %",
                    t::num::pct(vis.reset_ratio * 100.0)
                ),
            });
        }
        // Issue #59: a ratio outside the record's band reaches the engine and comes back as its named
        // limit - the row shows that limit where the numbers would be.
        if let Some(limit) = run.output.as_ref().and_then(|o| {
            o.validation
                .iter()
                .find(|limit| limit.field == "fan.speedRatio")
        }) {
            ui.label(
                RichText::new(format!(
                    "outside {}–{} % · no result",
                    t::num::pct(limit.min.unwrap_or(lo) * 100.0),
                    t::num::pct(limit.max.unwrap_or(hi) * 100.0)
                ))
                .size(10.5)
                .color(t::AMBER),
            );
        }
    });
}

/// Issue #91: the nozzle bank's two controls (an editor value, not an engine input) and the coverage badge.
/// The arrangement sentence, the cone geometry and the "illustrative" basis are in the notes drawer; the
/// badge keeps its rect (`badge:coverage`) and its amber "illustrative" signal.
pub(crate) fn nozzle_bank(
    ui: &mut egui::Ui,
    draft: &Draft,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    let pitch = m::effective_pitch(
        vis.nozzle_spacing_m,
        vis.nozzle_pattern == Pattern::Staggered,
    );
    let half = m::spray_half_angle_deg(draft.0.nozzle.orifice_diameter_m);
    let cone_r = m::spray_cone_radius_m(draft.0.tower.spray_zone_height_m, half);
    let cov = m::coverage_fraction(pitch, cone_r);
    let h = if phone { 44.0 } else { 28.0 };
    ui.horizontal_wrapped(|ui| {
        ui.label(t::eyebrow("nozzles"));
        let mut spacing = vis.nozzle_spacing_m;
        ui.spacing_mut().slider_width = if phone { 150.0 } else { 96.0 };
        let sp = ui.add_sized(
            egui::vec2(if phone { 210.0 } else { 150.0 }, h),
            egui::Slider::new(&mut spacing, 0.3..=2.0)
                .fixed_decimals(2)
                .suffix(" m"),
        );
        let sp = hit(hits, "ctl:spacing", sp).on_hover_text("nozzle pitch");
        if sp.changed() {
            vis.nozzle_spacing_m = spacing;
            vis.flash = Some(Flash {
                ok: true,
                text: format!("nozzle pitch {spacing:.2} m"),
            });
        }
        for p in [Pattern::SingleRow, Pattern::Staggered] {
            let sel = vis.nozzle_pattern == p;
            let pr = ui.add(
                egui::Button::selectable(
                    sel,
                    RichText::new(p.name()).size(11.0).color(if sel {
                        t::PRIMARY
                    } else {
                        t::MUTED
                    }),
                )
                .min_size(egui::vec2(0.0, h)),
            );
            let pr = hit(hits, &format!("ctl:pattern:{}", p.slug()), pr);
            if pr.clicked() {
                vis.nozzle_pattern = p;
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!("nozzle bank: {} pattern", p.name()),
                });
            }
        }
        // the pitch steppers: a slider alone is a coarse instrument on a phone, and the same one step
        // down / up the fan row carries (0.05 m of pitch per tap, clamped to the bank's own band).
        let step_w = if phone { 44.0 } else { 28.0 };
        if hit(
            hits,
            "ctl:spacing-down",
            ui.add_sized(egui::vec2(step_w, h), egui::Button::new("−")),
        )
        .on_hover_text("tighter pitch")
        .clicked()
        {
            vis.nozzle_spacing_m = (vis.nozzle_spacing_m - 0.05).clamp(0.3, 2.0);
            vis.flash = Some(Flash {
                ok: true,
                text: format!("nozzle pitch {:.2} m", vis.nozzle_spacing_m),
            });
        }
        if hit(
            hits,
            "ctl:spacing-up",
            ui.add_sized(egui::vec2(step_w, h), egui::Button::new("+")),
        )
        .on_hover_text("wider pitch")
        .clicked()
        {
            vis.nozzle_spacing_m = (vis.nozzle_spacing_m + 0.05).clamp(0.3, 2.0);
            vis.flash = Some(Flash {
                ok: true,
                text: format!("nozzle pitch {:.2} m", vis.nozzle_spacing_m),
            });
        }
        let pct = ui.label(
            RichText::new(format!("{:.0}%", cov * 100.0))
                .size(12.0)
                .color(t::AMBER)
                .family(t::family_mono_med()),
        );
        let badge = chip(
            ui,
            "illustrative",
            t::AMBER,
            t::AMBER_SOFT,
            t::with_alpha(t::AMBER, 120),
        )
        .on_hover_text(
            "coverage: the flat-area overlap of two neighbouring cones - no distribution model",
        );
        let r = badge.rect.union(pct.rect);
        hits.0.push((
            "badge:coverage".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
    });
}

/// A tachometer: the same speed ratio the blades spin at, drawn as an instrument.
pub(crate) fn tacho(ui: &mut egui::Ui, rect: Rect, ratio: f64, lo: f64, hi: f64) {
    let p = ui.painter_at(rect);
    let centre = egui::pos2(rect.center().x, rect.bottom() - 6.0);
    let r = (rect.height() - 12.0).min(rect.width() * 0.5) - 4.0;
    let a0 = 200_f32.to_radians();
    let a1 = 340_f32.to_radians();
    let at = |f: f32| {
        let a = a0 + (a1 - a0) * f;
        egui::pos2(centre.x + r * a.cos(), centre.y + r * a.sin() * 0.9)
    };
    let mut pts = Vec::new();
    for i in 0..=24 {
        pts.push(at(i as f32 / 24.0));
    }
    p.add(Shape::line(pts, Stroke::new(2.0, t::LINE)));
    for i in 0..=4 {
        let f = i as f32 / 4.0;
        let a = a0 + (a1 - a0) * f;
        let inner = egui::pos2(
            centre.x + (r - 6.0) * a.cos(),
            centre.y + (r - 6.0) * a.sin() * 0.9,
        );
        let outer = egui::pos2(centre.x + r * a.cos(), centre.y + r * a.sin() * 0.9);
        p.line_segment([inner, outer], Stroke::new(1.0, t::TICK));
    }
    // Issue #86: the needle reads the record's own band **without clamping** - when the operating point
    // leaves the band it points into the stall range (the span just past the band's end, drawn in the
    // limits' colour and named) instead of sticking on the band's end as if the fan were there.
    let (f, stall) = tacho_spans(ratio, lo, hi);
    if let Some((s0, s1)) = stall {
        let arc: Vec<egui::Pos2> = (0..=6)
            .map(|i| at(s0 + (s1 - s0) * i as f32 / 6.0))
            .collect();
        p.add(Shape::line(
            arc,
            Stroke::new(3.0, t::with_alpha(t::DANGER, 210)),
        ));
    }
    let a = a0 + (a1 - a0) * f;
    let tip = egui::pos2(
        centre.x + (r - 8.0) * a.cos(),
        centre.y + (r - 8.0) * a.sin() * 0.9,
    );
    let needle = if stall.is_some() {
        t::DANGER
    } else {
        t::PRIMARY
    };
    p.line_segment([centre, tip], Stroke::new(2.0, needle));
    p.circle_filled(centre, 3.0, needle);
    if let Some((s0, _)) = stall {
        // the word, so the state is not colour-only (and it sits on the side the needle left the band on)
        let at_txt = at(s0 + (if s0 < 0.0 { -0.02 } else { 0.02 }));
        p.text(
            egui::pos2(at_txt.x, at_txt.y - 2.0),
            Align2::CENTER_BOTTOM,
            "stall",
            FontId::new(9.0, t::family_semi()),
            t::DANGER,
        );
    }
    p.text(
        egui::pos2(rect.center().x, rect.top() + 2.0),
        Align2::CENTER_TOP,
        "rpm",
        FontId::new(9.0, t::family_semi()),
        t::MUTED,
    );
}

/// How far past the band's end the tacho's needle may ride, as a fraction of the sweep (issue #86).
pub const TACHO_STALL_SPAN: f32 = 0.25;

/// The tacho's two reads of an operating point (issue #86): `(needle, stall)` - the needle's position in
/// sweep fractions (it may ride past either end, into the stall range) and, when the point is outside the
/// fan record's own band, the stall span to draw. Inside the band the span is `None` and the needle is the
/// plain `fraction_of` reading. One function, so "inside the band" and "past the end" cannot drift apart.
pub fn tacho_spans(ratio: f64, lo: f64, hi: f64) -> (f32, Option<(f32, f32)>) {
    let f = m::band_fraction(ratio, [lo, hi]);
    if m::in_stable_band(ratio, [lo, hi]) {
        return (f.clamp(0.0, 1.0), None);
    }
    if f < 0.0 {
        (f.max(-TACHO_STALL_SPAN), Some((-TACHO_STALL_SPAN, 0.0)))
    } else {
        (
            f.min(1.0 + TACHO_STALL_SPAN),
            Some((1.0, 1.0 + TACHO_STALL_SPAN)),
        )
    }
}

// ============================================================================================== rail

#[allow(clippy::too_many_arguments)]
fn rail(
    ui: &mut egui::Ui,
    cat: Option<&Catalog>,
    mut draft: Option<&mut Draft>,
    run: &Run,
    anchor: &Anchor,
    vis: &mut Visual,
    _options: &StartOptions,
    hits: &mut HitMap,
    duty: &DutyRes,
    clip: &mut crate::clip::ClipProbe,
    info: &mut crate::clip::LayoutInfo,
    phone: bool,
) {
    // ---- round 4, item 4: the duty & site panel, at the top of this column above the read-out (a phone
    // calls it first in its own stack instead).
    if !phone {
        duty_panel::panel(
            ui,
            draft.as_deref_mut(),
            duty,
            run,
            vis,
            hits,
            clip,
            info,
            phone,
        );
        ui.add_space(8.0);
    }
    let Some(draft) = draft else { return };
    // The evidence gate: outside the recorded sweep the engine's re-expression is not evidence, so the rows
    // that depend on the duty say `out of fixture range` instead of printing a number. The rule itself lives
    // in `drafthouse_cockpit_seams::duty`; here it is applied.
    let verdict = duty_panel::verdict(duty, &draft.0.duty);
    let flow_ok = verdict.as_ref().map(|v| v.flow_in_range).unwrap_or(false);
    let duty_ok = verdict.as_ref().map(|v| v.in_range()).unwrap_or(false);

    // ---- the answer this instrument is tuned around, straight from Engine::run
    ui.label(t::eyebrow("read-out"));
    let readout_card = t::card_flat().show(ui, |ui| {
        match run.output.as_ref() {
            Some(o) => {
                // Round 3, item 5: the pressure is the engine's own fan/system solve at the flow beside it
                // (`airflow_m3_s` *is* that solve's flow). `data-pressure` still publishes the engine's
                // recorded air-path total, and the note below says which is which.
                let op_pa = op_pressure(o);
                duty_panel::gated_kv(
                    ui,
                    "readout.airflow",
                    "airflow",
                    &t::num::flow(o.airflow_m3_s),
                    "m³/s",
                    !flow_ok,
                    true,
                    clip,
                    info,
                );
                duty_panel::gated_kv(
                    ui,
                    "readout.total-pressure",
                    "total pressure",
                    // issue #137: one pressure unit on screen (Pa, kPa from 10 kPa); the mmWG copy
                    // stays in the output and the project file
                    &t::num::pa(op_pa),
                    "Pa",
                    !flow_ok,
                    true,
                    clip,
                    info,
                );
                duty_panel::gated_kv(
                    ui,
                    "readout.fan-power",
                    "fan power",
                    &t::fmt(o.fan_power_kw),
                    "kW",
                    !flow_ok,
                    true,
                    clip,
                    info,
                );
                duty_panel::gated_kv(
                    ui,
                    "readout.cold-water",
                    "cold water",
                    &t::fmt(o.cold_water_c),
                    "C",
                    !duty_ok,
                    false,
                    clip,
                    info,
                );
                duty_panel::gated_kv(
                    ui,
                    "readout.approach",
                    "approach",
                    &t::fmt(o.approach_c),
                    "C",
                    !duty_ok,
                    false,
                    clip,
                    info,
                );
                duty_panel::gated_kv(
                    ui,
                    "readout.capability",
                    "capability",
                    &t::fmt(o.capability_pct),
                    "%",
                    !duty_ok,
                    false,
                    clip,
                    info,
                );
                if !duty_ok {
                    // Which entry left the recorded domain, in the engine's own field names.
                    if let Some(v) = verdict.as_ref() {
                        for line in v.out_of_range.iter() {
                            ui.label(RichText::new(format!("• {line}")).size(9.5).color(t::AMBER));
                        }
                    }
                }
                // Issue #91: provenance and the recorded-run comparison are numbers now; the sentences
                // that explained them are in the notes drawer.
                ui.add_space(2.0);
                if let Some(a) = anchor.output.as_ref() {
                    let d_air = o.airflow_m3_s - a.airflow_m3_s;
                    let d_pa = op_pa - op_pressure(a);
                    ui.label(
                        RichText::new(format!(
                            "change since the run at {} % speed · {} m³/s · {} Pa",
                            t::num::pct(anchor.ratio * 100.0),
                            t::num::with_sign(t::num::flow(d_air.abs()), d_air),
                            t::num::with_sign(t::num::pa(d_pa.abs()), d_pa)
                        ))
                        .size(10.0)
                        .color(if d_air.abs() > 1e-9 || d_pa.abs() > 1e-9 {
                            t::AMBER
                        } else {
                            t::MUTED
                        })
                        .family(t::family_mono_med()),
                    );
                    ui.label(
                        RichText::new(format!(
                            "run air path, total {} Pa",
                            t::fmt(o.total_pressure_pa)
                        ))
                        .size(10.0)
                        .color(t::MUTED)
                        .family(t::family_mono_med()),
                    );
                }
                let _ = engine_label(&o.provenance.engine);
            }
            None => {
                ui.label(t::body("no result: this duty is outside a rated range."));
            }
        }
    });
    {
        // The card reports its own rect, so an evidence frame can prove *which* cards the right column
        // holds (round 3, item 3) instead of being taken on trust.
        let r = readout_card.response.rect;
        hits.0.push((
            "card:readout".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
    }

    // ---- the fill stack: order, depth, per-layer result, and the reorder/add/remove controls
    // (Round 3, item 3: the PRESSURE BY ZONE list that used to sit above this is gone - the section draws
    // the zone labels and one compact stacked bar, and this column keeps the read-out and the stack.)
    ui.add_space(8.0);
    let stack_head = ui
        .horizontal(|ui| {
            ui.label(t::eyebrow("fill stack · top first"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if hit(
                    hits,
                    "ctl:add-layer",
                    ui.add_sized(egui::vec2(104.0, 22.0), egui::Button::new("+ add layer")),
                )
                .clicked()
                {
                    let part = vis.selected_part.clone();
                    match add_layer(
                        cat.unwrap_or(&Catalog::default()),
                        &mut draft.0,
                        part.as_ref(),
                    ) {
                        Ok(msg) => {
                            vis.flash = Some(Flash {
                                ok: true,
                                text: msg,
                            })
                        }
                        Err(e) => vis.flash = Some(Flash { ok: false, text: e }),
                    }
                }
            });
        })
        .response
        .rect;
    {
        let head = "fill stack · top first";
        let avail = stack_head.width();
        let need = measure_text(ui, head, 9.5, egui::FontFamily::Proportional) + 104.0 + 12.0;
        clip.push("stack.header", head, stack_head, avail, need, 1);
    }
    let stack_card = t::card_flat().show(ui, |ui| {
        let n = draft.0.fill_layers.len();
        for i in 0..n {
            let fill_id = draft.0.fill_layers[i].fill_id.clone();
            let depth = draft.0.fill_layers[i].depth_m;
            let lr = layer_result(run, i).cloned();
            let selected = vis.selected_slot == Slot::Fill && vis.selected_layer == i;
            let frame = if selected { t::chip_frame(t::PRIMARY_SOFT, t::PRIMARY) } else { t::chip_frame(t::PANEL, t::LINE_SOFT) };
            frame.show(ui, |ui| {
                // Round 5, item 1(c): the layer's own row is the same two-value shape as the duty rows - the
                // identity on the left, the engine's per-layer result on the right. It measures itself and
                // puts the result on a second line when the column cannot hold both, instead of letting it
                // run off the right edge.
                let avail = ui.available_width();
                let ident = format!("{} {} {:.2} m", i + 1, fill_id, depth);
                let tail = lr.as_ref().map(|lr| {
                    if flow_ok {
                        format!("KaV/L {:.3} · {:.1} Pa", lr.kavl, lr.pressure_pa)
                    } else {
                        drafthouse_cockpit_seams::duty::OUT_OF_FIXTURE_RANGE.to_string()
                    }
                });
                let need = measure_text(ui, &ident, 11.0, egui::FontFamily::Proportional)
                    + tail
                        .as_ref()
                        .map(|x| measure_text(ui, x, 10.0, t::family_mono_med()))
                        .unwrap_or(0.0)
                    + 26.0;
                let wrap = tail.is_some() && need > avail;
                let row1 = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).size(10.0).color(t::MUTED).family(t::family_mono_med()));
                        ui.label(RichText::new(&fill_id).size(11.5).color(t::fill_color(&fill_id)).family(t::family_mono_med()));
                        ui.label(RichText::new(format!("{depth:.2} m")).size(10.5).color(t::INK_2).family(t::family_mono_med()));
                        if let Some(tail) = tail.as_ref().filter(|_| !wrap) {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(tail).size(10.0).color(if flow_ok { t::MUTED } else { t::AMBER }).family(t::family_mono_med()));
                            });
                        }
                    })
                    .response
                    .rect;
                let mut row_r = row1;
                if wrap {
                    if let Some(tail) = tail.as_ref() {
                        let r2 = ui
                            .horizontal(|ui| {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(tail).size(10.0).color(if flow_ok { t::MUTED } else { t::AMBER }).family(t::family_mono_med()));
                                });
                            })
                            .response
                            .rect;
                        row_r = row_r.union(r2);
                    }
                }
                clip.push(
                    format!("stack.layer{i}"),
                    match tail.as_ref() {
                        Some(tail) => format!("{ident}  {tail}"),
                        None => ident.clone(),
                    },
                    row_r,
                    avail,
                    need,
                    if wrap { 2 } else { 1 },
                );
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if hit(hits, &format!("layer:{i}:remove"), ui.add_sized(egui::vec2(24.0, 20.0), egui::Button::new("x"))).clicked() {
                            if let Some(msg) = remove_layer(&mut draft.0, i) {
                                vis.selected_layer = i.min(draft.0.fill_layers.len().saturating_sub(1));
                                vis.flash = Some(Flash { ok: true, text: msg });
                            }
                        }
                        if hit(hits, &format!("layer:{i}:down"), ui.add_sized(egui::vec2(24.0, 20.0), egui::Button::new("v"))).clicked() {
                            if let Some(msg) = move_layer(&mut draft.0, i, false) {
                                vis.selected_layer = (i + 1).min(draft.0.fill_layers.len().saturating_sub(1));
                                vis.flash = Some(Flash { ok: true, text: msg });
                            }
                        }
                        if hit(hits, &format!("layer:{i}:up"), ui.add_sized(egui::vec2(24.0, 20.0), egui::Button::new("^"))).clicked() {
                            if let Some(msg) = move_layer(&mut draft.0, i, true) {
                                vis.selected_layer = i.saturating_sub(1);
                                vis.flash = Some(Flash { ok: true, text: msg });
                            }
                        }
                        ui.label(RichText::new("reorder").size(9.5).color(t::MUTED));
                    });
                });
                // depth: the layer's own fill's heights (issue #136) - never the tower's total-stack list
                depth_row(ui, cat, &mut draft.0, i, hits);
            });
            ui.add_space(2.0);
        }
        ui.label(RichText::new("a drop on the fill bay replaces the selected layer's fill; the recorded multipliers are never re-invented here").size(9.5).color(t::MUTED));
    });
    {
        let r = stack_card.response.rect;
        hits.0.push((
            "card:fill-stack".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
    }
}

/// Issue #91: the internal host's command chips (the public host hides them). Round 5 drew them in the dock
/// with a sentence explaining them; the sentence is in the notes drawer.
fn host_actions(ui: &mut egui::Ui, options: &StartOptions) {
    if options.host.public {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(t::eyebrow("internal host"));
        for c in [
            cockpit::host::ServerCommand::SaveRevision {
                project: "tower selection".into(),
                note: "visual pass".into(),
            },
            cockpit::host::ServerCommand::ExportReportPdf {
                project: "tower selection".into(),
            },
            cockpit::host::ServerCommand::CompareLater {
                project: "tower selection".into(),
                against_revision: None,
            },
        ] {
            chip(ui, &c.label(), t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP);
        }
    });
}

// ===================================================================================== curves view

/// The operating point, the two curves the engine returns, and (round 3, item 4) the two CTI-style
/// performance charts. Nothing here is drawn by hand: the fan/system series and the KaV/L supply come back
/// from `Engine::run`, the recorded performance grid and the fill characteristics come from the fixture the
/// engine was built from, and every chart states its own basis on its own axes.
#[allow(clippy::too_many_arguments)]
fn curves_view(
    ui: &mut egui::Ui,
    draft: Option<&Draft>,
    cat: Option<&Catalog>,
    perf: Option<&crate::perf::PerfGrid>,
    run: &Run,
    anchor: &Anchor,
    phone: bool,
    charts: &mut crate::state::ChartStats,
    hits: &mut HitMap,
) {
    let Some(draft) = draft else {
        ui.label(t::body("the calculation is not available"));
        return;
    };
    ui.label(t::eyebrow("operating point — fan/system and performance"));
    ui.label(RichText::new(format!("every series below is calculated at {} % fan speed ({}), or read from the recorded performance tables. The dashed series is the recorded run for comparison.", t::num::pct(draft.0.speed_ratio * 100.0), crate::screens::kit::part(&draft.0.fan.id))).size(10.5).color(t::MUTED));
    ui.add_space(6.0);

    let Some(o) = run.output.as_ref() else {
        t::card().show(ui, |ui| {
            ui.label(t::heading("no run"));
            ui.label(t::body(
                "this duty is outside a rated range, so there is no curve to plot.",
            ));
        });
        return;
    };

    let fan_series = ChartSeries {
        label: format!(
            "fan curve at {} % speed",
            t::num::pct(draft.0.speed_ratio * 100.0)
        ),
        pts: o
            .fan_system_curve
            .fan
            .points
            .iter()
            .map(|p| (p.x, p.y))
            .collect(),
        gaps: Vec::new(),
        color: t::PRIMARY,
        dashed: false,
    };
    let system_series = ChartSeries {
        label: "tower system curve (air-path total)".into(),
        pts: o
            .fan_system_curve
            .system
            .points
            .iter()
            .map(|p| (p.x, p.y))
            .collect(),
        gaps: Vec::new(),
        color: t::AIR,
        dashed: false,
    };
    let mut fan_set = vec![fan_series, system_series];
    if let Some(a) = anchor.output.as_ref() {
        fan_set.push(ChartSeries {
            label: "recorded run (ratio 0.78)".into(),
            pts: a
                .fan_system_curve
                .fan
                .points
                .iter()
                .map(|p| (p.x, p.y))
                .collect(),
            gaps: Vec::new(),
            color: t::with_alpha(t::INK_2, 170),
            dashed: true,
        });
    }
    let op = o.fan_system_curve.operating_point;
    // item 5: the number the fan/system chart marks, published so a frame can compare it with the
    // read-out (which reports the same field).
    charts.fan_pa = op.y;

    let h = if phone { 210.0 } else { 250.0 };
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(720.0), h),
        Sense::hover(),
    );
    // The charts publish their rects: an evidence frame can scroll the one it is about into view, and the
    // claim lands on the chart rather than on the panel around it.
    hits.0.push((
        "chart:fan-system".to_string(),
        [rect.min.x, rect.min.y, rect.width(), rect.height()],
    ));
    // Round 3, item 5: the y axis says which pressure it plots, the marker is the engine's own crossing, and
    // the read-out in the instrument column reports the same number.
    chart(
        ui,
        rect,
        &o.fan_system_curve.x_label,
        "total pressure, Pa",
        false,
        false,
        &fan_set,
        &[Marker {
            x: op.x,
            y: op.y,
            label: format!(
                "operating point · {} m³/s, {} Pa",
                t::num::flow(op.x),
                t::num::pa(op.y)
            ),
            color: t::PRIMARY,
            above: true,
        }],
        &[],
        &[],
    );
    ui.horizontal(|ui| {
        for s in fan_set.iter() {
            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 3.0), Sense::hover());
            ui.painter()
                .rect_filled(r, egui::CornerRadius::same(1), s.color);
            ui.label(RichText::new(&s.label).size(10.0).color(t::MUTED));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            kv(
                ui,
                "operating point",
                &format!("{} m³/s · {}", t::num::flow(op.x), t::num::pa(op.y)),
                "Pa",
                true,
            );
        });
    });
    ui.label(RichText::new(format!("the crossing is where the fan curve meets the tower's resistance; the speed slider recalculates it and the crossing moves with the fan curve. Pressure basis: {} ({}), the same basis as the tower system curve, and the same number the instrument's read-out shows.", draft.0.fan.pressure_basis, crate::screens::kit::part(&draft.0.fan.id))).size(9.5).color(t::MUTED));

    // ---- round 3, item 4a: cold water vs entering wet bulb -------------------------------------------
    //
    // The series is the fixture's recorded wet-bulb sweep for the fill the fixture's anchor names, and the
    // points the engine's own sweep rejected are drawn as open markers rather than dropped. The brief asks
    // for a family of three water-flow lines; the fixture does not carry them (its sweeps are one-parameter),
    // so the two flow points it *does* record at the design wet bulb are marked, and the chart says in words
    // which series is missing. Nothing is computed to fill the gap.
    ui.add_space(12.0);
    ui.label(t::eyebrow(
        "thermal performance — cold water vs entering wet bulb",
    ));
    let h3 = if phone { 210.0 } else { 240.0 };
    match perf {
        Some(grid) => {
            let depth = grid
                .fill_depth_m
                .map(|d| format!("{d:.2} m"))
                .unwrap_or_else(|| "the recorded depth".to_string());
            // item 4a: the flow family is absent from this fixture, so 0 lines are drawn and the chart
            // says so in words. The count is published, so the frame that claims "no family" can prove it.
            charts.wb_lines = 0;
            charts.wb_pts = grid.wb.pts.iter().filter(|p| p.ok).count();
            let wb_series = ChartSeries {
                label: format!(
                    "recorded wet-bulb sweep · {} at {} · design flow {:.0} kg/s",
                    grid.fill_id, depth, grid.design_flow_kg_s
                ),
                pts: grid
                    .wb
                    .pts
                    .iter()
                    .filter(|p| p.ok)
                    .map(|p| (p.x, p.y))
                    .collect(),
                gaps: grid
                    .wb
                    .pts
                    .iter()
                    .filter(|p| !p.ok)
                    .map(|p| (p.x, p.y))
                    .collect(),
                color: t::PRIMARY,
                dashed: false,
            };
            let mut markers_a = vec![Marker {
                x: draft.0.duty.wet_bulb_c,
                y: o.cold_water_c,
                label: format!(
                    "duty point · {} m³/h, wet bulb {} °C, cold water {} °C",
                    t::num::flow(o.water_flow_m3_hr),
                    t::num::temp(draft.0.duty.wet_bulb_c),
                    t::num::temp(o.cold_water_c)
                ),
                color: t::AMBER,
                above: true,
            }];
            if let Some(p) = grid.flow_at_fraction(0.9) {
                markers_a.push(Marker {
                    x: grid.design_wb_c,
                    y: p.y,
                    label: format!(
                        "90 % flow · {} kg/s, {} °C",
                        t::num::flow(p.x),
                        t::num::temp(p.y)
                    ),
                    color: t::WATER,
                    above: false,
                });
            }
            let (r, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(720.0), h3),
                Sense::hover(),
            );
            hits.0.push((
                "chart:wetbulb".to_string(),
                [r.min.x, r.min.y, r.width(), r.height()],
            ));
            chart(
                ui,
                r,
                "entering wet bulb, °C",
                "cold water, °C",
                false,
                false,
                &[wb_series],
                &markers_a,
                &[],
                &[],
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "100 % flow = the design {} kg/s ({} m³/h) · the duty point sits on the sweep at the design wet bulb {} °C",
                        t::num::flow(grid.design_flow_kg_s),
                        t::num::flow(grid.design_flow_m3_hr),
                        t::num::temp(grid.design_wb_c)
                    ))
                    .size(10.0)
                    .color(t::MUTED),
                );
            });
            let inf_hi = grid.wb.pts.iter().filter(|p| !p.ok).count();
            ui.label(
                RichText::new(format!(
                    "no 90 % / 110 % water-flow lines: the recorded sweeps are one-parameter (wet bulb at the design flow, flow at the design wet bulb), and 110 % of the design flow has no result; {inf_hi} wet-bulb samples above the design point are drawn as open markers for the same reason. Nothing is computed to fill either gap."
                ))
                .size(9.5)
                .color(t::AMBER),
            );
        }
        None => {
            let (r, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(720.0), h3),
                Sense::hover(),
            );
            chart(
                ui,
                r,
                "entering wet bulb, °C",
                "cold water, °C",
                false,
                false,
                &[],
                &[],
                &[],
                &[],
            );
            ui.label(
                RichText::new("not recorded — there is no recorded wet-bulb sweep, so the axes stand alone and nothing is computed to fill them.")
                    .size(9.5)
                    .color(t::AMBER),
            );
        }
    }

    // ---- round 3, item 4b: KaV/L vs L/G, log-log -----------------------------------------------------
    charts.kavl_pts = 0;
    ui.add_space(12.0);
    ui.label(t::eyebrow("fill characteristic — KaV/L vs L/G (log-log)"));
    let dry_air = perf.map(|g| g.dry_air_loading_kg_m2_s).unwrap_or(0.0);
    let mut char_series: Vec<ChartSeries> = Vec::new();
    let mut stack_pts: Vec<(f64, f64)> = Vec::new();
    if let (Some(c), true) = (cat, dry_air > 0.0) {
        for (i, layer) in draft.0.fill_layers.iter().enumerate() {
            let Some(f) = c.fill(&layer.fill_id) else {
                continue;
            };
            let factor = layer.depth_m * layer.thermal_multiplier;
            let pts: Vec<(f64, f64)> = f
                .characteristic
                .points
                .iter()
                .map(|p| (p.water_loading_kg_m2_s / dry_air, p.kavl_per_m * factor))
                .collect();
            if stack_pts.is_empty() {
                stack_pts = pts.clone();
            } else if stack_pts.len() == pts.len() {
                for (k, p) in pts.iter().enumerate() {
                    stack_pts[k].1 += p.1;
                }
            }
            char_series.push(ChartSeries {
                label: format!(
                    "layer {} · {} at {:.2} m",
                    i + 1,
                    layer.fill_id,
                    layer.depth_m
                ),
                pts,
                gaps: Vec::new(),
                color: t::fill_color(&layer.fill_id),
                dashed: false,
            });
        }
    }
    let lg_op = perf.map(|g| g.water_loading_kg_m2_s / g.dry_air_loading_kg_m2_s.max(1e-9));
    charts.kavl_pts = char_series
        .iter()
        .map(|s| s.pts.len() + s.gaps.len())
        .sum::<usize>();
    let demand = perf.map(|g| g.fill_demand_kavl).unwrap_or(0.0);
    let supply: f64 = o.kavl_per_layer.iter().map(|l| l.kavl).sum();
    if char_series.len() > 1 {
        char_series.push(ChartSeries {
            label: "fitted stack (sum of the layers)".into(),
            pts: stack_pts.clone(),
            gaps: Vec::new(),
            color: t::INK,
            dashed: false,
        });
    }
    let mut markers_b: Vec<Marker> = Vec::new();
    if let (Some(lg), true) = (lg_op, supply > 0.0) {
        markers_b.push(Marker {
            x: lg,
            y: supply,
            label: format!("operating L/G {lg:.3} · fill KaV/L {supply:.3}"),
            color: t::PRIMARY,
            above: true,
        });
    }
    let hlines_b: Vec<(f64, String, Color32)> = if demand > 0.0 {
        vec![(
            demand,
            format!("recorded fill demand {demand:.3} at the design condition"),
            t::AMBER,
        )]
    } else {
        Vec::new()
    };
    let vlines_b: Vec<(f64, String, Color32)> = match lg_op {
        Some(lg) => vec![(lg, format!("operating L/G {lg:.2}"), t::WATER)],
        None => Vec::new(),
    };
    let (r2, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(720.0), h3),
        Sense::hover(),
    );
    chart(
        ui,
        r2,
        "L/G (water loading / dry-air loading, -)",
        "KaV/L, -",
        true,
        true,
        &char_series,
        &markers_b,
        &hlines_b,
        &vlines_b,
    );
    hits.0.push((
        "chart:kavl".to_string(),
        [r2.min.x, r2.min.y, r2.width(), r2.height()],
    ));
    ui.horizontal_wrapped(|ui| {
        for s in char_series.iter() {
            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 3.0), Sense::hover());
            ui.painter()
                .rect_filled(r, egui::CornerRadius::same(1), s.color);
            ui.label(RichText::new(&s.label).size(10.0).color(t::MUTED));
        }
    });
    ui.label(
        RichText::new(format!(
            "KaV/L is the layer's Merkel number: KaV/L per metre × depth × the layer's thermal factor, the same sum the cold-water calculation uses. L/G = water loading ÷ dry-air loading, both the recorded run's ({} ÷ {} kg/m²s = {}); the characteristic is tabulated at the recorded dry-air loading {} kg/m²s.",
            t::num::sig(perf.map(|g| g.water_loading_kg_m2_s).unwrap_or(0.0), 3),
            t::num::sig(dry_air, 3),
            t::num::kavl(lg_op.unwrap_or(0.0)),
            t::num::sig(dry_air, 3)
        ))
        .size(9.5)
        .color(t::MUTED),
    );
    ui.label(
        RichText::new("the demand is one recorded value, not a curve: there is no recorded demand sweep, so it is drawn as a reference line at the fill's calculated KaV/L, and the operating L/G is marked where the run sits against it.")
            .size(9.5)
            .color(t::AMBER),
    );

    ui.add_space(8.0);
    if let Some(cat) = cat {
        t::card_flat().show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} · catalog {} · {} · inlet air recorded at {} °C dry bulb / {} °C wet bulb",
                    crate::screens::kit::method(&o.provenance.engine),
                    cat.catalog_id,
                    cat.catalog_status.to_lowercase(),
                    t::num::temp(cat.ambient.map(|a| a.dry_bulb_c).unwrap_or(0.0)),
                    t::num::temp(cat.ambient.map(|a| a.wet_bulb_c).unwrap_or(0.0))
                ))
                .size(9.5)
                .color(t::MUTED),
            );
        });
    }
}

/// One polyline on a chart, with the points the engine's own sweep marked infeasible kept separate (they
/// are drawn as open markers, never silently dropped).
struct ChartSeries {
    label: String,
    pts: Vec<(f64, f64)>,
    gaps: Vec<(f64, f64)>,
    color: Color32,
    dashed: bool,
}

/// One annotated point on a chart.
struct Marker {
    x: f64,
    y: f64,
    label: String,
    color: Color32,
    /// Where the label sits relative to the dot: true = above, false = below.
    above: bool,
}

/// A chart with axes, units, ticks, labelled points and reference lines. Pure painting: every number is
/// the engine's own output or the recorded fixture's. `x_log` / `y_log` switch an axis to a log scale -
/// the KaV/L vs L/G chart is drawn log-log, the way a fill characteristic is read.
#[allow(clippy::too_many_arguments)]
fn chart(
    ui: &mut egui::Ui,
    rect: Rect,
    x_label: &str,
    y_label: &str,
    x_log: bool,
    y_log: bool,
    series: &[ChartSeries],
    markers: &[Marker],
    hlines: &[(f64, String, Color32)],
    vlines: &[(f64, String, Color32)],
) {
    let p = ui.painter_at(rect);
    let pad_l = 50.0;
    let pad_b = 32.0;
    let pad_t = 16.0;
    let pad_r = 14.0;
    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + pad_l, rect.top() + pad_t),
        egui::pos2(rect.right() - pad_r, rect.bottom() - pad_b),
    );
    p.rect_filled(rect, egui::CornerRadius::same(4), t::RAIL_BG);
    p.rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        Stroke::new(1.0, t::LINE_SOFT),
        StrokeKind::Inside,
    );

    let tx = |v: f64| if x_log { v.max(1e-9).ln() } else { v };
    let ty = |v: f64| if y_log { v.max(1e-9).ln() } else { v };

    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for s in series {
        for (x, y) in s.pts.iter().chain(s.gaps.iter()) {
            xs.push(*x);
            ys.push(*y);
        }
    }
    for m in markers {
        xs.push(m.x);
        ys.push(m.y);
    }
    for (y, _, _) in hlines {
        ys.push(*y);
    }
    for (x, _, _) in vlines {
        xs.push(*x);
    }
    if xs.len() < 2 || ys.len() < 2 {
        // The brief's rule: if the fixture lacks a series, compute nothing and say so on the axes.
        p.text(
            egui::pos2(inner.center().x, inner.center().y - 8.0),
            Align2::CENTER_CENTER,
            "not recorded",
            FontId::new(11.5, t::family_semi()),
            t::AMBER,
        );
        p.text(
            egui::pos2(inner.center().x, inner.center().y + 8.0),
            Align2::CENTER_CENTER,
            "axes drawn; nothing is computed to fill them",
            FontId::new(9.5, t::family_mono_med()),
            t::MUTED,
        );
        p.text(
            egui::pos2(inner.center().x, rect.bottom() - 3.0),
            Align2::CENTER_BOTTOM,
            x_label,
            FontId::new(9.5, t::family_semi()),
            t::INK_2,
        );
        p.text(
            egui::pos2(rect.left() + 3.0, rect.top() + pad_t - 3.0),
            Align2::LEFT_BOTTOM,
            y_label,
            FontId::new(9.5, t::family_semi()),
            t::INK_2,
        );
        return;
    }
    let xlo_t = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let xhi_t = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let ylo_t = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    let yhi_t = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let (x0t, x1t) = if (xhi_t - xlo_t).abs() < 1e-9 {
        (xlo_t - 1.0, xhi_t + 1.0)
    } else {
        (xlo_t, xhi_t)
    };
    // A linear chart keeps 0 at the bottom (a pressure axis reads from zero); a log chart is padded in log
    // space on both ends.
    let (y0t, y1t) = if (yhi_t - ylo_t).abs() < 1e-9 {
        (ylo_t - 1.0, yhi_t + 1.0)
    } else if y_log {
        let pad = (ty(yhi_t) - ty(ylo_t)) * 0.08;
        (ty(ylo_t) - pad, ty(yhi_t) + pad)
    } else {
        (ylo_t.min(0.0), yhi_t)
    };
    let x_span = (x1t - x0t).max(1e-9);
    let y_span = (y1t - y0t).max(1e-9);
    let map = |x: f64, y: f64| {
        let fx = ((tx(x) - x0t) / x_span).clamp(0.0, 1.0) as f32;
        let fy = ((ty(y) - y0t) / y_span).clamp(0.0, 1.0) as f32;
        egui::pos2(
            inner.left() + inner.width() * fx,
            inner.bottom() - inner.height() * fy,
        )
    };
    // ---- ticks: five even steps on a linear axis, decades on a log one
    let ticks = || {
        let mut txs: Vec<f64> = Vec::new();
        let mut tys: Vec<f64> = Vec::new();
        if x_log {
            let (lo, hi) = (x0t.exp(), x1t.exp());
            for k in (lo.log10().floor() as i32)..=(hi.log10().ceil() as i32) {
                let v = 10f64.powi(k);
                if v >= lo * 0.98 && v <= hi * 1.02 {
                    txs.push(v);
                }
            }
        } else {
            for i in 0..=4 {
                txs.push(x0t + x_span * i as f64 / 4.0);
            }
        }
        if y_log {
            let (lo, hi) = (y0t.exp(), y1t.exp());
            for k in (lo.log10().floor() as i32)..=(hi.log10().ceil() as i32) {
                let v = 10f64.powi(k);
                if v >= lo * 0.98 && v <= hi * 1.02 {
                    tys.push(v);
                }
            }
        } else {
            for i in 0..=4 {
                tys.push(y0t + y_span * i as f64 / 4.0);
            }
        }
        (txs, tys)
    };
    let (tick_xs, tick_ys) = ticks();
    for v in tick_xs.iter() {
        let x = map(*v, ylo_t).x;
        p.line_segment(
            [egui::pos2(x, inner.top()), egui::pos2(x, inner.bottom())],
            Stroke::new(1.0, t::with_alpha(t::GRID, 200)),
        );
        p.text(
            egui::pos2(x, inner.bottom() + 4.0),
            Align2::CENTER_TOP,
            t::fmt(*v),
            FontId::new(9.0, t::family_mono_med()),
            t::MUTED,
        );
    }
    for v in tick_ys.iter() {
        let y = map(xlo_t, *v).y;
        p.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            Stroke::new(1.0, t::with_alpha(t::GRID, 200)),
        );
        p.text(
            egui::pos2(inner.left() - 5.0, y),
            Align2::RIGHT_CENTER,
            t::fmt(*v),
            FontId::new(9.0, t::family_mono_med()),
            t::MUTED,
        );
    }
    if x_log || y_log {
        p.text(
            egui::pos2(inner.right() - 2.0, inner.top() + 1.0),
            Align2::RIGHT_TOP,
            if x_log && y_log {
                "log-log"
            } else {
                "log axis"
            },
            FontId::new(9.0, t::family_semi()),
            t::with_alpha(t::MUTED, 200),
        );
    }
    p.text(
        egui::pos2(inner.center().x, rect.bottom() - 3.0),
        Align2::CENTER_BOTTOM,
        x_label,
        FontId::new(9.5, t::family_semi()),
        t::INK_2,
    );
    p.text(
        egui::pos2(rect.left() + 3.0, rect.top() + pad_t - 3.0),
        Align2::LEFT_BOTTOM,
        y_label,
        FontId::new(9.5, t::family_semi()),
        t::INK_2,
    );

    // ---- the reference lines, then the series, then the annotated points
    for (y, label, color) in hlines {
        let yy = map(xlo_t, *y).y;
        let mut dashes = Vec::new();
        Shape::dashed_line_many(
            &[egui::pos2(inner.left(), yy), egui::pos2(inner.right(), yy)],
            Stroke::new(1.2, t::with_alpha(*color, 200)),
            6.0,
            4.0,
            &mut dashes,
        );
        p.extend(dashes);
        p.text(
            egui::pos2(inner.right() - 3.0, yy - 2.0),
            Align2::RIGHT_BOTTOM,
            label,
            FontId::new(9.0, t::family_mono_med()),
            *color,
        );
    }
    for (x, label, color) in vlines {
        let xx = map(*x, ylo_t).x;
        p.line_segment(
            [egui::pos2(xx, inner.top()), egui::pos2(xx, inner.bottom())],
            Stroke::new(1.2, t::with_alpha(*color, 190)),
        );
        p.text(
            egui::pos2(xx + 3.0, inner.bottom() - 2.0),
            Align2::LEFT_BOTTOM,
            label,
            FontId::new(9.0, t::family_mono_med()),
            *color,
        );
    }
    for s in series {
        let pts: Vec<egui::Pos2> = s.pts.iter().map(|(x, y)| map(*x, *y)).collect();
        if pts.len() >= 2 {
            if s.dashed {
                let mut shapes = Vec::new();
                Shape::dashed_line_many(&pts, Stroke::new(1.4, s.color), 5.0, 4.0, &mut shapes);
                p.extend(shapes);
            } else {
                p.add(Shape::line(pts, Stroke::new(1.8, s.color)));
            }
        }
        // the engine's own infeasible samples: an open marker, so a rejected point is visible as a rejection
        for (x, y) in s.gaps.iter() {
            let c = map(*x, *y);
            p.circle_stroke(c, 2.6, Stroke::new(1.1, t::with_alpha(t::DANGER, 200)));
        }
    }
    for m in markers {
        let c = map(m.x, m.y);
        p.circle_filled(c, 4.0, m.color);
        p.circle_stroke(c, 7.0, Stroke::new(1.0, t::with_alpha(m.color, 160)));
        if m.label.is_empty() {
            continue;
        }
        let galley = p.layout_no_wrap(
            m.label.clone(),
            FontId::new(10.0, t::family_mono_med()),
            m.color,
        );
        let y = if m.above { c.y - 16.0 } else { c.y + 6.0 };
        let r = Rect::from_min_size(
            egui::pos2((c.x + 8.0).min(inner.right() - galley.size().x - 8.0), y),
            galley.size() + egui::vec2(8.0, 3.0),
        );
        p.rect_filled(r, egui::CornerRadius::same(3), t::with_alpha(t::BG, 228));
        p.galley(r.min + egui::vec2(4.0, 1.0), galley, m.color);
    }
}

// --------------------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    /// The phone's eight plates as the 390x844 frame measured them (the `plate-rects` dataset in
    /// `docs/design/small-screens-r1/instrument-390x844.json`), with the phone's own gap (2 px) and the
    /// column the live fit uses: the section's height (234 px) less the packer's floor and ceiling.
    const PHONE_PLATES: [f32; 8] = [29.0, 38.0, 29.0, 29.0, 36.0, 17.0, 17.0, 29.0];
    const PHONE_GAP: f32 = 2.0;
    const PHONE_COLUMN: f32 = 230.0;

    /// The stack the phone drew did **not** fit its column: that is the eight-pixel overlap the frame
    /// showed between the fan plate and the operating-point call-out, and the reason `scene_overlay` now
    /// fits the column before it packs it.
    #[test]
    fn the_measured_phone_stack_overflows_its_column() {
        assert!(!column_holds(&PHONE_PLATES, PHONE_GAP, PHONE_COLUMN));
    }

    /// Once the type has stepped down to fit, the packer's own promise: band order, every plate inside the
    /// column, and a gap between each pair - one column, so no two plates can share pixels.
    #[test]
    fn the_packed_column_keeps_every_plate_on_its_own_pixels() {
        let fitted: Vec<f32> = PHONE_PLATES.iter().map(|h| h * 0.94).collect();
        assert!(
            column_holds(&fitted, PHONE_GAP, PHONE_COLUMN),
            "the fit step must make the stack fit"
        );
        let targets: Vec<f32> = (0..fitted.len()).map(|i| 6.0 + i as f32 * 27.0).collect();
        let ys = pack_column(&targets, &fitted, 0.0, 0.0, PHONE_COLUMN, PHONE_GAP);
        assert_eq!(ys.len(), fitted.len());
        for i in 0..ys.len() {
            assert!(
                ys[i] >= 0.0 && ys[i] + fitted[i] <= PHONE_COLUMN + 0.01,
                "plate {i} leaves the column ({}..{})",
                ys[i],
                ys[i] + fitted[i]
            );
            if i > 0 {
                let gap = ys[i] - (ys[i - 1] + fitted[i - 1]);
                assert!(
                    gap >= PHONE_GAP - 0.01,
                    "plate {i} rides on plate {} by {:.1} px",
                    i - 1,
                    -gap
                );
            }
        }
    }

    /// The same promise at every plate count a section can draw (one to twelve), including stacks that only
    /// just fit: whatever the caller hands over, the packer keeps them apart.
    #[test]
    fn the_packed_column_never_overlaps_at_any_count() {
        for n in 1..=12usize {
            let h = PHONE_COLUMN / n as f32 - PHONE_GAP;
            let heights = vec![h; n];
            assert!(column_holds(&heights, PHONE_GAP, PHONE_COLUMN));
            let targets: Vec<f32> = (0..n).map(|i| 4.0 + i as f32 * 9.0).collect();
            let ys = pack_column(&targets, &heights, 2.0, 0.0, PHONE_COLUMN, PHONE_GAP);
            for i in 1..ys.len() {
                assert!(
                    ys[i] >= ys[i - 1] + heights[i - 1] + PHONE_GAP - 0.01,
                    "n={n}: plate {i} overlaps plate {}",
                    i - 1
                );
            }
            assert!(*ys.last().unwrap() + heights[n - 1] <= PHONE_COLUMN + 0.01);
        }
    }

    /// Issue #86, defect (b): **no bay is accented at rest.** A fresh `Visual` - the fixture's default fan
    /// slot, nothing else - paints every bay the same dashed slate frame with no fill. The stray light-blue
    /// highlight the conductor saw on live staging was the *default* slot's solid accent frame and its wash
    /// sitting on the fan bay with nothing selected anywhere in the app. The accent comes back the moment
    /// something is *on* a bay, and only on that bay.
    #[test]
    fn no_bay_is_accented_at_rest() {
        let vis = Visual::default();
        for slot in Slot::ALL {
            let (color, width, wash) = bay_paint(&vis, slot, false, false);
            assert_eq!((color, width), (t::SLOT, 1.1), "{slot:?} at rest");
            assert!(wash.is_none(), "{slot:?} carries a wash at rest");
        }
        // The picker on the fan bay: the accent frame and the quiet wash, on that bay and no other.
        let vis = Visual {
            picker: Some(Picker::new(Slot::Fan, 0, [0.0; 4], false)),
            ..Default::default()
        };
        let (color, _, wash) = bay_paint(&vis, Slot::Fan, false, false);
        assert_eq!(color, t::with_alpha(t::PRIMARY, 200));
        assert_eq!(wash, Some((t::PRIMARY, 12)));
        assert!(bay_paint(&vis, Slot::Drift, false, false).2.is_none());
        // ... and an accepting bay is filled even with nothing selected at all.
        assert_eq!(
            bay_paint(&vis, Slot::Drift, true, false).2,
            Some((t::VALID, 30))
        );
    }

    /// Issue #86 AC 3: **the screen speaks the catalog's own refusal.** The bay's carried line and the drop
    /// strip's flash are the engine's sentence, byte for byte - the same string `check_drop` returns (and
    /// `apply_drop` returns unchanged, because it calls `check_drop` first). The strip used to prefix
    /// "refused: " and the call-out used to paraphrase ("won't take AX-700"); either coming back fails here.
    #[test]
    fn a_refusal_is_spoken_in_the_catalogs_own_words() {
        use cockpit::fixture_engine::FixtureEngine;
        let fx = FixtureEngine::from_json(include_str!("../assets/fixture.json"))
            .expect("the fixture parses");
        let cat = Catalog::from_fixture(&fx);
        let input = fx.default_input();
        let part = PartRef::new(Class::Fan, "AX-700");
        let engine_reason =
            check_drop(&cat, &input, Slot::Fan, &part).expect_err("AX-700 is not a listed fan");
        // the line the bay shows while the part is carried over it
        let (line, _) = carried_line(&part, &Err(engine_reason.clone()));
        assert_eq!(
            line, engine_reason,
            "the bay repeats the catalog's sentence, it does not paraphrase it"
        );
        // and the strip's flash, through the real drop path
        let mut vis = Visual::default();
        let mut draft = Draft(input);
        drop_part(Some(&cat), Some(&mut draft), &mut vis, Slot::Fan, &part);
        let flash = vis.flash.expect("a refusal flash");
        assert!(!flash.ok);
        assert_eq!(
            flash.text, engine_reason,
            "the strip carries the same sentence, verbatim"
        );
        // the accepted case still reads as the screen's own line (that is a message, not a refusal)
        let (line, _) = carried_line(&part, &Ok(()));
        assert!(line.starts_with("drop · accepts"), "{line}");
    }

    /// Issue #86: **the stall range is shown when the operating point leaves the fan's own band.** Inside
    /// the band the needle is the plain reading and no stall span is drawn; past either end the needle rides
    /// into the stall span (a quarter of the sweep) and the span is returned so the tacho can draw it in the
    /// limits' colour and name it. AX-500's record: allowedSpeedRatio [0.70, 1.13].
    #[test]
    fn the_tacho_shows_the_stall_range_only_outside_the_band() {
        let (lo, hi) = (0.70, 1.13);
        let (n, stall) = tacho_spans(0.78, lo, hi);
        assert!((n - 0.186_046_5).abs() < 1e-3, "inside the band: {n}");
        assert!(stall.is_none(), "inside the band no stall span is drawn");
        for edge in [lo, hi] {
            assert!(tacho_spans(edge, lo, hi).1.is_none(), "the edge is inside");
        }
        // above the band: the needle rides past the end, and the stall span is the quarter past it
        let (n, stall) = tacho_spans(1.29, lo, hi);
        assert!(
            (1.0..=1.0 + TACHO_STALL_SPAN + 1e-6).contains(&n),
            "above: {n}"
        );
        assert_eq!(stall, Some((1.0, 1.0 + TACHO_STALL_SPAN)));
        // below the band: the same, on the other side
        let (n, stall) = tacho_spans(0.60, lo, hi);
        assert!((-TACHO_STALL_SPAN - 1e-6..0.0).contains(&n), "below: {n}");
        assert_eq!(stall, Some((-TACHO_STALL_SPAN, 0.0)));
        // a runaway operating point cannot send the needle off the dial
        let (n, _) = tacho_spans(99.0, lo, hi);
        assert!(n <= 1.0 + TACHO_STALL_SPAN + 1e-6);
    }

    /// Issue #100: the fit loop's own call site. The pure tests above pin the helpers
    /// (`column_holds`, `pack_column`) but cannot see whether the loop around them ran; the
    /// behaviour that actually keeps the plates apart is orchestrated inside `scene_overlay`
    /// itself - the type step-down, the drop stage, the packer. This test drives
    /// `scene_overlay` at the four sizes, on the app's own fonts and each frame's own section
    /// rect (`docs/design/small-screens-r1/instrument-*.json`'s `sceneRect` - the exact rect
    /// the shell handed the overlay when that frame was captured), and reads the label rects
    /// the call published. The rule is the frames check's own (`check.mjs`): no two labels
    /// share more than half a pixel on both axes.
    ///
    /// Two stacks, both of them states the app itself can hold: the fixture's recorded one,
    /// and the full one its own add path builds at `state::MAX_LAYERS`. The recorded columns
    /// at 390x844 and 1024x768 overflow before the loop runs and are fitted by the type
    /// step-down; the full stack at 1024x768 is the state where even the floor-sized type
    /// cannot fit, so the drop stage is the stage that saves it. Disable that stage and this
    /// test fails on that state, naming the pair the packer left sharing pixels.
    #[test]
    fn the_overlay_fit_loop_keeps_the_plates_apart_at_the_four_sizes() {
        use cockpit::fixture_engine::FixtureEngine;
        let fx = FixtureEngine::from_json(include_str!("../assets/fixture.json"))
            .expect("the fixture parses");
        let cat = Catalog::from_fixture(&fx);
        let engine = crate::engine_select::build(include_str!("../assets/fixture.json"), None)
            .expect("the build's engine");
        let recorded = fx.default_input();
        let mut full = recorded.clone();
        while full.fill_layers.len() < state::MAX_LAYERS {
            add_layer(&cat, &mut full, None).expect("the app's own add path holds a layer");
        }
        let stacks = [("recorded stack", &recorded), ("full stack", &full)];
        // The four frames' own section rects (`sceneRect`), phone flag included: the shell
        // handed `scene_overlay` exactly these rects.
        let sizes: [(&str, Rect, bool); 4] = [
            (
                "1280x720",
                Rect::from_min_size(egui::pos2(116.0, 58.0), egui::vec2(1152.0, 610.0)),
                false,
            ),
            (
                "1440x900",
                Rect::from_min_size(egui::pos2(116.0, 58.0), egui::vec2(1312.0, 790.0)),
                false,
            ),
            (
                "1024x768",
                Rect::from_min_size(egui::pos2(116.0, 358.0), egui::vec2(896.0, 358.0)),
                false,
            ),
            (
                "390x844",
                Rect::from_min_size(egui::pos2(8.0, 549.0), egui::vec2(374.0, 234.0)),
                true,
            ),
        ];
        for (stack, input) in stacks {
            let run = Run {
                output: Some(engine.run(input).expect("the duty runs")),
                error: None,
                ratio: input.speed_ratio,
            };
            for (label, area, phone) in sizes {
                let ctx = egui::Context::default();
                ctx.set_fonts(t::fonts());
                // one pass binds the fonts (`set_fonts` only queues; the app skips the same first frame)
                let mut warm = ctx.run_ui(egui::RawInput::default(), |_| {});
                warm.textures_delta.clear();
                let mut draft = Draft(input.clone());
                let mut vis = Visual::default();
                let mut hits = HitMap::default();
                let mut info = crate::clip::LayoutInfo::default();
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    scene_overlay(
                        ui,
                        area,
                        Some(&cat),
                        &mut draft,
                        &mut vis,
                        &run,
                        &ctx,
                        &mut hits,
                        &mut info,
                        0.0,
                        true,
                        phone,
                    );
                });
                out.textures_delta.clear();
                // the frames check's own overlap rule, over the rects `scene_overlay` just
                // published: >0.5 px on both axes, so the failure names the two labels, not a
                // counter
                let labels: Vec<(&str, [f32; 4])> = hits
                    .0
                    .iter()
                    .filter(|(key, _)| key.starts_with("label:"))
                    .map(|(key, rect)| (key.as_str(), *rect))
                    .collect();
                assert!(
                    labels.len() >= 8,
                    "{stack} at {label}: the overlay published only {} labels: {:?}",
                    labels.len(),
                    labels.iter().map(|(k, _)| *k).collect::<Vec<_>>()
                );
                let mut overlaps: Vec<String> = Vec::new();
                for i in 0..labels.len() {
                    for j in i + 1..labels.len() {
                        let a = labels[i].1;
                        let b = labels[j].1;
                        let x = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
                        let y = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
                        if x > 0.5 && y > 0.5 {
                            overlaps.push(format!(
                                "{} x {} ({x:.1}x{y:.1} px)",
                                labels[i].0, labels[j].0
                            ));
                        }
                    }
                }
                assert!(
                    overlaps.is_empty(),
                    "{stack} at {label}: the overlay column leaves its plates sharing pixels: {} \
                     (the app's own plate_overlaps counter reads {})",
                    overlaps.join(", "),
                    info.plate_overlaps
                );
                assert_eq!(
                    info.plate_overlaps, 0,
                    "{stack} at {label}: the app's own plate counter"
                );
            }
        }
    }

    /// Issue #135: a tap on a picker card picks the row, and on a phone the sheet shows every record
    /// without running off the canvas.
    ///
    /// `egui::Ui::dnd_drag_source` senses **drag only** (its own docs say so), so the card's
    /// `clicked()` could never be true: on a phone the bottom sheet is the only fill path, and every
    /// tap on it landed on nothing. The test drives the real `picker_controls` at the 390x844 phone
    /// geometry and reads the rects the widget itself published. Two things must hold together: each
    /// of the seven fill cards sits inside the canvas (the 44 % height cap used to put the last four
    /// below it - drawn nowhere, untappable), and a tap in the middle of an accepting card reaches the
    /// same `pick_part` path the keyboard takes, which fits the record and closes the sheet.
    ///
    /// Drop the click sense from `picker_row` and the tap lands on nothing: the sheet stays open and
    /// the stack never moves. Put the phone cap back to a share of the instrument and the card rects
    /// leave the canvas.
    #[test]
    fn a_tap_on_a_picker_card_picks_the_row_on_a_phone() {
        use cockpit::fixture_engine::FixtureEngine;
        let fx = FixtureEngine::from_json(include_str!("../assets/fixture.json"))
            .expect("the fixture parses");
        let cat = Catalog::from_fixture(&fx);
        let input = fx.default_input();
        // The rect the shell handed the overlay on the 390x844 frame: the instrument starts below the
        // 52 px phone header and ends at the canvas' bottom edge.
        let screen = Rect::from_min_size(egui::pos2(0.0, 52.0), egui::vec2(390.0, 689.0));
        // ...and the viewport that rect lives in. The phone sheet hangs off the *canvas*, not off the
        // instrument rect (`Area::anchor` places against `ctx.content_rect()`), so the egui screen has to
        // be the shell's own 390x741 canvas - with the default (unset) screen the anchor lands at y~10000
        // and every card reads as off-canvas.
        let viewport = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(390.0, 741.0));
        let ctx = egui::Context::default();
        ctx.set_fonts(t::fonts());
        let mut warm = ctx.run_ui(egui::RawInput::default(), |_| {});
        warm.textures_delta.clear();
        let mut draft = Draft(input);
        let mut vis = Visual {
            picker: Some(state::Picker::new(Slot::Fill, 0, [0.0; 4], false)),
            ..Default::default()
        };
        let mut hits = HitMap::default();
        let draw = |ctx: &egui::Context,
                    vis: &mut Visual,
                    draft: &mut Draft,
                    hits: &mut HitMap,
                    raw: egui::RawInput| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..raw
                },
                |ui| {
                    let ctx = ui.ctx().clone();
                    picker_controls(
                        &ctx,
                        vis,
                        Some(&cat),
                        Some(draft),
                        hits,
                        true,
                        screen,
                        Rect::NOTHING,
                    );
                },
            );
            out.textures_delta.clear();
        };
        // Two settled frames: the first binds the fonts and registers the cards, the second carries the
        // sense union the interaction pass reads.
        for _ in 0..2 {
            draw(
                &ctx,
                &mut vis,
                &mut draft,
                &mut hits,
                egui::RawInput::default(),
            );
        }
        let rows = state::picker_rows(&cat, Slot::Fill);
        assert!(
            rows.len() >= 3,
            "the fixture's fill catalog has only {} records",
            rows.len()
        );
        let cards: Vec<[f32; 4]> = rows
            .iter()
            .map(|p| {
                let key = format!("picker:fill:{}", p.id);
                *hits
                    .0
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, r)| r)
                    .unwrap_or_else(|| panic!("the sheet did not publish a rect for {key}"))
            })
            .collect();
        // Issue #135's first half: every record inside the canvas. The old 44 % cap stopped the sheet
        // at ~y 734 with card 3 clipped and cards 4..7 entirely below the drawn area.
        for (part, r) in rows.iter().zip(&cards) {
            assert!(
                r[1] >= screen.top() && r[1] + r[3] <= screen.bottom(),
                "{}: the fill card at y {}..{} leaves the canvas ({}..{})",
                part.id,
                r[1],
                r[1] + r[3],
                screen.top(),
                screen.bottom()
            );
        }
        // The second half: tap an accepting card and the pick lands.
        let index = rows
            .iter()
            .position(|p| check_drop(&cat, &draft.0, Slot::Fill, p).is_ok())
            .expect("the fixture holds a fill the recorded stack can take");
        let part = rows[index].clone();
        let r = cards[index];
        let centre = egui::pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        let mut down = egui::RawInput::default();
        down.events.push(egui::Event::PointerMoved(centre));
        down.events.push(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        draw(&ctx, &mut vis, &mut draft, &mut hits, down);
        let mut up = egui::RawInput::default();
        up.events.push(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        draw(&ctx, &mut vis, &mut draft, &mut hits, up);
        assert_eq!(
            vis.picker.as_ref().map(|p| p.row),
            None,
            "the tap on {} was ignored: the sheet is still open on row {:?}",
            part.id,
            vis.picker.as_ref().map(|p| p.row)
        );
        assert_eq!(
            vis.flash.as_ref().map(|f| f.ok),
            Some(true),
            "the tap on {} did not fit the record: {:?}",
            part.id,
            vis.flash.as_ref().map(|f| f.text.as_str())
        );
        assert_eq!(
            draft.0.fill_layers.first().map(|l| l.fill_id.as_str()),
            Some(part.id.as_str()),
            "the tapped record is not the layer the stack starts with"
        );
        assert!(
            vis.drag.is_none(),
            "a tap must not be read as a drag: {:?}",
            vis.drag.as_ref().map(|d| d.part.slug())
        );
    }

    /// Issue #135: a tap on a rail chip selects it. `rail_chip` reads `resp.clicked()` at its foot, and
    /// its response came from `dnd_drag_source` - drag only - so the tap was ignored. This drives the
    /// chip itself: a press and release on its published rect must select the record (and must not be
    /// mistaken for a drag).
    #[test]
    fn a_tap_on_a_rail_chip_selects_it() {
        use cockpit::fixture_engine::FixtureEngine;
        let fx = FixtureEngine::from_json(include_str!("../assets/fixture.json"))
            .expect("the fixture parses");
        let cat = Catalog::from_fixture(&fx);
        let input = fx.default_input();
        let ctx = egui::Context::default();
        ctx.set_fonts(t::fonts());
        let mut warm = ctx.run_ui(egui::RawInput::default(), |_| {});
        warm.textures_delta.clear();
        let mut draft = Draft(input);
        let mut vis = Visual::default();
        let mut hits = HitMap::default();
        let part = state::picker_rows(&cat, Slot::Fan)
            .into_iter()
            .next()
            .expect("the fixture holds a fan");
        let key = format!("rail:chip:{}:{}", part.class.slug(), part.id);
        let draw = |ctx: &egui::Context,
                    vis: &mut Visual,
                    draft: &mut Draft,
                    hits: &mut HitMap,
                    raw: egui::RawInput| {
            let mut out = ctx.run_ui(raw, |ui| {
                rail_chip(
                    ui,
                    &cat,
                    Some(draft),
                    vis,
                    hits,
                    part.class,
                    part.id.as_str(),
                );
            });
            out.textures_delta.clear();
        };
        for _ in 0..2 {
            draw(
                &ctx,
                &mut vis,
                &mut draft,
                &mut hits,
                egui::RawInput::default(),
            );
        }
        let r = *hits
            .0
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, r)| r)
            .unwrap_or_else(|| panic!("the rail did not publish a rect for {key}"));
        let centre = egui::pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        let mut down = egui::RawInput::default();
        down.events.push(egui::Event::PointerMoved(centre));
        down.events.push(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        draw(&ctx, &mut vis, &mut draft, &mut hits, down);
        let mut up = egui::RawInput::default();
        up.events.push(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        draw(&ctx, &mut vis, &mut draft, &mut hits, up);
        assert_eq!(
            vis.selected_part.as_ref().map(|p| p.slug()),
            Some(part.slug()),
            "the tap on {key} was ignored: the selection is {:?}",
            vis.selected_part.as_ref().map(|p| p.slug())
        );
        assert_eq!(
            vis.flash.as_ref().map(|f| f.ok),
            Some(true),
            "the tap on {key} left no confirmation: {:?}",
            vis.flash.as_ref().map(|f| f.text.as_str())
        );
        assert!(
            vis.drag.is_none(),
            "a tap must not be read as a drag: {:?}",
            vis.drag.as_ref().map(|d| d.part.slug())
        );

        // ...and the chip still **drags**. The fix upgrades the chip's sense to `click_and_drag` on the
        // drag source's own id; if that had replaced the drag sense instead of unioning with it, the bay
        // drop path would go silent while every tap test stayed green. Press on the chip, move the
        // pointer, and the chip must be carried - while the strip must not claim a click, which is what
        // the tap above sets.
        vis.flash = None;
        vis.drag = None;
        let mut drag_down = egui::RawInput::default();
        drag_down.events.push(egui::Event::PointerMoved(centre));
        drag_down.events.push(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        draw(&ctx, &mut vis, &mut draft, &mut hits, drag_down);
        let mut drag_move = egui::RawInput::default();
        drag_move.events.push(egui::Event::PointerMoved(egui::pos2(
            centre.x,
            centre.y + 24.0,
        )));
        draw(&ctx, &mut vis, &mut draft, &mut hits, drag_move);
        assert_eq!(
            vis.drag.as_ref().map(|d| d.part.slug()),
            Some(part.slug()),
            "a drag from {key} no longer carries the chip"
        );
        assert!(
            vis.flash.is_none(),
            "the drag was read as a click: {:?}",
            vis.flash.as_ref().map(|f| f.text.as_str())
        );
    }

    /// Issue #136, AC 2/3: a layer's depth control lists **its own fill's** heights. The default draft's
    /// two menus are FILM-MF20's and FILM-WF25's own lists and hold the stack's 0.45 m / 0.90 m (neither is
    /// in the tower's `fillDepthOptionsM`); a click on an offered height sets the layer's depth; a fill
    /// swap re-reads the list and snaps a depth the new fill is not made in to its nearest height, with
    /// a flash that says so. Drives the stack card's own `depth_row` and the app's own `apply_drop`.
    #[test]
    fn a_layer_s_depth_menu_is_its_own_fill_s_heights() {
        use cockpit::fixture_engine::FixtureEngine;
        let fx = FixtureEngine::from_json(include_str!("../assets/fixture.json"))
            .expect("the fixture parses");
        let cat = Catalog::from_fixture(&fx);
        let mut input = fx.default_input();
        let tower = input.tower.fill_depth_options_m.clone();
        assert_eq!(
            (
                input.fill_layers[0].fill_id.as_str(),
                input.fill_layers[0].depth_m
            ),
            ("FILM-MF20", 0.45)
        );
        assert_eq!(
            (
                input.fill_layers[1].fill_id.as_str(),
                input.fill_layers[1].depth_m
            ),
            ("FILM-WF25", 0.9)
        );
        for (i, want) in [(0, 0.45), (1, 0.9)] {
            let l = &input.fill_layers[i];
            let (menu, note) = layer_depth_menu(Some(&cat), &l.fill_id, l.depth_m);
            assert_eq!(
                menu,
                cat.depth_options(&l.fill_id),
                "layer {i}: the fill's own list"
            );
            assert!(
                menu.contains(&want),
                "layer {i}: {want} m is offered: {menu:?}"
            );
            assert!(
                !tower.contains(&want),
                "the stack's {want} m is not a tower option"
            );
            assert_ne!(menu, tower, "layer {i}: not the tower's total-stack list");
            assert_eq!(note, format!("{} heights", l.fill_id));
        }

        // The drawn control: open layer 0's menu, and every published option is the fill's own list;
        // a click on 0.60 m sets the layer's depth.
        let ctx = egui::Context::default();
        ctx.set_fonts(t::fonts());
        let mut warm = ctx.run_ui(egui::RawInput::default(), |_| {});
        warm.textures_delta.clear();
        let mut hits = HitMap::default();
        let draw = |input: &mut EngineInput, hits: &mut HitMap, raw: egui::RawInput| {
            hits.0.clear();
            let mut out = ctx.run_ui(raw, |ui| depth_row(ui, Some(&cat), input, 0, hits));
            out.textures_delta.clear();
        };
        let click = |at: egui::Pos2| {
            let mut down = egui::RawInput::default();
            down.events.push(egui::Event::PointerMoved(at));
            down.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
            let mut up = egui::RawInput::default();
            up.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
            (down, up)
        };
        let centre = |hits: &HitMap, key: &str| {
            let r = hits
                .0
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, r)| *r)
                .unwrap_or_else(|| panic!("no rect for {key}: {:?}", hits.0));
            egui::pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
        };
        draw(&mut input, &mut hits, egui::RawInput::default());
        let (down, up) = click(centre(&hits, "layer:0:depth"));
        draw(&mut input, &mut hits, down);
        draw(&mut input, &mut hits, up);
        draw(&mut input, &mut hits, egui::RawInput::default());
        let offered: Vec<String> = hits
            .0
            .iter()
            .filter_map(|(k, _)| k.strip_prefix("layer:0:depth:").map(str::to_string))
            .collect();
        let own: Vec<String> = cat
            .depth_options("FILM-MF20")
            .iter()
            .map(|o| format!("{o:.2}"))
            .collect();
        assert_eq!(offered, own, "the open menu offers FILM-MF20's own heights");
        let (down, up) = click(centre(&hits, "layer:0:depth:0.60"));
        draw(&mut input, &mut hits, down);
        draw(&mut input, &mut hits, up);
        assert_eq!(
            input.fill_layers[0].depth_m, 0.6,
            "a click on 0.60 m sets the depth"
        );

        // A swap to a fill made in the same heights keeps the depth, and says nothing about it...
        let part = |id: &str| PartRef::parse(&format!("fill:{id}")).expect("a fill part");
        let msg = apply_drop(&cat, &mut input, Slot::Fill, &part("FILM-OF25"), 0).expect("drops");
        assert_eq!(input.fill_layers[0].depth_m, 0.6);
        assert!(!msg.contains("snapped"), "{msg}");
        // ...a swap to one that is not made in 0.75 m snaps to its nearest height and the flash says so.
        input.fill_layers[0].depth_m = 0.75;
        let msg = apply_drop(&cat, &mut input, Slot::Fill, &part("FILM-VF38"), 0).expect("drops");
        let vf38 = cat.depth_options("FILM-VF38");
        assert!(
            vf38.contains(&input.fill_layers[0].depth_m),
            "the depth is a FILM-VF38 height"
        );
        assert_eq!(
            input.fill_layers[0].depth_m, 0.6,
            "0.75 m snaps to the nearer 0.60 m (a tie goes down)"
        );
        assert!(
            msg.contains("0.75 m is not a FILM-VF38 height, snapped to 0.60 m"),
            "the flash names the snap: {msg}"
        );
        let (menu, _) = layer_depth_menu(Some(&cat), "FILM-VF38", 0.6);
        assert_eq!(
            menu, vf38,
            "the swapped layer's menu is the new fill's list"
        );
        assert!(!menu.contains(&0.45), "FILM-MF20's 0.45 m is gone with it");
    }

    /// Issue #135, AC 4: tapping `+ custom` in **each of the four rail sections** opens that class's
    /// form. The chip is drawn as a `t::chip_frame(...).show(...)`, whose response senses hover only -
    /// the tap used to be dead in all four sections, so the authoring surface was keyboard-only. This
    /// drives the chip itself once per class, on the app's own generated field list, and taps its
    /// published rect: the form must be open on that class and the strip must say so.
    #[test]
    fn a_tap_on_the_custom_chip_opens_that_sections_form() {
        let fields = state::FieldsRes::load()
            .fields
            .expect("the generated field list parses");
        for class in Class::ALL {
            let ctx = egui::Context::default();
            ctx.set_fonts(t::fonts());
            let mut warm = ctx.run_ui(egui::RawInput::default(), |_| {});
            warm.textures_delta.clear();
            let mut vis = Visual::default();
            let mut form = form::CustomForm::default();
            let mut hits = HitMap::default();
            let key = format!("rail:custom:{}", class.slug());
            let draw = |ctx: &egui::Context,
                        vis: &mut Visual,
                        form: &mut form::CustomForm,
                        hits: &mut HitMap,
                        raw: egui::RawInput| {
                let mut out = ctx.run_ui(raw, |ui| {
                    form::add_chip(ui, class, vis, form, hits, Some(&fields));
                });
                out.textures_delta.clear();
            };
            for _ in 0..2 {
                draw(
                    &ctx,
                    &mut vis,
                    &mut form,
                    &mut hits,
                    egui::RawInput::default(),
                );
            }
            let r = *hits
                .0
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, r)| r)
                .unwrap_or_else(|| panic!("the {} section did not publish its chip", class.slug()));
            let centre = egui::pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            let mut down = egui::RawInput::default();
            down.events.push(egui::Event::PointerMoved(centre));
            down.events.push(egui::Event::PointerButton {
                pos: centre,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
            draw(&ctx, &mut vis, &mut form, &mut hits, down);
            let mut up = egui::RawInput::default();
            up.events.push(egui::Event::PointerButton {
                pos: centre,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
            draw(&ctx, &mut vis, &mut form, &mut hits, up);
            assert_eq!(
                form.class,
                Some(class),
                "a tap on the {} `+ custom` chip did not open its form",
                class.slug()
            );
            assert_eq!(
                vis.flash.as_ref().map(|f| (f.ok, f.text.as_str())),
                Some((true, format!("new {} form open", class.name()).as_str())),
                "the {} chip's tap left no confirmation",
                class.slug()
            );
        }
    }
}
