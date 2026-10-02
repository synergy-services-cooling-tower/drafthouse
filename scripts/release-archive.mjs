#!/usr/bin/env node
/**
 * The release container — issue #36, slice 1 of #35.
 *
 * The archive a release carries is built here, not by the host: one POSIX ustar entry per
 * file, sorted by path, with every host-dependent field normalised away — mode 0644, uid
 * and gid 0, mtime 0, empty uname/gname — gzipped by this process (zlib, level 9, no
 * filename and no mtime in the gzip header). No host `tar` and no host `gzip` runs, so two
 * builds of the same commit on the same host produce the same archive bytes, and the
 * release workflow can measure that instead of claiming it.
 *
 * The reader lives beside the writer on purpose: the fresh-download verification parses the
 * published archive with the same layout it was written with, and refuses anything this
 * writer did not produce — an unknown typeflag, a link, a bad header checksum, a duplicate
 * path, trailing bytes. A reader that tolerated extra layout would verify bytes it did not
 * read.
 *
 * Usage — a library, not a command: `scripts/release-assemble.mjs` writes archives,
 * `scripts/release-verify.mjs` reads them, `scripts/release-tamper.mjs` edits one payload
 * byte for the pipeline's bite demonstration.
 */
import { createHash } from 'node:crypto';
import { gunzipSync, gzipSync } from 'node:zlib';

/** Tar is a block format: every header and every payload is padded to this many bytes. */
export const BLOCK_SIZE = 512;
/** The gzip level the archive is written with. Level 9, no header name, no header mtime. */
export const GZIP_LEVEL = 9;

export function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/** An octal numeric tar field: zero-padded digits, width - 1 characters, then NUL. */
function octalField(value, width) {
  const text = value.toString(8);
  if (text.length > width - 1) throw new Error(`tar: ${value} does not fit in a ${width}-byte octal field`);
  return `${text.padStart(width - 1, '0')}\0`;
}

/** ustar splits a long path at a slash: name holds the tail, prefix the head. */
function splitPath(path) {
  if (Buffer.byteLength(path, 'utf8') <= 100) return { name: path, prefix: '' };
  for (let at = path.lastIndexOf('/'); at > 0; at = path.lastIndexOf('/', at - 1)) {
    const prefix = path.slice(0, at);
    const name = path.slice(at + 1);
    if (Buffer.byteLength(name, 'utf8') <= 100 && Buffer.byteLength(prefix, 'utf8') <= 155) return { name, prefix };
  }
  throw new Error(`tar: ${path} cannot be split into a ustar name/prefix pair`);
}

/**
 * A file entry's header, written field by field. `size` is the payload's byte length; the
 * checksum is computed with the checksum field blanked to spaces, as POSIX requires.
 */
function header(path, size) {
  const { name, prefix } = splitPath(path);
  const buffer = Buffer.alloc(BLOCK_SIZE);
  const put = (text, offset, length) => buffer.write(text, offset, length, 'latin1');
  put(name, 0, 100);
  put(octalField(0o644, 8), 100, 8);
  put(octalField(0, 8), 108, 8);
  put(octalField(0, 8), 116, 8);
  put(octalField(size, 12), 124, 12);
  put(octalField(0, 12), 136, 12);
  put('        ', 148, 8);
  put('0', 156, 1);
  put('ustar\0', 257, 6);
  put('00', 263, 2);
  put(prefix, 345, 155);
  const sum = buffer.reduce((total, byte) => total + byte, 0);
  put(`${sum.toString(8).padStart(6, '0')}\0 `, 148, 8);
  return buffer;
}

/**
 * An archive over `files` — `{path, bytes}` with repository-relative paths — sorted by path
 * ascending, so the entry order does not depend on the order the caller passed them in.
 * Returns the gzipped bytes.
 */
export function writeArchive(files) {
  const sorted = [...files].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  const seen = new Set();
  const blocks = [];
  for (const file of sorted) {
    if (typeof file.path !== 'string' || file.path === '' || file.path.startsWith('/') || file.path.includes('\0')) {
      throw new Error(`tar: refusing the path ${JSON.stringify(file.path)}`);
    }
    if (file.path.split('/').some((part) => part === '' || part === '.' || part === '..')) {
      throw new Error(`tar: refusing the path ${JSON.stringify(file.path)}: it is not a plain relative path`);
    }
    if (seen.has(file.path)) throw new Error(`tar: ${file.path} is listed twice`);
    seen.add(file.path);
    const bytes = Buffer.isBuffer(file.bytes) ? file.bytes : Buffer.from(file.bytes);
    blocks.push(header(file.path, bytes.length), bytes);
    const padding = (BLOCK_SIZE - (bytes.length % BLOCK_SIZE)) % BLOCK_SIZE;
    if (padding) blocks.push(Buffer.alloc(padding));
  }
  return gzipSync(Buffer.concat([...blocks, Buffer.alloc(BLOCK_SIZE * 2)]), { level: GZIP_LEVEL });
}

/** A numeric tar field back to a number: leading zeros, NUL or space terminated. */
function readOctal(field, what) {
  const text = field.toString('latin1').replace(/\0/g, '').trim();
  if (!/^[0-7]*$/.test(text)) throw new Error(`tar: ${what} is not an octal number: ${JSON.stringify(text)}`);
  return text === '' ? 0 : parseInt(text, 8);
}

/**
 * The entries of a gzipped ustar archive this module wrote, in file order. Throws on
 * anything else — a bad gzip stream, a header checksum that does not match, a typeflag
 * other than a plain file, a payload that runs past the buffer, trailing bytes.
 */
export function readArchive(gzipped) {
  const buffer = gunzipSync(gzipped);
  const entries = [];
  const seen = new Set();
  let offset = 0;
  while (offset + BLOCK_SIZE <= buffer.length) {
    const headerAt = buffer.subarray(offset, offset + BLOCK_SIZE);
    if (headerAt.every((byte) => byte === 0)) break;
    if (headerAt.toString('latin1', 257, 263) !== 'ustar\0') throw new Error(`tar: entry at ${offset} is not ustar`);
    const stored = readOctal(headerAt.subarray(148, 156), 'header checksum');
    const blanked = Buffer.from(headerAt);
    blanked.fill(0x20, 148, 156);
    const computed = blanked.reduce((total, byte) => total + byte, 0);
    if (stored !== computed) throw new Error(`tar: entry at ${offset} has checksum ${stored}, computed ${computed}`);
    const typeflag = headerAt.toString('latin1', 156, 157);
    if (typeflag !== '0' && typeflag !== '\0') throw new Error(`tar: entry at ${offset} has typeflag ${JSON.stringify(typeflag)}; only plain files are written here`);
    if (headerAt.toString('latin1', 157, 257).replace(/\0/g, '') !== '') throw new Error(`tar: entry at ${offset} carries a link name`);
    const name = headerAt.toString('utf8', 0, 100).replace(/\0.*$/s, '');
    const prefix = headerAt.toString('utf8', 345, 500).replace(/\0.*$/s, '');
    const path = prefix === '' ? name : `${prefix}/${name}`;
    if (path === '' || path.startsWith('/')) throw new Error(`tar: entry at ${offset} has no usable path`);
    if (seen.has(path)) throw new Error(`tar: ${path} appears twice`);
    seen.add(path);
    const size = readOctal(headerAt.subarray(124, 136), `${path} size`);
    const dataAt = offset + BLOCK_SIZE;
    if (dataAt + size > buffer.length) throw new Error(`tar: ${path} claims ${size} bytes past the end of the archive`);
    entries.push({ path, bytes: Buffer.from(buffer.subarray(dataAt, dataAt + size)) });
    offset = dataAt + Math.ceil(size / BLOCK_SIZE) * BLOCK_SIZE;
  }
  const trailing = buffer.subarray(offset);
  if (trailing.length < BLOCK_SIZE * 2 || trailing.length % BLOCK_SIZE !== 0 || !trailing.every((byte) => byte === 0)) {
    throw new Error('tar: the archive does not end in the two zero blocks a complete archive ends in');
  }
  return entries;
}
