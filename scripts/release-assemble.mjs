#!/usr/bin/env node
/**
 * Release assembler — issue #36, slice 1 of #35: the products a release carries.
 *
 * Assembles, at the checked-out commit, the three things a release publishes:
 *
 *   bundle-<version>.tar.gz           the two pinned pieces at their repository-relative
 *                                     paths, plus the committed pins they satisfy,
 *                                     in a deterministic container
 *                                     (`scripts/release-archive.mjs`);
 *   bundle-<version>.manifest.json    every file's SHA-256 and byte size for both pieces,
 *                                     each piece's own digest, the pins', and the archive's
 *                                     own digest and size;
 *   bundle-<version>.provenance.json  the derived version `<package.json version>+<commit>`,
 *                                     the commit and tag, the toolchain channel and the
 *                                     rustc/cargo the build ran under, the Node version, the
 *                                     exact commands, the digests produced — and the explicit
 *                                     list of what the record does not establish.
 *
 * The pieces are not re-derived here: `scripts/bundle.mjs` (the publisher issue #31 added)
 * owns that, and this script runs the same `deriveManifests()` the `bundle-check` gate runs —
 * it rebuilds the wasm artifact with the recorded command and derives every digest from the
 * tree. What the assembler adds is the archive and the two records, and one refusal: the
 * committed pins must still agree with the tree, or the release would carry a pin that its
 * own bytes contradict.
 *
 * Usage — from any working directory:
 *
 *     node scripts/release-assemble.mjs --out <dir> --tag v0.1.0   # a release, from a tag
 *     node scripts/release-assemble.mjs --out <dir> --dry-run      # the same chain, no release
 *
 * Exactly one of `--tag` and `--dry-run` is required, so a run that lost its tag cannot be
 * recorded as a release by accident. `--tag` must name `v<package.json version>`.
 *
 * Exit status: 0 assembled; 2 refused or could not run (dirty tree, tag/version mismatch, a
 * pin that disagrees with the tree, no cargo, build failure, no git).
 */
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { deriveManifests, pinFile, root } from './bundle.mjs';
import { sha256, writeArchive } from './release-archive.mjs';

/**
 * What the release's records say it does not establish. The digests prove the bytes are the
 * ones listed; everything else a reader might assume is named here instead.
 */
const DOES_NOT_ESTABLISH = [
  'No signature: the archive, the manifest and this record are unsigned — no key, no certificate, no attestation. The digests bind the listed bytes to each other, not to a publisher.',
  'No CTI or certification claim: nothing in this release is a CTI certification, a CTI ToolKit result, or any other third-party validation of the numbers. The engineering claims and their status are the repository documents VALIDATION_RESULTS.md and docs/VALIDATION.md.',
  'No host deployment: this release is not deployed anywhere and says nothing about how it would be served. Fetching a pinned bundle, the CSP token and the application/wasm MIME type are a separate procedure, the private deployment procedure.',
];

function refuse(message) {
  console.error(`release-assemble: ${message}`);
  process.exit(2);
}

/** Raw stdout of a git command run against the repository this script lives in. */
function git(args) {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error) refuse(`git ${args.join(' ')} could not run: ${result.error.message}`);
  if (result.status !== 0) refuse(`git ${args.join(' ')} failed (exit ${result.status}): ${result.stderr.trim()}`);
  return result.stdout;
}

/** A tool's `--version`, or a refusal: the record names the tools the build actually ran. */
function toolVersion([command, ...args]) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8' });
  if (result.error) refuse(`${command} could not run: ${result.error.message}`);
  if (result.status !== 0) refuse(`${command} ${args.join(' ')} failed (exit ${result.status}): ${result.stderr.trim()}`);
  return result.stdout.trim();
}

/**
 * A release publishes a committed tree: a tracked modification means the recorded commit no
 * longer describes the bytes. Untracked files (this script's own output directory among them)
 * are not part of any commit and do not count.
 */
function refuseIfDirty() {
  const dirty = git(['status', '--porcelain', '--untracked-files=no']).split('\n').filter(Boolean);
  if (dirty.length) {
    refuse(
      `the tree has tracked modifications (${dirty.slice(0, 5).map((line) => line.slice(3)).join(', ')}`
      + `${dirty.length > 5 ? `, +${dirty.length - 5} more` : ''}); the records name the commit they were assembled from — commit the tree first`,
    );
  }
}

/**
 * The committed pin for each piece must agree with what the tree derives. The assembler does
 * not re-cut pins (that is `npm run bundle`); it refuses to publish bytes whose own pin
 * contradicts them.
 */
function checkPins(derived) {
  const failures = [];
  for (const piece of derived) {
    const path = `bundles/${piece.piece}.manifest.json`;
    let pin;
    try {
      pin = JSON.parse(readFileSync(pinFile(piece.piece), 'utf8'));
    } catch (error) {
      failures.push(`${path} is not readable JSON: ${error.message}`);
      continue;
    }
    if (pin.digest !== piece.digest) {
      failures.push(`${path} records piece digest ${pin.digest}; the tree derives ${piece.digest}`);
    }
    for (const file of piece.files) {
      const pinned = (pin.files ?? []).find((entry) => entry.path === file.path);
      if (!pinned) failures.push(`${path} does not list ${file.path}`);
      else if (pinned.sha256 !== file.sha256 || pinned.bytes !== file.bytes) {
        failures.push(`${path}: ${file.path} pinned ${pinned.bytes} bytes ${pinned.sha256}, rebuilt ${file.bytes} bytes ${file.sha256}`);
      }
    }
    for (const pinned of pin.files ?? []) {
      if (!piece.files.some((file) => file.path === pinned.path)) failures.push(`${path} pins ${pinned.path}, which the piece does not contain`);
    }
  }
  return failures;
}

/** The channel `rust/rust-toolchain.toml` pins — the same line the CI jobs read. */
function toolchainChannel() {
  const text = readFileSync(join(root, 'rust/rust-toolchain.toml'), 'utf8');
  const channel = text.match(/^\s*channel\s*=\s*"([^"]*)"/m);
  if (!channel || channel[1] === '') refuse('rust/rust-toolchain.toml does not pin a toolchain channel');
  return channel[1];
}

/** The exact command line this run was invoked with, so the record names the run, not a wish. */
function invocation() {
  return `node scripts/${basename(process.argv[1])} ${process.argv.slice(2).join(' ')}`.trim();
}

function main() {
  const argv = process.argv.slice(2);
  const value = (flag) => {
    const at = argv.indexOf(flag);
    if (at === -1) return null;
    if (!argv[at + 1] || argv[at + 1].startsWith('--')) refuse(`${flag} needs a value`);
    return argv[at + 1];
  };
  const out = value('--out');
  const tag = value('--tag');
  const dryRun = argv.includes('--dry-run');
  if (!out) refuse('--out needs a directory');
  if (dryRun === (tag !== null)) refuse('exactly one of --tag <tag> and --dry-run is required');
  if (dryRun && argv[argv.indexOf('--dry-run') + 1] !== undefined && !argv[argv.indexOf('--dry-run') + 1].startsWith('--')) {
    refuse('--dry-run takes no value');
  }

  refuseIfDirty();
  const derived = deriveManifests();
  const missing = derived.flatMap((piece) => piece.files.filter((file) => file.missing).map((file) => file.path));
  if (missing.length) refuse(`the tree cannot produce the piece file(s): ${missing.join(', ')}`);
  const { version, commit } = derived[0];
  const packageVersion = version.slice(0, version.lastIndexOf('+'));
  if (tag !== null && tag !== `v${packageVersion}`) {
    refuse(`the tag ${JSON.stringify(tag)} does not name v${packageVersion} — the release tag is the derived version, never a typed one`);
  }
  const pinFailures = checkPins(derived);
  if (pinFailures.length) refuse(`the committed pins disagree with the tree:\n  ${pinFailures.join('\n  ')}\nrun: npm run bundle-check`);

  const pins = derived.map((piece) => {
    const path = `bundles/${piece.piece}.manifest.json`;
    const bytes = readFileSync(pinFile(piece.piece));
    return { path, bytes, piece: piece.piece, digest: piece.digest };
  });
  const entries = [
    ...derived.flatMap((piece) => piece.files.map((file) => ({ path: file.path, bytes: readFileSync(join(root, file.path)) }))),
    ...pins.map((pin) => ({ path: pin.path, bytes: pin.bytes })),
  ];

  const archiveName = `bundle-${version}.tar.gz`;
  const archive = writeArchive(entries);
  const manifestName = `bundle-${version}.manifest.json`;
  const provenanceName = `bundle-${version}.provenance.json`;

  const manifest = {
    manifest: 1,
    version,
    commit,
    tag,
    archive: { name: archiveName, bytes: archive.length, sha256: sha256(archive) },
    pieces: derived.map((piece) => ({
      piece: piece.piece,
      digest: piece.digest,
      files: piece.files.map((file) => ({
        path: file.path,
        bytes: file.bytes,
        sha256: file.sha256,
        ...(file.command ? { command: file.command } : {}),
      })),
    })),
    pins: pins.map((pin) => ({
      path: pin.path,
      piece: pin.piece,
      bytes: pin.bytes.length,
      sha256: sha256(pin.bytes),
      digest: pin.digest,
    })),
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);

  const provenance = {
    provenance: 1,
    mode: dryRun ? 'dry-run' : 'tag',
    version,
    commit,
    tag,
    toolchain: {
      channel: toolchainChannel(),
      rustc: toolVersion(['rustc', '--version']),
      cargo: toolVersion(['cargo', '--version']),
    },
    node: process.version,
    commands: [
      {
        step: 'engine and pieces',
        command: derived.flatMap((piece) => piece.files.filter((file) => file.command).map((file) => file.command))[0],
      },
      { step: 'assemble', command: invocation() },
      {
        step: 'archive container',
        command: 'in-process ustar entries sorted by path (mode 0644, uid/gid 0, mtime 0) gzipped at level 9 — no host tar or gzip runs',
      },
    ],
    digests: {
      archive: { name: archiveName, bytes: archive.length, sha256: manifest.archive.sha256 },
      manifest: { name: manifestName, bytes: manifestBytes.length, sha256: sha256(manifestBytes) },
      pieces: Object.fromEntries(derived.map((piece) => [piece.piece, piece.digest])),
      pins: Object.fromEntries(pins.map((pin) => [pin.piece, sha256(pin.bytes)])),
    },
    doesNotEstablish: DOES_NOT_ESTABLISH,
  };
  const provenanceBytes = Buffer.from(`${JSON.stringify(provenance, null, 2)}\n`);

  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, archiveName), archive);
  writeFileSync(join(out, manifestName), manifestBytes);
  writeFileSync(join(out, provenanceName), provenanceBytes);

  console.log(JSON.stringify({
    assembled: true,
    mode: provenance.mode,
    version,
    commit,
    tag,
    out,
    products: [
      { name: archiveName, bytes: archive.length, sha256: sha256(archive) },
      { name: manifestName, bytes: manifestBytes.length, sha256: sha256(manifestBytes) },
      { name: provenanceName, bytes: provenanceBytes.length, sha256: sha256(provenanceBytes) },
    ],
    pieces: manifest.pieces.map((piece) => ({
      piece: piece.piece,
      files: piece.files.length,
      bytes: piece.files.reduce((sum, file) => sum + file.bytes, 0),
      digest: piece.digest,
    })),
    archiveEntries: entries.length,
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
