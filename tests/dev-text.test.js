/**
 * Issue #137: no developer text on screen.
 *
 * Developer text is any of: the words engine / Rust / WASM / Bevy / fixture / seam / adapter, a camelCase
 * field name, a snake_case or `Type::fn` identifier, a dotted field path, a file path, or an issue
 * reference (`#NN`). The rule lives in `tests/dev-text-rule.mjs` so the source scan, the
 * painted-text gate and this test judge the same thing.
 *
 * Two halves:
 *   1. the rule itself bites (each tell is caught, plain engineering wording is not);
 *   2. SOURCE: every string literal in the modules that paint text (the tabs, instrument, notes, header,
 *      hover cards, duty panel, custom-part form, Report and PDF sheet) is clean - logs, panics, test
 *      support and format placeholders excluded, because they never reach the screen;
 *   3. PAINTED: the committed after-captures (`design/137/after/*.json`, written by
 *      `design/137/tools/shots.mjs` from the running app) carry no developer text in what the frame
 *      painted or in what a screen reader reaches, and no number off the house format - and the
 *      committed before-captures of the same tabs FAIL the same check, so the test is known to bite.
 *
 * The public snapshot does not carry `design/**` (it is not in the allowlist), so there the two
 * capture tests report a named NOT VERIFIABLE diagnostic and assert nothing - a runner-level skip
 * would move the skipped count `tests/validation-results-drift.test.js` pins. Declarations for the
 * snapshot builder (issue #104):
 *
 *   public-snapshot:not-published design/137 — the #137 round (captures + tools) is excluded by the snapshot allowlist; the private tree runs both capture checks.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  violations,
  rustLiterals,
  stripPlaceholders,
  offScreen,
  PAINTING,
  TEST_MODULES,
} from './dev-text-rule.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

test('the rule catches every developer tell the issue names', () => {
  const caught = {
    'calculated by our engine': 'word:engine',
    'the Rust crate': 'word:Rust',
    'WASM build': 'word:WASM',
    'Bevy scene': 'word:Bevy',
    'fixture-driven preview': 'word:fixture',
    'data seam': 'word:seam',
    'the adapter copy': 'word:adapter',
    'hot water · hotWaterC': 'camel:hotWaterC',
    'drift_loss_kg_s · eliminator': 'ident:drift_loss_kg_s',
    'Engine::run at 0.92': 'ident:Engine::run',
    'as provenance.fixed records it': 'ident:provenance.fixed',
    'read from fixtures/engine-run.json': 'path',
    'see notes.rs': 'path',
    'per #91': 'issue:#91',
    'vendor records (synergy-apps#1808)': 'issue:s#1808',
  };
  for (const [text, tag] of Object.entries(caught)) {
    assert.ok(violations(text).includes(tag), `${JSON.stringify(text)} must be caught as ${tag}: ${violations(text)}`);
  }
});

test('the rule passes plain engineering wording and house units', () => {
  for (const ok of [
    'Range = T_hot − T_cold',
    'Comparable engineering metrics · not a certified CTI/MRL test',
    'cold water 31.7 °C · approach 3.7 K',
    'fan power 41.2 kW · 1,320 m³/h · 46.3 m³/s · 12.4 kg/s',
    'KaV/L 1.42 · L/G 1.18 · 450 Pa · 1.25 kPa',
    'Micro-fluted film · FILM-MF20',
    'save the project (.drafthouse)',
    'e.g. a second cell',
  ]) {
    // `T_hot` is a symbol in a formula, not an identifier: the snake rule wants a lower-case head
    const v = violations(ok, { format: true });
    assert.deepEqual(v, [], `${JSON.stringify(ok)} is plain wording, flagged: ${v}`);
  }
});

test('the format rule catches numbers off the house style', () => {
  const off = {
    '31.65 degC': 'unit:degC',
    '1320 m3/hr': 'unit:m3 (use m³)',
    '45 mmWG': 'unit:mmWG (use Pa or kPa)',
    'stack 3.10 m2': 'unit:m2 (use m²)',
    '355 rpm (speed ratio 0.780)': 'fmt:fan speed as % of rated',
    'max water 55 C': 'unit:bare C (use °C or K)',
    'cold water 31.65 °C': 'fmt:temperature to 0.1 °C',
    'fan 41.25 kW': 'fmt:power to 0.1 kW',
    'share 12.5 %': 'fmt:percentages whole',
    'flow 1,324.6 m³/h': 'fmt:flow 3 s.f. (1,324.6 m³/h)',
  };
  for (const [text, tag] of Object.entries(off)) {
    assert.ok(violations(text, { format: true }).includes(tag), `${JSON.stringify(text)} must be caught as ${tag}`);
  }
});

test('placeholders, logs and test support are not screen text', () => {
  assert.equal(stripPlaceholders('{depth_now:.2} m · {state_hash}'), '{} m · {}');
  assert.deepEqual(violations(stripPlaceholders('Revision {state_hash}: saved')), []);
  assert.ok(offScreen('const F: &str = include_str!('), 'a compile-time include is not screen text');
  assert.ok(offScreen('        eprintln!('));
  assert.ok(offScreen('.expect('));
  assert.ok(!offScreen('ui.label(t::body('));
  const lits = rustLiterals('fn a() { ui.label("plain"); }\n#[cfg(test)]\nfn b() { x("the engine"); }\n');
  assert.deepEqual(lits.map((l) => [l.s, l.test]), [['plain', false], ['the engine', true]]);
});

function walk(p, out = []) {
  if (statSync(p).isFile()) return out.push(p), out;
  for (const n of readdirSync(p)) walk(join(p, n), out);
  return out;
}

test('SOURCE: no string literal in a painting module carries developer text', () => {
  const files = PAINTING.flatMap((p) => walk(resolve(ROOT, p))).filter((f) => f.endsWith('.rs') && !TEST_MODULES.test(f));
  assert.ok(files.length >= 15, `the painting set resolved to ${files.length} file(s)`);
  const bad = [];
  let checked = 0;
  for (const f of files) {
    for (const { line, s, before, test: inTest } of rustLiterals(readFileSync(f, 'utf8'))) {
      if (s.length < 3 || inTest || offScreen(before)) continue;
      checked++;
      const v = violations(stripPlaceholders(s), { source: true });
      if (v.length) bad.push(`${relative(ROOT, f)}:${line} ${v.join(',')} ${s.slice(0, 100)}`);
    }
  }
  assert.ok(checked > 500, `only ${checked} literal(s) checked - the scan is not reading the sources`);
  assert.deepEqual(bad, [], `developer text in painted sources:\n${bad.join('\n')}`);
});

/**
 * The characters a TrueType font maps (its `cmap`, formats 4 and 12) - enough to tell whether a painted
 * string can draw without a fallback box. The UI fonts are the bundled Plex subsets.
 */
function cmapChars(path) {
  const b = readFileSync(path);
  const u16 = (o) => b.readUInt16BE(o);
  const i16 = (o) => b.readInt16BE(o);
  const u32 = (o) => b.readUInt32BE(o);
  const tables = {};
  for (let i = 0; i < u16(4); i++) tables[b.toString('latin1', 12 + 16 * i, 16 + 16 * i)] = u32(20 + 16 * i);
  const off = tables.cmap;
  const chars = new Set();
  for (let i = 0; i < u16(off + 2); i++) {
    const st = off + u32(off + 8 + 8 * i);
    const fmt = u16(st);
    if (fmt === 4) {
      const seg = u16(st + 6) / 2;
      const ends = st + 14, starts = ends + 2 * seg + 2, deltas = starts + 2 * seg, roffs = deltas + 2 * seg;
      for (let k = 0; k < seg; k++) {
        for (let c = u16(starts + 2 * k); c <= u16(ends + 2 * k) && c !== 0xffff; c++) {
          const ro = u16(roffs + 2 * k);
          let g = ro ? u16(roffs + 2 * k + ro + 2 * (c - u16(starts + 2 * k))) : c;
          if (g) g = (g + i16(deltas + 2 * k)) & 0xffff;
          if (g) chars.add(c);
        }
      }
    } else if (fmt === 12) {
      for (let k = 0; k < u32(st + 12); k++) {
        const g = st + 16 + 12 * k;
        for (let c = u32(g); c <= u32(g + 4); c++) chars.add(c);
      }
    }
  }
  return chars;
}
const FONTS = resolve(ROOT, 'cockpit/assets/fonts/subset');
const AFTER = resolve(ROOT, 'design/137/after');
const BEFORE = resolve(ROOT, 'design/137/before');
const TABS = ['instrument', 'size', 'curves', 'water', 'rate', 'compare', 'report'];

/** Every developer-text finding in a capture set, plus the tabs it covers and how much it read. */
function judge(dir) {
  const sidecars = readdirSync(dir).filter((f) => f.endsWith('.json') && !f.startsWith('_'));
  const tabs = new Set();
  const bad = new Set();
  const blank = [];
  let checked = 0;
  for (const f of sidecars) {
    const d = JSON.parse(readFileSync(join(dir, f), 'utf8'));
    if (d.screen?.screen) tabs.add(d.screen.screen);
    if (d.blank) blank.push(f);
    for (const [layer, list] of [['screen', d.screen?.strings], ['screen', d.painted], ['reader', d.sr_text]]) {
      for (const s of list || []) {
        checked++;
        const v = violations(s, { format: true });
        if (v.length) bad.add(`${layer} ${f}: ${v.join(',')} ${String(s).slice(0, 100)}`);
      }
    }
  }
  return { sidecars, tabs, bad: [...bad], blank, checked };
}

test('PAINTED: the after captures carry no developer text and no off-format number', (t) => {
  if (!existsSync(AFTER)) {
    t.diagnostic('NOT VERIFIABLE: not published: design/137 is excluded by the snapshot allowlist — the private tree runs this check');
    return;
  }
  const r = judge(AFTER);
  assert.ok(r.sidecars.length >= 12, `${r.sidecars.length} capture sidecar(s): every tab plus a calc card is expected`);
  assert.deepEqual(r.blank, [], 'blank frames');
  for (const tab of TABS) assert.ok(r.tabs.has(tab), `no after capture of the ${tab} tab`);
  assert.ok(r.checked > 300, `only ${r.checked} painted string(s) read`);
  assert.deepEqual(r.bad, [], `developer text on screen:\n${r.bad.join('\n')}`);
  // conductor review: every painted character is in the bundled fonts - a missing one draws as a box
  // (`‹` came out as "?"); geometric marks are drawn shapes, never glyphs
  const have = new Set();
  for (const f of readdirSync(FONTS).filter((f) => f.endsWith('.ttf'))) for (const c of cmapChars(join(FONTS, f))) have.add(c);
  assert.ok(have.has(0xb7) && have.has(0x2212) && have.has(0xb3), 'the cmap reader is not reading the fonts');
  const tofu = new Set();
  for (const f of r.sidecars) {
    const d = JSON.parse(readFileSync(join(AFTER, f), 'utf8'));
    for (const str of [...(d.screen?.strings || []), ...(d.painted || [])]) {
      for (const ch of String(str)) if (ch.codePointAt(0) > 0x20 && !have.has(ch.codePointAt(0))) tofu.add(`${ch} U+${ch.codePointAt(0).toString(16).toUpperCase()} in ${f}: ${String(str).slice(0, 60)}`);
    }
  }
  assert.deepEqual([...tofu], [], `painted characters the bundled fonts do not have:\n${[...tofu].join('\n')}`);
});

test('PAINTED: the same check fails on the before captures, naming the developer text', (t) => {
  if (!existsSync(BEFORE)) {
    t.diagnostic('NOT VERIFIABLE: not published: design/137 is excluded by the snapshot allowlist — the private tree runs this check');
    return;
  }
  const r = judge(BEFORE);
  for (const tab of TABS) assert.ok(r.tabs.has(tab), `no before capture of the ${tab} tab`);
  // the tells #137 was opened for, each read from the pre-fix frames
  for (const want of [/word:engine/, /word:Rust/, /ident/, /reader /]) {
    assert.ok(r.bad.some((b) => want.test(b)), `the check passed the before captures on ${want}:\n${r.bad.slice(0, 20).join('\n')}`);
  }
});
