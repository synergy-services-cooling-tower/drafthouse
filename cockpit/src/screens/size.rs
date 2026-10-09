//! **Size** (#83): the duty in, the ranked candidate towers out, as 3D-ish tiles; why a tower was
//! rejected on tap.
//!
//! Engine: `selection::run_selection` over the fixture catalog, one tower per call (so the work is sliced
//! across frames, `data::Cache::step`), ranked by the engine's own objective (least over-capacity). Every
//! number on a card is a field of the engine's `SelectionCandidate`; the rejection reasons are the run's
//! own `rejection_summary` strings.

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Shape, Stroke, StrokeKind};
use cockpit::engine::EngineInput;

use super::data::{self, Cand, TowerRun};
use super::kit::{self, text, text_fit, Plot, Src};

/// Where every candidate number on this screen comes from.
const SEL: &str = "this size, calculated at the duty";
use super::{info_card, title_band, toast, toggle_info, Env, Screen, State};
use crate::state;
use crate::theme as t;

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Size",
        "the duty in · the towers that meet it",
        env.phone,
    );
    let p = ui.painter().clone();
    let pad = if env.phone { 12.0 } else { 24.0 };

    // ---- the duty strip
    let strip_h = if env.phone { 64.0 } else { 74.0 };
    let strip = Rect::from_min_size(
        pos2(body.left() + pad, body.top() + 10.0),
        vec2(body.width() - 2.0 * pad, strip_h),
    );
    duty_strip(&p, strip, draft, env.phone);

    // issue #138: the `Arc` the cache holds - a pointer copy; the ranked list is computed once with it
    let Some(size) = st.cache.size.clone() else {
        let (done, n) = st.cache.size_progress();
        progress(&p, body.center(), done, n, "running the selection");
        return;
    };
    if size.towers.is_empty() {
        text(
            &p,
            body.center(),
            Align2::CENTER_CENTER,
            "The selection could not run on this catalog",
            kit::sans(13.0),
            t::DANGER,
        );
        return;
    }
    let ranked = &size.by_capacity;
    let towers = &size.towers;
    let total_feasible: usize = towers.iter().map(|t| t.feasible).sum();
    let total_rejected: usize = towers
        .iter()
        .flat_map(|t| t.rejections.iter().map(|r| r.1))
        .sum();

    let below = strip.bottom() + 14.0;
    if env.phone {
        phone_layout(
            ui,
            st,
            &p,
            Rect::from_min_max(
                pos2(body.left() + pad, below),
                pos2(body.right() - pad, body.bottom() - 8.0),
            ),
            ranked,
            towers,
            total_feasible,
            total_rejected,
            env,
        );
    } else {
        desk_layout(
            ui,
            st,
            &p,
            Rect::from_min_max(
                pos2(body.left() + pad, below),
                pos2(body.right() - pad, body.bottom() - 16.0),
            ),
            ranked,
            towers,
            total_feasible,
            total_rejected,
            size.ms,
            env,
        );
    }
    // a picked candidate (scatter point or card): its engine line and its tower's rejections
    if let (Some(i), None) = (st.cand, st.info.as_ref()) {
        let list: Vec<&Cand> = ranked
            .iter()
            .filter(|c| st.cand_filter().is_none_or(|x| c.crossflow == x))
            .collect();
        if let Some(c) = list.get(i).copied() {
            let tw = towers.iter().find(|x| x.id == c.tower_id).cloned();
            let c = c.clone();
            // the desk layout's side column (same arithmetic as desk_layout)
            let side = Rect::from_min_max(
                pos2(body.right() - pad - 330.0, strip.bottom() + 14.0),
                pos2(body.right() - pad, body.bottom() - 16.0),
            );
            pick_sheet(ui, st, area, side, i, &c, tw.as_ref(), draft, env);
        }
    }
    let all_reasons = aggregate(towers);
    if st.info.as_deref() == Some("rej") {
        rejection_sheet(ui, st, area, &all_reasons, total_rejected, env.phone);
    }
    if let Some(id) = st
        .info
        .clone()
        .and_then(|s| s.strip_prefix("tower:").map(str::to_string))
    {
        if let Some(tw) = towers.iter().find(|x| x.id == id) {
            tower_sheet(ui, st, area, tw, env.phone);
        }
    }
}

fn duty_strip(p: &egui::Painter, r: Rect, d: &EngineInput, phone: bool) {
    kit::glass_soft(p, r, 10);
    let items: [(&str, String, &str); 5] = [
        ("flow", t::num::flow(d.duty.water_flow_m3_hr), "m³/h"),
        ("hot", format!("{:.1}", d.duty.hot_water_c), "°C"),
        ("cold", format!("{:.1}", d.duty.target_cold_water_c), "°C"),
        ("wet bulb", format!("{:.1}", d.duty.wet_bulb_c), "°C"),
        (
            "cycles",
            format!("{:.1}", d.duty.cycles_of_concentration),
            "×",
        ),
    ];
    let n = if phone { 4 } else { 5 };
    let w = r.width() / n as f32;
    for (i, (l, v, u)) in items.iter().take(n).enumerate() {
        let at = pos2(
            r.left() + 14.0 + w * i as f32,
            r.top() + if phone { 10.0 } else { 14.0 },
        );
        kit::metric_src(
            p,
            at,
            l,
            v,
            u,
            if phone { 17.0 } else { 22.0 },
            t::INK,
            Src::Catalog,
            "the duty, as entered",
        );
        if i > 0 {
            p.line_segment(
                [
                    pos2(at.x - 14.0, r.top() + 12.0),
                    pos2(at.x - 14.0, r.bottom() - 12.0),
                ],
                Stroke::new(1.0, t::LINE_SOFT),
            );
        }
    }
}

/// The progress state of a cold path whose work is sliced across frames: the ring, `done/n` and what
/// is running. Issue #151: the comparison's file open paints it too.
pub(super) fn progress(p: &egui::Painter, c: Pos2, done: usize, n: usize, what: &str) {
    let f = if n == 0 { 0.0 } else { done as f32 / n as f32 };
    let pts: Vec<Pos2> = (0..=48)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * f * i as f32 / 48.0;
            c + vec2(a.cos(), a.sin()) * 26.0
        })
        .collect();
    p.circle_stroke(c, 26.0, Stroke::new(4.0, t::LINE_SOFT));
    p.add(Shape::line(pts, Stroke::new(4.0, t::PRIMARY)));
    text(
        p,
        c,
        Align2::CENTER_CENTER,
        &format!("{done}/{n}"),
        kit::num(13.0),
        t::INK,
    );
    text(
        p,
        c + vec2(0.0, 44.0),
        Align2::CENTER_CENTER,
        what,
        kit::sans(12.0),
        t::MUTED,
    );
}

fn aggregate(towers: &[TowerRun]) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = Vec::new();
    for t in towers {
        for (r, n) in &t.rejections {
            match v.iter_mut().find(|x| &x.0 == r) {
                Some(x) => x.1 += n,
                None => v.push((r.clone(), *n)),
            }
        }
    }
    v.sort_by_key(|b| std::cmp::Reverse(b.1));
    v
}

/// The engine's rejection key, in a few words a person reads.
pub fn reason_label(r: &str) -> &str {
    match r {
        "thermal duty" => "misses the cold-water target",
        "power limit" => "over the fan power limit",
        "drift limit" => "over the drift limit",
        "drift velocity range" => "air speed off the eliminator curve",
        "fill operating envelope" => "outside the fill's loading envelope",
        "motor size unavailable" => "no motor size fits",
        "drift material temperature" => "eliminator material too hot",
        "fill not in the catalog" => "fill record missing",
        "fill tower-type compatibility" => "fill not made for this tower type",
        "fill water-quality compatibility" => "fill not rated for this water",
        "numerical or curve domain" => "outside a curve's recorded range",
        other => other,
    }
}

// ================================================================================= the tile

/// A 3D-ish tower tile: an isometric box sized by the footprint, a fan shroud on top, the fill band and
/// (crossflow) louvres on both air faces. `h` is the fill depth.
/// The tile's (width, height, centre-y offset from `c`) at scale 1, so a caller can fit and centre it.
fn tile_dims(footprint_m2: f64, depth_m: f64, crossflow: bool) -> (f32, f32, f32) {
    let side = (footprint_m2.sqrt() as f32) * 6.2;
    let (wx, wy) = if crossflow {
        (side * 1.25, side * 0.8)
    } else {
        (side, side)
    };
    let h = 46.0 + depth_m as f32 * 26.0;
    let w = 0.866 * (wx + wy);
    let top = -(wx + wy) * 0.5 - h - 7.0;
    let bottom = (wx + wy) * 0.5;
    (w, bottom - top, (top + bottom) / 2.0)
}

/// A tile fitted into `r` at `scale` (shared across a grid so footprints compare), centred.
#[allow(clippy::too_many_arguments)]
fn tile_in(
    p: &egui::Painter,
    r: Rect,
    scale: f32,
    footprint_m2: f64,
    depth_m: f64,
    crossflow: bool,
    accent: Color32,
    fill: Color32,
    dim: bool,
) {
    let (_, _, cy) = tile_dims(footprint_m2, depth_m, crossflow);
    tile(
        p,
        pos2(r.center().x, r.center().y - cy * scale),
        scale,
        footprint_m2,
        depth_m,
        crossflow,
        accent,
        fill,
        dim,
    );
}

#[allow(clippy::too_many_arguments)]
fn tile(
    p: &egui::Painter,
    c: Pos2,
    scale: f32,
    footprint_m2: f64,
    depth_m: f64,
    crossflow: bool,
    accent: Color32,
    fill: Color32,
    dim: bool,
) {
    let side = (footprint_m2.sqrt() as f32) * 6.2 * scale;
    let (wx, wy) = if crossflow {
        (side * 1.25, side * 0.8)
    } else {
        (side, side)
    };
    let h = (46.0 + depth_m as f32 * 26.0) * scale;
    let iso = |x: f32, y: f32, z: f32| pos2(c.x + (x - y) * 0.866, c.y + (x + y) * 0.5 - z);
    let hx = wx / 2.0;
    let hy = wy / 2.0;
    let a = if dim { 170 } else { 255 };
    let shade = |col: Color32, k: f32| -> Color32 {
        let [r, g, b, _] = col.to_array();
        Color32::from_rgba_unmultiplied(
            (r as f32 * k) as u8,
            (g as f32 * k) as u8,
            (b as f32 * k) as u8,
            a,
        )
    };
    let body = t::PANEL_RAISED;
    // shadow
    p.add(Shape::convex_polygon(
        vec![
            iso(-hx, -hy, 0.0) + vec2(6.0, 4.0),
            iso(hx, -hy, 0.0) + vec2(6.0, 4.0),
            iso(hx, hy, 0.0) + vec2(6.0, 4.0),
            iso(-hx, hy, 0.0) + vec2(6.0, 4.0),
        ],
        t::with_alpha(Color32::BLACK, if dim { 40 } else { 90 }),
        Stroke::NONE,
    ));
    // left face (x = -hx .. hx at y = hy), right face (y = -hy .. hy at x = hx)
    let left = vec![
        iso(-hx, hy, 0.0),
        iso(hx, hy, 0.0),
        iso(hx, hy, h),
        iso(-hx, hy, h),
    ];
    let right = vec![
        iso(hx, hy, 0.0),
        iso(hx, -hy, 0.0),
        iso(hx, -hy, h),
        iso(hx, hy, h),
    ];
    let top = vec![
        iso(-hx, -hy, h),
        iso(hx, -hy, h),
        iso(hx, hy, h),
        iso(-hx, hy, h),
    ];
    let edge = Stroke::new(1.0, t::with_alpha(accent, if dim { 90 } else { 150 }));
    p.add(Shape::convex_polygon(left.clone(), shade(body, 1.25), edge));
    p.add(Shape::convex_polygon(right.clone(), shade(body, 0.9), edge));
    p.add(Shape::convex_polygon(top, shade(body, 1.55), edge));
    // the fill band: on both visible faces, a band whose height is the fill depth share
    let f0 = h * 0.22;
    let f1 = f0 + (depth_m as f32 * 26.0 * scale).min(h * 0.62);
    let band = |pts: [Pos2; 4]| {
        p.add(Shape::convex_polygon(
            pts.to_vec(),
            t::with_alpha(fill, if dim { 40 } else { 95 }),
            Stroke::NONE,
        ))
    };
    band([
        iso(-hx, hy, f0),
        iso(hx, hy, f0),
        iso(hx, hy, f1),
        iso(-hx, hy, f1),
    ]);
    band([
        iso(hx, hy, f0),
        iso(hx, -hy, f0),
        iso(hx, -hy, f1),
        iso(hx, hy, f1),
    ]);
    // fill lines
    let n = 7;
    for i in 1..n {
        let k = i as f32 / n as f32;
        let x = -hx + wx * k;
        p.line_segment(
            [iso(x, hy, f0), iso(x, hy, f1)],
            Stroke::new(0.8, t::with_alpha(fill, if dim { 40 } else { 150 })),
        );
        let y = hy - wy * k;
        p.line_segment(
            [iso(hx, y, f0), iso(hx, y, f1)],
            Stroke::new(0.8, t::with_alpha(fill, if dim { 30 } else { 110 })),
        );
    }
    if crossflow {
        // louvres on the right (air) face
        for i in 0..6 {
            let z = f0 + (f1 - f0) * (i as f32 + 0.5) / 6.0;
            p.line_segment(
                [iso(hx, hy - 2.0, z), iso(hx, -hy + 2.0, z - 3.0 * scale)],
                Stroke::new(1.0, t::with_alpha(t::AIR, if dim { 40 } else { 140 })),
            );
        }
    } else {
        // air inlet slot at the bottom of both faces
        let z0 = 4.0 * scale;
        let z1 = f0 - 4.0 * scale;
        p.add(Shape::convex_polygon(
            vec![
                iso(-hx + 4.0, hy, z0),
                iso(hx - 4.0, hy, z0),
                iso(hx - 4.0, hy, z1),
                iso(-hx + 4.0, hy, z1),
            ],
            t::with_alpha(t::BG, 140),
            Stroke::NONE,
        ));
        p.add(Shape::convex_polygon(
            vec![
                iso(hx, hy - 4.0, z0),
                iso(hx, -hy + 4.0, z0),
                iso(hx, -hy + 4.0, z1),
                iso(hx, hy - 4.0, z1),
            ],
            t::with_alpha(t::BG, 120),
            Stroke::NONE,
        ));
    }
    // fan shroud: an ellipse ring on the top face
    let fc = iso(0.0, 0.0, h);
    let fr = hx.min(hy) * 0.72;
    let ring: Vec<Pos2> = (0..=40)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 40.0;
            iso(a.cos() * fr, a.sin() * fr, h)
        })
        .collect();
    let ring_top: Vec<Pos2> = ring.iter().map(|q| *q - vec2(0.0, 7.0 * scale)).collect();
    p.add(Shape::line(
        ring.clone(),
        Stroke::new(1.2, t::with_alpha(t::INK_2, if dim { 50 } else { 160 })),
    ));
    p.add(Shape::line(
        ring_top,
        Stroke::new(1.4, t::with_alpha(t::INK, if dim { 60 } else { 200 })),
    ));
    p.circle_filled(
        fc - vec2(0.0, 7.0 * scale),
        2.0 * scale.max(0.6),
        t::with_alpha(t::INK, a),
    );
}

// ================================================================================= desktop

/// The cards to show: the best candidate of each tower first (so every tower that fits is seen), then the
/// next closest fits in the engine's order. Each carries its engine rank (1-based, over the filtered list).
fn pick<'a>(list: &[&'a Cand], n: usize) -> Vec<(usize, &'a Cand)> {
    let mut out: Vec<(usize, &Cand)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (i, c) in list.iter().enumerate() {
        if !seen.contains(&c.tower_id.as_str()) {
            seen.push(&c.tower_id);
            out.push((i, c));
        }
    }
    out.truncate(n);
    for (i, c) in list.iter().enumerate() {
        if out.len() >= n {
            break;
        }
        if !out.iter().any(|(j, _)| *j == i) {
            out.push((i, c));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn desk_layout(
    ui: &egui::Ui,
    st: &mut State,
    p: &egui::Painter,
    r: Rect,
    ranked: &[Cand],
    towers: &[TowerRun],
    feasible: usize,
    rejected: usize,
    ms: f64,
    env: &Env,
) {
    let side_w = 330.0;
    let grid = Rect::from_min_max(r.min, pos2(r.right() - side_w - 20.0, r.bottom()));
    let side = Rect::from_min_max(pos2(r.right() - side_w, r.top()), r.max);

    // header row over the grid: count + how they are ranked + the filter
    let hy = grid.top() + 18.0;
    let h1 = text(
        p,
        pos2(grid.left(), hy),
        Align2::LEFT_CENTER,
        &format!("{feasible}"),
        kit::num(20.0),
        t::INK,
    );
    let h2 = text(
        p,
        pos2(h1.right() + 6.0, hy + 1.0),
        Align2::LEFT_CENTER,
        "meet the duty",
        kit::sans(12.5),
        t::INK_2,
    );
    text(
        p,
        pos2(h2.right() + 14.0, hy + 1.0),
        Align2::LEFT_CENTER,
        "least spare capacity first, one per tower type",
        kit::sans(11.5),
        t::MUTED,
    );
    let filt = Rect::from_min_size(pos2(grid.right() - 300.0, hy - 18.0), vec2(300.0, 36.0));
    let _ = hy;
    let sel = st.filter;
    if let Some(i) = kit::segmented(
        ui,
        filt,
        "size.filter",
        &["All", "Counterflow", "Crossflow"],
        sel,
        12.0,
    ) {
        st.filter = i;
        st.cand = None;
    }
    let list: Vec<&Cand> = ranked
        .iter()
        .filter(|c| st.cand_filter().is_none_or(|x| c.crossflow == x))
        .collect();
    let shown = pick(&list, 3);

    // round 2 (#91 decisions, "Size = a candidate scatter"): every feasible candidate, fan power vs cold
    // water; below it, the best of each tower as cards. Tap a point or a card for its line + rejections.
    let top = filt.bottom() + 14.0;
    let card_h = 132.0;
    let lab_h = 22.0;
    let sc = Rect::from_min_max(
        pos2(grid.left(), top),
        pos2(grid.right(), grid.bottom() - card_h - lab_h - 14.0),
    );
    scatter(ui, st, p, sc, &list, towers, false);
    let cols = 3;
    let gap = 16.0;
    let cw = (grid.width() - gap * (cols - 1) as f32) / cols as f32;
    let y = grid.bottom() - card_h;
    text(
        p,
        pos2(grid.left(), y - 6.0),
        Align2::LEFT_BOTTOM,
        if shown.len() > 1 {
            "Best of each tower"
        } else {
            "Closest fit"
        },
        kit::semi(11.5),
        t::MUTED,
    );
    let max_dims = shown
        .iter()
        .map(|(_, c)| tile_dims(c.footprint_m2, c.depth_m, c.crossflow))
        .fold((1.0f32, 1.0f32), |a, d| (a.0.max(d.0), a.1.max(d.1)));
    let scale = ((card_h - 16.0) / max_dims.1)
        .min(150.0 / max_dims.0)
        .clamp(0.2, 0.8);
    for (k, (rank, c)) in shown.iter().enumerate() {
        let cr = Rect::from_min_size(
            pos2(grid.left() + (cw + gap) * k as f32, y),
            vec2(cw, card_h),
        );
        card(ui, st, p, cr, *rank, *rank, c, scale, true);
    }
    if list.is_empty() {
        text(
            p,
            pos2(grid.center().x, top + 120.0),
            Align2::CENTER_CENTER,
            "No configuration of this type meets the duty",
            kit::sans(13.0),
            t::MUTED,
        );
    }

    // the side: the towers, each with its feasible count; then why the rest failed, inline
    kit::glass(p, side, 12);
    kit::side_panel(side);
    let inner = side.shrink(16.0);
    text(
        p,
        pos2(inner.left(), inner.top()),
        Align2::LEFT_TOP,
        "Towers in the catalog",
        kit::semi(12.5),
        t::INK_2,
    );
    let mut y = inner.top() + 28.0;
    for tw in towers {
        let rr = Rect::from_min_size(
            pos2(inner.left() - 6.0, y - 4.0),
            vec2(inner.width() + 12.0, 72.0),
        );
        let resp = kit::hit(ui, rr, &format!("size.tower.{}", tw.id));
        let open = st.info.as_deref() == Some(&format!("tower:{}", tw.id));
        if resp.hovered() || open {
            p.rect_filled(rr, kit::r(8), t::PANEL_RAISED);
        }
        tile_in(
            p,
            Rect::from_min_size(pos2(rr.left() + 4.0, rr.top() + 4.0), vec2(64.0, 64.0)),
            0.30,
            tw.footprint_m2,
            1.2,
            tw.crossflow,
            if tw.feasible > 0 {
                t::PRIMARY
            } else {
                t::INK_2
            },
            t::INK_2,
            tw.feasible == 0,
        );
        text(
            p,
            pos2(rr.left() + 76.0, rr.top() + 10.0),
            Align2::LEFT_TOP,
            &tw.id,
            kit::semi(13.0),
            t::INK,
        );
        text(
            p,
            pos2(rr.left() + 76.0, rr.top() + 29.0),
            Align2::LEFT_TOP,
            &format!(
                "{} · {:.0} m²",
                if tw.crossflow {
                    "crossflow"
                } else {
                    "counterflow"
                },
                tw.footprint_m2
            ),
            kit::sans(11.5),
            t::MUTED,
        );
        let rej: usize = tw.rejections.iter().map(|x| x.1).sum();
        let tot = (tw.feasible + rej).max(1);
        let m = Rect::from_min_size(
            pos2(rr.left() + 76.0, rr.top() + 52.0),
            vec2(rr.right() - 12.0 - rr.left() - 76.0, 5.0),
        );
        p.rect_filled(m, kit::r(2), t::with_alpha(t::DANGER, 60));
        p.rect_filled(
            Rect::from_min_size(
                m.min,
                vec2(m.width() * tw.feasible as f32 / tot as f32, m.height()),
            ),
            kit::r(2),
            t::OK,
        );
        let nr = text(
            p,
            pos2(rr.right() - 12.0, rr.top() + 9.0),
            Align2::RIGHT_TOP,
            &format!("{}", tw.feasible),
            kit::num(17.0),
            if tw.feasible > 0 { t::OK } else { t::INK_2 },
        );
        let ft = text(
            p,
            pos2(nr.left() - 6.0, nr.bottom() - 2.0),
            Align2::RIGHT_BOTTOM,
            "fit",
            kit::sans(11.0),
            t::MUTED,
        );
        kit::src_mark(
            p,
            pos2(ft.left() - 14.0, ft.center().y),
            Src::Calc,
            "sizes that meet the duty, counted over every tower",
        );
        text(
            p,
            pos2(rr.right() - 12.0, rr.top() + 31.0),
            Align2::RIGHT_TOP,
            &format!("{rej} out"),
            kit::mono(11.0),
            t::INK_2,
        );
        if resp.clicked() {
            toggle_info(st, &format!("tower:{}", tw.id));
        }
        y += 78.0;
    }
    y += 6.0;
    p.line_segment(
        [pos2(inner.left(), y), pos2(inner.right(), y)],
        Stroke::new(1.0, t::LINE_SOFT),
    );
    y += 14.0;
    let hr = text(
        p,
        pos2(inner.left(), y),
        Align2::LEFT_TOP,
        &format!("Why {rejected} were rejected"),
        kit::semi(12.5),
        t::INK_2,
    );
    kit::src_mark(
        p,
        pos2(hr.right() + 6.0, hr.center().y),
        Src::Calc,
        "every size tried that missed the duty, by reason",
    );
    let reasons = aggregate(towers);
    let rl = Rect::from_min_max(
        pos2(inner.left(), y + 28.0),
        pos2(inner.right(), inner.bottom() - 24.0),
    );
    let fit = ((rl.height() + 6.0) / 36.0).floor().max(1.0) as usize;
    let cut = reasons.len() > fit;
    let fit = if cut { fit - 1 } else { fit };
    reasons_list(p, rl, &reasons[..reasons.len().min(fit)], rejected, 36.0);
    if cut {
        let rest: usize = reasons[fit..].iter().map(|x| x.1).sum();
        text(
            p,
            pos2(rl.left(), rl.top() + fit as f32 * 36.0),
            Align2::LEFT_TOP,
            &format!("+ {} more reasons · {rest}", reasons.len() - fit),
            kit::sans(12.0),
            t::MUTED,
        );
    }
    text(
        p,
        pos2(inner.left(), inner.bottom()),
        Align2::LEFT_BOTTOM,
        &format!("{} sizes tried · {:.0} ms", towers.len(), ms),
        kit::mono(10.5),
        t::MUTED,
    );
    let _ = env;
}

impl State {
    fn cand_filter(&self) -> Option<bool> {
        match self.filter {
            1 => Some(false),
            2 => Some(true),
            _ => None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn card(
    ui: &egui::Ui,
    st: &mut State,
    p: &egui::Painter,
    r: Rect,
    rank: usize,
    slot: usize,
    c: &Cand,
    scale: f32,
    compact: bool,
) {
    let id = format!("size.card.{slot}");
    let resp = kit::hit(ui, r, &id);
    let sel = st.cand == Some(slot);
    let accent = if rank == 0 { t::PRIMARY } else { t::INK_2 };
    let fill = t::fill_color(&c.fill_id);
    p.rect_filled(r, kit::r(12), if sel { t::PRIMARY_SOFT } else { t::PANEL });
    p.rect_stroke(
        r,
        kit::r(12),
        Stroke::new(
            if sel || resp.hovered() { 1.5 } else { 1.0 },
            if sel {
                t::PRIMARY
            } else if resp.hovered() {
                t::LINE
            } else {
                t::LINE_SOFT
            },
        ),
        StrokeKind::Inside,
    );
    if resp.clicked() {
        st.cand = if sel { None } else { Some(slot) };
        st.info = None;
    }
    let spare = c.cap_ratio.map(|x| (x - 1.0) * 100.0);
    let spare_s = spare.map(t::num::pct).unwrap_or("–".into());
    if compact {
        // phone row: tile left, numbers right
        tile_in(
            p,
            Rect::from_min_size(
                pos2(r.left() + 6.0, r.top() + 6.0),
                vec2(92.0, r.height() - 12.0),
            ),
            scale,
            c.footprint_m2,
            c.depth_m,
            c.crossflow,
            accent,
            fill,
            false,
        );
        let x = r.left() + 106.0;
        let rk = text(
            p,
            pos2(x, r.top() + 11.0),
            Align2::LEFT_TOP,
            &format!("{}", rank + 1),
            kit::num(12.5),
            if rank == 0 { t::PRIMARY } else { t::INK_2 },
        );
        text(
            p,
            pos2(rk.right() + 8.0, r.top() + 10.0),
            Align2::LEFT_TOP,
            &c.tower_id,
            kit::semi(14.0),
            t::INK,
        );
        text_fit(
            p,
            pos2(x, r.top() + 31.0),
            Align2::LEFT_TOP,
            &format!(
                "{} {} m · {}",
                kit::part(&c.fill_id),
                t::num::value(c.depth_m, "m"),
                c.fan_id
            ),
            kit::sans(11.5),
            t::INK_2,
            r.right() - x - 10.0,
        );
        let cw = (r.right() - x - 8.0) / 3.0;
        for (i, (l, v, u)) in [
            ("spare", spare_s.clone(), "%"),
            ("margin", t::num::kelvin_signed(c.margin_c), "K"),
            ("power", t::num::power(c.power_kw), "kW"),
        ]
        .iter()
        .enumerate()
        {
            kit::metric_src(
                p,
                pos2(x + cw * i as f32, r.bottom() - 46.0),
                l,
                v,
                u,
                15.0,
                if i == 0 { t::PRIMARY } else { t::INK },
                Src::Calc,
                SEL,
            );
        }
        // the tower's colour in the scatter
        p.circle_filled(
            pos2(r.right() - 14.0, r.top() + 16.0),
            4.5,
            tower_color(st, &c.tower_id),
        );
        return;
    }
    // rank + tower
    let rk = text(
        p,
        pos2(r.left() + 16.0, r.top() + 14.0),
        Align2::LEFT_TOP,
        &format!("{}", rank + 1),
        kit::num(15.0),
        if rank == 0 { t::PRIMARY } else { t::INK_2 },
    );
    text(
        p,
        pos2(rk.right() + 10.0, r.top() + 13.0),
        Align2::LEFT_TOP,
        &c.tower_id,
        kit::semi(15.0),
        t::INK,
    );
    if rank == 0 {
        kit::chip(
            p,
            pos2(r.right() - 14.0, r.top() + 13.0),
            Align2::RIGHT_TOP,
            "closest fit",
            t::PRIMARY,
            t::PRIMARY_SOFT,
            t::PRIMARY_DEEP,
        );
    } else {
        text(
            p,
            pos2(r.right() - 14.0, r.top() + 16.0),
            Align2::RIGHT_TOP,
            if c.crossflow {
                "crossflow"
            } else {
                "counterflow"
            },
            kit::sans(11.0),
            t::MUTED,
        );
    }
    // the tile, as the hero of the card, seated on its stage
    let stage = Rect::from_min_max(
        pos2(r.left() + 12.0, r.top() + 44.0),
        pos2(r.right() - 12.0, r.bottom() - 100.0),
    );
    kit::ground(
        p,
        stage,
        pos2(stage.center().x, stage.bottom() - stage.height() * 0.28),
        stage.width() * 0.34,
        t::with_alpha(accent, 24),
    );
    tile_in(
        p,
        stage,
        scale,
        c.footprint_m2,
        c.depth_m,
        c.crossflow,
        accent,
        fill,
        false,
    );
    // config line
    let ly = r.bottom() - 90.0;
    let cfg = text_fit(
        p,
        pos2(r.left() + 16.0, ly),
        Align2::LEFT_TOP,
        &format!(
            "{} · {} m",
            kit::part(&c.fill_id),
            t::num::value(c.depth_m, "m")
        ),
        kit::semi(12.0),
        fill,
        r.width() * 0.55,
    );
    text_fit(
        p,
        pos2(r.right() - 16.0, ly),
        Align2::RIGHT_TOP,
        &format!(
            "{} at {} speed",
            kit::part(&c.fan_id),
            t::num::pct(c.speed * 100.0) + " %"
        ),
        kit::sans(11.5),
        t::INK_2,
        r.right() - cfg.right() - 24.0,
    );
    // three numbers: the rank key first
    let cw = (r.width() - 32.0) / 3.0;
    for (i, (l, v, u)) in [
        ("spare capacity", spare_s, "%"),
        ("margin", t::num::kelvin_signed(c.margin_c), "K"),
        ("fan power", t::num::power(c.power_kw), "kW"),
    ]
    .iter()
    .enumerate()
    {
        let col = match i {
            0 => t::PRIMARY,
            1 if c.margin_c < 0.0 => t::DANGER,
            1 => t::OK,
            _ => t::INK,
        };
        kit::metric_src(
            p,
            pos2(r.left() + 16.0 + cw * i as f32, ly + 26.0),
            l,
            v,
            u,
            19.0,
            col,
            Src::Calc,
            SEL,
        );
    }
}

/// The towers' colours in the scatter, in catalog order.
fn tower_color(st: &State, id: &str) -> Color32 {
    const PAL: [Color32; 6] = [t::PRIMARY, t::AIR, t::AMBER, t::WATER, t::OK, t::INK_2];
    let i = st
        .cache
        .size
        .as_ref()
        .and_then(|s| s.towers.iter().position(|x| x.id == id))
        .unwrap_or(0);
    PAL[i % PAL.len()]
}

/// Every feasible candidate: fan power (y) against its cold water (x), the target as a line. Circles are
/// counterflow, squares crossflow, colour = tower. Tap a point: its line and its tower's rejections.
#[allow(clippy::too_many_arguments)]
fn scatter(
    ui: &egui::Ui,
    st: &mut State,
    p: &egui::Painter,
    r: Rect,
    list: &[&Cand],
    towers: &[TowerRun],
    phone: bool,
) {
    kit::glass_soft(p, r, 10);
    let head = text(
        p,
        pos2(r.left() + 14.0, r.top() + 12.0),
        Align2::LEFT_TOP,
        "Fan power vs cold water",
        kit::semi(12.5),
        t::INK,
    );
    let sub = text_fit(
        p,
        pos2(head.right() + 10.0, head.center().y + 1.0),
        Align2::LEFT_CENTER,
        &format!("{} feasible · tap a point", list.len()),
        kit::sans(11.0),
        t::MUTED,
        if phone {
            r.right() - head.right() - 40.0
        } else {
            220.0
        },
    );
    kit::src_mark(
        p,
        pos2(sub.right() + 6.0, sub.center().y),
        Src::Calc,
        "each point is one size that meets the duty",
    );
    let target = st
        .cache
        .size
        .as_ref()
        .map(|_| list.first().map(|c| c.cold_c + c.margin_c).unwrap_or(0.0))
        .unwrap_or(0.0);
    let power_limit = st.limits.and_then(|l| l.1);
    if list.is_empty() {
        text(
            p,
            r.center(),
            Align2::CENTER_CENTER,
            "No configuration of this type meets the duty",
            kit::sans(13.0),
            t::MUTED,
        );
        return;
    }
    let (mut x0, mut x1, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
    for c in list {
        x0 = x0.min(c.cold_c);
        x1 = x1.max(c.cold_c);
        y1 = y1.max(c.power_kw);
    }
    x1 = x1.max(target);
    if let Some(l) = power_limit {
        y1 = y1.max(l);
    }
    let pad_x = ((x1 - x0) * 0.08).max(0.05);
    let plot = Plot {
        rect: Rect::from_min_max(
            pos2(
                r.left() + 52.0,
                head.bottom() + if phone { 40.0 } else { 26.0 },
            ),
            pos2(r.right() - 16.0, r.bottom() - 44.0),
        ),
        x: (x0 - pad_x, x1 + pad_x),
        y: (0.0, y1 * 1.12),
    };
    let xs = super::rate::nice((plot.x.1 - plot.x.0) / if phone { 4.0 } else { 7.0 });
    let ys = super::rate::nice(plot.y.1 / 4.0);
    plot.axes(
        p,
        xs,
        ys,
        "cold water °C",
        "fan power kW",
        if xs < 0.1 { 2 } else { 1 },
        0,
    );
    // the target: a vertical line, catalog-marked
    let tx = plot.pt(target, plot.y.1);
    kit::dashed(
        p,
        tx,
        pos2(tx.x, plot.rect.bottom()),
        Stroke::new(1.4, t::OK),
        5.0,
        4.0,
    );
    let tl = kit::label_plate(
        p,
        tx + vec2(-8.0, 6.0),
        Align2::RIGHT_TOP,
        &format!("target {target:.1} °C"),
        kit::semi(10.5),
        t::OK,
    );
    kit::src_mark(
        p,
        pos2(tl.left() - 13.0, tl.center().y),
        Src::Catalog,
        "duty · target cold water (recorded)",
    );
    if let Some(l) = power_limit {
        let ly = plot.pt(plot.x.0, l);
        kit::dashed(
            p,
            ly,
            pos2(plot.rect.right(), ly.y),
            Stroke::new(1.2, t::DANGER),
            5.0,
            4.0,
        );
        let pl = kit::label_plate(
            p,
            pos2(plot.rect.left() + 8.0, ly.y - 4.0),
            Align2::LEFT_BOTTOM,
            &format!("power limit {l:.0} kW"),
            kit::semi(10.5),
            t::DANGER,
        );
        kit::src_mark(
            p,
            pos2(pl.right() + 3.0, pl.center().y),
            Src::Catalog,
            "fan power limit, from the duty",
        );
    }
    // the points: rank order, best last so it sits on top
    let reach = if phone { 22.0 } else { 12.0 };
    let hover = ui.ctx().input(|i| i.pointer.hover_pos());
    let hits: Vec<Pos2> = list.iter().map(|c| plot.pt(c.cold_c, c.power_kw)).collect();
    for (i, c) in list.iter().enumerate().rev() {
        let q = hits[i];
        let col = tower_color(st, &c.tower_id);
        let sel = st.cand == Some(i);
        let s = if i == 0 || sel { 5.5 } else { 3.6 };
        if c.crossflow {
            p.rect_filled(
                Rect::from_center_size(q, vec2(s * 1.8, s * 1.8)),
                kit::r(1),
                t::with_alpha(col, 210),
            );
        } else {
            p.circle_filled(q, s, t::with_alpha(col, 210));
        }
        if sel || i == 0 {
            p.circle_stroke(q, s + 4.0, Stroke::new(1.6, if sel { t::INK } else { col }));
        }
    }
    if let Some(c) = list.first() {
        let q = plot.pt(c.cold_c, c.power_kw);
        // left of the point: the target line runs just right of the closest fit
        kit::label_plate(
            p,
            q + vec2(-10.0, -10.0),
            Align2::RIGHT_BOTTOM,
            "closest fit",
            kit::semi(10.5),
            t::INK,
        );
    }
    // tower key: right-aligned on the header row (desktop), or the row under it (phone)
    let fit: Vec<&TowerRun> = towers.iter().filter(|x| x.feasible > 0).collect();
    let item_w = |tw: &TowerRun| kit::measure(p, &tw.id, kit::sans(10.5)).x + 27.0;
    let total: f32 = fit.iter().map(|tw| item_w(tw)).sum();
    let (mut kx, ky) = if phone {
        (r.left() + 14.0, head.bottom() + 12.0)
    } else {
        (r.right() - 14.0 - total, head.center().y)
    };
    for tw in fit {
        let col = tower_color(st, &tw.id);
        if tw.crossflow {
            p.rect_filled(
                Rect::from_center_size(pos2(kx + 4.0, ky), vec2(7.0, 7.0)),
                kit::r(1),
                col,
            );
        } else {
            p.circle_filled(pos2(kx + 4.0, ky), 3.8, col);
        }
        text(
            p,
            pos2(kx + 13.0, ky),
            Align2::LEFT_CENTER,
            &tw.id,
            kit::sans(10.5),
            t::INK_2,
        );
        kx += item_w(tw);
    }
    let resp = ui.interact(
        plot.rect.expand(8.0),
        egui::Id::new("size.scatter"),
        egui::Sense::click(),
    );
    // The tap target. A press reads at the press's own position - a finger has no hover - and
    // `scatter_pick` resolves it: the nearest candidate within `reach`, and a press while the nearest
    // is already picked walks to the next point under the finger, so candidates that share a pixel
    // are all reachable. A press on clear plot space puts the card down (issue #83: a point you
    // cannot press is a defect).
    let pick = resp
        .interact_pointer_pos()
        .or(hover)
        .and_then(|at| scatter_pick(&hits, at, reach, st.cand));
    if let Some(i) = pick {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            st.cand = if st.cand == Some(i) { None } else { Some(i) };
            st.info = None;
        }
    } else if resp.clicked() {
        st.cand = None;
        st.info = None;
    }
}

/// The candidate a press at `at` picks among the scatter's `points` (screen positions, in list
/// order): the nearest within `reach`. A press while the nearest is already picked walks on to the
/// next point under the finger and wraps, so overlapping points - a cluster that shares a pixel -
/// are all reachable by repeated taps; `None` when nothing is within `reach`.
///
/// The overlap rule is the point of this function (its test taps a shared pixel three times):
/// nearest-wins alone leaves every point but the nearest unpickable, which is the defect.
fn scatter_pick(points: &[Pos2], at: Pos2, reach: f32, current: Option<usize>) -> Option<usize> {
    let mut within: Vec<(f32, usize)> = points
        .iter()
        .enumerate()
        .filter_map(|(i, q)| {
            let d = at.distance(*q);
            (d <= reach).then_some((d, i))
        })
        .collect();
    if within.is_empty() {
        return None;
    }
    // nearest first; equally placed points in list order, so repeated taps walk the list
    within.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let next = match current {
        Some(c) if within.len() > 1 => within
            .iter()
            .position(|x| x.1 == c)
            .map(|k| (k + 1) % within.len())
            .unwrap_or(0),
        _ => 0,
    };
    Some(within[next].1)
}

/// A picked candidate: its full engine line (every number marked), the rejections on its tower, and
/// its way into the Instrument - the control hands this size to the shared run, or says why it cannot
/// (issue #150).
#[allow(clippy::too_many_arguments)]
fn pick_sheet(
    ui: &egui::Ui,
    st: &mut State,
    area: Rect,
    side: Rect,
    rank: usize,
    c: &Cand,
    tw: Option<&TowerRun>,
    d: &mut EngineInput,
    env: &Env,
) {
    let mut reasons = tw.map(|x| x.rejections.clone()).unwrap_or_default();
    reasons.sort_by_key(|b| std::cmp::Reverse(b.1));
    // round-2 QA: a cut list says what it left out, so the rows add up to the heading
    let more: Option<(usize, usize)> =
        (reasons.len() > 4).then(|| (reasons.len() - 3, reasons[3..].iter().map(|x| x.1).sum()));
    reasons.truncate(if more.is_some() { 3 } else { 4 });
    let rej: usize = tw
        .map(|x| x.rejections.iter().map(|r| r.1).sum())
        .unwrap_or(0);
    let rows_n = reasons.len().max(1) + usize::from(more.is_some());
    let h = 68.0 + 4.0 * 46.0 + 34.0 + rows_n as f32 * 36.0 + 30.0 + 60.0;
    // desktop: docked over the side column, so the scatter (and the picked point) stays in view
    let r = if env.phone {
        sheet_rect(area, env.phone, h)
    } else {
        Rect::from_min_size(
            pos2(side.left() - 6.0, side.top()),
            vec2(side.width() + 12.0, h.min(side.height())),
        )
    };
    let sub = format!(
        "{} · {} {} m · {} at {} % speed",
        if c.crossflow {
            "crossflow"
        } else {
            "counterflow"
        },
        kit::part(&c.fill_id),
        t::num::value(c.depth_m, "m"),
        kit::part(&c.fan_id),
        t::num::pct(c.speed * 100.0)
    );
    let title = match kit::part_name(&c.tower_id) {
        Some(n) => format!("{}. {} · {}", rank + 1, n, c.tower_id),
        None => format!("{}. {}", rank + 1, c.tower_id),
    };
    let (lp, inner) = sheet_frame(ui, st, r, &title, &sub);
    let rows: [(&str, String, &str); 8] = [
        ("cold water", t::num::temp(c.cold_c), "°C"),
        ("margin", t::num::kelvin_signed(c.margin_c), "K"),
        ("fan power", t::num::power(c.power_kw), "kW"),
        ("airflow", t::num::flow(c.airflow_m3_s), "m³/s"),
        ("static pressure", t::num::pa(c.pressure_pa), "Pa"),
        ("drift", t::num::sig(c.drift_ppm, 3), "ppm"),
        (
            "make-up",
            t::num::flow(c.makeup_kg_s / data::density(d) * 3600.0),
            "m³/h",
        ),
        ("KaV/L available", t::num::kavl(c.available_merkel), ""),
    ];
    let cw = inner.width() / 2.0;
    for (i, (l, v, u)) in rows.iter().enumerate() {
        kit::metric_src(
            &lp,
            pos2(
                inner.left() + cw * (i % 2) as f32,
                inner.top() + 46.0 * (i / 2) as f32,
            ),
            l,
            v,
            u,
            15.0,
            if i == 1 && c.margin_c < 0.0 {
                t::DANGER
            } else {
                t::INK
            },
            Src::Calc,
            SEL,
        );
    }
    let y = inner.top() + 4.0 * 46.0 + 6.0;
    let hr = text(
        &lp,
        pos2(inner.left(), y),
        Align2::LEFT_TOP,
        &format!("Rejected on {}: {rej}", c.tower_id),
        kit::semi(12.0),
        t::INK_2,
    );
    kit::src_mark(
        &lp,
        pos2(hr.right() + 6.0, hr.center().y),
        Src::Calc,
        "rejected sizes on this tower, by reason",
    );
    let rl = Rect::from_min_max(
        pos2(inner.left(), hr.bottom() + 10.0),
        pos2(inner.right(), inner.bottom() - 52.0),
    );
    if reasons.is_empty() {
        text(
            &lp,
            rl.min,
            Align2::LEFT_TOP,
            "Nothing rejected on this tower",
            kit::sans(12.0),
            t::INK_2,
        );
    } else {
        reasons_list(&lp, rl, &reasons, rej, 36.0);
        if let Some((n, k)) = more {
            text(
                &lp,
                pos2(rl.left(), rl.top() + reasons.len() as f32 * 36.0),
                Align2::LEFT_TOP,
                &format!("+ {n} more reasons · {k}"),
                kit::sans(12.0),
                t::MUTED,
            );
        }
    }
    // Issue #150: the control hands this candidate to the shared run. What the pick resolves to is
    // read from the session's catalog, so a pick the catalog cannot carry is a locked control with
    // its reason - never a press that only apologises.
    let load = match env.catalog {
        Some(cat) => state::candidate_machine(
            cat,
            d,
            &state::Pick {
                tower_id: c.tower_id.clone(),
                fill_id: c.fill_id.clone(),
                depth_m: c.depth_m,
                fan_id: c.fan_id.clone(),
                speed: c.speed,
                nozzle_id: c.nozzle_id.clone(),
                drift_ppm: c.drift_ppm,
                airflow_m3_s: c.airflow_m3_s,
            },
        ),
        None => Err("the catalog is not ready".to_string()),
    };
    kit::text_fit(
        &lp,
        pos2(inner.left(), inner.bottom() - 70.0),
        Align2::LEFT_TOP,
        match &load {
            Ok(_) => "The answer strip stays on the draft until this loads.",
            Err(why) => why.as_str(),
        },
        kit::sans(11.5),
        t::MUTED,
        inner.width(),
    );
    let br = Rect::from_min_size(
        pos2(inner.left(), inner.bottom() - 44.0),
        vec2(inner.width(), 44.0),
    );
    let resp = ui.interact(br, egui::Id::new("size.use"), egui::Sense::click());
    match &load {
        Ok(m) => {
            lp.rect_filled(
                br,
                kit::r(8),
                if resp.hovered() {
                    t::PRIMARY
                } else {
                    t::PRIMARY_DEEP
                },
            );
            text(
                &lp,
                br.center(),
                Align2::CENTER_CENTER,
                "Open in Instrument",
                kit::semi(12.5),
                t::INK,
            );
            if resp.clicked() {
                let line = state::load_candidate(d, m);
                st.crossflow = false;
                st.go(Screen::Instrument);
                toast(st, &line);
            }
        }
        Err(_) => {
            // a locked control: the muted fill and the padlock say it cannot be pressed, and the row
            // above says why. No apology toast - the reason lives on the sheet.
            lp.rect_filled(br, kit::r(8), t::PANEL);
            lp.rect_stroke(
                br,
                kit::r(8),
                Stroke::new(1.0, t::LINE_SOFT),
                StrokeKind::Inside,
            );
            let w = kit::measure(&lp, "Open in Instrument", kit::semi(12.5)).x;
            let x = br.center().x - (w + 17.0) / 2.0;
            kit::lock_glyph(&lp, pos2(x + 5.0, br.center().y), 5.0, t::MUTED);
            text(
                &lp,
                pos2(x + 17.0, br.center().y),
                Align2::LEFT_CENTER,
                "Open in Instrument",
                kit::semi(12.5),
                t::MUTED,
            );
        }
    }
}

// =================================================================================== phone

#[allow(clippy::too_many_arguments)]
fn phone_layout(
    ui: &egui::Ui,
    st: &mut State,
    p: &egui::Painter,
    r: Rect,
    ranked: &[Cand],
    towers: &[TowerRun],
    feasible: usize,
    rejected: usize,
    env: &Env,
) {
    let h1 = text(
        p,
        pos2(r.left(), r.top() + 8.0),
        Align2::LEFT_CENTER,
        &format!("{feasible}"),
        kit::num(17.0),
        t::INK,
    );
    text(
        p,
        pos2(h1.right() + 6.0, r.top() + 9.0),
        Align2::LEFT_CENTER,
        "meet the duty",
        kit::sans(12.0),
        t::INK_2,
    );
    // the card order needs saying: the rows lead with one per tower type
    kit::text_fit(
        p,
        pos2(r.left(), r.top() + 24.0),
        Align2::LEFT_TOP,
        "least spare capacity first, one per tower type",
        kit::sans(10.5),
        t::MUTED,
        r.width() - 6.0,
    );
    let filt = Rect::from_min_size(pos2(r.right() - 168.0, r.top() - 12.0), vec2(168.0, 40.0));
    if let Some(i) = kit::segmented(
        ui,
        filt,
        "size.filter",
        &["All", "CF", "XF"],
        st.filter,
        12.5,
    ) {
        st.filter = i;
        st.cand = None;
    }
    let list: Vec<&Cand> = ranked
        .iter()
        .filter(|c| st.cand_filter().is_none_or(|x| c.crossflow == x))
        .collect();
    let towers_h = 92.0;
    // round 2: the candidate scatter leads; the best of each tower follow as rows
    let sc_h = ((r.height() - towers_h - 52.0) * 0.46).clamp(200.0, 320.0);
    let sc = Rect::from_min_size(pos2(r.left(), r.top() + 44.0), vec2(r.width(), sc_h));
    scatter(ui, st, p, sc, &list, towers, true);
    let list_r = Rect::from_min_max(
        pos2(r.left(), sc.bottom() + 12.0),
        pos2(r.right(), r.bottom() - towers_h - 10.0),
    );
    // fill the list area: card height adapts so the last row lands on the panel edge
    let n = (((list_r.height() + 8.0) / 100.0).floor() as usize).clamp(1, 4);
    let row_h = (list_r.height() + 8.0) / n as f32 - 8.0;
    let shown = pick(&list, n);
    let max_dims = shown
        .iter()
        .map(|(_, c)| tile_dims(c.footprint_m2, c.depth_m, c.crossflow))
        .fold((1.0f32, 1.0f32), |a, d| (a.0.max(d.0), a.1.max(d.1)));
    let scale = ((row_h - 16.0) / max_dims.1)
        .min(88.0 / max_dims.0)
        .clamp(0.2, 0.8);
    for (k, (rank, c)) in shown.iter().enumerate() {
        let cr = Rect::from_min_size(
            pos2(list_r.left(), list_r.top() + (row_h + 8.0) * k as f32),
            vec2(list_r.width(), row_h),
        );
        card(ui, st, p, cr, *rank, *rank, c, scale, true);
    }
    // the towers as a chip row: tap one for its reasons
    let tr = Rect::from_min_max(pos2(r.left(), r.bottom() - towers_h), r.max);
    kit::glass_soft(p, tr, 10);
    let lab = text(
        p,
        pos2(tr.left() + 12.0, tr.top() + 10.0),
        Align2::LEFT_TOP,
        "Towers",
        kit::semi(11.0),
        t::MUTED,
    );
    let wr = Rect::from_min_size(pos2(tr.right() - 140.0, tr.top()), vec2(136.0, 32.0));
    let resp = kit::hit(ui, wr.expand2(vec2(0.0, 8.0)), "size.why");
    let col = if resp.hovered() { t::INK } else { t::PRIMARY };
    let wl = text(
        p,
        pos2(wr.left(), wr.center().y),
        Align2::LEFT_CENTER,
        &format!("{rejected} rejected"),
        kit::semi(12.5),
        col,
    );
    kit::chevron(p, pos2(wl.right() + 9.0, wr.center().y), 11.0, col);
    if resp.clicked() {
        toggle_info(st, "rej");
    }
    let cw = (tr.width() - 16.0) / towers.len().max(1) as f32;
    for (i, tw) in towers.iter().enumerate() {
        let cr = Rect::from_min_size(
            pos2(tr.left() + 8.0 + cw * i as f32, lab.bottom() + 6.0),
            vec2(cw - 6.0, 48.0),
        );
        let resp = kit::hit(ui, cr, &format!("size.tower.{}", tw.id));
        p.rect_filled(
            cr,
            kit::r(8),
            if resp.hovered() {
                t::PANEL_RAISED
            } else {
                t::PANEL
            },
        );
        text(
            p,
            pos2(cr.center().x, cr.top() + 7.0),
            Align2::CENTER_TOP,
            &tw.id,
            kit::semi(11.0),
            if tw.feasible > 0 { t::INK } else { t::INK_2 },
        );
        p.circle_filled(
            pos2(cr.left() + 10.0, cr.top() + 13.0),
            3.5,
            tower_color(st, &tw.id),
        );
        text(
            p,
            pos2(cr.center().x, cr.bottom() - 7.0),
            Align2::CENTER_BOTTOM,
            &format!("{} fit", tw.feasible),
            kit::mono(10.5),
            if tw.feasible > 0 { t::OK } else { t::DANGER },
        );
        if resp.clicked() {
            toggle_info(st, &format!("tower:{}", tw.id));
        }
    }
    let _ = env;
}

// ================================================================================== sheets

fn sheet_rect(area: Rect, phone: bool, h: f32) -> Rect {
    if phone {
        Rect::from_min_max(
            pos2(area.left() + 8.0, area.bottom() - h - 8.0),
            pos2(area.right() - 8.0, area.bottom() - 8.0),
        )
    } else {
        Rect::from_center_size(
            pos2(area.center().x, area.center().y + 20.0),
            vec2(520.0, h),
        )
    }
}

fn reasons_list(lp: &egui::Painter, r: Rect, reasons: &[(String, usize)], total: usize, row: f32) {
    let max = reasons.iter().map(|x| x.1).max().unwrap_or(1).max(1);
    for (i, (k, n)) in reasons.iter().enumerate() {
        let y = r.top() + i as f32 * row;
        text_fit(
            lp,
            pos2(r.left(), y),
            Align2::LEFT_TOP,
            reason_label(k),
            kit::sans(12.5),
            t::INK,
            r.width() - 60.0,
        );
        text(
            lp,
            pos2(r.right(), y),
            Align2::RIGHT_TOP,
            &format!("{n}"),
            kit::num(13.0),
            t::INK,
        );
        let bar = Rect::from_min_size(pos2(r.left(), y + 20.0), vec2(r.width(), 5.0));
        lp.rect_filled(bar, kit::r(2), t::LINE_SOFT);
        lp.rect_filled(
            Rect::from_min_size(bar.min, vec2(bar.width() * *n as f32 / max as f32, 5.0)),
            kit::r(2),
            t::with_alpha(t::DANGER, 200),
        );
    }
    let _ = total;
}

fn sheet_frame(
    ui: &egui::Ui,
    st: &mut State,
    r: Rect,
    title: &str,
    sub: &str,
) -> (egui::Painter, Rect) {
    let lp = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("size.sheet"),
    ));
    lp.rect_filled(
        r.expand(2.0),
        kit::r(14),
        t::with_alpha(Color32::BLACK, 120),
    );
    lp.rect_filled(r, kit::r(12), t::PANEL_RAISED);
    lp.rect_stroke(r, kit::r(12), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
    text(
        &lp,
        pos2(r.left() + 18.0, r.top() + 16.0),
        Align2::LEFT_TOP,
        title,
        kit::semi(15.0),
        t::INK,
    );
    text(
        &lp,
        pos2(r.left() + 18.0, r.top() + 38.0),
        Align2::LEFT_TOP,
        sub,
        kit::sans(11.5),
        t::MUTED,
    );
    let x = Rect::from_center_size(pos2(r.right() - 26.0, r.top() + 26.0), vec2(44.0, 44.0));
    let resp = ui.interact(x, egui::Id::new("size.sheet.close"), egui::Sense::click());
    kit::glyph(
        &lp,
        "close",
        x.center(),
        14.0,
        if resp.hovered() { t::INK } else { t::MUTED },
    );
    if resp.clicked() {
        st.info = None;
        st.cand = None;
    }
    (
        lp,
        Rect::from_min_max(
            pos2(r.left() + 18.0, r.top() + 68.0),
            pos2(r.right() - 18.0, r.bottom() - 16.0),
        ),
    )
}

fn rejection_sheet(
    ui: &egui::Ui,
    st: &mut State,
    area: Rect,
    reasons: &[(String, usize)],
    total: usize,
    phone: bool,
) {
    let h = 96.0 + reasons.len() as f32 * 40.0;
    let r = sheet_rect(area, phone, h);
    let (lp, inner) = sheet_frame(
        ui,
        st,
        r,
        &format!("{total} configurations rejected"),
        "why sizes were rejected, all towers",
    );
    reasons_list(&lp, inner, reasons, total, 40.0);
}

fn tower_sheet(ui: &egui::Ui, st: &mut State, area: Rect, tw: &TowerRun, phone: bool) {
    let mut reasons = tw.rejections.clone();
    reasons.sort_by_key(|b| std::cmp::Reverse(b.1));
    let rej: usize = reasons.iter().map(|x| x.1).sum();
    let h = 110.0 + reasons.len().max(1) as f32 * 40.0;
    let r = sheet_rect(area, phone, h);
    let sub = format!(
        "{} · {} fit · {rej} rejected · max {} m³/h",
        if tw.crossflow {
            "crossflow"
        } else {
            "counterflow"
        },
        tw.feasible,
        t::num::flow(tw.max_flow_kg_s * 3.6)
    );
    let (lp, inner) = sheet_frame(ui, st, r, &tw.id, &sub);
    if reasons.is_empty() {
        text(
            &lp,
            inner.min,
            Align2::LEFT_TOP,
            "Nothing rejected on this tower",
            kit::sans(12.5),
            t::INK_2,
        );
    } else {
        reasons_list(&lp, inner, &reasons, rej, 40.0);
    }
    let _ = info_card;
}

#[allow(dead_code)]
fn _unused(_d: &data::SizeData) {}

#[cfg(test)]
mod tests {
    use super::egui::pos2;
    use super::scatter_pick;

    /// The defect the mandate names: overlapping points can share a pixel, and nearest-wins alone
    /// leaves every one but the nearest unpickable. Repeated taps at the shared pixel walk the
    /// cluster and wrap - this is the selection path a finger takes, not the pixels.
    #[test]
    fn a_shared_pixels_points_are_all_reachable_by_repeated_taps() {
        let cluster = [pos2(10.0, 10.0), pos2(10.0, 10.0), pos2(10.0, 10.0)];
        let reach = 22.0;
        let tap = pos2(11.0, 10.0);
        let first = scatter_pick(&cluster, tap, reach, None);
        assert_eq!(first, Some(0), "the first tap lands on the cluster's head");
        let second = scatter_pick(&cluster, tap, reach, first);
        assert_eq!(
            second,
            Some(1),
            "a second tap reaches the point under the first"
        );
        let third = scatter_pick(&cluster, tap, reach, second);
        assert_eq!(third, Some(2), "a third tap reaches the last point");
        assert_eq!(
            scatter_pick(&cluster, tap, reach, third),
            Some(0),
            "further taps wrap"
        );
    }

    /// The hit area is a finger's, not a pixel's: a tap within `reach` picks the nearest point,
    /// and a tap beyond it picks nothing.
    #[test]
    fn the_tap_target_is_the_nearest_point_within_reach() {
        let points = [pos2(10.0, 10.0), pos2(30.0, 10.0)];
        assert_eq!(scatter_pick(&points, pos2(12.0, 10.0), 22.0, None), Some(0));
        assert_eq!(scatter_pick(&points, pos2(28.0, 10.0), 22.0, None), Some(1));
        assert_eq!(
            scatter_pick(&points, pos2(70.0, 10.0), 22.0, None),
            None,
            "clear plot space picks nothing"
        );
        // Equally placed points tie-break in list order, and a repeat tap at the same pixel walks
        // to the other one - the overlap rule, at the midpoint of a pair.
        assert_eq!(scatter_pick(&points, pos2(20.0, 10.0), 22.0, None), Some(0));
        assert_eq!(
            scatter_pick(&points, pos2(20.0, 10.0), 22.0, Some(0)),
            Some(1)
        );
    }

    /// A lone point stays picked across taps (nothing to walk to), and a press off the points is
    /// not a pick.
    #[test]
    fn a_lone_point_is_stable_and_empty_space_clears() {
        let points = [pos2(10.0, 10.0)];
        assert_eq!(
            scatter_pick(&points, pos2(10.0, 12.0), 22.0, Some(0)),
            Some(0)
        );
        assert_eq!(scatter_pick(&points, pos2(100.0, 100.0), 22.0, None), None);
    }
}
