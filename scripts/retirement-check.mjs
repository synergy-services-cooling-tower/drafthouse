#!/usr/bin/env node
/**
 * Retirement gate — the JavaScript showcase is retired and must not come back (issue #60).
 *
 * The pattern is the JavaScript-engine retirement's (issue #40 slice 4, D17): a retirement a gate
 * does not assert is a hope, and a skip must never read as a pass. Two axes, both fail-closed:
 *
 *   (a) the retired surface is absent — `showcase/` is not in the tree at all. A file or
 *       directory with that name, whatever it holds, is a failure naming it: the surface was
 *       retired as a whole (D22), so its own name coming back is the regression.
 *   (b) no artifact the deployment ships references the retired module — the files the
 *       deployment record lists (`deploy-manifest.txt`), the plane's document, the plane's
 *       built module (`cockpit/pkg/**`, when it has been built) and the published pieces'
 *       own bytes (`bundles/*.manifest.json` -> `files[].path`) are scanned for the literal
 *       `showcase.js`. A reference there means a shipped artifact would go looking for the
 *       retired surface.
 *
 * The scan literal is the retired module's own name. The recorded fixtures' provenance
 * *citations* of the retired path (`cockpit/assets/fixture.json`, a record copied from the
 * design pass — not a reference anything resolves) are deliberately not a match.
 *
 * Usage — from any working directory:
 *
 *     node scripts/retirement-check.mjs
 *
 * Exit status: 0 the surface is gone and nothing shipped references it; 1 the retired surface or
 * a reference to it is in the tree (every failure is listed, naming the file); 2 the check could
 * not run (no working tree, unreadable record).
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];

const refuse = (message) => {
  console.error(`retirement-check: ${message}`);
  process.exit(2);
};

/** The retired surface's own name, and the module every shipped artifact must not reference. */
const RETIRED_SURFACE = 'showcase';
const RETIRED_MODULE = 'showcase.js';
const MANIFEST = 'deploy-manifest.txt';
const DOCUMENT = 'cockpit/index.html';

/* ---- (a) the retired surface is absent ---- */

const surface = join(root, RETIRED_SURFACE);
if (existsSync(surface)) {
  const entries = statSync(surface).isDirectory() ? readdirSync(surface).slice(0, 8) : [];
  const holds = entries.length === 0 ? '' : ` — it holds: ${entries.join(', ')}${entries.length === 8 ? ', …' : ''}`;
  failures.push(`${RETIRED_SURFACE}/ is back in the tree${holds}; the retired JavaScript surface must stay retired (issue #60 / the private decision record D22)`);
}

/* ---- (b) nothing the deployment ships references the retired module ---- */

const scanTargets = new Set();
if (!existsSync(join(root, MANIFEST))) {
  failures.push(`${MANIFEST} is missing; it is the deployment record this gate scans (run: npm run deploy-manifest)`);
} else {
  for (const line of readFileSync(join(root, MANIFEST), 'utf8').split('\n')) {
    const entry = line.trim();
    if (entry === '' || entry.startsWith('#')) continue;
    scanTargets.add(entry.split(/\s{2,}/)[0]);
  }
}

if (!existsSync(join(root, DOCUMENT))) {
  failures.push(`${DOCUMENT} is missing; it is the plane's document`);
} else {
  scanTargets.add(DOCUMENT);
}

// The plane's built module, when it has been built: the bytes a deployment would run.
const built = join(root, 'cockpit', 'pkg');
if (existsSync(built) && statSync(built).isDirectory()) {
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile()) scanTargets.add(relative(root, path).split(sep).join('/'));
    }
  };
  walk(built);
}

// The published pieces' own bytes: a release ships these, so a reference in one is the class
// this gate exists for.
for (const pin of ['engine', 'visuals']) {
  const file = join(root, 'bundles', `${pin}.manifest.json`);
  if (!existsSync(file)) continue;
  let entries;
  try {
    entries = JSON.parse(readFileSync(file, 'utf8')).files ?? [];
  } catch (error) {
    refuse(`bundles/${pin}.manifest.json is not readable JSON: ${error.message}`);
  }
  for (const entry of entries) scanTargets.add(entry.path);
}

const moduleBytes = Buffer.from(RETIRED_MODULE);
for (const target of scanTargets) {
  const absolute = join(root, target);
  // A missing file is the deploy/bundle checks' failure to name; this gate scans what is there.
  if (!existsSync(absolute) || !statSync(absolute).isFile()) continue;
  const bytes = readFileSync(absolute);
  let found = 0;
  let firstAt = -1;
  for (let at = bytes.indexOf(moduleBytes); at !== -1; at = bytes.indexOf(moduleBytes, at + moduleBytes.length)) {
    found += 1;
    if (firstAt === -1) firstAt = at;
  }
  if (found > 0) {
    const line = bytes.subarray(0, firstAt).toString('utf8').split('\n').length;
    failures.push(`${target} references ${RETIRED_MODULE} (${found} occurrence${found === 1 ? '' : 's'}, first at line ${line}); a shipped artifact must not reference the retired surface`);
  }
}

/* ---- emit ---- */

if (failures.length) {
  console.error(JSON.stringify({ passed: false, retiredSurface: RETIRED_SURFACE, failures: [...new Set(failures)] }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({
  passed: true,
  surfaceAbsent: RETIRED_SURFACE,
  moduleUnreferenced: RETIRED_MODULE,
  scanned: [...scanTargets].length,
  files: [...scanTargets].sort(),
}, null, 2));
