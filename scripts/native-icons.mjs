#!/usr/bin/env node
/**
 * The installer icon set — issue #72.
 *
 * The mark is a **neutral, non-branded engineering glyph**: a hyperbolic cooling tower on a
 * base plate, in three neutral greys, with no lettering and no company name. It exists in this
 * repository because a packaged desktop application needs an icon on every platform, and it is
 * neutral because of the recorded rule (the private decision record D4b, D23): branding is injected at
 * the deployment boundary, never baked into a published PolyForm Noncommercial product. A *branded* mark is
 * the owner's call and would replace this one in the same way — edit `cockpit/icons/mark.svg`.
 *
 * `cockpit/icons/mark.svg` is the source. This script derives the icon set from it, dependency
 * free, so the set is reproducible on any host the repository's gates run on:
 *
 *     cockpit/icons/32x32.png        32 px
 *     cockpit/icons/128x128.png     128 px
 *     cockpit/icons/128x128@2x.png  256 px   (the `@2x` stem is what `cargo-packager` reads as
 *                                            density 2 when it packs the macOS .icns)
 *     cockpit/icons/icon.png        512 px
 *     cockpit/icons/icon.ico        16/32/48/64/128/256 px PNG payloads (Windows)
 *
 * The macOS `.icns` is **not** committed: `cargo-packager` builds one from the PNGs
 * (`util::create_icns_file`), so the sizes above are chosen to map onto ICNS types.
 *
 * Usage — from any working directory:
 *
 *     node scripts/native-icons.mjs           # regenerate every file from the mark
 *     node scripts/native-icons.mjs --check   # re-derive and compare; never writes
 *
 * The SVG parser reads the three path commands the mark uses (M, L, C, Z — absolute only) and
 * refuses anything else; the rasteriser fills with the even-odd rule at 4x vertical
 * supersampling with analytic horizontal coverage. `--check` compares **pixels**, decoded from
 * the committed files, not container bytes: the PNG container's deflate output is a zlib
 * property, and a gate that went red on a zlib upgrade would be a gate nobody trusts. A change
 * to the mark's geometry, colours or sizes still fails it, naming the file and the difference.
 *
 * Exit status: 0 written, or checked and identical; 1 a file disagrees with the mark (every
 * difference is named on stderr as JSON); 2 the check could not run (no mark, unreadable file,
 * a mark the parser refuses).
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { deflateSync, inflateSync } from 'node:zlib';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const ICON_DIR = join('cockpit', 'icons');
const MARK = join(ICON_DIR, 'mark.svg');
const ICO_FILE = join(ICON_DIR, 'icon.ico');
const PNG_SET = [
  { file: '32x32.png', px: 32 },
  { file: '128x128.png', px: 128 },
  { file: '128x128@2x.png', px: 256 },
  { file: 'icon.png', px: 512 },
];
const ICO_SIZES = [16, 32, 48, 64, 128, 256];
const CURVE_SEGMENTS = 32;
const SUPERSAMPLE = 4;

function refuse(message) {
  console.error(`native-icons: ${message}`);
  process.exit(2);
}

/** The mark, as shapes: one entry per `<path>`, in document order (later shapes paint over). */
function parseMark() {
  if (!existsSync(join(root, MARK))) refuse(`${MARK} is missing — it is the icon set's source`);
  const svg = readFileSync(join(root, MARK), 'utf8');

  const viewBox = /viewBox\s*=\s*"([^"]+)"/.exec(svg);
  if (!viewBox) refuse(`${MARK} carries no viewBox`);
  const box = viewBox[1].trim().split(/[\s,]+/).map(Number);

  const shapes = [];
  for (const tag of svg.match(/<path\b[^>]*>/g) ?? []) {
    const d = /\bd\s*=\s*"([^"]*)"/.exec(tag);
    const fill = /\bfill\s*=\s*"([^"]*)"/.exec(tag);
    if (!d || !fill) refuse(`${MARK}: a <path> without both a d and a fill is not something this generator can draw:\n  ${tag}`);
    shapes.push({ fill: parseColour(fill[1]), polys: pathsToPolygons(d[1]) });
  }
  if (shapes.length === 0) refuse(`${MARK} holds no <path> to draw`);
  return { box, shapes };
}

function parseColour(value) {
  const hex = /^#([0-9a-fA-F]{6})$/.exec(value.trim());
  if (!hex) refuse(`the fill ${JSON.stringify(value)} is not a #rrggbb colour; the mark is drawn in flat neutrals`);
  const n = Number.parseInt(hex[1], 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** `d` -> one or more closed polygons, curves flattened. M, L, C and Z only, absolute only. */
function pathsToPolygons(d) {
  const tokens = [];
  const pattern = /([MLCZ])|(-?\d+(?:\.\d+)?)/g;
  for (const match of d.matchAll(pattern)) tokens.push(match[1] ?? Number(match[2]));

  const polygons = [];
  let current = [];
  let at = 0;
  const number = (index) => {
    const value = tokens[index];
    if (typeof value !== 'number' || !Number.isFinite(value)) {
      refuse(`the path data ${JSON.stringify(d.trim())} is not M/L/C/Z with absolute coordinates`);
    }
    return value;
  };
  while (at < tokens.length) {
    const command = tokens[at];
    if (typeof command !== 'string') refuse(`the path data ${JSON.stringify(d.trim())} has coordinates before a command letter`);
    at += 1;
    if (command === 'Z') {
      if (current.length >= 3) polygons.push(current);
      current = [];
      continue;
    }
    const arity = command === 'C' ? 6 : 2;
    do {
      const args = [];
      for (let index = 0; index < arity; index += 1) args.push(number(at + index));
      at += arity;
      const last = current.length ? current[current.length - 1] : null;
      if (command === 'M' || (command === 'L' && last === null)) {
        current.push([args[0], args[1]]);
      } else if (command === 'L') {
        current.push([args[0], args[1]]);
      } else {
        const [x1, y1, x2, y2, x3, y3] = args;
        const [x0, y0] = last;
        for (let step = 1; step <= CURVE_SEGMENTS; step += 1) {
          const t = step / CURVE_SEGMENTS;
          const u = 1 - t;
          current.push([
            u * u * u * x0 + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t * x3,
            u * u * u * y0 + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t * y3,
          ]);
        }
      }
    } while (typeof tokens[at] === 'number' && command !== 'Z');
  }
  if (current.length >= 3) polygons.push(current);
  return polygons;
}

/** Per-pixel coverage (0..1) of one shape's polygons, even-odd, 4x vertical supersampling. */
function coverageOf(polys, size) {
  const coverage = new Float32Array(size * size);
  const crossings = [];
  const rows = size * SUPERSAMPLE;
  for (let row = 0; row < rows; row += 1) {
    const y = (row + 0.5) / SUPERSAMPLE;
    crossings.length = 0;
    for (const poly of polys) {
      for (let index = 0; index < poly.length; index += 1) {
        const [x0, y0] = poly[index];
        const [x1, y1] = poly[(index + 1) % poly.length];
        if ((y0 <= y && y1 > y) || (y1 <= y && y0 > y)) {
          crossings.push(x0 + ((y - y0) * (x1 - x0)) / (y1 - y0));
        }
      }
    }
    if (crossings.length < 2) continue;
    crossings.sort((left, right) => left - right);
    const base = Math.floor(y) * size;
    for (let pair = 0; pair + 1 < crossings.length; pair += 2) {
      const from = Math.max(0, crossings[pair]);
      const to = Math.min(size, crossings[pair + 1]);
      if (!(to > from)) continue;
      const last = Math.min(size - 1, Math.ceil(to) - 1);
      for (let px = Math.floor(from); px <= last; px += 1) {
        const overlap = Math.min(to, px + 1) - Math.max(from, px);
        if (overlap > 0) coverage[base + px] += overlap / SUPERSAMPLE;
      }
    }
  }
  return coverage;
}

/** Draw the mark into a size x size RGBA buffer (straight alpha, source-over). */
function draw(mark, size) {
  const scale = size / mark.box[2];
  const pixels = new Uint8ClampedArray(size * size * 4);
  for (const shape of mark.shapes) {
    const polys = shape.polys.map((poly) => poly.map(([x, y]) => [(x - mark.box[0]) * scale, (y - mark.box[1]) * scale]));
    const coverage = coverageOf(polys, size);
    for (let index = 0; index < coverage.length; index += 1) {
      const alpha = Math.min(1, coverage[index]);
      if (alpha <= 0) continue;
      const at = index * 4;
      const dstA = pixels[at + 3] / 255;
      const outA = alpha + dstA * (1 - alpha);
      for (let channel = 0; channel < 3; channel += 1) {
        pixels[at + channel] = Math.round((shape.fill[channel] * alpha + pixels[at + channel] * dstA * (1 - alpha)) / outA);
      }
      pixels[at + 3] = Math.round(outA * 255);
    }
  }
  return pixels;
}

// --- PNG (the container this writer emits: 8-bit RGBA, filter 0, one IDAT) ----------------------

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const typed = Buffer.concat([Buffer.from(type, 'latin1'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(typed));
  return Buffer.concat([length, typed, crc]);
}

function encodePng(pixels, size) {
  const stride = size * 4 + 1;
  const raw = Buffer.alloc(stride * size);
  for (let y = 0; y < size; y += 1) {
    raw[y * stride] = 0;
    Buffer.from(pixels.buffer, y * size * 4, size * 4).copy(raw, y * stride + 1);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8;
  header[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', header),
    pngChunk('IDAT', deflateSync(raw, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function decodePng(bytes, what) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  if (!bytes.subarray(0, 8).equals(signature)) refuse(`${what} is not a PNG`);
  let at = 8;
  let header = null;
  const data = [];
  while (at + 8 <= bytes.length) {
    const length = bytes.readUInt32BE(at);
    const type = bytes.subarray(at + 4, at + 8).toString('latin1');
    const body = bytes.subarray(at + 8, at + 8 + length);
    if (type === 'IHDR') header = { width: body.readUInt32BE(0), height: body.readUInt32BE(4), depth: body[8], colour: body[9], interlace: body[12] };
    if (type === 'IDAT') data.push(body);
    if (type === 'IEND') break;
    at += 12 + length;
  }
  if (!header) refuse(`${what} carries no IHDR`);
  if (header.depth !== 8 || header.colour !== 6 || header.interlace !== 0) {
    refuse(`${what} is not 8-bit RGBA non-interlaced (depth ${header.depth}, colour ${header.colour}, interlace ${header.interlace})`);
  }
  const raw = inflateSync(Buffer.concat(data));
  const { width, height } = header;
  const stride = width * 4;
  const pixels = new Uint8ClampedArray(width * height * 4);
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride));
    for (let x = 0; x < stride; x += 1) {
      const left = x >= 4 ? line[x - 4] : 0;
      const up = previous[x];
      const upLeft = x >= 4 ? previous[x - 4] : 0;
      if (filter === 1) line[x] = (line[x] + left) & 0xff;
      else if (filter === 2) line[x] = (line[x] + up) & 0xff;
      else if (filter === 3) line[x] = (line[x] + ((left + up) >> 1)) & 0xff;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        const predictor = pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
        line[x] = (line[x] + predictor) & 0xff;
      } else if (filter !== 0) refuse(`${what} uses PNG filter ${filter}, which this reader does not implement`);
    }
    Buffer.from(pixels.buffer, y * stride, stride).set(line);
    previous = line;
  }
  return { width, height, pixels };
}

// --- ICO ----------------------------------------------------------------------------------------

function encodeIco(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(entries.length, 4);
  let offset = 6 + entries.length * 16;
  const directory = entries.map((entry) => {
    const row = Buffer.alloc(16);
    row[0] = entry.size >= 256 ? 0 : entry.size;
    row[1] = entry.size >= 256 ? 0 : entry.size;
    row[2] = 0;
    row[3] = 0;
    row.writeUInt16LE(1, 4);
    row.writeUInt16LE(32, 6);
    row.writeUInt32LE(entry.png.length, 8);
    row.writeUInt32LE(offset, 12);
    offset += entry.png.length;
    return row;
  });
  return Buffer.concat([header, ...directory, ...entries.map((entry) => entry.png)]);
}

/** The PNG payloads of an ICO, in directory order: [{ size, png }]. */
function decodeIco(bytes, what) {
  if (bytes.length < 6 || bytes.readUInt16LE(0) !== 0 || bytes.readUInt16LE(2) !== 1) refuse(`${what} is not an ICO`);
  const count = bytes.readUInt16LE(4);
  const entries = [];
  for (let index = 0; index < count; index += 1) {
    const at = 6 + index * 16;
    if (at + 16 > bytes.length) refuse(`${what} declares ${count} images but its directory ends early`);
    const size = bytes[at] === 0 ? 256 : bytes[at];
    const length = bytes.readUInt32LE(at + 8);
    const offset = bytes.readUInt32LE(at + 12);
    if (offset + length > bytes.length) refuse(`${what}'s image ${index} runs past the end of the file`);
    entries.push({ size, png: bytes.subarray(offset, offset + length) });
  }
  return entries;
}

// --- modes --------------------------------------------------------------------------------------

function expected() {
  const mark = parseMark();
  const pngs = PNG_SET.map(({ file, px }) => ({ file, px, png: encodePng(draw(mark, px), px) }));
  const ico = encodeIco(ICO_SIZES.map((size) => ({ size, png: encodePng(draw(mark, size), size) })));
  return { mark, pngs, ico };
}

function write() {
  const { pngs, ico } = expected();
  mkdirSync(join(root, ICON_DIR), { recursive: true });
  const written = [];
  for (const { file, px, png } of pngs) {
    writeFileSync(join(root, ICON_DIR, file), png);
    written.push({ file: join(ICON_DIR, file), px, bytes: png.length });
  }
  writeFileSync(join(root, ICO_FILE), ico);
  written.push({ file: ICO_FILE, px: ICO_SIZES.join('/'), bytes: ico.length });
  console.log(JSON.stringify({ written: true, source: MARK, files: written }, null, 2));
}

function comparePixels(what, file, actual, wanted, width, height) {
  const failures = [];
  if (actual.width !== width || actual.height !== height) {
    failures.push(`${file}: the mark draws ${width}x${height} at this size, the file is ${actual.width}x${actual.height}`);
    return failures;
  }
  let differing = 0;
  let first = null;
  for (let index = 0; index < wanted.length; index += 1) {
    if (wanted[index] !== actual.pixels[index]) {
      differing += 1;
      if (first === null) first = index;
    }
  }
  if (differing) {
    const at = Math.floor(first / 4);
    failures.push(
      `${file} (${what}): ${differing} of ${wanted.length} channel values differ from the mark —`
      + ` first at pixel (${at % width}, ${Math.floor(at / width)}): file ${[...actual.pixels.subarray(at * 4, at * 4 + 4)].join(',')},`
      + ` mark ${[...wanted.subarray(at * 4, at * 4 + 4)].join(',')}. Run \`npm run native-icons\` after editing ${MARK}.`,
    );
  }
  return failures;
}

function check() {
  const { mark, pngs } = expected();
  const failures = [];

  const expectedNames = new Set([...pngs.map(({ file }) => file), 'icon.ico', 'mark.svg']);
  const presentNames = new Set(readdirSync(join(root, ICON_DIR)));
  for (const name of presentNames) {
    if (!expectedNames.has(name)) failures.push(`${join(ICON_DIR, name)}: not part of the icon set this generator writes (expected ${[...expectedNames].sort().join(', ')})`);
  }

  for (const { file, px, png } of pngs) {
    const path = join(root, ICON_DIR, file);
    if (!existsSync(path)) {
      failures.push(`${join(ICON_DIR, file)} is missing — run \`npm run native-icons\``);
      continue;
    }
    failures.push(...comparePixels('png', join(ICON_DIR, file), decodePng(readFileSync(path), file), decodePng(png, file).pixels, px, px));
  }

  if (!existsSync(join(root, ICO_FILE))) {
    failures.push(`${ICO_FILE} is missing — run \`npm run native-icons\``);
  } else {
    const entries = decodeIco(readFileSync(join(root, ICO_FILE)), ICO_FILE);
    const sizes = entries.map(({ size }) => size);
    if (sizes.join(',') !== ICO_SIZES.join(',')) {
      failures.push(`${ICO_FILE}: carries images ${sizes.join(', ') || '(none)'}; the set the mark defines is ${ICO_SIZES.join(', ')}`);
    } else {
      for (const entry of entries) {
        const wanted = draw(mark, entry.size);
        failures.push(...comparePixels('ico', ICO_FILE, decodePng(entry.png, `${ICO_FILE} @${entry.size}`), wanted, entry.size, entry.size));
      }
    }
  }

  if (failures.length) {
    console.error(JSON.stringify({ passed: false, source: MARK, failures }, null, 2));
    process.exit(1);
  }
  console.log(JSON.stringify({
    passed: true,
    source: MARK,
    pngs: pngs.map(({ file, px }) => `${file} ${px}x${px}`),
    ico: ICO_SIZES,
  }, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const argv = process.argv.slice(2);
  const unknown = argv.filter((flag) => flag !== '--check');
  if (unknown.length) refuse(`unknown argument(s): ${unknown.join(' ')} (--check is the only option)`);
  if (argv.includes('--check')) check();
  else write();
}

export { draw, encodePng, decodePng, parseMark };
