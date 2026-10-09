/**
 * Regression tests for the retirement gate (issue #60).
 *
 * Each case builds a throwaway fixture tree (the gate's script, the deployment record, the
 * plane's document and assets, the pins) in the system temp directory, mutates it, and runs the
 * check as a subprocess so the assertions see the real exit status and the real stderr/stdout —
 * the same way `npm run validate` invokes it.
 *
 * The gate's two axes, and what a green run must never mean:
 *   1. the retired surface is absent — re-adding `showcase/` (a stub is enough) must go red
 *      naming it, or the "retired" surface could quietly come back;
 *   2. no artifact the deployment ships references `showcase.js` — a reference in the plane's
 *      document, in a file the deployment record lists, or in a published piece's own bytes
 *      must go red naming the file and the line. The recorded fixture's provenance citation of
 *      the retired path is deliberately not a match (documented in the script).
 * A missing deployment record is a red, not a skip: fail-closed, the way the retirement gate's
 * pattern (D17) requires.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CHECK = 'scripts/retirement-check.mjs';
const FIXTURE_PATHS = [CHECK, 'deploy-manifest.txt', 'cockpit/index.html', 'cockpit/assets', 'bundles'];

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
  const directory = mkdtempSync(join(tmpdir(), 'retirement-check-'));
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

test('control: the delivered tree is retired and unreferenced', (t) => {
  const directory = fixture();
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 0, run.stderr);
  const report = JSON.parse(run.stdout);
  assert.equal(report.passed, true);
  assert.equal(report.surfaceAbsent, 'showcase');
  assert.ok(report.scanned > 0, 'the gate must scan something');
  assert.ok(report.files.includes('cockpit/index.html'), 'the plane document is in the scan set');
});

test('a stub showcase/ directory is red, naming it', (t) => {
  const directory = fixture((dir) => {
    mkdirSync(join(dir, 'showcase'), { recursive: true });
    writeFileSync(join(dir, 'showcase', 'stub.js'), 'export const stub = 1;\n');
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(failures.some((failure) => failure.includes('showcase/ is back') && failure.includes('stub.js')), JSON.stringify(failures));
});

test('a reference to the retired module in the plane document is red, naming the file and the line', (t) => {
  const directory = fixture((dir) => {
    writeFileSync(join(dir, 'cockpit', 'index.html'), `${readFileSync(join(dir, 'cockpit', 'index.html'), 'utf8')}<script src="showcase.js"></script>\n`);
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(failures.some((failure) => failure.includes('cockpit/index.html references showcase.js') && failure.includes('line')), JSON.stringify(failures));
});

test('a reference in a published piece’s bytes is red, naming that file', (t) => {
  const directory = fixture((dir) => {
    const pin = JSON.parse(readFileSync(join(dir, 'bundles', 'visuals.manifest.json'), 'utf8'));
    pin.files = [...(pin.files ?? []), { path: 'web/panel.js', bytes: 0, sha256: '0'.repeat(64) }];
    writeFileSync(join(dir, 'bundles', 'visuals.manifest.json'), `${JSON.stringify(pin, null, 2)}\n`);
    mkdirSync(join(dir, 'web'), { recursive: true });
    writeFileSync(join(dir, 'web', 'panel.js'), 'const ref = "showcase.js";\n');
  });
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(failures.some((failure) => failure.includes('web/panel.js references showcase.js')), JSON.stringify(failures));
});

test('a missing deployment record is a red, not a skip', (t) => {
  const directory = fixture((dir) => rmSync(join(dir, 'deploy-manifest.txt')));
  cleanup(t, directory);
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  const failures = failuresOf(run);
  assert.ok(failures.some((failure) => failure.includes('deploy-manifest.txt is missing')), JSON.stringify(failures));
});
