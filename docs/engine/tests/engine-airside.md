# `engine` tests · `airside`

Air-side losses and fan-stack discharge — the JavaScript suite's expectations for
`src/core/airside.js`, ported test for test.

Sources, quoted per test:

* `tests/airside.test.js` — the eight air-side expectations, against the bundled sample
  catalog records (`src/data/sampleCatalog.js`, quoted below verbatim; all coefficients
  are illustrative synthetic data).
* `validation/test-vectors.json` (family `airside`) — the recorded regression vectors,
  reproduced at the same 1e-6 relative tolerance `tests/vectors.test.js` uses.
* `src/core/airside.js` — the formulas, `??` defaults and guard messages.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `top`

/* ---------------- the bundled sample catalog ---------------- */

### `fn idcf_064`

`sampleCatalog.towers` / `IDCF-064`.

### `fn film_of25`

`sampleCatalog.fills` / `FILM-OF25`.

### `fn film_vf38`

`sampleCatalog.fills` / `FILM-VF38` (used by the envelope cases only).

### `fn de_3p_10`

`sampleCatalog.driftEliminators` / `DE-3P-10`.

### `fn ax_500`

`sampleCatalog.fans` / `AX-500`.

### `fn breakdown_at`

The `breakdownAt` helper of `tests/airside.test.js`: the catalog cell at the recorded
33/27 °C condition, 1.5 m of fill and 200 kg/s of water.

### `fn agrees`

/* ---------------- the loss definitions ---------------- */

### `fn velocity_pressure_and_minor_loss_follow_the_definition`

dp = 0.5 rho v^2, and a minor loss is K times that velocity pressure.

The reference expression is `0.5 * rho * v ** 2`; keep the same association.

Values pinned from the reference (src/core/airside.js).

/* ---------------- recorded vectors ---------------- */

### `fn the_recorded_airside_vectors_reproduce`

`validation/test-vectors.json`, family `airside`, at the same 1e-6 relative tolerance
`tests/vectors.test.js` uses.

The per-zone Merkel numbers are part of the recorded vector too.

/* ---------------- tests/airside.test.js, ported ---------------- */

### `fn available_merkel_number_includes_the_spray_and_rain_zones`

/* ---------------- areas ---------------- */

### `fn resolve_airside_areas_falls_back_exactly_as_the_reference_does`

`tower.airFreeAreaM2 ?? fillAreaM2` and the rest of the chain, verified against the
reference: with no fan the stack area is absent, and a fan with a zero stack area wins
over the tower's own non-zero field.

/* ---------------- drift and fill correlations ---------------- */

### `fn a_drift_performance_reading_interpolates_and_extrapolates_both_curves`

`clampEnds: false`: outside the tabulated range the end pair is continued linearly, so
a velocity beyond the curve is a value, not a refusal. Values read from the reference.

### `fn the_fill_correlations_reproduce_the_reference_values`

Water/air loadings of the recorded 130 m³/s case (see the vector test above).

The multipliers scale only their own term, as in the reference.

/* ---------------- the fill operating envelope ---------------- */

### `fn the_envelope_rejection_messages_match_the_reference_byte_for_byte`

The failure strings below were read from the reference (`checkFillOperatingEnvelope` in
`src/core/airside.js`) and are quoted verbatim, `toFixed` rounding included.

All three loading/temperature limits fail at once, in the reference's order.

### `fn the_envelope_messages_round_like_the_reference`

`Number.prototype.toFixed` rounds the exact value of the double, ties away from zero.
These cases were read from the reference; the ones that distinguish it from Rust's own
fixed-precision formatting are 1.0005 (below the tie) and 54.9995 (below the tie).

A fill whose limits are all zero, so the water-loading message is always the first
failure and the value under test is the one printed.

Values just above and below the tie, read from the reference: 1.9995 and 0.9995 are
above their tie, 2.9995 is below.

A fill that accepts the water and air loadings but not any hot water, so the
temperature message is the first (and only) failure.

/* ---------------- refusal behaviour ---------------- */

### `fn a_total_pressure_curve_without_a_stack_area_is_refused_with_the_reference_message`

Zero and negative areas are refused the same way (the reference guards `!(area > 0)`).

With no fan the breakdown declares the curve static, so no stack area is required.

### `fn a_stack_recovery_factor_outside_zero_one_is_refused`

The bounds themselves: 0 is accepted, 1 is not.

### `fn a_fill_without_a_correlation_is_refused_and_a_missing_zone_contributes_nothing`

A missing zone is not an error: it contributes zero, and the reference only checks the
height once there is a zone to apply it to.

### `fn the_defaults_of_the_breakdown_match_the_reference_option_defaults`

A tower with no loss coefficients, no recovery factor, no fixed loss and no fan
resolves the reference defaults (1.2, 0.8, 0.6, 0.7, 0, static basis).
