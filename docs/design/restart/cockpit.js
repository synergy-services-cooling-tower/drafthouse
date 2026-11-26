// Preview-only: fills the cockpit's docks from the fixture and draws one state per URL (?ga, ?refused, ?sheet=inputs|ranking|calc, ?light, ?poster). Not product code.
import { mountModel, mountGA } from "./model.js";

const qs = new URLSearchParams(location.search), $ = (s) => document.querySelector(s);
const phone = innerWidth < 700, W = innerWidth, H = innerHeight;
const ga = qs.has("ga"), refused = qs.has("refused"), sheet = qs.get("sheet"), light = qs.has("light");
document.body.classList.toggle("ga", ga);
if (phone) $(".design b").textContent = "IDCF-064 · FILM-VF38";
document.body.classList.toggle("refused", refused);
document.querySelectorAll("#mode span").forEach((s) => s.classList.toggle("on", (s.dataset.m === "ga") === ga));
$("#theme").innerHTML = light
  ? `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/></svg>`
  : `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>`;

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
if (drag) { $("#rated").textContent = "rated 4 ms ago, following the slider"; }
if (refused) { $("#rated").outerHTML = `<span class="q" style="color:var(--bad);font-size:11px">refused</span>`; }

const rows = RANKED.slice(0, 7).map((r) => `<tr class="${r.rank === 1 ? "on" : ""}"><td>${r.rank}</td><td class="id">${r.cell} · ${r.fill}<small>${r.depth} m · ${r.drift} · ${r.fan} ${r.speed} %</small></td><td>+${r.margin.toFixed(2)}</td><td>${r.kw.toFixed(1)}</td></tr>`).join("");
const rej = REJECTED.slice(0, 3).map(([t, n]) => `${t.toLowerCase()} <b>${n}</b>`).join(" · ");
const table = `<table class="rk"><tr><th>#</th><th>Design</th><th>Margin</th><th>kW</th></tr>${rows}</table><div class="rej" style="border-top:1px solid var(--rule)"><a href="#">Show all 202 ↓</a></div><div class="rej">Turned away, by check: ${rej} · 4 more checks.</div>`;
const r1 = RANKED[0];
if (refused) {
  $("#rankstate").outerHTML = `<span class="q" style="font-size:11px">kept</span>`;
  $("#feas").textContent = "for 27.0 °C";
  $("#rankbody").innerHTML = `<div class="refusal"><h3>No tower can hold this duty</h3><p>The wet bulb, 33.0 °C, is above the cold water you want, 32.0 °C. Air cannot cool water below its own wet bulb, so the engine sized nothing.</p><div class="fix"><span class="btn pri">Wet bulb back to 27.0 °C</span><span class="btn">Cold water to 38.0 °C</span></div></div><div class="stale">${table}</div>`;
  $("#picked").remove();
  $("#dstate").textContent = "kept from 27.0 °C";
} else {
  if (drag) $("#rankstate").outerHTML = `<span class="upd" id="rankstate">re-ranking</span>`;
  $("#rankbody").innerHTML = drag ? `<div class="stale">${table}</div>` : table;
  const hn = [["Cold water", "31.70", "°C"], ["Margin", "+0.30", "K", "ok"], ["Capability", "1.093", ""], ["Electrical", "44.9", "kW"], ["Make-up", "15.0", "m³/h"], ["Air side", "237.6", "Pa"], ["Heat out", "8,619", "kW"], ["Drift", "4.14", "ppm"], ["Footprint", "78", "m²"]];
  $("#picked").innerHTML = `<div style="display:flex;justify-content:space-between;align-items:baseline"><span class="eyebrow">Picked · rank 1</span><span class="live">holds the duty</span></div>
    <h3>${r1.cell} · ${r1.fill} ${r1.depth} m · ${r1.drift} · ${r1.fan} at ${r1.speed} %</h3>
    <div class="hn">${hn.map(([l, v, u, c = ""]) => `<div><span>${l}</span><b class="${c}">${v}${u ? `<small>${u}</small>` : ""}</b></div>`).join("")}</div>
    <div style="display:flex;gap:8px"><span class="btn">⧉ Copy link</span><span class="btn">‹ Rank 2 ›</span></div>`;
}

const sections = [["Duty", 4], ["Air side", 3], ["Fan", 3], ["Heat transfer", 2], ["Fill layers", 2], ["Water", 4]];
const on = refused ? "Duty" : "Fan";
$("#sect").innerHTML = sections.map(([t, n]) => `<span class="${t === on ? "on" : ""}">${t}<i>${n}</i></span>`).join("");
const step = ([n, lb, why, fx, s, r, ref, cls = ""]) => `<div class="st ${cls}"><div class="lb"><b>${lb}</b><span class="n">${n}</span></div><p class="why">${why}</p><div class="fx">${fx}</div><div class="s">${s}</div><div class="r">${r}</div><div class="ref">${ref}</div></div>`;
const fan = [
  ["3.1", "Operating point", "The fan settles where its curve meets the tower's system curve.", "p<sub>fan</sub>(V̇) = k · V̇²", "k = 237.6 ÷ 142.43² = 0.01171", "142.4 m³/s · 237.6 Pa", "AX-500 curve at 88 %"],
  ["3.2", "Shaft power", "The air power, grossed up by the fan's own losses.", "P<sub>shaft</sub> = V̇ · Δp / η<sub>fan</sub>", "142.43 × 237.6 ÷ 0.826 ÷ 1000", "40.97 kW", "fan efficiency 82.6 %"],
  ["3.3", "Electrical input", "What the site pays for, after gearbox and motor losses.", "P<sub>el</sub> = P<sub>shaft</sub> / (η<sub>drive</sub> · η<sub>motor</sub>)", "40.97 ÷ (0.96 × 0.95)", "44.93 kW", "drive and motor", "hl"],
];
const duty = [
  ["1.1", "Water volume flow", "The catalog rates cells by mass flow; you typed a volume.", "ṁ<sub>w</sub> = V̇<sub>w</sub> · ρ<sub>w</sub>", "725 ÷ 3600 m³/s × 993.4 kg/m³", "200.0 kg/s", "inputs"],
  ["1.2", "Range", "How far the tower must cool the water.", "R = t<sub>hot</sub> − t<sub>cold</sub>", "42.00 − 32.00 °C", "10.00 K", "definition"],
  ["1.3", "Approach", "How close the cold water gets to the wet bulb, the limit no tower can pass.", "A = t<sub>cold</sub> − t<sub>wb</sub>", "32.00 − 33.00 °C", "−1.00 K", "refused: must be above 0", "badst"],
  ["1.4", "Heat load", "The heat the water gives up is what the air must carry away.", "Q = ṁ<sub>w</sub> · c<sub>p</sub> · R", "200.0 × 4.183 × 10.00", "8,366 kW", "energy balance"],
];
const chart = () => {
  const w = 230, h = 150, l = 30, r = 8, t = 18, b = 22, X = (q) => l + (q / 200) * (w - l - r), Y = (p) => t + (1 - p / 400) * (h - t - b);
  const path = (f) => Array.from({ length: 41 }, (_, i) => i * 5).map((q, i) => `${i ? "L" : "M"}${X(q).toFixed(1)},${Y(f(q)).toFixed(1)}`).join("");
  const fc = (q) => Math.max(0, 380.6 - 0.00705 * q * q), sc = (q) => Math.min(400, 0.01171 * q * q);
  const grid = [0, 200, 400].map((p) => `<line x1="${l}" x2="${w - r}" y1="${Y(p)}" y2="${Y(p)}" stroke="var(--rule)"/><text x="${l - 6}" y="${Y(p) + 3}" text-anchor="end" font-size="9" fill="var(--fg-quiet)">${p}</text>`).join("")
    + [0, 100, 200].map((q) => `<text x="${X(q)}" y="${h - 6}" text-anchor="middle" font-size="9" fill="var(--fg-quiet)">${q}</text>`).join("");
  return `<div class="chart"><b>Fan and system curves</b><svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="Fan curve meets the system curve at 142.4 cubic metres a second and 237.6 pascals">${grid}
    <text x="${w - r}" y="${t - 6}" text-anchor="end" font-size="9" fill="var(--fg-quiet)">Pa over m³/s</text>
    <path d="${path(sc)}" fill="none" stroke="var(--fg-quiet)" stroke-width="1.5" stroke-dasharray="4 3"/><path d="${path(fc)}" fill="none" stroke="var(--accent)" stroke-width="1.8"/>
    <text x="${X(18)}" y="${Y(fc(18)) - 6}" font-size="9" fill="var(--fg)">fan 88 %</text><text x="${X(176)}" y="${Y(sc(176)) + 2}" font-size="9" fill="var(--fg)" text-anchor="end">system</text>
    <circle cx="${X(142.43)}" cy="${Y(237.6)}" r="4.5" fill="var(--accent)" stroke="var(--surface)" stroke-width="1.5"/><text x="${X(142.43) - 8}" y="${Y(237.6) - 9}" font-size="9.5" font-weight="600" fill="var(--fg)" text-anchor="end">142.4 · 237.6</text></svg></div>`;
};
$("#steps").innerHTML = refused ? duty.map(step).join("") : fan.map(step).join("") + chart();
if (ga) $("#drawerToggle").textContent = "▴ Open";

const tabs = [["inputs", "Inputs"], ["rank", "Ranking"], ["calc", "Calc"]];
const up = { inputs: "inputs", ranking: "rank", calc: "calc" }[sheet];
document.querySelectorAll("[data-tabs]").forEach((t) => { t.innerHTML = tabs.map(([k, l]) => `<span class="${k === up ? "on" : ""}">${l}</span>`).join(""); });
if (phone && up) { $(`#${up}`).classList.add("up"); document.body.classList.add("sheet-open"); }
$("#peeksum").innerHTML = refused
  ? `<span class="err" style="padding:0">Refused: wet bulb above the cold water</span><span class="q">Inputs ↑</span>`
  : `<span class="live" style="font-size:12px;color:var(--fg)">31.70 °C cold · +0.30 K · 44.9 kW</span><span class="q">rank 1 of 202</span>`;

const stage = $("#stage"), fanTop = [0, GEOMETRY.cells[0].fan.centre[1] + 0.25, 0.9];
const lead = (from, card) => {
  const b = card.getBoundingClientRect(), x2 = b.left, y2 = b.top + 26;
  document.body.insertAdjacentHTML("beforeend", `<svg class="lead"><path d="M${from[0]},${from[1]} L${from[0] + 40},${y2} L${x2},${y2}" fill="none" stroke="var(--accent-hi)" stroke-width="1.4" stroke-dasharray="5 3"/><circle cx="${from[0]}" cy="${from[1]}" r="4" fill="none" stroke="var(--accent-hi)" stroke-width="1.5"/><circle cx="${from[0]}" cy="${from[1]}" r="1.5" fill="var(--accent-hi)"/></svg>`);
};
const callout = (x, y, cls = "") => {
  document.body.insertAdjacentHTML("beforeend", `<div class="dock callout ${cls}" style="left:${x}px;top:${y}px">
    <span class="eyebrow">Picked part · click any part</span><h3>Fan · AX-500 · 8 blades</h3>
    <div class="row"><span>Diameter<div class="fx">√(4 × 19.635 / π)</div></span><span class="r">5.00 m</span></div>
    <div class="row"><span>Speed<div class="fx">chosen in 0.70 to 1.13</div></span><span class="r">88 %</span></div>
    <div class="row"><span>Electrical input<div class="fx">40.97 ÷ (0.96 × 0.95)</div></span><span class="r">44.9 kW</span></div>
    <span class="q" style="font-size:11px">Its steps are open in the calc drawer ↓</span></div>`);
  return document.body.lastElementChild;
};
const fixed = (html, style) => document.body.insertAdjacentHTML("beforeend", `<div style="${style}">${html}</div>`);

if (qs.has("poster")) {
  stage.insertAdjacentHTML("beforeend", poster(GEOMETRY));
  if (phone) stage.lastElementChild.style.cssText = "inset:auto;left:4%;top:14%;width:92%;height:52%";
  fixed(`<span class="live">Loading the 3D tower</span>`, `position:fixed;z-index:2;left:${phone ? 16 : 340}px;top:${phone ? 66 : 92}px`);
} else if (ga) {
  const views = phone
    ? [{ kind: "sect", rect: [0, 110, 390, 560], scale: 27 }]
    : [{ kind: "elev", rect: [328, 96, 372, 340] }, { kind: "sect", rect: [704, 96, 372, 340] }, { kind: "plan", rect: [328, 470, 372, 356] }];
  mountGA(stage, GEOMETRY, { views, scale: 25, tier: phone ? 1 : 2 });
  if (phone) {
    fixed(`<span class="seg"><span>Plan</span><span>Elevation</span><span class="on">Section A–A</span></span>`, "position:fixed;z-index:2;left:12px;top:66px");
    fixed(`<span class="vt">Section A–A <b>1 : 100</b></span>`, "position:fixed;z-index:2;left:16px;top:112px");
  } else {
    for (const v of views) {
      const name = { elev: "Elevation, front", sect: "Section A–A", plan: "Plan, from above" }[v.kind];
      fixed(`<span class="vt">${name} <b>1 : 100</b></span>`, `position:fixed;z-index:2;left:${v.rect[0] + 10}px;top:${v.rect[1] + 6}px`);
      fixed("", `position:fixed;z-index:1;left:${v.rect[0]}px;top:${v.rect[1]}px;width:${v.rect[2]}px;height:${v.rect[3]}px;border:1px dashed var(--dock-rule);border-radius:6px`);
    }
    const sched = [["Cell", "IDCF-064 · counterflow"], ["Fill", "FILM-VF38 · 2 × 0.60 m"], ["Spray", "78 × NZ-20 · 6 headers"], ["Eliminator", "DE-4P-ULTRA"], ["Fan", "AX-500 · Ø 5.00 m · 8 blades"], ["Drive", "45 kW motor · gearbox"]];
    fixed(`<div class="dock" style="position:static;padding:12px 14px;display:grid;gap:6px;font-size:11.5px"><span class="eyebrow">Parts</span>${sched.map(([a, b]) => `<div style="display:flex;justify-content:space-between;gap:10px;border-top:1px solid var(--rule);padding-top:5px"><span class="q" style="font-size:11.5px">${a}</span><b class="mono" style="font-size:11.5px">${b}</b></div>`).join("")}</div>`, "position:fixed;z-index:2;left:704px;top:470px;width:372px");
    fixed(`<div class="dock title" style="position:static;width:auto"><div class="wide"><span>Drawing</span><b>General arrangement · rank 1</b></div><div><span>Overall</span><b>8.60 × 8.60 × 8.20</b></div><div><span>Units</span><b>m</b></div><div><span>Engine</span><b>v1.0.0 · schema 1</b></div><div><span>Catalog</span><b>synthetic</b></div></div>`, "position:fixed;z-index:2;left:704px;top:690px;width:372px");
  }
} else {
  const free = phone ? [36, 60, W - 36, H - 60 - (up ? H * 0.64 : 112)] : [324, 76, W - 324 - 372 - (refused ? 0 : 250), H - 76 - 292];
  const m = mountModel(stage, GEOMETRY, phone
    ? { free, span: up ? 44 : 33, pick: "fan", tier: 1, cut: 0.55, dims: !up }
    : { free, span: 29, pick: "fan", cut: 0.55, target: [0, 3.9, 0] });
  if (refused) stage.style.opacity = 0.5;
  const fp = m.at(fanTop);
  if (!phone && !refused) lead(fp, callout(W - 372 - 276, 150));
  if (phone && !up) {
    fixed(`<div class="dock callout phone" style="position:static"><span class="eyebrow" style="color:var(--accent)">Fan · AX-500</span><b class="mono" style="font-size:12px">Ø 5.00 m · 88 % · 44.9 kW</b></div>`, `position:fixed;z-index:2;right:10px;top:68px`);
  }
  if (!phone) {
    fixed(`<span><b>Drag</b> orbit</span><span><b>Scroll</b> zoom</span><span><b>Click</b> a part for its formula</span>`, `position:fixed;z-index:2;left:340px;top:${H - 292 - 30}px;display:flex;gap:14px;font-size:11px;color:var(--fg-quiet)`);
    fixed(`<span class="eyebrow">${refused ? "Kept" : "Live"}</span><span class="mono" style="font-size:12px">${refused ? "showing the last design that held" : "cold water 31.70 °C · fan 88 % · 132 rpm"}</span>`, "position:fixed;z-index:2;left:340px;top:92px;display:grid;gap:2px");
  }
}
document.body.dataset.h = H;
