#!/usr/bin/env node
/**
 * Release verifier — issue #36, slice 1 of #35: the fresh-download re-verification.
 *
 * Takes a directory of **published** assets — fetched from a Release with the workflow token
 * on a tag run, fetched from the run's workflow artifacts on a dry run — and re-derives every
 * hash in them from the bytes that arrived, failing loudly on any disagreement. It is the job
 * that runs *after* the download, in a different job from the one that produced the bytes, so
 * "published" is a round trip and not a claim.
 *
 * What it checks, all of it recomputed and none of it taken on trust:
 *
 *   archive    the published archive's byte size and SHA-256 against the manifest's record;
 *   files      every file the manifest lists is present in the archive, at its path, with the
 *              recorded byte size and the recorded SHA-256 — and the archive holds nothing
 *              the manifest does not list;
 *   pieces     each piece's own digest, re-derived from the archive's files with the canonical
 *              listing formula this repository pins with (the private release procedure);
 *   pins       the committed pins inside the archive still list the files the manifest lists,
 *              with the same digests — the release carries the pin it satisfies;
 *   provenance the record's version, commit, tag, mode and every digest it names agree with
 *              the manifest, and the manifest's own digest is the one the record names.
 *
 * Usage — from any working directory:
 *
 *     node scripts/release-verify.mjs --dir <downloaded-assets-directory>
 *
 * Exit status: 0 every hash verified; 1 a hash, a size, the file set or a record disagrees
 * (every failure is listed); 2 the check could not run (no directory, no manifest or
 * provenance, ambiguous names, unreadable JSON).
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { readArchive, sha256 } from './release-archive.mjs';

function refuse(message) {
  console.error(`release-verify: ${message}`);
  process.exit(2);
}

/**
 * A piece's own digest: SHA-256 of its canonical listing — one line per file in ascending
 * path order, `<sha256><two spaces><bytes><two spaces><path>`, a trailing newline. The same
 * formula `scripts/bundle.mjs` derives the committed pins with, and the one the private release procedure
 * records. It is re-derived here from the *archive's* bytes, never from the manifest's record.
 */
function listingDigest(files) {
  const listing = `${files.map((file) => `${file.sha256}  ${file.bytes}  ${file.path}`).join('\n')}\n`;
  return sha256(Buffer.from(listing, 'utf8'));
}

function main() {
  const argv = process.argv.slice(2);
  const at = argv.indexOf('--dir');
  const dir = at === -1 ? null : argv[at + 1];
  if (!dir) refuse('--dir needs the directory of downloaded release assets');
  if (!existsSync(dir) || !statSync(dir).isDirectory()) refuse(`${dir} is not a directory`);
  const names = readdirSync(dir).filter((name) => statSync(join(dir, name)).isFile());
  const sole = (suffix) => {
    const matches = names.filter((name) => name.endsWith(suffix));
    if (matches.length !== 1) refuse(`${dir} must hold exactly one *${suffix}; found ${matches.length}`);
    return matches[0];
  };
  const manifestName = sole('.manifest.json');
  const provenanceName = sole('.provenance.json');

  let manifest;
  let provenance;
  let manifestBytes;
  try {
    manifestBytes = readFileSync(join(dir, manifestName));
    manifest = JSON.parse(manifestBytes.toString('utf8'));
  } catch (error) {
    refuse(`${manifestName} is not readable JSON: ${error.message}`);
  }
  try {
    provenance = JSON.parse(readFileSync(join(dir, provenanceName), 'utf8'));
  } catch (error) {
    refuse(`${provenanceName} is not readable JSON: ${error.message}`);
  }

  const failures = [];
  const pieces = Array.isArray(manifest.pieces) ? manifest.pieces : [];
  const pins = Array.isArray(manifest.pins) ? manifest.pins : [];
  const archiveName = manifest.archive?.name;

  let archiveBytes = null;
  if (typeof archiveName !== 'string' || archiveName === '') {
    failures.push('the manifest names no archive');
  } else if (!existsSync(join(dir, archiveName))) {
    failures.push(`the published set does not hold the archive the manifest names: ${archiveName}`);
  } else {
    archiveBytes = readFileSync(join(dir, archiveName));
    if (archiveBytes.length !== manifest.archive.bytes) {
      failures.push(`archive ${archiveName}: the manifest records ${manifest.archive.bytes} bytes, the published file is ${archiveBytes.length}`);
    }
    const digest = sha256(archiveBytes);
    if (digest !== manifest.archive.sha256) {
      failures.push(`archive ${archiveName}: the manifest records sha256 ${manifest.archive.sha256}, the published file is ${digest}`);
    }
  }

  let entries = [];
  if (archiveBytes) {
    try {
      entries = readArchive(archiveBytes);
    } catch (error) {
      failures.push(`archive ${archiveName} could not be read as the container this repository writes: ${error.message}`);
    }
  }
  const byPath = new Map(entries.map((entry) => [entry.path, entry.bytes]));

  // Every file the manifest lists, against the bytes that arrived.
  const expected = new Map();
  for (const piece of pieces) {
    for (const file of Array.isArray(piece.files) ? piece.files : []) expected.set(file.path, { file, piece: piece.piece });
  }
  for (const pin of pins) expected.set(pin.path, { pin });
  for (const path of byPath.keys()) {
    if (!expected.has(path)) failures.push(`the archive holds ${path}, which the manifest does not list`);
  }
  for (const [path, want] of expected) {
    const bytes = byPath.get(path);
    if (!bytes) {
      failures.push(`${path} is listed in the manifest but not in the archive`);
      continue;
    }
    const record = want.file ?? want.pin;
    const digest = sha256(bytes);
    if (bytes.length !== record.bytes) failures.push(`${path}: the manifest records ${record.bytes} bytes, the archive has ${bytes.length}`);
    if (digest !== record.sha256) failures.push(`${path}: the manifest records sha256 ${record.sha256}, the archive has ${digest}`);
  }

  // Each piece's own digest, re-derived from the archive's bytes.
  for (const piece of pieces) {
    const listed = Array.isArray(piece.files) ? piece.files : [];
    const derived = listed
      .map((file) => ({ path: file.path, bytes: byPath.get(file.path) }))
      .filter((file) => file.bytes !== undefined)
      .map((file) => ({ path: file.path, bytes: file.bytes.length, sha256: sha256(file.bytes) }))
      .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
    if (typeof piece.digest !== 'string' || piece.digest === '') {
      failures.push(`piece ${piece.piece} carries no digest`);
      continue;
    }
    if (derived.length !== listed.length) continue; // the missing files are already reported
    const digest = listingDigest(derived);
    if (digest !== piece.digest) {
      failures.push(`piece ${piece.piece}: the manifest records digest ${piece.digest}; the archive's files derive ${digest}`);
    }
  }

  // The committed pins the archive carries must list exactly what the manifest lists.
  const pieceByName = new Map(pieces.map((piece) => [piece.piece, piece]));
  for (const pin of pins) {
    const bytes = byPath.get(pin.path);
    if (!bytes) continue; // already reported above
    let pinned;
    try {
      pinned = JSON.parse(bytes.toString('utf8'));
    } catch (error) {
      failures.push(`${pin.path} inside the archive is not readable JSON: ${error.message}`);
      continue;
    }
    const piece = pieceByName.get(pin.piece);
    if (!piece) {
      failures.push(`${pin.path} names the piece ${JSON.stringify(pin.piece)}, which the manifest does not carry`);
      continue;
    }
    if (pinned.piece !== pin.piece) failures.push(`${pin.path}: the pin names the piece ${JSON.stringify(pinned.piece)}`);
    if (pinned.digest !== piece.digest) failures.push(`${pin.path}: the pin records piece digest ${pinned.digest}; the manifest records ${piece.digest}`);
    for (const file of Array.isArray(piece.files) ? piece.files : []) {
      const listed = (Array.isArray(pinned.files) ? pinned.files : []).find((entry) => entry.path === file.path);
      if (!listed) failures.push(`${pin.path} does not list ${file.path}`);
      else if (listed.sha256 !== file.sha256 || listed.bytes !== file.bytes) {
        failures.push(`${pin.path}: ${file.path} pinned ${listed.bytes} bytes ${listed.sha256}; the manifest records ${file.bytes} bytes ${file.sha256}`);
      }
    }
  }

  // The provenance record against the manifest it arrived with.
  const provenanceChecks = [
    ['version', provenance.version, manifest.version],
    ['commit', provenance.commit, manifest.commit],
    ['tag', provenance.tag ?? null, manifest.tag ?? null],
    ['archive digest', provenance.digests?.archive?.sha256, manifest.archive?.sha256],
    ['manifest digest', provenance.digests?.manifest?.sha256, sha256(manifestBytes)],
  ];
  for (const [what, recorded, actual] of provenanceChecks) {
    if (recorded !== actual) failures.push(`provenance ${what}: the record says ${JSON.stringify(recorded)}; the published assets say ${JSON.stringify(actual)}`);
  }
  if (provenance.mode === 'tag' && !provenance.tag) failures.push('the provenance records mode tag but names no tag');
  if (provenance.mode === 'dry-run' && provenance.tag) failures.push(`the provenance records mode dry-run but names the tag ${JSON.stringify(provenance.tag)}`);
  for (const piece of pieces) {
    if (provenance.digests?.pieces?.[piece.piece] !== piece.digest) {
      failures.push(`provenance piece digest for ${piece.piece}: the record says ${JSON.stringify(provenance.digests?.pieces?.[piece.piece])}; the manifest says ${JSON.stringify(piece.digest)}`);
    }
  }
  for (const pin of pins) {
    if (provenance.digests?.pins?.[pin.piece] !== pin.sha256) {
      failures.push(`provenance pin digest for ${pin.piece}: the record says ${JSON.stringify(provenance.digests?.pins?.[pin.piece])}; the manifest says ${JSON.stringify(pin.sha256)}`);
    }
  }
  if (!Array.isArray(provenance.doesNotEstablish) || provenance.doesNotEstablish.length === 0) {
    failures.push('the provenance record does not state what it does not establish');
  }
  if (typeof provenance.toolchain?.channel !== 'string' || provenance.toolchain.channel === '') failures.push('the provenance records no toolchain channel');
  if (typeof provenance.node !== 'string' || provenance.node === '') failures.push('the provenance records no Node version');
  if (!Array.isArray(provenance.commands) || provenance.commands.length === 0) failures.push('the provenance records no commands');

  if (failures.length) {
    console.error(JSON.stringify({ verified: false, dir, failures }, null, 2));
    process.exit(1);
  }

  console.log(JSON.stringify({
    verified: true,
    dir,
    version: manifest.version,
    commit: manifest.commit,
    tag: manifest.tag ?? null,
    archive: { name: archiveName, bytes: manifest.archive.bytes, sha256: manifest.archive.sha256 },
    pieces: pieces.map((piece) => ({ piece: piece.piece, files: piece.files.length, digest: piece.digest })),
    filesVerified: expected.size,
    provenance: { mode: provenance.mode, node: provenance.node, toolchain: provenance.toolchain.channel, doesNotEstablish: provenance.doesNotEstablish.length },
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
