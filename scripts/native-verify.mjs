#!/usr/bin/env node
/**
 * The native installers' fresh-download verification — issue #72.
 *
 * The counterpart of `scripts/release-verify.mjs`, for the four native artifacts. It runs in a
 * *different job* from the one that built them, against the **published** set — the Release assets
 * on a tag push, the run's workflow artifacts on a dry run — and re-derives every hash from the
 * bytes that arrived. Nothing it checks is taken on trust from the job that produced the file.
 *
 *     node scripts/native-verify.mjs --dir dist/downloaded --expect-mode dry-run
 *
 * What it re-derives and checks:
 *
 *   artifacts  the four names (issue #72's spelling, for the version the manifest names) are
 *              present, and nothing else that looks like a native artifact is; each file's byte
 *              size and SHA-256 are recomputed from its bytes;
 *   sidecars   each artifact's `.sha256` sidecar is present and carries that same digest;
 *   records    the provenance and the manifest agree with each other and with those bytes: version,
 *              commit, the mode's tag rule, every artifact digest, and the manifest's own digest;
 *   signing    the per-platform signing state matches the mode: on a tag push the macOS artifact
 *              must be `signed-and-notarized` (a tag build that skipped signing fails here, loudly),
 *              and on a dry run it must be `skipped` **with its notice**, which is what makes the
 *              skip visible in the published record rather than only in a log line; Windows and
 *              Linux are `unsigned` in both modes (the v1 decision).
 *
 * Exit status: 0 every hash verified and every state consistent; 1 a disagreement, each one listed;
 * 2 the check could not run (no directory, no manifest or provenance, ambiguous names, unreadable
 * JSON, an unknown expected mode).
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { PLATFORMS, artifactNames, sha256 } from './native-products.mjs';

function refuse(message) {
  console.error(`native-verify: ${message}`);
  process.exit(2);
}

/** The four release names' shape, for spotting an artifact that is not one of them. */
const NATIVE_NAME = /^(Synergy-Drafthouse-|synergy-drafthouse_)/;

function main() {
  const argv = process.argv.slice(2);
  const value = (flag) => {
    const at = argv.indexOf(flag);
    if (at === -1) return null;
    if (!argv[at + 1] || argv[at + 1].startsWith('--')) refuse(`${flag} needs a value`);
    return argv[at + 1];
  };
  const dir = value('--dir');
  const expectMode = value('--expect-mode');
  if (!dir) refuse('--dir needs the directory of downloaded native assets');
  if (!existsSync(dir) || !statSync(dir).isDirectory()) refuse(`${dir} is not a directory`);
  if (!['tag', 'dry-run'].includes(expectMode)) refuse(`--expect-mode must be tag or dry-run (got ${JSON.stringify(expectMode)})`);

  const names = readdirSync(dir).filter((name) => statSync(join(dir, name)).isFile()).sort();
  const sole = (suffix) => {
    const matches = names.filter((name) => name.endsWith(suffix));
    if (matches.length !== 1) refuse(`${dir} must hold exactly one *${suffix}; found ${matches.length}${matches.length ? `: ${matches.join(', ')}` : ''}`);
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
  const version = manifest.version;
  if (typeof version !== 'string' || version === '') failures.push('the manifest names no version');

  // The records against each other and against the mode this run is supposed to be.
  if (provenance.version !== version) failures.push(`provenance version: the record says ${JSON.stringify(provenance.version)}; the manifest says ${JSON.stringify(version)}`);
  if (provenance.commit !== manifest.commit) failures.push(`provenance commit: the record says ${JSON.stringify(provenance.commit)}; the manifest says ${JSON.stringify(manifest.commit)}`);
  if (provenance.mode !== expectMode) {
    failures.push(`provenance mode: the record says ${JSON.stringify(provenance.mode)}; this run is a ${expectMode} run — a tag build recorded as a dry run (or the reverse) is not a record of this run`);
  }
  if (expectMode === 'tag' && provenance.tag !== `v${version}`) {
    failures.push(`a tag run must name the release tag v${version}; the record says ${JSON.stringify(provenance.tag)}`);
  }
  if (expectMode === 'dry-run' && provenance.tag !== null && provenance.tag !== undefined) {
    failures.push(`a dry run must not name a tag; the record says ${JSON.stringify(provenance.tag)}`);
  }
  const manifestDigest = provenance.digests?.manifest;
  if (manifestDigest?.sha256 !== sha256(manifestBytes)) {
    failures.push(`provenance manifest digest: the record says ${JSON.stringify(manifestDigest?.sha256)}; the published manifest is ${sha256(manifestBytes)}`);
  }
  if (manifestDigest?.bytes !== manifestBytes.length) {
    failures.push(`provenance manifest size: the record says ${JSON.stringify(manifestDigest?.bytes)}; the published manifest is ${manifestBytes.length} bytes`);
  }
  if (!Array.isArray(provenance.doesNotEstablish) || provenance.doesNotEstablish.length === 0) failures.push('the provenance record does not state what it does not establish');
  if (!Array.isArray(provenance.commands) || provenance.commands.length === 0) failures.push('the provenance records no commands');

  // Every artifact, against the bytes that arrived.
  const expected = artifactNames(version ?? '');
  const listed = new Map((Array.isArray(manifest.artifacts) ? manifest.artifacts : []).map((artifact) => [artifact.name, artifact]));
  for (const artifact of listed.keys()) {
    if (!expected.some((entry) => entry.name === artifact)) failures.push(`the manifest lists ${artifact}, which is not one of the four installers' names for ${version}`);
  }
  for (const { platform, name } of expected) {
    const record = listed.get(name);
    if (!record) {
      failures.push(`${name} (${platform}) is listed by neither the manifest nor the published set — every release carries all four installers`);
      continue;
    }
    if (record.platform !== platform) failures.push(`${name}: the manifest records the platform ${JSON.stringify(record.platform)}`);
    if (!existsSync(join(dir, name))) {
      failures.push(`${name} is listed in the manifest but not in the published set`);
      continue;
    }
    const bytes = readFileSync(join(dir, name));
    const digest = sha256(bytes);
    if (bytes.length !== record.bytes) failures.push(`${name}: the manifest records ${record.bytes} bytes, the published file is ${bytes.length}`);
    if (digest !== record.sha256) failures.push(`${name}: the manifest records sha256 ${record.sha256}, the published file is ${digest}`);
    if (provenance.digests?.artifacts?.[name] !== record.sha256) {
      failures.push(`${name}: the provenance records ${JSON.stringify(provenance.digests?.artifacts?.[name])}; the manifest records ${record.sha256}`);
    }
    const sidecarPath = join(dir, `${name}.sha256`);
    if (!existsSync(sidecarPath)) {
      failures.push(`${name}.sha256 is missing beside the artifact`);
    } else {
      const [sidecarDigest, sidecarName] = readFileSync(sidecarPath, 'utf8').trim().split(/\s+/);
      if (sidecarDigest !== digest) failures.push(`${name}.sha256 carries ${sidecarDigest}; the published bytes are ${digest}`);
      if (sidecarName !== name) failures.push(`${name}.sha256 names ${JSON.stringify(sidecarName)}; the artifact it sits beside is ${name}`);
    }
  }

  // Nothing else that looks like one of the four.
  for (const name of names) {
    if (!NATIVE_NAME.test(name)) continue;
    const artifact = name.endsWith('.sha256') ? name.slice(0, -'.sha256'.length) : name;
    if (!expected.some((entry) => entry.name === artifact)) failures.push(`${name} is in the published set but is not one of the four installers for ${version}`);
  }

  // Signing: the state has to match the mode, per platform.
  const platforms = new Map((Array.isArray(provenance.platforms) ? provenance.platforms : []).map((entry) => [entry.platform, entry]));
  for (const platform of Object.keys(PLATFORMS)) {
    const entry = platforms.get(platform);
    if (!entry) {
      failures.push(`the provenance records no state for ${platform}`);
      continue;
    }
    const wanted = PLATFORMS[platform].signing[expectMode === 'tag' ? 'tag' : 'dryRun'];
    if (entry.signing?.state !== wanted) {
      failures.push(
        `${platform}: the run records signing ${JSON.stringify(entry.signing?.state)}, and a ${expectMode} run of ${platform} is ${wanted}`
        + (platform === 'macos-universal' && expectMode === 'tag'
          ? ' — a tag build that did not sign and notarize the dmg must not pass verification'
          : ''),
      );
    }
    if (entry.signing?.state === 'skipped' && !entry.signing?.notice) {
      failures.push(`${platform}: signing was skipped without a notice — a skip has to say why, in the record a reader gets`);
    }
  }

  if (failures.length) {
    console.error(JSON.stringify({ verified: false, dir, mode: expectMode, failures }, null, 2));
    process.exit(1);
  }

  console.log(JSON.stringify({
    verified: true,
    dir,
    mode: expectMode,
    version,
    commit: manifest.commit,
    tag: manifest.tag ?? null,
    artifacts: expected.map(({ platform, name }) => {
      const record = listed.get(name);
      return { name, platform, bytes: record.bytes, sha256: record.sha256, signing: record.signing?.state };
    }),
    provenance: { mode: provenance.mode, doesNotEstablish: provenance.doesNotEstablish.length, commands: provenance.commands.length },
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
