#!/usr/bin/env node
/**
 * Version guard — issue #67: the release version's source of truth, and the mirrors' lockstep.
 *
 * The release version is derived, never typed. `scripts/bundle.mjs`'s `deriveManifests()` reads
 * `package.json`'s `version` and stamps the commit onto it (`<package.json version>+<git commit>`),
 * both pieces carry that value, and `scripts/release-assemble.mjs` names the release with it —
 * it refuses a tag that does not name `v<package.json version>`, which is what a tagged release
 * that was never bumped against. `package.json` is therefore the source of truth, and every other
 * declaration of the version in this repository is a mirror of it:
 *
 *   1. **Lockstep.** The `[package] version` of every committed `Cargo.toml` — the engine, the
 *      cockpit adapter, the cockpit and its two workspace members — and each of those packages'
 *      entry in the `Cargo.lock` beside it. A mirror that lags `package.json` is exactly the
 *      condition that let `v0.2.0` be declared on a tree that said `0.1.0`.
 *   2. **The tag rule.** The declared version must equal the newest `v*` tag or be strictly
 *      greater: a tree whose release tag already exists but whose version was never bumped fails,
 *      naming both numbers. `v<package.json version>` is the release tag's only shape
 *      (`scripts/release-assemble.mjs`), so a tag below is compared as its `major.minor.patch`.
 *   3. **The packager metadata (issue #72).** A `version` written into a
 *      `[package.metadata.packager]` table is a second declaration of the same number, and it must
 *      equal it. The intended state is *absent*: cargo-packager fills the field from the crate
 *      (`crates/packager/src/cli/config.rs`), so every artifact name the native installers carry
 *      is built from rule 1's version, and the table cannot drift from it silently.
 *
 * The tag is read from this clone: a shallow checkout carries none, so the `validate` job fetches
 * them first (`git fetch --depth 1 origin '+refs/tags/v*:refs/tags/v*'`). A clone with no `v*` tag
 * visible prints NOT VERIFIABLE on stderr and exits 0 — loud, never a silent pass; the lockstep
 * rule needs no tag and is always enforced. A `v*` tag that is not `v<major>.<minor>.<patch>` is
 * refused rather than guessed at.
 *
 * Exit status: `0` both rules hold (or no tag is visible — NOT VERIFIABLE); `1` a rule is violated
 * (every failure is named on stderr as JSON); `2` the check could not run (no git, unreadable file,
 * an unparseable version or tag).
 */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE = 'package.json';
const VERSION_PATTERN = /^(\d+)\.(\d+)\.(\d+)$/;
const TAG_PATTERN = /^v(\d+)\.(\d+)\.(\d+)$/;

/** Could not run: no git, an unreadable file, a version or tag the guard will not guess at. */
function refuse(message) {
  console.error(`version-check: ${message}`);
  process.exit(2);
}

/** Raw stdout of a git command run against the repository this script lives in. */
function git(args) {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error) refuse(`git ${args.join(' ')} could not run: ${result.error.message}`);
  if (result.status !== 0) refuse(`git ${args.join(' ')} failed (exit ${result.status}): ${result.stderr.trim()}`);
  return result.stdout;
}

function read(path) {
  try {
    return readFileSync(join(root, path), 'utf8');
  } catch (error) {
    refuse(`${path} is not readable: ${error.message}`);
  }
}

/** Tracked files matching a pathspec, ascending. */
function tracked(pattern) {
  return git(['ls-files', '-z', '--', pattern]).split('\0').filter(Boolean).sort();
}

/** The `[package]` table of a Cargo.toml: the name and version a lock entry repeats. */
function packageTable(path) {
  const lines = read(path).split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === '[package]');
  if (start === -1) refuse(`${path} has no [package] table`);
  let end = lines.length;
  for (let at = start + 1; at < lines.length; at += 1) {
    if (lines[at].trimStart().startsWith('[')) {
      end = at;
      break;
    }
  }
  const field = (key) => {
    for (const line of lines.slice(start + 1, end)) {
      const match = new RegExp(`^\\s*${key}\\s*=\\s*"([^"]*)"`).exec(line);
      if (match) return match[1];
    }
    return null;
  };
  const name = field('name');
  const version = field('version');
  if (!name || !version) refuse(`${path}'s [package] table does not declare both name and version`);
  return { path, name, version };
}

/** name -> version for every `[[package]]` entry of a Cargo.lock. */
function lockEntries(path) {
  const entries = new Map();
  for (const block of read(path).split(/^\[\[package\]\]\r?\n/m).slice(1)) {
    const name = /^name\s*=\s*"([^"]*)"/m.exec(block);
    const version = /^version\s*=\s*"([^"]*)"/m.exec(block);
    if (name && version) entries.set(name[1], version[1]);
  }
  return entries;
}

/**
 * A `[package.metadata.packager]` table and the version it declares, if any (`null` when the
 * table carries none — the intended state). Nested tables (`…packager.deb`, `…packager.macos`)
 * are separate sections and are not read: only the packager's own `version` field is a mirror.
 * Returns `null` when the manifest has no such table at all.
 */
function packagerTable(path) {
  const lines = read(path).split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === '[package.metadata.packager]');
  if (start === -1) return null;
  let end = lines.length;
  for (let at = start + 1; at < lines.length; at += 1) {
    if (lines[at].trimStart().startsWith('[')) {
      end = at;
      break;
    }
  }
  for (const line of lines.slice(start + 1, end)) {
    const match = /^\s*version\s*=\s*"([^"]*)"/.exec(line);
    if (match) return { path, version: match[1] };
  }
  return { path, version: null };
}

function main() {
  const declared = JSON.parse(read(SOURCE)).version;
  if (typeof declared !== 'string' || !VERSION_PATTERN.test(declared)) {
    refuse(`${SOURCE} declares ${JSON.stringify(declared)}, which is not <major>.<minor>.<patch>`);
  }

  const packages = tracked('*Cargo.toml').map(packageTable);
  const locks = tracked('*Cargo.lock').map((path) => ({ path, entries: lockEntries(path) }));
  const packagers = tracked('*Cargo.toml').map(packagerTable).filter(Boolean);

  const failures = [];
  for (const pkg of packages) {
    if (pkg.version !== declared) {
      failures.push(`${pkg.path} declares ${pkg.version} for ${pkg.name}; ${SOURCE} declares ${declared} — a mirror must move with the source of truth`);
    }
    for (const lock of locks) {
      const locked = lock.entries.get(pkg.name);
      if (locked !== undefined && locked !== pkg.version) {
        failures.push(`${lock.path} records ${pkg.name} at ${locked}; ${pkg.path} declares ${pkg.version} — update the lock with cargo, never by hand`);
      }
    }
  }

  // Rule 3: the packager's own metadata. Absent is the intended state — the field is filled from
  // the crate, which rule 1 has already held equal to the source of truth.
  for (const packager of packagers) {
    if (packager.version !== null && packager.version !== declared) {
      failures.push(
        `${packager.path}'s [package.metadata.packager] declares version ${packager.version};`
        + ` ${SOURCE} declares ${declared} — cargo-packager fills that field from the crate's own`
        + ' [package] version, so a version written into the table is a mirror that must not drift'
        + ' (remove it and the crate version governs)',
      );
    }
  }

  const tags = git(['tag', '--list', 'v*', '--sort=-v:refname']).split('\n').filter(Boolean);
  const parsed = tags.map((tag) => {
    const match = TAG_PATTERN.exec(tag);
    if (!match) refuse(`the tag ${JSON.stringify(tag)} is not v<major>.<minor>.<patch>; the guard compares release versions and will not guess at this one`);
    return { tag, parts: match.slice(1).map(Number) };
  });
  const newest = parsed[0] ?? null;
  if (newest) {
    const declaredParts = VERSION_PATTERN.exec(declared).slice(1).map(Number);
    for (let at = 0; at < 3; at += 1) {
      if (declaredParts[at] !== newest.parts[at]) {
        if (declaredParts[at] < newest.parts[at]) {
          failures.push(
            `${SOURCE} declares ${declared}, below the newest release tag ${newest.tag} —`
            + ' a release was declared and the version was never bumped. A release tag names the derived'
            + ` version (scripts/release-assemble.mjs refuses a tag that does not name it): bump to ${newest.parts.join('.')} or higher`,
          );
        }
        break;
      }
    }
  }

  if (failures.length) {
    console.error(JSON.stringify({ passed: false, version: declared, newestTag: newest?.tag ?? null, failures }, null, 2));
    process.exit(1);
  }

  const notVerifiable = newest ? null : "no v* tag is visible in this clone, so the tag rule could not run; fetch them with: git fetch --depth 1 origin '+refs/tags/v*:refs/tags/v*'";
  if (notVerifiable) console.error(`version-check: NOT VERIFIABLE — ${notVerifiable}`);
  console.log(JSON.stringify({
    passed: true,
    verified: newest !== null,
    version: declared,
    newestTag: newest?.tag ?? null,
    mirrors: packages.map((pkg) => ({ path: pkg.path, package: pkg.name, version: pkg.version })),
    locks: locks.map((lock) => ({ path: lock.path, records: packages.filter((pkg) => lock.entries.has(pkg.name)).map((pkg) => `${pkg.name}@${lock.entries.get(pkg.name)}`) })),
    packagers: packagers.map((packager) => ({
      path: packager.path,
      declaresVersion: packager.version,
      versionSource: packager.version === null
        ? `${packager.path}'s [package] version — the crate the packager fills the field from`
        : packager.path,
    })),
    ...(notVerifiable ? { notVerifiable } : {}),
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
