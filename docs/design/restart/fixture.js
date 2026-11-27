// Preview-only fixture. The default duty and its rank-1 design come from validation/regression-baseline.json (the
// "documented default duty" select step). The layer, cell, structure and pin states are layout numbers until #2, #3, #4
// land in the engine. Geometry follows shapes.md section 3: cells along +x, fill layers top first; impl gets it from the engine.
window.DUTY = {
  flowM3H: 725, hotC: 42.0, coldC: 32.0, wetBulbC: 27.0, dryBulbC: 33.0, altitudeM: 0,
  rangeK: 10.0, approachK: 5.0, heatMW: 8.37,
};
window.FILLS = {
  "FILM-VF38": { kind: "film", stepM: 0.30, maxM: 1.80, note: "38 mm film, cross-fluted" },
  "FILM-VF19": { kind: "film", stepM: 0.30, maxM: 1.50, note: "19 mm film, clean water only" },
  "SPLASH-GRID": { kind: "splash", stepM: 0.15, maxM: 2.40, note: "PP splash grid" },
  "TRICKLE-50": { kind: "trickle", stepM: 0.25, maxM: 2.00, note: "50 mm trickle grid" },
};
window.RANKED = [
  { rank: 1, cell: "IDCF-064", fill: "FILM-VF38", depth: 1.2, drift: "DE-4P-ULTRA", fan: "AX-500", speed: 88, cold: 31.70, margin: 0.30, cap: 1.093, kw: 44.9, makeup: 15.0, pa: 238, ppm: 4.14, foot: 78 },
  { rank: 2, cell: "IDCF-064", fill: "SPLASH-GRID", depth: 1.5, drift: "DE-4P-ULTRA", fan: "AX-600", speed: 78, cold: 31.67, margin: 0.33, cap: 1.094, kw: 55.4, makeup: 15.4, pa: 258, ppm: 6.83, foot: 78 },
  { rank: 3, cell: "IDCF-064", fill: "TRICKLE-50", depth: 1.2, drift: "DE-4P-ULTRA", fan: "AX-600", speed: 78, cold: 31.67, margin: 0.33, cap: 1.095, kw: 55.6, makeup: 15.3, pa: 262, ppm: 6.57, foot: 78 },
  { rank: 4, cell: "IDCF-064", fill: "SPLASH-GRID", depth: 1.5, drift: "DE-3P-10", fan: "AX-500", speed: 98, cold: 31.65, margin: 0.35, cap: 1.099, kw: 58.6, makeup: 15.4, pa: 258, ppm: 19.21, foot: 78 },
  { rank: 5, cell: "IDXF-080", fill: "SPLASH-GRID", depth: 1.5, drift: "DE-3P-10", fan: "AX-600", speed: 88, cold: 31.68, margin: 0.32, cap: 1.099, kw: 70.4, makeup: 16.0, pa: 239, ppm: 23.63, foot: 118 },
  { rank: 6, cell: "IDCF-064", fill: "SPLASH-GRID", depth: 1.8, drift: "DE-4P-ULTRA", fan: "AX-500", speed: 88, cold: 31.66, margin: 0.34, cap: 1.102, kw: 44.1, makeup: 15.1, pa: 226, ppm: 4.4, foot: 78 },
  { rank: 7, cell: "IDCF-064", fill: "TRICKLE-50", depth: 1.2, drift: "DE-3P-10", fan: "AX-500", speed: 98, cold: 31.64, margin: 0.36, cap: 1.103, kw: 59.2, makeup: 15.4, pa: 264, ppm: 18.69, foot: 78 },
  { rank: 8, cell: "IDCF-064", fill: "TRICKLE-50", depth: 1.8, drift: "DE-4P-ULTRA", fan: "AX-500", speed: 78, cold: 31.69, margin: 0.31, cap: 1.108, kw: 31.6, makeup: 14.9, pa: 191, ppm: 2.95, foot: 78 },
];
window.PINNED_FAN = [
  { rank: 1, cell: "IDCF-064", fill: "SPLASH-GRID", depth: 1.5, drift: "DE-4P-ULTRA", fan: "AX-600", speed: 78, margin: 0.33, kw: 55.4 },
  { rank: 2, cell: "IDCF-064", fill: "TRICKLE-50", depth: 1.2, drift: "DE-4P-ULTRA", fan: "AX-600", speed: 78, margin: 0.33, kw: 55.6 },
  { rank: 3, cell: "IDCF-064", fill: "FILM-VF38", depth: 0.9, drift: "DE-4P-ULTRA", fan: "AX-600", speed: 72, margin: 0.38, kw: 47.2 },
  { rank: 4, cell: "IDXF-080", fill: "SPLASH-GRID", depth: 1.5, drift: "DE-3P-10", fan: "AX-600", speed: 88, margin: 0.32, kw: 70.4 },
  { rank: 5, cell: "IDCF-064", fill: "SPLASH-GRID", depth: 1.8, drift: "DE-3P-10", fan: "AX-600", speed: 74, margin: 0.41, kw: 52.9 },
  { rank: 6, cell: "IDCF-064", fill: "TRICKLE-50", depth: 1.5, drift: "DE-4P-ULTRA", fan: "AX-600", speed: 70, margin: 0.44, kw: 46.0 },
];
window.REJECTED = [["Power limit", 249], ["Drift limit", 235], ["Thermal duty", 153], ["Drift velocity range", 125], ["Fill operating envelope", 92], ["Fill water quality", 2], ["Tower water-flow limit", 1]];

const cellGeometry = ({ recordId = "IDCF-064", basinM = 8.6, inletM = 1.5, layers, fan = "AX-500", fanM = 5.0, blades = 8, rpm = 132, drift = "DE-4P-ULTRA", motorKW = 45, x = 0 }) => {
  const shell = basinM - 0.6, inletTop = 0.6 + inletM;
  let y = inletTop + layers.reduce((s, [, d]) => s + d, 0);
  const fillTop = y;
  const fill = layers.map(([id, d], i) => { y -= d; return { recordId: id, layer: i, box: { centre: [0, y + d / 2, 0], size: [shell, d, shell] }, airAxis: "y" }; });
  const headerY = fillTop + 0.65, driftY = headerY + 0.3, stackBottom = driftY + 0.12 + 1.63, stackTop = stackBottom + 2.2 * fanM / 5;
  const rows = Math.round(shell * 0.75), columns = Math.floor((shell - 0.2) / 0.6), panels = Math.round(shell / 2);
  return {
    recordId, origin: [x, 0, 0],
    basin: { box: { centre: [0, 0.3, 0], size: [basinM, 0.6, basinM] }, waterDepthM: 0.4 },
    shell: { box: { centre: [0, (0.6 + stackBottom) / 2, 0], size: [shell, stackBottom - 0.6, shell] }, wallM: 0.08 },
    inlets: ["+z", "-z"].map((face) => ({ face, bottomM: 0.6, heightM: inletM, spanM: shell })),
    fill,
    nozzles: { recordId: "NZ-20", count: rows * columns, rows, columns, pitchM: 0.6, headerYM: headerY, headerDiameterM: 0.2 },
    drift: { recordId: drift, panels: Array.from({ length: panels }, (_, i) => ({ centre: [-shell / 2 + (i + 0.5) * shell / panels, driftY, 0], size: [shell / panels, 0.2, shell] })) },
    plenum: { centre: [0, (driftY + 0.12 + stackBottom) / 2, 0], size: [shell, 1.63, shell] },
    fan: {
      recordId: fan, centre: [0, stackBottom + 0.35, 0], diameterM: fanM, hubDiameterM: fanM * 0.18, bladeCount: blades, rpm,
      stack: { bottomM: stackBottom, topM: +stackTop.toFixed(2), throatDiameterM: fanM + 0.2, exitDiameterM: fanM + 0.9 },
    },
    drive: { motorKW, gearbox: { centre: [0, stackBottom - 0.25, 0], size: [0.7, 0.5, 0.7] } },
  };
};

window.tower = ({ cells = 1, structure = "frp", ...p }) => {
  const b = p.basinM ?? 8.6, list = Array.from({ length: cells }, (_, i) => cellGeometry({ ...p, x: (i - (cells - 1) / 2) * b }));
  const top = list[0].fan.stack.topM;
  return {
    schema: 1, topology: "counterflow", structure, size: [b * cells, top, b], cells: list, flows: { air: [], water: [] },
    dimensions: [
      { key: "fan.diameter", label: "Fan diameter", value: 5.0, unit: "m", formula: "d = √(4·A_stack / π)", substitution: "√(4 × 19.635 / π)", source: "AX-500 stackAreaM2" },
      { key: "fan.speed", label: "Fan speed", value: 88, unit: "% of nominal", formula: "chosen by the sizing", substitution: "0.88 within 0.70 to 1.13", source: "AX-500 allowedSpeedRatio" },
      { key: "fan.power", label: "Electrical input", value: 44.9, unit: "kW", formula: "P_el = P_shaft / (η_drive · η_motor)", substitution: "41.0 / (0.96 × 0.95)", source: "AX-500 driveEfficiency, motorEfficiency" },
    ],
  };
};

const film2 = [["FILM-VF38", 0.6], ["FILM-VF38", 0.6]];
window.DESIGNS = {
  base: tower({ layers: film2 }),
  layers: tower({ layers: [...film2, ["SPLASH-GRID", 0.3]] }),
  layersDrag: tower({ layers: [film2[0], ["SPLASH-GRID", 0.3], film2[1]] }),
  cells: tower({ cells: 3, recordId: "IDCF-021", basinM: 5.2, inletM: 1.2, layers: film2, fan: "AX-300", fanM: 3.0, blades: 6, rpm: 228, motorKW: 18.5 }),
  steel: tower({ structure: "steel", layers: film2 }),
  pinned: tower({ layers: [["SPLASH-GRID", 0.75], ["SPLASH-GRID", 0.75]], fan: "AX-600", fanM: 6.0, blades: 8, rpm: 104, motorKW: 55 }),
};
window.GEOMETRY = DESIGNS.base;
