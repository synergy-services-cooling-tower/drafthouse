/**
 * The engine's thin JS binding (issue #24): data in, data out.
 *
 * This module is the JavaScript half of the publishable `engine` piece — the wasm module
 * (`rust/src/wasm.rs`, built from `rust/`) plus this file. It runs the engine and returns
 * what the engine computed:
 *
 *     inputs      the values the run used, catalog dimensions included
 *     metrics     the recommended candidate's headline numbers
 *     candidates  the ranked candidates
 *     steps       the worked step list, in the `worked.js` shape (data, not markup)
 *     worked      the whole worked sheet those steps belong to
 *
 * It does NOT render, derive geometry or produce HTML: there is no DOM access, no string
 * of markup, and no arithmetic beyond turning numbers into record specs for the engine's
 * own parser. Geometry stays in `web/geometry.js` (`visuals`), precisely so that a visual
 * change never needs a wasm rebuild and this file never grows one.
 *
 * Usage — Node:
 *
 *     import { readFileSync } from 'node:fs';
 *     import { createEngine } from './rust/wasm/binding.mjs';
 *     const engine = createEngine(readFileSync('rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm'));
 *     const run = engine.select({ catalog, requirements: { waterMassFlowKgS: 200 } });
 *
 * Browser: fetch the pinned `.wasm`, `createEngine(await response.arrayBuffer())`.
 *
 * The engine refuses rather than approximates, and the refusal is data too: every call
 * returns `{ ok: false, status, error }` with the status the `ct-engine` binary would have
 * exited with (1 domain, 2 usage), so a caller never has to guess what happened.
 */

/** Requirement field -> `ct-engine select` flag, for the fields the engine reads. */
const REQUIREMENT_FLAGS = [
  ['waterMassFlowKgS', '--water'],
  ['hotWaterC', '--hot'],
  ['targetColdWaterC', '--target-cold'],
  ['wetBulbC', '--wb'],
  ['dryBulbC', '--db'],
  ['pressurePa', '--p'],
  ['salinityGKg', '--salinity'],
  ['cyclesOfConcentration', '--cycles'],
  ['maxDriftPpm', '--max-drift-ppm'],
  ['maxElectricalInputKW', '--max-power'],
  ['maxFootprintM2', '--max-footprint'],
  ['minimumThermalMarginC', '--min-margin'],
  ['nozzlePressureDropPa', '--nozzle-dp'],
];

/** Catalog list name in the request -> `ct-engine select` flag. */
const CATALOG_FLAGS = [
  ['towers', '--towers'],
  ['fills', '--fills'],
  ['driftEliminators', '--drift-eliminators'],
  ['fans', '--fans'],
  ['nozzles', '--nozzles'],
];

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * Load an engine from wasm bytes (or an already compiled module).
 *
 * @param {BufferSource|WebAssembly.Module} wasm
 * @returns {{call: (args: string[]) => object, select: (request: object) => object, recordFields: () => object}}
 */
export function createEngine(wasm) {
  const module = wasm instanceof WebAssembly.Module ? wasm : new WebAssembly.Module(wasm);
  const instance = new WebAssembly.Instance(module, {});
  const { memory, ct_alloc, ct_call, ct_free } = instance.exports;

  /**
   * One engine command: the arguments of a `ct-engine` command line, as data.
   * Returns the reply envelope (`{ok: true, value}` or `{ok: false, status, error}`).
   */
  function call(args) {
    const payload = encoder.encode(args.join('\n'));
    // The ABI is length-prefixed: four little-endian bytes, then the payload.
    const pointer = ct_alloc(payload.length);
    new Uint8Array(memory.buffer, pointer + 4, payload.length).set(payload);
    const reply = ct_call(pointer);
    const length = new DataView(memory.buffer).getUint32(reply, true);
    const text = decoder.decode(new Uint8Array(memory.buffer, reply + 4, length));
    ct_free(reply);
    return JSON.parse(text);
  }

  /** The record field names the engine accepts, per catalog list (asked once, cached). */
  let fields = null;
  function recordFields() {
    if (fields === null) {
      const reply = call(['select-fields']);
      if (!reply.ok) throw new Error(`the engine refused select-fields: ${reply.error.message}`);
      fields = reply.value;
    }
    return fields;
  }

  /**
   * Run a selection: catalog in, ranked numerics out.
   *
   *     engine.select({
   *       catalog,                       // records with the engine's field names
   *       requirements: { … },           // duty, air state, ceilings — optional fields default
   *       objective,                     // an engineering objective name, optional
   *       maxResults,                    // ranked candidates to return, optional
   *       allOrders,                     // add the whole feasible set's order per objective
   *     })
   *
   * Returns the engine's own answer: on success `{ok: true, inputs, metrics, candidates,
   * steps, worked, …}`, on a refusal `{ok: false, status, error}`. Nothing is derived here
   * except the record specs the engine's parser reads them from.
   */
  function select({ catalog, requirements = {}, objective, maxResults = 30, allOrders = false } = {}) {
    if (!catalog) throw new Error('select needs a catalog');
    const args = ['select', ...catalogArgs(catalog), ...requirementArgs(requirements), '--max-results', String(maxResults)];
    if (objective) args.push('--objective', objective);
    if (allOrders) args.push('--all-orders');
    const reply = call(args);
    if (!reply.ok) return reply;
    const value = reply.value;
    return {
      ok: true,
      // The values the run used, the ranked candidates, the headline metrics and the
      // worked steps — the four things this surface exists to hand out.
      inputs: value.inputs,
      metrics: value.worked?.result,
      candidates: value.results,
      steps: value.worked?.steps,
      worked: value.worked,
      objective: value.objective,
      requirements: value.requirements,
      catalog: value.catalog,
      feasibleCandidateCount: value.feasibleCandidateCount,
      rejectionSummary: value.rejectionSummary,
      capacitiesResolved: value.capacitiesResolved,
      warning: value.warning,
      ...(value.candidates ? { allCandidates: value.candidates, orders: value.orders } : {}),
    };
  }

  function catalogArgs(catalog) {
    const accepted = recordFields();
    const args = [];
    for (const [list, flag] of CATALOG_FLAGS) {
      const records = catalog[list];
      if (!Array.isArray(records) || records.length === 0) continue;
      args.push(flag, records.map((record) => recordSpec(record, accepted[list])).join(';'));
    }
    if (catalog.waterQualityFactors) {
      args.push('--quality-factors', Object.entries(catalog.waterQualityFactors)
        .map(([name, factor]) => `${name}:${factor.thermalMultiplier}:${factor.pressureMultiplier}`)
        .join(';'));
    }
    if (catalog.metadata) {
      args.push('--catalog-id', String(catalog.metadata.id ?? ''));
      args.push('--catalog-revision', String(catalog.metadata.revision ?? ''));
      args.push('--catalog-status', String(catalog.metadata.status ?? ''));
    }
    return args;
  }

  /**
   * One record as a `key:value` spec, carrying only the fields the engine accepts — the
   * engine refuses a field it does not read by name, so a catalog's display-only or
   * commercial fields must not travel.
   */
  function recordSpec(record, acceptedFields) {
    return acceptedFields
      .filter((field) => record[field] !== undefined && record[field] !== null)
      .map((field) => `${field}:${fieldSpec(record[field])}`)
      .join(',');
  }

  /** A field value as the spec's `|`-separated text: scalars, lists, curves, correlations. */
  function fieldSpec(value) {
    if (Array.isArray(value)) return value.map(rowSpec).join('|');
    if (value !== null && typeof value === 'object') return Object.values(value).map(scalarSpec).join('|');
    return scalarSpec(value);
  }

  /** One `|` entry: a number, a string, or a row (a curve point, a correlation object). */
  function rowSpec(entry) {
    if (entry !== null && typeof entry === 'object') return Object.values(entry).map(scalarSpec).join(':');
    return scalarSpec(entry);
  }

  function scalarSpec(value) {
    return typeof value === 'number' ? String(value) : String(value);
  }

  function requirementArgs(requirements) {
    const args = [];
    for (const [field, flag] of REQUIREMENT_FLAGS) {
      if (requirements[field] !== undefined && requirements[field] !== null) {
        args.push(flag, String(requirements[field]));
      }
    }
    if (requirements.waterQualityClass !== undefined && requirements.waterQualityClass !== null) {
      args.push('--quality-class', String(requirements.waterQualityClass));
    }
    if (Array.isArray(requirements.speedRatios)) {
      args.push('--speed-ratios', requirements.speedRatios.join('|'));
    }
    return args;
  }

  return { call, select, recordFields };
}
