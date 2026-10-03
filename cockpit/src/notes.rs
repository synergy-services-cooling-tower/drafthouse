//! Issue #91 (usability round 1): **the notes drawer.**
//!
//! The owner's words were "too much text". The instrument's screen now carries numbers, units and short
//! labels; every explanatory sentence that used to be painted beside them lives here, behind the one
//! validation badge (header on a desktop, bottom bar on a phone), the `I` key, the control bar's `notes`
//! button, or the `notes` dispatch command. Nothing was deleted: each sentence below is the one that was
//! on screen, grouped by where it used to be. The decision-12 label itself is NOT moved - the badge
//! carries it verbatim on every frame (`cockpit::host::PUBLIC_LABEL` / `INTERNAL_LABEL`).
//!
//! Drawn in its own foreground layer, closed by its x, by Esc, or by the badge again.

use bevy_egui::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, StrokeKind};

use crate::app::{Draft, HitMap, StagedLog};
use crate::duty_panel::DutyRes;
use crate::state::{StartOptions, Visual};
use crate::theme as t;

/// The scene's honesty lines (the old legend's second and third lines).
pub const SCENE_NOTES: &[&str] = &[
    "Streamlines are an illustration, not a CFD result.",
    "Spray coverage is illustrative: there is no distribution model.",
    "Droplet density follows the water loading, streamline speed follows the engine airflow, the fan turns at the engine rpm; colour runs from hot water (top) to cold water (basin).",
    "The air-path split bar is the engine's per-zone pressure split at the recorded run; the operating-point pressure is stated once, with the operating point.",
];

/// The validation lines that used to be painted in the header and the status strip.
pub const VALIDATION_NOTES: &[&str] = &[
    "No CFD · no bypass · no CTI/MRL validation or certification claim · no money field.",
    "This visual pass adds no physics: every number is the engine's own output.",
];

/// The control explanations (dock, parts rail, selected-part strip).
pub const CONTROL_NOTES: &[&str] = &[
    "The engine is re-run on every frame the speed slider moves - the scene, the rail and the operating point all read that output.",
    "Drag a chip from the parts rail onto a bay, or tap a bay for its picker. Every bay answers before you release.",
    "A drop replaces a fixture identity only - no new physics field.",
    "Operating point: the engine's fan/system solve, on the fan record's own pressure basis.",
    "Read-out: the recorded run's own air-path total is held at the recorded flow; the operating point moves with the speed ratio, and the read-out moves with it. Δ is against the recorded run.",
    "Outside the recorded sweep the fixture engine may only interpolate what it recorded: edit the duty to come back inside it.",
];

/// The duty & site explanations.
pub const DUTY_NOTES: &[&str] = &[
    "Seeded from fixtures/engine-run.json → provenance.duty / provenance.fixed · every change re-runs the engine.",
    "Range and approach are the duty's design pair, live; the read-out reports the engine's own solved cold water and its range_c / approach_c.",
    "Outside the recorded domain the read-out prints `out of fixture range` instead of a number: the fixture engine may only interpolate what it recorded.",
];

/// The keyboard (the control bar's old hint line, completed).
pub const KEYS: &[(&str, &str)] = &[
    ("1 / 2 / 3", "Instrument / Operating point / Data seams"),
    ("[  ]", "fan speed down / up"),
    ("R", "back to the fixture's own ratio"),
    ("B · E", "next bay · open its picker"),
    ("↑ ↓ ⏎ Esc", "picker rows · pick · close"),
    ("Tab", "cycle the overlay emphasis"),
    ("G · L · K", "grid · scene labels · parts rail"),
    ("F · M", "freeze the clock · reduced motion"),
    ("I", "these notes"),
];

/// The drawer. `top` is the header's bottom edge; on a phone the drawer is a bottom sheet over the scene.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ctx: &egui::Context,
    vis: &mut Visual,
    hits: &mut HitMap,
    options: &StartOptions,
    engine_name: &str,
    staged: &StagedLog,
    duty: &DutyRes,
    draft: Option<&Draft>,
    screen: egui::Rect,
    top: f32,
    phone: bool,
) {
    if !vis.notes_open {
        return;
    }
    let w = if phone {
        screen.width()
    } else {
        420.0_f32.min(screen.width() - 24.0)
    };
    let rect = if phone {
        egui::Rect::from_min_max(
            egui::pos2(
                screen.left(),
                (screen.top() + screen.height() * 0.30).max(top),
            ),
            screen.max,
        )
    } else {
        egui::Rect::from_min_max(
            egui::pos2(screen.right() - w - 12.0, top + 10.0),
            egui::pos2(screen.right() - 12.0, screen.bottom() - 12.0),
        )
    };
    hits.0.push((
        "notes:panel".to_string(),
        [rect.min.x, rect.min.y, rect.width(), rect.height()],
    ));
    let notice = options.host.notice();
    egui::Area::new(egui::Id::new("viz.notes"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_clip_rect(rect);
            let p = ui.painter();
            p.rect_filled(
                rect.translate(egui::vec2(0.0, 6.0)).expand(2.0),
                egui::CornerRadius::same(12),
                Color32::from_black_alpha(90),
            );
            p.rect_filled(rect, egui::CornerRadius::same(10), t::with_alpha(t::PANEL, 250));
            p.rect_stroke(
                rect,
                egui::CornerRadius::same(10),
                Stroke::new(1.0, t::LINE),
                StrokeKind::Inside,
            );
            let inner = rect.shrink2(egui::vec2(16.0, 12.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                ui.set_width(inner.width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Notes & validation")
                            .size(15.0)
                            .color(t::INK)
                            .family(t::family_semi()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let side = if phone { 44.0 } else { 30.0 };
                        let (r, resp) =
                            ui.allocate_exact_size(egui::vec2(side, side), Sense::click());
                        let hot = resp.hovered();
                        ui.painter().rect_filled(
                            r,
                            egui::CornerRadius::same(6),
                            if hot { t::PANEL_RAISED } else { Color32::TRANSPARENT },
                        );
                        ui.painter().text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            "×",
                            FontId::new(18.0, t::family_semi()),
                            t::INK_2,
                        );
                        hits.0.push((
                            "notes:close".to_string(),
                            [r.min.x, r.min.y, r.width(), r.height()],
                        ));
                        if resp.clicked() {
                            vis.notes_open = false;
                        }
                    });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("notes-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        section(ui, "validation");
                        // The decision-12 label, verbatim, at the head of the drawer as well as on the badge.
                        t::chip_frame(
                            if notice.public { t::PANEL_RAISED } else { t::PRIMARY_SOFT },
                            t::LINE,
                        )
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&notice.text)
                                        .size(11.0)
                                        .color(t::INK)
                                        .family(t::family_semi()),
                                )
                                .wrap(),
                            );
                        });
                        line(ui, drafthouse_cockpit_seams::REQUIRED_COPY);
                        for s in VALIDATION_NOTES {
                            line(ui, s);
                        }
                        line(ui, &format!("Engine: {engine_name}."));
                        if let Some(l) = options.host.catalog_label.as_deref() {
                            line(ui, &format!("Catalog: {l} (synthetic data - not vendor data)."));
                        }

                        section(ui, "the scene");
                        for s in SCENE_NOTES {
                            line(ui, s);
                        }

                        section(ui, "controls");
                        for s in CONTROL_NOTES {
                            line(ui, s);
                        }
                        if let Some(d) = draft {
                            let [lo, hi] = d.0.fan.allowed_speed_ratio;
                            line(
                                ui,
                                &match d.0.fan.nominal_rpm {
                                    Some(n) => format!(
                                        "rpm = speed ratio × {} - the fan record's nominalRpm at ratio 1.000. {} accepts ratios {:.2}-{:.2}; outside that band the engine returns no numbers rather than a clamped ratio.",
                                        t::fmt(n),
                                        d.0.fan.id,
                                        lo,
                                        hi
                                    ),
                                    None => "This fan record states no rated speed: no rpm is shown rather than an invented one.".to_string(),
                                },
                            );
                            line(
                                ui,
                                &format!(
                                    "Pressure basis of the fan record: {}.",
                                    d.0.fan.pressure_basis
                                ),
                            );
                        }

                        section(ui, "duty & site");
                        for s in DUTY_NOTES {
                            line(ui, s);
                        }
                        if let Some(spec) = duty.spec.as_ref() {
                            line(
                                ui,
                                &format!(
                                    "The engine's own water pair: {} kg/s = {} m3/hr at {} kg/m3 (the form converts at 1000 kg/m3).",
                                    t::fmt(spec.psychro.recorded_water_mass_flow_kg_s),
                                    t::fmt(spec.psychro.recorded_water_flow_m3_hr),
                                    t::fmt(spec.psychro.water_density_kg_m3)
                                ),
                            );
                            line(
                                ui,
                                &format!(
                                    "Recorded evidence: water flow {:.0}-{:.0} m3/hr ({:.0}-{:.0} kg/s), wet bulb {:.1}-{:.1} C — {}",
                                    spec.evidence.water_flow_m3_hr[0],
                                    spec.evidence.water_flow_m3_hr[1],
                                    spec.evidence.water_mass_flow_kg_s[0],
                                    spec.evidence.water_mass_flow_kg_s[1],
                                    spec.evidence.wet_bulb_c[0],
                                    spec.evidence.wet_bulb_c[1],
                                    spec.evidence.note
                                ),
                            );
                        }

                        section(ui, "staged");
                        if staged.0.is_empty() {
                            line(ui, "Nothing staged from the URL.");
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(staged.0.join(" · "))
                                        .size(10.5)
                                        .color(t::INK_2)
                                        .family(t::family_mono_med()),
                                )
                                .wrap(),
                            );
                        }

                        section(ui, "keyboard");
                        egui::Grid::new("notes-keys")
                            .num_columns(2)
                            .spacing(egui::vec2(12.0, 4.0))
                            .show(ui, |ui| {
                                for (k, what) in KEYS {
                                    ui.label(
                                        RichText::new(*k)
                                            .size(11.0)
                                            .color(t::INK)
                                            .family(t::family_mono_med()),
                                    );
                                    ui.label(RichText::new(*what).size(11.0).color(t::INK_2));
                                    ui.end_row();
                                }
                            });
                        ui.add_space(12.0);
                    });
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        vis.notes_open = false;
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(t::eyebrow(title));
}

fn line(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(t::INK_2)).wrap());
}
