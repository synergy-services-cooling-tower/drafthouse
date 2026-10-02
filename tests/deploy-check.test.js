/**
 * Regression tests for the deployment-readiness check (issue #9; re-pointed at the plane by
 * issue #40 slice 3).
 *
 * Each case builds a throwaway fixture tree (the plane, the committed pins, the checker and
 * its imports, plus the committed `deploy-manifest.txt`) in the system temp directory, mutates
 * it, and runs the check as a subprocess so the assertions see the real exit status and the real
 * stderr/stdout — the same way CI invokes it.
 *
 * The holes these tests pin, all of which passed a plain run before their fix:
 *   1. a side-effect-only import (`import './x.js';`) was invisible to the module-graph walker,
 *      so a missing module in that form neither failed the check nor reached the manifest;
 *   2. the committed manifest was regenerated on the pass path instead of being compared, so a
 *      stale manifest could never fail the check;
 *   3. `--write-manifest` (the explicit regeneration path) must refuse to write when the
 *      derivation itself is broken;
 *   4. the served set carried a hardcoded `styles.css` instead of deriving the document's
 *      stylesheet links, so a newly linked sheet stayed out of the set and the manifest while
 *      the check passed (issue #18);
 *   5. the derivation recognised one spelling of a stylesheet link and skipped the rest in
 *      silence: a `rel` token list, single quotes, a fragment-only href, and uppercase
 *      tag/attribute names each exited 0 with the sheet out of the served set (issue #20);
 *   6. "external" was decided by a `scheme:` prefix, so `https:/x.css` (one slash, uppercase
 *      scheme, backslashes, `https:/./…`) exited 0 with the sheet unserved even though the WHATWG
 *      parser resolves it same-origin (fix round 1);
 *   7. the relation gate admitted two tokens, so `rel="preload" as="style"` — a fetch the browser
 *      performs — exited 0 with its file out of the served set (fix round 2);
 *   8. the reference sweep tested only whether a referenced file EXISTED, never whether it was in
 *      the served set, so a fetched file on disk and out of the manifest exited 0 (fix round 3);
 *   9. the sweep read only `src`/`href`, so `srcset` candidate lists, `<video poster>`,
 *      `<object data>`, the obsolete `background`, SVG `xlink:href` and every `url()`/`@import`
 *      in inline styling or a served stylesheet were invisible, and the parity case mirrored the
 *      checker's relation set by regexing its source text (fix round 4).
 *
 * What the #60 re-point adds, and what the tests below pin:
 *  10. the deploy set is the cockpit PLANE's committed half — the document, the modules and sheets
 *      it loads, and every file under `cockpit/assets/` by the plane's own definition (the app
 *      fetches the fixture, the field lists and the fonts at run time, which no static walk can
 *      see); the built module (`cockpit/pkg/**`) is deliberately NOT part of the record — it is a
 *      build product, and the plane artifact's own manifest covers it;
 *  11. the reference sweep and the module graph are the same rule, applied in the surface's own
 *      directory: a fetch target written in the surface's own modules is demanded exactly like a
 *      linked sheet — and a fetch position whose target cannot be named statically is REPORTED
 *      (`unresolvedFetchPositions`), never silently skipped.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CHECK = 'scripts/deploy-check.mjs';
const MANIFEST = 'deploy-manifest.txt';
const DOCUMENT = 'cockpit/index.html';
const SURFACE = 'cockpit';
/** A committed plane asset — the record the app fetches at run time, demanded by its directory. */
const ASSET = 'cockpit/assets/custom-fields.json';
const CHECKER_IMPORTS = ['scripts/bundle.mjs'];

function copyTree(source, destination) {
  if (statSync(source).isDirectory()) {
    mkdirSync(destination, { recursive: true });
    for (const name of readdirSync(source)) copyTree(join(source, name), join(destination, name));
  } else {
    copyFileSync(source, destination);
  }
}

function copyInto(directory, relativePath) {
  const destination = join(directory, relativePath);
  mkdirSync(dirname(destination), { recursive: true });
  copyTree(join(root, relativePath), destination);
}

/** A fixture tree containing everything `deploy-check.mjs` reads, plus the script itself. */
function fixture(mutate) {
  const directory = mkdtempSync(join(tmpdir(), 'deploy-check-'));
  copyInto(directory, CHECK);
  for (const path of CHECKER_IMPORTS) copyInto(directory, path);
  copyInto(directory, MANIFEST);
  // Copy only the committed serving inputs, not the Rust target tree or built pkg/.
  copyInto(directory, DOCUMENT);
  copyInto(directory, `${SURFACE}/assets`);
  if (mutate) mutate(directory);
  return directory;
}

function runCheck(directory, args = []) {
  const result = spawnSync(process.execPath, [CHECK, ...args], { cwd: directory, encoding: 'utf8' });
  return { status: result.status, stdout: result.stdout, stderr: result.stderr, summary: result.stdout ? JSON.parse(result.stdout) : null };
}

function cleanup(testContext, directory) {
  testContext.after(() => rmSync(directory, { recursive: true, force: true }));
}

function manifest(directory) {
  return readFileSync(join(directory, MANIFEST), 'utf8');
}

function manifestEntry(directory, path) {
  return manifest(directory).split('\n').find((line) => line === path || line.startsWith(`${path}  `));
}

function replaceManifest(directory, mutate) {
  writeFileSync(join(directory, MANIFEST), mutate(manifest(directory)));
}

function withoutManifestLine(directory, path) {
  const before = manifest(directory);
  const after = before.split('\n').filter((line) => !(line === path || line.startsWith(`${path}  `))).join('\n');
  assert.notEqual(after, before, `the fixture manifest lists no line for ${path}`);
  writeFileSync(join(directory, MANIFEST), after);
}

/** A well-formed manifest entry whose digest no file has — the "file the plane does not serve" shape. */
function unservedManifestLine(path) {
  return `${path}  ${'0'.repeat(64)}  1`;
}

function prepend(file, line) {
  writeFileSync(file, line + readFileSync(file, 'utf8'));
}

function append(file, line) {
  writeFileSync(file, `${readFileSync(file, 'utf8')}\n${line}\n`);
}

function insertMarkup(directory, markup) {
  const path = join(directory, DOCUMENT);
  writeFileSync(path, readFileSync(path, 'utf8').replace('<head>', `<head>\n  ${markup}`));
}

/**
 * The plane's own module, loaded by the document: the fixture's stand-in for the built module a
 * real plane carries (`cockpit/pkg/**`, a build product no fixture can have). A case injects the
 * source it needs — an import, a fetch position — and the document loads it, so the module graph
 * and the fetch-position scan apply exactly as they do to the real plane.
 *
 * The module joins the manifest in the same step: the fixture tree must stay consistent (a
 * demanded module is a record the manifest carries), so a case's own subject is the only thing
 * that can go red.
 */
function withAppModule(directory, source) {
  const module = `${SURFACE}/app.js`;
  writeFileSync(join(directory, module), source);
  insertMarkup(directory, '<script type="module" src="./app.js"></script>');
  writeFileSync(join(directory, MANIFEST), `${manifest(directory)}${module}\n`);
}

/** A red run must name the file and list the failure; the failures are JSON, so read them back. */
function assertNamesFailure(run, pattern) {
  assert.equal(run.status, 1, `expected exit 1; stdout: ${run.stdout}`);
  const failures = JSON.parse(run.stderr).failures;
  assert.match(failures.join('\n'), pattern);
}

test('control: the committed manifest and the plane tree pass', (t) => {
  const directory = fixture();
  cleanup(t, directory);
  assert.deepEqual(readdirSync(join(directory, SURFACE)).sort(), ['assets', 'index.html'],
    'fixtures must exclude Rust sources, target artifacts and built pkg/');

  const run = runCheck(directory);
  assert.equal(run.status, 0, `expected pass; stderr: ${run.stderr}`);
  assert.equal(run.summary.passed, true);
  assert.equal(run.summary.document, DOCUMENT);
  assert.equal(run.summary.manifestVerified, true);
  // The set is the plane's committed half: the document and every file under `assets/`.
  const files = run.summary.files.map((entry) => entry.split('  ')[2]);
  for (const path of [
    DOCUMENT,
    'cockpit/assets/fixture.json', 'cockpit/assets/custom-fields.json', 'cockpit/assets/duty-fields.json',
    'cockpit/assets/fonts/OFL.txt', 'cockpit/assets/fonts/subset/IBMPlexSans-Regular.ttf',
  ]) {
    assert.ok(files.includes(path), `${path} must be in the derived set; got ${files.join(', ')}`);
  }
  assert.equal(run.summary.fileCount, files.length);
  assert.equal(run.summary.assets, files.length - 1, 'every derived file but the document is a plane asset');
});

test('the plane’s assets are demanded by their directory: a new asset not in the manifest fails, naming it', (t) => {
  const directory = fixture((d) => writeFileSync(join(d, 'cockpit/assets/extra.json'), '{}\n'));
  cleanup(t, directory);

  // Nothing statically fetches `assets/extra.json` — that is why the directory is the declaration.
  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/assets\/extra\.json/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/assets/extra.json'), 'the regenerated manifest must list the new asset');
  assert.equal(runCheck(directory).status, 0);
});

test('the built module is not part of the committed record: pkg/** is neither demanded nor listed', (t) => {
  const directory = fixture((d) => {
    mkdirSync(join(d, 'cockpit/pkg'), { recursive: true });
    writeFileSync(join(d, 'cockpit/pkg/drafthouse_cockpit.js'), 'export const built = true;\n');
  });
  cleanup(t, directory);
  const before = manifest(directory);

  // The build product ships in the plane artifact, whose own manifest covers it; this committed
  // record neither demands it nor lets it be listed.
  const run = runCheck(directory);
  assert.equal(run.status, 0, `expected pass; stderr: ${run.stderr}`);
  assert.ok(!run.summary.files.some((entry) => entry.includes('cockpit/pkg/')), 'pkg/** is not a derived entry');
  assert.equal(manifest(directory), before, 'the check must not write to the committed manifest');

  const listed = fixture((d) => replaceManifest(d, (text) => `${text}${unservedManifestLine('cockpit/pkg/drafthouse_cockpit.js')}\n`));
  cleanup(t, listed);
  assertNamesFailure(runCheck(listed), /deploy-manifest\.txt lists files the plane does not serve: [^\n]*cockpit\/pkg\/drafthouse_cockpit\.js/);
});

test('a committed manifest missing a served file fails, naming it, and is not silently healed', (t) => {
  const directory = fixture((d) => withoutManifestLine(d, ASSET));
  cleanup(t, directory);
  const before = manifest(directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /deploy-manifest\.txt is missing: [^\n]*cockpit\/assets\/custom-fields\.json/);
  assert.equal(manifest(directory), before, 'the failing run must not rewrite the committed manifest');
});

test('a committed manifest missing the plane’s own licence notice fails, naming it', (t) => {
  const directory = fixture((d) => withoutManifestLine(d, 'cockpit/assets/fonts/OFL.txt'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /deploy-manifest\.txt is missing: [^\n]*fonts\/OFL\.txt/);
});

test('a committed manifest missing an asset the app fetches fails, naming it', (t) => {
  const directory = fixture((d) => withoutManifestLine(d, 'cockpit/assets/fixture.json'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /deploy-manifest\.txt is missing: [^\n]*cockpit\/assets\/fixture\.json/);
});

test('a manifest entry the plane does not serve fails, naming it', (t) => {
  const directory = fixture((d) => replaceManifest(d, (text) => `${text}${unservedManifestLine('cockpit/extra.js')}\n`));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /deploy-manifest\.txt lists files the plane does not serve: [^\n]*cockpit\/extra\.js/);
});

test('a manifest entry for the retired JavaScript surface fails: the deploy set is the plane', (t) => {
  const directory = fixture((d) => replaceManifest(d, (text) => `${text}${unservedManifestLine('index.html')}\n${unservedManifestLine('src/app.js')}\n`));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /lists files the plane does not serve: [^\n]*index\.html/);
  assert.match(run.stderr, /src\/app\.js/);
});

test('--write-manifest regenerates a stale manifest; the next plain run verifies it', (t) => {
  const directory = fixture((d) => withoutManifestLine(d, ASSET));
  cleanup(t, directory);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.equal(write.summary.manifestWritten, true);
  assert.ok(manifestEntry(directory, ASSET), 'the regenerated manifest must list the asset');
  assert.equal(readFileSync(join(directory, MANIFEST), 'utf8'), manifest(directory));
  // Regeneration is idempotent: the same tree writes the same bytes.
  const first = manifest(directory);
  assert.equal(runCheck(directory, ['--write-manifest']).status, 0);
  assert.equal(manifest(directory), first);

  const verify = runCheck(directory);
  assert.equal(verify.status, 0, `expected the regenerated manifest to verify; stderr: ${verify.stderr}`);
  assert.equal(verify.summary.manifestVerified, true);
  assert.equal(verify.summary.manifestEntries, verify.summary.fileCount);
});

test('--write-manifest refuses to write when the derivation is broken', (t) => {
  const directory = fixture((d) => withAppModule(d, "import './does-not-exist.js';\n"));
  cleanup(t, directory);
  const before = manifest(directory);

  const run = runCheck(directory, ['--write-manifest']);
  assert.equal(run.status, 1, `expected failure; stdout: ${run.stdout}`);
  assert.equal(manifest(directory), before, 'a broken derivation must not rewrite the manifest');
});

test('a manifest line outside <path> or <path>  <sha256>  <bytes> fails: the shape is not guessed', (t) => {
  const directory = fixture((d) => replaceManifest(d, (text) => text.replace(/^cockpit\/index\.html$/m, 'cockpit/index.html  1234')));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /it has a line that is neither <path> nor <path>  <sha256>  <bytes>: cockpit\/index\.html  1234/);
});

test('a digest recorded for a file that is not vendored fails: only a pinned file carries one', (t) => {
  const directory = fixture((d) => replaceManifest(d, (text) => text.replace(
    /^cockpit\/index\.html$/m,
    `cockpit/index.html  ${'4'.repeat(64)}  7351`
  )));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /cockpit\/index\.html — deploy-manifest\.txt records a digest for a file that is not vendored/);
});

test('a manifest that lists a file twice fails', (t) => {
  const directory = fixture((d) => replaceManifest(d, (text) => `${text}${manifestEntry(d, DOCUMENT)}\n`));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /it lists cockpit\/index\.html more than once/);
});

test('the missing plane document is a loud refusal, not a silent pass', (t) => {
  const directory = fixture((d) => rmSync(join(d, DOCUMENT)));
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 2, `expected a refusal; stdout: ${run.stdout}`);
  assert.match(run.stderr, /cockpit\/index\.html is missing; the deploy set cannot be derived/);
});

test('a stylesheet linked by the document but absent from the manifest fails, naming it', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
    insertMarkup(d, '<link rel="stylesheet" href="./extra.css">');
  });
  cleanup(t, directory);

  // At the reviewed base this exact tree passed: the linked sheet was never derived, so neither
  // the served set nor the manifest mentioned it, and a copy-deploy served a page with no styles.
  const run = runCheck(directory);
  assertNamesFailure(run, /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.css/);

  // Recording it deliberately (the regeneration path) is what makes it green.
  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/extra.css'));
  assert.equal(runCheck(directory).status, 0);
});

test('every spelling of a stylesheet link resolves to its file and joins the served set', (t) => {
  const spellings = [
    ['rel token list', '<link rel="stylesheet preload" as="style" href="./extra.css">'],
    ['single quotes', "<link rel='stylesheet' href='./extra.css'>"],
    ['no quotes', '<link rel=stylesheet href=./extra.css>'],
    ['uppercase names', '<LINK REL="STYLESHEET" HREF="./extra.css">'],
    ['attribute order', '<link href="./extra.css" rel="stylesheet">'],
    ['a fragment', '<link rel="stylesheet" href="./extra.css#top">'],
    ['a bare relative path', '<link rel="stylesheet" href="extra.css">'],
  ];
  for (const [label, tag] of spellings) {
    const directory = fixture((d) => {
      writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
      insertMarkup(d, tag);
    });
    const run = runCheck(directory);
    assert.equal(run.status, 1, `${label}: expected the unrecorded sheet to fail; stdout: ${run.stdout}`);
    assert.match(run.stderr, /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.css/, `${label}: must name the file`);
    rmSync(directory, { recursive: true, force: true });
  }
});

test('a linked stylesheet whose file is missing fails as a missing reference, naming it', (t) => {
  const directory = fixture((d) => insertMarkup(d, '<link rel="stylesheet" href="./not-here.css">'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /cockpit\/index\.html references a missing file: cockpit\/not-here\.css/);
});

test('a stylesheet link written as a same-origin slash-tolerant URL is resolved, not skipped', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
    insertMarkup(d, '<link rel="stylesheet" href="https:/./extra.css">');
  });
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 1, `expected the unrecorded sheet to fail; stdout: ${run.stdout}`);
  assert.match(run.stderr, /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.css/);
});

test('a root-relative stylesheet link is refused, naming the tag', (t) => {
  const directory = fixture((d) => insertMarkup(d, '<link rel="stylesheet" href="/extra.css">'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /cockpit\/index\.html has a stylesheet <link> that cannot be classified \(the URL is root-relative/);
});

test('an absolute stylesheet URL stays excluded and does not fail the check', (t) => {
  const directory = fixture((d) => insertMarkup(d, '<link rel="stylesheet" href="https://cdn.example.com/x.css">'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 0, `expected pass; stderr: ${run.stderr}`);
  assert.ok(!run.summary.files.some((entry) => entry.includes('cdn.example.com')));
});

test('a <link> that never closes mid-document is refused, and the write path cannot bless it', (t) => {
  const directory = fixture((d) => insertMarkup(d, '<link rel="stylesheet" href="./extra.css" <div>'));
  cleanup(t, directory);
  const before = manifest(directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /has a <link> tag that cannot be parsed \(the tag runs into the markup that follows it\)/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 1, `the write path must refuse; stdout: ${write.stdout}`);
  assert.equal(manifest(directory), before, 'the write path must not bless an unreadable document');
});

test('an unclosed <link> at end of file is refused by the same guard', (t) => {
  const directory = fixture((d) => append(join(d, DOCUMENT), '<link rel="stylesheet" href="./extra.css"'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assertNamesFailure(run, /has 1 <link> tag\(s\) that never close/);
});

test('every relation that dereferences the href is demanded; a foreign origin stays external', (t) => {
  for (const relation of ['modulepreload', 'prefetch', 'prerender', 'manifest', 'icon', 'apple-touch-icon', 'mask-icon', 'compression-dictionary']) {
    const directory = fixture((d) => {
      writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
      insertMarkup(d, `<link rel="${relation}" href="./extra.css">`);
    });
    const run = runCheck(directory);
    assert.equal(run.status, 1, `${relation}: expected the unrecorded file to fail; stdout: ${run.stdout}`);
    assert.match(run.stderr, /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.css/, `${relation}: must name the file`);
    rmSync(directory, { recursive: true, force: true });
  }

  const external = fixture((d) => insertMarkup(d, '<link rel="preload" as="script" href="https://cdn.example.com/x.js">'));
  cleanup(t, external);
  assert.equal(runCheck(external).status, 0, 'a foreign origin names no file of this tree');
});

test('the preload boundary: no destination is not a fetch, an unrecognized one is refused', (t) => {
  const noDestination = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
    insertMarkup(d, '<link rel="preload" href="./extra.css">');
  });
  cleanup(t, noDestination);
  assert.equal(runCheck(noDestination).status, 0, 'a preload with no as= requests nothing');

  const unknown = fixture((d) => insertMarkup(d, '<link rel="preload" as="not-a-destination" href="./extra.css">'));
  cleanup(t, unknown);
  assertNamesFailure(runCheck(unknown), /preload destination as="not-a-destination" is not one the browser requests/);
});

test('the fetching-relation table mirrors the checker’s runtime relation set', async () => {
  const module = await import(pathToFileURL(join(root, CHECK)).href);
  const table = [
    'stylesheet', 'modulepreload', 'preload', 'prefetch', 'prerender', 'manifest',
    'compression-dictionary',
    'icon', 'apple-touch-icon', 'apple-touch-icon-precomposed', 'mask-icon',
  ];
  assert.deepEqual([...module.FETCHING_RELATIONS].sort(), table.sort(),
    'the suite’s relation table must be the checker’s own set, or a relation can join one and not the other');
  for (const relation of ['expect', 'subresource', 'serviceworker', 'apple-touch-startup-image', 'dns-prefetch', 'preconnect']) {
    assert.ok(!module.FETCHING_RELATIONS.has(relation), `${relation} must stay a recorded non-fetch`);
  }
});

test('every relation recorded as a non-fetch is left alone', (t) => {
  for (const relation of ['expect', 'subresource', 'serviceworker', 'apple-touch-startup-image', 'dns-prefetch', 'preconnect', 'alternate']) {
    const directory = fixture((d) => insertMarkup(d, `<link rel="${relation}" href="./not-a-file-of-this-tree.json">`));
    const run = runCheck(directory);
    assert.equal(run.status, 0, `${relation}: a non-fetching relation must not demand its href; stderr: ${run.stderr}`);
    assert.ok(!run.summary.files.some((entry) => entry.includes('not-a-file-of-this-tree')), `${relation}: nothing was demanded`);
    rmSync(directory, { recursive: true, force: true });
  }
});

test('a multi-candidate srcset demands every candidate, not just the first', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/lo.png'), 'lo\n');
    writeFileSync(join(d, 'cockpit/hi.png'), 'hi\n');
    insertMarkup(d, '<img src="./lo.png" srcset="./lo.png 1x, ./hi.png 2x" alt="">');
    // Only the first candidate is recorded: the second is a fetch a hi-dpi screen would make.
    writeFileSync(join(d, MANIFEST), `${manifest(d)}cockpit/lo.png\n`);
  });
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/hi\.png/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/hi.png'));
  assert.equal(runCheck(directory).status, 0);
});

test('the tag-shaped fetches are demanded: video poster, object data, background, xlink:href', (t) => {
  const shapes = [
    ['a video poster', '<video poster="./extra.bin" width="1" height="1"></video>'],
    ['an object data', '<object data="./extra.bin" type="text/html"></object>'],
    ['a body background', '<body background="./extra.bin">'],
    ['a table background', '<table background="./extra.bin"><tr><td></td></tr></table>'],
    ['an SVG xlink:href', '<svg xmlns:xlink="http://www.w3.org/1999/xlink"><use xlink:href="./extra.bin"></use></svg>'],
  ];
  for (const [label, markup] of shapes) {
    const directory = fixture((d) => {
      writeFileSync(join(d, 'cockpit/extra.bin'), 'binary-ish\n');
      insertMarkup(d, markup);
    });
    const run = runCheck(directory);
    assert.equal(run.status, 1, `${label}: expected the unrecorded file to fail; stdout: ${run.stdout}`);
    assert.match(JSON.parse(run.stderr).failures.join('\n'), /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.bin/, `${label}: must name the file`);
    rmSync(directory, { recursive: true, force: true });
  }
});

test('a hyperlink is navigation, not a fetch, and a foreign-origin href is refused', (t) => {
  const navigation = fixture((d) => insertMarkup(d, '<a href="./not-served.html">a document</a>'));
  cleanup(t, navigation);
  const navigationRun = runCheck(navigation);
  assert.equal(navigationRun.status, 0, `a hyperlink names no fetch; stderr: ${navigationRun.stderr}`);
  assert.ok(!navigationRun.summary.files.some((entry) => entry.includes('not-served.html')));

  const foreign = fixture((d) => insertMarkup(d, '<link rel="stylesheet" href="https://cdn.example.com/theme.css">'));
  cleanup(t, foreign);
  assert.equal(runCheck(foreign).status, 0, 'another origin is not a file of this tree');
});

test('a srcset candidate resolves like any reference: foreign and data qualify, missing fail', (t) => {
  const excluded = fixture((d) => insertMarkup(d, '<img srcset="https://cdn.example.com/a.png 1x, data:image/svg+xml,%3Csvg/%3E 2x" alt="">'));
  cleanup(t, excluded);
  assert.equal(runCheck(excluded).status, 0, `excluded candidates request nothing of this tree; stderr: ${excluded.stderr}`);

  const missing = fixture((d) => insertMarkup(d, '<img srcset="./lo.png 1x, ./gone.png 2x" alt="">'));
  cleanup(t, missing);
  assertNamesFailure(runCheck(missing), /cockpit\/index\.html references a missing file: cockpit\/gone\.png/);
});

test('a reference the document makes but does not ship fails, and the writer records it', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.js'), 'export const extra = true;\n');
    insertMarkup(d, '<script src="./extra.js"></script>');
  });
  cleanup(t, directory);

  // The file exists on disk but is out of the served set: a copy-deploy would ship it missing.
  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.js/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/extra.js'));
  assert.equal(runCheck(directory).status, 0);
});

test('a missing script fails as a missing reference, naming it', (t) => {
  const directory = fixture((d) => insertMarkup(d, '<script src="./nope.js"></script>'));
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /cockpit\/index\.html references a missing file: cockpit\/nope\.js/);
});

test('a module the document loads statically is walked: a missing import fails, naming it', (t) => {
  const directory = fixture((d) => withAppModule(d, "import { nothing } from './not-a-module.js';\n"));
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /cockpit\/app\.js references a missing file: cockpit\/not-a-module\.js/);
});

test('a side-effect-only import of a missing module fails, naming the module', (t) => {
  const directory = fixture((d) => withAppModule(d, "import './does-not-exist.js';\n"));
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /cockpit\/app\.js references a missing file: cockpit\/does-not-exist\.js/);
});

test('a module the surface loads joins the set, and an unrecorded one is named', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.js'), 'export const extra = true;\n');
    withAppModule(d, "import { extra } from './extra.js';\n");
  });
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.js/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/extra.js'));
  assert.equal(runCheck(directory).status, 0);
});

test('a fetch target a surface module names as a literal is demanded, naming it when unrecorded', (t) => {
  const directory = fixture((d) => {
    mkdirSync(join(d, 'cockpit/data'), { recursive: true });
    writeFileSync(join(d, 'cockpit/data/extra.json'), '{}\n');
    withAppModule(d, "export const probe = () => fetch('./data/extra.json');\n");
  });
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/data\/extra\.json/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/data/extra.json'));
});

test('a fetch target named through a single literal const is demanded too', (t) => {
  const directory = fixture((d) => {
    mkdirSync(join(d, 'cockpit/data'), { recursive: true });
    writeFileSync(join(d, 'cockpit/data/extra.json'), '{}\n');
    withAppModule(d, "const EXTRA = './data/extra.json';\nexport const probe = () => fetch(EXTRA);\n");
  });
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/data\/extra\.json/);
});

test('a dynamic import of a module is a fetch: its target is demanded, and walked', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.js'), 'export const extra = true;\n');
    withAppModule(d, "export const load = async () => (await import('./extra.js')).extra;\n");
  });
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.js/);
});

test('a fetch position whose target cannot be named is reported, never silently skipped', (t) => {
  const directory = fixture((d) => withAppModule(d, "const forward = (url) => fetch(url);\nexport const probe = (base) => fetch(base + '/data/extra.json');\n"));
  cleanup(t, directory);

  // The surface's own helpers forward a parameter (`fetch(url)`), and the probe concatenates a
  // URL. Neither can be resolved statically, so nothing is demanded for them and both are
  // REPORTED: the browser-observed audit is what proves the page's real request set, and the
  // report says so instead of letting an unnamed fetch pass in silence.
  const run = runCheck(directory);
  assert.equal(run.status, 0, `an unnameable position demands nothing, so the tree still passes; stderr: ${run.stderr}`);
  const positions = run.summary.unresolvedFetchPositions;
  assert.ok(positions.some((position) => /cockpit\/app\.js:\d+: base \+ '\/data\/extra\.json'$/.test(position)), positions.join(' | '));
  assert.ok(positions.some((position) => /cockpit\/app\.js:\d+: url$/.test(position)), positions.join(' | '));
  assert.ok(!run.summary.files.some((entry) => entry.includes('extra.json')), 'an unnameable target is not demanded');
});

test('a fetch position written inside a comment is text, not a fetch', (t) => {
  const directory = fixture((d) => withAppModule(d, "// export const probe = () => fetch('./nowhere.json');\n"));
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 0, `expected pass; stderr: ${run.stderr}`);
  assert.ok(!run.summary.unresolvedFetchPositions.some((position) => position.includes('nowhere.json')), 'a comment is not a fetch position');
  assert.ok(!run.summary.files.some((entry) => entry.includes('nowhere.json')), 'a comment demands nothing');
});

test('an inline style url() and a <style> block are demanded by the same rule', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.png'), 'not really a png\n');
    insertMarkup(d, '<div style="background-image: url(./extra.png)"></div>\n<style>@import "./extra.css";</style>');
    writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
  });
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 1, `expected failure; stdout: ${run.stdout}`);
  assert.match(run.stderr, /deploy-manifest\.txt is missing: [^\n]*cockpit\/extra\.png/);
  assert.match(run.stderr, /cockpit\/extra\.css/);
});

test('a served stylesheet’s url() target is demanded, resolving against the sheet itself', (t) => {
  const directory = fixture((d) => {
    mkdirSync(join(d, 'cockpit/styles'), { recursive: true });
    writeFileSync(join(d, 'cockpit/styles/extra.css'), '.probe { background-image: url(./extra.png); }\n');
    writeFileSync(join(d, 'cockpit/styles/extra.png'), 'not really a png\n');
    insertMarkup(d, '<link rel="stylesheet" href="./styles/extra.css">');
  });
  cleanup(t, directory);

  // Resolved against the sheet's own directory (`cockpit/styles/`), not the document's: a
  // document-relative resolution would name `cockpit/extra.png`, which does not exist.
  const run = runCheck(directory);
  assertNamesFailure(run, /deploy-manifest\.txt is missing: [^\n]*cockpit\/styles\/extra\.png/);

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, 'cockpit/styles/extra.png'));
  assert.equal(runCheck(directory).status, 0);
});

test('a directory of the surface that serves nothing is not demanded: README.md stays out of the set', (t) => {
  const directory = fixture((d) => writeFileSync(join(d, 'cockpit/README.md'), '# not served\n'));
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 0, `expected pass; stderr: ${run.stderr}`);
  assert.ok(!run.summary.files.some((entry) => entry.includes('README.md')), 'a file nothing fetches is not part of the deploy set');
});

test('an asset missing from the tree is named: the manifest is not silently healed', (t) => {
  const directory = fixture((d) => rmSync(join(d, 'cockpit/assets/fixture.json')));
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt lists files the plane does not serve: [^\n]*cockpit\/assets\/fixture\.json/);
});

test('the document itself is the served set’s anchor: it is always listed', (t) => {
  const directory = fixture((d) => withoutManifestLine(d, DOCUMENT));
  cleanup(t, directory);

  assertNamesFailure(runCheck(directory), /deploy-manifest\.txt is missing: [^\n]*cockpit\/index\.html/);

  // Nothing was written on the failing path: the committed manifest is the record, and the
  // deliberate regeneration is the only writer.
  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  assert.ok(manifestEntry(directory, DOCUMENT));
});

test('a source quoted by a module but not a fetch is not demanded: a string is not a reference', (t) => {
  const directory = fixture((d) => withAppModule(d, "export const note = 'run ./nowhere.sh by hand';\n"));
  cleanup(t, directory);

  const run = runCheck(directory);
  assert.equal(run.status, 0, `expected pass; stderr: ${run.stderr}`);
  assert.ok(!run.summary.files.some((entry) => entry.includes('nowhere.sh')), 'a string outside a fetch position is not a fetch');
});

test('references that ship stay green for every demand shape (no false positives)', (t) => {
  const directory = fixture((d) => {
    writeFileSync(join(d, 'cockpit/extra.js'), 'export const extra = true;\n');
    writeFileSync(join(d, 'cockpit/extra.css'), 'body { color: red; }\n');
    mkdirSync(join(d, 'cockpit/data'), { recursive: true });
    writeFileSync(join(d, 'cockpit/data/extra.json'), '{}\n');
    insertMarkup(d, [
      '<link rel="stylesheet" href="./extra.css">',
      '<link rel="modulepreload" href="./extra.js">',
      '<script type="module" src="./extra.js"></script>',
      '<style>@import url("./extra.css");</style>',
    ].join('\n'));
    withAppModule(d, "export const probe = () => fetch('./data/extra.json');\n");
  });
  cleanup(t, directory);

  // Every one of them is demanded, so each must be recorded before the tree is consistent.
  const run = runCheck(directory);
  assert.equal(run.status, 1);
  for (const file of ['cockpit/extra.css', 'cockpit/extra.js', 'cockpit/data/extra.json']) {
    assert.match(run.stderr, new RegExp(`deploy-manifest\\.txt is missing: [^\\n"]*${file.replace(/[./]/g, '\\$&')}`), file);
  }

  const write = runCheck(directory, ['--write-manifest']);
  assert.equal(write.status, 0, `expected regeneration to pass; stderr: ${write.stderr}`);
  const verify = runCheck(directory);
  assert.equal(verify.status, 0, `expected pass after regeneration; stderr: ${verify.stderr}`);
});
