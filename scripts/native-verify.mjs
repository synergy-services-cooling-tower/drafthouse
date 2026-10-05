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
 *     node scripts/native-verify.mjs --dir dist/downloaded --expect-mode dry-run \
 *       --expect-tamper native --tamper-platform linux-x64
 *
 * `--expect-tamper native --tamper-platform <platform>` is for the run that dispatched a tamper
 * input (issue #72): the run says it corrupted that platform's primary artifact, so the verifier
 * *labels* the disagreements that corruption explains, in its failure output, in the job summary
 * and as `expectedTamper` in the JSON. It is labelling only — the exit status stays 1, every check
 * stays exactly as strict, and the label can only ever cover the corrupted artifact's own digest
 * lines. Any disagreement those lines do not explain stays a real failure; a run that finds no
 * disagreement at all fails as a self-test that did not bite. Without the flag the output is
 * unchanged: a mismatch is an unqualified failure.
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
import { appendFileSync, existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { PLATFORMS, artifactNames, sha256 } from './native-products.mjs';

function refuse(message) {
  console.error(`native-verify: ${message}`);
  process.exit(2);
}

/** The four release names' shape, for spotting an artifact that is not one of them. */
const NATIVE_NAME = /^(Synergy-Drafthouse-|synergy-drafthouse_)/;

/**
 * The dispatch's tamper scope (issue #72). A dispatch with `tamper=native` corrupts ONE platform's
 * primary artifact — after its `.sha256` sidecar and its record were written — so exactly one
 * class of disagreement is expected: that artifact's digest against the bytes that arrived, both
 * in the manifest check and in the sidecar check. Nothing else is: a size disagreement, a missing
 * file or any other artifact's digest stays real. The scope is derived from the manifest's own
 * version and the same `artifactNames` the checks use, so it can only name a file this verifier
 * actually checked — and if the name cannot be derived (no version), nothing is covered.
 */
function tamperScope(platform, version) {
  const name = artifactNames(version ?? '').find((entry) => entry.platform === platform)?.name ?? null;
  return {
    platform,
    name,
    explains: (failure) => name !== null && (
      failure.startsWith(`${name}: the manifest records sha256 `)
      || failure.startsWith(`${name}.sha256 carries `)
    ),
    // The footprint the corruption always leaves: the artifact's bytes against the manifest's
    // record. A run where only the sidecar line moved never had this artifact's bytes corrupted
    // in the way the dispatch says, so nothing is credited there either.
    signal: (failure) => name !== null && failure.startsWith(`${name}: the manifest records sha256 `),
    explained: [],
    unexplained: [],
    didNotBite: false,
  };
}

/**
 * Names the self-test, in the failure output and in the job summary (issue #72). This is the
 * surface a reader of a failed run sees, and the one this defect cost: the output alone read as
 * a release defect. It states what the label covers and, when anything is not covered, says so
 * instead of claiming the expected RED. The exit status is not touched — a designed RED is a RED.
 */
function reportTamperScope(scope) {
  const say = (line) => console.error(`native-verify: ${line}`);
  const what = scope.name ?? 'a platform artifact';
  const summary = [];
  if (scope.didNotBite) {
    say(`TAMPER DISPATCH (tamper=native, platform ${scope.platform}) — THE SELF-TEST DID NOT BITE.`);
    say(`this run dispatched the tamper self-test, which corrupts ${what} after its .sha256 sidecar and its record were written; every hash verified anyway.`);
    say('a self-test that cannot fail is not a self-test — the verifier, the tamper step or the download path is not doing its job.');
    summary.push(
      `### TAMPER SELF-TEST DID NOT BITE — tamper=native, platform ${scope.platform}`,
      '',
      `This dispatch carried the tamper self-test for \`${what}\`, so \`native-verify\` was required to find that artifact's digests disagreeing with the bytes that arrived. It found no disagreement at all.`,
      '',
      '**A defect in the self-test — not a release defect.**',
    );
  } else if (scope.unexplained.length === 0) {
    say(`EXPECTED RED — the tamper self-test (tamper=native, platform ${scope.platform}).`);
    say(`this dispatch deliberately corrupted ${what} after its .sha256 sidecar and its record were written, so the published bytes cannot agree with the digests recorded beside them. Every failure below is that corruption's own disagreement: the self-test biting as designed, and this job failing is its designed outcome.`);
    say('it is NOT a release defect, and it says nothing about the published artifact. The JSON below carries it as `expectedTamper`.');
    summary.push(
      `### EXPECTED RED — tamper self-test (tamper=native, platform ${scope.platform})`,
      '',
      `This dispatch deliberately corrupted \`${what}\` after its \`.sha256\` sidecar and its record were written. Every failure of this run is that corruption's own disagreement: \`native-verify\` failing is the self-test biting as designed.`,
      '',
      '**Not a release defect** — nothing here says the published artifact is wrong.',
    );
  } else {
    say(`TAMPER DISPATCH (tamper=native, platform ${scope.platform}) — THIS FAILURE IS NOT FULLY EXPLAINED BY THE SELF-TEST.`);
    say(`${scope.explained.length} disagreement(s) below are the self-test's expected RED for the corrupted artifact; ${scope.unexplained.length} further disagreement(s) are NOT explained by it and are real verification failures:`);
    for (const failure of scope.unexplained) say(`  unexpected: ${failure}`);
    summary.push(
      `### NOT an expected RED — tamper=native, platform ${scope.platform}`,
      '',
      `This dispatch carried the tamper self-test, but ${scope.unexplained.length} of its ${scope.explained.length + scope.unexplained.length} failures are NOT explained by the corrupted artifact and are **real verification failures**:`,
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
  const expectTamper = value('--expect-tamper');
  const tamperPlatform = value('--tamper-platform');
  if (!dir) refuse('--dir needs the directory of downloaded native assets');
  if (!existsSync(dir) || !statSync(dir).isDirectory()) refuse(`${dir} is not a directory`);
  if (!['tag', 'dry-run'].includes(expectMode)) refuse(`--expect-mode must be tag or dry-run (got ${JSON.stringify(expectMode)})`);
  if (expectTamper !== null && expectTamper !== 'native') {
    refuse(`--expect-tamper must be native — the mode that corrupts this verifier's products; the bundle's archive/manifest modes belong to scripts/release-verify.mjs (got ${JSON.stringify(expectTamper)})`);
  }
  if (expectTamper === 'native' && !tamperPlatform) {
    refuse('--expect-tamper native needs --tamper-platform: the dispatch that corrupted a product named which platform\'s artifact it corrupted, and the run must say which');
  }
  if (expectTamper === null && tamperPlatform !== null) refuse('--tamper-platform only means something with --expect-tamper native');
  if (tamperPlatform !== null && !PLATFORMS[tamperPlatform]) {
    refuse(`--tamper-platform ${JSON.stringify(tamperPlatform)} is not one of ${Object.keys(PLATFORMS).join(', ')}`);
  }

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

  // The dispatch's tamper, labelled (issue #72): a run that carries the self-test is told which
  // artifact it corrupted, and only the disagreements that corruption explains are marked. The
  // exit status stays 1 either way — a designed RED is a RED — and a run whose tamper explains
  // nothing, or that finds nothing at all, is not allowed to pass.
  const tamper = expectTamper === 'native' ? tamperScope(tamperPlatform, version) : null;
  if (tamper) {
    if (failures.length === 0) {
      tamper.didNotBite = true;
      failures.push(`this run dispatched tamper=native (platform ${tamper.platform}): the dispatch corrupts ${tamper.name ?? 'that platform\'s primary artifact'} after its .sha256 sidecar and its record are written, yet every hash verified`);
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
      mode: expectMode,
      failures,
      ...(tamper ? {
        expectedTamper: {
          mode: 'native',
          platform: tamper.platform,
          explainsAllFailures: tamper.explained.length > 0 && tamper.unexplained.length === 0,
          didNotBite: tamper.didNotBite,
          explainedFailures: tamper.explained,
          unexplainedFailures: tamper.unexplained,
        },
      } : {}),
    }, null, 2));
    if (tamper && !tamper.didNotBite && tamper.unexplained.length === 0) {
      console.error(`native-verify: EXPECTED RED — the tamper self-test (tamper=native, platform ${tamper.platform}), not a release defect.`);
    }
    if (tamper && tamper.unexplained.length > 0) {
      console.error(`native-verify: NOT an expected RED — ${tamper.unexplained.length} failure(s) are outside the tamper self-test (tamper=native, platform ${tamper.platform}).`);
    }
    if (tamper && tamper.didNotBite) {
      console.error(`native-verify: TAMPER SELF-TEST DID NOT BITE (tamper=native, platform ${tamper.platform}) — a defect in the self-test, not a release defect.`);
    }
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
