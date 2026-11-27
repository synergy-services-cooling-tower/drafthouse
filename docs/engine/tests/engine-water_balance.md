# `engine` tests · `water_balance`

Water balance — the JavaScript suite's expectations for `src/core/waterBalance.js`,
ported test for test.

Sources, quoted per test:

* `validation/test-vectors.json` (family `waterBalance`) — the two recorded cases,
  reproduced against the same `psychrometricState` inlet `tests/vectors.test.js` builds
  and at the same 1e-6 relative tolerance it uses.
* `tests/water-nozzle.test.js` — the conservation expectation (`makeup` equals the sum of
  evaporation, drift and blowdown).
* `tests/water-balance-validation.test.js` (issue #16) — the refusals this module now
  shares with the reference, input for input and message for message, plus the
  zero-valued cases that must stay legitimate.
* `tests/worked.test.js` — the worked water balance of the design cell (the outlet
  humidity ratio is read from the reference and quoted below).
* `src/core/waterBalance.js` — the formulas, the default arguments (`driftKgS` 0,
  `cyclesOfConcentration` 4), the `Math.max(0, …)` floors, the guard order and the guard
  messages.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `const VECTOR_TOLERANCE`

The tolerance `tests/vectors.test.js` uses.

### `fn number`

/* ---------------- recorded vectors ---------------- */

### `fn the_recorded_water_balance_vectors_reproduce`

`validation/test-vectors.json`, family `waterBalance`.

/* ---------------- tests/water-nozzle.test.js, ported ---------------- */

### `fn water_balance_conserves_makeup_equals_evaporation_plus_drift_plus_blowdown`

/* ---------------- tests/worked.test.js, ported ---------------- */

### `fn the_worked_water_balance_of_the_design_cell_closes`

The leaving humidity ratio of the design cell (33/27 °C air, 42/32 °C water, L/G 1.5):
read from the reference's `workedOutletAir`, which is a worked-steps renderer and
deliberately not ported.

### `const OUTLET_HUMIDITY_RATIO`

The rule-of-thumb sanity check of the JavaScript test: ~1 % of circulating flow per
5.5 K of range.

/* ---------------- default arguments ---------------- */

### `fn the_reference_default_arguments_apply_when_the_field_is_absent`

`src/core/waterBalance.js`: `coolingTowerWaterBalance({ evaporationKgS })` — `driftKgS`
defaults to 0 and `cyclesOfConcentration` to 4, and each default is independent.

The same request with both defaults spelled out is the same request.

Overriding one default leaves the other in place.

/* ---------------- the floors ---------------- */

### `fn blowdown_is_floored_at_zero_while_makeup_still_sums_the_losses`

`0.5 / (4 − 1) − 1.0` is below zero, so the reference's floor returns zero.

### `fn the_floors_do_not_swallow_a_nan`

The reference's floors are `Math.max(0, …)`, which returns `NaN` for a `NaN` input; a
plain Rust `value.max(0.0)` would silently return `0` and hide the broken input.

The NaN path is now reached through `cyclesOfConcentration`, which still passes its own
guard for a NaN (`NaN <= 1.0` is false, the leniency the module header states); the NaN
a humidity ratio used to carry is refused before the floor since issue #16.

Condensation floors to +0, not to −0.

/* ---------------- refusal behaviour, in the reference's guard order ---------------- */

### `fn a_non_positive_circulating_flow_is_refused_before_the_drift_ppm_check`

Guard order: the flow assertion is the reference's first statement, so a request that
breaks both rules is refused by the flow rule.

A zero ppm is a legitimate drift-free design and is not refused.

### `fn cycles_of_concentration_at_or_below_one_is_refused_first`

The guard is the first statement, so a request that also carries a NaN evaporation is
refused by the cycles rule rather than run through the arithmetic.

### `fn a_non_finite_or_negative_humidity_ratio_is_refused_while_zero_is_not`

`tests/water-balance-validation.test.js` (issue #16): the humidity ratios of the air-mass
balance are refused when non-finite or negative; a zero ratio (dry air) and the
condensation floor are not refused, and the flow guard stays first.

A zero humidity ratio is dry air: it computes. (Condensation still floors at zero.)

The flow assertion is still the reference's first statement.

### `fn a_non_finite_drift_ppm_is_refused_while_zero_is_not`

Issue #16: `driftLossKgS` refuses a non-finite ppm (its `< 0` check could not: `NaN < 0.0`
is false), keeping the negative refusal, and a zero ppm stays a legitimate design.

### `fn a_negative_or_non_finite_balance_loss_is_refused_while_zero_is_not`

Issue #16: the balance refuses a negative or non-finite loss on either input — the two
paths into `driftKgS` now agree — while a zero loss stays legitimate on both (the no-load
balance, and `driftKgS`'s documented default).

The issue's second reproduction: both losses negative, refused by the first guard.

Zero on either input computes: no load, and the drift-free default.

A zero evaporation with a real drift is the no-load case with drift: makeup is the
drift alone, blowdown floors at zero.
