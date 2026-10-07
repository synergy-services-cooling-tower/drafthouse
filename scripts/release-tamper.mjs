#!/usr/bin/env node
/**
 * Deliberate-mismatch helper — issue #36, slice 1 of #35. **Self-test only.**
 *
 * A verification job that has never failed is not demonstrated to bite. This helper makes one
 * published product disagree with the others, deterministically and by exactly one byte or one
 * hex digit, so the release pipeline's fresh-download re-verification can be shown red with a
 * raw exit status and then shown green again after the products are restored.
 *
 * It is invoked by exactly one place: the release workflow's dry-run path, and only when the
 * `tamper` dispatch input names what to corrupt. It mutates the products **in the directory it
 * is given** — never a Release, never a committed file — and it prints what it changed.
 *
 *     node scripts/release-tamper.mjs --dir <products-dir> --which archive
 *     node scripts/release-tamper.mjs --dir <products-dir> --which manifest
 *     node scripts/release-tamper.mjs --dir <native-dir> --which native --platform linux-x64
 *
 *   archive   flips the low bit of the first byte of the first non-empty file payload inside
 *             the archive, and rewrites the container (so the archive digest, the file digest
 *             and the piece digest all disagree with the manifest);
 *   manifest  changes the last hex digit of the first listed file digest, so the bytes and
 *             the record disagree with each other;
 *   native    flips one byte in the middle of a platform's primary native artifact (issue #72:
 *             the macOS dmg, the Windows msi, the Linux AppImage) after its `.sha256` sidecar and
 *             its platform record were written, so the published-set verification
 *             (`scripts/native-verify.mjs`) has a digest that disagrees with the bytes. The
 *             platform is named by `--platform`; the run that uses it is the workflow's dry-run
 *             self-test, and nothing else.
 *
 * Exit status: 0 tampered; 2 refused or could not run (no directory, no product, unknown
 * target, or a tamper that did not actually change the bytes).
 */
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { PLATFORMS } from './native-products.mjs';
import { readArchive, sha256, writeArchive } from './release-archive.mjs';

function refuse(message) {
  console.error(`release-tamper: ${message}`);
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
  const dir = value('--dir');
  const which = value('--which');
  const platform = value('--platform');
  if (!dir) refuse('--dir needs the products directory');
  if (!['archive', 'manifest', 'native'].includes(which)) refuse('--which must be archive, manifest or native');
  if (!existsSync(dir) || !statSync(dir).isDirectory()) refuse(`${dir} is not a directory`);

  const names = readdirSync(dir).filter((name) => statSync(join(dir, name)).isFile());

  if (which === 'native') {
    if (!platform) refuse('--which native needs --platform: the primary artifact is per platform, and a run that does not say which one is a run that corrupts an unspecified product');
    const spec = PLATFORMS[platform];
    if (!spec) refuse(`--platform ${JSON.stringify(platform)} is not one of ${Object.keys(PLATFORMS).join(', ')}`);
    // The platform's primary artifact, located the way the platform job located it: by extension.
    // No record is needed (and in the run this corrupts, the record still carries the digest the
    // sidecar does - the point is that the *bytes* no longer match either).
    const extension = spec.products[0].extension;
    const candidates = names.filter((name) => name.endsWith(extension) && !name.endsWith(`${extension}.sha256`));
    if (candidates.length !== 1) {
      refuse(`${dir} must hold exactly one *${extension} (${platform}'s primary artifact); found ${candidates.length}${candidates.length ? `: ${candidates.join(', ')}` : ''}`);
    }
    const path = join(dir, candidates[0]);
    const bytes = readFileSync(path);
    if (bytes.length < 3) refuse(`${candidates[0]} is too small to tamper with`);
    const tampered = Buffer.from(bytes);
    const at = Math.floor(bytes.length / 2);
    const before = bytes[at];
    tampered[at] = before ^ 0x01;
    if (tampered.equals(bytes)) refuse(`tampering with ${candidates[0]} did not change its bytes`);
    writeFileSync(path, tampered);
    console.log(JSON.stringify({
      tampered: true,
      which,
      platform,
      path,
      change: `${candidates[0]}: byte ${at} of ${bytes.length} 0x${before.toString(16).padStart(2, '0')} -> 0x${tampered[at].toString(16).padStart(2, '0')} (the .sha256 sidecar beside it, and the record, still carry the untampered digest)`,
      before: { bytes: bytes.length, sha256: sha256(bytes) },
      after: { bytes: tampered.length, sha256: sha256(tampered) },
    }, null, 2));
    return;
  }

  const sole = (suffix) => {
    const matches = names.filter((name) => name.endsWith(suffix));
    if (matches.length !== 1) refuse(`${dir} must hold exactly one *${suffix}; found ${matches.length}`);
    return matches[0];
  };
  const manifestName = sole('.manifest.json');
  const manifestBytes = readFileSync(join(dir, manifestName));
  const manifest = JSON.parse(manifestBytes.toString('utf8'));

  console.error(`release-tamper: SELF-TEST ONLY — mutating the products in ${dir} (never a Release, never a committed file)`);

  if (which === 'archive') {
    const archiveName = manifest.archive?.name;
    const path = join(dir, archiveName ?? '');
    if (typeof archiveName !== 'string' || !existsSync(path)) refuse(`${dir} holds no archive named by ${manifestName}`);
    const bytes = readFileSync(path);
    const entries = readArchive(bytes);
    // The largest payload, so the tampered file stays a file a reader could still use — the
    // disagreement is in the digests, not in a syntax error.
    const target = [...entries].sort((a, b) => b.bytes.length - a.bytes.length)[0];
    if (!target || target.bytes.length === 0) refuse(`archive ${archiveName} holds no payload to tamper with`);
    const before = target.bytes[0];
    target.bytes[0] = before ^ 0x01;
    const tampered = writeArchive(entries);
    if (tampered.equals(bytes)) refuse(`tampering with archive ${archiveName} did not change its bytes`);
    writeFileSync(path, tampered);
    console.log(JSON.stringify({
      tampered: true,
      which,
      path: join(dir, archiveName),
      change: `${target.path}: first payload byte 0x${before.toString(16).padStart(2, '0')} -> 0x${target.bytes[0].toString(16).padStart(2, '0')}`,
      before: { bytes: bytes.length, sha256: sha256(bytes) },
      after: { bytes: tampered.length, sha256: sha256(tampered) },
    }, null, 2));
    return;
  }

  const file = (manifest.pieces ?? []).flatMap((piece) => piece.files ?? [])[0];
  if (!file || typeof file.sha256 !== 'string' || file.sha256 === '') {
    refuse(`${manifestName} lists no file digest to tamper with`);
  }
  const flipped = file.sha256.slice(0, -1) + (file.sha256.endsWith('0') ? '1' : '0');
  file.sha256 = flipped;
  writeFileSync(join(dir, manifestName), `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(JSON.stringify({
    tampered: true,
    which,
    path: join(dir, manifestName),
    change: `${file.path}: recorded sha256 ...${file.sha256.slice(-1)} (was ...${flipped.slice(-1)}); the bytes are untouched`,
    before: { sha256: sha256(manifestBytes) },
    after: { sha256: sha256(readFileSync(join(dir, manifestName))) },
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
