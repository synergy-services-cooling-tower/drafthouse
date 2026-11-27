# `engine` tests · `crossflow`

Crossflow grid — the JavaScript suite's and the recorded vectors' expectations for
`src/core/crossflow.js`, ported test for test.

Sources, quoted per test:

* `validation/test-vectors.json` (family `crossflow`) — the recorded regression vectors,
  reproduced at the same 1e-6 relative tolerance `tests/vectors.test.js` uses.
* `tests/crossflow-natural.test.js` — the grid-convergence, monotonicity and
  thermodynamic-limit expectations.
* `src/core/crossflow.js` — the formulas, the `??` defaults and the refusal message.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `fn case_input`

The `CASE` of `tests/crossflow-natural.test.js`.

### `fn assert_close`

The same `agrees` rule `tests/vectors.test.js` uses for recorded vectors: the relative
tolerance with an absolute floor of 1, so a value recorded as 0.098917 is allowed its last
digit of rounding.

### `fn recorded_crossflow_vectors_reproduce`

The four recorded vectors of `validation/test-vectors.json`, family `crossflow`.

### `fn the_default_grid_is_grid_converged_against_a_much_finer_one`

tests/crossflow-natural.test.js: 'crossflow cold-water temperature is grid-converged at
the default cell count'.

### `fn the_discretisation_error_falls_as_the_grid_is_refined`

tests/crossflow-natural.test.js: 'crossflow discretisation error falls as the grid is
refined'.

### `fn the_crossflow_model_respects_its_thermodynamic_limits`

tests/crossflow-natural.test.js: 'crossflow respects its thermodynamic limits'.

### `fn cooling_increases_monotonically_with_available_transfer`

tests/crossflow-natural.test.js: 'crossflow cooling increases monotonically with
available transfer'.

### `fn the_convergence_study_sees_a_first_order_scheme`

tests/crossflow-natural.test.js: 'crossflow discretisation is first order, as the scheme
implies'.

### `fn the_saturated_temperature_inversion_round_trips`

`saturatedTemperatureFromEnthalpy` — the psychrometrics remainder this slice ports — can
be checked directly: it inverts the already-ported saturated enthalpy. The bounds are the
ones the crossflow and selection call sites use; a bound at or above the point where
saturation pressure reaches atmospheric pressure is refused by the saturation correlation,
exactly as the reference refuses it.

The reference's own default bounds are [-50, 100]; at 100 °C saturated air carries
atmospheric pressure, and the saturation correlation refuses (the reference refuses it
too — `solveBracketedRoot` evaluates every bound before it starts).
