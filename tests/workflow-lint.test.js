/**
 * Workflow lint (issue #92): this repository's Actions minutes are billed, so a pull request and
 * a push to `staging` start the `quick` tier and nothing else. It also guards the mirror image in
 * the public snapshot: the generated public CI must stay untiered (public runners are free).
 *
 * It reads every workflow under `WORKFLOW_LINT_ROOT/.github/workflows` (default: this repository)
 * and enforces, fail-closed:
 *
 *   * the workflow triggers match its shape — the private one: `pull_request`, a push to
 *     `main`/`staging`, `workflow_dispatch`; the generated public CI: `pull_request` and a push
 *     to `main`, nothing else;
 *   * a `quick` job exists exactly when the workflow is the private one, and in the private
 *     workflow every job is classified — quick or heavy (the list below) — with the quick job
 *     itself ungated (a gate on it would leave a PR running nothing);
 *   * every heavy job carries the private gate, spelled one way:
 *     `github.event_name == 'workflow_dispatch' || github.ref == 'refs/heads/main'` — and no job
 *     in the generated public CI carries it (the public suite must not skip a job on a PR);
 *   * no workflow that can trigger on `pull_request` or on a push to `staging` carries a job that
 *     reaches a macOS or Windows runner, or a heavy job, without that gate;
 *   * the private quick tier carries its four legs — the format gate, the static check, the fast
 *     unit suite, and the snapshot leak gate (the builder and the checker together) — while the
 *     generated public CI keeps the full chain and the plane build;
 *   * the job a pull request may start — the private `quick` tier, or the generated public CI's
 *     `validate` job — carries **no escape hatch** (fix contract 2, extended by issue #94): no
 *     `|| true` / `|| :` / `set +e` in any step's `run:`, no `continue-on-error` (step level or
 *     job level), no echo-only step, and **no step-level `if:`** — any value that can skip a
 *     step (`false`, a condition, an expression) is refused, because no step on either surface
 *     carries one, so nothing is allowlisted. A leg that cannot fail the job is not a gate, so
 *     each of those shapes is refused, named with its step and line;
 *   * the step-level `if:` refusal is **scoped by the trigger, and the scope enforces itself**
 *     (issue #123): it reaches every workflow a pull request can start — a file whose `on:` block
 *     carries `pull_request` may carry no step-level `if:` in any job, which
 *     `assertNoStepLevelIfOnPullRequestWorkflows` re-derives from each file's own trigger on every
 *     run. `.github/workflows/release.yml` is outside that set **because of its trigger**, not by
 *     allowlist: a `v*` tag push plus `workflow_dispatch`, never `pull_request` — its `on:` block
 *     is release.yml:15-18 — so it cannot start on a pull request, and the step-level `if:` sites
 *     it carries (23 when this note was written; the scan re-derives the sites from the parse,
 *     never from that number) are legal today. Add `pull_request` to that trigger and the scan
 *     refuses them by name;
 *   * the release workflow the builder emits (issue #102) keeps the properties a consumer pins
 *     by: a `v*` tag push and a dispatch dry run, public (`ubuntu`) runners only, no repository
 *     secret, the plane built through its gated build script, the plane's three parts asserted
 *     before packing, and the archive + `.sha256` + manifest published — with the recognition
 *     sentence below, so this lint reads it as generated rather than as a private workflow;
 *   * the lane archive's committed evidence carries no local paths (issue #94, part 2; the
 *     report half is issue #25; the commitability rule is the follow-up): a log, a markdown
 *     report under the archive, or the root lane report that still spells this machine's home,
 *     lane-worktree or per-user temp path fails unless it is byte-identical to its recorded
 *     seal; the writers redact before they write, and the pre-existing report population is
 *     sealed like the logs. The scan judges only what a commit can carry — a file git ignores
 *     (a probe's uncompressed `.log` leftover beside its committed `.log.gz`) reaches no commit
 *     and is not judged, while anything tracked is, so the exemption cannot become an escape
 *     hatch. (The check is embedded in the first case below, for the count-pin reason the
 *     release-workflow checks state there.)
 *
 * The generated public CI is recognised by the provenance sentence its builder writes into it
 * (`GENERATED` below, from `scripts/public-snapshot.mjs`); a file that loses that sentence is
 * checked against the private rules instead — loudly, never silently.
 *
 * The parse is deliberately small: it understands this repository's workflow shape (two-space
 * block indentation, inline lists) and throws on a shape it does not recognise, so a workflow
 * that grows something this lint cannot read fails loudly rather than passing unread. Comments
 * are stripped before step bodies are matched, so a commented-out command does not count.
 *
 * `WORKFLOW_LINT_ROOT` exists for the RED probe: copy `.github/workflows`, mutate the copy,
 * point the lint at it.
 */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const workflowDir = join(process.env.WORKFLOW_LINT_ROOT ?? root, '.github', 'workflows');

/** The gate every heavy job must carry — one spelling, no synonyms, no quotes. */
const GATE = "github.event_name == 'workflow_dispatch' || github.ref == 'refs/heads/main'";

/**
 * The provenance sentence the builder writes into the generated public CI
 * (`scripts/public-snapshot.mjs`, `PUBLIC_VALIDATE_WORKFLOW`). A public repository's standard
 * runners are free, so that workflow is deliberately untiered; this lint keys on the sentence to
 * check it against the public shape instead of the private one. A file that loses the sentence
 * falls back to the private rules — loudly, which is the failure mode we want.
 */
const GENERATED = "generated by the private repository's snapshot builder";

/** The private jobs that may not run on a pull request or a push to `staging` (issue #92). */
const HEAVY = [
  'validate',
  'rust',
  'wasm',
  'cockpit',
  'cockpit-native',
  'cockpit-native-platforms',
  'deployment-serving',
  'deployment'
];

/** The one job a pull request and a push to `staging` may start. */
const QUICK = ['quick'];

/** The workflow the classification rules apply to. */
const PR_WORKFLOW = 'validate.yml';

/** The quick tier's four legs (issue #92), matched against the comment-stripped job body. */
const QUICK_LEGS = [
  'cargo fmt --check',
  'npm run static',
  'npm test',
  'scripts/public-snapshot.mjs',
  'scripts/public-snapshot-check.mjs'
];

/**
 * The snapshot tooling is deliberately excluded from the public tree (the builder's own family is
 * not in the allowlist), so these two names are not carried there — this file only reaches them
 * when the tree is the private one: `reads()` returns before the legs are matched for the
 * generated public CI, and the release-workflow checks (which spawn the builder) return with it.
 * The declarations tell the snapshot builder (issue #104) why the references are deliberately
 * not carried:
 *
 *   public-snapshot:not-published scripts/public-snapshot.mjs — snapshot tooling, excluded from the public tree; only spawned when this tree is the private repository.
 *   public-snapshot:not-published scripts/public-snapshot-check.mjs — snapshot tooling, excluded; only matched against the private repository's quick tier.
 *   public-snapshot:not-published evidence — the lane archive is excluded from the public tree; the committed-evidence scan (issues #94, #25) reads it only when this tree is the private repository.
 *   public-snapshot:not-published .lane-report.md — the root lane report is private-repo-only; the scan reads it only when this tree is the private repository.
 */

const MAC_WIN = /macos|windows/i;

/**
 * The escape hatches the job a pull request may start must never carry (fix contract 2): a leg
 * that cannot fail the job is not a gate. Each entry names the shape and is matched against the
 * comment-stripped `run:` text of one step; `continue-on-error` and echo-only steps are
 * structural and checked separately below.
 */
const ESCAPES = [
  { shape: 'a `|| true` mask', re: /\|\|\s*['"]?true['"]?\b/ },
  { shape: 'a `|| :` mask', re: /\|\|\s*['"]?:['"]?(?=\s|$|;|#)/ },
  { shape: '`set +e`', re: /\bset\s+['"]?\+e['"]?(?=\s|$|;|#)/ }
];

const indentOf = (line) => line.length - line.trimStart().length;
const isBlankOrComment = (line) => line.trim() === '' || line.trimStart().startsWith('#');
const stripComments = (text) => text.split('\n').filter((line) => !line.trimStart().startsWith('#')).join('\n');

/**
 * Reads one step property. A `run:` block is the deeper-indented lines below it, kept verbatim
 * (the escape-hatch scan matches their text); an inline `run:` is the rest of the line.
 */
function readStepProperty(step, key, raw, lines, i) {
  const value = raw.trim();
  if (key === 'name') step.name = value;
  if (key === 'continue-on-error') step.continueOnError = { line: i + 1, value };
  // A step-level `if:` (issue #94): captured here, refused in `assertNoEscapeHatch` below.
  if (key === 'if') step.ifExpr = { line: i + 1, value };
  if (key !== 'run') return;
  step.runLine = i + 1;
  if (!/^[|>][-+]?$/.test(value)) {
    step.run = value;
    return;
  }
  const block = [];
  for (let j = i + 1; j < lines.length; j += 1) {
    if (lines[j].trim() === '') {
      block.push(lines[j]);
      continue;
    }
    if (indentOf(lines[j]) <= 8) break;
    block.push(lines[j]);
  }
  step.run = block.join('\n').trim();
}

/**
 * The two blocks this lint reads: `on:` (the triggers) and `jobs:` (one entry per job, with
 * `runs-on:`, an optional `if:`, any `os:` matrix entries, and its steps — each with its line,
 * `run:` text and `continue-on-error`). Unknown shapes throw. The file is read from `workflowDir`
 * unless `dir` names another directory (the emitted-release checks below pass the builder's own
 * output).
 */
function parseWorkflow(file, dir = workflowDir) {
  const text = readFileSync(join(dir, file), 'utf8');
  const lines = text.split('\n');
  const at = (re) => lines.findIndex((line) => re.test(line));

  const onAt = at(/^on:\s*$/);
  assert.notEqual(onAt, -1, `${file}: no top-level \`on:\` block`);
  const triggers = { pull_request: false, workflow_dispatch: false, pushBranches: [] };
  for (let i = onAt + 1; i < lines.length; i += 1) {
    const line = lines[i];
    if (isBlankOrComment(line)) continue;
    if (indentOf(line) === 0) break;
    if (indentOf(line) !== 2) continue;
    const key = line.trim().replace(/:.*$/, '');
    if (key === 'pull_request') triggers.pull_request = true;
    if (key === 'workflow_dispatch') triggers.workflow_dispatch = true;
    if (key === 'push') {
      for (let j = i + 1; j < lines.length && indentOf(lines[j]) > 2; j += 1) {
        const branchLine = lines[j].trim();
        if (!branchLine.startsWith('branches:')) continue;
        const inline = branchLine.match(/^branches:\s*\[(.*)\]\s*$/);
        assert.ok(inline, `${file}:${j + 1}: push.branches is not an inline list this lint can read`);
        triggers.pushBranches = inline[1].split(',').map((name) => name.trim()).filter(Boolean);
      }
    }
  }

  const jobsAt = at(/^jobs:\s*$/);
  assert.notEqual(jobsAt, -1, `${file}: no top-level \`jobs:\` block`);
  const jobs = new Map();
  let current = null;
  let body = [];
  const commit = () => jobs.set(current.name, { ...current, body: body.join('\n') });
  for (let i = jobsAt + 1; i < lines.length; i += 1) {
    const line = lines[i];
    if (isBlankOrComment(line)) {
      if (current) body.push(line);
      continue;
    }
    if (indentOf(line) === 0) break;
    if (indentOf(line) === 2) {
      const key = line.match(/^  ([A-Za-z0-9_-]+):\s*$/);
      assert.ok(key, `${file}:${i + 1}: unrecognised line at job level: ${line}`);
      if (current) commit();
      current = { name: key[1], line: i + 1, runsOn: null, ifExpr: null, continueOnError: null, matrixOs: [], steps: [] };
      body = [];
      continue;
    }
    if (!current) continue;
    body.push(line);
    const property = indentOf(line) === 4 ? line.match(/^    ([A-Za-z_-]+):\s*(.*)$/) : null;
    if (property?.[1] === 'runs-on') current.runsOn = property[2].trim();
    if (property?.[1] === 'if') current.ifExpr = property[2].trim();
    if (property?.[1] === 'continue-on-error') current.continueOnError = { line: i + 1, value: property[2].trim() };
    const os = line.match(/^\s+os:\s*\[(.*)\]\s*$/);
    if (os) current.matrixOs.push(...os[1].split(',').map((name) => name.trim()).filter(Boolean));
    const dash = line.match(/^ {6}- (.*)$/);
    if (dash) {
      const step = { line: i + 1, name: null, run: null, runLine: null, continueOnError: null, ifExpr: null };
      current.steps.push(step);
      const inline = dash[1].match(/^([A-Za-z_-]+):\s*(.*)$/);
      if (inline) readStepProperty(step, inline[1], inline[2], lines, i);
      continue;
    }
    const step = current.steps[current.steps.length - 1];
    if (step && indentOf(line) === 8) {
      const own = line.match(/^ {8}([A-Za-z_-]+):\s*(.*)$/);
      if (own) readStepProperty(step, own[1], own[2], lines, i);
    }
  }
  if (current) commit();

  assert.ok(jobs.size > 0, `${file}: \`jobs:\` carries no jobs`);
  for (const [name, job] of jobs) assert.ok(job.runsOn, `${file}: job \`${name}\` has no \`runs-on:\``);
  return { triggers, jobs, generated: text.includes(GENERATED) };
}

const reads = () => parseWorkflow(PR_WORKFLOW);
const reachesMacOrWindows = (job) => MAC_WIN.test(job.runsOn) || job.matrixOs.some((os) => MAC_WIN.test(os));

test('the workflow triggers match its shape', () => {
  const { triggers, generated } = reads();
  assert.equal(triggers.pull_request, true, 'pull_request is gone: pull requests would start nothing');
  if (generated) {
    // The generated public CI: public runners are free, so its whole suite runs on a PR and on a
    // push to its own `main`. It serves no lane branch, so it must not grow a dispatch trigger.
    assert.deepEqual(
      triggers.pushBranches.sort(),
      ['main'],
      `the generated public CI pushes to [${triggers.pushBranches.join(', ')}], expected main`
    );
    assert.equal(triggers.workflow_dispatch, false, 'the generated public CI grew a dispatch trigger');
    return;
  }
  assert.equal(
    triggers.workflow_dispatch,
    true,
    'workflow_dispatch is gone: a lane branch could no longer be proven on the hosted runner'
  );
  assert.deepEqual(
    [...triggers.pushBranches].sort(),
    ['main', 'staging'],
    `the private workflow pushes to [${triggers.pushBranches.join(', ')}], expected main and staging`
  );

  /* ------------------------------------------------------------------ *
   * The generated public release workflow (issue #102). The checks run
   * inside this test rather than as a new top-level one on purpose: the
   * suite's total test count is drift-pinned to VALIDATION_RESULTS.md,
   * and that figure must not move while two lanes share the tree.
   * ------------------------------------------------------------------ */
  {
    const scratch = mkdtempSync(join(tmpdir(), 'drafthouse-release-workflow-'));
    // The builder writes only into a directory that does not exist yet; `mkdtempSync` made one,
    // so the tree is one level below it.
    const out = join(scratch, 'snapshot');
    try {
      const env = { ...process.env };
      // The builder is a plain script, not a test-runner child; make sure it cannot be taken
      // for one when this suite itself runs under `node --test`.
      delete env.NODE_TEST_CONTEXT;
      const built = spawnSync(
        process.execPath,
        [join(root, 'scripts', 'public-snapshot.mjs'), '--out', out, '--ref', 'HEAD'],
        { cwd: root, encoding: 'utf8', env }
      );
      assert.equal(
        built.status,
        0,
        `the snapshot builder exited ${built.status} while emitting the release workflow:\n${built.stderr.slice(-2000)}`
      );

      // Issue #104: the builder refuses when a published test references a path the written tree
      // does not carry (`built.status` above would be non-zero, naming both). These assertions
      // pin that the scan actually read the tests and that this tree's declarations are in use —
      // a scan that silently stopped seeing the tests would defeat the point of the gate.
      const summary = JSON.parse(built.stdout);
      assert.ok(summary.publishedTests, 'the builder summary carries no publishedTests section (issue #104)');
      assert.ok(
        summary.publishedTests.files >= 6,
        `the published-tests scan read ${summary.publishedTests.files} file(s) under tests/, fewer than the suite's six test files`
      );
      assert.ok(
        summary.publishedTests.references > 0,
        'the published-tests scan found no reference at all — it is no longer reading the tests'
      );
      assert.ok(
        summary.publishedTests.declared.length > 0,
        'no published test declares a not-published reference — the declaration mechanism is unused, so the scan sees less than it should'
      );

      const emitted = join(out, '.github', 'workflows', 'release.yml');
      assert.ok(existsSync(emitted), 'the builder emitted no .github/workflows/release.yml');
      const text = readFileSync(emitted, 'utf8');

      // The recognition sentence: with it, this lint reads the file as the generated public CI
      // (untiered, no private gate); without it, as a private workflow — loudly, which is the
      // failure mode we want.
      assert.ok(
        text.includes(GENERATED),
        `the emitted release workflow lost the provenance sentence ${JSON.stringify(GENERATED)}`
      );

      // The trigger: a `v*` tag push plus a dispatch dry run — and the file must parse with this
      // lint's own parser (it throws on a shape it cannot read).
      const parsed = parseWorkflow('release.yml', join(out, '.github', 'workflows'));
      assert.equal(parsed.generated, true, 'the emitted release workflow is not recognised as generated');
      assert.ok(text.includes("tags: ['v*']"), "the emitted release workflow has no `v*` tag trigger");
      assert.equal(parsed.triggers.workflow_dispatch, true, 'the emitted release workflow lost its dry-run dispatch');
      assert.equal(parsed.triggers.pull_request, false, 'the emitted release workflow gained a pull_request trigger');
      assert.deepEqual([...parsed.jobs.keys()].sort(), ['release', 'verify'], 'the emitted release workflow lost a job');

      // Public runners only, and no repository secret: the default GITHUB_TOKEN is the one
      // credential a public repository may use.
      assert.ok(!/macos|windows/i.test(text), 'the emitted release workflow reaches a macOS or Windows runner');
      const runsOn = [...text.matchAll(/^    runs-on: (.+)$/gm)].map((match) => match[1].trim());
      assert.ok(runsOn.length > 0, 'the emitted release workflow has no runs-on entries');
      assert.ok(
        runsOn.every((value) => value === 'ubuntu-latest'),
        `the emitted release workflow runs on [${runsOn.join(', ')}], expected ubuntu-latest only`
      );
      assert.ok(!text.includes('secrets.'), 'the emitted release workflow references a repository secret');
      assert.ok(text.includes('github.token'), 'the emitted release workflow does not use the default token');

      // The gzip size gate, the plane's three parts, and the release it publishes.
      assert.ok(
        text.includes('cockpit/tools/build-web.sh release'),
        'the emitted release workflow no longer builds the plane through its gated build script'
      );
      for (const part of [
        'cockpit/index.html',
        "cockpit/pkg/' + address + '/drafthouse_cockpit.js",
        "cockpit/pkg/' + address + '/drafthouse_cockpit_bg.wasm",
        "cockpit/assets/' + address + '/'"
      ]) {
        assert.ok(text.includes(part), `the emitted release workflow does not assert the plane's ${part}`);
      }
      assert.ok(
        text.includes('manifest.address') && text.includes('const address = manifest.address'),
        'the emitted release workflow no longer reads the plane\'s content address (#148)'
      );
      for (const published of ['gh release create', '--verify-tag', 'dist/cockpit/*', 'github.sha']) {
        assert.ok(text.includes(published), `the emitted release workflow does not publish ${published}`);
      }
      assert.ok(
        text.includes('gh release download') && text.includes('--verify dist/downloaded'),
        'the emitted release workflow lost the fresh-download verification of its own release'
      );
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  /*
   * Issue #94, part 2 — the lane archive's committed logs, extended by issue #25 to its markdown
   * reports and the root lane report. Embedded in this case rather than added as a top-level one
   * for the same reason the release-workflow checks above are: the suite's total test count is
   * drift-pinned to VALIDATION_RESULTS.md, and that figure must not move while two lanes share
   * the tree. The public tree carries no `evidence` directory and no lane report, so the scan
   * has nothing to read there (the declarations above keep the references legal).
   */
  assertCommittedEvidenceHasNoLocalPaths();
  // Issue #25, the review's SHOULD-FIX: the scan's own bite witness, in-suite. The archive call
  // above can no longer testify to a narrowed pattern list or file set (its historical carriers
  // are sealed, so a narrowed scan still agrees with them), and the external `red-25-*` probes
  // are not scheduled by anything — see `assertEvidenceScanBites`.
  assertEvidenceScanBites();
});

test('the quick tier exists exactly when this is the private workflow', () => {
  const { jobs, generated } = reads();
  assert.equal(
    jobs.has('quick'),
    !generated,
    generated
      ? 'the generated public CI grew a `quick` job: its suite is not tiered'
      : 'the private workflow lost its `quick` tier (issue #92): a PR would start the heavy jobs'
  );
  if (generated) return;
  assert.deepEqual(
    [...jobs.keys()].sort(),
    [...QUICK, ...HEAVY].sort(),
    'a job was added or renamed in the private workflow: classify it in this lint as quick or heavy '
      + '(issue #92 — anything unclassified may be burning billed minutes on pull requests)'
  );
  for (const name of QUICK) {
    assert.equal(jobs.get(name).ifExpr, null, `the quick job \`${name}\` is gated; a PR would start nothing`);
  }
});

test('the private gate is on every heavy job — and nowhere in the generated public CI', () => {
  const { jobs, generated } = reads();
  if (generated) {
    for (const [name, job] of jobs) {
      assert.notEqual(
        job.ifExpr,
        GATE,
        `the generated public CI's \`${name}\` carries the private dispatch/main gate: the public `
          + 'suite is not tiered and must not skip a job on a pull request'
      );
    }
    return;
  }
  for (const name of HEAVY) {
    assert.equal(
      jobs.get(name).ifExpr,
      GATE,
      `heavy job \`${name}\` does not carry the exact gate:\n  if: ${GATE}`
    );
  }
});

test('no private workflow runs a macOS/Windows or heavy job on a pull_request or a staging push', () => {
  const files = workflowFiles();
  assert.ok(files.length > 0, `${workflowDir} carries no workflow files`);
  for (const file of files) {
    const { triggers, jobs, generated } = parseWorkflow(file);
    if (generated) continue; // the public CI: free runners, deliberately untiered
    const reachable = triggers.pull_request || triggers.pushBranches.includes('staging');
    if (!reachable) continue;
    for (const [name, job] of jobs) {
      const mustBeGated = HEAVY.includes(name) || reachesMacOrWindows(job);
      if (!mustBeGated) continue;
      assert.equal(
        job.ifExpr,
        GATE,
        `${file}: \`${name}\` can run on a pull request or a push to staging (runner `
          + `\`${job.runsOn}\`${job.matrixOs.length ? `, matrix ${job.matrixOs.join(', ')}` : ''}) — `
          + `it needs:\n  if: ${GATE}`
      );
    }
  }
});

/**
 * Refuses every escape hatch (fix contract 2, extended by issue #94) in the job a pull request
 * may start: a step-level `if:` first — any value, because neither the private `quick` tier nor
 * the generated public CI's `validate` job carries one, so no shape is allowlisted — then the
 * masked-run shapes, `continue-on-error` and echo-only steps. Each failure names the file, the
 * line, the step and the shape, so a masked leg cannot slip through.
 */
function assertNoEscapeHatch(file, job) {
  const stepName = (step) => `step ${JSON.stringify(step.name ?? '(unnamed)')}`;
  assert.equal(
    job.continueOnError,
    null,
    `${file}:${job.continueOnError?.line ?? job.line}: the job \`${job.name}\` carries `
      + `\`continue-on-error: ${job.continueOnError?.value ?? ''}\` — a job a pull request may start must be able to fail`
  );
  for (const step of job.steps) {
    if (step.ifExpr) {
      assert.fail(
        `${file}:${step.ifExpr.line}: ${stepName(step)} carries a step-level \`if: ${step.ifExpr.value}\` — `
          + 'a step a pull request may start must not be conditionally skipped (issue #94)'
      );
    }
    if (step.continueOnError) {
      assert.fail(
        `${file}:${step.continueOnError.line}: ${stepName(step)} carries `
          + `\`continue-on-error: ${step.continueOnError.value}\` — a step a pull request may start must be able to fail the job`
      );
    }
    const run = stripComments(step.run ?? '');
    for (const { shape, re } of ESCAPES) {
      const hit = run.match(re);
      assert.ok(
        !hit,
        `${file}:${step.runLine ?? step.line}: ${stepName(step)} uses ${shape} (\`${hit?.[0] ?? ''}\`) — `
          + 'a leg that swallows its own failure is not a gate'
      );
    }
    const commands = run.split('\n').map((line) => line.trim()).filter(Boolean);
    assert.ok(
      commands.length === 0 || !commands.every((command) => command.startsWith('echo')),
      `${file}:${step.runLine ?? step.line}: ${stepName(step)} is an echo-only step (\`${commands[0] ?? ''}\`) — `
        + 'a step that cannot fail is not a gate'
    );
  }
}

/**
 * The trigger-scoped half of the step-level `if:` refusal (issue #123): **every workflow a pull
 * request can start** — any file whose own `on:` block carries `pull_request` — must carry no
 * step-level `if:` in any job, because a step on such a surface must not be conditionally skipped.
 * `assertNoEscapeHatch` holds the eager surfaces to the same contract by job name; this scan
 * scopes it by the trigger, re-read from the file on every run, so no exemption list can rot.
 *
 * That is exactly why `.github/workflows/release.yml`'s step-level `if:` sites are legal today:
 * its trigger is a `v*` tag push plus `workflow_dispatch` (the boundary note in the header), so
 * it is not scanned — and the moment it (or any other workflow) gains `pull_request`, its sites
 * are refused, named with their file, line, step and value.
 */
function assertNoStepLevelIfOnPullRequestWorkflows() {
  const scanned = [];
  const refusals = [];
  for (const file of workflowFiles()) {
    const { triggers, jobs } = parseWorkflow(file);
    if (!triggers.pull_request) continue;
    scanned.push(file);
    for (const [name, job] of jobs) {
      for (const step of job.steps) {
        if (!step.ifExpr) continue;
        refusals.push(
          `${file}:${step.ifExpr.line}: job \`${name}\`, step ${JSON.stringify(step.name ?? '(unnamed)')} `
            + `carries a step-level \`if: ${step.ifExpr.value}\``
        );
      }
    }
  }
  assert.deepEqual(
    refusals,
    [],
    'a workflow a pull request can start carries a step-level `if:` — a step on such a surface '
      + "must not be conditionally skipped (issue #94's contract, scoped by the trigger since "
      + `issue #123):\n  ${refusals.join('\n  ')}`
  );
  // Refuse a vacuous pass: if no workflow a pull request can start was scanned at all, this check
  // would agree with every tree.
  assert.ok(
    scanned.length > 0,
    'no workflow a pull request can start was scanned — the step-level `if:` boundary check would pass vacuously'
  );
}

/**
 * The committed-evidence scan's file set (issue #94, part 2; extended by issue #25): run logs
 * and stderr captures, and — because the redaction class recurred in the lane reports' own
 * headers, where no writer runs — the archive's markdown reports and the root lane report. The
 * report half is therefore IN the gate's reach; its pre-existing population is sealed exactly
 * like the pre-existing logs (a byte-identical file is the only exemption), so the gate stays
 * green on history and bites on anything new.
 */
const LOG_FILE = /\.log(\.gz)?$|\.err$/;
const REPORT_FILE = /\.md$/;

/**
 * The shapes a committed log or report must not carry (issue #94, part 2; extended by issue
 * #25). The list is measured over the committed tree at the #25 head, not guessed — a future
 * extension repeats that measurement (scan this scan's file set for host-path-looking strings,
 * then decide) rather than widening on a hunch. The scan re-derives every run, so these
 * figures are the record of why the list is shaped this way, not a pin:
 *
 *   the machine's home path       414 logs,  22 reports   (the original #94 shape)
 *   a lane-worktree path          357 logs,  27 reports   (the original #94 shape)
 *   a macOS per-user temp path     75 logs,   3 reports   (added by #25 — the #114 finding)
 *
 * Deliberately NOT covered, each with the reason it is not this class: generic temp scratch
 * paths under the system `/tmp` root, including the `/private/tmp` mirror (100 logs, 14
 * reports — a standard location every Unix host carries, naming no account and no lane); the
 * hosted CI runner's home root (14 logs — that machine is GitHub's, not this one); standard
 * tool prefixes such as the Homebrew one (1 report); and a bare tool-directory token with no
 * path under it (1 log, a quoted string — not a path; the lane shape this scan covers is the
 * directory followed by its worktrees root, which IS matched).
 *
 * A writer must redact a home, lane-worktree or per-user temp prefix before it commits
 * evidence, and a lane report must write its worktree line without an absolute path. Spelled
 * so this file's own published copy does not trip the snapshot leak gate's machine-path check:
 * a slash pair the checker reads as a machine path is written with a backslash between its
 * parts or as a character class, which the checker's own senses do not read as the literal.
 */
const LOCAL_PATH_PATTERNS = [
  { shape: 'the machine\'s home path', re: /\/Users\// },
  { shape: 'a lane-worktree path', re: /[.]herdr\/worktrees\// },
  { shape: 'a macOS per-user temp path', re: /\/var\/folders\// }
];

/**
 * Scans a tree for committed evidence logs or reports that carry this machine's local paths
 * (issue #94, part 2; the report half is issue #25), returning one line per finding. The archive
 * call below refuses a non-empty result; the bite witness (`assertEvidenceScanBites`) scans a
 * planted scratch tree through this same function — so a narrowed pattern list or file set reds
 * the suite itself, not only the external probes.
 *
 * "Committed" is enforced, not assumed: the walk exempts every path git reports as ignored and
 * untracked (`gitIgnoredEvidencePaths`) — an earlier round's probes left uncompressed `.log`
 * files that `.gitignore`'s `*.log` excludes and no commit can carry, so the gate was red on a
 * checkout whose committed archive was clean (a fresh clone — what CI and the reviewer see —
 * has none: the defect this rule fixes). The same rule is why the exemption is not an escape
 * hatch: `git add`ing an offending file (or committing it with `-f`) makes git report it
 * tracked, it drops out of the exemption set, and the scan judges it again. In a tree that is
 * not a git repository — the bite witness's scratch fixture — the exemption set is empty by
 * design, so every planted file is judged; the witness's exact-findings assertion is what still
 * reds the suite for a narrowed pattern list or a reverted file set.
 *
 * The lane archive is a record of runs at their own commits, so the files committed before the
 * writers redacted are **sealed**: the issue #94 seal list in `evidence` records each offender's
 * sha256, and only a byte-identical file is exempt. Everything else — a log or report added
 * since, or a sealed file whose bytes changed — must be clean, because the writers redact before
 * they write and the report headers are written without an absolute path. The report half exists
 * because the class recurred twice in `Worktree:`-style headers (#107/#100, #117) with no writer
 * anywhere in the path: those files are scanned here, and their pre-existing population is
 * sealed the same way. The public tree carries no `evidence` directory and no lane report, so
 * absence is not a failure.
 */
function scanEvidenceLocalPaths(scanRoot) {
  const evidenceDir = join(scanRoot, 'evidence');
  const laneReport = join(scanRoot, '.lane-report.md');
  if (!existsSync(evidenceDir) && !existsSync(laneReport)) return [];
  const sealFile = join(evidenceDir, 'issue-94', 'legacy-log-seals.txt');
  const seals = new Map();
  if (existsSync(sealFile)) {
    for (const line of readFileSync(sealFile, 'utf8').split('\n')) {
      const entry = line.trim();
      if (entry === '' || entry.startsWith('#')) continue;
      const seal = entry.match(/^([0-9a-f]{64})\s+(\S.*)$/);
      assert.ok(seal, `${sealFile} carries a line this scan cannot read: ${entry}`);
      seals.set(seal[2], seal[1]);
    }
  }
  const ignored = gitIgnoredEvidencePaths(scanRoot);
  const failures = [];
  const scanFile = (path) => {
    const relativePath = relative(scanRoot, path).split(sep).join('/');
    const bytes = readFileSync(path);
    let text;
    try {
      text = path.endsWith('.gz') ? gunzipSync(bytes).toString('utf8') : bytes.toString('utf8');
    } catch (error) {
      failures.push(
        `${relativePath}: cannot be read as a gzip stream (${error.message}) — a log that cannot be read cannot be proven clean`
      );
      return;
    }
    for (const { shape, re } of LOCAL_PATH_PATTERNS) {
      const hit = text.match(re);
      if (!hit) continue;
      const digest = createHash('sha256').update(bytes).digest('hex');
      if (seals.get(relativePath) === digest) continue; // bytes-identical to its seal: the recorded legacy file
      const line = text.slice(0, hit.index).split('\n').length;
      const sample = hit[0].length > 80 ? `${hit[0].slice(0, 77)}…` : hit[0];
      failures.push(
        `${relativePath}:${line}: carries ${shape} (\`${sample}\`) — a committed log or report must not carry this machine's local paths; `
          + 'the writers redact before they write, and a pre-existing file is exempt only through its seal (issues #94, #25)'
      );
    }
  };
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        walk(path);
        continue;
      }
      if (!entry.isFile() || (!LOG_FILE.test(entry.name) && !REPORT_FILE.test(entry.name))) continue;
      if (ignored.has(relative(scanRoot, path).split(sep).join('/'))) continue;
      scanFile(path);
    }
  };
  if (existsSync(evidenceDir)) walk(evidenceDir);
  if (existsSync(laneReport) && !ignored.has('.lane-report.md')) scanFile(laneReport);
  return failures;
}

/**
 * The paths under `scanRoot` that git ignores and does not track — files no plain commit can
 * carry, so judging them would be a red gate on a checkout whose *committed* archive is clean.
 * They exist because an evidence probe writes its raw log before the lane compresses the
 * committed `.log.gz`, and `.gitignore` excludes the raw `.log`; a fresh clone (what CI checks
 * out) carries none, which is why only a developer's checkout saw the false red.
 *
 * Returns scan-root-relative POSIX paths. EMPTY whenever git cannot answer for the tree — the
 * bite witness's `mkdtemp` scratch fixture is not a repository, and judging everything there is
 * the only safe direction: a context that cannot say what a commit would carry must not exempt
 * anything silently. The set is derived per run and only for untracked paths git reports as
 * ignored: `git add`ing one of them (or committing it with `-f`) makes it tracked, git then
 * reports it not ignored, and the scan judges it again — the exemption is not an escape hatch.
 */
function gitIgnoredEvidencePaths(scanRoot) {
  const ignored = new Set();
  const result = spawnSync(
    'git',
    ['ls-files', '-z', '--others', '--ignored', '--exclude-standard', '--', 'evidence', '.lane-report.md'],
    { cwd: scanRoot, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }
  );
  if (result.status !== 0) return ignored;
  for (const path of (result.stdout ?? '').split('\0')) {
    if (path !== '') ignored.add(path);
  }
  return ignored;
}

/**
 * Refuses a committed evidence log or report that carries this machine's local paths (issues
 * #94 and #25) — the archive call of `scanEvidenceLocalPaths`. Its in-suite bite witness is
 * `assertEvidenceScanBites`.
 */
function assertCommittedEvidenceHasNoLocalPaths() {
  const failures = scanEvidenceLocalPaths(root);
  assert.deepEqual(
    failures,
    [],
    `committed evidence logs and reports carry this machine's local paths (issues #94 and #25):\n  ${failures.join('\n  ')}`
  );
}

/**
 * The scan's in-suite bite witness (issue #25, the review's SHOULD-FIX): plants each covered
 * shape into a scratch fixture — the per-user temp shape into a log, the `Worktree:` header
 * shapes into an archive-style report, and the lane-worktree shape into the root lane report —
 * then asserts the scan returns exactly the planted findings. A narrowed pattern list or file
 * set leaves the fixture short, so this suite reds on its own; nothing outside it is scheduled.
 * The fixture is a bare `mkdtemp` tree and never a git repository — load-bearing, now that the
 * scan exempts what git ignores: the exemption set is empty there by design
 * (`gitIgnoredEvidencePaths`), so the plants are judged exactly like a repository's committable
 * files would be, and a scan that found nothing cannot pass here: the exact-count assertion
 * below fires and names the shortfall. The archive's own files cannot testify to this: their
 * carriers are sealed, and a narrowed scan still agrees with a sealed file (it finds nothing
 * left to refuse). Non-vacuous by construction: the fixture is read back after writing, and a
 * scan that finds less, more or nothing than the plant fails here, loudly.
 *
 * The fixture text is assembled from parts at runtime, because this file is published: a literal
 * home, worktrees-root or per-user temp path in its own source would red the snapshot leak gate
 * — the same reason the scan's patterns above are written with escaped slashes.
 */
function assertEvidenceScanBites() {
  const slash = '/';
  const dot = '.';
  const plants = [
    {
      path: join('evidence', 'plant-25', 'log-plant.log'),
      lines: [
        'a committed run log, otherwise clean',
        `the budget read ${slash}var${slash}folders${slash}ab12${slash}T${slash}budget.json before the writers redacted`
      ],
      expect: [{ line: 2, shape: 'a macOS per-user temp path' }]
    },
    {
      path: join('evidence', 'plant-25', 'report-plant.md'),
      lines: [
        '# A lane report',
        '',
        `**Worktree:** ${slash}Users${slash}plant${slash}${dot}herdr${slash}worktrees${slash}drafthouse${slash}impl-plant`
      ],
      expect: [
        { line: 3, shape: "the machine's home path" },
        { line: 3, shape: 'a lane-worktree path' }
      ]
    },
    {
      path: '.lane-report.md',
      lines: [`**Worktree:** ${slash}${dot}herdr${slash}worktrees${slash}drafthouse${slash}impl-plant`],
      expect: [{ line: 1, shape: 'a lane-worktree path' }]
    }
  ];
  const fixture = mkdtempSync(join(tmpdir(), 'drafthouse-evidence-scan-'));
  try {
    for (const plant of plants) {
      const path = join(fixture, plant.path);
      const text = `${plant.lines.join('\n')}\n`;
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, text);
      assert.equal(
        readFileSync(path, 'utf8'),
        text,
        `the scratch fixture ${plant.path.split(sep).join('/')} could not be planted — the bite witness must fail, not pass vacuously`
      );
    }
    const findings = scanEvidenceLocalPaths(fixture);
    const expected = plants.flatMap((plant) => plant.expect.map((one) => ({ file: plant.path.split(sep).join('/'), ...one })));
    for (const { file, line, shape } of expected) {
      assert.ok(
        findings.some((found) => found.startsWith(`${file}:${line}:`) && found.includes(`carries ${shape}`)),
        'the committed-evidence scan no longer names '
          + `${file}:${line} carrying ${shape} in the planted fixture — a covered shape and file kind the scan must red on:\n`
          + `  ${findings.join('\n  ') || '(no findings)'}`
      );
    }
    assert.equal(
      findings.length,
      expected.length,
      `the scan found ${findings.length} finding(s) in the planted fixture, expected ${expected.length} — `
        + 'a narrowed pattern list or file set must red here, not come back short:\n  '
        + `${findings.join('\n  ') || '(no findings)'}`
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
}

test('the job a pull request may start carries its legs and no escape hatch', () => {
  // Issue #123: the same refusal, scoped by each workflow's own trigger — release.yml is out of
  // reach only because it carries no `pull_request`, and this scan re-reads that on every run.
  assertNoStepLevelIfOnPullRequestWorkflows();
  const { jobs, generated } = reads();
  if (generated) {
    const full = stripComments(jobs.get('validate').body);
    for (const leg of ['npm run validate', 'build-web.sh release']) {
      assert.ok(full.includes(leg), `the generated public CI no longer runs \`${leg}\``);
    }
    assertNoEscapeHatch(PR_WORKFLOW, jobs.get('validate'));
    return;
  }
  const quick = jobs.get('quick');
  const body = stripComments(quick.body);
  for (const leg of QUICK_LEGS) {
    assert.ok(body.includes(leg), `the quick tier no longer runs \`${leg}\` (issue #92)`);
  }
  assertNoEscapeHatch(PR_WORKFLOW, quick);
});

/** The workflow files, sorted, so the reachability test is deterministic. */
const workflowFiles = () => readdirSync(workflowDir)
  .filter((name) => name.endsWith('.yml') || name.endsWith('.yaml'))
  .sort();
