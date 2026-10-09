//! The HUD kit the new screens share (drafthouse#91 Part B).
//!
//! Everything here paints with egui primitives on the existing theme (`crate::theme`): glass panels over
//! the scene, the ⓘ dot, chips, a segmented control, a gauge, chart axes, flow particles and the hand-drawn
//! glyphs (no icon font - the shipped Plex subsets carry no symbol glyphs, and the brief rules out icon
//! tooling). Every string a screen paints goes through [`text`], so the on-screen inventory the evidence
//! counts (`#mirror-screens`) is the painted text, not a typed list.

use std::cell::RefCell;

use bevy_egui::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect,
    Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

use crate::theme as t;

// ------------------------------------------------------------------------------------ fonts

pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semi(size: f32) -> FontId {
    FontId::new(size, t::family_semi())
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn num(size: f32) -> FontId {
    FontId::new(size, t::family_mono_med())
}

// ------------------------------------------------------------------- painted-text inventory

thread_local! {
    static STRINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn strings_begin() {
    STRINGS.with(|s| s.borrow_mut().clear());
}
pub fn strings_take() -> Vec<String> {
    STRINGS.with(|s| std::mem::take(&mut *s.borrow_mut()))
}
fn record(s: &str) {
    if !s.trim().is_empty() {
        STRINGS.with(|v| v.borrow_mut().push(s.to_string()));
    }
}

// -------------------------------------------------------------------- control-hit inventory

// Issue #117: the screens' interactive controls, by their own ids, in canvas pixels - the screens'
// counterpart of the instrument's `data-hits` (`crate::app::HitMap`). The instrument reports what
// it drew, so a headless harness can aim a real pointer at a tray card instead of guessing
// coordinates; a screen like Compare owes a harness the same. The frame's start clears the
// inventory ([`hits_begin`]) and its publish step takes it ([`hits_take`]), so it always describes
// the frame that was just drawn.
// Issue #137 item 4: human names next to part codes. The catalog's own `name` for every record code,
// refreshed from the session catalog each frame (`screens::frame`), so a custom part or an imported
// revision names itself the same way. The catalog's "Illustrative " prefix is dropped on screen: the
// validation badge already says the whole catalog is illustrative, and every row repeating it is noise.
thread_local! {
    static PARTS: RefCell<std::collections::HashMap<String, String>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Issue #137: the calculation method a document names in its provenance block, in engineering words.
/// The provenance string itself (`EngineOutput::provenance.engine`) names the implementation - a crate
/// and a type - which is an audit fact for the project file, not words for a calc sheet. The recorded
/// replay build says it is a replay; anything else is the Merkel calculation the sheet's steps show.
pub fn method(provenance_engine: &str) -> &'static str {
    if provenance_engine.starts_with("FixtureEngine") || provenance_engine.contains("recorded") {
        "recorded replay of a Merkel calculation"
    } else {
        "Merkel method, fill layers in series"
    }
}

/// Issue #139: a worked step's title as the screen, the Report sheet and the PDF print it. The engine's
/// step label is a key the cockpit looks steps up by (and the parity suite pins), so it stays; only the
/// title disambiguates the two ranges. The build's engine writes `Range` as hot water minus the
/// CALCULATED cold water (the achieved range); the recorded replay's sheet writes it as hot water minus
/// the TARGET (`range = t_hot - t_cold,target`, the duty's design range). The step's own formula says
/// which one it is, so neither is ever printed under the other's name.
pub fn step_title(step: &cockpit::engine::WorkedStep) -> &str {
    match step.label.as_str() {
        "Range"
            if step
                .formula
                .as_deref()
                .is_some_and(|f| f.contains("target")) =>
        {
            "Design range"
        }
        "Range" => "Achieved range",
        other => other,
    }
}

/// Issue #137: a refusal limit's field, as the screen and the sheet name it. The engine keys its limits by
/// input path (`fan.speedRatio`, `run`); the path stays in the project file and the limit record, the
/// screen prints the quantity. An unknown key falls back to its last segment, split at its capitals
/// (`maxWaterTemperatureC` -> "max water temperature"), so no camelCase reaches the screen.
pub fn limit_name(field: &str) -> String {
    match field {
        "run" => "combinations tried".into(),
        "fan.speedRatio" => "fan speed".into(),
        "duty.hotWaterC" => "hot water".into(),
        "duty.targetColdWaterC" => "target cold water".into(),
        "duty.wetBulbC" => "wet bulb".into(),
        "duty.waterFlowM3Hr" | "duty.waterMassFlowKgS" => "water flow".into(),
        _ => {
            let last = field.rsplit('.').next().unwrap_or(field);
            let mut out = String::new();
            for (i, c) in last.chars().enumerate() {
                if c.is_ascii_uppercase() && i > 0 {
                    out.push(' ');
                }
                out.push(c.to_ascii_lowercase());
            }
            // a trailing unit letter ("... temperature c") is the unit column's job
            for unit in [" c", " k", " pa", " kw", " m", " kg s", " m3 hr"] {
                if let Some(s) = out.strip_suffix(unit) {
                    out = s.to_string();
                    break;
                }
            }
            out.replace('_', " ")
        }
    }
}

/// Issue #137: a limit's unit in the house style (`ratio` and count words are not units).
pub fn limit_unit(unit: &str) -> &str {
    match unit {
        "ratio" | "combinations" | "candidates" => "",
        "C" | "degC" => "°C",
        "m3/hr" | "m3/h" => "m³/h",
        "m3/s" => "m³/s",
        "m2" => "m²",
        "kg/m3" => "kg/m³",
        other => other,
    }
}

/// Refresh the code -> name table from the session catalog.
pub fn parts_set(cat: &crate::state::Catalog) {
    PARTS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len()
            == cat.towers.len()
                + cat.fills.len()
                + cat.fans.len()
                + cat.drifts.len()
                + cat.nozzles.len()
        {
            return;
        }
        m.clear();
        let mut put = |id: &str, name: &str| {
            let name = name.strip_prefix("Illustrative ").unwrap_or(name);
            m.insert(id.to_string(), name.to_string());
        };
        for x in &cat.towers {
            put(&x.id, &x.name);
        }
        for x in &cat.fills {
            put(&x.record.id, &x.record.name);
        }
        for x in &cat.fans {
            put(&x.id, &x.name);
        }
        for x in &cat.drifts {
            put(&x.id, &x.name);
        }
        for x in &cat.nozzles {
            put(&x.id, &x.name);
        }
    });
}

/// A part's catalog name, or `None` for a code the catalog does not carry.
pub fn part_name(code: &str) -> Option<String> {
    PARTS.with(|m| m.borrow().get(code).cloned())
}

/// A part as the screen names it: `Micro-Fluted Film Fill · FILM-MF20`; the bare code when the catalog
/// has no name for it.
pub fn part(code: &str) -> String {
    match part_name(code) {
        Some(n) => format!("{n} · {code}"),
        None => code.to_string(),
    }
}

thread_local! {
    static HITS: RefCell<Vec<(String, [f32; 4])>> = const { RefCell::new(Vec::new()) };
    static SIDE: std::cell::Cell<Option<Rect>> = const { std::cell::Cell::new(None) };
}

/// Issue #137 (conductor review): a screen with a docked side panel (its controls and read-outs, beside
/// the drawing) names that panel's rect each frame; the calculation card opens **over the panel**, in
/// exactly its column, never over the drawing. Reset by [`side_panel_take`] each frame.
pub fn side_panel(r: Rect) {
    SIDE.with(|s| s.set(Some(r)));
}

/// The side panel the screen drew this frame (if any), clearing it for the next.
pub fn side_panel_take() -> Option<Rect> {
    SIDE.with(|s| s.take())
}

pub fn hits_begin() {
    HITS.with(|h| h.borrow_mut().clear());
}
pub fn hits_take() -> Vec<(String, [f32; 4])> {
    HITS.with(|h| std::mem::take(&mut *h.borrow_mut()))
}
fn record_hit(id: &str, rect: Rect) {
    HITS.with(|h| {
        h.borrow_mut().push((
            id.to_string(),
            [rect.min.x, rect.min.y, rect.width(), rect.height()],
        ));
    });
}

/// Paint one string and record it in the inventory. Returns the drawn rect.
pub fn text(p: &Painter, pos: Pos2, align: Align2, s: &str, font: FontId, color: Color32) -> Rect {
    record(s);
    p.text(pos, align, s, font, color)
}

/// Issue #137: a wrapped paragraph from its top-left, `max_w` wide - the calculation card's formula and
/// substitution lines. Recorded in the painted-text inventory like every other string.
pub fn text_wrap(
    p: &Painter,
    pos: Pos2,
    s: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) -> Rect {
    record(s);
    let g = p.layout(s.to_string(), font, color, max_w);
    let r = Rect::from_min_size(pos, g.size());
    p.galley(pos, g, color);
    r
}

/// Issue #137 (conductor review): an engineering line wrapped only where a reader would break it - at
/// the operators (`=`, `×`, `÷`, `+`, `−`) and the `·` between quantities. Every other space becomes a
/// no-break space, so a name ("fan efficiency"), a number and its unit ("1.04 kg/s") stay on one line.
/// For painting only: [`text_wrap_bound`] records the line with its ordinary spaces.
pub fn bind_words(s: &str) -> String {
    const OPS: [&str; 8] = ["=", "×", "÷", "+", "−", "·", "(", ")"];
    let toks: Vec<&str> = s.split(' ').collect();
    let mut out = String::with_capacity(s.len() + 8);
    for (i, tok) in toks.iter().enumerate() {
        if i > 0 {
            let prev = toks[i - 1];
            let breakable =
                OPS.contains(&prev) || OPS.contains(tok) || prev.is_empty() || tok.is_empty();
            out.push(if breakable { ' ' } else { '\u{a0}' });
        }
        out.push_str(tok);
    }
    out
}

/// [`text_wrap`] for an engineering line: wrapped at its operators only ([`bind_words`]); records the
/// text as given.
pub fn text_wrap_bound(
    p: &Painter,
    pos: Pos2,
    s: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) -> Rect {
    record(s);
    let g = p.layout(bind_words(s), font, color, max_w);
    let r = Rect::from_min_size(pos, g.size());
    p.galley(pos, g, color);
    r
}

/// The height [`text_wrap`] will take, measured with the same layout (nothing is painted or recorded).
pub fn wrap_height(p: &Painter, s: &str, font: FontId, max_w: f32) -> f32 {
    p.layout(s.to_string(), font, t::INK, max_w).size().y
}

/// Paint one string rotated by `angle` (radians) about its own centre at `c`, and record it.
pub fn text_rotated(
    p: &Painter,
    c: Pos2,
    s: &str,
    font: FontId,
    color: Color32,
    angle: f32,
) -> Rect {
    record(s);
    let g = p.layout_no_wrap(s.to_string(), font, color);
    let half = g.size() / 2.0;
    let (sn, cs) = angle.sin_cos();
    let off = vec2(half.x * cs - half.y * sn, half.x * sn + half.y * cs);
    let rect = Rect::from_center_size(c, g.size());
    p.add(egui::epaint::TextShape::new(c - off, g, color).with_angle(angle));
    rect
}

/// A label on a small ground-coloured plate, so it reads over lines and fills.
pub fn label_plate(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    s: &str,
    font: FontId,
    color: Color32,
) -> Rect {
    let size = measure(p, s, font.clone()) + vec2(8.0, 4.0);
    let rect = align.anchor_size(pos, size);
    // 245/255: a curve behind the plate must not read through the label (round-2 QA)
    p.rect_filled(rect, r(3), t::with_alpha(t::BG, 245));
    text(p, rect.center(), Align2::CENTER_CENTER, s, font, color)
}

/// Width of a string in a font (no paint, no record).
pub fn measure(p: &Painter, s: &str, font: FontId) -> Vec2 {
    p.layout_no_wrap(s.to_string(), font, t::INK).size()
}

/// Paint a string truncated with an ellipsis to `max_w`.
pub fn text_fit(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    s: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) -> Rect {
    if measure(p, s, font.clone()).x <= max_w {
        return text(p, pos, align, s, font, color);
    }
    let mut cut: String = s.to_string();
    while !cut.is_empty() && measure(p, &format!("{cut}…"), font.clone()).x > max_w {
        cut.pop();
    }
    text(p, pos, align, &format!("{}…", cut.trim_end()), font, color)
}

// ----------------------------------------------------------------------------- surfaces

pub fn r(n: u8) -> CornerRadius {
    CornerRadius::same(n)
}

/// A HUD panel over the scene: the raised panel colour, slightly translucent, hairline edge.
pub fn glass(p: &Painter, rect: Rect, radius: u8) {
    p.rect_filled(rect, r(radius), t::with_alpha(t::PANEL, 236));
    p.rect_stroke(
        rect,
        r(radius),
        Stroke::new(1.0, t::LINE),
        StrokeKind::Inside,
    );
}

/// Issue #137: a surface that floats **over** other content (the phone's answer sheet, a toast): the
/// theme's panel fill, fully opaque. `glass` is for panels that sit on the ground; over a screen its
/// 92 % fill let the controls underneath read through the figures.
pub fn sheet(p: &Painter, rect: Rect, radius: u8) {
    p.rect_filled(rect, r(radius), t::PANEL);
    p.rect_stroke(
        rect,
        r(radius),
        Stroke::new(1.0, t::LINE),
        StrokeKind::Inside,
    );
}

pub fn glass_soft(p: &Painter, rect: Rect, radius: u8) {
    p.rect_filled(rect, r(radius), t::with_alpha(t::PANEL_RAISED, 200));
    p.rect_stroke(
        rect,
        r(radius),
        Stroke::new(1.0, t::LINE_SOFT),
        StrokeKind::Inside,
    );
}

/// The screen ground: ink, a faint measuring grid, and a soft glow where the hero sits.
pub fn ground(p: &Painter, area: Rect, glow_at: Pos2, glow_r: f32, glow: Color32) {
    p.rect_filled(area, r(0), t::BG);
    let step = 32.0;
    let mut x = area.left() + (step - (area.left() % step));
    while x < area.right() {
        p.line_segment(
            [pos2(x, area.top()), pos2(x, area.bottom())],
            Stroke::new(1.0, t::with_alpha(t::GRID, 150)),
        );
        x += step;
    }
    let mut y = area.top() + (step - (area.top() % step));
    while y < area.bottom() {
        p.line_segment(
            [pos2(area.left(), y), pos2(area.right(), y)],
            Stroke::new(1.0, t::with_alpha(t::GRID, 150)),
        );
        y += step;
    }
    radial(p, glow_at, glow_r, glow, Color32::TRANSPARENT);
}

/// A radial gradient disc (a triangle fan with per-vertex colour).
pub fn radial(p: &Painter, c: Pos2, radius: f32, inner: Color32, outer: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, inner);
    let n = 48;
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(c + vec2(a.cos(), a.sin()) * radius, outer);
    }
    for i in 1..=n as u32 {
        mesh.add_triangle(0, i, i + 1);
    }
    p.add(Shape::mesh(mesh));
}

/// A vertical gradient rect.
pub fn vgrad(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// A horizontal gradient rect.
pub fn hgrad(p: &Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// Hot (top) to cold (bottom) water colour, `f` in 0..1 from hot to cold.
pub fn water_temp(f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    let hot = (0xe0, 0x8a, 0x4a);
    let cold = (0x4a, 0x9d, 0xe0);
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f) as u8;
    Color32::from_rgb(
        lerp(hot.0, cold.0),
        lerp(hot.1, cold.1),
        lerp(hot.2, cold.2),
    )
}

// -------------------------------------------------------------------------------- chips

pub fn chip(
    p: &Painter,
    at: Pos2,
    align: Align2,
    s: &str,
    fg: Color32,
    bg: Color32,
    line: Color32,
) -> Rect {
    chip_sized(p, at, align, s, fg, bg, line, 10.5)
}

/// As [`chip`], at an explicit font size (the phone's tap-to-read hint is a thumb target, not a label).
#[allow(clippy::too_many_arguments)]
pub fn chip_sized(
    p: &Painter,
    at: Pos2,
    align: Align2,
    s: &str,
    fg: Color32,
    bg: Color32,
    line: Color32,
    font: f32,
) -> Rect {
    let font = semi(font);
    let size = measure(p, s, font.clone()) + vec2(14.0, 10.0);
    let rect = align.anchor_size(at, size);
    p.rect_filled(rect, r(4), bg);
    p.rect_stroke(rect, r(4), Stroke::new(1.0, line), StrokeKind::Inside);
    text(p, rect.center(), Align2::CENTER_CENTER, s, font, fg);
    rect
}

/// The DEMO badge: amber, the colour the theme reserves for synthetic data.
pub fn demo_badge(p: &Painter, at: Pos2, align: Align2) -> Rect {
    chip(p, at, align, "DEMO", t::INK, t::AMBER, t::AMBER)
}

/// The marker a value carries when it comes from sample readings, not a calculation of this duty
/// (issue #137: "STUB" was developer jargon; the Rate card already says "sample readings").
pub const STUB_TAG: &str = "SAMPLE";

/// The sample marker (see [`STUB_TAG`]).
pub fn stub_tag(p: &Painter, at: Pos2, align: Align2) -> Rect {
    chip(
        p,
        at,
        align,
        STUB_TAG,
        t::AMBER,
        t::AMBER_SOFT,
        t::with_alpha(t::AMBER, 150),
    )
}

// ------------------------------------------------------------------------------ controls

/// An invisible hit area with the pointer cursor. 44 px is the phone floor; callers pass rects that clear it.
pub fn hit(ui: &Ui, rect: Rect, id: &str) -> Response {
    let resp = ui.interact(rect, egui::Id::new(("screens", id)), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    record_hit(id, resp.rect);
    resp
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Ghost,
    Locked,
}

pub fn button(ui: &Ui, rect: Rect, id: &str, label: &str, kind: Btn) -> Response {
    let p = ui.painter();
    let resp = hit(ui, rect, id);
    let hot = resp.hovered();
    let (bg, line, fg) = match kind {
        Btn::Primary => (
            if hot {
                t::PRIMARY
            } else {
                t::with_alpha(t::PRIMARY, 225)
            },
            t::PRIMARY,
            t::BG,
        ),
        Btn::Ghost => (
            if hot {
                t::PRIMARY_SOFT
            } else {
                t::PANEL_RAISED
            },
            if hot { t::PRIMARY } else { t::LINE },
            t::INK,
        ),
        Btn::Locked => (t::PANEL, t::LINE_SOFT, t::MUTED),
    };
    p.rect_filled(rect, r(6), bg);
    p.rect_stroke(rect, r(6), Stroke::new(1.0, line), StrokeKind::Inside);
    if kind == Btn::Locked {
        let w = measure(p, label, semi(12.0)).x;
        let x0 = rect.center().x - (w + 16.0) / 2.0;
        lock_glyph(p, pos2(x0 + 5.0, rect.center().y), 5.0, t::MUTED);
        text(
            p,
            pos2(x0 + 16.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            semi(12.0),
            fg,
        );
    } else {
        text(
            p,
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            semi(12.0),
            fg,
        );
    }
    resp
}

/// A segmented control. Returns the index tapped this frame.
pub fn segmented(
    ui: &Ui,
    rect: Rect,
    id: &str,
    opts: &[&str],
    sel: usize,
    font: f32,
) -> Option<usize> {
    segmented_weighted(ui, rect, id, opts, sel, font, 1.0)
}

/// As [`segmented`], with the selected segment `weight` times as wide as the others (so it can carry
/// a name while the rest carry numbers).
pub fn segmented_weighted(
    ui: &Ui,
    rect: Rect,
    id: &str,
    opts: &[&str],
    sel: usize,
    font: f32,
    weight: f32,
) -> Option<usize> {
    let p = ui.painter();
    p.rect_filled(rect, r(7), t::BG);
    p.rect_stroke(rect, r(7), Stroke::new(1.0, t::LINE), StrokeKind::Inside);
    let units = opts.len() as f32 - 1.0 + weight;
    let unit = rect.width() / units;
    let mut out = None;
    let mut x = rect.left();
    for (i, o) in opts.iter().enumerate() {
        let w = if i == sel { unit * weight } else { unit };
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
        x += w;
        let resp = hit(ui, cell, &format!("{id}.{i}"));
        if i == sel {
            p.rect_filled(cell.shrink(3.0), r(5), t::PRIMARY_SOFT);
            p.rect_stroke(
                cell.shrink(3.0),
                r(5),
                Stroke::new(1.0, t::PRIMARY),
                StrokeKind::Inside,
            );
        } else if resp.hovered() {
            p.rect_filled(cell.shrink(3.0), r(5), t::PANEL_RAISED);
        }
        text(
            p,
            cell.center(),
            Align2::CENTER_CENTER,
            o,
            semi(font),
            if i == sel { t::INK } else { t::MUTED },
        );
        if resp.clicked() {
            out = Some(i);
        }
    }
    out
}

/// The ⓘ dot (the shipped fonts carry no ⓘ, so it is drawn). Returns its response.
pub fn info_dot(ui: &Ui, c: Pos2, id: &str, open: bool) -> Response {
    let p = ui.painter();
    let resp = hit(
        ui,
        Rect::from_center_size(c, vec2(32.0, 32.0)),
        &format!("info.{id}"),
    );
    let col = if open || resp.hovered() {
        t::PRIMARY
    } else {
        t::MUTED
    };
    if open {
        p.circle_filled(c, 8.5, t::PRIMARY_SOFT);
    }
    p.circle_stroke(c, 8.0, Stroke::new(1.3, col));
    p.circle_filled(pos2(c.x, c.y - 3.6), 1.2, col);
    p.line_segment(
        [pos2(c.x, c.y - 1.0), pos2(c.x, c.y + 4.2)],
        Stroke::new(1.6, col),
    );
    resp
}

/// A small value block: label above, big mono number + unit.
pub fn metric(
    p: &Painter,
    at: Pos2,
    label: &str,
    value: &str,
    unit: &str,
    size: f32,
    color: Color32,
) -> Rect {
    let l = text(p, at, Align2::LEFT_TOP, label, semi(10.5), t::MUTED);
    let v = text(
        p,
        pos2(at.x, l.bottom() + 3.0),
        Align2::LEFT_TOP,
        value,
        num(size),
        color,
    );
    let u = text(
        p,
        pos2(v.right() + 4.0, v.bottom() - 2.0),
        Align2::LEFT_BOTTOM,
        unit,
        sans(11.0),
        t::INK_2,
    );
    l.union(v).union(u)
}

// ------------------------------------------------------------------------------ source marks

/// Round 2 (#91 decision 2): every number carries where it comes from. Three classes, one mark each:
/// calculated (an engine call on this draft), catalog (a recorded catalog / requirement value) and
/// illustrative (a stand-in: the STUB constants, the drawing). Tapping a mark opens its detail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    Calc,
    Catalog,
    Illus,
}

impl Src {
    pub fn name(self) -> &'static str {
        match self {
            Src::Calc => "calculated",
            Src::Catalog => "catalog",
            Src::Illus => "illustrative",
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            Src::Calc => t::PRIMARY,
            Src::Catalog => t::INK_2,
            Src::Illus => t::AMBER,
        }
    }
    pub fn meaning(self) -> &'static str {
        match self {
            Src::Calc => "Calculated for this duty.",
            Src::Catalog => "A recorded catalog or requirement value.",
            Src::Illus => "A stand-in: not computed, not recorded.",
        }
    }
}

thread_local! {
    /// The marks painted this frame (rect, class, origin), for the shell's tap-to-detail.
    static MARKS: RefCell<Vec<(Rect, Src, String)>> = const { RefCell::new(Vec::new()) };
}

pub fn marks_take() -> Vec<(Rect, Src, String)> {
    MARKS.with(|m| std::mem::take(&mut *m.borrow_mut()))
}

/// The mark itself, centred at `c`: a filled dot (calculated), a ring (catalog), a hollow diamond
/// (illustrative). Drawn shapes, so the class reads without colour too. (Round-2 QA: a hollow square read
/// as a missing-glyph box, so catalog is a ring.)
pub fn src_glyph(p: &Painter, c: Pos2, src: Src, s: f32) {
    let col = src.color();
    match src {
        Src::Calc => {
            p.circle_filled(c, s * 0.5, col);
        }
        Src::Catalog => {
            p.circle_stroke(c, s * 0.42, Stroke::new(1.4, col));
        }
        Src::Illus => {
            let h = s * 0.6;
            p.add(Shape::closed_line(
                vec![
                    c + vec2(0.0, -h),
                    c + vec2(h, 0.0),
                    c + vec2(0.0, h),
                    c + vec2(-h, 0.0),
                ],
                Stroke::new(1.3, col),
            ));
        }
    }
}

/// A source mark beside a number: `at` is the left-centre of the mark. `origin` names exactly where the
/// number comes from (shown on tap). Returns the mark's rect.
pub fn src_mark(p: &Painter, at: Pos2, src: Src, origin: &str) -> Rect {
    // round-2 QA: 2.5 px of air between the text and the mark
    let c = at + vec2(6.0, 0.0);
    src_glyph(p, c, src, 7.0);
    let rect = Rect::from_center_size(c, vec2(9.0, 9.0));
    MARKS.with(|m| {
        m.borrow_mut()
            .push((rect.expand(7.0), src, origin.to_string()))
    });
    rect
}

/// [`metric`] with its source mark after the unit.
#[allow(clippy::too_many_arguments)]
pub fn metric_src(
    p: &Painter,
    at: Pos2,
    label: &str,
    value: &str,
    unit: &str,
    size: f32,
    color: Color32,
    src: Src,
    origin: &str,
) -> Rect {
    let m = metric(p, at, label, value, unit, size, color);
    // the mark rides the label, like a footnote marker: the value row keeps its full width
    let lw = measure(p, label, semi(10.5));
    let mk = src_mark(p, pos2(at.x + lw.x + 4.0, at.y + lw.y / 2.0), src, origin);
    m.union(mk)
}

// -------------------------------------------------------------------------------- gauge

/// A 240° gauge. `v` and the band are in the same unit; `band` paints a soft arc (e.g. the uncertainty).
#[allow(clippy::too_many_arguments)]
pub fn gauge(
    p: &Painter,
    c: Pos2,
    radius: f32,
    lo: f32,
    hi: f32,
    v: f32,
    target: f32,
    band: Option<(f32, f32)>,
    color: Color32,
) {
    let a0 = 150f32.to_radians();
    let sweep = 240f32.to_radians();
    let ang = |x: f32| a0 + sweep * ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    let arc = |from: f32, to: f32, rad: f32| -> Vec<Pos2> {
        let n = 64;
        (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                c + vec2(a.cos(), a.sin()) * rad
            })
            .collect()
    };
    let w = (radius * 0.13).max(8.0);
    p.add(Shape::line(
        arc(a0, a0 + sweep, radius),
        Stroke::new(w, t::LINE_SOFT),
    ));
    if let Some((b0, b1)) = band {
        p.add(Shape::line(
            arc(ang(b0), ang(b1), radius + w * 0.95),
            Stroke::new(w * 0.45, t::with_alpha(color, 110)),
        ));
    }
    // the value arc, from the 100 % mark (the design demand) toward the value
    let t100 = ang(target);
    let tv = ang(v);
    p.add(Shape::line(
        arc(a0, tv, radius),
        Stroke::new(w, t::with_alpha(color, 70)),
    ));
    p.add(Shape::line(
        arc(t100.min(tv), t100.max(tv), radius),
        Stroke::new(w, color),
    ));
    // the design mark
    let d0 = c + vec2(t100.cos(), t100.sin()) * (radius - w * 1.1);
    let d1 = c + vec2(t100.cos(), t100.sin()) * (radius + w * 1.1);
    p.line_segment([d0, d1], Stroke::new(2.0, t::INK));
    // ticks
    let mut x = (lo / 10.0).ceil() * 10.0;
    while x <= hi + 0.01 {
        let a = ang(x);
        let r0 = radius - w * 0.5 - 6.0;
        let r1 = r0 - if (x % 50.0).abs() < 0.01 { 8.0 } else { 4.0 };
        p.line_segment(
            [
                c + vec2(a.cos(), a.sin()) * r0,
                c + vec2(a.cos(), a.sin()) * r1,
            ],
            Stroke::new(1.0, t::TICK),
        );
        x += 10.0;
    }
    // needle
    let tip = c + vec2(tv.cos(), tv.sin()) * (radius - w);
    p.line_segment([c, tip], Stroke::new(2.4, t::INK));
    p.circle_filled(c, 5.0, t::INK);
    p.circle_filled(c, 2.4, t::BG);
}

// --------------------------------------------------------------------------------- chart

/// A plot frame with linear axes. Maps data to the rect.
#[derive(Clone, Copy)]
pub struct Plot {
    pub rect: Rect,
    pub x: (f64, f64),
    pub y: (f64, f64),
}

impl Plot {
    pub fn pt(&self, x: f64, y: f64) -> Pos2 {
        let fx = ((x - self.x.0) / (self.x.1 - self.x.0)) as f32;
        let fy = ((y - self.y.0) / (self.y.1 - self.y.0)) as f32;
        pos2(
            self.rect.left() + fx * self.rect.width(),
            self.rect.bottom() - fy * self.rect.height(),
        )
    }
    pub fn inv_x(&self, px: f32) -> f64 {
        self.x.0 + ((px - self.rect.left()) / self.rect.width()) as f64 * (self.x.1 - self.x.0)
    }
    /// Grid + tick labels. `xs`/`ys` are tick steps.
    #[allow(clippy::too_many_arguments)]
    pub fn axes(
        &self,
        p: &Painter,
        xs: f64,
        ys: f64,
        xl: &str,
        yl: &str,
        decimals_x: usize,
        decimals_y: usize,
    ) {
        p.rect_filled(self.rect, r(0), t::with_alpha(t::BG, 140));
        let mut x = (self.x.0 / xs).ceil() * xs;
        while x <= self.x.1 + 1e-9 {
            let a = self.pt(x, self.y.0);
            p.line_segment([a, pos2(a.x, self.rect.top())], Stroke::new(1.0, t::GRID));
            text(
                p,
                pos2(a.x, a.y + 6.0),
                Align2::CENTER_TOP,
                &format!("{x:.decimals_x$}"),
                mono(10.0),
                t::MUTED,
            );
            x += xs;
        }
        let mut y = (self.y.0 / ys).ceil() * ys;
        while y <= self.y.1 + 1e-9 {
            let a = self.pt(self.x.0, y);
            p.line_segment([a, pos2(self.rect.right(), a.y)], Stroke::new(1.0, t::GRID));
            text(
                p,
                pos2(a.x - 6.0, a.y),
                Align2::RIGHT_CENTER,
                &format!("{y:.decimals_y$}"),
                mono(10.0),
                t::MUTED,
            );
            y += ys;
        }
        p.rect_stroke(
            self.rect,
            r(0),
            Stroke::new(1.0, t::LINE),
            StrokeKind::Inside,
        );
        text(
            p,
            pos2(self.rect.center().x, self.rect.bottom() + 22.0),
            Align2::CENTER_TOP,
            xl,
            semi(11.0),
            t::INK_2,
        );
        text(
            p,
            pos2(self.rect.left() - 34.0, self.rect.top() - 10.0),
            Align2::LEFT_BOTTOM,
            yl,
            semi(11.0),
            t::INK_2,
        );
    }
    pub fn line(&self, p: &Painter, pts: &[(f64, f64)], stroke: Stroke) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        p.with_clip_rect(self.rect.intersect(p.clip_rect()))
            .add(Shape::line(v, stroke));
    }
}

// ----------------------------------------------------------------------------- motion

/// Dots travelling along a polyline. `phase` is distance travelled (px); motion off passes a constant.
pub fn flow_dots(
    p: &Painter,
    path: &[Pos2],
    phase: f32,
    spacing: f32,
    radius: f32,
    color: Color32,
) {
    if path.len() < 2 || spacing <= 1.0 {
        return;
    }
    let mut seg_len = Vec::with_capacity(path.len());
    let mut total = 0.0;
    for w in path.windows(2) {
        let l = (w[1] - w[0]).length();
        seg_len.push(l);
        total += l;
    }
    let mut d = phase.rem_euclid(spacing);
    while d < total {
        let mut acc = 0.0;
        for (i, l) in seg_len.iter().enumerate() {
            if d <= acc + l {
                let f = if *l > 0.0 { (d - acc) / l } else { 0.0 };
                let q = path[i] + (path[i + 1] - path[i]) * f;
                p.circle_filled(q, radius, color);
                break;
            }
            acc += l;
        }
        d += spacing;
    }
}

/// A pipe: a thick rounded polyline with a darker core, width ∝ flow.
pub fn pipe(p: &Painter, path: &[Pos2], width: f32, color: Color32) {
    p.add(Shape::line(
        path.to_vec(),
        Stroke::new(width + 4.0, t::with_alpha(t::BG, 220)),
    ));
    p.add(Shape::line(
        path.to_vec(),
        Stroke::new(width, t::with_alpha(color, 70)),
    ));
    p.add(Shape::line(
        path.to_vec(),
        Stroke::new((width * 0.35).max(1.0), t::with_alpha(color, 150)),
    ));
    for q in path {
        p.circle_filled(*q, width * 0.5, t::with_alpha(color, 70));
    }
}

/// Ease toward a target through egui's own animation memory; `secs` 0 snaps (reduced motion, frozen).
pub fn ease(ctx: &egui::Context, id: &str, target: f32, secs: f32) -> f32 {
    ctx.animate_value_with_time(egui::Id::new(("screens.ease", id)), target, secs)
}

// ------------------------------------------------------------------------------- glyphs

pub fn lock_glyph(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let body = Rect::from_center_size(pos2(c.x, c.y + s * 0.35), vec2(s * 1.6, s * 1.2));
    p.rect_filled(body, r(2), col);
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI + std::f32::consts::PI * i as f32 / 12.0;
            pos2(
                c.x + a.cos() * s * 0.55,
                body.top() - 0.5 + a.sin() * s * 0.75,
            )
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.4, col)));
}

/// A small hand-drawn straight arrow from `from` to `to` (the fonts' subset carries no arrows).
pub fn arrow_to(p: &Painter, from: Pos2, to: Pos2, w: f32, col: Color32) {
    let d = to - from;
    if d.length() < 1.0 {
        return;
    }
    let dir = d.normalized();
    let n = vec2(-dir.y, dir.x);
    p.line_segment([from, to - dir * w * 1.8], Stroke::new(w.max(1.2), col));
    p.add(Shape::convex_polygon(
        vec![
            to,
            to - dir * w * 3.4 + n * w * 1.5,
            to - dir * w * 3.4 - n * w * 1.5,
        ],
        col,
        Stroke::NONE,
    ));
}

/// A hand-drawn right chevron, `s` tall, its tip at `at` (the fonts' subset carries no `›`).
pub fn chevron(p: &Painter, tip: Pos2, s: f32, col: Color32) {
    p.add(Shape::line(
        vec![
            pos2(tip.x - s * 0.42, tip.y - s * 0.5),
            tip,
            pos2(tip.x - s * 0.42, tip.y + s * 0.5),
        ],
        Stroke::new(1.6, col),
    ));
}

/// The same chevron pointing left (a "back" mark), drawn - the UI fonts carry no `‹`.
pub fn chevron_left(p: &Painter, tip: Pos2, s: f32, col: Color32) {
    p.add(Shape::line(
        vec![
            pos2(tip.x + s * 0.42, tip.y - s * 0.5),
            tip,
            pos2(tip.x + s * 0.42, tip.y + s * 0.5),
        ],
        Stroke::new(1.6, col),
    ));
}

/// Like [`flow_dots`], but each mark is a short dash along the path: an air stream reads differently
/// from a water stream at a glance.
pub fn flow_ticks(p: &Painter, path: &[Pos2], phase: f32, spacing: f32, len: f32, col: Color32) {
    let st = Stroke::new(1.6, col);
    let start = (phase.rem_euclid(spacing)) - spacing;
    let mut carry = start;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let l = d.length();
        if l < 1.0 {
            continue;
        }
        let dir = d / l;
        let mut t = carry;
        while t < l {
            let c = a + dir * t;
            if t > -len * 0.5 {
                p.line_segment([c - dir * len * 0.5, c + dir * len * 0.5], st);
            }
            t += spacing;
        }
        carry = t - l;
    }
}

/// The nav glyphs, drawn in a `s`-sized box centred on `c`.
pub fn glyph(p: &Painter, name: &str, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.6, col);
    let h = s * 0.5;
    let at = |x: f32, y: f32| pos2(c.x + x * h, c.y + y * h);
    match name {
        "instrument" => {
            // a tower section: casing, fan stack, fill band
            p.add(Shape::closed_line(
                vec![at(-0.8, 0.9), at(-0.8, -0.35), at(0.8, -0.35), at(0.8, 0.9)],
                st,
            ));
            p.add(Shape::line(
                vec![
                    at(-0.42, -0.35),
                    at(-0.5, -0.9),
                    at(0.5, -0.9),
                    at(0.42, -0.35),
                ],
                st,
            ));
            p.rect_filled(
                Rect::from_min_max(at(-0.8, 0.05), at(0.8, 0.45)),
                r(0),
                t::with_alpha(col, 120),
            );
        }
        "crossflow" => {
            p.add(Shape::closed_line(
                vec![at(-0.9, 0.9), at(-0.6, -0.4), at(0.6, -0.4), at(0.9, 0.9)],
                st,
            ));
            p.add(Shape::line(
                vec![
                    at(-0.35, -0.4),
                    at(-0.4, -0.9),
                    at(0.4, -0.9),
                    at(0.35, -0.4),
                ],
                st,
            ));
            p.line_segment([at(-1.0, 0.3), at(-0.3, 0.3)], st);
            p.line_segment([at(1.0, 0.3), at(0.3, 0.3)], st);
        }
        "size" => {
            for (i, hh) in [0.6f32, 1.1, 1.6].iter().enumerate() {
                let x = -0.75 + i as f32 * 0.6;
                p.rect_filled(
                    Rect::from_min_max(at(x, 0.9 - hh), at(x + 0.38, 0.9)),
                    r(1),
                    if i == 2 { col } else { t::with_alpha(col, 120) },
                );
            }
        }
        "rate" => {
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = 150f32.to_radians() + 240f32.to_radians() * i as f32 / 20.0;
                    at(a.cos() * 0.85, a.sin() * 0.85 + 0.1)
                })
                .collect();
            p.add(Shape::line(pts, st));
            p.line_segment([at(0.0, 0.1), at(0.45, -0.4)], Stroke::new(1.8, col));
            p.circle_filled(at(0.0, 0.1), 2.0, col);
        }
        "curves" => {
            p.line_segment([at(-0.9, -0.9), at(-0.9, 0.9)], st);
            p.line_segment([at(-0.9, 0.9), at(0.9, 0.9)], st);
            for k in 0..3 {
                let off = k as f32 * 0.32;
                let pts: Vec<Pos2> = (0..=10)
                    .map(|i| {
                        let x = -0.75 + 1.6 * i as f32 / 10.0;
                        at(x, 0.55 - off - 0.55 * ((x + 0.75) / 1.6).powf(1.4))
                    })
                    .collect();
                p.add(Shape::line(
                    pts,
                    Stroke::new(1.4, if k == 1 { col } else { t::with_alpha(col, 120) }),
                ));
            }
        }
        "water" => {
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = i as f32 / 24.0 * std::f32::consts::TAU;
                    let y = a.sin() * 0.55 + 0.3;
                    let x = a.cos() * 0.55;
                    if y < 0.0 {
                        at(x * (1.0 + y * 1.2).max(0.0), y)
                    } else {
                        at(x, y)
                    }
                })
                .collect();
            p.add(Shape::closed_line(pts, st));
            p.line_segment([at(0.0, -0.95), at(-0.42, -0.1)], st);
            p.line_segment([at(0.0, -0.95), at(0.42, -0.1)], st);
        }
        "compare" => {
            p.rect_stroke(
                Rect::from_min_max(at(-0.9, -0.8), at(-0.12, 0.85)),
                r(2),
                st,
                StrokeKind::Middle,
            );
            p.rect_stroke(
                Rect::from_min_max(at(0.12, -0.8), at(0.9, 0.85)),
                r(2),
                st,
                StrokeKind::Middle,
            );
            p.line_segment([at(-0.7, -0.2), at(-0.32, -0.2)], st);
            p.line_segment([at(0.32, 0.25), at(0.7, 0.25)], Stroke::new(2.2, col));
        }
        "report" => {
            p.add(Shape::closed_line(
                vec![
                    at(-0.7, -0.95),
                    at(0.35, -0.95),
                    at(0.7, -0.6),
                    at(0.7, 0.95),
                    at(-0.7, 0.95),
                ],
                st,
            ));
            for (i, w) in [0.9f32, 1.1, 0.7].iter().enumerate() {
                let y = -0.35 + i as f32 * 0.4;
                p.line_segment([at(-0.45, y), at(-0.45 + w, y)], Stroke::new(1.2, col));
            }
        }
        "open" => {
            p.add(Shape::closed_line(
                vec![
                    at(-0.9, -0.6),
                    at(-0.3, -0.6),
                    at(-0.15, -0.4),
                    at(0.9, -0.4),
                    at(0.9, 0.75),
                    at(-0.9, 0.75),
                ],
                st,
            ));
        }
        "save" => {
            p.line_segment([at(0.0, -0.9), at(0.0, 0.35)], st);
            p.add(Shape::line(
                vec![at(-0.4, -0.05), at(0.0, 0.35), at(0.4, -0.05)],
                st,
            ));
            p.add(Shape::line(
                vec![
                    at(-0.85, 0.3),
                    at(-0.85, 0.85),
                    at(0.85, 0.85),
                    at(0.85, 0.3),
                ],
                st,
            ));
        }
        "motion" => {
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let x = -0.9 + 1.8 * i as f32 / 20.0;
                    at(x, (x * 5.0).sin() * 0.35)
                })
                .collect();
            p.add(Shape::line(pts, st));
        }
        "still" => {
            p.line_segment([at(-0.9, 0.0), at(0.9, 0.0)], st);
            p.line_segment([at(-0.35, -0.6), at(-0.35, 0.6)], Stroke::new(2.0, col));
            p.line_segment([at(0.35, -0.6), at(0.35, 0.6)], Stroke::new(2.0, col));
        }
        "user" => {
            p.circle_stroke(at(0.0, -0.35), h * 0.38, st);
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI + std::f32::consts::PI * i as f32 / 12.0;
                    at(a.cos() * 0.7, 0.9 + a.sin() * 0.6)
                })
                .collect();
            p.add(Shape::line(pts, st));
        }
        "chevron" => {
            p.add(Shape::line(
                vec![at(-0.5, -0.25), at(0.0, 0.25), at(0.5, -0.25)],
                st,
            ));
        }
        "close" => {
            p.line_segment([at(-0.5, -0.5), at(0.5, 0.5)], st);
            p.line_segment([at(0.5, -0.5), at(-0.5, 0.5)], st);
        }
        "export" => {
            p.line_segment([at(0.0, 0.35), at(0.0, -0.9)], st);
            p.add(Shape::line(
                vec![at(-0.4, -0.5), at(0.0, -0.9), at(0.4, -0.5)],
                st,
            ));
            p.add(Shape::line(
                vec![
                    at(-0.85, 0.3),
                    at(-0.85, 0.85),
                    at(0.85, 0.85),
                    at(0.85, 0.3),
                ],
                st,
            ));
        }
        _ => {
            p.circle_stroke(c, h * 0.8, st);
        }
    }
}

/// A short horizontal bar meter (0..1) used in cards.
pub fn meter(p: &Painter, rect: Rect, f: f32, color: Color32) {
    p.rect_filled(rect, r(2), t::LINE_SOFT);
    let w = rect.width() * f.clamp(0.0, 1.0);
    p.rect_filled(
        Rect::from_min_size(rect.min, vec2(w, rect.height())),
        r(2),
        color,
    );
}

/// Dashed line helper.
pub fn dashed(p: &Painter, a: Pos2, b: Pos2, stroke: Stroke, dash: f32, gap: f32) {
    let mut v = Vec::new();
    v.extend(Shape::dashed_line(&[a, b], stroke, dash, gap));
    p.extend(v);
}

pub fn fmt1(v: f64) -> String {
    format!("{v:.1}")
}
pub fn fmt0(v: f64) -> String {
    format!("{v:.0}")
}
pub fn fmt2(v: f64) -> String {
    format!("{v:.2}")
}

#[cfg(test)]
mod bind_tests {
    use super::bind_words;

    /// Issue #137 (conductor review): a card line breaks at its operators and between quantities only;
    /// a name, a number and its unit stay together.
    #[test]
    fn an_engineering_line_breaks_only_at_operators() {
        let nb = '\u{a0}';
        let s = bind_words("3.11 + 1.04 kg/s = 4.15 kg/s");
        assert_eq!(s, format!("3.11 + 1.04{nb}kg/s = 4.15{nb}kg/s"));
        let s = bind_words("air volume 125 m³/s · fan efficiency 0.826");
        assert_eq!(
            s,
            format!("air{nb}volume{nb}125{nb}m³/s · fan{nb}efficiency{nb}0.826")
        );
        assert_eq!(
            bind_words("range = hot water − cold water"),
            format!("range = hot{nb}water − cold{nb}water")
        );
    }
}
