//! **Crossflow** (#84): how the section looks for a crossflow tower.
//!
//! Engine: `crossflow::solve_crossflow_grid` on the best crossflow candidate the Size screen's selection run
//! found at this duty (its dry-air flow and available KaV/L). The grid is the engine's own cell field: water
//! temperature falling top to bottom, air enthalpy rising left to right. The section paints one fill pack per
//! side (the engine solves one; the tower is symmetric, so the left pack is its mirror), the plenum between
//! them and the fan stack above. Air dots cross the fill at a speed set by the candidate's airflow; water
//! drops fall at a speed set by the circulating flow.

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Shape, Stroke, StrokeKind};
use cockpit::engine::EngineInput;

use super::kit::{self, text};
use super::{info_card, title_band, toggle_info, Env, State};
use crate::theme as t;

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

    let (stage, panel) = if env.phone {
        let h = 214.0;
        (
            Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - h)),
            Rect::from_min_max(pos2(body.left(), body.bottom() - h), body.max),
        )
    } else {
        let w = 320.0;
        (
            Rect::from_min_max(body.min, pos2(body.right() - w, body.bottom())),
            Rect::from_min_max(pos2(body.right() - w, body.top()), body.max),
        )
    };
    kit::ground(
        &p,
        stage,
        stage.center(),
        stage.width().min(stage.height()) * 0.55,
        t::with_alpha(t::PRIMARY, 16),
    );

    // ---- section geometry
    let s = (stage.width() / 900.0)
        .min(stage.height() / 700.0)
        .clamp(0.36, 1.3);
    let c = pos2(stage.center().x, stage.center().y + 30.0 * s);
    let pack_w = 250.0 * s;
    // phone: the stage is tall and narrow (s is set by the width), so the packs take the height
    let pack_h = if env.phone {
        (stage.height() - 200.0).clamp(150.0, 320.0)
    } else {
        370.0 * s
    };
    let plenum_w = 150.0 * s;
    let top = c.y - pack_h / 2.0;
    let right_pack = Rect::from_min_size(pos2(c.x + plenum_w / 2.0, top), vec2(pack_w, pack_h));
    let left_pack = Rect::from_min_size(
        pos2(c.x - plenum_w / 2.0 - pack_w, top),
        vec2(pack_w, pack_h),
    );
    let deck_h = 22.0 * s;
    let casing = Rect::from_min_max(
        pos2(left_pack.left() - 6.0, top - deck_h - 8.0),
        pos2(right_pack.right() + 6.0, right_pack.bottom() + 40.0 * s),
    );
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
    let basin = Rect::from_min_max(
        pos2(casing.left() + 4.0, right_pack.bottom() + 4.0),
        pos2(casing.right() - 4.0, casing.bottom() - 4.0),
    );
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
    let plenum = Rect::from_min_max(
        pos2(left_pack.right(), top),
        pos2(right_pack.left(), right_pack.bottom()),
    );
    p.rect_filled(plenum, kit::r(0), t::with_alpha(t::BG, 160));
    let stack = Rect::from_min_max(
        pos2(c.x - plenum_w * 0.6, casing.top() - 46.0 * s),
        pos2(c.x + plenum_w * 0.6, casing.top() + 1.0),
    );
    let tt = env.t;
    // ---- motion: air across the fill (both sides inward), up the plenum, out of the stack
    let air_speed = (40.0 + grid.cand.airflow_m3_s as f32 * 0.35) * s;
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
                tt * air_speed * 0.28 + i as f32 * 7.0,
                26.0 * s.max(0.5),
                7.0 * s.max(0.6),
                t::with_alpha(Color32::from_rgb(0xe8, 0xee, 0xf2), 190),
            );
        }
    }
    // water drops falling through each pack
    let water_speed = (34.0 + (grid.cand.depth_m as f32) * 4.0) * s;
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
        let a = tt * 3.2 + k as f32 * std::f32::consts::TAU / 6.0;
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

    // ---- labels on the section, short
    if env.phone {
        // the outer corners, on the hot labels' row: clear of the frame and of the labels
        text(
            &p,
            pos2(stage.right() - 14.0, casing.top() - 8.0),
            Align2::RIGHT_BOTTOM,
            "air in",
            kit::semi(11.5),
            t::AIR,
        );
        text(
            &p,
            pos2(stage.left() + 14.0, casing.top() - 8.0),
            Align2::LEFT_BOTTOM,
            "air in",
            kit::semi(11.5),
            t::AIR,
        );
    } else {
        text(
            &p,
            pos2(right_pack.right() + 24.0 * s, top - 6.0),
            Align2::LEFT_BOTTOM,
            "air in",
            kit::semi(11.0),
            t::AIR,
        );
        text(
            &p,
            pos2(left_pack.left() - 24.0 * s, top - 6.0),
            Align2::RIGHT_BOTTOM,
            "air in",
            kit::semi(11.0),
            t::AIR,
        );
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
    let ao = if env.phone {
        pos2(c.x, stack.top() - 44.0 * s - 6.0)
    } else {
        pos2(stack.right() + 14.0, stack.top() - 10.0 * s)
    };
    kit::label_plate(
        &p,
        ao,
        if env.phone {
            Align2::CENTER_BOTTOM
        } else {
            Align2::LEFT_BOTTOM
        },
        &format!("air out {:.1} °C", grid.outlet_db),
        kit::num(12.0),
        t::INK_2,
    );
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
    for pk in [left_pack, right_pack] {
        kit::label_plate(
            &p,
            pos2(pk.center().x, casing.top() - 6.0),
            Align2::CENTER_BOTTOM,
            &format!("hot {:.1} °C", grid.hot_c),
            kit::num(12.0),
            kit::water_temp(0.0),
        );
    }
    // in the basin when it is tall enough, else just under the casing (the legend moves down)
    let cold_in = basin.height() >= 24.0;
    let cold_at = if cold_in {
        basin.center()
    } else {
        pos2(c.x, casing.bottom() + 14.0)
    };
    kit::label_plate(
        &p,
        cold_at,
        Align2::CENTER_CENTER,
        &format!("cold {:.2} °C", grid.cold_c),
        kit::num(13.0),
        t::INK,
    );

    // the legend bar under the left pack
    let lg = Rect::from_center_size(
        pos2(c.x, casing.bottom() + if cold_in { 16.0 } else { 40.0 }),
        vec2(pack_w * 1.2, 8.0),
    );
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
                "engine · crossflow.rs · {nx}×{ny} · {}",
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
