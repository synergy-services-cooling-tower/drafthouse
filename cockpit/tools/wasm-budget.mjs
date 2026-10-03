#!/usr/bin/env node
/**
 * wasm-budget.mjs - the committed wasm size budget (issue #82), raw and gzipped.
 *
 *   node cockpit/tools/wasm-budget.mjs --raw <bytes> --gzip <bytes> [--budget <path>] [--label <text>]
 *
 * The numbers come from the build that just ran: `cockpit/tools/build-web.sh` measures the wasm it
 * produced (`wc -c`, `gzip -9`) and calls this script, which is the only place the comparison lives.
 * `--budget` (or `COCKPIT_WASM_BUDGET`) points at another budget file - that is the knob the lane's
 * RED/GREEN pair moves, exactly like `COCKPIT_GZIP_LIMIT_BYTES` moves the 8 MiB gate beside it.
 *
 * Exit status: 0 within budget, 1 over budget (named, per number), 2 the check could not run (no
 * budget file, malformed JSON, missing numbers).
 *
 * The budget file is `cockpit/wasm-budget.json`, in the cockpit's own directory so the public
 * snapshot (`scripts/public-snapshot.mjs` allowlists `cockpit/`) carries it with the build script
 * that enforces it.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const DEFAULT_BUDGET = resolve(here, '..', 'wasm-budget.json');

function fail(why) {
  console.error(`WASM SIZE BUDGET: NOT CHECKED - ${why}`);
  process.exit(2);
}

function parseBytes(value, name) {
  if (!/^\d+$/.test(value)) fail(`${name} must be a whole number of bytes, got "${value}"`);
  return Number(value);
}

let raw = null;
let gzip = null;
let label = '';
let budgetPath = process.env.COCKPIT_WASM_BUDGET ?? DEFAULT_BUDGET;

const args = process.argv.slice(2);
for (let index = 0; index < args.length; index += 1) {
  const arg = args[index];
  const value = args[index + 1];
  switch (arg) {
    case '--raw':
      if (value === undefined) fail('--raw needs a value');
      raw = parseBytes(value, '--raw');
      index += 1;
      break;
    case '--gzip':
      if (value === undefined) fail('--gzip needs a value');
      gzip = parseBytes(value, '--gzip');
      index += 1;
      break;
    case '--budget':
      if (value === undefined) fail('--budget needs a value');
      budgetPath = resolve(value);
      index += 1;
      break;
    case '--label':
      if (value === undefined) fail('--label needs a value');
      label = value;
      index += 1;
      break;
    default:
      fail(`unknown argument "${arg}"`);
  }
}

if (raw === null || gzip === null) {
  fail('--raw <bytes> and --gzip <bytes> are required (build-web.sh measures them from the build)');
}

let budget;
try {
  budget = JSON.parse(readFileSync(budgetPath, 'utf8'));
} catch (error) {
  fail(`the budget file ${budgetPath} could not be read: ${error.message}`);
}

for (const key of ['raw_bytes', 'gzip_bytes']) {
  if (!Number.isInteger(budget[key]) || budget[key] <= 0) {
    fail(`the budget file ${budgetPath} has no positive integer "${key}"`);
  }
}

const named = label ? ` (${label})` : '';
const over = [];
if (raw > budget.raw_bytes) over.push(`raw ${raw} > ${budget.raw_bytes} (over by ${raw - budget.raw_bytes})`);
if (gzip > budget.gzip_bytes) over.push(`gzip ${gzip} > ${budget.gzip_bytes} (over by ${gzip - budget.gzip_bytes})`);

console.log(`  wasm budget${named}:     ${budgetPath}`);
console.log(`  raw:                 ${raw} bytes vs the committed ${budget.raw_bytes}`);
console.log(`  gzipped:             ${gzip} bytes vs the committed ${budget.gzip_bytes}`);

if (over.length > 0) {
  console.error(`WASM SIZE BUDGET: FAILED - ${over.join('; ')}`);
  console.error('If the growth is intended, move the numbers in the budget file in a commit that says why.');
  process.exit(1);
}
console.log(`WASM SIZE BUDGET: OK - raw ${raw} <= ${budget.raw_bytes}, gzip ${gzip} <= ${budget.gzip_bytes}`);
