#!/usr/bin/env node
/**
 * The native installers' shared contract — issue #72.
 *
 * One module owns the four artifacts' identity so the three scripts around it cannot drift from
 * one another: what each platform is called, which file the packager produces for it, what its
 * release name is, how a digest is written, and what the records say the release does *not*
 * establish. `scripts/native-artifacts.mjs` (per platform, on the runner), the aggregating
 * `scripts/native-manifest.mjs` and the fresh-download `scripts/native-verify.mjs` all read this
 * file, so a name is typed once.
 *
 * The names are the issue's, exactly:
 *
 *     Synergy-Drafthouse-<version>-macos-universal.dmg     arm64 + x86_64, one lipo'd binary
 *     Synergy-Drafthouse-<version>-windows-x64.msi
 *     Synergy-Drafthouse-<version>-linux-x64.AppImage
 *     synergy-drafthouse_<version>_amd64.deb
 *
 * `cargo-packager` names its own output from the crate (`Synergy Drafthouse_0.2.0_universal.dmg`,
 * `drafthouse_0.2.0_amd64.deb`, `drafthouse_0.2.0_x64_en-US.msi`), so the platform job locates the
 * packager's product by **extension** — never by that spelling — and renames it here. The version
 * is read from the crate (`cockpit/Cargo.toml`'s `[package] version`, the field the packager fills
 * its own `version` from) and cross-checked against `package.json`, which
 * `scripts/version-check.mjs` holds it equal to; nothing in this file types a version.
 */
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The three platform jobs, in the order the workflow's matrix lists them. */
export const PLATFORMS = {
  'macos-universal': {
    runner: 'macos-14',
    arch: 'universal',
    build: 'two architecture builds, lipo\'d into one binary',
    signing: { tag: 'signed-and-notarized', dryRun: 'skipped' },
    products: [{ extension: '.dmg', name: (version) => `Synergy-Drafthouse-${version}-macos-universal.dmg` }],
  },
  'windows-x64': {
    runner: 'windows-latest',
    arch: 'x64',
    build: 'the Windows runner\'s host target',
    signing: { tag: 'unsigned', dryRun: 'unsigned' },
    products: [{ extension: '.msi', name: (version) => `Synergy-Drafthouse-${version}-windows-x64.msi` }],
  },
  'linux-x64': {
    runner: 'ubuntu-22.04',
    arch: 'amd64',
    build: 'the Linux runner\'s host target',
    signing: { tag: 'unsigned', dryRun: 'unsigned' },
    products: [
      { extension: '.AppImage', name: (version) => `Synergy-Drafthouse-${version}-linux-x64.AppImage` },
      { extension: '.deb', name: (version) => `synergy-drafthouse_${version}_amd64.deb` },
    ],
  },
};

/** Every release name the four products can take, for a given version. */
export function artifactNames(version) {
  return Object.entries(PLATFORMS).flatMap(([platform, spec]) => (
    spec.products.map((product) => ({ platform, name: product.name(version) }))
  ));
}

/** What the records say the release does not establish. */
export const DOES_NOT_ESTABLISH = [
  'No publisher signature on the Windows or Linux artifacts: they ship unsigned for v1 (Windows shows the SmartScreen warning, and the AppImage is run without one). The macOS dmg is Developer-ID signed and notarized; that is recorded as a state string here, not as proof — the proof is `spctl` and `xcrun stapler validate` on the runner, named in the private release procedure.',
  'No build attestation and no reproducibility claim: the digests bind the listed bytes to each other. Nothing here says two builds of the same commit produce the same installer.',
  'No CTI or certification claim: nothing in this release is a CTI certification, a CTI ToolKit result, or any other third-party validation of the numbers. The engineering claims and their status are VALIDATION_RESULTS.md and docs/VALIDATION.md.',
  'No host deployment and no runtime support statement beyond the recorded floors: the Linux AppImage\'s glibc floor and the .deb\'s declared dependencies are in README.md and the .deb\'s own control data. Nothing here was executed on a user\'s machine.',
];

export function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/** Raw stdout of a git command against the repository this module lives in. */
export function git(args) {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error || result.status !== 0) return null;
  return result.stdout.trim();
}

/**
 * The version the artifacts carry: the crate's `[package] version` — the field `cargo-packager`
 * fills its own `version` from — cross-checked against `package.json`. A disagreement is a
 * refusal, never a guess: this is the number that ends up in a user's file name.
 */
export function artifactVersion() {
  const manifest = readFileSync(join(root, 'cockpit', 'Cargo.toml'), 'utf8');
  const crate = /^\s*version\s*=\s*"([^"]+)"/m.exec(section(manifest, '[package]'))?.[1];
  const packaged = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
  if (!crate) return { refused: 'cockpit/Cargo.toml declares no [package] version' };
  if (crate !== packaged) {
    return { refused: `cockpit/Cargo.toml's [package] version is ${crate}; package.json declares ${packaged} — scripts/version-check.mjs holds these equal, and the artifacts' names carry the crate's` };
  }
  return { version: crate };
}

/** The text of one `[section]` of a TOML file, up to the next table header. */
export function section(text, header) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === header);
  if (start === -1) return '';
  let end = lines.length;
  for (let at = start + 1; at < lines.length; at += 1) {
    if (lines[at].trimStart().startsWith('[')) {
      end = at;
      break;
    }
  }
  return lines.slice(start + 1, end).join('\n');
}

/** The record file name for a platform. */
export function recordName(platform) {
  return `native-${platform}.record.json`;
}

/** `<sha256>  <name>\n` — the sidecar spelling `scripts/cockpit-artifact.mjs` already writes. */
export function sidecar(digest, name) {
  return `${digest}  ${name}\n`;
}
