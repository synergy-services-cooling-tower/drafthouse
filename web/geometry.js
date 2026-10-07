/**
 * Parametric geometry for the cooling-tower parts panel.
 *
 * ONE CODE PATH FOR THE WHOLE LIBRARY (locked decision 4). Everything below derives from a
 * catalog tower record, the fan record the selection chose, the installed fill depth and the
 * two records the selection resolved (motor, nozzle arrangement). There is no per-record
 * branch that carries hand-entered dimensions: the two topologies in the bundled catalog
 * (counterflow, crossflow) differ only in the closed-form plan rules and in where the air
 * turns, and both are computed from record fields:
 *
 *   counterflow  plan square   : shell side = sqrt(footprintM2), fill side = sqrt(fillAreaM2)
 *                                air path is vertical through the fill
 *   crossflow    twin slabs    : tower length = footprintM2 / (2 x fillDepthM + fan diameter)
 *                                bank height = fillAreaM2 / (2 x tower length)
 *                                air path is horizontal through each bank into a central plenum
 *
 * Every assembly is drawn 1:1 against a record the selection produced (seven of them):
 * cell shell -> tower, fill bank -> fill, drift bank -> drift eliminator, fan -> fan,
 * motor/gearbox -> the standard-motor selection, distribution header -> the nozzle
 * arrangement, basin -> tower (the basin sits inside the cell record). Nothing is drawn
 * "for context only" and nothing carries a hand-entered dimension.
 *
 * Illustrative layout constants (declared below, not catalog data, not physics) cover only
 * what the records leave open: wall thickness, drift-bank thickness, basin water depth, the
 * plenum and stack proportions of the fan diameter, hub/blade/gearbox/header sizes, and the
 * exploded-view travel. Rain-zone and spray-zone heights are read from the tower record when
 * it states them, and fall back to a declared constant when it does not — every dimension
 * says which of the two it used in its `source` line.
 *
 * This module is pure arithmetic and data: no DOM, no three.js. The browser viewer and the
 * Node tests import the SAME functions, so a dimension asserted in a test is the dimension
 * the browser draws.
 *
 * NUMERIC FIDELITY: every dimension is either read from a record or computed here from
 * record fields. Each derived value carries the formula and the substitution that produced
 * it, and those two strings are what the panel shows in the tool's own worked-sheet
 * vocabulary. Nothing in this file is a physics model — it is layout arithmetic, and the
 * surface says so.
 */

/** Declared illustrative layout constants. These are NOT catalog data and NOT physics. */
export const LAYOUT = Object.freeze({
  basinWaterDepthM: 0.35,
  driftBankThicknessM: 0.15,
  wallThicknessM: 0.25,
  /** Fallback rain-zone height, used only when the tower record states none. */
  rainZoneDepthM: 1.1,
  /** Fallback spray-zone height, used only when the tower record states none. */
  sprayZoneDepthM: 0.6,
  /** Plenum clear height above the drift bank / spray zone. */
  plenumHeightFactor: 0.55,
  /** Fan-stack cylinder height. */
  stackHeightFactor: 0.35,
  fanHubDiameterFactor: 0.3,
  bladeCount: 6,
  gearboxDiameterFactor: 0.22,
  gearboxHeightM: 0.6,
  headerDiameterM: 0.3,
  /** Exploded-view travel: distance multipliers along each part's separation vector. */
  explodeScale: 1.0
});

/** Round-trip-safe number text for formula strings (plain ASCII, no locale grouping). */
export function num(value, digits = 4) {
  if (!Number.isFinite(value)) return "—";
  const fixed = Number(value).toFixed(digits);
  return fixed.replace(/\.?0+$/, "") || "0";
}

function dim(key, label, value, unit, formula, substitution, source) {
  return { key, label, value, unit, formula, substitution, source };
}

function box(id, role, size, position) {
  return { id, shape: "box", role, size, position, rotation: [0, 0, 0] };
}

function cyl(id, role, radiusTop, radiusBottom, height, position, rotation = [0, 0, 0]) {
  return { id, shape: "cylinder", role, radiusTop, radiusBottom, height, position, rotation, size: [radiusTop * 2, height, radiusTop * 2] };
}

function point(x, y, z) {
  return [x, y, z];
}

/** The record identity of the standard motor the engine selected: the selected rating. */
export function motorRecordId(motor) {
  if (!motor || !Number.isFinite(motor.selectedMotorKW)) return "—";
  return `MOT-${num(motor.selectedMotorKW, 2)}`;
}

/**
 * Derive the full geometry of one tower record.
 *
 * @param {object} input
 * @param {object} input.tower      catalog tower record (the selection's candidate.tower)
 * @param {object} input.fan        the fan record the selection chose
 * @param {number} input.fillDepthM fill depth from the candidate record
 * @param {object} [input.motor]    the standard-motor selection (candidate.motor)
 * @param {object} [input.nozzle]   the resolved nozzle arrangement (candidate.nozzle)
 * @returns {{
 *   topology: string, dims: Array<object>, levels: object, notes: string[],
 *   parts: Array<object>, pickableIds: string[], flows: object, extents: object
 * }}
 */
export function deriveGeometry({ tower, fan, fillDepthM, motor = null, nozzle = null }) {
  if (!tower || !fan) throw new Error("deriveGeometry needs a tower record and a fan record.");
  if (!Number.isFinite(fillDepthM) || fillDepthM <= 0) throw new Error("deriveGeometry needs a positive fillDepthM.");

  const topology = tower.type;
  const dims = [];
  const notes = [];

  // ---- Shared: fan from its own record -------------------------------------------------
  const fanDiaM = 2 * Math.sqrt(fan.stackAreaM2 / Math.PI);
  dims.push(dim(
    "fanDiameterM", "Fan diameter from the stack area",
    fanDiaM, "m",
    "d_fan = 2 x sqrt(A_stack / pi)",
    `d_fan = 2 x sqrt(${num(fan.stackAreaM2, 3)} / 3.14159) = ${num(fanDiaM, 4)} m`,
    `${fan.id}.stackAreaM2`
  ));

  const plenumHeightM = LAYOUT.plenumHeightFactor * fanDiaM;
  const stackHeightM = LAYOUT.stackHeightFactor * fanDiaM;

  // ---- Plan rules, per topology --------------------------------------------------------
  let lanes;      // counterflow: 1 centred lane; crossflow: 2 lanes either side of the plenum
  let planLenM;   // tower length along x
  let planWidM;   // tower width along z
  let fillFaceW;  // fill bank extent along x
  let fillHeightM;// fill bank vertical extent
  let fillAirPathM;
  const plenumWidthM = fanDiaM;
  let inletFaces; // louver faces: [{normal, offset}]

  if (topology === "counterflow") {
    const shellSideM = Math.sqrt(tower.footprintM2);
    const fillSideM = Math.sqrt(tower.fillAreaM2);
    fillAirPathM = fillDepthM;
    planLenM = shellSideM;
    planWidM = shellSideM;
    fillFaceW = fillSideM;
    fillHeightM = fillDepthM;
    lanes = [{ z: 0, widthM: fillSideM, clearanceM: (shellSideM - fillSideM) / 2 }];

    dims.push(dim(
      "shellSideM", "Cell shell plan side",
      shellSideM, "m",
      "L_shell = sqrt(A_footprint)",
      `L_shell = sqrt(${num(tower.footprintM2, 2)}) = ${num(shellSideM, 4)} m`,
      `${tower.id}.footprintM2`
    ));
    dims.push(dim(
      "fillSideM", "Fill bank plan side",
      fillSideM, "m",
      "L_fill = sqrt(A_fill)",
      `L_fill = sqrt(${num(tower.fillAreaM2, 2)}) = ${num(fillSideM, 4)} m`,
      `${tower.id}.fillAreaM2`
    ));
    dims.push(dim(
      "fillClearanceM", "Structure clearance per side",
      lanes[0].clearanceM, "m",
      "c = (L_shell - L_fill) / 2",
      `c = (${num(shellSideM, 4)} - ${num(fillSideM, 4)}) / 2 = ${num(lanes[0].clearanceM, 4)} m`,
      "derived from the two lines above"
    ));
    dims.push(dim(
      "fillAirPathM", "Fill depth along the air path",
      fillAirPathM, "m",
      "t = fillDepthM (counterflow: vertical)",
      `t = ${num(fillDepthM, 2)} m`,
      "candidate.fillDepthM"
    ));
    inletFaces = [
      { normal: [0, 0, 1], offset: shellSideM / 2, axis: "x", spanM: shellSideM },
      { normal: [0, 0, -1], offset: -shellSideM / 2, axis: "x", spanM: shellSideM },
      { normal: [1, 0, 0], offset: shellSideM / 2, axis: "z", spanM: shellSideM },
      { normal: [-1, 0, 0], offset: -shellSideM / 2, axis: "z", spanM: shellSideM }
    ];
  } else if (topology === "crossflow") {
    planLenM = tower.footprintM2 / (2 * fillDepthM + fanDiaM);
    planWidM = 2 * fillDepthM + fanDiaM;
    fillFaceW = planLenM;
    fillHeightM = tower.fillAreaM2 / (2 * planLenM);
    fillAirPathM = fillDepthM;

    dims.push(dim(
      "planLenM", "Tower plan length",
      planLenM, "m",
      "L = A_footprint / (2 x t + d_fan)",
      `L = ${num(tower.footprintM2, 2)} / (2 x ${num(fillDepthM, 2)} + ${num(fanDiaM, 4)}) = ${num(planLenM, 4)} m`,
      `${tower.id}.footprintM2 with the fan diameter and the fill depth`
    ));
    dims.push(dim(
      "planWidM", "Tower plan width",
      planWidM, "m",
      "W = 2 x t + d_fan   (two banks either side of the plenum)",
      `W = 2 x ${num(fillDepthM, 2)} + ${num(fanDiaM, 4)} = ${num(planWidM, 4)} m`,
      "derived: fill depth and fan diameter"
    ));
    dims.push(dim(
      "fillHeightM", "Fill bank vertical height",
      fillHeightM, "m",
      "h = A_fill / (2 x L)   (two banks, water-side face area)",
      `h = ${num(tower.fillAreaM2, 2)} / (2 x ${num(planLenM, 4)}) = ${num(fillHeightM, 4)} m`,
      `${tower.id}.fillAreaM2`
    ));
    dims.push(dim(
      "fillAirPathM", "Fill depth along the air path",
      fillAirPathM, "m",
      "t = fillDepthM (crossflow: horizontal)",
      `t = ${num(fillDepthM, 2)} m`,
      "candidate.fillDepthM"
    ));
    notes.push(
      `Crossflow topology: air crosses each bank horizontally (${num(fillAirPathM, 2)} m) and turns up a central `
      + `plenum ${num(plenumWidthM, 4)} m wide — the fan diameter, so the plenum clears the fan.`
    );
    const bankZ = plenumWidthM / 2 + fillDepthM / 2;
    lanes = [
      { z: bankZ, widthM: fillDepthM, heightM: fillHeightM },
      { z: -bankZ, widthM: fillDepthM, heightM: fillHeightM }
    ];
    inletFaces = [
      { normal: [0, 0, 1], offset: planWidM / 2, axis: "x", spanM: planLenM },
      { normal: [0, 0, -1], offset: -planWidM / 2, axis: "x", spanM: planLenM }
    ];
  } else {
    throw new Error(`Unsupported tower topology "${topology}" — the catalog records counterflow and crossflow.`);
  }

  // ---- Shared: water-side level stack --------------------------------------------------
  const basinTopM = LAYOUT.basinWaterDepthM;
  const recordRainZone = Number.isFinite(tower.rainZoneHeightM) ? tower.rainZoneHeightM : null;
  const recordSprayZone = Number.isFinite(tower.sprayZoneHeightM) ? tower.sprayZoneHeightM : null;
  const rainZoneHeightM = recordRainZone ?? LAYOUT.rainZoneDepthM;
  const sprayZoneHeightM = recordSprayZone ?? LAYOUT.sprayZoneDepthM;
  const driftThicknessM = LAYOUT.driftBankThicknessM;

  const rainBottomM = basinTopM;
  const rainTopM = rainBottomM + rainZoneHeightM;
  const fillBottomM = rainTopM;
  const fillTopM = fillBottomM + fillHeightM;
  const sprayTopM = fillTopM + sprayZoneHeightM;
  const driftBottomM = sprayTopM;
  const driftTopM = driftBottomM + driftThicknessM;
  const plenumTopM = driftTopM + plenumHeightM;
  const stackTopM = plenumTopM + stackHeightM;

  dims.push(dim(
    "rainZoneHeightM", "Rain-zone height",
    rainZoneHeightM, "m",
    recordRainZone === null ? `h_rain = ${LAYOUT.rainZoneDepthM} m (layout constant)` : "h_rain = tower.rainZoneHeightM",
    recordRainZone === null ? `h_rain = ${num(rainZoneHeightM, 2)} m` : `h_rain = ${num(rainZoneHeightM, 2)} m`,
    recordRainZone === null
      ? "layout constant — this record states no rain-zone height"
      : `${tower.id}.rainZoneHeightM`
  ));
  dims.push(dim(
    "sprayZoneHeightM", "Spray-zone height",
    sprayZoneHeightM, "m",
    recordSprayZone === null ? `h_spray = ${LAYOUT.sprayZoneDepthM} m (layout constant)` : "h_spray = tower.sprayZoneHeightM",
    recordSprayZone === null ? `h_spray = ${num(sprayZoneHeightM, 2)} m` : `h_spray = ${num(sprayZoneHeightM, 2)} m`,
    recordSprayZone === null
      ? "layout constant — this record states no spray-zone height"
      : `${tower.id}.sprayZoneHeightM`
  ));

  // ---- Shared: drift face scaled from the fill face -------------------------------------
  const driftScale = Math.sqrt(tower.driftAreaM2 / tower.fillAreaM2);
  dims.push(dim(
    "driftScale", "Drift face scale against the fill face",
    driftScale, "·",
    "s = sqrt(A_drift / A_fill)",
    `s = sqrt(${num(tower.driftAreaM2, 2)} / ${num(tower.fillAreaM2, 2)}) = ${num(driftScale, 5)}`,
    `${tower.id}.driftAreaM2 / ${tower.id}.fillAreaM2`
  ));

  // ---- Shared: inlet louver band -------------------------------------------------------
  const inletPerimeterM = inletFaces.reduce((sum, face) => sum + face.spanM, 0);
  const inletBandHeightM = tower.inletAreaM2 / inletPerimeterM;
  dims.push(dim(
    "inletBandHeightM", "Inlet louver band height",
    inletBandHeightM, "m",
    "h_inlet = A_inlet / (sum of louver face widths)",
    `h_inlet = ${num(tower.inletAreaM2, 2)} / ${num(inletPerimeterM, 4)} = ${num(inletBandHeightM, 4)} m`,
    `${tower.id}.inletAreaM2`
  ));
  if (inletBandHeightM > rainZoneHeightM) {
    notes.push(
      `The louver band (${num(inletBandHeightM, 3)} m) is taller than the ${num(rainZoneHeightM, 2)} m rain zone in this `
      + `record, so the band is drawn from the basin top upward and overlaps the fill face by `
      + `${num(inletBandHeightM - rainZoneHeightM, 3)} m.`
    );
  }

  // ---- Shared: plenum, stack, totals ---------------------------------------------------
  dims.push(dim(
    "plenumHeightM", "Plenum clear height above the drift bank",
    plenumHeightM, "m",
    `h_plenum = ${LAYOUT.plenumHeightFactor} x d_fan`,
    `h_plenum = ${LAYOUT.plenumHeightFactor} x ${num(fanDiaM, 4)} = ${num(plenumHeightM, 4)} m`,
    "layout constant x fan diameter"
  ));
  dims.push(dim(
    "stackHeightM", "Fan-stack cylinder height",
    stackHeightM, "m",
    `h_stack = ${LAYOUT.stackHeightFactor} x d_fan`,
    `h_stack = ${LAYOUT.stackHeightFactor} x ${num(fanDiaM, 4)} = ${num(stackHeightM, 4)} m`,
    "layout constant x fan diameter"
  ));
  dims.push(dim(
    "totalHeightM", "Overall height above the basin floor",
    stackTopM, "m",
    "H = basin + rain + fill + spray + drift + plenum + stack",
    `H = ${num(basinTopM, 2)} + ${num(rainZoneHeightM, 2)} + ${num(fillHeightM, 4)} + ${num(sprayZoneHeightM, 2)}`
      + ` + ${num(driftThicknessM, 2)} + ${num(plenumHeightM, 4)} + ${num(stackHeightM, 4)} = ${num(stackTopM, 4)} m`,
    "sum of the levels below"
  ));
  dims.push(dim(
    "basinWaterVolumeM3", "Basin water volume",
    tower.fillAreaM2 * LAYOUT.basinWaterDepthM, "m³",
    `V_basin = A_fill x ${LAYOUT.basinWaterDepthM}`,
    `V_basin = ${num(tower.fillAreaM2, 2)} x ${LAYOUT.basinWaterDepthM} = ${num(tower.fillAreaM2 * LAYOUT.basinWaterDepthM, 3)} m³`,
    "fill area x basin water depth (layout constant)"
  ));
  dims.push(dim(
    "fillVolumeM3", "Fill volume",
    tower.fillAreaM2 * fillAirPathM, "m³",
    "V_fill = A_fill x t",
    `V_fill = ${num(tower.fillAreaM2, 2)} x ${num(fillAirPathM, 2)} = ${num(tower.fillAreaM2 * fillAirPathM, 3)} m³`,
    "fill area x fill depth"
  ));

  notes.push(
    `Illustrative layout constants (not catalog data): basin water depth ${LAYOUT.basinWaterDepthM} m, drift bank `
    + `thickness ${LAYOUT.driftBankThicknessM} m, wall thickness ${LAYOUT.wallThicknessM} m, plenum `
    + `${LAYOUT.plenumHeightFactor} x fan diameter, stack ${LAYOUT.stackHeightFactor} x fan diameter.`
    + (recordRainZone === null ? ` Rain zone ${LAYOUT.rainZoneDepthM} m (this record states none).` : "")
    + (recordSprayZone === null ? ` Spray zone ${LAYOUT.sprayZoneDepthM} m (this record states none).` : "")
  );
  if (tower.airFreeAreaM2 !== tower.fillAreaM2) {
    notes.push(
      `airFreeAreaM2 (${num(tower.airFreeAreaM2, 2)} m²) differs from fillAreaM2 (${num(tower.fillAreaM2, 2)} m²) in this `
      + `record; the drawing uses fillAreaM2 and does not model sheet blockage inside the bank.`
    );
  }

  // ---- Parts ---------------------------------------------------------------------------
  const wallT = LAYOUT.wallThicknessM;
  const parts = [];

  // 1. Cell shell — walls, base, top deck, louver panels.
  {
    const solids = [];
    const shellH = plenumTopM;
    if (topology === "counterflow") {
      const side = planLenM;
      solids.push(box("shell-wall-z+", "body", [side, shellH, wallT], [0, shellH / 2, side / 2 - wallT / 2]));
      solids.push(box("shell-wall-z-", "body", [side, shellH, wallT], [0, shellH / 2, -side / 2 + wallT / 2]));
      solids.push(box("shell-wall-x+", "body", [wallT, shellH, side - 2 * wallT], [side / 2 - wallT / 2, shellH / 2, 0]));
      solids.push(box("shell-wall-x-", "body", [wallT, shellH, side - 2 * wallT], [-side / 2 + wallT / 2, shellH / 2, 0]));
      solids.push(box("shell-deck", "body", [side, wallT, side], [0, plenumTopM - wallT / 2, 0]));
    } else {
      const L = planLenM;
      const W = planWidM;
      solids.push(box("shell-wall-x+", "body", [wallT, shellH, W], [L / 2 - wallT / 2, shellH / 2, 0]));
      solids.push(box("shell-wall-x-", "body", [wallT, shellH, W], [-L / 2 + wallT / 2, shellH / 2, 0]));
      solids.push(box("shell-wall-z+", "body", [L - 2 * wallT, shellH, wallT], [0, shellH / 2, W / 2 - wallT / 2]));
      solids.push(box("shell-wall-z-", "body", [L - 2 * wallT, shellH, wallT], [0, shellH / 2, -W / 2 + wallT / 2]));
      solids.push(box("shell-deck", "body", [L, wallT, W], [0, plenumTopM - wallT / 2, 0]));
    }
    // Louver slats (detail meshes: drawn, never pickable).
    const slatCount = 4;
    const slatGap = inletBandHeightM / slatCount;
    for (const face of inletFaces) {
      for (let i = 0; i < slatCount; i += 1) {
        const y = basinTopM + slatGap * (i + 0.5);
        const horizontalAlongX = face.axis === "x";
        const size = horizontalAlongX ? [face.spanM - 2 * wallT, slatGap * 0.42, wallT * 0.5] : [wallT * 0.5, slatGap * 0.42, face.spanM - 2 * wallT];
        const position = horizontalAlongX ? [0, y, face.offset - Math.sign(face.offset) * wallT * 0.25] : [face.offset - Math.sign(face.offset) * wallT * 0.25, y, 0];
        solids.push(box(`louver-${face.axis}-${face.offset > 0 ? "p" : "n"}-${i}`, "detail", size, position));
      }
    }
    parts.push({
      id: "shell",
      pickable: true,
      order: 1,
      label: "Cell shell",
      recordId: tower.id,
      recordName: tower.name,
      recordSource: "candidate.tower (catalog.towers entry)",
      solids,
      explode: [0, -0.35, 0],
      wallSpread: true,
      anchor: [0, plenumTopM + 0.35, planWidM / 2 + 0.4],
      files: [
        `${tower.id}.type = ${tower.type} (${tower.draftType})`,
        `${tower.id}.maxWaterMassFlowKgS = ${num(tower.maxWaterMassFlowKgS, 3)}`,
        `${tower.id}.footprintM2 = ${num(tower.footprintM2, 2)}`
      ]
    });
  }

  // 2. Fill bank(s)
  {
    const solids = [];
    if (topology === "counterflow") {
      const side = Math.sqrt(tower.fillAreaM2);
      solids.push(box("fill-body", "body", [side, fillHeightM, side], [0, (fillBottomM + fillTopM) / 2, 0]));
      const ribs = 5;
      for (let i = 1; i <= ribs; i += 1) {
        const z = -side / 2 + (side * i) / (ribs + 1);
        solids.push(box(`fill-sheet-${i}`, "detail", [side * 0.98, fillHeightM * 0.98, 0.05], [0, (fillBottomM + fillTopM) / 2, z]));
      }
    } else {
      for (const [index, lane] of lanes.entries()) {
        const tag = index === 0 ? "p" : "n";
        solids.push(box(`fill-body-${tag}`, "body", [fillFaceW, fillHeightM, fillAirPathM], [0, (fillBottomM + fillTopM) / 2, lane.z]));
        const ribs = 4;
        for (let i = 1; i <= ribs; i += 1) {
          const x = -fillFaceW / 2 + (fillFaceW * i) / (ribs + 1);
          solids.push(box(`fill-sheet-${tag}-${i}`, "detail", [0.05, fillHeightM * 0.98, fillAirPathM * 0.98], [x, (fillBottomM + fillTopM) / 2, lane.z]));
        }
      }
    }
    parts.push({
      id: "fill",
      pickable: true,
      order: 2,
      label: "Fill bank",
      recordId: null, // bound by numbers.js from candidate.fill
      recordName: "Fill bank as installed",
      recordSource: "candidate.fill at candidate.fillDepthM",
      solids,
      explode: [0, topology === "counterflow" ? -1.5 : -1.1, 0],
      anchor: [0, fillTopM + 0.25, planWidM / 2 + 0.2],
      files: [
        `fillAreaM2 = ${num(tower.fillAreaM2, 2)}`,
        `fillDepthM = ${num(fillDepthM, 2)}`,
        `fillHeightM = ${num(fillHeightM, 4)}`
      ]
    });
  }

  // 3. Drift eliminator bank(s)
  {
    const solids = [];
    if (topology === "counterflow") {
      const side = Math.sqrt(tower.driftAreaM2);
      solids.push(box("drift-body", "body", [side, driftThicknessM, side], [0, (driftBottomM + driftTopM) / 2, 0]));
    } else {
      for (const [index, lane] of lanes.entries()) {
        const tag = index === 0 ? "p" : "n";
        const slabHeight = fillHeightM * driftScale;
        solids.push(box(
          `drift-body-${tag}`, "body",
          [fillFaceW * driftScale, slabHeight, driftThicknessM],
          [0, fillBottomM + slabHeight / 2, Math.sign(lane.z) * (plenumWidthM / 2 + driftThicknessM / 2)]
        ));
      }
    }
    parts.push({
      id: "drift",
      pickable: true,
      order: 3,
      label: "Drift eliminator bank",
      recordId: null, // bound by numbers.js from candidate.driftEliminator
      recordName: "Drift eliminator bank as installed",
      recordSource: "candidate.driftEliminator",
      solids,
      explode: [0, topology === "counterflow" ? 2.0 : 1.5, 0],
      anchor: [planLenM / 2 + 0.5, driftTopM + 0.3, 0],
      files: [`driftAreaM2 = ${num(tower.driftAreaM2, 2)}`, `driftScale = ${num(driftScale, 5)}`]
    });
  }

  // 4. Fan — rotor, hub and the stack it discharges through.
  {
    const solids = [];
    const hubDia = LAYOUT.fanHubDiameterFactor * fanDiaM;
    solids.push(cyl("fan-stack", "body", fanDiaM / 2, fanDiaM / 2, stackHeightM, [0, plenumTopM + stackHeightM / 2, 0]));
    solids.push(cyl("fan-hub", "detail", hubDia / 2, hubDia / 2, 0.25 * fanDiaM, [0, plenumTopM + 0.125 * fanDiaM, 0]));
    const blades = LAYOUT.bladeCount;
    for (let i = 0; i < blades; i += 1) {
      const angle = (i / blades) * Math.PI * 2;
      const r = fanDiaM / 2 * 0.62;
      solids.push({
        id: `fan-blade-${i}`,
        shape: "box",
        role: "detail",
        size: [fanDiaM * 0.42, 0.06, fanDiaM * 0.16],
        position: [Math.cos(angle) * r, plenumTopM + 0.06, Math.sin(angle) * r],
        rotation: [0.18, -angle, 0]
      });
    }
    parts.push({
      id: "fan",
      pickable: true,
      order: 4,
      label: "Fan",
      recordId: fan.id,
      recordName: fan.name,
      recordSource: "candidate.fan",
      solids,
      explode: [0, 2.9, 0],
      anchor: [fanDiaM / 2 + 0.6, plenumTopM + stackHeightM * 0.6, 0],
      files: [
        `stackAreaM2 = ${num(fan.stackAreaM2, 3)}`,
        `pressureBasis = ${fan.pressureBasis}`,
        `d_fan = ${num(fanDiaM, 4)} m`
      ]
    });
  }

  // 5. Motor / gearbox — under the fan deck, inside the plenum. Bound to the standard-motor
  //    selection the engine produced (candidate.motor).
  {
    const solids = [];
    const gearDia = LAYOUT.gearboxDiameterFactor * fanDiaM;
    solids.push(cyl("gearbox", "body", gearDia / 2, gearDia / 2, LAYOUT.gearboxHeightM, [0, plenumTopM - LAYOUT.gearboxHeightM / 2 - 0.1, 0]));
    solids.push(box("motor-body", "body", [gearDia * 1.1, gearDia * 0.75, gearDia * 0.95], [0, plenumTopM - LAYOUT.gearboxHeightM - 0.1 - gearDia * 0.4, 0]));
    parts.push({
      id: "motor",
      pickable: true,
      order: 5,
      label: "Motor / gearbox",
      recordId: motorRecordId(motor),
      recordName: Number.isFinite(motor?.selectedMotorKW)
        ? `Standard motor, ${num(motor.selectedMotorKW, 2)} kW selected`
        : "Standard motor selection",
      recordSource: "candidate.motor (standard-motor selection from the fan shaft power)",
      solids,
      explode: [0, 1.5, 0],
      anchor: [-gearDia, plenumTopM - LAYOUT.gearboxHeightM, 0],
      files: [`gearboxDiameter = ${num(LAYOUT.gearboxDiameterFactor, 2)} x fan diameter (layout constant)`]
    });
  }

  // 6. Distribution header — bound to the nozzle arrangement the engine resolved.
  {
    const solids = [];
    const headerY = fillTopM + sprayZoneHeightM * 0.55;
    const headerR = LAYOUT.headerDiameterM / 2;
    if (topology === "counterflow") {
      solids.push(cyl("header-main", "body", headerR, headerR, Math.sqrt(tower.fillAreaM2), [0, headerY, 0], [0, 0, Math.PI / 2]));
      const branches = 3;
      for (let i = 0; i < branches; i += 1) {
        const x = -fillFaceW / 2 + (fillFaceW * (i + 1)) / (branches + 1);
        solids.push(cyl(`header-branch-${i}`, "detail", headerR * 0.6, headerR * 0.6, fillFaceW * 0.96, [x, headerY, 0], [Math.PI / 2, 0, 0]));
      }
    } else {
      for (const [index, lane] of lanes.entries()) {
        const tag = index === 0 ? "p" : "n";
        solids.push(cyl(`header-main-${tag}`, "body", headerR, headerR, fillFaceW * 0.94, [0, headerY, lane.z], [0, 0, Math.PI / 2]));
      }
    }
    parts.push({
      id: "header",
      pickable: true,
      order: 6,
      label: "Distribution header",
      recordId: nozzle?.nozzleId ?? null,
      recordName: nozzle?.nozzleName ?? "Nozzle arrangement as selected",
      recordSource: "candidate.nozzle (nozzle arrangement over catalog.nozzles)",
      solids,
      explode: [0, 1.0, 0],
      anchor: [-fillFaceW / 2 - 0.5, headerY, 0],
      files: [`headerDiameterM = ${LAYOUT.headerDiameterM} (layout constant)`]
    });
  }

  // 7. Basin — floor slab and the water it holds.
  {
    const solids = [];
    const L = planLenM;
    const W = topology === "counterflow" ? planLenM : planWidM;
    solids.push(box("basin-floor", "body", [L, basinTopM, W], [0, basinTopM / 2, 0]));
    parts.push({
      id: "basin",
      pickable: true,
      order: 7,
      label: "Basin",
      recordId: null, // bound by numbers.js from candidate.tower (basin inside the cell record)
      recordName: "Cold-water basin under the fill",
      recordSource: "candidate.tower (basin sits inside the cell record)",
      solids,
      explode: [0, -1.3, 0],
      anchor: [0, basinTopM + 0.2, -planWidM / 2 - 0.5],
      files: [`basinWaterVolumeM3 = ${num(tower.fillAreaM2 * LAYOUT.basinWaterDepthM, 3)}`]
    });
  }

  // ---- Flow paths ----------------------------------------------------------------------
  const air = [];
  const water = [];
  if (topology === "counterflow") {
    const side = planLenM / 2;
    for (const [sx, sz] of [[0, 1], [0, -1], [1, 0], [-1, 0]]) {
      air.push([
        point(sx * (side + 1.6), basinTopM + inletBandHeightM * 0.5, sz * (side + 1.6)),
        point(sx * (side + 0.05), basinTopM + inletBandHeightM * 0.5, sz * (side + 0.05)),
        point(sx * side * 0.45, rainBottomM + 0.35, sz * side * 0.45),
        point(0, fillBottomM + fillHeightM * 0.4, 0),
        point(0, driftTopM + plenumHeightM * 0.5, 0),
        point(0, plenumTopM, 0),
        point(0, stackTopM + 0.9, 0)
      ]);
    }
    water.push([
      point(0, fillTopM + sprayZoneHeightM * 0.55, 0),
      point(0, sprayTopM - 0.1, 0),
      point(0, fillBottomM + fillHeightM * 0.35, 0),
      point(0, rainBottomM + rainZoneHeightM * 0.4, 0),
      point(0, basinTopM * 0.5, 0)
    ]);
  } else {
    for (const lane of lanes) {
      const sign = Math.sign(lane.z);
      air.push([
        point(0, fillBottomM + fillHeightM * 0.42, sign * (planWidM / 2 + 1.6)),
        point(0, fillBottomM + fillHeightM * 0.42, sign * (plenumWidthM / 2 + fillAirPathM * 0.5)),
        point(0, fillBottomM + fillHeightM * 0.42, sign * (plenumWidthM / 2 * 0.4)),
        point(0, driftTopM + plenumHeightM * 0.55, 0),
        point(0, plenumTopM, 0),
        point(0, stackTopM + 0.9, 0)
      ]);
      water.push([
        point(0, fillTopM + sprayZoneHeightM * 0.55, lane.z),
        point(0, fillTopM - 0.05, lane.z),
        point(0, fillBottomM + fillHeightM * 0.4, lane.z),
        point(0, rainBottomM + rainZoneHeightM * 0.45, lane.z),
        point(0, basinTopM * 0.5, lane.z)
      ]);
    }
  }

  // ---- Extents (framing) ---------------------------------------------------------------
  const extents = {
    minY: 0,
    maxY: stackTopM,
    halfX: Math.max(planLenM, planWidM) / 2,
    halfZ: Math.max(planLenM, planWidM) / 2,
    spanM: Math.max(planLenM, planWidM, stackTopM)
  };

  return {
    topology,
    dims,
    notes,
    levels: {
      basinTopM, rainBottomM, rainTopM, fillBottomM, fillTopM, sprayTopM,
      driftBottomM, driftTopM, plenumTopM, stackTopM,
      fillHeightM, fillAirPathM, plenumWidthM, driftScale,
      inletBandHeightM, planLenM, planWidM, fanDiaM,
      rainZoneHeightM, sprayZoneHeightM, driftThicknessM
    },
    parts,
    /** Every assembly maps 1:1 to a record the selection produced. */
    pickableIds: parts.filter((part) => part.pickable).map((part) => part.id),
    contextIds: parts.filter((part) => !part.pickable).map((part) => part.id),
    flows: { air, water },
    extents
  };
}

/** The seven record-backed assemblies, in rail order. */
export const PART_ORDER = ["shell", "fill", "drift", "fan", "motor", "header", "basin"];
