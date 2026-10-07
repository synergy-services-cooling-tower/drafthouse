# Validation and Uncertainty Plan

## 1. Validation levels

### Level A — numerical unit tests

Purpose: prove local code behavior and invariants.

The JavaScript reference's unit suite was deleted with the reference (issue #40 slice 4). Its scope
now lives in two places: the crate's own suites (`cd rust && cargo test` — psychrometrics, Merkel
demand and inverse, capability, performance curves, airside and fan balance, water balance, crossflow
and natural draft, nozzle, selection and the catalog validator) and the surviving JavaScript tests
(`npm test` — the deployment-shaped serving contract, the deploy set, the retirements and the
documentation drift gate). Both are hermetic: the JavaScript suite needs no cargo, browser or network,
and the crate's tests need no Node.

Commands:

```bash
npm test
cd rust && cargo test
```

### Level B — deterministic end-to-end smoke case

Purpose: prove that modules integrate coherently.

Command:

```bash
npm run smoke
```

The current smoke case calculates a Merkel demand, recovers the known CWT, evaluates an identical-point capability case, and runs the component selector.

### Level C — engineering benchmark cases

Required before internal engineering use:

- ASHRAE psychrometric reference tables across temperature, humidity, and pressure;
- approved water/seawater property references;
- hand-calculated Merkel examples with documented quadrature;
- known fan/system intersections and motor selections;
- fill/drift interpolation cases at nodes and between nodes;
- crossflow grid-refinement studies;
- natural-draft convergence studies;
- mass/energy closure under multiple duties.

### Level D — licensed software comparison

Required before claiming procedural or commercial equivalence:

- CTI Toolkit/ATC-105 characteristic cases;
- CTI Toolkit performance-curve/crossplot cases;
- applicable CTI drift evaluation cases;
- MRL IDCF/IDXF/FDCF/NDCF/NDXF/CFPC cases for supported families;
- manufacturer fan and tower-selection software cases.

### Level E — field and laboratory validation

Required for real product decisions:

- fill thermal and wet-pressure laboratory data;
- drift test data including installed sealing/bypass condition;
- fan test data and installation/system-effect evidence;
- tower acceptance-test datasets;
- repeatability, reproducibility, and uncertainty assessment;
- post-installation field checks.

### Level F — release qualification

A production release should require:

- approved requirements and equations;
- traceability from source/data to code and tests;
- independent engineering review;
- regression comparison to all controlled cases;
- browser/API/security tests;
- versioned catalog approval;
- documented known limitations;
- release sign-off.

## 2. Current automated evidence

`npm test` runs `node --test tests/*.test.js`. Its size, the `npm run validate` chain, the bundled
catalog revision and the smoke figures are re-derived in `VALIDATION_RESULTS.md`, and
`tests/validation-results-drift.test.js` fails when any of those documented figures stops matching
what the repository actually produces.

Issue #40 slice 4 retired the JavaScript reference; its test files went with it and the anchors they
established are carried by the Rust crate's own suites and the surviving repository contracts:

| Where | Evidence | Anchor |
|---|---|---|
| `rust/tests/anchors.rs` | Saturation pressure, saturated humidity ratio and saturated enthalpy 0–50 °C at 101 325 Pa; the ice branch vs IAPWS R14-08(2011) Eq. (6); the below-freezing wet-bulb relation vs an independent first-principles derivation; Buck (1981) and WMO/Sonntag enhancement factors; exact pressure scaling of the simplified path | ASHRAE published tables, IAPWS R14-08, PsychroLib recorded values, published equations — external data |
| `rust/tests/{psychrometrics,merkel,capability,performance_curve,airside,fan,water_balance,crossflow,natural_draft,nozzle,selection,validate}.rs` | The behavioural checks: integration agreement and CWT recovery, per-section reference velocities, pressure-component closure and quadratic loss scaling, grid refinement (convergence study), natural-draft pressure closure, selection constraints and tie-break chains, refusal messages byte-for-byte | Independent reference integration; the reference's own recorded messages and vectors |
| `rust/tests/vectors.rs`, `scripts/parity/run.mjs` | The recorded vectors replay; the drift check of the engine (native and wasm) against `validation/regression-baseline.json` | Regression only |
| `tests/deployment-serving.test.js`, `tests/deploy-check.test.js`, `tests/retirement-check.test.js`, `tests/validation-results-drift.test.js` | The serving/pin/cache contracts, the deploy set's derivation, the two retirements a gate must hold (D17, D22) and the documented figures | Repository contracts |

Passing local tests demonstrate code consistency, not external engineering accuracy. The
`validation/test-vectors.json` family is engine-generated (see its `provenance` block): replaying it
shows the engine still produces the recorded values, nothing more. Where a case does compare against
something outside this repository, the anchor is named in the file and in the table above.

## 3. CTI comparison matrix

**Status: no comparison has been made.** Nothing in this project has been run through CTI ToolKit.
`validation/cti-comparison-matrix.csv` is the prepared *input* matrix for that run — 21 Merkel
duties covering wet bulb, range, approach, `L/G` and pressure, in the units and conventions a human
would type into the Toolkit. Its `CTI_ToolKit_result` and `delta_pct` columns are **empty for every
row, and stay empty until a licensed CTI ToolKit run fills them**, with the Toolkit version
recorded. The two `KaV_L_*` columns are this engine's own output under the two inlet-enthalpy
conventions; they are not CTI output and must not be read as a comparison. The same statement is
carried at the top of the CSV itself so that a reader of the file does not have to infer it from
blank cells. The CSV was produced by `scripts/generate-vectors.mjs`, which was retired with the
JavaScript reference (issue #40 slice 4); it is now frozen evidence — do not edit by hand.

For each controlled comparison, record:

| Field | Required record |
|---|---|
| Standard/Toolkit version | Exact version and revision |
| Method | Characteristic or performance curve |
| Tower type | Induced/forced/natural; counterflow/crossflow |
| Reference basis | Contract/design curve source and revision |
| Inputs | Raw and corrected values with units |
| Corrections | Each applied rule and intermediate value |
| Psychrometrics | Humidity ratio, enthalpy, densities |
| Demand | `L/G`, `KaV/L`, integration method |
| Curve result | Intersection/crossplot intermediate values |
| Final result | Capability or leaving-water deviation |
| Uncertainty | Method and result |
| Difference | Absolute/relative residual and explanation |
| Approval | Reviewer, date, disposition |

Do not compare only the final percentage; intermediate agreement is necessary to locate equation, correction, interpolation, or rounding differences.

## 4. MRL comparison matrix

For each supported family, record:

- MRL program and version;
- tower type and complete geometry;
- fill identity, depth, data revision, and thermal/pressure corrections;
- fan curve, speed, pitch, density, and pressure definition;
- atmospheric and water properties;
- water flow and solution target;
- MRL and prototype airflow;
- `L/G`, `L′`, `G′`, `KaV/L`;
- fill, drift, and total pressure drop;
- CWT, heat rejection, outlet-air state, and evaporation;
- fan power and motor basis;
- residuals, tolerances, and engineering explanation.

Exact agreement is not assumed because public MRL descriptions do not disclose proprietary source code or databases.

## 5. Property validation

Psychrometrics should be validated at:

| Item | Status at this revision |
|---|---|
| subfreezing and above-freezing saturation branches | **Covered, external anchor.** The ice branch is checked against the IAPWS R14-08(2011) sublimation equation from -60 °C to 0 °C (~0.03 % worst case) and against ASHRAE over-ice table values; the liquid branch against ASHRAE tables 0–50 °C (`rust/tests/anchors.rs`; the JavaScript tests that first established them were retired in issue #40 slice 4). Only 101 325 Pa is tabulated against below 0 °C. |
| low/high humidity | Partly covered: 20/50/90 % RH round trips and near-saturation states at standard pressure; no external anchor for a low-humidity state. |
| sea-level and elevated/lowered pressure | **Partly covered.** The simplified path is anchored against published ASHRAE saturation pressures (temperature-only) plus the published equation, which fixes it exactly at any pressure, and 61.7–105 kPa cases are exercised (`rust/tests/anchors.rs`: the sub-zero and altitude round trips; the JavaScript tests that first established them were retired with the reference). The enhancement factor matches Buck (1981) exactly and two published pressure-only approximations to within 0.1 %, but **no external anchor for the real-mixture (enhanced) treatment away from 1 atm exists in this repository's cited literature** — see item 1.3 and `LIMITATIONS.md` §6. |
| near-saturation conditions | Covered as a code path (28 °C/27.5 °C, saturated-at-wet-bulb cases); no external anchor. |
| wet bulb close to dry bulb | Covered for both branches (`-2/-2`, small-depression cases), plus the documented positivity limit of the below-freezing relation (`LIMITATIONS.md` §6). |
| dew point inversion | Covered by round trips in `rust/tests/anchors.rs` (humidity ratio / relative humidity, the sub-zero wet-bulb relation, and states at altitude) — the JavaScript tests these were ported from were retired with the reference. |

"Covered, external anchor" means the expectation comes from a published source; "covered" alone
means the case pins behaviour or invariants and cannot show accuracy.

Water properties should be validated for the full intended freshwater/seawater temperature and salinity envelope.

## 6. Component-data validation

For each approved fill, drift eliminator, fan, and nozzle:

1. verify source identity and rights;
2. reproduce source curve nodes exactly;
3. verify interpolation between nodes;
4. reject or explicitly flag extrapolation;
5. preserve units, test configuration, and uncertainty;
6. add regression cases for each dataset revision;
7. verify installation constraints such as orientation, sealing, supports, and pressure definitions.

## 7. Numerical convergence

Required studies:

- Simpson segment count versus Merkel result;
- root-scan density and tolerance versus CWT/capability;
- crossflow cell count versus CWT and outlet-air state;
- natural-draft flow bracket and residual tolerance;
- Monte Carlo sample count versus percentile stability;
- selector ranking stability under numerical tolerances.

## 8. Measurement uncertainty

The current Monte Carlo function accepts user-specified standard uncertainties and assumes independent normal perturbations. A production uncertainty model should additionally support:

- correlated measurements;
- calibration bias and drift;
- repeated-observation statistics;
- distribution choices other than normal;
- curve/data uncertainty;
- fan-flow inference uncertainty;
- model-form uncertainty;
- prescribed coverage factors and reporting rules.

## 9. Regression release gates

Block release when:

- any test fails;
- a controlled benchmark exceeds its approved tolerance;
- catalog data change without revision, approval, and regression cases;
- an output label implies certification incorrectly;
- a numerical method changes without convergence evidence;
- a supported browser workflow produces console errors;
- documentation and code paths diverge;
- known limitations are removed without evidence.


## 8. Prioritised validation gate

Status as of 19 September 2026: the suite validates against ASHRAE property tables, the IAPWS sublimation equation and internal consistency. **Nothing in this project has yet been compared against anything from the cooling-tower domain.** The items below are ordered by what blocks what, not by effort.

### Tier 1 — blocks any public deployment

Per the private decision record D2, the website and the repository are a single disclosure; this tier gates both.

| # | Check | Method | Why it blocks |
| --- | --- | --- | --- |
| 1.1 | Merkel demand and capability vs **CTI ToolKit** | Run identical inputs through both; record deltas across a matrix of range/approach/wet-bulb/(L/G). **Set `inletEnthalpyConvention: 'cti-saturated-wetbulb'`** — the default `'bulk'` convention carries a built-in ~1.1 % offset that would otherwise be misread as an error. | CTI ToolKit is the tool your industry already uses. Any systematic disagreement must be known and explainable before a client finds it. |
| 1.2 | **Sub-zero psychrometrics** | **Covered as far as external anchors exist.** The ice-branch saturation correlation (`dryBulbC <= 0.01`) is checked against the IAPWS R14-08(2011) sublimation equation, Eq. (6), from -60 °C to 0 °C (worst case ~0.03 %, inside the equation's own uncertainty) and against ASHRAE over-ice table values (≤0.02 %). The below-freezing wet-bulb relation is checked against an independent first-principles adiabatic-saturation derivation using published constants (≤0.25 %; the residual is the handbook's rounded 2830 kJ/kg sublimation group against 2834.4 = 2501 + 333.4). Saturated humidity ratios below freezing agree with the ASHRAE 0 °C table value (0.002 %) and with PsychroLib's recorded references (≤0.08 %). Tests: `rust/tests/anchors.rs` (the JavaScript tests that first established these anchors were retired with the reference). | Cold-weather duties and every sub-zero input enter through this branch; an error here silently corrupts the state (and the wet-bulb round trip) for any below-freezing case. **Still open:** the relation's low-temperature positivity limit and the engine's silent floor of an impossible pair (`LIMITATIONS.md` §6), and any measured data. |
| 1.3 | **Non-standard pressure** | **Partly covered.** Reduced- and elevated-pressure cases at 61 660–105 000 Pa now exist in the suite and in the parity vectors (ideal-gas and real-mixture paths, sub-zero and ambient). The simplified (enhancement-off) path is *exactly* pressure-scaled given the published saturation pressures, which are anchored by ASHRAE tables at standard pressure; the enhancement factor is Buck's published form to the last bit and within 0.07 % of the independent WMO/Sonntag pressure-only form over 62–105 kPa. Tests: `rust/tests/anchors.rs` (the JavaScript tests that first established these anchors were retired with the reference). | Altitude and reduced-pressure duties are routine, and the pressure term multiplies every humidity ratio. **Still open:** ASHRAE's full enhancement factor depends on temperature *and* pressure (Hyland–Wexler / RP-1485). This repository holds neither that formulation nor reference data computed with it, so the real-mixture treatment's accuracy away from 1 atm is unverified, and no second published anchor for it exists here. |
| 1.4 | **Browser click-through** | Manual pass on Chrome, Safari and Firefox: every tab, every worked-calculation panel, error paths. | The interface has only ever been verified headlessly. `render.test.js` checks the emitted HTML, not that a browser renders or scripts it correctly. |

Item 1.1 remains entirely open: no CTI ToolKit run has been made, so the comparison matrix's CTI
columns are empty (§3). Item 1.4 remains manual and unperformed.

### Tier 2 — blocks anyone relying on the results

| # | Check | Method |
| --- | --- | --- |
| 2.1 | Published counterflow benchmark cases | Reproduce canonical cases from the literature (Kloppers & Kröger). Independent of CTI and citable. |
| 2.2 | Solver robustness sweep | Permanent property test across the full feasible envelope of range, approach, wet bulb, L/G and pressure: no unhandled failures, no non-convergence, no silently clamped extrapolation. |
| 2.3 | Merkel vs Poppe divergence | Once Poppe exists (the private decision record D7), quantify where the Merkel approximation costs accuracy — particularly outlet air state and evaporation rate. |

### Tier 3 — blocks selector output reaching a client

CTI ToolKit validates none of this. It is the weakest area of the project and entirely unmeasured.

| # | Check | Method |
| --- | --- | --- |
| 3.1 | **Air-side pressure against a real tower** | One installed cell with measured airflow and static pressure. Compare the itemised build-up, not just the total. |
| 3.2 | Fan operating point | Real fan curve, measured duty, measured shaft power. |
| 3.3 | Fill thermal correlation | Manufacturer or laboratory test data at several loadings and depths. |
| 3.4 | Drift performance | Installed-system drift measurement, not eliminator-only curves. |
| 3.5 | Evaporation and make-up | Measured make-up water on a running installation vs predicted. |

Until Tier 3 passes, every loss coefficient, fill correlation and drift curve in the bundled catalog is synthetic and has never met reality. Correcting *where* the terms are referenced (see `ENGINEERING_METHODS.md` §11) does not make the coefficients real.

### Not on the roadmap

The natural-draft model is labelled demonstration-only rather than scheduled for validation. See the private decision record D8.

### Recording results

Record every completed item in `VALIDATION_RESULTS.md` with the date, the tool or data source, the input matrix, and the observed deltas — including the ones that disagree. A published disagreement you can explain is an asset; an unpublished one is a liability.
