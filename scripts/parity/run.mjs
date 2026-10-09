#!/usr/bin/env node
/**
 * Parity harness — the engine under test (the Rust port, native or wasm) vs its comparison:
 * the RECORDED REGRESSION BASELINE in `validation/regression-baseline.json`.
 *
 * The live JavaScript comparison this harness once ran was RETIRED with the reference engine
 * (issue #40, slice 4). The last commit that contains the JavaScript reference implementation
 * is recorded in the private decision record (D17): the
 * reference lives in git history only, from that commit on. The baseline
 * holds the values the two implementations agreed on at capture, so this is a DRIFT CHECK, not
 * a judgement about whether the physics is right: a disagreement is a finding — the baseline
 * moves only with a cited external correction, never to make a port or a fix pass.
 *
 * Usage (from the repository root, one command):
 *
 *     node scripts/parity/run.mjs                      # the recorded regression baseline
 *     node scripts/parity/run.mjs --engine wasm        # the wasm build of the same engine
 *     node scripts/parity/run.mjs --reference frozen   # the same (kept for existing callers)
 *
 * `--reference live` is refused (exit 2): this tree no longer holds the JavaScript reference,
 * so a live comparison cannot run — and asking for one is an error, never a silent skip, so
 * "skipped" cannot read as "passed". `--write-baseline` is refused too: the baseline is frozen
 * recorded evidence and can no longer be re-recorded from a live reference.
 *
 * It builds the crate (`cargo build`, skipped with `--no-build`), replays every recorded
 * engine invocation and diffs every quantity per its recorded tolerance:
 *
 *     pass when |Δ| <= absolute tolerance OR |Δ| / |baseline| <= relative tolerance
 *
 * Each row names its case, its quantity and the provenance the recorded entry carries. Every
 * quantity is re-read from the engine's own reply by the entry's recorded extraction rule; an
 * entry the harness cannot replay is a loud exit-2 failure, never a dropped comparison.
 *
 * Exit status: 0 all quantities within tolerance; 1 at least one out (the failures are listed
 * at the end); 2 the harness itself could not run (no cargo, no binary, no baseline file, a
 * recorded entry that cannot be replayed).
 */
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createEngine } from '../../rust/wasm/binding.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const CRATE_MANIFEST = join(root, 'rust', 'Cargo.toml');
const ENGINE = join(root, 'rust', 'target', 'debug', 'ct-engine');
const NO_BUILD = process.argv.includes('--no-build');

/*
 * `--engine native` (the default) drives the binary. `--engine wasm` drives the wasm build
 * of the same engine through the thin JS binding (`rust/wasm/binding.mjs`) — the shape the
 * publishable `engine` piece has — and cross-checks every answer against the native binary,
 * so the two builds of one engine are diffed against each other rather than both merely
 * plausible.
 */
const engineFlag = process.argv.indexOf('--engine');
const ENGINE_KIND = engineFlag === -1 ? 'native' : process.argv[engineFlag + 1];
if (!['native', 'wasm'].includes(ENGINE_KIND)) {
  console.error(`--engine expects native or wasm, got ${ENGINE_KIND}`);
  process.exit(2);
}
const WASM_ENGINE = ENGINE_KIND === 'wasm';
const WASM_ARTIFACT = join(root, 'rust', 'target', 'wasm32-unknown-unknown', 'wasm', 'synergy_drafthouse.wasm');

/* ---------------- the comparison partner (issue #40 slices 2 and 4) ----------------
 *
 * The comparison this harness keeps is: the engine under test — the native binary, or the
 * wasm build of the same engine — reproduces the RECORDED REGRESSION BASELINE in
 * `validation/regression-baseline.json`, per case and per quantity, inside the tolerance that
 * quantity already carries. The harness has exactly one comparison partner.
 *
 *   --reference auto     (default) the recorded regression baseline
 *   --reference frozen   the same — kept so existing invocations and evidence scripts keep
 *                        working
 *   --reference live     refused (exit 2): the JavaScript reference was retired (issue #40
 *                        slice 4) and is no longer in this tree. A live comparison cannot
 *                        run, and it is an error rather than a skip, so a skipped live leg
 *                        can never be read as a passed one.
 *
 * `--write-baseline` is refused (exit 2): the baseline records what the JavaScript reference
 * produced at capture, and that reference is no longer in this tree, so the file can no
 * longer be re-recorded from a live run. It is frozen evidence; a value moves only with a
 * cited external correction, in the same commit as the citation.
 */
const referenceFlag = process.argv.indexOf('--reference');
const REFERENCE_MODE = referenceFlag === -1 ? 'auto' : process.argv[referenceFlag + 1];
if (!['auto', 'frozen'].includes(REFERENCE_MODE)) {
  console.error(`--reference expects auto or frozen, got ${REFERENCE_MODE}`);
  console.error('the live JavaScript reference was retired (issue #40 slice 4); the recorded regression baseline is the only comparison partner');
  process.exit(2);
}
if (process.argv.includes('--write-baseline')) {
  console.error('--write-baseline is retired: the recorded regression baseline holds the values the');
  console.error('JavaScript reference produced at capture, and that reference is no longer in this tree');
  console.error('(issue #40 slice 4). It is frozen evidence — a value moves only with a cited external');
  console.error('correction (see validation/regression-baseline.json documentation and the private decision record D17).');
  process.exit(2);
}

const BASELINE_PATH = join(root, 'validation', 'regression-baseline.json');

/* ---------------- named tolerances ---------------- */

const TOLERANCES = {
  // Echoed inputs: the JSON round trip must be exact.
  dryBulbC: { abs: 0, rel: 0 },
  pressurePa: { abs: 0, rel: 0 },
  // Direct evaluations. Only transcendental-implementation noise (a few ulp) can separate
  // the two engines here, so a 1e-9 relative tolerance already leaves ~7 orders of headroom.
  humidityRatio: { abs: 1e-12, rel: 1e-9 },
  relativeHumidity: { abs: 1e-12, rel: 1e-9 },
  enthalpyKJkgDryAir: { abs: 1e-9, rel: 1e-9 },
  dryAirDensityKgM3: { abs: 1e-12, rel: 1e-9 },
  moistAirDensityKgM3: { abs: 1e-12, rel: 1e-9 },
  enhancementFactor: { abs: 1e-12, rel: 1e-12 },
  saturationVaporPressurePa: { abs: 1e-9, rel: 1e-9 },
  saturationHumidityRatio: { abs: 1e-12, rel: 1e-9 },
  saturatedAirEnthalpyKJkgDryAir: { abs: 1e-9, rel: 1e-9 },
  merkelNumber: { abs: 1e-9, rel: 1e-9 },
  inletAirEnthalpyKJkgDryAir: { abs: 1e-9, rel: 1e-9 },
  cpWaterKJkgK: { abs: 1e-12, rel: 1e-9 },
  rangeC: { abs: 1e-6, rel: 1e-9 },
  // The demand evaluated at a root-solved cold-water temperature. The temperature carries
  // the scan's solver floor (~1e-7 K in the residual), which moves KaV/L by ~1e-7; the
  // direct quadrature comparison in the demand section is the 1e-9 check.
  requiredMerkelNumber: { abs: 1e-6, rel: 1e-6 },
  // Root-solved temperatures: the solvers stop on a bracket width (1e-6 K for the dew
  // point, 1e-9 K for the wet bulb, 1e-7 K for the cold-water scan), so an absolute floor
  // equal to that stopping width is the honest tolerance rather than a fudge factor.
  wetBulbC: { abs: 1e-6, rel: 1e-9 },
  dewPointC: { abs: 1e-6, rel: 1e-9 },
  coldWaterC: { abs: 1e-6, rel: 1e-9 },
  approachC: { abs: 1e-6, rel: 1e-9 },
  // Air-side quantities (slice 2): direct evaluations of the reference formulas — products,
  // quotients and `pow` terms. Only transcendental-implementation noise (a few ulp) can
  // separate the two engines, so 1e-9 relative already leaves ~7 orders of headroom.
  totalPa: { abs: 1e-9, rel: 1e-9 },
  fillPa: { abs: 1e-9, rel: 1e-9 },
  driftPa: { abs: 1e-9, rel: 1e-9 },
  inletPa: { abs: 1e-9, rel: 1e-9 },
  distributionPa: { abs: 1e-9, rel: 1e-9 },
  supportPa: { abs: 1e-9, rel: 1e-9 },
  plenumPa: { abs: 1e-9, rel: 1e-9 },
  fanStackPa: { abs: 1e-9, rel: 1e-9 },
  driftPpm: { abs: 1e-9, rel: 1e-9 },
  waterLoadingKgM2S: { abs: 1e-12, rel: 1e-9 },
  dryAirLoadingKgM2S: { abs: 1e-12, rel: 1e-9 },
  dryAirMassFlowKgS: { abs: 1e-9, rel: 1e-9 },
  fillVelocityMS: { abs: 1e-12, rel: 1e-9 },
  driftVelocityMS: { abs: 1e-12, rel: 1e-9 },
  inletVelocityMS: { abs: 1e-12, rel: 1e-9 },
  plenumVelocityMS: { abs: 1e-12, rel: 1e-9 },
  fanStackVelocityMS: { abs: 1e-12, rel: 1e-9 },
  fillMerkelNumber: { abs: 1e-12, rel: 1e-9 },
  sprayZoneMerkelNumber: { abs: 1e-12, rel: 1e-9 },
  rainZoneMerkelNumber: { abs: 1e-12, rel: 1e-9 },
  availableMerkelNumber: { abs: 1e-12, rel: 1e-9 },
  // Fields the breakdown echoes straight out of the catalog record: the JSON round trip
  // through the command line must reproduce them bit for bit.
  fixedPa: { abs: 0, rel: 0 },
  fillAreaM2: { abs: 0, rel: 0 },
  airFreeAreaM2: { abs: 0, rel: 0 },
  driftAreaM2: { abs: 0, rel: 0 },
  inletAreaM2: { abs: 0, rel: 0 },
  plenumAreaM2: { abs: 0, rel: 0 },
  fanStackAreaM2: { abs: 0, rel: 0 },
  // Fan operating point (slice 2). `flowM3S` is the root of the scan-then-bisect system
  // balance: the solvers stop on a 1e-7 Pa residual (or a bracket of the same width), and the
  // residual slope near the operating point (≈4.5 Pa per m³/s for these cases) bounds the flow
  // error at ≈2e-8 m³/s — a relative floor of 1e-9 leaves an order of headroom on top of that.
  // The four quantities derived from the solved flow inherit that floor exactly.
  flowM3S: { abs: 1e-6, rel: 1e-9 },
  fanPressurePa: { abs: 1e-6, rel: 1e-9 },
  systemPressurePa: { abs: 1e-6, rel: 1e-9 },
  efficiency: { abs: 1e-12, rel: 1e-9 },
  shaftPowerKW: { abs: 1e-6, rel: 1e-9 },
  // Both residuals are inside the solver's own 1e-7 Pa stopping tolerance; the comparison
  // only has to prove that neither engine returned a point where the balance is off.
  residualPa: { abs: 1e-6, rel: 1e-6 },
  // Fan operating limits are products of an echoed flow and the speed ratio.
  minFlowM3S: { abs: 0, rel: 1e-12 },
  maxFlowM3S: { abs: 0, rel: 1e-12 },
  speedRatio: { abs: 0, rel: 0 },
  // The affinity estimate: a cube root (`** (1/3)`), i.e. a transcendental evaluation.
  airflowM3S: { abs: 1e-9, rel: 1e-9 },
  // Motor selection: a quotient/product of echoed inputs, plus the table lookup itself.
  requiredMotorOutputKW: { abs: 0, rel: 1e-12 },
  selectedMotorKW: { abs: 0, rel: 0 },
  // The envelope's failure count is an integer both engines must agree on exactly.
  failureCount: { abs: 0, rel: 0 },
  // Water balance (slice 3). Every quantity is a product, quotient or difference of inputs
  // that crossed the command line as shortest-round-trip decimals (the reference's own
  // psychrometric inlet state included), so both engines run the identical IEEE-754
  // operations on the identical `f64`s and the comparison below is exact equality.
  evaporationKgS: { abs: 0, rel: 0 },
  driftKgS: { abs: 0, rel: 0 },
  blowdownKgS: { abs: 0, rel: 0 },
  makeupKgS: { abs: 0, rel: 0 },
  cyclesOfConcentration: { abs: 0, rel: 0 },
  // Crossflow (slice 4). `coldWaterC` is grid-solved (an `exp` per cell), so a few ulp of
  // transcendental noise is possible; the rest are products and means of the same grid, and
  // the recorded vectors carry the suite's own absolute floor via `agrees`-style tolerance.
  outletAirDryBulbC: { abs: 1e-6, rel: 1e-9 },
  outletAirHumidityRatio: { abs: 1e-12, rel: 1e-9 },
  outletAirEnthalpyKJkgDryAir: { abs: 1e-9, rel: 1e-9 },
  heatTransferKW: { abs: 1e-6, rel: 1e-9 },
  waterEnergyKW: { abs: 1e-6, rel: 1e-9 },
  coarseColdWaterC: { abs: 1e-6, rel: 1e-9 },
  fineColdWaterC: { abs: 1e-6, rel: 1e-9 },
  richardsonColdWaterC: { abs: 1e-6, rel: 1e-9 },
  estimatedDiscretizationErrorC: { abs: 1e-6, rel: 1e-9 },
  observedOrder: { abs: 1e-6, rel: 1e-6 },
  extrapolatedColdWaterC: { abs: 1e-6, rel: 1e-9 },
  fineGridErrorEstimateC: { abs: 1e-6, rel: 1e-9 },
  gridConvergenceIndexPct: { abs: 1e-9, rel: 1e-9 },
  coarseCellsColdWaterC: { abs: 1e-6, rel: 1e-9 },
  mediumCellsColdWaterC: { abs: 1e-6, rel: 1e-9 },
  fineCellsColdWaterC: { abs: 1e-6, rel: 1e-9 },
  // Nozzles (slice 4): a square root and exact quotients, so ulp-level noise only.
  nozzleFlowM3S: { abs: 1e-12, rel: 1e-9 },
  flowPerNozzleM3S: { abs: 1e-12, rel: 1e-9 },
  actualTotalFlowM3S: { abs: 1e-12, rel: 1e-9 },
  excessFlowPct: { abs: 1e-9, rel: 1e-9 },
  nozzleCount: { abs: 0, rel: 0 },
  // Selection (slice 4). `thermalMarginC` is the target minus the solved cold-water
  // temperature; `waterVolumetricFlowM3S` a quotient of echoed inputs. `fanFlowM3S` /
  // `electricalInputKW` carry the fan operating point's inherited scan width, like `flowM3S`
  // and `shaftPowerKW` above (the electrical input is the shaft power over two efficiencies).
  thermalMarginC: { abs: 1e-6, rel: 1e-9 },
  waterVolumetricFlowM3S: { abs: 1e-9, rel: 1e-9 },
  fanFlowM3S: { abs: 1e-6, rel: 1e-9 },
  electricalInputKW: { abs: 1e-6, rel: 1e-9 },
  feasibleCandidateCount: { abs: 0, rel: 0 },
  capacitiesResolved: { abs: 0, rel: 0 },
  // The two quantities the reference engine never computed. Both engines solve them by the
  // SAME bracketed bisection of the SAME physics: `capacityKgS` stops on a 1e-6 kg/s bracket
  // (so an absolute floor of 1e-4 is a hundred times the solver's own last step) and
  // `capabilityRatio` on a 1e-9 KaV/L bracket (1e-6 relative leaves four orders of headroom).
  capacityKgS: { abs: 1e-4, rel: 1e-6 },
  capabilityRatio: { abs: 1e-6, rel: 1e-6 },
  // Slice 5 (issue #40) — the capability projection. `capabilityPct` and
  // `capabilityWaterToDryAirRatio` are root-solved: the scan stops on a 1e-8 residual (or a
  // 1e-8 bracket width) and the capability moves ≈80 pct-points per unit of L/G, so the
  // solver's own floor is ≈1e-6 pct-points — the named absolute floor is that width with
  // headroom. `testCharacteristicCoefficient` and the two curve ordinates are direct
  // evaluations (a quotient, a power term, a quadrature), where only transcendental-
  // implementation noise can separate the engines: 1e-9 relative leaves ~7 orders of headroom.
  // The two ratio fields are quotients of echoed conditions and are compared exactly.
  capabilityPct: { abs: 1e-5, rel: 1e-9 },
  capabilityWaterToDryAirRatio: { abs: 1e-7, rel: 1e-9 },
  testMerkelNumber: { abs: 1e-9, rel: 1e-9 },
  testCharacteristicCoefficient: { abs: 1e-9, rel: 1e-9 },
  characteristicExponent: { abs: 0, rel: 0 },
  designWaterToDryAirRatio: { abs: 0, rel: 0 },
  testWaterToDryAirRatio: { abs: 0, rel: 0 },
  designDemandMerkel: { abs: 1e-9, rel: 1e-9 },
  testCharacteristicMerkel: { abs: 1e-9, rel: 1e-9 },
  // Slice 5 — the Monte-Carlo report. Every sample's capability is itself a projection whose
  // solved L/G carries the 1e-8 scan width, and each perturbation the stream applies can
  // differ between the engines by a few ulp (Math.log/Math.cos vs the platform libm), so the
  // summary statistics inherit that floor over ~1e3 samples rather than the 1e-9 of a single
  // direct evaluation. The measured worst deviation is recorded with the maintainers.
  meanCapabilityPct: { abs: 1e-4, rel: 1e-6 },
  standardDeviationPctPoints: { abs: 1e-4, rel: 1e-6 },
  p2_5: { abs: 1e-4, rel: 1e-6 },
  p50: { abs: 1e-4, rel: 1e-6 },
  p97_5: { abs: 1e-4, rel: 1e-6 },
  expandedUncertaintyApproxPctPoints: { abs: 1e-4, rel: 1e-6 },
  // Slice 5 — natural draft. `volumetricAirFlowM3S` is the root of the 480-sample scan: the
  // scan stops on a 1e-6 Pa residual, and the residual's slope near the operating point
  // (~3e-4 Pa per m³/s on the documented tower) bounds the flow error at ~3e-3 m³/s, which is
  // what the named absolute floor covers. The densities are direct evaluations; `residualPa`
  // and `thermal.*` reuse the tolerances the harness already names for those classes
  // (`residualPa`, `coldWaterC`, `requiredMerkelNumber`, `cpWaterKJkgK`).
  volumetricAirFlowM3S: { abs: 1e-2, rel: 1e-9 },
  waterToDryAirRatio: { abs: 1e-9, rel: 1e-9 },
  plumeDensityKgM3: { abs: 1e-12, rel: 1e-9 },
  draftPressurePa: { abs: 1e-9, rel: 1e-9 },
  // Slice 5 — performance curves. The bilinear prediction is a chain of lerps over values
  // that crossed the command line as shortest-round-trip decimals: the identical `f64`s and
  // the identical operations, so it is compared exactly. The inverse solve stops on a 1e-7 K
  // residual; the grid's slope near the test point (~0.02 K per kg/s) bounds the flow error at
  // ~1e-5 kg/s, and the prediction at that flow at ~1e-8 K — both named floors below.
  predictedColdWaterC: { abs: 0, rel: 0 },
  solvedPredictedColdWaterC: { abs: 1e-6, rel: 1e-9 },
  predictedWaterFlowKgS: { abs: 1e-3, rel: 1e-9 },
  // The capability ratio and the deviation follow from the solved flow, so they inherit the
  // same floor; the deviation is a difference against the test temperature (a 1e-6 K floor,
  // the class `coldWaterC` names).
  predictedColdWaterAtAdjustedFlowC: { abs: 1e-6, rel: 1e-9 },
  leavingWaterDeviationC: { abs: 1e-6, rel: 1e-9 },
};

/* ---------------- bookkeeping ---------------- */

const totals = { passed: 0, failed: 0 };
const failures = [];
let currentSection = '';
function section(title) {
  currentSection = title;
  console.log('');
  console.log(`== ${title} ==`);
}

function pad(text, width) {
  const value = String(text);
  return value.length >= width ? value : value + ' '.repeat(width - value.length);
}

function formatValue(value) {
  if (typeof value !== 'number') return String(value);
  if (!Number.isFinite(value)) return String(value);
  if (value === 0) return '0';
  return value.toPrecision(9);
}

function formatTolerance(tolerance) {
  return `rel ${tolerance.rel} / abs ${tolerance.abs}`;
}

function row({ caseId, quantity, js, rust, abs, rel, tolerance, pass, note, verdict }) {
  if (pass !== null) {
    totals[pass ? 'passed' : 'failed'] += 1;
  }
  console.log(
    [
      pad(caseId, 40),
      pad(quantity, 36),
      pad(formatValue(js), 20),
      pad(formatValue(rust), 20),
      pad(abs === null ? '-' : formatValue(abs), 12),
      pad(rel === null ? '-' : formatValue(rel), 12),
      pad(tolerance === null ? note ?? '-' : formatTolerance(tolerance), 26),
      verdict ?? (pass ? 'PASS' : 'FAIL'),
    ].join(' '),
  );
  if (pass === false) {
    failures.push({ section: currentSection, caseId, quantity, js, rust, abs, rel, tolerance, note });
  }
}

/** Compare one numeric quantity. */
function compareQuantity(caseId, quantity, js, rust, tolerance) {
  // Quantity labels carry a ` [source]` suffix; the tolerance is named after the leaf name.
  const leaf = String(quantity).replace(/ \[[^\]]*\]$/, '').split('.').pop();
  tolerance = tolerance ?? TOLERANCES[leaf];
  if (!tolerance) throw new Error(`no tolerance named for quantity ${quantity}`);
  // A field the engine may legitimately not resolve (`fanStackVelocityMS` on a
  // static-pressure curve): both sides must be null, or both must be numbers.
  if (js === null || js === undefined || rust === null || rust === undefined) {
    const pass = js === rust;
    row({ caseId, quantity, js, rust, abs: null, rel: null, tolerance: null, pass, note: 'both null' });
    return;
  }
  if (typeof js !== 'number' || typeof rust !== 'number') {
    row({ caseId, quantity, js, rust, abs: null, rel: null, tolerance: null, pass: false, note: 'non-numeric' });
    return;
  }
  const abs = Math.abs(js - rust);
  const rel = js === 0 ? Infinity : abs / Math.abs(js);
  const pass = abs <= tolerance.abs || rel <= tolerance.rel;
  row({ caseId, quantity, js, rust, abs, rel, tolerance, pass });
}

/** Compare refusals: the reference throws, the port must not return a value. */
/** Compare a non-numeric field (an enum spelling, a verdict, a message list) for equality. */
function compareText(caseId, quantity, js, rust, note = 'exact text') {
  row({
    caseId,
    quantity,
    js,
    rust,
    abs: null,
    rel: null,
    tolerance: null,
    pass: js === rust,
    note,
  });
}

/* ---------------- engines ---------------- */

function nativeRun(args) {
  const result = spawnSync(ENGINE, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (result.error) {
    throw new Error(`failed to run ${ENGINE}: ${result.error.message}`);
  }
  if (result.status === 0) {
    return { ok: true, value: JSON.parse(result.stdout) };
  }
  let error;
  if (result.status === 2) {
    error = { error: 'usage', message: result.stderr.trim() };
  } else {
    try {
      error = JSON.parse(result.stderr);
    } catch {
      error = { error: 'unparsable-error', message: result.stderr.trim() };
    }
  }
  return { ok: false, status: result.status, error };
}

/*
 * In `--engine wasm` mode every answer is cross-checked against the native binary for the
 * same arguments: one engine, two builds. The reply SHAPE must match exactly, and the
 * numbers must match to the last few ulp — wasm32-unknown-unknown has no host libm, so the
 * wasm build evaluates exp/ln/pow through Rust's own implementations and the quadratures
 * can land a few ulp away from the platform libm's. The worst spread measured over this
 * suite is 1.28e-13 relative — 0.128 of the 1e-12 tolerance, about 8x headroom, under one
 * order (an earlier "~3e-16 / three orders" figure here was falsified by the #24 review;
 * this one is the run's own). 1e-12 is still far below any tolerance this harness measures
 * a quantity with, so a real defect — a changed coefficient, a dropped term — cannot hide
 * inside it.
 */
const CROSS_CHECK_TOLERANCE = { abs: 1e-12, rel: 1e-12 };
const crossCheck = { calls: 0, identical: 0, within: 0, beyond: 0, worstUsed: 0, worstCase: null, worstAbsolute: 0, worstRelative: 0 };
let wasmEngine = null;

function replyDifference(wasm, native, path, state) {
  if (wasm === native) return null;
  if (typeof wasm === 'number' && typeof native === 'number') {
    const absolute = Math.abs(wasm - native);
    const relative = native === 0 ? absolute : absolute / Math.abs(native);
    state.differing += 1;
    // How much of the cross-check tolerance this pair used up: 1 means it would fail, so
    // the reported worst is the headroom the two builds actually left.
    const used = Math.min(absolute / CROSS_CHECK_TOLERANCE.abs, relative / CROSS_CHECK_TOLERANCE.rel);
    if (used > state.worstUsed) {
      state.worstUsed = used;
      state.worstPath = path;
      state.worstAbsolute = absolute;
      state.worstRelative = relative;
    }
    if (absolute <= CROSS_CHECK_TOLERANCE.abs || relative <= CROSS_CHECK_TOLERANCE.rel) return null;
    return `${path || 'value'}: wasm ${wasm} vs native ${native} (abs ${absolute}, rel ${relative})`;
  }
  if (wasm === null || native === null || typeof wasm !== 'object' || typeof native !== 'object') {
    return `${path || 'value'}: wasm ${JSON.stringify(wasm)} vs native ${JSON.stringify(native)}`;
  }
  const wasmIsArray = Array.isArray(wasm);
  if (wasmIsArray !== Array.isArray(native)) return `${path || 'value'}: array-ness differs`;
  if (wasmIsArray) {
    if (wasm.length !== native.length) return `${path || 'value'}: length ${wasm.length} vs ${native.length}`;
    for (let index = 0; index < wasm.length; index += 1) {
      const difference = replyDifference(wasm[index], native[index], `${path}[${index}]`, state);
      if (difference) return difference;
    }
    return null;
  }
  const keys = Object.keys(wasm);
  const nativeKeys = Object.keys(native);
  if (keys.join(',') !== nativeKeys.join(',')) {
    return `${path || 'value'}: keys ${keys.join(',')} vs ${nativeKeys.join(',')}`;
  }
  for (const key of keys) {
    const difference = replyDifference(wasm[key], native[key], path ? `${path}.${key}` : key, state);
    if (difference) return difference;
  }
  return null;
}

/** Compare one pair of replies: `{ differing, worstUsed, worstPath, difference }`. */
function compareReplies(wasm, native) {
  const state = { differing: 0, worstUsed: 0, worstPath: null, worstAbsolute: 0, worstRelative: 0 };
  if (wasm.ok !== native.ok) {
    return { ...state, difference: `ok ${wasm.ok} vs ${native.ok}` };
  }
  if (!wasm.ok) {
    // Refusals: the same kind and the same status. The binary prints the whole usage text
    // after a usage message, which is why only the message-free parts are compared.
    if (wasm.status !== native.status) {
      return { ...state, difference: `refusal status ${wasm.status} vs ${native.status}` };
    }
    if (wasm.error?.error !== native.error?.error) {
      return { ...state, difference: `refusal kind ${wasm.error?.error} vs ${native.error?.error}` };
    }
    return { ...state, difference: null };
  }
  const difference = replyDifference(wasm.value, native.value, '', state);
  return { ...state, difference };
}
function runEngine(args) {
  if (!WASM_ENGINE) {
    return nativeRun(args);
  }
  const reply = wasmEngine.call(args);
  const wasm = reply.ok
    ? { ok: true, value: reply.value }
    : { ok: false, status: reply.status, error: reply.error };
  if (existsSync(ENGINE)) {
    const native = nativeRun(args);
    const comparison = compareReplies(wasm, native);
    crossCheck.calls += 1;
    if (comparison.difference !== null) {
      crossCheck.beyond += 1;
      row({
        caseId: `ct-engine ${args[0]}`,
        quantity: 'wasm vs native reply',
        js: 'n/a',
        rust: comparison.difference,
        abs: null,
        rel: null,
        tolerance: null,
        pass: false,
        note: 'the wasm build and the binary must answer alike',
      });
    } else if (comparison.differing === 0) {
      crossCheck.identical += 1;
    } else {
      crossCheck.within += 1;
    }
    if (comparison.worstUsed > crossCheck.worstUsed) {
      crossCheck.worstUsed = comparison.worstUsed;
      crossCheck.worstCase = `ct-engine ${args[0]} ${comparison.worstPath ?? ''}`.trim();
      crossCheck.worstAbsolute = comparison.worstAbsolute;
      crossCheck.worstRelative = comparison.worstRelative;
    }
  }
  return wasm;
}

/* ---------------- case inputs ---------------- */

const vectors = JSON.parse(readFileSync(join(root, 'validation', 'test-vectors.json'), 'utf8'));
const ORDER_METRIC = {
  'least-over-capacity': 'capacityKgS',
  'lowest-electrical-input': 'electricalInputKW',
  'lowest-makeup-water': 'makeupKgS',
  'lowest-total-air-side-pressure': 'totalPa',
};

const SELECTION_PHYSICS_QUANTITIES = [
  'coldWaterC',
  'thermalMarginC',
  'rangeC',
  'approachC',
  'cpWaterKJkgK',
  'heatTransferKW',
  'electricalInputKW',
  'waterVolumetricFlowM3S',
  'totalPa',
  'fillPa',
  'driftPa',
  'inletPa',
  'distributionPa',
  'supportPa',
  'plenumPa',
  'fanStackPa',
  'fixedPa',
  'driftPpm',
  'waterLoadingKgM2S',
  'dryAirLoadingKgM2S',
  'dryAirMassFlowKgS',
  'availableMerkelNumber',
  'fillMerkelNumber',
  'sprayZoneMerkelNumber',
  'rainZoneMerkelNumber',
  'areas.fillAreaM2',
  'areas.airFreeAreaM2',
  'areas.driftAreaM2',
  'areas.inletAreaM2',
  'areas.plenumAreaM2',
  'areas.fanStackAreaM2',
  'fanFlowM3S',
  'fanPressurePa',
  'systemPressurePa',
  'efficiency',
  'shaftPowerKW',
  'evaporationKgS',
  'driftKgS',
  'blowdownKgS',
  'makeupKgS',
  'nozzleCount',
];


/** The candidate identity the recorded rows are keyed by: tower, fill, drift, fan, depth, speed. */
function selectionIdentity(candidate) {
  return `${candidate.towerId}|${candidate.fillId}|${candidate.driftId}|${candidate.fanId}|${candidate.fillDepthM}|${candidate.speedRatio}`;
}

/** Normalise one engine candidate to the flat record the recorded rows are keyed by. */
function selectionMetrics(candidate) {
  return {
    coldWaterC: candidate.coldWaterC,
    thermalMarginC: candidate.thermalMarginC,
    rangeC: candidate.rangeC,
    approachC: candidate.approachC,
    cpWaterKJkgK: candidate.cpWaterKJkgK,
    heatTransferKW: candidate.heatTransferKW,
    electricalInputKW: candidate.electricalInputKW,
    waterVolumetricFlowM3S: candidate.waterVolumetricFlowM3S,
    ...candidate.airside,
    fanFlowM3S: candidate.fanOperatingPoint.flowM3S,
    fanPressurePa: candidate.fanOperatingPoint.fanPressurePa,
    systemPressurePa: candidate.fanOperatingPoint.systemPressurePa,
    efficiency: candidate.fanOperatingPoint.efficiency,
    shaftPowerKW: candidate.fanOperatingPoint.shaftPowerKW,
    evaporationKgS: candidate.waterBalance.evaporationKgS,
    driftKgS: candidate.waterBalance.driftKgS,
    blowdownKgS: candidate.waterBalance.blowdownKgS,
    makeupKgS: candidate.waterBalance.makeupKgS,
    nozzleCount: candidate.nozzle.count,
  };
}

/** The reference-side thermal model of one candidate, at one water flow and KaV/L. */
/**
 * The documented order: the objective's own metric, then the fixed tie-break chain
 * (electrical input, make-up water, total air-side pressure, capability ratio, capacity),
 * then the candidate identity — recomputed here from the port's own reported metrics.
 */
function documentedOrder(candidates, objectiveName) {
  const metric = ORDER_METRIC[objectiveName];
  const identity = selectionIdentity;
  return candidates
    .map((candidate, index) => ({ candidate, index }))
    .sort((a, b) => (
      a.candidate[metric] - b.candidate[metric]
      || a.candidate.electricalInputKW - b.candidate.electricalInputKW
      || a.candidate.makeupKgS - b.candidate.makeupKgS
      || a.candidate.totalPa - b.candidate.totalPa
      || a.candidate.capabilityRatio - b.candidate.capabilityRatio
      || a.candidate.capacityKgS - b.candidate.capacityKgS
      || (identity(a.candidate) < identity(b.candidate) ? -1 : identity(a.candidate) > identity(b.candidate) ? 1 : 0)
      || a.index - b.index
    ))
    .map((entry) => identity(entry.candidate));
}


/** The worst-magnitude pair across two equal-shaped result arrays, for one key. */
function worstPair(jsRows, rustRows, key) {
  let worst = { js: undefined, rust: undefined, abs: -1 };
  const length = Math.min(jsRows.length, rustRows.length);
  for (let index = 0; index < length; index += 1) {
    const abs = Math.abs(jsRows[index][key] - rustRows[index][key]);
    if (abs > worst.abs) worst = { js: jsRows[index][key], rust: rustRows[index][key], abs };
  }
  return worst;
}

/** Run a reference call, capturing its refusal instead of aborting the harness with it. */
/** Resolve a dotted path with `[index]` steps: `a.b[0].c`. `[]` takes the entry's index. */
function pathValue(value, path, index = null) {
  let current = value;
  for (const step of String(path).split('.')) {
    const match = /^([^[\]]*)((?:\[\d*\])*)$/.exec(step);
    if (match === null) return undefined;
    const [, key, indexes] = match;
    if (key) {
      if (current === null || current === undefined) return undefined;
      current = current[key];
    }
    for (const token of indexes.match(/\[\d*\]/g) ?? []) {
      const position = token === '[]' ? index : Number(token.slice(1, -1));
      if (current === null || current === undefined || position === null || position === undefined) return undefined;
      current = current[position];
    }
  }
  return current;
}

/** Equal by value: numbers bit-for-bit, arrays element-wise, objects key-wise. */
/** The engine's own value for one recorded entry, re-read from its reply by the entry's rule. */
function extractActual(reply, extract, entry) {
  const index = extract.index ?? entry.index ?? null;
  const { how } = extract;
  if (how === 'path') return pathValue(reply, extract.path, index);
  if (how === 'path-or') {
    // The live row reads `reply.<path> ?? fallback` (a selection rejection count the reply
    // may simply not carry); the recorded replay must apply the same fallback.
    const value = pathValue(reply, extract.path, index);
    return value === undefined ? extract.fallback : value;
  }
  if (how === 'string-of') {
    const value = pathValue(reply, extract.path, index);
    return value === undefined ? undefined : String(value);
  }
  if (how === 'length') {
    const value = pathValue(reply, extract.path, index);
    return value === undefined ? undefined : value.length;
  }
  if (how === 'length-text') {
    const value = pathValue(reply, extract.path, index);
    return value === undefined ? undefined : String(value.length);
  }
  if (how === 'join') {
    const value = pathValue(reply, extract.path, index) ?? [];
    return value.map((item) => String(item)).join(extract.separator);
  }
  if (how === 'keys-join') {
    return Object.keys(pathValue(reply, extract.path, index) ?? {}).join(extract.separator);
  }
  if (how === 'nozzle-options') {
    const options = pathValue(reply, extract.path, index) ?? [];
    return options.map((option) => `${option.nozzleId}:${option.count}`).join(',');
  }
  if (how === 'worst-pair') {
    // The recorded JavaScript series and the engine's own, compared at the worst-deviation
    // pair of the two — the same pair (and the same rule) the live row compares.
    const rows = pathValue(reply, extract.path, index) ?? [];
    const witness = extract.witness ?? [];
    const worst = worstPair(witness, rows, extract.key);
    return worst.rust;
  }
  if (how === 'series') {
    return (pathValue(reply, extract.path, index) ?? []).map((row) => row[extract.key]);
  }
  if (how === 'candidate-field' || how === 'candidate-metrics') {
    const candidate = (pathValue(reply, 'results', null) ?? [])
      .find((item) => selectionIdentity(item) === extract.identity);
    if (candidate === undefined) return undefined;
    if (how === 'candidate-field') return pathValue(candidate, extract.path, null);
    return selectionMetrics(candidate)[extract.quantity];
  }
  if (how === 'candidate-nozzle') {
    const candidate = (pathValue(reply, 'results', null) ?? [])
      .find((item) => selectionIdentity(item) === extract.identity);
    return candidate === undefined ? undefined : `${candidate.nozzle.nozzleId}:${candidate.nozzle.nozzleName}`;
  }
  if (how === 'identity-match') {
    const known = new Set(pathValue(reply, 'results', null)?.map(selectionIdentity) ?? []);
    return (extract.identities ?? []).filter((identity) => known.has(identity)).length;
  }
  if (how === 'identity-missing') {
    const known = new Set(pathValue(reply, 'results', null)?.map(selectionIdentity) ?? []);
    return (extract.identities ?? []).filter((identity) => !known.has(identity)).length;
  }
  throw new Error(`the baseline names an extraction the harness does not implement: ${how}`);
}

/** Reply-only checks: the recorded entry re-runs an invariant of the answer itself. */
const BASELINE_PREDICATES = {
  'documented-order': (entry, reply) => {
    const objective = entry.predicate.objective;
    const expected = documentedOrder(reply.candidates ?? [], objective);
    const actual = reply.orders?.[objective] ?? [];
    return {
      pass: JSON.stringify(expected) === JSON.stringify(actual),
      js: `${expected.length} candidates`,
      rust: `${actual.length} candidates`,
      note: 'the documented objective + tie-break order',
    };
  },
  'margin-floor': (entry, reply) => {
    const objective = entry.predicate.objective;
    const minimum = reply.requirements?.minimumThermalMarginC;
    const candidates = reply.candidates ?? [];
    const holds = (reply.orders?.[objective] ?? []).every((identity) => {
      const candidate = candidates.find((item) => selectionIdentity(item) === identity);
      return candidate !== undefined && candidate.thermalMarginC >= minimum;
    });
    return {
      pass: holds,
      js: `>= ${minimum}`,
      rust: holds ? `>= ${minimum}` : 'a candidate below the floor ranked',
      note: 'duty and margin are constraints, never sort keys',
    };
  },
  'candidate-parity': (entry, reply) => {
    const known = new Set(entry.predicate.identities ?? []);
    const candidates = reply.results ?? [];
    const matched = candidates.filter((item) => known.has(selectionIdentity(item))).length;
    const missing = candidates.length - matched;
    return {
      pass: matched === entry.predicate.count && missing === 0,
      js: `${entry.predicate.count} candidate identities`,
      rust: `${matched} matched, ${missing} unmatched`,
      note: `${matched * SELECTION_PHYSICS_QUANTITIES.length} field comparisons + ${matched * 2} recomputed capacity/capability-ratio comparisons`,
    };
  },
  'ordering-difference': (entry, reply) => {
    const known = new Set(entry.predicate.identities ?? []);
    const candidates = reply.results ?? [];
    const matched = candidates.filter((item) => known.has(selectionIdentity(item))).length;
    const missing = candidates.length - matched;
    const portLeader = candidates.length > 0 ? selectionIdentity(candidates[0]) : 'none';
    return {
      pass: entry.predicate.leader !== portLeader && missing === 0 && matched === entry.predicate.count,
      js: `cost ranking leads with ${entry.predicate.leader}`,
      rust: `objective ranking leads with ${portLeader}`,
      note: 'physics parity holds for every matched identity; the port reads no economics field',
    };
  },
};
/* ---- replay: the recorded-baseline mode ---- */

/** `--reference frozen`: check the engine under test against the recorded regression baseline alone. */
function baselineParity() {
  if (!existsSync(BASELINE_PATH)) {
    console.error(`${BASELINE_PATH} does not exist; it is the recorded regression baseline this mode checks against.`);
    console.error('It is recorded evidence from the JavaScript reference, which this tree no longer holds (issue #40 slice 4).');
    process.exit(2);
  }
  const baseline = JSON.parse(readFileSync(BASELINE_PATH, 'utf8'));
  console.log(`  baseline:  ${BASELINE_PATH} (format ${baseline.format})`);
  console.log(`  recorded:  commit ${baseline.provenance.capturedAtCommit}, ${baseline.counts.quantities} entries`);
  console.log('  live leg:  RETIRED — the JavaScript reference is no longer in this tree (issue #40 slice 4); no live comparison ran, and none can be skipped into a pass');
  console.log('');
  const recordedSkips = (baseline.skips ?? []).map((entry) => `${entry.caseId} / ${entry.quantity}`);
  for (const caseId of new Set(recordedSkips)) {
    row({
      caseId,
      quantity: 'recorded skip (the reference decided there was nothing to compare)',
      js: 'skip',
      rust: 'skip',
      abs: null,
      rel: null,
      tolerance: null,
      pass: null,
      verdict: 'SKIP',
      note: 'recorded in the baseline, not an engine assertion',
    });
  }
  let currentTitle = null;
  for (const step of baseline.steps) {
    if (step.section !== currentTitle) {
      section(step.section);
      currentTitle = step.section;
    }
    const engine = runEngine(step.args);
    if (step.expectation === 'refusal') {
      for (const entry of step.entries) {
        const refused = engine.ok !== true
          && engine.status === entry.refusal?.status
          && engine.error?.error === entry.refusal?.error;
        row({
          caseId: entry.caseId,
          quantity: entry.quantity,
          js: entry.value,
          rust: engine.ok ? `returned a value` : `refused: ${engine.error?.error} (exit ${engine.status})`,
          abs: null,
          rel: null,
          tolerance: null,
          pass: refused,
          note: 'both must refuse (the recorded baseline: the reference refused at capture)',
        });
      }
      continue;
    }
    if (engine.ok !== true) {
      row({
        caseId: step.caseId,
        quantity: '*',
        js: 'expected an answer',
        rust: `refused: ${engine.error?.message ?? engine.error?.error} (exit ${engine.status})`,
        abs: null,
        rel: null,
        tolerance: null,
        pass: false,
        note: 'unexpected refusal against the recorded regression baseline',
      });
      continue;
    }
    for (const entry of step.entries) {
      if (entry.kind === 'predicate') {
        const predicate = BASELINE_PREDICATES[entry.predicate?.how];
        if (predicate === undefined) {
          console.error(`the baseline names a predicate the harness does not replay: ${entry.predicate?.how} (${entry.caseId} / ${entry.quantity})`);
          process.exit(2);
        }
        const outcome = predicate(entry, engine.value);
        row({
          caseId: entry.caseId,
          quantity: entry.quantity,
          js: outcome.js,
          rust: outcome.rust,
          abs: null,
          rel: null,
          tolerance: null,
          pass: outcome.pass,
          note: outcome.note,
        });
        continue;
      }
      if (entry.kind === 'skip') {
        row({
          caseId: entry.caseId,
          quantity: entry.quantity,
          js: entry.value,
          rust: 'skip',
          abs: null,
          rel: null,
          tolerance: null,
          pass: null,
          verdict: 'SKIP',
          note: entry.note,
        });
        continue;
      }
      if (entry.kind === 'series') {
        // The recorded JavaScript ordinates against the engine's own: the SAME tolerance the
        // live row carries, applied to every point rather than to the single worst-deviation
        // pair, so a moved worst index cannot compare two different points. The row displays
        // the pair that uses up the most of its tolerance.
        const rows = pathValue(engine.value, entry.extract.path) ?? [];
        const expected = entry.value ?? [];
        const tolerance = entry.tolerance ?? { abs: 0, rel: 0 };
        let worst = null;
        let failed = false;
        for (let index = 0; index < Math.max(expected.length, rows.length); index += 1) {
          const js = expected[index];
          const rust = rows[index]?.[entry.extract.key];
          const abs = Math.abs(js - rust);
          const rel = js === 0 ? Infinity : abs / Math.abs(js);
          const used = Math.min(
            tolerance.abs === 0 ? (abs === 0 ? 0 : Infinity) : abs / tolerance.abs,
            tolerance.rel === 0 ? (rel === 0 ? 0 : Infinity) : rel / tolerance.rel,
          );
          if (!(used <= 1)) failed = true;
          if (worst === null || used > worst.used) worst = { js, rust, abs, rel, used };
        }
        row({
          caseId: entry.caseId,
          quantity: entry.quantity,
          js: worst?.js,
          rust: worst?.rust,
          abs: worst?.abs,
          rel: worst?.rel,
          tolerance: entry.tolerance,
          pass: !failed,
          note: `every one of the ${expected.length} recorded ordinates`,
        });
        continue;
      }
      let actual;
      try {
        actual = extractActual(engine.value, entry.extract, entry);
      } catch (error) {
        console.error(`the baseline cannot be replayed: ${error.message} (${entry.caseId} / ${entry.quantity})`);
        process.exit(2);
      }
      if (entry.kind === 'number') {
        compareQuantity(entry.caseId, entry.quantity, entry.value, actual, entry.tolerance);
      } else if (entry.kind === 'text') {
        compareText(entry.caseId, entry.quantity, entry.value, actual, entry.note);
      } else if (entry.kind === 'nullish') {
        const expected = entry.value === 'null' ? null : undefined;
        compareQuantity(entry.caseId, entry.quantity, expected, actual, { abs: 0, rel: 0 });
      } else {
        console.error(`the baseline names an entry kind the harness does not replay: ${entry.kind} (${entry.caseId} / ${entry.quantity})`);
        process.exit(2);
      }
    }
  }
  return baseline;
}
/* ---------------- main ---------------- */

function cargo(args) {
  const result = spawnSync('cargo', args, { cwd: root, stdio: ['ignore', 'inherit', 'inherit'] });
  if (result.error) {
    console.error(`cargo could not run: ${result.error.message}`);
    process.exit(2);
  }
  if (result.status !== 0) {
    console.error(`cargo ${args.join(' ')} failed (exit ${result.status})`);
    process.exit(2);
  }
}

function build() {
  if (NO_BUILD) {
    const required = WASM_ENGINE ? [WASM_ARTIFACT] : [ENGINE];
    for (const artifact of required) {
      if (!existsSync(artifact)) {
        console.error(`--no-build given but ${artifact} does not exist; run without --no-build.`);
        process.exit(2);
      }
    }
    return;
  }
  cargo(['build', '--manifest-path', CRATE_MANIFEST]);
  if (WASM_ENGINE) {
    // The recorded wasm build command (`rust/README.md`, `docs/WASM_ENGINE.md`), the same
    // one the hosted workflow runs.
    cargo(['build', '--manifest-path', CRATE_MANIFEST, '--target', 'wasm32-unknown-unknown', '--profile', 'wasm', '--lib']);
  }
}
function main() {
  build();

  const nodeVersion = spawnSync('node', ['--version'], { encoding: 'utf8' }).stdout.trim();
  const rustVersion = spawnSync('rustc', ['--version'], { encoding: 'utf8' }).stdout.trim();
  console.log('parity harness — recorded regression baseline vs Rust port');
  console.log(`  cases:    ${root}`);
  if (WASM_ENGINE) {
    wasmEngine = createEngine(readFileSync(WASM_ARTIFACT));
    console.log(`  engine:   wasm — ${WASM_ARTIFACT}`);
    console.log('  binding:  rust/wasm/binding.mjs (data only)');
    console.log(`  cross-check: ${existsSync(ENGINE) ? ENGINE : 'native binary absent — wasm answers are not cross-checked'}`);
  } else {
    console.log(`  engine:   ${ENGINE}`);
  }
  console.log(`  node:     ${nodeVersion}`);
  console.log(`  rustc:    ${rustVersion || 'unknown'}`);
  console.log(`  vectors:  validation/test-vectors.json — ${vectors.status}`);
  console.log('  rule:     pass when |Δ| <= abs OR |Δ|/|baseline| <= rel');
  console.log('  note:     values are printed to 9 significant digits; the diff uses full precision');
  console.log('  baseline: the recorded regression baseline — the only comparison partner');
  console.log('  live leg: RETIRED — the JavaScript reference is no longer in this tree (issue #40 slice 4);');
  console.log('            no live comparison ran, and none can be skipped into a pass');
  console.log('  tolerances:');
  for (const [quantity, tolerance] of Object.entries(TOLERANCES)) {
    console.log(`    ${pad(quantity, 34)} ${formatTolerance(tolerance)}`);
  }

  if (WASM_ENGINE) {
    console.log('');
    console.log('='.repeat(100));
    console.log('WASM BINDING SECTION: NOT COMPARED — its rows diffed rust/wasm/binding.mjs\'s data-only');
    console.log('surface against the JavaScript reference, a JavaScript surface with no engine reply, so');
    console.log('they have no recorded counterpart. The binding itself is exercised by the wasm leg above');
    console.log('(every reply it produces is cross-checked against the native binary) and by the pinned');
    console.log('bytes `npm run bundle-check` re-derives and `npm run deploy-check` records.');
    console.log('='.repeat(100));
  }

  const baseline = baselineParity();
  const recorded = baseline.counts.quantities;
  const compared = totals.passed + totals.failed;
  if (recorded !== compared) {
    const detail = `the baseline records ${recorded} engine quantities but this run compared ${compared}`;
    if (failures.length === 0) {
      console.error(detail);
      process.exit(2);
    }
    console.log(`note: ${detail} — a step-level refusal replaced its per-quantity rows (see the failures below)`);
  }

  console.log('');
  console.log(`compared ${totals.passed + totals.failed} quantities: ${totals.passed} pass, ${totals.failed} fail`);
  console.log('baseline: the recorded regression baseline (validation/regression-baseline.json) — the only comparison partner; the live JavaScript leg is RETIRED');
  if (WASM_ENGINE) {
    console.log(`wasm vs native: ${crossCheck.calls} replies compared — ${crossCheck.identical} identical to the last bit, ${crossCheck.within} alike within ${CROSS_CHECK_TOLERANCE.rel}, ${crossCheck.beyond} beyond`);
    if (crossCheck.calls > 0) {
      console.log(`  worst cross-build deviation: abs ${crossCheck.worstAbsolute} / rel ${crossCheck.worstRelative} — ${crossCheck.worstUsed} of the ${CROSS_CHECK_TOLERANCE.rel} tolerance (${crossCheck.worstCase})`);
    }
    if (crossCheck.calls === 0) {
      console.log('  (no native binary beside the artifact, so the cross-check did not run)');
    }
  }
  if (failures.length) {
    console.log('');
    console.log('out of tolerance:');
    for (const failure of failures) {
      console.log(
        `  [${failure.section}] ${failure.caseId} / ${failure.quantity}:`
        + ` baseline=${formatValue(failure.js)} engine=${formatValue(failure.rust)}`
        + ` abs=${failure.abs === null ? '-' : failure.abs} rel=${failure.rel === null ? '-' : failure.rel}`
        + `${failure.tolerance ? ` tol=${formatTolerance(failure.tolerance)}` : ''}`
        + `${failure.note ? ` (${failure.note})` : ''}`,
      );
    }
    process.exit(1);
  }
  console.log('every quantity is within tolerance');
}
main();
