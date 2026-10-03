/**
 * Regression tests for the money gate (issue #83's extension of issue #1).
 *
 * Each case builds a throwaway fixture tree (the gate's script, the deployment record, the plane's
 * document, the app's sources and shipped assets) in the system temp directory, mutates it, and
 * runs the check as a subprocess so the assertions see the real exit status and the real
 * stderr/stdout — the same way `npm test` does and the same pattern `tests/retirement-check.test.js`
 * uses for the retirement gate.
 *
 * The gate's axes, and what a green run must never mean:
 *   1. the key pass — a money-ish token as (part of) a field or key name (`currency:`,
 *      `"bridge_price":`, `pub lifecycle_cost: f64`) is red, naming the file, the line and the
 *      token. This is the axis `deploytest-serve.mjs`'s wall-clock `cost: { wallClockMs }` shows
 *      exists: a token as a field name must be caught.
 *   2. the literal pass — `price`, `currency`, `capex`, `discount`, `lifecycle`, `usd`, `thb`,
 *      `penalty` as raw text are red wherever they sit;
 *   3. the deliberate exclusion — `cost` and `money` in the app's own prose ("no money field",
 *      "the measured cost of one real run") are NOT red: the rule's own attestation must not trip
 *      the rule. A case pins that, so the exclusion cannot rot into a hole in silence.
 * A missing deployment record or a missing source directory is a red, not a skip: fail-closed, the
 * way the retirement gate's pattern (D17) requires.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CHECK = 'cockpit/tools/currency-check.mjs';
const FIXTURE_PATHS = [
  CHECK,
  'deploy-manifest.txt',
  'cockpit/index.html',
  'cockpit/assets',
  'cockpit/src',
  'cockpit/contract',
  'cockpit/seams'
];

function copyTree(source, destination) {
  if (statSync(source).isDirectory()) {
    mkdirSync(destination, { recursive: true });
    for (const name of readdirSync(source)) copyTree(join(source, name), join(destination, name));
  } else {
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(source, destination);
  }
}

/** A fixture tree with everything the gate reads. */
function fixture(mutate) {
  const directory = mkdtempSync(join(tmpdir(), 'currency-check-'));
  for (const path of FIXTURE_PATHS) copyTree(join(root, path), join(directory, path));
  if (mutate) mutate(directory);
  return directory;
}

function runCheck(directory) {
  return spawnSync(process.execPath, [CHECK], { cwd: directory, encoding: 'utf8' });
}

function cleanup(t, directory) {
  t.after(() => rmSync(directory, { recursive: true, force: true }));
}

/** The failures are JSON on stderr; read them back so an assertion names what it found. */
function failuresOf(run) {
  try {
    return JSON.parse(run.stderr).failures ?? [];
  } catch {
    return [`stderr is not JSON: ${run.stderr}`];
  }
}

test('control: the delivered tree has no money field or currency token', (t) => {
  const directory = fixture();
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 0, run.stderr);
  const report = JSON.parse(run.stdout);
  assert.equal(report.passed, true);
  assert.ok(report.scanned > 0, 'the gate must scan something');
  assert.ok(report.files.includes('cockpit/index.html'), 'the plane document is in the scan set');
  assert.ok(
    report.files.some((file) => file.startsWith('cockpit/src/') && file.endsWith('.rs')),
    'the app\'s own sources are in the scan set'
  );
  assert.ok(
    report.skipped.some((file) => file.endsWith('.ttf')),
    'binary skips are reported, never silent'
  );
});

test('a planted currency key in a shipped asset is red, naming the file and the token', (t) => {
  const directory = fixture((dir) => {
    const path = join(dir, 'cockpit', 'assets', 'fixture.json');
    const fixtureJson = JSON.parse(readFileSync(path, 'utf8'));
    fixtureJson.catalog.currency = 'THB';
    writeFileSync(path, `${JSON.stringify(fixtureJson, null, 1)}\n`);
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some((failure) => failure.startsWith('cockpit/assets/fixture.json:') && failure.includes('currency')),
    JSON.stringify(failures)
  );
});

test('a planted currency string in the plane document is red', (t) => {
  const directory = fixture((dir) => {
    const path = join(dir, 'cockpit', 'index.html');
    writeFileSync(path, `${readFileSync(path, 'utf8')}\n<div data-note="USD 1,000"></div>\n`);
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some((failure) => failure.startsWith('cockpit/index.html:') && failure.includes('(usd)')),
    JSON.stringify(failures)
  );
});

test('a planted field in the app\'s own sources is red, naming the declaration', (t) => {
  const directory = fixture((dir) => {
    writeFileSync(
      join(dir, 'cockpit', 'src', 'screens', 'planted.rs'),
      'pub struct Quote {\n    pub lifecycle_cost: f64,\n}\n'
    );
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some(
      (failure) => failure.startsWith('cockpit/src/screens/planted.rs:') && failure.includes('field or key')
    ),
    JSON.stringify(failures)
  );
});

test('a planted reference in the built module is red when the module has been built', (t) => {
  const directory = fixture((dir) => {
    mkdirSync(join(dir, 'cockpit', 'pkg'), { recursive: true });
    writeFileSync(join(dir, 'cockpit', 'pkg', 'app.js'), 'export const opts = { "bridge_price": 3 };\n');
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some((failure) => failure.startsWith('cockpit/pkg/app.js:') && failure.includes('price')),
    JSON.stringify(failures)
  );
});

test('the app\'s own prose about the rule is not a false positive', (t) => {
  const directory = fixture((dir) => {
    writeFileSync(
      join(dir, 'cockpit', 'src', 'screens', 'attest.rs'),
      [
        '// no money field, and no further commercial surface anywhere in this contract.',
        '// the measured cost of one real run is 317 ms; that is the budget, not a field.',
        'pub fn attest() -> &\'static str { "no money field" }',
        ''
      ].join('\n')
    );
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 0, run.stderr);
});

test('the prose exemption stops at cost and money: other tokens stay literal', (t) => {
  const directory = fixture((dir) => {
    writeFileSync(
      join(dir, 'cockpit', 'src', 'screens', 'prose.rs'),
      '// a discount is not a field name either, so this line is a real violation.\n'
    );
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some(
      (failure) => failure.startsWith('cockpit/src/screens/prose.rs:') && failure.includes('(discount)')
    ),
    JSON.stringify(failures)
  );
});

test('a missing deployment record is a red, not a skip', (t) => {
  const directory = fixture((dir) => rmSync(join(dir, 'deploy-manifest.txt')));
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some((failure) => failure.includes('deploy-manifest.txt is missing')),
    JSON.stringify(failures)
  );
});

test('a missing source directory is a red, not a skip', (t) => {
  const directory = fixture((dir) => rmSync(join(dir, 'cockpit', 'src'), { recursive: true, force: true }));
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(
    failures.some((failure) => failure.includes('cockpit/src is missing')),
    JSON.stringify(failures)
  );
});
