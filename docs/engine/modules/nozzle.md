# `drafthouse-engine` · `nozzle`

Nozzle flow and arrangement — port of `src/core/nozzle.js`, line for line.

The reference's `waterDensityKgM3 = 997` default and its
`nozzle.referenceWaterDensityKgM3 ?? 997` fallback are [`NOZZLE_WATER_DENSITY_KG_M3`]
here; the count stays an `f64` because the reference never truncates `Math.ceil` to an
integer, and the sort is the reference's two-key comparator
(`excessFlowPct`, then `count`), stable like `Array.prototype.sort`.

## Notes carried from the source comments

### `const NOZZLE_WATER_DENSITY_KG_M3`

The reference's prototype water density when neither the call nor the nozzle record
supplies one.

### `fn nozzle_flow_m3_s`

Port of `nozzleFlowM3S`.

### `struct NozzleRecord`

A nozzle record — the fields of `sampleCatalog.nozzles` the selector reads.

### `pub reference_water_density_kg_m3: Option<f64>,`

`?? 997` at the call site.

### `struct NozzleArrangement`

The object `selectNozzleArrangement` returns, field for field.

### `struct NozzleArrangementInput`

Inputs for [`select_nozzle_arrangement`] — the port of the JavaScript
`selectNozzleArrangement({...})` call.

### `pub target_count_range: [f64; 2],`

Default `[4, 4000]`.

### `fn new`

The three physically required inputs; the count range takes the reference default.

### `fn with_target_count_range`

The reference's `targetCountRange` argument.

### `fn compare_arrangements`

The reference's comparator: `a.excessFlowPct - b.excessFlowPct || a.count - b.count`.
A zero difference and a NaN difference both fall through to the count, exactly as the
JavaScript `||` does.

### `fn select_nozzle_arrangement`

Port of `selectNozzleArrangement`: one candidate per nozzle, filtered to the valid count
range, sorted by excess flow. An empty result is refused, as the reference refuses it.
