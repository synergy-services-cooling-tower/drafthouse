#!/usr/bin/env node
/**
 * clip-check.mjs - round 5's layout gate: **for every evidence frame, no text element in the right column
 * overflows its container.**
 *
 *   tools/clip-check.mjs                       # every captured frame in the capture report
 *   tools/clip-check.mjs --verbose             # list every entry, not only the offenders
 *   tools/clip-check.mjs --only duty,readout   # frames whose name contains one of these
 *   tools/clip-check.mjs --report path.json    # a different report
 *
 * ## Where the numbers come from
 *
 * The app measures itself: while it draws, every text unit of the right column (the duty & site panel's
 * rows, the three collapsible headers, the operating read-out, the fill stack) records
 *
 *   { id, text, rect, avail, need, lines, over, overLeft }
 *
 * - `rect`   the rect the unit was drawn in,
 * - `avail`  the width its row had to lay the text out in,
 * - `need`   the natural (unwrapped) width of that text, measured with the same font,
 * - `lines`  how many lines the unit drew in,
 * - `over`   how far the rect passed the container's right edge (the right column's own inner rect),
 * - `overLeft` the same on the left.
 *
 * The whole set is published to `#mirror-clip` in the HTML mirror every frame (JSON), and
 * `tools/capture.mjs` stores the payload it read for each frame in its capture report. This
 * tool asserts, per frame:
 *
 *   1. the probe was published at all, is parseable, and is not empty;
 *   2. `over` and `overLeft` are 0 for every entry - nothing is drawn past the column's own edges;
 *   3. a unit that needed more room than its row had must have wrapped (`lines >= 2`); one that stayed on
 *      one line must have fitted (`need <= avail`). This is the owner's rule: *a two-value row wraps to
 *      two lines before it clips - never clip.* Entries with `need == 0` are units whose text is truncated
 *      by design (a collapsed section's one-line summary), where rule 2 is the whole claim;
 *   4. the reported counts agree with the app's own `data-clip-entries` / `data-clip-overflows` markers.
 *
 * Exit code 0 only when every frame has zero overflows and zero clips.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
};
const REPORT = resolve(ROOT, arg('report', 'reports/capture-report.json'));
const ONLY = arg('only', null);
const VERBOSE = process.argv.includes('--verbose');
/** egui rounds rects onto the physical pixel grid; half a point is inside. */
const EPS = 0.5;

const report = JSON.parse(readFileSync(REPORT, 'utf8'));
const needles = ONLY ? ONLY.split(',') : null;
const rows = (report.results || []).filter(
  (r) => r.file && (!needles || needles.some((n) => r.frame.includes(n))),
);

if (!rows.length) {
  console.error(`no captured frames in ${REPORT}${ONLY ? ` matching --only ${ONLY}` : ''}`);
  process.exit(1);
}

let framesFailed = 0;
let entriesTotal = 0;
let overflowsTotal = 0;
let clipsTotal = 0;
const failures = [];

for (const r of rows) {
  const clip = r.clip;
  const problems = [];
  // The seams and curves views have no right column at all: an empty probe is the truth there, not a gap.
  // Issue #58: the engine-unavailable state is the same truth - the asset failed to load, so there is
  // no column to measure (`state.source` is the load's own marker: "not loaded").
  const noColumn =
    Boolean(r.state?.view && r.state.view !== 'cockpit') || r.state?.source === 'not loaded';
  if (!clip) {
    problems.push('the frame carries no clip probe (the app did not publish #mirror-clip)');
  } else if (clip.error || clip.parse_error) {
    problems.push(`the clip probe did not parse: ${clip.error || clip.parse_error}`);
  } else if (!Array.isArray(clip.entries)) {
    problems.push('the clip probe has no entries array');
  } else if (!clip.entries.length && !noColumn) {
    problems.push('the clip probe is empty - a cockpit frame draws a right column and must measure it');
  }

  const entries = Array.isArray(clip?.entries) ? clip.entries : [];
  const overflows = entries.filter((e) => Number(e.over) > EPS || Number(e.overLeft) > EPS);
  const clips = entries.filter((e) => Number(e.need) > 0 && Number(e.lines) < 2 && Number(e.need) > Number(e.avail) + EPS);

  // The app's own markers must agree with the payload it published.
  const claimed = r.state?.clipEntries;
  if (claimed != null && Number(claimed) !== entries.length) {
    problems.push(`data-clip-entries=${claimed} but the payload has ${entries.length} entries`);
  }
  const claimedOver = r.state?.clipOverflows;
  if (claimedOver != null && Number(claimedOver) !== overflows.length) {
    problems.push(`data-clip-overflows=${claimedOver} but the payload has ${overflows.length} overflows`);
  }

  entriesTotal += entries.length;
  overflowsTotal += overflows.length;
  clipsTotal += clips.length;
  for (const e of overflows) {
    problems.push(`overflow ${Number(e.over) > EPS ? e.over : e.overLeft}px: "${e.id}" → ${e.text} (rect ${e.rect.join(',')} vs column ${clip.column.join(',')})`);
  }
  for (const e of clips) {
    problems.push(`clipped on one line: "${e.id}" needs ${e.need}px in ${e.avail}px and drew ${e.lines} line(s) - it had to wrap`);
  }

  const ok = problems.length === 0;
  if (!ok) {
    framesFailed++;
    failures.push({ frame: r.frame, viewport: r.viewport, problems });
  }
  const status = ok ? 'ok  ' : 'FAIL';
  console.log(
    `${status} ${String(r.frame).padEnd(22)} ${String(r.viewport).padEnd(9)} ` +
      `elements=${String(entries.length).padStart(3)} overflows=${overflows.length} clips=${clips.length} ` +
      `column=${clip?.column ? `[${clip.column.map((v) => Math.round(v)).join(',')}]` : 'n/a'}`,
  );
  if (VERBOSE && entries.length) {
    for (const e of entries) {
      console.log(`       ${e.id.padEnd(28)} rect=[${e.rect.map((v) => Math.round(v)).join(',')}] need=${Math.round(e.need)} avail=${Math.round(e.avail)} lines=${e.lines} over=${e.over}${e.overLeft ? ` overLeft=${e.overLeft}` : ''}`);
    }
  }
}

if (failures.length) {
  console.log('\n--- offending frames ---');
  for (const f of failures) {
    console.log(`${f.frame} @ ${f.viewport}`);
    for (const p of f.problems) console.log(`  · ${p}`);
  }
}

console.log(
  `\n${rows.length} frame(s) checked · ${entriesTotal} text element(s) · ` +
    `${overflowsTotal} overflow(s) · ${clipsTotal} one-line clip(s) · ${framesFailed} frame(s) failed`,
);
console.log(framesFailed || overflowsTotal || clipsTotal ? 'CLIP CHECK: FAILED' : 'CLIP CHECK: 0 overflows');
process.exit(framesFailed || overflowsTotal || clipsTotal ? 1 : 0);
