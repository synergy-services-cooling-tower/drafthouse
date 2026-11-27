# `cli` tests · `layers`

Ordered fill layers — the layered fill-stack contract (issue #54).

Sources, quoted per test:

* `validation/test-vectors.json` (family `airside`) — the recorded single-fill vectors this
  suite reproduces through a **one-layer stack**. The vectors record behaviour; the
  one-layer case is the single-fill arithmetic itself, so the recorded numbers are the
  layered path's own regression anchor, and `validation/regression-baseline.json` (the
  harness's comparison partner) carries the same numbers for the one-layer CLI call.
* `validation/test-vectors.json` (family `fillLayers`, added by issue #54) — the layered
  cases: one single-layer case and one mixed case, both expressed through the recorded
  single-fill values.
* `src/data/sampleCatalog.js` (retired with the reference, last commit `d5d4cf8`, recorded in
  the private decision record D17) — the tower, fill and drift records quoted below verbatim; all
  coefficients are illustrative synthetic data.

The JavaScript engine is the specification; nothing here re-derives a physics value.

## Notes carried from the source comments

### `const VECTOR_TOLERANCE`

The relative tolerance `tests/vectors.test.js` uses for the recorded vectors.

### `const DRY_AIR_DENSITY`

The two air densities the recorded `airside-*` cases cross the command line with.

### `fn close`

Compare two JSON numbers within a few ulp.

`serde_json`'s float parse is not correctly-rounded (the reply carries the shortest
round-trip text either way), so a sum of two *parsed* values can land a couple of ulp away
from the same sum computed inside the engine. Nothing here is looser than that: the
engine's own sums are compared exactly at the library seam.

/* ---------------- the bundled sample catalog ---------------- */

### `fn idcf_064`

`sampleCatalog.towers` / `IDCF-064`.

### `fn film_of25`

`sampleCatalog.fills` / `FILM-OF25`.

### `fn film_vf38`

`sampleCatalog.fills` / `FILM-VF38` — a second, different fill type for the mixed stacks.

### `fn de_3p_10`

`sampleCatalog.driftEliminators` / `DE-3P-10`.

### `fn ax_500`

`sampleCatalog.fans` / `AX-500`.

/* ---------------- the layered path at the library seam ---------------- */

### `fn single_fill`

The single-fill breakdown the reference's `systemPressureBreakdown` produces for this
fixture — the comparison partner of every one-layer stack below.

### `fn stacked`

The stack through the layered path, with the layers resolved against `fills` in order.

### `fn one_layer`

The single `(fill, depth)` pair a one-layer stack carries.

/* ---------------- issue #54: a one-layer stack IS the single fill ---------------- */

### `fn a_one_layer_stack_reproduces_the_single_fill_breakdown_bit_for_bit`

Acceptance criterion 4's anchor at the library seam: the layered path's fill terms are
the single-fill path's own values, not a re-derivation of them.

### `fn a_one_layer_stack_reproduces_the_recorded_airside_vector`

`validation/test-vectors.json`, family `airside`, case `airside-160`: the recorded
single-fill numbers, reached through a one-layer stack and the recorded inputs.

The layer's own terms are the recorded fill terms exactly.

/* ---------------- issue #54: mixes, in physical order ---------------- */

### `fn two_layers_of_the_same_fill_split_the_single_fill_terms_by_depth`

Same-type layers are equally valid (criterion 5). At fixed loadings both the transfer
number and the drop are linear in depth, so a 0.6 m + 0.9 m split of the recorded 1.5 m
case must sum to the recorded 1.5 m terms — and split them in the depth ratio.

The split is the depth ratio, to the last few ulp.

The running totals through each layer end at the stack's own totals.

Every layer sees the stack's own loadings and airflow.

### `fn a_mixed_stack_sums_its_layers_in_physical_order`

Different fill types in one stack (criterion 5): the upper layer is `FILM-OF25` over
`FILM-VF38`, the shape issue #54 names. Each layer's term is the ported fill function at
that layer's own fill, depth and multipliers; the totals are those terms summed top down.

Each layer is the ported primitive, called with the loadings the layer sees.

The combined totals are the layers' sums, in physical order.

The stack's transfer terms ride on top of the two zones, unchanged.

The layers' characteristics are the records' own, echoed per layer.

/* ---------------- issue #54: multipliers ---------------- */

### `fn a_layer_multiplier_composes_with_the_run_multiplier`

The layer's own multiplier and the run's are separate inputs; the layer's contribution is
the ported function at their product, and each is echoed separately.

The run's thermal multiplier still reaches the zones, as it does for a single fill.

/* ---------------- issue #54: refusals name the layer ---------------- */

### `fn a_layer_outside_its_fills_limits_is_refused_by_name`

The stack is refused at its own operating point: 400 kg/s over 64 m² is 6.25 kg/(m²·s),
above `FILM-OF25`'s 5.4 — and the message names the layer and the limit. The layer is
never clamped into range.

5.8 kg/(m²·s) is inside FILM-VF38's own 1.1–6.2 and outside FILM-OF25's 1.3–5.4, so the
failure names the first layer and only the first layer.

The declared dimensions are checked when the run declares them.

`FILM-VF38` allows 6.2, so its own layer passes the same loading.

/* ---------------- issue #54: the recorded layered vectors ---------------- */

### `fn recorded_stack`

Parse one recorded stack spelling: `<fillId>@<depthM>` per layer, `+`-joined. The family's
cases are the plain two-field spelling — a layer carrying its own multipliers has no
recorded case, and the multiplies are exercised at the library seam above.

### `fn the_recorded_fill_layer_vectors_reproduce`

The `fillLayers` family of `validation/test-vectors.json` — the layered cases issue #54
adds alongside the recorded families. Both cases carry the recorded `airside-160` fill
terms (the single layer directly, the split by depth shares), so a disagreement here is a
finding about the layered path, not a licence to move the vector.

The last cumulative value is the stack's own total.

/* ---------------- issue #54: the data-only surface ---------------- */

### `const TOWER_SPEC`

The recorded `airside-160` case's options, verbatim (`validation/regression-baseline.json`):
the tower, fill, drift and fan records and the two densities the single-fill leg compares.

### `const FILLS`

The `--fills` records in the selector's own encoding (also the `layers` command's).

### `fn common_args`

The air-side options every command below shares (the recorded case's own).

### `fn the_rate_command_reports_every_layer_and_the_combined_total`

Criterion 7's data surface: `ct-engine layers` is one command over the whole stack, and
its reply carries the layers in physical order plus the layers' totals.

Each layer identifies its characteristic, its multipliers and the limits it was checked
against; each sees the stack's own airflow and loadings.

The combined total is the layers' sum, and it is the breakdown's fill term too.

The breakdown around the layers is the single-fill one: the zones are unchanged.

### `fn the_rate_command_reproduces_the_single_fill_command_for_a_one_layer_stack`

Criterion 4 at the data surface: the identical inputs through `airside` (single fill) and
through `layers` (a one-layer stack) answer with the same air-side object.

### `fn the_rate_command_refuses_a_layer_outside_its_limits_and_names_it`

Criterion 2 at the data surface: the refusal is a named domain error, not a clamped
number, and it says which layer hit which limit.

400 kg/s over the 64 m² fill face is 6.25 kg/(m²·s): outside FILM-OF25's 5.4.

The same stack inside the limits still answers.

### `fn a_layer_stack_spelling_round_trips_through_the_command_surface`

The stack's own name parses back into the stack that produced it, so a caller can feed a
result's `stackLabel` straight back in.

A malformed spelling is a usage error that names the option.
