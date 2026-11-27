# `drafthouse-engine` · `lib`

Cooling-tower engine core, Rust port — slices 1 to 4.

This crate mirrors the JavaScript reference engine in `../src` (barrel: `src/index.js`)
for the ported slices only:

* [`psychrometrics`] — ASHRAE-style moist-air properties from dry bulb/wet bulb or dry
  bulb/relative humidity, including the water-vapour enhancement factor and the
  sub-zero (over-ice) saturation branch.
* [`merkel`] — counterflow Merkel demand by composite Simpson and four-point
  equal-weight Tchebycheff quadrature, with the selectable entering-air enthalpy
  convention, plus the cold-water-temperature solve from an available `KaV/L`.
* [`airside`] — the air-side pressure breakdown (fill, drift eliminator, minor losses,
  fan-stack discharge), the resolved flow areas, the per-zone Merkel contributions and the
  fill operating envelope.
* [`fan`] — fan operating limits, curve evaluation, the fan/system balance and standard
  motor selection.
* [`water`] — the water heat-capacity correlation the Merkel demand uses.
* [`water_balance`] — evaporation from the dry-air mass balance, drift loss, and the
  blowdown/makeup water balance.
* [`capability`] — the characteristic-curve capability projection (design demand against
  the test-anchored whole-tower characteristic) and its seeded Monte-Carlo propagation.
* [`natural_draft`] — the coupled natural-draft counterflow solve: buoyancy against the
  air-side resistance, with the fill's thermal demand solved at each trial flow.
* [`performance_curve`] — the rectangular wet-bulb × range curve set: bilinear cold-water
  prediction, the inverse water-flow solve, and the test capability ratio.
* [`crossflow`] — the simplified finite-volume crossflow grid, Richardson-extrapolated,
  plus the saturated temperature-from-enthalpy inversion the crossflow and selection
  slices both use.
* [`nozzle`] — nozzle flow and arrangement selection.
* [`selection`] — component selection: candidate generation, the feasibility constraints
  and the per-candidate physics ported from `selection.js`, ranked by the
  **engineering objectives** owner-routed from issue #1 (duty and margin as constraints,
  the objective declared per run, capacity and capability ratio reported) instead of the
  reference's superseded economics ranking. No commercial field exists anywhere in the
  port. This module also carries the unported-remainder helpers the selector needs
  (`waterDensityKgM3`, `heatRejectionKW`, `volumetricFlowM3SFromMassFlow`,
  `estimateOutletAirState`); see the module header for why they live there.
* [`validate`] — the fail-closed catalog-record validator (issue #5): the machine schema
  of the five catalog record types as data (`docs/CATALOG_SCHEMA.md` as code), and the
  refusals that keep a mistyped, missing or non-finite field out of a ranking.
* [`numeric`] — the quadrature and root-finding primitives the slices need.

Not ported yet: `generateDemandCurve` from `merkel.js` and the worked-steps renderer.
The JavaScript engine is the specification, not a work item.

The library has no dependencies outside the standard library; see `README.md` for the
JavaScript-to-Rust name mapping and for the parity harness
(`node scripts/parity/run.mjs`).

Two more modules are not part of the physics: [`cli`] is the JSON command surface shared
by the `ct-engine` binary and the wasm export, and [`wasm`] (built only for
`wasm32-unknown-unknown`) is the C ABI over it. Both are documented in `README.md`.
