import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { PLATFORMS, artifactNames, sha256, sidecar } from '../scripts/native-products.mjs';
import { writeArchive } from '../scripts/release-archive.mjs';

/**
 * The tamper self-test's RED has to name itself (issue #72).
 *
 * A tamper dispatch corrupts one product on purpose, so the fresh-download verification is shown
 * to bite. The defect this file pins: the verifier's failure was indistinguishable from a real
 * release defect — a reader (the owner included) read a healthy pipeline as broken. Both verifiers
 * now take the dispatch's tamper mode as a flag and *label* the disagreements that corruption
 * explains, in their failure output, their job summary and their JSON.
 *
 * Both halves of that contract are checked here, for both verifiers: a labelled RED still exits 1
 * and names the self-test, the tamper mode and — for the native one — the platform; an
 * unlabelled mismatch stays plain, unqualified output; a label that does not name the corrupted
 * artifact explains nothing; a tamper that never bit fails; and a real failure riding beside the
 * tamper keeps its own line.
 *
 * The fixture trees are built by the repository's own writers — the artifact names, the sidecar
 * spelling, the listing digest, the archive container — so the verifiers meet the shapes the
 * release jobs produce, with synthetic package bytes rather than a real installer.
 */

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const VERSION = '9.9.9';
const COMMIT = 'e'.repeat(40);

/** One verifier invocation; the label assertion below reads the exit status, never a pipe. */
function run(script, args, env = {}) {
  const result = spawnSync(process.execPath, [join(root, 'scripts', script), ...args], {
    encoding: 'utf8',
    env: { ...process.env, ...env },
  });
  return { status: result.status, output: `${result.stdout ?? ''}${result.stderr ?? ''}` };
}

/** The failure JSON a verifier prints, so a check can read the record and not only the prose. */
function record(output) {
  const start = output.indexOf('{\n  "verified"');
  assert.ok(start !== -1, `no failure JSON in the output:\n${output}`);
  return JSON.parse(output.slice(start, output.lastIndexOf('}') + 1));
}

/** The unqualified-failure check: none of the self-identifying wording may appear. */
function assertUnqualified(output) {
  assert.ok(
    !/EXPECTED RED|tamper self-test|expectedTamper|not a release defect/i.test(output),
    `an unlabelled failure must not carry the self-test's wording:\n${output}`,
  );
}

/** The published native set a dry run uploads: four installers, sidecars, manifest, provenance. */
function writeNativeSet(dir) {
  mkdirSync(dir, { recursive: true });
  const artifacts = [];
  for (const { platform, name } of artifactNames(VERSION)) {
    const bytes = Buffer.from(`synthetic installer for ${name}\n${'-'.repeat(96)}\n`);
    writeFileSync(join(dir, name), bytes);
    writeFileSync(join(dir, `${name}.sha256`), sidecar(sha256(bytes), name));
    const signing = PLATFORMS[platform].signing.dryRun === 'skipped'
      ? { state: 'skipped', notice: 'synthetic: the dry run holds no signing certificate' }
      : { state: PLATFORMS[platform].signing.dryRun };
    artifacts.push({ name, platform, bytes: bytes.length, sha256: sha256(bytes), signing });
  }
  artifacts.sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));

  const manifestName = `native-${VERSION}.manifest.json`;
  const manifestBytes = Buffer.from(`${JSON.stringify({ manifest: 1, version: VERSION, commit: COMMIT, tag: null, artifacts }, null, 2)}\n`);
  const provenance = {
    provenance: 1,
    mode: 'dry-run',
    version: VERSION,
    commit: COMMIT,
    tag: null,
    platforms: artifacts.map((artifact) => ({ platform: artifact.platform, signing: artifact.signing })),
    commands: [{ step: 'fixture', command: 'node fixture' }],
    digests: {
      artifacts: Object.fromEntries(artifacts.map((artifact) => [artifact.name, artifact.sha256])),
      manifest: { name: manifestName, bytes: manifestBytes.length, sha256: sha256(manifestBytes) },
    },
    doesNotEstablish: ['fixture: the synthetic set establishes nothing'],
  };
  writeFileSync(join(dir, manifestName), manifestBytes);
  writeFileSync(join(dir, `native-${VERSION}.provenance.json`), Buffer.from(`${JSON.stringify(provenance, null, 2)}\n`));
}

/** The published bundle a dry run uploads: one piece, its pin, the archive and the two records. */
function writeBundleSet(dir) {
  mkdirSync(dir, { recursive: true });
  const pieceFile = { path: 'engine/fixture.wasm', bytes: Buffer.from('synthetic engine payload\n'.repeat(64)) };
  const pinPath = 'bundles/engine.manifest.json';
  const files = [{ path: pieceFile.path, bytes: pieceFile.bytes.length, sha256: sha256(pieceFile.bytes) }];
  // A piece's own digest: the canonical listing formula the release pipeline pins with.
  const pieceDigest = sha256(Buffer.from(`${files.map((file) => `${file.sha256}  ${file.bytes}  ${file.path}`).join('\n')}\n`, 'utf8'));
  const pinBytes = Buffer.from(`${JSON.stringify({ piece: 'engine', digest: pieceDigest, files }, null, 2)}\n`);
  const archiveName = `bundle-${VERSION}.tar.gz`;
  const archive = writeArchive([{ path: pieceFile.path, bytes: pieceFile.bytes }, { path: pinPath, bytes: pinBytes }]);
  const manifestName = `bundle-${VERSION}.manifest.json`;
  const manifest = {
    manifest: 1,
    version: VERSION,
    commit: COMMIT,
    tag: null,
    archive: { name: archiveName, bytes: archive.length, sha256: sha256(archive) },
    pieces: [{ piece: 'engine', digest: pieceDigest, files }],
    pins: [{ path: pinPath, piece: 'engine', bytes: pinBytes.length, sha256: sha256(pinBytes), digest: pieceDigest }],
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  const provenance = {
    provenance: 1,
    mode: 'dry-run',
    version: VERSION,
    commit: COMMIT,
    tag: null,
    toolchain: { channel: 'fixture' },
    node: process.version,
    commands: [{ step: 'assemble', command: 'node fixture' }],
    digests: {
      archive: { name: archiveName, bytes: archive.length, sha256: manifest.archive.sha256 },
      manifest: { name: manifestName, bytes: manifestBytes.length, sha256: sha256(manifestBytes) },
      pieces: { engine: pieceDigest },
      pins: { engine: sha256(pinBytes) },
    },
    doesNotEstablish: ['fixture: the synthetic set establishes nothing'],
  };
  writeFileSync(join(dir, archiveName), archive);
  writeFileSync(join(dir, manifestName), manifestBytes);
  writeFileSync(join(dir, `bundle-${VERSION}.provenance.json`), Buffer.from(`${JSON.stringify(provenance, null, 2)}\n`));
}

test('native-verify: a tamper dispatch fails RED in the self-test\'s own name, and only then', () => {
  const scratch = mkdtempSync(join(tmpdir(), 'drafthouse-verify-native-'));
  try {
    const published = join(scratch, 'published');
    writeNativeSet(published);
    const mode = ['--expect-mode', 'dry-run'];

    // The clean set verifies: the label changes nothing about a run with nothing to label.
    const clean = run('native-verify.mjs', ['--dir', published, ...mode]);
    assert.equal(clean.status, 0, clean.output);

    // The dispatch's own tamper — the same invocation the dry run's self-test makes.
    const tamper = run('release-tamper.mjs', ['--dir', published, '--which', 'native', '--platform', 'linux-x64']);
    assert.equal(tamper.status, 0, tamper.output);

    // Without the tamper input the mismatch is unqualified output: the shape a real run keeps.
    const plain = run('native-verify.mjs', ['--dir', published, ...mode]);
    assert.equal(plain.status, 1);
    assertUnqualified(plain.output);
    assert.equal(record(plain.output).expectedTamper, undefined);

    // With it the failure names the self-test, the mode and the platform — and still exits 1.
    const summary = join(scratch, 'summary.md');
    const labelled = run('native-verify.mjs', ['--dir', published, ...mode, '--expect-tamper', 'native', '--tamper-platform', 'linux-x64'], { GITHUB_STEP_SUMMARY: summary });
    assert.equal(labelled.status, 1);
    assert.match(labelled.output, /EXPECTED RED/);
    assert.match(labelled.output, /the tamper self-test \(tamper=native, platform linux-x64\)/);
    assert.match(labelled.output, /not a release defect/);
    const labelledRecord = record(labelled.output);
    assert.equal(labelledRecord.verified, false);
    assert.equal(labelledRecord.expectedTamper.explainsAllFailures, true);
    assert.equal(labelledRecord.expectedTamper.unexplainedFailures.length, 0);
    assert.ok(labelledRecord.expectedTamper.explainedFailures.length >= 2, labelled.output);
    assert.match(readFileSync(summary, 'utf8'), /EXPECTED RED/);

    // A label that does not name the corrupted artifact must not read as the expected RED.
    const wrong = run('native-verify.mjs', ['--dir', published, ...mode, '--expect-tamper', 'native', '--tamper-platform', 'macos-universal']);
    assert.equal(wrong.status, 1);
    assert.match(wrong.output, /NOT FULLY EXPLAINED/);
    assert.match(wrong.output, /NOT an expected RED/);
    const wrongRecord = record(wrong.output);
    assert.equal(wrongRecord.expectedTamper.explainsAllFailures, false);
    assert.equal(wrongRecord.expectedTamper.explainedFailures.length, 0);
    assert.equal(wrongRecord.expectedTamper.unexplainedFailures.length, wrongRecord.failures.length);

    // A tamper that never bit is a defect in the self-test, never a pass.
    const untouched = join(scratch, 'untouched');
    writeNativeSet(untouched);
    const noBite = run('native-verify.mjs', ['--dir', untouched, ...mode, '--expect-tamper', 'native', '--tamper-platform', 'linux-x64']);
    assert.equal(noBite.status, 1);
    assert.match(noBite.output, /DID NOT BITE/);
    assert.equal(record(noBite.output).expectedTamper.didNotBite, true);

    // The label only exists for this verifier's own mode, and it needs its platform.
    assert.equal(run('native-verify.mjs', ['--dir', published, ...mode, '--expect-tamper', 'archive']).status, 2);
    assert.equal(run('native-verify.mjs', ['--dir', published, ...mode, '--expect-tamper', 'native']).status, 2);
    assert.equal(run('native-verify.mjs', ['--dir', published, ...mode, '--tamper-platform', 'linux-x64']).status, 2);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test('release-verify: a bundle tamper dispatch fails RED in the self-test\'s own name, and a real failure stays real', () => {
  const scratch = mkdtempSync(join(tmpdir(), 'drafthouse-verify-bundle-'));
  try {
    const published = join(scratch, 'published');
    writeBundleSet(published);
    assert.equal(run('release-verify.mjs', ['--dir', published]).status, 0);

    // manifest mode: the self-test's first shape.
    assert.equal(run('release-tamper.mjs', ['--dir', published, '--which', 'manifest']).status, 0);
    const plain = run('release-verify.mjs', ['--dir', published]);
    assert.equal(plain.status, 1);
    assertUnqualified(plain.output);
    assert.equal(record(plain.output).expectedTamper, undefined);

    const summary = join(scratch, 'summary.md');
    const labelled = run('release-verify.mjs', ['--dir', published, '--expect-tamper', 'manifest'], { GITHUB_STEP_SUMMARY: summary });
    assert.equal(labelled.status, 1);
    assert.match(labelled.output, /EXPECTED RED/);
    assert.match(labelled.output, /the tamper self-test \(tamper=manifest\)/);
    const labelledRecord = record(labelled.output);
    assert.equal(labelledRecord.expectedTamper.explainsAllFailures, true);
    assert.equal(labelledRecord.expectedTamper.unexplainedFailures.length, 0);
    assert.match(readFileSync(summary, 'utf8'), /EXPECTED RED/);

    // The label may not absorb a failure it does not explain: plant a second, real disagreement.
    const mixed = join(scratch, 'mixed');
    writeBundleSet(mixed);
    assert.equal(run('release-tamper.mjs', ['--dir', mixed, '--which', 'manifest']).status, 0);
    const provenancePath = join(mixed, `bundle-${VERSION}.provenance.json`);
    const provenance = JSON.parse(readFileSync(provenancePath, 'utf8'));
    provenance.version = '0.0.0-not-this-run';
    writeFileSync(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`);
    const mixedRun = run('release-verify.mjs', ['--dir', mixed, '--expect-tamper', 'manifest']);
    assert.equal(mixedRun.status, 1);
    assert.match(mixedRun.output, /NOT FULLY EXPLAINED/);
    assert.match(mixedRun.output, /NOT an expected RED/);
    const mixedRecord = record(mixedRun.output);
    assert.equal(mixedRecord.expectedTamper.explainsAllFailures, false);
    assert.ok(
      mixedRecord.expectedTamper.unexplainedFailures.some((failure) => /provenance version/.test(failure)),
      mixedRun.output,
    );
    assert.ok(mixedRecord.expectedTamper.explainedFailures.length > 0, mixedRun.output);

    // archive mode: the other shape, on a fresh copy of the same set.
    const archive = join(scratch, 'archive');
    writeBundleSet(archive);
    assert.equal(run('release-tamper.mjs', ['--dir', archive, '--which', 'archive']).status, 0);
    assert.equal(run('release-verify.mjs', ['--dir', archive]).status, 1);
    const archiveLabel = run('release-verify.mjs', ['--dir', archive, '--expect-tamper', 'archive']);
    assert.equal(archiveLabel.status, 1);
    assert.match(archiveLabel.output, /EXPECTED RED/);
    assert.match(archiveLabel.output, /the tamper self-test \(tamper=archive\)/);
    assert.equal(record(archiveLabel.output).expectedTamper.explainsAllFailures, true);

    // A label whose own footprint is absent credits nothing: the archive's lines stay real.
    const misplaced = run('release-verify.mjs', ['--dir', archive, '--expect-tamper', 'manifest']);
    assert.equal(misplaced.status, 1);
    assert.match(misplaced.output, /NOT FULLY EXPLAINED/);
    assert.equal(record(misplaced.output).expectedTamper.explainedFailures.length, 0);

    // The label only exists for the bundle's own modes.
    assert.equal(run('release-verify.mjs', ['--dir', published, '--expect-tamper', 'native']).status, 2);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
