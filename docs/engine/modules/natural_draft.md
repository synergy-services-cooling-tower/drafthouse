# `drafthouse-engine` · `natural_draft`

Natural-draft coupling — port of `src/core/naturalDraft.js`.

One buoyancy term and one coupled solve: a natural-draft shell draws air until the draft
pressure it can generate balances the air-side resistance at that flow, with the fill's
thermal demand solved at each trial flow.

The JavaScript engine is the specification. Mirrored here as it is there: the
`effectiveDraftHeightM > 0` guard before any physics, the `airFreeAreaM2 ?? fillAreaM2`
face area, the reference's `chebyshev4` integration for the cold-water solve, the
480-sample scan with `prefer: 'first'` and its 1e-6 Pa residual tolerance, and the bare
`catch` in the residual: a flow at which the physics refuses contributes a NaN to the scan
instead of failing the solve.

`TowerRecord` carries the air-side fields, so the draft height travels beside the record
(`effective_draft_height_m`) rather than inside it: the reference's tower object is one
object, but every other reader of that record in this crate takes the air-side fields only,
and an optional field on `TowerRecord` would invite a tower the schema does not declare.

## Notes carried from the source comments

### `const G`

The reference's `G` — standard gravity, m/s².

### `fn natural_draft_pressure_pa`

Port of `naturalDraftPressurePa`: the buoyancy of a column of ambient air against a column
of warmer, lighter plume air.

### `struct NaturalDraftInput`

The inputs of [`solve_natural_draft_counterflow`], carrying the reference's default
arguments: pressure 101 325 Pa, salinity 0 g/kg, both multipliers 1, and face velocities
0.2–6 m/s.

### `pub effective_draft_height_m: f64,`

The reference's `tower.effectiveDraftHeightM`, the field the draft solve is built on.

### `fn new`

The arguments the reference requires — the three records and the four condition values —
with every default argument applied.

### `fn with_pressure_pa`

The reference's `pressurePa` argument.

### `fn with_salinity_g_kg`

The reference's `salinityGKg` argument.

### `fn with_thermal_multiplier`

The reference's `thermalMultiplier` argument.

### `fn with_pressure_multiplier`

The reference's `pressureMultiplier` argument.

### `fn with_min_face_velocity_ms`

The reference's `minFaceVelocityMS` argument.

### `fn with_max_face_velocity_ms`

The reference's `maxFaceVelocityMS` argument.

### `struct NaturalDraftFlowPoint`

Everything `calculateAtFlow` computes for one trial flow — the reference's own result object
before the draft height and the ambient state are attached.

### `struct NaturalDraftResult`

The object `solveNaturalDraftCounterflow` returns, field for field (the `model` and `caveat`
strings travel with the result the CLI renders; this struct carries the physics).

### `fn solve_natural_draft_counterflow`

Port of `solveNaturalDraftCounterflow`: sample the face-velocity interval, solve the flow
whose draft balances the air-side resistance, then report the full coupled point at that
flow, plus the evaporation the plume's humidity rise implies.

`!(effectiveDraftHeightM > 0)`, written with `partial_cmp` so a NaN height is refused
too (the reference's comparison is true for a NaN, so it throws).

No fan: the reference passes no fan, so the stack discharge is not charged.

The reference swallows the refusal here: the scan skips this sample.
