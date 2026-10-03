//! The recording of the pass's arithmetic: the constants that are still the pass's own, and the
//! conversions of the record's own values.
//!
//! Nothing here is cooling-tower physics and nothing here is presented as an engineering result. Each
//! constant exists because the engine's data model (or the fixture catalog) does not carry the number a
//! *picture* needs - a cone angle for an orifice, an animation speed for an airflow, a drawn height -
//! and each is unit-tested so the picture cannot drift silently. Where the record **does** carry the
//! number (a fan's rated speed, `FanRecord.nominalRpm`), the pass reads it and invents nothing: the
//! rpm read-out is `speed_ratio × nominal_rpm` and is blank when the record states no rated speed.
//!
//! Every function is pure: same input, same output, no state, no randomness.

use std::f64::consts::PI;

/// The recorded run's airflow (`anchor.headline.airflow_m3_s` in the fixture): the 1x reference every
/// animation speed in the pass is scaled against.
pub const ANCHOR_AIRFLOW_M3_S: f64 = 124.841_699_574_828_14;

/// Smallest drawn height of a fill layer, in points. A 0.45 m layer at true scale on a 900 px screen is
/// about 15 px; without a floor the mixed stack's upper layer reads as a line.
pub const MIN_BAND_PX: f32 = 14.0;

/// Chevron pool bounds: arrow density has to stay readable, so it is clamped rather than proportional.
pub const MIN_CHEVRONS: usize = 3;
pub const MAX_CHEVRONS: usize = 8;

/// Spray cone base half-angle, in degrees, for a zero-orifice nozzle.
pub const SPRAY_BASE_HALF_ANGLE_DEG: f64 = 24.0;
/// Extra half-angle per millimetre of orifice diameter (a 20 mm nozzle -> 32 deg, a 40 mm -> 40 deg).
/// Invented: the nozzle records carry an orifice and a discharge coefficient, no spray angle.
pub const SPRAY_HALF_ANGLE_PER_MM_DEG: f64 = 0.4;

// ---- round 2: air/water line counts, and the 3D view's geometry constants -------------------------- //

/// Streamlines per side of the section (and inside the 3D cut cell): readability bounds, not a proportion.
pub const MIN_STREAMLINES: usize = 3;
pub const MAX_STREAMLINES: usize = 6;

/// Falling-water streaks through the spray + fill + rain path.
pub const MIN_WATER_STREAKS: usize = 4;
pub const MAX_WATER_STREAKS: usize = 14;

/// The hot-cold water spread (C) at which the falling water shows its full warm-to-cool walk. The engine
/// returns the two temperatures; the *look* between them is this pass's, so the reference spread is stated
/// here rather than buried in the draw code.
pub const WATER_RAMP_REFERENCE_C: f64 = 8.0;

/// How much of the warm end the falling water shows, from the run's own hot-cold spread: 0.35 (a nearly flat
/// run) .. 1.0 (a spread of [`WATER_RAMP_REFERENCE_C`] or more).
pub fn water_ramp(hot_c: f64, cold_c: f64) -> f32 {
    (((hot_c - cold_c) / WATER_RAMP_REFERENCE_C).clamp(0.55, 1.0)) as f32
}

/// The 3D view's opening camera: yaw/pitch in degrees, distance in tower widths.
pub const CAM_DEFAULT: (f32, f32, f32) = (38.0, 20.0, 2.6);
/// The orbit camera's limits (a still evidence frame needs a stated camera, and the limits are part of it).
pub const CAM_PITCH_RANGE: (f32, f32) = (2.0, 80.0);
pub const CAM_DIST_RANGE: (f32, f32) = (1.0, 12.0);

/// Drawn stack height, as a multiple of the stack diameter, in the 3D tower. The fixture carries a stack
/// **area** (`fan.stackAreaM2`) and a recovery factor, not a height - this is the pass inventing one.
pub const STACK_HEIGHT_FACTOR: f64 = 0.55;
/// Drawn thicknesses (m) of the parts the fixture geometry does not dimension.
pub const CUTAWAY_CASING_T_M: f64 = 0.12;
pub const DRIFT_BANK_T_M: f64 = 0.25;
pub const BASIN_DEPTH_M: f64 = 1.1;
/// The drawn plenum height, as a multiple of the cell plan, and the fan-deck thickness (m). The fixture
/// dimensions the plenum by a loss coefficient, not a height.
pub const PLENUM_HEIGHT_FACTOR: f64 = 0.12;
pub const DECK_T_M: f64 = 0.2;
/// How much of a cut cell is removed: exactly half, so the cut plane runs through the cell centre.
pub const CUT_FRACTION: f64 = 0.5;
/// Cell gap in the 3D row (m): a service lane between casings, so a row reads as cells, not as one block.
pub const CELL_GAP_M: f64 = 0.6;

// ---------------------------------------------------------------------------------------- the fan

/// The rpm the pass *shows* for a fan record at a speed ratio: the record's own rated speed
/// (`FanRecord.nominalRpm` — the speed its recorded curve is published at) times the ratio.
///
/// `None` when the record states no rated speed: the read-out then shows no rpm rather than a
/// number of the pass's own invention. Not a fixture value, not an engine value: the record's.
pub fn rpm(speed_ratio: f64, nominal_rpm: Option<f64>) -> Option<f64> {
    nominal_rpm.map(|nominal| (speed_ratio * nominal).max(0.0))
}

/// [`rpm`] as the read-outs spell it (`182`), or `-` when the record states no rated speed.
pub fn rpm_text(speed_ratio: f64, nominal_rpm: Option<f64>) -> String {
    match rpm(speed_ratio, nominal_rpm) {
        Some(rpm) => format!("{rpm:.0}"),
        None => "-".to_string(),
    }
}

/// The fan speed ratio behind a shown rpm (used by the rpm control, which is authored in rpm). The
/// rated speed is the record's own, so this is [`rpm`]'s exact inverse.
pub fn ratio_from_rpm(rpm: f64, nominal_rpm: f64) -> f64 {
    (rpm / nominal_rpm).max(0.0)
}

/// Blade rotation rate, in turns per second, for a shown rpm. One turn per minute-tenth: `rpm / 60`.
///
/// A real fan at 234 rpm is 3.9 turns/s, which strobes at 60 fps. The scene draws the *blade* rather than
/// the shaft, so it slows this by [`BLADE_VISUAL_SLOWDOWN`] to keep the rotation readable in an evidence
/// frame (a still frame of a 4 Hz rotation is meaningless).
pub fn blade_turn_hz(rpm: f64) -> f32 {
    ((rpm / 60.0) * BLADE_VISUAL_SLOWDOWN as f64) as f32
}

/// The slowdown applied to the drawn blade rotation. Documented because it is a *look*, not a speed.
pub const BLADE_VISUAL_SLOWDOWN: f32 = 0.16;

/// Airflow -> animation factor. 1.0 at the recorded run's airflow.
pub fn flow_factor(airflow_m3_s: f64) -> f32 {
    let f = airflow_m3_s / ANCHOR_AIRFLOW_M3_S;
    f.clamp(0.15, 2.2) as f32
}

/// Airflow -> how many chevrons a zone's flow channel draws.
pub fn chevron_count(airflow_m3_s: f64) -> usize {
    let n = (airflow_m3_s / 22.0).round() as i64;
    (n.clamp(MIN_CHEVRONS as i64, MAX_CHEVRONS as i64)) as usize
}

// ------------------------------------------------------------------------------------- the spray

/// Orifice diameter (m) -> spray cone half-angle (degrees). Invented; see the module docs.
pub fn spray_half_angle_deg(orifice_diameter_m: f64) -> f64 {
    let mm = (orifice_diameter_m * 1000.0).max(0.0);
    SPRAY_BASE_HALF_ANGLE_DEG + SPRAY_HALF_ANGLE_PER_MM_DEG * mm
}

/// Cone radius (m) at a given height below the nozzle.
pub fn spray_cone_radius_m(height_m: f64, half_angle_deg: f64) -> f64 {
    (height_m.max(0.0) * (half_angle_deg * PI / 180.0).tan()).max(0.0)
}

/// Fraction of the layer face two neighbouring cones cover between them, 0..1.
///
/// Flat-area overlap of two circles of `radius` whose centres are `spacing` apart: without a distribution
/// model this is the most that can honestly be said, and the label says `illustrative`.
pub fn coverage_fraction(spacing_m: f64, radius_m: f64) -> f64 {
    if spacing_m <= 0.0 {
        return 1.0;
    }
    ((2.0 * radius_m) / spacing_m).clamp(0.0, 1.0)
}

/// The nearest-neighbour pitch of an arrangement, in metres.
///
/// A staggered bank holds `sqrt(2)` times as many nozzles per unit area as a square grid at the same pitch,
/// so its equal-area square pitch is `pitch / sqrt(2)`. Illustrative - it changes the *illustration*, and
/// the panel says the distribution model is not implemented.
pub fn effective_pitch(spacing_m: f64, staggered: bool) -> f64 {
    if staggered {
        (spacing_m / std::f64::consts::SQRT_2).max(0.05)
    } else {
        spacing_m.max(0.05)
    }
}

/// Width of the drawn coverage band on a layer face (m).
pub fn coverage_width_m(spacing_m: f64, radius_m: f64) -> f64 {
    (2.0 * radius_m).min(spacing_m.max(0.0)).max(0.0)
}

/// How many nozzles the arrangement puts across a face of `width_m`. At least one, and integral.
pub fn nozzle_count(spacing_m: f64, width_m: f64) -> usize {
    if spacing_m <= 0.0 || width_m <= 0.0 {
        return 1;
    }
    (width_m / spacing_m).round().max(1.0) as usize
}

// ------------------------------------------------------------------------------------ the water

/// Circulating flow -> how many falling-water particles the rain zone draws.
pub fn rain_drop_count(water_flow_m3_hr: f64) -> usize {
    let n = (water_flow_m3_hr / 90.0).round() as i64;
    n.clamp(3, 9) as usize
}

// ------------------------------------------------------------------------------------ the rails

/// Where a value sits in a range, 0..1 (used by the operating-point marker and the rpm tachometer).
pub fn fraction_of(value: f64, lo: f64, hi: f64) -> f32 {
    if hi <= lo {
        return 0.0;
    }
    (((value - lo) / (hi - lo)) as f32).clamp(0.0, 1.0)
}

/// A zone's share of the total pressure, as a drawn height fraction. Floored so a 2 % zone is visible;
/// the floor is documented, not hidden.
pub fn rail_segment_fraction(share_pct: f64) -> f32 {
    ((share_pct / 100.0).max(0.012) as f32).clamp(0.0, 1.0)
}

/// The rpm range the control offers for a fan record: the record's `allowedSpeedRatio` band mapped
/// through its own rated speed. `None` when the record states no rated speed.
pub fn rpm_range(allowed_speed_ratio: [f64; 2], nominal_rpm: Option<f64>) -> Option<(f64, f64)> {
    let nominal = nominal_rpm?;
    let lo = allowed_speed_ratio[0] * nominal;
    let hi = allowed_speed_ratio[1] * nominal;
    Some((lo.min(hi).max(0.0), lo.max(hi).max(0.0)))
}

// --------------------------------------------------------------------------- round 2: the 3D tower

/// Airflow -> how many streamlines are drawn on **each side** of the section (and inside the cut cell).
/// The same readability clamp as `chevron_count`, so the two views can never disagree about density.
pub fn streamline_count(airflow_m3_s: f64) -> usize {
    let f = flow_factor(airflow_m3_s) as f64;
    let n = (MIN_STREAMLINES as f64
        + (MAX_STREAMLINES - MIN_STREAMLINES) as f64 * (f - 0.15) / 2.05)
        .round() as i64;
    (n.clamp(MIN_STREAMLINES as i64, MAX_STREAMLINES as i64)) as usize
}

/// Water flow -> how many falling streaks the water path draws.
pub fn water_streak_count(water_flow_m3_hr: f64) -> usize {
    let n = (water_flow_m3_hr / 70.0).round() as i64;
    n.clamp(MIN_WATER_STREAKS as i64, MAX_WATER_STREAKS as i64) as usize
}

/// The cell plan size the tower record implies: a square reading of `fillAreaM2`, in metres.
pub fn cell_plan_m(fill_area_m2: f64) -> f64 {
    fill_area_m2.max(1.0).sqrt()
}

/// The fan stack's diameter from the fitted fan's stack area (`4A / pi`), in metres.
pub fn stack_diameter_m(stack_area_m2: f64) -> f64 {
    (4.0 * stack_area_m2.max(0.01) / PI).sqrt()
}

/// The drawn stack height for a stack diameter (see [`STACK_HEIGHT_FACTOR`]).
pub fn stack_height_m(stack_diameter_m: f64) -> f64 {
    (stack_diameter_m.max(0.0) * STACK_HEIGHT_FACTOR).max(0.2)
}

/// The drawn width of a row of `cells` cells of `plan_m`, including the service lanes between casings.
pub fn cell_row_width_m(cells: u32, plan_m: f64) -> f64 {
    let n = cells.max(1) as f64;
    n * plan_m.max(0.1) + (n - 1.0).max(0.0) * CELL_GAP_M
}

/// Where cell `index` (0-based) sits along the row: its centre, in metres, row centred on zero.
pub fn cell_center_x_m(index: u32, cells: u32, plan_m: f64) -> f64 {
    let n = cells.max(1) as f64;
    let width = cell_row_width_m(cells, plan_m);
    let pitch = plan_m.max(0.1) + CELL_GAP_M;
    -width * 0.5 + plan_m.max(0.1) * 0.5 + index.min(n as u32) as f64 * pitch
}

// ================================================== round 4: the duty, the site and the evidence gate

/// The density the duty panel converts water flow with, exactly as the brief fixes it: 1000 kg/m3.
///
/// Note what this is *not*: the engine converts `waterMassFlowKgS` to `waterFlowM3Hr` at the water's own
/// density at the mean water temperature (`anchor.waterFlow.waterDensityKgM3`, 993.36 kg/m3 in this
/// fixture). The panel's editable field is the brief's m3/hr at 1000 kg/m3 and it says so, showing the
/// recorded pair beside the converted one - two stated conventions, never one silent number.
pub const DUTY_WATER_DENSITY_KG_M3: f64 = 1000.0;

/// kg/s -> m3/hr at [`DUTY_WATER_DENSITY_KG_M3`]. Volume flow is mass flow over density, times 3600.
pub fn m3_hr_from_kg_s(kg_s: f64) -> f64 {
    kg_s / DUTY_WATER_DENSITY_KG_M3 * 3600.0
}

/// m3/hr -> kg/s at [`DUTY_WATER_DENSITY_KG_M3`] (the inverse of [`m3_hr_from_kg_s`]).
pub fn kg_s_from_m3_hr(m3_hr: f64) -> f64 {
    m3_hr * DUTY_WATER_DENSITY_KG_M3 / 3600.0
}

/// ISA standard-atmosphere constants. The fixture carries a barometric pressure and no altitude, so the
/// panel's altitude field is derived from the pressure (and back) through this **definition** - a property
/// of the standard atmosphere, not a cooling-tower calculation and not an engine result.
pub const ISA_SEA_LEVEL_PA: f64 = 101_325.0;
pub const ISA_SEA_LEVEL_K: f64 = 288.15;
pub const ISA_LAPSE_K_PER_M: f64 = 0.0065;
/// g*M/(R*L) for the troposphere - the exponent of the ISA barometric relation.
pub const ISA_EXPONENT: f64 = 5.255_877_4;

/// Site altitude (m) for a barometric pressure (Pa), ISA troposphere.
pub fn altitude_m_from_pressure_pa(pressure_pa: f64) -> f64 {
    let ratio = (pressure_pa / ISA_SEA_LEVEL_PA).clamp(1e-6, 1.0);
    (ISA_SEA_LEVEL_K / ISA_LAPSE_K_PER_M) * (1.0 - ratio.powf(1.0 / ISA_EXPONENT))
}

/// Barometric pressure (Pa) for a site altitude (m), ISA troposphere (the inverse of the above).
pub fn pressure_pa_from_altitude_m(altitude_m: f64) -> f64 {
    let base = 1.0 - ISA_LAPSE_K_PER_M * altitude_m / ISA_SEA_LEVEL_K;
    ISA_SEA_LEVEL_PA * base.max(1e-3).powf(ISA_EXPONENT)
}

/// The approach margin the duty panel's own pre-check requires, in kelvin: entering cold water must clear
/// the entering wet bulb by at least this. The *fixture engine's* own rule is weaker (`approach > 0`);
/// the panel shows both, and never presents its rule as the engine's.
pub const APPROACH_MARGIN_MIN_C: f64 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm_is_the_records_own_rated_speed_times_the_ratio() {
        // AX-500's record states nominalRpm 233: the rated speed at which the recorded curve is
        // published, the 5.000 m stack (stackAreaM2 19.635) held at the cited 12 000 ft/min tip-speed
        // practice - the rule holds for the other fans too.
        let nominal = Some(233.0);
        for r in [0.7, 0.78, 1.0, 1.13] {
            assert!(
                (ratio_from_rpm(rpm(r, nominal).unwrap(), 233.0) - r).abs() < 1e-12,
                "round trip at {r}"
            );
        }
        assert!((rpm(0.78, nominal).unwrap() - 181.74).abs() < 1e-9);
        assert_eq!(rpm(1.0, nominal), Some(233.0));
        assert_eq!(rpm_text(0.78, nominal), "182");
    }

    #[test]
    fn a_record_without_a_rated_speed_shows_no_rpm() {
        // Never an invented number: no datum, no rpm, and no rpm control range.
        assert_eq!(rpm(0.78, None), None);
        assert_eq!(rpm_text(0.78, None), "-");
        assert_eq!(rpm_range([0.70, 1.13], None), None);
    }

    #[test]
    fn rpm_range_follows_the_records_own_band() {
        // catalog.fans[AX-500]: allowedSpeedRatio = [0.70, 1.13], nominalRpm 233.
        let (lo, hi) = rpm_range([0.70, 1.13], Some(233.0)).expect("a rated speed");
        assert!((lo - 163.1).abs() < 1e-9);
        assert!((hi - 263.29).abs() < 1e-9);
        // A record whose range is written the other way round still yields a usable slider.
        let (lo2, hi2) = rpm_range([1.13, 0.70], Some(233.0)).expect("a rated speed");
        assert!((lo2 - 163.1).abs() < 1e-9 && (hi2 - 263.29).abs() < 1e-9);
    }

    #[test]
    fn the_animation_factors_are_clamped_and_monotone() {
        assert!((flow_factor(ANCHOR_AIRFLOW_M3_S) - 1.0).abs() < 1e-6);
        assert!(flow_factor(1.0) >= 0.15);
        assert!(flow_factor(1.0e9) <= 2.2);
        assert!(flow_factor(80.0) < flow_factor(120.0));
        let counts: Vec<usize> = [20.0, 60.0, 125.0, 400.0]
            .iter()
            .map(|f| chevron_count(*f))
            .collect();
        assert!(
            counts.windows(2).all(|w| w[0] <= w[1]),
            "count is monotone: {counts:?}"
        );
        assert_eq!(counts[0], MIN_CHEVRONS);
        assert_eq!(counts[3], MAX_CHEVRONS);
    }

    #[test]
    fn the_blade_look_slows_the_shaft_and_stays_positive() {
        for r in [0.0, 0.7, 0.78, 1.13] {
            let shown = rpm(r, Some(233.0)).expect("a rated speed");
            let hz = blade_turn_hz(shown);
            assert!(hz >= 0.0);
            assert!(
                hz <= (shown / 60.0) as f32 + 1e-6,
                "the drawn blade never outruns the shaft"
            );
        }
        assert!(BLADE_VISUAL_SLOWDOWN > 0.0 && BLADE_VISUAL_SLOWDOWN <= 1.0);
    }

    #[test]
    fn the_spray_cone_is_invented_but_sane() {
        // NZ-20: 20 mm orifice -> 32 deg; NZ-40: 40 mm -> 40 deg.
        assert!((spray_half_angle_deg(0.020) - 32.0).abs() < 1e-9);
        assert!((spray_half_angle_deg(0.040) - 40.0).abs() < 1e-9);
        // Wider orifice -> wider cone; higher above the layer -> wider footprint.
        assert!(spray_half_angle_deg(0.032) > spray_half_angle_deg(0.020));
        assert!(spray_cone_radius_m(0.6, 32.0) > spray_cone_radius_m(0.3, 32.0));
        assert!(spray_cone_radius_m(0.6, 32.0) > 0.3);
    }

    #[test]
    fn coverage_cannot_exceed_one_and_grows_as_cones_get_closer() {
        let r = spray_cone_radius_m(0.5, 32.0);
        assert!(
            coverage_fraction(0.6, r) >= coverage_fraction(1.2, r),
            "closer nozzles cover more"
        );
        assert_eq!(coverage_fraction(0.01, r), 1.0, "fully overlapped");
        for spacing in [0.2, 0.5, 1.0, 2.0] {
            let c = coverage_fraction(spacing, r);
            assert!(
                (0.0..=1.0).contains(&c),
                "coverage {c} out of range at spacing {spacing}"
            );
        }
        assert_eq!(coverage_fraction(0.0, r), 1.0);
        assert!(coverage_width_m(1.0, 0.3) <= 1.0);
    }

    #[test]
    fn nozzle_count_and_rain_drops_stay_usable() {
        assert_eq!(nozzle_count(0.5, 5.0), 10);
        assert_eq!(nozzle_count(0.0, 5.0), 1);
        assert_eq!(nozzle_count(0.5, 0.0), 1);
        assert_eq!(rain_drop_count(724.8), 8);
        assert_eq!(rain_drop_count(0.0), 3);
        assert_eq!(rain_drop_count(1.0e9), 9);
    }

    #[test]
    fn a_staggered_bank_has_a_finer_effective_pitch() {
        assert!((effective_pitch(1.0, false) - 1.0).abs() < 1e-12);
        assert!(effective_pitch(1.0, true) < 1.0);
        let r = spray_cone_radius_m(0.6, 32.0);
        assert!(
            coverage_fraction(effective_pitch(1.0, true), r)
                >= coverage_fraction(effective_pitch(1.0, false), r),
            "staggering can only raise the covered share of the face"
        );
        assert!(effective_pitch(0.0, true) >= 0.05, "never degenerate");
    }

    #[test]
    fn rail_and_marker_fractions_are_bounded() {
        assert!((rail_segment_fraction(0.0) - 0.012).abs() < 1e-6);
        assert!((rail_segment_fraction(100.0) - 1.0).abs() < 1e-6);
        assert!((fraction_of(0.0, 0.0, 10.0) - 0.0).abs() < 1e-6);
        assert!((fraction_of(5.0, 0.0, 10.0) - 0.5).abs() < 1e-6);
        assert!((fraction_of(99.0, 0.0, 10.0) - 1.0).abs() < 1e-6);
        assert!(
            (fraction_of(1.0, 5.0, 5.0) - 0.0).abs() < 1e-6,
            "degenerate range"
        );
    }

    // ---- round 2: the streamlines, the water streaks and the 3D geometry --------------------------

    #[test]
    fn the_round_two_counts_are_bounded_and_monotone() {
        let streams: Vec<usize> = [20.0, 60.0, 125.0, 400.0]
            .iter()
            .map(|f| streamline_count(*f))
            .collect();
        assert!(streams.windows(2).all(|w| w[0] <= w[1]), "{streams:?}");
        assert_eq!(streams[0], MIN_STREAMLINES);
        assert_eq!(streams[3], MAX_STREAMLINES);
        let streaks: Vec<usize> = [50.0, 300.0, 725.0, 2000.0]
            .iter()
            .map(|f| water_streak_count(*f))
            .collect();
        assert!(streaks.windows(2).all(|w| w[0] <= w[1]), "{streaks:?}");
        assert_eq!(streaks[0], MIN_WATER_STREAKS);
        assert_eq!(streaks[3], MAX_WATER_STREAKS);
    }

    #[test]
    fn the_three_d_geometry_follows_the_fixture_records() {
        // IDCF-064: fillAreaM2 64 -> an 8.00 m cell plan. AX-500: stackAreaM2 19.635 -> a 5.00 m stack.
        assert!((cell_plan_m(64.0) - 8.0).abs() < 1e-9);
        assert!((stack_diameter_m(19.635) - 5.0).abs() < 1e-3);
        // AX-420 is the fixture's "4.2 m axial fan": stackAreaM2 13.854.
        assert!((stack_diameter_m(13.854) - 4.2).abs() < 1e-2);
        let d = stack_diameter_m(19.635);
        assert!(
            stack_height_m(d) > 2.0 && stack_height_m(d) < d,
            "a recovery stack, not a chimney"
        );
        assert!(cell_row_width_m(4, 8.0) > cell_row_width_m(2, 8.0));
        let w = cell_row_width_m(2, 8.0);
        assert!(
            (cell_center_x_m(0, 2, 8.0) + w * 0.5 - 4.0).abs() < 1e-9,
            "the first cell's centre sits half a plan in from the row's left edge"
        );
        assert!(cell_center_x_m(1, 2, 8.0) > cell_center_x_m(0, 2, 8.0));
        assert!(
            cell_center_x_m(0, 1, 8.0).abs() < 1e-9,
            "a single cell is centred"
        );
        // an 8-cell row: the last cell's centre sits half a plan inside the row's right edge
        let w8 = cell_row_width_m(8, 8.0);
        assert!((cell_center_x_m(7, 8, 8.0) - w8 * 0.5 + 4.0).abs() < 1e-9);
        assert!(cell_center_x_m(7, 8, 8.0) < w8 * 0.5);
    }

    #[test]
    fn the_camera_and_cut_defaults_are_inside_their_limits() {
        assert!(CAM_DEFAULT.1 >= CAM_PITCH_RANGE.0 && CAM_DEFAULT.1 <= CAM_PITCH_RANGE.1);
        assert!(CAM_DEFAULT.2 >= CAM_DIST_RANGE.0 && CAM_DEFAULT.2 <= CAM_DIST_RANGE.1);
        assert!(CAM_DEFAULT.2 > 1.5, "far enough to see a two-cell row");
        assert!((0.0..360.0).contains(&CAM_DEFAULT.0));
        assert!(CUT_FRACTION > 0.0 && CUT_FRACTION < 1.0);
        assert!(DRIFT_BANK_T_M > 0.0 && BASIN_DEPTH_M > 0.0 && CUTAWAY_CASING_T_M > 0.0);
    }

    // ---- round 4: the duty conversion and the site definition --------------------------------------

    #[test]
    fn the_duty_flow_conversion_round_trips_at_the_briefs_density() {
        // The recorded duty: 200 kg/s at 1000 kg/m3 is 720.00 m3/hr (the fixture's own pair, 200 kg/s and
        // 724.81 m3/hr, uses the engine's water density of 993.36 kg/m3 - a different, stated convention).
        assert!((m3_hr_from_kg_s(200.0) - 720.0).abs() < 1e-9);
        assert!((kg_s_from_m3_hr(720.0) - 200.0).abs() < 1e-12);
        for kg_s in [0.0, 1.0, 139.07, 200.0, 265.0] {
            assert!(
                (kg_s_from_m3_hr(m3_hr_from_kg_s(kg_s)) - kg_s).abs() < 1e-9,
                "round trip at {kg_s}"
            );
        }
        assert!(m3_hr_from_kg_s(0.0).abs() < 1e-12);
        assert_eq!(DUTY_WATER_DENSITY_KG_M3, 1000.0);
    }

    #[test]
    fn the_site_altitude_definition_matches_the_standard_atmosphere() {
        // Sea level is the definition's own fixpoint, and the ISA table's round numbers either side of it.
        assert!((altitude_m_from_pressure_pa(ISA_SEA_LEVEL_PA) - 0.0).abs() < 1e-6);
        assert!((pressure_pa_from_altitude_m(0.0) - ISA_SEA_LEVEL_PA).abs() < 1e-6);
        assert!(
            (pressure_pa_from_altitude_m(1000.0) - 89_875.0).abs() < 5.0,
            "1000 m is 89875 Pa in the ISA table, got {}",
            pressure_pa_from_altitude_m(1000.0)
        );
        assert!(
            (altitude_m_from_pressure_pa(89_875.0) - 1000.0).abs() < 2.0,
            "inverse at 1000 m"
        );
        // Monotone, and the round trip holds far above sea level too.
        assert!(altitude_m_from_pressure_pa(90_000.0) > altitude_m_from_pressure_pa(100_000.0));
        for z in [0.0, 250.0, 1000.0, 2000.0, 3000.0] {
            let back = altitude_m_from_pressure_pa(pressure_pa_from_altitude_m(z));
            assert!((back - z).abs() < 1e-3, "round trip at {z} m -> {back}");
        }
    }

    #[test]
    fn the_approach_margin_is_a_stated_panel_rule() {
        assert!(APPROACH_MARGIN_MIN_C > 0.0);
        assert!(
            APPROACH_MARGIN_MIN_C >= 0.5,
            "the brief's example minimum: cold water clears the wet bulb by at least 0.5 C"
        );
    }
}
