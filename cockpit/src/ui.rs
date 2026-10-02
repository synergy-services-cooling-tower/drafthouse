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

use cockpit::engine::{Engine, EngineOutput, ZoneId};
use drafthouse_cockpit_seams as seams;
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
    mut clock: ResMut<AnimClock>,
    mut hits: ResMut<HitMap>,
    mut aux: VizAux,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    if !fonts_bound(ctx) {
        return;
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

    let screen = ctx.viewport_rect();
    let phone = screen.width() < 760.0;
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
    let header_h = if phone { 140.0 } else { 96.0 };
    // Round 5, item 4: the desktop status strip is two rows (the notice + the staged log, then the engine
    // note + the mandated copy), so the staged string can never be drawn under another sentence.
    let footer_h = if phone { 60.0 } else { 46.0 };
    let dock_h = if phone { 152.0 } else { 244.0 };
    // Round 5, item 3: the strip carries one state-dependent hint string in a second row, so the strip is
    // taller than round 4's single row by one text line (a hint that does not fit its row is truncated, never
    // clipped - the row measures itself and reports to the clip probe).
    let strip_h = if phone { 0.0 } else { 70.0 };
    let header_r = Rect::from_min_size(screen.min, egui::vec2(screen.width(), header_h));
    let footer_r = Rect::from_min_max(
        egui::pos2(screen.left(), screen.bottom() - footer_h),
        screen.max,
    );
    let body_r = Rect::from_min_max(
        egui::pos2(screen.left(), header_r.bottom()),
        egui::pos2(screen.right(), footer_r.top()),
    );
    let left_w = if phone {
        0.0
    } else if vis.rail_open {
        if screen.width() < RAIL_NARROW_BELOW {
            RAIL_W_NARROW
        } else {
            RAIL_W
        }
    } else {
        RAIL_STRIP_W
    };
    let _ = LEFT_W; // the round-1 tray column is gone: this is a chip rail, not a wall of cards
    let right_w = if phone || vis.view == View::Seams {
        0.0
    } else if screen.width() >= RIGHT_WIDE_ABOVE {
        // Round 5, item 1(a): at 1280 px and above the column takes the width the duty form needs.
        RIGHT_W_WIDE
    } else {
        RIGHT_W
    };
    let left_r = Rect::from_min_max(
        body_r.min,
        egui::pos2(body_r.left() + left_w, body_r.bottom()),
    );
    let right_r = Rect::from_min_max(
        egui::pos2(body_r.right() - right_w, body_r.top()),
        body_r.max,
    );
    let centre_r = Rect::from_min_max(
        egui::pos2(left_r.right(), body_r.top()),
        egui::pos2(right_r.left(), body_r.bottom()),
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
                    engine.0.as_deref(),
                    phone,
                    &mut hits,
                )
            });

            if !load.ready {
                region(ui, centre_r, egui::vec2(16.0, 12.0), |ui| {
                    loading_state(ui, &load)
                });
                scene_rect.set(Rect::NOTHING, screen.size(), phone);
                region(ui, footer_r, egui::vec2(14.0, 5.0), |ui| {
                    footer(
                        ui,
                        &options,
                        &load,
                        &staged,
                        engine.0.as_deref(),
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
                            )
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
                    match vis.view {
                        View::Seams => {
                            scene_rect.set(Rect::NOTHING, screen.size(), phone);
                            egui::ScrollArea::vertical()
                                .auto_shrink([false; 2])
                                .show(ui, |ui| seams_view(ui, &mut vis, phone));
                        }
                        View::Curves => {
                            let scene_h = if phone {
                                (avail.height() * 0.32).clamp(150.0, 260.0)
                            } else {
                                (avail.height() * 0.42).clamp(190.0, 330.0)
                            };
                            let scene_r =
                                Rect::from_min_size(avail.min, egui::vec2(avail.width(), scene_h));
                            scene_rect.set(scene_r, screen.size(), phone);
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
                            scene_rect.set(Rect::NOTHING, screen.size(), phone);
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
                            let scene_h = if phone {
                                (avail.height() * 0.42).clamp(230.0, 330.0)
                            } else {
                                (avail.height() - dock_h - strip_h - 12.0).max(220.0)
                            };
                            let scene_r =
                                Rect::from_min_size(avail.min, egui::vec2(avail.width(), scene_h));
                            scene_rect.set(scene_r, screen.size(), phone);
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
                                );
                            }
                            let dock_r = Rect::from_min_max(
                                egui::pos2(avail.left(), scene_r.bottom() + 6.0),
                                egui::pos2(
                                    avail.right(),
                                    (scene_r.bottom() + 6.0 + dock_h).min(avail.bottom()),
                                ),
                            );
                            ui.scope_builder(egui::UiBuilder::new().max_rect(dock_r), |ui| {
                                ui.set_clip_rect(dock_r.intersect(ui.clip_rect()));
                                dock(
                                    ui,
                                    draft_mut.as_deref_mut(),
                                    &run,
                                    &anchor,
                                    &mut vis,
                                    catalog.as_deref(),
                                    &mut clock,
                                    &mut hits,
                                    phone,
                                );
                                // Round 3, item 3: the configuration cards the right column used to carry.
                                ui.add_space(6.0);
                                setup_cards(
                                    ui,
                                    draft_mut.as_deref(),
                                    &mut vis,
                                    catalog.as_deref(),
                                    &options,
                                    &mut hits,
                                    phone,
                                );
                            });
                            if strip_h > 0.0 {
                                let strip_r = Rect::from_min_max(
                                    egui::pos2(avail.left(), dock_r.bottom() + 6.0),
                                    avail.max,
                                );
                                ui.scope_builder(egui::UiBuilder::new().max_rect(strip_r), |ui| {
                                    ui.set_clip_rect(strip_r.intersect(ui.clip_rect()));
                                    strip(
                                        ui,
                                        catalog.as_deref(),
                                        draft_mut.as_deref(),
                                        &vis,
                                        &mut aux.info,
                                        phone,
                                    )
                                });
                            }
                            if phone {
                                // The phone stack scrolls below the dock: strip, read-outs, tray - in that order.
                                let rest = Rect::from_min_max(
                                    egui::pos2(avail.left(), dock_r.bottom() + 4.0),
                                    avail.max,
                                );
                                ui.scope_builder(egui::UiBuilder::new().max_rect(rest), |ui| {
                                    // Fixed width, like the desktop column (`region_exact`), and 12 px short of
                                    // the body's own edge: an unbounded stack let one long line (a carried part
                                    // plus its verdict plus the flash after the drop) widen every card below it -
                                    // the read-out and the fill stack were drawn 30 px past the screen. The 12 px
                                    // matches the column the clip probe measures (the body region inset by 8),
                                    // so a card that still insists on more is reported rather than drawn off it.
                                    let stack_w = (rest.width() - 12.0).max(120.0);
                                    ui.set_min_width(stack_w);
                                    ui.set_max_width(stack_w);
                                    // Issue #58, the owner's layout nit 1: the same reservation the
                                    // desktop column makes. The stack's last rows used to pass the
                                    // column the clip probe measures by a pixel or two, i.e. into the
                                    // band the bottom bar owns; one point off the stack's vertical item
                                    // spacing keeps every measured row inside it (`data-clip-below`).
                                    ui.spacing_mut().item_spacing.y = 5.0;
                                    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(
                                        ui,
                                        |ui| {
                                            // Round 4, item 4: a phone's first section is the duty & site
                                            // panel, above the strip and the read-outs.
                                            duty_panel::panel(
                                                ui,
                                                draft_mut.as_deref_mut(),
                                                &aux.duty,
                                                &run,
                                                &mut vis,
                                                &mut hits,
                                                &mut aux.clip,
                                                &mut aux.info,
                                                phone,
                                            );
                                            ui.add_space(8.0);
                                            // Round 2: the tray is gone (a bay tap opens the picker), so the
                                            // phone stack leads with the selected-part strip and the read-outs.
                                            strip(
                                                ui,
                                                catalog.as_deref(),
                                                draft_mut.as_deref(),
                                                &vis,
                                                &mut aux.info,
                                                phone,
                                            );
                                            ui.add_space(8.0);
                                            // Round 3, item 1: the chip rail is the same rail on a phone -
                                            // it scrolls with the stack instead of taking a column. The
                                            // bay-tap picker's bottom sheet stays the quick path.
                                            parts_rail(
                                                ui,
                                                catalog.as_deref(),
                                                draft_mut.as_deref(),
                                                &mut vis,
                                                &mut hits,
                                                &mut aux.form,
                                                aux.fields.fields.as_ref(),
                                            );
                                            ui.add_space(8.0);
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
                                            ui.add_space(8.0);
                                            setup_cards(
                                                ui,
                                                draft_mut.as_deref(),
                                                &mut vis,
                                                catalog.as_deref(),
                                                &options,
                                                &mut hits,
                                                phone,
                                            );
                                            // room for the control bar, so the last card is never under it
                                            ui.add_space(52.0);
                                        },
                                    );
                                });
                            }
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
                    engine.0.as_deref(),
                    &mut aux.info,
                    phone,
                )
            });
        });

    // ---- round 2, change A: the bay-tap picker. Drawn in its own foreground layer, one at a time,
    // dismissed by Esc, by a click outside, or by a drop.
    let centre_for_anchor = Rect::from_min_max(
        egui::pos2(0.0, header_r.bottom()),
        egui::pos2(centre_r.right(), body_r.bottom()),
    );
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
            engine.0.as_deref_mut(),
            &mut vis,
            &mut hits,
            screen,
            phone,
            options.host.public,
        );
    }
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
        (View::Cockpit, Some(d)) => {
            let inner = centre.shrink2(egui::vec2(if phone { 8.0 } else { 12.0 }, 8.0));
            let scene_h = if phone {
                (inner.height() * 0.42).clamp(230.0, 330.0)
            } else {
                (inner.height() - 132.0 - 54.0 - 12.0).max(220.0)
            };
            let scene_r = Rect::from_min_size(inner.min, egui::vec2(inner.width(), scene_h));
            let l = scene::layout(scene_r, Some(&d.0), phone);
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
    let (pos, width, max_h) = if phone {
        (
            egui::pos2(
                screen.left() + 6.0,
                (screen.bottom() - 6.0 - screen.height() * 0.44).max(screen.top() + 60.0),
            ),
            screen.width() - 12.0,
            screen.height() * 0.44,
        )
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
        (egui::pos2(x, y), w, (screen.bottom() - y - 16.0).min(360.0))
    };

    let mut picked: Option<PartRef> = None;
    let mut close = false;
    let area = egui::Area::new(egui::Id::new("viz.picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
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
                        ui.label(RichText::new(format!("{} bay", slot.name())).size(12.0).color(t::INK).family(t::family_semi()));
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
                        RichText::new("pick = replace · drag a card into the bay · esc or click outside to close")
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
    let inner = ui.dnd_drag_source(
        egui::Id::new(("picker", slot.slug(), part.id.as_str())),
        part.clone(),
        |ui| {
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
        },
    );
    if inner.response.clicked() {
        clicked = true;
    }
    let r = inner.response.rect;
    hits.0.push((
        format!("picker:{}:{}", slot.slug(), part.id),
        [r.min.x, r.min.y, r.width(), r.height()],
    ));
    // A hovered card becomes the selected part, so the strip describes the row under the pointer.
    if inner.response.hovered() {
        vis.selected_part = Some(part.clone());
        vis.selected_slot = slot;
    }
    // A card that has been *picked up* is carried: the app says so while the pointer is still on the card,
    // which is what the strip and the frame's `data-drag` show. The bay's own code overwrites this with the
    // bay it is over once the pointer gets there, so this only writes while the payload is not over a bay.
    if inner.response.dragged() && vis.drag.as_ref().map(|d| d.over.is_none()).unwrap_or(true) {
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
            .map(|f| format!("{:.1} m2", f.stack_area_m2))
            .unwrap_or_default(),
        Class::Drift => cat
            .drift(id)
            .map(|d| format!("max {:.0} C", d.max_water_temperature_c))
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
    ui.label(
        RichText::new("drag a chip onto its bay · tap a bay for its picker")
            .size(9.0)
            .color(t::MUTED),
    );
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
    let inner = ui.dnd_drag_source(
        egui::Id::new(("rail", class.slug(), id)),
        part.clone(),
        |ui| {
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
        },
    );
    let r = inner.response.rect;
    hits.0.push((
        format!("rail:chip:{}:{}", class.slug(), id),
        [r.min.x, r.min.y, r.width(), r.height()],
    ));
    if scroll_here {
        // The record the form just saved: bring its chip into view once, then forget the request.
        inner.response.scroll_to_me(Some(egui::Align::Center));
        vis.scroll_to = None;
    }
    if inner.response.hovered() {
        vis.selected_part = Some(part.clone());
        vis.selected_slot = class.slot();
    }
    // A held chip is a drag, exactly like a picker card: the bays light up before release (item 2).
    if inner.response.dragged() && vis.drag.as_ref().map(|d| d.over.is_none()).unwrap_or(true) {
        vis.drag = Some(Drag {
            part: part.clone(),
            over: None,
            verdict: Ok(()),
            staged: false,
        });
        vis.selected_part = Some(part.clone());
        vis.selected_slot = class.slot();
    }
    if inner.response.clicked() {
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

fn status_chip(ui: &mut egui::Ui, s: seams::Status) {
    let (fg, bg, line) = match s {
        seams::Status::Engine => (t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP),
        seams::Status::Fixture => (t::INK_2, t::PANEL_RAISED, t::LINE),
        seams::Status::Illustrative => (t::AMBER, t::AMBER_SOFT, t::with_alpha(t::AMBER, 120)),
        // Round 4's own status: a stated definition, not an illustration and not a result.
        seams::Status::Definition => (t::INK, t::PANEL, t::with_alpha(t::INK_2, 120)),
    };
    chip(ui, s.chip(), fg, bg, line);
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
                ("real engine · computed", false)
            } else {
                ("fixture-driven preview", true)
            }
        }
        // No engine in the slot: the build's default selection is the truth available (a frame is
        // never drawn before the slot is filled, but a default build must not claim "real" either
        // when its features exclude it).
        None => {
            if cfg!(feature = "real-engine") {
                ("real engine · computed", false)
            } else if cfg!(feature = "fixture-engine") {
                ("fixture-driven preview", true)
            } else {
                ("engine not loaded", false)
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

/// The header's sub-line: which instrument, over which engine, in the present tense.
fn engine_subline(engine: Option<&dyn Engine>) -> &'static str {
    match engine {
        Some(e) => {
            if engine_label(e.name()) == "RealEngine" {
                "an engineering instrument running on the real engine"
            } else {
                "a visual pass — an engineering instrument over the recorded fixture engine"
            }
        }
        None => {
            if cfg!(feature = "real-engine") {
                "an engineering instrument running on the real engine"
            } else {
                "a visual pass — an engineering instrument over the recorded fixture engine"
            }
        }
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

fn short_label(label: &str) -> &str {
    match label.find(" (") {
        Some(i) => &label[..i],
        None => label,
    }
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

    // Row 1: the mark, the name (which truncates rather than colliding), the host chips and - on a desktop -
    // the view switcher. Nothing here can overlap: every element is laid out in the row it belongs to.
    ui.horizontal(|ui| {
        let (mr, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::hover());
        ui.painter()
            .rect_filled(mr, egui::CornerRadius::same(5), t::with_alpha(accent, 46));
        ui.painter().rect_stroke(
            mr,
            egui::CornerRadius::same(5),
            Stroke::new(1.0, accent),
            StrokeKind::Inside,
        );
        ui.painter().text(
            mr.center(),
            Align2::CENTER_CENTER,
            &mark,
            FontId::new(12.0, t::family_semi()),
            accent,
        );

        if phone {
            ui.add(
                egui::Label::new(
                    RichText::new(&name)
                        .size(13.5)
                        .color(t::INK)
                        .family(t::family_semi()),
                )
                .truncate(),
            );
        } else {
            ui.vertical(|ui| {
                ui.label(t::strong(&name));
                ui.label(
                    RichText::new(engine_subline(engine))
                        .size(10.5)
                        .color(t::MUTED),
                );
            });
            ui.add_space(6.0);
            if host.public {
                chip(ui, "PUBLIC", t::INK_2, t::PANEL_RAISED, t::LINE);
            } else {
                chip(ui, "INTERNAL", t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP);
            }
            engine_mode_chip(ui, engine);
            if let Some(l) = &host.catalog_label {
                chip(ui, l, t::MUTED, t::PANEL_RAISED, t::LINE_SOFT);
            }
            if load.failure.is_some() {
                chip(
                    ui,
                    "FIXTURE FAILED",
                    t::DANGER,
                    t::DANGER_SOFT,
                    t::with_alpha(t::DANGER, 140),
                );
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if phone {
                // A phone keeps the host chip here and puts its view tabs on their own row below: 390 px
                // cannot hold the name and three tabs side by side.
                chip(
                    ui,
                    if host.public { "PUBLIC" } else { "INTERNAL" },
                    t::INK_2,
                    t::PANEL_RAISED,
                    t::LINE,
                );
            } else {
                // Round 4, item 5: ONE view bar - these header tabs - on both viewports. The bottom bar's
                // VIEW group is gone, so nothing is offered twice.
                for v in View::ALL.iter().copied().rev() {
                    let sel = vis.view == v;
                    let r = ui.selectable_label(
                        sel,
                        RichText::new(v.name())
                            .size(12.0)
                            .color(if sel { t::PRIMARY } else { t::MUTED })
                            .family(t::family_semi()),
                    );
                    if hit(hits, &format!("ctl:view:{}", v.slug()), r).clicked() {
                        vis.view = v;
                    }
                }
                ui.label(t::eyebrow("view"));
            }
        });
    });

    // Row 2: the required copy, always, and the limits of the pass.
    if phone {
        ui.add(
            egui::Label::new(
                RichText::new(seams::REQUIRED_COPY)
                    .size(11.0)
                    .color(t::PRIMARY)
                    .family(t::family_semi()),
            )
            .wrap(),
        );
        // Round 4, item 5: the phone's view tabs - the only view bar it has now.
        ui.horizontal(|ui| {
            for v in View::ALL.iter().copied() {
                let sel = vis.view == v;
                let r = ui.selectable_label(
                    sel,
                    RichText::new(v.name())
                        .size(11.5)
                        .color(if sel { t::PRIMARY } else { t::MUTED })
                        .family(t::family_semi()),
                );
                if hit(hits, &format!("ctl:view:{}", v.slug()), r).clicked() {
                    vis.view = v;
                }
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.label(RichText::new(seams::REQUIRED_COPY).size(11.5).color(t::PRIMARY).family(t::family_semi()));
            {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("no CFD · no bypass · no CTI/MRL validation or certification claim · no money field").size(10.0).color(t::MUTED));
            });
            }
        });
    }
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
            chip(
                ui,
                "no physics in this pass",
                t::MUTED,
                t::PANEL_RAISED,
                t::LINE_SOFT,
            );
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
    let engine_name = engine
        .map(|e| engine_label(e.name()))
        .unwrap_or_else(|| "not loaded".to_string());
    let engine_note = format!("engine: {engine_name} - this pass adds no physics");
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
        let (color, bg) = if notice.public { (t::MUTED, t::BG) } else { (t::PRIMARY, t::PRIMARY_SOFT) };
        let notice_chip = t::chip_frame(bg, t::with_alpha(color, 110)).show(ui, |ui| {
            ui.label(RichText::new(notice.text).size(10.0).color(color).family(t::family_semi()));
        });
        items.push(notice_chip.response.rect);
        let nophysics = chip(ui, "no physics in this pass", t::MUTED, t::PANEL_RAISED, t::LINE_SOFT)
            .on_hover_text("the visual pass adds no CFD, no bypass claim, no CTI/MRL validation or certification claim and no money field");
        items.push(nophysics.rect);
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
        items.push(
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(seams::REQUIRED_COPY)
                        .size(10.0)
                        .color(t::MUTED),
                )
            })
            .response
            .rect,
        );
    });
    info.footer_overlaps = overlaps(&items);
}

fn loading_state(ui: &mut egui::Ui, load: &Load) {
    t::card().show(ui, |ui| {
        ui.label(t::heading(if load.failure.is_some() { "Engine unavailable" } else { "Loading the fixture record" }));
        ui.label(t::body(if load.failure.is_some() {
            "The fixture asset did not load, so the instrument shows nothing rather than something invented. Nothing is estimated in its place."
        } else {
            "Fetching assets/fixture.json over HTTP and constructing the engine. The scene appears when the record is in."
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
            RichText::new(format!("cell plan {plan_m:.2} m · stack Ø {stack_d:.2} m · stack h {stack_h:.2} m"))
                .size(9.5)
                .color(t::MUTED),
        );
        ui.label(
            RichText::new(format!(
                "{} cell(s) on a shared basin and casing - all of it generated from the fixture records, none of it modelled by hand",
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
            RichText::new("blades rotate with the rpm control; the geometry is generated from the fixture records, not modelled by hand")
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

/// The engine's own Pa -> mmWG ratio, applied to any Pa value. Unit conversion only: nothing here invents
/// a pressure, it just prints the same number in the unit the read-out row carries.
fn to_mmwg(o: &EngineOutput, pa: f64) -> f64 {
    if o.total_pressure_pa.abs() > 1e-9 {
        pa * (o.total_pressure_mmwg / o.total_pressure_pa)
    } else {
        pa / 9.80665
    }
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
    lines: Vec<(String, FontId, Color32)>,
    tag: Option<(String, FontId, Color32)>,
    pad: f32,
    tone: Color32,
}

/// One line's height, from its own font size.
fn line_h(font: &FontId) -> f32 {
    (font.size * 1.36).round()
}

fn plate_size(
    painter: &egui::Painter,
    lines: &[(String, FontId, Color32)],
    tag: Option<&(String, FontId, Color32)>,
    pad: f32,
) -> egui::Vec2 {
    let mut w: f32 = 0.0;
    let mut h = pad * 2.0;
    for (text, font, color) in lines.iter() {
        w = w.max(
            painter
                .layout_no_wrap(text.clone(), font.clone(), *color)
                .size()
                .x,
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
    for (text, font, color) in pl.lines.iter() {
        last_top = y;
        p.text(
            egui::pos2(pl.rect.left() + pl.pad, y),
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

/// A bay's own label block, and the ladder that keeps it inside the bay it belongs to.
///
/// A block is a list of pieces (the record, the zone's own engine value, the invitation). The ladder runs in
/// this order, and every step is reported to the frame rather than hidden:
///
/// 1. **step the type down** - to 8 px - while the block is taller or wider than the room it has;
/// 2. **merge from the end** (the invitation joins the value line) while it is still too tall;
/// 3. **drop the leading piece** (the record's id - the rail chip, the strip and the picker all name it)
///    while a single line is still too wide: this is the phone's nozzle bay, whose band is 18 px tall;
/// 4. **truncate with an ellipsis** as the last resort.
///
/// The result is always inside the bay: a label that leaves its bay, or a pair that overlaps, is a layout
/// that failed, and the frames read both counts from the app (`data-bay-label-outside`, `-overlaps`).
fn bay_plate(
    painter: &egui::Painter,
    bay: Rect,
    mut lines: Vec<(String, FontId, Color32)>,
    tag: Option<(String, FontId, Color32)>,
    pad: f32,
    tone: Color32,
    free_h: f32,
) -> (Plate, bool, bool, bool) {
    let text_h =
        |ls: &[(String, FontId, Color32)]| -> f32 { ls.iter().map(|l| line_h(&l.1)).sum::<f32>() };
    let widest = |ls: &[(String, FontId, Color32)]| -> f32 {
        ls.iter()
            .map(|l| {
                painter
                    .layout_no_wrap(l.0.clone(), l.1.clone(), l.2)
                    .size()
                    .x
            })
            .fold(0.0f32, f32::max)
    };
    // The room the block has: the bay's own height, or the free strip above the bay's first content row (the
    // fill bay's layers are drawn from the bottom of the band up, and the block must not sit on the top one).
    let room = (free_h.min(bay.height()) - 2.0).max(6.0);
    let mut avail_w = (bay.width() - pad * 2.0 - 6.0).max(20.0);
    let mut truncated = false;
    let mut dropped = false;
    // 1. step the type down before anything is merged.
    while (pad * 2.0 + text_h(&lines) > room || widest(&lines) > avail_w) && lines[0].1.size > 8.0 {
        for l in lines.iter_mut() {
            l.1 = FontId::new(l.1.size - 0.5, l.1.family.clone());
        }
    }
    // 2. the pad gives way next: a plate with 1 px of padding beats a merged line, and the lines keep their
    //    own values.
    let pad = pad.min(((room - text_h(&lines)) / 2.0).max(1.0));
    // 3. still too tall: merge from the **start** - the pieces before the invitation join it, so the
    //    invitation stays the block's own last line (issue #58, the owner's layout nit 2). The block's
    //    last line is the short one the bay's own tag is drawn beside (`bay · drift`); merging the
    //    invitation *into* the value line instead put a long line at the end, and the tag then landed
    //    on top of it. Round 5's ladder merged from the end; the string a block ends up with is the
    //    same, in the same order - only which line survives alone changes.
    while lines.len() > 1 && text_h(&lines) + 2.0 > room {
        let head = lines.remove(1);
        lines[0].0 = format!("{} · {}", lines[0].0, head.0);
    }
    // 4. one line still too wide: drop the leading piece (the record's id), piece by piece.
    while lines.len() == 1 && widest(&lines) > avail_w && lines[0].0.contains(" · ") {
        let Some((_, rest)) = lines[0].0.split_once(" · ") else {
            break;
        };
        let rest = rest.to_string();
        if rest.is_empty() {
            break;
        }
        lines[0].0 = rest;
        dropped = true;
    }
    // 5. and the text is cut only when nothing else is left.
    avail_w = (bay.width() - pad * 2.0 - 6.0).max(20.0);
    for l in lines.iter_mut() {
        let w = painter
            .layout_no_wrap(l.0.clone(), l.1.clone(), l.2)
            .size()
            .x;
        if w > avail_w {
            l.0 = truncate_to(painter, &l.0, &l.1, avail_w);
            truncated = true;
        }
    }
    let size = plate_size(painter, &lines, tag.as_ref(), pad);
    let h = size.y.min(bay.height() - 2.0).min(room);
    let rect = Rect::from_min_size(
        bay.min + egui::vec2(4.0, 1.0),
        egui::vec2(
            (size.x + pad * 2.0).min(bay.width() - 6.0),
            h.max(line_h(&lines[0].1) + pad).min(bay.height() - 2.0),
        ),
    );
    let inside = bay.contains_rect(rect);
    (
        Plate {
            rect,
            lines,
            tag,
            pad,
            tone,
        },
        inside,
        truncated,
        dropped,
    )
}

/// A plate positioned against a point (the zone call-outs keep the anchors round 1 gave them).
fn plate_at(
    painter: &egui::Painter,
    at: egui::Pos2,
    align: Align2,
    lines: Vec<(String, FontId, Color32)>,
    pad: f32,
    tone: Color32,
) -> Plate {
    let size = plate_size(painter, &lines, None, pad) + egui::vec2(0.0, 0.0);
    let rect = align.anchor_size(at, size);
    Plate {
        rect,
        lines,
        tag: None,
        pad,
        tone,
    }
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
) {
    let phone = area.width() < 560.0;
    let l = scene::layout(area, Some(&draft.0), phone);
    let p = ui.painter_at(area);
    let mono = |size: f32| FontId::new(size, t::family_mono_med());
    let semi = |size: f32| FontId::new(size, t::family_semi());

    // ---- the depth ruler: what the fill band is worth in metres
    if !phone && l.m_per_px_y > 0.0 {
        let max_m = draft
            .0
            .tower
            .fill_depth_options_m
            .last()
            .copied()
            .unwrap_or(2.1);
        let mut m = 0.0;
        while m <= max_m + 0.001 {
            let y = l.fill_band.bottom() - (m as f32 / l.m_per_px_y);
            if y >= l.fill_band.top() - 2.0 && y <= l.fill_band.bottom() + 2.0 {
                let tick = Rect::from_min_max(
                    egui::pos2(l.ruler.right() - 7.0, y - 0.5),
                    egui::pos2(l.ruler.right(), y + 0.5),
                );
                p.rect_filled(
                    tick,
                    egui::CornerRadius::same(0),
                    t::with_alpha(t::TICK, 220),
                );
                p.text(
                    egui::pos2(l.ruler.right() - 10.0, y),
                    Align2::RIGHT_CENTER,
                    format!("{m:.2} m"),
                    mono(9.5),
                    t::MUTED,
                );
            }
            m += 0.5;
        }
        p.text(
            egui::pos2(l.ruler.center().x - 12.0, l.fill_band.top() - 14.0),
            Align2::LEFT_BOTTOM,
            "fill depth",
            semi(9.5),
            t::MUTED,
        );
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
    // The plenum's operating-point read-out is a plate of its own, right below the fan bay (round 3 drew the
    // same text there as free text over the flow). Measured here, once, so the fan bay's block can reserve
    // the strip it takes and the two plates can never touch.
    let op_text = |a: &EngineOutput| -> String {
        let op_pa = op_pressure(a);
        if phone {
            format!(
                "OP · {} m3/s · {} Pa",
                t::fmt(a.airflow_m3_s),
                t::fmt(op_pa)
            )
        } else {
            format!(
                "operating point · {} m3/s at {} Pa total (engine solve)",
                t::fmt(a.airflow_m3_s),
                t::fmt(op_pa)
            )
        }
    };
    // (On a phone the section is ~260 px tall and the plenum is 22 px: the read-out stays round 4's free
    // text there, and the fan bay's block gets the whole bay instead of reserving a plate that is not drawn.)
    let op_reserve = if phone {
        0.0
    } else {
        run.output
            .as_ref()
            .map(|a| plate_size(&p, &[(op_text(a), mono(9.5), t::PRIMARY)], None, 3.0).y + 2.0)
            .unwrap_or(0.0)
    };
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
            .on_hover_cursor(egui::CursorIcon::PointingHand);
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
        let dragged = vis
            .drag
            .as_ref()
            .map(|d| d.over == Some(*slot))
            .unwrap_or(false);
        let accepts = matches!(verdict.as_ref().map(|(_, v)| v), Some(Ok(())));
        let refuses = matches!(verdict.as_ref().map(|(_, v)| v), Some(Err(_)));
        let (color, width) = if dragged {
            let v = vis.drag.as_ref().map(|d| &d.verdict).unwrap_or(&Ok(()));
            (state::verdict_color(v), 2.4)
        } else if accepts {
            (t::VALID, 1.7)
        } else if refuses {
            (t::with_alpha(t::SLOT, 70), 1.0)
        } else if vis.selected_slot == *slot {
            (t::with_alpha(t::PRIMARY, 200), 1.4)
        } else {
            (t::SLOT, 1.1)
        };
        // The wash: an accepting bay is filled, a refusing one is left bare, the selected one keeps the
        // quiet accent frame it had in round 2.
        if accepts || vis.selected_slot == *slot {
            let tint = if accepts { t::VALID } else { t::PRIMARY };
            p.rect_filled(
                *r,
                egui::CornerRadius::same(3),
                t::with_alpha(tint, if accepts { 30 } else { 12 }),
            );
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

        // ---- round 5, item 2: ONE label block per bay. What round 4 drew as three separate strings (the
        // identity at the top-left, `bay · <name>` at the top-right, the invitation or the verdict pill in
        // the middle) is one plate with its own lines: the identity, the zone's own engine value, the
        // invitation (or, while a part is carried, the bay's verdict). It is measured against the bay it
        // belongs to, so it cannot leave the bay, and it is painted after the streamlines.
        let dragging_here = vis
            .drag
            .as_ref()
            .map(|d| d.over == Some(*slot))
            .unwrap_or(false);
        let tag = if !phone || dragging_here {
            Some((
                if dragging_here {
                    format!("bay · {} · drop", slot.name())
                } else {
                    format!("bay · {}", slot.name())
                },
                semi(9.0),
                t::with_alpha(t::MUTED, 210),
            ))
        } else {
            None
        };
        let mono_size = if phone { 9.0 } else { 10.0 };
        let inv_size = if phone { 8.5 } else { 9.5 };
        // A 390 px phone's bands are a third of the desktop's: the invitation is the same affordance, said
        // shorter (round 2's rule for the phone: simplify the detail, not the component).
        let invitation = if phone {
            "tap or drop"
        } else {
            "tap or drop here"
        };
        let mut lines: Vec<(String, FontId, Color32)> = Vec::new();
        let tone = match verdict.as_ref() {
            Some((part, v)) => {
                // The bay's answer while a part is carried - one line, in the verdict's own colour, in the
                // same block the identity was in. (A bay that refuses the part dims and says why.)
                let (txt, col) = match v {
                    Ok(()) => (format!("drop here · accepts {}", part.id), t::VALID),
                    Err(_) => (
                        format!("won't take {} ({})", part.id, part.class.name()),
                        t::with_alpha(t::MUTED, 220),
                    ),
                };
                lines.push((txt, semi(if phone { 9.5 } else { 10.0 }), col));
                col
            }
            None => {
                let identity = match slot {
                    Slot::Fan => {
                        if phone {
                            format!("fan: {} · {} rpm", draft.0.fan.id, rpm_now)
                        } else {
                            format!(
                                "fan: {} · {} rpm (speed ratio {:.2})",
                                draft.0.fan.id, rpm_now, draft.0.speed_ratio
                            )
                        }
                    }
                    Slot::Drift => format!("drift: {}", draft.0.drift.id),
                    Slot::Nozzle => format!("nozzle bank: {}", draft.0.nozzle.id),
                    Slot::Fill => {
                        let i = vis
                            .selected_layer
                            .min(draft.0.fill_layers.len().saturating_sub(1));
                        match draft.0.fill_layers.get(i) {
                            Some(l) => {
                                format!("fill layer {}: {} {:.2} m", i + 1, l.fill_id, l.depth_m)
                            }
                            None => "fill stack: empty".into(),
                        }
                    }
                };
                lines.push((identity, mono(mono_size), t::with_alpha(t::INK, 220)));
                // The zone's own engine value, in the bay it belongs to (round 4 drew the fan stack, the
                // drift and the spray call-outs as free-floating text beside the tower; they are the bays'
                // own values, so they live in the bays' blocks now - one label per thing).
                let zone = match slot {
                    Slot::Fan => zone_pa(run, ZoneId::Stack, None)
                        .map(|(pa, share)| format!("fan stack · {pa:.1} Pa · {share:.1}%")),
                    Slot::Drift => zone_pa(run, ZoneId::Drift, None)
                        .map(|(pa, share)| format!("{pa:.1} Pa · {share:.1}%")),
                    Slot::Nozzle => zone_pa(run, ZoneId::Spray, None).map(|(pa, share)| {
                        format!("spray / distribution · {pa:.1} Pa · {share:.1}%")
                    }),
                    Slot::Fill => None,
                };
                if let Some(z) = zone {
                    lines.push((z, mono(mono_size), t::INK_2));
                }
                lines.push((
                    invitation.to_string(),
                    semi(inv_size),
                    t::with_alpha(t::MUTED, 150),
                ));
                if vis.selected_slot == *slot {
                    t::with_alpha(t::PRIMARY, 220)
                } else {
                    t::SLOT
                }
            }
        };
        let free_h = match slot {
            // the fill bay: the strip above its first layer row
            Slot::Fill => l
                .layers
                .first()
                .map(|lr| lr.top() - r.top())
                .unwrap_or(r.height()),
            // the fan bay: everything above the plenum's operating-point plate
            Slot::Fan => (r.height() - op_reserve).max(20.0),
            _ => r.height(),
        };
        let (plate, inside, truncated, dropped) = bay_plate(
            &p,
            *r,
            lines,
            tag,
            if phone { 4.0 } else { 5.0 },
            tone,
            free_h,
        );
        if !inside {
            info.bay_label_outside += 1;
        }
        if truncated {
            info.bay_label_truncated += 1;
            let txt = plate
                .lines
                .iter()
                .map(|l| l.0.clone())
                .collect::<Vec<String>>()
                .join(" | ");
            info.bay_label_notes
                .push_str(&format!("{}:cut[{}];", slot.slug(), txt));
        }
        if dropped {
            info.bay_label_dropped += 1;
            info.bay_label_notes
                .push_str(&format!("{}:dropped;", slot.slug()));
        }
        bay_rects.push(plate.rect);
        plates.push(plate);

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

    // ---- the section call-outs that are not a bay's own value: every number below is engine output at the
    // current ratio. (The fan stack, drift and spray call-outs moved into those bays' blocks in round 5;
    // the rain zone, the inlet louvres and the basin are the tower's own bands, so their plates are anchored
    // to the bands exactly as round 1 anchored the text.)
    if let Some((pa, share)) = zone_pa(run, ZoneId::Rain, None) {
        // A phone's bands are 18 px tall and its plates are anchored *inside* them (round 4's anchor put the
        // text above the band, where it ran into the fill layer's own row).
        let pl = plate_at(
            &p,
            if phone {
                egui::pos2(l.rain.left() + 6.0, l.rain.top() + 1.0)
            } else {
                egui::pos2(l.rain.left() + 6.0, l.rain.bottom() - 2.0)
            },
            if phone {
                Align2::LEFT_TOP
            } else {
                Align2::LEFT_BOTTOM
            },
            vec![(
                format!("rain zone · {pa:.1} Pa · {share:.1}%"),
                // a phone's rain band is 16 px: the call-out steps down with it, or the plate would leave
                // the band and touch the basin's own plate below it.
                mono(if phone { 8.5 } else { 10.0 }),
                t::INK_2,
            )],
            if phone { 1.5 } else { 4.0 },
            t::SLOT,
        );
        plates.push(pl);
    }
    if let Some((pa, share)) = zone_pa(run, ZoneId::Inlet, None) {
        let (pos, align) = if phone {
            (
                egui::pos2(l.tower.left() + 5.0, l.plinth.bottom() - 3.0),
                Align2::LEFT_BOTTOM,
            )
        } else {
            (
                egui::pos2(l.inlet_l.left() - 3.0, l.inlet_l.top() - 4.0),
                Align2::LEFT_BOTTOM,
            )
        };
        let pl = plate_at(
            &p,
            pos,
            align,
            vec![(
                format!("inlet louvres {pa:.1} Pa · {share:.1}%"),
                mono(9.5),
                t::AIR,
            )],
            if phone { 2.0 } else { 4.0 },
            t::SLOT,
        );
        plates.push(pl);
    }
    if let Some(o) = run.output.as_ref() {
        let pl = plate_at(
            &p,
            if phone {
                egui::pos2(l.basin.left() + 6.0, l.basin.top() + 1.0)
            } else {
                egui::pos2(l.basin.left() + 6.0, l.basin.center().y)
            },
            if phone {
                Align2::LEFT_TOP
            } else {
                Align2::LEFT_CENTER
            },
            vec![(
                format!(
                    "basin · {} m3/hr (engine output)",
                    t::fmt(o.water_flow_m3_hr)
                ),
                mono(10.0),
                t::WATER,
            )],
            if phone { 2.0 } else { 4.0 },
            t::SLOT,
        );
        plates.push(pl);
    }

    // per-layer identity + KaV/L, straight from `EngineOutput.kavl_per_layer`
    for (i, r) in l.layers.iter().enumerate() {
        let Some(lr) = layer_result(run, i) else {
            continue;
        };
        let txt = if phone {
            format!("{} {:.2} m", lr.fill_id, lr.depth_m)
        } else {
            format!(
                "{} · {:.2} m · KaV/L {:.3} · {:.1} Pa · {:.0}%",
                lr.fill_id, lr.depth_m, lr.kavl, lr.pressure_pa, lr.cooling_share_pct
            )
        };
        let mut line = vec![(txt, mono(10.5), t::INK)];
        if !lr.inside_envelope {
            line.push((
                "outside the fill envelope".to_string(),
                semi(9.5),
                t::DANGER,
            ));
        }
        let pl = plate_at(
            &p,
            egui::pos2(r.left() + 6.0, r.center().y),
            Align2::LEFT_CENTER,
            line,
            if phone { 2.0 } else { 3.0 },
            t::SLOT,
        );
        plates.push(pl);
    }

    // ---- the fan deck: what is spinning and at what speed. Round 5 moved this line into the fan bay's own
    // label block (round 4 drew it right-aligned to the left of the stack, where the speed ratio - the left
    // end of the string - was cut off by the drawing's own left edge).
    if let Some(a) = run.output.as_ref() {
        if phone {
            // A phone: no plate for the operating point (see `op_reserve` above) - the label is drawn over
            // the flow as round 4 drew it, and the four bay blocks keep their backings.
            p.text(
                egui::pos2(l.op_rail.right(), l.op_rail.top() - 3.0),
                Align2::RIGHT_BOTTOM,
                op_text(a),
                mono(9.5),
                t::PRIMARY,
            );
        } else {
            let pl = plate_at(
                &p,
                egui::pos2(l.op_rail.right(), l.op_rail.top() - 3.0),
                Align2::RIGHT_BOTTOM,
                vec![(op_text(a), mono(9.5), t::PRIMARY)],
                3.0,
                t::with_alpha(t::PRIMARY, 200),
            );
            plates.push(pl);
        }
    }

    // ---- round 5, item 2: **the annotation layer, painted last.** Every plate built above (the four bay
    // blocks, the fill layers' own rows, the zone call-outs, the operating point, the ambient line) is
    // painted here - after `flow_overlay`, so a streamline passes under a label and never through it. The
    // section reports what it drew: how many blocks, how many left their bay, how many pairs of plates
    // overlap each other (a layout that overlaps is a layout that failed).
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
    let legend_lines: Vec<(String, Color32, bool)> = if phone {
        vec![(
            "illustrative streamlines · not CFD".to_string(),
            t::AIR,
            true,
        )]
    } else {
        vec![
            ("illustrative flow map".to_string(), t::AIR, true),
            (
                "streamlines are an illustration, not a CFD result".to_string(),
                t::MUTED,
                false,
            ),
            (
                "spray coverage: illustrative (no distribution model)".to_string(),
                t::AMBER,
                false,
            ),
            (
                if run
                    .output
                    .as_ref()
                    .map(|o| o.provenance.engine.starts_with("RealEngine"))
                    .unwrap_or(false)
                {
                    format!(
                        "rpm {} · ratio {:.2} · computed by the real engine",
                        m::rpm_text(draft.0.speed_ratio, draft.0.fan.nominal_rpm),
                        draft.0.speed_ratio
                    )
                } else {
                    format!(
                        "rpm {} · ratio {:.2} · fixture-driven preview",
                        m::rpm_text(draft.0.speed_ratio, draft.0.fan.nominal_rpm),
                        draft.0.speed_ratio
                    )
                },
                t::AMBER,
                false,
            ),
        ]
    };
    let legend_anchor = egui::pos2(l.area.left() + 8.0, l.plinth.top() + 4.0);
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

    // ---- the pressure rail labels: zone name, Pa, share
    if let Some(o) = run.output.as_ref() {
        let total_share: f64 = o
            .pressure_by_zone
            .iter()
            .map(|z| z.share_pct.max(0.6))
            .sum();
        let mut bottom = l.rail.bottom();
        for z in o.pressure_by_zone.iter() {
            let h = (l.rail.height() as f64 * z.share_pct.max(0.6) / total_share) as f32;
            let cy = bottom - h * 0.5;
            if h > 8.0 {
                let txt = if phone {
                    format!("{:.0}", z.pressure_pa)
                } else if h > 15.0 {
                    format!(
                        "{} · {:.1} Pa · {:.1}%",
                        short_label(&z.label),
                        z.pressure_pa,
                        z.share_pct
                    )
                } else {
                    format!("{} · {:.1} Pa", short_label(&z.label), z.pressure_pa)
                };
                p.text(
                    egui::pos2(l.area.right() - 4.0, cy),
                    Align2::RIGHT_CENTER,
                    txt,
                    mono(9.5),
                    t::INK_2,
                );
            }
            bottom -= h;
        }
        p.text(
            egui::pos2(l.area.right() - 4.0, l.rail.top() - 4.0),
            Align2::RIGHT_BOTTOM,
            if phone {
                "Zone Pa".to_string()
            } else {
                // Round 3, item 5: this bar is the engine's per-zone split, which the fixture holds at the
                // recorded run; the operating-point pressure is stated (once) with the operating point.
                format!(
                    "air-path split · recorded run · sum {:.1} Pa",
                    zone_sum_pa(o)
                )
            },
            semi(9.5),
            t::MUTED,
        );
    }

    // ---- the ambient (recorded) inlet air, next to the air that enters (desktop; the phone shows it in
    // the read-out rail, where there is room)
    if !phone {
        if let Some(cat) = cat {
            if let Some(a) = cat.ambient {
                let pl = plate_at(
                    &p,
                    egui::pos2(l.inlet_l.left() - 3.0, l.inlet_l.bottom() + 10.0),
                    Align2::LEFT_TOP,
                    vec![(
                        format!(
                            "inlet air {:.1} C DB / {:.1} C WB · RH {:.0}% (recorded)",
                            a.dry_bulb_c,
                            a.wet_bulb_c,
                            a.relative_humidity * 100.0
                        ),
                        mono(9.5),
                        t::MUTED,
                    )],
                    3.0,
                    t::SLOT,
                );
                // The ambient line is the last annotation of the section, so its plate is painted here -
                // after every other plate and after the flow, like all of them.
                paint_plate(&p, &pl);
                plates.push(pl);
            }
        }
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
                let t = (anim_t * 0.22 * flow_f
                    + k as f32 / heads as f32
                    + j as f32 * 0.13
                    + side * 0.05)
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
    for k in 0..streaks {
        let x = l.fill_band.left() + l.fill_band.width() * (k as f32 + 0.5) / streaks as f32;
        let phase = (anim_t * 0.30 * flow_f + k as f32 / streaks as f32) % 1.0;
        let y = top_y + (bottom_y - top_y) * phase;
        let len = if phone { 12.0 } else { 16.0 };
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(x, (y + len).min(bottom_y))],
            Stroke::new(
                if phone { 1.3 } else { 1.6 },
                t::with_alpha(t::WATER, (150.0 * f_water) as u8),
            ),
        );
        // the faint thread the drops are falling along
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(x, bottom_y)],
            Stroke::new(0.8, t::with_alpha(t::WATER, (34.0 * f_water) as u8)),
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
                vis.flash = Some(Flash {
                    ok: false,
                    text: format!("refused: {e}"),
                })
            }
        },
        None => {
            vis.flash = Some(Flash {
                ok: false,
                text: "the engine is not loaded".into(),
            })
        }
    }
}

// ============================================================================================== dock

/// The knob of the instrument: rpm -> the fixture's `speed_ratio` -> `Engine::run` -> everything redrawn.
#[allow(clippy::too_many_arguments)]
fn dock(
    ui: &mut egui::Ui,
    draft: Option<&mut Draft>,
    run: &Run,
    anchor: &Anchor,
    vis: &mut Visual,
    cat: Option<&Catalog>,
    clock: &mut AnimClock,
    hits: &mut HitMap,
    phone: bool,
) {
    t::card_flat().show(ui, |ui| {
        let Some(draft) = draft else {
            ui.label(t::body("the engine is not loaded - no rpm to set"));
            return;
        };
        let [lo, hi] = draft.0.fan.allowed_speed_ratio;
        // Issue #59: the rpm read-out is the fan record's own rated speed times the ratio - the
        // record's datum, not a constant of the pass. A record that states none shows no rpm
        // (`m::rpm` then answers `None`, and the control is not offered).
        let nominal_rpm = draft.0.fan.nominal_rpm;
        let rpm_range = m::rpm_range(draft.0.fan.allowed_speed_ratio, nominal_rpm);
        let air = run.output.as_ref().map(|o| o.airflow_m3_s).unwrap_or(0.0);
        // Round 3, item 5: the dock reports the operating point's pressure - the same field the fan/system
        // chart marks - so the knob, the read-out and the curve can never disagree again.
        let pa = run.output.as_ref().map(op_pressure).unwrap_or(0.0);
        let kw = run.output.as_ref().map(|o| o.fan_power_kw).unwrap_or(0.0);
        let anchor_air = anchor.output.as_ref().map(|o| o.airflow_m3_s).unwrap_or(air);
        let anchor_pa = anchor.output.as_ref().map(op_pressure).unwrap_or(pa);

        ui.horizontal(|ui| {
            // the tachometer, drawn in egui on the same data the scene spins the blades with
            let (tr, _) = ui.allocate_exact_size(egui::vec2(if phone { 84.0 } else { 104.0 }, if phone { 58.0 } else { 66.0 }), Sense::hover());
            tacho(ui, tr, draft.0.speed_ratio, lo, hi);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(m::rpm_text(draft.0.speed_ratio, nominal_rpm)).size(if phone { 26.0 } else { 32.0 }).color(t::PRIMARY).family(t::family_mono_med()));
                    ui.label(RichText::new("rpm").size(12.0).color(t::MUTED));
                    // Issue #58 fix round: the dock's chip names the engine that answered, not a
                    // hardcoded round-5 claim. The provenance of the output on the frame is the truth
                    // available here (the slot itself is borrowed `&`-ly upstream).
                    let computed_real = run
                        .output
                        .as_ref()
                        .map(|o| o.provenance.engine.starts_with("RealEngine"))
                        .unwrap_or(false);
                    if computed_real {
                        chip(ui, "real engine · computed", t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP);
                    } else {
                        chip(ui, "fixture-driven preview", t::AMBER, t::AMBER_SOFT, t::with_alpha(t::AMBER, 120));
                    }
                });
                ui.set_max_width(if phone { 210.0 } else { 330.0 });
                ui.label(RichText::new(format!("speed ratio {:.3} · {} range {:.2}-{:.2}", draft.0.speed_ratio, draft.0.fan.id, lo, hi)).size(10.0).color(t::MUTED));
                // Issue #59: the ratio is handed to the engine as authored - a ratio outside the
                // record's own band is never clamped into it here, so the engine's own named limit
                // is what the dock shows where the numbers would be.
                if let Some(limit) = run
                    .output
                    .as_ref()
                    .and_then(|o| o.validation.iter().find(|limit| limit.field == "fan.speedRatio"))
                {
                    ui.label(RichText::new(format!(
                        "outside the record's band {} - {}: the engine returns no numbers rather than a ratio clamped into the band",
                        t::fmt(limit.min.unwrap_or(lo)),
                        t::fmt(limit.max.unwrap_or(hi))
                    )).size(10.0).color(t::AMBER));
                }
                // Issue #59: the nominal is the fan record's own `nominalRpm` (the speed its recorded
                // curve is published at) - the number the read-out above multiplies, not a pass constant.
                ui.label(RichText::new(match nominal_rpm {
                    Some(nominal) => format!("rpm = ratio x {} · the record's nominalRpm at speed ratio 1.000", t::fmt(nominal)),
                    None => "this fan record states no rated speed: no rpm is shown rather than an invented one".to_string(),
                }).size(10.0).color(t::MUTED));
            });
            if !phone {
                let d_air = air - anchor_air;
                let d_pa = pa - anchor_pa;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (label, value, unit, hot) in [
                        ("delta", format!("{d_air:+.2} / {d_pa:+.2}"), "m3/s / Pa", d_air.abs() > 1e-9 || d_pa.abs() > 1e-9),
                        ("fan power", t::fmt(kw), "kW", true),
                        ("pressure", t::fmt(pa), "Pa", true),
                        ("airflow", t::fmt(air), "m3/s", true),
                    ] {
                        t::chip_frame(t::PANEL_RAISED, t::LINE_SOFT).show(ui, |ui| {
                            ui.set_min_width(74.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(label.to_uppercase()).size(8.5).color(t::MUTED).family(t::family_semi()).extra_letter_spacing(0.6));
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(value).size(13.5).color(if hot { t::PRIMARY } else { t::INK }).family(t::family_mono_med()));
                                    ui.label(RichText::new(unit).size(9.0).color(t::MUTED));
                                });
                            });
                        });
                    }
                });
            }
        });

        ui.add_space(2.0);
        // On a desktop the slider, the steppers and the engine read-out share a row; at 390px they cannot,
        // so the slider gets its own row and the buttons the next one. Both rows are laid out the same way,
        // which is why nothing on this panel is ever clipped.
        ui.horizontal(|ui| {
            ui.label(t::eyebrow("fan speed"));
            match (nominal_rpm, rpm_range) {
                (Some(nominal), Some((rpm_lo, rpm_hi))) => {
                    // The control's own grid is whole rpm, and the value it is handed starts on that
                    // grid (`.round()`): egui then has nothing to snap, so it never writes a ratio
                    // nobody authored - the recorded ratio stays exactly what the run staged (0.78
                    // stays 0.780, not 182/233). The guard below is the same rule at the write: only
                    // a move off the draft's own rpm authors `rpm / nominalRpm`.
                    let shown = m::rpm(draft.0.speed_ratio, Some(nominal))
                        .unwrap_or(rpm_lo)
                        .round();
                    let mut rpm = shown;
                    let slider = egui::Slider::new(&mut rpm, rpm_lo..=rpm_hi)
                        .step_by(1.0)
                        .fixed_decimals(0)
                        .suffix(" rpm")
                        .trailing_fill(true);
                    let sr = ui.add_sized(egui::vec2(if phone { 226.0 } else { 420.0 }, 22.0), slider);
                    let sr = hit(hits, "ctl:rpm-slider", sr);
                    if sr.changed() && (rpm - shown).abs() > 0.5 {
                        // The control's range IS the record's own band, and the ratio it writes is
                        // `rpm / nominalRpm`: the value the engine evaluates is the one the control
                        // authored - one mapping, no clamp of a ratio from outside the band (a ratio
                        // from outside reaches the engine and comes back as its named limit).
                        draft.0.speed_ratio = m::ratio_from_rpm(rpm, nominal);
                        vis.flash = Some(Flash {
                            ok: true,
                            text: format!("rpm {rpm:.0} (speed ratio {:.3})", draft.0.speed_ratio),
                        });
                    }
                }
                _ => {
                    ui.label(RichText::new("the record states no rated speed: no rpm to set").size(10.5).color(t::MUTED));
                }
            }
        });
        ui.horizontal(|ui| {
            if hit(hits, "ctl:rpm-down", ui.add_sized(egui::vec2(30.0, 24.0), egui::Button::new("-"))).clicked() {
                draft.0.speed_ratio = (draft.0.speed_ratio - 0.02).clamp(lo, hi);
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!("rpm {} (one step down)", m::rpm_text(draft.0.speed_ratio, nominal_rpm)),
                });
            }
            if hit(hits, "ctl:rpm-up", ui.add_sized(egui::vec2(30.0, 24.0), egui::Button::new("+"))).clicked() {
                draft.0.speed_ratio = (draft.0.speed_ratio + 0.02).clamp(lo, hi);
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!("rpm {} (one step up)", m::rpm_text(draft.0.speed_ratio, nominal_rpm)),
                });
            }
            if hit(hits, "ctl:reset", ui.add_sized(egui::vec2(96.0, 24.0), egui::Button::new("fixture 0.78"))).clicked() {
                draft.0.speed_ratio = vis.reset_ratio.clamp(lo, hi);
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!("back to the fixture ratio {:.2}", vis.reset_ratio),
                });
            }
            if hit(hits, "ctl:freeze", ui.add_sized(egui::vec2(74.0, 24.0), egui::Button::new(if clock.frozen { "unfreeze" } else { "freeze" }))).clicked() {
                clock.frozen = !clock.frozen;
                vis.flash = Some(Flash {
                    ok: true,
                    text: format!("clock {}", if clock.frozen { "frozen" } else { "running" }),
                });
            }
            if phone {
                ui.label(RichText::new(format!("{:.1} m3/s · {:.0} Pa", air, pa)).size(10.5).color(t::PRIMARY).family(t::family_mono_med()));
            } else {
                ui.label(RichText::new("the engine is re-run on every frame the slider moves - the scene, the rail and the operating point all read that output").size(10.0).color(t::MUTED));
            }
        });
        let _ = cat;
    });
}

/// A tachometer: the same speed ratio the blades spin at, drawn as an instrument.
fn tacho(ui: &mut egui::Ui, rect: Rect, ratio: f64, lo: f64, hi: f64) {
    let p = ui.painter_at(rect);
    let centre = egui::pos2(rect.center().x, rect.bottom() - 6.0);
    let r = (rect.height() - 12.0).min(rect.width() * 0.5) - 4.0;
    let a0 = 200_f32.to_radians();
    let a1 = 340_f32.to_radians();
    let mut pts = Vec::new();
    for i in 0..=24 {
        let f = i as f32 / 24.0;
        let a = a0 + (a1 - a0) * f;
        pts.push(egui::pos2(
            centre.x + r * a.cos(),
            centre.y + r * a.sin() * 0.9,
        ));
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
    let f = if hi > lo {
        ((ratio - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let a = a0 + (a1 - a0) * f;
    let tip = egui::pos2(
        centre.x + (r - 8.0) * a.cos(),
        centre.y + (r - 8.0) * a.sin() * 0.9,
    );
    // the recorded run's ratio, as a dim tick the needle moves away from
    p.line_segment([centre, tip], Stroke::new(2.0, t::PRIMARY));
    p.circle_filled(centre, 3.0, t::PRIMARY);
    p.text(
        egui::pos2(rect.center().x, rect.top() + 2.0),
        Align2::CENTER_TOP,
        "rpm",
        FontId::new(9.0, t::family_semi()),
        t::MUTED,
    );
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
    ui.label(t::eyebrow("operating read-out — engine output"));
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
                    &t::fmt(o.airflow_m3_s),
                    "m3/s",
                    !flow_ok,
                    true,
                    clip,
                    info,
                );
                duty_panel::gated_kv(
                    ui,
                    "readout.total-pressure",
                    "total pressure",
                    &format!("{} / {}", t::fmt(op_pa), t::fmt(to_mmwg(o, op_pa))),
                    "Pa / mmWG",
                    !flow_ok,
                    true,
                    clip,
                    info,
                );
                ui.label(
                    RichText::new(format!(
                        "at the operating point — the engine's fan/system solve, total-pressure basis ({})",
                        draft.0.fan.pressure_basis
                    ))
                    .size(9.5)
                    .color(t::MUTED),
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
                    ui.label(
                        RichText::new("the fixture engine may only interpolate what it recorded; edit the duty in the panel above to come back inside the recorded sweep")
                            .size(9.0)
                            .color(t::MUTED),
                    );
                }
                ui.add_space(2.0);
                ui.label(RichText::new(format!("engine {} · catalog {} · {}", engine_label(&o.provenance.engine), o.provenance.catalog_id, o.provenance.catalog_status)).size(9.5).color(t::MUTED));
                if let Some(a) = anchor.output.as_ref() {
                    let d_air = o.airflow_m3_s - a.airflow_m3_s;
                    let d_pa = op_pa - op_pressure(a);
                    ui.label(RichText::new(format!("against the recorded run (ratio {:.2}): airflow {d_air:+.2} m3/s · total pressure {d_pa:+.2} Pa", anchor.ratio)).size(9.5).color(if d_air.abs() > 1e-9 || d_pa.abs() > 1e-9 { t::AMBER } else { t::MUTED }));
                    ui.label(RichText::new(format!("the recorded run's own air-path total is {} Pa, held at the recorded flow; the operating point moves with the speed ratio, and this read-out moves with it", t::fmt(o.total_pressure_pa))).size(9.5).color(t::MUTED));
                }
            }
            None => {
                ui.label(t::body("no run: the engine refused this input."));
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
            ui.label(t::eyebrow("fill stack — engine input order, top first"));
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
        let head = "fill stack — engine input order, top first";
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
                        format!(
                            "KaV/L {:.3} · {:.1} Pa · {:.0}%",
                            lr.kavl, lr.pressure_pa, lr.cooling_share_pct
                        )
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
                // depth: the tower record's own options
                ui.horizontal(|ui| {
                    ui.label(RichText::new("depth").size(10.0).color(t::MUTED));
                    let options_m = draft.0.tower.fill_depth_options_m.clone();
                    let mut depth_now = depth;
                    egui::ComboBox::from_id_salt(("depth", i))
                        .selected_text(format!("{depth_now:.2} m"))
                        .width(84.0)
                        .show_ui(ui, |ui| {
                            for o in options_m.iter() {
                                if ui.selectable_value(&mut depth_now, *o, format!("{o:.2} m")).clicked() {
                                    draft.0.fill_layers[i].depth_m = *o;
                                }
                            }
                        });
                    ui.label(RichText::new(format!("tower {} options", draft.0.tower.id)).size(9.5).color(t::MUTED));
                });
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

/// Round 3, item 3: the right column keeps the operating read-out and the fill-stack list only, so the
/// configuration controls the column used to carry - the nozzle arrangement, the internal host's command
/// chips and the seams summary - moved into the bottom dock as three compact rows. Same controls, same hit
/// keys, same labels; one column fewer.
fn setup_cards(
    ui: &mut egui::Ui,
    draft: Option<&Draft>,
    vis: &mut Visual,
    cat: Option<&Catalog>,
    options: &StartOptions,
    hits: &mut HitMap,
    phone: bool,
) {
    let Some(draft) = draft else { return };
    let _ = cat;
    // Every row wraps: a `ui.horizontal` row wider than the panel stretches the whole scroll content on a
    // phone (the chips then lay out at 834 px and the rail's caret lands off-screen).
    // ---- the nozzle arrangement (an editor value, not an engine input)
    // The coverage the badge reports is computed here, before the row: since issue #58 the badge is its
    // own row *below* the toggles (nit 3), so its value is no longer computed inside that row's closure.
    let pitch = m::effective_pitch(
        vis.nozzle_spacing_m,
        vis.nozzle_pattern == Pattern::Staggered,
    );
    let half = m::spray_half_angle_deg(draft.0.nozzle.orifice_diameter_m);
    let cone_r = m::spray_cone_radius_m(draft.0.tower.spray_zone_height_m, half);
    let cov = m::coverage_fraction(pitch, cone_r);
    ui.horizontal_wrapped(|ui| {
        ui.label(t::eyebrow("nozzle bank"));
        ui.label(
            RichText::new(&draft.0.nozzle.id)
                .size(11.0)
                .color(t::INK)
                .family(t::family_mono_med()),
        );
        ui.label(
            RichText::new(format!(
                "{:.0} mm · Cd {:.2}",
                draft.0.nozzle.orifice_diameter_m * 1000.0,
                draft.0.nozzle.discharge_coefficient
            ))
            .size(9.5)
            .color(t::MUTED),
        );
        let mut spacing = vis.nozzle_spacing_m;
        let sp = ui.add_sized(
            egui::vec2(150.0, 20.0),
            egui::Slider::new(&mut spacing, 0.3..=2.0)
                .fixed_decimals(2)
                .suffix(" m")
                .text("pitch"),
        );
        let sp = hit(hits, "ctl:spacing", sp);
        if sp.changed() {
            vis.nozzle_spacing_m = spacing;
            vis.flash = Some(Flash {
                ok: true,
                text: format!("nozzle pitch {spacing:.2} m"),
            });
        }
        for p in [Pattern::SingleRow, Pattern::Staggered] {
            let sel = vis.nozzle_pattern == p;
            let pr = ui.selectable_label(
                sel,
                RichText::new(p.name())
                    .size(11.0)
                    .color(if sel { t::PRIMARY } else { t::MUTED }),
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
    });
    // Issue #58, the owner's layout nit 3: the coverage badge used to be laid out from the *right
    // edge of the toggles' own row* (`with_layout(right_to_left)` inside it). When that row was full,
    // the right-aligned group had less room than it needed and grew leftwards, over the `staggered`
    // toggle - the same failure the status strip's row had in round 4. It is its own row now: the
    // row's full width is available, the badge is right-aligned in it, and it cannot reach the toggle.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let badge = chip(
            ui,
            "coverage: illustrative",
            t::AMBER,
            t::AMBER_SOFT,
            t::with_alpha(t::AMBER, 120),
        );
        let pct = ui.label(
            RichText::new(format!("{:.0}%", cov * 100.0))
                .size(12.0)
                .color(t::AMBER)
                .family(t::family_mono_med()),
        );
        // The badge's own rect, so a frame can test the overlap the owner's nit 3 names against the
        // pattern toggles' rects (`ctl:pattern:*`) instead of reading a picture.
        let r = badge.rect.union(pct.rect);
        hits.0.push((
            "badge:coverage".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
    });
    ui.label(
        RichText::new(if phone {
            format!(
                "arrangement is an editor value, not an engine input yet · coverage is illustrative (flat-area overlap, no distribution model) · half-angle {:.1} deg, radius {:.2} m",
                m::spray_half_angle_deg(draft.0.nozzle.orifice_diameter_m),
                m::spray_cone_radius_m(draft.0.tower.spray_zone_height_m, m::spray_half_angle_deg(draft.0.nozzle.orifice_diameter_m)),
            )
        } else {
            format!(
            "the arrangement is an editor value, not an engine input yet · coverage is the flat-area overlap of two neighbouring cones (no distribution model) · cone half-angle {:.1} deg from the orifice, footprint radius {:.2} m at the recorded spray-zone height {:.2} m",
            m::spray_half_angle_deg(draft.0.nozzle.orifice_diameter_m),
            m::spray_cone_radius_m(draft.0.tower.spray_zone_height_m, m::spray_half_angle_deg(draft.0.nozzle.orifice_diameter_m)),
            draft.0.tower.spray_zone_height_m
            )
        })
        .size(9.0)
        .color(t::MUTED),
    );
    // ---- the host difference, visible
    if !options.host.public {
        ui.horizontal_wrapped(|ui| {
            ui.label(t::eyebrow("internal host"));
            for c in [
                cockpit::host::ServerCommand::SaveRevision { project: "tower selection".into(), note: "visual pass".into() },
                cockpit::host::ServerCommand::ExportReportPdf { project: "tower selection".into() },
                cockpit::host::ServerCommand::CompareLater { project: "tower selection".into(), against_revision: None },
            ] {
                chip(ui, &c.label(), t::PRIMARY, t::PRIMARY_SOFT, t::PRIMARY_DEEP);
            }
            ui.label(
                RichText::new("these exist here and enqueue a ServerCommand; the public host hides them and keeps authoring + the mandatory label")
                    .size(9.0)
                    .color(t::MUTED),
            );
        });
    }
    // ---- the seams, summarised (the full table is the third view)
    ui.horizontal_wrapped(|ui| {
        let counts = seams::status_counts();
        ui.label(t::eyebrow("data seams"));
        ui.label(
            RichText::new(format!("{} seams · {}", seams::SEAMS.len(), counts.line()))
                .size(10.5)
                .color(t::INK_2),
        );
        ui.label(
            RichText::new(if phone {
                "generated from seams/src/lib.rs - the same table the Data seams view shows".to_string()
            } else {
                "generated from seams/src/lib.rs (tools/gen-seams.sh) - the same table the Data seams view shows".to_string()
            })
            .size(9.0)
            .color(t::MUTED),
        );
        let b = ui.add_sized(egui::vec2(120.0, 20.0), egui::Button::new("open the table"));
        let b = hit(hits, "ctl:seams-open", b);
        if b.clicked() {
            vis.view = View::Seams;
        }
    });
    let _ = phone;
}

// ============================================================================================= strip

/// The selected part, the catalog's verdict on it, and **one** state-dependent hint.
///
/// Round 5, item 3. Round 4 put two hint sentences and a right-aligned group in one unbounded row: when the
/// row was wider than the region, the right-aligned group was laid out from the region's right edge, which is
/// *behind* the text the left side had already drawn - so the two strings were painted over each other. The
/// strip is now two bounded rows (the part and the catalog's answer, then the hint), and there is exactly one
/// hint string, chosen by the state:
///
/// | `data-hint-state` | when | the hint |
/// |---|---|---|
/// | `none` | nothing picked up | `drag a chip from the rail onto a bay, or tap a bay for its picker` |
/// | `selected` | a part is picked and it is *not* the fitted one | `drag it onto a bay, or tap a bay to open its picker` |
/// | `dragging` | a part is being carried | `release over a bay that accepts it - every bay answers before you release` |
/// | `fitted` | the picked part is the one in its bay | `fitted - drag a chip from the rail, or tap a bay to replace it` |
fn strip(
    ui: &mut egui::Ui,
    cat: Option<&Catalog>,
    draft: Option<&Draft>,
    vis: &Visual,
    info: &mut crate::clip::LayoutInfo,
    phone: bool,
) {
    let selected = vis.selected_part.clone();
    let slot = vis.selected_slot;
    let dragging = vis.drag.is_some();
    // "Fitted" is measured against the draft, not assumed: the picked part is fitted when its id is the one
    // the bay holds. (A chip click can select a part that is not the fitted record.)
    let fitted = match (&selected, draft) {
        (Some(part), Some(d)) => match part.class {
            Class::Fan => d.0.fan.id == part.id,
            Class::Drift => d.0.drift.id == part.id,
            Class::Nozzle => d.0.nozzle.id == part.id,
            Class::Fill => d.0.fill_layers.iter().any(|l| l.fill_id == part.id),
        },
        _ => false,
    };
    let (state, hint) = if dragging {
        (
            "dragging",
            "release over a bay that accepts it - every bay answers before you release",
        )
    } else if selected.is_some() && fitted {
        (
            "fitted",
            "fitted - drag a chip from the rail, or tap a bay to replace it",
        )
    } else if selected.is_some() {
        (
            "selected",
            "drag it onto a bay, or tap a bay to open its picker",
        )
    } else {
        (
            "none",
            "drag a chip from the rail onto a bay, or tap a bay for its picker",
        )
    };
    info.hint_state = state;
    info.hint = hint.to_string();

    let note = "a drop replaces a fixture identity only - no new physics field";
    let frame = t::card_flat();
    frame.show(ui, |ui| {
        // ---- row 1: the part, its spec and the catalog's own verdict.
        ui.horizontal_wrapped(|ui| {
            ui.label(t::eyebrow("selected part"));
            match (&selected, cat, draft) {
                (Some(part), Some(cat), Some(draft)) => {
                    chip(ui, part.class.name(), t::INK, t::PANEL_RAISED, t::LINE);
                    ui.label(
                        RichText::new(&part.id)
                            .size(13.0)
                            .color(t::INK)
                            .family(t::family_mono_med()),
                    );
                    if !phone {
                        ui.label(
                            RichText::new(state::part_spec(cat, part))
                                .size(10.5)
                                .color(t::MUTED),
                        );
                    }
                    let verdict = check_drop(cat, &draft.0, slot, part);
                    match &verdict {
                        Ok(()) => {
                            chip(
                                ui,
                                &format!("VALID for the {} bay", slot.name()),
                                t::VALID,
                                t::OK_SOFT,
                                t::with_alpha(t::VALID, 140),
                            );
                        }
                        Err(reason) => {
                            chip(
                                ui,
                                &format!("REFUSED by the {} bay", slot.name()),
                                t::INVALID,
                                t::DANGER_SOFT,
                                t::with_alpha(t::INVALID, 140),
                            );
                            ui.label(RichText::new(reason).size(10.5).color(t::DANGER));
                        }
                    }
                }
                _ => {
                    ui.label(RichText::new("nothing selected").size(11.5).color(t::INK_2));
                }
            }
            if let Some(flash) = &vis.flash {
                let (fg, bg) = if flash.ok {
                    (t::VALID, t::OK_SOFT)
                } else {
                    (t::INVALID, t::DANGER_SOFT)
                };
                chip(ui, &flash.text, fg, bg, t::with_alpha(fg, 130));
            }
        });
        // ---- row 2: the one hint string, and the drop note (right-aligned on a desktop, its own line on a
        // phone: a 390 px strip cannot hold both, and a wrapped hint is better than an overlapped one).
        ui.add_space(2.0);
        if phone {
            // Truncated, not wrapped-on-layout: the hint and the note are free text, and a phone's strip is
            // inside the stack - a label that wants more room than it has would widen the stack and every card
            // under it (the desktop branch has always truncated here).
            ui.add(egui::Label::new(RichText::new(hint).size(9.5).color(t::INK_2)).truncate());
            ui.add(egui::Label::new(RichText::new(note).size(9.0).color(t::MUTED)).truncate());
        } else {
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let note_w = measure_text(ui, note, 9.5, egui::FontFamily::Proportional);
                let room = (avail - note_w - 16.0).max(80.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(room, 16.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(hint).size(10.0).color(t::INK_2))
                                .truncate(),
                        );
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(note).size(9.5).color(t::MUTED));
                });
            });
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
        ui.label(t::body("the engine is not loaded"));
        return;
    };
    ui.label(t::eyebrow("operating point — fan/system and performance"));
    ui.label(RichText::new(format!("every series below is returned by Engine::run at speed ratio {:.3} ({}), or is the recorded fixture grid read through the engine's own tables. The dashed series is the recorded run for comparison.", draft.0.speed_ratio, draft.0.fan.id)).size(10.5).color(t::MUTED));
    ui.add_space(6.0);

    let Some(o) = run.output.as_ref() else {
        t::card().show(ui, |ui| {
            ui.label(t::heading("no run"));
            ui.label(t::body(
                "the engine refused this input, so there is no curve to plot.",
            ));
        });
        return;
    };

    let fan_series = ChartSeries {
        label: format!("fan curve at ratio {:.2}", draft.0.speed_ratio),
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
            label: format!("operating point · {:.1} m3/s, {:.1} Pa", op.x, op.y),
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
                &format!("{:.1} / {:.1}", op.x, op.y),
                "m3/s / Pa",
                true,
            );
        });
    });
    ui.label(RichText::new(format!("the crossing is the engine's own solve; the rpm slider re-runs it and the crossing moves with the fan curve. Pressure basis: total (fan record {} pressureBasis = {}), the same basis as the tower system curve, and the same number the instrument's read-out shows.", draft.0.fan.id, draft.0.fan.pressure_basis)).size(9.5).color(t::MUTED));

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
                    "duty point · {:.0} m3/hr, {:.1} C WB, {:.2} C CW",
                    o.water_flow_m3_hr, draft.0.duty.wet_bulb_c, o.cold_water_c
                ),
                color: t::AMBER,
                above: true,
            }];
            if let Some(p) = grid.flow_at_fraction(0.9) {
                markers_a.push(Marker {
                    x: grid.design_wb_c,
                    y: p.y,
                    label: format!("90 % flow · {:.0} kg/s, {:.2} C", p.x, p.y),
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
                "entering wet bulb, C",
                "cold water, C",
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
                        "100 % flow = the design {:.0} kg/s ({:.0} m3/hr) · the duty point sits on the sweep at the design wet bulb {:.1} C",
                        grid.design_flow_kg_s, grid.design_flow_m3_hr, grid.design_wb_c
                    ))
                    .size(10.0)
                    .color(t::MUTED),
                );
            });
            let inf_hi = grid.wb.pts.iter().filter(|p| !p.ok).count();
            ui.label(
                RichText::new(format!(
                    "series not in fixture: no 90 % / 110 % water-flow lines. The fixture's sweeps are one-parameter (wet bulb at the design flow, flow at the design wet bulb), and the engine's own sweep marks 110 % of the design flow infeasible; {inf_hi} wet-bulb samples above the design point are drawn as open markers for the same reason. Nothing is computed to fill either gap."
                ))
                .size(9.5)
                .color(t::AMBER),
            );
            ui.label(
                RichText::new("comparable to CTI/MRL curve presentation — engine output, not a certified test")
                    .size(9.5)
                    .color(t::MUTED),
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
                "entering wet bulb, C",
                "cold water, C",
                false,
                false,
                &[],
                &[],
                &[],
                &[],
            );
            ui.label(
                RichText::new("series not in fixture — this fixture carries no recorded wet-bulb sweep, so the axes stand alone and nothing is computed to fill them.")
                    .size(9.5)
                    .color(t::AMBER),
            );
            ui.label(
                RichText::new("comparable to CTI/MRL curve presentation — engine output, not a certified test")
                    .size(9.5)
                    .color(t::MUTED),
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
            "KaV/L convention: the layer's Merkel number — the engine's own sampled characteristic (kavlPerM x depth x that layer's recorded thermal multiplier), the same table and the same sum `Engine::run` takes for fill KaV/L. L/G = water loading / dry-air loading, both the recorded run's ({:.3} / {:.3} kg/m2 s = {:.3}); the characteristic is tabulated at the recorded dry-air loading {:.3} kg/m2 s.",
            perf.map(|g| g.water_loading_kg_m2_s).unwrap_or(0.0),
            dry_air,
            lg_op.unwrap_or(0.0),
            dry_air
        ))
        .size(9.5)
        .color(t::MUTED),
    );
    ui.label(
        RichText::new("the demand is one recorded value, not a curve: the fixture carries no demand sweep, so it is drawn as a reference line at the engine's own KaV/L for the fill, and the operating L/G is marked where the run sits against it.")
            .size(9.5)
            .color(t::AMBER),
    );
    ui.label(
        RichText::new(
            "comparable to CTI/MRL curve presentation — engine output, not a certified test",
        )
        .size(9.5)
        .color(t::MUTED),
    );

    ui.add_space(8.0);
    if let Some(cat) = cat {
        t::card_flat().show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} · catalog {} · {} · inlet air recorded at {:.1} C DB / {:.1} C WB",
                    o.provenance.engine.rsplit('/').next().unwrap_or("engine"),
                    cat.catalog_id,
                    cat.catalog_status,
                    cat.ambient.map(|a| a.dry_bulb_c).unwrap_or(0.0),
                    cat.ambient.map(|a| a.wet_bulb_c).unwrap_or(0.0)
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
            "series not in fixture",
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

// ======================================================================================= seams view

fn seams_view(ui: &mut egui::Ui, vis: &mut Visual, phone: bool) {
    let counts = seams::status_counts();
    ui.label(t::heading("Data seams"));
    ui.label(RichText::new(format!("{} bindings · {}. Generated from seams/src/lib.rs (tools/gen-seams.sh) - the same table VISUAL_DATA_SEAMS.md ships.", seams::SEAMS.len(), counts.line())).size(11.0).color(t::INK_2));
    ui.label(
        RichText::new(seams::REQUIRED_COPY)
            .size(11.0)
            .color(t::PRIMARY),
    );
    ui.add_space(8.0);
    for s in seams::SEAMS.iter() {
        t::card_flat().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(s.id)
                        .size(12.0)
                        .color(t::INK)
                        .family(t::family_mono_med()),
                );
                status_chip(ui, s.status);
                if !phone {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(s.code)
                                .size(9.5)
                                .color(t::MUTED)
                                .family(egui::FontFamily::Monospace),
                        );
                    });
                }
            });
            if phone {
                // A 390px card cannot hold a right-aligned code path: it gets its own wrapping line rather
                // than being clipped at both edges.
                ui.add(
                    egui::Label::new(
                        RichText::new(s.code)
                            .size(9.5)
                            .color(t::MUTED)
                            .family(egui::FontFamily::Monospace),
                    )
                    .wrap(),
                );
            }
            ui.label(
                RichText::new(format!("drives: {}", s.drives))
                    .size(10.5)
                    .color(t::INK_2),
            );
            ui.label(
                RichText::new(format!("today: {} → {}", s.source, s.rule))
                    .size(10.0)
                    .color(t::MUTED),
            );
            ui.label(
                RichText::new(format!("binds to the engine field: {}", s.engine_field))
                    .size(10.0)
                    .color(t::PRIMARY),
            );
        });
        ui.add_space(4.0);
    }
    // Room under the last card, so it is not sliced by the status strip.
    ui.add_space(if phone { 30.0 } else { 22.0 });
    if !phone
        && ui
            .add_sized(
                egui::vec2(180.0, 24.0),
                egui::Button::new("back to the instrument"),
            )
            .clicked()
    {
        vis.view = View::Cockpit;
    }
}
