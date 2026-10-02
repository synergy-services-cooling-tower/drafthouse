# Fill layers: the ordered stack contract (issue #54)

The engine's fill stack is an **ordered list of layers**, top first. This document is the layer
contract: what a layer is, what the engine computes from a stack, which refusals a stack earns,
and what the contract deliberately does not cover.

Everything below is the port's own contract over **synthetic fixtures** (`src/data/`, and the
`key:value` record specs a caller writes). Nothing here is a vendor or project report, and no
CTI/MRL certification is claimed anywhere.

## 1. What a layer is

A layer names the fill record it is built from, its own depth, and the multipliers that apply to
that fill's thermal and pressure characteristics:

| layer input | meaning |
|---|---|
| `fillId` | the fill record (its thermal characteristic, pressure characteristic and limits) |
| `depthM` | the layer's own depth, in metres |
| `thermalMultiplier` | the layer's own multiplier on the fill's **thermal** characteristic (1 = unmodified) |
| `pressureMultiplier` | the layer's own multiplier on the fill's **pressure** characteristic (1 = unmodified) |

A layer's **spelling** is `<fillId>@<depthM>[@<thermalMultiplier>[@<pressureMultiplier>]]`, and a
stack is its layers joined with `+`, top first — e.g. `FILM-OF25@0.45+FILM-VF38@0.9`. That
spelling is the stack's own name in every result (`stackLabel`, a candidate's `fillId` for a
mixed stack), so a stack read back out of a result parses into the stack that produced it.

Where the contract appears:

* the rate command `ct-engine layers --fill-stack <stack> --fills <records>` (§4);
* the selection tower field `fillStacks` — one or more stack variants, `|`-separated (§5);
* every result that carries layers: the `layers[]` array of a rate reply, each candidate's
  `fillLayers[]`, and the per-layer steps of the worked sheet (§6).

## 2. What the engine computes

The layers sit in **series** in both the air path and the water path, so every layer is
evaluated at the stack's water loading, the stack's dry-air loading and the stack's airflow
(`waterMassFlowKgS / fillAreaM2`, `dryAirMassFlowKgS / airFreeAreaM2`, `volumetricAirFlowM3S` —
the same quantities the single-fill breakdown resolves).

Per layer the engine evaluates the two ported fill functions at that layer's own inputs:

* thermal contribution (the layer's KaV/L equivalent): `KaV/L_layer =
  c · (L″/L″ref)^a · (G″/G″ref)^b · depthM · multiplier`, i.e. the ported
  `fillThermalMerkelNumber` at this layer's depth and effective thermal multiplier;
* pressure drop: the ported `fillPressureDropPa` at this layer's depth and effective pressure
  multiplier.

The **effective** multiplier of a layer is its own multiplier times the run's: `x_layer · x_run`.
The run-level multipliers reach the spray and rain zones above and below the stack exactly as
they do for a single fill.

The stack's totals are the layers' terms summed in physical order (top first):

* `layerTotals.pressureDropPa` — Σ pressure drops; this is the breakdown's `fillPa`;
* `layerTotals.merkelNumber` — Σ thermal contributions; this is the breakdown's
  `fillMerkelNumber`, and `layerTotals.availableMerkelNumber` adds the two zones to it.

Everything else in the reply — drift performance, the minor losses, the discharge term, the
zones — is the single-fill path unchanged, and the total keeps the single-fill sum's operand
order. **A one-layer stack is the single-fill contract**: it calls the same two functions with
the same operand order, which is what makes the published single-fill numbers the one-layer
case of this path rather than a parallel implementation of it. The recorded regression baseline
(`validation/regression-baseline.json`) is the drift guard for that case, and it is compared
through the parity harness like every other quantity — the layers are **ported and
drift-guarded, not validated**.

### 2.1 The per-layer result

Each layer's result is self-describing, so a caller can render or audit a stack without
re-deriving anything:

| field | meaning |
|---|---|
| `position` | 1-based position in the stack, **1 = top** |
| `label`, `fillId`, `depthM` | the layer's identity — the stack's own layer spelling |
| `thermalMultiplier`, `pressureMultiplier` | the layer's own multipliers, as declared |
| `effectiveThermalMultiplier`, `effectivePressureMultiplier` | the layer's × the run's |
| `thermal`, `pressure` | the characteristics the layer was computed from |
| `limits` | the limits the layer was checked against (§3) |
| `volumetricAirFlowM3S`, `dryAirMassFlowKgS`, `waterMassFlowKgS` | the flow the layer sees |
| `waterLoadingKgM2S`, `dryAirLoadingKgM2S`, `fillVelocityMS` | the loadings and velocity it sees |
| `pressureDropPa`, `merkelNumber` | what the layer itself contributes |
| `cumulativePressureDropPa`, `cumulativeMerkelNumber` | the running totals through this layer, so the last layer's cumulative value is the stack total |

## 3. Refusals: a layer outside its limits is refused, and the message names it

Every layered refusal names the layer it is about — `Layer <position> (<fillId>@<depthM>)` — and
nothing is clamped into range:

* an **empty** stack: `A fill stack must carry at least one layer.`
* a fill id no record in the catalog carries;
* a depth or a layer multiplier that is not positive (the reference's own `depthM must be
  positive.` / `thermalMultiplier must be positive.` texts, prefixed with the layer);
* a fill record without the thermal or pressure characteristic the layer needs;
* a layer **outside its own fill's limits** at this operating point. The ported envelope verdict
  supplies the body of the message (`Water loading … kg/(m²·s) is outside …–….`, `Dry-air loading
  …`, `Hot-water temperature … °C exceeds … °C.`, `Fill is not approved in the sample catalog for
  water-quality class “…”`), and the layer prefix names which layer hit which limit.

The loadings are always checked (they are computable from the stack's own inputs). The hot-water
temperature and the water-quality class are checked when the run declares them — a run that
declares no `--hot` is not making that claim, and the selector always supplies both.

In the **selection** path a declared variant whose layer fails one of the record-level checks
(fill not in the catalog, not usable on the tower's type, not approved for the run's
water-quality class) is not built, and the refusal is counted under a named rejection label. A
built candidate whose layer falls outside its limits at its operating point is rejected under
the selector's existing `fill operating envelope` label.

## 4. The rate flow: one selected stack

```
ct-engine layers --tower <spec> --fills <rec;...> --fill-stack <layer[+layer…]>
                 --drift-curve <v:ppm:pa,...> --flow <m3/s> --water <kg/s>
                 --dry-air-density <kg/m3> --moist-air-density <kg/m3>
                 [--spray <spec>] [--rain <spec>] [--fan-curve <q:pa:eff,...>]
                 [--fan-stack-area <m2>] [--fan-basis total|static] [--fan-ref-density <kg/m3>]
                 [--hot <C>] [--quality-class <name>]
                 [--thermal-multiplier <x>] [--pressure-multiplier <x>]
```

`--fills` takes the same fill-record encoding `select` reads, so a fill record authored for one
command is a fill record for the other. The reply carries `stackLabel`, `totalDepthM`,
`layers[]`, `layerTotals` and the air-side breakdown (`airside`, the same fields the `airside`
command reports, whose fill terms are the stack's totals).

## 5. Compare mode: complete stack variants

A selection tower may declare `fillStacks` — one or more complete stacks, `|`-separated:
`fillStacks:FILM-OF25@0.45+FILM-VF38@0.9|FILM-OF25@1.35`. Each declared variant is selected as
one candidate identity over the tower's drift eliminators, fans and speed ratios, so compare
mode compares **complete stacks** — including stacks whose layers use different fill types,
depths and multipliers.

* A **one-layer** stack keeps the single-fill identity: the candidate's `fillId` is the fill id
  and `fillDepthM` is that layer's depth, byte for byte what the recorded candidates carry.
* A **mixed** stack is its own identity: `fillId` is the stack spelling (`FILM-OF25@0.45+…`) and
  `fillDepthM` is the stack's total depth.
* Every candidate carries its `fillLayers[]`, so a comparison can show which layer contributed
  what.

`fillDepthOptionsM` remains the single-fill enumeration (one layer per compatible fill × depth
option). The two forms are both accepted; a tower that declares `fillStacks` is selected over
those variants *instead of* its depth options.

## 6. Worked steps

The worked sheet carries the stack through: one step per layer, in the same top-first order as
`fillLayers[]`, labelled `Fill layer <i> of <n> — <fillId> @ <depth> m`, with the characteristic
it was evaluated against, the loadings every layer of the stack sees, the multiplier that reached
it and its own transfer number. The fill total step that follows is the layers' sum.

## 7. What this contract does not cover

* **Natural draft, performance curves and the crossflow grid keep their own inputs.** The
  `natural-draft`, `curve-*` and `crossflow` commands read a single fill characteristic (or a
  single available KaV/L), and the selector hands them the stack's own `availableMerkelNumber`
  and dry-air mass flow — the layer split stops at the air-side and selection contract.
* **No per-layer water temperature.** The layers are a series composition of the ported fill
  functions at the stack's loadings; the contract does not redistribute the water temperature
  along the stack, and nothing here claims a vendor or CTI performance figure.
* **No UI.** This is the data contract; a surface renders the `layers[]` / `fillLayers[]` arrays
  and the worked steps as it likes (issue #52 owns the surface).

## 8. Where the code lives

| file | what it owns |
|---|---|
| `rust/src/layers.rs` | `FillLayer`, `FillStack`, `LayerTerms`, `FillLayerResult`, resolution and its refusals, `check_stack_operating_envelope` |
| `rust/src/airside.rs` | `layered_system_pressure_breakdown` (the layered breakdown); `system_pressure_breakdown` is its one-layer case |
| `rust/src/selection.rs` | `fill_stacks` variants, `fill_layers` on a candidate, the layered candidate physics |
| `rust/src/cli.rs` | the `layers` command, the stack spelling reader, `fillStacks`, `fillLayers`, the per-layer worked steps |
| `rust/src/validate.rs` | the schema's optional `fillStacks` field |
| `rust/tests/layers.rs` | the layered contract's own suite |
| `validation/test-vectors.json` (family `fillLayers`) | the recorded layered cases — additions alongside the existing families |

The layer cases in `validation/test-vectors.json` are recorded regression vectors, i.e. they
record behaviour; they are not accuracy reference data, and the file says so itself.
