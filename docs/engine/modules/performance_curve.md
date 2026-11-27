# `drafthouse-engine` · `performance_curve`

Performance-curve handling — port of `src/core/performanceCurve.js`.

A manufacturer's curve set on a rectangular wet-bulb × range grid: bilinear interpolation
of the cold-water temperature at a test point, the inverse solve for the water flow that
would have produced a given cold-water temperature, and the capability ratio those two
give a test run.

The JavaScript engine is the specification. Mirrored here as it is there: the 1e-9
wet-bulb/range corner match, the `interpolate1D` call with `clampEnds: false` (the corner
table extrapolates from its end points rather than clamping), the collapse of a degenerate
axis to a zero interpolation fraction, the 8-record floor on the bounds, and the inverse
solve's 400-sample scan with `prefer: 'first'` — where a corner the grid cannot complete
propagates out of the scan as the reference's `DomainError` does.

## Notes carried from the source comments

### `const CORNER_TOLERANCE`

The reference's `recordsAt` tolerance: a corner matches when both coordinates are within
1e-9.

### `struct PerformanceCurveRecord`

One performance-curve record — one point of the manufacturer's grid.

### `struct PerformanceCurveBounds`

The object `performanceCurveBounds` returns: the four axis ranges over the record set.

### `struct ColdWaterPrediction`

The object `predictColdWaterFromPerformanceCurves` returns (the brackets travel as the
reference's two-element lists, as tuples here).

### `struct WaterFlowPrediction`

The object `predictWaterFlowFromPerformanceCurves` returns.

### `struct PerformanceCurveCapability`

The object `evaluatePerformanceCurveCapability` returns (its `method`, `signConvention` and
`disclaimer` strings travel with the result the CLI renders).

### `fn records_at`

Port of `recordsAt`: every record at one grid corner, within 1e-9 on both coordinates.

### `fn interpolate_corner`

Port of `interpolateCorner`: the corner's cold-water curve as a function of water flow,
extrapolated (never clamped) beyond the ends.

### `fn performance_curve_bounds`

Port of `performanceCurveBounds`: the record set's four axis ranges. Fewer than eight
records is refused, as in the reference.

### `fn inverse_lerp`

Port of the reference's `inverseLerp`: a degenerate interval maps to 0 (which is why the
call site collapses it before calling).

### `fn predict_cold_water_from_performance_curves`

Port of `predictColdWaterFromPerformanceCurves`: bracket the test point on both axes,
interpolate the four corner curves, and blend them bilinearly.

### `fn predict_water_flow_from_performance_curves`

Port of `predictWaterFlowFromPerformanceCurves`: solve the water flow whose curve
prediction matches the given cold-water temperature, then report that prediction.

### `fn evaluate_performance_curve_capability`

Port of `evaluatePerformanceCurveCapability`: the test's adjusted flow over the flow the
curve set implies, and the leaving-water deviation against the curve prediction at that
flow.
