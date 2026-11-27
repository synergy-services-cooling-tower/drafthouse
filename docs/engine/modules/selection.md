# `drafthouse-engine` · `selection`

Component selection — port of `src/core/selection.js`, with the ranking replaced by the
engineering-objective semantics owner-routed from issue #1 into this port (issue #1, 18
September 2026: "this should land in the Rust port (#5), not in the JavaScript engine").

What is a faithful port, line for line: the candidate generation (towers → fills → fill
depths → drift eliminators → fans → speed ratios), every feasibility constraint and its
rejection label, the per-candidate thermal/air-side/fan/nozzle physics, and the inlet-air
state. What deliberately diverges from the reference — and from nothing else:

* **No economics.** The reference computes `candidateEconomics()` and sorts by it first;
  this port has no commercial field anywhere, reads none of the catalog's rate fields and
  produces no economics. The superseded reference ranking is documented in
  `rust/README.md`, not silently overwritten.
* **The ranking is the run's declared objective.** Duty and thermal margin are
  constraints, never sort keys. The four objectives are [`Objective`]; the default is
  least over-capacity — the smallest unit that meets the duty with the required margin,
  i.e. the smallest [`SelectionCandidate::capacity_kg_s`] (the water flow the unit holds
  at the required cold-water temperature). See `rust/README.md` for why the sort key is
  the capacity rather than the reported capability ratio.
* **Two quantities are reported per candidate that the reference never computed**:
  `capacityKgS` (an inverse solve at fixed cold-water temperature on the candidate's own
  physics) and `capabilityRatio` (available KaV/L over required KaV/L).

Three functions of `src/core/water.js` and one of `src/core/merkel.js` that the reviewed
slices left out ([`water_density_kg_m3`], [`heat_rejection_kw`],
[`volumetric_flow_m3_s_from_mass_flow`], [`estimate_outlet_air_state`]) are ported in this
file, because this lane's file fence keeps the already-reviewed modules
byte-identical. They are marked as such below; no physics is invented.

## Notes carried from the source comments

### `const SELECTION_WARNING`

The warning the reference returns, minus its economics clause: this port reads no rate
data, so it cannot promise anything about it. Documented in `rust/README.md`.

### `fn js_max`

Reference's `Math.max(a, b)`: NaN-propagating, unlike `f64::max`.

### `fn js_min`

Reference's `Math.min(a, b)`: NaN-propagating, unlike `f64::min`.

### `fn js_cmp`

The reference's sort comparators are sign-of-difference; NaN compares as equal, which keeps
the sort stable exactly as the JavaScript `Array.prototype.sort` does.

/* ---------------- the unported remainder of `water.js` / `merkel.js` ---------------- */

### `fn water_density_kg_m3`

Port of `waterDensityKgM3` from `src/core/water.js` (see the module header for why it
lives here). The reference's 0–100 °C refusal and its `Math.max(0, salinity)` floor are
kept.

Spelled as two comparisons rather than `(0.0..=100.0).contains(&t)`: for a NaN input both
comparisons are false and the value flows on, exactly as `t < 0 || t > 100` does in the
reference, whereas `contains` would refuse NaN.

### `fn heat_rejection_kw`

Port of `heatRejectionKW` from `src/core/water.js`.

### `fn volumetric_flow_m3_s_from_mass_flow`

Port of `volumetricFlowM3SFromMassFlow` from `src/core/water.js`.

### `fn estimate_outlet_air_state`

Port of `estimateOutletAirState` from `src/core/merkel.js`. The reference's `cpWaterKJkgK`
and `pressurePa` defaults are not ported: the selector always passes both explicitly.

/* ---------------- the selector's catalog records ---------------- */

### `enum TowerType`

`tower.type` — the reference compares it as a string.

### `fn as_str`

The JavaScript spelling.

### `fn parse`

Parse the JavaScript spelling; an unknown string is the caller's usage error.

### `struct CatalogMetadata`

Catalog metadata — `id`, `revision` and `status` only. The reference metadata also carries
a commercial field; this port does not read it.

### `struct WaterQualityFactor`

`catalog.waterQualityFactors[name]` — the reference record also carries a third field that
exists only for its economics; this port reads only the two physics multipliers.

### `struct SelectionTower`

A tower record plus the selection-only fields `selection.js` reads.

`fill_stacks` is the ordered-layer contract (issue #54): the tower's **stack variants**, each
a complete ordered list of layers that names its own fill ids, depths and multipliers. A
tower that declares variants is selected over them — compare mode over complete stacks —
instead of over `fill_depth_options_m` × the catalog's fills; `fill_depth_options_m` stays
the single-fill contract's own enumeration (one layer per fill × depth option).

### `struct SelectionFill`

A fill record plus its selection-only compatibility fields.

### `struct SelectionDriftEliminator`

A drift-eliminator record plus its selection-only temperature limit.

### `struct SelectionFan`

A fan record plus its selection-only speed window and efficiencies.

### `pub nominal_rpm: Option<f64>,`

The speed at which the recorded curve is published — speed ratio 1.0 — in rpm. The rpm a
read-out shows at a ratio is `speed_ratio × nominal_rpm`; `None` when the record states
none, because an unstated datum is never invented (see [`SelectionFan::rpm_at_speed_ratio`]).

### `struct SelectionCatalog`

The catalog the selector runs against — the records `selection.js` reads, never fetched.

/* ---------------- requirements and the run's objective ---------------- */

### `enum Objective`

The run's declared ranking objective. Duty and thermal margin are constraints under every
one of these; they are never sort keys.

### `LeastOverCapacity,`

The default: the smallest unit that meets the duty with the required margin, i.e. the
smallest `capacityKgS` among the feasible candidates.

### `LowestElectricalInput,`

Smallest `electricalInputKW`.

### `LowestMakeupWater,`

Smallest `makeupKgS`.

### `LowestTotalAirSidePressure,`

Smallest `airside.totalPa`.

### `const ALL`

The four objectives, in the order the README documents them.

### `fn as_str`

The reference-facing spelling (`--objective` accepts exactly this).

### `fn parse`

Parse the spelling; an unknown string is the caller's usage error.

### `struct SelectionRequirements`

The reference's `defaultSelectionRequirements()`, minus the six inputs that only fed its
economics (the energy and water rates, annual maintenance, analysis period and present-value
rate, and the operating hours); `rust/README.md` names them.

### `pub water_mass_flow_kg_s: f64,`

Default 200 kg/s.

### `pub hot_water_c: f64,`

Default 42 °C.

### `pub target_cold_water_c: f64,`

Default 32 °C.

### `pub wet_bulb_c: f64,`

Default 27 °C.

### `pub dry_bulb_c: f64,`

Default 33 °C.

### `pub pressure_pa: f64,`

Default 101 325 Pa.

### `pub salinity_g_kg: f64,`

Default 0 g/kg.

### `pub water_quality_class: String,`

Default `"moderate"`.

### `pub cycles_of_concentration: f64,`

Default 4.

### `pub max_drift_ppm: f64,`

Default 30 ppm.

### `pub max_electrical_input_kw: f64,`

Default 75 kW.

### `pub max_footprint_m2: f64,`

Default 130 m².

### `pub minimum_thermal_margin_c: f64,`

Default 0.3 °C — a real margin, because the default objective must not settle on a
unit sitting exactly on the thermal constraint (the reference's own note).

### `pub nozzle_pressure_drop_pa: f64,`

Default 65 000 Pa.

### `pub speed_ratios: Vec<f64>,`

Default `[0.78, 0.88, 0.98, 1.08]`.

### `struct SelectionInput`

Inputs for [`run_selection`] / [`select_cooling_tower_components`].

### `pub max_results: usize,`

The reference's `maxResults`, default 30.

### `fn new`

The reference defaults: `defaultSelectionRequirements()`, the default objective
(least over-capacity) and `maxResults` 30.

### `fn with_requirements`

Override the requirements wholesale, as the reference's spread does.

### `fn with_objective`

The run's declared objective.

### `fn with_max_results`

The reference's `maxResults` argument.

/* ---------------- results ---------------- */

### `struct ThermalModel`

The per-candidate thermal model, flattened to the fields the selector reports. The two
branches keep their reference `model` string.

### `struct SelectionCandidate`

One feasible candidate — everything the reference pushed into `feasible`, plus the two new
reported quantities and minus the economics object it also pushed.

### `pub rank: usize,`

1-based position in the declared objective's order (assigned to the reported slice).

### `pub fill_id: String,`

The fill stack's identity: the fill id for a one-layer stack (the single-fill
candidate's identity, byte for byte), or the stack's own name — `A@0.45+B@0.9` — for a
mixed stack.

### `pub fill_depth_m: f64,`

The stack's total depth, top to bottom.

### `pub fill_layers: Vec<FillLayerResult>,`

The stack's layers, in physical order (top first), with the results each layer
contributed.

### `pub capacity_kg_s: Option<f64>,`

The water flow this unit holds at the required cold-water temperature and wet bulb.
`None` only if the inverse solve refuses; the bundled catalog resolves all of them.

### `pub capability_ratio: Option<f64>,`

Available KaV/L over required KaV/L at the design duty.

### `fn objective_metric`

The metric the objective ranks by.

### `fn identity`

The candidate's identity as matched between the engines: tower, fill, drift
eliminator, fan, fill depth, speed ratio.

### `struct SelectionRun`

The full feasible set, ranked under the declared objective.

### `fn feasible_candidate_count`

The number of feasible candidates, as the reference counts it.

### `fn order_for`

The whole feasible set's order under one objective, as indices into
[`SelectionRun::candidates`].

### `struct SelectionResult`

The reference-shaped result: the declared objective's first `maxResults` candidates, each
with its 1-based rank.

/* ---------------- the ranking ---------------- */

### `fn rank_candidates`

Rank the feasible set under `objective`.

The primary key is the objective's own metric — never the duty and never the thermal
margin, which are constraints. Ties are broken by the remaining metrics in a fixed,
documented order (electrical input, make-up water, total air-side pressure, capability
ratio, capacity) and finally by the candidate identity, so every order is total and
reproducible from the objective alone.

### `fn compare_optional`

A metric that may be unresolved: `None` sorts after every `Some`.

/* ---------------- the per-candidate physics ---------------- */

### `struct CandidateContext`

Everything one candidate's physics needs from the run.

### `stack: &'a FillStack,`

The candidate's fill stack, and its layers resolved against the catalog. A one-layer
stack is what the single-fill contract enumerates.

### `fn breakdown`

The reference's `systemPressureFn`, now the layered breakdown: the whole stack is
evaluated at one flow and one water flow, and its fill terms are the layers' totals.

### `fn system_pressure_pa`

`systemPressureFn` — the reference's per-flow breakdown closure.

### `fn unit_airside`

The fan operating point and the air-side breakdown at one water flow — the two calls
the reference makes before the thermal model.

### `fn unit_cold_water_c`

The whole unit at one water flow: air-side, fan balance, then the thermal model. The
capacity inverse solve walks this closure.

### `fn capacity_kg_s`

`capacity` — the water flow the unit holds at the required cold-water temperature,
by bracketed root on the unit's own physics. The duty flow is the lower bracket: it is
feasible, so its cold water is at or below the target. The upper bracket expands by
1.5× until the unit's cold water crosses the target. `None` if the expansion or the
solve refuses (which the bundled catalog never triggers).

The duty flow is the lower bracket: every candidate here already cleared the
thermal-margin constraint, so its cold water sits at or below the target.

### `fn capability_ratio`

`capabilityRatio` — available KaV/L over required KaV/L at the design duty.

Counterflow: the required KaV/L is the Merkel demand evaluated at the required
cold-water temperature, which is the model's own inverse (`solveColdWaterTemperature`
solves demand(cold) = available), so no new solve is needed. Crossflow: the grid has no
scalar demand form, so the same quantity is bracketed and bisected on `availableMerkelNumber`
with the already-ported root machinery.

### `fn thermal_model`

The candidate thermal model — the reference's `thermalModelForTower`, where the crossflow
branch is the grid solve and everything else is the counterflow Merkel solve plus the
saturated outlet-air estimate.

### `enum CandidateFailure`

Why a candidate is not feasible: a labelled constraint failure, or a physics refusal the
reference's `catch` maps onto `'numerical or curve domain'` (the reference discards the
error object there too).

### `fn evaluate_candidate`

The reference's per-candidate body — everything inside its `try`, plus the whole build.

Every layer of the stack is checked against its OWN fill's limits at the loadings the
stack sees; a one-layer stack is exactly the single-fill envelope the reference checks.

The two new reported quantities. A refusal here must never remove the candidate from
the feasible set — the reference has no such step — so both degrade to `None`.

### `fn id`

The fan's identity, as `fan.id` in the reference.

### `fn rpm_at_speed_ratio`

The fan's speed in rpm at a speed ratio: `nominal_rpm × speed_ratio`.

`None` when the record states no rated speed: the ratio is still the engine's own input, but
there is no datum to turn it into an rpm, and nothing here invents one.

### `fn speed_ratio_limit`

`None` inside the fan's validity band; the named [`FanSpeedRatioLimit`] outside it.

### `struct FanSpeedRatioLimit`

The fan record's validity band, refused **by name** rather than clamped: the ratio a run asked
for against the band the record allows (`allowed_speed_ratio`).

A caller can tell "outside the fan's validity band" (this) from a computed value; nothing on
this path rounds a ratio into the band and reports it as if the engine had evaluated it.

### `const LABEL`

The rejection label the selector counts this refusal under (the reference's own spelling).

### `fn fan_speed_ratio_limit`

`None` inside the band `[min, max]`; the named [`FanSpeedRatioLimit`] outside it. The band-level
form of [`SelectionFan::speed_ratio_limit`], so a caller that carries the record's own band (the
adapter, checking its input) names the same outcome without rebuilding a record.

/* ---------------- the run ---------------- */

### `fn count_rejection`

Count one rejection under its label, keeping the reference's insertion order for the ties
its summary sorts (`counts.sort_by_key(Reverse(count))` is stable).

### `struct SweepContext`

Everything one tower's stack sweep needs: the catalog, the tower, the resolved stack and the
run's own inputs.

### `fn sweep_stack`

One resolved stack on one tower: sweep every drift eliminator, compatible fan and speed
ratio the requirements allow, appending the candidates the physics accepts and counting
every rejection under the reference's own label — in the reference's nesting order, so the
recorded rejection summary keeps its insertion order.

### `fn declared_stack_layers`

Resolve one declared stack variant against the catalog.

Every layer's fill must be in the catalog, usable on this tower's type and approved for the
run's water-quality class; a variant that fails any of those is not built, and the refusal
is counted under a named label. The returned terms borrow the catalog's own records, so the
sweep walks the declared layers in order without copying a record.

### `fn run_selection`

Run the selector and rank the whole feasible set under the declared objective.

The single-fill contract, unchanged: one layer per compatible fill × depth
option, in the reference's own nesting order.

Compare mode over COMPLETE stacks: each declared variant is one candidate
identity, and its layers may name different fill types, depths and multipliers.
A variant whose layers cannot be resolved is not built, and the reason is
counted — never a silent skip.

The reference sorts its rejection map by count, descending, keeping insertion order for
ties (a stable sort over the insertion-ordered map).

### `fn select_cooling_tower_components`

The reference-shaped selection: [`run_selection`], truncated to the first `maxResults`
candidates of the declared objective and ranked from 1.

### `fn default_selection_requirements`

Reference default requirements, exposed like `defaultSelectionRequirements()`.
