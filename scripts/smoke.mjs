/**
 * Deterministic smoke case — the documented duty, computed by the PINNED engine piece.
 *
 * This ran through the JavaScript reference until issue #40 slice 4; it now runs the same duty
 * through the engine artifact the pin records (`bundles/engine.manifest.json`: the wasm build,
 * produced by the recorded `cargo build … --profile wasm` command, plus its thin binding) and
 * the catalog `src/data/sampleCatalog.js` carries — so the numbers printed here are the engine's
 * own, read from its reply, with no arithmetic of this script's own. A served copy of the same
 * bytes went with the retired JavaScript surface (issue #60, D22).
 *
 * `VALIDATION_RESULTS.md` quotes this run's JSON block and `tests/validation-results-drift.test.js`
 * compares the two decimals (1e-9 relative), so a pin change or a physics change that moves a
 * documented number is caught rather than silently kept. Nothing here is a claim about the
 * physics: the engine is ported and drift-guarded (see docs/VALIDATION.md).
 */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { createEngine } from '../rust/wasm/binding.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The pin records the artifact and the command that produces it; the smoke reads what it built. */
const pin = JSON.parse(readFileSync(join(root, 'bundles', 'engine.manifest.json'), 'utf8'));
const wasm = (pin.files ?? []).find((file) => file.path.endsWith('.wasm'));
if (!wasm || typeof wasm.command !== 'string') {
  throw new Error('bundles/engine.manifest.json does not record the wasm artifact and its build command — run: npm run bundle');
}
const build = spawnSync(wasm.command, { shell: true, cwd: root, stdio: ['ignore', 'inherit', 'inherit'] });
if (build.error || build.status !== 0) {
  throw new Error(`the recorded wasm build command failed (${build.error ? build.error.message : `exit ${build.status}`}); the smoke needs the artifact the pin records`);
}
const engine = createEngine(readFileSync(join(root, wasm.path)));
const catalog = (await import(pathToFileURL(join(root, 'src', 'data', 'sampleCatalog.js')).href)).sampleCatalog;

const condition = {
  waterMassFlowKgS: 200,
  dryAirMassFlowKgS: 133.3333333,
  hotWaterC: 42,
  coldWaterC: 32,
  wetBulbC: 27,
  dryBulbC: 33,
  pressurePa: 101325
};
const waterToDryAirRatio = condition.waterMassFlowKgS / condition.dryAirMassFlowKgS;

/** One engine command; a refusal is a loud failure — the smoke has no result to print. */
function call(args) {
  const reply = engine.call(args);
  if (!reply.ok) {
    throw new Error(`the engine refused ${args[0]} (status ${reply.status}): ${reply.error.message}`);
  }
  return reply.value;
}

/** The `capability` command's condition spec, in the engine's own field names. */
const conditionSpec = (value) => ['waterMassFlowKgS', 'dryAirMassFlowKgS', 'hotWaterC', 'coldWaterC', 'wetBulbC', 'dryBulbC', 'pressurePa']
  .map((field) => `${field}:${value[field]}`)
  .join(',');

const demand = call([
  'merkel',
  '--hot', String(condition.hotWaterC),
  '--cold', String(condition.coldWaterC),
  '--wb', String(condition.wetBulbC),
  '--db', String(condition.dryBulbC),
  '--lg', String(waterToDryAirRatio),
  '--p', String(condition.pressurePa),
  '--integration', 'simpson',
  '--convention', 'bulk'
]);
const predicted = call([
  'inverse',
  '--hot', String(condition.hotWaterC),
  '--wb', String(condition.wetBulbC),
  '--db', String(condition.dryBulbC),
  '--lg', String(waterToDryAirRatio),
  '--p', String(condition.pressurePa),
  '--kavl', String(demand.merkelNumber),
  '--integration', 'simpson',
  '--convention', 'bulk'
]);
const capability = call([
  'capability',
  '--design', conditionSpec(condition),
  '--test', conditionSpec(condition)
]);
const selection = engine.select({ catalog, requirements: {}, maxResults: 3 });
if (!selection.ok) {
  throw new Error(`the engine refused the documented selection (status ${selection.status}): ${selection.error.message}`);
}
const top = selection.candidates[0];

console.log(JSON.stringify({
  demandMerkel: demand.merkelNumber,
  recoveredColdWaterC: predicted.coldWaterC,
  identicalPointCapabilityPct: capability.capabilityPct,
  feasibleSelections: selection.feasibleCandidateCount,
  topSelection: top ? {
    tower: top.towerId,
    fill: top.fillId,
    drift: top.driftId,
    fan: top.fanId,
    coldWaterC: top.coldWaterC,
    electricalInputKW: top.electricalInputKW
  } : null
}, null, 2));
