# Component Catalog Schema

## 1. Principle

The selector is data-driven. Its equations only become useful for real design when the catalog contains controlled, test-backed component data with explicit validity domains.

The served catalog is `cockpit/assets/fixture.json` — the record the product UI and the engine read. `src/data/sampleCatalog.js` remains the records' source file (the revision the documentation drift gate reads); a demo catalog extracted from it served the retired JavaScript surface and went with it (issue #60, D22). Its global metadata status is:

```text
SYNTHETIC / NOT VENDOR DATA
```

Every product name, curve, limit, material description, and cost in that file is illustrative.

## 2. Top-level structure

```js
const catalog = {
  metadata: { /* catalog identity and status */ },
  waterQualityFactors: { /* optional scenario multipliers */ },
  towers: [],
  fills: [],
  driftEliminators: [],
  fans: [],
  nozzles: []
};
```

Recommended production metadata:

```js
metadata: {
  id: 'controlled-catalog-id',
  revision: 'YYYY-MM-DD-or-revision',
  status: 'CONTROLLED',
  owner: 'Engineering',
  approvedBy: 'name/role',
  approvedAt: 'ISO-8601 timestamp',
  currency: 'THB',
  unitSystem: 'SI',
  changeRecord: 'document reference'
}
```

## 3. Tower geometry record

Fields consumed by the current selector:

```js
{
  id: 'IDCF-064',
  name: 'Tower description',
  type: 'counterflow',          // or 'crossflow'
  draftType: 'induced',         // descriptive in current prototype
  fillAreaM2: 64,
  airFreeAreaM2: 64,
  driftAreaM2: 61,
  inletAreaM2: 88,
  fillDepthOptionsM: [1.2, 1.5, 1.8, 2.1],
  fillStacks: [                  // optional: complete stack variants, compared as identities
    'FILM-OF25@0.45+FILM-VF38@0.9',
    'FILM-OF25@1.35'
  ],
  maxWaterMassFlowKgS: 265,
  footprintM2: 78,
  inletLossCoefficient: 0.95,
  distributionLossCoefficient: 0.70,
  supportLossCoefficient: 0.50,
  plenumLossCoefficient: 0.50,
  fixedPressureLossPa: 20,
  compatibleFanIds: ['AX-500', 'AX-600'],
  baseCost: 166000
}
```

`fillStacks` is the ordered-layer contract of [FILL_LAYERS.md](FILL_LAYERS.md): each entry is one
complete stack, its layers spelled `<fillId>@<depthM>[@<thermalMultiplier>[@<pressureMultiplier>]]`
and joined with `+`, top first. A tower that declares variants is selected over those complete stacks
— a mixed stack of different fill types is one candidate identity — instead of over
`fillDepthOptionsM` × the fills (which stays the single-fill enumeration, one layer per fill × depth
option). The field is optional; the layers' fill ids must be records of the same catalog's fill list.

Natural-draft calculations additionally require:

```js
effectiveDraftHeightM: 170
```

Recommended production extensions:

- number of cells and operating-cell logic;
- fan cylinder, discharge, plenum, inlet, louver, and distribution geometry;
- pressure definition and reference planes;
- rain/spray zones and bypass areas;
- structural/fouled load limits;
- sound, vibration, access, fire, wind, seismic, and material constraints;
- drawing and revision references.

## 4. Fill record

Fields consumed by the current selector:

```js
{
  id: 'FILM-VF38',
  name: 'Fill description',
  geometry: 'vertical-fluted film',
  compatibleTowerTypes: ['counterflow', 'crossflow'],
  allowedWaterQualityClasses: ['clean', 'moderate', 'dirty'],

  thermal: {
    coefficientPerM: 0.94,
    referenceWaterLoadingKgM2S: 3.0,
    referenceDryAirLoadingKgM2S: 2.0,
    waterExponent: -0.27,
    airExponent: 0.39
  },

  pressure: {
    coefficientPaPerM: 43,
    referenceWaterLoadingKgM2S: 3.0,
    referenceDryAirLoadingKgM2S: 2.0,
    waterExponent: 0.10,
    airExponent: 1.67
  },

  limits: {
    minWaterLoadingKgM2S: 1.1,
    maxWaterLoadingKgM2S: 6.2,
    minDryAirLoadingKgM2S: 0.8,
    maxDryAirLoadingKgM2S: 3.5,
    maxWaterTemperatureC: 75
  },

  material: 'PP',
  costPerM3: 505
}
```

Current equations:

```text
Mfill = H Cthermal (L′/L′ref)^a (G′/G′ref)^b Fthermal
ΔPfill = H Cpressure (L′/L′ref)^p (G′/G′ref)^q Fpressure
```

Production fill data should additionally include:

- manufacturer, model, drawing, flute/path geometry, orientation, and pack dimensions;
- exact test configuration, laboratory, date, standard, and uncertainty;
- water/air loading grid and depth-specific results;
- wet and dry pressure data where relevant;
- interpolation method and an explicit no-extrapolation policy;
- water quality, solids, oil, scaling, biological, and cleaning limits;
- resin, UV, chemical, temperature, smoke, and fire properties;
- dry/wet/fouled weight and support spacing;
- source license and redistribution restrictions.

## 5. Drift-eliminator record

Fields consumed by the current selector:

```js
{
  id: 'DE-3P-10',
  name: 'Drift eliminator description',
  material: 'PVC',
  maxWaterTemperatureC: 60,
  curve: [
    { faceVelocityMS: 1.0, driftPpm: 4, pressureDropPa: 10 },
    { faceVelocityMS: 1.5, driftPpm: 6, pressureDropPa: 18 }
  ],
  costPerM2: 78
}
```

The selector linearly interpolates drift and pressure drop and rejects face velocities outside the curve range.

Production extensions:

- water-loading dependence;
- tested orientation and number of direction changes;
- droplet-size distribution where available;
- pack depth, free area, support span, material, UV/chemical/fire limits;
- perimeter seals, joints, bypass allowance, and installation tolerances;
- ATC-140 or other controlled test evidence and uncertainty.

## 6. Fan record

Fields consumed by the current selector:

```js
{
  id: 'AX-500',
  name: 'Fan description',
  referenceDensityKgM3: 1.2,
  allowedSpeedRatio: [0.70, 1.13],
  nominalRpm: 233,
  driveEfficiency: 0.96,
  motorEfficiency: 0.95,
  cost: 32200,
  curve: [
    { flowM3S: 55, pressurePa: 520, efficiency: 0.63 },
    { flowM3S: 100, pressurePa: 470, efficiency: 0.74 }
  ]
}
```

`nominalRpm` (issue #59) is the speed the recorded curve is published at — speed ratio 1.0 — in
rpm; the rpm behind a run's ratio is `speedRatio × nominalRpm`. It is **optional**: a record that
states no rated speed still selects, and a read-out shows no rpm rather than an invented one. The
two catalog copies that exist in this repository differ in when they gained the field: the
cockpit's own catalog (`cockpit/assets/fixture.json`, the record the engine and the product UI
read) carries it for its four fans, while the JavaScript-era record source
(`src/data/sampleCatalog.js`, the revision the documentation drift gate reads) still states no
rated speed — the copy the retired JavaScript surface served from it is gone with that surface (issue #60, D22),
and the served record is the cockpit's own.

Production extensions:

- manufacturer/model/revision, diameter, blade count, pitch, hub ratio, and speed;
- static versus total pressure definition and test arrangement;
- shaft power or torque curve, not only calculated air power;
- stable operating range, stall boundary, maximum pitch/speed/tip speed;
- sound power, vibration, balance, materials, corrosion/erosion limits;
- gearbox/belt/direct-drive data, motor enclosure and hazardous-area rating;
- altitude, temperature, and VFD limits;
- test standard, laboratory, uncertainty, and system-effect corrections.

## 7. Nozzle record

Fields consumed by the current selector:

```js
{
  id: 'NZ-20',
  name: 'Nozzle description',
  dischargeCoefficient: 0.72,
  orificeDiameterM: 0.020,
  referenceWaterDensityKgM3: 997
}
```

Production extensions:

- flow-pressure test curve rather than a single discharge coefficient;
- minimum/maximum pressure and turndown;
- spray cone, radius, distribution uniformity, and height above fill;
- minimum passage size, clogging tolerance, material, temperature, chemistry;
- header and basin compatibility, mounting, maintenance, and drawing reference.

## 8. Water-quality scenario factors

The sample catalog contains qualitative scenario multipliers such as:

```js
waterQualityFactors: {
  moderate: {
    thermalMultiplier: 0.94,
    pressureMultiplier: 1.10,
    riskPenalty: 12000
  }
}
```

These are placeholders, not chemistry models. A production implementation should replace them with explicit, approved degradation scenarios tied to actual water analysis, treatment, cleaning interval, and fouling history.

## 9. Prefer tables over fitted power laws when available

The current prototype uses power laws for compact demonstration. A production catalog should preserve original measured points and interpolate within a controlled multidimensional grid:

```js
thermalSurface: [
  { waterLoadingKgM2S: 2.0, dryAirLoadingKgM2S: 1.5, depthM: 1.2, merkelNumber: 1.10 },
  // ...
],
pressureSurface: [
  { waterLoadingKgM2S: 2.0, dryAirLoadingKgM2S: 1.5, depthM: 1.2, pressureDropPa: 72 },
  // ...
]
```

Store the interpolation method, domain, and uncertainty with the dataset. Do not extrapolate silently.

## 10. Data governance checklist

Before a record can be used for real selection, verify:

- identity and revision are unambiguous;
- units and pressure definitions are explicit;
- source rights permit the intended use;
- test setup and uncertainty are documented;
- validity limits are machine-readable;
- interpolation is validated and extrapolation is blocked;
- material, temperature, chemistry, fire, and structural constraints are present;
- installation/sealing/bypass requirements are represented;
- engineering approval and change history are recorded;
- regression cases protect every approved dataset revision.
