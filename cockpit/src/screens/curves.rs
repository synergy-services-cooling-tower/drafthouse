//! **Curves** (#83): cold-water temperature against wet bulb at several flow %, interactive, with the
//! inverse lookup.
//!
//! Engine: the cockpit's own `Engine::run` (the adapter's `RealEngine`) on edited copies of the draft builds
//! the grid - wet bulb x range x flow %, with the hot water iterated so each record's range is the solved one
//! (`data::Cache::step_curves`, sliced across frames). The read-off at the probe is
//! `performance_curve::predict_cold_water_from_performance_curves`; the inverse ("what flow gives this cold
//! water?") is `predict_water_flow_from_performance_curves`. Both run every frame on the engine-built grid.

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Rect, Stroke};
use cockpit::engine::EngineInput;

use super::data::{self, CurveData};
use super::kit::{self, text, Plot, Src};
use super::{info_card, title_band, toggle_info, Env, State};
use crate::theme as t;

const FLOW_COLS: [Color32; 3] = [
    Color32::from_rgb(0x7f, 0xc8, 0xa9), // 80 %
    Color32::from_rgb(0x2f, 0xb3, 0xc9), // 100 % (the primary)
    Color32::from_rgb(0x8f, 0x9c, 0xf0), // 120 %
];

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Curves",
        "fan vs system · cold water vs wet bulb",
        env.phone,
    );
    let p = ui.painter().clone();
    // round 2: the Instrument's Operating point lives here now. Phone: one chart at a time.
    let body = if env.phone {
        let seg = Rect::from_min_size(
            pos2(body.left() + 12.0, body.top() + 8.0),
            vec2(body.width() - 24.0, 40.0),
        );
        if let Some(i) = kit::segmented(
            ui,
            seg,
            "curves.chart",
            &["Cold water", "Fan & system"],
            st.chart,
            12.0,
        ) {
            st.chart = i;
        }
        if st.chart == 1 {
            let r = Rect::from_min_max(
                pos2(body.left() + 12.0, seg.bottom() + 12.0),
                body.max - vec2(12.0, 8.0),
            );
            if let Some((ax, ay, kw, band)) = fan_system(&p, r, draft, env, true) {
                fan_readout(&p, band, ax, ay, kw, true);
            }
            return;
        }
        Rect::from_min_max(pos2(body.left(), seg.bottom() + 6.0), body.max)
    } else {
        body
    };
    // desktop: the left column is two charts, cold water (top) and fan/system (below it)
    let fan_h = 300.0;
    let mut op_vals: Option<(f64, f64, f64, Rect)> = None;
    let mut ro: Option<Rect> = None;
    if !env.phone {
        let pad = 24.0;
        let inner = body.shrink2(vec2(pad, 12.0));
        let fr = Rect::from_min_max(
            pos2(inner.left(), inner.bottom() - fan_h),
            pos2(inner.right() - 340.0 - 24.0, inner.bottom()),
        );
        // its readout sits in the side panel's foot, level with the chart; drawn after the side panel's
        // glass (below), so it is not washed out
        op_vals = fan_system(&p, fr, draft, env, false);
        ro = Some(Rect::from_min_max(
            pos2(inner.right() - 340.0 + 16.0, inner.bottom() - fan_h + 6.0),
            pos2(inner.right() - 16.0, inner.bottom() - fan_h + 150.0),
        ));
    }
    let Some(curves) = st.cache.curves.clone() else {
        let (done, n) = st.cache.curves_progress();
        let c = if env.phone {
            body.center()
        } else {
            pos2(
                body.left() + (body.width() - 364.0) / 2.0,
                body.top() + (body.height() - fan_h) / 2.0,
            )
        };
        let f = if n == 0 { 0.0 } else { done as f32 / n as f32 };
        let bar = Rect::from_center_size(c, vec2(220.0, 6.0));
        p.rect_filled(bar, kit::r(3), t::LINE_SOFT);
        p.rect_filled(
            Rect::from_min_size(bar.min, vec2(bar.width() * f, 6.0)),
            kit::r(3),
            t::PRIMARY,
        );
        text(
            &p,
            c + vec2(0.0, 22.0),
            Align2::CENTER_CENTER,
            &format!("engine runs {done}/{n}"),
            kit::mono(11.5),
            t::MUTED,
        );
        if let (Some((ax, ay, kw, _)), Some(rr)) = (op_vals, ro) {
            fan_readout(&p, rr, ax, ay, kw, false);
        }
        return;
    };
    let pad = if env.phone { 12.0 } else { 24.0 };
    let inner = body.shrink2(vec2(pad, 12.0));
    // range selector
    let ranges = data::CURVE_RANGE;
    let ri = ranges
        .iter()
        .position(|r| (r - st.range_c).abs() < 0.01)
        .unwrap_or(1);
    let range = ranges[ri];
    let wb_lo = data::CURVE_WB[0];
    let wb_hi = *data::CURVE_WB.last().unwrap();
    let probe = st.probe_wb.clamp(wb_lo, wb_hi);
    let design_flow = curves.design_flow_kg_s;

    let (chart_r, side_r) = if env.phone {
        let side_h = 250.0;
        (
            Rect::from_min_max(
                pos2(inner.left(), inner.top() + 52.0),
                pos2(inner.right(), inner.bottom() - side_h),
            ),
            Rect::from_min_max(pos2(inner.left(), inner.bottom() - side_h + 8.0), inner.max),
        )
    } else {
        let w = 340.0;
        (
            Rect::from_min_max(
                pos2(inner.left(), inner.top() + 52.0),
                pos2(inner.right() - w - 24.0, inner.bottom() - fan_h - 16.0),
            ),
            Rect::from_min_max(pos2(inner.right() - w, inner.top()), inner.max),
        )
    };
    // the controls over the chart: range segmented
    let seg_w = if env.phone { inner.width() } else { 300.0 };
    let seg = Rect::from_min_size(pos2(inner.left(), inner.top() + 4.0), vec2(seg_w, 38.0));
    let labels: Vec<String> = ranges.iter().map(|r| format!("{r:.0} K range")).collect();
    let lref: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    if let Some(i) = kit::segmented(
        ui,
        seg,
        "curves.range",
        &lref,
        ri,
        if env.phone { 11.5 } else { 12.0 },
    ) {
        st.range_c = ranges[i];
    }

    // ---- the chart
    let recs: Vec<&data::CurveRec> = curves
        .recs
        .iter()
        .filter(|r| (r.range - range).abs() < 0.01)
        .collect();
    let mut y0 = f64::INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for r in &recs {
        y0 = y0.min(r.cold);
        y1 = y1.max(r.cold);
    }
    if !y0.is_finite() {
        let (done, n) = st.cache.curves_progress();
        let msg = if done < n {
            format!("engine runs {done}/{n} · this range is next")
        } else {
            "no engine record at this range".into()
        };
        text(
            &p,
            chart_r.center(),
            Align2::CENTER_CENTER,
            &msg,
            kit::sans(12.5),
            if done < n { t::MUTED } else { t::DANGER },
        );
        if done < n {
            ui.ctx().request_repaint();
        }
        return;
    }
    let plot = Plot {
        rect: Rect::from_min_max(
            pos2(chart_r.left() + 40.0, chart_r.top() + 34.0),
            pos2(chart_r.right() - 10.0, chart_r.bottom() - 44.0),
        ),
        x: (wb_lo - 0.5, wb_hi + 0.5),
        y: ((y0 - 0.8).floor(), (y1 + 0.8).ceil()),
    };
    plot.axes(
        &p,
        2.0,
        if plot.y.1 - plot.y.0 > 12.0 { 2.0 } else { 1.0 },
        "wet bulb °C",
        "cold water °C",
        0,
        0,
    );
    // the approach floor: cold = wet bulb (a tower cannot cool below it)
    let floor: Vec<(f64, f64)> = [plot.x.0, plot.x.1].iter().map(|x| (*x, *x)).collect();
    plot.line(&p, &floor, Stroke::new(1.0, t::with_alpha(t::DANGER, 110)));
    // name it along the diagonal, above-right of the line, mid-plot (it was sitting on the bottom frame)
    let fx = (plot.y.0.max(plot.x.0) + plot.y.1.min(plot.x.1)) / 2.0;
    if fx < plot.x.1 {
        kit::label_plate(
            &p,
            plot.pt(fx, fx) + vec2(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            "cold = wet bulb",
            kit::sans(11.0),
            t::DANGER,
        );
    }
    // one line per flow %, through the engine-built records, and its points
    for (fi, fpct) in data::CURVE_FLOW_PCT.iter().enumerate() {
        let flow = design_flow * fpct / 100.0;
        let mut pts: Vec<(f64, f64)> = recs
            .iter()
            .filter(|r| (r.flow_kg_s - flow).abs() < 1e-6)
            .map(|r| (r.wb, r.cold))
            .collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let col = FLOW_COLS[fi];
        plot.line(
            &p,
            &pts,
            Stroke::new(
                if (*fpct - 100.0).abs() < 0.1 {
                    2.8
                } else {
                    2.0
                },
                col,
            ),
        );
        for (x, y) in &pts {
            p.circle_filled(plot.pt(*x, *y), 3.2, col);
        }
        // end label: at the line's first point (left end, where the three lines are furthest apart),
        // on a plate, above-right of the point so its own line runs away from it
        if let Some((x, y)) = pts.first() {
            kit::label_plate(
                &p,
                plot.pt(*x, *y) + vec2(8.0, -6.0),
                Align2::LEFT_BOTTOM,
                &format!("{fpct:.0}%"),
                kit::semi(11.0),
                col,
            );
        }
    }
    // the design point
    let dp = plot.pt(draft.duty.wet_bulb_c, draft.duty.target_cold_water_c);
    if (range - (draft.duty.hot_water_c - draft.duty.target_cold_water_c)).abs() < 0.6 {
        p.circle_stroke(dp, 7.0, Stroke::new(1.6, t::INK));
        // below-right, away from the lines (they rise left to right) and the probe readings (on the probe line)
        kit::label_plate(
            &p,
            dp + vec2(-12.0, 14.0),
            Align2::RIGHT_TOP,
            "design duty",
            kit::mono(10.5),
            t::INK,
        );
    }

    // ---- the probe: drag along the chart
    let resp = ui.interact(
        plot.rect,
        egui::Id::new("curves.probe"),
        egui::Sense::click_and_drag(),
    );
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            st.probe_wb = (plot.inv_x(pos.x) * 10.0).round() / 10.0;
        }
    }
    let px = plot.pt(probe, plot.y.0).x;
    p.line_segment(
        [pos2(px, plot.rect.top()), pos2(px, plot.rect.bottom())],
        Stroke::new(1.4, t::INK),
    );
    // the probe's handle: a tab above the frame, with its value
    let tab = Rect::from_center_size(pos2(px, plot.rect.top() - 13.0), vec2(86.0, 24.0));
    p.rect_filled(tab, kit::r(12), t::INK);
    text(
        &p,
        tab.center(),
        Align2::CENTER_CENTER,
        &format!("{probe:.1} °C wb"),
        kit::num(11.5),
        t::BG,
    );
    let mut readings = Vec::new();
    for (fi, fpct) in data::CURVE_FLOW_PCT.iter().enumerate() {
        let flow = design_flow * fpct / 100.0;
        if let Some((cold, extra)) = data::predict_cold(&curves, probe, range, flow) {
            let q = plot.pt(probe, cold);
            let qr = if env.phone { 4.5 } else { 6.0 };
            p.circle_filled(q, qr, t::BG);
            p.circle_stroke(q, qr, Stroke::new(2.4, FLOW_COLS[fi]));
            readings.push((fi, *fpct, cold, extra));
        }
    }

    // ---- inverse lookup: the target cold water -> the flow that gives it at the probe
    let target = st.target_cold.unwrap_or(draft.duty.target_cold_water_c);
    let inv = data::predict_flow(&curves, probe, range, target);
    if let Some((flow, _cold)) = inv {
        let ty = plot.pt(plot.x.0, target).y;
        if ty > plot.rect.top() && ty < plot.rect.bottom() {
            kit::dashed(
                &p,
                pos2(plot.rect.left(), ty),
                pos2(px, ty),
                Stroke::new(1.4, t::INK_2),
                6.0,
                4.0,
            );
            p.circle_filled(pos2(px, ty), 5.0, t::INK);
            kit::label_plate(
                &p,
                pos2(plot.rect.left() + 6.0, ty - 4.0),
                Align2::LEFT_BOTTOM,
                &format!("target {target:.1} °C"),
                kit::mono(10.5),
                t::INK,
            );
        }
        let _ = flow;
    }

    // ---- the side panel
    kit::glass(&p, side_r, 12);
    let ir = side_r.shrink(16.0);
    let mut y = ir.top();
    let h = text(
        &p,
        pos2(ir.left(), y),
        Align2::LEFT_TOP,
        &format!("At {probe:.1} °C wet bulb"),
        kit::semi(12.5),
        t::INK_2,
    );
    if kit::info_dot(
        ui,
        pos2(ir.right() - 8.0, h.center().y),
        "curves.i",
        st.info.as_deref() == Some("curves:i"),
    )
    .clicked()
    {
        toggle_info(st, "curves:i");
    }
    y = h.bottom() + 10.0;
    let rows = 3;
    let cw = ir.width() / rows as f32;
    for (k, (fi, fpct, cold, extra)) in readings.iter().enumerate() {
        let x = ir.left() + cw * k as f32;
        p.circle_filled(pos2(x + 4.0, y + 7.0), 4.0, FLOW_COLS[*fi]);
        text(
            &p,
            pos2(x + 13.0, y),
            Align2::LEFT_TOP,
            &format!("{fpct:.0}% flow"),
            kit::semi(10.5),
            t::INK_2,
        );
        let v = text(
            &p,
            pos2(x, y + 18.0),
            Align2::LEFT_TOP,
            &format!("{cold:.2}"),
            kit::num(if env.phone { 17.0 } else { 20.0 }),
            if *extra { t::DANGER } else { t::INK },
        );
        let u = text(
            &p,
            pos2(v.right() + 3.0, v.bottom() - 3.0),
            Align2::LEFT_BOTTOM,
            "°C",
            kit::sans(10.5),
            t::INK_2,
        );
        kit::src_mark(
            &p,
            pos2(u.right() + 4.0, v.center().y),
            Src::Calc,
            "predict_cold_water_from_performance_curves · engine-built grid",
        );
    }
    y += 60.0;
    p.line_segment(
        [pos2(ir.left(), y), pos2(ir.right(), y)],
        Stroke::new(1.0, t::LINE_SOFT),
    );
    y += 14.0;
    // inverse
    text(
        &p,
        pos2(ir.left(), y),
        Align2::LEFT_TOP,
        "Flow for a target cold water",
        kit::semi(12.5),
        t::INK_2,
    );
    y += 24.0;
    let bw = 48.0;
    let minus = Rect::from_min_size(pos2(ir.left(), y), vec2(bw, 44.0));
    let plus = Rect::from_min_size(pos2(ir.left() + bw + 110.0, y), vec2(bw, 44.0));
    if kit::button(ui, minus, "curves.t-", "−", kit::Btn::Ghost).clicked() {
        st.target_cold = Some(((target - 0.5) * 2.0).round() / 2.0);
    }
    if kit::button(ui, plus, "curves.t+", "+", kit::Btn::Ghost).clicked() {
        st.target_cold = Some(((target + 0.5) * 2.0).round() / 2.0);
    }
    let tv = text(
        &p,
        pos2(minus.right() + 55.0, minus.center().y),
        Align2::CENTER_CENTER,
        &format!("{target:.1}"),
        kit::num(20.0),
        t::INK,
    );
    text(
        &p,
        pos2(tv.right() + 3.0, tv.bottom() - 3.0),
        Align2::LEFT_BOTTOM,
        "°C",
        kit::sans(10.5),
        t::INK_2,
    );
    kit::src_mark(
        &p,
        pos2(tv.left() - 12.0, tv.top() - 2.0),
        Src::Catalog,
        if st.target_cold.is_none() {
            "duty · target cold water (recorded)"
        } else {
            "your entry (− / +), from the duty's target"
        },
    );
    let rx = plus.right() + 14.0;
    match inv {
        Some((flow, _)) => {
            let pct = 100.0 * flow / design_flow;
            let fr = text(
                &p,
                pos2(rx, y - 2.0),
                Align2::LEFT_TOP,
                &format!("{:.0}", flow * 3600.0 / data::density(draft)),
                kit::num(20.0),
                t::PRIMARY,
            );
            let mu = text(
                &p,
                pos2(fr.right() + 3.0, fr.bottom() - 3.0),
                Align2::LEFT_BOTTOM,
                "m³/h",
                kit::sans(10.5),
                t::INK_2,
            );
            kit::src_mark(
                &p,
                pos2(mu.right() + 4.0, fr.center().y),
                Src::Calc,
                "predict_water_flow_from_performance_curves · engine-built grid",
            );
            text(
                &p,
                pos2(rx, fr.bottom() + 2.0),
                Align2::LEFT_TOP,
                &format!("{pct:.0}% of design"),
                kit::mono(10.5),
                t::MUTED,
            );
        }
        None => {
            text(
                &p,
                pos2(rx, y + 12.0),
                Align2::LEFT_TOP,
                "out of the grid",
                kit::sans(11.5),
                t::DANGER,
            );
        }
    }
    y += 64.0;
    if !env.phone {
        p.line_segment(
            [pos2(ir.left(), y), pos2(ir.right(), y)],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        y += 14.0;
        // the table under the chart: cold water at each wet bulb, one column per flow, this range
        let hd = text(
            &p,
            pos2(ir.left(), y),
            Align2::LEFT_TOP,
            &format!("{range:.0} K range, every record"),
            kit::semi(12.5),
            t::INK_2,
        );
        kit::src_mark(
            &p,
            pos2(hd.right() + 6.0, hd.center().y),
            Src::Calc,
            "Engine::run · one run per cell, hot water iterated to the range",
        );
        y = hd.bottom() + 10.0;
        let cx = [
            ir.left(),
            ir.left() + 70.0,
            ir.left() + 145.0,
            ir.left() + 220.0,
        ];
        text(
            &p,
            pos2(cx[0], y),
            Align2::LEFT_TOP,
            "wb °C",
            kit::semi(10.5),
            t::MUTED,
        );
        for (fi, fpct) in data::CURVE_FLOW_PCT.iter().enumerate() {
            p.circle_filled(pos2(cx[fi + 1] + 4.0, y + 7.0), 3.5, FLOW_COLS[fi]);
            text(
                &p,
                pos2(cx[fi + 1] + 12.0, y),
                Align2::LEFT_TOP,
                &format!("{fpct:.0}%"),
                kit::semi(10.5),
                t::MUTED,
            );
        }
        y += 22.0;
        for wb in data::CURVE_WB.iter() {
            if y > ir.bottom() - 40.0 {
                break;
            }
            let near = (wb - probe).abs() < 1.0;
            if near {
                p.rect_filled(
                    Rect::from_min_size(
                        pos2(ir.left() - 6.0, y - 3.0),
                        vec2(ir.width() + 12.0, 22.0),
                    ),
                    kit::r(4),
                    t::PANEL_RAISED,
                );
            }
            text(
                &p,
                pos2(cx[0], y),
                Align2::LEFT_TOP,
                &format!("{wb:.0}"),
                kit::num(12.5),
                t::INK_2,
            );
            for (fi, fpct) in data::CURVE_FLOW_PCT.iter().enumerate() {
                let flow = design_flow * fpct / 100.0;
                let v = recs
                    .iter()
                    .find(|r| (r.wb - wb).abs() < 1e-6 && (r.flow_kg_s - flow).abs() < 1e-6)
                    .map(|r| format!("{:.2}", r.cold))
                    .unwrap_or("–".into());
                text(
                    &p,
                    pos2(cx[fi + 1], y),
                    Align2::LEFT_TOP,
                    &v,
                    kit::num(12.5),
                    t::INK,
                );
            }
            y += 24.0;
        }
        let (done, n) = st.cache.curves_progress();
        let foot = if done < n {
            format!("engine · performance_curve.rs · {done}/{n} records")
        } else {
            format!(
                "engine · {} runs · {:.0} s · range ±{:.2} K",
                curves.runs,
                curves.ms / 1000.0,
                data::CURVE_TOL
            )
        };
        text(
            &p,
            pos2(ir.left(), ir.bottom()),
            Align2::LEFT_BOTTOM,
            &foot,
            kit::mono(10.5),
            t::MUTED,
        );
        let _ = curves.recs.len();
    }
    if let (Some((ax, ay, kw, _)), Some(rr)) = (op_vals, ro) {
        fan_readout(&p, rr, ax, ay, kw, false);
    }
    info_card(
        ui,
        st,
        "curves:i",
        pos2(ir.right() - 8.0, ir.top() + 8.0),
        &[
            "Performance curves",
            "Each point is one engine run of this",
            "tower at that wet bulb, range and flow.",
            "Drag the line to read across; red",
            "means outside the recorded grid.",
        ],
        area,
    );
    let _: Option<&CurveData> = None;
}

/// Fan vs system with the operating point (round 2: the Instrument's Operating point tab, merged here).
/// Every series is this frame's `Engine::run` of the draft: the fan curve at the draft speed, the tower's
/// air-path system curve, and the engine's own crossing.
fn fan_system(
    p: &egui::Painter,
    r: Rect,
    d: &EngineInput,
    env: &Env,
    phone: bool,
) -> Option<(f64, f64, f64, Rect)> {
    let Some(o) = env.out else {
        text(
            p,
            r.center(),
            Align2::CENTER_CENTER,
            "no run: the engine refused this draft",
            kit::sans(12.5),
            t::DANGER,
        );
        return None;
    };
    let fs = &o.fan_system_curve;
    let op = fs.operating_point;
    let head = text(
        p,
        r.left_top(),
        Align2::LEFT_TOP,
        "Fan vs system",
        kit::semi(13.0),
        t::INK,
    );
    let sub = text(
        p,
        pos2(head.right() + 10.0, head.center().y + 1.0),
        Align2::LEFT_CENTER,
        &format!(
            "{} at {:.2}× · the operating point is the engine's crossing",
            d.fan.id, d.speed_ratio
        ),
        kit::sans(11.0),
        t::MUTED,
    );
    kit::src_mark(
        p,
        pos2(sub.right() + 6.0, sub.center().y),
        Src::Catalog,
        "draft · fan record + speed ratio",
    );
    // the readout: under the head on a phone, top-right on desktop
    let read_h = 0.0;
    let plot_r = Rect::from_min_max(
        pos2(r.left(), head.bottom() + if phone { 74.0 } else { 14.0 }),
        pos2(r.right(), r.bottom() - read_h),
    );
    let (mut x1, mut y1) = (0.0f64, 0.0f64);
    for s in [&fs.fan, &fs.system] {
        for q in &s.points {
            x1 = x1.max(q.x);
        }
    }
    // the y range: what the system curve reaches inside the x range, and the fan up to it
    for q in fs.system.points.iter().chain(fs.fan.points.iter()) {
        if q.x <= x1 {
            y1 = y1.max(q.y);
        }
    }
    if x1 <= 0.0 || y1 <= 0.0 {
        return None;
    }
    let plot = Plot {
        rect: Rect::from_min_max(
            pos2(plot_r.left() + 44.0, plot_r.top() + 18.0),
            pos2(plot_r.right() - 10.0, plot_r.bottom() - 40.0),
        ),
        x: (0.0, x1 * 1.04),
        y: (0.0, y1 * 1.08),
    };
    let xs = super::rate::nice(x1 / if phone { 4.0 } else { 7.0 });
    let ys = super::rate::nice(y1 / 4.0);
    plot.axes(p, xs, ys, "airflow m³/s", "total pressure Pa", 0, 0);
    let fan: Vec<(f64, f64)> = fs.fan.points.iter().map(|q| (q.x, q.y)).collect();
    let sys: Vec<(f64, f64)> = fs.system.points.iter().map(|q| (q.x, q.y)).collect();
    plot.line(p, &sys, Stroke::new(2.2, t::AIR));
    plot.line(p, &fan, Stroke::new(2.6, t::PRIMARY));
    // series names on plates, clear of their own curves: the fan at its left end (above it), the
    // system at its right end (above the rising line, which is what crossed the plate before)
    if let Some(q) = fan.first() {
        kit::label_plate(
            p,
            plot.pt(q.0, q.1) + vec2(0.0, -8.0),
            Align2::CENTER_BOTTOM,
            "fan",
            kit::semi(11.0),
            t::PRIMARY,
        );
    }
    if let Some(q) = sys
        .iter()
        .rev()
        .find(|q| q.0 <= plot.x.1 && q.1 <= plot.y.1)
    {
        kit::label_plate(
            p,
            plot.pt(q.0, q.1) + vec2(-4.0, -10.0),
            Align2::RIGHT_BOTTOM,
            "tower system",
            kit::semi(11.0),
            t::AIR,
        );
    }
    // the operating point
    let c = plot.pt(op.x, op.y);
    kit::dashed(
        p,
        c,
        pos2(c.x, plot.rect.bottom()),
        Stroke::new(1.0, t::INK_2),
        4.0,
        4.0,
    );
    kit::dashed(
        p,
        c,
        pos2(plot.rect.left(), c.y),
        Stroke::new(1.0, t::INK_2),
        4.0,
        4.0,
    );
    p.circle_filled(c, 8.0, t::BG);
    p.circle_stroke(c, 7.0, Stroke::new(2.6, t::INK));
    p.circle_filled(c, 3.0, t::INK);
    // the readout of the crossing is drawn by the caller - on desktop it sits in the side panel, whose
    // glass is painted after this chart (round-2 QA: drawn here it came out washed out)
    Some((
        op.x,
        op.y,
        o.fan_power_kw,
        Rect::from_min_size(pos2(r.left(), head.bottom() + 10.0), vec2(r.width(), 56.0)),
    ))
}

/// The crossing's readout: glass chip on a phone, a titled block in the side panel's foot on desktop.
fn fan_readout(p: &egui::Painter, br: Rect, ax: f64, ay: f64, kw: f64, phone: bool) {
    if phone {
        kit::glass_soft(p, br, 8);
    } else {
        p.line_segment(
            [
                br.left_top() - vec2(0.0, 8.0),
                br.right_top() - vec2(0.0, 8.0),
            ],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        text(
            p,
            br.left_top(),
            Align2::LEFT_TOP,
            "Operating point",
            kit::semi(12.5),
            t::INK,
        );
    }
    let figs = [
        ("airflow", format!("{ax:.1}"), "m³/s"),
        ("total pressure", format!("{ay:.1}"), "Pa"),
        ("fan power", format!("{kw:.1}"), "kW"),
    ];
    let (fx, fy, cw) = if phone {
        (br.left() + 12.0, br.top() + 8.0, (br.width() - 20.0) / 3.0)
    } else {
        (br.left(), br.top() + 26.0, br.width() / 3.0)
    };
    for (i, (l, v, u)) in figs.iter().enumerate() {
        kit::metric_src(
            p,
            pos2(fx + cw * i as f32, fy),
            l,
            v,
            u,
            15.0,
            t::INK,
            Src::Calc,
            "Engine::run · fan/system intersection",
        );
    }
    if !phone {
        kit::text_fit(
            p,
            pos2(br.left(), fy + 54.0),
            Align2::LEFT_TOP,
            "The answer's cold water is this run, at its own hot water;",
            kit::sans(11.0),
            t::MUTED,
            br.width(),
        );
        kit::text_fit(
            p,
            pos2(br.left(), fy + 70.0),
            Align2::LEFT_TOP,
            "the curves iterate hot water to an exact range.",
            kit::sans(11.0),
            t::MUTED,
            br.width(),
        );
    }
}
