//! #91 round 2: **the duty-first Instrument** - the answer card, the source marks and the tap-detail.
//!
//! The owner's round-2 decisions (issue #91, "Round-2 decisions"): one always-visible answer card - cold
//! water vs the target (PASS/FAIL and the margin), approach, fan power, drift, water use - over a full-bleed
//! tower; fan speed leaves the main controls and lives in the fan's own detail; every number carries a
//! source mark (calculated / catalog / illustrative) with its source on tap; a tap on a zone opens its
//! pressure breakdown.
//!
//! Nothing here computes physics. Every number is a field of `Engine::run`'s output, a field of a catalog
//! record, or the duty the user typed; the one derived value - the drift eliminator's ppm - is the
//! record's own curve read at the engine's airflow, the same reading both engines make internally
//! (`fixture_engine`'s face velocity, the real engine's `drift_performance_at_velocity`), and it is marked
//! *catalog* with that rule as its source line.
use bevy_egui::egui;
use egui::{Color32, RichText, Sense, Stroke};

use cockpit::engine::{EngineInput, EngineOutput, ZoneId};
use drafthouse_cockpit_seams::mapping as m;

use crate::app::{open_picker, Draft, HitMap};
use crate::duty_panel::{self, DutyRes};
use crate::state::{Detail, Run, Slot, View, Visual};
use crate::theme as t;
use drafthouse_cockpit_seams::duty::OUT_OF_FIXTURE_RANGE;

// ------------------------------------------------------------------------------------------- source marks

/// Where a number comes from. The mark is a painted shape (never a glyph, so it cannot fall back to tofu)
/// and the shape differs as well as the colour, so it reads without colour vision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    /// A field of `Engine::run`'s output: a filled teal disc.
    Calc,
    /// A field of a catalog record (or a record's curve read at the engine's operating point): a ring.
    Catalog,
    /// Illustrative - drawn to explain, not computed: an amber diamond.
    Illus,
}

impl Src {
    pub fn word(self) -> &'static str {
        match self {
            Src::Calc => "calculated",
            Src::Catalog => "catalog",
            Src::Illus => "illustrative",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Src::Calc => "calc",
            Src::Catalog => "catalog",
            Src::Illus => "illus",
        }
    }
}

/// Paint one mark centred on `c`, radius `r`.
pub fn paint_mark(p: &egui::Painter, c: egui::Pos2, r: f32, src: Src) {
    match src {
        Src::Calc => {
            p.circle_filled(c, r, t::PRIMARY);
        }
        Src::Catalog => {
            p.circle_stroke(c, r - 0.6, Stroke::new(1.4, t::INK_2));
        }
        Src::Illus => {
            let pts = vec![
                egui::pos2(c.x, c.y - r - 0.6),
                egui::pos2(c.x + r + 0.6, c.y),
                egui::pos2(c.x, c.y + r + 0.6),
                egui::pos2(c.x - r - 0.6, c.y),
            ];
            p.add(egui::Shape::convex_polygon(pts, t::AMBER, Stroke::NONE));
        }
    }
}

/// A mark as a widget: hover names the source kind, a click toggles the row's source line.
fn mark(ui: &mut egui::Ui, src: Src) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), Sense::click());
    paint_mark(ui.painter(), r.center(), 3.4, src);
    resp.on_hover_text(src.word())
}

fn hit(hits: &mut HitMap, key: &str, resp: egui::Response) -> egui::Response {
    let r = resp.rect;
    hits.0
        .push((key.to_string(), [r.min.x, r.min.y, r.width(), r.height()]));
    resp
}

// ------------------------------------------------------------------------------------------ the numbers

/// The drift eliminator's ppm: the record's own curve at the run's face velocity (engine airflow over the
/// tower record's drift area), interpolated linearly and extrapolated past the curve's ends - the reading
/// both engines make (`clampEnds: false` in the real engine's `drift_performance_at_velocity`).
pub fn drift_ppm(input: &EngineInput, airflow_m3_s: f64) -> Option<(f64, f64)> {
    let area = input.tower.drift_area_m2;
    if area <= 0.0 || input.drift.curve.len() < 2 {
        return None;
    }
    let v = airflow_m3_s / area;
    let mut pts: Vec<(f64, f64)> = input
        .drift
        .curve
        .iter()
        .map(|p| (p.face_velocity_m_s, p.drift_ppm))
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let seg = pts
        .windows(2)
        .find(|w| v <= w[1].0)
        .map(|w| (w[0], w[1]))
        .unwrap_or((pts[pts.len() - 2], pts[pts.len() - 1]));
    let ((x0, y0), (x1, y1)) = seg;
    let ppm = if (x1 - x0).abs() < 1e-12 {
        y0
    } else {
        y0 + (v - x0) * (y1 - y0) / (x1 - x0)
    };
    Some((ppm, v))
}

/// One row of the air path as the screen states it: the engine's zones in air-path order, with the fill's
/// per-layer rows grouped into one `fill` row (their sum). `share_shown` is the engine's share rounded by the
/// largest-remainder rule, so the whole-percent shares on screen add to exactly 100.
#[derive(Clone, Debug, PartialEq)]
pub struct PathRow {
    pub zone: ZoneId,
    pub name: &'static str,
    pub pa: f64,
    pub share: f64,
    pub share_shown: i64,
}

pub fn zone_name(z: ZoneId) -> &'static str {
    match z {
        ZoneId::Inlet => "inlet",
        ZoneId::Rain => "rain",
        ZoneId::Fill => "fill",
        ZoneId::Spray => "spray",
        ZoneId::Drift => "drift",
        ZoneId::Plenum => "plenum",
        ZoneId::Stack => "stack",
        ZoneId::Fixed => "fixed",
    }
}

pub fn air_path(o: &EngineOutput) -> Vec<PathRow> {
    let mut rows: Vec<PathRow> = Vec::new();
    for z in o.pressure_by_zone.iter() {
        if let Some(last) = rows.last_mut() {
            if last.zone == ZoneId::Fill && z.zone == ZoneId::Fill {
                last.pa += z.pressure_pa;
                last.share += z.share_pct;
                continue;
            }
        }
        rows.push(PathRow {
            zone: z.zone,
            name: zone_name(z.zone),
            pa: z.pressure_pa,
            share: z.share_pct,
            share_shown: 0,
        });
    }
    let shown = largest_remainder(&rows.iter().map(|r| r.share).collect::<Vec<f64>>());
    for (r, s) in rows.iter_mut().zip(shown) {
        r.share_shown = s;
    }
    rows
}

/// Whole-percent shares that add to the rounded total of the inputs (100 for the engine's shares): floor
/// every share, then hand the missing points to the largest remainders. Display rounding only - the gate
/// (`tools/gate.mjs`) checks each shown share is within one point of the engine's.
pub fn largest_remainder(shares: &[f64]) -> Vec<i64> {
    let total = shares.iter().sum::<f64>().round() as i64;
    let mut out: Vec<i64> = shares.iter().map(|s| s.floor() as i64).collect();
    let mut rem: Vec<(usize, f64)> = shares
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s - s.floor()))
        .collect();
    rem.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut missing = total - out.iter().sum::<i64>();
    let mut k = 0;
    while missing > 0 && !rem.is_empty() {
        out[rem[k % rem.len()].0] += 1;
        missing -= 1;
        k += 1;
    }
    out
}

/// The shown share of one zone (the fill = its grouped row).
pub fn shown_share(o: &EngineOutput, z: ZoneId) -> Option<(f64, i64)> {
    air_path(o)
        .into_iter()
        .find(|r| r.zone == z)
        .map(|r| (r.pa, r.share_shown))
}

/// The same reading straight off a `Run`: the call-out painter's entry point.
pub fn shown_share_f(run: &Run, z: ZoneId) -> Option<(f64, i64)> {
    run.output.as_ref().and_then(|o| shown_share(o, z))
}

/// The fill's own KaV/L: the layers' sum. The engine's `kavl_total` is the tower's *available* transfer
/// (fill + spray zone + rain zone).
pub fn fill_kavl(o: &EngineOutput) -> f64 {
    o.kavl_per_layer.iter().map(|l| l.kavl).sum()
}

// ------------------------------------------------------------------------------------------ answer card

/// The answer, in the order the decision names it. Each row: its key, label, value, unit, source and the
/// source line a tap opens.
struct Row {
    key: &'static str,
    label: &'static str,
    value: String,
    unit: String,
    src: Src,
    source: String,
    gated: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn answer_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    draft: &mut Draft,
    run: &Run,
    vis: &mut Visual,
    duty_res: &DutyRes,
    hits: &mut HitMap,
    phone: bool,
) -> egui::Rect {
    let verdict = duty_panel::verdict(duty_res, &draft.0.duty);
    let flow_ok = verdict.as_ref().map(|v| v.flow_in_range).unwrap_or(false);
    let duty_ok = verdict.as_ref().map(|v| v.in_range()).unwrap_or(false);
    let max_drift = duty_res
        .spec
        .as_ref()
        .and_then(|s| s.limits.iter().find(|l| l.key == "maxDriftPpm"))
        .and_then(|l| l.value);
    let mut out_rect = rect;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.set_width(rect.width());
        let frame = egui::Frame::new()
            .fill(t::with_alpha(t::BG, 226))
            .stroke(Stroke::new(1.0, t::with_alpha(t::LINE, 220)))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(
                if phone { 10 } else { 14 },
                if phone { 6 } else { 10 },
            ));
        let shown = frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = if phone { 1.0 } else { 3.0 };
            // ---- 1. the duty (the question): four inputs, edited in place
            duty_row(ui, draft, hits, phone, vis);
            ui.add_space(if phone { 2.0 } else { 4.0 });
            let Some(o) = run.output.as_ref() else {
                ui.label(
                    RichText::new("no result - the engine refused this input")
                        .size(12.0)
                        .color(t::DANGER),
                );
                if let Some(e) = run.error.as_ref() {
                    ui.label(RichText::new(format!("{e:?}")).size(10.0).color(t::MUTED));
                }
                return;
            };
            // ---- 2. the answer: PASS/FAIL and the margin
            let target = draft.0.duty.target_cold_water_c;
            let margin = target - o.cold_water_c;
            let pass = margin >= 0.0 && o.validation.is_empty();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if !duty_ok {
                    ui.label(duty_panel::out_of_range_text());
                } else {
                    let (fg, bg) = if pass {
                        (t::OK, t::OK_SOFT)
                    } else {
                        (t::DANGER, t::DANGER_SOFT)
                    };
                    let chip = t::chip_frame(bg, t::with_alpha(fg, 160)).show(ui, |ui| {
                        ui.label(
                            RichText::new(if pass { "PASS" } else { "FAIL" })
                                .size(if phone { 13.0 } else { 15.0 })
                                .color(fg)
                                .family(t::family_semi())
                                .extra_letter_spacing(0.8),
                        );
                    });
                    hit(hits, "answer:verdict", chip.response);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.label(
                                RichText::new(t::fmt(o.cold_water_c))
                                    .size(if phone { 18.0 } else { 22.0 })
                                    .color(t::INK)
                                    .family(t::family_mono_med()),
                            );
                            ui.label(
                                RichText::new(format!("C cold water · target {}", t::fmt(target)))
                                    .size(11.0)
                                    .color(t::MUTED),
                            );
                            let r = mark(ui, Src::Calc);
                            if hit(hits, "src:cold", r).clicked() {
                                toggle(vis, "cold");
                            }
                        });
                        ui.label(
                            RichText::new(format!(
                                "margin {:+.2} C{}",
                                margin,
                                if pass { "" } else { " short" }
                            ))
                            .size(11.5)
                            .color(if pass { t::OK } else { t::DANGER })
                            .family(t::family_mono_med()),
                        );
                    });
                }
            });
            if vis.source_open == Some("cold") {
                source_line(
                    ui,
                    "Engine::run -> cold_water_c, at this duty and fan speed; margin = target - cold water",
                );
            }
            if !o.validation.is_empty() {
                for l in o.validation.iter().take(2) {
                    ui.label(
                        RichText::new(format!("limit: {}", l.field))
                            .size(10.0)
                            .color(t::DANGER),
                    );
                }
            }
            if !duty_ok {
                if let Some(v) = verdict.as_ref() {
                    for line in v.out_of_range.iter() {
                        ui.label(RichText::new(line).size(10.0).color(t::AMBER));
                    }
                }
            }
            ui.add_space(if phone { 2.0 } else { 4.0 });
            // ---- 3. the four numbers the decision names
            let rpm = m::rpm_text(draft.0.speed_ratio, draft.0.fan.nominal_rpm);
            let drift = drift_ppm(&draft.0, o.airflow_m3_s);
            let rows = [
                Row {
                    key: "approach",
                    label: "approach",
                    value: t::fmt(o.approach_c),
                    unit: "C".into(),
                    src: Src::Calc,
                    source: "Engine::run -> approach_c (cold water - wet bulb)".into(),
                    gated: !duty_ok,
                },
                Row {
                    key: "fan",
                    label: "fan power",
                    value: t::fmt(o.fan_power_kw),
                    unit: format!("kW @ {rpm} rpm"),
                    src: Src::Calc,
                    source: format!(
                        "Engine::run -> fan_power_kw at speed ratio {:.3} (rpm = the record's rated {} x ratio)",
                        draft.0.speed_ratio,
                        draft
                            .0
                            .fan
                            .nominal_rpm
                            .map(|n| format!("{n:.0}"))
                            .unwrap_or_else(|| "-".into())
                    ),
                    gated: !flow_ok,
                },
                Row {
                    key: "drift",
                    label: "drift",
                    value: match (drift, max_drift) {
                        (Some((ppm, _)), Some(max)) => {
                            format!("{} / max {}", t::fmt(ppm), t::fmt(max))
                        }
                        (Some((ppm, _)), None) => t::fmt(ppm),
                        _ => "-".into(),
                    },
                    unit: "ppm".into(),
                    src: Src::Catalog,
                    source: match drift {
                        Some((_, v)) => format!(
                            "{} curve at face velocity {:.2} m/s (engine airflow / drift area); max from the recorded limits",
                            draft.0.drift.id, v
                        ),
                        None => "the drift record carries no curve".into(),
                    },
                    gated: !flow_ok,
                },
                Row {
                    key: "kavl",
                    label: "available transfer",
                    value: format!("KaV/L {:.3}", o.kavl_total),
                    unit: "fill + spray + rain".into(),
                    src: Src::Calc,
                    source: format!(
                        "Engine::run -> kavl_total, the tower's available transfer: the fill's own {:.3} plus the spray and rain zones - not a fill figure",
                        fill_kavl(o)
                    ),
                    gated: !flow_ok,
                },
                Row {
                    key: "water",
                    label: "water use",
                    value: t::fmt(o.makeup_m3_hr),
                    unit: "m3/hr make-up".into(),
                    src: Src::Calc,
                    source: format!(
                        "Engine::run -> makeup_m3_hr at {} cycles of concentration (evaporation {} % of the circulating flow)",
                        t::fmt(draft.0.duty.cycles_of_concentration),
                        t::fmt(o.evaporation_pct)
                    ),
                    gated: !duty_ok,
                },
            ];
            for row in rows.iter() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let r = mark(ui, row.src);
                    let r = hit(hits, &format!("src:{}", row.key), r);
                    let lab = ui.add(
                        egui::Label::new(RichText::new(row.label).size(11.5).color(t::MUTED))
                            .sense(Sense::click()),
                    );
                    if r.clicked() || lab.clicked() {
                        toggle(vis, row.key);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if row.gated {
                            ui.label(
                                RichText::new(OUT_OF_FIXTURE_RANGE)
                                    .size(10.5)
                                    .color(t::AMBER),
                            );
                        } else {
                            ui.label(RichText::new(&row.unit).size(10.0).color(t::MUTED));
                            ui.label(
                                RichText::new(&row.value)
                                    .size(if phone { 12.0 } else { 13.0 })
                                    .color(t::INK)
                                    .family(t::family_mono_med()),
                            );
                        }
                    });
                });
                if vis.source_open == Some(row.key) {
                    source_line(ui, &row.source);
                }
            }
            // ---- 4. the key to the marks, and the two ways on
            ui.add_space(if phone { 1.0 } else { 3.0 });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for s in [Src::Calc, Src::Catalog, Src::Illus] {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 12.0), Sense::hover());
                    paint_mark(ui.painter(), r.center(), 3.0, s);
                    ui.label(RichText::new(s.word()).size(9.5).color(t::MUTED));
                    ui.add_space(4.0);
                }
            });
            if !phone {
                ui.label(
                    RichText::new("tap a number for its source · a part or zone for its detail")
                        .size(9.5)
                        .color(t::with_alpha(t::MUTED, 200)),
                );
            }
        });
        let r = shown.response.rect;
        hits.0.push((
            "card:answer".to_string(),
            [r.min.x, r.min.y, r.width(), r.height()],
        ));
        out_rect = r;
    });
    out_rect
}

fn toggle(vis: &mut Visual, key: &'static str) {
    vis.source_open = if vis.source_open == Some(key) {
        None
    } else {
        Some(key)
    };
}

fn source_line(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .size(9.5)
                .color(t::INK_2)
                .family(t::family_mono_med()),
        )
        .wrap(),
    );
}

/// The duty, as four inputs. On a phone the inputs fold behind the summary until it is tapped.
fn duty_row(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    hits: &mut HitMap,
    phone: bool,
    vis: &mut Visual,
) {
    let d = &mut draft.0.duty;
    let field = |ui: &mut egui::Ui,
                 hits: &mut HitMap,
                 key: &str,
                 label: &str,
                 v: &mut f64,
                 speed: f64,
                 dec: usize,
                 w: f32| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(label).size(9.5).color(t::MUTED));
            let r = ui.add_sized(
                egui::vec2(w, if phone { 30.0 } else { 22.0 }),
                egui::DragValue::new(v).speed(speed).fixed_decimals(dec),
            );
            hit(hits, key, r);
        });
    };
    if phone && !vis.duty_open {
        let r = ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "duty {} m3/hr · {}→{} C · WB {} C  ▾",
                    t::fmt(d.water_flow_m3_hr),
                    t::fmt(d.hot_water_c),
                    t::fmt(d.target_cold_water_c),
                    t::fmt(d.wet_bulb_c)
                ))
                .size(11.0)
                .color(t::INK_2)
                .family(t::family_mono_med()),
            )
            .sense(Sense::click()),
        );
        if hit(hits, "ctl:duty-expand", r).clicked() {
            vis.duty_open = true;
        }
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = if phone { 6.0 } else { 8.0 };
        let w = if phone { 64.0 } else { 58.0 };
        field(
            ui,
            hits,
            "ctl:duty-flow",
            "flow m3/hr",
            &mut d.water_flow_m3_hr,
            5.0,
            0,
            w,
        );
        field(
            ui,
            hits,
            "ctl:duty-hot",
            "hot C",
            &mut d.hot_water_c,
            0.1,
            1,
            w - 8.0,
        );
        field(
            ui,
            hits,
            "ctl:duty-target",
            "target C",
            &mut d.target_cold_water_c,
            0.1,
            1,
            w - 8.0,
        );
        field(
            ui,
            hits,
            "ctl:duty-wetbulb",
            "wet bulb C",
            &mut d.wet_bulb_c,
            0.1,
            1,
            w - 8.0,
        );
        if phone {
            let r = ui.add_sized(egui::vec2(30.0, 30.0), egui::Button::new("▴"));
            if hit(hits, "ctl:duty-collapse", r).clicked() {
                vis.duty_open = false;
            }
        }
    });
}

// ------------------------------------------------------------------------------------------- tap-detail

/// The detail card for whatever was tapped. A foreground layer, dismissed by its `×`, by Esc, or by tapping
/// the same part again. On a desktop it sits under the answer card; on a phone it is a bottom sheet.
#[allow(clippy::too_many_arguments)]
pub fn detail_card(
    ctx: &egui::Context,
    vis: &mut Visual,
    draft: Option<&mut Draft>,
    run: &Run,
    hits: &mut HitMap,
    phone: bool,
    anchor: egui::Rect,
    screen: egui::Rect,
) {
    let Some(detail) = vis.detail else { return };
    let Some(draft) = draft else { return };
    if vis.picker.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        vis.detail = None;
        return;
    }
    let (pos, w, max_h) = if phone {
        let h = (screen.height() * 0.52).max(240.0);
        (
            egui::pos2(screen.left() + 6.0, screen.bottom() - h - 6.0),
            screen.width() - 12.0,
            h,
        )
    } else {
        (
            anchor.min,
            anchor.width(),
            (screen.bottom() - anchor.top() - 12.0).max(160.0),
        )
    };
    egui::Area::new(egui::Id::new("viz.detail"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_width(w);
            let frame = egui::Frame::new()
                .fill(t::with_alpha(t::PANEL, 246))
                .stroke(Stroke::new(1.0, t::with_alpha(t::PRIMARY, 120)))
                .corner_radius(egui::CornerRadius::same(10))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha(110),
                })
                .inner_margin(egui::Margin::symmetric(14, 10));
            let shown = frame.show(ui, |ui| {
                ui.set_width(w - 28.0);
                ui.set_max_height(max_h - 20.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height(max_h - 20.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        // title row + close
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(title(detail, &draft.0))
                                    .size(14.0)
                                    .color(t::INK)
                                    .family(t::family_semi()),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let s = if phone { 44.0 } else { 28.0 };
                                    let b = ui.add_sized(egui::vec2(s, s), egui::Button::new("×"));
                                    if hit(hits, "ctl:detail-close", b)
                                        .on_hover_text("close (Esc)")
                                        .clicked()
                                    {
                                        vis.detail = None;
                                    }
                                },
                            );
                        });
                        match detail {
                            Detail::Bay(Slot::Fan) => fan_detail(ui, draft, run, vis, hits, phone),
                            Detail::Bay(Slot::Drift) => {
                                drift_detail(ui, draft, run, vis, hits, phone)
                            }
                            Detail::Bay(Slot::Fill) => {
                                fill_detail(ui, draft, run, vis, hits, phone)
                            }
                            Detail::Bay(Slot::Nozzle) => {
                                nozzle_detail(ui, draft, run, vis, hits, phone)
                            }
                            Detail::Op => op_detail(ui, run, vis, hits, phone),
                            Detail::Zone(z) => zone_detail(ui, &draft.0, run, z),
                        }
                        if let Some(o) = run.output.as_ref() {
                            ui.add_space(6.0);
                            breakdown(ui, o, highlight(detail));
                        }
                    });
            });
            let r = shown.response.rect;
            hits.0.push((
                format!("card:detail:{}", detail.slug()),
                [r.min.x, r.min.y, r.width(), r.height()],
            ));
        });
}

fn title(d: Detail, input: &EngineInput) -> String {
    match d {
        Detail::Bay(Slot::Fan) => format!("Fan · {}", input.fan.id),
        Detail::Bay(Slot::Drift) => format!("Drift eliminator · {}", input.drift.id),
        Detail::Bay(Slot::Fill) => format!(
            "Fill · {} layers · {:.2} m",
            input.fill_layers.len(),
            input.fill_layers.iter().map(|l| l.depth_m).sum::<f64>()
        ),
        Detail::Bay(Slot::Nozzle) => format!("Nozzles · {}", input.nozzle.id),
        Detail::Op => "Operating point".into(),
        Detail::Zone(z) => match z {
            ZoneId::Rain => "Rain zone + supports".into(),
            ZoneId::Inlet => "Inlet louvres".into(),
            ZoneId::Fixed => "Fixed losses · basin".into(),
            other => zone_name(other).into(),
        },
    }
}

fn highlight(d: Detail) -> Option<ZoneId> {
    match d {
        Detail::Bay(Slot::Fan) => Some(ZoneId::Stack),
        Detail::Bay(Slot::Drift) => Some(ZoneId::Drift),
        Detail::Bay(Slot::Fill) => Some(ZoneId::Fill),
        Detail::Bay(Slot::Nozzle) => Some(ZoneId::Spray),
        Detail::Op => Some(ZoneId::Plenum),
        Detail::Zone(z) => Some(z),
    }
}

/// One detail row: a mark, a label, a right-aligned value and unit.
fn drow(ui: &mut egui::Ui, src: Src, label: &str, value: &str, unit: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), Sense::hover());
        paint_mark(ui.painter(), r.center(), 3.2, src);
        ui.label(RichText::new(label).size(11.0).color(t::MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !unit.is_empty() {
                ui.label(RichText::new(unit).size(10.0).color(t::MUTED));
            }
            ui.label(
                RichText::new(value)
                    .size(12.0)
                    .color(t::INK)
                    .family(t::family_mono_med()),
            );
        });
    });
}

fn swap_button(ui: &mut egui::Ui, vis: &mut Visual, hits: &mut HitMap, slot: Slot, phone: bool) {
    let b = ui.add(
        egui::Button::new(RichText::new(format!("swap {} …", slot.name())).size(11.5))
            .min_size(egui::vec2(0.0, if phone { 44.0 } else { 26.0 })),
    );
    if hit(hits, &format!("ctl:swap:{}", slot.slug()), b).clicked() {
        open_picker(vis, slot, false);
    }
}

/// The fan's own detail: **fan speed lives here now** (part-load, turn-down) - the tachometer, the rpm, the
/// slider over the record's allowed band, one step down / up, and the fixture's own ratio.
fn fan_detail(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    run: &Run,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    let [lo, hi] = draft.0.fan.allowed_speed_ratio;
    // the one fan-speed control in the app, the same row the Curves view carries (ui.rs::fan_speed_controls)
    crate::ui::fan_speed_controls(ui, draft, run, vis, hits, phone);
    if let Some((rpm_lo, rpm_hi)) = m::rpm_range([lo, hi], draft.0.fan.nominal_rpm) {
        ui.label(
            RichText::new(format!(
                "turn-down {rpm_lo:.0}–{rpm_hi:.0} rpm (the record's {lo:.2}–{hi:.2})"
            ))
            .size(10.0)
            .color(t::MUTED),
        );
    }
    swap_button(ui, vis, hits, Slot::Fan, phone);
    if let Some(o) = run.output.as_ref() {
        drow(ui, Src::Calc, "fan power", &t::fmt(o.fan_power_kw), "kW");
        drow(ui, Src::Calc, "airflow", &t::fmt(o.airflow_m3_s), "m3/s");
        if let Some((pa, s)) = shown_share(o, ZoneId::Stack) {
            drow(
                ui,
                Src::Calc,
                "fan stack",
                &format!("{} Pa · {s}%", t::fmt(pa)),
                "",
            );
        }
    }
    drow(
        ui,
        Src::Catalog,
        "stack area",
        &t::fmt(draft.0.fan.stack_area_m2),
        "m2",
    );
}

fn drift_detail(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    run: &Run,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    if let Some(o) = run.output.as_ref() {
        if let Some((ppm, v)) = drift_ppm(&draft.0, o.airflow_m3_s) {
            drow(ui, Src::Catalog, "drift", &t::fmt(ppm), "ppm");
            drow(ui, Src::Calc, "face velocity", &format!("{v:.2}"), "m/s");
            let loss = ppm * 1e-6 * o.water_flow_m3_hr;
            drow(
                ui,
                Src::Catalog,
                "drift loss",
                &format!("{loss:.4}"),
                "m3/hr",
            );
        }
        if let Some((pa, s)) = shown_share(o, ZoneId::Drift) {
            drow(
                ui,
                Src::Calc,
                "pressure",
                &format!("{} Pa · {s}%", t::fmt(pa)),
                "",
            );
        }
    }
    drow(
        ui,
        Src::Catalog,
        "max water temperature",
        &t::fmt(draft.0.drift.max_water_temperature_c),
        "C",
    );
    swap_button(ui, vis, hits, Slot::Drift, phone);
}

/// The fill's detail: each layer's own numbers, their sum, and - stated as what it is - the tower's
/// available transfer (correctness item (c)).
fn fill_detail(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    run: &Run,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    if let Some(o) = run.output.as_ref() {
        for (i, l) in o.kavl_per_layer.iter().enumerate() {
            ui.label(
                RichText::new(format!("L{} {} · {:.2} m", i + 1, l.fill_id, l.depth_m))
                    .size(11.0)
                    .color(t::fill_color(&l.fill_id))
                    .family(t::family_mono_med()),
            );
            drow(
                ui,
                Src::Calc,
                "  pressure · KaV/L",
                &format!("{} Pa · {:.3}", t::fmt(l.pressure_pa), l.kavl),
                "",
            );
            if !l.inside_envelope {
                ui.label(
                    RichText::new("  outside the fill's envelope")
                        .size(10.0)
                        .color(t::DANGER),
                );
            }
        }
        let fk = fill_kavl(o);
        if let Some((pa, s)) = shown_share(o, ZoneId::Fill) {
            drow(
                ui,
                Src::Calc,
                "fill (layers summed)",
                &format!("{} Pa · {s}% · KaV/L {fk:.3}", t::fmt(pa)),
                "",
            );
        }
        drow(
            ui,
            Src::Calc,
            "+ spray and rain zones",
            &format!("KaV/L {:.3}", o.kavl_total - fk),
            "",
        );
        drow(
            ui,
            Src::Calc,
            "= available transfer",
            &format!("KaV/L {:.3}", o.kavl_total),
            "",
        );
    }
    ui.horizontal_wrapped(|ui| {
        swap_button(ui, vis, hits, Slot::Fill, phone);
        let b = ui.add(
            egui::Button::new(RichText::new("edit the stack …").size(11.5))
                .min_size(egui::vec2(0.0, if phone { 44.0 } else { 26.0 })),
        );
        if hit(hits, "ctl:edit-stack", b).clicked() {
            vis.panel_open = true;
            vis.detail = None;
        }
    });
    let _ = draft;
}

fn nozzle_detail(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    run: &Run,
    vis: &mut Visual,
    hits: &mut HitMap,
    phone: bool,
) {
    drow(
        ui,
        Src::Catalog,
        "orifice",
        &format!("{:.1}", draft.0.nozzle.orifice_diameter_m * 1000.0),
        "mm",
    );
    // the bank's two editor controls (an editor value, not an engine input) and its coverage badge
    crate::ui::nozzle_bank(ui, draft, vis, hits, phone);
    if let Some(o) = run.output.as_ref() {
        if let Some((pa, s)) = shown_share(o, ZoneId::Spray) {
            drow(
                ui,
                Src::Calc,
                "spray zone",
                &format!("{} Pa · {s}%", t::fmt(pa)),
                "",
            );
        }
    }
    swap_button(ui, vis, hits, Slot::Nozzle, phone);
}

fn op_detail(ui: &mut egui::Ui, run: &Run, vis: &mut Visual, hits: &mut HitMap, phone: bool) {
    if let Some(o) = run.output.as_ref() {
        let op = &o.fan_system_curve.operating_point;
        drow(ui, Src::Calc, "airflow", &t::fmt(op.x), "m3/s");
        drow(
            ui,
            Src::Calc,
            "pressure (fan = system)",
            &t::fmt(op.y),
            "Pa",
        );
        drow(
            ui,
            Src::Calc,
            "air path, zones summed",
            &t::fmt(o.total_pressure_pa),
            "Pa",
        );
        if let Some((pa, s)) = shown_share(o, ZoneId::Plenum) {
            drow(
                ui,
                Src::Calc,
                "plenum",
                &format!("{} Pa · {s}%", t::fmt(pa)),
                "",
            );
        }
    }
    let b = ui.add(
        egui::Button::new(RichText::new("fan vs system curve → Curves").size(11.5))
            .min_size(egui::vec2(0.0, if phone { 44.0 } else { 26.0 })),
    );
    if hit(hits, "ctl:to-curves", b).clicked() {
        vis.view = View::Curves;
        vis.detail = None;
    }
}

fn zone_detail(ui: &mut egui::Ui, input: &EngineInput, run: &Run, z: ZoneId) {
    if let Some(o) = run.output.as_ref() {
        if let Some((pa, s)) = shown_share(o, z) {
            drow(
                ui,
                Src::Calc,
                "pressure",
                &format!("{} Pa · {s}%", t::fmt(pa)),
                "",
            );
        }
    }
    let tw = &input.tower;
    match z {
        ZoneId::Inlet => {
            drow(
                ui,
                Src::Catalog,
                "inlet area",
                &t::fmt(tw.inlet_area_m2),
                "m2",
            );
            drow(
                ui,
                Src::Catalog,
                "loss coefficient",
                &format!("{:.2}", tw.inlet_loss_coefficient),
                "",
            );
        }
        ZoneId::Rain => {
            drow(
                ui,
                Src::Catalog,
                "rain zone height",
                &format!("{:.2}", tw.rain_zone_height_m),
                "m",
            );
            drow(
                ui,
                Src::Catalog,
                "support loss coefficient",
                &format!("{:.2}", tw.support_loss_coefficient),
                "",
            );
        }
        ZoneId::Fixed => {
            drow(
                ui,
                Src::Catalog,
                "recorded fixed loss",
                &t::fmt(tw.fixed_pressure_loss_pa),
                "Pa",
            );
            if let Some(o) = run.output.as_ref() {
                drow(
                    ui,
                    Src::Calc,
                    "water flow",
                    &t::fmt(o.water_flow_m3_hr),
                    "m3/hr",
                );
            }
        }
        _ => {}
    }
}

/// The whole air path, one row per zone with the fill as one row, the tapped zone lit, and the total - the
/// shares on screen add to 100 (correctness item (b)).
fn breakdown(ui: &mut egui::Ui, o: &EngineOutput, lit: Option<ZoneId>) {
    let rows = air_path(o);
    ui.label(t::eyebrow("air path · pressure by zone"));
    let total: f64 = rows.iter().map(|r| r.pa).sum();
    let bar_w = ui.available_width();
    // one stacked bar
    let (bar, _) = ui.allocate_exact_size(egui::vec2(bar_w, 8.0), Sense::hover());
    let mut x = bar.left();
    for r in rows.iter() {
        let w = bar.width() * (r.pa / total.max(1e-9)) as f32;
        let seg = egui::Rect::from_min_size(egui::pos2(x, bar.top()), egui::vec2(w, bar.height()));
        let on = Some(r.zone) == lit;
        ui.painter().rect_filled(
            seg.shrink2(egui::vec2(0.5, 0.0)),
            egui::CornerRadius::same(1),
            if on {
                t::PRIMARY
            } else {
                t::with_alpha(t::INK_2, 70)
            },
        );
        x += w;
    }
    egui::Grid::new("viz.detail.path")
        .num_columns(3)
        .spacing(egui::vec2(10.0, 1.0))
        .show(ui, |ui| {
            for r in rows.iter() {
                let on = Some(r.zone) == lit;
                let c = if on { t::INK } else { t::MUTED };
                ui.label(RichText::new(r.name).size(10.5).color(c));
                ui.label(
                    RichText::new(format!("{} Pa", t::fmt(r.pa)))
                        .size(10.5)
                        .color(c)
                        .family(t::family_mono_med()),
                );
                ui.label(
                    RichText::new(format!("{}%", r.share_shown))
                        .size(10.5)
                        .color(c)
                        .family(t::family_mono_med()),
                );
                ui.end_row();
            }
            ui.label(RichText::new("total").size(10.5).color(t::INK_2));
            ui.label(
                RichText::new(format!("{} Pa", t::fmt(total)))
                    .size(10.5)
                    .color(t::INK_2)
                    .family(t::family_mono_med()),
            );
            ui.label(
                RichText::new(format!(
                    "{}%",
                    rows.iter().map(|r| r.share_shown).sum::<i64>()
                ))
                .size(10.5)
                .color(t::INK_2)
                .family(t::family_mono_med()),
            );
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::engine::Engine;
    use cockpit::fixture_engine::FixtureEngine;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    /// Correctness (b): the shares the screen states add to exactly 100, and each is within a point of the
    /// engine's own share.
    #[test]
    fn the_air_path_shares_shown_add_to_100() {
        let e = FixtureEngine::from_json(FIXTURE).unwrap();
        let o = e.run(&e.default_input()).unwrap();
        let rows = air_path(&o);
        assert_eq!(rows.iter().map(|r| r.share_shown).sum::<i64>(), 100);
        for r in rows.iter() {
            assert!((r.share - r.share_shown as f64).abs() < 1.0, "{r:?}");
        }
        // one fill row, the per-layer rows summed
        assert_eq!(rows.iter().filter(|r| r.zone == ZoneId::Fill).count(), 1);
        let layer_pa: f64 = o
            .pressure_by_zone
            .iter()
            .filter(|z| z.zone == ZoneId::Fill)
            .map(|z| z.pressure_pa)
            .sum();
        let fill = rows.iter().find(|r| r.zone == ZoneId::Fill).unwrap();
        assert!((fill.pa - layer_pa).abs() < 1e-9);
    }

    /// Correctness (c): the fill's KaV/L is its layers' sum; the engine's total is fill + spray + rain.
    #[test]
    fn the_fill_kavl_is_the_layers_sum_and_the_total_is_larger() {
        let e = FixtureEngine::from_json(FIXTURE).unwrap();
        let o = e.run(&e.default_input()).unwrap();
        let f = fill_kavl(&o);
        assert!((f - (o.kavl_per_layer[0].kavl + o.kavl_per_layer[1].kavl)).abs() < 1e-12);
        assert!(o.kavl_total > f, "available transfer includes spray + rain");
    }

    /// The drift reading reproduces the fixture's recorded drift ppm at the recorded airflow.
    #[test]
    fn drift_ppm_reads_the_records_curve_at_the_engines_airflow() {
        let e = FixtureEngine::from_json(FIXTURE).unwrap();
        let input = e.default_input();
        let o = e.run(&input).unwrap();
        let (ppm, v) = drift_ppm(&input, o.airflow_m3_s).unwrap();
        assert!(v > 0.0);
        assert!((ppm - 9.466).abs() < 0.01, "{ppm}");
    }

    #[test]
    fn largest_remainder_keeps_the_total() {
        assert_eq!(largest_remainder(&[33.4, 33.3, 33.3]), vec![34, 33, 33]);
        assert_eq!(largest_remainder(&[50.5, 49.5]).iter().sum::<i64>(), 100);
    }
}
