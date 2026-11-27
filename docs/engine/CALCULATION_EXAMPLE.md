# Worked Synthetic Example

## Purpose

This example demonstrates the calculation flow and software outputs. Every tower and component record is fictional; the result is not a product recommendation.

## Input duty

| Input | Value |
|---|---:|
| Circulating water | 200 kg/s |
| Hot-water temperature | 42 °C |
| Target cold-water temperature | 32 °C |
| Entering wet bulb | 27 °C |
| Entering dry bulb | 33 °C |
| Pressure | 101.325 kPa |
| Water quality | Moderate |
| Maximum drift | 30 ppm |
| Maximum electrical input | 75 kW |
| Maximum footprint | 130 m² |

## Standalone Merkel check

At `L/G = 1.5`, four-point Tchebycheff integration (`integration: 'chebyshev4'`) with the bulk entering-air enthalpy convention (`inletEnthalpyConvention: 'bulk'`, the engine default) produces:

```text
Required Merkel number = 1.492720078
```

Passing that same Merkel number back to the inverse solver, with the same quadrature and convention, returns:

```text
Predicted cold-water temperature = 31.9999999 °C
```

This is a numerical closure check, not an external physical validation.

## Characteristic identity check

Using the same condition as both design and test with characteristic exponent `m = -0.6` gives:

```text
Capability = 100.00000035 %
```

The small difference from exactly 100% is root-solver tolerance.

## Coupled catalog search

The sample search evaluates tower, fill depth, drift eliminator, fan and fan-speed combinations. It rejects candidates for reasons including:

- tower flow or footprint limit;
- incompatible fill/tower or water-quality class;
- fill air/water loading outside the synthetic curve domain;
- drift face velocity outside its curve;
- no fan/system intersection;
- insufficient thermal margin;
- excessive drift or electrical power;
- no suitable motor or nozzle arrangement.

With the bundled catalog, the deterministic search finds:

```text
Feasible combinations = 202
```

The first lifecycle-ranked synthetic result is:

| Item | Result |
|---|---|
| Tower | `IDCF-064` |
| Fill | `FILM-VF38` |
| Fill depth | 1.5 m |
| Drift eliminator | `DE-3P-10` |
| Fan | `AX-500` |
| Fan speed ratio | 0.78 |
| Predicted CWT | 31.654595 °C |
| Electrical input | 31.503260 kW |

The ranking is driven by fictional prices and factors. A higher-ranked synthetic result is not inherently a better real design.

## Reproduce from the command line

```bash
npm run smoke
```

For the full pressure, loading, water-balance, nozzle, motor and lifecycle breakdown, run the browser application and open **Tower & Parts Selection**.
