# `engine` tests · `performance_curve`

Performance-curve handling — the JavaScript suite's expectations for
`src/core/performanceCurve.js`, ported test for test.

Sources, quoted per test:

* `tests/performanceCurve.test.js` — an exact grid point interpolates, the inverse solve
  recovers the flow, and a matching test point evaluates to 100 % capability. The grid is
  `src/data/samplePerformanceCurves.js`, quoted in `sample_grid` below.
* `validation/test-vectors.json` (family `performanceCurve`) — the two recorded cases are
  replayed in `rust/tests/vectors.rs`.
* `src/core/performanceCurve.js` — the 1e-9 corner tolerance, the `clampEnds: false`
  extrapolation, the 8-record bounds floor, the 400-sample inverse scan and both refusal
  messages.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `fn sample_grid`

`samplePerformanceCurveRecords` — the 45-point grid of
`src/data/samplePerformanceCurves.js`, cold-water values included (the JavaScript engine's
own output for the generator's formula).

### `fn an_exact_grid_point_interpolates_and_the_inverse_solve_recovers_the_flow`

tests/performanceCurve.test.js: `|coldWaterC - 32.8| < 1e-9` at (27, 10, 200), and
`|waterFlowKgS - 200| < 1e-6` for the inverse solve against that same temperature.

`bracketValues` returns the FIRST pair that contains the target, and an exact member is
contained by its left pair: a test point on the grid line interpolates to that line, it
does not collapse the bracket (only a target at or beyond an end does).

### `fn a_matching_test_point_evaluates_to_one_hundred_percent_capability`

tests/performanceCurve.test.js: both the ratio and the leaving-water deviation are 0.

### `fn a_mid_cell_point_brackets_on_both_axes`

src/core/performanceCurve.js: `bracketValues` collapses to the ends at or beyond them,
and interpolates the four corner curves in between.

The four corner curves bracket the blended value: it never leaves their envelope.

### `fn a_point_outside_the_grid_is_flagged_and_extrapolated_not_clamped`

src/core/performanceCurve.js: the corner table is read with `clampEnds: false`, so a flow
beyond the table continues the end pair's line, and `extrapolated` reports the grid flag.

The reference's flag reports the wet-bulb/range axes only; a flow beyond the table is
still extrapolated (the value keeps rising) without raising the flag.

### `fn an_incomplete_corner_is_refused_naming_the_corner`

src/core/performanceCurve.js: `interpolateCorner` refuses when fewer than two records
carry the corner.

### `fn a_record_set_below_the_grid_floor_is_refused`

src/core/performanceCurve.js: `records.length < 8` refuses before any interpolation.

Eight records are enough for the bounds (but not for every corner: a separate refusal).

### `fn the_corner_match_tolerance_is_the_reference_onee_nine`

`recordsAt` matches when both coordinates are within 1e-9 (strictly less than), so a
record 5e-10 away is the corner and one 2e-9 away is not.

### `fn the_inverse_scan_stays_inside_the_flow_bounds_it_reports`

The reference solves over `performanceCurveBounds(records).waterFlowKgS`; a temperature
outside what the grid can produce leaves the bracket empty and is refused.
