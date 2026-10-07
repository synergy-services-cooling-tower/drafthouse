/**
 * Derived values for the parts panel — the only place a displayed panel number is produced.
 *
 * NUMERIC FIDELITY RULE: every number on this surface is either read from the selection
 * result the engine already computed (kind "given" — rendered as Field / As selected), or is
 * plain arithmetic on those values whose formula and substitution travel with it (kind
 * "math" — rendered as Formula / With your numbers / = value). Nothing here re-derives
 * physics: the panel explains the selection, it never re-rates, re-solves or re-ranks it.
 *
 * ENGINEERING FIELDS ONLY. The panel receives an engineering-only projection of the
 * candidate and of the requirements (`panelCandidate`, `panelRequirements`): every cost,
 * price, currency or lifecycle key the engine may carry is physically absent from what this
 * module can read, not merely unused. There is no formatter in this file that could render
 * a currency amount.
 *
 * MAP-TO-RECORD CONTRACT (tested in tests/panel.test.js, on the mapping and not on pixels):
 * every pickable assembly carries the id of the record the engine selected for it, and that
 * id is the same id the ranked table renders in its row for that record:
 *
 *   shell  -> candidate.tower.id                (the ranked row's Tower cell)
 *   fill   -> candidate.fill.id                 (the ranked row's Fill cell)
 *   drift  -> candidate.driftEliminator.id      (the ranked row's Drift cell)
 *   fan    -> candidate.fan.id                  (the ranked row's Fan cell)
 *   motor  -> MOT-<selected rating>             (the standard-motor selection record)
 *   header -> candidate.nozzle.nozzleId         (the nozzle arrangement record)
 *   basin  -> candidate.tower.id                (the basin sits inside the cell record)
 */

/** Rail order is the geometry's own order — imported so the two cannot drift apart. */
import { PART_ORDER } from "./geometry.js";

/** Same formatting semantics as the host tool's formatNumber(). */
export function fmt(value, digits = 2) {
  if (!Number.isFinite(value)) return "—";
  return new Intl.NumberFormat("en-US", {
    maximumFractionDigits: digits,
    minimumFractionDigits: Math.min(digits, 0)
  }).format(value);
}

/** Round-trip-safe number text for formula strings (plain ASCII, no locale grouping). */
const n4 = (value, digits = 4) => Number(value).toFixed(digits).replace(/\.?0+$/, "");

/* ------------------------------------------------------------------ *
 * Engineering-only projections
 * ------------------------------------------------------------------ */

/**
 * The requirements fields the panel may read. Everything else on the engine's requirement
 * object — including any commercial key a deployment may add later — is dropped here rather
 * than merely not rendered, so a cost figure cannot reach the panel's code path at all.
 */
export const ENGINEERING_REQUIREMENT_FIELDS = Object.freeze([
  "waterMassFlowKgS",
  "hotWaterC",
  "targetColdWaterC",
  "wetBulbC",
  "dryBulbC",
  "pressurePa",
  "salinityGKg",
  "waterQualityClass",
  "cyclesOfConcentration",
  "maxDriftPpm",
  "maxElectricalInputKW",
  "maxFootprintM2",
  "minimumThermalMarginC",
  "nozzlePressureDropPa",
  "operatingHoursPerYear"
]);

export function panelRequirements(requirements = {}) {
  const projected = {};
  for (const key of ENGINEERING_REQUIREMENT_FIELDS) {
    if (requirements[key] !== undefined) projected[key] = requirements[key];
  }
  return projected;
}

/** The candidate the panel reads: the engine's result minus any commercial keys. */
export function panelCandidate(candidate) {
  if (!candidate || typeof candidate !== "object") return candidate;
  const { economics, ...engineering } = candidate;
  return engineering;
}

export function materialOf(record) {
  if (!record || typeof record.material !== "string") return null;
  return record.material.replace(/\s*\(illustrative\)\s*/i, "").trim();
}

/* ------------------------------------------------------------------ *
 * Pressure: where the fan's pressure goes
 * ------------------------------------------------------------------ */

/** The eight loss terms, in the order the engine's airside breakdown carries them. */
export const LOSS_DEFS = [
  { key: "fill", label: "Fill", partId: "fill", where: "Fill bank", why: "Air crossing the wet fill sheets — by far the largest single term." },
  { key: "drift", label: "Drift eliminator", partId: "drift", where: "Drift eliminator bank", why: "The eliminator's own loss at the face velocity the selection produced." },
  { key: "inlet", label: "Inlet", partId: "shell", where: "Inlet louvers", why: "Loss at the inlet face, where the air is moving fastest into the cell." },
  { key: "distribution", label: "Distribution", partId: "fill", where: "Above the fill bank", why: "Water-distribution hardware the air has to pass on its way up." },
  { key: "supports", label: "Supports", partId: "fill", where: "Fill support structure", why: "Loss at the support velocity under the fill bank." },
  { key: "plenum", label: "Plenum and fan inlet", partId: "shell", where: "Plenum void", why: "Loss as the air turns into the fan inlet." },
  { key: "fanStack", label: "Fan-stack discharge", partId: "fan", where: "Fan stack", why: "Loss at the stack velocity." },
  { key: "fixed", label: "Fixed", partId: null, where: "No single location", why: "A fixed allowance for everything not itemised — the panel does not place it on the model." }
];

const LOSS_FIELDS = Object.freeze({
  fill: "fillPa",
  drift: "driftPa",
  inlet: "inletPa",
  distribution: "distributionPa",
  supports: "supportPa",
  plenum: "plenumPa",
  fanStack: "fanStackPa",
  fixed: "fixedPa"
});

export function pressureLosses(candidate) {
  const airside = candidate.airside;
  const totalPa = airside.totalPa;
  const rows = LOSS_DEFS.map((def) => {
    const pa = airside[LOSS_FIELDS[def.key]];
    return {
      ...def,
      pa,
      field: `airside.${LOSS_FIELDS[def.key]}`,
      sharePct: (pa / totalPa) * 100,
      shareFormula: "share = dp_part / dp_total x 100",
      shareSubstitution: `share = ${n4(pa)} / ${n4(totalPa)} x 100`
    };
  });
  const sumPa = rows.reduce((sum, row) => sum + row.pa, 0);
  return {
    rows,
    sumPa,
    totalPa,
    /** The eight terms must add up to the total the fan sees. Asserted in tests/panel.test.js. */
    check: {
      ok: Math.abs(sumPa - totalPa) < 1e-9,
      formula: "sum of the eight terms = total air-side pressure the fan must overcome",
      substitution: `${rows.map((row) => n4(row.pa)).join(" + ")} = ${n4(sumPa)} Pa`,
      delta: sumPa - totalPa
    }
  };
}

/* ------------------------------------------------------------------ *
 * Cooling: what the fill delivers against the duty
 * ------------------------------------------------------------------ */

export function cooling(candidate, requirements) {
  const airside = candidate.airside;
  const available = airside.availableMerkelNumber;
  const zones = [
    { id: "fill", label: "Fill bank", value: airside.fillMerkelNumber, field: "airside.fillMerkelNumber" },
    { id: "spray", label: "Spray zone", value: airside.sprayZoneMerkelNumber, field: "airside.sprayZoneMerkelNumber" },
    { id: "rain", label: "Rain zone", value: airside.rainZoneMerkelNumber, field: "airside.rainZoneMerkelNumber" }
  ];
  const zoneSum = zones.reduce((sum, zone) => sum + zone.value, 0);
  const deliveredC = candidate.thermal.coldWaterC;
  const targetC = requirements.targetColdWaterC;
  const marginC = targetC - deliveredC;
  const lg = requirements.waterMassFlowKgS / airside.dryAirMassFlowKgS;
  return {
    available,
    deliveredC,
    targetC,
    marginC,
    lg,
    zones,
    temperatureModel: candidate.thermal.model,
    /** The zone split the engine computed must add up to the available Merkel number it records. */
    zoneCheck: {
      ok: Math.abs(zoneSum - available) < 1e-9,
      formula: "fill + spray + rain = available KaV/L",
      substitution: `${n4(airside.fillMerkelNumber)} + ${n4(airside.sprayZoneMerkelNumber)} + ${n4(airside.rainZoneMerkelNumber)} = ${n4(zoneSum)} KaV/L`,
      delta: zoneSum - available
    },
    /** The engine records thermalMarginC; the same value is the target minus the delivered CWT. */
    marginCheck: {
      ok: Math.abs(marginC - candidate.thermalMarginC) < 1e-9,
      formula: "margin = target cold water - delivered cold water",
      substitution: `margin = ${n4(targetC)} - ${n4(deliveredC)} = ${n4(marginC)} °C`,
      delta: marginC - candidate.thermalMarginC
    },
    lgCheck: {
      formula: "L/G = circulating water flow / dry-air flow",
      substitution: `L/G = ${n4(requirements.waterMassFlowKgS)} / ${n4(airside.dryAirMassFlowKgS)} = ${n4(lg)}`,
      source: "requirements.waterMassFlowKgS / candidate.airside.dryAirMassFlowKgS"
    }
  };
}

/* ------------------------------------------------------------------ *
 * Limit state
 * ------------------------------------------------------------------ */

/** Display rule (not physics): utilisation at or above this reads as "tight". */
export const TIGHT_UTILISATION = 0.9;

function stateOf(utilization) {
  if (utilization > 1) return "violation";
  if (utilization >= TIGHT_UTILISATION) return "tight";
  return "pass";
}

function maxCheck(partId, name, value, unit, limit, limitLabel, source) {
  const utilization = value / limit;
  return {
    partId, name, unit, value, limit, limitLabel, utilization, source, kind: "max",
    formula: `utilisation = ${name} / limit`,
    substitution: `utilisation = ${n4(value)} / ${n4(limit)} = ${n4(utilization)}`
  };
}

function minCheck(partId, name, value, unit, limit, limitLabel, source) {
  const utilization = limit / value;
  return {
    partId, name, unit, value, limit, limitLabel, utilization, source, kind: "min",
    formula: `utilisation = limit / ${name}`,
    substitution: `utilisation = ${n4(limit)} / ${n4(value)} = ${n4(utilization)}`
  };
}

/**
 * Holds the selection's own numbers against the limits and requirements the run used — the
 * same constraints the engine enforced when it built this candidate. `spec` lets the surface
 * hold the SAME numbers against a tighter requirement value (the "spec stress" switch): the
 * switch moves a threshold only — no number is recalculated.
 */
export function limitChecks(candidate, requirements, spec = {}) {
  const tower = candidate.tower;
  const fill = candidate.fill;
  const drift = candidate.driftEliminator;
  const fan = candidate.fan;
  const airside = candidate.airside;
  const motor = candidate.motor;
  const maxDriftPpm = spec.maxDriftPpm ?? requirements.maxDriftPpm;
  const minimumThermalMarginC = spec.minimumThermalMarginC ?? requirements.minimumThermalMarginC;
  const driftVelocities = drift.curve.map((point) => point.faceVelocityMS);
  const driftVelocityMin = Math.min(...driftVelocities);
  const driftVelocityMax = Math.max(...driftVelocities);
  const speedBand = fan.allowedSpeedRatio;

  const checks = [
    maxCheck("fill", "water temperature", requirements.hotWaterC, "°C",
      fill.limits.maxWaterTemperatureC, `material limit ${n4(fill.limits.maxWaterTemperatureC)} °C`,
      "requirements.hotWaterC vs candidate.fill.limits.maxWaterTemperatureC"),
    minCheck("fill", "water loading", airside.waterLoadingKgM2S, "kg/(m²·s)",
      fill.limits.minWaterLoadingKgM2S, `envelope floor ${n4(fill.limits.minWaterLoadingKgM2S)} kg/(m²·s)`,
      "candidate.airside.waterLoadingKgM2S vs candidate.fill.limits.minWaterLoadingKgM2S"),
    maxCheck("fill", "water loading", airside.waterLoadingKgM2S, "kg/(m²·s)",
      fill.limits.maxWaterLoadingKgM2S, `envelope ceiling ${n4(fill.limits.maxWaterLoadingKgM2S)} kg/(m²·s)`,
      "candidate.airside.waterLoadingKgM2S vs candidate.fill.limits.maxWaterLoadingKgM2S"),
    minCheck("fill", "dry-air loading", airside.dryAirLoadingKgM2S, "kg/(m²·s)",
      fill.limits.minDryAirLoadingKgM2S, `envelope floor ${n4(fill.limits.minDryAirLoadingKgM2S)} kg/(m²·s)`,
      "candidate.airside.dryAirLoadingKgM2S vs candidate.fill.limits.minDryAirLoadingKgM2S"),
    maxCheck("fill", "dry-air loading", airside.dryAirLoadingKgM2S, "kg/(m²·s)",
      fill.limits.maxDryAirLoadingKgM2S, `envelope ceiling ${n4(fill.limits.maxDryAirLoadingKgM2S)} kg/(m²·s)`,
      "candidate.airside.dryAirLoadingKgM2S vs candidate.fill.limits.maxDryAirLoadingKgM2S"),
    minCheck("fill", "thermal margin", candidate.thermalMarginC, "°C", minimumThermalMarginC,
      `requirement minimum margin ${n4(minimumThermalMarginC)} °C`,
      spec.minimumThermalMarginC ? "requirements.minimumThermalMarginC, held at a tighter value by the spec switch" : "candidate.thermalMarginC vs requirements.minimumThermalMarginC"),
    maxCheck("drift", "water temperature", requirements.hotWaterC, "°C",
      drift.maxWaterTemperatureC, `material limit ${n4(drift.maxWaterTemperatureC)} °C`,
      "requirements.hotWaterC vs candidate.driftEliminator.maxWaterTemperatureC"),
    maxCheck("drift", "drift rate", airside.driftPpm, "ppm", maxDriftPpm,
      `requirement max drift ${n4(maxDriftPpm)} ppm`,
      spec.maxDriftPpm ? "requirements.maxDriftPpm, held at a tighter value by the spec switch" : "candidate.airside.driftPpm vs requirements.maxDriftPpm"),
    minCheck("drift", "face velocity", airside.driftVelocityMS, "m/s", driftVelocityMin,
      `curve floor ${n4(driftVelocityMin)} m/s`,
      "candidate.airside.driftVelocityMS vs candidate.driftEliminator.curve"),
    maxCheck("drift", "face velocity", airside.driftVelocityMS, "m/s", driftVelocityMax,
      `curve ceiling ${n4(driftVelocityMax)} m/s`,
      "candidate.airside.driftVelocityMS vs candidate.driftEliminator.curve"),
    minCheck("fan", "speed ratio", candidate.speedRatio, "×", speedBand[0],
      `allowed band ${n4(speedBand[0])}×`,
      "candidate.speedRatio vs candidate.fan.allowedSpeedRatio"),
    maxCheck("fan", "speed ratio", candidate.speedRatio, "×", speedBand[1],
      `allowed band ${n4(speedBand[1])}×`,
      "candidate.speedRatio vs candidate.fan.allowedSpeedRatio"),
    maxCheck("fan", "electrical input", candidate.electricalInputKW, "kW",
      requirements.maxElectricalInputKW, `requirement max input ${n4(requirements.maxElectricalInputKW)} kW`,
      "candidate.electricalInputKW vs requirements.maxElectricalInputKW"),
    maxCheck("shell", "circulating water flow", requirements.waterMassFlowKgS, "kg/s",
      tower.maxWaterMassFlowKgS, `tower limit ${n4(tower.maxWaterMassFlowKgS)} kg/s`,
      "requirements.waterMassFlowKgS vs candidate.tower.maxWaterMassFlowKgS"),
    maxCheck("shell", "footprint", tower.footprintM2, "m²",
      requirements.maxFootprintM2, `requirement max footprint ${n4(requirements.maxFootprintM2)} m²`,
      "candidate.tower.footprintM2 vs requirements.maxFootprintM2"),
    maxCheck("motor", "motor output", motor.requiredMotorOutputKW, "kW", motor.selectedMotorKW,
      `selected rating ${n4(motor.selectedMotorKW)} kW`,
      "candidate.motor.requiredMotorOutputKW vs candidate.motor.selectedMotorKW")
  ];

  for (const check of checks) check.state = stateOf(check.utilization);
  return {
    checks,
    gap: checks.filter((check) => check.state === "violation").length,
    tight: checks.filter((check) => check.state === "tight").length,
    byPart: (partId) => checks.filter((check) => check.partId === partId),
    spec
  };
}

/* ------------------------------------------------------------------ *
 * Records behind each pickable part — the map-to-record contract
 * ------------------------------------------------------------------ */

/** The id of the record an assembly maps to. This is the ONLY place the mapping is spelled. */
export function partRecordId(candidate, partId) {
  switch (partId) {
    case "shell": return candidate.tower.id;
    case "fill": return candidate.fill.id;
    case "drift": return candidate.driftEliminator.id;
    case "fan": return candidate.fan.id;
    case "motor": return motorRecordIdOf(candidate.motor);
    case "header": return candidate.nozzle ? candidate.nozzle.nozzleId : null;
    case "basin": return candidate.tower.id;
    default: return null;
  }
}

/** The record identity of the standard motor the engine selected: the selected rating. */
export function motorRecordIdOf(motor) {
  if (!motor || !Number.isFinite(motor.selectedMotorKW)) return null;
  return `MOT-${n4(motor.selectedMotorKW, 2)}`;
}

/**
 * The reverse direction: which assemblies map to a record id of this candidate. Two
 * assemblies can share one record — the basin sits inside the cell-shell record — so the
 * reverse mapping returns a list, in rail order.
 */
export function partsForRecordId(candidate, recordId) {
  return PART_ORDER.filter((partId) => partRecordId(candidate, partId) === recordId);
}

/**
 * The record ids the ranked table renders in a row, read from the same candidate fields
 * src/app.js renders into the row's Tower / Fill / Drift / Fan cells.
 */
export function rowRecordIds(candidate) {
  return {
    tower: candidate.tower.id,
    fill: candidate.fill.id,
    drift: candidate.driftEliminator.id,
    fan: candidate.fan.id
  };
}

/** The catalog record behind a pickable assembly, with the fields the panel shows. */
export function partRecord(candidate, partId, geometry) {
  const tower = candidate.tower;
  const fan = candidate.fan;
  const fill = candidate.fill;
  const drift = candidate.driftEliminator;
  const motor = candidate.motor;
  const nozzle = candidate.nozzle;
  const part = geometry?.parts.find((item) => item.id === partId);
  const dimValue = (key) => geometry.dims.find((item) => item.key === key).value;
  const base = {
    id: partRecordId(candidate, partId) ?? "—",
    name: part ? part.label : partId,
    recordPath: part ? part.recordSource : "",
    material: null,
    fields: [],
    files: part ? part.files : []
  };

  switch (partId) {
    case "shell":
      return {
        ...base,
        name: tower.name,
        material: materialOf(tower),
        fields: [
          ["Type / draft", `${tower.type} · ${tower.draftType}`],
          ["Fill area", `${fmt(tower.fillAreaM2, 2)} m²`],
          ["Air-free area", `${fmt(tower.airFreeAreaM2, 2)} m²`],
          ["Drift bank area", `${fmt(tower.driftAreaM2, 2)} m²`],
          ["Inlet area", `${fmt(tower.inletAreaM2, 2)} m²`],
          ["Footprint", `${fmt(tower.footprintM2, 2)} m²`],
          ["Fill-depth options", `${tower.fillDepthOptionsM.map((value) => fmt(value, 1)).join(" / ")} m`],
          ["Max water flow", `${fmt(tower.maxWaterMassFlowKgS, 1)} kg/s`]
        ]
      };
    case "fill":
      return {
        ...base,
        name: fill.name,
        material: materialOf(fill),
        fields: [
          ["Geometry", fill.geometry],
          ["Installed depth", `${fmt(candidate.fillDepthM, 2)} m`],
          ["Water-loading envelope", `${fmt(fill.limits.minWaterLoadingKgM2S, 1)} – ${fmt(fill.limits.maxWaterLoadingKgM2S, 1)} kg/(m²·s)`],
          ["Dry-air loading envelope", `${fmt(fill.limits.minDryAirLoadingKgM2S, 1)} – ${fmt(fill.limits.maxDryAirLoadingKgM2S, 1)} kg/(m²·s)`],
          ["Max water temperature", `${fmt(fill.limits.maxWaterTemperatureC, 0)} °C`]
        ]
      };
    case "drift":
      return {
        ...base,
        name: drift.name,
        material: materialOf(drift),
        fields: [
          ["Material", materialOf(drift) ?? "not declared"],
          ["Max water temperature", `${fmt(drift.maxWaterTemperatureC, 0)} °C`],
          ["Drift delivered", `${fmt(candidate.airside.driftPpm, 3)} ppm`],
          ["Face velocity at the operating point", `${fmt(candidate.airside.driftVelocityMS, 3)} m/s`],
          ["Curve envelope", `${fmt(Math.min(...drift.curve.map((point) => point.faceVelocityMS)), 2)} – ${fmt(Math.max(...drift.curve.map((point) => point.faceVelocityMS)), 2)} m/s`]
        ]
      };
    case "fan":
      return {
        ...base,
        name: fan.name,
        material: materialOf(fan),
        fields: [
          ["Stack area", `${fmt(fan.stackAreaM2, 3)} m²`],
          ["Pressure basis", fan.pressureBasis],
          ["Speed ratio", `${fmt(candidate.speedRatio, 2)}×`],
          ["Operating flow", `${fmt(candidate.fanOperatingPoint.flowM3S, 2)} m³/s`],
          ["Operating efficiency", `${fmt(candidate.fanOperatingPoint.efficiency * 100, 1)} %`],
          ["Shaft power", `${fmt(candidate.fanOperatingPoint.shaftPowerKW, 2)} kW`],
          ["Electrical input", `${fmt(candidate.electricalInputKW, 2)} kW`],
          ["Total air-side pressure", `${fmt(candidate.airside.totalPa, 2)} Pa`]
        ]
      };
    case "motor":
      return {
        ...base,
        name: base.name,
        fields: [
          ["Required output", `${fmt(motor.requiredMotorOutputKW, 2)} kW`],
          ["Selected standard rating", `${fmt(motor.selectedMotorKW, 2)} kW`],
          ["Drive efficiency", fmt(motor.driveEfficiency, 3)],
          ["Service factor", fmt(motor.serviceFactor, 2)],
          ["Fan shaft power", `${fmt(candidate.fanOperatingPoint.shaftPowerKW, 2)} kW`],
          ["Electrical input", `${fmt(candidate.electricalInputKW, 2)} kW`]
        ]
      };
    case "header":
      return {
        ...base,
        name: nozzle ? nozzle.nozzleName : base.name,
        fields: [
          ["Nozzle", nozzle ? `${nozzle.nozzleId} (${nozzle.nozzleName})` : "not resolved"],
          ["Count", `${nozzle ? nozzle.count : "—"} nozzles`],
          ["Flow per nozzle", nozzle ? `${fmt(nozzle.flowPerNozzleM3S * 1000, 3)} L/s` : "—"],
          ["Total nozzle flow", nozzle ? `${fmt(nozzle.actualTotalFlowM3S, 4)} m³/s` : "—"],
          ["Excess over the duty", nozzle ? `${fmt(nozzle.excessFlowPct, 2)} %` : "—"],
          ["Circulating water flow", `${fmt(candidate.waterVolumetricFlowM3S, 4)} m³/s`]
        ]
      };
    case "basin":
      return {
        ...base,
        name: "Cold-water basin under the fill",
        material: materialOf(tower),
        fields: [
          ["Record", `${tower.id} — the basin sits inside the cell record`],
          ["Basin water volume", `${fmt(dimValue("basinWaterVolumeM3"), 3)} m³`],
          ["Make-up water", `${fmt(candidate.waterBalance.makeupKgS, 4)} kg/s`],
          ["Evaporation", `${fmt(candidate.waterBalance.evaporationKgS, 4)} kg/s`],
          ["Drift loss", `${fmt(candidate.waterBalance.driftKgS, 7)} kg/s`],
          ["Blowdown", `${fmt(candidate.waterBalance.blowdownKgS, 4)} kg/s`],
          ["Circulating water flow", `${fmt(candidate.waterVolumetricFlowM3S, 4)} m³/s`]
        ]
      };
    default:
      return base;
  }
}

/* ------------------------------------------------------------------ *
 * Worked steps — the tool's Formula / With your numbers / = value idiom
 * ------------------------------------------------------------------ */

function readStep(label, why, field, value, unit, digits = 4) {
  return {
    kind: "given", label, why, field,
    formula: field,
    substitution: `${field} = ${fmt(value, digits)} ${unit}`,
    value, unit, digits, reference: "the selection result — read, not recalculated"
  };
}

function mathStep(label, why, formula, substitution, value, unit, digits = 4, reference = "arithmetic shown on the selection result") {
  return { kind: "math", label, why, formula, substitution, value, unit, digits, reference };
}

function noteStep(label, why, reference = "") {
  return { kind: "note", label, why, reference };
}

/** The four headline values for a part, for the tiles above its worked sheet. */
export function partTiles(candidate, partId, geometry, losses, checks, cap) {
  const shareOf = (key) => losses.rows.find((row) => row.key === key).sharePct;
  const dimValue = (key) => geometry.dims.find((item) => item.key === key).value;
  const util = (checkName) => {
    const found = checks.checks.find((check) => check.partId === partId && check.name === checkName);
    return found ? found.utilization : null;
  };
  const tile = (label, value, unit, digits) => ({ label, value, unit, digits });
  switch (partId) {
    case "shell":
      return [
        tile("Inlet Δp", losses.rows.find((row) => row.key === "inlet").pa, "Pa", 4),
        tile("Share of total Δp", shareOf("inlet"), "%", 2),
        tile("Footprint", candidate.tower.footprintM2, "m²", 2),
        tile("Water-flow utilisation", util("circulating water flow") ?? 0, "·", 4)
      ];
    case "fill":
      return [
        tile("Fill Δp", losses.rows.find((row) => row.key === "fill").pa, "Pa", 4),
        tile("Share of total Δp", shareOf("fill"), "%", 2),
        tile("Fill volume", dimValue("fillVolumeM3"), "m³", 3),
        tile("Fill depth as installed", candidate.fillDepthM, "m", 1)
      ];
    case "drift":
      return [
        tile("Eliminator Δp", losses.rows.find((row) => row.key === "drift").pa, "Pa", 4),
        tile("Share of total Δp", shareOf("drift"), "%", 2),
        tile("Drift delivered", candidate.airside.driftPpm, "ppm", 3),
        tile("Face velocity", candidate.airside.driftVelocityMS, "m/s", 3)
      ];
    case "fan":
      return [
        tile("Electrical input", candidate.electricalInputKW, "kW", 2),
        tile("Total air-side pressure", candidate.airside.totalPa, "Pa", 2),
        tile("Speed ratio", candidate.speedRatio, "×", 2),
        tile("Share of total Δp", shareOf("fanStack"), "%", 2)
      ];
    case "motor":
      return [
        tile("Selected rating", candidate.motor.selectedMotorKW, "kW", 2),
        tile("Required output", candidate.motor.requiredMotorOutputKW, "kW", 2),
        tile("Motor utilisation", util("motor output") ?? 0, "·", 4),
        tile("Shaft power", candidate.fanOperatingPoint.shaftPowerKW, "kW", 2)
      ];
    case "header":
      return [
        tile("Nozzles", candidate.nozzle.count, "count", 0),
        tile("Flow per nozzle", candidate.nozzle.flowPerNozzleM3S * 1000, "L/s", 3),
        tile("Excess over the duty", candidate.nozzle.excessFlowPct, "%", 2),
        tile("Circulating water", candidate.waterVolumetricFlowM3S, "m³/s", 4)
      ];
    case "basin":
      return [
        tile("Make-up water", candidate.waterBalance.makeupKgS, "kg/s", 4),
        tile("Drift loss", candidate.waterBalance.driftKgS, "kg/s", 7),
        tile("Basin water volume", dimValue("basinWaterVolumeM3"), "m³", 3),
        tile("Evaporation", candidate.waterBalance.evaporationKgS, "kg/s", 4)
      ];
    default:
      return [];
  }
}

/** The numbers panel behind a part selection: a worked sheet in the host tool's vocabulary. */
export function partSheet(candidate, partId, geometry, requirements, losses, cap, checks) {
  const tower = candidate.tower;
  const fan = candidate.fan;
  const fill = candidate.fill;
  const drift = candidate.driftEliminator;
  const motor = candidate.motor;
  const nozzle = candidate.nozzle;
  const dims = (key) => geometry.dims.find((item) => item.key === key);
  const shareOf = (key) => losses.rows.find((row) => row.key === key);
  const steps = [];

  switch (partId) {
    case "shell":
      steps.push(
        mathStep("Shell plan side from the footprint",
          "The drawing starts from the record's footprint; the fill area inside it is the bank that does the cooling.",
          "L_shell = sqrt(A_footprint)",
          dims("shellSideM").substitution, dims("shellSideM").value, "m", 4, `${tower.id}.footprintM2`),
        mathStep("Inlet louver band height",
          "The inlet area spread over the louvered faces gives the band the air enters through.",
          "h_inlet = A_inlet / (sum of louver face widths)",
          dims("inletBandHeightM").substitution, dims("inletBandHeightM").value, "m", 4, `${tower.id}.inletAreaM2`),
        mathStep("Water-flow utilisation",
          "The duty's circulating flow held against the cell's own limit.",
          "utilisation = flow / tower limit",
          `utilisation = ${n4(requirements.waterMassFlowKgS)} / ${n4(tower.maxWaterMassFlowKgS)} = ${n4(requirements.waterMassFlowKgS / tower.maxWaterMassFlowKgS)}`,
          requirements.waterMassFlowKgS / tower.maxWaterMassFlowKgS, "·", 4, "requirements.waterMassFlowKgS vs tower.maxWaterMassFlowKgS"),
        readStep("Total air-side pressure the shell carries",
          "The inlet and plenum losses are charged to this assembly; the fan must overcome the whole breakdown.",
          "candidate.airside.totalPa", candidate.airside.totalPa, "Pa", 4)
      );
      break;
    case "fill":
      steps.push(
        mathStep("Fill volume as installed",
          "The bank volume the air crosses: the record's fill area at the selected depth.",
          "V_fill = A_fill x t",
          dims("fillVolumeM3").substitution, dims("fillVolumeM3").value, "m³", 3, `${tower.id}.fillAreaM2 x candidate.fillDepthM`),
        mathStep("Share of the total air-side pressure",
          "How much of the fan's pressure is spent crossing this bank.",
          "share = dp_fill / dp_total",
          `${n4(shareOf("fill").pa)} / ${n4(losses.totalPa)} = ${n4(shareOf("fill").sharePct)} %`,
          shareOf("fill").sharePct, "%", 2, "candidate.airside.fillPa / candidate.airside.totalPa"),
        mathStep("Water-temperature utilisation",
          "The duty's hot water held against the fill's own material limit.",
          "utilisation = hot water / material limit",
          `utilisation = ${n4(requirements.hotWaterC)} / ${n4(fill.limits.maxWaterTemperatureC)} = ${n4(requirements.hotWaterC / fill.limits.maxWaterTemperatureC)}`,
          requirements.hotWaterC / fill.limits.maxWaterTemperatureC, "·", 4, "requirements.hotWaterC vs fill.limits.maxWaterTemperatureC"),
        readStep("Transfer this bank delivers",
          "The engine's own fill Merkel number at the operating point, part of the available KaV/L.",
          "candidate.airside.fillMerkelNumber", candidate.airside.fillMerkelNumber, "KaV/L", 4)
      );
      break;
    case "drift":
      steps.push(
        readStep("Drift delivered as selected",
          "Read from the selection result: what the eliminator passes at the air-side face velocity the run produced.",
          "candidate.airside.driftPpm", candidate.airside.driftPpm, "ppm", 3),
        mathStep("Drift loss",
          "Parts per million of the circulating water mass flow — the water that leaves with the air.",
          "m_drift = ppm x 1e-6 x flow",
          `m_drift = ${n4(candidate.airside.driftPpm)} x 1e-6 x ${n4(requirements.waterMassFlowKgS)} = ${n4(candidate.waterBalance.driftKgS, 9)} kg/s`,
          candidate.waterBalance.driftKgS, "kg/s", 7, "candidate.airside.driftPpm x requirements.waterMassFlowKgS"),
        mathStep("Drift-rate utilisation",
          "The delivered drift held against the requirement the run was given.",
          "utilisation = drift / requirement",
          `utilisation = ${n4(candidate.airside.driftPpm)} / ${n4(requirements.maxDriftPpm)} = ${n4(candidate.airside.driftPpm / requirements.maxDriftPpm)}`,
          candidate.airside.driftPpm / requirements.maxDriftPpm, "·", 4, "candidate.airside.driftPpm vs requirements.maxDriftPpm"),
        readStep("Face velocity at the operating point",
          "The velocity the eliminator curve was read at — the engine's own value, not recalculated here.",
          "candidate.airside.driftVelocityMS", candidate.airside.driftVelocityMS, "m/s", 3)
      );
      break;
    case "fan":
      steps.push(
        mathStep("Fan diameter from the stack area",
          "The drawing sizes the stack, the hub and the blades from this one record field.",
          "d_fan = 2 x sqrt(A_stack / pi)",
          dims("fanDiameterM").substitution, dims("fanDiameterM").value, "m", 4, `${fan.id}.stackAreaM2`),
        readStep("Operating point flow",
          "The fan/system intersection the engine solved: the flow the fan delivers against this system.",
          "candidate.fanOperatingPoint.flowM3S", candidate.fanOperatingPoint.flowM3S, "m³/s", 2),
        mathStep("Electrical input held against the requirement",
          "What the selection draws, against the maximum input the run allows.",
          "utilisation = input / maximum",
          `utilisation = ${n4(candidate.electricalInputKW)} / ${n4(requirements.maxElectricalInputKW)} = ${n4(candidate.electricalInputKW / requirements.maxElectricalInputKW)}`,
          candidate.electricalInputKW / requirements.maxElectricalInputKW, "·", 4, "candidate.electricalInputKW vs requirements.maxElectricalInputKW"),
        mathStep("Share of the total air-side pressure",
          "What this fan's own discharge adds to the pressure the fan must overcome.",
          "share = dp_fanStack / dp_total",
          `${n4(shareOf("fanStack").pa)} / ${n4(losses.totalPa)} = ${n4(shareOf("fanStack").sharePct)} %`,
          shareOf("fanStack").sharePct, "%", 2, "candidate.airside.fanStackPa / candidate.airside.totalPa")
      );
      break;
    case "motor":
      steps.push(
        readStep("Required motor output",
          "The engine's sizing rule: fan shaft power over drive efficiency, times the service factor.",
          "candidate.motor.requiredMotorOutputKW", motor.requiredMotorOutputKW, "kW", 2),
        readStep("Selected standard rating",
          "The smallest standard rating at or above the required output — the engine's own choice, read here.",
          "candidate.motor.selectedMotorKW", motor.selectedMotorKW, "kW", 2),
        mathStep("Motor utilisation",
          "How much of the selected rating the duty actually needs.",
          "utilisation = required / selected",
          `utilisation = ${n4(motor.requiredMotorOutputKW)} / ${n4(motor.selectedMotorKW)} = ${n4(motor.requiredMotorOutputKW / motor.selectedMotorKW)}`,
          motor.requiredMotorOutputKW / motor.selectedMotorKW, "·", 4, "candidate.motor.requiredMotorOutputKW vs candidate.motor.selectedMotorKW"),
        readStep("Fan shaft power",
          "The duty the motor has to carry, as solved at the fan/system intersection.",
          "candidate.fanOperatingPoint.shaftPowerKW", candidate.fanOperatingPoint.shaftPowerKW, "kW", 2)
      );
      break;
    case "header":
      steps.push(
        noteStep("Resolved nozzle record",
          `The engine sized the arrangement from the nozzle catalog at the run's pressure drop; the record it resolved is ${nozzle.nozzleId} — ${nozzle.nozzleName}.`,
          "candidate.nozzle (the nozzle arrangement over catalog.nozzles)"),
        readStep("Nozzle count",
          "How many of this nozzle cover the circulating water flow.",
          "candidate.nozzle.count", nozzle.count, "·", 0),
        readStep("Flow per nozzle",
          "The engine's discharge equation evaluated at the run's pressure drop.",
          "candidate.nozzle.flowPerNozzleM3S", nozzle.flowPerNozzleM3S * 1000, "L/s", 3),
        mathStep("Excess flow over the duty",
          "Whole nozzles cannot be split, so the arrangement covers slightly more than the circulating flow.",
          "excess = (total nozzle flow - circulating flow) / circulating flow x 100",
          `excess = (${n4(nozzle.actualTotalFlowM3S, 6)} - ${n4(candidate.waterVolumetricFlowM3S, 6)}) / ${n4(candidate.waterVolumetricFlowM3S, 6)} x 100 = ${n4(nozzle.excessFlowPct)} %`,
          nozzle.excessFlowPct, "%", 2, "candidate.nozzle vs candidate.waterVolumetricFlowM3S")
      );
      break;
    case "basin":
      steps.push(
        readStep("Make-up water as selected",
          "Read from the selection result: what the cell takes in at this duty.",
          "candidate.waterBalance.makeupKgS", candidate.waterBalance.makeupKgS, "kg/s", 4),
        mathStep("Make-up over a year",
          "The same figure held over the run's operating hours: a volume of water per year.",
          "V_year = makeup x 3600 x hours / 1000",
          `V_year = ${n4(candidate.waterBalance.makeupKgS)} x 3600 x ${n4(requirements.operatingHoursPerYear)} / 1000 = ${n4(candidate.waterBalance.makeupKgS * 3600 * requirements.operatingHoursPerYear / 1000, 1)} m³/year`,
          candidate.waterBalance.makeupKgS * 3600 * requirements.operatingHoursPerYear / 1000, "m³/year", 1, "candidate.waterBalance.makeupKgS x requirements.operatingHoursPerYear"),
        readStep("Evaporation and blowdown",
          "The water balance the engine closed for this candidate.",
          "candidate.waterBalance.evaporationKgS", candidate.waterBalance.evaporationKgS, "kg/s", 4),
        mathStep("Basin water volume",
          "Basin volume from the record's fill area at the declared basin water depth (layout constant).",
          "V_basin = A_fill x depth",
          dims("basinWaterVolumeM3").substitution, dims("basinWaterVolumeM3").value, "m³", 3, "candidate.tower.fillAreaM2 x layout constant")
      );
      break;
    default:
      break;
  }

  const label = geometry.parts.find((item) => item.id === partId).label.toLowerCase();
  return {
    title: `the numbers behind ${label}`,
    purpose: "Every value is either read from the selection result or is arithmetic on it, with the formula and the numbers that produced it.",
    steps
  };
}
