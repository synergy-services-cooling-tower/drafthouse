#!/usr/bin/env node
/**
 * The platform half of the native installers — issue #72.
 *
 * Runs on each `native` matrix runner after `cargo packager` has produced that platform's
 * package. It renames the packager's own output to the release name the issue fixes, writes the
 * `.sha256` sidecar beside it, checks the file is really the kind of package its extension claims,
 * and writes a per-platform record (the bytes, their digests, what signing state the run was in) —
 * the record is what the aggregating `scripts/native-manifest.mjs` collects from all three runners.
 *
 *     node scripts/native-artifacts.mjs --platform linux-x64 \
 *       --packager-out dist/packager --out dist/native --records dist/native-records \
 *       --signing unsigned --runner ubuntu-22.04 \
 *       --command 'cargo build --release --bin drafthouse' \
 *       --command 'cargo packager --release --formats appimage,deb ...'
 *
 * `--signing` is one of `unsigned` (a platform that ships unsigned, or a macOS dry run is
 * `skipped` — a *visible* skip, so `--signing-notice` is required with it and is what the record
 * carries), or `signed-and-notarized` (the macOS tag path, after the codesign + notarytool +
 * stapler steps the workflow runs). A macOS record claiming `unsigned` is refused: an unsigned
 * macOS artifact is a *skipped* run and has to say so.
 *
 * Nothing here types a version: it is the crate's `[package] version`, cross-checked against
 * `package.json` (`scripts/native-products.mjs`).
 *
 * Exit status: 0 the products are named, digested and recorded; 2 refused or could not run (no
 * such platform, the packager's product missing or ambiguous, a file that is not the package its
 * extension claims, a signing state that does not belong to this platform).
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { DOES_NOT_ESTABLISH, PLATFORMS, artifactVersion, git, recordName, sha256, sidecar } from './native-products.mjs';

/** The structure of a real package of each kind: what its first bytes are, or (the dmg) its trailer. */
const MAGIC = {
  '.dmg': { at: 'tail', offset: 512, bytes: [0x6b, 0x6f, 0x6c, 0x79], note: 'a UDIF disk image ends with a 512-byte `koly` trailer' },
  '.deb': { at: 'head', bytes: [0x21, 0x3c, 0x61, 0x72, 0x63, 0x68, 0x3e], note: 'a Debian package is an `!<arch>` archive' },
  '.AppImage': { at: 'head', bytes: [0x7f, 0x45, 0x4c, 0x46], note: 'an AppImage is an ELF executable' },
  '.msi': { at: 'head', bytes: [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1], note: 'an MSI is an OLE2 compound document' },
};

/** Where in the file the magic bytes have to be. */
function magicOffset(magic, length) {
  return magic.at === 'tail' ? length - (magic.offset ?? 0) : 0;
}

function refuse(message) {
  console.error(`native-artifacts: ${message}`);
  process.exit(2);
}

function main() {
  const argv = process.argv.slice(2);
  const values = (flag) => {
    const found = [];
    for (let at = 0; at < argv.length; at += 1) {
      if (argv[at] === flag) {
        if (!argv[at + 1] || argv[at + 1].startsWith('--')) refuse(`${flag} needs a value`);
        found.push(argv[at + 1]);
      }
    }
    return found;
  };
  const value = (flag) => values(flag)[0] ?? null;

  const platform = value('--platform');
  const packagerOut = value('--packager-out');
  const out = value('--out');
  const records = value('--records');
  const signing = value('--signing');
  const signingNotice = value('--signing-notice');
  const runner = value('--runner');
  const commands = values('--command');

  if (!platform) refuse(`--platform needs one of ${Object.keys(PLATFORMS).join(', ')}`);
  const spec = PLATFORMS[platform];
  if (!spec) refuse(`--platform ${JSON.stringify(platform)} is not one of ${Object.keys(PLATFORMS).join(', ')}`);
  for (const [flag, what] of [['--packager-out', packagerOut], ['--out', out], ['--records', records]]) {
    if (!what) refuse(`${flag} needs a directory`);
  }
  // `--out` and `--records` are created here; `--packager-out` is the packager's own directory and
  // has to exist, or there is nothing to publish.
  if (!existsSync(packagerOut) || !statSync(packagerOut).isDirectory()) refuse(`${packagerOut} (--packager-out) is not a directory`);
  if (!signing) refuse(`--signing needs one of ${[...new Set(Object.values(spec.signing))].join(', ')} for ${platform}`);
  const allowed = new Set(Object.values(spec.signing));
  if (!allowed.has(signing)) {
    refuse(
      `--signing ${JSON.stringify(signing)} is not a state this platform can be in (${[...allowed].join(', ')})`
      + (platform === 'macos-universal' && signing === 'unsigned'
        ? ' — an unsigned macOS artifact is a *skipped* run, and the record has to say so rather than claim a decision the run did not make' : ''),
    );
  }
  if (signing === 'skipped' && !signingNotice) {
    refuse('--signing skipped needs --signing-notice: the dry run has to say, in the record, why signing was skipped');
  }
  if (signing !== 'skipped' && signingNotice) refuse('--signing-notice is only for a skipped run');

  const { version, refused } = artifactVersion();
  if (refused) refuse(refused);
  const commit = git(['rev-parse', 'HEAD']);
  if (!commit) refuse('git rev-parse HEAD did not answer — the record names the commit it was assembled from');

  const products = readdirSync(packagerOut).filter((name) => statSync(join(packagerOut, name)).isFile());
  mkdirSync(out, { recursive: true });
  mkdirSync(records, { recursive: true });

  const artifacts = spec.products.map((product) => {
    const candidates = products.filter((name) => name.endsWith(product.extension));
    if (candidates.length !== 1) {
      refuse(`${packagerOut} must hold exactly one *${product.extension}; found ${candidates.length}${candidates.length ? `: ${candidates.join(', ')}` : ''} (the packager's own output names are not the release names — this step locates them by extension)`);
    }
    const source = join(packagerOut, candidates[0]);
    const bytes = readFileSync(source);
    const magic = MAGIC[product.extension];
    const at = magicOffset(magic, bytes.length);
    if (at < 0 || !bytes.subarray(at, at + magic.bytes.length).equals(Buffer.from(magic.bytes))) {
      refuse(`${candidates[0]} is not ${product.extension}: ${magic.note}`);
    }
    const name = product.name(version);
    copyFileSync(source, join(out, name));
    writeFileSync(join(out, `${name}.sha256`), sidecar(sha256(bytes), name));
    return { name, bytes: bytes.length, sha256: sha256(bytes) };
  });

  const record = {
    record: 1,
    platform,
    arch: spec.arch,
    runner: runner ?? spec.runner,
    version,
    commit,
    signing: { state: signing, ...(signingNotice ? { notice: signingNotice } : {}) },
    artifacts,
    commands: commands.map((command) => ({ platform, command })),
    doesNotEstablish: DOES_NOT_ESTABLISH,
  };
  const recordPath = join(records, recordName(platform));
  writeFileSync(recordPath, `${JSON.stringify(record, null, 2)}\n`);

  console.log(JSON.stringify({
    recorded: true,
    platform,
    version,
    commit,
    signing: record.signing,
    out,
    artifacts,
    record: recordPath,
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
