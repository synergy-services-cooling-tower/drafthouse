# `engine` tests · `fan`

Fans — the JavaScript suite's expectations for `src/core/fan.js`, ported test for test.

Sources, quoted per test:

* `tests/airside.test.js` — the balanced-operating-point expectation (the fan/system
  solver is exercised there, not in a file of its own).
* `validation/test-vectors.json` (family `fan`) — the three recorded operating points,
  reproduced against the same `35 + 0.0085 Q²` system curve and 1.14 kg/m³ air,
  at the same 1e-6 relative tolerance `tests/vectors.test.js` uses.
* `src/core/fan.js` — the formulas, defaults (`speedRatio: 1`,
  `minimumFlowFraction: 0.05`, the IEC `standardSizesKW` table) and guard messages.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `fn ax_500`

`sampleCatalog.fans` / `AX-500` (illustrative synthetic data, quoted verbatim).

### `fn documented_system_curve`

The `35 + 0.0085 Q²` system curve of `tests/airside.test.js` and the recorded vectors.

### `fn agrees`

/* ---------------- recorded vectors ---------------- */

### `fn the_recorded_fan_vectors_reproduce`

`validation/test-vectors.json`, family `fan`.

The reference reports the residual at the returned point; it must vanish.

/* ---------------- tests/airside.test.js, ported ---------------- */

### `fn the_fan_system_solver_returns_a_balanced_positive_operating_point`

/* ---------------- curve evaluation ---------------- */

### `fn a_curve_reading_uses_the_density_ratio_and_the_square_of_the_speed_ratio`

Values read from the reference at this point (tests/airside.test.js uses the same call).

The reference reads the curve at flow / speedRatio and scales by the affinity laws.

### `fn an_efficiency_reading_clamps_at_the_curve_ends`

Values read from the reference. 130/0.9 = 144.44… m³/s falls between the 100 and 145
points; a flow beyond the curve clamps to the last efficiency rather than extrapolating
(the pressure curve extrapolates — `clampEnds: false` — the efficiency curve does not).

### `fn shaft_power_is_flow_times_pressure_over_efficiency`

/* ---------------- affinity estimate and motor selection ---------------- */

### `fn an_airflow_estimate_follows_the_affinity_law`

Value read from the reference.

Same power and density is the identity; more power at the same density raises the flow.

### `fn motor_selection_walks_the_iec_size_table`

Values read from the reference (`chooseStandardMotor` defaults).

Above the largest standard size the reference returns null (`?? null`).

The chosen size always covers the requirement, and the table is the reference's.

/* ---------------- refusal behaviour ---------------- */

### `fn a_non_positive_reference_density_speed_ratio_or_air_density_is_refused`

the reference default is 1.2, which is accepted

fanEfficiencyAtFlow does not assert the speed ratio in the reference either: a zero
ratio is an infinite reference flow, which clamps to the last efficiency.

### `fn a_system_curve_the_fan_never_meets_is_refused`

The reference scans `[max(minFlow, maxFlow * 0.05), maxFlow]` and refuses when no
bracketed root is found, rather than returning a clamp.

A system curve that cannot be evaluated is refused as well, from inside the scan.

### `fn non_positive_estimate_inputs_are_refused_and_named`

A non-positive shaft power is refused by the motor chooser.

/* ---------------- issue #59: the record's rated speed, the affinity legs and the named limit ---- */

### `fn ax_500_selection`

The AX-500 record as the selector carries it: the physics record above, the fixture's own speed
window `[0.70, 1.13]`, and the rated speed its recorded curve is published at — 233 rpm, the
speed its own stack diameter gives at one blade tip speed across the fan family.

### `fn ratio_one_reproduces_the_recorded_curve_exactly`

The recorded curve **is** the ratio-1.0 curve: at ratio 1.0 every reading is the tabulated
point itself, bit for bit - no scaling, no interpolation, no clamp.

### `fn ratio_078_matches_the_fixtures_recorded_operating_point`

The fixture's recorded run sits at speed ratio 0.78 on AX-500; its recorded operating point
is `validation/test-vectors.json`'s `fan` vector (the same numbers the cockpit's anchored run
carries), and the rpm behind it is the record's own rated speed times the ratio.

### `fn the_power_leg_is_the_third_power_of_the_speed_ratio_from_the_recorded_curve`

The record stores no power: the engine computes it from the recorded curve, and along the
affinity family - one fixed reference flow - it carries the flow leg (n^1) times the pressure
leg (n^2): the third power. A compared value has to MOVE here for the mutation proof to mean
anything, so the assertion names the n^2 law it must not be.

### `fn an_out_of_band_ratio_is_a_named_limit_and_the_rpm_is_the_records_own_datum`

Inside the record's own band there is no limit to report.

Outside it the outcome is named - the fan, the ratio asked for, the record's band - never a
ratio clamped into the band and reported as if the engine had evaluated it.

The same check at the band level - the adapter's own call shape, no record rebuilt.

The rpm read-out is the record's datum times the ratio; a record without one has no rpm.
