//! Performance-curve handling — the JavaScript suite's expectations for
//! `src/core/performanceCurve.js`, ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `tests/performanceCurve.test.js` — an exact grid point interpolates, the inverse solve
//!   recovers the flow, and a matching test point evaluates to 100 % capability. The grid is
//!   `src/data/samplePerformanceCurves.js`, quoted in `sample_grid` below.
//! * `validation/test-vectors.json` (family `performanceCurve`) — the two recorded cases are
//!   replayed in `rust/tests/vectors.rs`.
//! * `src/core/performanceCurve.js` — the 1e-9 corner tolerance, the `clampEnds: false`
//!   extrapolation, the 8-record bounds floor, the 400-sample inverse scan and both refusal
//!   messages.
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    evaluate_performance_curve_capability, performance_curve_bounds,
    predict_cold_water_from_performance_curves, predict_water_flow_from_performance_curves,
    PerformanceCurveRecord,
};

/// `samplePerformanceCurveRecords` — the 45-point grid of
/// `src/data/samplePerformanceCurves.js`, cold-water values included (the JavaScript engine's
/// own output for the generator's formula).
fn sample_grid() -> Vec<PerformanceCurveRecord> {
    let mut records = Vec::new();
    for wet_bulb_c in [24.0_f64, 27.0, 30.0] {
        for range_c in [8.0_f64, 10.0, 12.0] {
            for water_flow_kg_s in [120.0_f64, 160.0, 200.0, 240.0, 280.0] {
                let flow_ratio = water_flow_kg_s / 200.0;
                let approach_c = 3.15
                    + 2.65 * flow_ratio.powf(1.45)
                    + 0.11 * (range_c - 10.0)
                    + 0.012 * (wet_bulb_c - 27.0).powi(2);
                records.push(PerformanceCurveRecord {
                    wet_bulb_c,
                    range_c,
                    water_flow_kg_s,
                    cold_water_c: wet_bulb_c + approach_c,
                });
            }
        }
    }
    records
}

#[test]
fn an_exact_grid_point_interpolates_and_the_inverse_solve_recovers_the_flow() {
    // tests/performanceCurve.test.js: `|coldWaterC - 32.8| < 1e-9` at (27, 10, 200), and
    // `|waterFlowKgS - 200| < 1e-6` for the inverse solve against that same temperature.
    let records = sample_grid();
    let direct = predict_cold_water_from_performance_curves(&records, 27.0, 10.0, 200.0)
        .expect("interpolates");
    assert!(
        (direct.cold_water_c - 32.8).abs() < 1e-9,
        "coldWaterC {}",
        direct.cold_water_c
    );
    assert!(!direct.extrapolated);
    // `bracketValues` returns the FIRST pair that contains the target, and an exact member is
    // contained by its left pair: a test point on the grid line interpolates to that line, it
    // does not collapse the bracket (only a target at or beyond an end does).
    assert_eq!(direct.wet_bulb_bracket_c, (24.0, 27.0));
    assert_eq!(direct.range_bracket_c, (8.0, 10.0));

    let inverse =
        predict_water_flow_from_performance_curves(&records, 27.0, 10.0, 32.8).expect("solves");
    assert!(
        (inverse.water_flow_kg_s - 200.0).abs() < 1e-6,
        "waterFlowKgS {}",
        inverse.water_flow_kg_s
    );
    assert!((inverse.predicted_cold_water_c - 32.8).abs() < 1e-9);
}

#[test]
fn a_matching_test_point_evaluates_to_one_hundred_percent_capability() {
    // tests/performanceCurve.test.js: both the ratio and the leaving-water deviation are 0.
    let records = sample_grid();
    let result = evaluate_performance_curve_capability(&records, 27.0, 10.0, 32.8, 200.0)
        .expect("evaluates");
    assert!(
        (result.capability_pct - 100.0).abs() < 1e-9,
        "{}",
        result.capability_pct
    );
    assert!(result.leaving_water_deviation_c.abs() < 1e-9);
    assert!((result.predicted_water_flow_kg_s - 200.0).abs() < 1e-6);
}

#[test]
fn a_mid_cell_point_brackets_on_both_axes() {
    // src/core/performanceCurve.js: `bracketValues` collapses to the ends at or beyond them,
    // and interpolates the four corner curves in between.
    let records = sample_grid();
    let result = predict_cold_water_from_performance_curves(&records, 25.5, 11.0, 180.0)
        .expect("interpolates");
    assert_eq!(result.wet_bulb_bracket_c, (24.0, 27.0));
    assert_eq!(result.range_bracket_c, (10.0, 12.0));
    assert!(!result.extrapolated);
    // The four corner curves bracket the blended value: it never leaves their envelope.
    let corners = [
        predict_cold_water_from_performance_curves(&records, 24.0, 10.0, 180.0)
            .expect("corner")
            .cold_water_c,
        predict_cold_water_from_performance_curves(&records, 27.0, 10.0, 180.0)
            .expect("corner")
            .cold_water_c,
        predict_cold_water_from_performance_curves(&records, 24.0, 12.0, 180.0)
            .expect("corner")
            .cold_water_c,
        predict_cold_water_from_performance_curves(&records, 27.0, 12.0, 180.0)
            .expect("corner")
            .cold_water_c,
    ];
    let smallest = corners.iter().copied().fold(f64::INFINITY, f64::min);
    let largest = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(result.cold_water_c >= smallest && result.cold_water_c <= largest);
}

#[test]
fn a_point_outside_the_grid_is_flagged_and_extrapolated_not_clamped() {
    // src/core/performanceCurve.js: the corner table is read with `clampEnds: false`, so a flow
    // beyond the table continues the end pair's line, and `extrapolated` reports the grid flag.
    let records = sample_grid();
    let inside = predict_cold_water_from_performance_curves(&records, 27.0, 10.0, 280.0)
        .expect("interpolates");
    let beyond = predict_cold_water_from_performance_curves(&records, 27.0, 10.0, 400.0)
        .expect("extrapolates");
    // The reference's flag reports the wet-bulb/range axes only; a flow beyond the table is
    // still extrapolated (the value keeps rising) without raising the flag.
    assert!(!inside.extrapolated);
    assert!(!beyond.extrapolated);
    assert!(beyond.cold_water_c > inside.cold_water_c);

    let off_grid = predict_cold_water_from_performance_curves(&records, 33.0, 6.0, 140.0)
        .expect("extrapolates");
    assert!(off_grid.extrapolated);
    assert_eq!(off_grid.wet_bulb_bracket_c, (30.0, 30.0));
    assert_eq!(off_grid.range_bracket_c, (8.0, 8.0));
}

#[test]
fn an_incomplete_corner_is_refused_naming_the_corner() {
    // src/core/performanceCurve.js: `interpolateCorner` refuses when fewer than two records
    // carry the corner.
    let records: Vec<PerformanceCurveRecord> = sample_grid()
        .into_iter()
        .filter(|record| !(record.wet_bulb_c == 24.0 && record.range_c == 10.0))
        .collect();
    let error = predict_cold_water_from_performance_curves(&records, 25.5, 11.0, 180.0)
        .expect_err("must refuse");
    assert_eq!(
        error.message(),
        "Performance-curve grid is incomplete at a wet-bulb/range corner."
    );
}

#[test]
fn a_record_set_below_the_grid_floor_is_refused() {
    // src/core/performanceCurve.js: `records.length < 8` refuses before any interpolation.
    for count in [0_usize, 1, 7] {
        let records: Vec<PerformanceCurveRecord> = sample_grid().into_iter().take(count).collect();
        let error = performance_curve_bounds(&records).expect_err("must refuse");
        assert_eq!(
            error.message(),
            "Performance curve requires a populated rectangular record set."
        );
    }
    // Eight records are enough for the bounds (but not for every corner: a separate refusal).
    let eight: Vec<PerformanceCurveRecord> = sample_grid().into_iter().take(8).collect();
    assert!(performance_curve_bounds(&eight).is_ok());
}

#[test]
fn the_bounds_report_the_axis_ranges_of_the_grid() {
    let bounds = performance_curve_bounds(&sample_grid()).expect("bounds");
    assert_eq!(bounds.wet_bulb_c, (24.0, 30.0));
    assert_eq!(bounds.range_c, (8.0, 12.0));
    assert_eq!(bounds.water_flow_kg_s, (120.0, 280.0));
    let cold_min = sample_grid()
        .iter()
        .map(|record| record.cold_water_c)
        .fold(f64::INFINITY, f64::min);
    let cold_max = sample_grid()
        .iter()
        .map(|record| record.cold_water_c)
        .fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(bounds.cold_water_c, (cold_min, cold_max));
}

#[test]
fn the_corner_match_tolerance_is_the_reference_onee_nine() {
    // `recordsAt` matches when both coordinates are within 1e-9 (strictly less than), so a
    // record 5e-10 away is the corner and one 2e-9 away is not.
    let grid = sample_grid();
    let mut nudged = grid.clone();
    for record in nudged.iter_mut() {
        if record.wet_bulb_c == 24.0 && record.range_c == 10.0 {
            record.wet_bulb_c += 5e-10;
        }
    }
    let matched = predict_cold_water_from_performance_curves(&nudged, 24.0, 10.0, 180.0)
        .expect("still a corner");
    let exact =
        predict_cold_water_from_performance_curves(&grid, 24.0, 10.0, 180.0).expect("corner");
    assert!((matched.cold_water_c - exact.cold_water_c).abs() < 1e-6);

    let mut missed = grid;
    for record in missed.iter_mut() {
        if record.wet_bulb_c == 24.0 && record.range_c == 10.0 {
            record.wet_bulb_c += 2e-9;
        }
    }
    let error = predict_cold_water_from_performance_curves(&missed, 24.0, 10.0, 180.0)
        .expect_err("the nudged record is no longer the corner");
    assert_eq!(
        error.message(),
        "Performance-curve grid is incomplete at a wet-bulb/range corner."
    );
}

#[test]
fn the_inverse_scan_stays_inside_the_flow_bounds_it_reports() {
    // The reference solves over `performanceCurveBounds(records).waterFlowKgS`; a temperature
    // outside what the grid can produce leaves the bracket empty and is refused.
    let records = sample_grid();
    let error = predict_water_flow_from_performance_curves(&records, 27.0, 10.0, -50.0)
        .expect_err("must refuse");
    assert_eq!(
        error.message(),
        "No bracketed root was found in the requested interval."
    );
    let solved =
        predict_water_flow_from_performance_curves(&records, 27.0, 10.0, 34.0).expect("solves");
    assert!(solved.water_flow_kg_s >= 120.0 && solved.water_flow_kg_s <= 280.0);
}
