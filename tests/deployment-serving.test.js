/**
 * Deployment-shaped serving tests — issue #38 (slice 2 of #35): the pin/cache half, hermetic.
 *
 * The built published plane (the product UI's `cockpit/` plane, issue #58) is served the way the
 * production host would (`scripts/deploytest-serve.mjs`) and the pin/cache contract is asserted
 * **from the served URLs and headers only**, the way a host or a CDN cache sees them:
 *
 *   1. the repository's own plane satisfies the contract, and the check writes nothing;
 *   2. the pin-change simulation (the served bytes move under the URL) runs on a copy — it
 *      shows nothing stale, and restores the copy byte-identically;
 *   3..8. each mutation axis is named by its own failure, so the assertions discriminate:
 *      a long-cached mutable URL, a missing validator, a 304 for any validator, bytes served
 *      under a false pin identity, a document policy without `'wasm-unsafe-eval'` (the website's
 *      current header, verbatim), and a `.wasm` served as `application/octet-stream`.
 *
 * The policy half (this lane) closes the "an approximation of the host's header" gap:
 *
 *   · the production policy is the PUBLIC host's header **verbatim**, pinned with its provenance
 *     in `tests/fixtures/site-policy.json` (website repository `synergy-services-website`,
 *     commit `4937a1e`, `static/staticwebapp.config.json`, `globalHeaders`). `PRODUCTION_CSP` is
 *     the resulting header after the owner-approved change (the token present);
 *     `CURRENT_SITE_CSP` is the header the site serves today (the token absent). Removing the
 *     token from the resulting header reproduces the current header byte-for-byte — asserted
 *     below, so the token-absent leg can never be a paraphrase of the real thing;
 *   · the serving shape is the host's own path (`/tools/cooling-tower/`, `DEFAULT_BASE_PATH`),
 *     not the repository root, and nothing is served outside that mount;
 *   · the **v0.1.0 tag-era surface** — what the approved public deployment plan vendors — is
 *     materialised from this worktree's own git object store (`git archive v0.1.0` of the tag's own
 *     surface directory: the tag is immutable and is never renamed or re-cut) and
 *     served the same way, under its own frozen bytes: the current pin cannot name that artifact,
 *     and the tag-era surface has no labelled engine-unavailable state. What a browser makes of
 *     either surface under either header is the hosted `deploytest-audit` run's business (this
 *     host has no browser binary); the tag-era token-absent record is its `observe` leg.
 *
 * No browser and no network beyond loopback: this file runs in `npm test` (and therefore in
 * the deploy-plane CI job) on the CI node pin. The browser half is `scripts/deploytest-audit.mjs`.
 *
 * Two cases here need something the machine may not have — the website repository (a different
 * repository; CI does not carry it) and the v0.1.0 tag (a shallow CI checkout has no tags; the
 * validate and deployment-serving jobs fetch it). When such an input is absent the case prints a
 * `NOT VERIFIABLE` diagnostic naming the input and the way to supply it, makes no assertion, and
 * passes: never a silent pass, never a failure. It is deliberately **not** a runner-level skip —
 * `tests/validation-results-drift.test.js` asserts that its counting child skips exactly one test
 * (the count gate itself), so an environment-dependent `t.skip()` would turn that gate red on CI.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  CSP_TOKEN, CURRENT_SITE_CSP, DEFAULT_BASE_PATH, PRODUCTION_CSP, SITE_POLICY, checkCacheContract, copyServedTree, pinnedDigests, servedFiles, sha256, startDeployServer,
} from '../scripts/deploytest-serve.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/**
 * The product UI's plane (issue #58): `cockpit/index.html`, `cockpit/pkg/**` and
 * `cockpit/assets/**` — the directory `scripts/serve.mjs` serves at `/` and the release publishes
 * (`scripts/cockpit-artifact.mjs`). `pkg/` is a **build product** (`cockpit/tools/build-web.sh
 * release`), so a tree that has not built it has no cockpit plane to check: those cases print the
 * NOT VERIFIABLE diagnostic below and assert nothing, exactly like the tag-era case does when the
 * tag is absent. The `cockpit` CI job builds the plane and then runs this file, so the cases do
 * verify there.
 */
const cockpitRoot = join(root, 'cockpit');

/** A tiny served plane: one document, one module, one catalog, one wasm artifact. */
function fixture() {
  const servedRoot = mkdtempSync(join(tmpdir(), 'deployment-serving-'));
  const bytes = new Map([
    ['index.html', Buffer.from('<!doctype html><html><head><title>fixture</title></head><body></body></html>\n')],
    ['app.js', Buffer.from('export const one = 1;\n')],
    ['data/sample-catalog.json', Buffer.from('{"metadata":{"id":"fixture"}}\n')],
    ['vendor/engine/synergy_drafthouse.wasm', Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04])],
  ]);
  for (const [file, content] of bytes) {
    const target = join(servedRoot, file);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, content);
  }
  return { servedRoot, files: servedFiles(servedRoot) };
}

/** Run the contract against one server configuration; returns the check result. */
async function checkWith(options, { pinChange = false } = {}) {
  const served = await startDeployServer(options);
  try {
    return await checkCacheContract({ origin: served.url, servedRoot: options.servedRoot, files: servedFiles(options.servedRoot), pins: options.pins, pinChange });
  } finally {
    await served.close();
  }
}

const idsOf = (result) => new Set(result.failures.map((failure) => failure.id));

/* ------------------------------------------------------------------ *
 * The pinned host policy: verbatim, with provenance, and checkable.
 * ------------------------------------------------------------------ */

const WEBSITE_REPO = process.env.SYNERGY_SERVICES_WEBSITE ?? SITE_POLICY.provenance.checkout;
const WEBSITE_FILE = SITE_POLICY.provenance.file;

/** The header as the website's own file carried it at the pinned commit, or a not-verifiable reason. */
function websiteHeaderAtPinnedCommit() {
  if (!existsSync(join(WEBSITE_REPO, WEBSITE_FILE))) {
    return { skip: `the website repository is not present at ${WEBSITE_REPO} — it is a different repository and CI does not carry it; set SYNERGY_SERVICES_WEBSITE to its checkout to run this cross-check` };
  }
  try {
    const raw = execFileSync('git', ['-C', WEBSITE_REPO, 'show', `${SITE_POLICY.provenance.commit}:${WEBSITE_FILE}`], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    return { value: JSON.parse(raw).globalHeaders['Content-Security-Policy'] };
  } catch (error) {
    return { skip: `the website repository at ${WEBSITE_REPO} does not resolve ${SITE_POLICY.provenance.commit}:${WEBSITE_FILE} (${String(error.message).split('\n')[0]}) — a shallow or rewritten clone is a skip, never a failure` };
  }
}

test('the pinned host policy: both headers verbatim, one token apart, with provenance', () => {
  const policy = SITE_POLICY;
  assert.equal(policy.provenance.repository, 'synergy-services-website');
  assert.equal(policy.provenance.commit, '4937a1ea452bb6aaa241b6af142d442a0f4de400');
  assert.equal(policy.provenance.commitShort, policy.provenance.commit.slice(0, 7));
  assert.equal(policy.provenance.file, 'static/staticwebapp.config.json');
  assert.match(policy.provenance.field, /Content-Security-Policy/);
  assert.match(policy.provenance.blob, /^[0-9a-f]{40}$/);

  assert.equal(PRODUCTION_CSP, policy.resultingHeader, 'PRODUCTION_CSP must be the pinned resulting header, verbatim');
  assert.equal(CURRENT_SITE_CSP, policy.currentHeader, 'CURRENT_SITE_CSP must be the pinned current header, verbatim');
  assert.equal(sha256(Buffer.from(PRODUCTION_CSP)), policy.digests.resultingHeaderSha256);
  assert.equal(sha256(Buffer.from(CURRENT_SITE_CSP)), policy.digests.currentHeaderSha256);

  assert.ok(PRODUCTION_CSP.includes(CSP_TOKEN), 'the resulting header carries the wasm token');
  assert.ok(!CURRENT_SITE_CSP.includes(CSP_TOKEN), 'the current header does not carry the wasm token');
  assert.equal(PRODUCTION_CSP.split(CSP_TOKEN).length - 1, 1, 'the token appears exactly once in the resulting header');
  assert.equal(PRODUCTION_CSP.replace(` ${CSP_TOKEN}`, ''), CURRENT_SITE_CSP, 'the approved change is exactly one token: removing it from the resulting header reproduces the current header byte-for-byte');

  assert.equal(DEFAULT_BASE_PATH, policy.deployPath);
  assert.equal(DEFAULT_BASE_PATH, '/tools/cooling-tower/');
});

test('the website repository carries the pinned current header at the pinned commit (reported as not verifiable, never a failure, when the repository is absent)', (t) => {
  const found = websiteHeaderAtPinnedCommit();
  if (found.skip !== undefined) {
    t.diagnostic(`NOT VERIFIABLE: ${found.skip}`);
    return;
  }
  assert.equal(found.value, CURRENT_SITE_CSP, `the website's ${WEBSITE_FILE} no longer carries the pinned current header at ${SITE_POLICY.provenance.commit} — update tests/fixtures/site-policy.json with the new provenance and strings`);
});

/* ------------------------------------------------------------------ *
 * The serving contract, on the served headers/URLs alone.
 * ------------------------------------------------------------------ */

test('the cockpit plane (the product UI) satisfies the production serving contract, and the check writes nothing', async (t) => {
  if (!existsSync(join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm'))) {
    t.diagnostic(`NOT VERIFIABLE: ${join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm')} is not built — run cockpit/tools/build-web.sh release (the cockpit CI job does, before running this file)`);
    return;
  }
  const before = servedFiles(cockpitRoot).map((file) => `${file} ${sha256(readFileSync(join(cockpitRoot, file)))}`);
  const result = await checkWith({ servedRoot: cockpitRoot });
  assert.deepEqual(result.failures, [], result.failures.map((failure) => `${failure.id}: ${failure.detail}`).join('\n'));
  assert.equal(result.passed, true);
  assert.ok(result.files >= 10, `expected the whole cockpit plane, saw ${result.files} files`);
  /* The plane is the cockpit's own: the document, the wasm build and the asset the app fetches. */
  const files = servedFiles(cockpitRoot);
  for (const required of ['index.html', 'pkg/drafthouse_cockpit.js', 'pkg/drafthouse_cockpit_bg.wasm', 'assets/fixture.json', 'assets/fonts/subset/IBMPlexSans-Regular.ttf']) {
    assert.ok(files.includes(required), `the cockpit plane must carry ${required}; it carries ${files.join(', ')}`);
  }
  const after = servedFiles(cockpitRoot).map((file) => `${file} ${sha256(readFileSync(join(cockpitRoot, file)))}`);
  assert.deepEqual(after, before, 'the check must not write to the served plane');
});

test('the pin-change simulation shows nothing stale on a copy of the cockpit plane, and restores the copy byte-identically', async (t) => {
  // Run the copy regression even without a built cockpit. Cargo output is not a deploy asset.
  const source = fixture();
  let fixtureCopy;
  try {
    const buildOutput = join(source.servedRoot, 'target/debug/deps');
    mkdirSync(buildOutput, { recursive: true });
    writeFileSync(join(buildOutput, 'unused.wasm'), Buffer.from([0x00]));
    assert.deepEqual(servedFiles(source.servedRoot), source.files, 'Cargo output must not enter cache checks or pin-change selection');
    fixtureCopy = copyServedTree(source.servedRoot);
    assert.equal(existsSync(join(fixtureCopy, 'target')), false, 'the serving copy must not duplicate the Cargo build cache');
    assert.deepEqual(servedFiles(fixtureCopy), source.files, 'all deploy assets must still be copied');
    for (const file of source.files) {
      assert.deepEqual(readFileSync(join(fixtureCopy, file)), readFileSync(join(source.servedRoot, file)), `${file} must be copied byte-identically`);
    }
    const result = await checkWith({ servedRoot: fixtureCopy, pins: new Map() }, { pinChange: true });
    assert.deepEqual(result.failures, [], 'the copy must still support the pin-change simulation');
  } finally {
    if (fixtureCopy) rmSync(fixtureCopy, { recursive: true, force: true });
    rmSync(source.servedRoot, { recursive: true, force: true });
  }
  if (!existsSync(join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm'))) {
    t.diagnostic(`NOT VERIFIABLE: ${join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm')} is not built — run cockpit/tools/build-web.sh release (the cockpit CI job does, before running this file)`);
    return;
  }
  const copy = copyServedTree(cockpitRoot);
  try {
    const before = sha256(readFileSync(join(copy, 'pkg/drafthouse_cockpit_bg.wasm')));
    const result = await checkWith({ servedRoot: copy }, { pinChange: true });
    assert.deepEqual(result.failures, [], result.failures.map((failure) => `${failure.id}: ${failure.detail}`).join('\n'));
    assert.ok(result.checks.some((check) => check.id === 'no-stale-after-pin-change'), 'the pin-change simulation must have run');
    assert.equal(sha256(readFileSync(join(copy, 'pkg/drafthouse_cockpit_bg.wasm'))), before, 'the simulation must restore the copy byte-identically');
  } finally {
    rmSync(copy, { recursive: true, force: true });
  }
});

test('the cockpit plane satisfies the contract under the host mount path, and nothing outside the mount is served', async (t) => {
  if (!existsSync(join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm'))) {
    t.diagnostic(`NOT VERIFIABLE: ${join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm')} is not built — run cockpit/tools/build-web.sh release (the cockpit CI job does, before running this file)`);
    return;
  }
  const served = await startDeployServer({ servedRoot: cockpitRoot, basePath: DEFAULT_BASE_PATH });
  try {
    const result = await checkCacheContract({ origin: served.url, servedRoot: cockpitRoot, files: servedFiles(cockpitRoot) });
    assert.deepEqual(result.failures, [], result.failures.map((failure) => `${failure.id}: ${failure.detail}`).join('\n'));
    assert.equal(served.mount, '/tools/cooling-tower');
    /* The mount path is load-bearing: the same bytes are NOT reachable at the root. */
    const outside = await fetch(`${served.origin}/index.html`);
    assert.equal(outside.status, 404, 'the tool is not served at the root of the origin');
    const inside = await fetch(`${served.url}/index.html`);
    assert.equal(inside.status, 200);
    assert.equal(inside.headers.get('content-security-policy'), PRODUCTION_CSP, 'the document carries the resulting header verbatim');
    const wasm = await fetch(`${served.url}/pkg/drafthouse_cockpit_bg.wasm`);
    assert.equal(wasm.headers.get('content-type'), 'application/wasm');
  } finally {
    await served.close();
  }
});

test('the cockpit plane reaches the engine-unavailable state: the fixture record is served, the missing asset is missing (the rendered state is the browser frame)', async (t) => {
  if (!existsSync(join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm'))) {
    t.diagnostic(`NOT VERIFIABLE: ${join(cockpitRoot, 'pkg/drafthouse_cockpit_bg.wasm')} is not built — run cockpit/tools/build-web.sh release (the cockpit CI job does, before running this file)`);
    return;
  }
  const served = await startDeployServer({ servedRoot: cockpitRoot, basePath: '/' });
  try {
    /* `?engine=unavailable` makes the app request `assets/engine-missing.json`: it must be absent
       (that 404 is the state), while the recorded fixture the ready path needs is served. */
    const missing = await fetch(`${served.url}/assets/engine-missing.json`);
    assert.equal(missing.status, 404, 'the unavailable state needs the missing asset to be missing');
    const fixture = await fetch(`${served.url}/assets/fixture.json`);
    assert.equal(fixture.status, 200);
    assert.equal(fixture.headers.get('content-type'), 'application/json; charset=utf-8');
    /* The document the host serves boots the app from the plane's own wasm build. */
    const document = await (await fetch(`${served.url}/index.html`)).text();
    assert.ok(document.includes('pkg/drafthouse_cockpit.js'), "index.html must boot the plane's own wasm build");
  } finally {
    await served.close();
  }
});

/* ------------------------------------------------------------------ *
 * The v0.1.0 tag-era surface: what the approved public deployment plan vendors.
 * ------------------------------------------------------------------ */

const TAG = 'v0.1.0';

/** True when this clone carries the tag (a shallow CI checkout does not until a step fetches it). */
function tagIsPresent() {
  try {
    execFileSync('git', ['-C', root, 'rev-parse', '--verify', `${TAG}^{commit}`], { stdio: ['ignore', 'pipe', 'pipe'] });
    return true;
  } catch {
    return false;
  }
}

/** `git archive` the tag's surface directory into a temp dir, drop README.md — nothing outside this worktree. */
function materialiseTagSurface() {
  const dir = mkdtempSync(join(tmpdir(), 'deployment-serving-tag-'));
  const archive = join(dir, 'tag-surface.tar');
  execFileSync('git', ['-C', root, 'archive', '--format=tar', '-o', archive, TAG, 'showcase'], { stdio: ['ignore', 'pipe', 'pipe'] });
  const servedRoot = join(dir, 'served');
  mkdirSync(servedRoot, { recursive: true });
  execFileSync('tar', ['-xf', archive, '-C', servedRoot, '--strip-components=1'], { stdio: ['ignore', 'pipe', 'pipe'] });
  rmSync(archive, { force: true });
  rmSync(join(servedRoot, 'README.md'), { force: true });
  return servedRoot;
}

test(`the ${TAG} tag-era surface serves at the host path under its own frozen bytes, with no labelled unavailable state (the token-absent behaviour is the hosted observe leg)`, async (t) => {
  if (!tagIsPresent()) {
    t.diagnostic(`NOT VERIFIABLE: the ${TAG} tag is not in this clone (a shallow checkout has no tags; the validate and deployment-serving jobs fetch it) — the frozen-surface case needs it`);
    return;
  }

  const servedRoot = materialiseTagSurface();
  try {
    assert.ok(existsSync(join(servedRoot, 'index.html')) && existsSync(join(servedRoot, 'vendor/engine/cooling_tower_calculator.wasm')), 'the tag-era surface must be self-contained');

    /* Frozen history must use its own manifests, never today's pin or an empty pin map. */
    const currentPins = pinnedDigests();
    const currentWasmPin = currentPins.get('vendor/engine/synergy_drafthouse.wasm');
    const tagWasm = sha256(readFileSync(join(servedRoot, 'vendor/engine/cooling_tower_calculator.wasm')));
    assert.ok(currentWasmPin !== undefined, 'the engine pin must name the served wasm artifact');
    assert.notEqual(tagWasm, currentWasmPin, 'the v0.1.0 artifact is its own bytes; the current pin cannot bless it');

    /* The surface a reader of the deployed tag tree gets has no labelled engine-unavailable state
       (it predates #38) — the static half of the record; the hosted observe leg records what the
       page actually does with the token absent. */
    const tagIndex = readFileSync(join(servedRoot, 'index.html'), 'utf8');
    const tagSurfaceJs = readFileSync(join(servedRoot, 'showcase.js'), 'utf8');
    assert.ok(!tagIndex.includes('engine-unavailable') && !tagSurfaceJs.includes('engine-unavailable'), 'the tag-era surface predates the labelled unavailable state; if this ever changes, the frozen tree moved');

    /* The pins come from the frozen revision itself — { ref: TAG }, the explicit override,
       because the frozen archive carries no bundles/ on disk; the default (the audited tree's
       own on-disk bundles/) has nothing to find here, and must never fall back to today's
       pins. The frozen deployment is validated against its own era's identity. */
    const pins = pinnedDigests({ ref: TAG });
    assert.equal(pins.get('vendor/engine/cooling_tower_calculator.wasm'), tagWasm);
    assert.notEqual(pins.get('vendor/engine/binding.mjs'), currentPins.get('vendor/engine/binding.mjs'));
    const result = await checkWith({ servedRoot, basePath: DEFAULT_BASE_PATH, pins });
    assert.deepEqual(result.failures, [], result.failures.map((failure) => `${failure.id}: ${failure.detail}`).join('\n'));
    assert.ok(result.files >= 15, `expected the whole tag-era plane, saw ${result.files} files`);

    const probe = JSON.parse(execFileSync(process.execPath, [
      join(root, 'scripts/deploytest-probe.mjs'), '--dir', servedRoot, '--pin-ref', TAG, '--no-browser',
    ], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }));
    assert.equal(probe.passed, true, 'the CLI must forward historical pins to both serving checks');
    assert.ok(probe.probes.filter((entry) => !entry.skipped).every((entry) => entry.ok));

    const binding = join(servedRoot, 'vendor/engine/binding.mjs');
    const original = readFileSync(binding);
    try {
      writeFileSync(binding, Buffer.concat([original, Buffer.from('\n// changed bytes\n')]));
      const mutated = await checkWith({ servedRoot, basePath: DEFAULT_BASE_PATH, pins });
      assert.ok(idsOf(mutated).has('validator-is-the-pin'), 'historical pins must still reject changed served bytes');
    } finally {
      writeFileSync(binding, original);
    }
    assert.ok(servedFiles(servedRoot).every((file) => file !== 'README.md'), 'README.md is dropped: the served plane is the tool');
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test('a long-cached mutable URL is named as the stale hazard', async () => {
  const { servedRoot } = fixture();
  try {
    const result = await checkWith({ servedRoot, cachePolicy: 'naive' });
    assert.ok(idsOf(result).has('plain-cache-not-stale-able'), JSON.stringify(result.failures));
    assert.equal(result.passed, false);
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test('a missing validator is named: the pin cannot be revalidated, so an unchanged pin is not reusable', async () => {
  const { servedRoot } = fixture();
  try {
    const result = await checkWith({ servedRoot, cachePolicy: 'no-validator' });
    assert.ok(idsOf(result).has('validator-is-the-pin'), JSON.stringify(result.failures));
    assert.ok(idsOf(result).has('unchanged-pin-reuses'), JSON.stringify(result.failures));
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test('a 304 for any validator is named: a pin change would be served stale, and the simulation says so', async () => {
  const { servedRoot } = fixture();
  try {
    const result = await checkWith({ servedRoot, cachePolicy: 'accept-stale' }, { pinChange: true });
    assert.ok(idsOf(result).has('stale-validator-refreshes'), JSON.stringify(result.failures));
    assert.ok(idsOf(result).has('no-stale-after-pin-change'), JSON.stringify(result.failures));
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test('bytes served under a pin identity they do not have are named', async () => {
  const { servedRoot } = fixture();
  try {
    const result = await checkWith({ servedRoot, cachePolicy: 'any-digest' }, { pinChange: true });
    assert.ok(idsOf(result).has('pin-address-exact'), JSON.stringify(result.failures));
    assert.ok(idsOf(result).has('no-stale-after-pin-change'), JSON.stringify(result.failures));
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test("a document policy without the wasm token is named: the website's current header, verbatim, must bite", async () => {
  const { servedRoot } = fixture();
  try {
    const result = await checkWith({ servedRoot, csp: CURRENT_SITE_CSP });
    assert.ok(idsOf(result).has('csp-production'), JSON.stringify(result.failures));
    assert.equal(result.passed, false);
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test('a .wasm served as application/octet-stream is named', async () => {
  const { servedRoot } = fixture();
  try {
    const result = await checkWith({ servedRoot, wasmContentType: 'application/octet-stream' });
    assert.ok(idsOf(result).has('mime-wasm'), JSON.stringify(result.failures));
    assert.ok(idsOf(result).has('content-type-pinned'), JSON.stringify(result.failures));
  } finally {
    rmSync(servedRoot, { recursive: true, force: true });
  }
});

test("the pin identity is the tree under audit's own: a frozen surface is never asserted against another tree's pins (issue #60 fix round)", async (t) => {
  /* The regression the retirement's CI run exposed: `pinnedDigests()` defaulted to the CURRENT
     tree, so once the pins' served paths (`vendor/engine/binding.mjs`, …) resolved for the
     tag-era surface, the frozen v0.1.0 bytes were asserted against the current digests — a
     comparison that can never hold (the tag is immutable). The scoping rule is the #56
     precedent: a `--dir` surface's expectations come from that tree. These cases pin the rule
     hermetically, with bytes that stand in for the frozen surface. */
  const currentPins = pinnedDigests();
  const frozenRoot = mkdtempSync(join(tmpdir(), 'deployment-serving-frozen-'));
  const binding = Buffer.from('// the frozen surface binding: bytes from another era\\n');
  mkdirSync(join(frozenRoot, 'vendor/engine'), { recursive: true });
  writeFileSync(join(frozenRoot, 'index.html'), '<!doctype html><html><body></body></html>\\n');
  writeFileSync(join(frozenRoot, 'vendor/engine/binding.mjs'), binding);
  try {
    assert.ok(currentPins.get('vendor/engine/binding.mjs') !== undefined, 'the current tree publishes an engine binding pin; without it this regression cannot be expressed');
    assert.notEqual(sha256(binding), currentPins.get('vendor/engine/binding.mjs'), 'the frozen stand-in bytes must differ from the current pin for this test to bite');
    assert.equal(pinnedDigests({ fromDirectory: frozenRoot }).size, 0, 'a tree with no bundles/ of its own publishes no pins: the lookup is scoped, never inherited');

    /* A tree with no pin manifest of its own is checked WITHOUT foreign pins — the semantics
       the frozen tag-era leg runs under — and the per-file validator assertion still ran. */
    const scoped = await checkWith({ servedRoot: frozenRoot, pins: new Map() });
    assert.deepEqual(scoped.failures, [], JSON.stringify(scoped.failures));
    const validator = scoped.checks.find((check) => check.id === 'validator-is-the-pin');
    assert.ok(validator && validator.ok, 'the ETag-is-the-served-bytes assertion must still run on the frozen tree');

    /* The pre-fix default (another tree's pins) must keep failing, naming both digests. */
    const foreign = await checkWith({ servedRoot: frozenRoot, pins: currentPins });
    assert.equal(foreign.passed, false);
    const named = foreign.failures.filter((failure) => failure.id === 'validator-is-the-pin' && failure.detail.includes('vendor/engine/binding.mjs'));
    assert.ok(named.length > 0, JSON.stringify(foreign.failures));
    assert.ok(named.every((failure) => failure.detail.includes(sha256(binding)) && failure.detail.includes(currentPins.get('vendor/engine/binding.mjs'))), 'the failure must name the served bytes and the pin');

    /* A tree that publishes its own pins (bytes matching them) is green: the digest assertion
       is scoped, not muted. */
    const ownPins = new Map([['vendor/engine/binding.mjs', sha256(binding)]]);
    const honest = await checkWith({ servedRoot: frozenRoot, pins: ownPins });
    assert.deepEqual(honest.failures, [], JSON.stringify(honest.failures));
  } finally {
    rmSync(frozenRoot, { recursive: true, force: true });
  }
});
