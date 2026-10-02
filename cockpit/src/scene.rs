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
}

/// Vertical fractions of the tower column, top -> bottom. They sum to 1.
const F_STACK: f32 = 0.11;
const F_DECK: f32 = 0.055;
/// Round 5, item 2: the plenum holds the operating-point rail *and* its read-out label, and the label is a
/// solid plate now (it used to be free text drawn under the fan bay's own labels). The band grew by 3.5 % of
/// the section's height so the plate sits above the rail instead of across it - the plinth, which is a
/// pedestal and carries no drawn machinery above the inlets, gave the 3.5 % up.
const F_PLENUM: f32 = 0.085;
/// Issue #58, the owner's layout nit 2: the drift and nozzle bays carry a label block of their own
/// (the record, the zone's engine value, the invitation). A block fits its bay's band in two lines -
/// the invitation on its own last line, where the bay's tag is drawn beside it - and the two bands
/// were a line short of that. Both grew, and the plinth (a pedestal that carries no drawn machinery
/// above the inlets) gave the height up, exactly as it did for the plenum in round 5.
const F_DRIFT: f32 = 0.075;
const F_SPRAY: f32 = 0.075;
const F_FILL: f32 = 0.30;
const F_RAIN: f32 = 0.07;
const F_BASIN: f32 = 0.095;
const F_PLINTH: f32 = 0.14;

/// The tower record's depth options set the ruler's domain; this is the fallback when there is no draft.
const FALLBACK_MAX_DEPTH: f32 = 2.1;

/// How much wider the stack mouth is than its base: a velocity-recovery flare. The fixture carries a stack
/// *area* and a recovery factor, no profile, so the flare is a look (and the README says so).
pub const STACK_FLARE: f32 = 1.14;

pub fn layout(area: egui::Rect, input: Option<&EngineInput>, phone: bool) -> Layout {
    let w = area.width();
    let h = area.height();

    // Columns: ruler | tower | rail. With the parts tray gone (round 2) the tower takes the width the
    // tray used to hold, so the machine reads as a machine at both viewports.
    let ruler_w = if phone { 0.0 } else { 52.0 };
    let tower_frac = if phone { 0.74 } else { 0.62 };
    let rail_w = if phone { 16.0 } else { 28.0 };

    let tower_x0 = area.left() + ruler_w;
    let tower_x1 = (tower_x0 + w * tower_frac).min(area.right() - rail_w - 16.0);
    let ruler = egui::Rect::from_min_max(area.min, egui::pos2(tower_x0, area.bottom()));
    let rail_x0 = tower_x1 + (area.right() - tower_x1 - rail_w) * 0.42;
    let rail = egui::Rect::from_min_max(
        egui::pos2(rail_x0, area.top() + h * 0.05),
        egui::pos2(rail_x0 + rail_w, area.bottom() - h * 0.12),
    );

    let mut y = area.top();
    let mut band = |frac: f32| {
        let r =
            egui::Rect::from_min_max(egui::pos2(tower_x0, y), egui::pos2(tower_x1, y + h * frac));
        y += h * frac;
        r
    };
    let stack = band(F_STACK);
    let deck = band(F_DECK);
    let plenum = band(F_PLENUM);
    let drift = band(F_DRIFT);
    let spray = band(F_SPRAY);
    let fill_band = band(F_FILL);
    let rain = band(F_RAIN);
    let basin = band(F_BASIN);
    let plinth = band(F_PLINTH);

    // The section's horizontal scale: a square-face reading of the fixture's fill area.
    let face_width_m = input
        .map(|i| i.tower.fill_area_m2.max(1.0).sqrt() as f32)
        .unwrap_or(8.0);
    let px_per_m_x = (tower_x1 - tower_x0) / face_width_m.max(1.0);
    // The stack diameter comes from the fitted fan's stack area, so dropping AX-420 for AX-500 widens the
    // stack in the frame (4.2 m -> 5.0 m on the fixture records).
    let stack_diameter_m = input
        .map(|i| m::stack_diameter_m(i.fan.stack_area_m2) as f32)
        .unwrap_or(5.0);
    // `clamp` panics when its upper bound falls below its lower one, and on a phone in the Operating-point
    // view the section is only a couple of hundred pixels wide and a hundred tall - narrow enough that
    // `(tower_x1 - tower_x0) * 0.92` dropped under the 24 px floor and the whole curves view trapped. The
    // bound is raised to the floor in that degenerate case (the drawn size is unchanged at any normal size).
    let base_w =
        (stack_diameter_m * px_per_m_x).clamp(24.0, ((tower_x1 - tower_x0) * 0.92).max(24.0));
    let top_w = (base_w * STACK_FLARE).min((tower_x1 - tower_x0) * 0.96);
    let cyl_top = egui::pos2(
        tower_x0 + (tower_x1 - tower_x0) * 0.5,
        stack.top() + stack.height() * 0.10,
    );
    let cyl_bottom = egui::pos2(tower_x0 + (tower_x1 - tower_x0) * 0.5, stack.bottom() - 2.0);
    let stack_cyl = egui::Rect::from_min_max(
        egui::pos2(cyl_top.x - top_w * 0.5, cyl_top.y),
        egui::pos2(cyl_bottom.x + base_w * 0.5, cyl_bottom.y),
    );
    let rim_h = (top_w * 0.20).clamp(6.0, 22.0);
    let stack_rim = egui::Rect::from_center_size(
        egui::pos2(cyl_top.x, cyl_top.y + rim_h * 0.5),
        egui::vec2(top_w, rim_h),
    );

    // Inlets: louvred banks on BOTH sides of the casing, from the rain zone down across the plinth, sized
    // from the tower's recorded inlet area (inletAreaM2) against its air-free area.
    let inlet_frac = input
        .map(|i| (i.tower.inlet_area_m2 / i.tower.air_free_area_m2.max(1.0)) as f32)
        .unwrap_or(0.5)
        .clamp(0.18, 0.62);
    let inlet_h = (plinth.height() * inlet_frac).clamp(28.0, (plinth.height() * 0.9).max(28.0));
    let inlet_top = plinth.bottom() - inlet_h - plinth.height() * 0.06;
    let louvre_w = if phone { 9.0 } else { 13.0 };
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
        egui::pos2(in_l0, inlet_top),
        egui::pos2(in_l1, inlet_top + inlet_h),
    );
    let inlet_r = egui::Rect::from_min_max(
        egui::pos2(in_r0, inlet_top),
        egui::pos2(in_r1, inlet_top + inlet_h),
    );

    // Fill layers: drawn to scale from the bottom of the band upward, minimum band from the mapping.
    let layers_in: Vec<f64> = input
        .map(|i| i.fill_layers.iter().map(|l| l.depth_m).collect())
        .unwrap_or_default();
    let total: f32 = layers_in.iter().map(|d| *d as f32).sum();
    let span = input
        .map(|i| {
            i.tower
                .fill_depth_options_m
                .last()
                .copied()
                .unwrap_or(FALLBACK_MAX_DEPTH as f64) as f32
        })
        .unwrap_or(FALLBACK_MAX_DEPTH);
    let used = if total > 0.0 {
        (total / span).clamp(0.22, 1.0)
    } else {
        0.0
    };
    let band_h = fill_band.height() * used;
    let mut rects = vec![egui::Rect::NOTHING; layers_in.len()];
    let mut bottom = fill_band.bottom();
    for i in (0..layers_in.len()).rev() {
        let frac = if total > 0.0 {
            layers_in[i] as f32 / total
        } else {
            1.0
        };
        let lh = (band_h * frac).max(m::MIN_BAND_PX);
        let top = (bottom - lh).max(fill_band.top() - m::MIN_BAND_PX);
        rects[i] = egui::Rect::from_min_max(
            egui::pos2(tower_x0 + 3.0, top),
            egui::pos2(tower_x1 - 3.0, bottom - 2.0),
        );
        bottom = top - 2.0;
    }

    let m_per_px_y = if band_h > 1.0 && total > 0.0 {
        total / band_h
    } else {
        0.0
    };

    // The operating-point rail sits in the plenum band, under the deck: it is an airflow rail, and the
    // plenum is where the fan's airflow is already collected.
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
        px_per_m_x,
        face_width_m,
        stack_diameter_m,
        phone,
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
    FanBlade(u8),
    FanHub,
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
            Part::Louvre(_, _) => 2.55,
            Part::StackRimBase(_) => 2.6,
            Part::RailBg => 2.62,
            Part::FanBlade(_) => 2.65,
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
            Part::FanHub => 4.5,
            Part::StackRim(_) => 4.6,
            Part::SlotWash(_) => 6.5,
            Part::SlotFocus(_) => 6.8,
            Part::CasingEdge(_) => 8.0,
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Viz(pub Part);

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

    // ---- the fan stack: a tapered cylinder standing on the deck, with blades in its mouth
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
    // The fan: a disc that brightens with rpm, six blades at the fixture's speed ratio, a hub.
    d.insert(
        Part::FanDisc,
        Draw::at(
            l.stack_rim.center(),
            egui::vec2(l.stack_rim.width() * 0.90, l.stack_rim.height() * 1.5),
            t::with_alpha(t::PRIMARY, (16.0 + 36.0 * speed_fraction) as u8),
        ),
    );
    let spin = anim_t * std::f32::consts::TAU * m::blade_turn_hz(rpm);
    let blade_n = 6;
    let hub_r = l.stack_rim.width() * 0.10;
    for i in 0..blade_n {
        let a = spin + i as f32 * std::f32::consts::TAU / blade_n as f32;
        let radial = l.stack_rim.width() * 0.26;
        let c = egui::pos2(
            l.stack_rim.center().x + radial * a.cos(),
            l.stack_rim.center().y + radial * a.sin() * 0.34,
        );
        d.insert(
            Part::FanBlade(i as u8),
            Draw::bar(
                c,
                l.stack_rim.width() * 0.30,
                if phone { 3.0 } else { 4.0 },
                a + std::f32::consts::FRAC_PI_2,
                t::with_alpha(t::INK_2, (150.0 + 90.0 * speed_fraction) as u8),
            ),
        );
    }
    d.insert(
        Part::FanHub,
        Draw::at(
            l.stack_rim.center(),
            egui::vec2(hub_r * 1.6, hub_r * 0.8),
            t::PRIMARY,
        ),
    );

    // ---- the fan deck: a plate with ribs, under the stack
    d.insert(
        Part::DeckPlate,
        Draw::rect(l.deck, t::with_alpha(t::INK_2, 30)),
    );
    for i in 0..3 {
        let x = l.deck.left() + l.deck.width() * (i as f32 + 1.0) / 4.0;
        d.insert(
            Part::DeckRib(i),
            Draw::at(
                egui::pos2(x, l.deck.center().y),
                egui::vec2(4.0, l.deck.height() * 0.8),
                t::with_alpha(t::LINE, 200),
            ),
        );
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
                t::with_alpha(t::PRIMARY, a.max(14)),
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
