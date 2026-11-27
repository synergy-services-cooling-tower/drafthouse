# `cli` tests · `selection`

Component selection — port of `src/core/selection.js` with the engineering-objective
ranking of issue #1, tested against the reference engine's recorded behaviour.

Sources, quoted per test:

* `tests/selection.test.js` — 'default synthetic catalog returns feasible constrained
  selections'.
* `src/core/selection.js` — the candidate generation, the rejection labels and the
  per-candidate physics (the bundled catalog records below are quoted verbatim from
  `src/data/sampleCatalog.js`; all coefficients are illustrative synthetic data).
* issue #1's measured table (18 September 2026) — the four-objective anchors on the
  bundled catalog; the capacity and capability-ratio numbers were measured from the
  reference engine's own functions at this commit (`tools/parity/run.mjs` recomputes
  them independently on every run).

The JavaScript engine is the specification; nothing here re-derives a physics value.

## Notes carried from the source comments

### `top`

/* ---------------- the bundled sample catalog, quoted verbatim ---------------- */

### `fn fan`

These fixtures predate the record's rated speed; the rpm read-out is not what they test.

### `fn small_catalog`

The single-cell catalog the focused tests run on: `IDCF-064` with `FILM-OF25`, `DE-3P-10`
and `AX-500`.

### `fn identity`

/* ---------------- focused tests on the single-cell catalog ---------------- */

### `fn a_small_catalog_returns_feasible_constrained_candidates`

`tests/selection.test.js` — 'default synthetic catalog returns feasible constrained
selections' — on the single-cell catalog, every constraint checked on the winner.

The winner is the smallest unit: its capacity is the minimum over the feasible set.

### `fn the_capacity_inverse_solve_is_pinned_and_self_consistent`

The inverse solve, pinned. The case: the single-cell catalog's least-over-capacity winner
(`IDCF-064 / FILM-OF25 / DE-3P-10 / AX-500`, depth 1.5 m, speed ratio 0.78) at the
reference's default duty. The expected 225.32138116657734 was measured with the reference
engine's own functions (2026-09-19, `solveFanSystemIntersection` + `solveColdWaterTemperature`
per flow, then `solveBracketedRoot` on water mass flow, tolerance 1e-9 / xTolerance 1e-6 —
the same bracketed bisection `tools/parity/run.mjs` performs), and the harness reproduces
it for every candidate of the bundled catalog. The winner's identity is reproducible from
the selection itself.

Forward check in the port itself: re-evaluating the unit at the capacity must reproduce
the required cold-water temperature to the solver's own floor.

And the capability ratio is the reported available KaV/L over the required demand at the
required cold-water temperature.

### `fn re_evaluate_unit`

Re-evaluate one candidate's unit at one water flow, using only the public ported
primitives — the same call sequence `selection.js` makes, built independently here.

### `fn a_candidate_below_the_required_margin_cannot_rank_under_any_objective`

Duty and thermal margin are constraints, never sort keys: the candidate that ranks first
under `LowestElectricalInput` while the margin floor is 0.3 K is refused under every one of
the four objectives once the floor rises above its margin.

Raise the floor above that candidate's margin. It must leave every order, and the count
of 'thermal duty' refusals must rise by exactly one.

### `fn the_temperature_order_and_the_quality_class_are_refused`

/* ---------------- the bundled catalog: issue #1's anchors ---------------- */

### `fn the_bundled_catalog_winners_are_the_issue_1_anchors`

The four-objective anchors on the bundled catalog, matching issue #1's measured table
(least over-capacity and lowest electrical input) and the reference engine's own values
for the other two. Also checks that every order is a total order built from the declared
objective plus the documented tie-break chain, and that the margin floor holds in each.

The full-bundled-catalog run computes the capacity and capability ratio of all 202 feasible
candidates, so this test is the slow one (tens of seconds in a debug build).

Every order is the documented total order: the objective's own metric, then the
fixed tie-break chain, then the identity — recomputed here independently.

### `fn compare_documented`

The documented tie-break chain, recomputed in the test: objective metric, then electrical
input, make-up water, total air-side pressure, capability ratio, capacity, identity.

### `fn rank_candidates_agrees_with_the_run_order`

`rank_candidates` is the public seam the CLI's `--all-orders` uses; it must agree with the
order `run_selection` reports for the same objective.

/* ---------------- the tie-break chain's interior links ---------------- */

### `fn tie_base_catalog`

The base both exact-tie catalogs start from: the single-cell catalog at one fill depth, so
the order is the tie-break chain's and not a depth or speed grid's.

### `fn efficiency_tie_catalog`

The catalog of the **electrical-input** case. `AX-500-ECO` is `AX-500` record for record
except its **motor efficiency** — the fan curve, the stack area, the pressure basis, the
drive efficiency, the speed window and every physics input are identical. `motorEfficiency`
enters `electricalInputKW` and nothing else (`chooseStandardMotor` is called with the drive
efficiency), so the two units tie on the objective metric, on the air-side solve, on the
thermal solve, on make-up water and on the capability ratio: they differ on electrical input
alone.

The new id sorts *after* the base one (`AX-500` < `AX-500-ECO`) while its electrical input is
the lower of the two, so the identity tail orders the pair against the chain. A case that
dropped the interior links fails rather than passes.

### `fn drift_tie_catalog`

The catalog of the **make-up water** case. `DE-3P-10-LP` is `DE-3P-10` record for record
except the **drift-ppm column** of its curve; the pressure-drop column — the only one the
fan/system balance reads — is identical, so the operating point, the electrical input, the
capacity, the total air-side pressure and the capability ratio are bit-identical and the pair
differs on make-up water alone.

The make-up water is drift-sensitive only where the balance's blowdown term floors at zero
(`coolingTowerWaterBalance`: `blowdown = evaporation / (cycles − 1) − drift`, floored;
`makeup = evaporation + drift + blowdown`, in which the drift cancels algebraically while
blowdown stays positive). The case therefore runs at 3000 cycles of concentration, where the
base eliminator's blowdown floors and the halved one's does not — the two make-up values
below are pinned, so the case fails loudly if that premise ever stops holding.

### `fn the_electrical_link_orders_an_exact_objective_metric_tie`

The chain's **first** interior link, on a synthetic catalog with an exact objective-metric
tie: `efficiency_tie_catalog()`'s two units tie on the objective metric — the capacities
compared below are the same `f64`, not merely close — and on every link after the decisive
one, and differ on electrical input alone. The order asserted is the electrical link's, and
it is the reverse of the identity order.

The identity tail reads `AX-500` before `AX-500-ECO`, the opposite of the order the chain
produces: the case is evidence about the chain, not about the names.

### `fn the_makeup_link_orders_an_exact_objective_metric_tie`

The chain's **second** interior link, the same way: `drift_tie_catalog()`'s two units tie on
the objective metric, on electrical input, on total air-side pressure and on the capability
ratio, and differ on make-up water alone. The lower make-up water ranks first; the identity
order is again the reverse.

The premise of the case, pinned: the base eliminator's blowdown floors at zero (which is
what makes any make-up value drift-sensitive at all) and the halved one's does not.

### `struct LinkCase`

One row of the link-by-link discrimination table: two candidates whose metrics differ on
exactly the named links, and the order the documented chain must produce.

### `label: &'static str,`

What the row discriminates, for the failure message.

### `a: [f64; 5],`

`(electricalInputKW, makeupKgS, totalPa, capabilityRatio, capacityKgS)`

### `expected_first: &'static str,`

Which side the documented chain must rank first.

### `fn engineered_candidate`

A candidate of the row's pair: a real candidate from `small_catalog()` with its tie-break
metrics replaced by the row's values. The comparator reads those metrics as fields, and its
contract is the order it imposes on them; the rows below are that contract, one link at a
time. The tower ids (`TIE-B` for `a`, `TIE-A` for `b`) put the identity tail *against* the
row's expected order, so no row passes on the identity comparison.

### `fn link_cases`

The rows: for each of the chain's five interior links, a pair that ties on every *other*
metric and differs on that one; and for each adjacent pair of links, a pair whose two
metrics are anti-correlated, so the links' **order** is exercised as well as their
direction. `a` is always the side the documented chain must rank first.

Direction, link 1 of the chain: electrical input, ascending.

Direction, link 2: make-up water, ascending.

Direction, link 3: total air-side pressure, ascending.

Direction, link 4: capability ratio, ascending.

Direction, link 5: capacity, ascending. Its own objective would make it the primary
metric, so this row ranks under an objective it does not carry.

Order, links 1-2: electrical input and make-up water anti-correlated, so the pair is
ordered by which link the chain consults first.

Order, links 2-3: make-up water before total air-side pressure.

Order, links 3-4: total air-side pressure before capability ratio.

Order, links 4-5: capability ratio before capacity.

### `fn the_tie_break_chain_discriminates_every_interior_link`

Every interior link of the chain, discriminated: each row ties on the objective metric and
on every other link and is ordered by its own. A link whose direction or position moved
flips its row (and for the anti-correlated rows, the link *order* flips them too).

### `fn the_identity_tail_orders_a_full_metric_tie`

The identity tail on a full metric tie: every metric is equal, so the tail decides — and it
compares the tower before the fill, which matters because the two candidates disagree on the
two axes in opposite directions (`TIE-A`/`FILL-Z` against `TIE-B`/`FILL-A`).

/* ---------------- the fail-closed money guard ---------------- */

### `fn no_money_ish_identifier_exists_anywhere_in_the_port`

The same fail-closed spirit as `tests/injection.test.js`: a money-ish identifier must not
reappear anywhere in the port's source, and the selector's own result must not carry one.
The guard is demonstrated to bite in a scratch tree (see the lane report).

### `const TOKENS`

The result the CLI actually prints must be just as clean.

And a cost-shaped field is a usage error, never a silently ignored value.

### `fn collect_sources`

Walk `directory` for `.rs` sources, as `(path, text)`.

### `fn small_catalog_args`

The `ct-engine select` arguments of the single-cell catalog, with a representative
requirements set. Returns the argument list and the tower spec it contains (so a test can
poison that exact spec).

/* ---------------- issue #54: complete stack variants ---------------- */

### `fn pair_catalog`

The bundled catalog reduced to the records a mixed stack needs: `IDCF-064` on `AX-500`
through `DE-3P-10`, with the two counterflow/crossflow fills the variant cases below use.

### `fn variant_requirements`

The reduced duty the recorded `select` case uses, which keeps both variants feasible.

### `fn variant_stacks`

The mixed stack issue #54 names, over the single-layer variant of the same tower.

### `fn a_tower_without_declared_stacks_keeps_the_single_fill_enumeration`

Acceptance criterion 1's regression pin: a tower that declares no stacks is enumerated
exactly as before — one layer per compatible fill × depth option — and every candidate
carries that one layer, so the identity is the fill's own.

### `fn a_tower_selects_over_its_complete_stack_variants`

Acceptance criterion 6: the declared variants are the identities compared — a mixed
stack and a single-layer one — each carrying its own layers.

The candidate's fill term is its layers' own total, and the layers ride on the stack.

### `fn the_selection_surface_carries_the_layers_into_the_worked_steps`

Acceptance criterion 7: the data-only surface carries the layers through — the reply's
`fillLayers` and the worked sheet's per-layer steps name the same layers in the same
top-first order.

The mixed stack is the recommended unit at this duty (least over-capacity); the
single-layer variant is the other feasible candidate. Both are complete stacks.
