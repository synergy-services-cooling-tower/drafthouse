// Preview-only: the wizard's step rail and its side column (tower, read-outs, current design), shared by the steps and the embed. Not product code.
import { mountTower } from "./tower.js";

const steps = ["Duty", "Site and water", "Priority", "Ranked towers"];

export function wizard(step, { updating = false, embedded = false } = {}) {
  const rail = document.querySelector("[data-rail]");
  rail.outerHTML = `<nav class="rail" aria-label="Steps">${steps
    .map((t, i) => `<a href="#" class="${i + 1 === step ? "on" : i + 1 < step ? "done" : ""}"><b>${i + 1 < step ? "✓" : i === 3 ? "✓" : i + 1}</b>${t}</a>`)
    .join('<span class="sep"></span>')}<span class="end">Step ${step} of 3</span></nav>`;
  const r = RANKED[0];
  const side = document.querySelector("[data-side]");
  side.outerHTML = `<aside class="side">
    <section class="island t" id="tw">
      <div class="cap"><b>Following your edits</b><span>· ${r.cell} · ${r.fill} ${r.depth} m · ${r.fan}</span></div>
      <div class="bar"><span class="chip on">◧ Cutaway</span><span style="flex:1"></span><a class="btn sm" href="#">⤢ Full view</a></div>
    </section>
    <section class="card" style="padding:20px var(--s-3)">
      <div class="ro">
        <div class="stat"><span>Range</span><b class="num">10.0<small>K</small></b></div>
        <div class="stat"><span>Approach</span><b class="num">5.0<small>K</small></b></div>
        <div class="stat"><span>Heat load</span><b class="num">8.37<small>MW</small></b></div>
      </div>
      <div class="live" style="margin-top:var(--s-3)">Rated just now · fan ${r.speed} % · cold water ${r.cold.toFixed(2)} °C</div>
    </section>
    <section class="card cur">
      <div class="row"><span class="eyebrow">Current design</span>${updating ? '<span class="upd">Ranking updating</span>' : '<span class="live">Ranked</span>'}</div>
      <div class="row"><b>Rank 1 of 202</b><span class="q">least over-capacity</span></div>
      <dl class="kv"><dt>Margin on cold water</dt><dd>+${r.margin.toFixed(2)} K</dd><dt>Electrical input</dt><dd>${r.kw} kW</dd><dt>Make-up water</dt><dd>${r.makeup.toFixed(1)} m³/h</dd><dt>Footprint</dt><dd>${r.foot} m²</dd></dl>
      ${embedded ? '<div class="notice">Catalog from the host app · revision shown in its records</div>' : '<div class="notice"><b>Synthetic catalog.</b> Not CTI certified.</div>'}
    </section></aside>`;
  const narrow = innerWidth < 700;
  mountTower(document.getElementById("tw"), GEOMETRY, narrow
    ? { cutaway: 0.55, air: false, tier: 1, span: 15, target: [0, 4.2, 0] }
    : { cutaway: 0.55, span: 14.5, target: [0, 4.6, 0] });
}
