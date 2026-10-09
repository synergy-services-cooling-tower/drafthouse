# synergy-drafthouse (Rust) — ported slices 1 to 4

The Rust engine for this repository. **The JavaScript engine in `../src` remains the
reference**: this crate is judged by the parity harness (`../scripts/parity/run.mjs`), not by
its own opinion of the physics. Four vertical slices are ported so far, each with external
anchors and recorded vectors:

* **slice 1** — psychrometrics and counterflow Merkel demand;
* **slice 2** — air-side losses and fan balance;
* **slice 3** — water balance;
* **slice 4** — crossflow grid, convergent-nozzle hydraulics and component selection.

The catalog-record validator from issue #5 is not a slice of the reference port: it is
`docs/CATALOG_SCHEMA.md` as machine-enforced data, and it is documented in its own section
below.

## What is ported

| JavaScript (`src/core/…`) | Rust | Notes |
|---|---|---|
| `numeric.js` | `src/numeric.rs` | `DomainError`, `assertFiniteNumber`, `assertPositive`, `clamp`, `integrateSimpson`, `integrateChebyshev4`, `solveBracketedRoot` (`BracketedRootOptions`), `findRootByScan` (`RootScanOptions`, `RootPreference`), `interpolate1D` (`interpolate_1d`) |
| `psychrometrics.js` | `src/psychrometrics.rs` | every function the slice needs, including the enhancement factor, the over-ice branch, `psychrometricState`, and (slice 4) `saturatedTemperatureFromEnthalpy` |
| `water.js` | `src/water.rs` | `waterSpecificHeatKJkgK`, `waterDensityKgM3`, `volumetricFlowM3SFromMassFlow` (slices 3 and 4) |
| `merkel.js` | `src/merkel.rs` | `validateThermalTemperatures`, `inletAirEnthalpyKJkgDryAir`, `airEnthalpyOperatingLineKJkg`, `merkelDemand`, `safeMerkelDemand`, `solveColdWaterTemperature`, and (slice 4) `estimateOutletAirState` |
| `airside.js` | `src/airside.rs` | `velocityPressurePa`, `minorLossPressurePa`, `zoneMerkelNumber`, `fillThermalMerkelNumber`, `fillPressureDropPa`, `driftPerformanceAtVelocity`, `resolveAirsideAreas`, `fanStackDischargePressurePa`, `systemPressureBreakdown`, `checkFillOperatingEnvelope` |
| `fan.js` | `src/fan.rs` | `fanOperatingLimits`, `fanPressurePaAtFlow`, `fanEfficiencyAtFlow`, `fanShaftPowerKW`, `solveFanSystemIntersection`, `estimateAirflowFromFanPower`, `chooseStandardMotor` |
| `waterBalance.js` | `src/water_balance.rs` | `evaporationFromAirMassBalance`, `driftLossKgS`, `coolingTowerWaterBalance` (through `WaterBalanceInput`, below) |
| `crossflow.js` | `src/crossflow.rs` | `solveCrossflowGrid`, `crossflowConvergenceStudy`, and the internal explicit finite-volume sweep, cell by cell |
| `nozzle.js` | `src/nozzle.rs` | `nozzleFlowM3S`, `selectNozzleArrangement` |
| `selection.js` | `src/selection.rs` | candidate generation, feasibility constraints, `airside`/`fanOperatingPoint`/`thermal`/`waterBalance` per candidate, `capacity` and `capabilityRatio`, and the engineering-objective ranking — with the reference's economics left out deliberately (below) |
| — | `src/cli.rs` | JSON command surface used by the parity harness and by the wasm export; no JavaScript counterpart |
| — | `src/wasm.rs` | the wasm C ABI over `cli` (issue #24), built only for `wasm32-unknown-unknown`; no JavaScript counterpart |
| — | `src/bin/ct-engine.rs` | the process shell over `cli`; no JavaScript counterpart |

Name mapping: JavaScript `camelCase` → Rust `snake_case`, with option objects replaced by
structs. `psychrometricState({…})` becomes `psychrometric_state(PsychrometricStateInput)` —
build it with `from_wet_bulb(…).with_pressure(…)` or
`from_relative_humidity(…).with_enhancement_factor(…)`. `merkelDemand({…})` becomes
`merkel_demand(&MerkelInput)` with `MerkelInput::new(hot, cold, wet_bulb, dry_bulb, lg)` plus
field overrides; `solveColdWaterTemperature({…})` becomes
`solve_cold_water_temperature(&ColdWaterTemperatureInput)`.

Slice 2 follows the same pattern, with three shapes worth naming explicitly:

* **Catalog records are structs with public fields**, reduced to the fields the slice reads:
  `TowerRecord`, `FillRecord` (with `ZoneCorrelation`, `FillPressureCorrelation`,
  `FillLimits`), `DriftEliminatorRecord` (+ `DriftCurvePoint`) and `FanRecord`
  (+ `FanCurvePoint`). The JavaScript `??` fallbacks for absent fields are `Option<T>` on the
  struct, and the default is applied inside the ported function, in the reference's order.
* **Small inputs are positional arguments**: `velocityPressurePa(rho, v)`,
  `minorLossPressurePa(k, rho, v)`, `fanPressurePaAtFlow(&fan, flow, speed_ratio, rho)`,
  `fanEfficiencyAtFlow(&fan, flow, speed_ratio)`, `fanShaftPowerKW(&fan, flow, pressure,
  speed_ratio)`, `chooseStandardMotor(shaft_kw, drive_efficiency, service_factor)`.
* **Large inputs keep the options-object shape as a public-field struct**:
  `SystemPressureBreakdownInput { tower, fill, fill_depth_m, drift_eliminator, fan, … }`,
  `FanStackDischargeInput { … }`, `FanSystemIntersectionInput::new(&fan, air_density)` (with
  `with_speed_ratio` / `with_minimum_flow_fraction` for the reference's default arguments),
  `FillOperatingEnvelopeInput { … }`, `AirflowEstimateInput { … }`. Fields the reference can
  return as `null` are `Option<f64>` (`fanStackVelocityMS`, `fanStackAreaM2`,
  `selectedMotorKW`).

Slice 3 is three functions, so it is mostly the two shapes above, with one addition worth
naming: the reference's **default arguments** (`driftKgS` 0, `cyclesOfConcentration` 4) live
on `WaterBalanceInput::new(evaporation_kg_s)`, with `with_drift_kg_s` /
`with_cycles_of_concentration` for the cases that override them, and on the CLI by leaving
`--drift` / `--cycles` out. `evaporationFromAirMassBalance` and `driftLossKgS` take their
arguments positionally, and the two `Math.max(0, …)` floors keep the reference's NaN
behaviour: a NaN input yields NaN rather than a silent zero.

Slice 4 is three modules:

* `crossflow.rs` mirrors the reference's explicit finite-volume sweep cell by cell (freeze the
  inlet air, sweep the water column down the grid, then integrate the air side against the
  frozen water matrix, twice — the coarse pass and the fine pass with one more air cell per
  cell — then Richardson-extrapolate). `solve_crossflow_grid(&CrossflowGridInput)` takes the
  five thermodynamics inputs plus `available_merkel_number`; `with_air_cells` (18 by default,
  the reference's own default) and `with_richardson(false)` mirror the documented options.
  `crossflow_convergence_study(&CrossflowStudyInput)` runs the coarse/medium/fine triple and
  reports the observed order and the extrapolated temperature.
* `nozzle.rs` is two functions with the reference's fallback spelled out:
  `nozzle_flow_m3_s(discharge_coefficient, orifice_diameter_m, pressure_drop_pa,
  water_density_kg_m3)` — the `waterDensityKgM3 = 997` default — and
  `select_nozzle_arrangement(&NozzleArrangementInput)`.
* `selection.rs` is the whole search: build every tower × fill × depth × drift eliminator ×
  fan × speed-ratio candidate, apply the feasibility constraints, compute the per-candidate
  air-side, fan operating point, thermal model and water balance, then rank. Name mapping:
  `selectCoolingTowerComponents({ requirements, catalog, maxResults })` becomes
  `select_cooling_tower_components(&SelectionInput::new(&catalog))`, with
  `with_requirements(SelectionRequirements)` and `with_max_results(n)`; the return is a
  `SelectionRun` whose `results` are `SelectionCandidate`s (ranked) and whose `candidates` are
  the whole feasible set in generation order. Each refusal reason the reference counts in its
  `rejectionSummary` is `CandidateFailure::Constraint(&'static str)` in the port, so the same
  strings come out; a numerical or curve-domain refusal is `CandidateFailure::Domain`, the
  reference's `catch` for `'numerical or curve domain'`.

## Selection ranking: the engineering objectives

The reference ranks feasible candidates by **whole-life cost** — `candidateEconomics()` sorts
on `lifecycleCost` (capex from `tower.baseCost`, `fill.costPerM3`, `driftEliminator.costPerM2`,
`fan.cost` and a motor estimate, plus the present value of annual energy, water and
maintenance cost at `energyCostPerKWh`, `waterCostPerM3`, `annualMaintenanceCost`,
`analysisYears`, `discountRate` and `operatingHoursPerYear`, plus a per-quality-class
`riskPenalty`).

**That ranking is superseded, and not ported.** Issue #1 routed the selector onto engineering
objectives; the owner's decision is that money does not enter this crate. Concretely, `src/selection.rs`,
`src/crossflow.rs` and `src/nozzle.rs` read none of the fields above, `SelectionRequirements`
carries none of the cost inputs (only the physics ones — flow, temperatures, pressure,
salinity, quality class, drift, electrical and footprint ceilings, thermal margin and nozzle
pressure drop), and no type in the crate can hold a price. `tests/selection.rs` enforces this:
`no_money_ish_identifier_exists_anywhere_in_the_port` walks every file under `rust/src` and
fails on `cost`, `price`, `currency`, `capex`, `discount`, `lifecycle`, `money`, `usd`, `thb`
or `penalty` in any spelling, in the code *or* in the CLI's selection output.

Ranking is instead over four objectives (`Objective::ALL`), each a metric the port already
computes:

| Objective (`--objective`) | Metric |
|---|---|
| `least-over-capacity` (default) | `capacity` — the water flow the unit holds at the required cold-water temperature (kg/s) |
| `lowest-electrical-input` | `electricalInputKW` |
| `lowest-makeup-water` | `waterBalance.makeupKgS` |
| `lowest-total-air-side-pressure` | `airside.totalPa` |

For every objective the sort is the objective's metric first, then a fixed tie-break chain
(electrical input, make-up water, total air-side pressure, capability ratio, capacity), then
the candidate identity (tower, fill, drift eliminator, fan, fill depth, speed ratio) with
`f64::total_cmp`-style comparison, so a tie is reproducible rather than input-order
coincidence. Duty flow and the thermal margin are **constraints, never sort keys**: a candidate
whose cold water does not clear `targetColdWaterC` by `minimumThermalMarginC` is rejected
before ranking, and `--all-orders` prints the whole feasible set's order under all four
objectives.

Two quantities are new because the reference never needed them: `capacity` (bracketed root on
water mass flow, re-solving the air-side at each flow) and `capabilityRatio` (available KaV/L
over required KaV/L at the duty; for crossflow, a root on `availableMerkelNumber`). Both are
solved with the already-ported `solveBracketedRoot`, and `scripts/parity/run.mjs` recomputes
them from the reference engine's own functions and diffs them per candidate.

`interpolate1D(points, x, xKey, yKey, { clampEnds })` becomes
`interpolate_1d(points, x, x_of, y_of, clamp_ends)`: the JavaScript key names are accessor
closures, so one primitive serves every record shape. Its edge behaviour is mirrored, not
"fixed" — clamped or linearly extrapolated ends, tables sorted on a copy, duplicate abscissae,
and the two refusals (`At least two interpolation points are required.`,
`Interpolation failed unexpectedly.` for an input that cannot be bracketed).

## What is not ported

Natural draft, the characteristic/performance capability tables, the worked-steps renderer,
and the sample catalog data (the catalog records live in `../src/data/sampleCatalog.js` and are
handed to this crate, never fetched by it). The reference's selection **economics** are not
ported by decision, not by omission — see the ranking section above. No UI concerns and no
egui. The wasm build arrived with issue #24 (below) and is a second *build* of this crate, not
a second engine.

## The wasm build (issue #24)

The engine compiles for `wasm32-unknown-unknown` with no dependency and no binding
generator. `src/cli.rs` holds the command surface once; `src/bin/ct-engine.rs` is its process
shell and `src/wasm.rs` — compiled only for the wasm target — is a three-function C ABI over
it, so the two builds cannot drift into two parsers. `../rust/wasm/binding.mjs` is the thin
JavaScript binding: data in, data out, no DOM, no geometry, no markup. The full story, the
ABI, the binding's API and the equivalence check are in `../docs/WASM_ENGINE.md`.

## The catalog-record validator (issue #5)

`docs/CATALOG_SCHEMA.md` was prose with nothing enforcing it: a mistyped, missing or
non-finite field became `undefined`/`NaN` and flowed into a ranking. `src/validate.rs` is
that schema as data — one table per record type (`tower`, `fill`, `drift-eliminator`, `fan`,
`nozzle`) declaring every field, whether it is required, and what its value must be — and
`ct-engine select` runs it over every record **before** the catalog is parsed or the
selector is built, so nothing invalid can reach a ranking. `ct-engine validate-catalog` runs
the same gate without selecting and prints the record counts.

What is enforced, per record:

* **Required fields.** The split comes from the port's own record types: a field the struct
  holds as a plain `f64`/`String` (or the CLI's `required*` accessors) is required, an
  `Option` field is optional. A record that omits a required field is refused, with the
  record and the field named.
* **No undeclared fields at all.** A mistyped name (`fillAreM2`) or any field the record
  type does not declare is refused, never dropped — the fail-closed property. Field names
  are the JavaScript catalog's.
* **Numbers are finite.** `NaN`, `Infinity`, `-Infinity` and literals that overflow to
  infinity (`1e999`) are refused; that is exactly the value class that used to reach a
  ranking. A value that is not a number at all is refused as such.
* **Declared domains.** Areas, masses/flows, densities, depths, efficiencies, discharge
  coefficients, curve flows and drift elimination rates are positive or non-negative; the
  fill/zone power-law exponents are signed and only checked finite; `type`, `pressureBasis`
  and the `compatibleTowerTypes`/`allowedWaterQualityClasses` sets are enumerated; the
  five-column zone/pressure/limits rows and the `flowM3S:pressurePa:efficiency` /
  `faceVelocityMS:driftPpm:pressureDropPa` curves are checked for shape; a curve needs at
  least the two points `interpolate1D` already demands; `allowedSpeedRatio` must be
  `lower|upper` with `lower <= upper`; `nominalRpm` is optional and, when present, positive
  (issue #59: the speed the fan's recorded curve is published at, speed ratio 1.0).
* **Duplicate field names** in one record are refused — the first would otherwise win
  silently.

What is deliberately **not** enforced (all of it stated here so the gap is visible):
cross-field relations (`minWaterLoadingKgM2S <= maxWaterLoadingKgM2S` is not checked),
curve monotonicity or sortedness, referential integrity (`compatibleFanIds` may name a fan
the catalog does not carry — the selector then never matches it), uniqueness of record ids,
and upper bounds the schema does not declare (an efficiency above 1 is out of range
physically but the schema states no ceiling; the engine's own guards stay where they are).
Catalog `metadata` and `waterQualityFactors` are not record types and stay typed CLI
options; the standard-motor table (`STANDARD_MOTOR_SIZES_KW`) is crate data, not a catalog
record.

### The reference catalog's four commercial fields

The bundled `src/data/sampleCatalog.js` carries the reference engine's four commercial
fields (`baseCost` on towers, `costPerM3` on fills, `costPerM2` on drift eliminators,
`cost` on fans), and the schema prose shows them. The machine schema does **not** declare
them, for a reason of this repository's own making: `tests/selection.rs::no_money_ish_identifier_exists_anywhere_in_the_port`
fails if any of their spellings appears anywhere under `rust/src`, and issue #1's decision
is that money does not enter this crate. So the handling is:

* the bundled data is **untouched** — `src/**` is byte-identical, and the JavaScript
  reference keeps reading those fields;
* **no type in the crate can hold one**, and no selection result carries one;
* a record that carries one is **refused by name** ("does not know the field"), exactly the
  usage error behaviour slice 4 shipped — validation did not relax it;
* the machine schema is therefore the **port's record shape** (what `selection.js` feeds the
  selector, plus the declared fields the port does not read: `name`, `draftType`,
  `effectiveDraftHeightM`, `geometry`, `material`).

Two layers answer two different questions, and they are not the same gate. `validate-catalog`
answers the *catalog-format* question: a record is well-formed for its type. `select` stays
stricter on its own intake: it still refuses every field it does not read — the
declared-but-unread fields above included — with the long-standing `does not know the field`
usage error, so a record `validate-catalog` accepts can still be a `select` usage error,
never the reverse. The schema gate (required fields, undeclared names, finiteness, declared
domains) runs before either and names the record and the field.

The one consequence, stated plainly: the bundled records validate clean **through the
projection the parity harness already applies** — its spec builders never pass a commercial
field — and the harness now holds that projection against the schema. A *raw* catalog record
that still carries e.g. `baseCost` is refused by the validator; that refusal is the port's
commercial boundary, not a data error. If a future slice wants the catalog file itself
validated verbatim, the four names have to be declared somewhere the port's money guard
allows, which is an owner decision, not a validator one.

## How to run

```sh
cd rust
cargo test                                # 125 tests: 27 anchored, 20 air-side, 13 fan,
                                          # 10 water balance, 8 crossflow, 12 selection,
                                          # 4 nozzle, 4 recorded-vector, 15 validator,
                                          # 7 numeric unit, 5 wasm-engine
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The breakdown above is `cargo test`'s own per-target `running N tests` line — the names map one
to one onto `tests/*.rs`, plus `src/lib.rs` for the numeric unit tests — so it can be re-derived
rather than trusted:

```sh
cargo test 2>&1 | grep -E 'Running tests/|Running unittests|^running [0-9]+ tests'
```

Parity harness, from the repository root, one command:

```sh
node scripts/parity/run.mjs                # builds the crate, then diffs both engines
node scripts/parity/run.mjs --no-build     # reuse an existing rust/target/debug/ct-engine
node scripts/parity/run.mjs --engine wasm  # build and diff the wasm build as well (issue #24)
```

`--engine wasm` builds the wasm artifact, runs this whole suite through it
(`rust/wasm/binding.mjs`) against the reference, and cross-checks every reply against the
binary; see `../docs/WASM_ENGINE.md`.

The wasm build on its own:

```sh
cargo build --manifest-path rust/Cargo.toml \
    --target wasm32-unknown-unknown --profile wasm --lib
```

It runs the JavaScript engine and the Rust binary over the recorded vectors
(`validation/test-vectors.json`) and the cases documented in this repository's JavaScript
suite and design docs, diffs every quantity per a **named tolerance**
(`pass when |Δ| <= abs OR |Δ|/|JS| <= rel`, printed in the table header), and exits non-zero
if anything is out of tolerance, listing what. Every row names its case source.

The air-side and fan cases cannot be driven by scalar flags alone, so the CLI takes the
catalog records as `key:value` specs using the **JavaScript field names**, e.g.
`--tower fillAreaM2:64,inletAreaM2:32,sprayZoneHeightM:0.6` and
`--fan-curve 55:520:0.63,100:470:0.74,…` (the key lists live in `src/bin/ct-engine.rs`,
`TOWER_FIELDS` / `ZONE_FIELDS` / `PRESSURE_FIELDS` / `FILL_LIMIT_FIELDS`); the harness
serialises the bundled records into them. An unknown key is a usage error rather than a
silently ignored field.

The water-balance slice needs no records: its `evaporation`, `drift` and `balance` commands
take scalar flags — the dry-air flow, the humidity ratios and the circulating flow the
reference computed — and leaving `--drift` / `--cycles` out applies the reference's default
arguments.

Slice 4 adds `crossflow`, `convergence`, `nozzle-flow` and `nozzle` for the two new physics
modules, and `select`, which takes the whole catalog as record specs (the same `key:value`
form as the air-side slices): `--towers`, `--fills`, `--drift-eliminators`, `--fans`,
`--nozzles`, `--quality-factors name:thermal:pressure`, plus the catalog metadata and the
requirement flags (`--water`, `--hot`, `--target-cold`, `--wb`, `--db`, `--pressure`,
`--salinity`, `--quality-class`, `--max-drift`, `--max-electrical`, `--max-footprint`,
`--min-margin`, `--nozzle-dp`, `--speed-ratios`). Leaving a requirement flag out applies the
reference's default, so the harness exercises the defaults by omission. `--objective` selects
one of the four engineering objectives, `--max-results` limits the ranked list, and
`--all-orders` adds the whole feasible set's order under every objective. `select` validates
every record against the machine schema first (see the validator section below), so a record
spec that carries a field the selector does not know — including any price field — is a usage
error (exit 2), never a silently ignored value.

`ct-engine validate-catalog` runs that same schema over the same record options without
selecting anything, and prints the record counts it validated
(`{"towers":4,"fills":5,"driftEliminators":3,"fans":4,"nozzles":4,"records":20}`); the parity
harness uses it to hold the machine schema against the bundled catalog.

`ct-engine select-fields` prints the record field names `select` accepts, per catalog list —
the intake its record readers enforce. A caller that builds record specs (the wasm binding)
projects a catalog against that list instead of mirroring it, because a field the selector
does not read is a usage error, not a silently ignored value.

`ct-engine select` also reports, for the recommended candidate, the values the run used
(`inputs`, catalog dimensions included) and its worked sheet (`worked`, in the
`src/core/worked.js` shape — `title`/`purpose`/`steps`/`result`, each step carrying `label`,
`why`, `formula`, `substitution`, `value`, `unit`, `reference`, `kind`). Both are data only:
they re-derive nothing, and every value in them is a number the selection already computed.

The crate is not a workspace root, so `cargo` commands run in `rust/` (or with
`--manifest-path rust/Cargo.toml`); the repository root deliberately has no `Cargo.toml` —
the JavaScript reference and its port sit side by side until parity is proven.

## Conventions

* Zero runtime dependencies: the physics is standard library only. `serde_json` is a
  **dev-dependency**, used only by `tests/vectors.rs` because the vectors are JSON.
* The water-vapour enhancement factor is on by default; `PsychrometricOptions
  { enhancement_factor: false }` gives the simplified ASHRAE set.
* Out-of-domain input returns `DomainError`, never a clamped value: wet bulb above dry bulb,
  relative humidity outside 0…1, saturation pressure at or above total pressure, an enthalpy
  pinch, a missing state, a non-positive L/G or available `KaV/L`, temperatures outside the
  correlations, a fan curve with fewer than two points, a non-positive speed ratio, air
  density or fan efficiency, a total-pressure fan curve without a fan-stack area, a stack
  recovery factor outside `[0, 1)`, a missing fill correlation and non-positive zone heights
  or air flows are all refused.
* Any `.max()`/clamp in the port mirrors a line that carries one in the reference; the
  clamp-like default arguments (`?? 1.2`, `?? 0.8`, `stackRecoveryFactor ?? 0`) are applied
  where the reference applies them, in the same order relative to the guards.

## Deliberate deviations from the JavaScript reference

1. `DomainError` carries the message only; the JavaScript version also carries a `details`
   object, which nothing in the ported slices consumes. The message text is mirrored, but it
   is diagnostic text, not a compared quantity.
2. The solvers take fallible closures (`Fn(f64) -> Result<f64, DomainError>`), so a domain
   failure raised *inside* an integrand, a root function or a system-curve callback propagates
   as an error exactly as a JavaScript `throw` does, instead of being flattened into a
   non-finite value.
3. `inletEnthalpyConvention`, `integration` and the fan `pressureBasis` are Rust enums: an
   unknown value cannot be constructed at the library seam. The CLI reports an unrecognised
   spelling as a usage error (exit 2) where the JavaScript API raises a `DomainError`; both
   refuse, and the harness prints which kind of refusal each side gave.
4. `solveColdWaterTemperature` has no segment option in JavaScript either, so the inverse
   path always uses the reference default of 240 Simpson segments.
5. `chooseStandardMotor`'s `standardSizesKW` parameter is not exposed: no JavaScript caller
   overrides it, so the reference's IEC table is the crate's `STANDARD_MOTOR_SIZES_KW`.
6. `interpolate1D`'s key names become accessor closures (`interpolate_1d`), and the
   `checkFillOperatingEnvelope` failure messages are built with a local port of
   `Number.prototype.toFixed` because Rust's fixed-precision formatting rounds a different
   digit sequence (it prints `1.0005` with three digits as `"1.001"` where the reference
   prints `"1.000"`).
7. The recorded tolerances are the harness's, not the crate's: direct evaluations agree to a
   few ulp (`exp`/`ln`/`pow` differ between V8 and the platform libm), while root-solved
   temperatures inherit the solver's stopping width (1e-6 K for the dew point, 1e-7 K for the
   cold-water scan) and the fan operating point inherits the 1e-7 Pa residual tolerance of the
   scan. Measured deltas are printed per row by the harness; at the recorded cases the
   air-side, fan and water-balance rows currently differ by exactly zero. The water rows are
   exact by construction: every quantity is a product, quotient or difference of the same
   `f64`s on both sides, with no transcendental evaluation involved.
8. The ranking is by engineering objective, not by the reference's whole-life cost, and
   `capacity` / `capabilityRatio` are port additions solved with the ported root machinery.
   This is the one intentional behavioural difference in the crate; it is enforced by a test
   that fails if any money-ish identifier reappears anywhere under `rust/src` or in the
   selection CLI's output, and the harness recomputes both new quantities from the reference
   engine's own functions per candidate.
9. The crossflow sweep drops the reference's per-cell `totalHeatKW` accumulator: it is
   summed from the same cells but never read by any reference caller (the crossflow result's
   own `heatTransferKW` is recomputed from the water balance, which the port keeps). The water
   and air temperature grids, the extrapolated temperatures, the convergence block and the
   outlet state are all kept, and the harness diffs them through the CLI.
10. `SelectionRequirements` refuses `NaN`/non-finite at the seam like every other ported input
    (`assertFiniteNumber`), and a `waterQualityClass` the catalog does not define is a
    `DomainError`, matching the reference's throw rather than a silent fallback.
