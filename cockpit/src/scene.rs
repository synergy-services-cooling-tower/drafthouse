//! The tower section, drawn by **Bevy sprites** behind the egui instrument.
//!
//! Round 2 draws a **recognisable induced-draft counterflow cell** instead of a labelled rectangle: a
//! velocity-recovery fan stack with rotating blades, a fan deck, casing walls, a chevron drift bank, a
//! nozzle header with heads, two fill layers with distinct textures (film channels and wide-flute rows),
//! an open rain zone on support columns, a cold-water basin and louvred air inlets on **both** sides.
//!
//! One source of geometry: [`layout`] turns the band egui hands the scene into the section, at the
//! fixture's own scale (fill depths, stack area, inlet height). `crate::ui` paints its labels on the
//! *same* rects, so text and geometry cannot drift.
//!
//! One source of *look*: [`plan`] is a pure function from (layout, engine input, engine output, ui state,
//! clock) to a map of drawn sprites, and the Bevy system at the bottom of this file does nothing but
//! apply that map. Every engineering value is quoted from [`EngineOutput`]; every value this file
//! *invents* is a documented function in `drafthouse_cockpit_seams::mapping`, labelled `illustrative` in the
//! frame. See `VISUAL_DATA_SEAMS.md`. Air streamlines and falling water are drawn by `crate::ui` on top
//! of these sprites (they are annotation, and they carry the honesty labels).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiPostUpdateSet;
use cockpit::engine::{EngineInput, EngineOutput, ZoneId};

use crate::app::{AnimClock, Draft, SceneRect};
use crate::state::{
    fill_texture, Anchor, Catalog, FillTexture, Focus, Pattern, Run, Slot, Visual, MAX_LAYERS,
    MAX_NOZZLES_DRAWN,
};
use crate::theme as t;
use drafthouse_cockpit_seams::mapping as m;

// ---------------------------------------------------------------------------------------- layout

/// The section, in egui points (screen space, y down).
#[derive(Clone, Debug)]
pub struct Layout {
    pub area: egui::Rect,
    pub tower: egui::Rect,
    pub ruler: egui::Rect,
    /// The fan-stack band (top): the cylinder lives inside it, [`Layout::stack_cyl`].
    pub stack: egui::Rect,
    /// The drawn stack cylinder: width from the fitted fan's stack area, at the section's scale.
    pub stack_cyl: egui::Rect,
    /// The cylinder's mouth: the ellipse the rim and the fan blades sit in.
    pub stack_rim: egui::Rect,
    /// The fan deck plate under the cylinder.
    pub deck: egui::Rect,
    pub plenum: egui::Rect,
    pub drift: egui::Rect,
    pub spray: egui::Rect,
    pub fill_band: egui::Rect,
    pub rain: egui::Rect,
    pub basin: egui::Rect,
    /// The cold-water surface line inside the basin.
    pub water_surface_y: f32,
    pub plinth: egui::Rect,
    pub inlet_l: egui::Rect,
    pub inlet_r: egui::Rect,
    /// One rect per fill layer, index-aligned with `EngineInput::fill_layers` (0 = top).
    pub layers: Vec<egui::Rect>,
    pub rail: egui::Rect,
    pub op_rail: egui::Rect,
    /// The four bays, in scene order.
    pub slots: [(Slot, egui::Rect); 4],
    /// Metres of fill depth per pixel - the ruler's scale.
    pub m_per_px_y: f32,
    /// Pixels per metre across the face (the section's own horizontal scale).
    pub px_per_m_x: f32,
    /// The face width the fixture's fill area implies (square-face reading).
    pub face_width_m: f32,
    /// The fitted fan's stack diameter, in metres (from its stack area).
    pub stack_diameter_m: f32,
    pub phone: bool,
    /// Issue #81 / #91: **the one scale** - pixels per metre on both axes. `px_per_m_x` and `1 / m_per_px_y`
    /// are this same number (kept as fields because the ruler and the 3D view read them by those names).
    pub px_per_m: f32,
    /// The drawn height of the whole section, in metres (basin floor to stack mouth).
    pub height_m: f32,
    /// Issue #91: the call-out gutter right of the tower - every section label is placed here, on a leader
    /// line to the band it describes, so no label is ever drawn on top of the drawing or of another label.
    pub gutter: egui::Rect,
    /// Issue #91: the read-out HUD's zone (top right of the section), `Rect::NOTHING` when the section is
    /// too narrow to hold it beside the tower - the caller then draws the read-out elsewhere.
    pub hud: egui::Rect,
}

/// Issue #81: the bands are metres, not fractions of the window. These are the drawn heights of the parts the
/// fixture does not dimension - the same constants the 3D tower uses (`seams::mapping`), so the two views
/// agree; the spray zone, the fill and the rain zone are the tower record's own heights.
pub fn band_heights_m(input: Option<&EngineInput>) -> [f32; 8] {
    let plan = crate::state::cell_plan_m(input) as f32;
    let stack_d = crate::state::stack_diameter_m(input) as f32;
    let stack_h = m::stack_height_m(stack_d as f64) as f32;
    let spray = input
        .map(|i| i.tower.spray_zone_height_m as f32)
        .unwrap_or(0.6);
    let rain = input
        .map(|i| i.tower.rain_zone_height_m as f32)
        .unwrap_or(1.4);
    // The fill band holds the deepest stack the tower record offers (so a layer added later fits the same
    // drawing), or the authored stack if a custom one is deeper.
    let authored: f32 = input
        .map(|i| i.fill_layers.iter().map(|l| l.depth_m as f32).sum())
        .unwrap_or(0.0);
    let span = input
        .and_then(|i| i.tower.fill_depth_options_m.last().copied())
        .map(|d| d as f32)
        .unwrap_or(FALLBACK_MAX_DEPTH)
        .max(authored);
    [
        stack_h,
        m::DECK_T_M as f32,
        plan * m::PLENUM_HEIGHT_FACTOR as f32,
        m::DRIFT_BANK_T_M as f32,
        spray,
        span,
        rain,
        m::BASIN_DEPTH_M as f32,
    ]
}

/// The tower record's depth options set the ruler's domain; this is the fallback when there is no draft.
const FALLBACK_MAX_DEPTH: f32 = 2.1;

/// How much wider the stack mouth is than its base: a velocity-recovery flare. The fixture carries a stack
/// *area* and a recovery factor, no profile, so the flare is a look (and the README says so).
pub const STACK_FLARE: f32 = 1.14;

/// Segments per ring when an ellipse is drawn as sprite bars (the stack rim uses [`RIM`]).
const RING: u32 = 48;
/// Slices of the fan's disc and of its hub when an ellipse is filled (the renderer draws rectangles).
const DISC: u32 = 44;
const HUBSLICE: u32 = 22;
/// The wheel's halo rings.
const FAN_GLOW: u32 = 16;

/// Issue #91: headroom above the stack mouth (m) - room for the plume the streamlines leave in.
pub const HEADROOM_M: f32 = 0.9;
/// Issue #91: the call-out gutter's width (points), desktop / phone.
pub const GUTTER_W: f32 = 232.0;
pub const GUTTER_W_PHONE: f32 = 128.0;
/// Issue #91: the read-out HUD column (points) - only when the section is wide enough to hold it beside
/// the tower and the gutter.
/// #91 round 2: the HUD slot is the answer card's - wide enough for the duty's four inputs on one row.
pub const HUD_W: f32 = 306.0;
pub const HUD_MIN_SECTION_W: f32 = 1000.0;
/// Issue #81: **the section's agreed minimum height** (points). The section is the drawing; the answer card
/// is an overlay. When the window cannot hold both, the card takes what is above the minimum and scrolls
/// inside it, so the section is never laid out shorter than this. The number is the phone's own resting
/// section - the smallest supported viewport, whose eight label plates fit their column at the type floor
/// (measured 234 px at 390x844; asserted for all four target sizes in
/// `tests::the_section_keeps_its_agreed_minimum`).
pub const MIN_SECTION_H: f32 = 230.0;
/// Issue #81: the gap between the answer card and the section when the card sits above the tower.
pub const CARD_GAP: f32 = 4.0;
/// Issue #81: the most height the answer card may take when it sits **above** the tower. The section keeps
/// [`MIN_SECTION_H`] and the card scrolls inside what is left (a card is never a reason for the drawing to
/// be squeezed). The `120.0` floor is the least a card can be and still show its duty row.
pub fn card_max_h(avail_h: f32) -> f32 {
    (avail_h - MIN_SECTION_H - CARD_GAP).max(120.0).min(avail_h)
}
/// Issue #81: is there room for the answer card's own slot (a HUD) beside the tower and the gutter? Below
/// this width the card goes **above** the tower instead. One rule, read by [`layout`] and by `crate::ui`.
pub fn hud_fits(area_w: f32, phone: bool) -> bool {
    !phone && area_w >= HUD_MIN_SECTION_W
}
/// The largest scale the section is drawn at (px per metre): past this the tower stops growing and the
/// spare room is letterboxed, so a 4K window does not draw a 2 m-tall fan bay.
pub const MAX_PX_PER_M: f32 = 92.0;

/// The section's layout. **Issue #81: one metres-to-points scale for both axes.** Every band's height is a
/// length in metres ([`band_heights_m`]) and the face width is the tower record's square reading of
/// `fillAreaM2`; the scale is the largest one at which the whole tower, its headroom and its call-out gutter
/// fit, and the spare room is letterboxed. Proportions never depend on the window's aspect.
pub fn layout(area: egui::Rect, input: Option<&EngineInput>, phone: bool) -> Layout {
    let bands_m = band_heights_m(input);
    let height_m: f32 = bands_m.iter().sum();
    let face_width_m = crate::state::cell_plan_m(input) as f32;
    let stack_diameter_m = input
        .map(|i| m::stack_diameter_m(i.fan.stack_area_m2) as f32)
        .unwrap_or(5.0);

    let ruler_w = if phone { 0.0 } else { 46.0 };
    // A desktop draws the louvres outside the casing; a phone draws them inside it (no room either side).
    let louvre_w = if phone { 9.0 } else { 13.0 };
    let louvre_out = if phone { 0.0 } else { louvre_w + 2.0 };
    let rail_w = if phone { 8.0 } else { 12.0 };
    let gap = if phone { 6.0 } else { 16.0 };
    let gutter_w = if phone { GUTTER_W_PHONE } else { GUTTER_W };
    let hud_w = if hud_fits(area.width(), phone) {
        HUD_W
    } else {
        0.0
    };
    let pad_x = if phone { 4.0 } else { 14.0 };
    let pad_y = if phone { 6.0 } else { 14.0 };
    let fixed_w = pad_x * 2.0
        + ruler_w
        + louvre_out * 2.0
        + gap
        + rail_w
        + gap
        + gutter_w
        + if hud_w > 0.0 { hud_w + gap } else { 0.0 };
    let avail_w = (area.width() - fixed_w).max(40.0);
    let avail_h = (area.height() - pad_y * 2.0).max(40.0);
    // One scale for the whole section: the height that fits, the width that fits, the cap, floored at 4 px/m.
    let upper = (avail_w / face_width_m.max(1.0)).clamp(4.0, MAX_PX_PER_M);
    let px_per_m = (avail_h / (height_m + HEADROOM_M)).clamp(4.0, upper);

    let tower_w = face_width_m * px_per_m;
    let comp_w = ruler_w + louvre_out * 2.0 + tower_w + gap + rail_w + gap + gutter_w;
    let comp_h = (height_m + HEADROOM_M) * px_per_m;
    let room_w = area.width() - pad_x * 2.0 - if hud_w > 0.0 { hud_w + gap } else { 0.0 };
    let x0 = area.left() + pad_x + ((room_w - comp_w) * 0.5).max(0.0);
    let y0 = area.top() + pad_y + ((avail_h - comp_h) * 0.5).max(0.0);

    let tower_x0 = x0 + ruler_w + louvre_out;
    let tower_x1 = tower_x0 + tower_w;
    let ruler = egui::Rect::from_min_max(
        egui::pos2(x0, y0),
        egui::pos2(tower_x0 - louvre_out, y0 + comp_h),
    );

    // ---- the bands, top -> bottom, each its own length in metres at the one scale
    let mut y = y0 + HEADROOM_M * px_per_m;
    let mut band = |metres: f32| {
        let r = egui::Rect::from_min_max(
            egui::pos2(tower_x0, y),
            egui::pos2(tower_x1, y + metres * px_per_m),
        );
        y += metres * px_per_m;
        r
    };
    let stack = band(bands_m[0]);
    let deck = band(bands_m[1]);
    let plenum = band(bands_m[2]);
    let drift = band(bands_m[3]);
    let spray = band(bands_m[4]);
    let fill_band = band(bands_m[5]);
    let rain = band(bands_m[6]);
    let basin = band(bands_m[7]);
    // The plinth is the slab the basin stands on: a few points, not a band of the drawing.
    let plinth = egui::Rect::from_min_max(
        egui::pos2(tower_x0, basin.bottom()),
        egui::pos2(tower_x1, basin.bottom() + if phone { 3.0 } else { 5.0 }),
    );

    // ---- the stack: a tapered cylinder the full height of its band, at the section's scale
    let base_w = (stack_diameter_m * px_per_m).clamp(24.0, (tower_w * 0.92).max(24.0));
    let top_w = (base_w * STACK_FLARE).min(tower_w * 0.96);
    let cx = tower_x0 + tower_w * 0.5;
    let rim_h = (top_w * 0.20).clamp(8.0, 26.0);
    let stack_cyl = egui::Rect::from_min_max(
        egui::pos2(cx - top_w * 0.5, stack.top()),
        egui::pos2(cx + top_w * 0.5, stack.bottom() - 1.0),
    );
    let stack_rim = egui::Rect::from_center_size(
        egui::pos2(cx, stack.top() + rim_h * 0.5),
        egui::vec2(top_w, rim_h),
    );

    // ---- the inlets: in the rain zone, as tall as the record's inlet area over two faces of the cell
    // (`inletAreaM2 / (2 x plan)`), never taller than the rain zone itself.
    let inlet_m = input
        .map(|i| (i.tower.inlet_area_m2 / (2.0 * face_width_m.max(1.0) as f64)) as f32)
        .unwrap_or(1.2)
        .min(bands_m[6] * 0.92)
        .max(0.3);
    let inlet_h = inlet_m * px_per_m;
    let inlet_bottom = rain.bottom() - 1.0;
    let (in_l0, in_l1, in_r0, in_r1) = if phone {
        (
            tower_x0 + 2.0,
            tower_x0 + 2.0 + louvre_w,
            tower_x1 - 2.0 - louvre_w,
            tower_x1 - 2.0,
        )
    } else {
        (
            tower_x0 - louvre_w - 2.0,
            tower_x0 - 2.0,
            tower_x1 + 2.0,
            tower_x1 + 2.0 + louvre_w,
        )
    };
    let inlet_l = egui::Rect::from_min_max(
        egui::pos2(in_l0, inlet_bottom - inlet_h),
        egui::pos2(in_l1, inlet_bottom),
    );
    let inlet_r = egui::Rect::from_min_max(
        egui::pos2(in_r0, inlet_bottom - inlet_h),
        egui::pos2(in_r1, inlet_bottom),
    );

    // ---- the fill layers: each its own depth in metres, stacked up from the bottom of the fill band (the
    // band holds the deepest stack the record offers; what the stack does not use is spare depth above it)
    let layers_in: Vec<f64> = input
        .map(|i| i.fill_layers.iter().map(|l| l.depth_m).collect())
        .unwrap_or_default();
    let mut rects = vec![egui::Rect::NOTHING; layers_in.len()];
    let mut bottom = fill_band.bottom();
    for i in (0..layers_in.len()).rev() {
        let lh = (layers_in[i] as f32 * px_per_m).max(m::MIN_BAND_PX.min(fill_band.height()));
        let top = (bottom - lh).max(fill_band.top() - m::MIN_BAND_PX);
        rects[i] = egui::Rect::from_min_max(
            egui::pos2(tower_x0 + 3.0, top),
            egui::pos2(tower_x1 - 3.0, bottom - 1.0),
        );
        bottom = top - 1.0;
    }
    let m_per_px_y = 1.0 / px_per_m;

    // ---- the pressure split rail and the call-out gutter, right of the drawing
    let rail_x0 = tower_x1 + louvre_out + gap;
    let rail = egui::Rect::from_min_max(
        egui::pos2(rail_x0, stack.top()),
        egui::pos2(rail_x0 + rail_w, basin.bottom()),
    );
    let gutter = egui::Rect::from_min_max(
        egui::pos2(rail.right() + gap, stack.top()),
        egui::pos2(
            (rail.right() + gap + gutter_w).min(area.right() - 2.0),
            basin.bottom(),
        ),
    );
    let hud = if hud_w > 0.0 {
        egui::Rect::from_min_max(
            egui::pos2(area.right() - pad_x - hud_w, area.top() + pad_y),
            egui::pos2(area.right() - pad_x, area.bottom() - pad_y),
        )
    } else {
        egui::Rect::NOTHING
    };

    let op_rail = egui::Rect::from_min_max(
        egui::pos2(tower_x0 + 8.0, plenum.center().y - 3.0),
        egui::pos2(tower_x1 - 8.0, plenum.center().y + 3.0),
    );
    let water_surface_y = basin.top() + basin.height() * 0.34;

    let slots = [
        (
            Slot::Fan,
            egui::Rect::from_min_max(
                egui::pos2(stack_cyl.left() - 4.0, stack.top() + 1.0),
                egui::pos2(stack_cyl.right() + 4.0, deck.bottom()),
            ),
        ),
        (
            Slot::Drift,
            egui::Rect::from_min_max(
                egui::pos2(tower_x0 + 3.0, drift.top()),
                egui::pos2(tower_x1 - 3.0, drift.bottom()),
            ),
        ),
        (
            Slot::Fill,
            egui::Rect::from_min_max(
                egui::pos2(tower_x0 + 3.0, fill_band.top()),
                egui::pos2(tower_x1 - 3.0, fill_band.bottom()),
            ),
        ),
        (
            Slot::Nozzle,
            egui::Rect::from_min_max(
                egui::pos2(tower_x0 + 3.0, spray.top()),
                egui::pos2(tower_x1 - 3.0, spray.bottom()),
            ),
        ),
    ];

    Layout {
        area,
        tower: egui::Rect::from_min_max(
            egui::pos2(tower_x0, stack.top()),
            egui::pos2(tower_x1, plinth.bottom()),
        ),
        ruler,
        stack,
        stack_cyl,
        stack_rim,
        deck,
        plenum,
        drift,
        spray,
        fill_band,
        rain,
        basin,
        water_surface_y,
        plinth,
        inlet_l,
        inlet_r,
        layers: rects,
        rail,
        op_rail,
        slots,
        m_per_px_y,
        px_per_m_x: px_per_m,
        face_width_m,
        stack_diameter_m,
        phone,
        px_per_m,
        height_m,
        gutter,
        hud,
    }
}

// ------------------------------------------------------------------------------------ components

/// Every sprite the scene can draw. Fixed pool: the plan decides which ones are visible.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    Ground,
    GridH(u8),
    GridV(u8),
    Casing,
    CasingEdge(u8),
    /// The fan deck plate under the stack.
    DeckPlate,
    DeckRib(u8),
    /// The two walls of the tapered stack cylinder.
    StackWall(u8),
    /// Segments of the stack mouth's ellipse.
    StackRim(u8),
    /// Segments of the ellipse where the cylinder meets the deck.
    StackRimBase(u8),
    /// The stack's mouth (the aperture the air leaves through).
    StackMouth,
    FanDisc,
    /// Vertical slices of the fan's disc (the renderer draws rectangles; a disc is tiled slices).
    DiscSlice(u8),
    /// Segments of the fan disc's ellipse (the wheel's rim, seen from above).
    FanRing(u8),
    /// One of the wheel's spokes.
    FanSpoke(u8),
    FanBlade(u8),
    /// A blade's own leading edge (a bright line from its bar to its tip).
    FanBladeTip(u8),
    FanHub,
    /// Vertical slices of the hub (the filled cap).
    FanHubCap(u8),
    /// Rings of the wheel's halo - the moving column of air at the mouth.
    FanGlow(u8),
    OpRailBar,
    OpTick(u8),
    OpCaretCurrent,
    OpCaretAnchor,
    PlenumBox,
    DriftBox,
    /// Chevron segments of the drift-eliminator bank.
    DriftChev(u8),
    DriftHaze(u8),
    SprayBox,
    /// The distribution header pipe.
    Header,
    /// A nozzle head hanging off the header.
    HeaderStub(u8),
    NozzleTip(u8),
    ConeEdge(u8, u8),
    ConeBar(u8, u8),
    FillBack,
    Layer(u8),
    /// Texture inside a layer: film channels, wide-flute rows, a grid or splash bars.
    FillDetail(u8, u8),
    LayerTick(u8),
    Coverage(u8),
    RainBox,
    /// A support column in the open rain zone.
    Column(u8),
    BasinBox,
    WaterBody,
    /// The bright surface line on the basin water.
    WaterSurface,
    InletBox(u8),
    /// A louvre slat (both inlets draw the same pool).
    Louvre(u8, u8),
    RailBg,
    RailSeg(u8),
    SlotPlate(u8),
    SlotWash(u8),
    /// The affordance outline on the bay the pointer / keyboard is on.
    SlotFocus(u8),
}

impl Part {
    fn z(self) -> f32 {
        match self {
            Part::Ground => 0.2,
            Part::GridH(_) | Part::GridV(_) => 0.35,
            Part::Casing => 1.0,
            Part::SlotPlate(_) => 1.6,
            Part::DeckPlate
            | Part::StackMouth
            | Part::PlenumBox
            | Part::DriftBox
            | Part::SprayBox
            | Part::RainBox
            | Part::BasinBox
            | Part::FillBack
            | Part::InletBox(_) => 2.0,
            Part::WaterBody => 2.15,
            Part::StackWall(_) => 2.2,
            Part::Column(_) => 2.3,
            Part::WaterSurface => 2.35,
            Part::DeckRib(_) => 2.45,
            Part::FanDisc => 2.5,
            Part::DiscSlice(_) => 2.5,
            Part::FanRing(_) => 2.52,
            Part::Louvre(_, _) => 2.55,
            Part::StackRimBase(_) => 2.6,
            Part::RailBg => 2.62,
            Part::FanBlade(_) => 2.65,
            Part::FanBladeTip(_) => 2.67,
            Part::FanSpoke(_) => 2.62,
            Part::FillDetail(_, _) => 2.8,
            Part::Layer(_) => 3.0,
            Part::LayerTick(_) => 3.45,
            Part::RailSeg(_) => 3.2,
            Part::Coverage(_) => 3.4,
            Part::DriftChev(_) => 3.5,
            Part::OpRailBar | Part::OpTick(_) => 3.52,
            Part::DriftHaze(_) => 3.6,
            Part::Header => 3.8,
            Part::HeaderStub(_) => 3.9,
            Part::NozzleTip(_) => 4.0,
            Part::ConeBar(_, _) | Part::ConeEdge(_, _) => 4.2,
            Part::OpCaretAnchor => 4.3,
            Part::OpCaretCurrent => 4.4,
            Part::FanHub => 4.56,
            Part::FanHubCap(_) => 4.52,
            Part::FanGlow(_) => 2.51,
            Part::StackRim(_) => 4.6,
            Part::SlotWash(_) => 6.5,
            Part::SlotFocus(_) => 6.8,
            Part::CasingEdge(_) => 8.0,
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Viz(pub Part);

/// Issue #91 / #82: how many frames this session has drawn. The page publishes it as `data-frames`, so a
/// measurement can sample it twice and read the real presented-frame rate - `requestAnimationFrame` from
/// outside is throttled in a headless browser and cannot see the app's own loop.
static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Frames drawn since the app started.
pub fn frames() -> u32 {
    FRAMES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Entity per part, so the plan is applied by lookup instead of a 40-arm match.
#[derive(Resource, Default)]
pub struct Pool(pub HashMap<Part, Entity>);

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_pool)
            .add_systems(PostUpdate, sync_scene.after(EguiPostUpdateSet::EndPass));
    }
}

pub const GRID_H: u8 = 5;
pub const GRID_V: u8 = 7;
pub const LOUVRES: u8 = 9;
pub const DRIFT_CHEV: u8 = 30;
pub const COLUMNS: u8 = 6;
pub const FILL_DETAIL: u8 = 64;
pub const RIM: u8 = 16;
pub const NOZZLES: u8 = MAX_NOZZLES_DRAWN as u8;
pub const CONE_BARS: u8 = 8;
pub const RAIL_SEGS: u8 = 16;

fn spawn_pool(mut commands: Commands) {
    let mut pool: HashMap<Part, Entity> = HashMap::new();
    {
        let mut spawn = |part: Part, pool: &mut HashMap<Part, Entity>| {
            let e = commands
                .spawn((
                    Sprite::from_color(Color::WHITE, Vec2::ONE),
                    Transform::default(),
                    Viz(part),
                    Visibility::Hidden,
                ))
                .id();
            pool.insert(part, e);
        };
        spawn(Part::Ground, &mut pool);
        for i in 0..GRID_H {
            spawn(Part::GridH(i), &mut pool);
        }
        for i in 0..GRID_V {
            spawn(Part::GridV(i), &mut pool);
        }
        spawn(Part::Casing, &mut pool);
        for i in 0..4 {
            spawn(Part::CasingEdge(i), &mut pool);
        }
        for p in [
            Part::DeckPlate,
            Part::StackMouth,
            Part::PlenumBox,
            Part::DriftBox,
            Part::SprayBox,
            Part::RainBox,
            Part::BasinBox,
            Part::FillBack,
            Part::WaterBody,
            Part::WaterSurface,
            Part::Header,
            Part::OpRailBar,
            Part::OpCaretCurrent,
            Part::OpCaretAnchor,
            Part::FanDisc,
            Part::FanHub,
            Part::RailBg,
        ] {
            spawn(p, &mut pool);
        }
        for i in 0..3 {
            spawn(Part::DeckRib(i), &mut pool);
        }
        for i in 0..2 {
            spawn(Part::StackWall(i), &mut pool);
        }
        for i in 0..RIM {
            spawn(Part::StackRim(i), &mut pool);
            spawn(Part::StackRimBase(i), &mut pool);
        }
        for i in 0..8 {
            spawn(Part::FanBlade(i), &mut pool);
            spawn(Part::FanBladeTip(i), &mut pool);
        }
        for i in 0..RING {
            spawn(Part::FanRing(i as u8), &mut pool);
        }
        for i in 0..8 {
            spawn(Part::FanSpoke(i), &mut pool);
        }
        for i in 0..HUBSLICE {
            spawn(Part::FanHubCap(i as u8), &mut pool);
        }
        for i in 0..DISC {
            spawn(Part::DiscSlice(i as u8), &mut pool);
        }
        for i in 0..FAN_GLOW {
            spawn(Part::FanGlow(i as u8), &mut pool);
        }
        for i in 0..5 {
            spawn(Part::OpTick(i), &mut pool);
        }
        for i in 0..3 {
            spawn(Part::DriftHaze(i), &mut pool);
        }
        for i in 0..DRIFT_CHEV {
            spawn(Part::DriftChev(i), &mut pool);
        }
        for i in 0..NOZZLES {
            spawn(Part::NozzleTip(i), &mut pool);
            spawn(Part::HeaderStub(i), &mut pool);
            for j in 0..4 {
                spawn(Part::ConeEdge(i, j), &mut pool);
            }
            for j in 0..(CONE_BARS * 2) {
                spawn(Part::ConeBar(i, j), &mut pool);
            }
        }
        for i in 0..(MAX_LAYERS as u8) {
            spawn(Part::Layer(i), &mut pool);
            spawn(Part::LayerTick(i), &mut pool);
            spawn(Part::Coverage(i), &mut pool);
        }
        for l in 0..(MAX_LAYERS as u8) {
            for i in 0..FILL_DETAIL {
                spawn(Part::FillDetail(l, i), &mut pool);
            }
        }
        for i in 0..COLUMNS {
            spawn(Part::Column(i), &mut pool);
        }
        for i in 0..2 {
            spawn(Part::InletBox(i), &mut pool);
        }
        for side in 0..2u8 {
            for i in 0..LOUVRES {
                spawn(Part::Louvre(side, i), &mut pool);
            }
        }
        for i in 0..RAIL_SEGS {
            spawn(Part::RailSeg(i), &mut pool);
        }
        for i in 0..4 {
            spawn(Part::SlotPlate(i), &mut pool);
            spawn(Part::SlotWash(i), &mut pool);
            spawn(Part::SlotFocus(i), &mut pool);
        }
    }
    commands.insert_resource(Pool(pool));
}

// ------------------------------------------------------------------------------------------ plan

/// One sprite the plan wants on screen.
#[derive(Clone, Copy, Debug)]
pub struct Draw {
    pub center: egui::Pos2,
    pub size: egui::Vec2,
    /// Rotation in egui screen space (clockwise), radians.
    pub rot: f32,
    pub color: egui::Color32,
}

impl Draw {
    fn rect(r: egui::Rect, color: egui::Color32) -> Draw {
        Draw {
            center: r.center(),
            size: r.size(),
            rot: 0.0,
            color,
        }
    }
    fn at(center: egui::Pos2, size: egui::Vec2, color: egui::Color32) -> Draw {
        Draw {
            center,
            size,
            rot: 0.0,
            color,
        }
    }
    fn bar(center: egui::Pos2, len: f32, w: f32, rot_rad: f32, color: egui::Color32) -> Draw {
        Draw {
            center,
            size: egui::vec2(len, w),
            rot: rot_rad,
            color,
        }
    }
    /// A bar between two points (used by the ellipse segments and the chevron band).
    fn seg(a: egui::Pos2, b: egui::Pos2, w: f32, color: egui::Color32) -> Draw {
        Draw {
            center: egui::pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5),
            size: egui::vec2(((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt(), w),
            rot: (b.y - a.y).atan2(b.x - a.x),
            color,
        }
    }
}

fn alpha(c: egui::Color32, f: f32) -> egui::Color32 {
    t::with_alpha(c, ((c.a() as f32) * f.clamp(0.0, 1.0)) as u8)
}

fn zone_share(run: Option<&EngineOutput>, zone: ZoneId, layer: Option<usize>) -> Option<f64> {
    run?.pressure_by_zone
        .iter()
        .find(|z| z.zone == zone && z.layer == layer)
        .map(|z| z.share_pct)
}

fn zone_tint(base: egui::Color32, share: Option<f64>) -> egui::Color32 {
    let a = match share {
        Some(s) => 0.10 + 0.52 * (s / 100.0) as f32,
        None => 0.10,
    };
    t::with_alpha(base, (a * 255.0) as u8)
}

/// Points on an ellipse inscribed in `r` (egui screen space, y down).
fn ellipse_pts(r: egui::Rect, n: u32) -> Vec<egui::Pos2> {
    (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            egui::pos2(
                r.center().x + r.width() * 0.5 * a.cos(),
                r.center().y + r.height() * 0.5 * a.sin(),
            )
        })
        .collect()
}

/// The whole frame's look, as a pure function of data.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    l: &Layout,
    input: &EngineInput,
    run: Option<&EngineOutput>,
    anchor: Option<&EngineOutput>,
    vis: &Visual,
    cat: Option<&Catalog>,
    anim_t: f32,
) -> HashMap<Part, Draw> {
    let mut d: HashMap<Part, Draw> = HashMap::new();
    let phone = l.phone;
    let flow = run
        .map(|o| o.airflow_m3_s)
        .unwrap_or(m::ANCHOR_AIRFLOW_M3_S);
    let flow_f = m::flow_factor(flow);
    let rpm = m::rpm(input.speed_ratio, input.fan.nominal_rpm).unwrap_or(0.0);
    let _flow_soft = 0.30 + 0.70 * flow_f;

    let _f_flow = if matches!(vis.focus, Focus::All | Focus::Airflow) {
        1.0
    } else {
        0.24
    };
    let f_press = if matches!(vis.focus, Focus::All | Focus::Pressure) {
        1.0
    } else {
        0.30
    };
    let f_nozzle = if matches!(vis.focus, Focus::All | Focus::Nozzle) {
        1.0
    } else {
        0.26
    };
    let f_fan = if matches!(vis.focus, Focus::All | Focus::Fan) {
        1.0
    } else {
        0.45
    };

    // ---- ground + calibration grid
    d.insert(Part::Ground, Draw::rect(l.area, t::RAIL_BG));
    if vis.grid {
        for i in 0..GRID_H {
            let y = l.area.top() + l.area.height() * (i as f32 + 1.0) / (GRID_H as f32 + 1.0);
            d.insert(
                Part::GridH(i),
                Draw::at(
                    egui::pos2(l.area.center().x, y),
                    egui::vec2(l.area.width(), 1.0),
                    t::GRID,
                ),
            );
        }
        for i in 0..GRID_V {
            let x = l.area.left() + l.area.width() * (i as f32 + 1.0) / (GRID_V as f32 + 1.0);
            d.insert(
                Part::GridV(i),
                Draw::at(
                    egui::pos2(x, l.area.center().y),
                    egui::vec2(1.0, l.area.height()),
                    t::GRID,
                ),
            );
        }
    }

    // ---- the casing: walls around the machine, open at the top where the stack leaves it
    d.insert(Part::Casing, Draw::rect(l.tower, t::PANEL));
    let tr = l.tower;
    let th = 1.4;
    for (i, r) in [
        egui::Rect::from_min_max(
            egui::pos2(tr.left(), l.plinth.top()),
            egui::pos2(tr.right(), l.plinth.bottom()),
        ),
        egui::Rect::from_min_max(tr.min, egui::pos2(tr.left() + th, tr.bottom())),
        egui::Rect::from_min_max(egui::pos2(tr.right() - th, l.plinth.top()), tr.max),
        egui::Rect::from_min_max(egui::pos2(tr.left(), l.plinth.bottom() - th), tr.max),
    ]
    .into_iter()
    .enumerate()
    {
        d.insert(Part::CasingEdge(i as u8), Draw::rect(r, t::LINE));
    }

    // ---- zone-tinted structure, driven by the engine's per-zone pressure shares
    let inlet_share = zone_share(run, ZoneId::Inlet, None);
    let _stack_share = zone_share(run, ZoneId::Stack, None);
    let _plenum_share = zone_share(run, ZoneId::Plenum, None);
    let drift_share = zone_share(run, ZoneId::Drift, None);
    let spray_share = zone_share(run, ZoneId::Spray, None);
    let rain_share = zone_share(run, ZoneId::Rain, None);

    // ---- the fan stack: a tapered cylinder standing on the deck, with the wheel inside its mouth
    let speed_fraction = m::fraction_of(input.speed_ratio, 0.0, 1.5) as f32;
    let cyl = l.stack_cyl;
    d.insert(
        Part::StackMouth,
        Draw::rect(cyl, t::with_alpha(t::INK_2, 14)),
    );
    // The walls taper from the deck width to the mouth width (a velocity-recovery flare).
    let wall_a = (STACK_FLARE - 1.0) * 0.5;
    let left_top = egui::pos2(cyl.left(), cyl.top());
    let left_bot = egui::pos2(cyl.left() + cyl.width() * wall_a, cyl.bottom());
    let right_top = egui::pos2(cyl.right(), cyl.top());
    let right_bot = egui::pos2(cyl.right() - cyl.width() * wall_a, cyl.bottom());
    let wall_w = if phone { 1.6 } else { 2.2 };
    d.insert(
        Part::StackWall(0),
        Draw::seg(left_top, left_bot, wall_w, t::INK_2),
    );
    d.insert(
        Part::StackWall(1),
        Draw::seg(right_top, right_bot, wall_w, t::INK_2),
    );
    // The mouth ellipse and the base ellipse (the cylinder's two ends).
    let rim = ellipse_pts(l.stack_rim, RIM as u32);
    let base = ellipse_pts(
        egui::Rect::from_center_size(
            egui::pos2(cyl.center().x, cyl.bottom()),
            egui::vec2(
                cyl.width() - cyl.width() * wall_a,
                l.stack_rim.height() * 0.8,
            ),
        ),
        RIM as u32,
    );
    for i in 0..RIM as usize {
        d.insert(
            Part::StackRim(i as u8),
            Draw::seg(
                rim[i],
                rim[i + 1],
                1.3,
                alpha(t::LINE, if f_fan > 0.5 { 1.0 } else { 0.55 }),
            ),
        );
        d.insert(
            Part::StackRimBase(i as u8),
            Draw::seg(
                base[i],
                base[i + 1],
                1.1,
                t::with_alpha(t::LINE_SOFT, if f_fan > 0.5 { 230 } else { 140 }),
            ),
        );
    }
    // The wheel, seen slightly from above through the mouth. The renderer draws rotated rectangles, so an
    // ellipse is built two ways: a **fill** as vertical slices of varying height (the disc, the hub), and an
    // **outline** as short tangential segments (the rim). Rim, spokes, blades and hub all belong to the one
    // ellipse, so the blades radiate from the hub and stay inside the barrel (issue #91; round 5's blades
    // sat on the rim ellipse as tangential bars and read as a scatter).
    let rim_r = l.stack_rim;
    let flat = 0.24_f32;
    let fan_el = egui::Rect::from_center_size(
        egui::pos2(rim_r.center().x, rim_r.center().y + cyl.height() * 0.34),
        egui::vec2(rim_r.width() * 0.94, rim_r.width() * 0.94 * flat),
    );
    let hub_r = (fan_el.width() * 0.13).max(4.0);
    for i in 0..DISC {
        let t01 = (i as f32 + 0.5) / DISC as f32;
        let x = fan_el.left() + fan_el.width() * t01;
        let dx = (x - fan_el.center().x) / (fan_el.width() * 0.5);
        let hh = fan_el.height() * 0.5 * (1.0 - dx * dx).max(0.0).sqrt();
        d.insert(
            Part::DiscSlice(i as u8),
            Draw::at(
                egui::pos2(x, fan_el.center().y),
                egui::vec2(fan_el.width() / DISC as f32 + 0.8, hh * 2.0),
                t::with_alpha(t::PRIMARY, (7.0 + 20.0 * speed_fraction) as u8),
            ),
        );
    }
    for i in 0..RING {
        let aa = i as f32 / RING as f32 * std::f32::consts::TAU;
        let aa2 = (i as f32 + 0.9) / RING as f32 * std::f32::consts::TAU;
        let (s1, c1) = aa.sin_cos();
        let (s2, c2) = aa2.sin_cos();
        d.insert(
            Part::FanRing(i as u8),
            Draw::seg(
                egui::pos2(
                    fan_el.center().x + fan_el.width() * 0.5 * c1,
                    fan_el.center().y + fan_el.height() * 0.5 * s1,
                ),
                egui::pos2(
                    fan_el.center().x + fan_el.width() * 0.5 * c2,
                    fan_el.center().y + fan_el.height() * 0.5 * s2,
                ),
                if phone { 1.3 } else { 1.6 },
                t::with_alpha(t::PRIMARY, (90.0 + 120.0 * speed_fraction) as u8),
            ),
        );
    }
    for i in 0..8 {
        let aa = i as f32 * std::f32::consts::TAU / 8.0;
        let or = fan_el.width() * 0.47;
        d.insert(
            Part::FanSpoke(i),
            Draw::seg(
                egui::pos2(
                    fan_el.center().x + hub_r * aa.cos(),
                    fan_el.center().y + hub_r * flat * 2.0 * aa.sin(),
                ),
                egui::pos2(
                    fan_el.center().x + or * aa.cos(),
                    fan_el.center().y + or * flat * 2.0 * aa.sin(),
                ),
                1.0,
                t::with_alpha(t::LINE, 110),
            ),
        );
    }
    let spin = anim_t * std::f32::consts::TAU * m::blade_turn_hz(rpm);
    let blade_n = 6;
    for i in 0..blade_n {
        let aa = spin + i as f32 * std::f32::consts::TAU / blade_n as f32;
        let (s1, c1) = aa.sin_cos();
        let outer = fan_el.width() * 0.47;
        let pt = |r: f32| {
            egui::pos2(
                fan_el.center().x + r * c1,
                fan_el.center().y + r * flat * 2.0 * s1,
            )
        };
        d.insert(
            Part::FanBlade(i as u8),
            Draw::seg(
                pt(hub_r * 0.9),
                pt(outer),
                if phone { 3.2 } else { 4.6 },
                t::with_alpha(t::INK_2, (170.0 + 85.0 * speed_fraction) as u8),
            ),
        );
        // the leading edge: the blade's forward side, brighter, so the wheel's spin reads even frozen
        let la = aa + 0.22;
        let (s2, c2) = la.sin_cos();
        let pt2 = |r: f32| {
            egui::pos2(
                fan_el.center().x + r * c2,
                fan_el.center().y + r * flat * 2.0 * s2,
            )
        };
        d.insert(
            Part::FanBladeTip(i as u8),
            Draw::seg(
                pt2(hub_r * 1.1),
                pt2(outer),
                1.0,
                t::with_alpha(t::INK, 200),
            ),
        );
    }
    // the halo: the column of air the wheel pulls, as a soft ellipse just outside the disc (slices, like
    // the disc itself, so it is an ellipse and not a box), brightening with rpm
    let halo = fan_el.expand2(egui::vec2(fan_el.width() * 0.07, fan_el.height() * 0.07));
    for i in 0..FAN_GLOW {
        let t01 = (i as f32 + 0.5) / FAN_GLOW as f32;
        let x = halo.left() + halo.width() * t01;
        let dx = (x - halo.center().x) / (halo.width() * 0.5);
        let hh = halo.height() * 0.5 * (1.0 - dx * dx).max(0.0).sqrt();
        d.insert(
            Part::FanGlow(i as u8),
            Draw::at(
                egui::pos2(x, halo.center().y),
                egui::vec2(halo.width() / FAN_GLOW as f32 + 0.8, hh * 2.0),
                t::with_alpha(t::PRIMARY, (4.0 + 16.0 * speed_fraction) as u8),
            ),
        );
    }
    for i in 0..HUBSLICE {
        let t01 = (i as f32 + 0.5) / HUBSLICE as f32;
        let x = fan_el.center().x - hub_r + 2.0 * hub_r * t01;
        let dx = (x - fan_el.center().x) / hub_r.max(0.01);
        let hh = hub_r * flat * (1.0 - dx * dx).max(0.0).sqrt();
        d.insert(
            Part::FanHubCap(i as u8),
            Draw::at(
                egui::pos2(x, fan_el.center().y),
                egui::vec2(2.0 * hub_r / HUBSLICE as f32 + 0.8, hh * 2.0),
                t::PRIMARY,
            ),
        );
    }
    d.insert(
        Part::FanHub,
        Draw::at(
            fan_el.center(),
            egui::vec2(hub_r * 0.7, hub_r * 0.7 * flat * 2.0),
            t::with_alpha(t::BG, 200),
        ),
    );

    // ---- the fan deck: a plate with ribs, under the stack
    d.insert(
        Part::DeckPlate,
        Draw::rect(l.deck, t::with_alpha(t::INK_2, 30)),
    );
    // ribs on the visible deck only - the deck inside the stack's base flare is hidden by the barrel
    let base_w = l.stack_cyl.width() * (1.0 - (STACK_FLARE - 1.0) * 0.5);
    let mut rib = 0;
    for x in [
        l.deck.left() + l.deck.width() * 0.10,
        l.deck.right() - l.deck.width() * 0.10,
    ] {
        if (x - l.deck.center().x).abs() > base_w * 0.5 + 6.0 {
            d.insert(
                Part::DeckRib(rib),
                Draw::at(
                    egui::pos2(x, l.deck.center().y),
                    egui::vec2(4.0, l.deck.height() * 0.8),
                    t::with_alpha(t::LINE, 200),
                ),
            );
            rib += 1;
        }
    }

    // ---- the drift-eliminator bank: a chevron band across the casing
    d.insert(
        Part::DriftBox,
        Draw::rect(l.drift, zone_tint(t::OK, drift_share)),
    );
    let chev_n = if phone {
        DRIFT_CHEV as usize / 2
    } else {
        DRIFT_CHEV as usize
    };
    let chev_h = l.drift.height();
    let chev_seg_w = l.drift.width() / chev_n as f32;
    for i in 0..chev_n {
        let x0 = l.drift.left() + chev_seg_w * i as f32;
        let up = i % 2 == 0;
        let a = egui::pos2(
            x0,
            if up {
                l.drift.bottom() - chev_h * 0.12
            } else {
                l.drift.top() + chev_h * 0.12
            },
        );
        let b = egui::pos2(
            x0 + chev_seg_w,
            if up {
                l.drift.top() + chev_h * 0.12
            } else {
                l.drift.bottom() - chev_h * 0.12
            },
        );
        let c = t::with_alpha(t::OK, (120.0 + 90.0 * f_press) as u8);
        d.insert(
            Part::DriftChev(i as u8),
            Draw::seg(a, b, if phone { 1.6 } else { 1.8 }, c),
        );
    }
    let haze = 0.25 + 0.75 * (drift_share.unwrap_or(8.0) / 100.0) as f32;
    for i in 0..3 {
        let y = l.drift.top() + l.drift.height() * (i as f32 + 1.0) / 4.0;
        d.insert(
            Part::DriftHaze(i),
            Draw::at(
                egui::pos2(l.drift.center().x, y),
                egui::vec2(l.drift.width() * 0.92, 2.0),
                t::with_alpha(t::OK, (haze * 150.0 * f_press) as u8),
            ),
        );
    }

    // ---- the distribution header and its nozzle heads
    d.insert(
        Part::SprayBox,
        Draw::rect(l.spray, zone_tint(t::PRIMARY, spray_share)),
    );
    let _pitch_m = m::effective_pitch(
        vis.nozzle_spacing_m,
        vis.nozzle_pattern == Pattern::Staggered,
    );
    // The water enters the tower hottest, so the cones are drawn in the run's own warm end: the ramp is the
    // same one the falling water walks (`mapping::water_ramp`, seam `water.temperature`).
    let warm = t::water_tint(1.0);
    let half_angle = m::spray_half_angle_deg(input.nozzle.orifice_diameter_m);
    let cone_h_m = input.tower.spray_zone_height_m;
    let radius_m = m::spray_cone_radius_m(cone_h_m, half_angle);
    let true_count = m::nozzle_count(vis.nozzle_spacing_m, l.face_width_m as f64);
    let draw_count = true_count.min(NOZZLES as usize).min(MAX_NOZZLES_DRAWN);
    let header_r = egui::Rect::from_min_max(
        egui::pos2(l.spray.left() + 4.0, l.spray.top() + 3.0),
        egui::pos2(l.spray.right() - 4.0, l.spray.top() + 6.0),
    );
    d.insert(
        Part::Header,
        Draw::rect(header_r, alpha(t::PRIMARY, f_nozzle)),
    );
    let apex_y = header_r.bottom() + 2.0;
    // The water lands on the top fill layer, so that is where the cone closes.
    let land_y = l
        .layers
        .first()
        .map(|r| r.top())
        .unwrap_or(l.fill_band.top());
    let base_y = land_y.max(apex_y + 10.0);
    let radius_px = (radius_m as f32 * l.px_per_m_x).max(3.0);
    let cone_h = (base_y - apex_y).max(4.0);
    for i in 0..NOZZLES {
        let idx = i as usize;
        if idx >= draw_count {
            continue;
        }
        let frac = (idx as f32 + 0.5) / draw_count as f32;
        let x = header_r.left() + header_r.width() * frac;
        // The head: a short stub hanging off the header, so a nozzle is a thing, not a dot.
        d.insert(
            Part::HeaderStub(i),
            Draw::at(
                egui::pos2(x, header_r.bottom() + 1.5),
                egui::vec2(if phone { 2.0 } else { 2.6 }, 3.0),
                alpha(t::PRIMARY, f_nozzle),
            ),
        );
        let rows: [(f32, f32); 2] = match vis.nozzle_pattern {
            Pattern::SingleRow => [(0.0, 1.0), (0.0, 0.0)],
            Pattern::Staggered => [(0.0, 0.62), (l.spray.height() * 0.30, 0.38)],
        };
        for (row, (dy, shrink)) in rows.into_iter().enumerate() {
            if shrink <= 0.0 {
                if vis.nozzle_pattern == Pattern::Staggered {
                    continue;
                }
                let x2 = x + header_r.width() / (draw_count.max(1) as f32) * 0.5;
                if x2 > header_r.right() {
                    continue;
                }
                draw_cone(
                    &mut d,
                    i,
                    row as u8,
                    x2,
                    apex_y,
                    base_y,
                    radius_px * 0.62,
                    f_nozzle,
                    warm,
                );
                continue;
            }
            let tip_y = apex_y + dy;
            let base = if row == 0 {
                base_y
            } else {
                base_y + cone_h * 0.35
            };
            draw_cone(
                &mut d,
                i,
                row as u8,
                x,
                tip_y,
                base,
                radius_px * shrink.max(0.55),
                f_nozzle,
                warm,
            );
        }
        d.insert(
            Part::NozzleTip(i),
            Draw::at(
                egui::pos2(x, header_r.center().y),
                egui::vec2(if phone { 4.0 } else { 5.0 }, if phone { 4.0 } else { 5.0 }),
                t::with_alpha(t::PRIMARY, ((f_nozzle * 255.0) as u8).max(60)),
            ),
        );
    }

    // ---- the two fill layers, with a texture that follows the fixture's own `geometry` string
    d.insert(
        Part::FillBack,
        Draw::rect(l.fill_band, t::with_alpha(t::LINE_SOFT, 120)),
    );
    for (i, r) in l.layers.iter().enumerate() {
        if i >= MAX_LAYERS {
            break;
        }
        let fill_id = input
            .fill_layers
            .get(i)
            .map(|x| x.fill_id.as_str())
            .unwrap_or("?");
        let selected = vis.selected_slot == Slot::Fill && vis.selected_layer == i;
        let base = t::fill_color(fill_id);
        d.insert(
            Part::Layer(i as u8),
            Draw::rect(*r, t::with_alpha(base, if selected { 210 } else { 168 })),
        );
        // Texture: film = fine vertical channels; wide-flute = coarser horizontal rows; grid/splash differ
        // again. Counts scale with the viewport so a phone stays legible instead of turning into noise.
        let kind = fill_texture(cat, fill_id);
        let detail = if phone {
            FILL_DETAIL as usize / 2
        } else {
            FILL_DETAIL as usize
        };
        match kind {
            FillTexture::Film => {
                let n = detail.min((r.width() / 4.0).max(4.0) as usize);
                for k in 0..n {
                    let x = r.left() + r.width() * (k as f32 + 0.5) / n as f32;
                    d.insert(
                        Part::FillDetail(i as u8, k as u8),
                        Draw::at(
                            egui::pos2(x, r.center().y),
                            egui::vec2(1.1, r.height() - 1.0),
                            t::with_alpha(t::BG, 90),
                        ),
                    );
                }
            }
            FillTexture::WideFlute => {
                let n = (r.height() / 5.0).max(2.0).min(detail as f32) as usize;
                for k in 0..n {
                    let y = r.top() + r.height() * (k as f32 + 0.5) / n as f32;
                    d.insert(
                        Part::FillDetail(i as u8, k as u8),
                        Draw::at(
                            egui::pos2(r.center().x, y),
                            egui::vec2(r.width() - 2.0, 1.6),
                            t::with_alpha(t::BG, 95),
                        ),
                    );
                }
                // plus a few vertical hangers, so the wide-flute sheet still reads as fill
                for k in 0..4usize {
                    let x = r.left() + r.width() * (k as f32 + 0.5) / 4.0;
                    d.insert(
                        Part::FillDetail(i as u8, (n + k).min(detail - 1) as u8),
                        Draw::at(
                            egui::pos2(x, r.center().y),
                            egui::vec2(1.0, r.height() - 3.0),
                            t::with_alpha(t::BG, 70),
                        ),
                    );
                }
            }
            FillTexture::Grid => {
                let nx = (r.width() / 7.0).max(3.0).min(detail as f32) as usize;
                for k in 0..nx {
                    let x = r.left() + r.width() * (k as f32 + 0.5) / nx as f32;
                    d.insert(
                        Part::FillDetail(i as u8, k as u8),
                        Draw::at(
                            egui::pos2(x, r.center().y),
                            egui::vec2(1.0, r.height() - 2.0),
                            t::with_alpha(t::BG, 90),
                        ),
                    );
                }
            }
            FillTexture::Splash | FillTexture::Unknown => {
                let n = (r.height() / 6.0).max(2.0).min(detail as f32) as usize;
                for k in 0..n {
                    let y = r.top() + r.height() * (k as f32 + 0.5) / n as f32;
                    let dx = if k % 2 == 0 { 0.0 } else { 6.0 };
                    d.insert(
                        Part::FillDetail(i as u8, k as u8),
                        Draw::at(
                            egui::pos2(r.center().x + dx, y),
                            egui::vec2(r.width() - 10.0, 2.2),
                            t::with_alpha(t::BG, 80),
                        ),
                    );
                }
            }
        }
        if selected {
            d.insert(
                Part::LayerTick(i as u8),
                Draw::rect(
                    egui::Rect::from_min_max(
                        egui::pos2(r.left() - 2.0, r.top()),
                        egui::pos2(r.left() + 2.0, r.bottom()),
                    ),
                    t::PRIMARY,
                ),
            );
        }
    }
    // Coverage bands are drawn on the layer the water lands on first (index 0 = top).
    if let Some(top) = l.layers.first() {
        let w = (2.0 * radius_px).min(top.width());
        for (i, r) in l.layers.iter().enumerate() {
            if i >= MAX_LAYERS {
                break;
            }
            let a = if i == 0 || vis.selected_slot != Slot::Fill {
                ((f_nozzle * 150.0) as u8).max(40)
            } else {
                ((f_nozzle * 90.0) as u8).max(20)
            };
            d.insert(
                Part::Coverage(i as u8),
                Draw::at(
                    egui::pos2(r.center().x, r.top() + 2.0),
                    egui::vec2(w, 3.0),
                    t::with_alpha(t::PRIMARY, a),
                ),
            );
        }
    }

    // ---- the rain zone: open space on support columns, onto the basin
    d.insert(
        Part::RainBox,
        Draw::rect(l.rain, zone_tint(t::WATER, rain_share)),
    );
    let cols = if phone { 4 } else { COLUMNS as usize };
    for i in 0..cols {
        let x = l.rain.left() + l.rain.width() * (i as f32 + 1.0) / (cols as f32 + 1.0);
        d.insert(
            Part::Column(i as u8),
            Draw::at(
                egui::pos2(x, l.rain.center().y),
                egui::vec2(if phone { 2.0 } else { 3.0 }, l.rain.height()),
                t::with_alpha(t::MUTED, 190),
            ),
        );
    }

    // ---- the cold-water basin, with a water surface
    d.insert(
        Part::BasinBox,
        Draw::rect(l.basin, t::with_alpha(t::LINE_SOFT, 150)),
    );
    d.insert(
        Part::WaterBody,
        Draw::rect(
            egui::Rect::from_min_max(
                egui::pos2(l.basin.left() + 2.0, l.water_surface_y),
                egui::pos2(l.basin.right() - 2.0, l.basin.bottom() - 2.0),
            ),
            t::with_alpha(t::WATER, 72),
        ),
    );
    d.insert(
        Part::WaterSurface,
        Draw::at(
            egui::pos2(l.basin.center().x, l.water_surface_y),
            egui::vec2(l.basin.width() - 4.0, 1.6),
            t::with_alpha(t::WATER, 210),
        ),
    );

    // ---- louvred air inlets on BOTH sides
    d.insert(
        Part::InletBox(0),
        Draw::rect(l.inlet_l, zone_tint(t::AIR, inlet_share)),
    );
    d.insert(
        Part::InletBox(1),
        Draw::rect(l.inlet_r, zone_tint(t::AIR, inlet_share)),
    );
    let slats = if phone { LOUVRES / 2 } else { LOUVRES } as usize;
    for (side, r) in [l.inlet_l, l.inlet_r].into_iter().enumerate() {
        for i in 0..slats {
            let y0 = r.top() + r.height() * i as f32 / slats as f32;
            let y1 = r.top() + r.height() * (i as f32 + 1.0) / slats as f32;
            // Slats tilt down toward the inside, which is what makes a louvre read as a louvre.
            let a = egui::pos2(r.left(), y0 + 1.0);
            let b = egui::pos2(r.right(), y1 - 1.0);
            d.insert(
                Part::Louvre(side as u8, i as u8),
                Draw::seg(
                    a,
                    b,
                    if phone { 1.4 } else { 1.8 },
                    t::with_alpha(t::AIR, 150),
                ),
            );
        }
    }

    // ---- operating-point rail: the engine's own airflow, and the anchor run for comparison
    if f_fan > 0.5 {
        d.insert(
            Part::OpRailBar,
            Draw::rect(l.op_rail, t::with_alpha(t::LINE_SOFT, 200)),
        );
        let max_flow = run
            .map(|o| {
                o.fan_system_curve
                    .fan
                    .points
                    .iter()
                    .map(|p| p.x)
                    .fold(0.0_f64, f64::max)
            })
            .filter(|v| *v > 1.0)
            .unwrap_or(260.0);
        for i in 0..5 {
            let x = l.op_rail.left() + l.op_rail.width() * i as f32 / 4.0;
            d.insert(
                Part::OpTick(i),
                Draw::at(
                    egui::pos2(x, l.op_rail.center().y),
                    egui::vec2(1.0, 7.0),
                    t::with_alpha(t::TICK, 200),
                ),
            );
        }
        let cur = m::fraction_of(flow, 0.0, max_flow);
        let x = l.op_rail.left() + l.op_rail.width() * cur;
        d.insert(
            Part::OpCaretCurrent,
            Draw::at(
                egui::pos2(x, l.op_rail.center().y),
                egui::vec2(8.0, 3.0),
                t::PRIMARY,
            ),
        );
        if let Some(a) = anchor {
            let ax = l.op_rail.left()
                + l.op_rail.width() * m::fraction_of(a.airflow_m3_s, 0.0, max_flow);
            d.insert(
                Part::OpCaretAnchor,
                Draw::at(
                    egui::pos2(ax, l.op_rail.center().y),
                    egui::vec2(5.0, 1.6),
                    t::with_alpha(t::PRIMARY, 110),
                ),
            );
        }
    }

    // ---- the pressure rail: one segment per zone, in air-path order, bottom-up
    d.insert(Part::RailBg, Draw::rect(l.rail, t::RAIL_BG));
    let zones: Vec<(egui::Color32, f64)> = match run {
        Some(o) => o
            .pressure_by_zone
            .iter()
            .map(|z| {
                let fill = z
                    .layer
                    .and_then(|li| o.kavl_per_layer.get(li))
                    .map(|lr| lr.fill_id.as_str());
                let base = match z.zone {
                    ZoneId::Inlet => t::AIR,
                    ZoneId::Rain => t::WATER,
                    ZoneId::Fill => fill.map(t::fill_color).unwrap_or(t::PRIMARY),
                    ZoneId::Spray => t::PRIMARY,
                    ZoneId::Drift => t::OK,
                    ZoneId::Plenum => t::MUTED,
                    ZoneId::Stack => t::INK_2,
                    ZoneId::Fixed => t::LINE,
                };
                (base, z.share_pct)
            })
            .collect(),
        None => vec![(t::MUTED, 100.0 / 7.0); 7],
    };
    let total_share: f64 = zones.iter().map(|(_, s)| s.max(0.6)).sum();
    let mut bottom = l.rail.bottom();
    for (i, (color, share)) in zones.iter().enumerate().take(RAIL_SEGS as usize) {
        let h = (l.rail.height() as f64 * share.max(0.6) / total_share) as f32;
        let r = egui::Rect::from_min_max(
            egui::pos2(l.rail.left(), bottom - h),
            egui::pos2(l.rail.right(), bottom - 1.0),
        );
        d.insert(
            Part::RailSeg(i as u8),
            Draw::rect(r, t::with_alpha(*color, (95.0 + 150.0 * f_press) as u8)),
        );
        bottom -= h;
    }

    // ---- the four bays: a plate always, a wash when a drag is over them, an outline when focused
    for (i, (slot, r)) in l.slots.iter().enumerate() {
        d.insert(
            Part::SlotPlate(i as u8),
            Draw::rect(*r, t::with_alpha(t::PANEL_RAISED, 90)),
        );
        let dragged = vis
            .drag
            .as_ref()
            .and_then(|dg| dg.over)
            .map(|s| s == *slot)
            .unwrap_or(false);
        if dragged {
            let (color, verdict) = match vis.drag.as_ref().map(|dg| &dg.verdict) {
                Some(Ok(())) => (t::VALID, 1.0),
                Some(Err(_)) => (t::INVALID, 1.0),
                None => (t::PRIMARY, 0.6),
            };
            d.insert(
                Part::SlotWash(i as u8),
                Draw::rect(*r, t::with_alpha(color, (52.0 * verdict) as u8)),
            );
        }
        // The picker's bay, and the keyboard's bay, get a wash too: the affordance has to be visible in a
        // still frame, not only under a pointer.
        let picker_here = vis
            .picker
            .as_ref()
            .map(|p| p.slot == *slot)
            .unwrap_or(false);
        let focus_here = vis.bay_focus == Some(*slot);
        if picker_here || focus_here {
            let color = if picker_here { t::PRIMARY } else { t::AIR };
            d.insert(
                Part::SlotFocus(i as u8),
                Draw::rect(*r, t::with_alpha(color, if picker_here { 34 } else { 20 })),
            );
        }
    }

    d
}

/// A cone: two edges from the tip to the base, plus bars that fade with depth.
#[allow(clippy::too_many_arguments)]
fn draw_cone(
    d: &mut HashMap<Part, Draw>,
    nozzle: u8,
    row: u8,
    x: f32,
    tip_y: f32,
    base_y: f32,
    radius: f32,
    f: f32,
    warm: egui::Color32,
) {
    let tip = egui::pos2(x, tip_y);
    let half = radius;
    let h = (base_y - tip_y).max(1.0);
    for j in 0..CONE_BARS {
        let t0 = j as f32 / CONE_BARS as f32;
        let t1 = (j as f32 + 1.0) / CONE_BARS as f32;
        let y = tip_y + h * (t0 + t1) * 0.5;
        let bar_h = h * (t1 - t0) + 1.0;
        let w = (half * 2.0 * t1).max(2.5);
        let a = ((26.0 + 20.0 * t1) * f) as u8;
        d.insert(
            Part::ConeBar(nozzle, j),
            Draw::at(
                egui::pos2(x, y),
                egui::vec2(w, bar_h),
                // The spray is where the water enters the tower: the cones carry the hot end of the run's
                // own temperature ramp (the walk itself is `water.temperature` in the seams table).
                t::with_alpha(warm, a.max(14)),
            ),
        );
    }
    for (j, sx) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let base = egui::pos2(x + sx * half, base_y);
        let mid = egui::pos2((tip.x + base.x) * 0.5, (tip.y + base.y) * 0.5);
        let len = ((base.x - tip.x).powi(2) + (base.y - tip.y).powi(2)).sqrt();
        let ang = (base.y - tip.y).atan2(base.x - tip.x);
        d.insert(
            Part::ConeEdge(nozzle, j as u8),
            Draw::bar(
                mid,
                len,
                1.3,
                ang,
                t::with_alpha(t::PRIMARY, ((190.0 * f) as u8).max(70)),
            ),
        );
    }
    let _ = row;
}

// --------------------------------------------------------------------------------------- driver

#[allow(clippy::too_many_arguments)]
fn sync_scene(
    clock: Res<AnimClock>,
    scene: Res<SceneRect>,
    draft: Option<Res<Draft>>,
    run: Res<Run>,
    anchor: Res<Anchor>,
    vis: Res<Visual>,
    cat: Option<Res<Catalog>>,
    pool: Res<Pool>,
    mut q: Query<(&Viz, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let screen = scene.window;
    let degenerate = screen.x < 8.0 || screen.y < 8.0 || scene.max.x - scene.min.x < 8.0;
    // The 3D view owns the picture in its own tab: the sprites stay hidden there.
    // Round 4, item 1: the sprites hide for the 3D view; without the `three-d` feature that view does not
    // exist, so nothing hides them.
    #[cfg(feature = "three-d")]
    let hidden = vis.view == crate::state::View::Three;
    #[cfg(not(feature = "three-d"))]
    let hidden = false;
    let Some(draft) = draft.filter(|_| !degenerate && !pool.0.is_empty() && !hidden) else {
        for (_, _, _, mut v) in &mut q {
            *v = Visibility::Hidden;
        }
        return;
    };

    FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let l = layout(scene.rect(), Some(&draft.0), scene.phone);
    let plan = plan(
        &l,
        &draft.0,
        run.output.as_ref(),
        anchor.output.as_ref(),
        &vis,
        cat.as_deref(),
        clock.t,
    );

    for (viz, mut sprite, mut transform, mut visibility) in &mut q {
        match plan.get(&viz.0) {
            Some(draw) => {
                let c = draw.center;
                transform.translation =
                    Vec3::new(c.x - screen.x * 0.5, screen.y * 0.5 - c.y, viz.0.z());
                transform.scale = Vec2::new(draw.size.x.max(0.6), draw.size.y.max(0.6)).extend(1.0);
                // egui's y axis points down, the world's up: the rotation flips with it.
                transform.rotation = Quat::from_rotation_z(-draw.rot);
                sprite.color = Color::srgba_u8(
                    draw.color.r(),
                    draw.color.g(),
                    draw.color.b(),
                    draw.color.a(),
                );
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
}

// --------------------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::fixture_engine::FixtureEngine;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    /// The four sizes issue #81 names, as (label, the section rect's own size, phone). The sizes are the
    /// ones the shell hands the section in the committed frames (`docs/design/small-screens-r1/`, mirrored
    /// by `data-scene-rect`/`data-scene-frac`): the Instrument's centre rect, less the header and the
    /// region's own margins. Only the size matters to the scale (`layout` reads `area.width()`/`height()`),
    /// so the rects are built at the origin.
    const TARGETS: [(&str, f32, f32, bool); 4] = [
        ("1280x720", 1152.0, 610.0, false),
        ("1440x900", 1312.0, 790.0, false),
        ("1024x768", 896.0, 358.0, false),
        ("390x844", 374.0, 234.0, true),
    ];

    fn engine() -> FixtureEngine {
        FixtureEngine::from_json(FIXTURE).unwrap()
    }

    fn section(w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h))
    }

    /// Issue #81, criterion 1: **the section has one metres-to-points scale on both axes.** Asserted on the
    /// layout struct, at the four sizes: the horizontal scale (the face's own px/m), the vertical scale (the
    /// bands' px/m) and `px_per_m` agree within 5 %, and so does the drawn tower's own width:height ratio.
    #[test]
    fn one_scale_for_both_axes_at_the_four_target_sizes() {
        let e = engine();
        let input = e.default_input();
        for (label, w, h, phone) in TARGETS {
            let l = layout(section(w, h), Some(&input), phone);
            let x = l.px_per_m_x;
            let y = 1.0 / l.m_per_px_y;
            let drawn_y = (l.basin.bottom() - l.stack.top()) / l.height_m;
            let drawn_x = l.tower.width() / l.face_width_m;
            for (what, a, b) in [
                ("px_per_m_x vs px_per_m", x, l.px_per_m),
                ("px_per_m_x vs 1/m_per_px_y", x, y),
                ("drawn width vs drawn height", drawn_x, drawn_y),
            ] {
                let rel = (a - b).abs() / b.abs().max(1e-6);
                assert!(
                    rel <= 0.05,
                    "{label}: {what} disagree by {:.1}% ({a} vs {b})",
                    rel * 100.0
                );
            }
        }
    }

    /// Issue #81, criterion 2: **the section is never laid out shorter than the agreed minimum.** The card
    /// above the tower takes at most `card_max_h`, so what is left for the section is the minimum or more -
    /// whatever the card's own content height is (its resting rows, the duty row open, a source line open,
    /// or a content so tall it has to scroll). Proved over a sweep of centre heights, not just the four
    /// sizes, because the promise is about every window.
    #[test]
    fn the_section_keeps_its_agreed_minimum() {
        for h in (MIN_SECTION_H + CARD_GAP + 120.0) as i32..=1400 {
            let h = h as f32;
            for card_h in [0.0_f32, 120.0, 296.0, 335.0, 385.0, 900.0] {
                let used = card_h.min(card_max_h(h));
                let below = h - used - CARD_GAP;
                assert!(
                    below >= MIN_SECTION_H,
                    "a {h:.0} px centre with a {card_h} px card leaves the section {below:.0} px, \
                     under the minimum {MIN_SECTION_H}"
                );
            }
        }
        // ... and the four sizes the issue names are all above that floor: the section rect heights the
        // shell hands over are the frames' own `data-scene-h`.
        for (label, _, h, _) in TARGETS {
            assert!(h >= MIN_SECTION_H, "{label}: the section is {h:.0} px");
        }
    }

    /// Issue #81, criterion 2: the fold rule the shell and the section share. A phone always puts the card
    /// above the tower; a desktop does it when its centre cannot hold the card's own slot beside the tower -
    /// which is what a 1024x768 window does, and what used to drop the card (and the duty inputs and the
    /// verdict with it) from the screen.
    #[test]
    fn hud_fits_is_the_one_fold_rule() {
        assert!(!hud_fits(374.0, true), "a phone never has the HUD slot");
        assert!(
            !hud_fits(1312.0, true),
            "a phone never has the HUD slot, however wide its section"
        );
        assert!(!hud_fits(896.0, false), "1024x768: the card goes above");
        assert!(hud_fits(1152.0, false), "1280x720: the card has its slot");
        assert!(hud_fits(1312.0, false), "1440x900: the card has its slot");
    }

    /// Issue #81, criterion 2: **the panels stay reachable where the rail folds.** The parts rail folds to
    /// its icon strip below 1100 px of screen (`RAIL_FOLD_BELOW`, in `crate::ui`) and is not drawn at all on
    /// a phone, where the tower's own bays are the part controls - so every bay must still be drawn, and be
    /// a rect the pick path can hit, at all four sizes. The bands are as thin as the machine is: the drift
    /// eliminator is a slit (165x5 px at 390x844, 249x8 at 1024x768 in the round's frames), and the phone
    /// reaches the panels themselves through the drawer (`instrument-panel-390x844`), whose controls the
    /// round's frames carry.
    #[test]
    fn the_parts_stay_reachable_when_the_rail_folds() {
        let e = engine();
        let input = e.default_input();
        for (label, w, h, phone) in TARGETS {
            let l = layout(section(w, h), Some(&input), phone);
            assert_eq!(l.slots.len(), 4, "{label}: the four bays");
            for (slot, r) in l.slots.iter() {
                assert!(
                    r.width() >= 60.0 && r.height() >= 4.0,
                    "{label}: the {slot:?} bay is {:.0}x{:.0}",
                    r.width(),
                    r.height()
                );
                assert!(
                    l.area.contains_rect(*r),
                    "{label}: the {slot:?} bay is outside the section"
                );
            }
            // the gutter the labels live in is inside the section too
            assert!(
                l.area.contains_rect(l.gutter),
                "{label}: the call-out gutter leaves the section"
            );
        }
    }
}
