# JavaScript reference API (retired)

> **Retired with the reference.** Issue #40 slice 4 deleted the JavaScript engine this document
> describes (`src/core/**`, `src/index.js`, `src/app.js`, `index.html`); the last commit that contains
> it is recorded in the private decision record (D17). The engine of
> this repository is the Rust crate in `rust/` — see `rust/README.md` for the module-by-module port
> map, [`docs/WASM_ENGINE.md`](WASM_ENGINE.md) for the published wasm piece, and
> `scripts/parity/run.mjs` for the drift check against the recorded regression baseline. Everything
> below is the record of the reference **as it was**, kept for the port's traceability; it is not an
> API of this repository any more.

## Import

All public exports are re-exported from:

```js
import * as coolingTower from './src/index.js';
```

Individual modules may also be imported directly.

## Errors

Engineering domain failures throw `DomainError` from `src/core/numeric.js`.

```js
try {
  // calculation
} catch (error) {
  if (error.name === 'DomainError') {
    console.error(error.message, error.details);
  }
}
```

A domain error may indicate infeasible temperatures, an enthalpy pinch, no bracketed root, no fan/system intersection, an incomplete curve grid, or operation outside a catalog range.

## Psychrometrics

### `psychrometricState(input)`

Input:

```js
{
  dryBulbC: 33,
  wetBulbC: 27,          // or relativeHumidity: 0.63
  pressurePa: 101325
}
```

Important outputs:

```js
{
  humidityRatio,
  enthalpyKJkgDryAir,
  dryAirDensityKgM3,
  moistAirDensityKgM3,
  relativeHumidity,
  wetBulbC,
  dewPointC
}
```

Other exports include saturation pressure/humidity, relative humidity conversion, dew point, wet bulb, moist-air enthalpy, specific volume, and saturated-temperature recovery.

Module: `src/core/psychrometrics.js`.

## Water and duty

Exports:

- `waterDensityKgM3()`
- `waterSpecificHeatKJkgK()`
- `coolingRangeC()`
- `coolingApproachC()`
- `coolingEffectiveness()`
- `heatRejectionKW()`
- mass/volume flow conversions

Module: `src/core/water.js`.

## Merkel rating

### `merkelDemand(input)`

```js
const result = merkelDemand({
  hotWaterC: 42,
  coldWaterC: 32,
  wetBulbC: 27,
  dryBulbC: 33,
  pressurePa: 101325,
  waterToDryAirRatio: 1.5,
  salinityGKg: 0,
  integration: 'simpson', // or 'chebyshev4'
  segments: 240
});
```

Returns `merkelNumber`, inlet-air state, water heat capacity, and integration method.

### `solveColdWaterTemperature(input)`

```js
const result = solveColdWaterTemperature({
  hotWaterC: 42,
  wetBulbC: 27,
  dryBulbC: 33,
  pressurePa: 101325,
  waterToDryAirRatio: 1.5,
  availableMerkelNumber: 1.50
});
```

Returns predicted CWT, range, approach, required Merkel number, inlet-air state, and water heat capacity.

### Other exports

- `towerCharacteristic()`
- `characteristicCoefficient()`
- `estimateOutletAirState()`
- `generateDemandCurve()`

Module: `src/core/merkel.js`.

## Characteristic capability

### `evaluateCharacteristicCapability(input)`

```js
const condition = {
  waterMassFlowKgS: 200,
  dryAirMassFlowKgS: 133.3333333,
  hotWaterC: 42,
  coldWaterC: 32,
  wetBulbC: 27,
  dryBulbC: 33,
  pressurePa: 101325
};

const result = evaluateCharacteristicCapability({
  design: condition,
  test: condition,
  characteristicExponent: -0.6,
  integration: 'chebyshev4'
});
```

Important outputs:

- `capabilityPct`
- design/test/capability `L/G`
- test Merkel number and characteristic coefficient
- demand/supply curve records
- explicit method disclaimer

### `monteCarloCharacteristicCapability(input)`

```js
const uncertainty = monteCarloCharacteristicCapability({
  baseInput: {
    design: condition,
    test: condition,
    characteristicExponent: -0.6
  },
  standardUncertainty: {
    test: {
      waterMassFlowKgS: 1,
      dryAirMassFlowKgS: 1,
      hotWaterC: 0.03,
      coldWaterC: 0.03,
      wetBulbC: 0.05
    }
  },
  samples: 1000,
  seed: 20260813
});
```

Module: `src/core/capability.js`.

## Performance curves

### Record format

```js
{
  wetBulbC: 27,
  rangeC: 10,
  waterFlowKgS: 200,
  coldWaterC: 32.8
}
```

### Exports

- `performanceCurveBounds(records)`
- `predictColdWaterFromPerformanceCurves()`
- `predictWaterFlowFromPerformanceCurves()`
- `evaluatePerformanceCurveCapability()`

Example:

```js
const result = evaluatePerformanceCurveCapability({
  records: samplePerformanceCurveRecords,
  testWetBulbC: 27,
  testRangeC: 10,
  testColdWaterC: 32.8,
  adjustedTestWaterFlowKgS: 200
});
```

Module: `src/core/performanceCurve.js`.

## Fill, drift, and system resistance

Exports:

- `fillThermalMerkelNumber()`
- `fillPressureDropPa()`
- `driftPerformanceAtVelocity()`
- `systemPressureBreakdown()`
- `checkFillOperatingEnvelope()`
- `velocityPressurePa()`
- `minorLossPressurePa()`

Module: `src/core/airside.js`.

## Fan and motor

Exports:

- `fanOperatingLimits()`
- `fanPressurePaAtFlow()`
- `fanEfficiencyAtFlow()`
- `fanShaftPowerKW()`
- `solveFanSystemIntersection()`
- `estimateAirflowFromFanPower()`
- `chooseStandardMotor()`

Module: `src/core/fan.js`.

## Nozzle and water balance

Nozzle exports:

- `nozzleFlowM3S()`
- `selectNozzleArrangement()`

Water-balance exports:

- `evaporationFromAirMassBalance()`
- `driftLossKgS()`
- `coolingTowerWaterBalance()`

Modules: `src/core/nozzle.js` and `src/core/waterBalance.js`.

## Crossflow

### `solveCrossflowGrid(input)`

Required inputs include hot/dry/wet-bulb temperatures, pressure, water flow, dry-air flow, available Merkel number, and grid size.

Returns CWT, range, approach, heat transfer, energy-balance error, outlet-air state, and water/air state grids.

Module: `src/core/crossflow.js`.

## Natural draft

### `solveNaturalDraftCounterflow(input)`

Requires a tower object containing `effectiveDraftHeightM` plus the same fill, drift, atmospheric, thermal, and water inputs used by the mechanical model.

Returns the coupled volume flow, airside breakdown, `L/G`, thermal result, plume density, draft pressure, residual, and evaporation estimate.

Module: `src/core/naturalDraft.js`.

## Component selection

### `defaultSelectionRequirements()`

Returns the bundled default design/constraint/economic inputs.

### `selectCoolingTowerComponents(input)`

```js
const selection = selectCoolingTowerComponents({
  requirements: {
    ...defaultSelectionRequirements(),
    waterMassFlowKgS: 200,
    hotWaterC: 42,
    targetColdWaterC: 32,
    wetBulbC: 27,
    dryBulbC: 33,
    maxDriftPpm: 30,
    maxElectricalInputKW: 75
  },
  catalog: sampleCatalog,
  maxResults: 10
});
```

Important outputs:

```js
{
  requirements,
  catalogMetadata,
  inletAirState,
  results,                 // ranked feasible candidates
  feasibleCandidateCount,
  rejectionSummary,
  warning
}
```

Each result contains the selected tower, fill/depth, drift eliminator, fan/speed, fan operating point, pressure breakdown, thermal result, margin, motor, electrical input, water balance, nozzle, economics, and provenance status.

Module: `src/core/selection.js`.

## Sample data

- `sampleCatalog` from `src/data/sampleCatalog.js`
- `samplePerformanceCurveRecords` and metadata from `src/data/samplePerformanceCurves.js`

These records are synthetic and must not be used for procurement or guarantees.


## Worked calculations (`src/core/worked.js`)

Each function returns `{ title, purpose, steps, result }`, where every entry in `steps` is:

| Field | Meaning |
| --- | --- |
| `kind` | `'calc'` or `'note'` |
| `label` | short name of the step |
| `why` | plain-language explanation of why the step exists |
| `formula` | symbolic form (`null` for notes) |
| `substitution` | the same formula with this run's numbers |
| `value` | numeric result, or `null` where the step has no single value |
| `unit` | unit of `value` |
| `reference` | source citation where one applies |

`result` is the object the corresponding solver would have returned, so a worked sheet can be used in place of a direct solver call.

| Function | Extra fields |
| --- | --- |
| `workedPsychrometrics({ dryBulbC, wetBulbC, pressurePa })` | — |
| `workedMerkelDemand({ hotWaterC, coldWaterC, wetBulbC, dryBulbC, waterToDryAirRatio, ... })` | `integrationTable`, `integrationSummary`, `crossCheck` |
| `workedColdWaterPrediction({ hotWaterC, wetBulbC, dryBulbC, waterToDryAirRatio, availableMerkelNumber, waterMassFlowKgS })` | `searchTable` |
| `workedAirsidePressure({ tower, fill, fillDepthM, driftEliminator, fan, volumetricAirFlowM3S, inletAirState, waterMassFlowKgS })` | `components`, `transferZones` |
| `workedFanBalance({ fan, flowM3S, speedRatio, airDensityKgM3, systemPressurePa })` | — |
| `workedOutletAir({ inletAirState, waterToDryAirRatio, hotWaterC, coldWaterC, cpWaterKJkgK })` | — |
| `workedWaterBalance({ inletAirState, outletAirState, dryAirMassFlowKgS, circulatingWaterMassFlowKgS, driftPpm, cyclesOfConcentration })` | — |
| `workedCapability({ design, test, characteristicExponent })` | — |
| `workedNozzleSelection({ nozzle, pressureDropPa, totalVolumetricWaterFlowM3S })` | — |

## Options added to existing functions

| Function | Option | Default | Effect |
| --- | --- | --- | --- |
| `psychrometricState` | `enhancementFactor` | `true` | apply the water-vapour enhancement factor; `false` reproduces the ASHRAE simplified equation set |
| `saturationHumidityRatio`, `saturatedAirEnthalpyKJkgDryAir`, and related | third `options` argument | `{ enhancementFactor: true }` | as above |
| `merkelDemand`, `solveColdWaterTemperature`, `generateDemandCurve`, `evaluateCharacteristicCapability` | `inletEnthalpyConvention` | `'bulk'` | `'cti-saturated-wetbulb'` matches published CTI demand curves |
| `solveCrossflowGrid` | `richardson` | `true` | Richardson-extrapolate from a doubled grid |
| `solveCrossflowGrid` | `cpIterations` | `3` | iterations on mean-water-temperature specific heat |
| `systemPressureBreakdown` | `fan` | `null` | supplies the fan throat area, pressure basis, and stack recovery factor |

## New functions

| Function | Purpose |
| --- | --- |
| `waterVaporEnhancementFactor(pressurePa, options)` | Buck (1981) enhancement factor |
| `effectiveSaturationPressurePa(temperatureC, pressurePa, options)` | `f · pws(t)` |
| `inletAirEnthalpyKJkgDryAir({ inletAirState, pressurePa, convention })` | resolves the entering-air enthalpy convention |
| `resolveAirsideAreas(tower, fan)` | every flow area the airside model uses, with fallbacks |
| `fanStackDischargePressurePa({ ... })` | discharge velocity pressure, honouring pressure basis and stack recovery |
| `zoneMerkelNumber({ zone, heightM, ... })` | transfer contribution of fill, spray zone, or rain zone |
| `crossflowConvergenceStudy({ baseCells, ... })` | three-grid study: observed order and Grid Convergence Index |
