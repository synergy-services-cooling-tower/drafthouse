# `engine` tests · `nozzle`

Nozzle flow and arrangement — the JavaScript suite's expectations for `src/core/nozzle.js`,
ported test for test.

Sources, quoted per test:

* `tests/water-nozzle.test.js` — the nozzle-selector count and flow-balance expectation,
  against the bundled sample-catalog nozzles (quoted below verbatim; illustrative
  synthetic data).
* `src/core/nozzle.js` — the formula, the `?? 997` density fallback, the `[4, 4000]`
  count range and the refusal message. The expected numbers were measured from the
  reference engine at this commit.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `fn nozzle_flow_uses_the_reference_default_density`

Measured from `src/core/nozzle.js` at this commit: `nozzleFlowM3S({ dischargeCoefficient:
0.72, orificeDiameterM: 0.02, pressureDropPa: 65000 })` with the default density 997.

The reference's `waterDensityKgM3` argument is not used by this formula; a caller that
passes one gets the reference's own number for it.

### `fn the_arrangement_list_is_the_reference_order_and_count`

`tests/water-nozzle.test.js` — 'nozzle selector returns count and flow balance' — plus the
reference's own numbers for the same call.

### `fn the_count_range_option_filters_and_an_empty_result_is_refused`

The default range's lower end: a trickle of flow asks for a single nozzle.
