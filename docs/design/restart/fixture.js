// Preview-only fixture: the rank-1 design at the default duty, numbers from validation/regression-baseline.json
// (the "documented default duty" select step). The geometry follows shapes.md section 3; impl gets it from the engine.
window.DUTY = {
  flowM3H: 725, hotC: 42.0, coldC: 32.0, wetBulbC: 27.0, dryBulbC: 33.0, altitudeM: 0,
  rangeK: 10.0, approachK: 5.0, heatMW: 8.37,
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
window.REJECTED = [["Power limit", 249], ["Drift limit", 235], ["Thermal duty", 153], ["Drift velocity range", 125], ["Fill operating envelope", 92], ["Fill water quality", 2], ["Tower water-flow limit", 1]];
window.GEOMETRY = {
  schema: 1,
  topology: "counterflow",
  size: [8.6, 8.2, 8.6],
  cells: [{
    recordId: "IDCF-064",
    basin: { box: { centre: [0, 0.3, 0], size: [8.6, 0.6, 8.6] }, waterDepthM: 0.4 },
    shell: { box: { centre: [0, 3.3, 0], size: [8.0, 5.4, 8.0] }, wallM: 0.08 },
    inlets: [
      { face: "+z", bottomM: 0.6, heightM: 1.5, spanM: 8.0 },
      { face: "-z", bottomM: 0.6, heightM: 1.5, spanM: 8.0 },
    ],
    fill: [
      { recordId: "FILM-VF38", layer: 0, box: { centre: [0, 2.4, 0], size: [8.0, 0.6, 8.0] }, airAxis: "y" },
      { recordId: "FILM-VF38", layer: 1, box: { centre: [0, 3.0, 0], size: [8.0, 0.6, 8.0] }, airAxis: "y" },
    ],
    nozzles: { recordId: "NZ-20", count: 78, rows: 6, columns: 13, pitchM: 0.6, headerYM: 3.95, headerDiameterM: 0.2 },
    drift: { recordId: "DE-4P-ULTRA", panels: [0, 1, 2, 3].map((i) => ({ centre: [-3 + i * 2, 4.25, 0], size: [2.0, 0.2, 8.0] })) },
    plenum: { centre: [0, 5.175, 0], size: [8.0, 1.65, 8.0] },
    fan: {
      recordId: "AX-500", centre: [0, 6.35, 0], diameterM: 5.0, hubDiameterM: 0.9, bladeCount: 8, rpm: 132,
      stack: { bottomM: 6.0, topM: 8.2, throatDiameterM: 5.2, exitDiameterM: 5.9 },
    },
    drive: { motorKW: 45, gearbox: { centre: [0, 5.75, 0], size: [0.7, 0.5, 0.7] } },
  }],
  flows: { air: [], water: [] },
  dimensions: [
    { key: "fan.diameter", label: "Fan diameter", value: 5.0, unit: "m", formula: "d = √(4·A_stack / π)", substitution: "√(4 × 19.635 / π)", source: "AX-500 stackAreaM2" },
    { key: "fan.speed", label: "Fan speed", value: 88, unit: "% of nominal", formula: "chosen by the sizing", substitution: "0.88 within 0.70 to 1.13", source: "AX-500 allowedSpeedRatio" },
    { key: "fan.flow", label: "Air flow", value: 142.4, unit: "m³/s", formula: "Q = V̇ at the operating point", substitution: "fan curve ∩ system curve at 238 Pa", source: "rated" },
    { key: "fan.power", label: "Electrical input", value: 44.9, unit: "kW", formula: "P_el = P_shaft / (η_drive · η_motor)", substitution: "41.0 / (0.96 × 0.95)", source: "AX-500 driveEfficiency, motorEfficiency" },
    { key: "stack.height", label: "Stack height", value: 2.2, unit: "m", formula: "top − bottom", substitution: "8.2 − 6.0", source: "layout constant" },
  ],
};
