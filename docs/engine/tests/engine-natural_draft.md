# `engine` tests · `natural_draft`

Natural-draft coupling — the JavaScript suite's expectations for `src/core/naturalDraft.js`,
ported test for test.

Sources, quoted per test:

* `tests/crossflow-natural.test.js` — the two natural-draft expectations: the documented
  balance on the 4200 m² shell, and the taller shell drawing more air. The tower there is a
  literal, quoted verbatim below.
* `validation/test-vectors.json` (family `naturalDraft`) — the two recorded catalog-cell
  cases are replayed in `rust/tests/vectors.rs`.
* `src/core/naturalDraft.js` — the draft-height guard and its message, the
  `airFreeAreaM2 ?? fillAreaM2` face area, the 480-sample scan (`prefer: 'first'`, 1e-6 Pa
  residual), the `chebyshev4` cold-water solve and the bare `catch` that lets a refusing
  sample fall out of the scan.
* The bundled sample catalog (`src/data/sampleCatalog.js`) — `FILM-VF38` and `DE-3P-10`,
  quoted below verbatim.

The JavaScript engine is the specification; nothing here re-derives a value.

## Notes carried from the source comments

### `fn documented_tower`

The tower of `tests/crossflow-natural.test.js` (the draft height travels beside the record
in this port: see `rust/src/natural_draft.rs`).

### `fn film_vf38`

`sampleCatalog.fills` / `FILM-VF38`.

### `fn de_3p_10`

`sampleCatalog.driftEliminators` / `DE-3P-10`.

### `fn documented_input`

The documented case of `tests/crossflow-natural.test.js`.

### `fn the_documented_case_balances_buoyancy_against_the_system_resistance`

tests/crossflow-natural.test.js asserts: a positive flow, a balance within 0.01 Pa, a
plume genuinely lighter than the ambient air, and a positive draft.

The flow the scan returns sits inside the face-velocity window the call declares.

`evaporationFromAirMassBalance` floors at zero and is a mass balance of the dry air.

### `fn a_taller_shell_draws_more_air`

tests/crossflow-natural.test.js: `run(200) > run(140)`.

### `fn the_default_arguments_match_the_reference_declarations`

`pressurePa` 101 325, `salinityGKg` 0, both multipliers 1, face velocities 0.2–6: a
`NaturalDraftInput::new` leaves every one of them at the reference's default, and the
solve still lands on a balanced point.

The `??` fallbacks: with no `airFreeAreaM2` the face area is the fill area, so the
solve runs over the same window the fill area defines and still lands on a flow.

### `fn a_tower_without_a_draft_height_is_refused_before_any_physics`

src/core/naturalDraft.js: `if (!(tower.effectiveDraftHeightM > 0)) throw …`.

### `fn the_draft_term_is_the_reference_buoyancy_product`

`naturalDraftPressurePa` = G * height * (ambient - plume), with G = 9.80665. The two
literals are the reference's own output for these inputs (measured in the engine).

### `fn the_salinity_reaches_the_cold_water_solve`

`solveColdWaterTemperature(… salinityGKg …)`: the salinity moves the water heat capacity,
so two runs that differ only in salinity must not report the same cold water.

A fresh-water solve is unchanged by the default argument being left at zero.
