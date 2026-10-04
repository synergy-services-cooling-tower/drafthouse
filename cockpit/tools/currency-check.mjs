#!/usr/bin/env node
/**
 * Money gate — no money field anywhere (issue #1), extended to the currency terms (issue #83).
 *
 * The pattern is the retirement gate's (`scripts/retirement-check.mjs`): a rule a gate does not
 * assert is a hope, and a skip must never read as a pass. This gate walks the surfaces THIS plane
 * deploys and the shapes the app declares, and fails closed if a money-ish token appears where a
 * field would be. Two passes over every scanned file:
 *
 *   (a) the key pass, every token: the token is (part of) a field or key name —
 *       `currency:`, `"bridge_price":`, `pub lifecycle_cost: f64`, `usd = …`;
 *   (b) the literal pass, the tokens whose absence cannot be a wording choice — `price`,
 *       `currency`, `capex`, `discount`, `lifecycle`, `usd`, `thb`, `penalty` — as raw text,
 *       wherever they sit. One exception, and it is a fragment rule, not a hole: a run of the
 *       token's letters BETWEEN two letters is part of a longer word, not an occurrence of the
 *       token — `thb` inside the std type `PathBuf` (pa-THB-uf) is the case that named it, and
 *       every honest occurrence (a word, a snake or camel segment, a quoted value, a
 *       digit-adjacent code) still has a non-letter flank and is caught. `cost` and `money` are
 *       deliberately key-pass only: the app's own comments attest the rule ("no money field",
 *       "the measured cost of one real run"), and banning those two words in prose would red the
 *       attestation itself.
 *
 * The engine port keeps its own raw-token gate on `rust/src`
 * (`rust/tests/selection.rs::no_money_ish_identifier_exists_anywhere_in_the_port`, the same ten
 * tokens); this is that gate's app-surface counterpart, so "no money field" holds on both sides of
 * the wire.
 *
 * The scan set — what "anywhere" means here — is what this deployment ships and what the app
 * declares: the committed files `deploy-manifest.txt` lists, the plane's document
 * (`cockpit/index.html`), the built module (`cockpit/pkg/**`, when it has been built), the app's
 * own sources (`cockpit/src`, `cockpit/contract/src`, `cockpit/seams/src`) and the shipped data
 * shapes (`cockpit/assets/**`). Binaries (fonts, wasm) are reported as skipped, never silently
 * dropped. Deliberately out of scope: the public prototype's pieces (`web/**`, `src/**` — the
 * `visuals` bundle's own files, whose comments speak about price/currency/lifecycle to say they
 * are absent, and whose sample catalog carries a currency field the public demo has shipped for
 * its whole life). The rule this gate enforces is the private plane's.
 *
 * Usage — from any working directory:
 *
 *     node cockpit/tools/currency-check.mjs
 *
 * Exit status: 0 no money-ish field or token on the scanned surfaces (JSON report on stdout);
 * 1 one is there — every hit is listed naming the file, the line and the token (JSON on stderr);
 * 2 the check could not run (no working tree, an unreadable file).
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const failures = [];

const refuse = (message) => {
  console.error(`currency-check: ${message}`);
  process.exit(2);
};

/** The engine port's own ten tokens — one list, both surfaces (issue #1). */
const TOKENS = ['cost', 'price', 'currency', 'capex', 'discount', 'lifecycle', 'money', 'usd', 'thb', 'penalty'];
/** Tokens whose absence cannot be a wording choice: scanned as raw text wherever they sit (see the
 * fragment rule in the header: letters between letters do not count). */
const LITERAL = TOKENS.filter((token) => token !== 'cost' && token !== 'money');

const MANIFEST = 'deploy-manifest.txt';
const DOCUMENT = 'cockpit/index.html';
const SOURCE_DIRS = ['cockpit/src', 'cockpit/contract/src', 'cockpit/seams/src'];
const DATA_DIRS = ['cockpit/assets'];
const BUILT = 'cockpit/pkg';

const TEXT = /\.(?:json|html|js|mjs|cjs|css|rs|toml|txt|svg)$/i;
const BINARY = /\.(?:ttf|otf|woff2?|png|jpe?g|gif|ico|webp|wasm|zip|gz)$/i;

const scanned = new Set();
const skipped = new Set();

/** Collect every plain file under `directory` (no recursion limit — these trees are bounded). */
const walk = (directory) => {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) walk(path);
    else if (entry.isFile()) {
      const rel = relative(root, path).split(sep).join('/');
      if (BINARY.test(rel)) skipped.add(rel);
      else if (TEXT.test(rel)) scanned.add(rel);
      else skipped.add(rel);
    }
  }
};

/* ---- the deployment record: the committed half this plane ships ---- */

if (!existsSync(join(root, MANIFEST))) {
  failures.push(`${MANIFEST} is missing; it is the deployment record this gate scans (run: npm run deploy-manifest)`);
} else {
  for (const line of readFileSync(join(root, MANIFEST), 'utf8').split('\n')) {
    const entry = line.trim();
    if (entry === '' || entry.startsWith('#')) continue;
    const target = entry.split(/\s{2,}/)[0];
    const absolute = join(root, target);
    // A missing file is the deploy check's failure to name; this gate scans what is there.
    if (!existsSync(absolute) || !statSync(absolute).isFile()) continue;
    if (BINARY.test(target)) skipped.add(target);
    else scanned.add(target);
  }
}

/* ---- the plane's document, the built module, the app's sources and shapes ---- */

if (!existsSync(join(root, DOCUMENT))) {
  failures.push(`${DOCUMENT} is missing; it is the plane's document`);
} else {
  scanned.add(DOCUMENT);
}

if (existsSync(join(root, BUILT)) && statSync(join(root, BUILT)).isDirectory()) {
  walk(join(root, BUILT));
}

for (const directory of [...SOURCE_DIRS, ...DATA_DIRS]) {
  if (!existsSync(join(root, directory))) {
    failures.push(`${directory} is missing; it is a surface this gate scans`);
    continue;
  }
  walk(join(root, directory));
}

if (scanned.size === 0) {
  failures.push('nothing was scanned; the gate must scan something');
}

/* ---- the two passes ---- */

const keyPass = new RegExp(`(^|[^A-Za-z0-9_])(token)[A-Za-z0-9_]*\\s*["']?\\s*[:=]`, 'gi');
const literalTokens = LITERAL.map((token) => token.toLowerCase());

for (const target of [...scanned].sort()) {
  const absolute = join(root, target);
  let text;
  try {
    text = readFileSync(absolute, 'utf8');
  } catch (error) {
    refuse(`${target} is not readable: ${error.message}`);
  }
  const lineAt = (index) => text.slice(0, index).split('\n').length;
  for (const token of TOKENS) {
    const pattern = new RegExp(keyPass.source.replace('(token)', `(${token})`), 'gi');
    for (let match = pattern.exec(text); match !== null; match = pattern.exec(text)) {
      // the match may start at the boundary character before the token (which can be a newline)
      const at = match.index + match[0].toLowerCase().indexOf(token);
      failures.push(`${target}:${lineAt(at)} declares a money-ish field or key (${token})`);
    }
  }
  const lowered = text.toLowerCase();
  for (const token of literalTokens) {
    for (let at = lowered.indexOf(token); at !== -1; at = lowered.indexOf(token, at + token.length)) {
      // The fragment rule (issue #74 merge): a run of the token's letters BETWEEN two letters is part
      // of a longer word, not an occurrence - `thb` inside `PathBuf` (pa-THB-uf) reds the whole app
      // tree otherwise. Both flanks must be letters for the skip; every standalone, quoted,
      // snake/camel-segment or digit-adjacent occurrence keeps a non-letter flank and stays red.
      const betweenLetters =
        /[a-z]/.test(lowered[at - 1] ?? '') && /[a-z]/.test(lowered[at + token.length] ?? '');
      if (betweenLetters) continue;
      failures.push(`${target}:${lineAt(at)} contains the currency token (${token})`);
    }
  }
}

/* ---- emit ---- */

if (failures.length) {
  console.error(JSON.stringify({
    passed: false,
    tokens: TOKENS,
    failures: [...new Set(failures)],
  }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({
  passed: true,
  tokens: TOKENS,
  literalTokens: LITERAL,
  scanned: scanned.size,
  files: [...scanned].sort(),
  skipped: [...skipped].sort(),
}, null, 2));
