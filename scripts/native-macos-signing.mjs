#!/usr/bin/env node
/**
 * The macOS signing run's preparation — issue #72.
 *
 * `cargo-packager` signs and notarizes a macOS bundle when (and only when) the manifest names a
 * `macos.signing-identity`: with it set, the packager imports `APPLE_CERTIFICATE` /
 * `APPLE_CERTIFICATE_PASSWORD` into a temporary keychain, signs every binary inside-out with
 * `codesign --force -s <identity> --options runtime --timestamp` (no entitlements file is
 * configured, which is the v1 decision: hardened runtime defaults, no sandbox), then notarizes the
 * app with `xcrun notarytool submit … --wait` and `xcrun stapler staple` (its own source:
 * `crates/packager/src/package/app/mod.rs`, `codesign/macos.rs`).
 *
 * The identity **cannot be in the repository**: it is the owner's certificate's common name
 * (`Developer ID Application: … (TEAMID)`), and this repository is public. So the tag path runs
 * this script on the runner, before the packager:
 *
 *   * it refuses, loudly and by name, when a signing secret is absent — **the fail-loud path the
 *     issue asks for**: a tag build without `MACOS_DEVELOPER_ID_P12` / `…_PASSWORD` (and, for
 *     notarization, `ASC_API_KEY_ID` / `ASC_API_ISSUER_ID` / `ASC_API_KEY_P8`) must not produce a
 *     Release that looks signed. Nothing here is on the dry-run path, where the workflow instead
 *     prints a notice and packages an unsigned artifact;
 *   * it reads the certificate out of `MACOS_DEVELOPER_ID_P12` (base64 p12, the spelling the
 *     packager's own keychain import expects) and derives that common name with `openssl` — the
 *     exact string `codesign -s` matches, so the packager never has to guess an identity;
 *   * it writes `signing-identity` into the **run's copy** of the manifest and says so. The value
 *     is never echoed into a file the repository tracks, and the copy is a checkout on a runner.
 *
 * Usage (the workflow sets the two certificate variables; the three App Store Connect variables are
 * the packager's own notarization inputs and are checked here too):
 *
 *     node scripts/native-macos-signing.mjs --manifest cockpit/Cargo.toml
 *
 * Exit status: 0 the identity is discovered and written; 1 a required secret is missing (named,
 * never printed) or the certificate carries no common name; 2 the script could not run (no
 * manifest, no openssl, a manifest that already names an identity, a p12 that does not decode).
 */
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

/** The names, in the order the issue lists them. The values never leave this process. */
const SECRETS = ['MACOS_DEVELOPER_ID_P12', 'MACOS_DEVELOPER_ID_PASSWORD', 'ASC_API_KEY_ID', 'ASC_API_ISSUER_ID', 'ASC_API_KEY_P8'];
const CERTIFICATE_SECRETS = SECRETS.slice(0, 2);
const TABLE = '[package.metadata.packager.macos]';

function refuse(message) {
  console.error(`native-macos-signing: ${message}`);
  process.exit(1);
}

function couldNotRun(message) {
  console.error(`native-macos-signing: ${message}`);
  process.exit(2);
}

function openssl(args, input) {
  const result = spawnSync('openssl', args, { encoding: 'utf8', input });
  if (result.error) couldNotRun(`openssl could not run: ${result.error.message}`);
  if (result.status !== 0) return null;
  return result.stdout;
}

/** The common name of the first certificate in the p12: what `codesign -s` matches. */
function identityFrom(p12Path, password) {
  const pem = openssl(['pkcs12', '-in', p12Path, '-passin', `pass:${password}`, '-nokeys', '-clcerts'], null);
  if (!pem) {
    const legacy = openssl(['pkcs12', '-legacy', '-in', p12Path, '-passin', `pass:${password}`, '-nokeys', '-clcerts'], null);
    if (!legacy) return { failed: 'openssl could not read MACOS_DEVELOPER_ID_P12 as a PKCS#12 file with MACOS_DEVELOPER_ID_PASSWORD (re-export it from Keychain Access as a .p12 with a password, then base64 it)' };
    return nameFrom(legacy);
  }
  return nameFrom(pem);
}

function nameFrom(pem) {
  const multiline = openssl(['x509', '-noout', '-subject', '-nameopt', 'multiline'], pem);
  const match = multiline && /^\s*commonName\s*=\s*(.+?)\s*$/m.exec(multiline);
  if (match) return { identity: match[1] };
  const plain = openssl(['x509', '-noout', '-subject'], pem);
  const fallback = plain && /CN\s*=\s*([^/\n]+)/.exec(plain);
  if (fallback) return { identity: fallback[1].trim() };
  return { failed: 'the certificate carries no common name this script could read — Developer ID certificates name it `Developer ID Application: <name> (<TEAM>)`' };
}

function main() {
  const argv = process.argv.slice(2);
  const at = argv.indexOf('--manifest');
  const manifestPath = at === -1 ? null : argv[at + 1];
  if (!manifestPath) couldNotRun('--manifest needs the packager manifest to prepare (cockpit/Cargo.toml)');
  if (!existsSync(manifestPath)) couldNotRun(`${manifestPath} does not exist`);

  const missing = SECRETS.filter((name) => !process.env[name]);
  if (missing.length) {
    refuse(
      `the tag build requires the macOS signing secrets and these are absent: ${missing.join(', ')}.`
      + ' The workflow only runs this on a tag push; add the secrets to the repository (the names are'
      + ' the contract — the private release procedure lists them) or dispatch a dry run, which does not sign.',
    );
  }

  const manifest = readFileSync(manifestPath, 'utf8');
  if (/^\s*signing-identity\s*=/m.test(manifest)) {
    couldNotRun(`${manifestPath} already declares a signing-identity — the repository must not carry one (it is the owner's, not the project's), so this run will not overwrite it`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'ct72-signing-'));
  const p12Path = join(scratch, 'developer-id.p12');
  try {
    writeFileSync(p12Path, Buffer.from(process.env.MACOS_DEVELOPER_ID_P12, 'base64'), { mode: 0o600 });
    if (!readFileSync(p12Path).length) couldNotRun('MACOS_DEVELOPER_ID_P12 did not decode to any bytes — it has to be the base64 of the .p12 file');
    const { identity, failed } = identityFrom(p12Path, process.env.MACOS_DEVELOPER_ID_PASSWORD);
    if (failed) refuse(failed);

    const insertion = [
      '',
      '# Injected by scripts/native-macos-signing.mjs for this run only (never committed): the owner\'s',
      '# Developer ID certificate common name, read out of MACOS_DEVELOPER_ID_P12 on the runner.',
      TABLE,
      `signing-identity = ${JSON.stringify(identity)}`,
      '',
    ].join('\n');
    const anchor = '[package.metadata.packager.deb]';
    const updated = manifest.includes(anchor)
      ? manifest.replace(anchor, `${insertion.trimStart()}\n${anchor}`)
      : `${manifest.trimEnd()}\n${insertion}`;
    writeFileSync(manifestPath, updated);

    console.log(JSON.stringify({
      prepared: true,
      manifest: manifestPath,
      table: TABLE,
      identity,
      certificateBytes: readFileSync(p12Path).length,
      notarization: { keyId: true, issuer: true, keyFile: true },
      note: 'the identity is written into this run\'s copy of the manifest only; the repository keeps none',
    }, null, 2));
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
