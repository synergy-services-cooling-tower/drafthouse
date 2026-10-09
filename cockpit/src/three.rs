//! Round 2, change C: the **3D tower view** - the build/orientation view of the same state.
//!
//! Nothing here is hand-modelled. The tower is *generated* from the same resources the 2D section reads:
//!
//! | what is generated | from |
//! |---|---|
//! | the cell plan (`plan x plan`) | `tower.fillAreaM2` (a square-face reading) |
//! | the fan-stack diameter, and the drawn stack height | `fan.stackAreaM2` (`4A/pi`), x [`m::STACK_HEIGHT_FACTOR`] |
//! | the two fill layers, their depths and their materials | `EngineInput.fill_layers`, tinted by the fill ids |
//! | the drift bank, the nozzle header + heads, the rain zone, the basin water | the tower record's heights + the fitted drift / nozzle records |
//! | the cell count (1..8) | `Visual.cells` - a **UI count**, not an engine input |
//! | the cutaway, the focus cell, the camera | `Visual.cutaway` / `focus_cell` / `cam_*` |
//!
//! The 2D view stays the engineering view; this is the orientation view. Both read the same state, so
//! changing one changes the other (drop a fan in the 2D view and the 3D stacks widen; move the rpm and both
//! blade sets speed up).
//!
//! The flow inside the cut cell is the *same illustration* as the 2D view' streamlines: density and speed
//! follow the engine's airflow, colour follows the zone shares, and nothing here is a CFD result.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiPostUpdateSet;

use cockpit::engine::EngineInput;

use crate::app::{open_picker, AnimClock, Draft};
use crate::state::{Slot, View, Visual};
use crate::theme as t;
use drafthouse_cockpit_seams::mapping as m;

/// What the frame publishes about the 3D view (the `data-*` markers and the capture report read this).
#[derive(Resource, Default, Clone)]
pub struct ThreeInfo {
    pub meshes: usize,
    pub cut_parts: usize,
    pub cells: u32,
    pub cutaway: bool,
    pub focus_cell: usize,
    pub cam: (f32, f32, f32),
    /// The last tap resolved to something: `cell:<i>` or `bay:<slot>`.
    pub last_pick: String,
    pub frames: u64,
    /// The fan phase in radians, advanced by `mapping::blade_turn_hz(rpm)`. A still proves the blades are
    /// drawn; two samples of this marker prove they turn, and turn faster when the rpm control rises.
    pub blade_phase: f32,
    /// Round 3, item 6: how many pieces the cut *removed* (walls skipped, the plenum halved) and how many
    /// cut faces it *drew* (the exposed faces of both fill layers, the drift bank, the basin water and the
    /// frame around the opening). Counts written by the builder, so an evidence frame can prove the cut
    /// opened something instead of trusting a toggle.
    pub cut_removed: usize,
    pub cut_faces: usize,
}

/// The pickable things in the 3D scene: a whole cell (tap = cut it) or an exposed component (tap = picker).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    Cell(usize),
    Bay(Slot),
}

#[derive(Clone, Copy)]
struct PickBox {
    what: Pick,
    min: Vec3,
    max: Vec3,
}

#[derive(Resource, Default)]
struct ThreeScene {
    root: Option<Entity>,
    pickables: Vec<PickBox>,
    signature: String,
    /// Round 3, item 6: what the last build actually did for the cut - the pieces it removed and the cut
    /// faces it drew. Counted by the builder itself, so an evidence frame can prove the cut opened
    /// something rather than reading a toggle back.
    cut_removed: usize,
    cut_faces: usize,
}

#[derive(Clone)]
struct Mats {
    casing: Handle<StandardMaterial>,
    structure: Handle<StandardMaterial>,
    fill_a: Handle<StandardMaterial>,
    fill_b: Handle<StandardMaterial>,
    /// The *cut* face of each fill layer: the same hue, lifted, so both layers read as distinct materials
    /// across the opening (round 3, item 6).
    fill_a_cut: Handle<StandardMaterial>,
    fill_b_cut: Handle<StandardMaterial>,
    drift: Handle<StandardMaterial>,
    drift_cut: Handle<StandardMaterial>,
    /// Structure one step lighter than the casing: the rain-zone columns and other internals.
    structure_lit: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    blade: Handle<StandardMaterial>,
    flow_air: Handle<StandardMaterial>,
    flow_water: Handle<StandardMaterial>,
    cut_face: Handle<StandardMaterial>,
}

/// The three unit meshes everything is an instance of: a cube, a cylinder, a sphere.
#[derive(Resource, Default)]
struct Units {
    cube: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    sphere: Handle<Mesh>,
}

/// A mesh that exists only to be moved each frame: a fan blade, an air head, a water dash.
#[derive(Component, Clone, Copy)]
enum Mover {
    Blade {
        cell: usize,
        index: usize,
        count: usize,
        radius: f32,
        axis: Vec3,
        y: f32,
        sign: f32,
    },
    AirFlow {
        line: usize,
        head: usize,
        heads: usize,
        /// The height the line ends at: the top of the drift bank, which is where the opened half of the cut
        /// cell ends (the plenum above it is solid, so a particle lifted higher would be buried in it).
        top: f32,
        anchor: Vec3,
    },
    WaterFlow {
        streak: usize,
        dash: usize,
        dashes: usize,
        x: f32,
        top: f32,
        bottom: f32,
    },
}

pub struct ThreePlugin;

impl Plugin for ThreePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ThreeScene>()
            .init_resource::<ThreeInfo>()
            .init_resource::<Units>()
            .add_systems(Startup, (setup_three, unit_meshes))
            .add_systems(
                Update,
                (
                    follow_camera,
                    rebuild_when_state_changes,
                    animate_movers,
                    resolve_taps,
                    count_frames,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, publish_three.after(EguiPostUpdateSet::EndPass));
    }
}

// ------------------------------------------------------------------------------- camera and lights

/// A 3D camera that renders **before** the 2D one (which then clears nothing and paints egui on top).
fn setup_three(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::srgb_u8(0x0b, 0x12, 0x17)),
            ..default()
        },
        // No filmic tonemapping: the LUTs are a `tonemapping_luts` feature this pass does not need (Bevy
        // logs an error when the method is left at its default without the feature).
        Tonemapping::None,
        Transform::from_xyz(18.0, 12.0, 24.0).looking_at(Vec3::new(0.0, 6.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            ..default()
        },
        Transform::from_xyz(14.0, 22.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn(AmbientLight {
        color: Color::srgb_u8(0x9f, 0xb6, 0xc2),
        brightness: 480.0,
        ..default()
    });
}

fn unit_meshes(mut units: ResMut<Units>, mut meshes: ResMut<Assets<Mesh>>) {
    units.cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    units.cylinder = meshes.add(Cylinder::new(0.5, 1.0));
    units.sphere = meshes.add(Sphere::new(0.5));
}

/// The camera follows `Visual` (yaw / pitch / distance), and the 2D camera's clear is disabled whenever the
/// 3D view is up, so the 3D picture survives under the egui layer.
#[allow(clippy::type_complexity)]
fn follow_camera(
    vis: Res<Visual>,
    mut cam3: Query<
        (&mut Transform, &mut Camera, &mut Projection),
        (With<Camera3d>, Without<Camera2d>),
    >,
    mut cam2: Query<&mut Camera, (With<Camera2d>, Without<Camera3d>)>,
    draft: Option<Res<Draft>>,
) {
    let in_three = vis.view == View::Three;
    let plan = crate::state::cell_plan_m(draft.as_deref().map(|d| &d.0)) as f32;
    let row = m::cell_row_width_m(vis.cells, plan as f64) as f32;
    let height = tower_height(draft.as_deref().map(|d| &d.0));
    let target = Vec3::new(0.0, height * 0.45, 0.0);
    if let Ok((mut tf, mut cam, mut proj)) = cam3.single_mut() {
        cam.is_active = in_three;
        let yaw = vis.cam_yaw.to_radians();
        let pitch = vis.cam_pitch.to_radians();
        let dist = vis.cam_dist * row.max(6.0) * 0.55;
        let dir = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        tf.translation = target + dir * dist;
        tf.look_at(target, Vec3::Y);
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = 42_f32.to_radians();
        }
    }
    if let Ok(mut cam) = cam2.single_mut() {
        cam.clear_color = if in_three {
            ClearColorConfig::None
        } else {
            ClearColorConfig::Default
        };
    }
}

/// Total drawn tower height (m) - used for the camera target and the flow paths.
fn tower_height(input: Option<&EngineInput>) -> f32 {
    let plan = crate::state::cell_plan_m(input) as f32;
    let stack_d = crate::state::stack_diameter_m(input) as f32;
    let stack_h = m::stack_height_m(stack_d as f64) as f32;
    let plenum = plan * m::PLENUM_HEIGHT_FACTOR as f32;
    let rain = input
        .map(|i| i.tower.rain_zone_height_m as f32)
        .unwrap_or(1.4);
    let spray = input
        .map(|i| i.tower.spray_zone_height_m as f32)
        .unwrap_or(0.6);
    let fill: f32 = input
        .map(|i| i.fill_layers.iter().map(|l| l.depth_m as f32).sum())
        .unwrap_or(1.35);
    m::BASIN_DEPTH_M as f32
        + rain
        + fill
        + spray
        + m::DRIFT_BANK_T_M as f32
        + plenum
        + m::DECK_T_M as f32
        + stack_h
}

// ---------------------------------------------------------------------------------------- geometry

fn material(
    colors: &mut Assets<StandardMaterial>,
    base: egui::Color32,
    alpha: f32,
    rough: f32,
) -> Handle<StandardMaterial> {
    colors.add(StandardMaterial {
        base_color: Color::srgba_u8(base.r(), base.g(), base.b(), (alpha * 255.0) as u8),
        perceptual_roughness: rough,
        metallic: 0.0,
        alpha_mode: if alpha < 0.99 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        ..default()
    })
}

fn build_mats(colors: &mut Assets<StandardMaterial>, input: &EngineInput) -> Mats {
    let fill_a = t::fill_color(
        input
            .fill_layers
            .first()
            .map(|l| l.fill_id.as_str())
            .unwrap_or("FILM-MF20"),
    );
    let fill_b = t::fill_color(
        input
            .fill_layers
            .get(1)
            .map(|l| l.fill_id.as_str())
            .unwrap_or("FILM-WF25"),
    );
    Mats {
        casing: material(colors, t::LINE_SOFT, 1.0, 0.85),
        structure: material(colors, t::PANEL_RAISED, 1.0, 0.7),
        fill_a: material(colors, fill_a, 1.0, 0.95),
        fill_b: material(colors, fill_b, 1.0, 0.95),
        fill_a_cut: material(colors, lift(fill_a, 46), 1.0, 0.9),
        fill_b_cut: material(colors, lift(fill_b, 46), 1.0, 0.9),
        drift: material(colors, t::OK, 1.0, 0.8),
        drift_cut: material(colors, lift(t::OK, 40), 1.0, 0.75),
        structure_lit: material(colors, lift(t::PANEL_RAISED, 34), 1.0, 0.7),
        water: material(colors, t::WATER, 0.72, 0.25),
        accent: material(colors, t::PRIMARY, 1.0, 0.5),
        blade: material(colors, t::INK_2, 1.0, 0.4),
        flow_air: material(colors, lift(t::AIR, 30), 0.62, 0.5),
        flow_water: material(colors, lift(t::WATER, 30), 0.66, 0.3),
        cut_face: material(colors, t::PRIMARY_SOFT, 1.0, 0.8),
    }
}

/// A lifted colour: the same hue, brighter. Used for the *cut faces* of the exposed layers, so a cut
/// surface reads as a cut surface rather than as one more shaded side.
fn lift(c: egui::Color32, by: u8) -> egui::Color32 {
    egui::Color32::from_rgb(
        c.r().saturating_add(by),
        c.g().saturating_add(by),
        c.b().saturating_add(by),
    )
}

/// One box instance: a unit cube scaled and moved. Everything solid in the 3D tower is one of these.
fn box_at(
    commands: &mut Commands,
    units: &Units,
    mat: &Handle<StandardMaterial>,
    c: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Mesh3d(units.cube.clone()),
        MeshMaterial3d(mat.clone()),
        Transform::from_translation(c).with_scale(size),
    ));
}

// ------------------------------------------------------------------------------------- the rebuild

/// A signature of everything the geometry depends on: when it changes, the tower is rebuilt.
fn signature(vis: &Visual, input: &EngineInput) -> String {
    let stack: Vec<String> = input
        .fill_layers
        .iter()
        .map(|l| format!("{}:{:.2}", l.fill_id, l.depth_m))
        .collect();
    format!(
        "cells={} cut={} focus={} fan={} drift={} nozzle={} tower={} stack={}",
        vis.cells,
        vis.cutaway,
        vis.focus_cell.min(vis.cells.saturating_sub(1) as usize),
        input.fan.id,
        input.drift.id,
        input.nozzle.id,
        input.tower.id,
        stack.join(",")
    )
}

#[allow(clippy::too_many_arguments)]
fn rebuild_when_state_changes(
    mut commands: Commands,
    mut scene: ResMut<ThreeScene>,
    units: Res<Units>,
    mut colors: ResMut<Assets<StandardMaterial>>,
    vis: Res<Visual>,
    draft: Option<Res<Draft>>,
    run: Option<Res<crate::state::Run>>,
) {
    let Some(draft) = draft else { return };
    let input = &draft.0;
    let sig = signature(&vis, input);
    if scene.root.is_some() && scene.signature == sig {
        return;
    }
    // rebuild: drop the old root (children go with it) and generate the tower from the state
    if let Some(root) = scene.root.take() {
        commands.entity(root).despawn();
    }
    // The materials follow the identities too: a fill replaced in the picker is tinted the same colour in
    // the 3D view as it is in the 2D section (`theme::fill_color`).
    let mats = build_mats(&mut colors, input);

    let root = commands
        .spawn((Transform::default(), Visibility::default()))
        .id();
    scene.root = Some(root);
    scene.pickables.clear();
    let mut pickables: Vec<PickBox> = Vec::new();
    // round 3, item 6: what this build removes and what it exposes for the cut
    let mut cut_removed = 0usize;
    let mut cut_faces = 0usize;

    let plan = crate::state::cell_plan_m(Some(input)) as f32;
    let stack_d = crate::state::stack_diameter_m(Some(input)) as f32;
    let stack_h = m::stack_height_m(stack_d as f64) as f32;
    let plenum = plan * m::PLENUM_HEIGHT_FACTOR as f32;
    let rain_h = input.tower.rain_zone_height_m as f32;
    let spray_h = input.tower.spray_zone_height_m as f32;
    let drift_t = m::DRIFT_BANK_T_M as f32;
    let basin_h = m::BASIN_DEPTH_M as f32;
    let casing_t = m::CUTAWAY_CASING_T_M as f32;
    let deck_t = m::DECK_T_M as f32;
    let flow_f = run
        .as_deref()
        .and_then(|r| r.output.as_ref())
        .map(|o| m::flow_factor(o.airflow_m3_s))
        .unwrap_or(1.0);

    let cells = vis.cells.max(1);
    let focus = vis.focus_cell.min(cells as usize - 1);
    let cutaway = vis.cutaway;
    let half_of_plan = plan * 0.5;

    // vertical stack of the zones (bottom -> top), all from the records
    let y_basin = 0.0;
    let y_rain = basin_h;
    let y_fill = y_rain + rain_h;
    let fill_total: f32 = input.fill_layers.iter().map(|l| l.depth_m as f32).sum();
    let y_spray = y_fill + fill_total;
    let y_drift = y_spray + spray_h;
    let y_plenum = y_drift + drift_t;
    let y_deck = y_plenum + plenum;
    let y_stack = y_deck + deck_t;
    let top = y_stack + stack_h;

    let spawn = |commands: &mut Commands, c: Vec3, size: Vec3, mat: &Handle<StandardMaterial>| {
        box_at(commands, &units, mat, c, size);
    };

    // ---- the shared basin: a shell (floor + walls), not a solid, so the cut cell can be opened into it and
    // the water surface is something the cutaway can actually show.
    let row_w = m::cell_row_width_m(cells, plan as f64) as f32;
    let water_t = basin_h * 0.62;
    spawn(
        &mut commands,
        Vec3::new(0.0, casing_t * 0.5, 0.0),
        Vec3::new(row_w, casing_t, plan),
        &mats.structure,
    );
    spawn(
        &mut commands,
        Vec3::new(0.0, basin_h * 0.5, -plan * 0.5 + casing_t * 0.5),
        Vec3::new(row_w, basin_h, casing_t),
        &mats.structure,
    );
    for side in [-1.0_f32, 1.0] {
        spawn(
            &mut commands,
            Vec3::new(side * (row_w * 0.5 - casing_t * 0.5), basin_h * 0.5, 0.0),
            Vec3::new(casing_t, basin_h, plan),
            &mats.structure,
        );
    }
    // the near wall, in pieces around the cut cell (its own stretch is the opening)
    let near_z = plan * 0.5 - casing_t * 0.5;
    let mut near_spans: Vec<(f32, f32)> = vec![(-row_w * 0.5, row_w * 0.5)];
    if cutaway {
        let fx = m::cell_center_x_m(focus as u32, cells, plan as f64) as f32;
        let mut split: Vec<(f32, f32)> = Vec::new();
        for (a, b) in near_spans.drain(..) {
            let (lo, hi) = (fx - half_of_plan, fx + half_of_plan);
            if lo - a > 1e-3 {
                split.push((a, lo.min(b)));
            }
            if b - hi > 1e-3 {
                split.push((hi.max(a), b));
            }
        }
        near_spans = split;
    }
    for (a, b) in near_spans {
        let (a, b) = (a.max(-row_w * 0.5), b.min(row_w * 0.5));
        if b - a > 1e-3 {
            spawn(
                &mut commands,
                Vec3::new((a + b) * 0.5, basin_h * 0.5, near_z),
                Vec3::new(b - a, basin_h, casing_t),
                &mats.structure,
            );
        }
    }
    // the water surface in the shared basin
    spawn(
        &mut commands,
        Vec3::new(0.0, water_t, 0.0),
        Vec3::new(row_w - casing_t * 2.0, 0.06, plan - casing_t * 2.0),
        &mats.water,
    );

    for cell in 0..cells as usize {
        let cx = m::cell_center_x_m(cell as u32, cells, plan as f64) as f32;
        let cut = cutaway && cell == focus;
        // The cell's own footprint: the plan, centred on the cell's slot in the row.
        let half = plan * 0.5;
        // everything is generated in the cell's own frame, then offset by cx
        let at = |y: f32| Vec3::new(cx, y, 0.0);

        // --- the casing: four walls around the zones (skipped on the cut cell's near side)
        let wall_h = y_drift + drift_t * 0.5 - y_basin;
        let wall_y = y_basin + wall_h * 0.5;
        let walls: [(Vec3, Vec3); 4] = [
            (
                Vec3::new(0.0, 0.0, -half + casing_t * 0.5),
                Vec3::new(plan, wall_h, casing_t),
            ),
            (
                Vec3::new(-half + casing_t * 0.5, 0.0, 0.0),
                Vec3::new(casing_t, wall_h, plan),
            ),
            (
                Vec3::new(half - casing_t * 0.5, 0.0, 0.0),
                Vec3::new(casing_t, wall_h, plan),
            ),
            (
                Vec3::new(0.0, 0.0, half - casing_t * 0.5),
                Vec3::new(plan, wall_h, casing_t),
            ),
        ];
        for (i, (off, size)) in walls.into_iter().enumerate() {
            let near = off.z > 0.0;
            if cut && near {
                cut_removed += 1;
                continue; // the cut removes the near wall
            }
            // index 2 is the +X wall: in a row it is the *next* cell's partition, so the row draws it once
            if i == 2 && cell + 1 < cells as usize {
                continue;
            }
            // Round 3, item 6: the cut cell loses the two walls on the camera's side as well, and the
            // neighbour's half of the shared partition goes with them. Without that the opening is only ever
            // seen at a grazing angle - the round-2 frame read as a narrow slit rather than as a cut.
            if cut && (i == 1 || i == 2) {
                cut_removed += 1;
                continue;
            }
            if i == 1 && cell > 0 && cell == focus + 1 {
                continue; // the previous cell's +X wall is the same plane as the cut cell's -X wall
            }
            spawn(&mut commands, at(wall_y) + off, size, &mats.casing);
        }
        // the casing edge: a lit rim along the top of the casing, so the machine reads as a solid with an
        // edge instead of a dark block (round 3, item 6)
        for (off, size) in [
            (
                Vec3::new(0.0, 0.0, half - casing_t * 0.5),
                Vec3::new(plan, 0.10, casing_t),
            ),
            (
                Vec3::new(0.0, 0.0, -half + casing_t * 0.5),
                Vec3::new(plan, 0.10, casing_t),
            ),
            (
                Vec3::new(-half + casing_t * 0.5, 0.0, 0.0),
                Vec3::new(casing_t, 0.10, plan),
            ),
        ] {
            if cut && off.z > 0.0 {
                continue;
            }
            spawn(
                &mut commands,
                at(wall_y + wall_h * 0.5) + off,
                size,
                &mats.accent,
            );
        }

        // --- louvred air inlets on both sides the section shows (the cell's long faces, +/-Z), at the inlet
        // height. In a row the cells share the casing, so the +/-X walls are partitions and only the row's
        // two ends carry louvres of their own.
        let louvre_h = rain_h * 0.9;
        let slats = 5;
        for side in [-1.0_f32, 1.0] {
            if cut && side > 0.0 {
                cut_removed += slats;
                continue; // the cut removes the near louvre wall as well
            }
            for k in 0..slats {
                let y = y_rain + louvre_h * (k as f32 + 0.5) / slats as f32;
                // Round 3, item 6: the slats sit *on* the wall face (they used to sit inside it, where the
                // casing hid them), so the inlet reads as a louvred wall from outside.
                let size = Vec3::new(plan * 0.88, louvre_h / slats as f32 * 0.42, 0.08);
                let off = Vec3::new(0.0, 0.0, side * (half + 0.05));
                spawn(&mut commands, at(y) + off, size, &mats.accent);
            }
        }
        // the two ends of the row get an inlet face as well, so cell 1 and cell N do not read as blank ends
        let end_side = if cell == 0 {
            Some(-1.0_f32)
        } else if cell + 1 == cells as usize {
            Some(1.0)
        } else {
            None
        };
        if let Some(side) = end_side {
            if !(cut && side > 0.0) {
                for k in 0..slats {
                    let y = y_rain + louvre_h * (k as f32 + 0.5) / slats as f32;
                    let size = Vec3::new(0.08, louvre_h / slats as f32 * 0.42, plan * 0.88);
                    let off = Vec3::new(side * (half + 0.05), 0.0, 0.0);
                    spawn(&mut commands, at(y) + off, size, &mats.accent);
                }
            }
        }

        // --- the rain-zone support columns (round 3, item 6: lifted and thickened, so the open rain zone
        // reads as a void on columns rather than as a dark band)
        for c in 0..3 {
            let x = -half * 0.6 + half * 0.6 * c as f32;
            let off = Vec3::new(x, 0.0, -half * 0.35);
            if cut && off.z > 0.0 {
                continue;
            }
            spawn(
                &mut commands,
                at(y_rain + rain_h * 0.5) + off,
                Vec3::new(casing_t * 1.7, rain_h * 0.94, casing_t * 1.7),
                &mats.structure_lit,
            );
        }

        // --- the fill layers, in the engine's own order (index 0 = top)
        let mut y = y_fill;
        for (i, layer) in input.fill_layers.iter().enumerate() {
            let h = layer.depth_m as f32;
            let mat = if i == 0 {
                mats.fill_a.clone()
            } else {
                mats.fill_b.clone()
            };
            let mat_cut = if i == 0 {
                mats.fill_a_cut.clone()
            } else {
                mats.fill_b_cut.clone()
            };
            let depth = if cut { plan * 0.5 } else { plan };
            let z = if cut { -plan * 0.25 } else { 0.0 };
            spawn(
                &mut commands,
                at(y + h * 0.5) + Vec3::new(0.0, 0.0, z),
                Vec3::new(plan - casing_t * 2.0, h, depth - casing_t * 2.0),
                &mat,
            );
            if cut {
                cut_faces += 1;
                // The exposed cut face of the layer, on the cut plane: the same hue lifted, so both layers
                // read as two distinct materials across the opening.
                spawn(
                    &mut commands,
                    at(y + h * 0.5) + Vec3::new(0.0, 0.0, 0.035),
                    Vec3::new(plan - casing_t * 2.0, h * 0.94, 0.03),
                    &mat_cut,
                );
                pickables.push(PickBox {
                    what: Pick::Bay(Slot::Fill),
                    min: Vec3::new(cx - half, y, -plan * 0.5),
                    max: Vec3::new(cx + half, y + h, 0.0),
                });
            }
            y += h;
        }

        // --- the drift bank above the spray zone
        let drift_depth = if cut { plan * 0.5 } else { plan };
        let drift_z = if cut { -plan * 0.25 } else { 0.0 };
        spawn(
            &mut commands,
            at(y_drift + drift_t * 0.5) + Vec3::new(0.0, 0.0, drift_z),
            Vec3::new(plan - casing_t * 2.0, drift_t, drift_depth - casing_t * 2.0),
            &mats.drift,
        );
        if cut {
            cut_faces += 1;
            // the exposed cut face of the drift bank, lifted the same way the fill layers are
            spawn(
                &mut commands,
                at(y_drift + drift_t * 0.5) + Vec3::new(0.0, 0.0, 0.035),
                Vec3::new(plan - casing_t * 2.0, drift_t * 0.92, 0.03),
                &mats.drift_cut,
            );
            pickables.push(PickBox {
                what: Pick::Bay(Slot::Drift),
                min: Vec3::new(cx - half, y_drift, -plan * 0.5),
                max: Vec3::new(cx + half, y_drift + drift_t, 0.0),
            });
        }

        // --- the distribution header + nozzle heads (in the cutaway, hanging under the drift bank)
        let header_off = Vec3::new(0.0, 0.0, if cut { -plan * 0.28 } else { 0.0 });
        spawn(
            &mut commands,
            at(y_spray + spray_h * 0.85) + header_off,
            Vec3::new(plan - casing_t * 3.0, 0.12, 0.12),
            &mats.accent,
        );
        for n in 0..3 {
            let x = -half * 0.6 + half * 0.6 * n as f32;
            spawn(
                &mut commands,
                at(y_spray + spray_h * 0.72) + header_off + Vec3::new(x, 0.0, 0.0),
                Vec3::new(0.10, 0.26, 0.10),
                &mats.accent,
            );
            // the sheet of water the heads throw: a thin cone of boxes
            if !cut || header_off.z <= 0.0 {
                spawn(
                    &mut commands,
                    at(y_spray + spray_h * 0.35) + header_off + Vec3::new(x, 0.0, 0.0),
                    Vec3::new(0.34, spray_h * 0.55, 0.34),
                    &mats.flow_water,
                );
            }
        }
        if cut {
            pickables.push(PickBox {
                what: Pick::Bay(Slot::Nozzle),
                min: Vec3::new(cx - half, y_spray, -plan * 0.5),
                max: Vec3::new(cx + half, y_spray + spray_h, 0.0),
            });
        }

        // --- the plenum and the fan deck. Round 3, item 6: in the cut cell the plenum loses its near half
        // too - it stands directly over the drift bank, and a full-depth plenum block is exactly what hid the
        // drift bank, the header and the fill layers from the camera in the round-2 frame.
        if cut {
            cut_removed += 1; // the plenum loses its near half as well
        }
        let plenum_depth = if cut { plan * 0.5 } else { plan };
        let plenum_z = if cut { -plan * 0.25 } else { 0.0 };
        spawn(
            &mut commands,
            at(y_plenum + plenum * 0.5) + Vec3::new(0.0, 0.0, plenum_z),
            Vec3::new(plan - casing_t * 2.0, plenum, plenum_depth - casing_t * 2.0),
            &mats.structure,
        );
        spawn(
            &mut commands,
            at(y_deck + deck_t * 0.5),
            Vec3::new(plan, deck_t, plan),
            &mats.casing,
        );

        // --- the cut: a lit frame around the opened half, so the frame reads as a cut through the machine
        // and not as a missing wall (round 3, item 6)
        if cut {
            cut_faces += 4; // the four edges of the frame
            let top = y_drift + drift_t;
            for (c, size) in [
                (
                    Vec3::new(cx, y_basin + 0.05, 0.03),
                    Vec3::new(plan, 0.10, 0.10),
                ),
                (Vec3::new(cx, top - 0.05, 0.03), Vec3::new(plan, 0.10, 0.10)),
                (
                    Vec3::new(cx - half + 0.04, (y_basin + top) * 0.5, 0.03),
                    Vec3::new(0.09, top - y_basin, 0.09),
                ),
                (
                    Vec3::new(cx + half - 0.04, (y_basin + top) * 0.5, 0.03),
                    Vec3::new(0.09, top - y_basin, 0.09),
                ),
            ] {
                spawn(&mut commands, c, size, &mats.accent);
            }
            // the basin water, on the cut plane: the water level the rain zone falls into
            cut_faces += 1;
            spawn(
                &mut commands,
                Vec3::new(cx, water_t * 0.5, 0.045),
                Vec3::new(plan - casing_t * 2.0, water_t, 0.03),
                &mats.water,
            );
        }

        // --- the fan stack: a tapered cylinder (the fixture carries the fan's stack area, not a profile)
        let base_r = stack_d * 0.5 * 0.92;
        let top_r = stack_d * 0.5;
        let seg_h = stack_h / 3.0;
        for s in 0..3 {
            let f = s as f32 / 3.0;
            let r = base_r + (top_r - base_r) * f;
            commands.spawn((
                Mesh3d(units.cylinder.clone()),
                MeshMaterial3d(mats.casing.clone()),
                Transform::from_translation(at(y_stack + seg_h * (s as f32 + 0.5)))
                    .with_scale(Vec3::new(r * 2.0, seg_h, r * 2.0)),
            ));
        }
        // the rim, so the mouth reads as an opening
        commands.spawn((
            Mesh3d(units.cylinder.clone()),
            MeshMaterial3d(mats.accent.clone()),
            Transform::from_translation(at(y_stack + stack_h - 0.04)).with_scale(Vec3::new(
                top_r * 2.06,
                0.08,
                top_r * 2.06,
            )),
        ));

        // --- the fan blades, rotating with the rpm control. A blade reaches from (r - len/2) to (r + len/2):
        // with r = 0.35 * top_r and len = 0.95 * top_r the blade tip stays inside the stack mouth, which is
        // what the fan is drawn in. (Round 2 first used r = 0.42 and len = 1.5, so the tips stuck out of the
        // cylinder as pale plates on the fan deck.)
        let blades = 6;
        for b in 0..blades {
            let r = top_r * 0.35;
            let angled = b as f32 * std::f32::consts::TAU / blades as f32;
            let pos = Vec3::new(
                cx + r * angled.cos(),
                y_stack + stack_h * 0.35,
                r * angled.sin(),
            );
            commands.spawn((
                Mesh3d(units.cube.clone()),
                MeshMaterial3d(mats.blade.clone()),
                Transform::from_translation(pos).with_scale(Vec3::new(
                    top_r * 0.95,
                    0.06,
                    top_r * 0.30,
                )),
                Mover::Blade {
                    cell,
                    index: b,
                    count: blades,
                    radius: r,
                    axis: Vec3::new(cx, y_stack + stack_h * 0.35, 0.0),
                    y: y_stack + stack_h * 0.35,
                    sign: 1.0,
                },
            ));
        }
        // the hub
        commands.spawn((
            Mesh3d(units.cylinder.clone()),
            MeshMaterial3d(mats.structure.clone()),
            Transform::from_translation(at(y_stack + stack_h * 0.35)).with_scale(Vec3::new(
                top_r * 0.34,
                0.34,
                top_r * 0.34,
            )),
        ));

        // --- the pickable whole cell: tapping it makes it the cut cell
        pickables.push(PickBox {
            what: Pick::Cell(cell),
            min: Vec3::new(cx - half, 0.0, -plan * 0.5),
            max: Vec3::new(cx + half, top, plan * 0.5),
        });

        // --- the cut cell's section face: the *back* wall of the opened cell. Round 2 first drew this plate on
        // the cut plane itself, where - being opaque and nearest the camera - it hid the drift bank, the header
        // and the columns the cut is for. It now sits at the far side of the exposed half, so the cut reads as
        // an open box with the internals standing in front of a dark section face.
        if cut {
            spawn(
                &mut commands,
                Vec3::new(cx, (y_basin + y_drift) * 0.5, -plan * 0.5 + 0.04),
                Vec3::new(plan, y_drift - y_basin, 0.06),
                &mats.cut_face,
            );

            // --- the flow inside the cut cell: the same illustration as the 2D streamlines.
            //
            // Round 3, item 6: the particles live in the half the cut *opened* (z > 0). Round 2 placed them
            // behind the cut plane, where the fill / drift / plenum solids swallowed them - which is why the
            // frame showed a slit with no flow in it. Counts are still capped so the software rasteriser keeps
            // up (see the README: the cap, not the GPU, is the constraint in evidence capture).
            let lines = m::streamline_count(
                run.as_deref()
                    .and_then(|r| r.output.as_ref())
                    .map(|o| o.airflow_m3_s)
                    .unwrap_or(m::ANCHOR_AIRFLOW_M3_S),
            );
            let heads = (3.0 + 5.0 * flow_f).round().min(8.0) as usize;
            for l in 0..lines {
                let x = cx - half * 0.62 + half * 1.24 * (l as f32 + 0.5) / lines.max(1) as f32;
                for h in 0..heads {
                    commands.spawn((
                        Mesh3d(units.cube.clone()),
                        MeshMaterial3d(mats.flow_air.clone()),
                        Transform::from_translation(Vec3::new(
                            x,
                            y_rain + rain_h * 0.5,
                            plan * (0.16 + 0.26 * (h as f32) / heads as f32),
                        ))
                        .with_scale(Vec3::new(0.22, 0.22, 0.62)),
                        Mover::AirFlow {
                            line: l,
                            head: h,
                            heads,
                            top: y_drift + drift_t,
                            anchor: Vec3::new(x, y_rain, 0.0),
                        },
                    ));
                }
            }
            let streaks = m::water_streak_count(
                run.as_deref()
                    .and_then(|r| r.output.as_ref())
                    .map(|o| o.water_flow_m3_hr)
                    .unwrap_or(700.0),
            )
            .min(6);
            let dashes = 3;
            for s in 0..streaks {
                let x = cx - half * 0.6 + half * 1.2 * (s as f32 + 0.5) / streaks.max(1) as f32;
                for dd in 0..dashes {
                    commands.spawn((
                        Mesh3d(units.cube.clone()),
                        MeshMaterial3d(mats.flow_water.clone()),
                        Transform::from_translation(Vec3::new(x, y_spray, plan * 0.24))
                            .with_scale(Vec3::new(0.14, 0.62, 0.14)),
                        Mover::WaterFlow {
                            streak: s,
                            dash: dd,
                            dashes,
                            x,
                            top: y_fill + fill_total + spray_h * 0.7,
                            bottom: water_t,
                        },
                    ));
                }
            }
        }
    }

    scene.pickables = pickables;
    scene.cut_removed = cut_removed;
    scene.cut_faces = cut_faces;
    scene.signature = sig;
}

// ---------------------------------------------------------------------------------- the animation

fn animate_movers(
    clock: Res<AnimClock>,
    draft: Option<Res<Draft>>,
    vis: Res<Visual>,
    run: Option<Res<crate::state::Run>>,
    mut info: ResMut<ThreeInfo>,
    mut q: Query<(&Mover, &mut Transform)>,
) {
    let Some(draft) = draft else { return };
    let rpm = m::rpm(draft.0.speed_ratio, draft.0.fan.nominal_rpm).unwrap_or(0.0);
    let spin = clock.t * std::f32::consts::TAU * m::blade_turn_hz(rpm);
    let flow = run
        .as_deref()
        .and_then(|r| r.output.as_ref())
        .map(|o| o.airflow_m3_s)
        .unwrap_or(m::ANCHOR_AIRFLOW_M3_S);
    let flow_f = m::flow_factor(flow);
    let _ = &vis;
    // the same phase the blades below are placed with, so the marker and the picture cannot drift apart
    info.blade_phase = spin; // left unwrapped: the marker is a rate, the blades use it modulo a turn
    for (mover, mut tf) in &mut q {
        match *mover {
            Mover::Blade {
                cell,
                index,
                count,
                radius,
                axis,
                y,
                sign,
                ..
            } => {
                // blades are drawn per cell and per blade, so a row of cells does not strobe in step
                let a = spin * sign
                    + (index as f32 + cell as f32 * 0.5) * std::f32::consts::TAU / count as f32;
                tf.translation = Vec3::new(axis.x + radius * a.cos(), y, radius * a.sin());
                tf.rotation = Quat::from_rotation_y(-a);
            }
            Mover::AirFlow {
                line,
                head,
                heads,
                top,
                anchor,
                ..
            } => {
                let t = (clock.t * 0.28 * flow_f + head as f32 / heads as f32 + line as f32 * 0.07)
                    % 1.0;
                // in at the louvre height, up through the fill, out at the drift bank: a simple rising path
                let y = anchor.y + (top - anchor.y) * t;
                let x = anchor.x;
                tf.translation = Vec3::new(x, y, tf.translation.z);
                tf.rotation = Quat::from_rotation_y(0.0);
            }
            Mover::WaterFlow {
                streak,
                dash,
                dashes,
                x,
                top,
                bottom,
                ..
            } => {
                let t =
                    (clock.t * 0.32 * flow_f + dash as f32 / dashes as f32 + streak as f32 * 0.11)
                        % 1.0;
                let y = top - (top - bottom) * t;
                tf.translation = Vec3::new(x, y, tf.translation.z);
            }
        }
    }
}

/// The very top of the drawn tower (m): where a streamline leaves.
pub fn stack_top(input: &EngineInput) -> f32 {
    tower_height(Some(input))
}

// ------------------------------------------------------------------------------------------ picking

/// Resolve a tap published by the egui viewport (`Visual.three_click`): nearest box under the ray wins.
#[allow(clippy::too_many_arguments)]
fn resolve_taps(
    mut vis: ResMut<Visual>,
    scene: Res<ThreeScene>,
    mut info: ResMut<ThreeInfo>,
    mut draft: Option<ResMut<Draft>>,
    cam: Query<(&GlobalTransform, &Projection), With<Camera3d>>,
) {
    let Some(click) = vis.three_click.take() else {
        return;
    };
    if vis.view != View::Three {
        return;
    }
    let [lx, ly, w, h] = click;
    if w < 4.0 || h < 4.0 {
        return;
    }
    let Ok((tf, proj)) = cam.single() else { return };
    let fov = match proj {
        Projection::Perspective(p) => p.fov,
        _ => 45_f32.to_radians(),
    };
    let aspect = w / h;
    let ndc = Vec2::new(2.0 * lx / w - 1.0, 1.0 - 2.0 * ly / h);
    let dir_view = Vec3::new(
        ndc.x * (fov * 0.5).tan() * aspect,
        ndc.y * (fov * 0.5).tan(),
        -1.0,
    )
    .normalize();
    let dir = tf.rotation() * dir_view;
    let origin = tf.translation();

    // A tap resolves in two passes: an exposed *component* of the cut cell wins over the cell box that
    // encloses it (otherwise every tap on the cutaway would just re-select the cell), and a tap that only
    // meets a whole cell selects that cell as the focus cell.
    let mut best_bay: Option<(f32, Pick)> = None;
    let mut best_cell: Option<(f32, Pick)> = None;
    for pb in scene.pickables.iter() {
        let Some(t) = ray_box(origin, dir, pb.min, pb.max) else {
            continue;
        };
        let slot = match pb.what {
            Pick::Bay(_) => &mut best_bay,
            Pick::Cell(_) => &mut best_cell,
        };
        if slot.map(|(bt, _)| t < bt).unwrap_or(true) {
            *slot = Some((t, pb.what));
        }
    }
    let Some((_, what)) = best_bay.or(best_cell) else {
        vis.flash = Some(crate::state::Flash {
            ok: false,
            text: "nothing there - tap a cell or an exposed component".into(),
        });
        return;
    };
    match what {
        Pick::Cell(i) => {
            vis.focus_cell = i;
            vis.cutaway = true;
            vis.flash = Some(crate::state::Flash {
                ok: true,
                text: format!("cell {} is the focus cell: cut away", i + 1),
            });
            if let Some(d) = draft.as_deref_mut() {
                let _ = &mut d.0;
            }
        }
        Pick::Bay(slot) => {
            open_picker(&mut vis, slot, false);
        }
    }
    info.last_pick = match what {
        Pick::Cell(i) => format!("cell:{i}"),
        Pick::Bay(s) => format!("bay:{}", s.slug()),
    };
}

/// Slab test: the distance along `dir` where the ray enters the box, if it does.
fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        if d.abs() < 1e-6 {
            if o < min[axis] || o > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d;
        let mut t1 = (min[axis] - o) * inv;
        let mut t2 = (max[axis] - o) * inv;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmin > tmax {
            return None;
        }
    }
    if tmax < 0.0 {
        return None;
    }
    Some(tmin.max(0.0))
}

// ---------------------------------------------------------------------------------------- the frame

fn count_frames(mut info: ResMut<ThreeInfo>) {
    info.frames = info.frames.wrapping_add(1);
}

/// Copy what the 3D view is showing into the resource the bridge publishes.
fn publish_three(
    mut info: ResMut<ThreeInfo>,
    scene: Res<ThreeScene>,
    vis: Res<Visual>,
    meshes: Query<&Mesh3d>,
) {
    info.meshes = meshes.iter().count();
    info.cells = vis.cells;
    info.cutaway = vis.cutaway;
    info.focus_cell = vis.focus_cell.min(vis.cells.saturating_sub(1) as usize);
    info.cam = (vis.cam_yaw, vis.cam_pitch, vis.cam_dist);
    info.cut_parts = if vis.cutaway { 1 } else { 0 };
    info.cut_removed = scene.cut_removed;
    info.cut_faces = scene.cut_faces;
    let _ = &scene;
}
