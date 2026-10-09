//! The design system. Tokens are **shared with the approved baseline** (`cockpit/src/theme.rs`) so the two
//! view layers read as one product; this file adds only what the instrument layer needs (calibration grid,
//! slot states, drag ghost, rail grounds).
//!
//! Surface archetype: **OPERATE + COMMAND-INSPECT** - you turn a knob (rpm), place parts, and read the
//! section change. Not a dashboard: no KPI wall, no cards grid. The tower section is the interface.

use bevy_egui::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};

// ------------------------------------------------------------------------------------- colour

pub const BG: Color32 = Color32::from_rgb(0x0f, 0x17, 0x1c); // deep ink - the canvas ground
pub const PANEL: Color32 = Color32::from_rgb(0x15, 0x1f, 0x26);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(0x1b, 0x27, 0x30);
pub const LINE: Color32 = Color32::from_rgb(0x2a, 0x3a, 0x45);
pub const LINE_SOFT: Color32 = Color32::from_rgb(0x20, 0x2d, 0x36);
pub const INK: Color32 = Color32::from_rgb(0xe8, 0xee, 0xf1);
pub const INK_2: Color32 = Color32::from_rgb(0xb4, 0xc2, 0xcb);
pub const MUTED: Color32 = Color32::from_rgb(0x7f, 0x91, 0x9c);
pub const PRIMARY: Color32 = Color32::from_rgb(0x2f, 0xb3, 0xc9); // the host teal, lifted for dark ground
pub const PRIMARY_DEEP: Color32 = Color32::from_rgb(0x0b, 0x60, 0x73);
pub const PRIMARY_SOFT: Color32 = Color32::from_rgb(0x14, 0x3a, 0x45);
pub const AMBER: Color32 = Color32::from_rgb(0xe0, 0x9a, 0x3a); // synthetic-data signal ONLY
pub const AMBER_SOFT: Color32 = Color32::from_rgb(0x3a, 0x2c, 0x14);
pub const DANGER: Color32 = Color32::from_rgb(0xe0, 0x63, 0x63); // refusal / limit ONLY
pub const DANGER_SOFT: Color32 = Color32::from_rgb(0x3d, 0x1b, 0x1b);
pub const OK: Color32 = Color32::from_rgb(0x4f, 0xb8, 0x87);
pub const OK_SOFT: Color32 = Color32::from_rgb(0x14, 0x30, 0x27);
pub const WATER: Color32 = Color32::from_rgb(0x4a, 0x9d, 0xe0);
/// Issue #91: the hot end of the water circuit - the water enters at the spray, leaves at the basin, and the
/// falling water is drawn as that cooling (a warm terracotta, distinct from the synthetic-data amber).
pub const WATER_HOT: Color32 = Color32::from_rgb(0xd9, 0x7c, 0x5a);
pub const AIR: Color32 = Color32::from_rgb(0xa8, 0xc8, 0xd8);

// ---- additions for the instrument layer ----

/// Calibration grid behind the section (a measuring ground, not a decoration).
pub const GRID: Color32 = Color32::from_rgb(0x18, 0x24, 0x2c);
/// Ruler ticks and the zero line of the section.
pub const TICK: Color32 = Color32::from_rgb(0x33, 0x45, 0x51);
/// A slot bay: the dashed frame the tray drops into.
pub const SLOT: Color32 = Color32::from_rgb(0x39, 0x50, 0x5d);
/// Drop accepted (the catalog says the part fits).
pub const VALID: Color32 = OK;
/// Drop refused (the catalog says it does not).
pub const INVALID: Color32 = DANGER;
/// The drag ghost carried by the pointer.
pub const GHOST: Color32 = Color32::from_rgb(0x2f, 0xb3, 0xc9);
/// The rail ground under the pressure zones / operating-point rail.
pub const RAIL_BG: Color32 = Color32::from_rgb(0x11, 0x1b, 0x22);

/// Fill-layer palette: same hues as the baseline, so a fill id means the same thing in both crates.
/// The warm middle of the water's thermal walk: a sand tone, so the hue reads as cooling water rather than
/// as blue mixed into orange (a two-stop blue -> terracotta blend passes through grey-violet, which reads as
/// neither). Three stops: basin blue, warm sand, spray terracotta.
pub const WATER_SAND: Color32 = Color32::from_rgb(0xc0, 0x93, 0x66);

/// The falling water's colour at `t01` along its fall: 0.0 = the cold end (the basin), 1.0 = the hot end
/// (the spray header). The two *ends* are the run's own water temperatures; the walk between them - three
/// stops, cool to sand to warm - is this pass's look, and how far down the hot end reaches is scaled by
/// `drafthouse_cockpit_seams::mapping::water_ramp`.
pub fn water_tint(t01: f32) -> Color32 {
    let t01 = t01.clamp(0.0, 1.0);
    let (a, b, u) = if t01 <= 0.5 {
        (WATER, WATER_SAND, t01 * 2.0)
    } else {
        (WATER_SAND, WATER_HOT, (t01 - 0.5) * 2.0)
    };
    let mix = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * u) as u8 };
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

pub fn fill_color(fill_id: &str) -> Color32 {
    match fill_id {
        "FILM-MF20" => Color32::from_rgb(0x5c, 0xa8, 0xb8),
        "FILM-WF25" => Color32::from_rgb(0x8a, 0x7f, 0xc4),
        "FILM-CF19" => Color32::from_rgb(0x6d, 0xb8, 0x9a),
        "FILM-OF25" => Color32::from_rgb(0xc4, 0x9a, 0x6a),
        "FILM-VF38" => Color32::from_rgb(0x7a, 0x9c, 0xc4),
        "TRICKLE-50" => Color32::from_rgb(0xb0, 0x8e, 0x7a),
        "SPLASH-GRID" => Color32::from_rgb(0x9c, 0x8f, 0x6a),
        _ => Color32::from_rgb(0x9a, 0x9a, 0x9a),
    }
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

// --------------------------------------------------------------------------------------- type

pub const SANS: &str = "plex-sans";
pub const SANS_SEMI: &str = "plex-sans-semibold";
pub const MONO: &str = "plex-mono";
pub const MONO_MED: &str = "plex-mono-medium";

pub fn family_semi() -> FontFamily {
    FontFamily::Name(SANS_SEMI.into())
}
pub fn family_mono_med() -> FontFamily {
    FontFamily::Name(MONO_MED.into())
}

pub fn fonts() -> egui::FontDefinitions {
    use egui::FontData;
    use std::sync::Arc;
    let mut d = egui::FontDefinitions::empty();
    d.font_data.insert(
        SANS.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/subset/IBMPlexSans-Regular.ttf"
        ))),
    );
    d.font_data.insert(
        SANS_SEMI.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/subset/IBMPlexSans-SemiBold.ttf"
        ))),
    );
    d.font_data.insert(
        MONO.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/subset/IBMPlexMono-Regular.ttf"
        ))),
    );
    d.font_data.insert(
        MONO_MED.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/subset/IBMPlexMono-Medium.ttf"
        ))),
    );
    d.families.insert(
        FontFamily::Proportional,
        vec![SANS.into(), SANS_SEMI.into()],
    );
    d.families
        .insert(FontFamily::Monospace, vec![MONO.into(), MONO_MED.into()]);
    d.families
        .insert(family_semi(), vec![SANS_SEMI.into(), SANS.into()]);
    d.families
        .insert(family_mono_med(), vec![MONO_MED.into(), MONO.into()]);
    d
}

/// The type scale. The instrument's numbers are mono-medium so they line up column-wise while animating.
pub fn apply_style(ctx: &egui::Context, phone: bool) {
    let theme = ctx.theme();
    ctx.style_mut_of(theme, |style| {
        let s = if phone { 0.95 } else { 1.0 };
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(11.0 * s, FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(13.0 * s, FontFamily::Proportional),
            ),
            (TextStyle::Button, FontId::new(13.0 * s, family_semi())),
            (TextStyle::Heading, FontId::new(18.0 * s, family_semi())),
            (
                TextStyle::Monospace,
                FontId::new(12.0 * s, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, if phone { 9.0 } else { 5.0 });
        // Touch targets: 44 pt is the floor on a phone, so every control clears it there.
        style.spacing.interact_size = egui::vec2(40.0, if phone { 40.0 } else { 24.0 });
        style.spacing.slider_width = if phone { 200.0 } else { 260.0 };
        style.spacing.combo_width = 130.0;
        style.spacing.window_margin = Margin::same(12);

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(INK);
        v.panel_fill = PANEL;
        v.window_fill = PANEL_RAISED;
        v.window_stroke = Stroke::new(1.0, LINE);
        v.extreme_bg_color = BG;
        v.faint_bg_color = PANEL_RAISED;
        v.code_bg_color = BG;
        v.text_edit_bg_color = Some(BG);
        v.hyperlink_color = PRIMARY;
        v.warn_fg_color = AMBER;
        v.error_fg_color = DANGER;
        v.selection.bg_fill = PRIMARY_SOFT;
        v.selection.stroke = Stroke::new(1.0, PRIMARY);
        v.window_corner_radius = CornerRadius::same(6);
        v.widgets.noninteractive.bg_fill = PANEL;
        v.widgets.noninteractive.weak_bg_fill = PANEL;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE_SOFT);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK_2);
        v.widgets.noninteractive.corner_radius = CornerRadius::same(4);
        v.widgets.inactive.bg_fill = PANEL_RAISED;
        v.widgets.inactive.weak_bg_fill = PANEL_RAISED;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
        v.widgets.inactive.corner_radius = CornerRadius::same(4);
        v.widgets.hovered.bg_fill = PRIMARY_SOFT;
        v.widgets.hovered.weak_bg_fill = PRIMARY_SOFT;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
        v.widgets.hovered.corner_radius = CornerRadius::same(4);
        v.widgets.active.bg_fill = PRIMARY_DEEP;
        v.widgets.active.weak_bg_fill = PRIMARY_DEEP;
        v.widgets.active.bg_stroke = Stroke::new(1.0, PRIMARY);
        v.widgets.active.fg_stroke = Stroke::new(1.0, INK);
        v.widgets.active.corner_radius = CornerRadius::same(4);
        v.widgets.open.bg_fill = PRIMARY_SOFT;
        v.widgets.open.weak_bg_fill = PRIMARY_SOFT;
        v.widgets.open.bg_stroke = Stroke::new(1.0, PRIMARY);
        v.widgets.open.fg_stroke = Stroke::new(1.0, INK);
        v.widgets.open.corner_radius = CornerRadius::same(4);
        v.striped = false;
        v.slider_trailing_fill = true;
    });
}

// ------------------------------------------------------------------------------------ helpers

pub fn label(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(11.0)
        .color(MUTED)
        .family(family_semi())
}
pub fn eyebrow(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase())
        .size(9.5)
        .color(MUTED)
        .family(family_semi())
        .extra_letter_spacing(1.1)
}
pub fn body(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(12.5).color(INK_2)
}
pub fn strong(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(13.0)
        .color(INK)
        .family(family_semi())
}
pub fn heading(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(17.0)
        .color(INK)
        .family(family_semi())
}
pub fn num(text: &str, size: f32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(INK)
        .family(family_mono_med())
}
pub fn mono(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(11.5)
        .color(INK_2)
        .family(FontFamily::Monospace)
}
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL_RAISED)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 10))
}
pub fn card_flat() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE_SOFT))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
}
pub fn chip_frame(bg: Color32, line: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, line))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(6, 2))
}

/// Issue #137: **the house number style** - one formatter for every number the cockpit paints, the Report
/// prints and the worked steps substitute. The source is `numfmt.rs` (plain `std`), which the engine
/// adapter compiles too, so a calculation card's substitution and the screen's read-out share every digit.
/// `t::num::with_unit(31.65, "°C")` -> `31.7 °C`; see the module for the table.
#[path = "numfmt.rs"]
pub mod num;

/// A number with no known unit: 3 significant figures (the house default for an unlabelled quantity).
/// Prefer [`num::value`] / [`num::with_unit`] wherever the unit is known.
pub fn fmt(v: f64) -> String {
    num::sig(v, 3)
}

/// A zone's label - the engine's own `PressureZone.label` is authoritative; this is the fallback chip
/// text for a zone the engine has not reported yet.
pub fn zone_fallback_label(z: cockpit::engine::ZoneId) -> &'static str {
    use cockpit::engine::ZoneId::*;
    match z {
        Inlet => "inlet",
        Rain => "rain",
        Fill => "fill",
        Spray => "spray",
        Drift => "drift",
        Plenum => "plenum",
        Stack => "fan stack",
        Fixed => "fixed",
    }
}
