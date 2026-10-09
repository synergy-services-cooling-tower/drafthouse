#!/usr/bin/env node
/**
 * Bundle pin gate — issue #31.
 *
 * Verifies the committed pins (`bundles/<piece>.manifest.json`) against the source tree: it
 * rebuilds the wasm artifact with the recorded command, re-derives every file digest and the
 * piece digest, and fails when the pin and the source disagree. A manifest that cannot go red
 * when the source changes is not a pin, so nothing here reads a digest out of the pin and
 * trusts it — every number is recomputed.
 *
 * The pin checks content, not the commit label: a tree at a later commit whose piece files
 * are byte-identical still passes (the pins are re-cut by `npm run bundle` when a piece
 * changes), while any change to a pinned file — or to the Rust build inputs behind the wasm
 * — is a failure naming the file and both digests.
 *
 * Usage — from any working directory:
 *
 *     node scripts/bundle-check.mjs
 *
 * Exit status: 0 the pin is satisfied; 1 the pin and the tree disagree (the failures are
 * listed); 2 the check could not run (no cargo, build failure, no git, unreadable pin).
 */
import { existsSync, readFileSync } from 'node:fs';
import { relative } from 'node:path';
import { pathToFileURL } from 'node:url';

import { deriveManifests, pinFile, root } from './bundle.mjs';

function refuse(message) {
  console.error(`bundle-check: ${message}`);
  process.exit(2);
}

/** One pinned piece against one freshly derived piece; appends human-readable failures. */
function comparePiece(pin, derived, pinName, failures) {
  if (pin.piece !== derived.piece) {
    failures.push(`${pinName}: the pin names piece ${JSON.stringify(pin.piece)}, expected ${JSON.stringify(derived.piece)}`);
  }
  // A pin that carries no file list cannot be compared, and must not pass by absence.
  if (!Array.isArray(pin.files)) {
    failures.push(`${pinName}: the pin carries no file list`);
    return;
  }
  const pinned = new Map(pin.files.map((file) => [file.path, file]));
  for (const file of derived.files) {
    if (file.missing) {
      failures.push(`${pinName}: the tree does not contain ${file.path}`);
      continue;
    }
    const known = pinned.get(file.path);
    if (!known) {
      failures.push(`${pinName}: ${file.path} is in the piece but not in the pin — run: npm run bundle`);
      continue;
    }
    if (known.sha256 !== file.sha256 || known.bytes !== file.bytes) {
      failures.push(`${pinName}: ${file.path} — pinned ${known.bytes} bytes ${known.sha256}, rebuilt ${file.bytes} bytes ${file.sha256}`);
    }
  }
  for (const path of pinned.keys()) {
    if (!derived.files.some((file) => file.path === path)) {
      failures.push(`${pinName}: ${path} is pinned but not in the piece — run: npm run bundle`);
    }
  }
  if (typeof pin.digest !== 'string' || pin.digest === '') {
    failures.push(`${pinName}: the pin carries no piece digest`);
  } else if (derived.digest && pin.digest !== derived.digest) {
    failures.push(`${pinName}: piece digest pinned ${pin.digest} != derived ${derived.digest}`);
  }
}

function main() {
  const derived = deriveManifests();

  const failures = [];
  const notes = [];
  for (const piece of derived) {
    const file = pinFile(piece.piece);
    const pinName = relative(root, file);
    if (!existsSync(file)) {
      failures.push(`${pinName} is missing — run: npm run bundle`);
      continue;
    }
    let pin;
    try {
      pin = JSON.parse(readFileSync(file, 'utf8'));
    } catch (error) {
      refuse(`${pinName} is not readable JSON: ${error.message}`);
    }
    comparePiece(pin, piece, pinName, failures);
    if (pin.commit !== piece.commit) {
      notes.push(`${pinName} was published from ${pin.commit}; this tree is at ${piece.commit} — the pin checks file digests, not the commit label`);
    }
  }

  if (failures.length) {
    console.error(JSON.stringify({ passed: false, failures }, null, 2));
    process.exit(1);
  }

  for (const note of notes) console.log(`note: ${note}`);
  console.log(JSON.stringify({
    passed: true,
    pins: derived.map((piece) => relative(root, pinFile(piece.piece))),
    version: derived[0].version,
    commit: derived[0].commit,
    pieces: derived.map((piece) => ({
      piece: piece.piece,
      files: piece.files.length,
      bytes: piece.files.reduce((sum, file) => sum + file.bytes, 0),
      digest: piece.digest,
      built: piece.files.filter((file) => file.command).map((file) => file.path),
    })),
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
