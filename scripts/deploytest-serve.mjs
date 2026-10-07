#!/usr/bin/env node
/**
 * Production-shaped serving of a built published surface, and its pin/cache contract —
 * issue #38, slice 2 of #35.
 *
 * The served root is the surface's directory: by default the product UI's plane (`cockpit/` —
 * `index.html`, `pkg/**`, `assets/**`: the directory `scripts/serve.mjs` serves and the release
 * publishes), and any `--dir` for the sorted tree a caller names — a consumer deployment with
 * the pinned pieces placed at their served paths (`scripts/bundle.mjs` `servedPath`), or the
 * frozen tag-era surface (`git archive` of the v0.1.0 tag). The pin identity a check asserts is
 * the audited tree's own (`--dir`'s `bundles/*.manifest.json`, when that tree carries one) —
 * never the current tree's, whose digests a frozen surface can never satisfy. Nothing here
 * serves the JavaScript reference (`src/**`) — that tree is not part of any deploy plane, which
 * is the whole point of the deployment-shaped tests.
 *
 * This module is the one definition of "the way the production host would serve it":
 *
 *   · the host's Content-Security-Policy on documents, **verbatim** — both strings are pinned in
 *     `tests/fixtures/site-policy.json`, derived from the website repository's
 *     `static/staticwebapp.config.json` at commit 4937a1e (the approved change adds one token to
 *     `script-src`). `PRODUCTION_CSP` is the resulting header, `CURRENT_SITE_CSP` is what the site
 *     serves today; removing the token from the resulting header reproduces the current header
 *     byte-for-byte, so the token-absent leg serves the site's real header, not a paraphrase;
 *   · the deploy path `/tools/cooling-tower/` (the website serves plain static files from
 *     `static/tools/cooling-tower/` and prefixes no other one), the default mount for every server
 *     here — `basePath` / `--base-path`; pass `''` to serve at the root;
 *   · `.wasm` as `application/wasm` (the MIME a wrong host would break);
 *   · and a pin/cache contract that makes a pin change the only thing that can invalidate a
 *     cached copy. Two URL forms per file:
 *
 *       plain (`/vendor/engine/synergy_drafthouse.wasm`)
 *           strong `ETag` = the sha256 of the served bytes (the pin identity), and
 *           `Cache-Control: no-cache`: a cache may store the copy but must revalidate, and the
 *           validator IS the pin — so an unchanged pin revalidates to 304 (the copy is reused)
 *           and a pin change can never be served stale (any other validator gets 200 + the
 *           current bytes, never a 304);
 *       pin-addressed (`?v=<sha256>`, at least 16 hex digits)
 *           `Cache-Control: public, max-age=31536000, immutable` — a pin-addressed copy is the
 *           only thing a host may cache immutably, and a `?v=` that does not name the served
 *           bytes is refused (404), so bytes can never be served under a pin identity they do
 *           not have.
 *
 * `checkCacheContract()` asserts that contract from the served URLs and headers alone — what a
 * host (or a CDN cache) actually sees — including a pin-change simulation on a byte-changed
 * copy: it fails when a stale copy could be served after a pin change.
 *
 * Usage — from any working directory:
 *
 *     node scripts/deploytest-serve.mjs                       # serve the deploy plane (prints the origin)
 *     node scripts/deploytest-serve.mjs --check               # run the pin/cache check, exit 0/1/2
 *     node scripts/deploytest-serve.mjs --check --cache-policy naive
 *
 *   --dir <path>               the served root (default: the cockpit plane, `cockpit/`)
 *   --base-path <path>         the mount path the files are served under (default: the website's
 *                              `/tools/cooling-tower/`; pass '' to serve at the root)
 *   --port <n>                 port for the serve mode (default 0: an ephemeral port)
 *   --csp <policy>             the document CSP to serve (the mutations pass the site's current,
 *                              token-less header, `CURRENT_SITE_CSP`)
 *   --wasm-content-type <type> the `.wasm` content type (the MIME mutation overrides it)
 *   --cache-policy <policy>    pinned (default) | naive | no-validator | accept-stale | any-digest
 *   --pin-ref <git ref>        read bundle pins from that revision (default: the working tree)
 *   --check                    run the hermetic pin/cache check instead of serving
 *
 * The mutation policies exist for the issue #38 mutation probes — `naive` is the plausible
 * static-host default (long, immutable caching on a mutable URL), `no-validator` drops the
 * ETag, `accept-stale` 304s any validator, `any-digest` serves bytes under any pin identity.
 * Each one is caught by a named failure in the check; `tests/deployment-serving.test.js` drives
 * them on a fixture tree.
 *
 * Exit status (--check): 0 the contract holds; 1 it does not (the failures are named); 2 the
 * check could not run (no served tree, the server could not start).
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { root, servedPath } from './bundle.mjs';

export { root };
export const DEFAULT_SERVED_ROOT = join(root, 'cockpit');
export const CSP_TOKEN = "'wasm-unsafe-eval'";
/** The pinned host policy: both headers verbatim, with the provenance that makes them checkable. */
export const SITE_POLICY = JSON.parse(readFileSync(join(root, 'tests/fixtures/site-policy.json'), 'utf8'));
/** The header the website serves today (`globalHeaders["Content-Security-Policy"]` at 4937a1e). */
export const CURRENT_SITE_CSP = SITE_POLICY.currentHeader;
/** The header after the approved change: the current header plus one token in `script-src`. */
export const PRODUCTION_CSP = SITE_POLICY.resultingHeader;
/** The website's deploy path: plain static files under `static/tools/cooling-tower/`. */
export const DEFAULT_BASE_PATH = SITE_POLICY.deployPath;
export const PRODUCTION_WASM_TYPE = 'application/wasm';
export const CACHE_POLICIES = ['pinned', 'naive', 'no-validator', 'accept-stale', 'any-digest'];
/** The shortest `?v=` form the server accepts: a 16-hex-digit prefix of the digest. */
export const MIN_ADDRESS_DIGITS = 16;

export const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

const CONTENT_TYPES = new Map([
  ['.html', 'text/html; charset=utf-8'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.mjs', 'text/javascript; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.wasm', PRODUCTION_WASM_TYPE],
  ['.svg', 'image/svg+xml'],
  ['.png', 'image/png'],
  ['.txt', 'text/plain; charset=utf-8'],
  ['.md', 'text/markdown; charset=utf-8'],
]);

export function contentTypeFor(file, wasmContentType = PRODUCTION_WASM_TYPE) {
  const extension = extname(file);
  if (extension === '.wasm') return wasmContentType;
  return CONTENT_TYPES.get(extension) ?? 'application/octet-stream';
}

/** Every file except root Cargo build output, relative, forward slashes, ascending. */
export function servedFiles(servedRoot = DEFAULT_SERVED_ROOT) {
  if (!existsSync(servedRoot) || !statSync(servedRoot).isDirectory()) return [];
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(join(servedRoot, dir), { withFileTypes: true })) {
      // The cockpit root is also a Rust workspace. Never traverse its build cache.
      if (dir === '' && entry.name === 'target') continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (statSync(join(servedRoot, path)).isFile()) files.push(path.split(sep).join('/'));
    }
  };
  walk('');
  return files.sort();
}

/** Bundle sha256s keyed by served path.
 *
 * The pin identity is read from the tree under audit (issue #60 fix round; the #56 precedent: a
 * `--dir` surface's expectations come from that tree): `fromDirectory` (default the repository
 * root) supplies `bundles/*.manifest.json` — a consumer deployment laid out by
 * `npm run bundle --out` publishes its pins there, a tree with none (the cockpit plane) is
 * checked without pin-digest assertions, never against another tree's pins. `ref` is the
 * explicit override for a tree whose pins are not on disk at all (the frozen v0.1.0 tag-era
 * archive): it reads both manifests out of that revision. A named revision must contain both
 * manifests; missing historical pins must not pass silently. Either way the
 * `validator-is-the-pin` assertion keeps its per-file half — the ETag must be the sha256 of the
 * served bytes — on every file of every tree. */
export function pinnedDigests({ fromDirectory = root, ref } = {}) {
  const pins = new Map();
  for (const piece of ['engine', 'visuals']) {
    if (ref !== undefined) {
      const text = execFileSync('git', ['show', `${ref}:bundles/${piece}.manifest.json`], { cwd: root, encoding: 'utf8' });
      const pin = JSON.parse(text);
      for (const entry of pin.files ?? []) pins.set(servedPath(piece, entry.path), entry.sha256);
      continue;
    }
    const file = join(fromDirectory, 'bundles', `${piece}.manifest.json`);
    if (!existsSync(file)) continue;
    const pin = JSON.parse(readFileSync(file, 'utf8'));
    for (const entry of pin.files ?? []) pins.set(servedPath(piece, entry.path), entry.sha256);
  }
  return pins;
}

/** True when a `?v=` value names these bytes: the full digest, or a >=16-hex-digit prefix of it. */
export function digestUnderAddress(address, digest) {
  if (typeof address !== 'string' || address.length < MIN_ADDRESS_DIGITS) return false;
  if (!/^[0-9a-f]+$/i.test(address)) return false;
  return digest.startsWith(address.toLowerCase());
}

/** Parse the cache directives the contract talks about, without pretending to be a full parser. */
export function cacheDirectives(header) {
  const value = String(header ?? '').toLowerCase();
  const maxAge = /(?:^|[,\s])max-age\s*=\s*(\d+)/.exec(value);
  return {
    present: value.trim() !== '',
    immutable: /(?:^|[,\s])immutable(?:[,\s]|$)/.test(value),
    noCache: /(?:^|[,\s])no-cache(?:[,\s]|$)/.test(value),
    mustRevalidate: /(?:^|[,\s])must-revalidate(?:[,\s]|$)/.test(value),
    maxAge: maxAge === null ? null : Number(maxAge[1]),
  };
}

/** A copy may be reused without asking the origin again — the stale hazard, as a predicate. */
export function reusableWithoutValidation(header) {
  const directives = cacheDirectives(header);
  return directives.immutable || (directives.maxAge !== null && directives.maxAge > 0 && !directives.noCache);
}

/** `/tools/cooling-tower/` -> `/tools/cooling-tower`; `''` stays `''` (serve at the root). */
export function normaliseBasePath(basePath) {
  const trimmed = String(basePath ?? '').replace(/^\/+|\/+$/g, '');
  return trimmed === '' ? '' : `/${trimmed}`;
}

/**
 * The production-shaped server. `cachePolicy` and the content types exist as parameters so the
 * mutation probes can serve the wrong shape on purpose; the default is the production shape.
 * `basePath` is the mount path the host serves the tool under (default: the root); a request
 * outside it is a 404, because in this deploy plane nothing exists outside the mount.
 */
export function createDeployServer({
  servedRoot = DEFAULT_SERVED_ROOT,
  csp = PRODUCTION_CSP,
  wasmContentType = PRODUCTION_WASM_TYPE,
  cachePolicy = 'pinned',
  basePath = '',
} = {}) {
  if (!CACHE_POLICIES.includes(cachePolicy)) throw new Error(`unknown cache policy ${JSON.stringify(cachePolicy)}`);
  const mount = normaliseBasePath(basePath);
  const requests = [];
  const server = createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1');
    let path = decodeURIComponent(url.pathname);
    if (mount !== '') {
      if (path === mount) path = '/';
      else if (path.startsWith(`${mount}/`)) path = path.slice(mount.length);
      else path = null;
    }
    const address = url.searchParams.get('v');
    const file = path === null ? null : join(servedRoot, path.endsWith('/') ? `${path}index.html` : path);

    let status = 200;
    let body = Buffer.alloc(0);
    let digest = null;
    const inside = file !== null && (file === servedRoot || file.startsWith(servedRoot + sep));
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      status = 405;
      body = Buffer.from('method not allowed\n');
    } else if (!inside || !existsSync(file) || !statSync(file).isFile()) {
      status = 404;
      body = Buffer.from('not in this deploy\n');
    } else {
      body = readFileSync(file);
      digest = sha256(body);
    }
    if (status === 200 && address !== null && !digestUnderAddress(address, digest) && cachePolicy !== 'any-digest') {
      status = 404;
      body = Buffer.from('no copy is pinned to that digest\n');
      digest = null;
    }

    const headers = {
      'content-type': contentTypeFor(file ?? '', wasmContentType),
      'x-content-type-options': 'nosniff',
    };
    if (file !== null && file.endsWith('.html')) headers['content-security-policy'] = csp;
    if (status === 200 && digest !== null) {
      if (cachePolicy !== 'no-validator') headers.etag = `"${digest}"`;
      const addressed = address !== null && (cachePolicy === 'any-digest' || digestUnderAddress(address, digest));
      headers['cache-control'] = addressed || cachePolicy === 'naive'
        ? 'public, max-age=31536000, immutable'
        : 'no-cache';
    }

    const ifNoneMatch = request.headers['if-none-match'];
    if (status === 200 && ifNoneMatch !== undefined && headers.etag !== undefined) {
      if (cachePolicy === 'accept-stale' || ifNoneMatch === headers.etag) {
        status = 304;
        body = Buffer.alloc(0);
      }
    }

    requests.push({
      method: request.method ?? 'GET',
      path: `${url.pathname}${url.search}`,
      status,
      bytes: body.length,
      contentType: headers['content-type'],
      cacheControl: headers['cache-control'] ?? null,
    });
    response.writeHead(status, headers);
    response.end(request.method === 'HEAD' || status === 304 ? undefined : body);
  });
  server.requests = requests;
  return server;
}

export async function startDeployServer(options = {}) {
  const server = createDeployServer(options);
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const mount = normaliseBasePath(options.basePath ?? '');
  return {
    server,
    origin,
    /** The base URL the served files are reachable at: `origin` plus the mount path. */
    url: `${origin}${mount}`,
    mount,
    requests: server.requests,
    close: () => new Promise((done) => {
      server.closeAllConnections?.();
      server.close(done);
    }),
  };
}

/* ------------------------------------------------------------------ *
 * The pin/cache contract, asserted from served URLs and headers only.
 * ------------------------------------------------------------------ */

const fetchResponse = async (fetchImpl, url, headers = {}) => {
  const response = await fetchImpl(url, { headers, redirect: 'manual', cache: 'no-store' });
  const bytes = Buffer.from(await response.arrayBuffer());
  return { status: response.status, headers: response.headers, bytes, digest: sha256(bytes) };
};

/** A digest that is guaranteed to differ from the given one (for the false-identity probe). */
export function otherDigest(digest) {
  const digit = digest[0];
  const flipped = digit === 'f' ? '0' : (Number.parseInt(digit, 16) + 1).toString(16);
  return `${flipped}${digest.slice(1)}`;
}

export async function checkCacheContract({
  origin,
  servedRoot = DEFAULT_SERVED_ROOT,
  files = servedFiles(servedRoot),
  pins = pinnedDigests(),
  fetchImpl = fetch,
  pinChange = false,
} = {}) {
  const failures = [];
  const checks = [];
  const fail = (id, detail) => {
    failures.push({ id, detail });
    const check = checks.find((entry) => entry.id === id);
    if (check) {
      check.ok = false;
      check.failures.push(detail);
    } else {
      checks.push({ id, ok: false, failures: [detail] });
    }
  };
  const assertion = (id) => {
    if (!checks.some((entry) => entry.id === id)) checks.push({ id, ok: true, failures: [] });
  };
  for (const id of [
    'csp-production', 'mime-wasm', 'content-type-pinned', 'validator-is-the-pin', 'plain-cache-not-stale-able',
    'unchanged-pin-reuses', 'stale-validator-refreshes', 'pin-addressed-immutable', 'pin-address-exact',
  ]) assertion(id);
  if (pinChange) assertion('no-stale-after-pin-change');

  for (const file of files) {
    const url = `${origin}/${file}`;
    const plain = await fetchResponse(fetchImpl, url);
    /* The expectations are the PRODUCTION shape: only the server under test varies, so a
       mutated content type (or policy) can never be blessed by a matching expectation. */
    const expectedType = contentTypeFor(join(servedRoot, file), PRODUCTION_WASM_TYPE);
    if (file.endsWith('.wasm')) {
      if (plain.headers.get('content-type') !== PRODUCTION_WASM_TYPE) fail('mime-wasm', `${file}: content-type ${JSON.stringify(plain.headers.get('content-type'))} != ${JSON.stringify(PRODUCTION_WASM_TYPE)}`);
    }
    if (plain.status !== 200) {
      fail('content-type-pinned', `${file}: GET -> ${plain.status}, nothing to check`);
      continue;
    }
    if (plain.headers.get('content-type') !== expectedType) fail('content-type-pinned', `${file}: content-type ${JSON.stringify(plain.headers.get('content-type'))} != ${JSON.stringify(expectedType)}`);
    if (file.endsWith('.html')) {
      const header = plain.headers.get('content-security-policy');
      if (header !== PRODUCTION_CSP || !String(header).includes(CSP_TOKEN)) fail('csp-production', `${file}: content-security-policy ${JSON.stringify(header)} is not the production policy`);
    }

    const etag = plain.headers.get('etag');
    if (etag !== `"${plain.digest}"`) fail('validator-is-the-pin', `${file}: etag ${JSON.stringify(etag)} != the sha256 of the served bytes ("${plain.digest}")`);
    const pin = pins.get(file);
    if (pin !== undefined && pin !== plain.digest) fail('validator-is-the-pin', `${file}: served bytes ${plain.digest} are not the published pin ${pin}`);
    if (reusableWithoutValidation(plain.headers.get('cache-control'))) {
      fail('plain-cache-not-stale-able', `${file}: ${JSON.stringify(plain.headers.get('cache-control'))} lets a cache reuse the copy without asking — a pin change could be served stale`);
    }

    const revalidated = await fetchResponse(fetchImpl, url, { 'if-none-match': `"${plain.digest}"` });
    if (revalidated.status !== 304) fail('unchanged-pin-reuses', `${file}: a conditional GET with the current pin returned ${revalidated.status}, not 304 — an unchanged pin is not reusable`);
    const stale = await fetchResponse(fetchImpl, url, { 'if-none-match': `"${otherDigest(plain.digest)}"` });
    if (stale.status !== 200 || stale.digest !== plain.digest) {
      fail('stale-validator-refreshes', `${file}: a validator that is not the served bytes returned ${stale.status} — a pin change must invalidate, never 304`);
    }

    const addressed = await fetchResponse(fetchImpl, `${url}?v=${plain.digest}`);
    if (addressed.status !== 200 || addressed.digest !== plain.digest || !cacheDirectives(addressed.headers.get('cache-control')).immutable) {
      fail('pin-addressed-immutable', `${file}: ?v=<pin> -> ${addressed.status} ${JSON.stringify(addressed.headers.get('cache-control'))} — a pin-addressed copy must be served and may be cached immutably`);
    }
    const wrongIdentity = await fetchResponse(fetchImpl, `${url}?v=${otherDigest(plain.digest)}`);
    if (wrongIdentity.status !== 404) {
      fail('pin-address-exact', `${file}: ?v=<not these bytes> -> ${wrongIdentity.status} — bytes must never be served under a pin identity they do not have`);
    }
  }

  /* The pin change: the served bytes move under the URL. A cache holding the old copy must be
     told the truth (200 + the new bytes), and the old pin identity must be gone (404) — never
     the old bytes served as current. The caller hands us a copy it owns (the CLI and the tests
     work on a temp copy), and we restore it byte-identically in any case. */
  if (pinChange) {
    const target = files.find((file) => file.endsWith('.wasm'));
    if (target === undefined) {
      fail('no-stale-after-pin-change', 'no .wasm file in the served tree to change; the pin-change simulation cannot run');
    } else {
      const absolute = join(servedRoot, target);
      const original = readFileSync(absolute);
      const before = sha256(original);
      const changed = Buffer.from(original);
      changed[changed.length - 1] = changed[changed.length - 1] ^ 0xff;
      writeFileSync(absolute, changed);
      try {
        const afterDigest = sha256(changed);
        const afterChange = await fetchResponse(fetchImpl, `${origin}/${target}`, { 'if-none-match': `"${before}"` });
        if (afterChange.status !== 200 || afterChange.digest !== afterDigest) {
          fail('no-stale-after-pin-change', `${target}: after the pin changed, a cache holding the old copy was answered ${afterChange.status} ${afterChange.digest.slice(0, 12)} — a stale copy could be served`);
        }
        const oldAddress = await fetchResponse(fetchImpl, `${origin}/${target}?v=${before}`);
        if (oldAddress.status !== 404) {
          fail('no-stale-after-pin-change', `${target}?v=<old pin> -> ${oldAddress.status} — the old pin identity is still served`);
        }
        const newAddress = await fetchResponse(fetchImpl, `${origin}/${target}?v=${afterDigest}`);
        if (newAddress.status !== 200 || newAddress.digest !== afterDigest) {
          fail('no-stale-after-pin-change', `${target}?v=<new pin> -> ${newAddress.status} — the new pin identity is not served`);
        }
      } finally {
        writeFileSync(absolute, original);
        if (sha256(readFileSync(absolute)) !== before) fail('restore-identical', `${target} was not restored byte-identically after the pin-change simulation`);
      }
    }
  }

  return { passed: failures.length === 0, files: files.length, checks, failures };
}

/** A temp copy of the served tree, for the pin-change simulation (the repository is never written). */
export function copyServedTree(servedRoot = DEFAULT_SERVED_ROOT) {
  const copy = mkdtempSync(join(tmpdir(), 'deployment-serving-'));
  for (const file of servedFiles(servedRoot)) {
    const target = join(copy, file);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(join(servedRoot, file), target);
  }
  return copy;
}

/* ------------------------------------------------------------------ *
 * CLI: serve, or run the check.
 * ------------------------------------------------------------------ */

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : process.argv[index + 1];
}

function refuse(message) {
  console.error(`deploytest-serve: ${message}`);
  process.exit(2);
}

async function main() {
  const dirFlag = option('--dir', undefined);
  const servedRoot = dirFlag === undefined ? DEFAULT_SERVED_ROOT : resolve(dirFlag);
  const csp = option('--csp', PRODUCTION_CSP);
  const wasmContentType = option('--wasm-content-type', PRODUCTION_WASM_TYPE);
  const cachePolicy = option('--cache-policy', 'pinned');
  const basePath = option('--base-path', DEFAULT_BASE_PATH);
  const check = process.argv.includes('--check');

  if (!existsSync(servedRoot) || !statSync(servedRoot).isDirectory()) refuse(`--dir ${dirFlag ?? servedRoot} is not a directory; there is no deploy plane to serve`);
  if (!existsSync(join(servedRoot, 'index.html'))) refuse(`${join(servedRoot, 'index.html')} is missing; there is no surface to serve`);
  if (!CACHE_POLICIES.includes(cachePolicy)) refuse(`unknown --cache-policy ${JSON.stringify(cachePolicy)}`);

  const shape = { servedRoot: relative(root, servedRoot) || servedRoot, basePath: normaliseBasePath(basePath), csp, wasmContentType, cachePolicy };

  if (!check) {
    const { url, close } = await startDeployServer({ servedRoot, csp, wasmContentType, cachePolicy, basePath });
    console.log(JSON.stringify({ serving: shape, origin: url, files: servedFiles(servedRoot).length }, null, 2));
    process.on('SIGINT', () => { close().then(() => process.exit(0)); });
    return;
  }

  const files = servedFiles(servedRoot);
  if (files.length === 0) refuse(`${relative(root, servedRoot)} has no files; there is nothing to check`);

  /* The pin identity is the tree under audit's own on-disk `bundles/` — the default, which
     needs no flag and never reaches across trees — except when `--pin-ref` names the revision
     to read the pins from, for a tree whose pins are not on disk at all (the frozen v0.1.0
     tag-era archive; its own tags' manifests are its own era's identity). A --pin-ref naming
     today's tree against a frozen --dir would assert current digests against immutable bytes;
     that comparison can only fail. */
  const pinRef = option('--pin-ref', undefined);
  const pins = pinnedDigests(pinRef === undefined ? { fromDirectory: servedRoot } : { ref: pinRef });
  const started = Date.now();
  const served = await startDeployServer({ servedRoot, csp, wasmContentType, cachePolicy, basePath });
  const first = await checkCacheContract({ origin: served.url, servedRoot, files, pins, pinChange: false });
  await served.close();

  const copy = copyServedTree(servedRoot);
  const copyServer = await startDeployServer({ servedRoot: copy, csp, wasmContentType, cachePolicy, basePath });
  const second = await checkCacheContract({ origin: copyServer.url, servedRoot: copy, files, pins, pinChange: true });
  await copyServer.close();
  rmSync(copy, { recursive: true, force: true });

  const failures = [...first.failures, ...second.failures];
  const report = {
    passed: failures.length === 0,
    shape,
    files: files.length,
    pinChangeSimulation: second.failures.length === 0 ? 'passed' : 'failed',
    checks: [...first.checks, ...second.checks],
    failures,
    cost: { wallClockMs: Date.now() - started },
  };
  if (report.passed) console.log(JSON.stringify(report, null, 2));
  else console.error(JSON.stringify(report, null, 2));
  process.exitCode = report.passed ? 0 : 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
