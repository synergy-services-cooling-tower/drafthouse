# Engineering Methods and Equations

> **Retired code references.** The `Code:` lines below cite the JavaScript reference implementation
> (`src/core/**`), retired in issue #40 slice 4; the ported implementation of every method is the Rust
> crate — the module-by-module map is in `rust/README.md`, and the crate's suites (`rust/tests/*.rs`)
> carry the checks that were the JavaScript tests. The citations are kept as the port's provenance;
> the last commit containing the reference is recorded in the private decision record (D17).

## 1. Scope and result identity

The repository deliberately keeps four result types separate:

1. **Physics-based rating:** predicts cold-water temperature from atmospheric conditions, water/air flow, and an available tower or fill characteristic.
2. **Characteristic-curve capability:** fits a whole-tower characteristic through a test point and intersects it with design thermal demand.
3. **Performance-curve capability:** interpolates and inverts a supplied reference performance grid.
4. **Component selection:** solves tower, fill, drift-eliminator, fan, speed, motor, and nozzle combinations against constraints.

A physics-model prediction is not a contractual CTI acceptance result. A model-generated curve is not a certified manufacturer curve.

## 2. Units and conventions

The engine uses SI units:

- temperature: °C;
- absolute pressure: Pa;
- water and dry-air mass flow: kg/s;
- air/water volume flow: m³/s;
- pressure difference: Pa;
- heat rate and power: kW;
- area: m²;
- fill depth and diameter: m;
- humidity ratio: kg water/kg dry air;
- moist-air enthalpy: kJ/kg dry air.

`L/G` always means circulating-water mass flow divided by **dry-air** mass flow.

## 3. Basic cooling-tower quantities

Range:

```text
R = Th − Tc
```

Approach:

```text
A = Tc − Twb
```

Effectiveness:

```text
ε = (Th − Tc) / (Th − Twb)
```

Water-side heat rejection:

```text
Q̇ = ṁw cp,w (Th − Tc)
```

Code: `src/core/water.js`.

The included water density and heat-capacity functions are engineering approximations. Production freshwater/seawater calculations should use a validated property package and explicit uncertainty.

## 4. Psychrometrics

Humidity ratio from vapor partial pressure:

```text
W = 0.621945 pv / (P − pv)
```

Moist-air enthalpy:

```text
h = 1.006 Tdb + W(2501 + 1.86 Tdb)
```

Dry-air specific volume per kg dry air:

```text
vda = Rda TK (1 + 1.607858 W) / P
```

The saturation-pressure function implements the common ASHRAE polynomial/logarithmic forms for ice and liquid water, with a software domain of −100 to 200 °C. Wet-bulb input is converted to humidity ratio using the usual above- and below-freezing psychrometric relations. Dew point and wet bulb are solved with bracketed roots.

### Water-vapour enhancement factor

Water vapour in air is not an ideal gas, and dissolved air raises the equilibrium vapour pressure above the pure-substance value. The true saturation partial pressure is `f · pws(t)` with `f` slightly above 1:

```text
Ws = 0.621945 f pws / (P − f pws)
f  = 1.0007 + 3.46e-6 P_hPa      (Buck, 1981)
```

ASHRAE's published tables include this effect; the ASHRAE *simplified* equation set omits it and is consequently 0.3–0.6 % low in humidity ratio near ambient conditions. This prototype applies the enhancement factor **by default**. Measured against ASHRAE Table 2 saturated-air values at 101 325 Pa over 0–50 °C:

| Treatment | Worst humidity-ratio error | Worst saturated-enthalpy error |
| --- | --- | --- |
| Enhancement factor applied (default) | 0.14 % | 0.36 kJ/kg |
| Simplified equation set | 0.62 % | 1.42 kJ/kg |

Pass `{ enhancementFactor: false }` (or `enhancementFactor: false` to `psychrometricState`) to recover the simplified behaviour when cross-checking against tools that make the same simplification, such as PsychroLib.

Boundary: the Buck form is a pressure-only approximation of the full Hyland–Wexler/ASHRAE RP-1485 formulation, which depends on temperature as well. Use the full formulation for contractual or laboratory-grade work, particularly far from 1 atm or above roughly 60 °C.

Code: `src/core/psychrometrics.js`; `waterVaporEnhancementFactor()`, `effectiveSaturationPressurePa()`.

Boundary: numerical support over a range does not itself establish handbook-grade accuracy over that entire range. Validate every intended operating envelope against approved ASHRAE or laboratory reference cases.

## 5. Counterflow Merkel demand

The classical Merkel demand is:

```text
KaV/L = ∫[Tc→Th] cp,w dTw / (hs(Tw) − ha(Tw))
```

The bulk-air operating line is approximated as:

```text
ha(Tw) = hin + (L/G) cp,w (Tw − Tc)
```

where `hs(Tw)` is the enthalpy of saturated air at the local water temperature.

### Entering-air enthalpy convention

`hin` is ambiguous in the literature and the two conventions do not agree:

| `inletEnthalpyConvention` | `hin` | Use |
| --- | --- | --- |
| `'bulk'` (default) | true moist-air enthalpy from the measured dry bulb and wet bulb | physically consistent modelling; what the operating-line derivation calls for |
| `'cti-saturated-wetbulb'` | enthalpy of *saturated* air at the entering wet bulb | cross-checking against published CTI demand curves and worksheets, which assume air enthalpy is a function of wet bulb alone |

At the reference design point (42/32 °C water, 33 °C DB / 27 °C WB, L/G = 1.5) the CTI convention yields a `KaV/L` about 1.1 % higher, because taking the entering air as saturated raises `hin` and shrinks the driving potential. Anyone comparing this tool against a CTI Blue Book demand curve must select `'cti-saturated-wetbulb'` or expect that offset.

The implementation offers:

- composite Simpson integration with a configurable even segment count;
- four-point equal-weight Tchebycheff integration for worksheet compatibility.

When `hs − ha <= 0`, the function raises a thermal-pinch domain error rather than integrating through an infeasible state.

Code: `merkelDemand()` in `src/core/merkel.js`; numerical methods in `src/core/numeric.js`.

## 6. Cold-water prediction

For known available characteristic, the solver finds:

```text
F(Tc) = Mrequired(Tc) − Mavailable = 0
```

The search interval is bounded between a small distance above entering wet bulb and a small distance below hot-water temperature. The code scans for a valid sign change and then uses a bracketed bisection-style root solve, which is safer than an unconstrained Newton step near a thermal pinch.

Code: `solveColdWaterTemperature()` in `src/core/merkel.js`.

The result includes predicted CWT, range, approach, required Merkel number, inlet-air state, and water heat capacity.

## 7. Whole-tower characteristic

The prototype represents a whole-tower characteristic as:

```text
Mavailable = KaV/L = C(L/G)^m
```

The exponent is stored with its sign. A source written as `C(L/G)^−n` should be imported as `m = −n`.

A characteristic through a known point is fitted by:

```text
C = Mpoint / (L/Gpoint)^m
```

Code: `towerCharacteristic()` and `characteristicCoefficient()` in `src/core/merkel.js`.

## 8. Characteristic-curve capability mechanics

The implemented sequence is:

1. calculate the test Merkel demand at measured/test `L/G`;
2. fit `Ctest = Mtest/(L/Gtest)^m`;
3. construct the test-point tower characteristic;
4. calculate design-condition demand over a range of `L/G`;
5. solve the characteristic/demand intersection at `(L/G)cap`;
6. report:

```text
Capability % = 100 (L/Gcap) / (L/Gdesign)
```

Code: `evaluateCharacteristicCapability()` in `src/core/capability.js`.

The module exposes the demand and supply curves for plotting and reports an explicit disclaimer. It does not encode the complete current ATC-105 procedure, including all test-validity limits, measurement rules, fan-power/airflow corrections, recirculation/wind requirements, contractual reference handling, or prescribed uncertainty treatment.

## 9. Performance-curve method

The input is a flat rectangular record set:

```js
{
  wetBulbC: 27,
  rangeC: 10,
  waterFlowKgS: 200,
  coldWaterC: 32.8
}
```

For a requested wet bulb, range, and water flow, the engine:

1. brackets wet bulb and range;
2. linearly interpolates CWT versus water flow at each of four wet-bulb/range corners;
3. bilinearly interpolates across wet bulb and range.

Inverse flow is found by solving:

```text
CWTgrid(WB, R, qpredicted) − CWTtest = 0
```

Capability is then:

```text
Capability % = 100 qtest,adjusted / qpredicted
```

Leaving-water deviation uses:

```text
ΔTc = Tctest − Tcpredicted-at-adjusted-flow
```

Positive deviation is warmer/worse under this convention.

Code: `src/core/performanceCurve.js`. Synthetic records: `src/data/samplePerformanceCurves.js`.

The caller must apply all applicable test-code corrections before passing `adjustedTestWaterFlowKgS`.

## 10. Fill thermal and wet-pressure models

The sample catalog uses independent power-law models.

Thermal supply:

```text
Mfill = H Mref
        (L′/L′ref)^a
        (G′/G′ref)^b
        Fthermal
```

Wet pressure drop:

```text
ΔPfill = H ΔPref
         (L′/L′ref)^p
         (G′/G′ref)^q
         Fpressure
```

where `H` is fill depth, `L′` is water loading by fill plan area, and `G′` is dry-air loading by free area.

The two models are per **layer**: a tower's fill stack is an ordered list of layers, each with its own
fill record, depth and thermal/pressure multipliers. The layers sit in series in both the air path
and the water path, so every layer is evaluated at the stack's own `L′`, `G′` and airflow, and the
stack's fill transfer number and pressure drop are the layers' terms summed in physical order (top
first). A one-layer stack is the single-fill case of the same two models — the ported functions, the
same operand order — so the single-fill numbers are the one-layer result rather than a separate
correlation. The contract, its refusals and its data surface are in
[FILL_LAYERS.md](FILL_LAYERS.md).

Thermal and pressure multipliers are intentionally separate because fouling may reduce transfer while increasing resistance. A layer carries its own pair on top of the run's (the water-quality factors):
the effective multiplier of a layer is the product of the two.

Code: `fillThermalMerkelNumber()` and `fillPressureDropPa()` in `rust/src/airside.rs` (ported from the
retired `src/core/airside.js`), and the layered stack in `rust/src/layers.rs`.

No universal coefficients exist. Real data must come from controlled tested curves or correlations and remain inside their validated domains.

## 11. Airside system resistance

Minor-loss components use:

```text
ΔPi = Ki ρ Vi² / 2
```

### Reference velocities are part of the loss coefficient

A loss coefficient `K` is meaningless without the velocity it was referenced to. Every minor loss in this model is therefore bound to a named flow area, and the model computes that section's own velocity:

| Loss | Reference area | Why |
| --- | --- | --- |
| Air inlet and rain zone | `tower.inletAreaM2` (louvre face) | genuinely smaller than the plan area, so the air is fastest here |
| Water distribution | `airFreeAreaM2` | spray headers spread across the full section |
| Fill supports | `airFreeAreaM2` | structure spread across the full section |
| Plenum and fan inlet | `tower.plenumAreaM2`, defaulting to the fan throat | the air accelerates into the fan |
| Fan-stack discharge | fan throat (`fan.stackAreaM2`) | discharge velocity pressure |

Referencing all coefficients to the fill-face velocity — the slowest section in the tower — collapses every minor loss to a few pascals and materially understates fan power. An earlier revision of this prototype did exactly that.

### Fan-stack discharge loss

Air leaving the stack carries kinetic energy `½ρv²` per unit volume out of the control volume. That energy is supplied by the fan and not recovered by the tower. Whether it is charged to the *system* depends on how the fan curve is published:

```text
FTP = FSP + outlet velocity pressure
```

- `fan.pressureBasis: 'total'` (default) — the curve is fan total pressure, so the discharge term must be added to the system curve.
- `fan.pressureBasis: 'static'` — the discharge term is already netted out of the published curve and must **not** be added again.

A velocity-recovery stack decelerates the air before discharge; `stackRecoveryFactor` is the fraction recovered (0 = plain cylindrical stack, 0.3–0.5 typical for a recovery stack):

```text
ΔPstack = (1 − recovery) ½ ρ v_stack²
```

With no fan supplied (natural draft) there is no fan stack and the term is zero; shell exit losses belong to the draft balance instead.

The prototype sums:

```text
ΔPsystem = ΔPfill
         + ΔPdrift
         + ΔPinlet
         + ΔPdistribution
         + ΔPsupport
         + ΔPplenum
         + ΔPfanstack
         + ΔPfixed
```

Dry-air mass flow comes from volume flow multiplied by dry-air density per mixture volume.

### Heat-transfer zones

Available `KaV/L` is the sum of every zone the air passes through, not the fill alone:

```text
Me_available = Me_fill + Me_spray + Me_rain
```

All three use the same correlation form `c (L″/L″ref)^a (G″/G″ref)^b · height`, with per-record coefficients. In the bundled synthetic catalog the spray and rain zones contribute roughly 17 % of the total, consistent with the 10–20 % commonly reported for counterflow towers. Omitting them is conservative on temperature, while omitting the pressure terms above is optimistic on fan power — the two errors act on different outputs and do not cancel.

Code: `systemPressureBreakdown()`, `fanStackDischargePressurePa()`, `resolveAirsideAreas()`, `zoneMerkelNumber()` in `src/core/airside.js`.

Boundary: the pressure model still evaluates every section at the *entering* air density. A production model should carry density through the tower, and should derive `K` values and reference planes from the same test data and plane definitions as the fan curve.

## 12. Fan curve, affinity scaling, power, and motor

Around a supplied reference curve:

```text
Q2/Q1 = N2/N1
ΔP2/ΔP1 = (ρ2/ρ1)(N2/N1)²
```

The reference fan curve is evaluated at `Q2/(N2/N1)`, then pressure is density- and speed-scaled.

Shaft power along the same affinity family (the flow and pressure laws at one point, at constant
density) — the recorded curve carries no power of its own, so this leg is computed from it:

```text
P2/P1 = (N2/N1)³
```

The speed at which a fan's recorded curve is published is the record's own `nominalRpm` (speed
ratio 1.0), so the rpm behind a run's ratio is `speedRatio × nominalRpm`; the record's
`allowedSpeedRatio` window is its validity band, and a ratio outside it is reported by name rather
than clamped into the band.

The operating point solves:

```text
ΔPfan(Q) − ΔPsystem(Q) = 0
```

Fan shaft power:

```text
Pshaft = Q ΔP / ηfan
```

Electrical input:

```text
Pelectrical = Pshaft / (ηdrive ηmotor)
```

Standard motor sizing applies drive efficiency and a service factor before selecting the next catalog size.

Code: `src/core/fan.js`.

Final engineering requires tested manufacturer curves and checks for stall, pitch, tip speed, system effects, noise, vibration, motor overload, ambient/altitude derating, and drive limits.

## 13. Drift eliminator

The sample drift eliminators use piecewise records of:

```js
{ faceVelocityMS, driftPpm, pressureDropPa }
```

Both drift and pressure drop are interpolated versus face velocity. The selector rejects operation outside the curve velocity range or above the specified drift limit.

Drift mass loss:

```text
ṁD = (drift ppm × 10⁻⁶) ṁcirculation
```

Code: `driftPerformanceAtVelocity()` in `src/core/airside.js` and `driftLossKgS()` in `src/core/waterBalance.js`.

Real selection also requires tested installed-system evidence, correct orientation, perimeter sealing, and bypass control.

## 14. Pressurized nozzles

Nozzle flow is:

```text
q = Cd Ao √(2ΔP/ρw)
```

For each catalog nozzle, the current selector calculates flow at the specified pressure drop, rounds the nozzle count upward, and ranks valid counts by minimum excess capacity and then count.

Code: `src/core/nozzle.js`.

The prototype does not yet solve header balance, spray radius/overlap, local loading uniformity, gravity distribution, clogging, elevation, or turndown.

## 15. Outlet air and water balance

The simplified outlet-air enthalpy is:

```text
hout = hin + (L/G) cp,w (Th − Tc)
```

The outlet is treated as saturated at `hout`; the corresponding saturated temperature is found by root solving.

Evaporation from dry-air mass balance:

```text
ṁE = ṁda (Wout − Win)
```

Steady-state water balance:

```text
M = E + B + D
```

For cycles of concentration `COC`:

```text
B = E/(COC − 1) − D
```

Blowdown is clamped at zero if the simplified equation becomes negative.

Code: `estimateOutletAirState()` in `src/core/merkel.js` and `src/core/waterBalance.js`.

## 16. Coupled component selection

For every compatible combination, `selectCoolingTowerComponents()`:

1. filters tower flow and footprint;
2. checks tower/fill/fan compatibility;
3. enumerates fill depth, drift eliminator, fan, and speed ratio;
4. solves fan/system airflow;
5. calculates `L′`, `G′`, fill supply, pressure breakdown, and drift;
6. checks fill, drift, speed, temperature, and water-quality domains;
7. predicts CWT using a counterflow or crossflow thermal solver;
8. checks thermal margin, drift, motor, and electrical-input limits;
9. selects a nozzle arrangement;
10. estimates evaporation, blowdown, makeup, energy, water, CAPEX, maintenance, and risk;
11. ranks feasible candidates by lifecycle cost, then thermal margin and power.

Lifecycle objective:

```text
J = CAPEX + NPV(energy + water + maintenance) + risk penalty
```

Code: `src/core/selection.js`; synthetic catalog: `src/data/sampleCatalog.js`.

## 17. Crossflow model

`solveCrossflowGrid()` uses a two-dimensional finite-volume topology:

- water advances downward by row;
- air advances horizontally by column;
- total `KaV` is distributed uniformly across cells;
- local transfer uses an enthalpy-potential effectiveness relation;
- outlet water is averaged across columns;
- outlet air is averaged across rows and treated as saturated for state recovery.

Water specific heat is evaluated at the mean bulk water temperature, which is not known until the outlet is known, so it is iterated rather than guessed from a fixed offset.

### Grid convergence

Each cell holds the water temperature constant across itself, so the scheme is first order: the error falls as `C/n`. A raw 18 × 18 grid is roughly 0.18 °C optimistic on cold-water temperature — a material bias when the selector screens on thermal margins of a few tenths of a degree.

The solver therefore runs the grid at `n` and `2n` and applies Richardson extrapolation:

```text
f_exact ≈ f_2n + (f_2n − f_n) / (2^p − 1),   p = 1
```

At the default 18 × 18 this reduces the error against a 384 × 384 reference from about 0.18 °C to under 0.01 °C. `gridConvergence.estimatedDiscretizationErrorC` is reported alongside; note it bounds the *raw fine grid*, not the extrapolated value that is returned, so it is deliberately conservative.

`crossflowConvergenceStudy()` runs a three-grid study (`n`, `2n`, `4n`) that additionally recovers the *observed* order of convergence and a Roache Grid Convergence Index. On the reference case the observed order is 1.04, confirming the expected first-order behaviour. It is not called by the selector because it costs roughly seven times a single sweep.

Note that this model's water-side and air-side energy totals agree to machine precision **by construction**. An "energy balance closes" check is therefore an algebraic identity that passes even on a 2 × 2 grid that is 2 °C wrong, and is not evidence of convergence or correctness. Use the grid-convergence estimate instead.

Code: `src/core/crossflow.js`.

This establishes a crossflow calculation structure but is not a validated commercial crossflow rating model. It omits nonuniform distribution, detailed local correlations, wall effects, and explicit Lewis-factor treatment, and it applies counterflow-style fill correlations to a crossflow air path.

## 18. Natural-draft model

The simplified buoyancy pressure is:

```text
ΔPdraft = g Heff (ρambient − ρplume)
```

For each trial airflow, the solver calculates:

1. tower system resistance and fill supply;
2. counterflow cold-water temperature;
3. saturated outlet-air state and plume density;
4. buoyancy pressure;
5. residual `ΔPdraft − ΔPsystem`.

It then finds the airflow at which draft and system resistance balance.

Code: `src/core/naturalDraft.js`.

Production natural-draft work needs vertical/radial density integration, shell and rain-zone losses, wind effects, multiple fill zones/rings, plume behavior, and validation against licensed software or test data.

## 19. Measurement uncertainty

`monteCarloCharacteristicCapability()` deep-clones the base input, applies normally distributed perturbations to user-specified fields, and recalculates capability with a seeded pseudo-random generator.

Outputs include mean, standard deviation, 2.5th/50th/97.5th percentiles, rejected sample count, and an approximate `2σ` expanded uncertainty.

Code: `src/core/capability.js` and random/statistical helpers in `src/core/numeric.js`.

This is a generic propagation engine, not a prescribed ATC-105 uncertainty method.

## 20. Numerical behavior

The numerical library contains:

- one-dimensional interpolation with optional extrapolation;
- bracketing and bilinear interpolation;
- composite Simpson and four-point Tchebycheff quadrature;
- root scanning followed by a bracketed bisection-style solve;
- deterministic seeded random and Gaussian generation;
- percentile calculation.

Domain failures are meaningful. Typical causes are an enthalpy pinch, no physical CWT root, no fan/system intersection, or operation outside a curve domain. Production software should surface these failures rather than silently extrapolating.


## 21. Worked calculations (educational layer)

`src/core/worked.js` re-derives each headline result one step at a time. Every step carries:

- a plain-language statement of *why* the step exists;
- the symbolic formula;
- the same formula with this run's numbers substituted in;
- the resulting value and unit;
- a source reference where one applies.

Available sheets: `workedPsychrometrics`, `workedMerkelDemand` (including the four-point Tchebycheff worksheet table), `workedColdWaterPrediction` (including the residual search table), `workedAirsidePressure` (itemised pressure build-up and transfer zones), `workedFanBalance` (the full air-power to electrical-input chain), `workedOutletAir`, `workedWaterBalance`, `workedCapability`, and `workedNozzleSelection`.

These functions call the same core functions as the solvers rather than re-implementing the physics, so a worked sheet cannot drift away from what the tool actually computed (`tests/worked.test.js` asserted that numerically while the reference existed — the crate's suites and the recorded baseline rows carry the same quantities now). The rendered-HTML checks (balanced tags, leaked `undefined`/`NaN`, unevaluated template placeholders, escaping) belonged to the prototype's renderer and were deleted with it in issue #40 slice 4; the surviving surfaces' pixels are covered by the browser audits (`npm run deployment-audit` against a bundle-shaped tree, and the cockpit's own frame harness, `cockpit/tools/capture.mjs`).

No tracing overhead is added to the solvers themselves; the worked sheets are computed only when rendered.

Code: `src/core/worked.js`; rendering in `src/app.js`; styling under `.worked*` in `styles.css`.
