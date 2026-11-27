# `drafthouse-engine` · `crossflow`

Simplised finite-volume crossflow grid — port of `src/core/crossflow.js`, line for line.

The explicit cell scheme is the reference's: water flows down the columns, air flows across
the rows, each cell applies a local enthalpy-potential effectiveness relation, the coarse
grid is Richardson-extrapolated against a doubled grid, and the water-side heat capacity is
iterated to the mean bulk temperature. The JavaScript engine is the specification; nothing
here re-derives a value, and a reference quirk found here is a finding to report.

One function of `src/core/psychrometrics.js` is ported in this file, because the
psychrometrics slice explicitly left it out and this lane's file fence keeps that reviewed
module byte-identical: [`saturated_temperature_from_enthalpy`] (`saturatedTemperatureFrom
Enthalpy`), the inversion `crossflow.js` and the selection slice both need. It uses the
already-ported [`solve_bracketed_root`] machinery, so no new numeric primitive is introduced.

## Notes carried from the source comments

### `const CROSSFLOW_MODEL`

`model` string `solveCrossflowGrid` returns.

### `fn js_max`

Port of the reference's `Math.max(a, b)`: a NaN operand wins, where `f64::max` would
return the other operand and quietly swallow it.

### `fn js_min`

Port of the reference's `Math.min(a, b)`, NaN-propagating for the same reason.

### `fn saturated_temperature_from_enthalpy`

Port of `saturatedTemperatureFromEnthalpy` from `src/core/psychrometrics.js`: the
temperature at which saturated air carries the given enthalpy, by bracketed root.

The JavaScript `options` argument is not ported (every call site in this slice uses the
default enhancement-factor-on option set), and the default bounds stay `[-50, 100]` when
the caller passes none.

### `struct SweepInput`

`sweepCrossflowGrid` — one explicit sweep at a fixed cell count and water specific heat.

### `fn sweep_crossflow_grid`

The reference's sweep also accumulates `totalHeatKW`; its caller never reads it, so the
port does not carry the dead field.

### `struct CrossflowGridInput`

Inputs for [`solve_crossflow_grid`] — the port of the JavaScript
`solveCrossflowGrid({...})` call, with the reference's default arguments on
[`CrossflowGridInput::new`].

### `pub pressure_pa: f64,`

Default 101 325 Pa.

### `pub salinity_g_kg: f64,`

Default 0 g/kg.

### `pub air_cells: usize,`

Default 18.

### `pub water_cells: usize,`

Default 18.

### `pub richardson: bool,`

Default true.

### `pub cp_iterations: usize,`

Default 3.

### `fn new`

The seven physically required inputs; everything else takes the reference default.

### `fn with_pressure`

The reference's `pressurePa` argument.

### `fn with_salinity`

The reference's `salinityGKg` argument, default 0.

### `fn with_cells`

The reference's `airCells` / `waterCells` arguments, both defaulting to 18.

### `fn with_richardson`

The reference's `richardson` argument, default true.

### `fn with_cp_iterations`

The reference's `cpIterations` argument, default 3.

### `struct CrossflowGridConvergence`

The `gridConvergence` object `solveCrossflowGrid` returns.

### `struct OutletAirState`

The `outletAirState` object the crossflow solve returns (and the one
`estimateOutletAirState` builds in the selection slice).

### `struct CrossflowGridResult`

The object `solveCrossflowGrid` returns, field for field.

### `fn model`

The reference's `model` field.

### `fn solve_at`

One `solveAt` call: a sweep at `nx × ny` with the water-specific-heat iteration.

### `fn solve_crossflow_grid`

Port of `solveCrossflowGrid`.

The water cp is a property of the mean bulk temperature, which is not known until the
outlet temperature is known; iterate (or take the caller's converged seed).

### `struct CrossflowStudyInput`

Inputs for [`crossflow_convergence_study`]: the reference's `{ baseCells, safetyFactor }`
plus the grid inputs it spreads into every solve.

### `pub base_cells: usize,`

Default 12.

### `pub safety_factor: f64,`

Default 1.25.

### `fn new`

The reference's defaults; `grid` carries the case's inputs.

### `fn with_base_cells`

The reference's `baseCells` argument.

### `fn with_safety_factor`

The reference's `safetyFactor` argument.

### `struct CrossflowConvergenceStudy`

The object `crossflowConvergenceStudy` returns.

### `fn crossflow_convergence_study`

Port of `crossflowConvergenceStudy` — the three-grid (n, 2n, 4n) convergence study the
selector deliberately does not call.

f_exact ~ f_fine + (f_fine - f_medium) / (2^p - 1); medium_gap is (medium - fine), so it
enters with a negative sign.
