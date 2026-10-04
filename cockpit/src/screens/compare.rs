//! **Compare** (#89): two or three variants side by side, the differences highlighted.
//!
//! A variant is a **saved `.drafthouse` project file**, opened through #74's reader and recomputed
//! by the engine on open ([`crate::compare`] owns that model; this module only draws it). Nothing
//! here edits the draft or trusts a file's saved results: every number on the surface is the
//! engine's own run of the file's inputs, and the engine's own refusal is shown when it declines.
//!
//! Tap a column's head to make it the baseline; a cell whose value differs from the baseline by
//! more than the row's displayed precision is lit and carries the direction arrow; the best value
//! per engineering objective is filled (`best in row`). At phone width the same matrix compresses:
//! the label column narrows, the columns stay side by side, and the arrows drop their delta text -
//! the design round's own arrangement (`docs/design/screens-r2/final/compare-base-b-390.png`).
//!
//! Geometry is one source for the painter and the four-size tests ([`layout`] plus the cell
//! functions below), the same discipline the section drawing's coverage uses in `crossflow.rs`.

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Rect, Stroke, StrokeKind};
use cockpit::engine::EngineInput;

use super::kit::{self, text, text_fit, Src};
use super::{info_card, title_band, toast, toggle_info, Env, State};
use crate::compare::{self, Metric, Variant};
use crate::theme as t;

/// A delta that is worse than the baseline. Not DANGER: the theme keeps red for refusals and limits.
const WORSE: Color32 = Color32::from_rgb(0xd9, 0x8f, 0x8f);

/// The smallest row the layout will draw a table at; the heads give up height before the rows do.
const ROW_MIN: f32 = 16.0;

// ============================================================================================ layout

/// The comparison's geometry at one size: every rect the painter draws into, computed once. The
/// tests assert on these same rects (see the module tests), so an arrangement change that breaks
/// the promises fails there, not on a user's screen.
pub struct L {
    pub phone: bool,
    pub inner: Rect,
    /// The label column: the legend block (desktop) and every row's label.
    pub label: Rect,
    /// One head per variant: the section drawing, the file name and the change line.
    pub heads: Vec<Rect>,
    /// The key-result rows: label column + one value column per variant.
    pub key: Rect,
    /// The per-zone pressure rows, under the key rows.
    pub zones: Rect,
    /// The status row: one cell per variant (accepted / refused with the engine's own reason).
    pub status: Rect,
    /// The export control.
    pub foot: Rect,
    pub label_w: f32,
    pub gap: f32,
    pub col_w: f32,
    pub head_h: f32,
    pub row_h: f32,
}

/// Lay the comparison out in `body` (the rect below the title band). `k_rows` is the key-result row
/// count and `z_rows` the zone row count; the rows get height before the heads do, so a short
/// screen keeps every row legible and the section drawings take what is left.
pub fn layout(body: Rect, phone: bool, n: usize, k_rows: usize, z_rows: usize) -> L {
    let pad = if phone { 10.0 } else { 24.0 };
    let inner = body.shrink2(vec2(pad, 10.0));
    let label_w = if phone { 78.0 } else { 150.0 };
    let gap = if phone { 5.0 } else { 14.0 };
    let n = n.max(1);
    let col_w = ((inner.width() - label_w - gap * n as f32) / n as f32).max(24.0);
    let rows = (k_rows + z_rows + 1) as f32;
    let foot_h = if phone { 40.0 } else { 48.0 };
    let gap_h = 12.0;
    let head_max = if phone { 148.0 } else { 218.0 };
    let reserve_bottom = foot_h + 10.0;
    let head_h = (inner.height() - rows * ROW_MIN - reserve_bottom - gap_h).clamp(120.0, head_max);
    let head_x0 = inner.left() + label_w + gap;
    let heads = (0..n)
        .map(|i| {
            Rect::from_min_size(
                pos2(head_x0 + (col_w + gap) * i as f32, inner.top()),
                vec2(col_w, head_h),
            )
        })
        .collect();
    let top = inner.top() + head_h + gap_h;
    let avail = (inner.bottom() - reserve_bottom - top).max(rows * 8.0);
    let row_h = (avail / rows).clamp(8.0, 34.0);
    let key = Rect::from_min_size(
        pos2(inner.left(), top),
        vec2(inner.width(), k_rows as f32 * row_h),
    );
    let zones = Rect::from_min_size(
        pos2(inner.left(), key.bottom()),
        vec2(inner.width(), z_rows as f32 * row_h),
    );
    let status = Rect::from_min_size(
        pos2(inner.left(), zones.bottom()),
        vec2(inner.width(), row_h),
    );
    let foot = if phone {
        Rect::from_min_size(
            pos2(inner.left(), inner.bottom() - foot_h),
            vec2(inner.width(), foot_h),
        )
    } else {
        Rect::from_min_size(
            pos2(inner.right() - 300.0, inner.bottom() - foot_h),
            vec2(300.0, foot_h),
        )
    };
    L {
        phone,
        inner,
        label: Rect::from_min_size(inner.min, vec2(label_w, head_h)),
        heads,
        key,
        zones,
        status,
        foot,
        label_w,
        gap,
        col_w,
        head_h,
        row_h,
    }
}

/// The x of value column `col`.
fn col_x(l: &L, col: usize) -> f32 {
    l.inner.left() + l.label_w + l.gap + (l.col_w + l.gap) * col as f32
}

/// A key-result value cell: row `row`, variant column `col`.
pub fn key_cell(l: &L, row: usize, col: usize) -> Rect {
    Rect::from_min_size(
        pos2(col_x(l, col), l.key.top() + l.row_h * row as f32),
        vec2(l.col_w, l.row_h),
    )
}

/// A key-result label cell (the row name and its unit).
pub fn key_label(l: &L, row: usize) -> Rect {
    Rect::from_min_size(
        pos2(l.inner.left(), l.key.top() + l.row_h * row as f32),
        vec2(l.label_w, l.row_h),
    )
}

/// A zone row's value cell.
pub fn zone_cell(l: &L, row: usize, col: usize) -> Rect {
    Rect::from_min_size(
        pos2(col_x(l, col), l.zones.top() + l.row_h * row as f32),
        vec2(l.col_w, l.row_h),
    )
}

/// A zone row's label cell.
pub fn zone_label(l: &L, row: usize) -> Rect {
    Rect::from_min_size(
        pos2(l.inner.left(), l.zones.top() + l.row_h * row as f32),
        vec2(l.label_w, l.row_h),
    )
}

/// The status row's cell for a variant.
pub fn status_cell(l: &L, col: usize) -> Rect {
    Rect::from_min_size(pos2(col_x(l, col), l.status.top()), vec2(l.col_w, l.row_h))
}

/// The name line inside a head.
pub fn head_name(l: &L, i: usize) -> Rect {
    let r = l.heads[i];
    Rect::from_min_max(
        pos2(r.left() + 12.0, r.top() + 8.0),
        pos2(r.right() - 12.0, r.top() + 30.0),
    )
}

/// The change line inside a head (what differs from the baseline input, or `baseline`).
pub fn head_note(l: &L, i: usize) -> Rect {
    let r = l.heads[i];
    let h = if l.phone { 20.0 } else { 24.0 };
    Rect::from_min_max(
        pos2(r.left() + 12.0, r.bottom() - h - 4.0),
        pos2(r.right() - 12.0, r.bottom() - 4.0),
    )
}

/// The section drawing inside a head.
pub fn head_section(l: &L, i: usize) -> Rect {
    let r = l.heads[i];
    Rect::from_min_max(
        pos2(r.left() + 6.0, r.top() + 34.0),
        pos2(
            r.right() - 6.0,
            r.bottom() - if l.phone { 28.0 } else { 32.0 },
        ),
    )
}

// ========================================================================================= the paint

pub fn ui(ui: &mut egui::Ui, st: &mut State, _draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(
        ui,
        area,
        "Compare",
        if env.phone {
            "2-3 saved variants · what changes"
        } else {
            "2-3 saved project files · differences highlighted"
        },
        env.phone,
    );
    let p = ui.painter().clone();
    if st.compare.variants.is_empty() {
        empty(&p, body, st, env.phone);
        return;
    }
    // The variants are the loaded files' full engine runs; the per-frame clone is the old screen's
    // own pattern (a handful of KB), and it keeps the paint code free of borrow gymnastics.
    let variants = st.compare.variants.clone();
    let n = variants.len();
    let base = st.base.min(n - 1);
    let z_labels = compare::zone_labels(&variants);
    let l = layout(body, env.phone, n, Metric::ALL.len(), z_labels.len());

    // ---- the heads: the section, the file, what differs from the baseline
    let mut tapped: Option<usize> = None;
    for (i, v) in variants.iter().enumerate() {
        let r = l.heads[i];
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
        // the name line: the column letter, then the file's own name (fitted; the digest is exact)
        let k = text(
            &p,
            head_name(&l, i).left_top(),
            Align2::LEFT_TOP,
            &format!("{}", column_letter(i)),
            kit::num(if env.phone { 14.0 } else { 17.0 }),
            if is_base { t::INK } else { t::PRIMARY },
        );
        text_fit(
            &p,
            pos2(k.right() + 6.0, k.top() + 2.0),
            Align2::LEFT_TOP,
            &v.name,
            kit::semi(if env.phone { 11.0 } else { 12.5 }),
            t::INK,
            (head_name(&l, i).right() - k.right() - 12.0).max(20.0),
        );
        text_fit(
            &p,
            pos2(head_name(&l, i).right(), head_name(&l, i).top() + 3.0),
            Align2::RIGHT_TOP,
            &v.digest[..8],
            kit::mono(9.5),
            t::MUTED,
            l.col_w * 0.35,
        );
        section(
            &p,
            head_section(&l, i),
            &v.input,
            &variants[base].input,
            env.t,
            v.output(),
        );
        let declined = compare::verdict(v).declined();
        let note = if is_base {
            "baseline".to_string()
        } else if declined {
            "refused".to_string()
        } else {
            let changed = compare::inputs_changed(&variants[base].input, &v.input);
            if changed.is_empty() {
                "same inputs".to_string()
            } else {
                changed.join(" · ")
            }
        };
        let ncol = if is_base {
            t::INK_2
        } else if declined {
            t::DANGER
        } else {
            t::PRIMARY
        };
        let nr = head_note(&l, i);
        text_fit(
            &p,
            nr.left_top(),
            Align2::LEFT_TOP,
            &note,
            kit::semi(if env.phone { 11.0 } else { 12.0 }),
            ncol,
            nr.width(),
        );
        if resp.clicked() {
            tapped = Some(i);
        }
    }
    if let Some(i) = tapped {
        st.base = i;
    }

    // ---- the label column: the legend (desktop) and the rows' labels
    let info_open = st.info.as_deref() == Some("cmp:i");
    if legend(ui, &l, info_open) {
        toggle_info(st, "cmp:i");
    }

    // ---- the key rows
    let values: Vec<Vec<Option<f64>>> = Metric::ALL
        .iter()
        .map(|m| compare::values(*m, &variants))
        .collect();
    for (ri, m) in Metric::ALL.iter().enumerate() {
        let lc = key_label(&l, ri);
        let lr = text(
            &p,
            pos2(
                lc.left(),
                lc.center().y - if m.unit().is_empty() { 0.0 } else { 7.0 },
            ),
            Align2::LEFT_CENTER,
            m.label(),
            kit::semi(if env.phone { 10.5 } else { 12.5 }),
            t::INK_2,
        );
        text(
            &p,
            pos2(lc.left(), lr.bottom() + 1.0),
            Align2::LEFT_TOP,
            m.unit(),
            kit::sans(10.0),
            t::MUTED,
        );
        let best = compare::best_column(*m, &values[ri]);
        for (col, v) in variants.iter().enumerate() {
            let cell = key_cell(&l, ri, col);
            cell_bg(
                &p,
                if ri.is_multiple_of(2) {
                    Some(cell)
                } else {
                    None
                },
            );
            paint_value(
                ui,
                &l,
                cell,
                *m,
                v,
                compare::value(*m, &variants[base]),
                Some(col) == best,
                col == base,
                &format!("the engine's run of {} · {}", v.name, m.label()),
            );
        }
    }

    // ---- the zone rows: the engine's own air-path split, per variant (Pa · share%), by label
    p.line_segment(
        [l.zones.left_top(), l.zones.right_top()],
        Stroke::new(1.0, t::LINE_SOFT),
    );
    for (zi, label) in z_labels.iter().enumerate() {
        let lc = zone_label(&l, zi);
        text_fit(
            &p,
            pos2(lc.left(), lc.center().y),
            Align2::LEFT_CENTER,
            label,
            kit::semi(if env.phone { 10.0 } else { 11.5 }),
            t::INK_2,
            lc.width() - 4.0,
        );
        let base_pa = compare::zone_of(&variants[base], label).map(|(pa, _)| pa);
        for (col, v) in variants.iter().enumerate() {
            let cell = zone_cell(&l, zi, col);
            cell_bg(
                &p,
                if (Metric::ALL.len() + zi).is_multiple_of(2) {
                    Some(cell)
                } else {
                    None
                },
            );
            match compare::zone_of(v, label) {
                Some((pa, share)) => {
                    let lit = compare::changed(1, base_pa, Some(pa)) && col != base;
                    if lit {
                        p.rect_filled(cell.shrink2(vec2(2.0, 3.0)), kit::r(6), t::PANEL_RAISED);
                    }
                    let vr = text(
                        &p,
                        pos2(
                            cell.left() + if env.phone { 6.0 } else { 10.0 },
                            cell.center().y,
                        ),
                        Align2::LEFT_CENTER,
                        &format!("{pa:.1}·{share:.0}%"),
                        kit::num(if env.phone { 12.0 } else { 14.0 }),
                        if lit { t::PRIMARY } else { t::INK },
                    );
                    if lit {
                        arrow(
                            &p,
                            pos2(vr.right() + 9.0, cell.center().y),
                            pa > base_pa.unwrap_or(pa),
                            t::INK_2,
                        );
                    }
                }
                None => {
                    text(
                        &p,
                        cell.center(),
                        Align2::CENTER_CENTER,
                        "-",
                        kit::sans(11.0),
                        t::MUTED,
                    );
                }
            }
        }
    }

    // ---- the status row: the engine's verdict per variant, in its own words
    p.line_segment(
        [l.status.left_top(), l.status.right_top()],
        Stroke::new(1.0, t::LINE_SOFT),
    );
    let sr = Rect::from_min_size(
        pos2(l.inner.left(), l.status.top()),
        vec2(l.label_w, l.row_h),
    );
    text(
        &p,
        pos2(sr.left(), sr.center().y),
        Align2::LEFT_CENTER,
        "status",
        kit::semi(if env.phone { 10.0 } else { 12.0 }),
        t::INK_2,
    );
    for (col, v) in variants.iter().enumerate() {
        let cell = status_cell(&l, col);
        let (line, colr) = match compare::verdict(v) {
            compare::Verdict::Accepted => ("accepted".to_string(), t::OK),
            compare::Verdict::Refused(why) => (format!("refused · {}", why[0]), t::DANGER),
        };
        text_fit(
            &p,
            pos2(
                cell.left() + if env.phone { 6.0 } else { 10.0 },
                cell.center().y,
            ),
            Align2::LEFT_CENTER,
            &line,
            kit::sans(if env.phone { 10.5 } else { 11.5 }),
            colr,
            cell.width() - 14.0,
        );
    }

    // ---- the export: the same comparison as its own sheet, through the report's own delivery path
    let demo = st.account == super::Account::Demo;
    let label = if demo {
        "Export comparison (DEMO watermark)"
    } else {
        "Export comparison"
    };
    let resp = kit::button(ui, l.foot, "cmp.export", label, kit::Btn::Primary);
    if resp.clicked() {
        match compare::export_pdf(&st.compare.variants, demo) {
            Ok(sheet) => match super::report_pdf::deliver(&sheet.bytes, &sheet.name) {
                Ok(done) => toast(st, &done),
                Err(e) => toast(st, &format!("export failed: {e}")),
            },
            Err(e) => toast(st, &format!("export failed: {e}")),
        }
    }
    if !env.phone {
        text(
            &p,
            pos2(l.foot.left() - 16.0, l.foot.center().y),
            Align2::RIGHT_CENTER,
            "a sheet of this comparison",
            kit::sans(11.5),
            t::MUTED,
        );
    }
    // the last named refusal from loading (a file that could not open), left of the export
    if !env.phone {
        if let Some(problem) = st.compare.problems.last() {
            text_fit(
                &p,
                pos2(l.inner.left(), l.foot.center().y),
                Align2::LEFT_CENTER,
                problem,
                kit::sans(11.0),
                t::DANGER,
                (l.foot.left() - l.inner.left() - 340.0).max(120.0),
            );
        }
    }

    info_card(
        ui,
        st,
        "cmp:i",
        if env.phone {
            pos2(l.inner.left() + 10.0, l.inner.top() + l.head_h + 8.0)
        } else {
            pos2(l.foot.left() - 320.0, l.foot.top())
        },
        &[
            "Compare",
            "Each column is a saved project file,",
            "recomputed by the engine on open.",
            "Tap a head to make it the baseline;",
            "a lit cell differs from it.",
        ],
        area,
    );
}

/// The letter a variant's column carries (A, B, C… by position).
fn column_letter(i: usize) -> char {
    char::from(b'A' + (i as u8).min(25))
}

/// The subtle row stripe: every even row gets a background band (the old screen's own rule).
fn cell_bg(p: &egui::Painter, cell: Option<Rect>) {
    if let Some(cell) = cell {
        p.rect_filled(cell, kit::r(6), t::with_alpha(t::PANEL, 120));
    }
}

/// One key-result cell: refused, or a value with the changed/best rules applied.
#[allow(clippy::too_many_arguments)]
fn paint_value(
    ui: &egui::Ui,
    l: &L,
    cell: Rect,
    metric: Metric,
    v: &Variant,
    base: Option<f64>,
    best: bool,
    is_base: bool,
    src: &str,
) {
    let p = ui.painter();
    let Some(value) = compare::value(metric, v) else {
        text(
            p,
            cell.center(),
            Align2::CENTER_CENTER,
            "refused",
            kit::sans(11.0),
            t::DANGER,
        );
        return;
    };
    let lit = !is_base && compare::changed(metric.decimals(), base, Some(value));
    if best {
        p.rect_filled(cell, kit::r(6), t::PRIMARY_SOFT);
        p.rect_stroke(
            cell,
            kit::r(6),
            Stroke::new(1.0, t::PRIMARY_DEEP),
            StrokeKind::Inside,
        );
    } else if lit {
        p.rect_filled(cell.shrink2(vec2(2.0, 3.0)), kit::r(6), t::PANEL_RAISED);
    }
    let vr0 = text(
        p,
        pos2(
            cell.left() + if l.phone { 6.0 } else { 10.0 },
            cell.center().y,
        ),
        Align2::LEFT_CENTER,
        &compare::display(metric, value),
        kit::num(if l.phone { 13.0 } else { 16.0 }),
        if lit { t::PRIMARY } else { t::INK },
    );
    let mk = kit::src_mark(p, pos2(vr0.right(), vr0.center().y), Src::Calc, src);
    let _ = vr0.union(mk);
    if lit {
        let d = value - base.unwrap_or(value);
        let good = metric.better().map(|b| match b {
            compare::Better::Lower => d < 0.0,
            compare::Better::Higher => d > 0.0,
        });
        let colr = match good {
            Some(true) => t::OK,
            Some(false) => WORSE,
            None => t::INK_2,
        };
        arrow(p, pos2(cell.right() - 8.0, cell.center().y), d > 0.0, colr);
        if !l.phone {
            text(
                p,
                pos2(cell.right() - 22.0, cell.center().y),
                Align2::RIGHT_CENTER,
                &format!(
                    "{}{:.*}",
                    if d > 0.0 { "+" } else { "−" },
                    metric.decimals(),
                    d.abs()
                ),
                kit::num(13.0),
                colr,
            );
        }
    }
}

/// The legend in the label column (desktop): what the fill and the arrows mean. Returns whether the
/// info dot was clicked.
fn legend(ui: &egui::Ui, l: &L, info_open: bool) -> bool {
    let p = ui.painter();
    if !l.phone {
        let mut y = l.label.top() + 12.0;
        let chip = Rect::from_min_size(pos2(l.label.left(), y), vec2(26.0, 18.0));
        p.rect_filled(chip, kit::r(4), t::PRIMARY_SOFT);
        p.rect_stroke(
            chip,
            kit::r(4),
            Stroke::new(1.0, t::PRIMARY_DEEP),
            StrokeKind::Inside,
        );
        text(
            p,
            pos2(chip.right() + 8.0, chip.center().y),
            Align2::LEFT_CENTER,
            "best in row",
            kit::sans(12.0),
            t::INK_2,
        );
        y += 30.0;
        for (colr, label) in [
            (t::OK, "better"),
            (WORSE, "worse"),
            (t::INK_2, "neither (airflow)"),
        ] {
            arrow(p, pos2(l.label.left() + 8.0, y + 8.0), true, colr);
            text(
                p,
                pos2(l.label.left() + 34.0, y + 8.0),
                Align2::LEFT_CENTER,
                label,
                kit::sans(12.0),
                t::INK_2,
            );
            y += 24.0;
        }
        text(
            p,
            pos2(l.label.left(), y + 2.0),
            Align2::LEFT_TOP,
            "than the baseline;",
            kit::sans(11.5),
            t::MUTED,
        );
        text(
            p,
            pos2(l.label.left(), y + 18.0),
            Align2::LEFT_TOP,
            "lit = differs",
            kit::sans(11.5),
            t::MUTED,
        );
        y += 34.0;
        let outlined = Rect::from_min_size(pos2(l.label.left(), y), vec2(26.0, 14.0));
        p.rect_stroke(
            outlined,
            kit::r(0),
            Stroke::new(2.0, t::PRIMARY),
            StrokeKind::Inside,
        );
        text(
            p,
            pos2(outlined.right() + 8.0, outlined.center().y),
            Align2::LEFT_CENTER,
            "changed part",
            kit::sans(12.0),
            t::INK_2,
        );
    }
    let idot = if l.phone {
        pos2(l.inner.left() + 10.0, l.inner.top() + l.head_h + 8.0)
    } else {
        pos2(l.label.left() + 8.0, l.label.bottom() - 16.0)
    };
    kit::info_dot(ui, idot, "cmp.i", info_open).clicked()
}

/// The empty state: nothing loaded, and the honest list of what was refused.
fn empty(p: &egui::Painter, body: Rect, st: &State, phone: bool) {
    let c = body.center();
    text(
        p,
        pos2(c.x, c.y - if phone { 40.0 } else { 46.0 }),
        Align2::CENTER_CENTER,
        "no variants loaded",
        kit::semi(15.0),
        t::INK_2,
    );
    text(
        p,
        pos2(c.x, c.y - 14.0),
        Align2::CENTER_CENTER,
        "open two or three saved project files to compare them side by side",
        kit::sans(if phone { 11.5 } else { 13.0 }),
        t::MUTED,
    );
    text(
        p,
        pos2(c.x, c.y + 10.0),
        Align2::CENTER_CENTER,
        "load them with --compare a.drafthouse b.drafthouse",
        kit::mono(11.0),
        t::MUTED,
    );
    for (i, problem) in st.compare.problems.iter().rev().take(3).enumerate() {
        text_fit(
            p,
            pos2(c.x, c.y + 44.0 + 18.0 * i as f32),
            Align2::CENTER_CENTER,
            problem,
            kit::sans(11.0),
            t::DANGER,
            body.width() - 48.0,
        );
    }
}

fn arrow(p: &egui::Painter, c: egui::Pos2, up: bool, col: Color32) {
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
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// A small tower section: the fan stack, the eliminator, the fill layers to scale, the basin. Parts that
/// differ from the baseline input are outlined in the primary colour; the fan turns at the variant's speed.
fn section(
    p: &egui::Painter,
    r: Rect,
    inp: &EngineInput,
    base: &EngineInput,
    tt: f32,
    out: Option<&cockpit::engine::EngineOutput>,
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
    p.add(egui::Shape::convex_polygon(
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
    // eliminator (outlined when the variant carries a different one)
    let de = Rect::from_min_size(
        pos2(body.left() + 4.0, body.top() + 6.0),
        vec2(body.width() - 8.0, 8.0),
    );
    let de_changed = inp.drift.id != base.drift.id;
    let mut zig = Vec::new();
    for i in 0..=16 {
        let x = de.left() + de.width() * i as f32 / 16.0;
        zig.push(pos2(x, if i % 2 == 0 { de.top() } else { de.bottom() }));
    }
    p.add(egui::Shape::line(
        zig,
        Stroke::new(
            if de_changed { 1.8 } else { 1.0 },
            if de_changed { t::PRIMARY } else { t::INK_2 },
        ),
    ));
    // fill layers, depth to scale (top of the stack = first layer)
    let fill_top = de.bottom() + 16.0;
    let fill_h_max = body.bottom() - 26.0 - fill_top;
    let mut y = fill_top;
    for (li, layer) in inp.fill_layers.iter().enumerate() {
        let h = fill_h_max * (layer.depth_m / max_total) as f32;
        let fr = Rect::from_min_size(pos2(body.left() + 4.0, y), vec2(body.width() - 8.0, h));
        let changed = base
            .fill_layers
            .get(li)
            .map(|b| (b.depth_m - layer.depth_m).abs() > 1e-6 || b.fill_id != layer.fill_id)
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
                &format!("{:.2} m", layer.depth_m),
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

#[cfg(test)]
mod tests {
    //! Issue #89 AC 3: the comparison at the four Phase-1 sizes, asserted on the same rects the
    //! painter draws into ([`layout`] + the cell functions - `ui` computes every rect through them,
    //! nothing at its own coordinates) and on the app's own fonts, the way `crossflow.rs` covers
    //! the section drawing: every row label, unit, value, zone cell and status word the real
    //! committed fixture variants produce must fit its own cell, measured with `t::fonts`
    //! installed.
    use super::*;
    use bevy_egui::egui::FontId;

    /// The rect the shell hands `ui` at the four sizes (the rail layout, less the answer strip and,
    /// on desktop, the title band) - the same targets `crossflow.rs` covers.
    const TARGETS: [(&str, f32, f32, bool); 4] = [
        ("1280x720", 1204.0, 604.0, false),
        ("1440x900", 1364.0, 784.0, false),
        ("1024x768", 948.0, 652.0, false),
        ("390x844", 390.0, 742.0, true),
    ];

    fn area(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
    }

    /// A painter over a real font context carrying the app's own fonts (`ui.rs` installs the same
    /// [`t::fonts`]; a pass activates them). Measuring through it is measuring what the user sees.
    fn font_painter() -> egui::Painter {
        let ctx = egui::Context::default();
        ctx.set_fonts(t::fonts());
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        egui::Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING)
    }

    fn inside(outer: Rect, r: Rect) -> bool {
        r.left() >= outer.left() - 0.5
            && r.top() >= outer.top() - 0.5
            && r.right() <= outer.right() + 0.5
            && r.bottom() <= outer.bottom() + 0.5
    }

    fn fixture_variants() -> Vec<crate::compare::Variant> {
        crate::compare::fixture_variants(&[
            "a-base.drafthouse",
            "b-fan-faster.drafthouse",
            "c-recorded.drafthouse",
        ])
    }

    /// The matrix holds at the four sizes: the heads are the columns (side by side, one per
    /// variant, inside the body), the key rows, zone rows and status row are contiguous below
    /// them, every rect sits inside the body, and the export control keeps its own strip.
    #[test]
    fn the_matrix_holds_at_the_four_sizes() {
        let variants = fixture_variants();
        assert!(variants.len() >= 2, "the fixture files open");
        let z_rows = crate::compare::zone_labels(&variants).len();
        assert!(z_rows > 0);
        for (label, w, h, phone) in TARGETS {
            let body = area(w, h);
            let l = layout(body, phone, variants.len(), Metric::ALL.len(), z_rows);
            for (what, r) in [
                ("the label column", l.label),
                ("the key rows", l.key),
                ("the zone rows", l.zones),
                ("the status row", l.status),
                ("the export control", l.foot),
            ] {
                assert!(inside(l.inner, r), "{label}: {what} leaves the body");
                assert!(
                    r.width() > 0.0 && r.height() > 0.0,
                    "{label}: {what} is empty"
                );
            }
            for head in &l.heads {
                assert!(inside(l.inner, *head), "{label}: a head leaves the body");
            }
            // the heads ARE the columns: same width, same gap, left to right, and every value
            // column lines up under its head
            assert_eq!(l.heads.len(), variants.len());
            for i in 1..l.heads.len() {
                assert!(
                    (l.heads[i].left() - l.heads[i - 1].right() - l.gap).abs() < 0.01,
                    "{label}: the columns keep their gap, side by side"
                );
            }
            for (i, head) in l.heads.iter().enumerate() {
                assert!(
                    (head.width() - l.col_w).abs() < 0.01,
                    "{label}: every head is a column"
                );
                assert!(
                    (head.left() - key_cell(&l, 0, i).left()).abs() < 0.01,
                    "{label}: column {i} lines up with its head"
                );
            }
            assert!(
                l.heads[0].left() >= l.label.right(),
                "{label}: the label column stays clear of the heads"
            );
            // the tables stack contiguously, in order, and stay inside the body
            assert!(
                (l.zones.top() - l.key.bottom()).abs() < 0.01,
                "{label}: the zone rows follow the key rows"
            );
            assert!(
                (l.status.top() - l.zones.bottom()).abs() < 0.01,
                "{label}: the status row follows the zone rows"
            );
            assert!(
                l.status.bottom() <= l.inner.bottom() + 0.01,
                "{label}: the rows stay inside the body"
            );
            assert!(
                l.foot.top() >= l.status.bottom() - 0.01,
                "{label}: the export control sits under the rows"
            );
            assert!(
                l.foot.bottom() <= l.inner.bottom() + 0.01,
                "{label}: the export control stays inside the body"
            );
            assert!(
                l.row_h >= 15.0,
                "{label}: rows stay legible ({} px)",
                l.row_h
            );
            assert!(
                l.head_h >= 120.0,
                "{label}: the sections keep their stage ({} px)",
                l.head_h
            );
            // every head's own parts: name, section, change line - inside it, in order, apart
            for i in 0..variants.len() {
                let head = l.heads[i];
                let name = head_name(&l, i);
                let sec = head_section(&l, i);
                let note = head_note(&l, i);
                for r in [name, sec, note] {
                    assert!(inside(head, r), "{label}: a head part leaves its head");
                }
                assert!(
                    name.bottom() <= sec.top() && sec.bottom() <= note.top(),
                    "{label}: the head's parts stack"
                );
                assert!(
                    sec.width() > 20.0 && sec.height() > 20.0,
                    "{label}: the section keeps a stage"
                );
            }
        }
    }

    /// Every string the painter prints into the rows fits its own cell at the app's real fonts, at
    /// all four sizes: the longest key label and unit in the label column, and - for every value
    /// the committed fixture variants actually produce - the number plus the mark, arrow and delta
    /// the painter reserves room for. A column too narrow for the fixture's own numbers fails here.
    #[test]
    fn the_fixture_values_fit_their_cells_at_the_four_sizes() {
        let p = font_painter();
        let measure = |s: &str, f: FontId| kit::measure(&p, s, f);
        let variants = fixture_variants();
        let z_labels = crate::compare::zone_labels(&variants);
        assert!(!z_labels.is_empty());
        for (label, w, h, phone) in TARGETS {
            let l = layout(
                area(w, h),
                phone,
                variants.len(),
                Metric::ALL.len(),
                z_labels.len(),
            );
            let label_font = kit::semi(if phone { 10.5 } else { 12.5 });
            for m in Metric::ALL {
                let lw = measure(m.label(), label_font.clone()).x;
                assert!(
                    lw <= l.label_w - 6.0,
                    "{label}: `{}` needs {lw:.1} px of {}",
                    m.label(),
                    l.label_w
                );
                let uw = measure(m.unit(), kit::sans(10.0)).x;
                assert!(
                    uw <= l.label_w - 6.0,
                    "{label}: unit `{}` needs {uw:.1} px of {}",
                    m.unit(),
                    l.label_w
                );
            }
            // the arrow the painter reserves (11 px wide) plus the delta column (desktop) plus the
            // provenance mark (12 px): what a lit cell must still fit
            let reserve = if phone { 34.0 } else { 86.0 };
            for m in Metric::ALL {
                for v in &variants {
                    match crate::compare::value(m, v) {
                        Some(x) => {
                            let s = crate::compare::display(m, x);
                            let vw = measure(&s, kit::num(if phone { 13.0 } else { 16.0 })).x;
                            assert!(
                                vw + reserve <= l.col_w - 12.0,
                                "{label}: `{s}` needs {:.1} px of {}",
                                vw + reserve,
                                l.col_w
                            );
                        }
                        None => {
                            let rw = measure("refused", kit::sans(11.0)).x;
                            assert!(rw <= l.col_w - 12.0, "{label}: `refused` needs {rw:.1} px");
                        }
                    }
                }
            }
            for zl in &z_labels {
                for v in &variants {
                    if let Some((pa, share)) = crate::compare::zone_of(v, zl) {
                        let s = format!("{pa:.1}·{share:.0}%");
                        let zw = measure(&s, kit::num(if phone { 12.0 } else { 14.0 })).x;
                        // a zone cell carries no provenance mark: its reserve is the left pad the
                        // painter uses (6/10), the gap before the arrow (9) and the arrow's half
                        // width (5.5), plus 2 px of breathing room at the cell's right edge
                        assert!(
                            zw + 23.0 <= l.col_w,
                            "{label}: zone `{s}` needs {:.1} px of {}",
                            zw + 23.0,
                            l.col_w
                        );
                    }
                }
            }
            let aw = measure("accepted", kit::sans(if phone { 10.5 } else { 11.5 })).x;
            assert!(
                aw <= l.col_w - 14.0,
                "{label}: `accepted` needs {aw:.1} px of {}",
                l.col_w
            );
            // the export button's own label, at the button's own font
            let bw = measure("Export comparison (DEMO watermark)", kit::semi(12.0)).x;
            assert!(
                bw <= l.foot.width() - 24.0,
                "{label}: the export label needs {bw:.1} px of {}",
                l.foot.width()
            );
        }
    }

    /// The phone keeps the design round's arrangement: the same side-by-side matrix as the desktop,
    /// with the label column narrowed and the delta text given up - never a horizontal overflow,
    /// never a squeezed-out column. (Guards the one phone-specific rule the design frames show:
    /// three heads across, at 390.)
    #[test]
    fn the_phone_keeps_the_columns_side_by_side() {
        let variants = fixture_variants();
        let z_rows = crate::compare::zone_labels(&variants).len();
        let l = layout(
            area(390.0, 742.0),
            true,
            variants.len(),
            Metric::ALL.len(),
            z_rows,
        );
        assert!(l.phone);
        assert_eq!(l.heads.len(), 3, "three heads across at 390");
        for i in 1..l.heads.len() {
            assert!(
                l.heads[i].left() > l.heads[i - 1].right(),
                "the phone heads sit side by side, not stacked"
            );
            assert!(
                (l.heads[i].top() - l.heads[i - 1].top()).abs() < 0.01,
                "the phone heads share one row"
            );
        }
        assert!(
            l.label_w < 100.0,
            "the phone label column is the narrow one"
        );
        assert!(
            l.heads.last().unwrap().right() <= l.inner.right() + 0.01,
            "the last phone column stays inside the body"
        );
    }
}
