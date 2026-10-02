#!/usr/bin/env node
/**
 * Archive reproducibility measurement — issue #36, slice 1 of #35.
 *
 * Two assemblies of the same commit either produce the same archive bytes or they do not, and
 * this repository measures which one it is instead of claiming it. The workflow assembles
 * twice — the second time after removing the built wasm artifact, so the second run really
 * rebuilds it — and hands both product directories here; the answer is a record of what was
 * measured.
 *
 * The container is normalised by `scripts/release-archive.mjs` (entries sorted by path, mode
 * 0644, uid/gid 0, mtime 0, gzip with no name and no mtime), and this comparison is what
 * holds that normalisation to account.
 *
 * Usage — from any working directory:
 *
 *     node scripts/release-repro.mjs <first-dir> <second-dir> [--out <file>]
 *
 * Exit status: 0 the two archives are byte-identical; 1 they hold the same files but the
 * archive bytes differ, or the manifest/provenance records differ, with the difference
 * enumerated; 2 the contents themselves differ (or either directory could not be read).
 */
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { readArchive, sha256 } from './release-archive.mjs';

function refuse(message) {
  console.error(`release-repro: ${message}`);
  process.exit(2);
}

/** The three products of one assembly: the manifest, the archive it names, the provenance. */
function products(dir) {
  if (!existsSync(dir) || !statSync(dir).isDirectory()) refuse(`${dir} is not a directory`);
  const names = readdirSync(dir).filter((name) => statSync(join(dir, name)).isFile());
  const sole = (suffix) => {
    const matches = names.filter((name) => name.endsWith(suffix));
    if (matches.length !== 1) refuse(`${dir} must hold exactly one *${suffix}; found ${matches.length}`);
    return matches[0];
  };
  const manifestName = sole('.manifest.json');
  const provenanceName = sole('.provenance.json');
  const manifestBytes = readFileSync(join(dir, manifestName));
  let manifest;
  try {
    manifest = JSON.parse(manifestBytes.toString('utf8'));
  } catch (error) {
    refuse(`${manifestName} is not readable JSON: ${error.message}`);
  }
  const archiveName = manifest.archive?.name;
  if (typeof archiveName !== 'string' || !existsSync(join(dir, archiveName))) {
    refuse(`${dir} does not hold the archive its manifest names: ${JSON.stringify(archiveName)}`);
  }
  return {
    dir,
    manifestBytes,
    manifest,
    archiveName,
    archive: readFileSync(join(dir, archiveName)),
    provenanceBytes: readFileSync(join(dir, provenanceName)),
  };
}

/** Every file of an archive keyed by path, with size and digest — the comparison's unit. */
function members(archiveBytes, archiveName) {
  let entries;
  try {
    entries = readArchive(archiveBytes);
  } catch (error) {
    refuse(`archive ${archiveName} could not be read: ${error.message}`);
  }
  return new Map(entries.map((entry) => [entry.path, { bytes: entry.bytes.length, sha256: sha256(entry.bytes) }]));
}

function main() {
  const argv = process.argv.slice(2);
  const outAt = argv.indexOf('--out');
  const out = outAt === -1 ? null : argv[outAt + 1];
  if (outAt !== -1 && !out) refuse('--out needs a file');
  const dirs = argv.filter((arg, index) => !arg.startsWith('--') && !(outAt !== -1 && index === outAt + 1));
  if (dirs.length !== 2) refuse('two assembled product directories are required');

  const [first, second] = dirs.map((dir) => products(dir));
  if (first.manifest.version !== second.manifest.version) {
    refuse(`the two assemblies name different versions: ${first.manifest.version} and ${second.manifest.version}`);
  }

  const archiveIdentical = first.archive.equals(second.archive);
  const manifestIdentical = first.manifestBytes.equals(second.manifestBytes);
  const provenanceIdentical = first.provenanceBytes.equals(second.provenanceBytes);

  const differences = [];
  let contentsIdentical = true;
  if (!archiveIdentical) {
    const left = members(first.archive, first.archiveName);
    const right = members(second.archive, second.archiveName);
    for (const path of left.keys()) {
      if (!right.has(path)) differences.push(`${path} is in ${first.archiveName} but not in ${second.archiveName}`);
    }
    for (const path of right.keys()) {
      if (!left.has(path)) differences.push(`${path} is in ${second.archiveName} but not in ${first.archiveName}`);
    }
    for (const [path, entry] of left) {
      const other = right.get(path);
      if (!other) continue;
      if (entry.sha256 !== other.sha256 || entry.bytes !== other.bytes) {
        differences.push(`${path}: ${entry.bytes} bytes ${entry.sha256} vs ${other.bytes} bytes ${other.sha256}`);
      }
    }
    contentsIdentical = differences.length === 0;
  }
  if (contentsIdentical && !manifestIdentical) {
    differences.push(`the two manifests differ: ${sha256(first.manifestBytes)} vs ${sha256(second.manifestBytes)}`);
  }
  if (contentsIdentical && !provenanceIdentical) {
    differences.push(`the two provenance records differ: ${sha256(first.provenanceBytes)} vs ${sha256(second.provenanceBytes)}`);
  }

  const result = {
    measured: true,
    version: first.manifest.version,
    commit: first.manifest.commit,
    archive: {
      name: first.archiveName,
      bytes: first.archive.length,
      sha256: sha256(first.archive),
      secondSha256: sha256(second.archive),
    },
    archiveIdentical,
    contentsIdentical,
    manifestIdentical,
    provenanceIdentical,
    differences,
  };
  if (out) writeFileSync(out, `${JSON.stringify(result, null, 2)}\n`);
  const text = JSON.stringify(result, null, 2);

  if (!contentsIdentical) {
    console.error(text);
    process.exit(2);
  }
  if (!archiveIdentical || !manifestIdentical || !provenanceIdentical) {
    console.error(text);
    process.exit(1);
  }
  console.log(text);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
