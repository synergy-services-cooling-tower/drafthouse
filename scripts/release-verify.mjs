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
 *     node scripts/release-verify.mjs --dir <downloaded-assets-directory> --expect-tamper archive
 *
 * `--expect-tamper archive|manifest` is for the run that dispatched a tamper input (issue #72):
 * the run says it corrupted the bundle one of those two ways, so the verifier *labels* the
 * disagreements that corruption explains, in its failure output, in the job summary and as
 * `expectedTamper` in the JSON. It is labelling only — the exit status stays 1, every check stays
 * exactly as strict, and the label can only ever cover the digests the named corruption moves.
 * Any disagreement those do not explain stays a real failure; a run that finds no disagreement at
 * all fails as a self-test that did not bite. Without the flag the output is unchanged: a
 * mismatch is an unqualified failure.
 *
 * Exit status: 0 every hash verified; 1 a hash, a size, the file set or a record disagrees
 * (every failure is listed); 2 the check could not run (no directory, no manifest or
 * provenance, ambiguous names, unreadable JSON, an unknown expected tamper mode).
 */
import { appendFileSync, existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { readArchive, sha256 } from './release-archive.mjs';

function refuse(message) {
  console.error(`release-verify: ${message}`);
  process.exit(2);
}

/**
 * The dispatch's tamper scope (issue #72): which failure shapes the named corruption explains,
 * and nothing more. `archive` flips one payload byte inside the archive and re-packs the
 * container, so the archive's own digest and size, the tampered file's digest and the piece
 * digest derived from it move — those four are it. `manifest` changes one recorded file digest
 * in the manifest after the provenance recorded the manifest's own digest, so the manifest's
 * own digest comparison, the tampered file's digest comparison and the pin that still lists the
 * untampered digest move — those three are it. Anything else — a missing file, an unreadable
 * archive, a record that disagrees about the version — stays a real failure.
 *
 * A shape alone is not enough: the label is only credited while the corruption's own footprint
 * is present (the archive's own digest line for `archive`, the provenance manifest-digest line
 * for `manifest`). Without that signal nothing is credited — so a mismatching digest cannot ride
 * the label in a run where the corruption the label names never touched the bytes in question.
 */
const TAMPER_EXPLAINS = {
  archive: [
    /^archive .*: the manifest records \d+ bytes, the published file is \d+$/,
    /^archive .*: the manifest records sha256 [0-9a-f]+, the published file is [0-9a-f]+$/,
    /^.+: the manifest records sha256 [0-9a-f]+, the archive has [0-9a-f]+$/,
    /^piece .*: the manifest records digest [0-9a-f]+; the archive's files derive [0-9a-f]+$/,
  ],
  manifest: [
    /^.+: the manifest records sha256 [0-9a-f]+, the archive has [0-9a-f]+$/,
    /^.*: .* pinned \d+ bytes [0-9a-f]+; the manifest records \d+ bytes [0-9a-f]+$/,
    /^provenance manifest digest: /,
  ],
};

/** The footprint the named corruption always leaves, without which the label credits nothing. */
const TAMPER_SIGNAL = {
  archive: [
    /^archive .*: the manifest records \d+ bytes, the published file is \d+$/,
    /^archive .*: the manifest records sha256 [0-9a-f]+, the published file is [0-9a-f]+$/,
  ],
  manifest: [/^provenance manifest digest: /],
};

/** The scope object for the named mode; `explains` is the only thing a failure may match. */
function tamperScope(mode) {
  return {
    mode,
    explains: (failure) => TAMPER_EXPLAINS[mode].some((pattern) => pattern.test(failure)),
    signal: (failure) => TAMPER_SIGNAL[mode].some((pattern) => pattern.test(failure)),
    explained: [],
    unexplained: [],
    didNotBite: false,
  };
}

/** The corruption the named mode performs, as the failure output should state it. */
const TAMPER_CHANGE = {
  archive: 'flipped one payload byte inside the bundle archive and re-packed the container, so the archive\'s bytes — and the file and piece digests derived from them — cannot agree with the manifest\'s records',
  manifest: 'changed one recorded file digest inside the manifest after the provenance record that digests the manifest was written, so the record and the bytes cannot agree with each other',
};

/**
 * Names the self-test, in the failure output and in the job summary (issue #72). This is the
 * surface a reader of a failed run sees, and the one this defect cost: the output alone read as
 * a release defect. It states what the label covers and, when anything is not covered, says so
 * instead of claiming the expected RED. The exit status is not touched — a designed RED is a RED.
 */
function reportTamperScope(scope) {
  const say = (line) => console.error(`release-verify: ${line}`);
  const summary = [];
  if (scope.didNotBite) {
    say(`TAMPER DISPATCH (tamper=${scope.mode}) — THE SELF-TEST DID NOT BITE.`);
    say(`this run dispatched the tamper self-test, which ${TAMPER_CHANGE[scope.mode]}; every hash verified anyway.`);
    say('a self-test that cannot fail is not a self-test — the verifier, the tamper step or the download path is not doing its job.');
    summary.push(
      `### TAMPER SELF-TEST DID NOT BITE — tamper=${scope.mode}`,
      '',
      `This dispatch carried the tamper self-test (\`tamper=${scope.mode}\`), so \`release-verify\` was required to find the disagreement the tamper makes. It found no disagreement at all.`,
      '',
      '**A defect in the self-test — not a release defect.**',
    );
  } else if (scope.unexplained.length === 0) {
    say(`EXPECTED RED — the tamper self-test (tamper=${scope.mode}).`);
    say(`this dispatch deliberately ${TAMPER_CHANGE[scope.mode]}. Every failure below is that corruption's own disagreement: the self-test biting as designed, and this job failing is its designed outcome.`);
    say('it is NOT a release defect, and it says nothing about the published assets. The JSON below carries it as `expectedTamper`.');
    summary.push(
      `### EXPECTED RED — tamper self-test (tamper=${scope.mode})`,
      '',
      `This dispatch deliberately ${TAMPER_CHANGE[scope.mode]}. Every failure of this run is that corruption's own disagreement: \`release-verify\` failing is the self-test biting as designed.`,
      '',
      '**Not a release defect** — nothing here says the published assets are wrong.',
    );
  } else {
    say(`TAMPER DISPATCH (tamper=${scope.mode}) — THIS FAILURE IS NOT FULLY EXPLAINED BY THE SELF-TEST.`);
    say(`${scope.explained.length} disagreement(s) below are the self-test's expected RED; ${scope.unexplained.length} further disagreement(s) are NOT explained by it and are real verification failures:`);
    for (const failure of scope.unexplained) say(`  unexpected: ${failure}`);
    summary.push(
      `### NOT an expected RED — tamper=${scope.mode}`,
      '',
      `This dispatch carried the tamper self-test, but ${scope.unexplained.length} of its ${scope.explained.length + scope.unexplained.length} failures are NOT explained by it and are **real verification failures**:`,
      '',
      ...scope.unexplained.map((failure) => `- \`${failure}\``),
    );
  }
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (path) {
    // The job summary is a courtesy: if it cannot be written the banner above still carries the
    // same words and the exit status is untouched either way.
    try { appendFileSync(path, `${summary.join('\n')}\n`); } catch { /* see above */ }
  }
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
  const tamperAt = argv.indexOf('--expect-tamper');
  const expectTamper = tamperAt === -1 ? null : argv[tamperAt + 1];
  if (!dir) refuse('--dir needs the directory of downloaded release assets');
  if (tamperAt !== -1 && (!argv[tamperAt + 1] || argv[tamperAt + 1].startsWith('--'))) refuse('--expect-tamper needs a value');
  if (expectTamper !== null && !Object.keys(TAMPER_EXPLAINS).includes(expectTamper)) {
    refuse(`--expect-tamper must be one of ${Object.keys(TAMPER_EXPLAINS).join(', ')} — the bundle's modes; the native self-test (tamper=native) belongs to scripts/native-verify.mjs (got ${JSON.stringify(expectTamper)})`);
  }
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

  // The dispatch's tamper, labelled (issue #72): a run that carries the self-test names the
  // corruption it performed, and only the disagreements that corruption moves are marked. The
  // exit status stays 1 either way — a designed RED is a RED — and a run whose tamper explains
  // nothing, or that finds nothing at all, is not allowed to pass.
  const tamper = expectTamper === null ? null : tamperScope(expectTamper);
  if (tamper) {
    if (failures.length === 0) {
      tamper.didNotBite = true;
      failures.push(`this run dispatched tamper=${tamper.mode}, which ${TAMPER_CHANGE[tamper.mode]}, yet every hash verified`);
    } else {
      const credited = failures.some(tamper.signal);
      for (const failure of failures) (credited && tamper.explains(failure) ? tamper.explained : tamper.unexplained).push(failure);
    }
  }

  if (failures.length) {
    if (tamper) reportTamperScope(tamper);
    console.error(JSON.stringify({
      verified: false,
      dir,
      failures,
      ...(tamper ? {
        expectedTamper: {
          mode: tamper.mode,
          explainsAllFailures: tamper.explained.length > 0 && tamper.unexplained.length === 0,
          didNotBite: tamper.didNotBite,
          explainedFailures: tamper.explained,
          unexplainedFailures: tamper.unexplained,
        },
      } : {}),
    }, null, 2));
    if (tamper && !tamper.didNotBite && tamper.unexplained.length === 0) {
      console.error(`release-verify: EXPECTED RED — the tamper self-test (tamper=${tamper.mode}), not a release defect.`);
    }
    if (tamper && tamper.unexplained.length > 0) {
      console.error(`release-verify: NOT an expected RED — ${tamper.unexplained.length} failure(s) are outside the tamper self-test (tamper=${tamper.mode}).`);
    }
    if (tamper && tamper.didNotBite) {
      console.error(`release-verify: TAMPER SELF-TEST DID NOT BITE (tamper=${tamper.mode}) — a defect in the self-test, not a release defect.`);
    }
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
