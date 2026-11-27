//! Performance-curve handling — port of `src/core/performanceCurve.js`.
//!
//! A manufacturer's curve set on a rectangular wet-bulb × range grid: bilinear interpolation
//! of the cold-water temperature at a test point, the inverse solve for the water flow that
//! would have produced a given cold-water temperature, and the capability ratio those two
//! give a test run.
//!
//! The JavaScript engine is the specification. Mirrored here as it is there: the 1e-9
//! wet-bulb/range corner match, the `interpolate1D` call with `clampEnds: false` (the corner
//! table extrapolates from its end points rather than clamping), the collapse of a degenerate
//! axis to a zero interpolation fraction, the 8-record floor on the bounds, and the inverse
//! solve's 400-sample scan with `prefer: 'first'` — where a corner the grid cannot complete
//! propagates out of the scan as the reference's `DomainError` does.

use crate::numeric::{
    bilinear, bracket_values, find_root_by_scan, interpolate_1d, js_max, js_min, DomainError,
    RootPreference, RootScanOptions,
};

/// The reference's `recordsAt` tolerance: a corner matches when both coordinates are within
/// 1e-9.
const CORNER_TOLERANCE: f64 = 1e-9;

/// One performance-curve record — one point of the manufacturer's grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceCurveRecord {
    pub wet_bulb_c: f64,
    pub range_c: f64,
    pub water_flow_kg_s: f64,
    pub cold_water_c: f64,
}

/// The object `performanceCurveBounds` returns: the four axis ranges over the record set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceCurveBounds {
    pub wet_bulb_c: (f64, f64),
    pub range_c: (f64, f64),
    pub water_flow_kg_s: (f64, f64),
    pub cold_water_c: (f64, f64),
}

/// The object `predictColdWaterFromPerformanceCurves` returns (the brackets travel as the
/// reference's two-element lists, as tuples here).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColdWaterPrediction {
    pub cold_water_c: f64,
    pub wet_bulb_bracket_c: (f64, f64),
    pub range_bracket_c: (f64, f64),
    pub extrapolated: bool,
}

/// The object `predictWaterFlowFromPerformanceCurves` returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterFlowPrediction {
    pub water_flow_kg_s: f64,
    pub predicted_cold_water_c: f64,
}

/// The object `evaluatePerformanceCurveCapability` returns (its `method`, `signConvention` and
/// `disclaimer` strings travel with the result the CLI renders).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceCurveCapability {
    pub predicted_water_flow_kg_s: f64,
    pub adjusted_test_water_flow_kg_s: f64,
    pub capability_pct: f64,
    pub predicted_cold_water_at_adjusted_flow_c: f64,
    pub leaving_water_deviation_c: f64,
}

/// Port of `recordsAt`: every record at one grid corner, within 1e-9 on both coordinates.
fn records_at(
    records: &[PerformanceCurveRecord],
    wet_bulb_c: f64,
    range_c: f64,
) -> Vec<PerformanceCurveRecord> {
    records
        .iter()
        .copied()
        .filter(|record| {
            (record.wet_bulb_c - wet_bulb_c).abs() < CORNER_TOLERANCE
                && (record.range_c - range_c).abs() < CORNER_TOLERANCE
        })
        .collect()
}

/// Port of `interpolateCorner`: the corner's cold-water curve as a function of water flow,
/// extrapolated (never clamped) beyond the ends.
fn interpolate_corner(
    records: &[PerformanceCurveRecord],
    wet_bulb_c: f64,
    range_c: f64,
    water_flow_kg_s: f64,
) -> Result<f64, DomainError> {
    let corner = records_at(records, wet_bulb_c, range_c);
    if corner.len() < 2 {
        return Err(DomainError::new(
            "Performance-curve grid is incomplete at a wet-bulb/range corner.",
        ));
    }
    interpolate_1d(
        &corner,
        water_flow_kg_s,
        |record| record.water_flow_kg_s,
        |record| record.cold_water_c,
        false,
    )
}

/// Port of `performanceCurveBounds`: the record set's four axis ranges. Fewer than eight
/// records is refused, as in the reference.
pub fn performance_curve_bounds(
    records: &[PerformanceCurveRecord],
) -> Result<PerformanceCurveBounds, DomainError> {
    if records.len() < 8 {
        return Err(DomainError::new(
            "Performance curve requires a populated rectangular record set.",
        ));
    }
    let range_over = |axis: fn(&PerformanceCurveRecord) -> f64| -> (f64, f64) {
        records
            .iter()
            .map(axis)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), value| {
                (js_min(lo, value), js_max(hi, value))
            })
    };
    Ok(PerformanceCurveBounds {
        wet_bulb_c: range_over(|record| record.wet_bulb_c),
        range_c: range_over(|record| record.range_c),
        water_flow_kg_s: range_over(|record| record.water_flow_kg_s),
        cold_water_c: range_over(|record| record.cold_water_c),
    })
}

/// Port of the reference's `inverseLerp`: a degenerate interval maps to 0 (which is why the
/// call site collapses it before calling).
fn inverse_lerp(a: f64, b: f64, value: f64) -> f64 {
    if a == b {
        0.0
    } else {
        (value - a) / (b - a)
    }
}

/// Port of `predictColdWaterFromPerformanceCurves`: bracket the test point on both axes,
/// interpolate the four corner curves, and blend them bilinearly.
pub fn predict_cold_water_from_performance_curves(
    records: &[PerformanceCurveRecord],
    wet_bulb_c: f64,
    range_c: f64,
    water_flow_kg_s: f64,
) -> Result<ColdWaterPrediction, DomainError> {
    let wet_bulbs: Vec<f64> = records.iter().map(|record| record.wet_bulb_c).collect();
    let ranges: Vec<f64> = records.iter().map(|record| record.range_c).collect();
    let (wet_bulb_lo, wet_bulb_hi) = bracket_values(&wet_bulbs, wet_bulb_c)?;
    let (range_lo, range_hi) = bracket_values(&ranges, range_c)?;

    let q11 = interpolate_corner(records, wet_bulb_lo, range_lo, water_flow_kg_s)?;
    let q21 = interpolate_corner(records, wet_bulb_hi, range_lo, water_flow_kg_s)?;
    let q12 = interpolate_corner(records, wet_bulb_lo, range_hi, water_flow_kg_s)?;
    let q22 = interpolate_corner(records, wet_bulb_hi, range_hi, water_flow_kg_s)?;
    let tx = if wet_bulb_lo == wet_bulb_hi {
        0.0
    } else {
        inverse_lerp(wet_bulb_lo, wet_bulb_hi, wet_bulb_c)
    };
    let ty = if range_lo == range_hi {
        0.0
    } else {
        inverse_lerp(range_lo, range_hi, range_c)
    };
    let cold_water_c = bilinear(q11, q21, q12, q22, tx, ty);

    let wet_bulb_min = wet_bulbs.iter().copied().fold(f64::INFINITY, js_min);
    let wet_bulb_max = wet_bulbs.iter().copied().fold(f64::NEG_INFINITY, js_max);
    let range_min = ranges.iter().copied().fold(f64::INFINITY, js_min);
    let range_max = ranges.iter().copied().fold(f64::NEG_INFINITY, js_max);
    Ok(ColdWaterPrediction {
        cold_water_c,
        wet_bulb_bracket_c: (wet_bulb_lo, wet_bulb_hi),
        range_bracket_c: (range_lo, range_hi),
        extrapolated: wet_bulb_c < wet_bulb_min
            || wet_bulb_c > wet_bulb_max
            || range_c < range_min
            || range_c > range_max,
    })
}

/// Port of `predictWaterFlowFromPerformanceCurves`: solve the water flow whose curve
/// prediction matches the given cold-water temperature, then report that prediction.
pub fn predict_water_flow_from_performance_curves(
    records: &[PerformanceCurveRecord],
    wet_bulb_c: f64,
    range_c: f64,
    cold_water_c: f64,
) -> Result<WaterFlowPrediction, DomainError> {
    let bounds = performance_curve_bounds(records)?.water_flow_kg_s;
    let residual = |water_flow_kg_s: f64| -> Result<f64, DomainError> {
        Ok(predict_cold_water_from_performance_curves(
            records,
            wet_bulb_c,
            range_c,
            water_flow_kg_s,
        )?
        .cold_water_c
            - cold_water_c)
    };
    let water_flow_kg_s = find_root_by_scan(
        residual,
        bounds.0,
        bounds.1,
        RootScanOptions {
            samples: 400,
            tolerance: 1e-7,
            prefer: RootPreference::First,
            ..RootScanOptions::default()
        },
    )?;
    let predicted_cold_water_c =
        predict_cold_water_from_performance_curves(records, wet_bulb_c, range_c, water_flow_kg_s)?
            .cold_water_c;
    Ok(WaterFlowPrediction {
        water_flow_kg_s,
        predicted_cold_water_c,
    })
}

/// Port of `evaluatePerformanceCurveCapability`: the test's adjusted flow over the flow the
/// curve set implies, and the leaving-water deviation against the curve prediction at that
/// flow.
pub fn evaluate_performance_curve_capability(
    records: &[PerformanceCurveRecord],
    test_wet_bulb_c: f64,
    test_range_c: f64,
    test_cold_water_c: f64,
    adjusted_test_water_flow_kg_s: f64,
) -> Result<PerformanceCurveCapability, DomainError> {
    let predicted = predict_water_flow_from_performance_curves(
        records,
        test_wet_bulb_c,
        test_range_c,
        test_cold_water_c,
    )?;
    let capability_pct = 100.0 * adjusted_test_water_flow_kg_s / predicted.water_flow_kg_s;
    let predicted_cold_water_at_adjusted_flow_c = predict_cold_water_from_performance_curves(
        records,
        test_wet_bulb_c,
        test_range_c,
        adjusted_test_water_flow_kg_s,
    )?
    .cold_water_c;
    Ok(PerformanceCurveCapability {
        predicted_water_flow_kg_s: predicted.water_flow_kg_s,
        adjusted_test_water_flow_kg_s,
        capability_pct,
        predicted_cold_water_at_adjusted_flow_c,
        leaving_water_deviation_c: test_cold_water_c - predicted_cold_water_at_adjusted_flow_c,
    })
}
