# `drafthouse-cli` · `ct-engine`

The engine's JSON command surface, shared by the `ct-engine` binary and the wasm export.

`run` turns one command line into one JSON object; `Failure` splits the two ways a
request can be refused, and each carries the exit status the binary reports for it,
because the parity harness and the wasm binding both key off that status rather than off
a message.

The parity harness (`tools/parity/run.mjs`) drives this over the recorded
inputs and diffs its JSON against the JavaScript reference engine; with `--engine wasm`
it drives the same commands through the wasm build instead of the binary. JSON keys use
the JavaScript names on purpose: the harness compares the two engine outputs field by
field, so the names should be the ones both sides already agree on.

Exit status of `ct-engine`, the binary over this module:

* `0` — one JSON object on stdout.
* `1` — the engine refused an out-of-domain request. `{"error":"DomainError","message":…}`
  on stderr; stdout stays empty.
* `2` — usage error (unknown command, missing or unparsable option).

## Notes carried from the source comments

### `enum Failure`

A request that cannot be computed, split by what the caller should do about it.

The variant carries the exit status the binary reports for it, so a caller that is not
the binary (the wasm export) can hand the same status back without a second mapping.

### `Usage(String),`

The command line itself is wrong.

### `Domain(drafthouse_engine::DomainError),`

The physics refuses this input.

### `fn reply`

Run one command and wrap the outcome as the reply envelope a caller that is not a
process reads — the wasm export (`crates/wasm/src/lib.rs`, driven by `tools/parity/binding.mjs`).
The value is the same JSON the binary prints; a refusal becomes the `status`/`error`
pair the binary reports through its exit status and stderr, so both surfaces agree on
what "refused" means.

### `struct Cli`

`--key value` options plus bare `--switches`.

### `fn integration_default`

The `--integration` option with a caller-chosen default: `merkel.js`'s callers default
to `simpson`, but the capability projection's own default is `chebyshev4`, and the
harness compares both engines under whichever default the reference declares.

### `fn airside_object`

Every field the air-side breakdown reports, nested areas included — shared by the
`airside` command and the selector's per-candidate output.

### `fn fill_stack`

The layer spelling: `<fillId>@<depthM>[@<thermalMultiplier>[@<pressureMultiplier>]]`, with
layers joined with `+` in physical order (top first). This is the same spelling a stack's
own `label()` prints and the one the `select` tower field `fillStacks` carries, so a stack
read out of a result parses back into the stack that produced it.

### `fn rate_fill_records`

The `--fills` records `layers` resolves a stack against: the same spec encoding `select`
reads (a fill record authored for one is a fill record for the other), with the
tower-type compatibility `select` needs and a rating flow has no use for.

### `fn layers`

`layers` — the **rating** flow: one selected stack, resolved in physical order against the
`--fills` records, with a result per layer and the stack's combined totals.

Every layer is checked against its own fill's limits at this operating point (the loadings
are the stack's — layers sit in series), so a layer outside them is a refusal naming the
layer and the limit; `--hot` and `--quality-class` add the two dimensions the caller may
declare. Nothing is clamped into range.

### `fn layers_object`

The `layers` reply: the stack's own name and total depth, one result per layer in physical
order, the layers' combined totals, and the air-side breakdown `airside` reports (its fill
terms are the stack's totals). Data only — nothing here renders.

### `fn layer_object`

One layer's result, field for field: its identity and depth, the characteristic and
multipliers it was computed from, the limits it was checked against, what it sees
(airflow, loadings, velocity) and what it contributes (drop, transfer number) — plus the
running totals through it, whose last value is the stack total.

### `fn zone_object`

One heat/mass-transfer correlation, as the record declares it.

### `fn pressure_object`

One fill pressure-drop correlation, as the record declares it.

### `fn limits_object`

A fill's own operating limits, as the record declares them.

### `fn motor`

/* ---------------- water balance ---------------- */

### `fn balance`

The reference's default arguments are applied by leaving `--drift` / `--cycles` out, the
way an absent property takes the default in JavaScript.

/* ---------------- capability, natural draft and performance curves ---------------- */

### `const CONDITION_FIELDS`

The fields of a `--design` / `--test` condition spec, in the reference condition object's
own field order.

### `const METHOD_CAPABILITY`

The reference's own narrative strings, verbatim — the harness compares their text, because
a port that quietly rewords a method note is not the same result object.

### `fn capability_condition`

One `condition` spec: the six required numbers plus the optional `pressurePa`, every one
under the field name the reference reads.

### `fn condition_field`

The field a sigma spec names, under the reference's field name.

### `fn condition_sigmas`

One `sigmas` spec, in the order the caller wrote it: that order decides which field draws
which Gaussian, exactly as the reference's `Object.entries` order does.

### `fn capability_input`

The `capability` / `capability-mc` inputs: the two conditions and the reference's default
arguments (an exponent of −0.6, `chebyshev4`, 80 curve points, the bulk convention).

### `fn air_state_object`

A full psychrometric state, in the reference's field names — the states the capability and
natural-draft results carry.

### `fn convention_name`

The enthalpy convention's JavaScript spelling.

### `fn capability_object`

The `evaluateCharacteristicCapability` result object, field for field.

### `fn capability_monte_carlo`

The `monteCarloCharacteristicCapability` result object, field for field.

### `fn natural_draft`

The `solveNaturalDraftCounterflow` result object, in the reference's own key names. The
draft height is its own option (`--draft-height`): the reference reads it off the tower
object, and every other reader of a tower record here takes the air-side fields only.

### `const CURVE_RECORD_FIELDS`

The fields of a performance-curve record spec.

### `fn performance_curve_records`

The `--records` list: `;`-separated records of `wetBulbC,rangeC,waterFlowKgS,coldWaterC`.

### `fn range_pair`

One `[lo, hi]` axis range, as the reference returns it.

### `fn curve_capability`

/* ---------------- selection ---------------- */

### `fn select`

The selector, driven from record specs — its result JSON is the projection the parity
harness compares against the reference `selectCoolingTowerComponents`.

Fail-closed: nothing invalid may reach the ranking. The schema gate runs before the
records are parsed into their types, so a mistyped, missing or non-finite field is a
named refusal rather than an `undefined`/`NaN` that a candidate would rank on.

Issue #24: the data a non-shell caller needs. Both keys are data only — the values
the recommended candidate actually used, and its worked sheet in the `worked.js`
shape. Nothing here renders, derives geometry or emits markup.

/* ---------------- the selection's inputs and worked sheet (issue #24) ---------------- */

### `fn selection_inputs_object`

The input values the run used for the recommended candidate: the identity it resolved,
the requirements it ranked against, and the catalog records that candidate was built
from, dimension fields included. Record fields are echoed from the `key:value` specs the
caller passed — this module's parser is their only reader, so the echo is the parsed
value, not a second reading of the record.

### `fn spec_record_object`

One record out of a `;`-separated record list, matched on its `id` field, as a JSON
object of the values it carried — or `null` when the list does not hold that id (a
record the list omitted, which the selector cannot then have used).

### `fn spec_value`

A spec field value at its own type: `1.2` is a number, `a|b` a list, `1:2:3|4:5:6` a
list of lists (a curve), anything else a string.

### `const FILL_LAYER_STEP_POSITION`

Where the per-layer steps go in the worked sheet: after the four setup steps (duty flow,
range, heat, dry-air mass flow) and before the fill's own total, so the sheet reads
stack → fill total → zones → demand.

### `fn fill_layer_steps`

One worked step per layer of the candidate's fill stack, top first.

The sheet names each layer by fill id and depth — the same identity the `fillLayers` block
carries — and shows the characteristic it was evaluated with, the loadings every layer of
the stack sees and the multiplier that reached it. The value is the layer's own transfer
number, read from the computed layer result rather than re-derived.

### `fn selection_worked_object`

The recommended candidate's worked sheet, in the `worked.js` shape: `title`, `purpose`,
`steps` (each `label` / `why` / `formula` / `substitution` / `value` / `unit` /
`reference` / `kind`) and the `result` the steps resolve to. Every `value` is read from
the selection result the engine already computed; the sheet re-derives nothing, exactly
as `worked.js` calls the same core functions rather than restating their physics.

The stack's layers, immediately before the fill's own total: the sheet names every layer
by fill id and depth, in the same top-first order the `fillLayers` block reports, so the
worked steps and the data-only surface identify the same stack.

### `fn worked_step`

One `worked.js` step: the label, why it is computed, the formula, the numbers
substituted into it, the value, its unit, an optional source reference, and the kind.

### `fn worked_note`

A `worked.js` note: narrative with no value of its own.

### `fn fixed`

A fixed-precision display of a quantity inside a step's `substitution` text — the job
`Number.prototype.toFixed` does in `worked.js`.

### `fn select_fields_object`

The record field names `select` accepts, per catalog list — the same constants its
record readers enforce, so a caller that builds record specs (the wasm binding) projects
a catalog against the engine's own intake instead of a mirrored list. A field the
selector does not read is a usage error, which is why the projection has to be exact.

### `fn requirements_object`

The resolved requirements, in the reference's field names minus the inputs that only fed
its economics (see `docs/engine/README.md`).

### `fn candidate_object`

One feasible candidate, every physics field included.

### `fn nozzle_arrangement_object`

One nozzle arrangement, field for field.

### `fn compact_candidate_object`

One feasible candidate's identity and ranking metrics — the `candidates` table the
`--all-orders` switch emits so the harness can check every objective's order at once.

### `fn orders_object`

The whole feasible set's order under each objective, as identity strings.

/* ---------------- crossflow, convergence and nozzles ---------------- */

### `fn crossflow_input`

The crossflow grid input every `crossflow` / `convergence` invocation shares.

### `struct Spec`

A `key:value` record spec using the JavaScript catalog field names, e.g.
`--tower fillAreaM2:64,inletAreaM2:32,sprayZoneHeightM:0.6`.

### `fn parse_text`

One record's `key:value` pairs, comma-separated.

### `fn list`

A `|`-separated field; absent is an empty list.

### `fn required_list`

A `|`-separated field that must carry at least one entry.

### `fn fan_record`

/* ---------------- selection catalog from record specs ---------------- */

### `fn record_list`

A `;`-separated list of `key:value,…` records for the `select` command, each record a
[`Spec`]; list-valued fields spell their entries with `|` (see the usage text).

### `fn zone_from_value`

The five-field zone correlation, in the reference's field order, `|`-separated.

### `fn fill_physics`

One fill record's physics out of its spec — the reader `select` and `layers` share, so a
fill record authored for one is a fill record for the other.

`id`, `thermal`, `pressure` and `limits` are required: they are what a layer is computed
from. `allowedWaterQualityClasses` is read when present and empty otherwise — `select`'s
schema requires it, while a `layers` run that declares no `--quality-class` has nothing to
check it against. `compatibleTowerTypes` belongs to the selector's candidate generation and
is read by `selection_fill`, not here.

### `fn selection_fan`

The speed the recorded curve is published at. Optional: a record that states no rated
speed still selects, but it has no rpm read-out (nothing here invents one).

### `fn quality_factors`

`--quality-factors clean:1:1;moderate:0.92:1.12;…` — name, thermal multiplier, pressure
multiplier. There is no third field: the selector reads only these two.

/* ---------------- the catalog schema gate ---------------- */

### `struct CatalogList`

The five record lists of the `select` catalog, in `docs/engine/CATALOG_SCHEMA.md`'s order: the
option the records arrive under, the JSON key `validate-catalog` reports them as, and the
record type each list must satisfy.

### `fn validate_catalog_records`

Validate every record of the catalog against the machine schema in [`drafthouse_engine::validate`],
returning the per-list record counts. This is the fail-closed gate `select` runs before
it builds or ranks anything: every violation is a usage error naming the record and the
field, and all of them are reported at once.

### `fn validate_catalog_command`

`validate-catalog` — the schema over the records without running the selector. The
counts it prints are how the parity harness ties the machine schema to the bundled
catalog.

### `fn number`

JSON numbers use Rust's shortest round-trip float formatting, so the harness parses back
the exact `f64` the engine produced. Non-finite values cannot appear on a success path;
they would serialize as `null` rather than as invalid JSON.

### `fn optional_number`

A field the engine may not resolve at all (`fanStackVelocityMS` on a static-pressure
curve, `fanStackAreaM2` on a tower with no stack): the JSON `null` the harness compares.

### `fn json_array`

A JSON array of pre-rendered items.

### `fn json_string`

One JSON string literal, escaped the way both the binary's stderr and the wasm envelope
need it (the reference `JSON.stringify` set of escapes).
