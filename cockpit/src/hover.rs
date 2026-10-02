//! Round 4, item 3: **the parameter card.**
//!
//! Hovering (desktop) or long-pressing (phone) a rail chip, a picker card or a fitted bay shows every field
//! of that record, with its unit and its source (`catalog` / `custom` / `fixture`), and - for the fitted
//! part - the values `Engine::run` actually used. A curve-typed field draws a sparkline of its own recorded
//! points instead of a scalar. The card is always placed so that it does **not** cover the rect it describes.
//!
//! The rows come from `drafthouse_cockpit_seams::fields` (unit-tested); this module resolves what is under the
//! pointer, decides when to show the card, and draws it.

use bevy::prelude::*;
use bevy_egui::egui;
use egui::{Color32, Rect, RichText, Sense, Stroke};

use drafthouse_cockpit_seams::fields::{self, Row};

use crate::app::{Draft, HitMap};
use crate::state::{Anchor, Catalog, Class, PartRef, Run, Slot, Visual};
use crate::theme as t;

/// How long a press has to be held on a phone before the card appears.
pub const LONG_PRESS_S: f64 = 0.6;
/// How far the pointer may move during a press before it counts as a drag instead.
pub const PRESS_SLOP_PX: f32 = 12.0;
/// The gap between the card and the rect it describes.
pub const CARD_GAP_PX: f32 = 10.0;
pub const CARD_W: f32 = 286.0;

/// Which record the pointer is on, and what the card must not cover.
#[derive(Resource, Clone, Debug, Default)]
pub struct HoverState {
    pub part: Option<PartRef>,
    /// The rect (canvas points) the card is about; it must never be covered by the card.
    pub anchor: [f32; 4],
    /// True when a long press put it up (the phone path), false for a desktop hover.
    pub long_press: bool,
    /// True when the hovered bay holds the part (the bay card, not a chip card).
    pub bay: bool,
    /// True when a `?hover=` URL put the card up: the evidence path, which the pointer must not clear.
    pub staged: bool,
    /// What the card actually drew: how many curve fields became a sparkline, and how many rows came from
    /// the engine's own run. Published as `data-hover-curve-rows` / `data-hover-engine-rows`.
    pub curve_rows: usize,
    pub engine_rows: usize,
    press: Option<Press>,
}

#[derive(Clone, Debug)]
struct Press {
    t: f64,
    at: egui::Pos2,
    part: PartRef,
    anchor: [f32; 4],
    bay: bool,
}

impl HoverState {
    /// Show a specific part (the URL path: `?hover=fill:FILM-MF20&hover-long=1`).
    pub fn stage(&mut self, part: PartRef, long_press: bool, bay: bool) {
        self.part = Some(part);
        self.anchor = [0.0; 4];
        self.long_press = long_press;
        self.bay = bay;
        self.staged = true;
        self.press = None;
    }
    pub fn clear(&mut self) {
        self.part = None;
        self.anchor = [0.0; 4];
        self.long_press = false;
        self.bay = false;
        self.staged = false;
        self.press = None;
    }
}

/// What is under the pointer, from the hit rects the instrument published this frame.
///
/// Priority: a bay first (the biggest target, and the one the brief names last), then a rail chip, then a
/// picker card. A bay resolves to the part fitted *in* it - an empty bay has no record to describe.
fn under(
    hits: &HitMap,
    pos: egui::Pos2,
    draft: Option<&Draft>,
) -> Option<(PartRef, [f32; 4], bool)> {
    let inside = |r: &[f32; 4]| {
        r[2] > 1.0
            && r[3] > 1.0
            && pos.x >= r[0]
            && pos.x <= r[0] + r[2]
            && pos.y >= r[1]
            && pos.y <= r[1] + r[3]
    };
    let find = |prefix: &str| -> Option<(String, [f32; 4])> {
        hits.0
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .find(|(_, r)| inside(r))
            .map(|(k, r)| (k.clone(), *r))
    };
    if let Some((key, r)) = find("bay:") {
        let slot_slug = key.trim_start_matches("bay:");
        if let Some(slot) = Slot::ALL.iter().find(|s| s.slug() == slot_slug) {
            if let Some(part) = fitted(draft, *slot) {
                return Some((part, r, true));
            }
        }
        return None;
    }
    if let Some((key, r)) = find("rail:chip:") {
        let rest = key.trim_start_matches("rail:chip:");
        if let Some(part) = PartRef::parse(rest) {
            return Some((part, r, false));
        }
    }
    if let Some((key, r)) = find("picker:") {
        let rest = key.trim_start_matches("picker:");
        if let Some(part) = PartRef::parse(rest) {
            return Some((part, r, false));
        }
    }
    None
}

/// The part fitted in a bay (empty for a bay with nothing in it).
fn fitted(draft: Option<&Draft>, slot: Slot) -> Option<PartRef> {
    let d = draft?;
    match slot {
        Slot::Fan => Some(PartRef::new(Class::Fan, d.0.fan.id.clone())),
        Slot::Drift => Some(PartRef::new(Class::Drift, d.0.drift.id.clone())),
        Slot::Nozzle => Some(PartRef::new(Class::Nozzle, d.0.nozzle.id.clone())),
        Slot::Fill => {
            d.0.fill_layers
                .first()
                .map(|l| PartRef::new(Class::Fill, l.fill_id.clone()))
        }
    }
}

/// Update the state from the pointer: hover on a desktop, a held press on a phone. `staged` keeps a
/// URL-staged card up (the evidence path) instead of clearing it on the next frame.
#[allow(clippy::too_many_arguments)]
pub fn update(
    ctx: &egui::Context,
    hits: &HitMap,
    draft: Option<&Draft>,
    st: &mut HoverState,
    phone: bool,
) {
    let pos = ctx.pointer_latest_pos();
    let (down, now) = ctx.input(|i| (i.pointer.any_down(), i.time));
    let target = pos.and_then(|p| under(hits, p, draft));

    if down {
        let p = pos.unwrap_or(egui::pos2(-99.0, -99.0));
        match &st.press {
            None => {
                if let Some((part, anchor, bay)) = target {
                    st.press = Some(Press {
                        t: now,
                        at: p,
                        part,
                        anchor,
                        bay,
                    });
                }
            }
            Some(press) => {
                let moved = (p - press.at).length();
                if moved > PRESS_SLOP_PX {
                    st.press = None; // a drag, not a hold: the chip is being carried to a bay
                } else if now - press.t >= LONG_PRESS_S {
                    st.part = Some(press.part.clone());
                    st.anchor = press.anchor;
                    st.long_press = true;
                    st.bay = press.bay;
                    st.staged = false;
                }
            }
        }
        return;
    }

    st.press = None;
    if st.staged {
        return;
    }
    if phone {
        // A phone has no hover: the card is put up by the long press and stays until the next tap.
        if ctx.input(|i| i.pointer.any_click()) {
            st.clear();
        }
        return;
    }
    match target {
        Some((part, anchor, bay)) => {
            st.part = Some(part);
            st.anchor = anchor;
            st.long_press = false;
            st.bay = bay;
            st.staged = false;
        }
        None => st.clear(),
    }
}

/// Draw the card, next to the rect it describes and never over it. Returns the card's rect.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ctx: &egui::Context,
    st: &mut HoverState,
    cat: Option<&Catalog>,
    fields: &drafthouse_cockpit_seams::custom::Fields,
    run: &Run,
    anchor_run: &Anchor,
    vis: &mut Visual,
    hits: &mut HitMap,
    draft: Option<&Draft>,
    screen: Rect,
) -> Option<Rect> {
    let part = st.part.clone()?;
    // A staged card has no rect to sit beside (nothing was hovered in this session): it takes the right
    // column, which no bay reaches.
    let anchor = if st.anchor[2] <= 1.0 {
        Rect::from_min_size(
            egui::pos2(screen.right() - CARD_W - 18.0, screen.top() + 120.0),
            egui::vec2(1.0, 1.0),
        )
    } else {
        Rect::from_min_size(
            egui::pos2(st.anchor[0], st.anchor[1]),
            egui::vec2(st.anchor[2].max(1.0), st.anchor[3].max(1.0)),
        )
    };
    let (rows, source, title) = rows_for(&part, cat, fields, run, anchor_run, draft)?;
    st.curve_rows = rows.iter().filter(|r| r.curve.is_some()).count();
    st.engine_rows = rows.iter().filter(|r| r.source == "engine").count();

    // ---- placement: right, left, below, above - the first one that fits and does not overlap the anchor.
    let est_h = (72.0 + rows.len() as f32 * 15.0).min(screen.height() - 80.0);
    let w = CARD_W.min(screen.width() - 24.0);
    let candidates = [
        egui::pos2(anchor.right() + CARD_GAP_PX, anchor.top()),
        egui::pos2(anchor.left() - CARD_GAP_PX - w, anchor.top()),
        egui::pos2(anchor.left(), anchor.bottom() + CARD_GAP_PX),
        egui::pos2(anchor.left(), anchor.top() - CARD_GAP_PX - est_h),
    ];
    let fit = |p: egui::Pos2| -> Option<egui::Pos2> {
        let r = Rect::from_min_size(p, egui::vec2(w, est_h));
        if r.left() < screen.left() + 6.0
            || r.right() > screen.right() - 6.0
            || r.top() < screen.top() + 6.0
            || r.bottom() > screen.bottom() - 6.0
        {
            return None;
        }
        if r.intersects(anchor) {
            return None;
        }
        Some(p)
    };
    // Fall back to the least-overlapping side rather than hiding the card.
    let pos = candidates.iter().find_map(|p| fit(*p)).unwrap_or_else(|| {
        egui::pos2(
            (screen.right() - w - 8.0).max(screen.left() + 8.0),
            (screen.top() + 110.0).min(screen.bottom() - est_h - 8.0),
        )
    });

    let area = egui::Area::new(egui::Id::new("viz.hover-card"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_max_width(w);
            egui::Frame::new()
                .fill(t::PANEL_RAISED)
                .stroke(Stroke::new(
                    1.0,
                    if st.long_press { t::AMBER } else { t::LINE },
                ))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(9, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&part.id)
                                .size(12.0)
                                .color(t::INK)
                                .family(t::family_mono_med()),
                        );
                        ui.label(
                            RichText::new(format!("· {}", part.class.name()))
                                .size(10.0)
                                .color(t::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            chip(ui, source_label(source), source_color(source));
                            if st.bay {
                                chip(ui, "fitted bay", t::PRIMARY);
                            }
                            if st.long_press {
                                chip(ui, "long press", t::AMBER);
                            }
                        });
                    });
                    ui.label(RichText::new(title).size(9.5).color(t::MUTED));
                    ui.add_space(2.0);
                    // A record can carry more rows than the frame has height (a fill's characteristic, a
                    // phone's narrow column), so the row list scrolls inside the card: the card's own rect
                    // stays the size the placement above measured, and it can never run off the viewport.
                    egui::ScrollArea::vertical()
                        .id_salt("viz.hover-rows")
                        .auto_shrink([false, true])
                        .max_height((est_h - 62.0).max(60.0))
                        .show(ui, |ui| {
                            for row in rows.iter() {
                                row_line(ui, row, w - 40.0);
                            }
                        });
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(if st.bay {
                            "every field of the fitted record, and the values the engine used · esc clears"
                        } else {
                            "every field of this record · hover for the values the engine used on the fitted copy"
                        })
                        .size(9.0)
                        .color(t::MUTED),
                    );
                });
        });
    let rect = Some(area.response.rect);
    if let Some(r) = rect {
        hits.0.push((
            "hover:card".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
    }
    if area.response.hovered() && !st.long_press {
        // Keep the card alive while the pointer is over the card itself (it is not interactable, but the
        // rect is a seam the frame reads).
        vis.selected_part = Some(part.clone());
    }
    let _ = Sense::hover();
    rect
}

/// The rows of a part: its record's own fields, and - for the fitted copy - the engine's values.
fn rows_for(
    part: &PartRef,
    cat: Option<&Catalog>,
    fields: &drafthouse_cockpit_seams::custom::Fields,
    run: &Run,
    anchor_run: &Anchor,
    draft: Option<&Draft>,
) -> Option<(Vec<Row>, &'static str, String)> {
    let cat = cat?;
    let custom = cat.custom_part(part.class, &part.id);
    let source: &'static str = if custom.is_some() {
        "custom"
    } else {
        "catalog"
    };
    let (mut rows, title) = match part.class {
        Class::Fan => {
            let r = cat.fan(&part.id)?;
            (
                fields::fan_rows(r, source),
                format!("fan record · stack {:.2} m2", r.stack_area_m2),
            )
        }
        Class::Drift => {
            let r = cat.drift(&part.id)?;
            (
                fields::drift_rows(r, source),
                format!(
                    "drift eliminator · max water {:.0} C",
                    r.max_water_temperature_c
                ),
            )
        }
        Class::Fill => {
            let r = cat.fill(&part.id)?;
            (
                fields::fill_rows(r, source),
                format!(
                    "fill record · {} · {}",
                    r.record.geometry,
                    r.record.compatible_tower_types.join("/")
                ),
            )
        }
        Class::Nozzle => {
            let r = cat.nozzle(&part.id)?;
            (
                fields::nozzle_rows(r, source),
                format!(
                    "nozzle record · orifice {:.0} mm",
                    r.orifice_diameter_m * 1000.0
                ),
            )
        }
    };
    if let Some(c) = custom {
        // A custom record's card reads the values the user entered, through the generated descriptor - and
        // says which of them the fixture catalog could not have held.
        let custom_rows = fields::custom_rows(c, fields);
        if !custom_rows.is_empty() {
            rows = custom_rows;
        }
    }

    // ---- the values the engine actually used, when the record is the one in the machine.
    let fitted_now = draft
        .map(|d| match part.class {
            Class::Fan => d.0.fan.id == part.id,
            Class::Drift => d.0.drift.id == part.id,
            Class::Nozzle => d.0.nozzle.id == part.id,
            Class::Fill => d.0.fill_layers.iter().any(|l| l.fill_id == part.id),
        })
        .unwrap_or(false);
    if fitted_now {
        for r in engine_rows(part, run, anchor_run, draft) {
            rows.push(r);
        }
    }
    Some((rows, source, title))
}

/// The engine's own numbers for the fitted part, as extra rows (source `engine`).
fn engine_rows(part: &PartRef, run: &Run, anchor_run: &Anchor, draft: Option<&Draft>) -> Vec<Row> {
    let mut out = Vec::new();
    let Some(o) = run.output.as_ref() else {
        return out;
    };
    match part.class {
        Class::Fan => {
            out.push(Row::new(
                "engine airflow",
                t::fmt(o.airflow_m3_s),
                "m3/s",
                "engine",
            ));
            out.push(Row::new(
                "engine fan power",
                t::fmt(o.fan_power_kw),
                "kW",
                "engine",
            ));
            out.push(Row::new(
                "engine operating point",
                format!(
                    "{} / {}",
                    t::fmt(o.fan_system_curve.operating_point.x),
                    t::fmt(o.fan_system_curve.operating_point.y)
                ),
                "m3/s / Pa",
                "engine",
            ));
            if let (Some(a), Some(d)) = (anchor_run.output.as_ref(), draft) {
                let _ = d;
                out.push(Row::new(
                    "engine delta vs recorded",
                    format!(
                        "{:+.2} / {:+.2}",
                        o.airflow_m3_s - a.airflow_m3_s,
                        o.fan_system_curve.operating_point.y - a.fan_system_curve.operating_point.y
                    ),
                    "m3/s / Pa",
                    "engine",
                ));
            }
        }
        Class::Fill => {
            for l in o.kavl_per_layer.iter() {
                if l.fill_id != part.id {
                    continue;
                }
                out.push(Row::new(
                    format!("engine layer {} KaV/L", l.index + 1),
                    format!("{:.3}", l.kavl),
                    "-",
                    "engine",
                ));
                out.push(Row::new(
                    format!("engine layer {} pressure", l.index + 1),
                    t::fmt(l.pressure_pa),
                    "Pa",
                    "engine",
                ));
                out.push(Row::new(
                    format!("engine layer {} cooling share", l.index + 1),
                    format!("{:.1}", l.cooling_share_pct),
                    "%",
                    "engine",
                ));
                out.push(Row::new(
                    format!("engine layer {} envelope", l.index + 1),
                    if l.inside_envelope {
                        "inside"
                    } else {
                        "outside"
                    },
                    "-",
                    "engine",
                ));
            }
        }
        Class::Drift => {
            if let Some(z) = o
                .pressure_by_zone
                .iter()
                .find(|z| z.zone == cockpit::engine::ZoneId::Drift)
            {
                out.push(Row::new(
                    "engine drift pressure",
                    t::fmt(z.pressure_pa),
                    "Pa",
                    "engine",
                ));
                out.push(Row::new(
                    "engine drift share",
                    format!("{:.1}", z.share_pct),
                    "%",
                    "engine",
                ));
            }
        }
        Class::Nozzle => {
            if let Some(z) = o
                .pressure_by_zone
                .iter()
                .find(|z| z.zone == cockpit::engine::ZoneId::Spray)
            {
                out.push(Row::new(
                    "engine spray-zone pressure",
                    t::fmt(z.pressure_pa),
                    "Pa",
                    "engine",
                ));
            }
            out.push(Row::new(
                "engine circulating flow",
                t::fmt(o.water_flow_m3_hr),
                "m3/hr",
                "engine",
            ));
        }
    }
    out
}

fn source_label(source: &str) -> &'static str {
    match source {
        "custom" => "custom · session only",
        "catalog" => "catalog",
        "fixture" => "fixture data",
        _ => "engine",
    }
}

fn source_color(source: &str) -> Color32 {
    match source {
        "custom" => t::AMBER,
        "catalog" => t::INK_2,
        _ => t::PRIMARY,
    }
}

fn chip(ui: &mut egui::Ui, text: &str, fg: Color32) {
    t::chip_frame(t::PANEL, t::with_alpha(fg, 140)).show(ui, |ui| {
        ui.label(
            RichText::new(text)
                .size(9.0)
                .color(fg)
                .family(t::family_semi()),
        );
    });
}

/// One row of the card: key, value, unit - and for a curve-typed field, a sparkline of its own points.
fn row_line(ui: &mut egui::Ui, row: &Row, w: f32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&row.key)
                .size(9.5)
                .color(t::MUTED)
                .family(t::family_mono_med()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !row.unit.is_empty() && row.unit != "-" {
                ui.label(
                    RichText::new(&row.unit)
                        .size(8.5)
                        .color(t::MUTED)
                        .family(t::family_mono_med()),
                );
            }
            ui.label(
                RichText::new(&row.value)
                    .size(10.5)
                    .color(if row.source == "engine" {
                        t::PRIMARY
                    } else {
                        t::INK
                    })
                    .family(t::family_mono_med()),
            );
        });
    });
    if let Some(pts) = &row.curve {
        sparkline(ui, pts, w, 22.0);
    }
}

/// A tiny polyline of a curve field's recorded points (x to the right, y up), auto-scaled, with the ends
/// marked so the reader can see which way the curve runs.
pub fn sparkline(ui: &mut egui::Ui, pts: &[(f64, f64)], w: f32, h: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w.max(60.0), h), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, egui::CornerRadius::same(2), t::with_alpha(t::BG, 220));
    if pts.len() < 2 {
        return;
    }
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    let (x0, x1) = (
        xs.iter().cloned().fold(f64::INFINITY, f64::min),
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    let (y0, y1) = (
        ys.iter().cloned().fold(f64::INFINITY, f64::min),
        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    let (dx, dy) = ((x1 - x0).max(1e-12), (y1 - y0).max(1e-12));
    let inner = rect.shrink(3.0);
    let mapped: Vec<egui::Pos2> = pts
        .iter()
        .map(|q| {
            let fx = ((q.0 - x0) / dx) as f32;
            let fy = ((q.1 - y0) / dy) as f32;
            egui::pos2(
                inner.left() + fx * inner.width(),
                inner.bottom() - fy * inner.height(),
            )
        })
        .collect();
    p.add(egui::Shape::line(
        mapped.clone(),
        Stroke::new(1.2, t::PRIMARY),
    ));
    if let Some(a) = mapped.first() {
        p.circle_filled(*a, 1.8, t::PRIMARY);
    }
    if let Some(b) = mapped.last() {
        p.circle_filled(*b, 1.8, t::with_alpha(t::PRIMARY, 140));
    }
    p.text(
        egui::pos2(rect.right() - 3.0, rect.top() + 1.0),
        egui::Align2::RIGHT_TOP,
        format!("{} pts", pts.len()),
        egui::FontId::new(8.0, t::family_semi()),
        t::MUTED,
    );
}
