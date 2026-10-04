//! **Crossflow** (#84): how the section looks for a crossflow tower.
//!
//! Engine: `crossflow::solve_crossflow_grid` on the best crossflow candidate the Size screen's selection run
//! found at this duty (its dry-air flow and available KaV/L). The grid is the engine's own cell field: water
//! temperature falling top to bottom, air enthalpy rising left to right. The section paints one fill pack per
//! side (the engine solves one; the tower is symmetric, so the left pack is its mirror), the plenum between
//! them and the fan stack above. Air dots cross the fill at a speed set by the candidate's airflow; water
//! drops fall at a speed set by the circulating flow.

use bevy_egui::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
};
use cockpit::engine::EngineInput;

use drafthouse_cockpit_seams::mapping as m;

use super::kit::{self, text};
use super::{data::XfGrid, info_card, title_band, toggle_info, Env, State};
use crate::theme as t;

// ================================================================================== the geometry

/// The drawing's unit sizes (issue #84): the two crossing fill packs, the plenum between them, the
/// deck (the hot-water distribution basin) over each pack, the casing's cold-water basin and the
/// fan stack. Every one of them is drawn as `units * Sect::s`, so the section has ONE scale on both
/// axes at every size - the Phase-1 proportion promise, checked by the tests below.
pub const PACK_W_U: f32 = 250.0;
pub const PACK_H_U: f32 = 370.0;
pub const PLENUM_W_U: f32 = 150.0;
pub const DECK_H_U: f32 = 22.0;
pub const BASIN_H_U: f32 = 40.0;
pub const STACK_H_U: f32 = 46.0;
/// The desktop panel's width.
const PANEL_W: f32 = 320.0;
/// The phone's panel height: the four metrics, the numerics block (the mesh, the discretisation
/// error and the 12/24/48 study) and the caption must fit inside it; the rest is the stage.
const PHONE_PANEL_H: f32 = 288.0;

/// The section's rectangles, computed once per frame from the rect the shell hands the screen. The
/// painter draws exactly these and the geometry tests check exactly these, so neither can drift.
#[derive(Clone, Copy, Debug)]
pub struct Sect {
    /// The drawing's area (the panel takes its own strip out of the body).
    pub stage: Rect,
    /// The desktop's right-hand panel, or the phone's bottom one.
    pub panel: Rect,
    /// The drawing's one scale: points per unit, on BOTH axes.
    pub s: f32,
    /// The section's centre, 30 units down so the stack above it has room.
    pub c: Pos2,
    pub pack_l: Rect,
    pub pack_r: Rect,
    pub plenum: Rect,
    pub deck_h: f32,
    pub casing: Rect,
    pub basin: Rect,
    pub stack: Rect,
    /// True when the cold-water label fits inside the basin; else it sits under the casing and the
    /// legend moves down with it.
    pub cold_in: bool,
    pub legend: Rect,
}

/// The section's geometry at this body rect: the one place `s`, the pack boxes, the casing, the
/// basin, the stack and the legend are derived.
pub fn layout(body: Rect, phone: bool) -> Sect {
    let (stage, panel) = if phone {
        let cut = pos2(body.left(), body.bottom() - PHONE_PANEL_H);
        (
            Rect::from_min_max(body.min, pos2(body.right(), cut.y)),
            Rect::from_min_max(cut, body.max),
        )
    } else {
        let cut = body.right() - PANEL_W;
        (
            Rect::from_min_max(body.min, pos2(cut, body.bottom())),
            Rect::from_min_max(pos2(cut, body.top()), body.max),
        )
    };
    let s = (stage.width() / 900.0)
        .min(stage.height() / 700.0)
        .clamp(0.36, 1.3);
    let c = pos2(stage.center().x, stage.center().y + 30.0 * s);
    let pack_w = PACK_W_U * s;
    // phone: the stage is tall and narrow (s is set by the width), so the packs take the height -
    // the one dimension that is not `unit * s`, clamped, and asserted as such in the tests
    let pack_h = if phone {
        (stage.height() - 200.0).clamp(150.0, 320.0)
    } else {
        PACK_H_U * s
    };
    let plenum_w = PLENUM_W_U * s;
    let top = c.y - pack_h / 2.0;
    let pack_r = Rect::from_min_size(pos2(c.x + plenum_w / 2.0, top), vec2(pack_w, pack_h));
    let pack_l = Rect::from_min_size(
        pos2(c.x - plenum_w / 2.0 - pack_w, top),
        vec2(pack_w, pack_h),
    );
    let deck_h = DECK_H_U * s;
    let casing = Rect::from_min_max(
        pos2(pack_l.left() - 6.0, top - deck_h - 8.0),
        pos2(pack_r.right() + 6.0, pack_r.bottom() + BASIN_H_U * s),
    );
    let plenum = Rect::from_min_max(
        pos2(pack_l.right(), top),
        pos2(pack_r.left(), pack_r.bottom()),
    );
    let stack = Rect::from_min_max(
        pos2(c.x - plenum_w * 0.6, casing.top() - STACK_H_U * s),
        pos2(c.x + plenum_w * 0.6, casing.top() + 1.0),
    );
    let basin = Rect::from_min_max(
        pos2(casing.left() + 4.0, pack_r.bottom() + 4.0),
        pos2(casing.right() - 4.0, casing.bottom() - 4.0),
    );
    let cold_in = basin.height() >= 24.0;
    let legend = Rect::from_center_size(
        pos2(c.x, casing.bottom() + if cold_in { 16.0 } else { 40.0 }),
        vec2(pack_w * 1.2, 8.0),
    );
    Sect {
        stage,
        panel,
        s,
        c,
        pack_l,
        pack_r,
        plenum,
        deck_h,
        casing,
        basin,
        stack,
        cold_in,
        legend,
    }
}

/// One label the section paints: its text, where it goes, and whether it wears a plate. The painter
/// draws `rect` and the collision test checks `rect` - the same numbers, from the same source.
#[derive(Clone, Debug)]
pub struct Label {
    pub key: &'static str,
    pub text: String,
    pub rect: Rect,
    pub font: FontId,
    pub color: Color32,
    pub plate: bool,
}

/// The section's own labels, laid out from the same [`Sect`] the painter draws and measured with the
/// caller's font source ([`kit::measure`] when painting; a real `egui::Context`'s fonts in the
/// tests), so a test can check the rects the user sees. Six labels: "air in" on both outer faces,
/// "air out" above the stack, a hot-water label over each pack's deck and the cold-water label (in
/// the basin, or under the casing when the basin is too shallow for it).
pub fn labels(
    sec: &Sect,
    grid: &XfGrid,
    phone: bool,
    measure: &dyn Fn(&str, FontId) -> Vec2,
) -> Vec<Label> {
    let mut out: Vec<Label> = Vec::new();
    let mut place = |key: &'static str,
                     text: String,
                     at: Pos2,
                     align: Align2,
                     font: FontId,
                     color: Color32,
                     plate: bool| {
        let size = measure(&text, font.clone()) + if plate { vec2(8.0, 4.0) } else { Vec2::ZERO };
        out.push(Label {
            key,
            text,
            rect: align.anchor_size(at, size),
            font,
            color,
            plate,
        });
    };
    // the corners first, in the order the drawing has always painted them (right, then left)
    if phone {
        place(
            "air in · right",
            "air in".into(),
            pos2(sec.stage.right() - 14.0, sec.casing.top() - 8.0),
            Align2::RIGHT_BOTTOM,
            kit::semi(11.5),
            t::AIR,
            false,
        );
        place(
            "air in · left",
            "air in".into(),
            pos2(sec.stage.left() + 14.0, sec.casing.top() - 8.0),
            Align2::LEFT_BOTTOM,
            kit::semi(11.5),
            t::AIR,
            false,
        );
    } else {
        place(
            "air in · right",
            "air in".into(),
            pos2(sec.pack_r.right() + 24.0 * sec.s, sec.pack_r.top() - 6.0),
            Align2::LEFT_BOTTOM,
            kit::semi(11.0),
            t::AIR,
            false,
        );
        place(
            "air in · left",
            "air in".into(),
            pos2(sec.pack_l.left() - 24.0 * sec.s, sec.pack_l.top() - 6.0),
            Align2::RIGHT_BOTTOM,
            kit::semi(11.0),
            t::AIR,
            false,
        );
    }
    place(
        "air out",
        format!("air out {:.1} °C", grid.outlet_db),
        if phone {
            pos2(sec.c.x, sec.stack.top() - 44.0 * sec.s - 6.0)
        } else {
            pos2(sec.stack.right() + 14.0, sec.stack.top() - 10.0 * sec.s)
        },
        if phone {
            Align2::CENTER_BOTTOM
        } else {
            Align2::LEFT_BOTTOM
        },
        kit::num(12.0),
        t::INK_2,
        true,
    );
    for (key, pk) in [("hot · left", sec.pack_l), ("hot · right", sec.pack_r)] {
        place(
            key,
            format!("hot {:.1} °C", grid.hot_c),
            pos2(pk.center().x, sec.casing.top() - 6.0),
            Align2::CENTER_BOTTOM,
            kit::num(12.0),
            kit::water_temp(0.0),
            true,
        );
    }
    let cold_at = if sec.cold_in {
        sec.basin.center()
    } else {
        pos2(sec.c.x, sec.casing.bottom() + 14.0)
    };
    place(
        "cold",
        format!("cold {:.2} °C", grid.cold_c),
        cold_at,
        Align2::CENTER_CENTER,
        kit::num(13.0),
        t::INK,
        true,
    );
    out
}

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Crossflow",
        "air crosses the falling water",
        env.phone,
    );
    let p = ui.painter().clone();
    let grid = match st.cache.xf.as_ref() {
        None => {
            let (done, n) = st.cache.size_progress();
            text(
                &p,
                body.center(),
                Align2::CENTER_CENTER,
                &format!("running the selection {done}/{n}"),
                kit::sans(13.0),
                t::MUTED,
            );
            return;
        }
        Some(Err(e)) => {
            text(
                &p,
                body.center(),
                Align2::CENTER_CENTER,
                e,
                kit::sans(13.0),
                t::DANGER,
            );
            return;
        }
        Some(Ok(g)) => g.clone(),
    };
    let _ = draft;
    let show_air = st.info.as_deref() == Some("xf:air");

    // the geometry comes from `layout` - the painter and the geometry tests read the same rects
    let sec = layout(body, env.phone);
    let (stage, panel) = (sec.stage, sec.panel);
    kit::ground(
        &p,
        stage,
        stage.center(),
        stage.width().min(stage.height()) * 0.55,
        t::with_alpha(t::PRIMARY, 16),
    );
    let s = sec.s;
    let c = sec.c;
    let pack_h = sec.pack_r.height();
    let top = sec.pack_r.top();
    let right_pack = sec.pack_r;
    let left_pack = sec.pack_l;
    let deck_h = sec.deck_h;
    let casing = sec.casing;
    p.rect_filled(casing, kit::r(6), t::with_alpha(t::PANEL, 140));
    p.rect_stroke(
        casing,
        kit::r(6),
        Stroke::new(1.0, t::LINE),
        StrokeKind::Inside,
    );

    // hot-water distribution decks (the crossflow tower's open basins over each pack)
    for pk in [left_pack, right_pack] {
        let deck = Rect::from_min_size(
            pos2(pk.left(), top - deck_h - 2.0),
            vec2(pk.width(), deck_h),
        );
        kit::vgrad(
            &p,
            deck,
            t::with_alpha(kit::water_temp(0.0), 170),
            t::with_alpha(kit::water_temp(0.05), 120),
        );
        p.rect_stroke(
            deck,
            kit::r(2),
            Stroke::new(1.0, t::LINE),
            StrokeKind::Inside,
        );
    }
    // cold-water basin under everything
    let basin = sec.basin;
    kit::vgrad(
        &p,
        basin,
        t::with_alpha(kit::water_temp(1.0), 150),
        t::with_alpha(t::WATER, 60),
    );

    // ---- the engine's grid on both packs
    let ny = grid.water.len().saturating_sub(1).max(1);
    let nx = grid.water.first().map(|r| r.len()).unwrap_or(1).max(1);
    let (lo_t, hi_t) = (grid.cold_c.min(grid.wb_c + 0.5) - 0.5, grid.hot_c);
    let air_lo = grid
        .air_h
        .iter()
        .filter_map(|r| r.first().copied())
        .fold(f64::INFINITY, f64::min);
    let air_hi = grid
        .air_h
        .iter()
        .filter_map(|r| r.last().copied())
        .fold(f64::NEG_INFINITY, f64::max);
    let cell_col = |y: usize, x: usize| -> Color32 {
        if show_air {
            let h = grid
                .air_h
                .get(y)
                .and_then(|r| r.get(x + 1))
                .copied()
                .unwrap_or(air_lo);
            let f = ((h - air_lo) / (air_hi - air_lo).max(1e-6)) as f32;
            lerp(
                t::with_alpha(t::AIR, 40),
                t::with_alpha(Color32::from_rgb(0xf0, 0xd8, 0xa8), 210),
                f,
            )
        } else {
            let w = grid
                .water
                .get(y + 1)
                .and_then(|r| r.get(x))
                .copied()
                .unwrap_or(lo_t);
            let f = 1.0 - ((w - lo_t) / (hi_t - lo_t).max(1e-6)) as f32;
            t::with_alpha(kit::water_temp(f.clamp(0.0, 1.0)), 200)
        }
    };
    for (pk, mirror) in [(right_pack, false), (left_pack, true)] {
        let cw = pk.width() / nx as f32;
        let ch = pk.height() / ny as f32;
        for y in 0..ny {
            for x in 0..nx {
                // the engine's x = 0 is the air inlet face: the outer face of each pack
                let gx = if mirror { nx - 1 - x } else { x };
                let r = Rect::from_min_size(
                    pos2(pk.left() + cw * gx as f32, pk.top() + ch * y as f32),
                    vec2(cw + 0.5, ch + 0.5),
                );
                p.rect_filled(r, kit::r(0), cell_col(y, x));
            }
        }
        // fill sheets
        for i in 1..12 {
            let x = pk.left() + pk.width() * i as f32 / 12.0;
            p.line_segment(
                [pos2(x, pk.top()), pos2(x, pk.bottom())],
                Stroke::new(0.6, t::with_alpha(t::BG, 90)),
            );
        }
        p.rect_stroke(pk, kit::r(0), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
        // inlet louvres on the outer face
        let face = if mirror {
            pk.left() - 14.0 * s
        } else {
            pk.right() + 4.0 * s
        };
        for i in 0..10 {
            let y = pk.top() + pk.height() * (i as f32 + 0.5) / 10.0;
            let (a, b) = if mirror {
                (pos2(face, y - 6.0 * s), pos2(face + 10.0 * s, y + 2.0 * s))
            } else {
                (pos2(face + 10.0 * s, y - 6.0 * s), pos2(face, y + 2.0 * s))
            };
            p.line_segment([a, b], Stroke::new(1.6, t::with_alpha(t::AIR, 150)));
        }
    }
    // plenum + fan stack
    let plenum = sec.plenum;
    p.rect_filled(plenum, kit::r(0), t::with_alpha(t::BG, 160));
    let stack = sec.stack;
    let tt = env.t;
    // ---- motion: air across the fill (both sides inward), up the plenum, out of the stack
    // The speed is the engine's airflow through the seams' rate (issue #86 AC 1), not a number of the canvas.
    let air_speed = m::streamline_speed_px_s(grid.cand.airflow_m3_s) * s;
    let rows = 6;
    for i in 0..rows {
        let y = top + pack_h * (i as f32 + 0.5) / rows as f32;
        let h_in = grid
            .air_h
            .get(((i as f32 + 0.5) / rows as f32 * ny as f32) as usize)
            .and_then(|r| r.first().copied())
            .unwrap_or(air_lo);
        let _ = h_in;
        for mirror in [false, true] {
            let sgn = if mirror { -1.0 } else { 1.0 };
            let x0 = if mirror {
                left_pack.left() - 40.0 * s
            } else {
                right_pack.right() + 40.0 * s
            };
            // nested L-paths: lower rows run further in before turning up, so the streams fill the
            // plenum and rise into the throat without crossing
            let f = i as f32 / rows as f32;
            let x1 = c.x + sgn * plenum.width() * 0.5 * (0.92 - 0.8 * f);
            let throat = c.x + sgn * stack.width() * 0.34 * (0.95 - 0.8 * f);
            let path = [
                pos2(x0, y),
                pos2(x1, y),
                pos2(x1, plenum.top() + 24.0 * s),
                pos2(throat, stack.bottom() - 4.0),
                // stops below the fan, so the fan stays legible
                pos2(throat, stack.top() + 16.0 * s),
            ];
            kit::flow_ticks(
                &p,
                &path,
                tt * air_speed + i as f32 * 7.0,
                26.0 * s.max(0.5),
                7.0 * s.max(0.6),
                t::with_alpha(Color32::from_rgb(0xe8, 0xee, 0xf2), 190),
            );
        }
    }
    // water drops falling through each pack, at the rate the engine's water loading sets (issue #86 AC 1)
    let water_speed = m::droplet_speed_px_s(grid.water_loading_kg_m2_s) * s;
    for pk in [left_pack, right_pack] {
        for i in 0..8 {
            let x = pk.left() + pk.width() * (i as f32 + 0.5) / 8.0;
            let path = [pos2(x, pk.top() + 2.0), pos2(x, pk.bottom() - 2.0)];
            kit::flow_dots(
                &p,
                &path,
                tt * water_speed + i as f32 * 11.0,
                26.0 * s.max(0.6),
                1.7 * s.max(0.7),
                t::with_alpha(t::WATER, 210),
            );
        }
    }

    p.add(Shape::convex_polygon(
        vec![
            pos2(stack.left() + 14.0 * s, stack.top()),
            pos2(stack.right() - 14.0 * s, stack.top()),
            pos2(stack.right(), stack.bottom()),
            pos2(stack.left(), stack.bottom()),
        ],
        t::PANEL_RAISED,
        Stroke::new(1.2, t::LINE),
    ));
    // the fan, seen from the side: a ring and six blades turning in its plane
    let fan_c = pos2(stack.center().x, stack.top() + 6.0 * s);
    let fr = stack.width() * 0.40;
    let ring: Vec<Pos2> = (0..=40)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 40.0;
            fan_c + vec2(a.cos() * fr, a.sin() * 5.0 * s)
        })
        .collect();
    p.add(Shape::line(
        ring,
        Stroke::new(1.2, t::with_alpha(t::INK_2, 160)),
    ));
    for k in 0..6 {
        // the blades turn at the candidate fan's own rpm (issue #86): the seams map the record's rated
        // speed and this run's ratio to turns, so the wheel's speed is the engine's number, not a look.
        let a = tt * std::f32::consts::TAU * m::blade_turn_hz(grid.rpm.unwrap_or(0.0))
            + k as f32 * std::f32::consts::TAU / 6.0;
        let tip = fan_c + vec2(a.cos() * fr * 0.94, a.sin() * 4.6 * s);
        let front = a.sin() > 0.0;
        p.line_segment(
            [fan_c, tip],
            Stroke::new(
                if front { 3.0 } else { 2.0 },
                t::with_alpha(t::AIR, if front { 230 } else { 110 }),
            ),
        );
    }
    p.circle_filled(fan_c, 3.0 * s.max(0.8), t::INK);

    // ---- tap a cell: its engine values
    let resp = ui.interact(
        Rect::from_min_max(left_pack.min, right_pack.max),
        egui::Id::new("xf.grid"),
        egui::Sense::click(),
    );
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked()) {
        let pk = if pos.x >= right_pack.left() {
            Some((right_pack, false))
        } else if pos.x <= left_pack.right() {
            Some((left_pack, true))
        } else {
            None
        };
        if let Some((pk, mirror)) = pk {
            let gx = (((pos.x - pk.left()) / pk.width()) * nx as f32)
                .floor()
                .clamp(0.0, (nx - 1) as f32) as usize;
            let x = if mirror { nx - 1 - gx } else { gx };
            let y = (((pos.y - pk.top()) / pk.height()) * ny as f32)
                .floor()
                .clamp(0.0, (ny - 1) as f32) as usize;
            st.info = Some(format!("xf:cell:{x}:{y}"));
        }
    }
    if let Some((x, y)) = st
        .info
        .as_deref()
        .and_then(|s| s.strip_prefix("xf:cell:"))
        .and_then(|s| {
            let (a, b) = s.split_once(':')?;
            Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
        })
    {
        let cw = right_pack.width() / nx as f32;
        let ch = right_pack.height() / ny as f32;
        let r = Rect::from_min_size(
            pos2(
                right_pack.left() + cw * x as f32,
                right_pack.top() + ch * y as f32,
            ),
            vec2(cw, ch),
        );
        p.rect_stroke(
            r.expand(1.0),
            kit::r(0),
            Stroke::new(2.0, t::INK),
            StrokeKind::Outside,
        );
        let wt = grid
            .water
            .get(y + 1)
            .and_then(|r| r.get(x))
            .copied()
            .unwrap_or(f64::NAN);
        let ah = grid
            .air_h
            .get(y)
            .and_then(|r| r.get(x + 1))
            .copied()
            .unwrap_or(f64::NAN);
        let card = Rect::from_min_size(
            pos2((r.right() + 10.0).min(stage.right() - 170.0), r.top() - 8.0),
            vec2(160.0, 58.0),
        );
        kit::glass(&p, card, 8);
        text(
            &p,
            pos2(card.left() + 10.0, card.top() + 8.0),
            Align2::LEFT_TOP,
            &format!("{wt:.2} °C water"),
            kit::num(13.0),
            t::INK,
        );
        text(
            &p,
            pos2(card.left() + 10.0, card.top() + 30.0),
            Align2::LEFT_TOP,
            &format!("{ah:.1} kJ/kg air"),
            kit::num(13.0),
            t::INK_2,
        );
    }

    // ---- labels on the section, short: the rects `labels` measured, painted where they land
    let labels = labels(&sec, &grid, env.phone, &|s, f| kit::measure(&p, s, f));
    for l in &labels {
        if l.plate {
            kit::label_plate(
                &p,
                l.rect.center(),
                Align2::CENTER_CENTER,
                &l.text,
                l.font.clone(),
                l.color,
            );
        } else {
            text(
                &p,
                l.rect.center(),
                Align2::CENTER_CENTER,
                &l.text,
                l.font.clone(),
                l.color,
            );
        }
    }
    // the labels state the direction with a drawn arrow, since the strings carry none: each one sits
    // between its label and the pack and points INTO the pack (dir = +1 on the left, -1 on the right)
    for (x, dir) in [
        (left_pack.left() - 24.0 * s, 1.0f32),
        (right_pack.right() + 24.0 * s, -1.0f32),
    ] {
        if !env.phone {
            let y = top - 3.0;
            kit::arrow_to(
                &p,
                pos2(x + dir * 4.0, y),
                pos2(x + dir * 20.0, y),
                1.6,
                t::AIR,
            );
        }
    }
    if !env.phone {
        // out of the stack, up: beside its own "air out" label, not across the stack from it
        kit::arrow_to(
            &p,
            pos2(stack.right() + 6.0, stack.top() + 8.0 * s),
            pos2(stack.right() + 6.0, stack.top() - 22.0 * s),
            1.6,
            t::AIR,
        );
    }

    // the legend bar under the left pack
    let lg = sec.legend;
    if show_air {
        kit::hgrad(
            &p,
            lg,
            t::with_alpha(t::AIR, 60),
            Color32::from_rgb(0xf0, 0xd8, 0xa8),
        );
        text(
            &p,
            pos2(lg.left() - 8.0, lg.center().y),
            Align2::RIGHT_CENTER,
            &format!("{air_lo:.0}"),
            kit::mono(11.0),
            t::INK_2,
        );
        text(
            &p,
            pos2(lg.right() + 8.0, lg.center().y),
            Align2::LEFT_CENTER,
            &format!("{air_hi:.0} kJ/kg air"),
            kit::mono(11.0),
            t::INK_2,
        );
    } else {
        kit::hgrad(&p, lg, kit::water_temp(1.0), kit::water_temp(0.0));
        text(
            &p,
            pos2(lg.left() - 8.0, lg.center().y),
            Align2::RIGHT_CENTER,
            &format!("{lo_t:.0}"),
            kit::mono(11.0),
            t::INK_2,
        );
        text(
            &p,
            pos2(lg.right() + 8.0, lg.center().y),
            Align2::LEFT_CENTER,
            &format!("{hi_t:.0} °C water"),
            kit::mono(11.0),
            t::INK_2,
        );
    }

    // ---- the panel
    let pp = panel.shrink2(vec2(if env.phone { 12.0 } else { 18.0 }, 12.0));
    kit::glass(&p, pp, 12);
    let inner = pp.shrink(16.0);
    let seg = Rect::from_min_size(inner.min, vec2(inner.width() - 40.0, 40.0));
    if let Some(i) = kit::segmented(
        ui,
        seg,
        "xf.layer",
        &["Water °C", "Air enthalpy"],
        show_air as usize,
        12.0,
    ) {
        st.info = if i == 1 { Some("xf:air".into()) } else { None };
    }
    if kit::info_dot(
        ui,
        pos2(inner.right() - 10.0, seg.center().y),
        "xf.i",
        st.info.as_deref() == Some("xf:i"),
    )
    .clicked()
    {
        toggle_info(st, "xf:i");
    }
    let mut y = seg.bottom() + 16.0;
    let rows: [(&str, String, &str); 4] = [
        ("cold water", format!("{:.2}", grid.cold_c), "°C"),
        ("approach", format!("{:.2}", grid.cold_c - grid.wb_c), "K"),
        ("air out", format!("{:.1}", grid.outlet_db), "°C"),
        ("fan power", format!("{:.1}", grid.cand.power_kw), "kW"),
    ];
    let cols = 2;
    let cw = inner.width() / cols as f32;
    for (i, (l, v, u)) in rows.iter().enumerate() {
        kit::metric(
            &p,
            pos2(
                inner.left() + cw * (i % cols) as f32,
                y + 48.0 * (i / cols) as f32,
            ),
            l,
            v,
            u,
            if env.phone { 16.0 } else { 20.0 },
            if i == 0 { t::INK } else { t::INK_2 },
        );
    }
    y += 48.0 * 2.0 + 6.0;
    // ---- the solver's numerics (issue #84 AC 2): the mesh it solved, the engine's discretisation
    // error estimate and the 12/24/48 study - on the surface with the result, not only in a test.
    let err = match grid.error_c {
        Some(e) => format!("±{e:.3} K error"),
        None => "error estimate off".to_string(),
    };
    let mesh = match grid.fine_cells {
        Some(f) => format!(
            "mesh {}×{} → {}×{} · {err}",
            grid.cells[0], grid.cells[1], f[0], f[1]
        ),
        None => format!("mesh {}×{} · {err}", grid.cells[0], grid.cells[1]),
    };
    let st_ = &grid.study;
    let study = format!(
        "study {}→{}→{} · order {}",
        st_.cells[0],
        st_.cells[1],
        st_.cells[2],
        match st_.order {
            Some(o) => format!("{o:.2}"),
            None => "-".into(),
        }
    );
    let gci = format!(
        "GCI {:.2} % · extrapolated {:.2} °C",
        st_.gci_pct, st_.extrapolated
    );
    for line in [&mesh, &study, &gci] {
        kit::text_fit(
            &p,
            pos2(inner.left(), y),
            Align2::LEFT_TOP,
            line,
            kit::mono(10.5),
            t::INK_2,
            inner.width(),
        );
        y += 15.0;
    }
    y += 5.0;
    if !env.phone {
        p.line_segment(
            [pos2(inner.left(), y), pos2(inner.right(), y)],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        y += 14.0;
        text(
            &p,
            pos2(inner.left(), y),
            Align2::LEFT_TOP,
            &grid.cand.tower_id,
            kit::semi(14.0),
            t::INK,
        );
        text(
            &p,
            pos2(inner.left(), y + 22.0),
            Align2::LEFT_TOP,
            &format!(
                "{} {:.2} m · {} @ {:.2}",
                grid.cand.fill_id, grid.cand.depth_m, grid.cand.fan_id, grid.cand.speed
            ),
            kit::sans(11.5),
            t::MUTED,
        );
        text(
            &p,
            pos2(inner.left(), y + 44.0),
            Align2::LEFT_TOP,
            "best crossflow fit from Size",
            kit::sans(11.5),
            t::MUTED,
        );
        y += 78.0;
        // the grid's two edges, as profiles: water leaving the pack bottom (outer face to plenum), and
        // air leaving the inner face (top to bottom). Both are the engine's last row / column.
        let prof_h = ((inner.bottom() - 30.0 - y) / 2.0 - 40.0).clamp(40.0, 170.0);
        if let Some(bottom) = grid.water.last() {
            let r = Rect::from_min_size(pos2(inner.left(), y + 20.0), vec2(inner.width(), prof_h));
            text(
                &p,
                pos2(r.left(), y),
                Align2::LEFT_TOP,
                "water leaving the pack",
                kit::semi(11.5),
                t::INK_2,
            );
            text(
                &p,
                pos2(r.right(), y),
                Align2::RIGHT_TOP,
                "°C",
                kit::sans(10.5),
                t::MUTED,
            );
            bars(
                &p,
                r,
                bottom,
                |v| {
                    t::with_alpha(
                        kit::water_temp(
                            (1.0 - ((v - lo_t) / (hi_t - lo_t).max(1e-6)) as f32).clamp(0.0, 1.0),
                        ),
                        220,
                    )
                },
                "air in",
                "plenum",
            );
            y = r.bottom() + 30.0;
        }
        let inner_face: Vec<f64> = grid
            .air_h
            .iter()
            .filter_map(|r| r.last().copied())
            .collect();
        if !inner_face.is_empty() && y + prof_h + 20.0 < inner.bottom() - 24.0 {
            let r = Rect::from_min_size(pos2(inner.left(), y + 20.0), vec2(inner.width(), prof_h));
            text(
                &p,
                pos2(r.left(), y),
                Align2::LEFT_TOP,
                "air leaving into the plenum",
                kit::semi(11.5),
                t::INK_2,
            );
            text(
                &p,
                pos2(r.right(), y),
                Align2::RIGHT_TOP,
                "kJ/kg",
                kit::sans(10.5),
                t::MUTED,
            );
            bars(
                &p,
                r,
                &inner_face,
                |v| {
                    lerp(
                        t::with_alpha(t::AIR, 90),
                        Color32::from_rgb(0xf0, 0xd8, 0xa8),
                        ((v - air_lo) / (air_hi - air_lo).max(1e-6)) as f32,
                    )
                },
                "top",
                "bottom",
            );
        }
        kit::text_fit(
            &p,
            pos2(inner.left(), inner.bottom()),
            Align2::LEFT_BOTTOM,
            &format!(
                "engine · crossflow.rs · {}×{} · {}",
                grid.cells[0],
                grid.cells[1],
                if grid.ms < 1.0 {
                    "<1 ms".to_string()
                } else {
                    format!("{:.0} ms", grid.ms)
                }
            ),
            kit::mono(10.5),
            t::MUTED,
            inner.width(),
        );
    } else {
        text(
            &p,
            pos2(inner.left(), y),
            Align2::LEFT_TOP,
            &format!(
                "{} · {} {:.2} m · tap a cell",
                grid.cand.tower_id, grid.cand.fill_id, grid.cand.depth_m
            ),
            kit::sans(11.0),
            t::MUTED,
        );
    }
    info_card(
        ui,
        st,
        "xf:i",
        pos2(inner.right() - 10.0, seg.center().y),
        &[
            "Crossflow section",
            "Water falls; air crosses it sideways.",
            "Each cell is one engine grid cell: hot",
            "along the top, coldest where the air enters.",
            "Tap a cell for its values.",
            "Numerics: the mesh is doubled and the",
            "cold water is Richardson-extrapolated;",
            "the error line bounds that fine mesh, not",
            "the reported value. The 12/24/48 study",
            "confirms the scheme's first order.",
        ],
        area,
    );
    if env.motion {
        ui.ctx().request_repaint();
    }
}

fn lerp(a: Color32, b: Color32, f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_array();
    let [br, bg, bb, ba] = b.to_array();
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) as u8;
    Color32::from_rgba_premultiplied(m(ar, br), m(ag, bg), m(ab, bb), m(aa, ba))
}

#[allow(dead_code)]
fn _p(_: Pos2) {}

/// A strip of bars, one per value; height over the strip's own spread (floor 15%) so the shape shows,
/// colour from the value. The end labels name the axis and carry the end values.
fn bars(p: &egui::Painter, r: Rect, vals: &[f64], col: impl Fn(f64) -> Color32, a: &str, b: &str) {
    let n = vals.len().max(1);
    let bw = r.width() / n as f32;
    let (vlo, vhi) = vals
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |m, v| {
            (m.0.min(*v), m.1.max(*v))
        });
    for (i, v) in vals.iter().enumerate() {
        let f = 0.15 + 0.85 * ((v - vlo) / (vhi - vlo).max(1e-6)) as f32;
        let br = Rect::from_min_max(
            pos2(r.left() + bw * i as f32 + 1.0, r.bottom() - r.height() * f),
            pos2(r.left() + bw * (i + 1) as f32 - 1.0, r.bottom()),
        );
        p.rect_filled(br, kit::r(1), col(*v));
    }
    let first = vals.first().copied().unwrap_or(0.0);
    let last = vals.last().copied().unwrap_or(0.0);
    text(
        p,
        pos2(r.left(), r.bottom() + 4.0),
        Align2::LEFT_TOP,
        &format!("{a} · {first:.1}"),
        kit::mono(10.5),
        t::INK_2,
    );
    text(
        p,
        pos2(r.right(), r.bottom() + 4.0),
        Align2::RIGHT_TOP,
        &format!("{last:.1} · {b}"),
        kit::mono(10.5),
        t::INK_2,
    );
}

// --------------------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    //! Issue #84, AC 3: the section drawing's own Phase-1 promises, asserted on the same rects the
    //! painter draws (`layout`/`labels`, one source for both). At the four sizes issue #81 names:
    //! the drawing has **one scale on both axes** (every drawn dimension is its unit count times
    //! `Sect::s`; the phone's pack height is the design's one documented adaptation, asserted as
    //! such), **no label overlaps another** (the same >0.5 px rule as the Phase-1 frames check,
    //! `docs/design/small-screens-r1/tools/check.mjs`), **every label sits inside the stage**, and
    //! the **stage keeps the section minimum**.
    use super::*;

    /// The rect the shell hands `ui` at the four sizes: its rail layout, less the answer strip and
    /// (desktop) the title band - 1280x720 -> 1204x604, 1440x900 -> 1364x784, 1024x768 -> 948x652,
    /// 390x844 phone -> 390x742. Only the size reaches `layout`.
    const TARGETS: [(&str, f32, f32, bool); 4] = [
        ("1280x720", 1204.0, 604.0, false),
        ("1440x900", 1364.0, 784.0, false),
        ("1024x768", 948.0, 652.0, false),
        ("390x844", 390.0, 742.0, true),
    ];

    /// Issue #81's agreed section minimum, the frames' `data-min-section-h`.
    const MIN_SECTION_H: f32 = 230.0;

    fn area(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
    }

    /// The section's labels carry the fixture's own duty (hot 42 / wb 27) and a cold water between
    /// them: representative of the longest strings the screen prints at these fonts.
    fn fixture_grid() -> XfGrid {
        XfGrid {
            hot_c: 42.0,
            wb_c: 27.0,
            cold_c: 31.29,
            outlet_db: 36.5,
            ..XfGrid::default()
        }
    }

    /// A painter over a real font context carrying the app's own fonts (`ui.rs` installs the same
    /// [`t::fonts`]; a pass activates them). Measuring through it is measuring what the user sees.
    fn font_painter() -> egui::Painter {
        let ctx = egui::Context::default();
        ctx.set_fonts(t::fonts());
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        // the pass's texture deltas belong to a renderer; there is none here
        out.textures_delta.clear();
        egui::Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING)
    }

    fn rel(a: f32, b: f32) -> f32 {
        (a - b).abs() / b.abs().max(1e-6)
    }

    /// Issue #81, criterion 1, applied to the crossflow drawing: **one scale, both axes**. Every
    /// drawn dimension is its unit count times the drawing's one `s` within 5 %; on the desktop the
    /// pack's own px-per-unit in x and y agree (the criterion verbatim); the phone's pack height is
    /// the design round's documented adaptation, taking the stage's spare height inside its clamp.
    #[test]
    fn one_scale_for_both_axes_at_the_four_target_sizes() {
        for (label, w, h, phone) in TARGETS {
            let sec = layout(area(w, h), phone);
            let s = sec.s;
            for (what, points, units) in [
                ("the pack width", sec.pack_r.width(), PACK_W_U),
                ("the plenum width", sec.plenum.width(), PLENUM_W_U),
                ("the deck height", sec.deck_h, DECK_H_U),
                // the basin band is the casing's own lower extension (the drawn basin is inset 4 px)
                (
                    "the casing's basin band",
                    sec.casing.bottom() - sec.pack_r.bottom(),
                    BASIN_H_U,
                ),
                ("the stack height", sec.stack.height() - 1.0, STACK_H_U),
            ] {
                assert!(
                    rel(points, units * s) <= 0.05,
                    "{label}: {what} is {points:.1} px, not {:.1} (unit * s)",
                    units * s
                );
            }
            assert!(
                rel(sec.pack_l.width(), sec.pack_r.width()) <= 1e-6,
                "{label}: the two packs are the same width"
            );
            if phone {
                let want = (sec.stage.height() - 200.0).clamp(150.0, 320.0);
                assert_eq!(
                    sec.pack_r.height(),
                    want,
                    "{label}: the phone pack height is the documented rule"
                );
                assert!(
                    (150.0..=320.0).contains(&sec.pack_r.height()),
                    "{label}: the phone pack stays inside its clamp"
                );
            } else {
                assert!(
                    rel(sec.pack_r.height(), PACK_H_U * s) <= 0.05,
                    "{label}: the drawn pack's height is its unit count * s"
                );
                assert!(
                    rel(
                        sec.pack_r.width() / PACK_W_U,
                        sec.pack_r.height() / PACK_H_U
                    ) <= 0.05,
                    "{label}: the drawn pack's px per unit agree on both axes"
                );
            }
        }
    }

    /// The drawing's proportions are its own, not the window's: across the three desktop sizes the
    /// casing's width:height ratio agrees within 5 % - a layout that let one axis follow the stage's
    /// aspect would fail here.
    #[test]
    fn the_drawn_section_keeps_its_aspect_across_desktop_sizes() {
        let mut first: Option<(&str, f32)> = None;
        for (label, w, h, phone) in TARGETS {
            if phone {
                continue;
            }
            let sec = layout(area(w, h), phone);
            let ratio = sec.casing.width() / sec.casing.height();
            match first {
                None => first = Some((label, ratio)),
                Some((on, r)) => assert!(
                    rel(ratio, r) <= 0.05,
                    "{label}: the casing is {ratio:.3} wide per tall; {on} is {r:.3}"
                ),
            }
        }
    }

    /// Issue #81, criterion 2, applied to the crossflow screen: the stage (the section's own rect)
    /// is never shorter than the declared minimum at the four sizes; the panel keeps its own area.
    #[test]
    fn the_stage_keeps_the_section_minimum_at_the_four_sizes() {
        for (label, w, h, phone) in TARGETS {
            let sec = layout(area(w, h), phone);
            assert!(
                sec.stage.height() >= MIN_SECTION_H,
                "{label}: the stage is {:.0} px, under the minimum {MIN_SECTION_H}",
                sec.stage.height()
            );
            assert!(
                sec.panel.width() >= 300.0 || phone,
                "{label}: the desktop panel keeps its width"
            );
            assert!(
                sec.panel.height() >= 200.0 || !phone,
                "{label}: the phone panel keeps the metrics and the numerics"
            );
        }
    }

    /// Issue #81, criterion 3: **no label overlaps another, and every label sits inside the
    /// section's own rect** - the Phase-1 frames check's own two predicates, applied to the
    /// drawing's real measured label rects (`labels` is the painter's own source, see `ui`).
    #[test]
    fn no_label_overlaps_another_at_the_four_target_sizes() {
        let p = font_painter();
        let measure = |s: &str, f: FontId| kit::measure(&p, s, f);
        let grid = fixture_grid();
        for (label, w, h, phone) in TARGETS {
            let sec = layout(area(w, h), phone);
            let labels = labels(&sec, &grid, phone, &measure);
            assert_eq!(labels.len(), 6, "{label}: the section's six labels");
            let stage = sec.stage;
            for (i, a) in labels.iter().enumerate() {
                assert!(
                    a.rect.left() >= stage.left() - 1.0
                        && a.rect.top() >= stage.top() - 1.0
                        && a.rect.right() <= stage.right() + 1.0
                        && a.rect.bottom() <= stage.bottom() + 1.0,
                    "{label}: {} at {:?} leaves the stage {stage:?}",
                    a.key,
                    a.rect
                );
                for b in labels.iter().skip(i + 1) {
                    let dx = a.rect.right().min(b.rect.right()) - a.rect.left().max(b.rect.left());
                    let dy = a.rect.bottom().min(b.rect.bottom()) - a.rect.top().max(b.rect.top());
                    assert!(
                        !(dx > 0.5 && dy > 0.5),
                        "{label}: {} and {} share {dx:.1}x{dy:.1} px",
                        a.key,
                        b.key
                    );
                }
            }
        }
    }
}
