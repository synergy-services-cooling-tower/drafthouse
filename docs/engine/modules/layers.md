# `drafthouse-engine` · `layers`

Ordered fill layers — the layered fill-stack contract (issue #54).

A tower's fill stack is an **ordered list of layers**, top first. Each layer names the fill
record it is built from, its own depth, and the multipliers that apply to that fill's
thermal and pressure characteristics. The stack's result carries **one entry per layer** —
the airflow and loadings the layer sees, its own pressure drop and thermal contribution —
plus the stack's combined totals.

The layer is a contract, not a second physics model. A layer's thermal contribution is the
ported `fillThermalMerkelNumber` at that layer's depth and multipliers, and its pressure
drop is the ported `fillPressureDropPa`; the stack's totals are those terms summed in
physical order ([`crate::layered_system_pressure_breakdown`]). A one-layer stack is
therefore the single-fill contract itself — the same functions, the same operand order —
which is what makes the existing published numbers the one-layer case of this path.

**Refusals name the layer.** An empty stack, a layer whose fill record is not in the
catalog, a non-positive depth or multiplier, a fill record without the characteristic the
layer needs, and a layer outside its fill's own operating limits are all refusals that name
the layer's position, fill id and depth. Nothing here clamps a value into range.

These records are synthetic fixtures (`src/data/`, and the specs a caller writes); nothing
in this module reads or claims a vendor or project report.

## Notes carried from the source comments

### `struct FillLayer`

One layer of a tower's fill stack.

`thermal_multiplier` and `pressure_multiplier` are the layer's own multipliers — a
manufacturer's derating for that layer, or 1 for a layer that contributes its fill record's
characteristics unmodified. The run-level multipliers (the water-quality factor in the
selector, or the CLI's `--thermal-multiplier`) are a separate input to
[`resolve_fill_layers`], so a layer's result can report both.

### `fn new`

A layer at both multipliers 1: it contributes exactly its fill record's characteristics
over `depth_m`. Non-positive depths and multipliers are refused at resolution, not here.

### `fn with_thermal_multiplier`

The layer's own thermal multiplier.

### `fn with_pressure_multiplier`

The layer's own pressure multiplier.

### `fn label`

`<fillId>@<depthM>` — the layer's name, in the result and in the record specs the CLI
reads (`format!`'s shortest round-trip form of the depth parses back to the same value).

### `struct FillStack`

A complete fill stack: the layers in physical order, **top first**.

### `fn single`

The single-fill contract as the one-layer case of the stack contract.

### `fn total_depth_m`

The top-to-bottom total depth of the stack.

### `fn label`

`<layer>+<layer>+…` — the stack's name, used as a candidate's fill identity and echoed
in the layered results. A one-layer stack's label is that layer's own label.

### `fn identity`

The stack's identity: `(fill id, depth)` for a **one-layer** stack — the single-fill
candidate's own identity, byte for byte — and `(stack name, total depth)` for a mixed
stack, whose identity is the stack itself rather than any one of its fills.

### `fn empty_stack_error`

The refusal an empty stack earns, wherever it is raised (resolution, or the breakdown's own
backstop guard).

### `struct LayerTerms`

One layer resolved against the catalog: the fill record it is built from, its depth and its
own multipliers. The run-level multipliers are the breakdown input's
([`crate::LayeredBreakdownInput`]), so one stack cannot mix two runs' factors.

### `fn of_fill`

A layer at both of its own multipliers 1: it contributes exactly its fill record's
characteristics over `depth_m`. This is the single-fill contract as one layer.

### `fn label`

The layer's own name, as the caller declared it.

### `fn layer_head`

`Layer <position> (<fillId>@<depthM>)` — the head every layered refusal carries.

### `fn resolve_fill_layers`

Resolve a stack against the catalog, in physical order.

Refusals, all naming the layer they are about: an empty stack; a fill id no record in the
catalog carries; a depth or a multiplier that is not positive (or not finite); and a fill
record missing the thermal or pressure characteristic its layers need. The reference's own
guard texts are reused (`depthM must be positive.`, `Fill thermal correlation is missing.`),
each prefixed with the layer's identity so a seven-layer stack cannot report a bare `depthM`.

### `struct FillLayerResult`

One layer's results, in physical order.

`position` is 1-based with the **top** layer first — the order the stack was declared in.
`cumulative_*` are the running totals through this layer inclusive, so the last layer's
cumulative value is the stack total.

### `pub thermal: ZoneCorrelation,`

The thermal characteristic the layer's contribution was computed from.

### `pub pressure: FillPressureCorrelation,`

The pressure characteristic the layer's drop was computed from.

### `pub limits: FillLimits,`

The limits the layer was checked against.

### `fn label`

The layer's name in a result: `<fillId>@<depthM>`.

### `fn check_stack_operating_envelope`

Where a stack's layers stand against their own fills' limits at one operating point.

The loadings are the **stack's**: layers sit in series in both the air path and the water
path, so every layer sees the same water loading and the same dry-air loading. The water
temperature and the water-quality class are checked only when the run declares them (`None`
means the run is not making that claim). Every failure names its layer; an empty vector
means no layer is outside the limits it echoes.
