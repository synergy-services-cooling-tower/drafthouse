# CTI Toolkit and MRL Mapping

> **Retired code references.** The `src/core/**` paths in the mapping table are the JavaScript
> reference implementation, deleted in issue #40 slice 4; the ported implementation is the Rust crate
> (`rust/src/**`; module-by-module map in `rust/README.md`), and the citations are kept as the port's
> provenance. The last commit containing the reference is recorded in the private decision record (D17).

## Purpose

This document maps publicly described CTI Toolkit and MRL functions to the prototype. It does not claim feature equivalence and does not reverse engineer proprietary source code.

## CTI Toolkit public functions

The CTI Toolkit public page describes:

- ASHRAE-compliant psychrometrics;
- a Thermal Design Worksheet and demand-curve tab;
- a Performance Evaluator;
- induced- or forced-draft evaluation;
- crossflow or counterflow evaluation;
- percent performance or leaving-water deviation; and
- manual/file data entry with automatic crossplotting.

Prototype mapping:

| CTI Toolkit public function | Prototype implementation | Status |
|---|---|---|
| Air properties | `src/core/psychrometrics.js` | Working SI implementation; external qualification still required |
| Thermal demand worksheet | `src/core/merkel.js:merkelDemand()` | Working counterflow Merkel demand |
| Demand curve | `generateDemandCurve()` | Working |
| Characteristic evaluation | `src/core/capability.js:evaluateCharacteristicCapability()` | Public concept demonstrated; not a complete ATC-105 implementation |
| Performance curves/crossplot | `src/core/performanceCurve.js` | Working rectangular-grid interpolation and inverse-flow solve |
| Leaving-water deviation | `evaluatePerformanceCurveCapability()` | Working with explicit sign convention |
| Crossflow | `src/core/crossflow.js` | Simplified finite-volume prototype |
| Forced/induced draft | `src/core/fan.js`, `src/core/airside.js`, `src/core/selection.js` | Mechanical-draft coupled search implemented with synthetic data |
| Reports and controlled forms | Browser display, print styling and JSON export | Controlled contractual report workflow not implemented |

## ATC-105 boundary

The CTI Marketplace description states that ATC-105 contains:

- Part I: test procedure and instrumentation for mechanical- and natural-draft towers;
- Part II: characteristic- and performance-curve evaluation; and
- Part III: examples, `KaV/L` calculation, enthalpy tables and forms.

The prototype implements only public mathematical concepts. The current licensed standard must control:

- instrumentation accuracy, placement and calibration;
- data collection, averaging and rejection;
- allowable deviations and test validity;
- water-flow, fan-power, airflow and atmospheric corrections;
- characteristic exponent and contractual reference acceptance;
- performance-curve crossplot procedure;
- uncertainty and reporting; and
- contractual interpretation.

Do not describe a report from this repository as an “ATC-105 result” until the implementation has been checked against the licensed standard, CTI Toolkit and project-specific reference data by qualified engineers.

## ATC-140 boundary

The public CTI description says ATC-140 defines instrumentation and procedures for testing and evaluating cooling-tower drift.

This prototype only selects an eliminator from supplied drift and pressure records. It does not implement isokinetic sampling, traverse design, sample recovery, laboratory analysis, corrections or an ATC-140 report.

## STD-201 boundary

CTI describes STD-201 certification as independent verification that a product line performs in accordance with a manufacturer’s published thermal ratings. The prototype does not create, certify or validate a published product line. It therefore labels all bundled ratings as synthetic model results.

## MRL public product scope

The MRL site publicly lists:

| Program | Public description | Prototype relationship |
|---|---|---|
| IDCF | Induced-draft counterflow rating/sizing | The coupled selector has a comparable problem structure, but different code, coefficients and data |
| IDXF | Induced-draft crossflow rating/sizing | The crossflow grid demonstrates a separate topology; it is not a validated replacement |
| FDCF | Forced-draft counterflow rating/sizing | The fan/system framework can represent forced draft after geometry and pressure-plane changes |
| NDCF | Natural-draft counterflow; up to three fills and 16 concentric rings | The natural-draft prototype has one uniform zone; rings and multiple fills are production gaps |
| NDXF | Natural-draft crossflow rating/sizing | A coupled natural-draft crossflow solver is not implemented |
| CFPC | Induced-draft performance-curve plotting | Synthetic performance grids and interpolation are implemented, without an ATC-105 compliance claim |

The MRL site also states that:

- leading-supplier fill data are included in several products;
- fresh and seawater can be modeled by several programs; and
- source code is not provided.

Exact internal MRL equations, coefficients, iteration order, corrections and fill data therefore cannot be verified from the public description. The prototype uses an original engineering architecture:

```text
geometry → wet resistance → airflow → L/G → fill supply → thermal result → constraints/ranking
```

## Fan affinity laws: the source

The engine scales a fan's **recorded** curve with speed, at constant air density, by the fan
(affinity) laws:

```text
Q ∝ n        ΔP ∝ n²        Pshaft ∝ n³
```

- **Source.** Hudson Products Corporation, R. C. Monroe, *Fans Key to Optimum Cooling-Tower
  Design* (paper presented to the Cooling Technology Institute annual meeting, New Orleans, 1974;
  published by Chart Industries) states the basic fan law for axial cooling-tower fans as
  `CFM = f(rpm)¹`, `TP = f(rpm)²`, `HP = f(rpm)³`. [`docs/REFERENCES.md`](REFERENCES.md) item 18.
  The AMCA references there (items 15-16) identify the same affinity-law changes in airflow,
  pressure and power with speed and density.
- **In the engine.** The recorded curve **is** the ratio-1.0 curve. `rust/src/fan.rs` applies the
  three exponents as named constants (`FLOW_AFFINITY_EXPONENT`, `PRESSURE_AFFINITY_EXPONENT`, and
  the power leg `SHAFT_POWER_AFFINITY_EXPONENT`, which is the flow leg times the pressure leg —
  the engine stores no power curve and computes the shaft power from the recorded curve's own
  airflow, pressure and efficiency). The speed at which the curve is published is the record's own
  `nominalRpm`, so the rpm behind a ratio is `speedRatio × nominalRpm`; the record's
  `allowedSpeedRatio` band is its validity window, and a ratio outside it is refused **by name**
  (`rust/src/selection.rs::FanSpeedRatioLimit`) rather than clamped into the band.
- **Not a validation claim.** The engine is ported and drift-guarded, not validated: the source
  above states the relation the port implements, and nothing here claims the model, the synthetic
  curves or any result have been qualified against test data.

## Intellectual-property boundary

This repository contains no:

- CTI standard text, examples, tables or forms;
- CTI Toolkit executable or source;
- MRL executable, manual, source or proprietary database;
- manufacturer catalog curves; or
- copied commercial interface assets.

It provides original prototype code based on public engineering equations and clearly synthetic data.
