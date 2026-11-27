// Preview-only: fills the cockpit from the fixture and draws one state per URL. ?s=layers|cells|structure|parts|compare picks
// the state, ?ga the GA sheet, ?refused the refused duty, ?sheet=inputs|build|ranking|calc|parts|compare the phone sheet, ?code=asce the ASCE 7 load code, ?light,
// ?poster. Not product code.
import { mountModel, mountGA } from "./model.js";

const qs = new URLSearchParams(location.search), $ = (s) => document.querySelector(s);
const phone = innerWidth < 700, W = innerWidth, H = innerHeight;
const S = qs.get("s") || "base", ga = qs.has("ga"), refused = qs.has("refused"), sheet = qs.get("sheet"), light = qs.has("light");
const add = (html, style = "") => { document.body.insertAdjacentHTML("beforeend", style ? `<div style="${style}">${html}</div>` : html); return document.body.lastElementChild; };
document.body.classList.toggle("ga", ga);
document.body.classList.toggle("refused", refused);
document.querySelectorAll("#mode span").forEach((s) => s.classList.toggle("on", (s.dataset.m === "ga") === ga));
$("#theme").innerHTML = light
  ? `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/></svg>`
  : `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>`;

const PIN = `<svg viewBox="0 0 12 12" fill="currentColor"><path d="M4 1h4l-.6 3.2L9.5 6v1H6.5v4L6 11.5 5.5 11V7h-3V6l2.1-1.8z"/></svg>`;
const ICON = {
  fill: `<svg viewBox="0 0 28 22" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="2" y="3" width="24" height="16" rx="1"/><path d="M6 3l3 8-3 8M12 3l3 8-3 8M18 3l3 8-3 8"/></svg>`,
  drift: `<svg viewBox="0 0 28 22" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M3 8l4-4 4 4 4-4 4 4 4-4 2 2M3 16l4-4 4 4 4-4 4 4 4-4 2 2"/></svg>`,
  fan: `<svg viewBox="0 0 28 22" fill="none" stroke="currentColor" stroke-width="1.2"><circle cx="14" cy="11" r="9"/><circle cx="14" cy="11" r="1.8"/><path d="M14 9.2c-1-3 0-5.5 2.5-6.6M15.6 11.8c3 .6 4.6 2.8 4.3 5.4M12.4 11.8c-2.4 2-5.1 2-7-.2"/></svg>`,
  nozzles: `<svg viewBox="0 0 28 22" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M2 5h24M7 5v3M14 5v3M21 5v3"/><path d="M7 11v2M5 15v2M9 15v2M14 11v2M12 15v2M16 15v2M21 11v2M19 15v2M23 15v2" stroke-dasharray="1.5 1.5"/></svg>`,
  casing: `<svg viewBox="0 0 28 22" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M4 7l10-4 10 4v11l-10 3-10-3z"/><path d="M4 7l10 3 10-3M14 10v11"/></svg>`,
  frp: `<svg viewBox="0 0 30 22" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="4" y="3" width="22" height="17" rx="1"/><path d="M11.3 3v17M18.6 3v17"/></svg>`,
  steel: `<svg viewBox="0 0 30 22" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M4 3v17M15 3v17M26 3v17M4 3h22M4 20h22M4 3l11 17M15 3 4 20M15 3l11 17M26 3 15 20"/></svg>`,
  concrete: `<svg viewBox="0 0 30 22" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="4" y="3" width="22" height="17"/><rect x="4" y="13" width="22" height="7" stroke-width="2.2"/><path d="M9 7h1M16 9h1M21 6h1M12 16h1M19 17h1" stroke-width="1.6"/></svg>`,
};

const geom = { base: DESIGNS.base, layers: phone && sheet === "build" ? DESIGNS.layersDrag : DESIGNS.layers, cells: DESIGNS.cells, structure: DESIGNS.steel, parts: DESIGNS.pinned, compare: DESIGNS.base }[S];
const leftPane = { layers: "build", cells: "build", structure: "build" }[S] || "duty";
const picks = { base: "fan", compare: "fan", layers: phone && sheet === "build" ? "fill:1" : "fill:2", structure: "frame" };

$("#dstate").textContent = refused ? "kept from 27.0 °C" : { cells: "3 cells · rank 1 of 164", parts: "fan pinned · rank 1 of 58", layers: "edited · re-ranking" }[S] || "rank 1 of 202";
if (phone) $("#scen b").textContent = "Plant B peak";

$("#lt-duty").classList.toggle("on", leftPane === "duty");
$("#lt-build").classList.toggle("on", leftPane === "build");
const sheetPane = sheet === "build" ? "build" : sheet === "inputs" ? "duty" : leftPane;
$(`#p-${sheetPane}`).classList.add("on");

const drag = sheet === "inputs" && !refused;
const wb = refused ? 33.0 : 27.0;
const fields = [
  ["Water flow", "725", "m³/h", 100, 1500, 725],
  ["Hot water in", "42.0", "°C", 25, 60, 42],
  ["Cold water wanted", "32.0", "°C", 20, 45, 32],
  ["Wet bulb", wb.toFixed(1), "°C", 10, 35, wb, refused ? "bad" : drag ? "drag" : ""],
  ["Dry bulb", "33.0", "°C", 10, 50, 33],
  ["Altitude", "0", "m", 0, 3000, 0],
];
$("#duty").innerHTML = fields.map(([l, v, u, lo, hi, x, st = ""]) => {
  const p = ((x - lo) / (hi - lo)) * 100;
  const err = st === "bad" ? `<p class="err">Wet bulb must be below the cold water wanted, 32.0 °C. Lower it, or raise the cold water.</p>` : "";
  return `<div class="f ${st}"><label>${l}</label><span class="in"><b>${v}</b><span>${u}</span></span><div class="sl"><div class="t"><i style="width:${p}%"></i></div><b class="k" style="left:${p}%"></b></div>${err}</div>`;
}).join("");
$("#derived").innerHTML = [["Range", "10.0", "K"], ["Approach", refused ? "−1.0" : "5.0", "K"], ["Heat load", "8.37", "MW"]]
  .map(([l, v, u]) => `<div><span>${l}</span><b ${l === "Approach" && refused ? 'style="color:var(--bad)"' : ""}>${v}<small>${u}</small></b></div>`).join("");
if (drag) $("#rated").textContent = "rated 4 ms ago, following the slider";
if (S === "layers") $("#rated").textContent = "re-rated 5 ms ago";
if (refused) $("#rated").outerHTML = `<span class="q" style="color:var(--bad);font-size:11px">refused</span>`;

const layersOf = (g) => g.cells[0].fill.map((f) => [f.recordId, f.box.size[1]]);
const layerRow = ([id, d], i, { on = false, lift = false, snap = false } = {}) => {
  const f = FILLS[id], n = Math.round(f.maxM / f.stepM), k = Math.round(d / f.stepM), pct = `${(k / n) * 100}%`;
  return `<div class="ly ${on ? "on" : ""} ${lift ? "lift" : ""}"><i class="grip">⋮⋮</i><span class="n">${i + 1}</span><span class="in sel">${id}<i>▾</i></span><span class="x">×</span>
    <div class="rl"><span class="ticks" style="--d:${pct}">${Array.from({ length: n }, (_, j) => `<i class="${j < k ? "f" : ""}"></i>`).join("")}<b class="k"></b></span><span class="v">${d.toFixed(2)} m</span></div>${snap ? `<span class="snapn">${k} modules × ${f.stepM.toFixed(2)} m · snapped</span>` : ""}</div>`;
};
const struct = S === "structure" ? "steel" : "frp", cells = S === "cells" ? 3 : 1, code = qs.get("code") === "asce" ? "asce" : "dpt";
const site = {
  dpt: [["Wind zone", "1 · 25 m/s"], ["Terrain", "A · open"], ["Importance", "1.0"], ["Province", "Rayong"], ["Site class", "D"]],
  asce: [["Basic wind speed", "30 m/s"], ["Exposure", "C"], ["K<sub>zt</sub>", "1.00"], ["S<sub>s</sub> · S<sub>1</sub>", "0.12 · 0.05 g"], ["Site class", "D"]],
}[code];
const layers = layersOf(geom), depth = layers.reduce((s, [, d]) => s + d, 0);
let rows;
if (S === "layers" && phone && sheet === "build") rows = layerRow(layers[0], 0) + `<div class="ly slot"></div>` + layerRow(["SPLASH-GRID", 0.3], 1, { lift: true }) + layerRow(layers[2], 2);
else rows = layers.map((l, i) => layerRow(l, i, { on: S === "layers" && i === 2, snap: S === "layers" && i === 2 })).join("");
const groups = [`  <div class="grp"><span class="eyebrow">Cells</span>
    <div class="f"><label>Identical cells in a row</label><span class="stp ${S === "cells" ? "focus" : ""}"><span>−</span><b>${cells}</b><span>+</span></span></div>
    <dl class="kv" style="margin-top:10px"><dt>Flow a cell</dt><dd>${(725 / cells).toFixed(1)} m³/h</dd><span></span></dl></div>
  <div class="grp"><span class="eyebrow">Structure</span>
    <div class="seg3">${[["frp", "FRP"], ["steel", "Steel"], ["concrete", "Concrete"]].map(([k, l]) => `<span class="${k === struct ? "on" : ""}">${ICON[k]}${l}</span>`).join("")}</div>
    <span class="eyebrow" style="display:block;margin:12px 0 8px">Load code</span><div class="seg3" style="grid-template-columns:repeat(2,1fr)">${[["dpt", "Thai DPT"], ["asce", "ASCE 7"]].map(([k, l]) => `<span class="${k === code ? "on" : ""}">${l}</span>`).join("")}</div>
    <dl class="kv" style="margin-top:10px">${site.map(([t, v]) => `<dt>${t}</dt><dd>${v}</dd><span class="tag">site</span>`).join("")}</dl></div>
`, `  <div class="grp"><div class="lyh"><span class="eyebrow">Fill layers · top first</span><b>${depth.toFixed(2)} m</b></div>${rows}
    <span class="addly">+ Add layer</span></div>`];
$("#build").innerHTML = (phone && S !== "structure" ? [groups[1], groups[0]] : groups).join("");

const base = RANKED[0];
const cellsRank = [
  { rank: 1, cell: "3 × IDCF-021", fill: "FILM-VF38", depth: 1.2, drift: "DE-4P-ULTRA", fan: "AX-300", speed: 90, margin: 0.31, kw: 46.8 },
  { rank: 2, cell: "3 × IDCF-021", fill: "SPLASH-GRID", depth: 1.5, drift: "DE-4P-ULTRA", fan: "AX-300", speed: 84, margin: 0.34, kw: 51.2 },
  { rank: 3, cell: "3 × IDCF-025", fill: "FILM-VF38", depth: 0.9, drift: "DE-4P-ULTRA", fan: "AX-300", speed: 92, margin: 0.30, kw: 49.9 },
  { rank: 4, cell: "3 × IDCF-021", fill: "TRICKLE-50", depth: 1.2, drift: "DE-3P-10", fan: "AX-360", speed: 74, margin: 0.36, kw: 53.0 },
  { rank: 5, cell: "3 × IDCF-025", fill: "SPLASH-GRID", depth: 1.2, drift: "DE-4P-ULTRA", fan: "AX-300", speed: 86, margin: 0.33, kw: 50.7 },
  { rank: 6, cell: "3 × IDCF-021", fill: "FILM-VF38", depth: 1.5, drift: "DE-3P-10", fan: "AX-300", speed: 80, margin: 0.38, kw: 44.2 },
  { rank: 7, cell: "3 × IDCF-025", fill: "TRICKLE-50", depth: 1.0, drift: "DE-4P-ULTRA", fan: "AX-360", speed: 70, margin: 0.35, kw: 54.8 },
];
const list = { cells: cellsRank, parts: PINNED_FAN }[S] || RANKED;
const tr = (r, i) => S === "layers" && i > 0
  ? `<tr class="sk"><td>${r.rank}</td><td class="id"><span class="skel" style="width:${150 - i * 9}px"></span><span class="skel" style="width:${110 + (i % 3) * 14}px;opacity:.6"></span></td><td><span class="skel" style="width:34px;margin-left:auto"></span></td><td><span class="skel" style="width:28px;margin-left:auto"></span></td></tr>`
  : `<tr class="${r.rank === 1 ? "on" : ""}"><td>${r.rank}</td><td class="id">${r.cell} · ${S === "layers" ? "3 layers" : r.fill}<small>${S === "layers" ? "1.50 m · DE-4P-ULTRA · AX-500 88 %" : `${r.depth.toFixed(1)} m · ${r.drift} · ${r.fan} ${r.speed} %`}</small></td><td>+${(S === "layers" ? 0.42 : r.margin).toFixed(2)}</td><td>${(S === "layers" ? 47.1 : r.kw).toFixed(1)}</td></tr>`;
const rowsHtml = list.slice(0, phone ? 5 : (S === "parts" ? 6 : 7)).map(tr).join("");
const total = { cells: 164, parts: 58 }[S] || 202;
const rej = REJECTED.slice(0, 3).map(([t, n]) => `${t.toLowerCase()} <b>${n}</b>`).join(" · ");
const table = `<table class="rk"><tr><th>#</th><th>Design</th><th>Margin ↑</th><th>kW</th></tr>${rowsHtml}</table><div class="rej" style="border-top:1px solid var(--rule)"><a href="#">Show all ${total} ↓</a></div>${S === "parts" ? "" : `<div class="rej">Turned away, by check: ${rej} · 4 more checks.</div>`}`;
const nums = {
  base: [["Cold water", "31.70", "°C"], ["Margin", "+0.30", "K"], ["Capability", "1.093", ""], ["Electrical", "44.9", "kW"], ["Make-up", "15.0", "m³/h"], ["Air side", "237.6", "Pa"], ["Heat out", "8,619", "kW"], ["Drift", "4.14", "ppm"], ["Footprint", "78", "m²"]],
  layers: [["Cold water", "31.58", "°C", "−0.12"], ["Margin", "+0.42", "K", "+0.12"], ["Capability", "1.131", "", "+0.038"], ["Electrical", "47.1", "kW", "+2.2"], ["Make-up", "15.0", "m³/h"], ["Air side", "252.8", "Pa", "+15.2"], ["Heat out", "8,672", "kW", "+53"], ["Drift", "4.14", "ppm"], ["Footprint", "78", "m²"]],
  cells: [["Cold water", "31.69", "°C"], ["Margin", "+0.31", "K"], ["Capability", "1.096", ""], ["Electrical", "46.8", "kW"], ["Make-up", "15.1", "m³/h"], ["Air side", "214.0", "Pa"], ["Heat out", "8,605", "kW"], ["Drift", "4.10", "ppm"], ["Footprint", "81", "m²"]],
  parts: [["Cold water", "31.67", "°C"], ["Margin", "+0.33", "K"], ["Capability", "1.094", ""], ["Electrical", "55.4", "kW"], ["Make-up", "15.4", "m³/h"], ["Air side", "258.0", "Pa"], ["Heat out", "8,627", "kW"], ["Drift", "6.83", "ppm"], ["Footprint", "78", "m²"]],
};
nums.structure = nums.compare = nums.base;
const title = {
  layers: "IDCF-064 · FILM-VF38 0.60 + 0.60 + SPLASH-GRID 0.30 · DE-4P-ULTRA · AX-500 at 88 %",
  cells: "3 × IDCF-021 · FILM-VF38 1.2 m · DE-4P-ULTRA · 3 × AX-300 at 90 %",
  parts: "IDCF-064 · SPLASH-GRID 1.5 m · DE-4P-ULTRA · AX-600 at 78 %",
}[S] || `${base.cell} · ${base.fill} ${base.depth} m · ${base.drift} · ${base.fan} at ${base.speed} %${S === "structure" ? " · steel frame" : ""}`;

if (refused) {
  $("#rankstate").outerHTML = `<span class="q" style="font-size:11px">kept</span>`;
  $("#feas").textContent = "for 27.0 °C";
  $("#rankbody").innerHTML = `<div class="refusal"><h3>No tower can hold this duty</h3><p>The wet bulb, 33.0 °C, is above the cold water you want, 32.0 °C. Air cannot cool water below its own wet bulb, so the engine sized nothing.</p><div class="fix"><span class="btn pri">Wet bulb back to 27.0 °C</span><span class="btn">Cold water to 38.0 °C</span></div></div><div class="stale">${table}</div>`;
  $("#picked").remove();
} else {
  $("#feas").textContent = { cells: "164 hold as 3 cells", parts: "58 hold with the pin", layers: "your stack first" }[S] || "202 hold the duty";
  if (drag || S === "layers") $("#rankstate").outerHTML = `<span class="upd" id="rankstate">re-ranking</span>`;
  if (S === "parts") $("#rankbody").insertAdjacentHTML("beforebegin", `<div class="pinrow"><span class="q" style="font-size:12px">Pinned</span><span class="pinchip">${PIN} Fan · AX-600<i>×</i></span><span class="q" style="font-size:11px">ranked around it</span></div>`);
  $("#rankbody").innerHTML = drag ? `<div class="stale">${table}</div>` : table;
  const n = nums[S];
  $("#picked").innerHTML = `<div style="display:flex;justify-content:space-between;align-items:baseline"><span class="eyebrow">Picked · rank 1${S === "layers" ? " · your stack" : ""}</span><span class="live">holds the duty</span></div>
    <h3>${title}</h3>
    <div class="hn">${n.map(([l, v, u, dv]) => `<div><span>${l}</span><b>${v}${u ? `<small>${u}</small>` : ""}${dv ? `<em class="dv">${dv}</em>` : ""}</b></div>`).join("")}</div>
    <div style="display:flex;gap:8px"><span class="btn">⊞ Add to compare <span class="kbd">C</span></span><span class="btn">‹ Rank 2 ›</span></div>`;
}

const sections = [["Duty", 4], ...(S === "cells" ? [["Cells", 3]] : []), ["Air side", 3], ["Fan", 3], ["Heat transfer", 2], ["Fill layers", S === "layers" ? 3 : 2], ["Water", 4], ["Structure", 5]];
const on = refused ? "Duty" : { layers: "Fill layers", cells: "Cells", structure: "Structure" }[S] || "Fan";
$("#sect").innerHTML = sections.map(([t, n]) => `<span class="${t === on ? "on" : ""}">${t}<i>${n}</i></span>`).join("");
if (phone) requestAnimationFrame(() => { const t = $("#sect"); t.scrollLeft = t.querySelector(".on").offsetLeft - t.offsetLeft - 12; });
const step = ([n, lb, why, fx, s, r, ref, cls = ""]) => `<div class="st ${cls}"><div class="lb"><b>${lb}</b><span class="n">${n}</span></div><p class="why">${why}</p><div class="fx">${fx}</div><div class="s">${s}</div><div class="r">${r}</div><div class="ref">${ref}</div></div>`;
const fanPt = S === "parts" ? { q: 160.2, p: 258.0, k: 0.01005, a: 420, lbl: "fan 78 %", fan: "AX-600" } : { q: 142.43, p: 237.6, k: 0.01171, a: 380.6, lbl: "fan 88 %", fan: "AX-500" };
const steps = {
  Fan: S === "parts" ? [
    ["3.1", "Operating point", "The pinned fan settles where its curve meets the tower's system curve.", "p<sub>fan</sub>(V̇) = k · V̇²", "k = 258.0 ÷ 160.2² = 0.01005", "160.2 m³/s · 258.0 Pa", "AX-600 curve at 78 %"],
    ["3.2", "Shaft power", "The air power, grossed up by the fan's own losses.", "P<sub>shaft</sub> = V̇ · Δp / η<sub>fan</sub>", "160.2 × 258.0 ÷ 0.818 ÷ 1000", "50.53 kW", "fan efficiency 81.8 %"],
    ["3.3", "Electrical input", "What the site pays for, after gearbox and motor losses.", "P<sub>el</sub> = P<sub>shaft</sub> / (η<sub>drive</sub> · η<sub>motor</sub>)", "50.53 ÷ (0.96 × 0.95)", "55.41 kW", "drive and motor", "hl"],
  ] : [
    ["3.1", "Operating point", "The fan settles where its curve meets the tower's system curve.", "p<sub>fan</sub>(V̇) = k · V̇²", "k = 237.6 ÷ 142.43² = 0.01171", "142.4 m³/s · 237.6 Pa", "AX-500 curve at 88 %"],
    ["3.2", "Shaft power", "The air power, grossed up by the fan's own losses.", "P<sub>shaft</sub> = V̇ · Δp / η<sub>fan</sub>", "142.43 × 237.6 ÷ 0.826 ÷ 1000", "40.97 kW", "fan efficiency 82.6 %"],
    ["3.3", "Electrical input", "What the site pays for, after gearbox and motor losses.", "P<sub>el</sub> = P<sub>shaft</sub> / (η<sub>drive</sub> · η<sub>motor</sub>)", "40.97 ÷ (0.96 × 0.95)", "44.93 kW", "drive and motor", "hl"],
  ],
  Duty: [
    ["1.1", "Water volume flow", "The catalog rates cells by mass flow; you typed a volume.", "ṁ<sub>w</sub> = V̇<sub>w</sub> · ρ<sub>w</sub>", "725 ÷ 3600 m³/s × 993.4 kg/m³", "200.0 kg/s", "inputs"],
    ["1.2", "Range", "How far the tower must cool the water.", "R = t<sub>hot</sub> − t<sub>cold</sub>", "42.00 − 32.00 °C", "10.00 K", "definition"],
    ["1.3", "Approach", "How close the cold water gets to the wet bulb, the limit no tower can pass.", "A = t<sub>cold</sub> − t<sub>wb</sub>", "32.00 − 33.00 °C", "−1.00 K", "refused: must be above 0", "badst"],
    ["1.4", "Heat load", "The heat the water gives up is what the air must carry away.", "Q = ṁ<sub>w</sub> · c<sub>p</sub> · R", "200.0 × 4.183 × 10.00", "8,366 kW", "energy balance"],
  ],
  "Fill layers": [
    ["5.1", "Layer 1 · FILM-VF38 0.60", "Each layer adds transfer by its own tested curve, at the tower's L/G of 1.40.", "KaV/L<sub>i</sub> = C<sub>i</sub> · (L/G)<sup>−n<sub>i</sub></sup> · d<sub>i</sub>", "1.38 × 1.40<sup>−0.62</sup> × 0.60", "0.672", "FILM-VF38 curve · layer 2 the same"],
    ["5.2", "Layer 3 · SPLASH-GRID 0.30", "The layer you added. Splash transfers less per metre, but drops less pressure.", "KaV/L<sub>3</sub> = C · (L/G)<sup>−n</sup> · d", "0.94 × 1.40<sup>−0.51</sup> × 0.30", "0.238", "SPLASH-GRID curve", "hl"],
    ["5.3", "Stack against the duty", "The stack holds the duty when its transfer beats what the duty needs.", "capability = Σ KaV/L<sub>i</sub> ÷ KaV/L<sub>req</sub>", "(0.672 + 0.672 + 0.238) ÷ 1.399", "1.131", "required from the Merkel integral"],
  ],
  Cells: [
    ["1.5", "Flow per cell", "The duty splits evenly over identical cells.", "V̇<sub>cell</sub> = V̇ / N", "725 ÷ 3", "241.7 m³/h", "3 cells in a row"],
    ["1.6", "One cell rated", "The engine rates one cell at its share; the others are the same.", "Q<sub>cell</sub> = ṁ<sub>cell</sub> · c<sub>p</sub> · R", "66.67 × 4.183 × 10.00", "2,789 kW", "IDCF-021 at 241.7 m³/h"],
    ["1.7", "Row totals", "Power, make-up and footprint scale with N. Temperatures don't.", "P<sub>el</sub> = N · P<sub>el, cell</sub>", "3 × 15.60", "46.8 kW", "multiply, not re-rate", "hl"],
  ],
  Structure: code === "dpt" ? [
    ["7.1", "Reference velocity pressure", "DPT 1311, zone 1.", "q = ½ · ρ · V̄²", "0.5 × 1.25 × 25²", "390.6 Pa", "V̄ 25 m/s · 50-year"],
    ["7.2", "Wind on the casing", "Windward face.", "F = I<sub>w</sub> · q · C<sub>e</sub> · C<sub>g</sub> · C<sub>p</sub> · A", "1.0 × 390.6 × 0.90 × 2.0 × 0.90 × 70.52", "44.6 kN", "seismic DPT 1301 35.4 kN · wind governs"],
    ["7.3", "Corner column", "Worst member.", "N / φN<sub>c</sub> + M / φM<sub>b</sub> ≤ 1", "61.7 ÷ 731 + 17.9 ÷ 38.6", "0.55", "SHS 150 × 150 × 6 · S275", "hl"],
  ] : [
    ["7.1", "Design wind pressure", "ASCE 7, exposure C.", "q<sub>z</sub> = 0.613 · K<sub>z</sub> · K<sub>zt</sub> · V²", "0.613 × 1.04 × 1.00 × 30²", "574 Pa", "30 m/s · exposure C"],
    ["7.2", "Wind on the casing", "Windward face.", "F = q<sub>z</sub> · G · C<sub>f</sub> · A", "0.574 × 0.85 × 1.3 × (8.60 × 8.20)", "44.7 kN", "seismic ASCE 7 35.4 kN · wind governs"],
    ["7.3", "Corner column", "Worst member.", "N / φN<sub>c</sub> + M / φM<sub>b</sub> ≤ 1", "61.8 ÷ 731 + 17.9 ÷ 38.6", "0.55", "SHS 150 × 150 × 6 · S275", "hl"],
  ],
};
const chart = () => {
  const { q: q0, p: p0, k, a, lbl } = fanPt, b = (a - p0) / (q0 * q0);
  const w = 230, h = 150, l = 30, r = 8, t = 18, bt = 22, X = (q) => l + (q / 200) * (w - l - r), Y = (p) => t + (1 - p / 400) * (h - t - bt);
  const path = (f) => Array.from({ length: 41 }, (_, i) => i * 5).map((q, i) => `${i ? "L" : "M"}${X(q).toFixed(1)},${Y(f(q)).toFixed(1)}`).join("");
  const fc = (q) => Math.max(0, a - b * q * q), sc = (q) => Math.min(400, k * q * q);
  const grid = [0, 200, 400].map((p) => `<line x1="${l}" x2="${w - r}" y1="${Y(p)}" y2="${Y(p)}" stroke="var(--rule)"/><text x="${l - 6}" y="${Y(p) + 3}" text-anchor="end" font-size="9" fill="var(--fg-quiet)">${p}</text>`).join("")
    + [0, 100, 200].map((q) => `<text x="${X(q)}" y="${h - 6}" text-anchor="middle" font-size="9" fill="var(--fg-quiet)">${q}</text>`).join("");
  return `<div class="chart"><b>Fan and system curves</b><svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="Fan curve meets the system curve at ${q0} cubic metres a second and ${p0} pascals">${grid}
    <text x="${w - r}" y="${t - 6}" text-anchor="end" font-size="9" fill="var(--fg-quiet)">Pa over m³/s</text>
    <path d="${path(sc)}" fill="none" stroke="var(--fg-quiet)" stroke-width="1.4" stroke-dasharray="4 3"/><path d="${path(fc)}" fill="none" stroke="var(--fg)" stroke-width="1.6"/>
    <text x="${X(18)}" y="${Y(fc(18)) - 6}" font-size="9" fill="var(--fg)">${lbl}</text><text x="${X(176)}" y="${Y(sc(176)) + 2}" font-size="9" fill="var(--fg)" text-anchor="end">system</text>
    <circle cx="${X(q0)}" cy="${Y(p0)}" r="4" fill="var(--fg)" stroke="var(--surface)" stroke-width="1.5"/><text x="${X(q0) - 8}" y="${Y(p0) - 9}" font-size="9.5" font-weight="600" fill="var(--fg)" text-anchor="end">${q0.toFixed(1)} · ${p0.toFixed(1)}</text></svg></div>`;
};
const side = {
  Fan: chart,
  Duty: () => "",
  "Fill layers": () => {
    const parts = [["1", 0.672], ["2", 0.672], ["3", 0.238]], sum = 1.582, scale = 100 / 1.75;
    return `<div class="chart"><b>Transfer by layer</b><span class="q" style="font-size:11px">KaV/L, top layer first</span>
      <div class="stackbar">${parts.map(([n, v], i) => `<i class="${i === 2 ? "on" : ""}" style="width:${v * scale}%;background:color-mix(in srgb,var(--fg) ${[22, 16, 10][i]}%,transparent)">${n}</i>`).join("")}</div>
      <div class="req"><i style="left:${1.399 * scale}%"></i><span style="left:0;transform:none">needs 1.399 · <b style="color:var(--fg)">has ${sum}</b></span></div>
      <table class="tbl"><tr><th>Layer</th><th>KaV/L</th><th>Δp Pa</th></tr><tr><td>1 FILM-VF38</td><td>0.672</td><td>41.2</td></tr><tr><td>2 FILM-VF38</td><td>0.672</td><td>41.2</td></tr><tr><td>3 SPLASH-GRID</td><td>0.238</td><td>17.6</td></tr></table></div>`;
  },
  Cells: () => `<div class="chart"><b>One cell, then the row</b><table class="tbl"><tr><th></th><th>a cell</th><th>row × 3</th></tr>${[["Water flow", "241.7", "725.0 m³/h"], ["Heat load", "2,789", "8,366 kW"], ["Electrical", "15.6", "46.8 kW"], ["Make-up", "5.03", "15.1 m³/h"], ["Footprint", "27.0", "81.1 m²"], ["Cold water", "31.69", "31.69 °C"]].map(([a, b, c]) => `<tr><td>${a}</td><td>${b}</td><td>${c}</td></tr>`).join("")}</table></div>`,
  Structure: () => `<div class="chart"><b>Member checks</b><span class="q" style="font-size:11px">utilisation, 1.00 is the limit</span><div class="bars">${[["Inlet brace L75", 0.71], ["Corner column", 0.55], ["Fan deck beam", 0.48], ["Cladding girt", 0.39], ["Anchor bolts", 0.33], ["Basin slab", 0.27]].map(([n, u]) => `<div class="bar1"><span>${n}</span><i style="--u:${u * 100}%"></i><b>${u.toFixed(2)}</b></div>`).join("")}</div></div>`,
};
$("#steps").innerHTML = steps[on].filter((s) => on !== "Duty" || refused || s[0] !== "1.3").map(step).join("") + side[on]();
if (ga) $("#drawerToggle").textContent = "▴";

const tabs = [["inputs", "Inputs"], ["build", "Build"], ["rank", "Ranking"], ["calc", "Calc"]];
const up = { inputs: "inputs", build: "inputs", ranking: "rank", calc: "calc", parts: "tray", compare: "cmp" }[sheet];
const tabOn = { inputs: "inputs", build: "build", ranking: "rank", calc: "calc" }[sheet];
document.querySelectorAll("[data-tabs]").forEach((t) => { t.innerHTML = tabs.map(([k, l]) => `<span class="${k === tabOn ? "on" : ""}">${l}</span>`).join(""); });
const peek = {
  layers: `<span class="live" style="font-size:12px;color:var(--fg)">31.58 °C cold · +0.42 K · 47.1 kW</span><span class="q">your stack</span>`,
  cells: `<span class="live" style="font-size:12px;color:var(--fg)">3 cells · 31.69 °C · +0.31 K · 46.8 kW</span><span class="q">rank 1 of 164</span>`,
  parts: `<span class="live" style="font-size:12px;color:var(--fg)">31.67 °C · +0.33 K · 55.4 kW</span><span class="q">fan pinned</span>`,
}[S] || `<span class="live" style="font-size:12px;color:var(--fg)">31.70 °C cold · +0.30 K · 44.9 kW</span><span class="q">rank 1 of 202</span>`;
$("#peeksum").innerHTML = refused ? `<span class="err" style="padding:0">Refused: wet bulb above the cold water</span><span class="q">Inputs ↑</span>` : peek;

const trayOpen = S === "parts";
const cats = [["fill", "Fill"], ["drift", "Eliminator"], ["fan", "Fan"], ["nozzles", "Nozzles"], ["casing", "Casing"]];
$("#cats").innerHTML = `<span class="eyebrow">Parts</span>` + cats.map(([k, l]) => `<span class="cat ${trayOpen && k === "fill" ? "on" : ""}">${ICON[k]}${l}${S === "parts" && k === "fan" ? `<b class="pinned">1</b>` : ""}</span>`).join("");
const card = ([id, note, tag = "", cls = ""]) => `<div class="card ${cls}">${ICON.fill}<b>${id}</b><span>${note}</span>${tag ? `<em>${tag}</em>` : ""}</div>`;
if (trayOpen) {
  $("#tray").classList.add("open");
  $("#cards").innerHTML = `<span class="q">Drag a fill onto the tower to pin it. The engine ranks the rest around it.</span>` + [
    ["SPLASH-GRID", "PP splash grid · steps of 0.15", "in use"], ["FILM-VF38", "38 mm film · steps of 0.30"], ["FILM-VF19", "19 mm film · clean water only", "dragging", "src"], ["TRICKLE-50", "50 mm trickle · steps of 0.25"],
  ].map(card).join("");
}
if (phone && up) { $(`#${up}`).classList.add("up"); document.body.classList.add("sheet-open"); }
if (S !== "compare") $("#cmp").remove();
else {
  $("#cmp").hidden = false;
  $("#cmpbtn").classList.add("on");
  $("#cmpn").textContent = "3 / 3";
  if (!phone) $("#calc").remove();
  const cols = [
    { g: DESIGNS.base, n: "A · rank 1 · picked", t: "IDCF-064 · FILM-VF38 1.2 · AX-500", s: "1 cell · FRP", pick: true },
    { g: DESIGNS.pinned, n: "B · fan pinned", t: "IDCF-064 · SPLASH-GRID 1.5 · AX-600", s: "1 cell · FRP" },
    { g: DESIGNS.cells, n: "C · 3 cells", t: "3 × IDCF-021 · FILM-VF38 1.2 · AX-300", s: "3 cells · FRP" },
  ];
  const rowsC = [
    ["Cold water", ["31.70", "31.67", "31.69"], "°C", -1], ["Margin", ["+0.30", "+0.33", "+0.31"], "K", 1], ["Electrical", ["44.9", "55.4", "46.8"], "kW", 0],
    ["Make-up", ["15.0", "15.4", "15.1"], "m³/h", 0], ["Drift", ["4.14", "6.83", "4.10"], "ppm", 2], ["Footprint", ["78", "78", "81"], "m²", -1],
    ["Height", ["8.20", "8.94", "7.02"], "m", 2],
  ];
  const head = (c) => `<div class="col ${c.pick ? "pick" : ""}">${poster(c.g, { dims: false, cls: "" })}<span class="tag">${c.n}</span><b>${c.t}</b><span>${c.s} · <span class="x">remove</span></span></div>`;
  $("#cmpg").innerHTML = `<div class="col head"></div>` + cols.map(head).join("")
    + rowsC.map(([l, v, u, best]) => `<div class="lbl">${l}</div>` + v.map((x, i) => `<div class="val ${i === 0 ? "pick" : ""} ${i === best ? "best" : ""}">${x}${u ? `<small>${u}</small>` : ""}</div>`).join("")).join("");
  if (phone) $("#cmpg").innerHTML = cols.map(head).join("") + rowsC.slice(0, 6).map(([l, v, u, best]) => `<div class="lbl">${l}${u ? ` · ${u}` : ""}</div>` + v.map((x, i) => `<div class="val ${i === best ? "best" : ""}">${x}</div>`).join("")).join("");
}

const stage = $("#stage");
const lead = (from, el) => {
  const b = el.getBoundingClientRect(), x2 = b.left, y2 = b.top + 26;
  add(`<svg class="lead"><path d="M${from[0]},${from[1]} L${from[0] + 40},${y2} L${x2},${y2}" fill="none" stroke="var(--fg-quiet)" stroke-width="1.2" stroke-dasharray="5 3"/><circle cx="${from[0]}" cy="${from[1]}" r="4" fill="none" stroke="var(--accent)" stroke-width="1.5"/><circle cx="${from[0]}" cy="${from[1]}" r="1.5" fill="var(--accent)"/></svg>`);
};
const callouts = {
  fan: ["Fan · AX-500 · 8 blades", [["Diameter", "√(4 × 19.635 / π)", "5.00 m"], ["Speed", "chosen in 0.70 to 1.13", "88 %"], ["Electrical input", "40.97 ÷ (0.96 × 0.95)", "44.9 kW"]]],
  "fill:2": ["Fill layer 3 · SPLASH-GRID", [["Depth", "2 modules × 0.15", "0.30 m"], ["Transfer", "0.94 × 1.40<sup>−0.51</sup> × 0.30", "0.238"], ["Pressure drop", "share of 252.8 Pa", "17.6 Pa"]]],
  frame: ["Frame · steel · 10 columns", [["Corner column", "SHS 150 × 150 × 6", "0.55"], ["Inlet brace", "L 75 × 75 × 6, X-braced", "0.71"], ["Wind on casing", code === "dpt" ? "632.8 Pa × 70.5 m²" : "0.574 × 0.85 × 1.3 × 70.5", code === "dpt" ? "44.6 kN" : "44.7 kN"]]],
};
const callout = (k, x, y) => {
  const [h, rows2] = callouts[k];
  return add(`<div class="dock callout" style="left:${x}px;top:${y}px"><span class="eyebrow">Picked part</span><h3>${h}</h3>
    ${rows2.map(([a, f, r]) => `<div class="row"><span>${a}<div class="fx">${f}</div></span><span class="r">${r}</span></div>`).join("")}</div>`);
};
const hints = `<span><b>Drag</b> orbit</span><span><b>Scroll</b> zoom</span><span><b>Click</b> a part</span><span><b>1–4</b> views</span><span><b>P</b> pin</span><span><b>F</b> fit</span><span><b>⌘Z</b> undo</span>`;

if (qs.has("poster")) {
  stage.insertAdjacentHTML("beforeend", poster(geom));
  if (phone) stage.lastElementChild.style.cssText = "inset:auto;left:4%;top:14%;width:92%;height:52%";
  else stage.lastElementChild.style.cssText = "inset:auto;left:430px;top:150px;width:520px;height:420px";
  add(`<span class="live">Loading the 3D tower</span>`, `position:fixed;z-index:2;left:${phone ? 16 : 400}px;top:${phone ? 66 : 124}px`);
} else if (ga) {
  const many = geom.cells.length > 1;
  const views = phone
    ? (many ? [{ kind: "elev", rect: [0, 110, 390, 560], scale: 19 }] : [{ kind: "sect", rect: [0, 110, 390, 560], scale: 27 }])
    : many
      ? [{ kind: "elev", rect: [328, 88, 744, 318], scale: 26 }, { kind: "plan", rect: [328, 416, 436, 272], scale: 21 }, { kind: "sectx", rect: [772, 416, 300, 272], scale: 24 }]
      : [{ kind: "elev", rect: [328, 96, 372, 340] }, { kind: "sect", rect: [704, 96, 372, 340] }, { kind: "plan", rect: [328, 470, 372, 356] }];
  mountGA(stage, geom, { views, scale: 25, tier: phone ? 1 : 2, pick: many ? null : "fan" });
  const frame = struct === "steel" ? "steel" : "FRP";
  const overall = `${geom.size[0].toFixed(2)} × ${geom.size[2].toFixed(2)} × ${geom.size[1].toFixed(2)}`;
  if (phone) {
    add(`<span class="seg"><span>Plan</span><span class="${many ? "on" : ""}">Elevation</span><span class="${many ? "" : "on"}">Section A–A</span></span>`, "position:fixed;z-index:2;left:12px;top:66px");
    add(`<span class="vt">${many ? "Elevation, the row of 3" : "Section A–A"} <b>1 : ${many ? 150 : 100}</b></span>`, "position:fixed;z-index:2;left:16px;top:112px");
    if (many) add(`<div class="dock title" style="position:static;width:auto;grid-template-columns:1fr 1fr 1fr"><div><span>Overall</span><b>${overall}</b></div><div><span>Cells</span><b>3 × IDCF-021</b></div><div><span>Frame</span><b>${frame}</b></div></div>`, "position:fixed;z-index:2;left:12px;right:12px;top:600px");
  } else {
    const names = { elev: many ? "Elevation, the row of 3" : "Elevation, front", sect: "Section A–A", sectx: "Section B–B, cell 1", plan: "Plan, from above" };
    for (const v of views) {
      add(`<span class="vt">${names[v.kind]} <b>1 : ${many ? (v.kind === "elev" ? 100 : 150) : 100}</b></span>`, `position:fixed;z-index:2;left:${v.rect[0] + 10}px;top:${v.rect[1] + 6}px`);
      add("", `position:fixed;z-index:1;left:${v.rect[0]}px;top:${v.rect[1]}px;width:${v.rect[2]}px;height:${v.rect[3]}px;border:1px dashed var(--dock-rule);border-radius:6px`);
    }
    if (many) {
      add(`<div class="dock title" style="position:static;width:auto;grid-template-columns:2fr 1.4fr 1.2fr .8fr .6fr 1fr .9fr"><div><span>Drawing</span><b>General arrangement · rank 1</b></div><div><span>Overall</span><b>${overall}</b></div><div><span>Cells</span><b>3 × IDCF-021</b></div><div><span>Frame</span><b>${frame}</b></div><div><span>Units</span><b>m</b></div><div><span>Engine</span><b>v1.0.0 · s1</b></div><div><span>Catalog</span><b>synthetic</b></div></div>`, "position:fixed;z-index:2;left:328px;top:700px;width:744px");
      document.querySelectorAll(".title div").forEach((d) => { d.style.borderTop = "0"; });
    } else {
      const sched = [["Cell", "IDCF-064 · counterflow"], ["Fill", "FILM-VF38 · 2 × 0.60 m"], ["Spray", "78 × NZ-20 · 6 headers"], ["Eliminator", "DE-4P-ULTRA"], ["Fan", "AX-500 · Ø 5.00 m · 8 blades"], ["Frame", "FRP panels · 6 columns"]];
      add(`<div class="dock" style="position:static;padding:10px 14px;display:grid;gap:4px;font-size:11.5px"><span class="eyebrow">Parts</span>${sched.map(([a, b]) => `<div style="display:flex;justify-content:space-between;gap:10px;border-top:1px solid var(--rule);padding-top:5px"><span class="q" style="font-size:11.5px">${a}</span><b class="mono" style="font-size:11.5px">${b}</b></div>`).join("")}</div>`, "position:fixed;z-index:2;left:704px;top:470px;width:372px");
      add(`<div class="dock title" style="position:static;width:auto"><div class="wide"><span>Drawing</span><b>General arrangement · rank 1</b></div><div><span>Overall</span><b>${overall}</b></div><div><span>Frame</span><b>${frame}</b></div><div><span>Units · scale</span><b>m · 1 : 100</b></div><div><span>Catalog</span><b>synthetic</b></div><div class="wide"><span>Engine</span><b>v1.0.0 · schema 1</b></div></div>`, "position:fixed;z-index:2;left:704px;top:664px;width:372px");
    }
  }
} else {
  const pick = picks[S] || null;
  const callK = !phone && !refused && callouts[pick] ? pick : null;
  const left = trayOpen ? 584 : 400, toolsL = $("#tools").style.left = `${left}px`, right = W - 372 - (callK ? 276 : 0);
  const drawerH = S === "compare" ? 330 : 292, sheetH = up === "tray" ? 250 : up === "cmp" ? H * 0.62 : up ? H * 0.64 : 112;
  const free = phone ? [36, 60, W - 36, H - 60 - sheetH] : [left + 80, 130, right - left - 80, H - 130 - drawerH];
  const span = phone ? (S === "cells" ? (up ? 50 : 40) : up ? 44 : 33) : S === "cells" ? 36 : S === "parts" ? 31 : 29;
  const m = mountModel(stage, geom, {
    free, span, pick: refused ? null : pick, refuse: S === "parts" ? "fill" : null, tier: phone ? 1 : 2, cut: 0.55, dims: !(phone && up),
    target: S === "cells" ? [0, 3.4, 0] : [0, 3.9, 0], fillDim: S === "layers" && !phone,
  });
  if (refused) stage.style.opacity = 0.5;
  if (callK) lead(m.anchor(callK), callout(callK, W - 372 - 276, 150));
  if (phone && !up && pick && callouts[pick]) {
    const [hh, rr] = callouts[pick];
    add(`<div class="dock callout phone" style="position:static"><span class="eyebrow">${hh.split(" · ").slice(0, 2).join(" · ")}</span><b class="mono" style="font-size:12px">${rr.map((r) => r[2]).join(" · ")}</b></div>`, `position:fixed;z-index:2;right:10px;top:68px`);
  }
  if (S === "parts") {
    const [fx, fy] = m.anchor("fan");
    add(`<div class="dl pin">${PIN}AX-600 pinned<span>drag off to unpin</span></div>`).style.cssText = `left:${fx}px;top:${fy - 6}px`;
    const [ax, ay] = m.anchor("fillFront");
    const gx = phone ? 110 : ax - 85, gy = phone ? 330 : ay - 70;
    const rx = phone ? 12 : ax - 134, ry = phone ? 420 : ay - 2;
    add(`<div class="dock refuse" style="left:${rx}px;top:${ry}px"><span class="eyebrow">Can't go here</span><span><b>FILM-VF19</b> clogs in moderate water: its 19 mm flutes need clean water. The fill stays SPLASH-GRID.</span><div class="fix"><span class="btn">Set water to Clean</span><span class="btn">Use FILM-VF38</span></div></div>`);
    add(card(["FILM-VF19", "19 mm film · clean water only"]).replace('class="card ', 'class="card ghost '), "");
    const gh = document.body.lastElementChild;
    gh.style.cssText = `left:${gx}px;top:${gy}px`;
    add(`<svg class="cursor" viewBox="0 0 18 22" style="left:${gx + 96}px;top:${gy + 26}px"><path d="M2 2v16l4.5-4 3 6.5 2.6-1.2-3-6.3H15z" fill="var(--fg)" stroke="var(--bg)" stroke-width="1.2"/></svg>`);
  }
  if (!phone) {
    if (S !== "layers") add(hints, `position:fixed;z-index:2;left:${left}px;top:${H - drawerH - 30}px;display:flex;gap:14px;font-size:11px;color:var(--fg-quiet)`).classList.add("hintbar");
    const live = {
      base: "cold water 31.70 °C · fan 88 % · 132 rpm", compare: "cold water 31.70 °C · fan 88 % · 132 rpm", structure: "steel frame · corner column at 0.55",
      layers: "fill 1.50 m in 3 layers · cold water 31.58 °C", cells: "3 cells · 241.7 m³/h each · 3 × 15.6 kW", parts: "fan AX-600 pinned · cold water 31.67 °C",
    }[S];
    add(`<span class="eyebrow">${refused ? "Kept" : "Live"}</span><span class="mono" style="font-size:12px">${refused ? "showing the last design that held" : live}</span>`, `position:fixed;z-index:2;left:${left}px;top:122px;display:grid;gap:2px`);
  }
}
if (!phone && S === "layers") add(`<div class="dock toast" style="left:400px;top:${H - 292 - 50}px"><span><b>Added layer 3</b> · SPLASH-GRID 0.30 m · re-rated in 5 ms</span><span class="btn">↶ Undo <span class="kbd">⌘Z</span></span></div>`);
if (S === "compare" && !phone) {
  const r = $("#scen").getBoundingClientRect();
  add(`<div class="dock menu" style="left:${r.left}px;top:${r.bottom + 8}px"><span class="eyebrow" style="padding:6px 10px 2px">Saved scenarios</span>
    <div class="it on"><b>Plant B peak</b><span>now</span></div><div class="it">Plant B · 3 cells<span>2 h ago</span></div><div class="it">Plant B · pinned AX-600<span>2 h ago</span></div><div class="it">Plant A · retrofit<span>Tue</span></div>
    <hr><div class="it">Save as a new scenario…<b class="kbd">⇧⌘S</b></div><div class="it">⧉ Copy share link</div><div class="link">…/s/k7Qm2 · opens this scenario read-only</div></div>`);
}
document.body.dataset.h = H;
