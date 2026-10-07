#!/usr/bin/env node
/**
 * Browser-observed deploy audit (issue #20 fix round 4, G3; re-pointed at the cockpit plane by
 * issue #60 — the plane the host has served since #58).
 *
 * The static deploy check derives the surface's served set from its document, its modules and the
 * plane's own directories; four review rounds showed that each enumeration of "the syntaxes a
 * browser fetches" closes the last shape and invites the next one nobody listed. This audit
 * measures the browser itself
 * instead: it serves a copy-deploy of `deploy-manifest.txt` — the manifest's files, plus the
 * plane's built module (`pkg/**`, a build product the manifest does not record) — drives a real
 * browser at it over CDP, records every request the browser makes,
 * and fails if any of them is unanswered (a 404 means the manifest omitted a file the page really
 * loads, whatever syntax asked for it). It cannot be defeated by an unlisted syntax because it
 * never predicts one.
 *
 * The manifest names the plane's files at their repository paths (`cockpit/…`); the served
 * root is that directory (the private deployment procedure §3), so the copy places each file at the path the
 * host publishes and the audit drives `/index.html`. The deep drive — the pinned pieces at a
 * consumer's served paths, under the production headers — is `deploytest-audit.mjs`; this audit
 * is the manifest-and-built-module copy of the plane's own load.
 *
 * Usage: node scripts/deploy-audit.mjs [--site <dir>] [--endpoint <url>]
 *                                     [--timeout-ms <n>] [--quiet-ms <n>]
 *   --site      audit an existing directory instead of building the manifest-only copy
 *   --endpoint  CDP HTTP endpoint to attach to (default $DEPLOY_AUDIT_CDP or 127.0.0.1:9333)
 *   --target       drive an EXISTING target id (from /json/list) instead of creating one; use
 *                  this on a host whose browser can no longer spawn renderers
 *   --target-url   like --target, selecting the first page target whose URL contains the text
 *
 * Exit status: 0 every request the page makes is answered (and the page finished loading);
 * 1 a request the copy could not answer, or the page did not finish loading; 2 the audit could
 * not run (no reachable browser, no manifest, no site). Nothing is silently skipped: a missing
 * browser is a loud refusal.
 *
 * The contract's subject is what the PAGE fetches. The user agent's own implicit
 * `GET /favicon.ico` — sent by the browser, not by anything in the document; CI's full Chrome
 * makes it where a headless shell does not (issue #60 fix round 2) — is outside the contract,
 * but ONLY while the served document itself references no icon (`<link rel=… icon>`, an
 * `og:image` meta): the moment the document declares one, that request is the page's own and
 * an unanswered one fails the audit like any other.
 *
 * Where it runs: anywhere with a CDP-reachable Chromium (a local
 * `chrome-headless-shell --remote-debugging-port=9333`, or another endpoint via --endpoint).
 * It is deliberately NOT part of `npm run validate`: that suite stays hermetic and browser-free,
 * and a CI runner without a browser is no place to fish for one. Cost on this host: one
 * headless-shell process, one ephemeral port, ~3-15 s wall clock per run (load event plus a
 * quiet period), no dependencies — Node's global WebSocket does the protocol (Node 22+).
 *
 * Zero page dependencies: the site is served with Node's built-in http module and plain files.
 */

import { createServer } from 'node:http';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';

import { root } from './bundle.mjs';

const MANIFEST = 'deploy-manifest.txt';
const SURFACE = 'cockpit';
const failures = [];

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : process.argv[index + 1];
}

const endpoint = option('--endpoint', process.env.DEPLOY_AUDIT_CDP ?? 'http://127.0.0.1:9333').replace(/\/$/, '');
const givenSite = option('--site', undefined);
const givenTarget = option('--target', undefined);
const givenTargetUrl = option('--target-url', undefined);
const timeoutMs = Number(option('--timeout-ms', '20000'));
const quietMs = Number(option('--quiet-ms', '750'));

const refuse = (message) => {
  console.error(`deploy-audit: ${message}`);
  process.exit(2);
};

/* ---- the copy-deploy: the manifest's files and nothing else, or a given site directory ---- */

/** Copy a directory tree, file by file. */
function copyDirectory(source, destination) {
  mkdirSync(destination, { recursive: true });
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    const from = join(source, entry.name);
    const to = join(destination, entry.name);
    if (entry.isDirectory()) copyDirectory(from, to);
    else if (entry.isFile()) copyFileSync(from, to);
  }
}

let site;
let temporarySite = false;
if (givenSite !== undefined) {
  site = resolve(givenSite);
  if (!existsSync(site) || !statSync(site).isDirectory()) refuse(`--site ${givenSite} is not a directory`);
} else {
  if (!existsSync(join(root, MANIFEST))) refuse(`${MANIFEST} is missing; the audit has nothing to deploy`);
  // `<path>` or `<path>  <sha256>  <bytes>`: the path is the first column, and it is the file's
  // repository path. The served root is the surface's own directory, so the copy strips that
  // prefix — where the surface lives in the repository is not where it is served.
  const entries = readFileSync(join(root, MANIFEST), 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#'))
    .map((line) => line.split(/\s{2,}/)[0]);
  site = mkdtempSync(join(tmpdir(), 'deploy-audit-'));
  temporarySite = true;
  for (const entry of entries) {
    const source = join(root, entry);
    if (!existsSync(source) || !statSync(source).isFile()) refuse(`${MANIFEST} lists ${entry}, which is not a file on disk`);
    if (!entry.startsWith(`${SURFACE}/`)) refuse(`${MANIFEST} lists ${entry}, which is not part of the ${SURFACE}/ surface the audit deploys`);
    const served = entry.slice(SURFACE.length + 1);
    const destination = join(site, served);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(source, destination);
  }
  // The plane's built module: the document boots `pkg/**`, which is a build product the manifest
  // does not record (issue #60 — a committed manifest cannot verify bytes a build produces; the
  // plane artifact's own manifest covers them). The copy must carry it, or the page cannot boot
  // and every boot request would read as an unanswered request — a report about the copy, not
  // about the deploy set.
  const built = join(root, SURFACE, 'pkg');
  if (!existsSync(built) || !statSync(built).isDirectory()) {
    rmSync(site, { recursive: true, force: true });
    refuse(`${SURFACE}/pkg is not built; the audit drives the plane's document, which boots it — build it first (cockpit/tools/build-web.sh release)`);
  }
  copyDirectory(built, join(site, 'pkg'));
}

/* ---- a logging static server: the request log is the ground truth ---- */

const CONTENT_TYPES = new Map([
  ['.html', 'text/html; charset=utf-8'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.mjs', 'text/javascript; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.webmanifest', 'application/manifest+json'],
  ['.svg', 'image/svg+xml'],
  ['.png', 'image/png'],
  ['.jpg', 'image/jpeg'],
  ['.jpeg', 'image/jpeg'],
  ['.gif', 'image/gif'],
  ['.ico', 'image/x-icon'],
  ['.woff2', 'font/woff2'],
  ['.txt', 'text/plain; charset=utf-8'],
  ['.md', 'text/plain; charset=utf-8'],
]);

const requests = [];
let lastRequestAt = 0;
/* The bytes the audit actually served as the document — the scoping rule below reads them, so
   it follows the copy the browser saw, never the repository file. */
let documentBytes = Buffer.alloc(0);
const server = createServer((request, response) => {
  const path = decodeURIComponent((request.url ?? '/').split('?')[0]);
  const file = join(site, path.endsWith('/') ? `${path}index.html` : path);
  let status = 200;
  let body;
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    status = 405;
    body = Buffer.from('method not allowed\n');
  } else if ((file !== site && !file.startsWith(site + sep)) || !existsSync(file) || !statSync(file).isFile()) {
    status = 404;
    body = Buffer.from('not in this copy\n');
  } else {
    body = readFileSync(file);
    if (path === '/index.html' || path === '/' || path === '') documentBytes = body;
  }
  requests.push({ method: request.method ?? 'GET', path, status, bytes: body.length });
  lastRequestAt = Date.now();
  response.writeHead(status, { 'content-type': CONTENT_TYPES.get(file.slice(file.lastIndexOf('.'))) ?? 'application/octet-stream' });
  response.end(body);
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const origin = `http://127.0.0.1:${server.address().port}`;

/* ---- drive a real browser over CDP: no dependencies, Node's WebSocket does the protocol ---- */

const withTimeout = (promise, ms, label) => Promise.race([
  promise,
  new Promise((_, reject) => setTimeout(() => reject(new Error(`${label} did not respond within ${ms} ms`)), ms)),
]);

const cleanup = async () => {
  try { await closeTarget(); } catch {}
  try { websocket?.close(); } catch {}
  server.closeAllConnections?.();
  await new Promise((done) => server.close(done));
  if (temporarySite) rmSync(site, { recursive: true, force: true });
};

if (typeof WebSocket === 'undefined') {
  await cleanup();
  refuse('this Node has no global WebSocket; the audit needs Node 22 or newer');
}

let version;
try {
  version = await withTimeout(fetch(`${endpoint}/json/version`).then((response) => response.json()), 5000, 'the CDP endpoint');
} catch (error) {
  await cleanup();
  refuse(`no CDP browser reachable at ${endpoint} (${error.message}). Start one, e.g. "chrome-headless-shell --remote-debugging-port=9333", or pass --endpoint / set DEPLOY_AUDIT_CDP. The audit refuses to run rather than skip silently`);
}

// The target: a fresh one, or an EXISTING one when the host's browser can no longer spawn
// renderers (`--target <id>` / `--target-url <substring>`, the escape hatch `deploytest-audit.mjs`
// documents). A reused target is driven through its own debugger socket and is never closed.
let websocket;
let session;
let targetId;
let closeTarget = async () => {};
if (givenTarget !== undefined || givenTargetUrl !== undefined) {
  let list;
  try {
    list = await withTimeout(fetch(`${endpoint}/json/list`).then((response) => response.json()), 8000, 'the CDP target list');
  } catch (error) {
    await cleanup();
    refuse(`the CDP target list could not be read at ${endpoint} (${error.message})`);
  }
  const found = givenTarget !== undefined
    ? list.find((target) => target.id === givenTarget)
    : list.find((target) => target.type === 'page' && (target.url ?? '').includes(givenTargetUrl));
  const reuseLabel = givenTarget !== undefined ? `--target ${givenTarget}` : `--target-url ${givenTargetUrl}`;
  if (!found || !found.webSocketDebuggerUrl) {
    await cleanup();
    refuse(`no target found for ${reuseLabel} at ${endpoint}; list them with "curl -s ${endpoint}/json/list"`);
  }
  targetId = found.id;
  websocket = new WebSocket(found.webSocketDebuggerUrl);
} else {
  websocket = new WebSocket(version.webSocketDebuggerUrl);
}
await withTimeout(new Promise((done, fail) => {
  websocket.onopen = done;
  websocket.onerror = () => fail(new Error('the DevTools WebSocket could not be opened'));
}), 5000, 'the DevTools socket').catch(async (error) => {
  await cleanup();
  refuse(`${error.message} (${endpoint}); is the browser still running?`);
});

let sequence = 0;
const pending = new Map();
const loadingFailed = [];
const requestedUrls = new Map();
let loadEventFired = false;
websocket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const settle = pending.get(message.id);
    pending.delete(message.id);
    settle(message);
  }
  if (message.method === 'Page.loadEventFired') loadEventFired = true;
  if (message.method === 'Network.requestWillBeSent') requestedUrls.set(message.params.requestId, message.params.request.url);
  if (message.method === 'Network.loadingFailed' && !message.params.canceled) {
    loadingFailed.push({ url: requestedUrls.get(message.params.requestId) ?? '?', errorText: message.params.errorText });
  }
};
const send = (method, params = {}, sessionId) => withTimeout(new Promise((done, fail) => {
  const id = ++sequence;
  pending.set(id, (message) => message.error ? fail(new Error(message.error.message)) : done(message.result));
  websocket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
}), 15000, method);

if (givenTarget === undefined && givenTargetUrl === undefined) {
  // Only a target this audit created is closed again; a reused one belongs to whoever opened it.
  closeTarget = async () => {
    if (targetId !== undefined) await send('Target.closeTarget', { targetId });
  };
}
try {
  if (targetId === undefined) {
    const created = await send('Target.createTarget', { url: 'about:blank' });
    targetId = created.targetId;
    const attached = await send('Target.attachToTarget', { targetId, flatten: true });
    session = attached.sessionId;
  }
  await send('Network.enable', {}, session);
  await send('Page.enable', {}, session);
  await send('Runtime.enable', {}, session);
  await send('Page.navigate', { url: `${origin}/index.html` }, session);

  // Settle: the load event, then a quiet period with no new server requests; both bounded.
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await new Promise((done) => setTimeout(done, 100));
    if (loadEventFired && Date.now() - lastRequestAt >= quietMs) break;
  }

  const page = (await send('Runtime.evaluate', {
    expression: '({ readyState: document.readyState, title: document.title })',
    returnByValue: true,
  }, session)).result.value;

  cleanup();

  if (!loadEventFired) failures.push(`the page did not finish loading within ${timeoutMs} ms`);
  if (page.readyState !== 'complete') failures.push(`the document is ${page.readyState} when the audit closed`);
  /* The contract's subject is what the PAGE fetches. The user agent also asks for /favicon.ico
     on its own initiative — no browser convention is part of any deploy set, and CI's full
     Chrome makes the request where a headless shell does not (issue #60 fix round 2) — so that
     one request is outside the contract, but ONLY while the served document itself references
     no icon: the moment the document declares one (a <link rel=icon>, or an og:image meta), the
     request is the page's own and must be answered like any other. */
  const documentDeclaresIcon = /<link[^>]*\brel=[^>]*\bicon/i.test(documentBytes)
    || /<meta[^>]*\bproperty=[^>]*og:image/i.test(documentBytes);
  const uaImplicit = (request) => request.method === 'GET' && request.path === '/favicon.ico';
  const unanswered = requests.filter((request) => request.status >= 400
    && !(!documentDeclaresIcon && uaImplicit(request)));
  for (const request of unanswered) failures.push(`unanswered: ${request.method} ${request.path} -> ${request.status}`);

  const summary = {
    passed: failures.length === 0,
    site,
    origin,
    target: { id: targetId, reused: givenTarget !== undefined || givenTargetUrl !== undefined },
    document: page,
    requestCount: requests.length,
    answers: Object.fromEntries([...new Set(requests.map((request) => request.status))].sort().map((status) => [status, requests.filter((request) => request.status === status).length])),
    requests: requests.map((request) => `${request.status} ${request.path}`),
    loadingFailed,
    failures,
  };
  if (summary.passed) console.log(JSON.stringify(summary, null, 2));
  else console.error(JSON.stringify(summary, null, 2));
  process.exitCode = summary.passed ? 0 : 1;
} catch (error) {
  await cleanup();
  refuse(`the browser could not be driven at ${endpoint}: ${error.message}`);
}
// An unref'd watchdog: nothing should keep this alive, but a wedged socket must not hang a gate.
setTimeout(() => process.exit(process.exitCode ?? 0), 3000).unref();
