/**
 * The cockpit plane's served identity — issue #148.
 *
 * The defect this file guards: `index.html` loaded its module, its `.wasm` and its assets at
 * **fixed names** while the host serves `/pkg/*` and `/assets/*` `public, max-age=31536000,
 * immutable`. A returning browser that has fully loaded build N therefore keeps build N for a
 * year, and the next deploy is invisible to it. The property the fix must hold, and what these
 * cases pin:
 *
 *   1. **build N+1 must use a payload URL the browser cannot have cached from build N** — two
 *      planes whose payload bytes differ share no payload URL, and (because the whole payload is
 *      one address) an assets-only change moves the module URL too: the pair can never be served
 *      half-stale;
 *   2. **a rebuild of the same bytes keeps every URL** — addressing is content, not a timestamp,
 *      so an unchanged release stays a cache hit (the issue's own reason for preferring
 *      content-addressing over dropping `immutable` on a ~23.9 MB binary);
 *   3. **the packed document's references resolve inside the plane that will be served** — a
 *      shell asking for a name the host does not carry is worse than the bug, and a plane whose
 *      address does not derive from the bytes under it is refused;
 *   4. **a document whose anchors have drifted is refused**, never silently shipped unaddressed.
 *
 * Case 5 drives the real packer on the built plane when the machine has one (`cockpit/pkg/` is a
 * build product); a tree that has not built it prints the NOT VERIFIABLE diagnostic the
 * deployment-shaped cases also use and asserts nothing. The `cockpit` CI job builds the plane,
 * so the case verifies there.
 *
 * The guard BITES: it is written against the mechanism (`scripts/cockpit-plane.mjs`), and the
 * pre-fix behaviour — a packer that leaves names fixed — fails cases 1 and 2 while the
 * unaddressed document fails case 3. The lane's report records that mutation probe.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { addressOf, addressPlane, referenceFailures, references } from '../scripts/cockpit-plane.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The flat document shape the tree carries: the two anchors the packer rewrites. */
const flatDocument = (extra = '') =>
  Buffer.from(
    `<!doctype html>\n<script type="module">\n${extra}async function boot() {\n` +
      `  const module = await import('./pkg/drafthouse_cockpit.js');\n` +
      `  module.viz_start('#viz-canvas', JSON.stringify({ ...options, assets: 'assets' }));\n` +
      `}\nboot();\n</script>\n`,
  );

/** A synthetic plane: the document, the module, the wasm, one asset file. */
function plane({ wasm = 'wasm-bytes-A', asset = 'fixture-A', document = flatDocument() } = {}) {
  return [
    { path: 'cockpit/index.html', bytes: document },
    { path: 'cockpit/pkg/drafthouse_cockpit.js', bytes: Buffer.from('// wasm-bindgen glue\n') },
    { path: 'cockpit/pkg/drafthouse_cockpit_bg.wasm', bytes: Buffer.from(wasm) },
    { path: 'cockpit/assets/fixture.json', bytes: Buffer.from(asset) },
    { path: 'cockpit/assets/fonts/subset/x.ttf', bytes: Buffer.from('font') },
  ].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

const paths = (entries) => entries.map((entry) => entry.path);
const documentOf = (entries) => entries.find((entry) => entry.path === 'cockpit/index.html').bytes.toString('utf8');
/** The URLs the served document makes the browser fetch by name. */
function payloadUrls(entries) {
  const text = documentOf(entries);
  const module = /\.\/pkg\/([0-9a-f]{16}\/[^']+)'/.exec(text)[1];
  const assets = /\bassets: 'assets\/([0-9a-f]{16})'/.exec(text)[1];
  const directory = module.slice(0, module.lastIndexOf('/'));
  return {
    module: `/pkg/${module}`,
    wasm: `/pkg/${directory}/drafthouse_cockpit_bg.wasm`,
    assets: `/assets/${assets}/fixture.json`,
  };
}

test('two builds of different bytes never share a payload URL — the property a deploy needs (#148)', () => {
  const before = addressPlane(plane());
  const after = addressPlane(plane({ wasm: 'wasm-bytes-B' }));

  assert.notEqual(before.digest, after.digest, 'a payload change must move the address; the two builds derive the same one');
  const payloadOf = (entries) => paths(entries).filter((path) => path !== 'cockpit/index.html');
  const stale = payloadOf(before.entries).filter((path) => payloadOf(after.entries).includes(path));
  assert.deepEqual(stale, [], `two builds share served payload URLs: ${stale.join(', ')}`);

  const beforeUrls = payloadUrls(before.entries);
  const afterUrls = payloadUrls(after.entries);
  for (const key of Object.keys(beforeUrls)) {
    assert.notEqual(beforeUrls[key], afterUrls[key], `the ${key} URL is identical between the two builds: ${beforeUrls[key]}`);
  }
  // The wasm follows the module because wasm-bindgen resolves it against `import.meta.url`;
  // both files sit in the same addressed directory, so it can never be left behind.
  assert.equal(beforeUrls.wasm.slice(0, beforeUrls.wasm.lastIndexOf('/')), beforeUrls.module.slice(0, beforeUrls.module.lastIndexOf('/')), 'the wasm must sit in the module\'s own addressed directory');
});

test('an assets-only change moves the module URL too — the pair can never be served half-stale (#148)', () => {
  const module = '/* identical */';
  const before = addressPlane(plane({ asset: 'fixture-A', document: flatDocument(module) }));
  const after = addressPlane(plane({ asset: 'fixture-B', document: flatDocument(module) }));

  assert.notEqual(before.digest, after.digest, 'the fixture changed and the address did not move');
  assert.notEqual(
    payloadUrls(before.entries).module,
    payloadUrls(after.entries).module,
    'a new fixture could otherwise satisfy an old cached module (or the reverse)',
  );
});

test('a rebuild of the same bytes keeps every URL — addressing is content, not a timestamp (#148)', () => {
  const first = addressPlane(plane());
  const second = addressPlane(plane());
  assert.equal(first.digest, second.digest, 'the same bytes derived two addresses');
  assert.deepEqual(paths(first.entries), paths(second.entries));
  assert.equal(documentOf(first.entries), documentOf(second.entries));

  // The document itself is not part of the address (it is `no-cache` and names the address):
  // a comment-only document change does not evict the year-long payload cache.
  const commented = addressPlane(plane({ document: flatDocument('// a new comment\n') }));
  assert.equal(commented.digest, first.digest, 'a document-only change moved the payload address');
  assert.notEqual(documentOf(commented.entries), documentOf(first.entries), 'the case cannot bite if the documents are identical');
});

test('the packed document\'s references resolve inside the plane that will be served (#148)', () => {
  const packed = addressPlane(plane());
  assert.deepEqual(referenceFailures(packed.entries), [], 'a good addressed plane must have no unresolved reference');

  // Pre-fix shape: the document names flat files — exactly what this lane removes.
  const flat = plane();
  assert.ok(
    referenceFailures(flat).some((failure) => failure.includes('no content address')),
    'a flat document must be refused: its names cannot be told apart between builds',
  );

  // Every payload file participates in the address: drop one and the plane no longer derives it.
  const dropped = packed.entries.filter((entry) => entry.path !== `cockpit/assets/${packed.digest}/fonts/subset/x.ttf`);
  assert.ok(
    referenceFailures(dropped).some((failure) => failure.includes('the bytes and the address disagree')),
    'a plane missing a payload file must not pass as addressed',
  );

  // A byte flipped under the address is the same failure, named with both digests.
  const tampered = packed.entries.map((entry) =>
    entry.path.endsWith('drafthouse_cockpit_bg.wasm') ? { ...entry, bytes: Buffer.from('wasm-bytes-!') } : entry,
  );
  const failures = referenceFailures(tampered);
  assert.ok(failures.some((failure) => failure.includes('the bytes and the address disagree')), JSON.stringify(failures));

  // And the address itself is the payload's own, derived the documented way.
  const payload = packed.entries
    .filter((entry) => /^cockpit\/(pkg|assets)\//.test(entry.path))
    .map((entry) => ({ path: entry.path.replace(`cockpit/${entry.path.split('/')[1]}/${packed.digest}/`, `${entry.path.split('/')[1]}/`), bytes: entry.bytes }));
  assert.equal(addressOf(payload), packed.digest, 'the manifest address must be the payload listing\'s own digest');
});

test('a document whose anchors have drifted is refused, not silently shipped unaddressed (#148)', () => {
  const cases = [
    ['the import is written with double quotes', flatDocument().toString('utf8').replace("import('./pkg/drafthouse_cockpit.js')", 'import("./pkg/drafthouse_cockpit.js")'), 'no pkg module import'],
    ['the assets anchor is gone', flatDocument().toString('utf8').replace(", assets: 'assets'", ''), 'no assets reference'],
    ['the assets anchor names something else', flatDocument().toString('utf8').replace("assets: 'assets'", "assets: 'somewhere-else'"), 'refusing to guess'],
  ];
  for (const [label, text, expected] of cases) {
    assert.throws(
      () => addressPlane(plane({ document: Buffer.from(text) })),
      new RegExp(expected),
      `the packer must refuse a document whose ${label}`,
    );
  }
});

test('the built plane packs addressed and verifies from its own files (#148)', () => {
  const built = join(root, 'cockpit', 'pkg', 'drafthouse_cockpit_bg.wasm');
  if (!existsSync(built)) {
    console.log('NOT VERIFIABLE: cockpit/pkg/ is a build product and this tree has not built it — run cockpit/tools/build-web.sh release (the cockpit CI job does) and re-run; asserting nothing');
    return;
  }
  const out = mkdtempSync(join(tmpdir(), 'cockpit-plane-'));
  try {
    const written = execFileSync('node', [join(root, 'scripts/cockpit-artifact.mjs'), '--write', out], { cwd: root, encoding: 'utf8' });
    const manifest = JSON.parse(readFileSync(join(out, `drafthouse-cockpit-${JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version}.manifest.json`), 'utf8'));
    assert.match(manifest.address ?? '', /^[0-9a-f]{16}$/, `the packed manifest carries no content address: ${written}`);
    const parts = manifest.files.map((file) => file.path);
    for (const prefix of ['pkg', 'assets']) {
      assert.ok(
        parts.some((path) => path.startsWith(`cockpit/${prefix}/${manifest.address}/`)),
        `the packed plane carries no addressed ${prefix}/ payload: ${parts.join(', ')}`,
      );
    }
    assert.ok(parts.includes('cockpit/index.html'), 'the packed plane lost its document');
    const verified = execFileSync('node', [join(root, 'scripts/cockpit-artifact.mjs'), '--verify', out], { cwd: root, encoding: 'utf8' });
    assert.match(verified, /verified from the files themselves/, verified);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
});

/** The two anchors are the packer's whole contract with the tree; a rename must fail loudly. */
test('the tree carries exactly the two anchors the packer rewrites (#148)', () => {
  const document = readFileSync(join(root, 'cockpit', 'index.html'), 'utf8');
  const anchors = references(document);
  assert.equal(anchors.module, 'drafthouse_cockpit.js', 'the module import anchor moved — the packer would refuse every release');
  assert.equal(anchors.assets, 'assets', 'the assets anchor moved — the packer would refuse every release');
  assert.equal(document.split(`await import('./pkg/${anchors.module}')`).length - 1, 1, 'the module anchor must appear exactly once');
  assert.equal(document.split("assets: 'assets'").length - 1, 1, 'the assets anchor must appear exactly once');
});
