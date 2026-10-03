//! **Rate** (#83): field test points in, the fitted characteristic curve against the design demand, the
//! capability as a gauge.
//!
//! Engine: `capability::evaluate_characteristic_capability` (the demand and characteristic curves and their
//! intersection) and `capability::monte_carlo_characteristic_capability` (the uncertainty band on the
//! gauge). Design condition: the draft duty and the run's own dry-air flow. Test readings: STUB
//! (`data::STUB_TEST_POINTS`) - nothing in the fixture records a field test - and the measurement sigmas
//! (`data::STUB_SIGMA`); both carry the amber STUB tag where painted.

use bevy_egui::egui::{self, pos2, vec2, Align2, Pos2, Rect, Shape, Stroke};
use cockpit::engine::EngineInput;

use super::data::{self, RatePoint};
use super::kit::{self, text, Plot, Src};
use super::{info_card, title_band, toggle_info, Env, State};

/// The origin line for anything computed from the STUB test readings.
const ON_STUB: &str = "evaluate_characteristic_capability · on the STUB test readings";
use crate::theme as t;

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Rate",
        "test points in · capability out",
        env.phone,
    );
    let p = ui.painter().clone();
    let Some(rate) = st.cache.rate.clone() else {
        text(
            &p,
            body.center(),
            Align2::CENTER_CENTER,
            "running the capability evaluation",
            kit::sans(13.0),
            t::MUTED,
        );
        return;
    };
    if rate.points.is_empty() {
        text(
            &p,
            body.center(),
            Align2::CENTER_CENTER,
            "the real engine is not in this build",
            kit::sans(13.0),
            t::DANGER,
        );
        return;
    }
    let sel = st.test_point.min(rate.points.len() - 1);
    let pad = if env.phone { 12.0 } else { 24.0 };
    let inner = body.shrink2(vec2(pad, 12.0));

    // ---- layout
    let (gauge_r, chips_r, chart_r, read_r) = if env.phone {
        let g = Rect::from_min_size(inner.min, vec2(inner.width(), 250.0));
        let c = Rect::from_min_size(
            pos2(inner.left(), g.bottom() + 8.0),
            vec2(inner.width(), 76.0),
        );
        let ch = Rect::from_min_max(
            pos2(inner.left(), c.bottom() + 14.0),
            pos2(inner.right(), inner.bottom() - 6.0),
        );
        (g, c, ch, Rect::NOTHING)
    } else {
        let side = 360.0;
        let g = Rect::from_min_size(pos2(inner.right() - side, inner.top()), vec2(side, 340.0));
        let c = Rect::from_min_size(pos2(g.left(), g.bottom() + 14.0), vec2(side, 96.0));
        let r = Rect::from_min_max(
            pos2(g.left(), c.bottom() + 14.0),
            pos2(inner.right(), inner.bottom()),
        );
        let ch = Rect::from_min_max(inner.min, pos2(g.left() - 24.0, inner.bottom()));
        (g, c, ch, r)
    };

    // ---- the gauge: the hero
    kit::glass(&p, gauge_r, 14);
    let pt = rate.points[sel].as_ref();
    let gc = pos2(gauge_r.center().x, gauge_r.top() + gauge_r.height() * 0.58);
    let rad = (gauge_r.height() * 0.36).min(gauge_r.width() * 0.36);
    match pt {
        Ok(rp) => {
            let v = rp.capability_pct as f32;
            let col = if rp.capability_pct >= 100.0 {
                t::OK
            } else {
                t::DANGER
            };
            let band = rp.band.map(|(a, b)| (a as f32, b as f32));
            // an eased needle so a change of test point swings rather than jumps
            let shown = if env.motion {
                kit::ease(ui.ctx(), "rate.needle", v, 0.45)
            } else {
                v
            };
            kit::gauge(&p, gc, rad, 70.0, 130.0, shown, 100.0, None, col);
            if let Some((a, b)) = band {
                // the Monte Carlo band rides outside the dial, amber: its sigmas are STUB
                let a0 = 150f32.to_radians();
                let sw = 240f32.to_radians();
                let ang = |x: f32| a0 + sw * ((x - 70.0) / 60.0).clamp(0.0, 1.0);
                let rb = rad + (rad * 0.13).max(8.0) * 0.95;
                let pts: Vec<egui::Pos2> = (0..=40)
                    .map(|i| {
                        let an = ang(a) + (ang(b) - ang(a)) * i as f32 / 40.0;
                        gc + vec2(an.cos(), an.sin()) * rb
                    })
                    .collect();
                p.add(Shape::line(
                    pts,
                    Stroke::new(4.0, t::with_alpha(t::AMBER, 170)),
                ));
            }
            let big = text(
                &p,
                pos2(gc.x, gc.y + rad * 0.28),
                Align2::CENTER_CENTER,
                &format!("{:.1}", rp.capability_pct),
                kit::num(if env.phone { 34.0 } else { 44.0 }),
                t::INK,
            );
            let pc = text(
                &p,
                pos2(big.right() + 3.0, big.bottom() - 6.0),
                Align2::LEFT_BOTTOM,
                "%",
                kit::sans(if env.phone { 18.0 } else { 22.0 }),
                t::INK_2,
            );
            kit::src_mark(
                &p,
                pos2(pc.right() + 5.0, big.center().y),
                Src::Illus,
                ON_STUB,
            );
            let capl = text(
                &p,
                pos2(gc.x, big.bottom() + 6.0),
                Align2::CENTER_TOP,
                "capability",
                kit::semi(11.5),
                t::MUTED,
            );
            if let Some((a, b)) = rp.band {
                let bs = format!("{a:.1} – {b:.1} %");
                let bw = kit::measure(&p, &bs, kit::mono(11.0)).x
                    + 10.0
                    + kit::measure(&p, "STUB", kit::semi(10.0)).x
                    + 16.0;
                // below both the label and the dial's end labels (which sit at 0.5 rad + 18 .. +32)
                let by = (capl.bottom() + 8.0).max(gc.y + rad * 0.5 + 36.0);
                let br = text(
                    &p,
                    pos2(gc.x - bw / 2.0, by),
                    Align2::LEFT_TOP,
                    &bs,
                    kit::mono(11.0),
                    t::AMBER,
                );
                let sg = kit::stub_tag(
                    &p,
                    pos2(br.right() + 10.0, br.center().y),
                    Align2::LEFT_CENTER,
                );
                kit::src_mark(
                    &p,
                    pos2(sg.right() + 6.0, sg.center().y),
                    Src::Illus,
                    "monte_carlo_characteristic_capability · STUB sigmas",
                );
            }
            // scale labels at the dial's own ends, just below them
            let e0 = 150f32.to_radians();
            let e1 = 30f32.to_radians();
            text(
                &p,
                gc + vec2(e0.cos(), e0.sin()) * rad + vec2(0.0, 18.0),
                Align2::CENTER_TOP,
                "70",
                kit::mono(10.5),
                t::INK_2,
            );
            text(
                &p,
                gc + vec2(e1.cos(), e1.sin()) * rad + vec2(0.0, 18.0),
                Align2::CENTER_TOP,
                "130",
                kit::mono(10.5),
                t::INK_2,
            );
            let a100 = 270f32.to_radians();
            text(
                &p,
                gc + vec2(a100.cos(), a100.sin()) * (rad + (rad * 0.13).max(8.0) * 1.6 + 10.0),
                Align2::CENTER_BOTTOM,
                "100",
                kit::mono(11.0),
                t::INK,
            );
        }
        Err(e) => {
            kit::gauge(&p, gc, rad, 70.0, 130.0, 70.0, 100.0, None, t::MUTED);
            text(&p, gc, Align2::CENTER_CENTER, e, kit::sans(12.0), t::DANGER);
        }
    }
    if kit::info_dot(
        ui,
        pos2(gauge_r.right() - 18.0, gauge_r.top() + 18.0),
        "rate.i",
        st.info.as_deref() == Some("rate:i"),
    )
    .clicked()
    {
        toggle_info(st, "rate:i");
    }

    // ---- test-point chips (STUB readings)
    let head = text(
        &p,
        pos2(chips_r.left(), chips_r.top()),
        Align2::LEFT_TOP,
        "Test readings",
        kit::semi(12.0),
        t::INK_2,
    );
    kit::stub_tag(
        &p,
        pos2(head.right() + 8.0, head.center().y),
        Align2::LEFT_CENTER,
    );
    let n = rate.points.len();
    let cw = (chips_r.width() - 8.0 * (n - 1) as f32) / n as f32;
    for i in 0..n {
        let r = Rect::from_min_size(
            pos2(chips_r.left() + (cw + 8.0) * i as f32, head.bottom() + 8.0),
            vec2(cw, chips_r.bottom() - head.bottom() - 8.0),
        );
        let resp = kit::hit(ui, r, &format!("rate.test.{i}"));
        let on = i == sel;
        p.rect_filled(
            r,
            kit::r(10),
            if on {
                t::PRIMARY_SOFT
            } else if resp.hovered() {
                t::PANEL_RAISED
            } else {
                t::PANEL
            },
        );
        p.rect_stroke(
            r,
            kit::r(10),
            Stroke::new(
                if on { 1.5 } else { 1.0 },
                if on { t::PRIMARY } else { t::LINE_SOFT },
            ),
            egui::StrokeKind::Inside,
        );
        let (_fw, _fa, hot, cold, _wb, _db) = data::STUB_TEST_POINTS[i];
        text(
            &p,
            pos2(r.left() + 12.0, r.top() + 10.0),
            Align2::LEFT_TOP,
            &format!("T{}", i + 1),
            kit::semi(13.0),
            if on { t::INK } else { t::INK_2 },
        );
        let cap = rate.points[i].as_ref().map(|x| x.capability_pct).ok();
        if let Some(c) = cap {
            let cr = text(
                &p,
                pos2(r.right() - 22.0, r.top() + 9.0),
                Align2::RIGHT_TOP,
                &format!("{c:.0}%"),
                kit::num(14.0),
                if c >= 100.0 { t::OK } else { t::DANGER },
            );
            kit::src_mark(
                &p,
                pos2(cr.right() + 4.0, cr.center().y),
                Src::Illus,
                ON_STUB,
            );
        }
        kit::text_fit(
            &p,
            pos2(r.left() + 12.0, r.bottom() - 10.0),
            Align2::LEFT_BOTTOM,
            &format!("{hot:.1} → {cold:.1} °C"),
            kit::mono(11.0),
            t::INK_2,
            r.width() - 20.0,
        );
        if resp.clicked() {
            st.test_point = i;
        }
    }

    // ---- the chart: demand vs characteristic over L/G
    chart(&p, chart_r, &rate.points, sel, env.phone);

    // ---- desktop read-out under the chips
    if !env.phone {
        if let Ok(rp) = pt {
            kit::glass_soft(&p, read_r, 10);
            let ir = read_r.shrink(16.0);
            let cw = ir.width() / 2.0;
            let rows: [(&str, String, &str, Src, &str); 4] = [
                (
                    "design L/G",
                    format!("{:.3}", rp.design_lg),
                    "",
                    Src::Calc,
                    "evaluate_characteristic_capability · the draft duty + the run's dry air",
                ),
                (
                    "capability L/G",
                    format!("{:.3}", rp.cap_lg),
                    "",
                    Src::Illus,
                    ON_STUB,
                ),
                (
                    "test L/G",
                    format!("{:.3}", rp.test_lg),
                    "",
                    Src::Illus,
                    ON_STUB,
                ),
                (
                    "dry air",
                    format!("{:.1}", rate.dry_air_kg_s),
                    "kg/s",
                    Src::Calc,
                    "Engine::run · dry-air mass flow (worked step)",
                ),
            ];
            for (i, (l, v, u, s, o)) in rows.iter().enumerate() {
                kit::metric_src(
                    &p,
                    pos2(
                        ir.left() + cw * (i % 2) as f32,
                        ir.top() + 46.0 * (i / 2) as f32,
                    ),
                    l,
                    v,
                    u,
                    17.0,
                    if i == 1 { t::PRIMARY } else { t::INK },
                    *s,
                    o,
                );
            }
            let mut y = ir.top() + 100.0;
            p.line_segment(
                [pos2(ir.left(), y), pos2(ir.right(), y)],
                Stroke::new(1.0, t::LINE_SOFT),
            );
            y += 12.0;
            let h = text(
                &p,
                pos2(ir.left(), y),
                Align2::LEFT_TOP,
                &format!("T{} readings", sel + 1),
                kit::semi(12.0),
                t::INK_2,
            );
            kit::stub_tag(&p, pos2(h.right() + 8.0, h.center().y), Align2::LEFT_CENTER);
            y = h.bottom() + 10.0;
            let (fw, fa, hot, cold, wb, db) = data::STUB_TEST_POINTS[sel];
            let cw3 = ir.width() / 3.0;
            let readings: [(&str, String, &str); 6] = [
                ("water flow", format!("{:.0}", fw * 100.0), "%"),
                ("air flow", format!("{:.0}", fa * 100.0), "%"),
                ("wet bulb", format!("{wb:.1}"), "°C"),
                ("hot", format!("{hot:.1}"), "°C"),
                ("cold", format!("{cold:.1}"), "°C"),
                ("dry bulb", format!("{db:.1}"), "°C"),
            ];
            for (i, (l, v, u)) in readings.iter().enumerate() {
                if y + 44.0 * (i / 3) as f32 + 40.0 > ir.bottom() - 18.0 {
                    break;
                }
                kit::metric_src(
                    &p,
                    pos2(ir.left() + cw3 * (i % 3) as f32, y + 44.0 * (i / 3) as f32),
                    l,
                    v,
                    u,
                    15.0,
                    t::AMBER,
                    Src::Illus,
                    "data::STUB_TEST_POINTS · no field test is recorded",
                );
            }
            text(
                &p,
                pos2(ir.left(), ir.bottom()),
                Align2::LEFT_BOTTOM,
                &format!(
                    "engine · capability.rs · {} samples",
                    data::STUB_SIGMA_SAMPLES
                ),
                kit::mono(10.5),
                t::MUTED,
            );
        }
    }

    info_card(
        ui,
        st,
        "rate:i",
        pos2(gauge_r.right() - 18.0, gauge_r.top() + 18.0),
        &[
            "Capability",
            "Where the test's characteristic curve",
            "meets the design demand: that L/G over",
            "the design L/G. 100 % meets the duty.",
            "Band: Monte Carlo on stub sigmas.",
        ],
        area,
    );
    let _ = draft;
}

/// Round 2 (#91 decisions, "Rate = test points vs the design curve"): the one design demand curve, every
/// test point's fitted characteristic through it (the selected one bold), each test point and where its
/// characteristic meets the demand.
fn chart(p: &egui::Painter, r: Rect, pts: &[Result<RatePoint, String>], sel: usize, phone: bool) {
    let Some(Ok(rp)) = pts.get(sel) else {
        return;
    };
    if rp.curve.is_empty() {
        return;
    }
    // round-2 QA: the full curve range (L/G ~0.8-2.2) packs every test point into a few pixels. The chart
    // shows the window that matters - design, capability and every test L/G, with room either side - and the
    // y range of the curves inside it.
    let (cx0, cx1) = rp
        .curve
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, q| {
            (a.0.min(q.0), a.1.max(q.0))
        });
    let keys: Vec<f64> = pts
        .iter()
        .flatten()
        .flat_map(|q| [q.design_lg, q.cap_lg, q.test_lg])
        .filter(|v| v.is_finite())
        .collect();
    let lo = keys.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = keys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let half = ((hi - lo) * 1.6).max(0.18);
    let mid = (lo + hi) / 2.0;
    let (x0, x1) = ((mid - half).max(cx0), (mid + half).min(cx1));
    let (mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY);
    for q in pts.iter().flatten() {
        for (lg, d, c) in &q.curve {
            if *lg < x0 || *lg > x1 {
                continue;
            }
            for v in [d, c] {
                if v.is_finite() {
                    y0 = y0.min(*v);
                    y1 = y1.max(*v);
                }
            }
        }
    }
    let ypad = (y1 - y0) * 0.08;
    let plot = Plot {
        rect: Rect::from_min_max(
            pos2(r.left() + 40.0, r.top() + 26.0),
            pos2(
                r.right() - 8.0,
                r.bottom() - if phone { 70.0 } else { 40.0 },
            ),
        ),
        x: (x0, x1),
        y: ((y0 - ypad).max(0.0), y1 + ypad),
    };
    let xs = nice((x1 - x0) / if phone { 4.0 } else { 6.0 });
    let ys = nice((plot.y.1 - plot.y.0) / 5.0);
    plot.axes(p, xs, ys, "L/G (zoomed to the test points)", "KaV/L", 2, 2);
    // The fitted characteristic's own constants (issue #83): KaV/L = C · (L/G)^m through the test
    // point, read straight from `evaluate_characteristic_capability`. Its test anchor is the STUB
    // reading set, so the mark is illustrative; the demand side is the draft duty.
    let fit = text(
        p,
        pos2(r.left() + 40.0, r.top() + 13.0),
        Align2::LEFT_CENTER,
        &format!("fitted C {:.4} · m {:.3}", rp.test_c, rp.exponent_m),
        kit::mono(11.5),
        t::INK_2,
    );
    kit::src_mark(
        p,
        pos2(fit.right() + 6.0, fit.center().y),
        Src::Illus,
        ON_STUB,
    );
    let demand: Vec<(f64, f64)> = rp.curve.iter().map(|(x, d, _)| (*x, *d)).collect();
    let charac: Vec<(f64, f64)> = rp.curve.iter().map(|(x, _, c)| (*x, *c)).collect();
    plot.line(p, &demand, Stroke::new(2.4, t::INK_2));
    // the other test points' characteristics, faint, each with its diamond and name
    for (i, q) in pts.iter().enumerate() {
        let Ok(q) = q else { continue };
        if i == sel {
            continue;
        }
        let c: Vec<(f64, f64)> = q.curve.iter().map(|(x, _, c)| (*x, *c)).collect();
        plot.line(p, &c, Stroke::new(1.3, t::with_alpha(t::PRIMARY, 90)));
        if let Some(yt) = interp(q.test_lg, &c) {
            let d = plot.pt(q.test_lg, yt);
            diamond(p, d, 5.0, t::with_alpha(t::AMBER, 150));
            kit::label_plate(
                p,
                d + vec2(0.0, 9.0),
                Align2::CENTER_TOP,
                &format!("T{}", i + 1),
                kit::semi(10.5),
                t::INK_2,
            );
        }
    }
    plot.line(p, &charac, Stroke::new(2.6, t::PRIMARY));
    // the design L/G and the capability L/G as verticals
    let vline = |x: f64, col: egui::Color32, dash: bool| {
        let a = plot.pt(x, plot.y.0);
        let b = pos2(a.x, plot.rect.top());
        if dash {
            kit::dashed(p, a, b, Stroke::new(1.2, col), 5.0, 4.0);
        } else {
            p.line_segment([a, b], Stroke::new(1.4, col));
        }
    };
    if rp.design_lg >= x0 && rp.design_lg <= x1 {
        vline(rp.design_lg, t::MUTED, true);
        kit::label_plate(
            p,
            plot.pt(rp.design_lg, plot.y.1) + vec2(4.0, 6.0),
            Align2::LEFT_TOP,
            "design",
            kit::mono(10.5),
            t::INK_2,
        );
    }
    let at = |x: f64, pts: &[(f64, f64)]| -> Option<f64> {
        pts.windows(2)
            .find(|w| (w[0].0 - x) * (w[1].0 - x) <= 0.0)
            .map(|w| {
                let f = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
                w[0].1 + (w[1].1 - w[0].1) * f
            })
    };
    if let Some(yc) = at(rp.cap_lg, &charac) {
        vline(rp.cap_lg, t::with_alpha(t::PRIMARY, 140), false);
        kit::label_plate(
            p,
            plot.pt(rp.cap_lg, plot.y.1) + vec2(-4.0, 6.0),
            Align2::RIGHT_TOP,
            "capability",
            kit::mono(10.5),
            t::PRIMARY,
        );
        let q = plot.pt(rp.cap_lg, yc);
        p.circle_filled(q, 6.0, t::BG);
        p.circle_stroke(q, 6.0, Stroke::new(2.4, t::PRIMARY));
    }
    if let Some(yt) = at(rp.test_lg, &charac) {
        // the test point: amber, because its readings are STUB; exactly at its L/G (round 2: no nudge, the
        // zoomed window gives it room), with a BG ring so it stays legible on the lines.
        let q = plot.pt(rp.test_lg, yt);
        p.circle_filled(q, 9.0, t::BG);
        p.add(Shape::convex_polygon(
            vec![
                q + vec2(0.0, -7.0),
                q + vec2(7.0, 0.0),
                q + vec2(0.0, 7.0),
                q + vec2(-7.0, 0.0),
            ],
            t::AMBER,
            Stroke::new(1.5, t::BG),
        ));
        kit::label_plate(
            p,
            q + vec2(0.0, -11.0),
            Align2::CENTER_BOTTOM,
            &format!("T{}", sel + 1),
            kit::semi(11.0),
            t::INK,
        );
    }
    if phone {
        // legend as one row under the axis title
        let y = r.bottom() - 10.0;
        let mut x = r.left() + 4.0;
        for (name, col, dia) in [
            ("fitted", t::PRIMARY, false),
            ("demand", t::INK_2, false),
            ("test", t::AMBER, true),
        ] {
            if dia {
                let q = pos2(x + 6.0, y - 7.0);
                p.add(Shape::convex_polygon(
                    vec![
                        q + vec2(0.0, -5.0),
                        q + vec2(5.0, 0.0),
                        q + vec2(0.0, 5.0),
                        q + vec2(-5.0, 0.0),
                    ],
                    col,
                    Stroke::NONE,
                ));
                x += 16.0;
            } else {
                p.line_segment(
                    [pos2(x, y - 7.0), pos2(x + 16.0, y - 7.0)],
                    Stroke::new(2.6, col),
                );
                x += 22.0;
            }
            let lr = text(
                p,
                pos2(x, y),
                Align2::LEFT_BOTTOM,
                name,
                kit::sans(11.5),
                t::INK_2,
            );
            x = lr.right() + 16.0;
        }
        return;
    }
    // legend, top right inside the plot
    let lx = plot.rect.right() - 12.0;
    let ly = plot.rect.top() + 12.0;
    let lg = Rect::from_min_max(pos2(lx - 222.0, ly - 8.0), pos2(lx + 8.0, ly + 60.0));
    p.rect_filled(lg, kit::r(6), t::PANEL_RAISED);
    p.rect_stroke(
        lg,
        kit::r(6),
        Stroke::new(1.0, t::LINE),
        egui::StrokeKind::Inside,
    );
    let l1 = text(
        p,
        pos2(lx, ly),
        Align2::RIGHT_TOP,
        "fitted characteristic (selected)",
        kit::sans(11.0),
        t::INK,
    );
    p.line_segment(
        [
            pos2(l1.left() - 22.0, l1.center().y),
            pos2(l1.left() - 6.0, l1.center().y),
        ],
        Stroke::new(2.6, t::PRIMARY),
    );
    let l2 = text(
        p,
        pos2(lx, l1.bottom() + 6.0),
        Align2::RIGHT_TOP,
        "design demand",
        kit::sans(11.0),
        t::INK,
    );
    p.line_segment(
        [
            pos2(l2.left() - 22.0, l2.center().y),
            pos2(l2.left() - 6.0, l2.center().y),
        ],
        Stroke::new(2.4, t::INK_2),
    );
    let l3 = text(
        p,
        pos2(lx, l2.bottom() + 6.0),
        Align2::RIGHT_TOP,
        "test points T1–T3",
        kit::sans(11.0),
        t::INK,
    );
    let q = pos2(l3.left() - 14.0, l3.center().y);
    p.add(Shape::convex_polygon(
        vec![
            q + vec2(0.0, -5.0),
            q + vec2(5.0, 0.0),
            q + vec2(0.0, 5.0),
            q + vec2(-5.0, 0.0),
        ],
        t::AMBER,
        Stroke::NONE,
    ));
}

fn interp(x: f64, pts: &[(f64, f64)]) -> Option<f64> {
    pts.windows(2)
        .find(|w| (w[0].0 - x) * (w[1].0 - x) <= 0.0)
        .map(|w| {
            let f = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
            w[0].1 + (w[1].1 - w[0].1) * f
        })
}

fn diamond(p: &egui::Painter, q: Pos2, s: f32, col: egui::Color32) {
    p.add(Shape::convex_polygon(
        vec![
            q + vec2(0.0, -s),
            q + vec2(s, 0.0),
            q + vec2(0.0, s),
            q + vec2(-s, 0.0),
        ],
        col,
        Stroke::new(1.2, t::BG),
    ));
}

pub fn nice(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let m = 10f64.powf(raw.log10().floor());
    let f = raw / m;
    let n = if f < 1.5 {
        1.0
    } else if f < 3.5 {
        2.0
    } else if f < 7.5 {
        5.0
    } else {
        10.0
    };
    n * m
}

#[allow(dead_code)]
fn _p(_: Pos2) {}
