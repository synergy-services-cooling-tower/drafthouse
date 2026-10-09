/**
 * dev-text-rule.mjs - the #137 rule for "developer text on screen", shared by the repo test
 * (`tests/dev-text.test.js`), the source scan (`design/137/tools/scan-literals.mjs`) and the
 * painted-text gate (`design/137/tools/strings-gate.mjs`). It lives under tests/ so the published
 * suite carries it; the design tools re-export it from `design/137/tools/dev-text.mjs`.
 *
 * A string is developer text when it carries any of:
 *   word      engine, Rust, WASM, Bevy, fixture, seam, adapter (whole words, any case)
 *   camel     a camelCase identifier (hotWaterC, waterQualityClass) - units such as kW / kPa / mmWG
 *             are not identifiers (a lower-case run of one, or four letters or fewer)
 *   ident     a snake_case identifier (range_c), a Rust path (Engine::run), a dotted field
 *             path (provenance.fixed, rate.inputs.requirements)
 *   path      a file path or a source/data file name (fixtures/engine-run.json, notes.rs)
 *   issue     an issue reference (#91, synergy-apps#1808)
 *
 * Format placeholders (`{state_hash}`, `{depth_now:.2}`) are stripped before judging a source literal: they
 * are replaced by a number at paint time, never shown.
 *
 * and, for the painted text only (`format: true`), a number or unit off the one house style:
 *   unit      degC, m3/hr, m3/s, kg/m3, m2, mmWG, a bare "C" after a number
 *   fmt       a temperature or K with 2+ decimals, a power with 2+ decimals, a percentage with
 *             decimals, a KaV/L with 3+ decimals
 */

export const WORDS = ['engine', 'engines', 'rust', 'wasm', 'bevy', 'fixture', 'fixtures', 'seam', 'seams', 'adapter', 'adapters'];

const RE = {
  word: new RegExp(`\\b(${WORDS.join('|')})\\b`, 'i'),
  camel: /\b[a-z]{2,}[A-Z][A-Za-z0-9]*\b/g,
  snake: /\b[a-z][a-z0-9]*_[a-z0-9_]+\b/,
  rustPath: /\b[A-Za-z_]+::[A-Za-z_]+/,
  dotted: /\b[a-z][a-z0-9]*\.[a-z][a-zA-Z0-9]+(\.[a-zA-Z0-9]+)*\b/,
  path: /(?:^|[\s(`'"])(?:\.{0,2}\/)?[\w.-]+(?:\/[\w.-]+)+\.[a-z]{1,5}\b/,
  file: /\b[\w-]+\.(?:json|rs|mjs|js|toml|wasm|ts|py|md|html)\b/,
  issue: /(?:^|[\s(\w-])#\d{1,5}\b/,
};

/** Dotted runs that are prose, not field paths. */
const DOTTED_OK = new Set(['e.g', 'i.e', 'vs.']);

/** The house units (#137 item 3), and the spellings that are off it. */
const UNIT_BAD = [
  [/\bdeg ?C\b/, 'degC'],
  [/\bm3\b|m3\/|\/m3\b/, 'm3 (use m³)'],
  [/\bm2\b/, 'm2 (use m²)'],
  [/\/hr\b/, '/hr (use /h)'],
  [/\bmm ?WG\b/i, 'mmWG (use Pa or kPa)'],
  [/\d(?:\.\d+)? C\b(?! ?\w)/, 'bare C (use °C or K)'],
];

/**
 * Number style (#137 item 3): 0.1 °C / K, 0.1 kW, whole %, KaV/L to 2 decimals. Flows are 3 significant
 * figures, which a regex cannot judge without the magnitude: a flow is checked as "no more than 3
 * significant figures shown" (trailing zeros of an integer part are not significant).
 */
const FMT = [
  [/(-?\d+\.\d{2,}) ?°C/, 'temperature to 0.1 °C'],
  [/(-?\d+\.\d{2,}) ?K\b/, 'K to 0.1'],
  [/(-?\d+\.\d{2,}) ?kW\b/, 'power to 0.1 kW'],
  [/(-?\d+\.\d+) ?%/, 'percentages whole'],
  [/speed ratio \d/, 'fan speed as % of rated'],
];
const FLOW = /(\d[\d,]*(?:\.\d+)?) ?(m³\/h|m³\/s|kg\/s)/g;

function sigFigs(s) {
  const t = s.replace(/,/g, '');
  const [i, f = ''] = t.split('.');
  const digits = (i.replace(/^0+/, '') + f).replace(/^0+/, '');
  if (!f) return i.replace(/^0+/, '').replace(/0+$/, '').length || 1;
  return digits.length;
}

/**
 * The violations one string carries, as short tags. `source: true` relaxes the identifier rules for the
 * source scan (a literal that is ONLY an identifier - a key, an id, a command - is not prose).
 */
export function violations(s, { source = false, format = false } = {}) {
  const out = [];
  const text = String(s);
  const prose = /\s/.test(text.trim());
  if (source && !prose) {
    // a single token in source: only flag the hard words and paths (ids and keys are not on screen)
    if (RE.path.test(text) || RE.file.test(text)) out.push('path');
    return out;
  }
  const w = text.match(RE.word);
  if (w) out.push(`word:${w[1]}`);
  for (const m of text.matchAll(RE.camel)) {
    if (m[0].length > 4) {
      out.push(`camel:${m[0]}`);
      break;
    }
  }
  if (RE.snake.test(text)) out.push(`ident:${text.match(RE.snake)[0]}`);
  if (RE.rustPath.test(text)) out.push(`ident:${text.match(RE.rustPath)[0]}`);
  const d = text.match(RE.dotted);
  if (d && !DOTTED_OK.has(d[0]) && !/\.(drafthouse)$/.test(d[0])) out.push(`ident:${d[0]}`);
  if (RE.path.test(text) || (RE.file.test(text) && !/\.drafthouse\b/.test(text))) out.push('path');
  if (RE.issue.test(text)) out.push(`issue:${text.match(RE.issue)[0].trim()}`);
  if (format) {
    for (const [re, tag] of UNIT_BAD) if (re.test(text)) out.push(`unit:${tag}`);
    for (const [re, tag] of FMT) if (re.test(text)) out.push(`fmt:${tag}`);
    for (const m of text.matchAll(FLOW)) {
      if (sigFigs(m[1]) > 3) {
        out.push(`fmt:flow 3 s.f. (${m[0]})`);
        break;
      }
    }
  }
  return out;
}

/** `{name}`, `{name:.2}`, `{:>4}` -> `{}`: a placeholder is filled at paint time, it is never on screen. */
export function stripPlaceholders(s) {
  return String(s).replace(/\{[A-Za-z_][\w.]*(?::[^}]*)?\}/g, '{}').replace(/\{:[^}]*\}/g, '{}');
}

/**
 * A literal that never reaches the screen: the argument of a log / panic / assert macro, an `expect(..)`
 * message, or a `#[cfg(test)]` item. `before` is the source text just ahead of the literal.
 */
export function offScreen(before) {
  return /(?:include_str!|include_bytes!|e?println!|e?print!|panic!|unreachable!|todo!|assert\w*!|debug_assert\w*!|expect|log::\w+!|tracing::\w+!|(?:debug|info|warn|error|trace)!)\s*\(\s*(?:\n\s*)?(?:format!\s*\(\s*)?$/.test(before);
}

/**
 * Rust string literals with their line, skipping comments and stopping at the file's test module.
 * Each carries `before` (the 60 characters ahead of it) and `test` (inside a `#[cfg(test)]` fn).
 */
export function rustLiterals(src) {
  const out = [];
  let i = 0, line = 1;
  const testAt = src.search(/#\[cfg\(test\)\]\s*\n\s*mod\s/);
  const end = testAt >= 0 ? testAt : src.length;
  // `#[cfg(test)]` free functions above the test module: their bodies are test support, not UI
  const testFns = [];
  for (const m of src.matchAll(/#\[cfg\(test\)\]\s*\n\s*(?:pub(?:\([^)]*\))?\s+)?fn\s/g)) {
    const open = src.indexOf('{', m.index);
    let depth = 0, k = open;
    for (; k < src.length; k++) {
      if (src[k] === '{') depth++;
      else if (src[k] === '}' && --depth === 0) break;
    }
    testFns.push([m.index, k]);
  }
  const inTest = (p) => testFns.some(([a, b]) => p >= a && p <= b);
  while (i < end) {
    const c = src[i];
    if (c === '\n') { line++; i++; continue; }
    if (c === '/' && src[i + 1] === '/') { while (i < end && src[i] !== '\n') i++; continue; }
    if (c === '/' && src[i + 1] === '*') { const j = src.indexOf('*/', i + 2); for (let k = i; k < j; k++) if (src[k] === '\n') line++; i = j + 2; continue; }
    if (c === "'") {
      const m = src.slice(i, i + 12).match(/^'(\\u\{[0-9a-fA-F]+\}|\\.|[^'\\])'/);
      i += m ? m[0].length : 1;
      continue;
    }
    if (c === 'r' && /^r#*"/.test(src.slice(i, i + 6)) && !/[A-Za-z0-9_]/.test(src[i - 1] || '')) {
      const hashes = src.slice(i + 1).match(/^#*/)[0];
      const close = '"' + hashes;
      const start = i + 2 + hashes.length;
      const j = src.indexOf(close, start);
      const s = src.slice(start, j);
      out.push({ line, s, before: src.slice(Math.max(0, i - 60), i), test: inTest(i) });
      for (const ch of s) if (ch === '\n') line++;
      i = j + close.length;
      continue;
    }
    if (c === '"') {
      let j = i + 1, s = '';
      const l0 = line;
      while (j < src.length && src[j] !== '"') {
        if (src[j] === '\\') {
          if (src[j + 1] === '\n') { line++; j += 2; while (/\s/.test(src[j])) { if (src[j] === '\n') line++; j++; } continue; }
          s += src.slice(j, j + 2); j += 2; continue;
        }
        if (src[j] === '\n') line++;
        s += src[j++];
      }
      out.push({ line: l0, s, before: src.slice(Math.max(0, i - 60), i), test: inTest(i) });
      i = j + 1;
      continue;
    }
    i++;
  }
  return out;
}

/**
 * The cockpit sources that PAINT text (every tab, the instrument, notes, header, hover cards, the duty
 * panel, the custom-part form, the Report and the PDF sheet). Startup, CLI, project-file and engine
 * plumbing modules log to the terminal and are not in this list.
 */
export const PAINTING = [
  'cockpit/src/answer.rs',
  'cockpit/src/bridge.rs',
  'cockpit/src/compare.rs',
  'cockpit/src/duty_panel.rs',
  'cockpit/src/form.rs',
  'cockpit/src/hover.rs',
  'cockpit/src/notes.rs',
  'cockpit/src/state.rs', // the tray specs, the drop verdicts and the layer messages the strip shows
  'cockpit/src/ui.rs',
  'cockpit/src/screens/', // every file
];

/** Test-only modules inside the painting set (`#[cfg(test)] mod fixture_equals_cli;`): never painted. */
export const TEST_MODULES = /(fixture_equals_cli|crossflow_equals_cli)\.rs$/;
