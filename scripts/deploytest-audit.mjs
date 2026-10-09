#!/usr/bin/env node
/**
 * Deployment-shaped browser audit — issue #38 (slice 2 of #35).
 *
 * It serves the **built published plane** the way the production host would — the production CSP,
 * `application/wasm`, the pin/cache contract (`scripts/deploytest-serve.mjs`) — and drives a real
 * browser at it over CDP with **zero dependencies** (Node's global WebSocket does the protocol,
 * Node 22+), in the repository's zero-dependency CDP shape (`scripts/deploy-audit.mjs`): an
 * endpoint that is configurable, an existing
 * target that can be reused when the host cannot mint new renderers, and a **loud refusal**
 * (exit 2) when no browser is reachable — never a silent skip, never a vacuous pass.
 *
 * Three legs, each with its own server shape. Every document policy is one of the two strings the
 * host itself uses — pinned verbatim in `tests/fixtures/site-policy.json` from the website
 * repository's `static/staticwebapp.config.json` at commit 4937a1e — never an approximation; the
 * served files sit under the host's own path (`/tools/cooling-tower/`), the `--base-path` default.
 *
 *   production    the resulting header (the approved change: the token is present) and
 *                 `application/wasm`. Asserts, from the rendered page and the browser's own
 *                 network signals, that the surface renders and computes **through wasm**:
 *                 the wasm request carried `application/wasm`, the module instantiated, the
 *                 headline numbers are rendered, and every rendered number equals the value the
 *                 engine itself computes in Node from the SERVED wasm bytes + catalog for the
 *                 duty the page displays — a real result read out of the rendered page, not a
 *                 probe of the module registry. And the permission is load-bearing: the
 *                 `unavailable` leg shows the same page refuses to compute without the token.
 *
 *   unavailable   the website's **current** header, verbatim (token absent — what the site serves
 *                 today). Asserts the explicit engine-unavailable state — labelled and visible,
 *                 never blank, never a number, never a stale number (a second run is asserted
 *                 too). The leg fails if the page goes blank or prints a number instead. This is
 *                 the contract of the CURRENT surface; a frozen release tree that predates it is
 *                 recorded by `observe` instead, never asserted against.
 *
 *   observe       the same token-absent serving shape, **recording instead of asserting the
 *                 surface's contract**: for a surface that predates the labelled unavailable
 *                 state (a frozen release tree, or a consumer with no error handling) it captures
 *                 what the page actually does — painted or not, a visible error line or not, a
 *                 labelled state or not, numbers or not, console messages, screenshot — and
 *                 classifies it mechanically (`numbers`, `labelled-unavailable-state`,
 *                 `raw-error-line`, `blank-page`, `no-visible-change`). Its assertions cover only
 *                 the observation itself: the page painted, the document carried the policy named
 *                 on the command line, and something was read out of the rendered page.
 *
 * Every leg refuses to click before the browser has painted (`Page.setLifecycleEventsEnabled(true)`
 * is required for `Page.lifecycleEvent` to arrive at all; a missing first paint is a failure,
 * not a pass) and assert the engine was fetched only after that paint.
 *
 * Usage — from any working directory:
 *
 *     node scripts/deploytest-audit.mjs [--leg production|unavailable|observe|both] --dir <served tree>
 *         [--base-path /tools/cooling-tower/] [--endpoint http://127.0.0.1:9333]
 *         [--target <id> | --target-url <substring>]
 *         [--timeout-ms 300000] [--out <dir>] [--label deployment]
 *         [--csp <policy>] [--unavailable-csp <policy>] [--wasm-content-type <type>]
 *         [--cache-policy <policy>] [--wasm-file <filename>]
 *
 *   --leg       the leg to run (default: both — `production` then `unavailable`; `observe` is
 *               always requested explicitly, because it records rather than asserts)
 *   --dir       the served root, REQUIRED: the tree being audited must be named. It audits a
 *               bundle-shaped plane — the pinned pieces placed at the served paths a consumer
 *               deployment uses, or the frozen v0.1.0 tag-era surface the deployment job
 *               materialises (the product UI's own plane is asserted by its own gates:
 *               `tests/deployment-serving.test.js`, `scripts/server-smoke.mjs` and the CI
 *               `cockpit` job). The
 *               engine artifact is read from THAT tree's own surface — the tree's own top-level
 *               files name the wasm it fetches — never from a name baked into this file: the
 *               current-era trees name `synergy_drafthouse.wasm`, while the frozen v0.1.0 tree
 *               names `cooling_tower_calculator.wasm` and is audited as it stands (that tree is
 *               the old surface by definition; it is never renamed or re-cut). A tree
 *               that names no artifact, names more than one, or names one that is not there,
 *               refuses (exit 2) — the audit never runs without the engine it audits.
 *   --base-path the mount path the files are served under (default: the website's
 *               `/tools/cooling-tower/`; pass '' to serve at the root)
 *   --csp       the document policy the serving shape carries (default: the pinned resulting
 *               header; the mutations pass the website's current, token-less header or a wrong
 *               form here); the `unavailable` and `observe` legs serve --unavailable-csp instead
 *   --unavailable-csp  what the `unavailable`/`observe` legs serve (default: the pinned
 *               `CURRENT_SITE_CSP`, the header the site serves today). Pass the resulting policy
 *               here to prove the legs bite when the engine is allowed: the page then computes,
 *               so `unavailable` must go red and `observe` must record numbers.
 *   --target    drive an EXISTING target id (from /json/list) instead of creating one; use when
 *               this host can no longer spawn renderers (it will, loudly, tell you)
 *   --wasm-file <filename>  name the engine artifact under `vendor/engine` explicitly (the
 *               deployment job passes `cooling_tower_calculator.wasm` for the frozen v0.1.0
 *               surface). Without it the name is DERIVED from the tree under audit (see `--dir`);
 *               with it the run still refuses when the named file is missing, and it refuses a
 *               name the tree's own surface contradicts — the page fetched one artifact, and
 *               auditing another would make every rendered-number comparison meaningless.
 *   --out       directory for this run's JSON report, log and screenshots
 *
 * Exit status: 0 every assertion of every requested leg held; 1 an assertion failed (the leg's
 * report names each failure); 2 the audit could not run (no browser, no served plane, no
 * screenshot). Nothing is silently skipped.
 */

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import {
  CSP_TOKEN, CURRENT_SITE_CSP, DEFAULT_BASE_PATH, PRODUCTION_CSP, PRODUCTION_WASM_TYPE, startDeployServer,
} from './deploytest-serve.mjs';

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : process.argv[index + 1];
}

const servedDir = option('--dir', undefined);
if (servedDir === undefined) {
  console.error('deploytest-audit: --dir <served tree> is required — the tree being audited must be named (it is never guessed)');
  process.exit(2);
}
const servedRoot = resolve(servedDir);
const endpoint = option('--endpoint', process.env.DEPLOVTEST_CDP ?? process.env.DEPLOYTEST_CDP ?? 'http://127.0.0.1:9333').replace(/\/$/, '');
const targetId = option('--target', undefined);
const targetUrl = option('--target-url', undefined);
const timeoutMs = Number(option('--timeout-ms', '300000'));
const outDir = option('--out', undefined) === undefined ? undefined : resolve(option('--out'));
const label = option('--label', 'deployment');
const legFlag = option('--leg', 'both');
const basePath = option('--base-path', DEFAULT_BASE_PATH);
const csp = option('--csp', PRODUCTION_CSP);
const unavailableCsp = option('--unavailable-csp', CURRENT_SITE_CSP);
const wasmContentType = option('--wasm-content-type', PRODUCTION_WASM_TYPE);
const cachePolicy = option('--cache-policy', 'pinned');
const wasmFileName = option('--wasm-file', undefined);

const LEGS = legFlag === 'both' ? ['production', 'unavailable'] : [legFlag];
const LEG_NAMES = new Set(['production', 'unavailable', 'observe']);
if (LEGS.some((leg) => !LEG_NAMES.has(leg))) {
  console.error(`deploytest-audit: unknown --leg ${JSON.stringify(legFlag)}`);
  process.exit(2);
}

/** Which pinned host header a policy string is — or 'other' for a mutation the caller passed. */
const policyLabel = (policy) => (policy === PRODUCTION_CSP ? 'resulting' : policy === CURRENT_SITE_CSP ? 'current' : 'other');

const refuse = (message) => {
  console.error(`deploytest-audit: ${message}`);
  process.exit(2);
};

/**
 * The engine artifact this run audits, under `<dir>/vendor/engine`.
 *
 * The primary mechanism is the tree itself: its own top-level surface files name the wasm they
 * fetch (`vendor/engine/<name>.wasm`) and that name is used, never one baked in here. The trees
 * that reach this audit do not carry the same name: the current-era trees name
 * `synergy_drafthouse.wasm`, while the frozen v0.1.0 tree the deployment job materialises names
 * `cooling_tower_calculator.wasm` and is right to — that tree is the old surface by definition
 * and is never renamed or re-cut.
 *
 * `--wasm-file <filename>` names it explicitly instead (the deployment job passes it for the
 * frozen tree, so the leg says out loud which artifact it audits). It is a consistency check,
 * not a way around the facts: a bare `.wasm` file name is required, a name the tree's own
 * surface contradicts is refused (the page fetched a different artifact), and a name that is
 * not there is refused. Every refusal stays real at exit 2: the audit never runs without the
 * engine it audits.
 */
function engineArtifact(dir, explicit) {
  const named = new Map();          // <wasm file name> -> the surface file that names it
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
    if (!entry.isFile()) continue;
    for (const match of readFileSync(join(dir, entry.name), 'utf8').matchAll(/vendor\/engine\/([A-Za-z0-9._-]+\.wasm)/g)) {
      if (!named.has(match[1])) named.set(match[1], entry.name);
    }
  }

  if (explicit !== undefined) {
    if (!/^[A-Za-z0-9._-]+\.wasm$/.test(explicit)) {
      refuse(`--wasm-file ${JSON.stringify(explicit)} is not a bare .wasm file name under vendor/engine`);
    }
    const [surfaceName] = [...named.keys()];
    if (named.size === 1 && !named.has(explicit)) {
      refuse(`--wasm-file ${JSON.stringify(explicit)} contradicts the tree: the surface's ${named.get(surfaceName)} names ${JSON.stringify(surfaceName)} under vendor/engine; audit the artifact the surface fetches, or omit --wasm-file to derive it`);
    }
    const artifact = join(dir, 'vendor/engine', explicit);
    if (!existsSync(artifact)) refuse(`${artifact} is named by --wasm-file and is missing; there is no engine to audit`);
    return artifact;
  }

  if (named.size !== 1) {
    refuse(`${dir} names ${named.size === 0 ? 'no vendor/engine/*.wasm artifact' : `${named.size} engine artifacts (${[...named.keys()].join(', ')})`} in its own surface files; there is no engine to audit`);
  }
  const [name, source] = [...named.entries()][0];
  const artifact = join(dir, 'vendor/engine', name);
  if (!existsSync(artifact)) refuse(`${artifact} is named by the surface's ${source} and is missing; there is no engine to audit`);
  return artifact;
}

if (!existsSync(servedRoot) || !statSync(servedRoot).isDirectory()) refuse(`--dir ${servedRoot} is not a directory (expected the built published plane)`);
if (!existsSync(join(servedRoot, 'index.html'))) refuse(`${join(servedRoot, 'index.html')} is missing; there is no surface to audit`);
const engineWasm = engineArtifact(servedRoot, wasmFileName);

/* ---- the CDP client: no dependencies, Node's WebSocket does the protocol ---- */

const withTimeout = (promise, ms, what) => Promise.race([
  promise,
  new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} did not respond within ${ms} ms`)), ms)),
]);

if (typeof WebSocket === 'undefined') {
  refuse('this Node has no global WebSocket; the browser half needs Node 22 or newer (the hermetic half does not)');
}

let socketUrl;
let targetDescription;
let closeTarget = async () => {};
const cleanup = async () => {
  try { await closeTarget(); } catch {}
  try { websocket?.close(); } catch {}
};

try {
  if (targetId !== undefined || targetUrl !== undefined) {
    const list = await withTimeout(fetch(`${endpoint}/json/list`).then((response) => response.json()), 8000, 'the CDP target list');
    const found = targetId !== undefined
      ? list.find((target) => target.id === targetId)
      : list.find((target) => target.type === 'page' && (target.url ?? '').includes(targetUrl));
    if (!found) refuse(`no target found for ${targetId !== undefined ? `--target ${targetId}` : `--target-url ${targetUrl}`}`);
    socketUrl = found.webSocketDebuggerUrl;
    targetDescription = { reused: true, id: found.id, url: found.url };
  } else {
    const created = await withTimeout(fetch(`${endpoint}/json/new?about:blank`, { method: 'PUT' }).then((response) => response.json()), 15000, 'target creation');
    if (!created.webSocketDebuggerUrl) refuse(`the endpoint created a target without a debugger socket: ${JSON.stringify(created)}`);
    socketUrl = created.webSocketDebuggerUrl;
    targetDescription = { reused: false, id: created.id, url: created.url ?? 'about:blank' };
    closeTarget = () => fetch(`${endpoint}/json/close/${created.id}`).then(() => {});
  }
} catch (error) {
  await cleanup();
  refuse(`no usable CDP browser at ${endpoint} (${error.message}). Start one, e.g. "chrome-headless-shell --remote-debugging-port=9333", or pass --endpoint. If this host can no longer spawn renderers, reuse an existing one with --target <id> / --target-url <substring>. The audit refuses to run rather than skip silently`);
}

const websocket = new WebSocket(socketUrl);
await withTimeout(new Promise((done, fail) => {
  websocket.onopen = done;
  websocket.onerror = () => fail(new Error('the DevTools WebSocket could not be opened'));
}), 8000, 'the DevTools socket').catch(async (error) => {
  await cleanup();
  refuse(`${error.message} (${endpoint}); is the browser still running?`);
});

let sequence = 0;
const pending = new Map();
const pageRequests = [];            // Network.requestWillBeSent
const responses = new Map();        // requestId -> Network.responseReceived (url, status, mimeType, headers)
const lifecycles = [];              // Page.lifecycleEvent
const loadingFailed = [];
const consoleEvents = [];           // Runtime.consoleAPICalled + Runtime.exceptionThrown
let lastNetworkAt = Date.now();

websocket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const settle = pending.get(message.id);
    pending.delete(message.id);
    settle(message);
  }
  if (message.method === 'Network.requestWillBeSent') {
    pageRequests.push({ url: message.params.request.url, timestamp: message.params.timestamp, type: message.params.type });
    lastNetworkAt = Date.now();
  }
  if (message.method === 'Network.responseReceived') {
    responses.set(message.params.requestId, {
      url: message.params.response.url,
      status: message.params.response.status,
      mimeType: message.params.response.mimeType,
      headers: message.params.response.headers,
    });
  }
  if (message.method === 'Page.lifecycleEvent') lifecycles.push({ name: message.params.name, timestamp: message.params.timestamp });
  if (message.method === 'Network.loadingFailed' && !message.params.canceled) {
    loadingFailed.push({ requestId: message.params.requestId, errorText: message.params.errorText, blockedReason: message.params.blockedReason ?? null });
  }
  /* The console is where a CSP refusal shows up on a surface that has no labelled state — the
     observe leg records these, so "what does the reader get" is not only the DOM text. */
  if (message.method === 'Runtime.consoleAPICalled') {
    consoleEvents.push({ type: message.params.type, text: (message.params.args ?? []).map((arg) => String(arg.value ?? arg.description ?? '')).join(' ').slice(0, 500) });
  }
  if (message.method === 'Runtime.exceptionThrown') {
    consoleEvents.push({ type: 'exception', text: String(message.params.exceptionDetails?.exception?.description ?? message.params.exceptionDetails?.text ?? '').slice(0, 500) });
  }
};

const send = (method, params = {}, timeout = 20000) => withTimeout(new Promise((done, fail) => {
  const id = ++sequence;
  pending.set(id, (message) => (message.error ? fail(new Error(`${method}: ${message.error.message}`)) : done(message.result)));
  websocket.send(JSON.stringify({ id, method, params }));
}), timeout, method);

const evaluate = async (expression, timeout = 20000) => (await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: false }, timeout)).result.value;
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const progress = (message) => console.error(`deploytest-audit[${new Date().toISOString().slice(11, 19)}]: ${message}`);

const DUTY_KEYS = ['waterMassFlowKgS', 'hotWaterC', 'targetColdWaterC', 'wetBulbC', 'dryBulbC', 'pressurePa'];
const HEADLINE_KEYS = ['coldWaterC', 'thermalMarginC', 'electricalInputKW', 'heatTransferKW', 'totalPa', 'fanFlowM3S', 'fanShaftPowerKW', 'motorSelectedKW', 'capacityKgS', 'capabilityRatio', 'evaporationKgS', 'makeupKgS'];
const FIXED_LABELS = new Map([
  ['Water quality class', 'waterQualityClass'],
  ['Maximum drift', 'maxDriftPpm'],
  ['Maximum electrical input', 'maxElectricalInputKW'],
  ['Maximum footprint', 'maxFootprintM2'],
  ['Minimum thermal margin', 'minimumThermalMarginC'],
  ['Cycles of concentration', 'cyclesOfConcentration'],
  ['Salinity', 'salinityGKg'],
]);

/** The duty and the fixed ceilings, parsed from what the page DISPLAYS (no copy to drift). */
async function readRenderedRequirements() {
  return evaluate(`(() => {
    const requirements = {};
    const inputs = {};
    for (const key of ${JSON.stringify(DUTY_KEYS)}) {
      const node = document.getElementById('input-' + (${JSON.stringify({ waterMassFlowKgS: 'water', hotWaterC: 'hot', targetColdWaterC: 'target-cold', wetBulbC: 'wet-bulb', dryBulbC: 'dry-bulb', pressurePa: 'pressure' })}[key]));
      inputs[key] = node ? node.value : null;
    }
    const fixed = {};
    for (const row of document.querySelectorAll('.fixed-constraints li')) {
      const text = row.textContent.replace(/\\\\s+/g, ' ').trim();
      const label = text.split(':')[0].trim();
      const value = text.slice(label.length + 1).trim();
      fixed[label] = value;
    }
    return { inputs, fixed };
  })()`);
}

/** Map the page's displayed fixed-constraint rows onto the engine's requirement fields. */
function fixedRequirements(fixed, failures) {
  const requirements = {};
  for (const [label, field] of FIXED_LABELS) {
    const raw = fixed[label];
    if (raw === undefined) {
      failures.push(`requirements-from-page: the page displays no "${label}" row`);
      continue;
    }
    if (field === 'waterQualityClass') {
      requirements[field] = raw;
      continue;
    }
    const value = Number(raw.replace(/[^0-9.eE+-]/g, ''));
    if (!Number.isFinite(value)) {
      failures.push(`requirements-from-page: the page's "${label}" row (${JSON.stringify(raw)}) does not carry a number`);
      continue;
    }
    requirements[field] = value;
  }
  return requirements;
}

/** The engine's own answer, computed in Node from the SERVED bytes — the rendered page must agree. */
async function engineExpectation(requirements) {
  const binding = await import(pathToFileURL(join(servedRoot, 'vendor/engine/binding.mjs')).href);
  const wasmBytes = readFileSync(engineWasm);
  const catalog = JSON.parse(readFileSync(join(servedRoot, 'data/sample-catalog.json'), 'utf8'));
  const run = binding.createEngine(wasmBytes).select({ catalog, requirements, maxResults: 3 });
  return run;
}

/** Compare one rendered headline value with the engine's own, at the precision rendered. */
function renderedMatches(renderedText, engineValue) {
  if (renderedText === null || renderedText === '' || renderedText === '—') return { ok: false, why: `nothing rendered (${JSON.stringify(renderedText)})` };
  const rendered = Number(String(renderedText).replaceAll(',', ''));
  if (!Number.isFinite(rendered) || !Number.isFinite(engineValue)) return { ok: false, why: `rendered ${JSON.stringify(renderedText)} vs engine ${JSON.stringify(engineValue)}` };
  const decimals = String(renderedText).includes('.') ? String(renderedText).split('.')[1].length : 0;
  const tolerance = 0.5 * 10 ** -decimals;
  const delta = Math.abs(rendered - engineValue);
  return delta <= tolerance
    ? { ok: true }
    : { ok: false, why: `rendered ${renderedText} vs engine ${engineValue} (delta ${delta.toExponential(3)} > ${tolerance.toExponential(3)})` };
}

/* ---- one leg ---- */

async function runLeg(leg) {
  const failures = [];
  pageRequests.length = 0;
  responses.clear();
  lifecycles.length = 0;
  loadingFailed.length = 0;
  consoleEvents.length = 0;
  lastNetworkAt = Date.now();

  const legCsp = leg === 'unavailable' || leg === 'observe' ? unavailableCsp : csp;
  const server = await startDeployServer({ servedRoot, csp: legCsp, wasmContentType, cachePolicy, basePath });
  const started = Date.now();
  try {
    progress(`[${leg}] serving ${servedRoot} at ${server.url} (policy=${policyLabel(legCsp)} csp=${JSON.stringify(legCsp)} wasm=${JSON.stringify(wasmContentType)} cache=${JSON.stringify(cachePolicy)})`);
    await send('Network.enable', {}, 30000);
    await send('Page.enable');
    /* Required, or Page.lifecycleEvent never arrives and a pre/post-paint split passes vacuously. */
    await send('Page.setLifecycleEventsEnabled', { enabled: true });
    await send('Runtime.enable');
    await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });

    await send('Page.navigate', { url: `${server.url}/index.html` });

    const deadline = Date.now() + timeoutMs;
    let firstPaintAt = null;
    while (Date.now() < deadline) {
      const paints = lifecycles.filter((entry) => entry.name === 'firstPaint' || entry.name === 'firstContentfulPaint');
      if (paints.length > 0 && lifecycles.some((entry) => entry.name === 'load') && Date.now() - lastNetworkAt >= 300) {
        firstPaintAt = Math.min(...paints.map((entry) => entry.timestamp));
        break;
      }
      await sleep(100);
    }
    progress(`[${leg}] first paint ${firstPaintAt === null ? 'NOT SEEN' : 'seen'}; lifecycle: ${lifecycles.map((entry) => entry.name).join(', ') || '(none)'}`);
    if (firstPaintAt === null) failures.push({ id: 'first-paint-before-run', detail: `the page did not paint and finish loading within ${timeoutMs} ms — the run is not triggered, so a pre/post-paint split cannot pass vacuously` });

    const before = await evaluate(`({
      readyState: document.readyState,
      title: document.title,
      engine: document.body.dataset.engine ?? null,
      resultHidden: document.getElementById('result-region')?.hidden ?? null,
      unavailableHidden: document.getElementById('engine-unavailable')?.hidden ?? null,
      runState: document.getElementById('run-state')?.textContent.trim() ?? null,
    })`);

    let rendered = null;
    let engineRun = null;
    let observation = null;
    if (firstPaintAt !== null) {
      progress(`[${leg}] setting the displayed duty and running the selection`);
      await evaluate(`(() => {
        const values = ${JSON.stringify({ waterMassFlowKgS: '200', hotWaterC: '42', targetColdWaterC: '32', wetBulbC: '27', dryBulbC: '33', pressurePa: '101325' })};
        const ids = ${JSON.stringify({ waterMassFlowKgS: 'input-water', hotWaterC: 'input-hot', targetColdWaterC: 'input-target-cold', wetBulbC: 'input-wet-bulb', dryBulbC: 'input-dry-bulb', pressurePa: 'input-pressure' })};
        for (const [key, value] of Object.entries(values)) document.getElementById(ids[key]).value = value;
        document.getElementById('run-selection').click();
        return 'clicked';
      })()`);

      let terminal = null;
      while (Date.now() < deadline) {
        try {
          terminal = await evaluate(`({
            engine: document.body.dataset.engine ?? null,
            stage: document.body.dataset.stage ?? null,
            stageFallback: Boolean(document.querySelector('#stage .stage-fallback')),
            resultHidden: document.getElementById('result-region')?.hidden ?? null,
            coldWater: document.getElementById('metric-coldWaterC')?.textContent ?? null,
            state: document.getElementById('run-state')?.textContent.trim() ?? null,
            tone: document.getElementById('run-state')?.dataset.tone ?? null,
          })`, Math.max(30000, deadline - Date.now()));
        } catch (error) {
          terminal = { error: String(error.message ?? error) };
        }
        progress(`[${leg}] poll: engine=${terminal?.engine ?? '—'} stage=${terminal?.stage ?? '—'} result=${terminal?.resultHidden === false ? 'shown' : 'hidden'}`);
        if (terminal?.engine === 'unavailable') break;
        if (terminal?.stage === 'ready' || terminal?.stageFallback === true) break;
        /* A terminal error status is as terminal as 'ready' — a surface whose engine-refusal path
           has no labelled state ends there, and the observe leg records it rather than waiting
           out the deadline. */
        if (terminal?.tone === 'error') break;
        /* A serving shape the browser already refused cannot be rescued by waiting: stop as soon
           as the wasm response is wrong, or as soon as a subresource was blocked (a policy the
           page cannot run under at all — the wrong-header probe). */
        const wasmResponse = [...responses.values()].find((response) => response.url.endsWith('.wasm'));
        if (wasmResponse && (wasmResponse.status !== 200 || wasmResponse.mimeType !== PRODUCTION_WASM_TYPE)) break;
        if (loadingFailed.some((failure) => failure.blockedReason !== null)) break;
        await sleep(1000);
      }

      rendered = await evaluate(`(() => {
        const text = (id) => { const node = document.getElementById(id); return node ? node.textContent.trim() : null; };
        const metrics = {};
        for (const key of ${JSON.stringify(HEADLINE_KEYS)}) metrics[key] = text('metric-' + key);
        const canvas = document.getElementById('stage-canvas');
        const context = canvas ? (canvas.getContext('webgl2') || canvas.getContext('webgl')) : null;
        const unavailable = document.getElementById('engine-unavailable');
        return {
          engine: document.body.dataset.engine ?? null,
          stage: document.body.dataset.stage ?? null,
          resultHidden: document.getElementById('result-region')?.hidden ?? null,
          unavailablePresent: unavailable !== null,
          unavailableHidden: unavailable?.hidden ?? null,
          unavailableText: String(unavailable?.innerText ?? '').trim(),
          resultText: String(document.getElementById('result-region')?.innerText ?? '').trim().slice(0, 2000),
          runState: document.getElementById('run-state')?.textContent.trim() ?? null,
          runStateTone: document.getElementById('run-state')?.dataset.tone ?? null,
          metrics,
          identity: document.getElementById('result-identity')?.innerText.trim() ?? '',
          steps: document.querySelectorAll('#worked-steps .step').length,
          stepsSummary: document.getElementById('steps-summary')?.innerText ?? null,
          webgl: Boolean(context),
          bodyText: document.body.innerText,
          formPresent: Boolean(document.getElementById('duty-form')),
        };
      })()`, 60000);
    }

    /* The requirements the page displays — the engine run below uses exactly those. The observe
       leg does not read requirements: it asserts nothing about the engine's agreement, it records
       what the page did under a policy that refuses wasm compilation. */
    const requirements = {};
    if (leg !== 'observe') {
      const displayed = await readRenderedRequirements();
      Object.assign(requirements, fixedRequirements(displayed.fixed, failures));
      for (const key of DUTY_KEYS) {
        const value = Number(displayed.inputs[key]);
        if (!Number.isFinite(value)) failures.push({ id: 'requirements-from-page', detail: `the page's ${key} input is ${JSON.stringify(displayed.inputs[key])}` });
        else requirements[key] = value;
      }
    }

    /* ---- assertions ---- */

    const assertions = [];
    const assertThat = (id, ok, detail) => assertions.push({ id, ok, detail });
    const wasmResponse = [...responses.values()].find((response) => response.url.endsWith('.wasm'));
    const firstEngineRequest = pageRequests.find((request) => /\/(vendor\/engine|vendor\/visuals)|three[^/]*\.js/.test(request.url));
    const srcRequests = pageRequests.filter((request) => /\/src\//.test(new URL(request.url).pathname ?? ''));
    const documentResponse = [...responses.values()].find((response) => response.url.endsWith('/index.html'));
    const documentCsp = documentResponse?.headers?.['content-security-policy'] ?? null;

    assertThat('first-paint-before-run', firstPaintAt !== null && (firstEngineRequest === undefined || firstEngineRequest.timestamp >= firstPaintAt),
      firstPaintAt === null ? 'no first paint' : `engine fetched ${firstEngineRequest === undefined ? '(never)' : 'after first paint'}`);

    if (leg === 'production') {
      assertThat('serving-csp', documentCsp !== null && documentCsp.includes(CSP_TOKEN), `the document was served with content-security-policy ${JSON.stringify(documentCsp)}`);
      assertThat('serving-mime-wasm', Boolean(wasmResponse) && wasmResponse.status === 200 && wasmResponse.mimeType === PRODUCTION_WASM_TYPE,
        wasmResponse === undefined ? 'the browser never received the wasm module' : `wasm response ${wasmResponse.status} ${JSON.stringify(wasmResponse.mimeType)}`);
      assertThat('reference-not-requested', srcRequests.length === 0, srcRequests.length ? `src/** was requested: ${srcRequests.map((request) => request.url).join(', ')}` : 'the JavaScript reference is not on this page\'s execution path');
      assertThat('engine-instantiated', rendered?.engine === 'wasm-instantiated', `dataset.engine=${JSON.stringify(rendered?.engine)}`);
      const numbersPresent = rendered !== null && rendered.resultHidden === false && rendered.unavailableHidden !== false
        && Object.values(rendered.metrics).every((value) => value !== null && value !== '' && value !== '—');
      assertThat('result-rendered', numbersPresent, rendered === null ? 'no page read' : `resultHidden=${rendered.resultHidden} unavailableHidden=${rendered.unavailableHidden} metrics=${JSON.stringify(rendered.metrics)}`);
      assertThat('stage-or-fallback', rendered === null ? false : (rendered.webgl ? rendered.stage === 'ready' : String(rendered.bodyText).includes('The 3D view is unavailable here')),
        rendered === null ? 'no page read' : `webgl=${rendered.webgl} stage=${JSON.stringify(rendered.stage)}`);
      assertThat('no-unanswered-requests', server.requests.every((request) => request.status < 400), server.requests.filter((request) => request.status >= 400).map((request) => `${request.status} ${request.path}`).join(', ') || 'every request answered');

      if (rendered !== null && numbersPresent !== true) {
        failures.push({ id: 'result-rendered', detail: 'the numbers were not all rendered; the engine cross-check was not reached' });
      }
      if (rendered?.engine === 'wasm-instantiated') {
        try {
          engineRun = await engineExpectation(requirements);
        } catch (error) {
          failures.push({ id: 'engine-cross-check', detail: `the served engine could not be run in Node: ${error.message}` });
        }
        if (engineRun !== null && engineRun.ok !== true) {
          failures.push({ id: 'engine-cross-check', detail: `the served engine refused the page's own duty: status ${engineRun.status} ${engineRun.error?.message}` });
        } else if (engineRun !== null) {
          const mismatches = [];
          for (const key of HEADLINE_KEYS) {
            const comparison = renderedMatches(rendered.metrics[key], engineRun.metrics[key]);
            if (!comparison.ok) mismatches.push(`${key}: ${comparison.why}`);
          }
          for (const [field, metric] of [['towerId', 'towerId'], ['fillId', 'fillId'], ['driftEliminatorId', 'driftEliminatorId'], ['fanId', 'fanId'], ['nozzleId', 'nozzleId']]) {
            if (!String(rendered.identity).includes(String(engineRun.metrics[metric]))) mismatches.push(`identity ${field}: ${JSON.stringify(engineRun.metrics[metric])} is not in the rendered identity`);
          }
          if (rendered.steps !== (engineRun.worked?.steps?.length ?? -1)) mismatches.push(`steps: rendered ${rendered.steps} vs engine ${engineRun.worked?.steps?.length}`);
          assertThat('numbers-are-the-engines', mismatches.length === 0, mismatches.join(' | ') || `all ${HEADLINE_KEYS.length} rendered numbers equal the served engine's own values for the displayed duty`);
          for (const mismatch of mismatches) failures.push({ id: 'numbers-are-the-engines', detail: mismatch });
        }
      }
    }

    if (leg === 'unavailable') {
      assertThat('serving-csp-token-absent', documentCsp !== null && !documentCsp.includes(CSP_TOKEN), `the document was served with content-security-policy ${JSON.stringify(documentCsp)}`);
      const explicit = rendered !== null && rendered.engine === 'unavailable' && rendered.unavailableHidden === false
        && String(rendered.unavailableText).length > 40
        && /engine/i.test(rendered.unavailableText)
        && /(could not start|unavailable|blocked)/i.test(rendered.unavailableText);
      assertThat('unavailable-explicit', explicit, rendered === null ? 'no page read' : `engine=${JSON.stringify(rendered.engine)} hidden=${rendered.unavailableHidden} text=${JSON.stringify(String(rendered.unavailableText).slice(0, 160))}`);
      assertThat('not-blank', rendered !== null && rendered.formPresent === true && String(rendered.bodyText).trim().length > 400 && rendered.runState !== '',
        rendered === null ? 'no page read' : `form=${rendered.formPresent} bodyText=${String(rendered.bodyText).trim().length} chars`);
      const noNumbers = rendered !== null && rendered.resultHidden === true
        && Object.values(rendered.metrics).every((value) => value === null)
        && !String(rendered.bodyText).includes('Cold water achieved');
      assertThat('no-numbers', noNumbers, rendered === null ? 'no page read' : `resultHidden=${rendered.resultHidden} metrics=${JSON.stringify(rendered.metrics)}`);
      assertThat('explicit-not-initial', rendered !== null && /(could not|unavailable)/i.test(rendered.runState) && !/Nothing computed yet/.test(rendered.runState),
        `run-state: ${JSON.stringify(String(rendered?.runState ?? '').slice(0, 160))}`);

      /* A second run must not leave a stale number behind: run again and re-assert. */
      let repeat = null;
      if (rendered?.engine === 'unavailable') {
        await evaluate(`document.getElementById('run-selection').click(); 'clicked again'`);
        await sleep(3000);
        repeat = await evaluate(`({
          engine: document.body.dataset.engine ?? null,
          resultHidden: document.getElementById('result-region').hidden,
          metrics: ${JSON.stringify(HEADLINE_KEYS)}.map((key) => document.getElementById('metric-' + key)?.textContent ?? null),
          bodyText: document.body.innerText,
        })`);
        const stillNoNumbers = repeat.resultHidden === true && repeat.metrics.every((value) => value === null) && !String(repeat.bodyText).includes('Cold water achieved');
        assertThat('repeat-run-no-numbers', repeat.engine === 'unavailable' && stillNoNumbers, `engine=${JSON.stringify(repeat.engine)} resultHidden=${repeat.resultHidden} metrics=${JSON.stringify(repeat.metrics)}`);
      }
    }

    if (leg === 'observe') {
      /* This leg RECORDS, it does not bless: the only assertions are that the observation is
         meaningful — the page painted, the document carried the policy named on the command line,
         and something was read back from the rendered page. What the page then did is classified
         mechanically and reported (see `observation`). */
      assertThat('observe-first-paint', firstPaintAt !== null, firstPaintAt === null ? 'the page did not paint within the deadline; nothing could be observed' : 'the page painted before the run');
      assertThat('observe-served-policy', documentCsp === legCsp, `the document was served with content-security-policy ${JSON.stringify(documentCsp)} — the observation must name the policy it was made under (${JSON.stringify(legCsp)})`);
      assertThat('observe-read', rendered !== null && String(rendered.bodyText ?? '').trim().length > 0, rendered === null ? 'no page read' : `bodyText ${String(rendered.bodyText ?? '').trim().length} chars`);

      const numbersRendered = rendered !== null && rendered.resultHidden === false
        && Object.values(rendered.metrics ?? {}).some((value) => value !== null && value !== '' && value !== '—');
      const labelledUnavailableState = rendered !== null && rendered.unavailablePresent === true && rendered.unavailableHidden === false && String(rendered.unavailableText).length > 40;
      const errorLine = rendered !== null && /(could not|unavailable|blocked|error)/i.test(String(rendered.runState ?? ''));
      const blankPage = rendered === null || rendered.formPresent !== true || String(rendered.bodyText ?? '').trim().length < 400;
      observation = {
        classification: numbersRendered ? 'numbers'
          : labelledUnavailableState ? 'labelled-unavailable-state'
            : errorLine ? 'raw-error-line'
              : blankPage ? 'blank-page' : 'no-visible-change',
        numbersRendered,
        labelledUnavailableState,
        errorLine,
        blankPage,
        runState: rendered?.runState ?? null,
        runStateTone: rendered?.runStateTone ?? null,
        resultHidden: rendered?.resultHidden ?? null,
        resultText: rendered?.resultText ?? null,
        unavailablePresent: rendered?.unavailablePresent ?? null,
        unavailableVisible: rendered === null ? null : (rendered.unavailablePresent === true && rendered.unavailableHidden === false),
        metrics: rendered?.metrics ?? null,
        formPresent: rendered?.formPresent ?? null,
        bodyTextChars: rendered === null ? null : String(rendered.bodyText).trim().length,
        wasmResponse: wasmResponse === undefined ? null : { status: wasmResponse.status, mimeType: wasmResponse.mimeType },
        consoleMessages: consoleEvents.slice(-20),
      };
    }

    for (const entry of assertions) if (entry.ok !== true) failures.push(entry);

    if (outDir !== undefined && rendered !== null) {
      mkdirSync(outDir, { recursive: true });
      const height = Math.min(await evaluate('document.documentElement.scrollHeight'), 8000);
      await send('Emulation.setDeviceMetricsOverride', { width: 1440, height, deviceScaleFactor: 1, mobile: false });
      await sleep(500);
      const shot = (await send('Page.captureScreenshot', { format: 'png' }, 60000)).data;
      writeFileSync(join(outDir, `${label}-${leg}.png`), Buffer.from(shot, 'base64'));
      await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });
      progress(`[${leg}] screenshot written to ${join(outDir, `${label}-${leg}.png`)}`);
    }

    const report = {
      leg,
      passed: failures.length === 0,
      legCsp,
      policy: policyLabel(legCsp),
      serving: { servedRoot, csp, wasmContentType, cachePolicy, basePath, origin: server.origin, url: server.url },
      endpoint,
      target: targetDescription,
      wallClockMs: Date.now() - started,
      firstPaintSeen: firstPaintAt !== null,
      loadingFailed,
      consoleMessages: consoleEvents.slice(-20),
      before,
      requests: pageRequests.map((entry) => ({
        phase: firstPaintAt !== null && entry.timestamp < firstPaintAt ? 'pre-paint' : 'post-paint',
        url: entry.url.replace(server.origin, ''),
        type: entry.type,
      })),
      wasmResponse: wasmResponse ?? null,
      documentCsp,
      serverLog: server.requests.map((request) => `${request.status} ${request.path} (${request.bytes} bytes)`),
      rendered: rendered === null ? null : {
        engine: rendered.engine,
        stage: rendered.stage,
        resultHidden: rendered.resultHidden,
        unavailablePresent: rendered.unavailablePresent,
        unavailableHidden: rendered.unavailableHidden,
        unavailableText: rendered.unavailableText,
        resultText: rendered.resultText,
        runState: rendered.runState,
        metrics: rendered.metrics,
        identity: rendered.identity,
        steps: rendered.steps,
        webgl: rendered.webgl,
      },
      requirements,
      engineCrossCheck: engineRun === null ? null : { ok: engineRun.ok, towerId: engineRun.metrics?.towerId ?? null, steps: engineRun.worked?.steps?.length ?? null },
      observation,
      assertions,
      failures,
    };
    return report;
  } finally {
    await server.close();
  }
}

/* ---- run the requested legs ---- */

const reports = [];
try {
  for (const leg of LEGS) reports.push(await runLeg(leg));
} catch (error) {
  await cleanup();
  refuse(`the browser could not be driven at ${endpoint}: ${error.message} — a host whose browser can no longer spawn renderers should reuse an existing target (--target <id> / --target-url <substring>)`);
}

if (targetDescription.reused && targetDescription.url === 'about:blank') {
  /* Leave a reused scratch target as it was found. */
  try { await send('Page.navigate', { url: 'about:blank' }); } catch {}
}
await cleanup();

const passed = reports.every((report) => report.passed);
const payload = reports.length === 1 ? reports[0] : reports;
if (outDir !== undefined) {
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, `${label}-audit.json`), `${JSON.stringify(payload, null, 2)}\n`);
}
if (passed) console.log(JSON.stringify(payload, null, 2));
else console.error(JSON.stringify(payload, null, 2));
process.exitCode = passed ? 0 : 1;
// An unref'd watchdog: nothing should keep this alive, but a wedged socket must not hang a gate.
setTimeout(() => process.exit(process.exitCode ?? 0), 3000).unref();
