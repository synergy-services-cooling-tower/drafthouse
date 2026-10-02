#!/usr/bin/env node
/**
 * The native installers' aggregate record — issue #72.
 *
 * The four installers are built by three different runners, so no single job sees them all. Each
 * platform job writes its own record (`scripts/native-artifacts.mjs`); this script runs once,
 * after all three, and turns those records into the pair the bundle already has: a **manifest**
 * (every artifact with its byte size and SHA-256) and a **provenance** record (the mode, the
 * commit, the platform jobs' exact commands and signing state, the digests, and what none of it
 * establishes).
 *
 *     node scripts/native-manifest.mjs --records dist/native-records --out dist/native-manifest --dry-run
 *     node scripts/native-manifest.mjs --records dist/native-records --out dist/native-manifest --tag v0.2.0
 *
 * Exactly one of `--tag` and `--dry-run` is required, and `--tag` must name `v<version>` — the same
 * refusal `scripts/release-assemble.mjs` makes, for the same reason: a run that lost its tag must
 * not be recorded as a release.
 *
 * The manifest's own digest is in the provenance, so a fresh download can check the record against
 * the manifest it arrived with (`scripts/native-verify.mjs`). Nothing here re-hashes an artifact:
 * the bytes were hashed where they were built, and the verification job is the one that re-derives
 * every one of them from what it downloads.
 *
 * Exit status: 0 assembled; 2 refused or could not run (a record missing, a platform the contract
 * does not know, records that disagree about the version or the commit, a tag that does not name
 * the version, an output directory that cannot be written).
 */
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { DOES_NOT_ESTABLISH, PLATFORMS, artifactVersion, git, recordName, sha256 } from './native-products.mjs';

function refuse(message) {
  console.error(`native-manifest: ${message}`);
  process.exit(2);
}

function main() {
  const argv = process.argv.slice(2);
  const value = (flag) => {
    const at = argv.indexOf(flag);
    if (at === -1) return null;
    if (!argv[at + 1] || argv[at + 1].startsWith('--')) refuse(`${flag} needs a value`);
    return argv[at + 1];
  };
  const records = value('--records');
  const out = value('--out');
  const tag = value('--tag');
  const dryRun = argv.includes('--dry-run');
  if (!records) refuse('--records needs the directory the platform records were downloaded into');
  if (!out) refuse('--out needs a directory');
  if (dryRun === (tag !== null)) refuse('exactly one of --tag <tag> and --dry-run is required');

  const { version, refused } = artifactVersion();
  if (refused) refuse(refused);
  if (tag !== null && tag !== `v${version}`) {
    refuse(`the tag ${JSON.stringify(tag)} does not name v${version} — the release tag is the derived version, never a typed one`);
  }
  const commit = git(['rev-parse', 'HEAD']);
  if (!commit) refuse('git rev-parse HEAD did not answer — the records name the commit they were assembled from');

  const names = readdirSync(records).filter((name) => name.endsWith('.record.json')).sort();
  const known = new Map(Object.keys(PLATFORMS).map((platform) => [recordName(platform), platform]));
  for (const name of names) {
    if (!known.has(name)) refuse(`${join(records, name)} is not a record this contract knows (${[...known.keys()].join(', ')})`);
  }
  const missing = [...known.keys()].filter((name) => !names.includes(name));
  if (missing.length) {
    refuse(`the platform record(s) ${missing.join(', ')} are missing from ${records} — every platform job writes one, and a release missing one of the four installers is not a release (the aggregate is written here, after all three runners, never by the job that built one of them)`);
  }

  const platformRecords = names.map((name) => {
    const path = join(records, name);
    let record;
    try {
      record = JSON.parse(readFileSync(path, 'utf8'));
    } catch (error) {
      refuse(`${path} is not readable JSON: ${error.message}`);
    }
    const platform = known.get(name);
    if (record.platform !== platform) refuse(`${path} names the platform ${JSON.stringify(record.platform)}; its file name says ${platform}`);
    if (record.version !== version) {
      refuse(`${path} was built from version ${JSON.stringify(record.version)}; this tree declares ${version} — the records and the release must name the same version`);
    }
    if (record.commit !== commit) {
      refuse(`${path} was built from commit ${JSON.stringify(record.commit)}; this checkout is ${commit} — the records and the release must be the same commit`);
    }
    if (!Array.isArray(record.artifacts) || record.artifacts.length === 0) refuse(`${path} lists no artifacts`);
    const expected = PLATFORMS[platform].products.map((product) => product.name(version)).sort();
    const actual = record.artifacts.map((artifact) => artifact.name).sort();
    if (expected.join(',') !== actual.join(',')) {
      refuse(`${path} lists ${actual.join(', ')}; ${platform} publishes ${expected.join(', ')}`);
    }
    return { platform, record };
  });

  const artifacts = platformRecords
    .flatMap(({ platform, record }) => record.artifacts.map((artifact) => ({
      name: artifact.name,
      platform,
      arch: record.arch,
      runner: record.runner,
      bytes: artifact.bytes,
      sha256: artifact.sha256,
      signing: record.signing,
    })))
    .sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));

  const commands = [
    ...platformRecords.flatMap(({ record }) => record.commands ?? []),
    { step: 'aggregate', command: `node scripts/${basename(process.argv[1])} ${argv.join(' ')}`.trim() },
  ];

  const manifest = {
    manifest: 1,
    version,
    commit,
    tag,
    artifacts,
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  const manifestName = `native-${version}.manifest.json`;

  const provenance = {
    provenance: 1,
    mode: dryRun ? 'dry-run' : 'tag',
    version,
    commit,
    tag,
    platforms: platformRecords.map(({ platform, record }) => ({
      platform,
      runner: record.runner,
      arch: record.arch,
      signing: record.signing,
    })),
    commands,
    digests: {
      artifacts: Object.fromEntries(artifacts.map((artifact) => [artifact.name, artifact.sha256])),
      manifest: { name: manifestName, bytes: manifestBytes.length, sha256: sha256(manifestBytes) },
    },
    doesNotEstablish: DOES_NOT_ESTABLISH,
  };
  const provenanceBytes = Buffer.from(`${JSON.stringify(provenance, null, 2)}\n`);
  const provenanceName = `native-${version}.provenance.json`;

  mkdirSync(out, { recursive: true });
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
      { name: manifestName, bytes: manifestBytes.length, sha256: sha256(manifestBytes) },
      { name: provenanceName, bytes: provenanceBytes.length, sha256: sha256(provenanceBytes) },
    ],
    artifacts: artifacts.map((artifact) => ({ name: artifact.name, platform: artifact.platform, bytes: artifact.bytes, sha256: artifact.sha256, signing: artifact.signing.state })),
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
