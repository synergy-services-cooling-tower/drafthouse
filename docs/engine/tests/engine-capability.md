# `engine` tests · `capability`

Characteristic-curve capability — the JavaScript suite's expectations for
`src/core/capability.js`, ported test for test.

Sources, quoted per test:

* `tests/capability.test.js` — identical design and test conditions evaluate to 100 %
  capability, and a seeded Monte Carlo is repeatable for the same seed.
* `validation/test-vectors.json` (family `capability`) — the recorded cases are replayed in
  `crates/engine/tests/vectors.rs`, next to the two JavaScript vector families.
* `src/core/capability.js` — the guard order and messages, the default exponent, the
  `-1e12` infeasible-demand residual, the curve sampling range and filter, and the
  Monte-Carlo acceptance floor.
* `tests/vectors.test.js` — the seeded generator's first draws, quoted in `rand_js_draws`
  (produced by `createSeededRandom(17)` in the JavaScript engine itself).

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `fn condition`

The condition of `tests/capability.test.js`.

### `fn capability_input`

`evaluateCharacteristicCapability({ design, test, characteristicExponent: -0.6 })`.

### `fn identical_design_and_test_conditions_evaluate_to_one_hundred_percent`

tests/capability.test.js: `Math.abs(capabilityPct - 100) < 0.001`.

### `fn the_default_exponent_applies_when_the_input_omits_it`

src/core/capability.js: `characteristicExponent = -0.6`. `CharacteristicCapabilityInput::new`
applies the reference's default; `with_characteristic_exponent` overrides it.

The reference's own test asserts within 0.001: the capability is root-solved, so an
identical design and test condition lands on 100 % to within the scan's stopping width.

### `fn a_warmer_test_cold_water_declines_and_a_cooler_one_gains_capability`

The recorded vectors (`cap-worse` 88.7939 %, `cap-better` 111.605272 %) pin the
direction; this test only asserts the direction, not the recorded number.

### `fn a_positive_exponent_is_refused_after_both_conditions_are_validated`

The reference validates `design`, then `test`, then the exponent.

A bad condition is reported before the exponent.

### `fn the_curves_span_the_reference_window_and_carry_both_ordinates`

src/core/capability.js: `range(max(0.05, min(...) * 0.55), max(...) * 1.55, curvePoints)`,
filtered to the points whose design demand is finite. The documented condition is
feasible at the low end of the window (the approach to the wet bulb pinches only at
higher L/G), so every point survives here.

The characteristic ordinate follows `coefficient * L/G ** exponent` exactly.

### `fn a_case_with_a_pinch_in_the_curve_window_drops_the_infeasible_points`

A design condition whose demand pinches inside the sampling window loses those curve
points to the reference's `Number.isFinite` filter; 80 sampled points come back as 78
(measured in the reference for this pair of conditions).

The dropped points are the high-L/G end: the surviving window starts where it started.

/* ---------------- the seeded stream ---------------- */

### `fn the_seeded_generator_draws_the_javascript_stream`

Measured in the reference engine itself (`createSeededRandom(17)`): the generator is
32-bit wrapping arithmetic, so these draws are exact, not approximate.

`gaussianRandom` over that stream (measured in the reference in the same place).

### `fn percentile_matches_the_reference_definition`

`percentile([3, 1, 4, 1, 5, 9, 2, 6], p)` in the reference: sorted, position clamped,
then linearly interpolated between the neighbours.

A position outside [0, 1] clamps to the ends.

/* ---------------- Monte Carlo ---------------- */

### `fn documented_uncertainty`

The uncertainty of `tests/capability.test.js`: five perturbed test fields, in its order.

### `fn a_seeded_monte_carlo_is_repeatable`

tests/capability.test.js: the same input twice gives the same p50, and p2_5 < p97_5.

Every sample is either accepted or rejected, and the seed is echoed.

### `fn a_run_that_cannot_clear_the_acceptance_floor_is_refused`

The reference refuses when fewer than `max(20, samples * 0.5)` samples are accepted, so
a request for ten samples can never clear its own floor.

### `fn the_perturbation_order_decides_which_field_draws_which_gaussian`

The reference walks `Object.entries(standardUncertainty)`: two fields whose sigmas are
swapped between orders produce different samples, so the order is part of the contract.
