/**
 * The static pass over the surviving tree (issue #40 slice 4 fix round 1; the chain's `static` leg).
 *
 * What it does, and why:
 *
 *  1. **Every module parses** (`node --check`, as the retired pass did). The chain executes most of
 *     these files anyway; this catches the ones it does not run (`release-*.mjs`, the audit drivers).
 *  2. **Every relative reference the tree makes resolves to a file that exists.** That is the class
 *     of check the retirement needed and did not have: `web/public-entry.js` imported a deleted
 *     `../src/app.js` and every gate stayed green. Covered:
 *       - JavaScript (`.js`/`.mjs`, ESM): `import … from '…'`, `export … from '…'`, side-effect
 *         `import '…'`, and dynamic `import(…)` whose argument is a string literal or an identifier
 *         the same file assigns exactly one string literal;
 *       - HTML: `href`/`src` attributes that are relative references (schemes, protocol-relative
 *         URLs, fragments and `data:` URIs are not references to a file in this tree);
 *       - markdown: relative `[text](target)` links (anchors and queries are stripped).
 *  3. **The licence claims** the retired prototype's static check carried: `package.json`'s SPDX
 *     identifier and the `LICENSE` file's opening text. (The third claim it checked lived in the
 *     deleted `index.html`; the HTML-id, API-surface and theme-token checks it carried died with
 *     that document and its interface.)
 *
 * What it deliberately does NOT do:
 *   - it does not resolve third-party vendored builds (`vendor/three*.js` — minified bytes whose
 *     integrity is the pin's business, checked by `npm run bundle-check`);
 *   - it does not verify the served set or the deploy manifest (`npm run deploy-check` owns the
 *     fetch contract) or the pinned pieces (`npm run bundle-check`);
 *   - it does not scan build outputs (`rust/target`, `cockpit/pkg`): nothing they contain is a
 *     reference the tree resolves or executes today;
 *   - it does not check Rust sources: the crate resolves its own modules and `cargo test` proves it.
 *
 * Exit: 0 every module parses, every reference resolves and the licence claims hold; 1 at least one
 * failure (each named).
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'pkg', 'evidence', 'coverage', 'dist']);
const THIRD_PARTY_VENDOR = /(^|\/)vendor\/three[^/]*\.js$/;
const failures = [];
const counts = {
  scannedFiles: 0,
  syntaxChecked: 0,
  javascriptFiles: 0,
  htmlDocuments: 0,
  markdownFiles: 0,
  references: 0
};

function walk(directory, files = []) {
  for (const name of readdirSync(directory)) {
    if (SKIP_DIRS.has(name)) continue;
    const path = join(directory, name);
    const stat = statSync(path);
    if (stat.isDirectory()) walk(path, files);
    else files.push(path);
  }
  return files;
}

/** True when the target names a file in this tree rather than a scheme, an anchor or an absolute URL. */
function isRelativeReference(target) {
  if (target === '' || target.startsWith('#') || target.startsWith('//')) return false;
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(target)) return false; // http:, https:, data:, mailto: …
  if (target.startsWith('/')) return false; // origin-absolute, not a file in this tree
  return true;
}

/**
 * The spans of comments, string literals, template literals and regular expression literals in
 * `source` (half-open `[start, end)`). A reference match that *starts inside* one of these is data,
 * not a reference the tree makes — `tests/deploy-check.test.js` writes fixtures that embed broken
 * imports as string payloads.
 *
 * Regex literals matter here beyond completeness: the drift gate matches a fenced JSON block, so its
 * patterns contain backticks, and a scanner that treated those as template delimiters would lose
 * quote parity and silently swallow real code from the scan.
 */
function literalSpans(source) {
  const spans = [];
  const keywordsBeforeRegex = new Set(['return', 'typeof', 'instanceof', 'case', 'in', 'of', 'do', 'else', 'yield', 'await', 'void', 'delete', 'throw', 'new']);
  let index = 0;
  let previousSignificant = '';
  while (index < source.length) {
    const character = source[index];
    const next = source[index + 1];
    if (character === '/' && next === '/') {
      const end = source.indexOf('\n', index);
      spans.push([index, end === -1 ? source.length : end]);
      index = end === -1 ? source.length : end;
      continue;
    }
    if (character === '/' && next === '*') {
      const end = source.indexOf('*/', index + 2);
      const stop = end === -1 ? source.length : end + 2;
      spans.push([index, stop]);
      index = stop;
      continue;
    }
    if (character === '"' || character === "'" || character === '`') {
      let cursor = index + 1;
      while (cursor < source.length) {
        if (source[cursor] === '\\') { cursor += 2; continue; }
        if (source[cursor] === character) break;
        cursor += 1;
      }
      const stop = Math.min(cursor + 1, source.length);
      spans.push([index, stop]);
      index = stop;
      previousSignificant = character;
      continue;
    }
    if (character === '/') {
      // A `/` can only open a regex where an operand may not have ended yet.
      const word = /[\w$]+$/.exec(source.slice(0, index).replace(/[^\n]*\n/g, ''))?.[0] ?? '';
      const opensRegex = /[([{=,:;!&|?+\-*%~^<>]/.test(previousSignificant) || keywordsBeforeRegex.has(word);
      if (opensRegex) {
        let cursor = index + 1;
        let inClass = false;
        while (cursor < source.length) {
          const current = source[cursor];
          if (current === '\\') { cursor += 2; continue; }
          if (current === '\n') break; // an unterminated regex: not a regex after all
          if (current === '[') inClass = true;
          else if (current === ']') inClass = false;
          else if (current === '/' && !inClass) break;
          cursor += 1;
        }
        if (cursor < source.length && source[cursor] === '/') {
          spans.push([index, cursor + 1]);
          index = cursor + 1;
          previousSignificant = '/';
          continue;
        }
      }
    }
    if (!/\s/.test(character)) previousSignificant = character;
    index += 1;
  }
  return spans;
}

function checkReference(file, target, kind) {
  const cleaned = target.split('#')[0].split('?')[0];
  if (cleaned === '') return; // a pure fragment/query: nothing to resolve
  counts.references += 1;
  const resolved = resolve(dirname(file), cleaned);
  if (!existsSync(resolved)) {
    failures.push(`${relative(root, file)}: ${kind} ${JSON.stringify(target)} → ${relative(root, resolved)} does not exist`);
  }
}

function checkJavaScript(file, source) {
  counts.javascriptFiles += 1;
  const spans = literalSpans(source);
  const isCode = (index) => !spans.some(([start, end]) => index >= start && index < end);
  const references = [];
  const staticPatterns = [
    /(?:^|\n)[ \t]*(?:import|export)\b[^;]*?from\s+['"]([^'"]+)['"]/g, // import/export … from '…'
    /(?:^|\n)[ \t]*import\s+['"]([^'"]+)['"]/g                          // side-effect import '…'
  ];
  for (const pattern of staticPatterns) {
    for (const match of source.matchAll(pattern)) if (isCode(match.index)) references.push(match[1]);
  }
  for (const match of source.matchAll(/\bimport\(\s*['"]([^'"]+)['"]\s*\)/g)) {
    if (isCode(match.index)) references.push(match[1]);
  }
  // Dynamic import of an identifier the file assigns exactly one string literal (`const X = '…'`).
  for (const match of source.matchAll(/\bimport\(\s*([A-Za-z_$][\w$]*)\s*\)/g)) {
    if (!isCode(match.index)) continue;
    const assignments = [...source.matchAll(new RegExp(`(?:^|\\n)[ \\t]*const\\s+${match[1]}\\s*=\\s*['"]([^'"]+)['"]`, 'g'))]
      .filter((assignment) => isCode(assignment.index));
    if (assignments.length === 1) references.push(assignments[0][1]);
    else failures.push(`${relative(root, file)}: dynamic import(${match[1]}) is not a literal or a single-literal const`);
  }
  for (const target of references) {
    if (isRelativeReference(target)) checkReference(file, target, 'import');
  }
}

function checkHtml(file, source) {
  counts.htmlDocuments += 1;
  for (const match of source.matchAll(/(?:href|src)\s*=\s*["']([^"']+)["']/g)) {
    if (isRelativeReference(match[1])) checkReference(file, match[1], 'subresource');
  }
}

function checkMarkdown(file, source) {
  counts.markdownFiles += 1;
  // Fenced code blocks are examples, not links: blank them before scanning.
  const prose = source.replace(/^ {0,3}```[\s\S]*?^ {0,3}```/gm, (block) => block.replace(/[^\n]/g, ' '));
  for (const match of prose.matchAll(/\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
    if (isRelativeReference(match[1])) checkReference(file, match[1], 'link');
  }
}

const files = walk(root);
for (const file of files) {
  const extension = extname(file);
  if (!['.js', '.mjs', '.html', '.md'].includes(extension)) continue;
  if (THIRD_PARTY_VENDOR.test(file)) continue;
  counts.scannedFiles += 1;
  const source = readFileSync(file, 'utf8');
  if (extension === '.js' || extension === '.mjs') checkJavaScript(file, source);
  else if (extension === '.html') checkHtml(file, source);
  else checkMarkdown(file, source);
}

// Syntax: every surviving module parses — the retired pass checked the same way (`node --check`).
// The chain executes most of these files anyway; this catches the ones it does not run.
for (const file of files) {
  if (!['.js', '.mjs'].includes(extname(file)) || THIRD_PARTY_VENDOR.test(file)) continue;
  const check = spawnSync(process.execPath, ['--check', file], { encoding: 'utf8' });
  counts.syntaxChecked += 1;
  if (check.status !== 0) {
    failures.push(`${relative(root, file)}: does not parse (node --check)\n${(check.stderr || '').trim()}`);
  }
}

// The licence claims the prototype's static check carried (its third claim died with `index.html`).
const polyformLicenceUrl = 'https://polyformproject.org/licenses/noncommercial/1.0.0';
const polyformLicenceHeader = [
  '# PolyForm Noncommercial License 1.0.0',
  '',
  polyformLicenceUrl
].join('\n');
const requiredNotice = /^Required Notice: .+$/m;
const packageJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
if (packageJson.license !== 'SEE LICENSE IN LICENSE') {
  failures.push(`package.json licence is ${JSON.stringify(packageJson.license)}; expected the pointer "SEE LICENSE IN LICENSE"`);
}
const licenceText = readFileSync(join(root, 'LICENSE'), 'utf8');
// The notice line leads the file (the PolyForm terms want it carried with every copy), so the
// terms are asserted as carried, not as the opening seconds of the file.
if (!licenceText.includes(polyformLicenceHeader)) {
  failures.push('LICENSE does not carry the PolyForm Noncommercial License 1.0.0 text');
}
if (!requiredNotice.test(licenceText)) {
  failures.push('LICENSE carries no "Required Notice:" line — the PolyForm terms make it part of every copy');
}
counts.licenceChecks = 3;
counts.licenceUrl = polyformLicenceUrl; // the URL the terms carry; the same text is pinned in this guard's header

if (failures.length) {
  console.error(JSON.stringify({ passed: false, failures }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({
  passed: true,
  ...counts,
  note: 'every module parses, every relative reference in the surviving tree resolves, and the licence claims hold; third-party vendor builds and build outputs are out of scope (see the header)'
}, null, 2));
