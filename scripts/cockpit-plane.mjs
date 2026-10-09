#!/usr/bin/env node
/**
 * The cockpit plane's identity: issue #148 — a deploy must reach a returning browser.
 *
 * The defect: `index.html` loaded `./pkg/drafthouse_cockpit.js` at a **fixed name**, and the host
 * serves `/pkg/*` and `/assets/*` with `Cache-Control: public, max-age=31536000, immutable`
 * (`synergy-apps infra/drafthouse/staticwebapp.config.json`). A browser that has fully loaded
 * build N keeps build N's module, its ~23.9 MB `.wasm` and its `fixture.json` for a year: the
 * next deploy is invisible to it, and the entry document's own `no-cache` cannot rescue it —
 * the names it asks for have not changed.
 *
 * The fix, and the property this module exists to hold: **build N+1 must use a URL the browser
 * cannot have cached from build N.** Every name the page loads at run time is therefore placed
 * under an address derived from the digest of the payload's bytes:
 *
 *   pkg/drafthouse_cockpit.js      -> pkg/<digest>/drafthouse_cockpit.js
 *   pkg/drafthouse_cockpit_bg.wasm -> pkg/<digest>/drafthouse_cockpit_bg.wasm
 *   assets/…                       -> assets/<digest>/…
 *
 * The `.wasm` follows the module for free: wasm-bindgen's glue fetches it relative to its own
 * URL (`new URL('drafthouse_cockpit_bg.wasm', import.meta.url)`), so both files moving together
 * moves it too. The addresses are one digest over the **whole payload** — module, `.wasm` and
 * every served asset — so a JS/wasm pair can never be served apart from the assets it belongs
 * to, and an assets-only change still moves the module (the pair reloads together, never
 * half-stale). Identical bytes keep the identical URL: a rebuild of the same commit is a cache
 * hit, not a second download.
 *
 * The prefixes stay `/pkg/` and `/assets/` (the host's immutable routes), so the host policy
 * needs no new rule: it was correct, it was the *names* that were wrong. The document itself
 * stays at a fixed name and is **never** long-cached (`no-cache` — revalidated on every load),
 * which is what lets it name the *new* address. That division is the whole design: a fixed name
 * is safe exactly when it is revalidated, an immutable name is safe exactly when it is
 * content-addressed.
 *
 * Two anchors in `cockpit/index.html` are rewritten, exactly once each:
 *
 *   await import('./pkg/drafthouse_cockpit.js')   ->  './pkg/<digest>/drafthouse_cockpit.js'
 *   assets: 'assets'                              ->  'assets/<digest>'
 *
 * The second one is passed to bevy's `AssetPlugin` (`file_path`), because the wasm asset server
 * resolves names relative to the *document*, not the module — see `cockpit/src/bootstrap.rs`.
 * The dev tree keeps its flat names (a from-source build, `scripts/serve.mjs` and the local
 * suite all read them as before); only the packed release artifact is addressed, and the
 * packer is the only writer.
 */
import { createHash } from 'node:crypto';

/** The payload prefixes, relative to `cockpit/` — the two the host serves immutable. */
export const PAYLOAD_PREFIXES = ['pkg', 'assets'];

/** The entry document, relative to `cockpit/`. */
export const DOCUMENT = 'index.html';

export const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

/** The address of a payload: 16 hex digits of sha256 over its sorted `sha256  bytes  path` listing. */
export function addressOf(payload) {
  const listing = [...payload]
    .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
    .map((file) => `${sha256(file.bytes)}  ${file.bytes.length}  ${file.path}`)
    .join('\n');
  return sha256(Buffer.from(`${listing}\n`)).slice(0, 16);
}

/** `cockpit/pkg/x` -> `pkg/x`; `cockpit/index.html` -> null (not payload). */
function payloadRelative(path) {
  const local = path.slice('cockpit/'.length);
  const prefix = PAYLOAD_PREFIXES.find((candidate) => local.startsWith(`${candidate}/`));
  return prefix === undefined ? null : local;
}

/** The document's two runtime references as they are written on a flat (unaddressed) plane. */
export function references(document) {
  const module = /\bawait import\('\.\/pkg\/([^']+)'\)/.exec(document);
  const assets = /\bassets: '([^']*)'/.exec(document);
  return { module: module ? module[1] : null, assets: assets ? assets[1] : null };
}

/**
 * Address a plane: every file under `pkg/` and `assets/` moves to `<prefix>/<digest>/…`, and the
 * document's two runtime references are rewritten to match.
 *
 * `entries` is `[{ path, bytes }]` with `cockpit/`-shaped paths, exactly what `planeFiles()` in
 * `scripts/cockpit-artifact.mjs` produces. Returns `{ digest, entries }`; byte-identical input
 * yields byte-identical output. A document whose anchors have drifted throws — the caller
 * refuses the release rather than shipping a plane whose names nothing serves.
 */
export function addressPlane(entries) {
  const payload = entries
    .map((entry) => {
      const local = payloadRelative(entry.path);
      return local === null ? null : { path: local, bytes: entry.bytes };
    })
    .filter((entry) => entry !== null);
  if (payload.length === 0) throw new Error('the plane carries no runtime payload to address');

  const digest = addressOf(payload);
  const moved = entries.map((entry) => {
    const local = payloadRelative(entry.path);
    if (local === null) return entry;
    const [prefix, ...rest] = local.split('/');
    return { ...entry, path: `cockpit/${prefix}/${digest}/${rest.join('/')}` };
  });

  const document = entries.find((entry) => entry.path === `cockpit/${DOCUMENT}`);
  if (!document) throw new Error(`the plane carries no cockpit/${DOCUMENT}`);
  const before = references(document.bytes.toString('utf8'));
  if (before.module === null) throw new Error(`cockpit/${DOCUMENT} carries no pkg module import to address`);
  if (before.assets === null) throw new Error(`cockpit/${DOCUMENT} carries no assets reference to address`);
  if (before.assets !== 'assets') {
    throw new Error(`cockpit/${DOCUMENT}'s assets reference is ${JSON.stringify(before.assets)}, not the flat 'assets' this packer rewrites — refusing to guess`);
  }

  const text = document.bytes.toString('utf8');
  const addressed = text
    .replace(`await import('./pkg/${before.module}')`, `await import('./pkg/${digest}/${before.module}')`)
    .replace(`assets: 'assets'`, `assets: 'assets/${digest}'`);
  for (const anchor of [`./pkg/${digest}/${before.module}`, `assets: 'assets/${digest}'`]) {
    const occurrences = addressed.split(anchor).length - 1;
    if (occurrences !== 1) throw new Error(`the address anchor ${JSON.stringify(anchor)} occurs ${occurrences} times in ${DOCUMENT}, expected exactly 1`);
  }

  return {
    digest,
    entries: moved.map((entry) =>
      entry.path === `cockpit/${DOCUMENT}` ? { ...entry, bytes: Buffer.from(addressed, 'utf8') } : entry,
    ),
  };
}

/**
 * Every runtime reference the addressed document makes, resolved against the plane that will be
 * served — what a host (or a CDN cache) actually fetches. Returns the failures, each naming what
 * is missing or inconsistent; an addressed, self-consistent plane returns `[]`.
 */
export function referenceFailures(entries) {
  const byPath = new Map(entries.map((entry) => [entry.path, entry.bytes]));
  const document = byPath.get(`cockpit/${DOCUMENT}`);
  if (document === undefined) return [`cockpit/${DOCUMENT}: missing from the plane`];
  const text = document.toString('utf8');

  const module = /\.\/pkg\/([0-9a-f]{16})\/([^']+)'/.exec(text);
  const assets = /\bassets: 'assets\/([0-9a-f]{16})'/.exec(text);
  const failures = [];
  if (module === null) failures.push(`cockpit/${DOCUMENT}: the module import carries no content address (./pkg/<digest>/…)`);
  if (assets === null) failures.push(`cockpit/${DOCUMENT}: the assets reference carries no content address (assets/<digest>)`);
  if (module === null || assets === null) return failures;
  if (module[1] !== assets[1]) {
    failures.push(`cockpit/${DOCUMENT}: the module address ${module[1]} and the assets address ${assets[1]} disagree — the payload must move as one`);
  }
  const digest = module[1];

  const payload = [];
  for (const entry of entries) {
    const local = payloadRelative(entry.path);
    if (local === null) continue;
    const prefix = local.split('/')[0];
    const marker = `${prefix}/${digest}/`;
    if (!local.startsWith(marker)) continue; // not under this address
    const rest = local.slice(marker.length);
    if (rest.length === 0) continue;
    payload.push({ path: `${prefix}/${rest}`, bytes: entry.bytes });
  }
  if (payload.length === 0) {
    failures.push(`cockpit/${DOCUMENT}: names the address ${digest}, but the plane carries no payload under cockpit/<prefix>/${digest}/`);
    return failures;
  }
  const derived = addressOf(payload);
  if (derived !== digest) {
    failures.push(`cockpit/${DOCUMENT}: names the address ${digest}, but the payload under it derives ${derived} — the bytes and the address disagree`);
  }

  // The two names the document itself fetches inside the addressed payload.
  for (const part of [`cockpit/pkg/${digest}/${module[2]}`, `cockpit/assets/${digest}/fixture.json`]) {
    if (!byPath.has(part)) failures.push(`${part}: named by cockpit/${DOCUMENT}, missing from the plane`);
  }
  return failures;
}
