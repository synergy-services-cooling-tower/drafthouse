#!/usr/bin/env node
/**
 * Bundle publisher — issue #31, step 2 of the ratified build order: artifact publishing.
 *
 * Publishes the engine and the visuals as pinned bundles (a file list with per-file SHA-256
 * and byte size, plus the piece's own digest) instead of npm packages. The two pieces, and
 * the one reason the split exists:
 *
 *   engine   the wasm build plus its thin, data-only JS binding (`rust/wasm/binding.mjs`):
 *            numerics only — metrics, ranked candidates, the worked step list, and the input
 *            values used (catalog dimensions included).
 *   visuals  what the existing 3D panel draws from: the geometry derivation, the panel
 *            modules and the vendored renderer. Plain JS, so a visual change never triggers
 *            a wasm rebuild — which is the whole reason the split exists.
 *
 * The wasm artifact is not committed. The `engine` manifest records the exact command that
 * produced it
 *
 *     cargo build --manifest-path rust/Cargo.toml --target wasm32-unknown-unknown --profile wasm --lib
 *
 * and its digest; publishing runs that command against the worktree — never copying the
 * artifact out of a build directory the publisher did not create — and a consumer rebuilds
 * and verifies the same digest.
 *
 * Version identity is derived, never typed: `<package.json version>+<HEAD commit>`. Nothing
 * host-dependent (working directory, hostname, timestamps) enters a manifest, so two
 * publishes of the same commit are byte-identical — and `node scripts/bundle-check.mjs`
 * re-derives the digests from the tree and fails when the pin and the source disagree.
 *
 * Usage — from any working directory; paths resolve from this file, not from the cwd:
 *
 *     node scripts/bundle.mjs                 # write the pins to bundles/
 *     node scripts/bundle.mjs --out <dir>     # write them to <dir> instead, same bytes
 *
 * It publishes a committed tree: a tracked modification anywhere other than the pin files
 * themselves is refused, because a manifest records the commit it was published from. Commit
 * the piece change, publish, then commit the manifests.
 *
 * Exit status: 0 written; 2 refused or could not run (dirty tree, missing file, no cargo,
 * build failure, no git). Never 1 — `bundle-check.mjs` is the gate that goes red.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Where the committed pins live, per piece: `bundles/<piece>.manifest.json`. */
export const PIN_DIR = 'bundles';
export const pinFile = (piece) => join(root, PIN_DIR, `${piece}.manifest.json`);

/**
 * Where a pinned file is served from, per piece — the pin's own layout contract, and the one
 * mapping a served copy is placed by (the private deployment procedure §2c). It lived in the retired
 * surface's packager until issue #60; the mapping itself is the pins', so it lives
 * with them.
 *
 *   engine   rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm
 *              -> vendor/engine/synergy_drafthouse.wasm
 *            rust/wasm/binding.mjs          -> vendor/engine/binding.mjs
 *   visuals  web/geometry.js                -> vendor/visuals/geometry.js
 *            web/vendor/three.module.js     -> vendor/visuals/vendor/three.module.js
 */
export function servedPath(piece, path) {
  if (piece === 'engine') return `vendor/${piece}/${basename(path)}`;
  if (piece === 'visuals') return `vendor/${piece}/${relative('web', path)}`;
  throw new Error(`no served-path rule for piece ${JSON.stringify(piece)}`);
}

export function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

const WASM_ARTIFACT = 'rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm';
const WASM_BUILD_COMMAND = 'cargo build --manifest-path rust/Cargo.toml --target wasm32-unknown-unknown --profile wasm --lib';

/**
 * The two pieces. `engine`'s first entry is the built artifact and carries the recorded
 * command; `visuals` enumerates the files `web/vendor/` tracks, so a newly vendored file
 * joins the piece (and re-pins) rather than slipping past the pin. The pin describes the
 * commit: untracked files are not part of any commit and are not part of a piece.
 */
const PIECES = [
  {
    name: 'engine',
    files: () => [
      { path: WASM_ARTIFACT, command: WASM_BUILD_COMMAND },
      { path: 'rust/wasm/binding.mjs' },
    ],
  },
  {
    name: 'visuals',
    files: () => [
      ...[
        'web/geometry.js',
        'web/numbers.js',
        'web/panel-ui.js',
        'web/panel.js',
        'web/panel.css',
        'web/viewer.js',
      ].map((path) => ({ path })),
      ...git(['ls-files', '--cached', '-z', '--', 'web/vendor'])
        .split('\0')
        .filter(Boolean)
        .map((path) => ({ path })),
    ],
  },
];

function refuse(message) {
  console.error(`bundle: ${message}`);
  process.exit(2);
}

/** Raw stdout of a git command run against the repository this script lives in. */
function git(args) {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error) refuse(`git ${args.join(' ')} could not run: ${result.error.message}`);
  if (result.status !== 0) refuse(`git ${args.join(' ')} failed (exit ${result.status}): ${result.stderr.trim()}`);
  return result.stdout;
}

/** The wasm artifact, produced by the recorded command — a failed build refuses, never reads stale bytes. */
function buildWasm() {
  const args = ['build', '--manifest-path', 'rust/Cargo.toml', '--target', 'wasm32-unknown-unknown', '--profile', 'wasm', '--lib'];
  const result = spawnSync('cargo', args, { cwd: root, stdio: ['ignore', 'inherit', 'inherit'] });
  if (result.error) refuse(`cargo could not run: ${result.error.message}`);
  if (result.status !== 0) refuse(`the recorded wasm build command failed (exit ${result.status}); refusing to read the artifact it did not produce`);
  if (!existsSync(join(root, WASM_ARTIFACT))) refuse(`the recorded wasm build command exited 0 but ${WASM_ARTIFACT} does not exist`);
}

/** One file entry: path, byte size and SHA-256; built files carry the command that produced them. */
function fileEntry(entry) {
  const absolute = join(root, entry.path);
  if (!existsSync(absolute)) return { path: entry.path, missing: true };
  const bytes = readFileSync(absolute);
  return {
    path: entry.path,
    bytes: bytes.length,
    sha256: sha256(bytes),
    ...(entry.command ? { command: entry.command } : {}),
  };
}

/**
 * The piece's own digest: SHA-256 of its canonical listing — one line per file, in ascending
 * path order, `<sha256><two spaces><bytes><two spaces><path>`, a trailing newline after the
 * last line. Built files, wasm included, participate by digest like any other.
 */
function listingDigest(files) {
  const listing = `${files.map((file) => `${file.sha256}  ${file.bytes}  ${file.path}`).join('\n')}\n`;
  return sha256(Buffer.from(listing, 'utf8'));
}

/**
 * The two manifests for the current tree, at the commit it is on. Derives nothing from the
 * environment: same tree, same bytes out. Throws nothing — environment failure refuses (2);
 * a missing piece file is reported as `missing: true` so the gate can name it.
 */
export function deriveManifests() {
  const commit = git(['rev-parse', 'HEAD']).trim();
  const version = `${JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version}+${commit}`;
  buildWasm();
  return PIECES.map((piece) => {
    const files = piece.files()
      .map((entry) => fileEntry(entry))
      .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
    return {
      piece: piece.name,
      version,
      commit,
      files,
      digest: files.every((file) => !file.missing) ? listingDigest(files) : null,
    };
  });
}

/**
 * A published manifest must describe a commit: a tracked modification outside the output
 * directory means the label would be false, so publishing is refused until the tree is
 * committed.
 */
function refuseIfDirty(outDir) {
  const outRelative = relative(root, outDir);
  const excluded = outRelative !== '' && !outRelative.startsWith('..') ? outRelative : null;
  const dirty = git(['status', '--porcelain', '--untracked-files=no'])
    .split('\n')
    .filter(Boolean)
    .map((line) => line.slice(3).replace(/^"(.*)"$/, '$1'))
    .filter((path) => !(excluded && (path === excluded || path.startsWith(`${excluded}/`))));
  if (dirty.length) {
    refuse(
      `the tree has tracked modifications (${dirty.slice(0, 5).join(', ')}${dirty.length > 5 ? `, +${dirty.length - 5} more` : ''});`
      + ' a manifest records the commit it was published from — commit the tree first',
    );
  }
}

function main() {
  const outFlag = process.argv.indexOf('--out');
  const outDir = outFlag === -1 ? join(root, PIN_DIR) : resolve(process.argv[outFlag + 1] ?? '');
  if (outFlag !== -1 && !process.argv[outFlag + 1]) refuse('--out needs a directory');

  refuseIfDirty(outDir);
  const manifests = deriveManifests();
  const missing = manifests.flatMap((manifest) => manifest.files.filter((file) => file.missing).map((file) => file.path));
  if (missing.length) refuse(`the tree cannot produce the piece file(s): ${missing.join(', ')}`);

  mkdirSync(outDir, { recursive: true });
  for (const manifest of manifests) {
    writeFileSync(join(outDir, `${manifest.piece}.manifest.json`), `${JSON.stringify(manifest, null, 2)}\n`);
  }

  const relativeOut = relative(root, outDir);
  const out = relativeOut === '' ? '.' : relativeOut.startsWith('..') ? outDir : relativeOut;
  console.log(JSON.stringify({
    published: true,
    version: manifests[0].version,
    commit: manifests[0].commit,
    out,
    pieces: manifests.map((manifest) => ({
      piece: manifest.piece,
      files: manifest.files.length,
      bytes: manifest.files.reduce((sum, file) => sum + file.bytes, 0),
      digest: manifest.digest,
      manifest: `${manifest.piece}.manifest.json`,
      built: manifest.files.filter((file) => file.command).map((file) => file.path),
    })),
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
