#!/usr/bin/env node
/**
 * The deployment-shaped mutation battery — issue #38, item 4.
 *
 * "A test that cannot fail is not a test": this driver runs each serving-shape mutation and the
 * restored shape through the deployment-shaped checks and requires the *exact* exit status the
 * mutation must produce — red where the mutated header/token/MIME must bite, green where the
 * shape is correct — then proves the served plane byte-identical before and after.
 *
 * It probes a **named bundle-shaped tree** (`--dir`, required): the pinned pieces placed at the
 * served paths a consumer deployment uses, or the frozen v0.1.0 tag-era surface the deployment
 * job materialises — the same trees `deploytest-audit.mjs` audits. The probes that assert the
 * labelled engine-unavailable state (the retired JavaScript surface's contract) are gone with
 * that surface; its successor, the cockpit plane, is asserted by its own gates
 * (`tests/deployment-serving.test.js`, `scripts/server-smoke.mjs`, the CI `cockpit` job).
 *
 * Probes, in order:
 *
 *   serving-contract            the pin/cache contract on the served plane, at the host path  -> 0
 *   serving-contract-at-root    the same contract when the files are served at the root       -> 0
 *   serving-contract-current-header  the website's CURRENT header (no token) must bite        -> 1
 *   serving-contract-stale      the same check with a long-cached mutable URL                 -> 1
 *   production-baseline         the production leg against the resulting header               -> 0
 *   token-removed-production    ... under the website's current, token-less header            -> 1
 *   header-wrong-production     ... with a whole wrong CSP header                             -> 1
 *   mime-wrong-production       ... with .wasm served as application/octet-stream             -> 1
 *   production-restored         the production leg against the resulting header again         -> 0
 *
 * The three `serving-contract*` probes are hermetic (no browser) and give the policy and
 * pin/cache axes their own raw exits; the website's current header is the mutation for the
 * `script-src` token, and the driver refuses to run at all unless removing the token from the
 * pinned resulting header reproduces that current header byte-for-byte. The browser probes need
 * a reachable CDP endpoint: without one this driver exits 2 — it never reports a pass it could
 * not produce. Nothing under version control is edited by any probe; the mutations are
 * parameters of the serving shape, and the served bytes are hashed before and after.
 *
 * Usage: node scripts/deploytest-probe.mjs --dir <served tree> [--wasm-file <name>]
 *                                          [--pin-ref <git ref>] (bundle pins for the served revision)
 *                                          [--endpoint http://127.0.0.1:9333]
 *                                          [--target <id> | --target-url <substring>]
 *                                          [--no-browser] [--out <dir>]
 *
 * Exit status: 0 every probe produced its exact status and the served plane is byte-identical;
 * 1 a probe produced the wrong status; 2 the probes could not run (no browser, no tree).
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { CURRENT_SITE_CSP, DEFAULT_BASE_PATH, PRODUCTION_CSP, servedFiles, sha256 } from './deploytest-serve.mjs';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : process.argv[index + 1];
}

const servedDir = option('--dir', undefined);
if (servedDir === undefined) {
  console.error('deploytest-probe: --dir <served tree> is required — the tree being probed must be named (it is never guessed)');
  process.exit(2);
}
const servedRoot = resolve(servedDir);
const wasmFileName = option('--wasm-file', undefined);
const endpoint = option('--endpoint', process.env.DEPLOYTEST_CDP ?? 'http://127.0.0.1:9333').replace(/\/$/, '');
const targetId = option('--target', undefined);
const targetUrl = option('--target-url', undefined);
const noBrowser = process.argv.includes('--no-browser');
const outDir = option('--out', undefined) === undefined ? undefined : resolve(option('--out'));

const TOKENLESS_CSP = CURRENT_SITE_CSP;

/* The mutation axis is exactly one token: refuse to run unless the pinned current header IS the
   pinned resulting header with `'wasm-unsafe-eval'` removed. */
const TOKEN_REMOVED = PRODUCTION_CSP.replace(/\s*'wasm-unsafe-eval'/, '');
if (TOKEN_REMOVED !== CURRENT_SITE_CSP) {
  console.error(`deploytest-probe: the pinned headers disagree: removing 'wasm-unsafe-eval' from the resulting header does not reproduce the current header.\n  resulting-minus-token: ${JSON.stringify(TOKEN_REMOVED)}\n  current header:        ${JSON.stringify(CURRENT_SITE_CSP)}`);
  process.exit(2);
}

if (!existsSync(servedRoot) || !statSync(servedRoot).isDirectory()) {
  console.error(`deploytest-probe: --dir ${servedRoot} is not a directory (expected a built published plane)`);
  process.exit(2);
}

/** The served plane's identity: every file and its digest. The restore proof compares these. */
const digestList = () => servedFiles(servedRoot).map((file) => `${sha256(readFileSync(join(servedRoot, file)))}  ${file}`);

const auditArgs = () => {
  const args = ['--dir', servedRoot];
  if (wasmFileName !== undefined) args.push('--wasm-file', wasmFileName);
  if (targetId !== undefined) args.push('--target', targetId);
  else if (targetUrl !== undefined) args.push('--target-url', targetUrl);
  return args;
};

const dirArgs = ['--dir', servedRoot];
const pinRef = option('--pin-ref', undefined);
if (pinRef !== undefined) dirArgs.push('--pin-ref', pinRef);

const PROBES = [
  { name: 'serving-contract', script: 'scripts/deploytest-serve.mjs', args: ['--check', ...dirArgs], expected: 0, browser: false, what: 'the pin/cache contract on the served plane, at the host mount path (the default)' },
  { name: 'serving-contract-at-root', script: 'scripts/deploytest-serve.mjs', args: ['--check', ...dirArgs, '--base-path', ''], expected: 0, browser: false, what: 'the same contract when the files are served at the root instead' },
  { name: 'serving-contract-current-header', script: 'scripts/deploytest-serve.mjs', args: ['--check', ...dirArgs, '--csp', CURRENT_SITE_CSP], expected: 1, browser: false, what: "the website's current header (token absent) must bite the production-policy assertion" },
  { name: 'serving-contract-stale', script: 'scripts/deploytest-serve.mjs', args: ['--check', ...dirArgs, '--cache-policy', 'naive'], expected: 1, browser: false, what: 'a long-cached mutable URL must be named as the stale hazard' },
  { name: 'production-baseline', script: 'scripts/deploytest-audit.mjs', args: ['--leg', 'production'], expected: 0, browser: true, what: 'the resulting header lets the surface render and compute through wasm' },
  { name: 'token-removed-production', script: 'scripts/deploytest-audit.mjs', args: ['--leg', 'production', '--csp', TOKENLESS_CSP], expected: 1, browser: true, what: "under the website's current header the production leg must go red" },
  { name: 'header-wrong-production', script: 'scripts/deploytest-audit.mjs', args: ['--leg', 'production', '--csp', "script-src 'none'"], expected: 1, browser: true, what: 'a wrong CSP header must bite the production leg' },
  { name: 'mime-wrong-production', script: 'scripts/deploytest-audit.mjs', args: ['--leg', 'production', '--wasm-content-type', 'application/octet-stream'], expected: 1, browser: true, what: 'a wrong .wasm MIME must bite the production leg' },
  { name: 'production-restored', script: 'scripts/deploytest-audit.mjs', args: ['--leg', 'production'], expected: 0, browser: true, what: 'the restored, resulting header is green again' },
];

/** Every balanced {...} in some text — progress lines may surround the report. */
function balancedObjects(text) {
  const objects = [];
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] !== '{') continue;
    let depth = 0;
    let inString = false;
    let escaped = false;
    for (let cursor = index; cursor < text.length; cursor += 1) {
      const character = text[cursor];
      if (inString) {
        if (escaped) escaped = false;
        else if (character === '\\') escaped = true;
        else if (character === '"') inString = false;
        continue;
      }
      if (character === '"') inString = true;
      else if (character === '{') depth += 1;
      else if (character === '}') {
        depth -= 1;
        if (depth === 0) {
          objects.push(text.slice(index, cursor + 1));
          index = cursor;
          break;
        }
      }
    }
  }
  return objects;
}

/** The report a child printed: the last balanced object that parses (progress lines come first). */
function lastJson(text) {
  const objects = balancedObjects(text);
  for (let index = objects.length - 1; index >= 0; index -= 1) {
    try {
      return JSON.parse(objects[index]);
    } catch {
      /* not the report; keep looking */
    }
  }
  return null;
}

/** What a report says failed — the assertion ids, so the probe names what bit. */
function failureIds(report) {
  if (report === null) return [];
  if (Array.isArray(report)) return [...new Set(report.flatMap((leg) => failureIds(leg)))];
  if (Array.isArray(report.failures)) {
    return [...new Set(report.failures.map((failure) => (typeof failure === 'string' ? failure.split(':')[0] : failure.id)))];
  }
  return [];
}

const before = digestList();
const probes = [];
for (const probe of PROBES) {
  if (probe.browser && noBrowser) {
    probes.push({ name: probe.name, skipped: 'browser probes disabled (--no-browser)', expected: probe.expected });
    continue;
  }
  const args = [...probe.args, ...(probe.browser ? ['--endpoint', endpoint, ...auditArgs()] : [])];
  let exit = 0;
  let output = '';
  try {
    output = execFileSync(process.execPath, [join(root, probe.script), ...args], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, cwd: root });
  } catch (error) {
    exit = error.status ?? 1;
    /* stdout + stderr only: error.message repeats stderr and would duplicate the report. */
    output = `${error.stdout ?? ''}${error.stderr ?? ''}` || String(error.message ?? '');
  }
  const report = lastJson(output);
  const ids = failureIds(report);
  const observed = Array.isArray(report)
    ? report.map((leg) => leg?.observation?.classification ?? null).join(',')
    : (report?.observation?.classification ?? null);
  const observationOk = probe.expectObservation === undefined || observed === probe.expectObservation;
  probes.push({
    name: probe.name,
    what: probe.what,
    argv: [`node`, probe.script, ...args].join(' ').replace(`${root}${sep}`, ''),
    expected: probe.expected,
    exit,
    ok: exit === probe.expected && observationOk,
    expectObservation: probe.expectObservation ?? null,
    observed,
    failedAssertions: ids,
    note: exit === 2 && probe.browser ? 'no browser reachable — the probe could not run' : undefined,
  });
  const status = exit !== probe.expected
    ? `EXPECTED ${probe.expected}, GOT ${exit}`
    : observationOk ? 'as expected' : `EXPECTED observation ${JSON.stringify(probe.expectObservation)}, GOT ${JSON.stringify(observed)}`;
  console.error(`deploytest-probe: ${probe.name}: exit ${exit} (${status})${ids.length ? ` — failed: ${ids.join(', ')}` : ''}`);
}
const after = digestList();

const browserUnrunnable = probes.some((probe) => probe.note !== undefined);
const skipped = probes.filter((probe) => probe.skipped !== undefined).map((probe) => probe.name);
const restores = {
  servedTreeBefore: sha256(Buffer.from(before.join('\n'))),
  servedTreeAfter: sha256(Buffer.from(after.join('\n'))),
  servedTreeUnchanged: before.length === after.length && before.every((entry, index) => entry === after[index]),
};
const mismatched = probes.filter((probe) => probe.ok !== true && probe.note === undefined && probe.skipped === undefined);
const report = {
  passed: mismatched.length === 0 && restores.servedTreeUnchanged && !browserUnrunnable,
  endpoint,
  target: targetId ?? targetUrl ?? null,
  basePath: DEFAULT_BASE_PATH,
  policy: {
    resulting: PRODUCTION_CSP,
    current: CURRENT_SITE_CSP,
    tokenRemovedFromResulting: TOKEN_REMOVED,
    tokenRemovedEqualsCurrent: TOKEN_REMOVED === CURRENT_SITE_CSP,
  },
  probes,
  skipped,
  restores,
  failures: [
    ...mismatched.map((probe) => `${probe.name}: expected exit ${probe.expected}, got ${probe.exit}${probe.ok ? '' : probe.expectObservation === null ? '' : ` (observation: expected ${JSON.stringify(probe.expectObservation)}, got ${JSON.stringify(probe.observed)})`}`),
    ...(restores.servedTreeUnchanged ? [] : ['the served plane is not byte-identical before and after the probes']),
    ...(browserUnrunnable ? ['a browser probe could not run: no reachable CDP browser (the driver refuses to report a pass it could not produce)'] : []),
  ],
};

if (outDir !== undefined) {
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, 'deployment-probe.json'), `${JSON.stringify(report, null, 2)}\n`);
}
if (report.passed) console.log(JSON.stringify(report, null, 2));
else console.error(JSON.stringify(report, null, 2));
process.exitCode = report.passed ? 0 : browserUnrunnable ? 2 : 1;
