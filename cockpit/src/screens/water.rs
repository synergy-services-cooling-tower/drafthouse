//! **Water** (#83): evaporation / drift / blowdown / makeup / cycles as a living flow diagram.
//!
//! Engine: the draft's own run (`Engine::run` -> evaporation), the drift eliminator's curve at the run's face
//! velocity (`airside::drift_performance_at_velocity` + `water_balance::drift_loss_kg_s`) and the balance
//! itself (`water_balance::cooling_tower_water_balance`) - recomputed every frame at the cycles the slider sets.
//!
//! The picture: the tower basin in the middle, makeup piped in from the left, blowdown out at the bottom
//! right, evaporation and drift rising off the top. Pipe width is the flow (sqrt scale, so evaporation and
//! drift stay visible next to each other), and the particle speed along each pipe is the flow too: halve the
//! makeup and its dots slow to half. Motion off paints the same pipes with the dots at rest.

use bevy_egui::egui::{self, pos2, vec2, Align2, Pos2, Rect, Shape, Stroke, StrokeKind};
use cockpit::engine::EngineInput;

use super::kit::{self, text, Src};

const BAL: &str = "water balance · at the cycles set here";
use super::{data, info_card, title_band, toggle_info, Env, State};
use crate::theme as t;

/// Series colours, shared by the picture, the bar and the rows. Amber is the theme's synthetic-data
/// signal only, so blowdown (the concentrated water leaving) takes its own violet.
const BLOWDOWN: egui::Color32 = egui::Color32::from_rgb(0xa4, 0x8b, 0xe0);
const DRIFT: egui::Color32 = t::PRIMARY;

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Water",
        "make-up · evaporation · drift · blowdown",
        env.phone,
    );
    let p = ui.painter().clone();
    let Some(out) = env.out else {
        text(
            &p,
            body.center(),
            Align2::CENTER_CENTER,
            "No result for this duty",
            kit::sans(13.0),
            t::DANGER,
        );
        return;
    };
    let w = match data::water(draft, out, st.cycles) {
        Ok(w) => w,
        Err(e) => {
            text(
                &p,
                body.center(),
                Align2::CENTER_CENTER,
                &e,
                kit::sans(13.0),
                t::DANGER,
            );
            return;
        }
    };
    let to_m3h = |kg_s: f64| kg_s / w.density * 3600.0;
    // The same flows per day (issue #83: the balance is read in m³/hr and m³/day).
    let to_m3d = |m3h: f64| m3h * 24.0;
    // the parts at their own values, printed by the house formatter (3 significant figures) like every
    // other flow. Issue #137 (conductor review): forcing them to sum to the make-up at 0.01 m³/h
    // (largest remainder) turned drift's 0.00686 m³/h into 0.01, then printed it as "0.0100" - a
    // 46 % error that disagreed with the answer's drift rate. Each shown part is now within 0.5 % of
    // its own value; their shown sum can differ from the shown make-up in the third figure.
    let parts = [
        to_m3h(w.evaporation_kg_s),
        to_m3h(w.drift_kg_s),
        to_m3h(w.blowdown_kg_s),
    ];

    // ---- layout: the diagram is the hero; the cycles control + read-outs dock on the right (desk) or below
    let (stage, panel) = if env.phone {
        let ctrl_h = 300.0;
        (
            Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - ctrl_h)),
            Rect::from_min_max(pos2(body.left(), body.bottom() - ctrl_h), body.max),
        )
    } else {
        let pw = 340.0;
        (
            Rect::from_min_max(body.min, pos2(body.right() - pw, body.bottom())),
            Rect::from_min_max(pos2(body.right() - pw, body.top()), body.max),
        )
    };
    kit::ground(
        &p,
        stage,
        stage.center(),
        stage.width().min(stage.height()) * 0.6,
        t::with_alpha(t::WATER, 22),
    );

    // ---- the diagram geometry, in a box fitted to the stage
    let s = (stage.width() / if env.phone { 600.0 } else { 760.0 })
        .min(stage.height() / 560.0)
        .clamp(0.42, 1.25);
    let c = pos2(
        stage.center().x,
        stage.center().y + if env.phone { 40.0 } else { 18.0 } * s,
    );
    let tower_w = 300.0 * s;
    let tower_h = 210.0 * s;
    let tower = Rect::from_center_size(pos2(c.x, c.y - 24.0 * s), vec2(tower_w, tower_h));
    let basin = Rect::from_min_max(
        pos2(tower.left() - 26.0 * s, tower.bottom()),
        pos2(tower.right() + 26.0 * s, tower.bottom() + 58.0 * s),
    );

    // flows -> pipe widths and particle speeds
    let max_w = 34.0 * s;
    let wfor = |kg_s: f64| {
        ((kg_s / w.circulating_kg_s.max(1e-6)).sqrt() as f32 * max_w * 2.2).clamp(2.5, max_w)
    };
    let speed = |kg_s: f64| 26.0 + 120.0 * ((kg_s / w.makeup_kg_s.max(1e-6)) as f32).sqrt();
    let tt = env.t;

    // ---- tower shell
    p.rect_filled(tower, kit::r(4), t::with_alpha(t::PANEL, 210));
    p.rect_stroke(
        tower,
        kit::r(4),
        Stroke::new(1.2, t::LINE),
        StrokeKind::Inside,
    );
    // fill band with a hot->cold gradient (the circulating water inside)
    let fill = Rect::from_min_max(
        pos2(tower.left() + 10.0 * s, tower.top() + tower_h * 0.38),
        pos2(tower.right() - 10.0 * s, tower.top() + tower_h * 0.74),
    );
    kit::vgrad(
        &p,
        fill,
        t::with_alpha(kit::water_temp(0.0), 120),
        t::with_alpha(kit::water_temp(1.0), 120),
    );
    for i in 1..9 {
        let x = fill.left() + fill.width() * i as f32 / 9.0;
        p.line_segment(
            [pos2(x, fill.top()), pos2(x, fill.bottom())],
            Stroke::new(1.0, t::with_alpha(t::BG, 120)),
        );
    }
    // fan stack
    let stack = Rect::from_center_size(
        pos2(tower.center().x, tower.top() - 16.0 * s),
        vec2(tower_w * 0.46, 32.0 * s),
    );
    p.add(Shape::convex_polygon(
        vec![
            pos2(stack.left() + 10.0 * s, stack.top()),
            pos2(stack.right() - 10.0 * s, stack.top()),
            pos2(stack.right(), stack.bottom()),
            pos2(stack.left(), stack.bottom()),
        ],
        t::with_alpha(t::PANEL_RAISED, 230),
        Stroke::new(1.2, t::LINE),
    ));
    // the fan: blades turning at the run's own rpm, scaled down for legibility
    let fan_c = stack.center();
    let rpm = out.airflow_m3_s; // only the direction matters; the rotation is decorative-but-driven:
    let ang = tt * (0.6 + (rpm as f32 / 200.0)) * std::f32::consts::TAU * 0.25;
    for k in 0..5 {
        let a = ang + k as f32 * std::f32::consts::TAU / 5.0;
        let tip = fan_c + vec2(a.cos() * stack.width() * 0.42, a.sin() * 4.0 * s);
        p.line_segment(
            [fan_c, tip],
            Stroke::new(2.0 * s.max(0.7), t::with_alpha(t::AIR, 160)),
        );
    }
    // hot water in at the top (circulating) - a distribution header
    let hdr_y = tower.top() + tower_h * 0.26;
    p.line_segment(
        [
            pos2(tower.left() + 8.0 * s, hdr_y),
            pos2(tower.right() - 8.0 * s, hdr_y),
        ],
        Stroke::new(4.0 * s, t::with_alpha(kit::water_temp(0.0), 200)),
    );
    // falling water inside the tower: dots falling through the fill at the circulating rate
    let rows = 7;
    for i in 0..rows {
        let x = tower.left() + 26.0 * s + (tower_w - 52.0 * s) * i as f32 / (rows - 1) as f32;
        let path = [pos2(x, hdr_y + 4.0), pos2(x, tower.bottom() - 2.0)];
        let col = t::with_alpha(kit::water_temp(0.5), 150);
        kit::flow_dots(
            &p,
            &path,
            tt * 70.0 * s + i as f32 * 9.0,
            22.0 * s,
            1.6 * s.max(0.8),
            col,
        );
    }
    // basin
    p.rect_filled(basin, kit::r(6), t::with_alpha(t::PANEL, 220));
    let level = Rect::from_min_max(
        pos2(basin.left() + 3.0, basin.top() + basin.height() * 0.32),
        pos2(basin.right() - 3.0, basin.bottom() - 3.0),
    );
    kit::vgrad(
        &p,
        level,
        t::with_alpha(kit::water_temp(1.0), 150),
        t::with_alpha(t::WATER, 80),
    );
    // the surface ripple
    let ripple: Vec<Pos2> = (0..=40)
        .map(|i| {
            let x = level.left() + level.width() * i as f32 / 40.0;
            pos2(x, level.top() + (x * 0.06 + tt * 2.2).sin() * 1.6 * s)
        })
        .collect();
    p.add(Shape::line(
        ripple,
        Stroke::new(1.4, t::with_alpha(t::INK, 120)),
    ));
    p.rect_stroke(
        basin,
        kit::r(6),
        Stroke::new(1.2, t::LINE),
        StrokeKind::Inside,
    );

    // ---- the four flows as pipes with particles
    // makeup: in from the left edge into the basin
    let mk_w = wfor(w.makeup_kg_s);
    let mk_y = basin.top() + basin.height() * 0.58;
    let mk_path = [
        pos2(stage.left() + if env.phone { 26.0 } else { 18.0 }, mk_y),
        pos2(basin.left() - 12.0, mk_y),
    ];
    kit::pipe(&p, &mk_path, mk_w, t::WATER);
    kit::flow_dots(
        &p,
        &mk_path,
        tt * speed(w.makeup_kg_s),
        18.0,
        (mk_w * 0.22).max(1.6),
        t::INK,
    );
    // blowdown: out of the basin floor to the right edge
    let bd_w = wfor(w.blowdown_kg_s.max(1e-6));
    let bd_path = [
        pos2(basin.right() - 30.0 * s, basin.bottom() - 2.0),
        pos2(basin.right() - 30.0 * s, basin.bottom() + 34.0 * s),
        pos2(
            stage.right() - if env.phone { 30.0 } else { 18.0 },
            basin.bottom() + 34.0 * s,
        ),
    ];
    if w.blowdown_kg_s > 1e-6 {
        kit::pipe(&p, &bd_path, bd_w, BLOWDOWN);
        kit::flow_dots(
            &p,
            &bd_path,
            tt * speed(w.blowdown_kg_s),
            18.0,
            (bd_w * 0.22).max(1.4),
            t::INK,
        );
    } else {
        kit::dashed(
            &p,
            bd_path[0],
            bd_path[1],
            Stroke::new(1.2, t::MUTED),
            4.0,
            4.0,
        );
        kit::dashed(
            &p,
            bd_path[1],
            bd_path[2],
            Stroke::new(1.2, t::MUTED),
            4.0,
            4.0,
        );
    }
    // evaporation: a plume of vapour rising out of the stack (particles drift up and fan out)
    let ev_w = wfor(w.evaporation_kg_s);
    let plume_n = ((ev_w / max_w) * 26.0).clamp(6.0, 26.0) as usize;
    let rise = stage.top() + 40.0 * s;
    for i in 0..plume_n {
        let f = i as f32 / plume_n as f32;
        let x0 = stack.left() + 14.0 * s + (stack.width() - 28.0 * s) * f;
        let lean = (f - 0.5) * 90.0 * s;
        let path = [
            pos2(x0, stack.top()),
            pos2(x0 + lean * 0.4, (stack.top() + rise) / 2.0),
            pos2(x0 + lean, rise),
        ];
        let a = (60.0 + 80.0 * (1.0 - (f - 0.5).abs() * 2.0)) as u8;
        kit::flow_dots(
            &p,
            &path,
            tt * speed(w.evaporation_kg_s) * 0.55 + i as f32 * 13.0,
            20.0,
            2.6 * s.max(0.8),
            t::with_alpha(t::AIR, a),
        );
    }
    // drift: a few heavier droplets that leave the stack's rim sideways, clear of the plume
    // (rate-proportional count), coloured as in the panel
    let dr_n = ((w.drift_kg_s / w.evaporation_kg_s.max(1e-9)) * 300.0).clamp(1.0, 6.0) as usize;
    let dr_end = if env.phone {
        pos2(stack.right() + 70.0 * s, stack.top() - 12.0 * s)
    } else {
        pos2(stack.right() + 120.0 * s, stack.top() - 6.0 * s)
    };
    for i in 0..dr_n {
        let path = [
            pos2(stack.right() - 4.0, stack.top() + 2.0),
            pos2(
                stack.right() + 50.0 * s,
                stack.top() - 30.0 * s - i as f32 * 3.0,
            ),
            dr_end + vec2(0.0, i as f32 * 2.0),
        ];
        kit::flow_dots(
            &p,
            &path,
            tt * speed(w.drift_kg_s) + i as f32 * 31.0,
            40.0,
            2.4 * s.max(0.8),
            DRIFT,
        );
    }
    // direction marks at the pipe mouths
    arrow(
        &p,
        mk_path[1],
        vec2(1.0, 0.0),
        (mk_w * 0.5).max(6.0),
        t::WATER,
    );
    if w.blowdown_kg_s > 1e-6 {
        arrow(
            &p,
            bd_path[2],
            vec2(1.0, 0.0),
            (bd_w * 0.5).max(6.0),
            BLOWDOWN,
        );
    }

    // ---- labels on the picture: number + unit, short label (details behind the panel's ⓘ)
    let vsize = if env.phone { 15.0 } else { 18.0 };
    // name over (or under) "value unit"; the pair is placed as one block so the unit always follows
    let lab = |at: Pos2, align: Align2, name: &str, v: f64, col: egui::Color32| {
        let up = align.y() == egui::Align::Max;
        let vs = t::num::flow(v);
        let vw = kit::measure(&p, &vs, kit::num(vsize)).x
            + 4.0
            + kit::measure(&p, "m³/h", kit::sans(10.5)).x;
        let x0 = match align.x() {
            egui::Align::Min => at.x,
            egui::Align::Center => at.x - vw / 2.0,
            egui::Align::Max => at.x - vw,
        };
        let r1 = text(&p, at, align, name, kit::semi(11.0), t::INK_2);
        let vy = if up {
            r1.top() - 2.0
        } else {
            r1.bottom() + 1.0
        };
        let vr = text(
            &p,
            pos2(x0, vy),
            if up {
                Align2::LEFT_BOTTOM
            } else {
                Align2::LEFT_TOP
            },
            &vs,
            kit::num(vsize),
            col,
        );
        let u = text(
            &p,
            pos2(vr.right() + 4.0, vr.bottom() - 3.0),
            Align2::LEFT_BOTTOM,
            "m³/h",
            kit::sans(10.5),
            t::INK_2,
        );
        kit::src_mark(
            &p,
            pos2(u.right() + 4.0, vr.center().y),
            Src::Calc,
            match name {
                "evaporation" => "evaporation · air flow × moisture picked up",
                "drift" => "drift · eliminator loss curve at this air flow",
                _ => BAL,
            },
        );
    };
    lab(
        pos2(stage.left() + 20.0, mk_y - mk_w * 0.5 - 6.0),
        Align2::LEFT_BOTTOM,
        "make-up",
        to_m3h(w.makeup_kg_s),
        t::WATER,
    );
    // under the pipe's run on both: above the elbow (the phone's old place) the value sat across the
    // basin's outline - 47 px between the basin and the bezel do not hold "3.75 m³/h" (conductor review)
    let bd_lab = pos2(
        stage.right() - if env.phone { 34.0 } else { 20.0 },
        bd_path[2].y + bd_w * 0.5 + 6.0,
    );
    lab(bd_lab, Align2::RIGHT_TOP, "blowdown", parts[2], BLOWDOWN);
    // phone: evaporation left of the plume, drift right of it, both clear of the title
    let ev_at = if env.phone {
        pos2(stack.left() - 34.0 * s, stack.top() - 34.0 * s)
    } else {
        pos2(stack.center().x + 120.0 * s, rise + 30.0 * s)
    };
    lab(
        ev_at,
        if env.phone {
            Align2::RIGHT_BOTTOM
        } else {
            Align2::LEFT_BOTTOM
        },
        "evaporation",
        parts[0],
        t::AIR,
    );
    // drift: label at the droplets' end, joined by a short leader
    let dr_at = if env.phone {
        pos2(stack.right() + 36.0 * s, stack.top() - 34.0 * s)
    } else {
        dr_end + vec2(10.0, -2.0)
    };
    lab(dr_at, Align2::LEFT_BOTTOM, "drift", parts[1], DRIFT);
    // circulating, quietly, on the basin
    kit::label_plate(
        &p,
        pos2(basin.center().x, basin.bottom() - 8.0 * s),
        Align2::CENTER_BOTTOM,
        &format!(
            "{} m³/h circulating",
            t::num::flow(to_m3h(w.circulating_kg_s))
        ),
        kit::mono(11.5),
        t::INK,
    );

    // ---- the panel: cycles control + the balance, read as one equation
    let pp = panel.shrink2(vec2(if env.phone { 12.0 } else { 18.0 }, 12.0));
    kit::glass(&p, pp, 12);
    if !env.phone {
        kit::side_panel(pp);
    }
    let inner = pp.shrink(16.0);
    let mut y = inner.top();
    text(
        &p,
        pos2(inner.left(), y),
        Align2::LEFT_TOP,
        "Cycles of concentration",
        kit::semi(12.0),
        t::INK_2,
    );
    let ir = kit::info_dot(
        ui,
        pos2(inner.right() - 8.0, y + 8.0),
        "cycles",
        st.info.as_deref() == Some("cycles"),
    );
    if ir.clicked() {
        toggle_info(st, "cycles");
    }
    y += 22.0;
    let big = text(
        &p,
        pos2(inner.left(), y),
        Align2::LEFT_TOP,
        &format!("{:.1}", w.cycles),
        kit::num(if env.phone { 30.0 } else { 40.0 }),
        t::INK,
    );
    let x = text(
        &p,
        pos2(big.right() + 4.0, big.bottom() - 4.0),
        Align2::LEFT_BOTTOM,
        "×",
        kit::sans(if env.phone { 18.0 } else { 22.0 }),
        t::INK_2,
    );
    kit::src_mark(
        &p,
        pos2(x.right() + 6.0, big.center().y),
        Src::Catalog,
        if (st.cycles - draft.duty.cycles_of_concentration).abs() < 1e-9 {
            "duty · cycles of concentration (recorded)"
        } else {
            "your entry (− / +), from the duty's cycles"
        },
    );
    // − / + steppers
    let bw = 48.0;
    let minus = Rect::from_min_size(
        pos2(inner.right() - 2.0 * bw - 8.0, y + 4.0),
        vec2(bw, 44.0),
    );
    let plus = Rect::from_min_size(pos2(inner.right() - bw, y + 4.0), vec2(bw, 44.0));
    if kit::button(ui, minus, "cyc.minus", "−", kit::Btn::Ghost).clicked() {
        st.cycles = ((st.cycles * 2.0).round() / 2.0 - 0.5).max(1.5);
    }
    if kit::button(ui, plus, "cyc.plus", "+", kit::Btn::Ghost).clicked() {
        st.cycles = ((st.cycles * 2.0).round() / 2.0 + 0.5).min(10.0);
    }
    y = big.bottom() + 12.0;
    // the slider track: drag anywhere
    let track = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 30.0));
    let resp = ui.interact(
        track,
        egui::Id::new("screens.water.cycles"),
        egui::Sense::click_and_drag(),
    );
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let f = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
            st.cycles = ((1.5 + f * 8.5) * 10.0).round() / 10.0;
        }
    }
    let line_y = track.center().y;
    p.line_segment(
        [pos2(track.left(), line_y), pos2(track.right(), line_y)],
        Stroke::new(4.0, t::LINE_SOFT),
    );
    let f = ((w.cycles - 1.5) / 8.5) as f32;
    let kx = track.left() + track.width() * f;
    p.line_segment(
        [pos2(track.left(), line_y), pos2(kx, line_y)],
        Stroke::new(4.0, t::PRIMARY),
    );
    // the duty's own cycles as a tick
    let fd = ((draft.duty.cycles_of_concentration - 1.5) / 8.5) as f32;
    let dx = track.left() + track.width() * fd;
    p.line_segment(
        [pos2(dx, line_y - 9.0), pos2(dx, line_y + 9.0)],
        Stroke::new(1.5, t::INK_2),
    );
    p.circle_filled(pos2(kx, line_y), 9.0, t::INK);
    p.circle_filled(pos2(kx, line_y), 4.0, t::PRIMARY);
    y = track.bottom() + 2.0;
    text(
        &p,
        pos2(inner.left(), y),
        Align2::LEFT_TOP,
        "1.5",
        kit::mono(10.5),
        t::MUTED,
    );
    text(
        &p,
        pos2(dx, y),
        Align2::CENTER_TOP,
        &format!("duty {:.1}", draft.duty.cycles_of_concentration),
        kit::mono(10.5),
        t::INK_2,
    );
    text(
        &p,
        pos2(inner.right(), y),
        Align2::RIGHT_TOP,
        "10",
        kit::mono(10.5),
        t::MUTED,
    );
    y += 22.0;

    if !env.phone {
        p.line_segment(
            [pos2(inner.left(), y), pos2(inner.right(), y)],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        y += 16.0;
        // the balance as a stacked bar: makeup = evaporation + drift + blowdown
        let bar = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 16.0));
        let total = w.makeup_kg_s.max(1e-9);
        let mut x = bar.left();
        for (v, col) in [
            (w.evaporation_kg_s, t::AIR),
            (w.drift_kg_s, DRIFT),
            (w.blowdown_kg_s, BLOWDOWN),
        ] {
            let ww = bar.width() * (v / total) as f32;
            p.rect_filled(
                Rect::from_min_size(
                    pos2(x, bar.top()),
                    vec2(ww.max(if v > 0.0 { 2.0 } else { 0.0 }), bar.height()),
                ),
                kit::r(0),
                col,
            );
            x += ww;
        }
        y = bar.bottom() + 14.0;
        // each part at its own value, 3 significant figures (see `parts`)
        let row = |y: f32, name: &str, v: f64, pct: f64, col: egui::Color32, strong: bool| {
            p.circle_filled(pos2(inner.left() + 5.0, y + 8.0), 4.5, col);
            text(
                &p,
                pos2(inner.left() + 16.0, y),
                Align2::LEFT_TOP,
                name,
                if strong {
                    kit::semi(12.5)
                } else {
                    kit::sans(12.5)
                },
                if strong { t::INK } else { t::INK_2 },
            );
            let vr = text(
                &p,
                pos2(inner.right() - 36.0, y - 1.0),
                Align2::RIGHT_TOP,
                &t::num::flow(v),
                kit::num(if strong { 16.0 } else { 14.0 }),
                t::INK,
            );
            text(
                &p,
                pos2(inner.right(), vr.center().y),
                Align2::RIGHT_CENTER,
                "m³/h",
                kit::sans(10.5),
                t::MUTED,
            );
            // the same flow per day, under the m³/h value (issue #83's unit pair)
            text(
                &p,
                pos2(inner.right(), vr.bottom() + 1.0),
                Align2::RIGHT_TOP,
                &format!("{} m³/day", t::num::flow(to_m3d(v))),
                kit::mono(10.5),
                t::MUTED,
            );
            let mk = kit::src_mark(&p, pos2(vr.left() - 14.0, vr.center().y), Src::Calc, BAL);
            let ps = format!("{} %", t::num::pct(pct));
            // the share sits in its column, or left of the value's mark when the value is wide
            // ("0.00686" pushed the mark onto "<1 %")
            text(
                &p,
                pos2((inner.right() - 98.0).min(mk.left() - 6.0), y + 1.0),
                Align2::RIGHT_TOP,
                &ps,
                kit::mono(10.5),
                t::MUTED,
            );
        };
        row(
            y,
            "evaporation",
            parts[0],
            100.0 * w.evaporation_kg_s / total,
            t::AIR,
            false,
        );
        y += 44.0;
        row(
            y,
            "drift",
            parts[1],
            100.0 * w.drift_kg_s / total,
            DRIFT,
            false,
        );
        y += 44.0;
        row(
            y,
            "blowdown",
            parts[2],
            100.0 * w.blowdown_kg_s / total,
            BLOWDOWN,
            false,
        );
        y += 46.0;
        p.line_segment(
            [pos2(inner.left(), y - 6.0), pos2(inner.right(), y - 6.0)],
            Stroke::new(1.0, t::LINE),
        );
        row(y, "make-up", to_m3h(w.makeup_kg_s), 100.0, t::WATER, true);
        y += 40.0;
        // the trade-off the slider walks: make-up and blowdown over the cycles range, one engine
        // balance per point, the current cycles as the line
        let chart = Rect::from_min_max(
            pos2(inner.left() + 30.0, y + 24.0),
            pos2(inner.right() - 12.0, inner.bottom() - 104.0),
        );
        if chart.height() > 90.0 {
            sweep(&p, chart, draft, out, w.cycles, &to_m3h);
        }
        y = inner.bottom() - 50.0;
        // engine provenance line, short
        let ok = w.check_m3_h < 0.01;
        kit::chip(
            &p,
            pos2(inner.left(), y),
            Align2::LEFT_TOP,
            if ok {
                "matches the tower's result"
            } else {
                "differs from the tower's result"
            },
            if ok { t::OK } else { t::DANGER },
            if ok { t::OK_SOFT } else { t::DANGER_SOFT },
            t::with_alpha(if ok { t::OK } else { t::DANGER }, 120),
        );
        text(
            &p,
            pos2(inner.left(), y + 30.0),
            Align2::LEFT_TOP,
            &format!(
                "drift {} ppm at the run's face velocity",
                t::num::sig(w.drift_ppm, 3)
            ),
            kit::sans(11.0),
            t::MUTED,
        );
    } else {
        // phone: the four numbers as a 2x2 grid under the slider, each with its m³/day line
        let cw = inner.width() / 2.0;
        for (i, (name, v, col)) in [
            ("make-up", to_m3h(w.makeup_kg_s), t::WATER),
            ("evaporation", parts[0], t::AIR),
            ("blowdown", parts[2], BLOWDOWN),
            ("drift", parts[1], DRIFT),
        ]
        .into_iter()
        .enumerate()
        {
            let at = pos2(
                inner.left() + cw * (i % 2) as f32,
                y + 6.0 + 60.0 * (i / 2) as f32,
            );
            kit::metric_src(
                &p,
                at,
                name,
                &t::num::flow(v),
                "m³/h",
                17.0,
                col,
                Src::Calc,
                BAL,
            );
            text(
                &p,
                pos2(at.x, at.y + 40.0),
                Align2::LEFT_TOP,
                &format!("{} m³/day", t::num::flow(to_m3d(v))),
                kit::mono(10.5),
                t::MUTED,
            );
        }
    }
    info_card(
        ui,
        st,
        "cycles",
        pos2(inner.right() - 8.0, inner.top() + 8.0),
        &[
            "Cycles of concentration",
            "Dissolved solids in the basin over",
            "the make-up. Higher cycles: less",
            "blowdown, harder water.",
            "Blowdown = evap/(cycles−1) − drift",
        ],
        area,
    );
    if env.motion {
        ui.ctx().request_repaint();
    }
}

fn arrow(p: &egui::Painter, tip: Pos2, dir: egui::Vec2, s: f32, col: egui::Color32) {
    let n = vec2(-dir.y, dir.x);
    p.add(Shape::convex_polygon(
        vec![
            tip + dir * s * 0.9,
            tip - dir * s * 0.5 + n * s * 0.7,
            tip - dir * s * 0.5 - n * s * 0.7,
        ],
        col,
        Stroke::NONE,
    ));
}

/// Make-up and blowdown over cycles 1.5..10: `data::water` (the engine's balance) at each point.
fn sweep(
    p: &egui::Painter,
    r: Rect,
    d: &EngineInput,
    o: &cockpit::engine::EngineOutput,
    now: f64,
    to_m3h: &dyn Fn(f64) -> f64,
) {
    let xs: Vec<f64> = (0..=34).map(|i| 1.5 + 0.25 * i as f64).collect();
    let pts: Vec<(f64, f64, f64)> = xs
        .iter()
        .filter_map(|c| {
            data::water(d, o, *c)
                .ok()
                .map(|w| (*c, to_m3h(w.makeup_kg_s), to_m3h(w.blowdown_kg_s)))
        })
        .collect();
    if pts.len() < 2 {
        return;
    }
    let ymax = pts.iter().map(|x| x.1).fold(0.0, f64::max) * 1.05;
    let plot = kit::Plot {
        rect: r,
        x: (1.5, 10.0),
        y: (0.0, ymax),
    };
    let ys = super::rate::nice(ymax / 4.0);
    plot.axes(p, 2.0, ys, "cycles", "m³/h", 0, 0);
    plot.line(
        p,
        &pts.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(),
        Stroke::new(2.2, t::WATER),
    );
    plot.line(
        p,
        &pts.iter().map(|x| (x.0, x.2)).collect::<Vec<_>>(),
        Stroke::new(2.2, BLOWDOWN),
    );
    let a = plot.pt(now, 0.0);
    p.line_segment([a, pos2(a.x, r.top())], Stroke::new(1.2, t::INK));
    if let Ok(w) = data::water(d, o, now) {
        p.circle_filled(plot.pt(now, to_m3h(w.makeup_kg_s)), 4.0, t::WATER);
        p.circle_filled(plot.pt(now, to_m3h(w.blowdown_kg_s)), 4.0, BLOWDOWN);
    }
    let l1 = text(
        p,
        pos2(r.right() - 6.0, r.top() + 6.0),
        Align2::RIGHT_TOP,
        "make-up",
        kit::sans(10.5),
        t::WATER,
    );
    text(
        p,
        pos2(r.right() - 6.0, l1.bottom() + 2.0),
        Align2::RIGHT_TOP,
        "blowdown",
        kit::sans(10.5),
        BLOWDOWN,
    );
}
