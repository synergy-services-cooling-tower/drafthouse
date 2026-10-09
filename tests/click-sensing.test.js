// drafthouse#135: no control may read `clicked()` / `double_clicked()` / `drag_started()` on a response
// that cannot sense it.
//
// Two shapes in this app look clickable and are not, and both come from egui itself:
//   * `ui.dnd_drag_source(...)` builds its widget with `Sense::drag()` - the response it hands back can
//     never report a click (`egui/src/ui.rs`);
//   * `Frame::show(...)` hands back a response it clears to hover only - `containers/frame.rs` ends with
//     `response.sense = Sense::hover()`, so a frame-drawn chip cannot report a click either.
// A control built from either one, or from `.response` off one, has to put a click sense back on the same
// rect (`ui.interact(rect, id, Sense::click_and_drag())` - the idiom `ctl:notes` already uses). This file
// is the repo-wide half of issue #135's second acceptance criterion: it reads every `.rs` file under
// `cockpit/src`, traces each click-ish read back to the binding that built the response, and fails when
// that binding is one of the two shapes.
//
// The check is a check only if it can fail, so the second test drives the same scanner over the pre-fix
// shape of each site - the `+ custom` chip, the rail chip, the picker card - and over the shape that
// replaces them. A scanner that cannot tell those apart is not evidence, and this round's own diff would
// pass either way.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.join(HERE, '..');
const SRC = path.join(ROOT, 'cockpit', 'src');

const READ = /\.(clicked|double_clicked|drag_started|drag_stopped)\s*\(\s*\)/g;
const ID = /[A-Za-z0-9_.]/;
const DELIM = '=,;({\n';

// Rust source with comments and string literals blanked out, offsets (and so line numbers) preserved: a
// shape named in a comment is not a violation, and a `;` inside a string is not a statement end.
function blank(src) {
  let out = '';
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    const c2 = src[i + 1];
    if (c === '/' && c2 === '/') {
      while (i < n && src[i] !== '\n') {
        out += ' ';
        i++;
      }
      continue;
    }
    if (c === '/' && c2 === '*') {
      out += '  ';
      i += 2;
      while (i < n && !(src[i] === '*' && src[i + 1] === '/')) {
        out += src[i] === '\n' ? '\n' : ' ';
        i++;
      }
      out += '  ';
      i += 2;
      continue;
    }
    if (c === '"') {
      out += ' ';
      i++;
      while (i < n && src[i] !== '"') {
        if (src[i] === '\\') {
          out += '  ';
          i += 2;
          continue;
        }
        out += src[i] === '\n' ? '\n' : ' ';
        i++;
      }
      out += ' ';
      i++;
      continue;
    }
    // a char literal is `'x'` or `'\n'`; anything else after `'` is a lifetime, which stays readable
    if (c === "'" && (src[i + 2] === "'" || src[i + 1] === '\\')) {
      out += ' ';
      i++;
      while (i < n && src[i] !== "'") {
        if (src[i] === '\\') {
          out += '  ';
          i += 2;
          continue;
        }
        out += ' ';
        i++;
      }
      out += ' ';
      i++;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

// Every `let <ident> = <statement>;` on the blanked source, balanced to the statement's own `;`. Per
// identifier the whole list is kept in source order: a name can be bound more than once in a function,
// and the read that matters is served by one of them.
function bindings(blanked) {
  const out = new Map();
  const re = /\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=;]*)?=/g;
  let m;
  while ((m = re.exec(blanked))) {
    let i = m.index + m[0].length;
    let depth = 0;
    let text = '';
    while (i < blanked.length) {
      const c = blanked[i];
      if (c === '(' || c === '[' || c === '{') depth++;
      if (c === ')' || c === ']' || c === '}') depth--;
      if (c === ';' && depth <= 0) break;
      text += c;
      i++;
    }
    const list = out.get(m[1]) ?? [];
    list.push({ rhs: text, at: m.index });
    out.set(m[1], list);
  }
  return out;
}

// The receiver chain of each click-ish read, walked back over identifier and member-access characters.
// `inline` is the expression the read sits on, for the reads that are not on a plain binding.
function reads(blanked) {
  const out = [];
  READ.lastIndex = 0;
  let m;
  while ((m = READ.exec(blanked))) {
    const j = m.index;
    let s = j;
    while (s > 0 && ID.test(blanked[s - 1])) s--;
    let p = s;
    while (p > 0 && !DELIM.includes(blanked[p - 1])) p--;
    out.push({
      kind: m[1],
      chain: blanked.slice(s, j),
      inline: blanked.slice(p, j).trim(),
      at: m.index
    });
  }
  return out;
}

// The two shapes, named for the report. A binding that carries `.interact(` has already put a click sense
// back on the rect, which is exactly the fix.
function shape(text) {
  if (/dnd_drag_source\s*\(/.test(text) && !/\.interact\s*\(/.test(text)) {
    return 'dnd_drag_source senses drag only';
  }
  if (/\.show\s*\(/.test(text) && /\.response/.test(text) && !/\.interact\s*\(/.test(text)) {
    return 'Frame::show(...).response senses hover only';
  }
  return null;
}

// Walk the response back through plain re-wrappers: `let resp = resp.on_hover_text(...)` (or a `match`
// over `resp`) hands back the *same* response, so the nearest binding that actually built it is the one
// whose shape the read sees. The walk stops at the first binding not built from that identifier.
function shape_of(binds, ident, pos) {
  const word = new RegExp(`\\b${ident}\\b`);
  const list = (binds.get(ident) ?? [])
    .filter((b) => b.at < pos)
    .sort((a, b) => b.at - a.at);
  for (const b of list) {
    const why = shape(b.rhs);
    if (why) return { why, bind: b };
    if (!word.test(b.rhs)) break;
  }
  return null;
}

function lineOf(src, offset) {
  return src.slice(0, offset).split('\n').length;
}

function scan(src) {
  const b = blank(src);
  const binds = bindings(b);
  const found = [];
  for (const r of reads(b)) {
    const first = r.chain.split('.')[0].replace(/^[&*]+/, '');
    const hit = first ? shape_of(binds, first, r.at) : null;
    const why = hit ? hit.why : r.inline.length > 0 ? shape(r.inline) : null;
    if (!why) continue;
    found.push({
      line: lineOf(src, r.at),
      read: `${r.chain || r.inline}.${r.kind}()`,
      why,
      source: (hit ? hit.bind.rhs : r.inline).trim().slice(0, 70).replace(/\s+/g, ' ')
    });
  }
  return found;
}

function rustFiles(dir, acc = []) {
  for (const entry of readdirSync(dir)) {
    const p = path.join(dir, entry);
    if (statSync(p).isDirectory()) rustFiles(p, acc);
    else if (p.endsWith('.rs')) acc.push(p);
  }
  return acc;
}

test('no cockpit control reads a click on a response that cannot sense one', () => {
  const files = rustFiles(SRC).sort();
  assert.ok(
    files.length >= 10,
    `the audit read only ${files.length} files under cockpit/src - the scan is not looking at the app`
  );
  const violations = [];
  for (const f of files) {
    for (const v of scan(readFileSync(f, 'utf8'))) {
      violations.push(`${path.relative(ROOT, f)}:${v.line}  ${v.read}  <- ${v.why}  [${v.source}...]`);
    }
  }
  assert.deepEqual(
    violations,
    [],
    `controls ask a response to click but nothing put a click sense on it:\n${violations.join('\n')}`
  );
});

test('the audit flags the three shapes it exists for', () => {
  // The pre-fix shape of each site #135 names: the `+ custom` chip (frame-drawn, then re-wrapped by
  // `on_hover_text`), the rail chip and the picker card (both drag sources whose `.response` is read).
  const chip = [
    'let resp = t::chip_frame(t::PANEL, line)',
    '    .show(ui, |ui| { ui.label(RichText::new("+ custom")); })',
    '    .response;',
    'let resp = match fields {',
    '    Some(_) => resp.on_hover_text("the field list"),',
    '    None => resp.on_hover_text("no field list"),',
    '};',
    'if resp.clicked() { form.open(class, fields, false); }'
  ].join('\n');
  const rail = [
    'let inner = ui.dnd_drag_source(chip_id, part.clone(), |ui| { frame.show(ui, |ui| {}); });',
    'if inner.response.clicked() { vis.selected_part = Some(part.clone()); }'
  ].join('\n');
  const card = [
    'let inner = ui.dnd_drag_source(card_id, part.clone(), |ui| { frame.show(ui, |ui| {}); });',
    'if inner.response.clicked() { clicked = true; }'
  ].join('\n');
  // ...and the shape that replaces each of them: the same rect, interacted again with a click sense.
  const fixed = [
    'let inner = ui.dnd_drag_source(card_id, part.clone(), |ui| { frame.show(ui, |ui| {}); });',
    'let resp = ui.interact(inner.response.rect, card_id, Sense::click_and_drag());',
    'if resp.clicked() { clicked = true; }'
  ].join('\n');
  const chip_fixed = [
    'let resp = t::chip_frame(t::PANEL, line)',
    '    .show(ui, |ui| { ui.label(RichText::new("+ custom")); })',
    '    .response',
    '    .interact(egui::Sense::click());',
    'if resp.clicked() { form.open(class, fields, false); }'
  ].join('\n');

  assert.equal(scan(chip).length, 1, 'the `+ custom` chip shape must be flagged');
  assert.match(scan(chip)[0].why, /hover only/, 'the chip shape must be named as hover-only');
  assert.equal(scan(rail).length, 1, 'the rail chip shape must be flagged');
  assert.match(scan(rail)[0].why, /drag only/, 'the rail chip shape must be named as drag-only');
  assert.equal(scan(card).length, 1, 'the picker card shape must be flagged');
  assert.equal(scan(fixed).length, 0, 'a drag source with a click sense put back on its rect is the fix');
  assert.equal(
    scan(chip_fixed).length,
    0,
    'a frame response that re-interacts its own rect with a click sense is the fix'
  );
});

// The same scanner over any tree, which is how the base-commit half of this round's pair was produced
// (the base tree's copy lists the three sites the fix answers, at the base the branch started from; the
// head tree's copy reports none). `node --test tests/*.test.js` never passes `--dir`, so the suite above
// is the only thing a normal run sees.
//
//   node tests/click-sensing.test.js --dir <cockpit/src of the tree you want scored>
//
const dirArg = process.argv.indexOf('--dir');
if (
  dirArg >= 0 &&
  process.argv[dirArg + 1] &&
  import.meta.url === pathToFileURL(process.argv[1] ?? '').href
) {
  let total = 0;
  for (const f of rustFiles(process.argv[dirArg + 1]).sort()) {
    for (const v of scan(readFileSync(f, 'utf8'))) {
      total++;
      console.log(`${f}:${v.line}  ${v.read}  <- ${v.why}  [${v.source}...]`);
    }
  }
  console.log(`violations: ${total}`);
  process.exit(total === 0 ? 0 : 1);
}
