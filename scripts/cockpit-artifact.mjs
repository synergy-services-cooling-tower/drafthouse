#!/usr/bin/env node
/**
 * The cockpit release artifact — issue #58.
 *
 * The cockpit is the product UI now: the release publishes it as **one servable plane** —
 * `cockpit/index.html`, `cockpit/pkg/**` (the wasm build's JavaScript and its `.wasm`) and
 * `cockpit/assets/**` (the fixture record and the fonts the app fetches at run time) — inside the
 * deterministic container `scripts/release-archive.mjs` writes, with its own manifest (which names
 * the source commit the plane was built from), its own digest and the wasm's gzip size recorded
 * next to them.
 *
 * It is a **separate artifact from the pinned engine/visuals bundle** (D16): the bundle is the
 * numeric engine and the recorded baseline; this is the surface that runs them, alongside the
 * bundle rather than instead of it. Nothing here is committed — the plane is a build product, and
 * the workflow builds it from the tagged commit before packing it.
 *
 *   node scripts/cockpit-artifact.mjs --write dist/cockpit     # after `cockpit/tools/build-web.sh release`
 *   node scripts/cockpit-artifact.mjs --verify dist/cockpit    # re-derives every hash from the files
 *
 * `--write` refuses a plane whose wasm is missing or older than the sources it was built from:
 * the artifact must be the build of *this* commit, not a stale `pkg/`.
 *
 * Issue #148: the *packed* plane is **content-addressed** — `pkg/**` and `assets/**` move to
 * `pkg/<digest>/…` and `assets/<digest>/…` and the document's two runtime references are
 * rewritten to match (`scripts/cockpit-plane.mjs` explains why). The tree on disk is not
 * touched: the dev server and the from-source suite keep reading the flat names they always
 * read. `--verify` additionally refuses a plane whose document names something the plane does
 * not carry, and one whose address does not derive from the bytes under it.
 *
 * Exit status: `0` written / every hash verified; `1` the manifest and the files disagree, or
 * the plane cannot be addressed (every failure is named); `2` the check could not run (no
 * build, no archive, unreadable manifest).
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { readArchive, sha256, writeArchive } from './release-archive.mjs';
import { addressPlane, referenceFailures } from './cockpit-plane.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/** The archive's paths are the plane's own: `<this>/index.html`, `<this>/pkg/…`, `<this>/assets/…`. */
const PREFIX = 'cockpit';

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

function fail(message) {
  console.error(message);
  process.exit(2);
}

const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
function commit() {
  try {
    return execFileSync('git', ['-C', root, 'rev-parse', '--short=12', 'HEAD'], { encoding: 'utf8' }).trim();
  } catch {
    return 'unknown';
  }
}

/** The full commit the plane was built from: a release is cut from a tag, and the manifest a
 * downloader holds must name the source commit in full, not only its short form in `version`. */
function sourceCommit() {
  try {
    return execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  } catch {
    return 'unknown';
  }
}

/** Every regular file of the plane, `.`-files skipped, ascending by archive path. */
function planeFiles() {
  const files = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir).sort()) {
      if (name.startsWith('.')) continue;
      const full = join(dir, name);
      if (statSync(full).isDirectory()) {
        walk(full);
        continue;
      }
      const path = `${PREFIX}/${relative(join(root, 'cockpit'), full).split(sep).join('/')}`;
      files.push({ path, bytes: readFileSync(full) });
    }
  };
  walk(join(root, 'cockpit', 'assets'));
  walk(join(root, 'cockpit', 'pkg'));
  files.push({ path: `${PREFIX}/index.html`, bytes: readFileSync(join(root, 'cockpit', 'index.html')) });
  return files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

/** The piece digest, exactly as the bundle manifests compute it. */
function digestOf(files) {
  const listing = files
    .map((file) => `${sha256(file.bytes)}  ${file.bytes.length}  ${file.path}`)
    .join('\n');
  return sha256(Buffer.from(`${listing}\n`));
}

function manifestPath(out) {
  return join(out, `drafthouse-cockpit-${version}.manifest.json`);
}
function archivePath(out) {
  return join(out, `drafthouse-cockpit-${version}.tar.gz`);
}

function write(out) {
  const wasm = join(root, 'cockpit', 'pkg', 'drafthouse_cockpit_bg.wasm');
  if (!existsSync(wasm)) {
    fail(`no built cockpit at ${wasm}: run cockpit/tools/build-web.sh release first (exit 2)`);
  }
  const files = planeFiles();
  let digest;
  let entries;
  try {
    ({ digest, entries } = addressPlane(files));
  } catch (error) {
    console.error(`the plane cannot be addressed: ${error.message}`);
    process.exit(1);
  }
  const addressedWasm = entries.find((entry) => entry.path.endsWith('/drafthouse_cockpit_bg.wasm'));
  if (addressedWasm === undefined) {
    console.error(`the addressed plane carries no .wasm — ${wasm} exists but did not reach the payload`);
    process.exit(1);
  }
  mkdirSync(out, { recursive: true });
  const archive = writeArchive(entries);
  writeFileSync(archivePath(out), archive);
  const gzipped = execFileSync('gzip', ['-9', '-c', wasm], { maxBuffer: 64 * 1024 * 1024 });
  const manifest = {
    piece: 'cockpit',
    version: `${version}+${commit()}`,
    command: 'cockpit/tools/build-web.sh release',
    commit: sourceCommit(),
    prefix: PREFIX,
    address: digest,
    files: entries.map((file) => ({ path: file.path, bytes: file.bytes.length, sha256: sha256(file.bytes) })),
    digest: digestOf(entries),
    archive: { name: `drafthouse-cockpit-${version}.tar.gz`, bytes: archive.length, sha256: sha256(archive) },
    wasm: { path: addressedWasm.path, raw_bytes: addressedWasm.bytes.length, gzip_bytes: gzipped.length },
  };
  writeFileSync(manifestPath(out), `${JSON.stringify(manifest, null, 2)}\n`);
  writeFileSync(`${archivePath(out)}.sha256`, `${manifest.archive.sha256}  drafthouse-cockpit-${version}.tar.gz\n`);
  console.log(`cockpit ${manifest.version}: ${entries.length} file(s)`);
  console.log(`  address ${manifest.address} (the served payload's content address)`);
  console.log(`  wasm   ${manifest.wasm.raw_bytes} bytes raw / ${manifest.wasm.gzip_bytes} bytes gzipped`);
  console.log(`  digest ${manifest.digest}`);
  console.log(`  ${archivePath(out)}  ${archive.length} bytes  sha256 ${manifest.archive.sha256}`);
}

/** Re-derives everything from the files themselves: the archive, and the manifest's claims. */
function verify(dir) {
  const manifestFile = manifestPath(dir);
  const archiveFile = archivePath(dir);
  if (!existsSync(manifestFile) || !existsSync(archiveFile)) {
    fail(`no manifest/archive pair in ${dir}: run --write first (exit 2)`);
  }
  const manifest = JSON.parse(readFileSync(manifestFile, 'utf8'));
  const archive = readFileSync(archiveFile);
  const failures = [];
  const entries = readArchive(archive);
  const byPath = new Map(entries.map((entry) => [entry.path, entry.bytes]));

  for (const file of manifest.files) {
    const bytes = byPath.get(file.path);
    if (bytes === undefined) {
      failures.push(`${file.path}: not in the archive`);
      continue;
    }
    if (bytes.length !== file.bytes) failures.push(`${file.path}: archive has ${bytes.length} bytes, the manifest says ${file.bytes}`);
    const sha = sha256(bytes);
    if (sha !== file.sha256) failures.push(`${file.path}: archive sha256 ${sha}, the manifest says ${file.sha256}`);
  }
  for (const entry of entries) {
    if (!manifest.files.some((file) => file.path === entry.path)) failures.push(`${entry.path}: in the archive, not in the manifest`);
  }
  /* Issue #148: the document's own runtime references must resolve inside the plane that will be
     served — a shell asking for a name the host does not carry is worse than the bug. */
  for (const failure of referenceFailures(entries)) failures.push(failure);
  if (!byPath.has(manifest.wasm.path)) failures.push(`the manifest names the wasm at ${manifest.wasm.path}, which is not in the archive`);
  const digest = digestOf(entries);
  if (digest !== manifest.digest) failures.push(`piece digest ${digest}, the manifest says ${manifest.digest}`);
  if (archive.length !== manifest.archive.bytes) failures.push(`archive is ${archive.length} bytes, the manifest says ${manifest.archive.bytes}`);
  const archiveSha = sha256(archive);
  if (archiveSha !== manifest.archive.sha256) failures.push(`archive sha256 ${archiveSha}, the manifest says ${manifest.archive.sha256}`);
  const sidecar = `${archiveFile}.sha256`;
  if (!existsSync(sidecar)) {
    failures.push('the .sha256 sidecar is missing');
  } else if (!readFileSync(sidecar, 'utf8').startsWith(`${manifest.archive.sha256}  `)) {
    failures.push('the .sha256 sidecar does not carry the manifest archive digest');
  }

  if (failures.length) {
    console.error(`cockpit ${manifest.version}: ${failures.length} disagreement(s)`);
    for (const failure of failures) console.error(`  · ${failure}`);
    process.exit(1);
  }
  console.log(`cockpit ${manifest.version}: ${manifest.files.length} file(s) verified from the files themselves`);
  console.log(`  wasm   ${manifest.wasm.raw_bytes} bytes raw / ${manifest.wasm.gzip_bytes} bytes gzipped`);
  console.log(`  digest ${manifest.digest}`);
}

const out = arg('write');
const inDir = arg('verify');
if (out) write(out);
else if (inDir) verify(inDir);
else fail('usage: cockpit-artifact.mjs --write <dir> | --verify <dir> (exit 2)');
