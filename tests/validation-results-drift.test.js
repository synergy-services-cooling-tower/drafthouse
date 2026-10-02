import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { sampleCatalog } from '../src/data/sampleCatalog.js';

/**
 * Drift gate for the numbers `VALIDATION_RESULTS.md` states.
 *
 * Why it exists: `VALIDATION_RESULTS.md` claimed 51 tests when the suite had grown to 144, and it
 * described a chain that had since grown four legs. Documented figures rot silently unless
 * something reads them back. This file is that something.
 *
 * What it checks, and how it is kept honest:
 *
 *  - **the test count** is read out of the file's own quoted reporter output and compared with a
 *    real `node --test tests/*.test.js` run — the same invocation `npm test` uses, run as a child
 *    process. Nothing here writes the document: the gate fails and says which two numbers disagree
 *    and how to re-derive them, so a stale figure is fixed deliberately, never patched by a script.
 *  - **the chain** is read out of the file's `Chain:` line and compared with the script list in
 *    `package.json`'s `validate`, so a new leg cannot be added while the document keeps describing
 *    the old chain.
 *  - **the environment figures that cannot drift** — prototype version (`package.json`) and catalog
 *    revision (`src/data/sampleCatalog.js`) — are compared with their sources of truth.
 *  - **the smoke figures** are read out of the file's JSON block and compared with a live
 *    `npm run smoke`, at 1e-9 relative tolerance, so a physics change that moves a documented number
 *    is caught while a last-ulp noise is not.
 *
 * What it deliberately does NOT check: the node version and the recording date. Those describe the
 * environment the recorded run happened in, not a claim about the current one, and gating them
 * would make the gate fail on a host change for the wrong reason (CI runs a pinned node older than
 * a developer's). They are recorded in the document as environment, not as an assertion.
 *
 * The child run skips this file's count check (env-guarded, and the guard itself is asserted), so
 * the counting run terminates and its total matches the parent's — a skipped test still counts in
 * node's `tests N` total, which is what makes the comparison exact rather than approximate.
 */

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const COUNT_DEPTH = Number(process.env.CT_VALIDATION_COUNT_DEPTH ?? '0');
const inCountingChild = COUNT_DEPTH >= 1;

const document = readFileSync(join(root, 'VALIDATION_RESULTS.md'), 'utf8');

function documented(regex, label) {
  const match = regex.exec(document);
  assert.ok(match, `VALIDATION_RESULTS.md no longer contains the ${label} this gate reads`);
  return match[1].trim();
}

function run(command, args) {
  // `node --test` refuses to run a second test runner from inside a test process unless the
  // `NODE_TEST_CONTEXT` marker it set for this file is removed — the recursion guard below is ours,
  // not the runner's. Everything else in the environment is inherited unchanged.
  const env = { ...process.env, CT_VALIDATION_COUNT_DEPTH: String(COUNT_DEPTH + 1) };
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    timeout: 600_000,
    maxBuffer: 32 * 1024 * 1024,
    env
  });
  assert.ok(!result.error, `${command} ${args.join(' ')} failed to run: ${result.error?.message}`);
  return result;
}

/**
 * Node's test runner prints its summary in the spec format ("ℹ tests 144") on a terminal and in
 * TAP ("# tests 144") otherwise, so match either by stripping the leading decoration.
 */
function summaryValue(output, name) {
  const lines = output.split('\n')
    .map((line) => line.replace(/^[^\w]+/, '').trim())
    .filter((line) => new RegExp(`^${name}\\s+\\d+$`).test(line));
  if (lines.length === 0) return null;
  return Number(lines[lines.length - 1].split(/\s+/)[1]);
}

test('the documented test count equals what node --test tests/*.test.js actually runs', { skip: inCountingChild }, () => {
  const documentedCount = Number(documented(/^\W*tests\s+(\d+)\W*$/m, 'quoted test count'));

  // The same file set `npm test` runs (`node --test tests/*.test.js`).
  const files = readdirSync(join(root, 'tests'))
    .filter((name) => name.endsWith('.test.js'))
    .sort()
    .map((name) => join('tests', name));
  assert.ok(files.length > 0, 'no test files found');

  const runResult = run(process.execPath, ['--test', ...files]);
  const summary = runResult.stdout;
  const actualCount = summaryValue(summary, 'tests');
  assert.notEqual(actualCount, null, `the child suite printed no summary; tail:\n${summary.slice(-2000)}`);

  assert.equal(
    runResult.status,
    0,
    `the child suite failed (exit ${runResult.status}); tail:\n${summary.slice(-2000)}`
  );
  // The guard must have held: in the counting child exactly this one test is skipped. If a
  // recursion ever happened, this count would explode or the totals would not line up.
  assert.equal(
    summaryValue(summary, 'skipped'),
    1,
    `the counting child skipped ${summaryValue(summary, 'skipped')} tests, not exactly this gate; tail:\n${summary.slice(-2000)}`
  );

  assert.equal(
    actualCount,
    documentedCount,
    `VALIDATION_RESULTS.md says ${documentedCount} tests; \`npm test\` runs ${actualCount}. `
      + 'Re-derive with `npm test` and update the quoted block and this figure in VALIDATION_RESULTS.md.'
  );
});

test('the documented chain matches package.json validate', () => {
  const documentedChain = documented(/^Chain \(from `package\.json`\):\s*(.+)$/m, 'chain line')
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
  const packageJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  const actualChain = packageJson.scripts.validate.split('&&')
    .map((step) => step.trim().replace(/^npm (?:run )?/, ''))
    .filter(Boolean);
  assert.deepEqual(
    documentedChain,
    actualChain,
    'the documented chain no longer matches `npm run validate` in package.json'
  );
});

test('the documented prototype version and catalog revision are current', () => {
  const packageJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  assert.equal(
    documented(/\*\*Prototype version:\*\*\s*(\S+)/, 'prototype version'),
    packageJson.version,
    'the documented prototype version is not package.json version'
  );
  assert.equal(
    documented(/\*\*Catalog revision:\*\*\s*`([^`]+)`/, 'catalog revision'),
    sampleCatalog.metadata.id,
    'the documented catalog revision is not the bundled catalog revision'
  );
});

test('the documented smoke figures are what `node scripts/smoke.mjs` prints', () => {
  const quoted = JSON.parse(/```json\n([\s\S]*?)\n```/.exec(document)[1]);
  const result = run(process.execPath, [join('scripts', 'smoke.mjs')]);
  assert.equal(result.status, 0, `smoke run exited ${result.status}; tail:\n${result.stdout.slice(-2000)}`);
  const live = JSON.parse(result.stdout);

  const TOLERANCE = 1e-9;
  const compared = [];
  const compare = (path) => {
    const expected = path.split('.').reduce((value, key) => value?.[key], quoted);
    const actual = path.split('.').reduce((value, key) => value?.[key], live);
    if (typeof expected === 'number' && Number.isFinite(expected)) {
      const scale = Math.max(1, Math.abs(expected));
      assert.ok(
        Math.abs(actual - expected) / scale < TOLERANCE,
        `VALIDATION_RESULTS.md quotes ${path} = ${expected}; the smoke run now prints ${actual}. `
          + 'Re-run `npm run smoke` and update the JSON block in VALIDATION_RESULTS.md.'
      );
    } else {
      assert.equal(actual, expected, `VALIDATION_RESULTS.md quotes ${path} = ${expected}; the smoke run now prints ${actual}`);
    }
    compared.push(path);
  };
  for (const path of [
    'demandMerkel',
    'recoveredColdWaterC',
    'identicalPointCapabilityPct',
    'feasibleSelections',
    'topSelection.tower',
    'topSelection.fill',
    'topSelection.drift',
    'topSelection.fan',
    'topSelection.coldWaterC',
    'topSelection.electricalInputKW'
  ]) {
    compare(path);
  }
  // Every leaf the document quotes must be covered above: adding a figure to the JSON block
  // without extending this list would otherwise leave it unguarded.
  const leaves = (value, prefix = '') => Object.entries(value).flatMap(([key, entry]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return entry !== null && typeof entry === 'object' ? leaves(entry, path) : [path];
  });
  const uncovered = leaves(quoted).filter((path) => !compared.includes(path));
  assert.deepEqual(uncovered, [], `the document quotes ${uncovered.join(', ')} but the gate does not compare it`);
});
