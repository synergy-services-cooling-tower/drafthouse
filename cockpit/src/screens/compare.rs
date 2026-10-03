//! **Compare** (#89): two or three variants side by side, the differences highlighted.
//!
//! Engine: the cockpit's own `Engine::run` (the adapter's `RealEngine`) on three edited copies of the draft
//! (`data::variants`): A as drafted, B with the lower fill layer 0.3 m deeper, C with the fan 0.10 faster
//! inside its own speed band. Every value is a field of that variant's `EngineOutput`. Tap a column's head
//! to make it the baseline; every other cell then shows its difference from it, and the cell that changed
//! most in each row is lit.

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Shape, Stroke, StrokeKind};
use cockpit::engine::{EngineInput, EngineOutput};

use super::data::Variant;
use super::kit::{self, text, text_fit, Plot, Src};
use super::{info_card, title_band, toast, toggle_info, Env, State};
use crate::theme as t;

/// A delta that is worse than the baseline. Not DANGER: the theme keeps red for refusals and limits.
const WORSE: Color32 = Color32::from_rgb(0xd9, 0x8f, 0x8f);

/// (label, unit, decimals, value, lower is better?)
type Row = (
    &'static str,
    &'static str,
    usize,
    fn(&EngineOutput) -> f64,
    Option<bool>,
);

const ROWS: [Row; 8] = [
    ("cold water", "°C", 2, |o| o.cold_water_c, Some(true)),
    ("capability", "%", 1, |o| o.capability_pct, Some(false)),
    ("approach", "K", 2, |o| o.approach_c, Some(true)),
    ("fan power", "kW", 1, |o| o.fan_power_kw, Some(true)),
    ("airflow", "m³/s", 1, |o| o.airflow_m3_s, None),
    ("pressure", "Pa", 1, |o| o.total_pressure_pa, Some(true)),
    ("KaV/L", "fill", 3, |o| o.kavl_total, Some(false)),
    ("make-up", "m³/h", 2, |o| o.makeup_m3_hr, Some(true)),
];

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Compare",
        if env.phone {
            "three variants · what changes"
        } else {
            "three variants · differences highlighted"
        },
        env.phone,
    );
    let p = ui.painter().clone();
    let Some(vars) = st.cache.variants.clone() else {
        text(
            &p,
            body.center(),
            Align2::CENTER_CENTER,
            "running the variants",
            kit::sans(13.0),
            t::MUTED,
        );
        return;
    };
    let base = st.base.min(vars.len().saturating_sub(1));
    let pad = if env.phone { 10.0 } else { 24.0 };
    let inner = body.shrink2(vec2(pad, 12.0));
    let label_w = if env.phone { 84.0 } else { 150.0 };
    let n = vars.len();
    let gap = if env.phone { 6.0 } else { 16.0 };
    let col_w = (inner.width() - label_w - gap * n as f32) / n as f32;
    // phone: the heads use the full width (the label column has nothing to say beside them)
    let head_w = if env.phone {
        (inner.width() - gap * (n as f32 - 1.0)) / n as f32
    } else {
        col_w
    };
    let head_x0 = if env.phone {
        inner.left()
    } else {
        inner.left() + label_w + gap
    };
    // round 2 (#91 decisions, "Compare = the same charts side by side"): a band of fan-vs-system charts
    // under the heads, one per variant on shared axes, between the heads and the table
    let head_h = if env.phone { 150.0 } else { 236.0 };
    let band_h = if env.phone { 104.0 } else { 150.0 };
    let foot = if env.phone { 56.0 } else { 64.0 };
    let row_h = ((inner.bottom() - foot - (inner.top() + head_h + band_h + 24.0))
        / ROWS.len() as f32)
        .clamp(26.0, 56.0);

    // ---- the column heads: a section each, the changed part lit
    for (i, v) in vars.iter().enumerate() {
        let x = head_x0 + (head_w + gap) * i as f32;
        let r = Rect::from_min_size(pos2(x, inner.top()), vec2(head_w, head_h));
        let resp = kit::hit(ui, r, &format!("cmp.head.{i}"));
        let is_base = i == base;
        p.rect_filled(
            r,
            kit::r(12),
            if is_base { t::PANEL_RAISED } else { t::PANEL },
        );
        p.rect_stroke(
            r,
            kit::r(12),
            Stroke::new(
                if is_base { 1.5 } else { 1.0 },
                if is_base {
                    t::INK_2
                } else if resp.hovered() {
                    t::LINE
                } else {
                    t::LINE_SOFT
                },
            ),
            StrokeKind::Inside,
        );
        let k = text(
            &p,
            pos2(r.left() + 12.0, r.top() + 12.0),
            Align2::LEFT_TOP,
            v.key,
            kit::num(if env.phone { 15.0 } else { 18.0 }),
            if is_base { t::INK } else { t::PRIMARY },
        );
        if !env.phone || head_w > 96.0 {
            text_fit(
                &p,
                pos2(k.right() + 8.0, r.top() + 14.0),
                Align2::LEFT_TOP,
                &v.label,
                kit::semi(if env.phone { 11.5 } else { 13.5 }),
                t::INK,
                r.right() - k.right() - 18.0,
            );
        }
        let sec = Rect::from_min_max(
            pos2(r.left() + 6.0, r.top() + 40.0),
            pos2(
                r.right() - 6.0,
                r.bottom() - if env.phone { 46.0 } else { 40.0 },
            ),
        );
        section(
            &p,
            sec,
            &v.input,
            &vars[base].input,
            env.t,
            v.out.as_ref().ok(),
        );
        // the change line (two lines on a phone: what, then how much)
        let cl = if is_base {
            "baseline".to_string()
        } else {
            v.change.clone()
        };
        let col = if is_base { t::INK_2 } else { t::PRIMARY };
        if env.phone {
            let cut = cl.find(|c: char| c.is_ascii_digit()).unwrap_or(cl.len());
            let (a, b) = cl.split_at(cut);
            let b = b
                .split_once('→')
                .map(|x| format!("→ {}", x.1.trim()))
                .unwrap_or_else(|| b.trim().to_string());
            text_fit(
                &p,
                pos2(r.left() + 10.0, r.bottom() - 40.0),
                Align2::LEFT_TOP,
                a.trim(),
                kit::sans(11.5),
                col,
                r.width() - 16.0,
            );
            text_fit(
                &p,
                pos2(r.left() + 10.0, r.bottom() - 22.0),
                Align2::LEFT_TOP,
                &b,
                kit::num(12.5),
                col,
                r.width() - 16.0,
            );
        } else {
            text_fit(
                &p,
                pos2(r.left() + 14.0, r.bottom() - 30.0),
                Align2::LEFT_TOP,
                &cl,
                kit::semi(12.5),
                col,
                r.width() - 28.0,
            );
        }
        if resp.clicked() {
            st.base = i;
        }
    }
    // label column head: the hint + ⓘ
    let lh = Rect::from_min_size(inner.min, vec2(label_w, head_h));
    if !env.phone {
        let mut y = lh.top() + 12.0;
        let lit = Rect::from_min_size(pos2(lh.left(), y), vec2(26.0, 18.0));
        p.rect_filled(lit, kit::r(4), t::PRIMARY_SOFT);
        p.rect_stroke(
            lit,
            kit::r(4),
            Stroke::new(1.0, t::PRIMARY_DEEP),
            StrokeKind::Inside,
        );
        text(
            &p,
            pos2(lit.right() + 8.0, lit.center().y),
            Align2::LEFT_CENTER,
            "best in row",
            kit::sans(12.0),
            t::INK_2,
        );
        y += 30.0;
        for (col, l) in [
            (t::OK, "better"),
            (WORSE, "worse"),
            (t::INK_2, "neither (airflow)"),
        ] {
            arrow(&p, pos2(lh.left() + 8.0, y + 8.0), true, col);
            text(
                &p,
                pos2(lh.left() + 34.0, y + 8.0),
                Align2::LEFT_CENTER,
                l,
                kit::sans(12.0),
                t::INK_2,
            );
            y += 24.0;
        }
        text(
            &p,
            pos2(lh.left(), y + 2.0),
            Align2::LEFT_TOP,
            "than the baseline;",
            kit::sans(11.5),
            t::MUTED,
        );
        text(
            &p,
            pos2(lh.left(), y + 18.0),
            Align2::LEFT_TOP,
            "arrow = up or down",
            kit::sans(11.5),
            t::MUTED,
        );
        y += 16.0;
        y += 34.0;
        let outlined = Rect::from_min_size(pos2(lh.left(), y), vec2(26.0, 14.0));
        p.rect_stroke(
            outlined,
            kit::r(0),
            Stroke::new(2.0, t::PRIMARY),
            StrokeKind::Inside,
        );
        text(
            &p,
            pos2(outlined.right() + 8.0, outlined.center().y),
            Align2::LEFT_CENTER,
            "changed part",
            kit::sans(12.0),
            t::INK_2,
        );
    }
    let idot = if env.phone {
        pos2(inner.left() + 12.0, inner.top() + head_h + 16.0)
    } else {
        pos2(lh.left() + 10.0, lh.bottom() - 18.0)
    };
    if kit::info_dot(ui, idot, "cmp.i", st.info.as_deref() == Some("cmp:i")).clicked() {
        toggle_info(st, "cmp:i");
    }

    // ---- the chart band
    let band = Rect::from_min_size(
        pos2(inner.left(), inner.top() + head_h + 12.0),
        vec2(inner.width(), band_h),
    );
    chart_band(
        &p, band, &vars, base, label_w, gap, col_w, head_x0, head_w, env.phone,
    );

    // ---- the rows
    let top = band.bottom() + 12.0;
    let base_out = vars[base].out.as_ref().ok();
    for (ri, (label, unit, dec, get, lower_better)) in ROWS.iter().enumerate() {
        let y = top + row_h * ri as f32;
        if y + row_h > inner.bottom() - foot + 4.0 {
            break;
        }
        if ri % 2 == 0 {
            p.rect_filled(
                Rect::from_min_size(
                    pos2(inner.left() - 6.0, y),
                    vec2(inner.width() + 12.0, row_h),
                ),
                kit::r(6),
                t::with_alpha(t::PANEL, 120),
            );
        }
        let lr = text(
            &p,
            pos2(
                inner.left(),
                y + row_h / 2.0 - if unit.is_empty() { 0.0 } else { 7.0 },
            ),
            Align2::LEFT_CENTER,
            label,
            kit::semi(if env.phone { 11.0 } else { 12.5 }),
            t::INK_2,
        );
        if !unit.is_empty() {
            text(
                &p,
                pos2(inner.left(), lr.bottom() + 1.0),
                Align2::LEFT_TOP,
                unit,
                kit::sans(10.5),
                t::MUTED,
            );
        }
        let deltas: Vec<Option<f64>> = vars
            .iter()
            .map(|v| match (v.out.as_ref().ok(), base_out) {
                (Some(o), Some(b)) => Some(get(o) - get(b)),
                _ => None,
            })
            .collect();
        // the best value in the row (lit), when the row has a better direction and the values differ
        let vals: Vec<Option<f64>> = vars.iter().map(|v| v.out.as_ref().ok().map(get)).collect();
        let tol = 0.5 * 10f64.powi(-(*dec as i32));
        let spread = {
            let f: Vec<f64> = vals.iter().flatten().copied().collect();
            f.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - f.iter().copied().fold(f64::INFINITY, f64::min)
        };
        let most = lower_better.filter(|_| spread > tol).and_then(|lb| {
            vals.iter()
                .enumerate()
                .filter_map(|(i, v)| v.map(|x| (i, x)))
                .min_by(|a, b| {
                    if lb {
                        a.1.total_cmp(&b.1)
                    } else {
                        b.1.total_cmp(&a.1)
                    }
                })
                .map(|x| x.0)
        });
        for (i, v) in vars.iter().enumerate() {
            let x = inner.left() + label_w + gap + (col_w + gap) * i as f32;
            let cell = Rect::from_min_size(pos2(x, y + 3.0), vec2(col_w, row_h - 6.0));
            let Some(o) = v.out.as_ref().ok() else {
                text(
                    &p,
                    cell.center(),
                    Align2::CENTER_CENTER,
                    "refused",
                    kit::sans(11.0),
                    t::DANGER,
                );
                continue;
            };
            let val = get(o);
            if Some(i) == most {
                p.rect_filled(cell, kit::r(6), t::PRIMARY_SOFT);
                p.rect_stroke(
                    cell,
                    kit::r(6),
                    Stroke::new(1.0, t::PRIMARY_DEEP),
                    StrokeKind::Inside,
                );
            }
            let vr0 = text(
                &p,
                pos2(
                    cell.left() + if env.phone { 6.0 } else { 10.0 },
                    cell.center().y,
                ),
                Align2::LEFT_CENTER,
                &format!("{val:.dec$}", dec = *dec),
                kit::num(if env.phone { 13.0 } else { 16.0 }),
                t::INK,
            );
            let mk = kit::src_mark(
                &p,
                pos2(vr0.right(), vr0.center().y),
                Src::Calc,
                &format!("Engine::run · variant {} · {label}", v.key),
            );
            let vr = vr0.union(mk);
            if i != base {
                if let Some(d) = deltas[i] {
                    let tiny = d.abs() < 0.5 * 10f64.powi(-(*dec as i32));
                    if !tiny {
                        let good = lower_better.map(|lb| if lb { d < 0.0 } else { d > 0.0 });
                        let col = match good {
                            Some(true) => t::OK,
                            Some(false) => WORSE,
                            None => t::INK_2,
                        };
                        let ax = if env.phone {
                            cell.right() - 8.0
                        } else {
                            vr.right() + 10.0
                        };
                        let s = format!(
                            "{}{:.dec$}",
                            if d > 0.0 { "+" } else { "−" },
                            d.abs(),
                            dec = *dec
                        );
                        if env.phone {
                            arrow(&p, pos2(cell.right() - 8.0, cell.center().y), d > 0.0, col);
                        } else {
                            arrow(&p, pos2(ax, cell.center().y), d > 0.0, col);
                            text(
                                &p,
                                pos2(ax + 10.0, cell.center().y),
                                Align2::LEFT_CENTER,
                                &s,
                                kit::num(13.0),
                                col,
                            );
                        }
                    }
                }
            }
        }
    }

    // ---- the footer: the action (design only), under the table
    let fr = if env.phone {
        Rect::from_min_size(
            pos2(inner.left(), inner.bottom() - 46.0),
            vec2(inner.width(), 44.0),
        )
    } else {
        Rect::from_min_size(
            pos2(inner.right() - 260.0, inner.bottom() - 46.0),
            vec2(260.0, 44.0),
        )
    };
    let bi = best(&vars);
    if kit::button(
        ui,
        fr,
        "cmp.keep",
        &format!("Keep {} as the draft", vars[bi].key),
        kit::Btn::Ghost,
    )
    .clicked()
    {
        toast(st, "Design only - the draft is not changed");
    }
    if !env.phone {
        text(
            &p,
            pos2(fr.left() - 16.0, fr.center().y),
            Align2::RIGHT_CENTER,
            &format!(
                "{} cools best: {} °C",
                vars[bi].key,
                vars[bi]
                    .out
                    .as_ref()
                    .map(|o| format!("{:.2}", o.cold_water_c))
                    .unwrap_or_default()
            ),
            kit::sans(12.0),
            t::INK_2,
        );
    }
    info_card(
        ui,
        st,
        "cmp:i",
        if env.phone {
            idot
        } else {
            pos2(inner.left() + 10.0 + 250.0, inner.top() + head_h - 18.0)
        },
        &[
            "Compare",
            "Each column is a full engine run.",
            "Tap a head to make it the baseline;",
            "the lit cell is the row's best.",
        ],
        area,
    );
    let _ = draft;
}

/// One fan-vs-system chart per variant, on one pair of axes ranges so the columns compare; the baseline's
/// operating point is ghosted on every other column, so the move reads at a glance.
#[allow(clippy::too_many_arguments)]
fn chart_band(
    p: &egui::Painter,
    band: Rect,
    vars: &[Variant],
    base: usize,
    label_w: f32,
    gap: f32,
    col_w: f32,
    head_x0: f32,
    head_w: f32,
    phone: bool,
) {
    let outs: Vec<Option<&EngineOutput>> = vars.iter().map(|v| v.out.as_ref().ok()).collect();
    let (mut x1, mut y1) = (0.0f64, 0.0f64);
    for o in outs.iter().flatten() {
        let fs = &o.fan_system_curve;
        // the window that holds every crossing, with room: 1.6x the furthest operating point
        x1 = x1.max(fs.operating_point.x * 1.6);
        y1 = y1.max(fs.operating_point.y * 1.8);
    }
    if x1 <= 0.0 || y1 <= 0.0 {
        return;
    }
    if !phone {
        text(
            p,
            pos2(band.left(), band.top() + 6.0),
            Align2::LEFT_TOP,
            "Fan vs system",
            kit::semi(12.5),
            t::INK_2,
        );
        let mut y = band.top() + 30.0;
        for (col, l, dot) in [
            (t::PRIMARY, "fan", false),
            (t::AIR, "tower system", false),
            (t::INK, "operating point", true),
            (t::MUTED, "baseline's point", true),
        ] {
            if dot {
                p.circle_stroke(pos2(band.left() + 8.0, y + 7.0), 4.0, Stroke::new(1.6, col));
            } else {
                p.line_segment(
                    [
                        pos2(band.left(), y + 7.0),
                        pos2(band.left() + 16.0, y + 7.0),
                    ],
                    Stroke::new(2.0, col),
                );
            }
            text(
                p,
                pos2(band.left() + 24.0, y + 7.0),
                Align2::LEFT_CENTER,
                l,
                kit::sans(11.5),
                t::INK_2,
            );
            y += 20.0;
        }
        text(
            p,
            pos2(band.left(), band.bottom()),
            Align2::LEFT_BOTTOM,
            "same axes in every column",
            kit::sans(11.0),
            t::MUTED,
        );
    }
    let x0 = if phone {
        head_x0
    } else {
        band.left() + label_w + gap
    };
    let w = if phone { head_w } else { col_w };
    let bo = outs.get(base).copied().flatten();
    for (i, o) in outs.iter().enumerate() {
        let cell = Rect::from_min_size(
            pos2(x0 + (w + gap) * i as f32, band.top()),
            vec2(w, band.height()),
        );
        kit::glass_soft(p, cell, 8);
        let Some(o) = o else {
            text(
                p,
                cell.center(),
                Align2::CENTER_CENTER,
                "refused",
                kit::sans(11.0),
                t::DANGER,
            );
            continue;
        };
        let plot = Plot {
            rect: Rect::from_min_max(
                cell.min + vec2(if phone { 8.0 } else { 34.0 }, 10.0),
                cell.max - vec2(8.0, if phone { 18.0 } else { 22.0 }),
            ),
            x: (0.0, x1),
            y: (0.0, y1),
        };
        p.rect_filled(plot.rect, kit::r(0), t::with_alpha(t::BG, 140));
        p.rect_stroke(
            plot.rect,
            kit::r(0),
            Stroke::new(1.0, t::LINE_SOFT),
            StrokeKind::Inside,
        );
        if !phone {
            for (v, a) in [(0.0, Align2::RIGHT_BOTTOM), (y1, Align2::RIGHT_TOP)] {
                text(
                    p,
                    pos2(plot.rect.left() - 4.0, plot.pt(0.0, v).y),
                    a,
                    &format!("{v:.0}"),
                    kit::mono(9.5),
                    t::MUTED,
                );
            }
            text(
                p,
                pos2(plot.rect.left() - 4.0, plot.rect.center().y),
                Align2::RIGHT_CENTER,
                "Pa",
                kit::mono(9.5),
                t::MUTED,
            );
        }
        text(
            p,
            pos2(plot.rect.right(), plot.rect.bottom() + 3.0),
            Align2::RIGHT_TOP,
            &format!("{x1:.0} m³/s"),
            kit::mono(9.5),
            t::MUTED,
        );
        let fs = &o.fan_system_curve;
        let fan: Vec<(f64, f64)> = fs.fan.points.iter().map(|q| (q.x, q.y)).collect();
        let sys: Vec<(f64, f64)> = fs.system.points.iter().map(|q| (q.x, q.y)).collect();
        plot.line(p, &sys, Stroke::new(1.6, t::AIR));
        plot.line(p, &fan, Stroke::new(2.0, t::PRIMARY));
        if let (Some(b), true) = (bo, i != base) {
            let g = plot.pt(
                b.fan_system_curve.operating_point.x,
                b.fan_system_curve.operating_point.y,
            );
            let c = plot.pt(fs.operating_point.x, fs.operating_point.y);
            // round-2 QA: a baseline point under this one reads as a smudge - a short tail from it instead
            if g.distance(c) >= 9.0 {
                p.circle_stroke(g, 4.5, Stroke::new(1.4, t::MUTED));
                kit::dashed(p, g, c, Stroke::new(1.0, t::MUTED), 3.0, 3.0);
            }
        }
        let c = plot.pt(fs.operating_point.x, fs.operating_point.y);
        p.circle_filled(c, 5.5, t::BG);
        p.circle_stroke(c, 5.0, Stroke::new(2.0, t::INK));
        if !phone {
            // above the crossing, centred: the fan falls up-left and the system rises up-right, so the
            // wedge straight above the point is the one open region (round-2 QA: both side quadrants sit
            // on a curve)
            let lr = kit::label_plate(
                p,
                pos2(
                    c.x.clamp(plot.rect.left() + 70.0, plot.rect.right() - 70.0),
                    c.y - 9.0,
                ),
                Align2::CENTER_BOTTOM,
                &format!(
                    "{:.0} m³/s · {:.0} Pa",
                    fs.operating_point.x, fs.operating_point.y
                ),
                kit::mono(10.0),
                t::INK,
            );
            kit::src_mark(
                p,
                pos2(lr.right(), lr.center().y),
                Src::Calc,
                "Engine::run · fan/system intersection of this variant",
            );
        }
    }
}

/// The variant with the lowest cold water (the one that cools best).
fn best(vars: &[Variant]) -> usize {
    vars.iter()
        .enumerate()
        .filter_map(|(i, v)| v.out.as_ref().ok().map(|o| (i, o.cold_water_c)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
        .unwrap_or(0)
}

fn arrow(p: &egui::Painter, c: Pos2, up: bool, col: Color32) {
    let s = 5.5;
    let pts = if up {
        vec![
            c + vec2(0.0, -s),
            c + vec2(s, s * 0.6),
            c + vec2(-s, s * 0.6),
        ]
    } else {
        vec![
            c + vec2(0.0, s),
            c + vec2(s, -s * 0.6),
            c + vec2(-s, -s * 0.6),
        ]
    };
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// A small tower section: the fan stack, the eliminator, the fill layers to scale, the basin. Parts that
/// differ from the baseline input are outlined in the primary colour; the fan turns at the variant's speed.
fn section(
    p: &egui::Painter,
    r: Rect,
    inp: &EngineInput,
    base: &EngineInput,
    tt: f32,
    out: Option<&EngineOutput>,
) {
    let total: f64 = inp
        .fill_layers
        .iter()
        .map(|l| l.depth_m)
        .sum::<f64>()
        .max(0.1);
    let max_total = 3.2;
    let w = r.width().min(r.height() * 1.3);
    let body = Rect::from_center_size(
        pos2(r.center().x, r.center().y + 10.0),
        vec2(w * 0.86, r.height() * 0.74),
    );
    p.rect_filled(body, kit::r(3), t::with_alpha(t::BG, 160));
    p.rect_stroke(
        body,
        kit::r(3),
        Stroke::new(1.0, t::LINE),
        StrokeKind::Inside,
    );
    // fan stack
    let stack = Rect::from_center_size(
        pos2(body.center().x, body.top() - 10.0),
        vec2(body.width() * 0.55, 18.0),
    );
    let fan_changed =
        (inp.speed_ratio - base.speed_ratio).abs() > 1e-6 || inp.fan.id != base.fan.id;
    p.add(Shape::convex_polygon(
        vec![
            pos2(stack.left() + 6.0, stack.top()),
            pos2(stack.right() - 6.0, stack.top()),
            stack.right_bottom(),
            stack.left_bottom(),
        ],
        t::PANEL_RAISED,
        Stroke::new(
            if fan_changed { 2.0 } else { 1.2 },
            if fan_changed { t::PRIMARY } else { t::INK_2 },
        ),
    ));
    let rpm = inp.speed_ratio as f32;
    // the fan side-on: one blade pair, foreshortened as it turns (a still frame reads as a blade)
    let c = stack.center();
    let a = tt * 4.0 * rpm;
    let reach = stack.width() * 0.34 * (0.4 + 0.6 * a.cos().abs());
    let col = if fan_changed { t::PRIMARY } else { t::AIR };
    p.line_segment(
        [c - vec2(reach, 0.0), c + vec2(reach, 0.0)],
        Stroke::new(2.4, col),
    );
    p.circle_filled(c, 3.0, col);
    // eliminator
    let de = Rect::from_min_size(
        pos2(body.left() + 4.0, body.top() + 6.0),
        vec2(body.width() - 8.0, 8.0),
    );
    let mut zig = Vec::new();
    for i in 0..=16 {
        let x = de.left() + de.width() * i as f32 / 16.0;
        zig.push(pos2(x, if i % 2 == 0 { de.top() } else { de.bottom() }));
    }
    p.add(Shape::line(zig, Stroke::new(1.0, t::INK_2)));
    // fill layers, depth to scale (top of the stack = first layer)
    let fill_top = de.bottom() + 16.0;
    let fill_h_max = body.bottom() - 26.0 - fill_top;
    let mut y = fill_top;
    for (li, l) in inp.fill_layers.iter().enumerate() {
        let h = fill_h_max * (l.depth_m / max_total) as f32;
        let fr = Rect::from_min_size(pos2(body.left() + 4.0, y), vec2(body.width() - 8.0, h));
        let changed = base
            .fill_layers
            .get(li)
            .map(|b| (b.depth_m - l.depth_m).abs() > 1e-6 || b.fill_id != l.fill_id)
            .unwrap_or(true);
        let col = if li == 0 {
            Color32::from_rgb(0x2f, 0xb3, 0xc9)
        } else {
            Color32::from_rgb(0x8f, 0x9c, 0xf0)
        };
        p.rect_filled(fr, kit::r(0), t::with_alpha(col, 70));
        for k in 1..10 {
            let x = fr.left() + fr.width() * k as f32 / 10.0;
            p.line_segment(
                [pos2(x, fr.top()), pos2(x, fr.bottom())],
                Stroke::new(0.6, t::with_alpha(col, 120)),
            );
        }
        if changed {
            p.rect_stroke(
                fr,
                kit::r(0),
                Stroke::new(2.0, t::PRIMARY),
                StrokeKind::Inside,
            );
        }
        // the chip sits to the right of a changed layer's outline, so the stroke stays visible
        let chip_at = if changed {
            pos2(fr.center().x + 26.0, fr.center().y)
        } else {
            fr.center()
        };
        if h > 15.0 {
            kit::label_plate(
                p,
                chip_at,
                Align2::CENTER_CENTER,
                &format!("{:.2} m", l.depth_m),
                kit::mono(10.5),
                if changed { t::PRIMARY } else { t::INK },
            );
        }
        y += h + 2.0;
    }
    let _ = total;
    // basin with the variant's cold water as its colour
    let basin = Rect::from_min_max(
        pos2(body.left() + 2.0, body.bottom() - 18.0),
        pos2(body.right() - 2.0, body.bottom() - 2.0),
    );
    let f = out
        .map(|o| {
            ((inp.duty.hot_water_c - o.cold_water_c)
                / (inp.duty.hot_water_c - inp.duty.wet_bulb_c).max(1e-6)) as f32
        })
        .unwrap_or(0.5);
    p.rect_filled(
        basin,
        kit::r(2),
        t::with_alpha(kit::water_temp(f.clamp(0.0, 1.0)), 170),
    );
    // water falling
    for i in 0..6 {
        let x = body.left() + body.width() * (i as f32 + 0.5) / 6.0;
        kit::flow_dots(
            p,
            &[pos2(x, fill_top - 6.0), pos2(x, basin.top())],
            tt * 40.0 + i as f32 * 7.0,
            16.0,
            1.3,
            t::with_alpha(t::WATER, 180),
        );
    }
}
