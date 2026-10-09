//! **Report** (#85): the export entry and a page preview of the PDF calc sheet - cover, inputs, worked
//! steps, results, validation statement. The export is real: the button builds the same sheet as a PDF
//! in [`super::report_pdf`] and delivers it (a download on the web, a save on native); the pages below
//! are a preview of that document.
//!
//! When a saved or opened project document backs the session, the sheet's revision row carries that
//! document's own sha-256, computed from the document's canonical bytes at export time (issue #85
//! completion); a sheet exported from a draft with no document behind it carries its labelled
//! state-hash fallback ([`super::report_pdf::Revision`]).
//!
//! Every value on the pages is the cockpit's live draft and its `EngineOutput`: the duty, the tower record,
//! the fill layers, `worked_steps` (label, formula, substitution, value, unit, reference), the headline
//! results, `validation` (every limit the run checked) and `provenance` (engine, catalog id + revision +
//! status, warning). The project name and the report number are STUB (`data::STUB_PROJECT`,
//! `data::STUB_REPORT_NO`) - the `.drafthouse` format carries no name field, so those stay stubs - and
//! carry the amber tag.

use bevy_egui::egui::{self, pos2, vec2, Align2, Color32, Rect, Stroke, StrokeKind};
use cockpit::engine::{EngineInput, EngineOutput};

use super::data;
use super::kit::{self, text, text_fit};
use super::report_pdf;
use super::{title_band, toast, Account, DocumentSource, Env, State};
use crate::theme as t;

/// Round 2 (#91 decisions, "Report = all of them static"): a Charts page between Results and Validation.
const PAGES: [&str; 6] = [
    "Cover",
    "Inputs",
    "Steps",
    "Results",
    "Charts",
    "Validation",
];
/// The phone tab strip's labels (the full names do not fit 366 px legibly).
fn short_page(name: &str) -> &'static str {
    match name {
        "Cover" => "Cover",
        "Inputs" => "Inputs",
        "Steps" => "Steps",
        "Results" => "Results",
        "Charts" => "Charts",
        _ => "Checks",
    }
}
/// Page ink (the sheet is printed: dark on paper).
const PAPER: Color32 = Color32::from_rgb(0xf4, 0xf1, 0xea);
const PINK: Color32 = Color32::from_rgb(0x1d, 0x26, 0x2c);
const PINK_2: Color32 = Color32::from_rgb(0x4b, 0x58, 0x60);
const PRULE: Color32 = Color32::from_rgb(0xd6, 0xd0, 0xc4);
/// Placeholder text on the paper (sign-off fields): above PRULE, below PINK_2.
const PRULE_TXT: Color32 = Color32::from_rgb(0x9a, 0x96, 0x8c);
const PTEAL: Color32 = Color32::from_rgb(0x0b, 0x60, 0x73);

pub fn ui(ui: &mut egui::Ui, st: &mut State, draft: &mut EngineInput, env: &Env, area: Rect) {
    let body = title_band(ui, area, "Report", "calc sheet · page preview", env.phone);
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
    let page = st.page.min(PAGES.len() - 1);
    let pad = if env.phone { 12.0 } else { 24.0 };
    let inner = body.shrink2(vec2(pad, 12.0));
    let demo = st.account == Account::Demo;

    if env.phone {
        // page tabs as a strip, the page fills the rest, export at the bottom
        let tabs = Rect::from_min_size(inner.min, vec2(inner.width(), 38.0));
        // the current page by name, the rest by number (five names do not fit 366px legibly)
        // five names do not fit 366 px at a legible size: the phone uses the short ones
        let labels: Vec<&str> = PAGES.iter().map(|s| short_page(s)).collect();
        if let Some(i) = kit::segmented_weighted(ui, tabs, "report.tabs", &labels, page, 11.5, 1.6)
        {
            st.page = i;
        }
        let btn = Rect::from_min_max(pos2(inner.left(), inner.bottom() - 46.0), inner.max);
        let stage = Rect::from_min_max(
            pos2(inner.left() - 4.0, tabs.bottom() + 8.0),
            pos2(inner.right() + 4.0, btn.top() - 8.0),
        );
        // tap the page to read it at 2x around the tap; tap again to fit
        let zoom = st
            .info
            .as_deref()
            .and_then(|s| s.strip_prefix("rp:zoom:"))
            .and_then(|s| {
                let (a, b) = s.split_once(':')?;
                Some(pos2(a.parse().ok()?, b.parse().ok()?))
            });
        let resp = ui.interact(stage, egui::Id::new("report.zoom"), egui::Sense::click());
        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked()) {
            st.info = if zoom.is_some() {
                None
            } else {
                Some(format!("rp:zoom:{:.0}:{:.0}", pos.x, pos.y))
            };
        }
        match zoom {
            Some(at) => {
                // the virtual stage, twice the size, placed so the tapped point stays under the finger
                let big = Rect::from_min_size(at - (at - stage.min) * 2.0, stage.size() * 2.0);
                let zp = p.with_clip_rect(stage);
                sheet(
                    &zp,
                    big,
                    page,
                    draft,
                    out,
                    &st.cache,
                    demo,
                    true,
                    st.steps_sheet,
                );
                kit::chip(
                    &p,
                    pos2(stage.right() - 8.0, stage.top() + 8.0),
                    Align2::RIGHT_TOP,
                    "2× · tap to fit",
                    t::INK,
                    t::PANEL_RAISED,
                    t::LINE,
                );
            }
            None => {
                let sheets = sheet(
                    &p,
                    stage,
                    page,
                    draft,
                    out,
                    &st.cache,
                    demo,
                    true,
                    st.steps_sheet,
                );
                steps_pager(ui, st, &p, stage, sheets);
                kit::chip_sized(
                    &p,
                    pos2(stage.right() - 8.0, stage.bottom() - 8.0),
                    Align2::RIGHT_BOTTOM,
                    "tap to read · 2×",
                    t::INK,
                    t::PANEL_RAISED,
                    t::LINE,
                    12.5,
                );
            }
        }
        export_button(ui, st, draft, out, btn, demo, env.document);
    } else {
        // left: the page thumbnails; centre: the page; right: the export card
        let thumbs_w = 150.0;
        let side_w = 300.0;
        let thumbs = Rect::from_min_max(inner.min, pos2(inner.left() + thumbs_w, inner.bottom()));
        let side = Rect::from_min_max(pos2(inner.right() - side_w, inner.top()), inner.max);
        let stage = Rect::from_min_max(
            pos2(thumbs.right() + 24.0, inner.top()),
            pos2(side.left() - 24.0, inner.bottom()),
        );
        let n = PAGES.len() as f32;
        let th_h = ((thumbs.height() - 12.0 * (n - 1.0)) / n).min(140.0);
        let th_w = th_h / 1.414;
        for (i, name) in PAGES.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(thumbs.left() + 8.0, thumbs.top() + (th_h + 12.0) * i as f32),
                vec2(th_w, th_h),
            );
            let resp = kit::hit(
                ui,
                Rect::from_min_size(r.min - vec2(6.0, 4.0), vec2(thumbs_w, th_h + 8.0)),
                &format!("report.page.{i}"),
            );
            mini(&p, r, i);
            p.rect_stroke(
                r.expand(3.0),
                kit::r(4),
                Stroke::new(
                    if i == page { 2.0 } else { 1.0 },
                    if i == page {
                        t::PRIMARY
                    } else if resp.hovered() {
                        t::LINE
                    } else {
                        t::LINE_SOFT
                    },
                ),
                StrokeKind::Inside,
            );
            text(
                &p,
                pos2(r.right() + 12.0, r.top() + 4.0),
                Align2::LEFT_TOP,
                &format!("{}", i + 1),
                kit::num(13.0),
                if i == page { t::INK } else { t::MUTED },
            );
            text(
                &p,
                pos2(r.right() + 12.0, r.top() + 22.0),
                Align2::LEFT_TOP,
                name,
                kit::sans(11.5),
                if i == page { t::INK_2 } else { t::MUTED },
            );
            if resp.clicked() {
                st.page = i;
            }
        }
        let sheets = sheet(
            &p,
            stage,
            page,
            draft,
            out,
            &st.cache,
            demo,
            false,
            st.steps_sheet,
        );
        steps_pager(ui, st, &p, stage, sheets);
        // export card
        kit::glass(&p, side, 12);
        let ir = side.shrink(18.0);
        text(
            &p,
            ir.min,
            Align2::LEFT_TOP,
            "Calc sheet",
            kit::semi(15.0),
            t::INK,
        );
        text(
            &p,
            pos2(ir.left(), ir.top() + 24.0),
            Align2::LEFT_TOP,
            &format!("A4 · {} pages", PAGES.len()),
            kit::sans(12.0),
            t::MUTED,
        );
        let mut y = ir.top() + 62.0;
        let stats: [(&str, String); 4] = [
            ("worked steps", format!("{}", out.worked_steps.len())),
            ("limits outside range", format!("{}", out.validation.len())),
            ("fill layers", format!("{}", draft.fill_layers.len())),
            (
                "catalog",
                if out.provenance.catalog_status.len() > 14 {
                    "synthetic".into()
                } else {
                    out.provenance.catalog_status.clone()
                },
            ),
        ];
        for (l, v) in stats.iter() {
            text(
                &p,
                pos2(ir.left(), y),
                Align2::LEFT_TOP,
                l,
                kit::sans(12.0),
                t::INK_2,
            );
            text(
                &p,
                pos2(ir.right(), y - 1.0),
                Align2::RIGHT_TOP,
                v,
                kit::num(13.5),
                t::INK,
            );
            y += 28.0;
        }
        y += 8.0;
        let fails = out.validation.iter().filter(|l| outside(l)).count();
        let ok = fails == 0;
        kit::chip(
            &p,
            pos2(ir.left(), y),
            Align2::LEFT_TOP,
            if ok {
                "no limit refused"
            } else {
                "a limit refused the run"
            },
            if ok { t::OK } else { t::DANGER },
            if ok { t::OK_SOFT } else { t::DANGER_SOFT },
            t::with_alpha(if ok { t::OK } else { t::DANGER }, 120),
        );
        y += 44.0;
        p.line_segment(
            [pos2(ir.left(), y), pos2(ir.right(), y)],
            Stroke::new(1.0, t::LINE_SOFT),
        );
        y += 14.0;
        text(
            &p,
            pos2(ir.left(), y),
            Align2::LEFT_TOP,
            "Signed by",
            kit::semi(12.0),
            t::INK_2,
        );
        y += 22.0;
        let signer = if demo {
            "—  sign in to sign the sheet".to_string()
        } else {
            "Staff engineer (signed in)".to_string()
        };
        text(
            &p,
            pos2(ir.left(), y),
            Align2::LEFT_TOP,
            &signer,
            kit::sans(12.0),
            if demo { t::MUTED } else { t::INK },
        );
        y += 34.0;
        text(
            &p,
            pos2(ir.left(), y),
            Align2::LEFT_TOP,
            "Method",
            kit::semi(12.0),
            t::INK_2,
        );
        y += 22.0;
        let eng = kit::method(&out.provenance.engine);
        let cut = eng
            .char_indices()
            .filter(|(i, c)| *c == ' ' && *i < 34)
            .map(|x| x.0)
            .next_back()
            .filter(|_| eng.len() > 34);
        match cut {
            Some(c) => {
                text(
                    &p,
                    pos2(ir.left(), y),
                    Align2::LEFT_TOP,
                    &eng[..c],
                    kit::sans(11.5),
                    t::INK,
                );
                y += 17.0;
                kit::text_fit(
                    &p,
                    pos2(ir.left(), y),
                    Align2::LEFT_TOP,
                    &eng[c + 1..],
                    kit::sans(11.5),
                    t::INK,
                    ir.width(),
                );
            }
            None => {
                kit::text_fit(
                    &p,
                    pos2(ir.left(), y),
                    Align2::LEFT_TOP,
                    eng,
                    kit::sans(11.5),
                    t::INK,
                    ir.width(),
                );
            }
        }
        y += 20.0;
        kit::text_fit(
            &p,
            pos2(ir.left(), y),
            Align2::LEFT_TOP,
            &format!(
                "{} rev {}",
                out.provenance.catalog_id, out.provenance.catalog_revision
            ),
            kit::mono(11.0),
            t::INK_2,
            ir.width(),
        );
        let btn = Rect::from_min_max(pos2(ir.left(), ir.bottom() - 48.0), ir.max);
        export_button(ui, st, draft, out, btn, demo, env.document);
        if demo {
            let lr = Rect::from_min_max(
                pos2(ir.left(), btn.top() - 66.0),
                pos2(ir.right(), btn.top() - 10.0),
            );
            p.rect_filled(lr, kit::r(8), t::AMBER_SOFT);
            kit::lock_glyph(&p, pos2(lr.left() + 18.0, lr.center().y), 14.0, t::AMBER);
            text(
                &p,
                pos2(lr.left() + 34.0, lr.top() + 10.0),
                Align2::LEFT_TOP,
                "DEMO catalog",
                kit::semi(12.0),
                t::AMBER,
            );
            text(
                &p,
                pos2(lr.left() + 34.0, lr.top() + 28.0),
                Align2::LEFT_TOP,
                "watermarked; sign in to export",
                kit::sans(11.0),
                t::INK_2,
            );
        }
    }
}

fn export_button(
    ui: &egui::Ui,
    st: &mut State,
    draft: &EngineInput,
    out: &EngineOutput,
    r: Rect,
    demo: bool,
    document: Option<DocumentSource<'_>>,
) {
    let label = if demo {
        "Export (DEMO watermark)"
    } else {
        "Export PDF"
    };
    let resp = kit::button(ui, r, "report.export", label, kit::Btn::Primary);
    kit::glyph(
        ui.painter(),
        "export",
        pos2(r.left() + 22.0, r.center().y),
        15.0,
        t::INK,
    );
    if resp.clicked() {
        // The same bytes both hosts get: build the sheet, name it after its state hash, deliver it
        // (download / save). The DEMO account exports the watermarked sample. Issue #85 completion:
        // when a saved or opened project document backs the session, the revision row carries the
        // document's own digest - the canonical bytes are produced (through the session's own
        // writer) at this moment, so the sheet's revision is the state it was built from.
        let document = document.and_then(|source| source(draft));
        let mut meta = report_pdf::meta_for_export(st.account == Account::Staff, draft);
        if let Some(document) = document.as_ref() {
            meta.carry_document(document);
        }
        let bytes = report_pdf::document(draft, out, &st.cache, &meta);
        let name = report_pdf::file_name(&meta.state_hash);
        match report_pdf::deliver(&bytes, &name) {
            Ok(done) => toast(st, &done),
            Err(e) => toast(st, &format!("export failed: {e}")),
        }
    }
}

fn outside(l: &cockpit::engine::Limit) -> bool {
    l.min.map(|m| l.value < m).unwrap_or(false) || l.max.map(|m| l.value > m).unwrap_or(false)
}

/// A thumbnail: the page's skeleton in grey bars.
fn mini(p: &egui::Painter, r: Rect, i: usize) {
    p.rect_filled(r, kit::r(2), PAPER);

    let bar = |y: f32, w: f32, h: f32, c: Color32| {
        p.rect_filled(
            Rect::from_min_size(
                pos2(r.left() + 8.0, r.top() + y),
                vec2((r.width() - 16.0) * w, h),
            ),
            kit::r(1),
            c,
        )
    };
    bar(8.0, 0.35, 3.0, PTEAL);
    if i == 4 {
        // four small chart frames
        let g = Rect::from_min_max(r.min + vec2(8.0, 20.0), r.max - vec2(8.0, 10.0));
        let cw = (g.width() - 4.0) / 2.0;
        let ch = (g.height() - 4.0) / 2.0;
        for k in 0..4 {
            let c = Rect::from_min_size(
                g.min + vec2((cw + 4.0) * (k % 2) as f32, (ch + 4.0) * (k / 2) as f32),
                vec2(cw, ch),
            );
            p.rect_stroke(c, kit::r(1), Stroke::new(0.8, PRULE), StrokeKind::Inside);
            p.line_segment(
                [
                    c.left_bottom() + vec2(3.0, -4.0),
                    c.right_top() + vec2(-3.0, 5.0),
                ],
                Stroke::new(1.0, PTEAL),
            );
        }
        return;
    }
    match i {
        0 => {
            bar(r.height() * 0.32, 0.8, 7.0, PINK);
            bar(r.height() * 0.32 + 12.0, 0.6, 4.0, PINK_2);
            bar(r.height() * 0.62, 0.5, 3.0, PRULE);
            bar(r.height() * 0.62 + 6.0, 0.5, 3.0, PRULE);
        }
        _ => {
            for k in 0..((r.height() - 30.0) / 7.0) as usize {
                let w = [0.9, 0.7, 0.85, 0.6, 0.95][(k + i) % 5];
                bar(
                    22.0 + k as f32 * 7.0,
                    w,
                    2.5,
                    if k % 4 == 0 { PINK_2 } else { PRULE },
                );
            }
        }
    }
}

/// Paper axes: a frame, a few ticks, the names. `x`/`y` = (min, max).
#[allow(clippy::too_many_arguments)]
fn paper_axes(
    p: &egui::Painter,
    r: Rect,
    x: (f64, f64),
    y: (f64, f64),
    xs: f64,
    ys: f64,
    xl: &str,
    yl: &str,
    s: f32,
) -> kit::Plot {
    let fm = |size: f32| kit::mono((size * s).max(6.0));
    let plot = kit::Plot {
        rect: Rect::from_min_max(
            r.min + vec2(30.0 * s, 14.0 * s),
            r.max - vec2(6.0 * s, 24.0 * s),
        ),
        x,
        y,
    };
    p.rect_stroke(
        plot.rect,
        kit::r(0),
        Stroke::new(0.7, PRULE),
        StrokeKind::Inside,
    );
    let mut v = (x.0 / xs).ceil() * xs;
    while v <= x.1 + 1e-9 {
        let a = plot.pt(v, y.0);
        p.line_segment([a, pos2(a.x, plot.rect.top())], Stroke::new(0.4, PRULE));
        text(
            p,
            a + vec2(0.0, 3.0 * s),
            Align2::CENTER_TOP,
            &format!("{v:.prec$}", prec = if xs < 1.0 { 1 } else { 0 }),
            fm(7.0),
            PINK_2,
        );
        v += xs;
    }
    let mut v = (y.0 / ys).ceil() * ys;
    while v <= y.1 + 1e-9 {
        let a = plot.pt(x.0, v);
        p.line_segment([a, pos2(plot.rect.right(), a.y)], Stroke::new(0.4, PRULE));
        text(
            p,
            a - vec2(3.0 * s, 0.0),
            Align2::RIGHT_CENTER,
            &format!("{v:.prec$}", prec = if ys < 1.0 { 1 } else { 0 }),
            fm(7.0),
            PINK_2,
        );
        v += ys;
    }
    text(
        p,
        pos2(plot.rect.center().x, plot.rect.bottom() + 13.0 * s),
        Align2::CENTER_TOP,
        xl,
        kit::sans((7.5 * s).max(6.0)),
        PINK_2,
    );
    text(
        p,
        pos2(r.left(), r.top()),
        Align2::LEFT_TOP,
        yl,
        kit::sans((7.5 * s).max(6.0)),
        PINK_2,
    );
    plot
}

/// Page 5, Charts: the screens' charts, static, on paper - fan vs system (Curves), cold water vs wet bulb
/// at the duty's range (Curves), the candidate scatter (Size) and the water balance (Water). Rate's test
/// points are STUB readings, so they stay off the calc sheet.
fn charts_page(
    p: &egui::Painter,
    ir: Rect,
    top: f32,
    s: f32,
    d: &EngineInput,
    o: &EngineOutput,
    cache: &data::Cache,
) {
    let fs = |size: f32| kit::semi((size * s).max(6.5));
    let f = |size: f32| kit::sans((size * s).max(6.0));
    text(
        p,
        pos2(ir.left(), top),
        Align2::LEFT_TOP,
        "4  Charts",
        fs(13.0),
        PINK,
    );
    let g = Rect::from_min_max(
        pos2(ir.left(), top + 28.0 * s),
        ir.max - vec2(0.0, 20.0 * s),
    );
    let gap = 16.0 * s;
    let cw = (g.width() - gap) / 2.0;
    let ch = (g.height() - gap) / 2.0;
    let cell = |k: usize| {
        Rect::from_min_size(
            g.min + vec2((cw + gap) * (k % 2) as f32, (ch + gap) * (k / 2) as f32),
            vec2(cw, ch),
        )
    };
    let title = |r: Rect, t1: &str| {
        text(p, r.min, Align2::LEFT_TOP, t1, fs(8.5), PTEAL);
        Rect::from_min_max(r.min + vec2(0.0, 14.0 * s), r.max)
    };
    // issue #151: a cell whose data is still sliced across frames says how far it is, so a
    // half-drawn page reads as loading rather than stuck
    let pending = |r: Rect, what: &str, (done, n): (usize, usize)| {
        text(
            p,
            r.center(),
            Align2::CENTER_CENTER,
            &format!("{what}: computing {done}/{n}"),
            f(8.0),
            PINK_2,
        );
    };
    let nice = super::rate::nice;

    // 1. fan vs system
    {
        let r = title(cell(0), "Fan vs system (operating point)");
        let fsc = &o.fan_system_curve;
        let x1 = fsc
            .fan
            .points
            .iter()
            .chain(fsc.system.points.iter())
            .map(|q| q.x)
            .fold(0.0, f64::max);
        let y1 = fsc
            .fan
            .points
            .iter()
            .chain(fsc.system.points.iter())
            .filter(|q| q.x <= x1)
            .map(|q| q.y)
            .fold(0.0, f64::max);
        if x1 > 0.0 && y1 > 0.0 {
            let plot = paper_axes(
                p,
                r,
                (0.0, x1 * 1.04),
                (0.0, y1 * 1.08),
                nice(x1 / 4.0),
                nice(y1 / 4.0),
                "airflow m³/s",
                "Pa",
                s,
            );
            let fan: Vec<(f64, f64)> = fsc.fan.points.iter().map(|q| (q.x, q.y)).collect();
            let sys: Vec<(f64, f64)> = fsc.system.points.iter().map(|q| (q.x, q.y)).collect();
            plot.line(p, &sys, Stroke::new(1.0 * s.max(0.8), PINK_2));
            plot.line(p, &fan, Stroke::new(1.4 * s.max(0.8), PTEAL));
            let c = plot.pt(fsc.operating_point.x, fsc.operating_point.y);
            p.circle_stroke(c, 3.5 * s.max(0.8), Stroke::new(1.2, PINK));
            // above the crossing, centred: the open wedge between the falling fan and the rising system
            text(
                p,
                pos2(c.x, c.y - 7.0 * s),
                Align2::CENTER_BOTTOM,
                &format!(
                    "{} m³/s · {} Pa",
                    t::num::flow(fsc.operating_point.x),
                    t::num::pa(fsc.operating_point.y)
                ),
                f(7.0),
                PINK,
            );
        }
    }
    // 2. cold water vs wet bulb at the duty's range
    {
        let r = title(cell(1), "Cold water vs wet bulb (80 / 100 / 120 % flow)");
        match cache.curves.as_ref().filter(|c| !c.recs.is_empty()) {
            None => pending(r, "the curves", cache.curves_progress()),
            Some(cd) => {
                let want = d.duty.hot_water_c - d.duty.target_cold_water_c;
                let range = data::CURVE_RANGE
                    .iter()
                    .copied()
                    .min_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()))
                    .unwrap_or(10.0);
                let recs: Vec<&data::CurveRec> = cd
                    .recs
                    .iter()
                    .filter(|x| (x.range - range).abs() < 1e-6)
                    .collect();
                let (y0, y1) = recs
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |a, x| {
                        (a.0.min(x.cold), a.1.max(x.cold))
                    });
                let x = (data::CURVE_WB[0], data::CURVE_WB[data::CURVE_WB.len() - 1]);
                let plot = paper_axes(
                    p,
                    r,
                    x,
                    ((y0 - 0.5).floor(), (y1 + 0.5).ceil()),
                    2.0,
                    nice((y1 - y0 + 1.0) / 4.0),
                    &format!("wet bulb °C · {range:.0} K range"),
                    "cold °C",
                    s,
                );
                let cols = [PINK_2, PTEAL, PINK];
                for (fi, fpct) in data::CURVE_FLOW_PCT.iter().enumerate() {
                    let flow = cd.design_flow_kg_s * fpct / 100.0;
                    let mut pts: Vec<(f64, f64)> = recs
                        .iter()
                        .filter(|x| (x.flow_kg_s - flow).abs() < 1e-6)
                        .map(|x| (x.wb, x.cold))
                        .collect();
                    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
                    plot.line(
                        p,
                        &pts,
                        Stroke::new(if fi == 1 { 1.4 } else { 0.9 }, cols[fi]),
                    );
                    if let Some(q) = pts.last() {
                        text(
                            p,
                            plot.pt(q.0, q.1) - vec2(2.0 * s, 3.0 * s),
                            Align2::RIGHT_BOTTOM,
                            &format!("{fpct:.0} %"),
                            f(6.5),
                            cols[fi],
                        );
                    }
                }
                let dp = plot.pt(d.duty.wet_bulb_c, d.duty.target_cold_water_c);
                p.circle_stroke(dp, 3.0 * s.max(0.8), Stroke::new(1.2, PINK));
            }
        }
    }
    // 3. the candidate scatter
    {
        let r = title(cell(2), "Feasible candidates: fan power vs cold water");
        match cache.size.as_ref().filter(|x| !x.towers.is_empty()) {
            None => pending(r, "the selection", cache.size_progress()),
            Some(sz) => {
                let list = sz.ranked(None);
                let (x0, x1, y1) = list.iter().fold(
                    (f64::INFINITY, d.duty.target_cold_water_c, 0.0f64),
                    |a, c| (a.0.min(c.cold_c), a.1.max(c.cold_c), a.2.max(c.power_kw)),
                );
                let plot = paper_axes(
                    p,
                    r,
                    (x0 - 0.1, x1 + 0.1),
                    (0.0, y1 * 1.1),
                    nice((x1 - x0 + 0.2) / 4.0),
                    nice(y1 / 4.0),
                    "cold water °C",
                    "kW",
                    s,
                );
                let tx = plot.pt(d.duty.target_cold_water_c, plot.y.1);
                kit::dashed(
                    p,
                    tx,
                    pos2(tx.x, plot.rect.bottom()),
                    Stroke::new(0.8, PINK_2),
                    3.0,
                    3.0,
                );
                for (i, c) in list.iter().enumerate().rev() {
                    let q = plot.pt(c.cold_c, c.power_kw);
                    let col = if i == 0 { PTEAL } else { PINK_2 };
                    if c.crossflow {
                        p.rect_filled(
                            Rect::from_center_size(q, vec2(2.6, 2.6) * s.max(0.8)),
                            kit::r(0),
                            col,
                        );
                    } else {
                        p.circle_filled(q, 1.4 * s.max(0.8), col);
                    }
                }
                text(
                    p,
                    pos2(plot.rect.left() + 4.0 * s, plot.rect.top() + 3.0 * s),
                    Align2::LEFT_TOP,
                    &format!("{} feasible · ● counterflow ■ crossflow", list.len()),
                    f(6.5),
                    PINK_2,
                );
            }
        }
    }
    // 4. the water balance, as one bar: make-up = evaporation + drift + blowdown
    {
        let r = title(cell(3), "Water balance at the duty's cycles");
        match data::water(d, o, d.duty.cycles_of_concentration) {
            Err(e) => {
                text(p, r.center(), Align2::CENTER_CENTER, &e, f(7.5), PINK_2);
            }
            Ok(w) => {
                let to = |kg: f64| kg / w.density * 3600.0;
                let parts = [
                    ("evaporation", to(w.evaporation_kg_s), PTEAL),
                    ("drift", to(w.drift_kg_s), PINK),
                    ("blowdown", to(w.blowdown_kg_s), PINK_2),
                ];
                let total = to(w.makeup_kg_s).max(1e-9);
                let bar = Rect::from_min_size(
                    pos2(r.left(), r.top() + 18.0 * s),
                    vec2(r.width(), 16.0 * s),
                );
                let mut x = bar.left();
                for (_, v, col) in parts.iter() {
                    let w_px = bar.width() * (*v / total) as f32;
                    p.rect_filled(
                        Rect::from_min_size(pos2(x, bar.top()), vec2(w_px.max(1.0), bar.height())),
                        kit::r(0),
                        *col,
                    );
                    x += w_px;
                }
                let mut y = bar.bottom() + 12.0 * s;
                for (name, v, col) in parts.iter() {
                    p.rect_filled(
                        Rect::from_min_size(
                            pos2(r.left(), y + 2.0 * s),
                            vec2(7.0, 7.0) * s.max(0.8),
                        ),
                        kit::r(0),
                        *col,
                    );
                    text(
                        p,
                        pos2(r.left() + 12.0 * s, y),
                        Align2::LEFT_TOP,
                        name,
                        f(8.0),
                        PINK_2,
                    );
                    text(
                        p,
                        pos2(r.right(), y),
                        Align2::RIGHT_TOP,
                        &format!("{} m³/h", t::num::flow(*v)),
                        kit::mono((8.0 * s).max(6.0)),
                        PINK,
                    );
                    y += 16.0 * s;
                }
                p.line_segment(
                    [pos2(r.left(), y + 2.0 * s), pos2(r.right(), y + 2.0 * s)],
                    Stroke::new(0.6, PRULE),
                );
                y += 6.0 * s;
                text(
                    p,
                    pos2(r.left() + 12.0 * s, y),
                    Align2::LEFT_TOP,
                    "make-up",
                    kit::semi((8.0 * s).max(6.0)),
                    PINK,
                );
                text(
                    p,
                    pos2(r.right(), y),
                    Align2::RIGHT_TOP,
                    &format!("{} m³/h", t::num::flow(total)),
                    kit::mono((8.0 * s).max(6.0)),
                    PINK,
                );
                text(
                    p,
                    pos2(r.left(), y + 18.0 * s),
                    Align2::LEFT_TOP,
                    &format!("at {} cycles of concentration", t::num::sig(w.cycles, 2)),
                    f(7.5),
                    PINK_2,
                );
            }
        }
    }
    text(
        p,
        pos2(ir.left(), ir.bottom()),
        Align2::LEFT_BOTTOM,
        "Every series is calculated for this duty; the Rate test readings are samples, so they are not shown.",
        f(7.0),
        PINK_2,
    );
}

/// Issue #137: the steps run over more than one sheet (each step now carries its inputs line): a pager
/// at the stage's bottom-left turns them - "< 1 / 2 >" (drawn chevrons), each arrow a 44 px target. Nothing when the
/// page fits one sheet.
fn steps_pager(ui: &egui::Ui, st: &mut State, p: &egui::Painter, stage: Rect, sheets: usize) {
    if sheets < 2 {
        st.steps_sheet = 0;
        return;
    }
    st.steps_sheet = st.steps_sheet.min(sheets - 1);
    let h = 44.0;
    let r = Rect::from_min_size(
        pos2(stage.left() + 8.0, stage.bottom() - 8.0 - h),
        vec2(140.0, h),
    );
    p.rect_filled(r, kit::r(12), t::PANEL_RAISED);
    p.rect_stroke(r, kit::r(12), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
    let prev = Rect::from_min_size(r.min, vec2(44.0, h));
    let next = Rect::from_min_size(pos2(r.right() - 44.0, r.top()), vec2(44.0, h));
    let on = |b: bool| if b { t::PRIMARY } else { t::MUTED };
    // drawn chevrons: the UI fonts have no `‹ ›`
    kit::chevron_left(
        p,
        prev.center() - vec2(3.0, 0.0),
        12.0,
        on(st.steps_sheet > 0),
    );
    kit::chevron(
        p,
        next.center() + vec2(3.0, 0.0),
        12.0,
        on(st.steps_sheet + 1 < sheets),
    );
    text(
        p,
        r.center(),
        Align2::CENTER_CENTER,
        &format!("{} / {sheets}", st.steps_sheet + 1),
        kit::num(13.0),
        t::INK,
    );
    if kit::hit(ui, prev, "report.steps.prev").clicked() && st.steps_sheet > 0 {
        st.steps_sheet -= 1;
    }
    if kit::hit(ui, next, "report.steps.next").clicked() && st.steps_sheet + 1 < sheets {
        st.steps_sheet += 1;
    }
}

/// One page of the sheet, at A4 proportion, fitted to the stage. Returns how many sheets the page
/// runs to (the worked steps can take more than one; `steps_sheet` picks which is shown).
#[allow(clippy::too_many_arguments)]
fn sheet(
    p: &egui::Painter,
    stage: Rect,
    page: usize,
    d: &EngineInput,
    o: &EngineOutput,
    cache: &data::Cache,
    demo: bool,
    phone: bool,
    steps_sheet: usize,
) -> usize {
    let mut sheets = 1;
    let h = stage.height().min(stage.width() * 1.414);
    let w = h / 1.414;
    let r = Rect::from_center_size(stage.center(), vec2(w, h));
    p.rect_filled(
        r.translate(vec2(0.0, 6.0)).expand(2.0),
        kit::r(3),
        t::with_alpha(Color32::BLACK, 120),
    );
    p.rect_filled(r, kit::r(2), PAPER);
    let s = w / 595.0; // A4 points
    if demo {
        // the DEMO watermark: the real-catalog lock made visible on the paper, diagonal across the page
        let g = kit::num((84.0 * s).max(24.0));
        kit::text_rotated(
            p,
            pos2(r.center().x, r.top() + r.height() * 0.72),
            "DEMO",
            g,
            Color32::from_rgba_unmultiplied(0xe0, 0x9a, 0x3a, 22),
            -0.62,
        );
    }
    let m = 40.0 * s;
    let ir = r.shrink(m);
    let f = |size: f32| kit::sans((size * s).max(6.5));
    let fs = |size: f32| kit::semi((size * s).max(6.5));
    let fm = |size: f32| kit::mono((size * s).max(6.5));
    // running header
    text(
        p,
        ir.min,
        Align2::LEFT_TOP,
        "DRAFTHOUSE · CALC SHEET",
        fs(8.0),
        PTEAL,
    );
    let pr = if phone {
        Rect::from_min_size(pos2(ir.right() + 18.0 * s, ir.top()), vec2(0.0, 0.0))
    } else {
        text(
            p,
            pos2(ir.right(), ir.top()),
            Align2::RIGHT_TOP,
            &format!("{} / {}", page + 1, PAGES.len()),
            fm(8.0),
            PINK_2,
        )
    };
    let no = text(
        p,
        pos2(pr.left() - 18.0 * s, ir.top()),
        Align2::RIGHT_TOP,
        data::STUB_REPORT_NO,
        fm(8.0),
        PINK_2,
    );
    if !phone || page == 0 {
        kit::stub_tag(
            p,
            pos2(no.left() - 10.0, no.center().y),
            Align2::RIGHT_CENTER,
        );
    }
    // under the STUB chip, at any scale (the chip keeps a legible floor size)
    let rule = ir.top() + (16.0 * s).max(if phone || page == 0 { 24.0 } else { 14.0 });
    p.line_segment(
        [pos2(ir.left(), rule), pos2(ir.right(), rule)],
        Stroke::new(1.0, PRULE),
    );
    let top = rule + 18.0 * s;
    let row = |y: f32, l: &str, v: &str, u: &str| {
        text(p, pos2(ir.left(), y), Align2::LEFT_TOP, l, f(9.5), PINK_2);
        let vr = text(
            p,
            pos2(ir.right() - 40.0 * s, y),
            Align2::RIGHT_TOP,
            v,
            fm(9.5),
            PINK,
        );
        text(p, pos2(ir.right(), y), Align2::RIGHT_TOP, u, f(9.0), PINK_2);
        p.line_segment(
            [
                pos2(ir.left(), vr.bottom() + 3.0 * s),
                pos2(ir.right(), vr.bottom() + 3.0 * s),
            ],
            Stroke::new(0.6, PRULE),
        );
    };
    let lh = 19.0 * s;
    match page {
        0 => {
            let y = ir.top() + ir.height() * 0.12;
            text(
                p,
                pos2(ir.left(), y),
                Align2::LEFT_TOP,
                "Thermal & airside",
                fs(22.0),
                PINK,
            );
            text(
                p,
                pos2(ir.left(), y + 30.0 * s),
                Align2::LEFT_TOP,
                "calculation sheet",
                fs(22.0),
                PINK,
            );
            let pr = text(
                p,
                pos2(ir.left(), y + 74.0 * s),
                Align2::LEFT_TOP,
                data::STUB_PROJECT,
                f(12.0),
                PINK_2,
            );
            kit::stub_tag(
                p,
                pos2(pr.right() + 14.0, pr.center().y),
                Align2::LEFT_CENTER,
            );
            let sub_y = (y + 94.0 * s).max(pr.center().y + 12.0);
            text(
                p,
                pos2(ir.left(), sub_y),
                Align2::LEFT_TOP,
                &format!("{} · {}", d.tower.name, d.tower.tower_type),
                f(11.0),
                PINK_2,
            );
            // the headline, big
            let hy = ir.top() + ir.height() * 0.36;
            p.line_segment(
                [pos2(ir.left(), hy), pos2(ir.right(), hy)],
                Stroke::new(1.2, PINK),
            );
            let cw = ir.width() / 3.0;
            for (i, (l, v, u)) in [
                ("cold water", t::num::temp(o.cold_water_c), "°C"),
                ("capability", t::num::pct(o.capability_pct), "%"),
                ("fan power", t::num::power(o.fan_power_kw), "kW"),
            ]
            .iter()
            .enumerate()
            {
                let x = ir.left() + cw * i as f32;
                text(
                    p,
                    pos2(x, hy + 12.0 * s),
                    Align2::LEFT_TOP,
                    l,
                    f(9.0),
                    PINK_2,
                );
                let vr = text(
                    p,
                    pos2(x, hy + 26.0 * s),
                    Align2::LEFT_TOP,
                    v,
                    kit::num((20.0 * s).max(9.0)),
                    PINK,
                );
                text(
                    p,
                    pos2(vr.right() + 3.0 * s, vr.bottom() - 3.0 * s),
                    Align2::LEFT_BOTTOM,
                    u,
                    f(9.0),
                    PINK_2,
                );
            }
            // the scope: what the sheet covers, one line each, with the page it is on
            let sy = hy + 82.0 * s;
            text(
                p,
                pos2(ir.left(), sy),
                Align2::LEFT_TOP,
                "Contents",
                fs(9.5),
                PTEAL,
            );
            let toc = [
                (
                    "1",
                    "Inputs",
                    format!(
                        "duty, tower, {} fill layers, fan, eliminator, nozzle",
                        d.fill_layers.len()
                    ),
                ),
                (
                    "2",
                    "Worked steps",
                    format!(
                        "{} steps, each with formula and reference",
                        o.worked_steps.len()
                    ),
                ),
                (
                    "3",
                    "Results",
                    "cold water, capability, airside, water".to_string(),
                ),
                (
                    "4",
                    "Validation",
                    if o.validation.is_empty() {
                        "every limit held".to_string()
                    } else {
                        format!("{} refusals", o.validation.len())
                    },
                ),
            ];
            for (i, (n, h, sub)) in toc.iter().enumerate() {
                let y = sy + 18.0 * s + i as f32 * 28.0 * s;
                text(p, pos2(ir.left(), y), Align2::LEFT_TOP, n, fm(9.0), PINK_2);
                text(
                    p,
                    pos2(ir.left() + 18.0 * s, y),
                    Align2::LEFT_TOP,
                    h,
                    fs(10.0),
                    PINK,
                );
                text_fit(
                    p,
                    pos2(ir.left() + 18.0 * s, y + 12.0 * s),
                    Align2::LEFT_TOP,
                    sub,
                    f(8.5),
                    PINK_2,
                    ir.width() - 18.0 * s,
                );
                text(
                    p,
                    pos2(ir.right(), y),
                    Align2::RIGHT_TOP,
                    &format!("{}", i + 2),
                    fm(9.0),
                    PINK_2,
                );
            }
            // the sign-off block every calc sheet carries: empty boxes, filled by hand or on export
            let by = ir.bottom() - 70.0 * s - 112.0 * s;
            let bw = (ir.width() - 16.0 * s) / 3.0;
            for (i, l) in ["Prepared", "Checked", "Approved"].iter().enumerate() {
                let br = Rect::from_min_size(
                    pos2(ir.left() + (bw + 8.0 * s) * i as f32, by),
                    vec2(bw, 70.0 * s),
                );
                p.rect_stroke(br, kit::r(0), Stroke::new(0.8, PRULE), StrokeKind::Inside);
                text(
                    p,
                    br.min + vec2(6.0 * s, 5.0 * s),
                    Align2::LEFT_TOP,
                    l,
                    fs(8.0),
                    PINK_2,
                );
                text(
                    p,
                    pos2(br.left() + 6.0 * s, br.bottom() - 9.0 * s),
                    Align2::LEFT_BOTTOM,
                    "name · date",
                    f(8.0),
                    PRULE_TXT,
                );
            }
            let py = ir.bottom() - 70.0 * s;
            for (i, (l, v)) in [
                ("method", kit::method(&o.provenance.engine)),
                ("catalog", o.provenance.catalog_id.as_str()),
                ("revision", o.provenance.catalog_revision.as_str()),
                ("status", o.provenance.catalog_status.as_str()),
            ]
            .iter()
            .enumerate()
            {
                text(
                    p,
                    pos2(ir.left(), py + i as f32 * 14.0 * s),
                    Align2::LEFT_TOP,
                    l,
                    f(8.5),
                    PINK_2,
                );
                text_fit(
                    p,
                    pos2(ir.left() + 70.0 * s, py + i as f32 * 14.0 * s),
                    Align2::LEFT_TOP,
                    v,
                    fm(8.5),
                    PINK,
                    ir.width() - 70.0 * s,
                );
            }
        }
        1 => {
            text(
                p,
                pos2(ir.left(), top),
                Align2::LEFT_TOP,
                "1  Inputs",
                fs(13.0),
                PINK,
            );
            let mut y = top + 26.0 * s;
            let duty: [(&str, String, &str); 7] = [
                ("water flow", t::num::flow(d.duty.water_flow_m3_hr), "m³/h"),
                ("hot water", t::num::temp(d.duty.hot_water_c), "°C"),
                (
                    "target cold water",
                    t::num::temp(d.duty.target_cold_water_c),
                    "°C",
                ),
                ("wet bulb", t::num::temp(d.duty.wet_bulb_c), "°C"),
                ("dry bulb", t::num::temp(d.duty.dry_bulb_c), "°C"),
                (
                    "pressure",
                    t::num::sig(d.duty.pressure_pa / 1000.0, 3),
                    "kPa",
                ),
                (
                    "cycles",
                    format!("{:.1}", d.duty.cycles_of_concentration),
                    "×",
                ),
            ];
            text(
                p,
                pos2(ir.left(), y),
                Align2::LEFT_TOP,
                "Duty",
                fs(9.5),
                PTEAL,
            );
            y += 16.0 * s;
            for (l, v, u) in duty.iter() {
                row(y, l, v, u);
                y += lh;
            }
            y += 10.0 * s;
            text(
                p,
                pos2(ir.left(), y),
                Align2::LEFT_TOP,
                "Tower & equipment",
                fs(9.5),
                PTEAL,
            );
            y += 16.0 * s;
            row(y, "tower", &d.tower.id, "");
            y += lh;
            row(y, "fill area", &t::num::sig(d.tower.fill_area_m2, 3), "m²");
            y += lh;
            for (i, l) in d.fill_layers.iter().enumerate() {
                row(
                    y,
                    &format!("fill layer {}", i + 1),
                    &format!(
                        "{} · {}",
                        kit::part(&l.fill_id),
                        t::num::value(l.depth_m, "m")
                    ),
                    "m",
                );
                y += lh;
            }
            row(
                y,
                "fan · speed",
                &format!(
                    "{} · {}",
                    kit::part(&d.fan.id),
                    t::num::pct(d.speed_ratio * 100.0)
                ),
                "%",
            );
            y += lh;
            row(y, "drift eliminator", &d.drift.id, "");
            y += lh;
            row(y, "nozzle", &d.nozzle.id, "");
        }
        2 => {
            // issue #137: each step reads inputs -> formula -> numbers -> result, as on the calculation
            // card and in the PDF. Four lines a step no longer fit every step on one sheet: the steps run
            // over sheets, as the PDF's do, and the pager under the sheet turns them.
            let first_y = top + 26.0 * s;
            let line_h = 12.0 * s;
            let text_w = ir.width() - 18.0 * s;
            // the inputs (a note's `why` is its narrative), the formula, the numbers - each wrapped at
            // its operators (never cut short), so a step is as tall as its lines; the steps are paged
            // by their measured height, as the PDF's are
            fn lines_of(
                ws: &cockpit::engine::WorkedStep,
                s: f32,
            ) -> [(Option<&str>, egui::FontId, egui::Color32); 3] {
                [
                    (
                        Some(ws.why.as_str()).filter(|w| !w.is_empty()),
                        kit::sans((8.0 * s).max(6.5)),
                        PINK_2,
                    ),
                    (ws.formula.as_deref(), kit::mono((8.0 * s).max(6.5)), PTEAL),
                    (
                        ws.substitution.as_deref(),
                        kit::mono((7.5 * s).max(6.5)),
                        PINK_2,
                    ),
                ]
            }
            let height = |ws: &cockpit::engine::WorkedStep| {
                let body: f32 = lines_of(ws, s)
                    .into_iter()
                    .map(|(l, font, _)| {
                        l.map(|l| kit::wrap_height(p, &kit::bind_words(l), font, text_w))
                            .unwrap_or(0.0)
                            .max(line_h)
                    })
                    .sum();
                14.0 * s + body + 8.0 * s
            };
            let n = o.worked_steps.len();
            let room = (ir.bottom() - 18.0 * s) - first_y;
            let mut starts = vec![0usize];
            let mut used = 0.0;
            for (i, ws) in o.worked_steps.iter().enumerate() {
                let h = height(ws);
                if used + h > room && used > 0.0 {
                    starts.push(i);
                    used = 0.0;
                }
                used += h;
            }
            sheets = starts.len();
            let k = steps_sheet.min(sheets - 1);
            let shown = starts[k]..starts.get(k + 1).copied().unwrap_or(n);
            text(
                p,
                pos2(ir.left(), top),
                Align2::LEFT_TOP,
                &if sheets > 1 {
                    format!("2  Worked steps ({}–{} of {n})", shown.start + 1, shown.end)
                } else {
                    "2  Worked steps".to_string()
                },
                fs(13.0),
                PINK,
            );
            let mut y = first_y;
            for i in shown {
                let ws = &o.worked_steps[i];
                let step_h = height(ws);
                text(
                    p,
                    pos2(ir.left(), y),
                    Align2::LEFT_TOP,
                    &format!("{}", i + 1),
                    fm(8.5),
                    PINK_2,
                );
                text_fit(
                    p,
                    pos2(ir.left() + 18.0 * s, y),
                    Align2::LEFT_TOP,
                    kit::step_title(ws),
                    fs(9.5),
                    PINK,
                    ir.width() * 0.62,
                );
                if let Some(v) = ws.value {
                    text(
                        p,
                        pos2(ir.right(), y),
                        Align2::RIGHT_TOP,
                        &t::num::with_unit(v, &ws.unit),
                        fm(9.5),
                        PINK,
                    );
                }
                let mut ly = y + 14.0 * s;
                for (line, font, col) in lines_of(ws, s) {
                    let h = match line {
                        Some(line) => kit::text_wrap_bound(
                            p,
                            pos2(ir.left() + 18.0 * s, ly),
                            line,
                            font,
                            col,
                            text_w,
                        )
                        .height(),
                        None => 0.0,
                    };
                    ly += h.max(line_h);
                }
                p.line_segment(
                    [
                        pos2(ir.left(), y + step_h - 6.0 * s),
                        pos2(ir.right(), y + step_h - 6.0 * s),
                    ],
                    Stroke::new(0.5, PRULE),
                );
                y += step_h;
            }
        }
        3 => {
            text(
                p,
                pos2(ir.left(), top),
                Align2::LEFT_TOP,
                "3  Results",
                fs(13.0),
                PINK,
            );
            let mut y = top + 26.0 * s;
            let res: [(&str, String, &str); 10] = [
                ("cold water", t::num::temp(o.cold_water_c), "°C"),
                // issue #139: hot water − the calculated cold water (the duty's design range is hot − target)
                ("achieved range", t::num::kelvin(o.range_c), "K"),
                ("approach", t::num::kelvin(o.approach_c), "K"),
                ("capability", t::num::pct(o.capability_pct), "%"),
                ("KaV/L", t::num::kavl(o.kavl_total), ""),
                ("airflow", t::num::flow(o.airflow_m3_s), "m³/s"),
                ("total pressure", t::num::pa(o.total_pressure_pa), "Pa"),
                ("fan power", t::num::power(o.fan_power_kw), "kW"),
                ("evaporation", t::num::pct(o.evaporation_pct), "%"),
                ("make-up", t::num::flow(o.makeup_m3_hr), "m³/h"),
            ];
            for (l, v, u) in res.iter() {
                row(y, l, v, u);
                y += lh;
            }
            y += 14.0 * s;
            text(
                p,
                pos2(ir.left(), y),
                Align2::LEFT_TOP,
                "KaV/L by layer",
                fs(9.5),
                PTEAL,
            );
            y += 16.0 * s;
            for l in &o.kavl_per_layer {
                row(
                    y,
                    &format!(
                        "{} · {} m",
                        kit::part(&l.fill_id),
                        t::num::value(l.depth_m, "m")
                    ),
                    &format!(
                        "{}  ({} %)",
                        t::num::kavl(l.kavl),
                        t::num::pct(l.cooling_share_pct)
                    ),
                    "",
                );
                y += lh;
            }
            // round 2 (#91 correctness gate c): KaV/L total = fill layers + spray + rain zones
            // (`available_merkel_number`); the layers alone do not sum to it, so the remainder is a row.
            let fill: f64 = o.kavl_per_layer.iter().map(|l| l.kavl).sum();
            let share: f64 = o.kavl_per_layer.iter().map(|l| l.cooling_share_pct).sum();
            row(
                y,
                "spray + rain zones",
                &format!(
                    "{}  ({} %)",
                    t::num::kavl(o.kavl_total - fill),
                    t::num::pct(100.0 - share)
                ),
                "",
            );
            y += lh;
            row(
                y,
                "total",
                &format!("{}  (100 %)", t::num::kavl(o.kavl_total)),
                "",
            );
        }
        4 => charts_page(p, ir, top, s, d, o, cache),
        _ => {
            text(
                p,
                pos2(ir.left(), top),
                Align2::LEFT_TOP,
                "4  Validation",
                fs(13.0),
                PINK,
            );
            let mut y = top + 26.0 * s;
            let fails = o.validation.iter().filter(|l| outside(l)).count();
            let stmt = if fails == 0 {
                "Every limit held: each part ran inside its rated range.".to_string()
            } else {
                format!(
                    "Outside rated range: {} limit(s) - this run is not valid.",
                    o.validation.len()
                )
            };
            let sr = Rect::from_min_size(pos2(ir.left(), y), vec2(ir.width(), 40.0 * s));
            p.rect_filled(
                sr,
                kit::r(2),
                if fails == 0 {
                    Color32::from_rgb(0xdf, 0xee, 0xe5)
                } else {
                    Color32::from_rgb(0xf6, 0xdd, 0xdb)
                },
            );
            text_fit(
                p,
                pos2(sr.left() + 10.0 * s, sr.center().y),
                Align2::LEFT_CENTER,
                &stmt,
                fs(9.5),
                PINK,
                sr.width() - 20.0 * s,
            );
            y = sr.bottom() + 14.0 * s;
            for l in &o.validation {
                if y + lh > ir.bottom() - 60.0 * s {
                    break;
                }
                let bad = outside(l);
                let range = match (l.min, l.max) {
                    (Some(a), Some(b)) => format!("{} – {}", t::num::sig(a, 3), t::num::sig(b, 3)),
                    (Some(a), None) => format!("≥ {}", t::num::sig(a, 3)),
                    (None, Some(b)) => format!("≤ {}", t::num::sig(b, 3)),
                    _ => "–".into(),
                };
                text_fit(
                    p,
                    pos2(ir.left(), y),
                    Align2::LEFT_TOP,
                    &kit::limit_name(&l.field),
                    f(9.0),
                    PINK_2,
                    ir.width() * 0.42,
                );
                text(
                    p,
                    pos2(ir.left() + ir.width() * 0.46, y),
                    Align2::LEFT_TOP,
                    &t::num::with_unit(l.value, kit::limit_unit(&l.unit)),
                    fm(9.0),
                    if bad {
                        Color32::from_rgb(0xb0, 0x30, 0x30)
                    } else {
                        PINK
                    },
                );
                text(
                    p,
                    pos2(ir.right(), y),
                    Align2::RIGHT_TOP,
                    &range,
                    fm(8.5),
                    PINK_2,
                );
                p.line_segment(
                    [
                        pos2(ir.left(), y + 15.0 * s),
                        pos2(ir.right(), y + 15.0 * s),
                    ],
                    Stroke::new(0.5, PRULE),
                );
                y += lh;
            }
            // the statement + provenance block at the foot
            let fy = ir.bottom() - 46.0 * s;
            p.line_segment(
                [pos2(ir.left(), fy), pos2(ir.right(), fy)],
                Stroke::new(1.0, PINK),
            );
            text_fit(
                p,
                pos2(ir.left(), fy + 8.0 * s),
                Align2::LEFT_TOP,
                &format!(
                    "Computed by {} on catalog {} rev {}.",
                    o.provenance.engine, o.provenance.catalog_id, o.provenance.catalog_revision
                ),
                f(8.5),
                PINK,
                ir.width(),
            );
            if !o.provenance.warning.is_empty() {
                text_fit(
                    p,
                    pos2(ir.left(), fy + 22.0 * s),
                    Align2::LEFT_TOP,
                    &o.provenance.warning,
                    f(8.0),
                    PINK_2,
                    ir.width(),
                );
            }
        }
    }
    // footer
    text(
        p,
        pos2(ir.left(), r.bottom() - 22.0 * s),
        Align2::LEFT_BOTTOM,
        &format!(
            "{} · {}",
            o.provenance.catalog_id, o.provenance.catalog_status
        ),
        fm(7.5),
        PINK_2,
    );
    sheets
}
