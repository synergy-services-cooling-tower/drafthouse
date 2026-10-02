#!/usr/bin/env node
/**
 * Deployment readiness check — the deploy set is the cockpit plane (issue #60; the served
 * surface is the product UI, since issue #58).
 *
 * The public surface is the cockpit plane: `cockpit/index.html`, its committed assets
 * (`cockpit/assets/**` — the fixture record, the generated field lists and the fonts the app
 * fetches at run time) and the built module. Deployment is a file copy — which makes it easy to
 * get subtly wrong: a file omitted from the copy fails at runtime in the browser with nothing
 * failing here. So this derives the committed served set and verifies the committed
 * `deploy-manifest.txt` against it. Regeneration is deliberate: `--write-manifest`
 * (`npm run deploy-manifest`).
 *
 * The set is derived from two sources, and nothing is hand-typed:
 *
 *   1. the document (`cockpit/index.html`) — every `<link>`, `src`/`href`/`srcset`/`poster`/
 *      `data`/`background`/`xlink:href` and inline `url()`/`@import` a browser dereferences,
 *      classified by resolution rather than by a `scheme:` prefix (issue #20), then the module
 *      graph those scripts load and the stylesheets' own `url()`/`@import` targets walked to a
 *      fixpoint;
 *   2. the plane's committed assets — every file under `cockpit/assets/`, by the plane's own
 *      definition (the directory `scripts/cockpit-artifact.mjs` packs, minus the build product):
 *      the fixture, the field lists and the fonts are fetched by the running app, which no
 *      static walk can see, so they are declared by their directory rather than guessed file by
 *      file. A fetch position the static walk cannot name is still reported
 *      (`unresolvedFetchPositions`), never silently skipped.
 *
 * What this record does not cover, deliberately: the built module (`cockpit/pkg/**`). It is a
 * build product with no committed bytes, and a committed manifest cannot verify bytes a build
 * produces — so it ships in the plane artifact (`scripts/cockpit-artifact.mjs`), whose own
 * manifest re-derives every built file's hash from the archive at build time, and its serving
 * is asserted by `tests/deployment-serving.test.js`, `scripts/server-smoke.mjs` and the CI
 * `cockpit` job. This check needs no cargo, no wasm-pack and no browser.
 *
 * Usage — from any working directory:
 *
 *     node scripts/deploy-check.mjs                    # verify the committed manifest
 *     node scripts/deploy-check.mjs --write-manifest    # regenerate it (npm run deploy-manifest)
 *
 * Exit status: 0 the manifest matches the derived set; 1 it does not (every failure is listed,
 * naming the file); 2 the check could not run (no surface document, unreadable manifest).
 */

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

import { root, sha256 } from './bundle.mjs';

const MANIFEST = 'deploy-manifest.txt';
const SURFACE = 'cockpit';
const DOCUMENT = `${SURFACE}/index.html`;
const writeManifest = process.argv.includes('--write-manifest');
const failures = [];

const refuse = (message) => {
  console.error(`deploy-check: ${message}`);
  process.exit(2);
};

if (!existsSync(join(root, DOCUMENT)) || !statSync(join(root, DOCUMENT)).isFile()) {
  refuse(`${DOCUMENT} is missing; the deploy set cannot be derived from a surface that is not here`);
}

const html = readFileSync(join(root, DOCUMENT), 'utf8');
const documentDir = dirname(DOCUMENT);

/* ---- the document: every fetch it makes, and the stylesheets' own targets ---- */

// Stylesheets are fetches the document itself makes, so they are derived from the document: a
// newly linked sheet must join the served set on its own, rather than depending on a literal
// list that goes stale the moment someone adds a <link>. Paths normalise relative to the
// document's own directory, as module specifiers do.
//
// A canonical pattern is not enough to do that. HTML writes the same fetch several ways —
// attribute order, quote style, tag and attribute case, `rel` as a token list — and every
// spelling the pattern did not recognise was skipped in silence: the sheet stayed out of the
// served set, a copy-deploy omitted it, and the gate exited 0 (issue #20). Every `<link>` tag is
// therefore parsed and classified below, and a stylesheet link whose href does not resolve to a
// repository file fails by name instead of being skipped. A file whose name cannot be read
// (`./x.css#top` is `./x.css`) is never a reason to drop the link.
const linkTags = [...html.matchAll(/<link\b[^>]*>/gi)].map((match) => match[0]);

// Every `<link` opener must produce a tag that closes: a `>` outside a quoted value before the
// next `<`. An opener that runs into the markup which follows — or into the end of the document —
// can never close, and the tag pattern would otherwise swallow that markup and classify the
// absorbed text as an ordinary link, which `--write-manifest` would then bless. Report it instead
// (issue #20 F3).
const unclosedLinks = [...html.matchAll(/<link\b/gi)]
  .filter((opener) => tagClosure(html.slice(opener.index + opener[0].length)) !== 'closed').length;
if (unclosedLinks > 0) {
  failures.push(`${DOCUMENT} has ${unclosedLinks} <link> tag(s) that never close; an unreadable link must not be skipped`);
}

/**
 * Where a `<link …` tag ends, scanning the text that follows its opener: a `>` outside a quoted
 * value closes it, a `<` outside one means it ran into following markup instead, and neither
 * means the text ends first.
 */
function tagClosure(text) {
  let quote = null;
  for (const character of text) {
    if (quote) {
      if (character === quote) quote = null;
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === '>') {
      return 'closed';
    } else if (character === '<') {
      return 'markup';
    }
  }
  return 'eof';
}

/**
 * Parse a `<link …>` tag as HTML may legally write it: case-insensitive names, single, double or
 * absent quotes, any attribute order, `/` or whitespace before the closing `>`. A tag the parser
 * cannot read with certainty — an unterminated quoted value, a repeated attribute — returns an
 * error rather than a guess; the caller refuses it instead of skipping it.
 */
/** The attributes of a tag body, quoted values unwrapped; an error when one cannot be read. */
function parseAttributes(body) {
  const attributes = new Map();
  const pattern = /([^\s=/>]+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s>]+))?/g;
  let match;
  while ((match = pattern.exec(body)) !== null) {
    const name = match[1].toLowerCase();
    const raw = match[2];
    let value = '';
    if (raw !== undefined) {
      if (raw[0] === '"' || raw[0] === "'") {
        if (raw.length < 2 || raw.at(-1) !== raw[0]) return { error: `${name} has an unterminated ${raw[0]} quote` };
        value = raw.slice(1, -1);
      } else {
        value = raw;
      }
    }
    if (attributes.has(name)) return { error: `${name} appears twice` };
    attributes.set(name, value);
  }
  return { attributes };
}

function parseLinkTag(tag) {
  const body = tag.replace(/^<link\b/i, '').replace(/\/?\s*>$/, '');
  // A `<` outside a quoted value means the tag never closed and the pattern absorbed the markup
  // that follows it: refuse it rather than classifying the absorbed text as attributes (F3).
  if (tagClosure(body) === 'markup') return { error: 'the tag runs into the markup that follows it' };
  return parseAttributes(body);
}

// A `<link>` that makes the browser fetch a file — a stylesheet, a `modulepreload`ed module — must
// be answered by the served set, and "external" is decided by RESOLUTION, not by a `scheme:`
// prefix. The WHATWG parser applies slash tolerance to special schemes, so `https:/x.css` (one
// slash, uppercase scheme, backslashes, `https:/./…`) resolves to the document's own origin and
// the browser then fetches `x.css` from it; a prefix test waved the sheet through while a
// copy-deploy omitted it (issue #20 F1). The document's own origin is not known statically, so a
// sentinel document URL supplies one: a canonical absolute URL (`scheme://host/…`) carries an
// authority of its own and stays external, and so does any non-HTTP scheme; a special-scheme URL
// without an authority resolves against the sentinel, and its path maps back onto the repository.
// Both fetch kinds go through this one classifier, so the sheet and script checks cannot drift
// apart again (issue #20 F2).
function classifyHref(href) {
  const scheme = href.match(/^([a-z][a-z0-9+.-]*):/i)?.[1]?.toLowerCase();
  if (scheme === undefined) {
    if (href.startsWith('//')) return { external: true };
    // A root-relative URL resolves against wherever the copy is served from, not against the
    // repository: which file it fetches is not knowable here, so it is refused, not guessed.
    if (href.startsWith('/')) return { refuse: 'the URL is root-relative, so it names no repository file' };
    // A fragment or query is not part of the file name: `./x.css#top` is `./x.css`.
    const cut = href.search(/[?#]/);
    return { path: cut === -1 ? href : href.slice(0, cut) };
  }
  if (scheme !== 'http' && scheme !== 'https') return { external: true };
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(href)) return { external: true };
  const sentinel = `${scheme}://deploy.invalid`;
  let resolved;
  try {
    resolved = new URL(href, `${sentinel}/index.html`);
  } catch (error) {
    return { refuse: `the URL cannot be resolved: ${href}` };
  }
  if (resolved.origin !== sentinel) {
    return { refuse: `the URL resolves to a different origin (${resolved.origin}): ${href}` };
  }
  return { path: resolved.pathname.replace(/^\//, '') };
}

/**
 * The URLs in a `srcset`-style candidate list (issue #20 fix round 4): comma-separated entries,
 * each a URL plus an optional descriptor (`1x`, `2x`, `640w`). Follows the HTML "parse a srcset"
 * algorithm closely enough for a gate — a candidate URL runs to the next whitespace and trailing
 * commas end it (so `a.png,b.png` without whitespace is ONE URL, exactly the file a browser
 * requests — Chromium-witnessed), descriptors are skipped, and a comma inside parentheses does
 * not split an entry. Every candidate is returned, not just the first: which one a browser picks
 * depends on its viewport and pixel density, and a `srcset` with no `src` fallback is exactly
 * the case that ships broken images on hi-dpi screens with every gate green.
 */
function srcsetUrls(value) {
  const urls = [];
  let index = 0;
  const isWhitespace = (character) => character === ' ' || character === '\t' || character === '\n' || character === '\r' || character === '\f';
  while (index < value.length) {
    while (index < value.length && (isWhitespace(value[index]) || value[index] === ',')) index += 1;
    if (index >= value.length) break;
    const start = index;
    while (index < value.length && !isWhitespace(value[index])) index += 1;
    let url = value.slice(start, index);
    if (url.endsWith(',')) {
      url = url.replace(/,+$/, '');
      if (url !== '') urls.push(url);
      continue;
    }
    if (url !== '') urls.push(url);
    // Skip this candidate's descriptor: to the next top-level comma, which begins the next one.
    let parentheses = 0;
    while (index < value.length) {
      const character = value[index];
      if (character === '(') parentheses += 1;
      else if (character === ')') parentheses = Math.max(0, parentheses - 1);
      else if (character === ',' && parentheses === 0) break;
      index += 1;
    }
  }
  return urls;
}

/** A CSS string token starting at its quote: the value inside it and where the token ends. */
function readCssString(source, start) {
  const quote = source[start];
  let index = start + 1;
  let value = '';
  while (index < source.length) {
    const character = source[index];
    if (character === '\\') { value += source[index + 1] ?? ''; index += 2; continue; }
    if (character === quote) return { value, index: index + 1 };
    if (character === '\n') return { value, index };
    value += character;
    index += 1;
  }
  return { value, index: source.length };
}

/** A `url(…)` token: the value it carries (quoted or raw) and where the token ends. */
function readCssUrl(source, start) {
  let index = start;
  while (index < source.length && /\s/.test(source[index])) index += 1;
  if (source[index] === '"' || source[index] === "'") {
    const parsed = readCssString(source, index);
    index = parsed.index;
    while (index < source.length && source[index] !== ')') index += 1;
    return { value: parsed.value, index: index + 1 };
  }
  let value = '';
  while (index < source.length && source[index] !== ')') { value += source[index]; index += 1; }
  return { value: value.trim(), index: index + 1 };
}

/**
 * The fetch references a piece of CSS carries (issue #20 fix round 4): every `url(…)` token and,
 * unless `atImports` is off, every `@import` target in its bare-string spelling. Comment- and
 * string-aware, because a browser is: `url()` inside a comment is text, and `content: "url(x)"`
 * is a quoted string that fetches nothing. `@import "x.css";` carries no `url(` to find, so the
 * at-rule is read for both spellings; the `@import url(…)` spelling is found by the `url(` scan.
 */
function cssReferences(source, { atImports = true } = {}) {
  const references = [];
  let index = 0;
  const tokenBoundary = (position) => position === 0 || !/[a-z0-9_-]/i.test(source[position - 1]);
  while (index < source.length) {
    const character = source[index];
    if (character === '/' && source[index + 1] === '*') {
      const end = source.indexOf('*/', index + 2);
      index = end === -1 ? source.length : end + 2;
      continue;
    }
    if (character === '"' || character === "'") { index = readCssString(source, index).index; continue; }
    if (tokenBoundary(index) && /^url\(/i.test(source.slice(index, index + 4))) {
      const parsed = readCssUrl(source, index + 4);
      if (parsed.value !== '') references.push(parsed.value);
      index = parsed.index;
      continue;
    }
    if (atImports && tokenBoundary(index) && /^@import/i.test(source.slice(index, index + 7))) {
      let cursor = index + 7;
      while (cursor < source.length && /\s/.test(source[cursor])) cursor += 1;
      if (source[cursor] === '"' || source[cursor] === "'") {
        const parsed = readCssString(source, cursor);
        if (parsed.value !== '') references.push(parsed.value);
        index = parsed.index;
        continue;
      }
      index = cursor;
      continue;
    }
    index += 1;
  }
  return references;
}

/**
 * Demand one subresource reference a browser dereferences, whether it is written as an attribute
 * value, a `srcset` candidate, a CSS `url()`/`@import` target (issue #20 fix round 4) or a fetch
 * target a surface module names: the file must exist, and it must join the served set — the
 * manifest check below names it otherwise, and `npm run deploy-manifest` records it deliberately.
 * `base` is the directory of the file that carries the reference (the document's own directory,
 * or the sheet's/module's own directory — each resolves its relative URLs against itself);
 * `source` labels a failure. Returns the file when it was demanded.
 *
 * The exclusions match the reference sweep's: a foreign origin by any spelling, a non-HTTP
 * scheme, a protocol-relative URL, and a root-relative URL — whose target the copy's mount point
 * decides — name no repository file here. A `scheme:` prefix is not an origin test (fix round 1):
 * the slash-tolerant same-origin spellings (`https:/x.png`) route through `classifyHref` and are
 * demanded exactly as the browser fetches them, and a reference that resolves to a foreign origin
 * is another origin, not a file of this tree.
 */
function demandReference(rawValue, { base, source } = {}) {
  const value = rawValue.trim();
  if (value === '') return undefined;
  if (value.startsWith('//') || value.startsWith('/')) return undefined;
  const scheme = value.match(/^([a-z][a-z0-9+.-]*):/i)?.[1]?.toLowerCase();
  let repositoryPath;
  if (scheme === undefined) {
    const cut = value.search(/[?#]/);
    // A fragment-only reference (`url(#gradient)`) names nothing to fetch.
    const relativeReference = cut === -1 ? value : value.slice(0, cut);
    if (relativeReference === '') return undefined;
    repositoryPath = join(base ?? '', relativeReference);
  } else {
    if (scheme !== 'http' && scheme !== 'https') return undefined;
    const classified = classifyHref(value);
    if (classified.external || classified.refuse) return undefined;
    repositoryPath = classified.path;
  }
  const normalised = relative(root, resolve(root, repositoryPath));
  if (normalised === '' || normalised === '..' || normalised.startsWith('../')) {
    failures.push(`${source} references a path outside the repository: ${value}`);
    return undefined;
  }
  if (!existsSync(join(root, normalised))) {
    failures.push(`${source} references a missing file: ${normalised}`);
    return undefined;
  }
  demanded.push(normalised);
  return normalised;
}

// Which relations make the browser dereference the href. The list is enumerated deliberately:
// admitting tokens one at a time is how `preload` slipped past the sheet and modulepreload checks
// (issue #20 fix round 2, G1), and D13 records both this list and the relations that are
// deliberately not fetches, so the boundary can be attacked as a list. The export is the
// suite's handle: its parity case imports this module and reads this set, not the source text,
// so a relation that joins the set by ANY means is covered (fix round 4, G2).
export const FETCHING_RELATIONS = new Set([
  'stylesheet', 'modulepreload', 'preload', 'prefetch', 'prerender', 'manifest',
  'compression-dictionary',
  'icon', 'apple-touch-icon', 'apple-touch-icon-precomposed', 'mask-icon',
]);

// The HTML spec's `as` destinations: a `preload` without one of these is not a preload link and
// nothing is requested, so a missing `as` is not a fetch; an unrecognized one is refused rather
// than assumed harmless, because the destination list can grow.
const PRELOAD_DESTINATIONS = new Set([
  'audio', 'audioworklet', 'document', 'embed', 'fetch', 'font', 'image', 'json', 'manifest',
  'object', 'paintworklet', 'report', 'script', 'serviceworker', 'sharedworker', 'style',
  'track', 'video', 'webidentity', 'worker', 'xslt',
]);

/** The relations of this tag that fetch the href, or a refusal when one cannot be classified. */
function fetchedKinds(rel, attributes) {
  const kinds = new Set(rel.filter((token) => token !== 'preload' && FETCHING_RELATIONS.has(token)));
  if (!rel.includes('preload')) return { kinds: [...kinds] };
  const as = attributes.get('as');
  if (as === undefined) return { kinds: [...kinds] };
  const destination = as.trim().toLowerCase();
  if (!PRELOAD_DESTINATIONS.has(destination)) {
    return { refuse: `preload destination as="${as}" is not one the browser requests` };
  }
  kinds.add('preload');
  return { kinds: [...kinds] };
}

const demanded = [];
const modulepreloads = new Set();
for (const tag of linkTags) {
  const parsed = parseLinkTag(tag);
  if (parsed.error) {
    failures.push(`${DOCUMENT} has a <link> tag that cannot be parsed (${parsed.error}): ${tag}`);
    continue;
  }
  const rel = (parsed.attributes.get('rel') ?? '').toLowerCase().split(/[\s,]+/);
  const fetched = fetchedKinds(rel, parsed.attributes);
  if (fetched.refuse) {
    failures.push(`${DOCUMENT} has a <link> that cannot be classified (${fetched.refuse}): ${tag}`);
    continue;
  }
  if (fetched.kinds.length === 0) continue;
  // A preload's `imagesrcset` is a candidate list of its own (issue #20 fix round 4). When the
  // destination is an image, the browser loads a candidate from it and never the href
  // (Chromium-witnessed: the candidate is fetched, a present href is not), so each candidate is
  // demanded by the reference rule and a preload that carries candidates needs no href. In any
  // other spelling the attribute is inert and the href stays what is fetched.
  const imagesrcset = parsed.attributes.get('imagesrcset');
  const imageCandidates = imagesrcset === undefined ? [] : srcsetUrls(imagesrcset);
  const candidatesAreTheFetch = rel.includes('preload')
    && (parsed.attributes.get('as') ?? '').trim().toLowerCase() === 'image'
    && imageCandidates.length > 0;
  if (candidatesAreTheFetch) {
    for (const candidate of imageCandidates) demandReference(candidate, { base: documentDir, source: DOCUMENT });
    // A second fetching relation on the same tag (e.g. `rel="preload stylesheet"`) still owns
    // the href; a plain preload does not.
    if (fetched.kinds.every((kind) => kind === 'preload') || parsed.attributes.get('href') === undefined) continue;
  }
  const refuse = (reason) => failures.push(
    `${DOCUMENT} has a ${fetched.kinds[0]} <link> that cannot be classified (${reason}): ${tag}`
  );
  const href = parsed.attributes.get('href');
  if (href === undefined) {
    refuse('no href');
    continue;
  }
  const classified = classifyHref(href);
  if (classified.external) continue;
  if (classified.refuse) {
    refuse(classified.refuse);
    continue;
  }
  // The href resolves against the document: a relative one against the document's own directory,
  // and a slash-tolerant same-origin spelling (`https:/x.css`) against the served root, which is
  // that same directory — the document is the site root of the copy (the private deployment procedure §3).
  const normalised = classified.path === '' ? '' : relative(root, resolve(root, documentDir, classified.path));
  if (normalised === '' || normalised === '..' || normalised.startsWith('../')) {
    refuse(`the URL names no repository file: ${href}`);
    continue;
  }
  const absolute = join(root, normalised);
  if (existsSync(absolute) && !statSync(absolute).isFile()) {
    refuse(`the URL resolves to a directory, not a file: ${href}`);
    continue;
  }
  if (existsSync(absolute)) demanded.push(normalised);
  else failures.push(`${DOCUMENT} references a missing file: ${normalised}`);
  if (fetched.kinds.includes('modulepreload')) modulepreloads.add(normalised);
}

// Every `src`, every `href` on a tag that is not a hyperlink, and every other attribute or
// syntax a browser dereferences, is a fetch the copy must satisfy: the file has to exist AND it
// has to join the served set, exactly like a linked sheet. The manifest verification below
// reports the served-set half for everything demanded here, and `npm run deploy-manifest`
// records a deliberate change (issue #20 fix round 3, G1: the old sweep reported only files that
// did not exist at all, so a fetched file that shipped not stayed green).
//
// Fix round 4 (G1) closes the shapes that sweep could not see, every one of them browser-fetched
// and previously exiting 0 with the file on disk and absent from the manifest: `srcset` candidate
// lists (EVERY candidate — a browser picks one by viewport and density, and a `srcset` with no
// `src` fallback is how images break on hi-dpi screens with all gates green), a `poster` on
// `<video>`, `data` on `<object>`, the obsolete `background` on `<body>`, `<table>`, `<td>` and
// `<th>`, and the SVG `xlink:href`. Bare relative references (`x.js`, no `./`) resolve against
// the document exactly as `./x.js` does and are covered. `<a>`, `<area>`, `<base>` and `<form>`
// are navigation or resolution rather than fetches, and `<link>` is classified above from its
// relation — demanding its href here too would report the same file twice. One rule, one
// resolver: every reference goes through `demandReference`, so the exclusions and the
// slash-tolerant same-origin spellings cannot drift apart between attribute names.
const NON_FETCHING_TAGS = new Set(['a', 'area', 'base', 'form', 'link']);
// Attributes that name a fetch only on the tag that dereferences them (Chromium-witnessed for
// each; `poster` on any other tag and `data` outside `<object>` are inert).
const TAG_REFERENCE_ATTRIBUTES = new Map([
  ['video', ['poster']],
  ['object', ['data']],
  ['body', ['background']],
  ['table', ['background']],
  ['td', ['background']],
  ['th', ['background']],
]);
for (const tag of html.matchAll(/<([a-z][^\s/>]*)\b[^>]*>/gi)) {
  const tagName = tag[1].toLowerCase();
  if (NON_FETCHING_TAGS.has(tagName)) continue;
  const body = tag[0].replace(/^<[^\s/>]+/, '').replace(/\/?\s*>$/, '');
  const parsed = parseAttributes(body);
  if (parsed.error) continue;
  const referenceAttributes = ['src', 'href', 'xlink:href', ...(TAG_REFERENCE_ATTRIBUTES.get(tagName) ?? []), 'srcset'];
  for (const attribute of referenceAttributes) {
    const value = parsed.attributes.get(attribute);
    if (value === undefined) continue;
    const references = attribute === 'srcset' ? srcsetUrls(value) : [value];
    for (const reference of references) demandReference(reference, { base: documentDir, source: DOCUMENT });
  }
}

// The document's own styling fetches too: a `style` attribute and a `<style>` block carry
// `url()`/`@import` references that no src/href sweep can see (issue #20 fix round 4, G1/F3 —
// the gate never read a `.css` file at all, so stylesheet targets were unreachable by
// construction). Both inline forms resolve against the document and are demanded here; the
// served stylesheets are walked below, once the served set is known, because a sheet resolves
// its relative URLs against itself. A `<style>` block with no closing tag swallows the rest of
// the document as CSS in a browser, so the same text is scanned here rather than skipped.
for (const tag of html.matchAll(/<([a-z][^\s/>]*)\b[^>]*>/gi)) {
  const body = tag[0].replace(/^<[^\s/>]+/, '').replace(/\/?\s*>$/, '');
  const parsed = parseAttributes(body);
  if (parsed.error) continue;
  const style = parsed.attributes.get('style');
  if (style === undefined) continue;
  for (const reference of cssReferences(style, { atImports: false })) demandReference(reference, { base: documentDir, source: DOCUMENT });
}
for (const opener of html.matchAll(/<style\b[^>]*>/gi)) {
  const start = opener.index + opener[0].length;
  const close = html.indexOf('</style', start);
  const block = close === -1 ? html.slice(start) : html.slice(start, close);
  for (const reference of cssReferences(block)) demandReference(reference, { base: documentDir, source: DOCUMENT });
}

/* ---- the surface's own modules and the module graph they load ---- */

const isModule = (path) => path.endsWith('.js') || path.endsWith('.mjs');

/**
 * The text of a module with every comment's contents blanked out (same length, same offsets), so
 * a `fetch('./x')` written in a comment is text and not a fetch, while a position's string
 * argument — which lives in code — survives to be read. String and template tokens are copied
 * verbatim: they are where fetch targets are written.
 */
function withoutComments(source) {
  const characters = [...source];
  let index = 0;
  const inString = (quote) => quote === '"' || quote === "'" || quote === '`';
  while (index < source.length) {
    const character = source[index];
    if (inString(character)) {
      index += 1;
      while (index < source.length) {
        if (source[index] === '\\') { index += 2; continue; }
        if (source[index] === character) { index += 1; break; }
        index += 1;
      }
      continue;
    }
    if (character === '/' && source[index + 1] === '/') {
      while (index < source.length && source[index] !== '\n') { characters[index] = ' '; index += 1; }
      continue;
    }
    if (character === '/' && source[index + 1] === '*') {
      const end = source.indexOf('*/', index + 2);
      const stop = end === -1 ? source.length : end + 2;
      while (index < stop) { if (source[index] !== '\n') characters[index] = ' '; index += 1; }
      continue;
    }
    index += 1;
  }
  return characters.join('');
}

/**
 * The string values a module binds to a name at the top level of a line: `const NAME = 'literal';`.
 * A name bound more than once is not resolvable — the value a fetch position would use is not the
 * one this scan read — so it is dropped rather than guessed.
 */
function literalConstants(source) {
  const bindings = new Map();
  for (const match of source.matchAll(/(?:^|\n)\s*const\s+([A-Za-z_$][\w$]*)\s*=\s*(["'])([^"'\n]*)\2\s*;/g)) {
    const [, name, , value] = match;
    bindings.set(name, bindings.has(name) ? undefined : value);
  }
  return new Map([...bindings].filter(([, value]) => value !== undefined));
}

/**
 * The fetch positions a module carries: every `import(…)` and `fetch(…)` call site, with its
 * argument read to the first `,` or `)`. `import.meta`/`.fetch(` spells a property, not a call,
 * and is left alone. Each position is classified here: a literal argument, or a name this module
 * binds to a single literal, becomes a target demanded by the same rule as every other reference;
 * anything else — a parameter, a concatenation, a helper's own `fetch(url)` — is a position whose
 * target cannot be named statically and is reported, never silently skipped.
 */
function fetchPositions(source) {
  const constants = literalConstants(source);
  const positions = [];
  const pattern = /(?:^|[^\w$.])import\s*\(([^),]*)|(?:^|[^\w$.])fetch\s*\(([^),]*)/g;
  let match;
  while ((match = pattern.exec(source)) !== null) {
    const written = (match[1] ?? match[2]).trim();
    let target;
    const literal = written.match(/^(["'])([^"'\n]*)\1$/);
    if (literal) target = literal[2];
    else if (/^[A-Za-z_$][\w$]*$/.test(written) && constants.has(written)) target = constants.get(written);
    const line = source.slice(0, match.index + match[0].length).split('\n').length;
    positions.push({ written, target, line });
  }
  return positions;
}

/**
 * Is this one of the surface's own modules? The plane's own files are ours to scan for fetch
 * positions; nothing is vendored under it any more (the pinned pieces are a consumer surface's
 * business, not the plane's).
 */
const isSurfaceModule = (path) => path.startsWith(`${SURFACE}/`);

const modulesWalked = new Set();
const unresolvedFetchPositions = [];
const moduleQueue = [...new Set(demanded)].filter(isModule);
while (moduleQueue.length > 0) {
  const moduleRelative = moduleQueue.shift();
  if (modulesWalked.has(moduleRelative)) continue;
  modulesWalked.add(moduleRelative);
  const absolute = join(root, moduleRelative);
  if (!existsSync(absolute)) continue; // the demand that queued it already named the missing file
  const source = readFileSync(absolute, 'utf8');
  // Both static import forms are real fetches: `… from './x.js'` (including `export … from`) and
  // side-effect-only `import './x.js';`, which has no `from` clause to match on. A bare specifier
  // is a package name, not a URL, so only relative specifiers are fetches of this tree.
  const fromSpecifiers = [...source.matchAll(/(?:^|\n)\s*(?:import|export)[\s\S]*?from\s+['"](\.[^'"]+)['"]/g)]
    .map((match) => match[1]);
  const sideEffectSpecifiers = [...source.matchAll(/(?:^|\n)\s*import\s+['"](\.[^'"]+)['"]/g)]
    .map((match) => match[1]);
  for (const specifier of new Set([...fromSpecifiers, ...sideEffectSpecifiers])) {
    const target = demandReference(specifier, { base: dirname(moduleRelative), source: moduleRelative });
    if (target !== undefined && isModule(target)) moduleQueue.push(target);
  }
  if (!isSurfaceModule(moduleRelative)) continue;
  for (const position of fetchPositions(withoutComments(source))) {
    if (position.target === undefined) {
      unresolvedFetchPositions.push(`${moduleRelative}:${position.line}: ${position.written}`);
      continue;
    }
    const target = demandReference(position.target, { base: dirname(moduleRelative), source: moduleRelative });
    if (target !== undefined && isModule(target)) moduleQueue.push(target);
  }
}

/* ---- the plane's committed assets, by the plane's own definition ---- */

// The running app fetches these at run time — the fixture record, the generated field lists and
// the fonts — so no static walk of the document can name them. The directory is the declaration
// (the same directory `scripts/cockpit-artifact.mjs` packs, minus the built `pkg/`), walked
// recursively and sorted, so an asset added to the plane joins the set on its own instead of
// depending on a hand-typed list that goes stale.
const ASSETS_DIR = `${SURFACE}/assets`;
const planeAssets = [];
if (!existsSync(join(root, ASSETS_DIR)) || !statSync(join(root, ASSETS_DIR)).isDirectory()) {
  failures.push(`${ASSETS_DIR} is missing; the plane's committed assets are the deploy set's asset half (run: npm run deploy-manifest)`);
} else {
  const walk = (directory) => {
    for (const entry of readdirSync(join(root, directory), { withFileTypes: true })) {
      const path = `${directory}/${entry.name}`;
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile()) planeAssets.push(path);
    }
  };
  walk(ASSETS_DIR);
  for (const path of planeAssets.sort()) demanded.push(path);
}

// The served stylesheets are fetch sources in their own right (issue #20 fix round 4, G1/F3):
// walk every `.css` the copy serves — the linked sheets, anything demanded, and whatever an
// `@import` adds — to a fixpoint and demand its `url()` and `@import` targets by the same rule.
// `base` carries the sheet's own directory: a stylesheet resolves a relative URL against itself,
// not against the document.
const pendingStylesheets = [...new Set(demanded)].filter((file) => file.endsWith('.css'));
const walkedStylesheets = new Set();
while (pendingStylesheets.length > 0) {
  const stylesheet = pendingStylesheets.shift();
  if (walkedStylesheets.has(stylesheet)) continue;
  walkedStylesheets.add(stylesheet);
  const absolute = join(root, stylesheet);
  if (!existsSync(absolute)) continue;
  for (const reference of cssReferences(readFileSync(absolute, 'utf8'))) {
    const demandedFile = demandReference(reference, { base: dirname(stylesheet), source: stylesheet });
    if (demandedFile !== undefined && demandedFile.endsWith('.css')) pendingStylesheets.push(demandedFile);
  }
}

/* ---- the derived deploy set ---- */

const derived = [...new Set([DOCUMENT, ...demanded])]
  .map((path) => {
    const absolute = join(root, path);
    // A demanded file that is not on disk has already been named; it is carried with the failure
    // that named it rather than crashing the report.
    if (!existsSync(absolute)) return { path, missing: true };
    const bytes = readFileSync(absolute);
    return { path, bytes: bytes.length, sha256: sha256(bytes) };
  })
  .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));

// Preloads only help if they name modules that are really in the graph.
for (const path of modulepreloads) {
  if (!modulesWalked.has(path)) failures.push(`modulepreload points at a module not in the graph: ${path}`);
}

/* ---- the committed manifest is verified, not regenerated ---- */

/**
 * The committed manifest, as this check writes it: one entry per line, `<path>`. Every file of
 * this deploy set is a repository file — there is nothing vendored, so a digest column would
 * look like a record this check does not make; a line carrying one is refused rather than
 * ignored (pinned bytes are `npm run bundle-check`'s record, not this one's).
 */
function parseManifest(text) {
  const entries = [];
  const seen = new Set();
  for (const line of text.split('\n')) {
    const entry = line.trim();
    if (entry === '' || entry.startsWith('#')) continue;
    const columns = entry.split(/\s{2,}/);
    if (columns.length === 1 && columns[0] !== '') {
      if (seen.has(columns[0])) return { error: `it lists ${columns[0]} more than once` };
      seen.add(columns[0]);
      entries.push({ path: columns[0] });
      continue;
    }
    if (columns.length !== 3 || !/^[0-9a-f]{64}$/.test(columns[1]) || !/^\d+$/.test(columns[2])) {
      return { error: `it has a line that is neither <path> nor <path>  <sha256>  <bytes>: ${entry}` };
    }
    if (seen.has(columns[0])) return { error: `it lists ${columns[0]} more than once` };
    seen.add(columns[0]);
    entries.push({ path: columns[0], sha256: columns[1], bytes: Number(columns[2]) });
  }
  return { entries };
}

const manifestPath = join(root, MANIFEST);
let manifestEntries = [];

if (!writeManifest) {
  if (!existsSync(manifestPath)) {
    failures.push(`${MANIFEST} is missing; generate it with "npm run deploy-manifest"`);
  } else {
    const committed = parseManifest(readFileSync(manifestPath, 'utf8'));
    if (committed.error) failures.push(`${MANIFEST}: ${committed.error}`);
    manifestEntries = committed.entries ?? [];
    const committedPaths = new Set(manifestEntries.map((entry) => entry.path));
    const committedByPath = new Map(manifestEntries.map((entry) => [entry.path, entry]));
    const missing = derived.filter((entry) => !entry.missing && !committedPaths.has(entry.path)).map((entry) => entry.path);
    const unknown = manifestEntries.filter((entry) => !derived.some((candidate) => candidate.path === entry.path)).map((entry) => entry.path);
    if (missing.length) {
      failures.push(`${MANIFEST} is missing: ${missing.join(', ')} (the plane serves these; if the set changed deliberately, run "npm run deploy-manifest")`);
    }
    if (unknown.length) {
      failures.push(`${MANIFEST} lists files the plane does not serve: ${unknown.join(', ')}`);
    }
    for (const entry of derived) {
      // A file that is not on disk was named by the derivation itself; the manifest is not the
      // thing that is wrong about it.
      if (entry.missing) continue;
      const recorded = committedByPath.get(entry.path);
      if (!recorded) continue;
      if (recorded.sha256 !== undefined) {
        // Nothing in this deploy set is vendored, so a digest column would look like a record
        // this check does not make. Pinned bytes are the pins' own record (`npm run
        // bundle-check`); a digest here is refused rather than ignored.
        failures.push(`${entry.path} — ${MANIFEST} records a digest for a file that is not vendored; this deploy set carries no pinned bytes (run "npm run deploy-manifest")`);
      }
    }
  }
}

/* ---- emit ---- */

const totalBytes = derived.reduce((sum, entry) => sum + (entry.bytes ?? 0), 0);

if (failures.length) {
  // One missing file can be reached by two checks (the stylesheet walk and the reference sweep);
  // it is reported once.
  console.error(JSON.stringify({ passed: false, failures: [...new Set(failures)] }, null, 2));
  process.exit(1);
}

// Writing is the deliberate act; the pass path never touches the committed manifest.
if (writeManifest) {
  writeFileSync(
    manifestPath,
    `# Files required for a public deployment of the cockpit plane; sorted, order is not significant.\n`
    + `# Generated by scripts/deploy-check.mjs — do not edit by hand.\n`
    + `# The plane's committed half: its document and the assets the app fetches at run time.\n`
    + `# The built module (\`pkg/**\`) is a build product with no committed bytes; it ships in the\n`
    + `# plane artifact the release publishes (\`scripts/cockpit-artifact.mjs\`), whose own manifest\n`
    + `# re-derives every built file's hash at build time. Verification input is the pins\n`
    + `# (\`bundles/*.manifest.json\`), checked by \`npm run bundle-check\`.\n`
    + `\n`
    + `${derived.map((entry) => entry.path).join('\n')}\n`
  );
}

console.log(JSON.stringify({
  passed: true,
  surface: SURFACE,
  document: DOCUMENT,
  fileCount: derived.length,
  totalBytes,
  assets: planeAssets.length,
  files: derived.map((entry) => (entry.missing ? `not served  —  ${entry.path}` : `${entry.sha256}  ${entry.bytes}  ${entry.path}`)),
  unresolvedFetchPositions,
  manifest: MANIFEST,
  ...(writeManifest ? { manifestWritten: true } : { manifestVerified: true, manifestEntries: manifestEntries.length })
}, null, 2));
