/**
 * Companion panel for the selection surface — approved Variant A ("anatomy and numbers").
 *
 * Placement (locked decision 2): the panel mounts INSIDE the existing "Tower & parts
 * selection" output, directly after the ranked table, which keeps its columns, its position
 * and its priority above the panel. There is no new top-level tab.
 *
 * What this module owns:
 *  · the colour scale: a part is tinted by the share of the total air-side pressure
 *    attributed to it (its own terms plus the terms that physically occur on it), with the
 *    numeric value printed on every chip so colour is never the only carrier;
 *  · the marker pins and chips that place the eight pressure terms where they physically
 *    occur, plus one cooling chip on the fill bank (available Merkel number and the thermal
 *    margin, both read from the selection result);
 *  · the part rail: one real button per assembly, 1:1 with the records the engine selected;
 *  · the detail block under the rail: tiles, the record, the worked sheet, and the limit
 *    table for that part — with the spec-stress switch that holds the same numbers against a
 *    tighter requirement;
 *  · the two-way link between the host's ranked table and this panel: a rank button in the
 *    table loads that candidate here, and this panel keeps the table's row and buttons in
 *    sync when the position changes from inside the panel.
 *
 * The panel reads an ENGINEERING-ONLY projection of the selection result (see numbers.js):
 * commercial keys are physically absent from what it can see, not merely unrendered.
 */
import { deriveGeometry } from "./geometry.js";
import {
  fmt, panelCandidate, panelRequirements, pressureLosses, cooling, limitChecks,
  partRecord, partTiles, partSheet, partRecordId
} from "./numbers.js";
import { escapeHtml, keyValueRows, workedSheet, stateTag, materialTag, checksTable } from "./panel-ui.js";
import { createViewer } from "./viewer.js";

const WEBGL_FORCE_OFF = ["off", "0", "false", "none"];

export const PRESSURE_BANDS = [
  { id: "b5", label: "over 25 %", min: 25, colour: "#0b6073" },
  { id: "b4", label: "15 – 25 %", min: 15, colour: "#3994a4" },
  { id: "b3", label: "6 – 15 %", min: 6, colour: "#7dbdc7" },
  { id: "b2", label: "3 – 6 %", min: 3, colour: "#bfe0e5" },
  { id: "b1", label: "under 3 %", min: 0, colour: "#e7f5f6" },
  { id: "b0", label: "no term attributed", min: -1, colour: "#e9eef0" }
];

/** Pressure terms attributed to each pickable assembly (a display choice, stated on screen). */
export function attribution(losses) {
  const map = new Map();
  for (const row of losses.rows) {
    if (!row.partId) continue;
    const current = map.get(row.partId) ?? { pa: 0, terms: [] };
    current.pa += row.pa;
    current.terms.push(row);
    map.set(row.partId, current);
  }
  return map;
}

function bandFor(sharePct, attributed) {
  if (!attributed) return PRESSURE_BANDS[PRESSURE_BANDS.length - 1];
  return PRESSURE_BANDS.find((band) => sharePct >= band.min) ?? PRESSURE_BANDS[PRESSURE_BANDS.length - 1];
}

const LAYER_TOGGLES = [
  { key: "pressure", label: "Pressure", aria: "Pressure terms" },
  { key: "capability", label: "Cooling", aria: "Cooling balance" },
  { key: "parts", label: "Parts", aria: "Part names" }
];

const PART_LABEL_OFFSETS = {
  shell: [56, -12],
  fill: [-84, -42],
  drift: [96, -26],
  fan: [86, 16],
  motor: [-96, 26],
  header: [-104, -6],
  basin: [10, 34]
};

/** Where each pressure term physically occurs on the model. */
function lossAnchor(geometry, key, index) {
  const L = geometry.levels;
  const zOut = L.planWidM / 2;
  const xOut = L.planLenM / 2;
  const crossflow = geometry.topology === "crossflow";
  const laneZ = crossflow ? L.plenumWidthM / 2 + L.fillAirPathM / 2 : 0;
  switch (key) {
    case "fill":
      return { position: [xOut * 0.35, L.fillBottomM + L.fillHeightM * 0.5, laneZ], dx: 0, dy: -22 };
    case "distribution":
      return { position: [xOut * 0.5, L.fillTopM + 0.28, 0], dx: 24, dy: -6 };
    case "supports":
      return { position: [0, L.fillBottomM - 0.05, laneZ], dx: -8, dy: 16 };
    case "drift":
      return crossflow
        ? { position: [xOut * 0.55, L.fillBottomM + L.fillHeightM * 0.72, L.plenumWidthM / 2], dx: 26, dy: -12 }
        : { position: [xOut * 0.4, L.driftBottomM + 0.075, 0], dx: 20, dy: -14 };
    case "inlet":
      return { position: [0, L.basinTopM + L.inletBandHeightM * 0.5, zOut + 0.1], dx: 0, dy: 18 };
    case "plenum":
      return { position: [0, L.driftTopM + (L.plenumTopM - L.driftTopM) * 0.5, 0], dx: 0, dy: 0 };
    case "fanStack":
      return { position: [0, L.plenumTopM + (L.stackTopM - L.plenumTopM) * 0.55, L.fanDiaM / 2], dx: 26, dy: 6 };
    default:
      return { position: [0, L.basinTopM, 0], dx: 0, dy: 0, index };
  }
}

/**
 * Mount (or refresh) the companion panel inside the host's selection output.
 *
 * @param {object}  options
 * @param {Element} options.host       the element the panel mounts inside (#selection-output)
 * @param {object}  options.selection  the engine's selection result (untouched)
 * @returns {object|null} the panel controller, or null when there is nothing to explain
 */
export function mountSelectionPanel({ host, selection }) {
  if (!host || !selection?.results?.length) {
    host?.querySelector("#part-panel")?.remove();
    return null;
  }

  // Engineering-only projections: the panel cannot see a commercial key even by accident.
  const candidates = selection.results.map(panelCandidate);
  const requirements = panelRequirements(selection.requirements ?? {});
  const catalogStatus = selection.catalogMetadata?.status ?? "status not declared";
  const catalogId = selection.catalogMetadata?.id ?? "unidentified catalog";

  const params = new URLSearchParams(window.location.search);
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const state = {
    rank: (() => {
      const requested = Number(params.get("rank"));
      return Number.isFinite(requested) && requested >= 1 && requested <= candidates.length ? requested : candidates[0].rank;
    })(),
    partId: (() => {
      const requested = params.get("part");
      return !requested || requested === "none" ? null : requested;
    })(),
    exploded: (params.get("view") ?? "exploded") !== "assembled",
    layers: {},
    spec: params.get("spec") === "stress",
    forceNoWebGL: WEBGL_FORCE_OFF.includes((params.get("webgl") ?? "").toLowerCase()) || window.__forceNoWebGL === true,
    errors: []
  };
  for (const toggle of LAYER_TOGGLES) state.layers[toggle.key] = params.get(`layer-${toggle.key}`) !== "off";

  /* ---- mount point: directly after the ranked table, inside the output ---- */

  const existing = host.querySelector("#part-panel");
  if (existing) existing.remove();
  const panel = document.createElement("article");
  panel.className = "pp-panel";
  panel.id = "part-panel";
  panel.setAttribute("aria-labelledby", "part-panel-title");
  panel.tabIndex = -1;
  const tableHost = host.querySelector(".data-table-wrap");
  if (tableHost) tableHost.insertAdjacentElement("afterend", panel);
  else host.append(panel);

  panel.innerHTML = `
      <div class="pp-head">
        <div>
          <h3 id="part-panel-title">3D anatomy — where this selection's numbers sit</h3>
          <p class="pp-lede">Generated from the selected records; every number is read from the selection result — select a part
            to open its numbers.</p>
        </div>
        <div class="pp-head-meta">
          <span class="status-badge warning">${escapeHtml(catalogStatus)}</span>
          <span class="pp-source">${escapeHtml(catalogId)} · geometry from the selected records</span>
        </div>
      </div>
      <div class="pp-split">
        <div>
          <div class="pp-stage" id="pp-stage-wrap">
            <div class="pp-stage-canvas" id="pp-stage">
              <canvas id="pp-canvas" tabindex="0" role="img"
                aria-label="Illustrative 3D section of the selected tower assembly. The same pickable parts are listed as buttons in the rail beside this view."></canvas>
              <div class="pp-labels" id="pp-labels" aria-hidden="true"></div>
            </div>
            <div class="pp-stage-foot">
              <div class="pp-stage-controls" role="group" aria-label="Which candidate the panel explains, and the view options">
                <label class="pp-control-label" for="pp-rank-select">Explaining</label>
                <select class="pp-select" id="pp-rank-select"></select>
                <span class="pp-control-label">View</span>
                <span id="pp-view"></span>
                <span class="pp-control-label">Layers</span>
                <span id="pp-layers"></span>
                <button class="pp-chip" type="button" id="pp-reset">Reset view</button>
              </div>
              <span class="pp-state" id="pp-state-line" aria-live="polite"></span>
              <span class="pp-viewhint" id="pp-view-hint">drag to orbit · wheel or pinch to zoom · arrow keys when focused · grid 1 m</span>
            </div>
          </div>
          <div id="pp-legend"></div>
        </div>
        <div class="pp-rail-col">
          <div class="pp-rail-head">
            <h4>Pickable assemblies</h4>
            <span class="pp-rail-count" id="pp-rail-count"></span>
          </div>
          <p class="pp-rail-hint" id="pp-rail-hint"></p>
          <ul class="pp-rail" id="pp-rail" aria-label="Pickable assemblies — select a part to open its numbers"></ul>
        </div>
      </div>
      <div class="pp-detail-grid" id="pp-detail"></div>
      <div id="pp-tables"></div>
      <p class="pp-fineprint">
        <strong>Illustrative geometry, synthetic catalog.</strong> The model is generated parametrically from
        the selected catalog records (${escapeHtml(catalogStatus)}) — it is a diagram of the selection's own numbers, not vendor
        geometry, not a certification, validation or acceptance of any component, and not a basis for a guarantee.
        The ranked table above is the source of truth for the ranking.
      </p>`;

  const dom = {
    stage: panel.querySelector("#pp-stage"),
    rail: panel.querySelector("#pp-rail"),
    detail: panel.querySelector("#pp-detail"),
    tables: panel.querySelector("#pp-tables"),
    stateLine: panel.querySelector("#pp-state-line"),
    legend: panel.querySelector("#pp-legend"),
    railCount: panel.querySelector("#pp-rail-count"),
    railHint: panel.querySelector("#pp-rail-hint")
  };
  const rankSelect = panel.querySelector("#pp-rank-select");

  let viewer = null;

  /* ---- derived context --------------------------------------------- */

  /**
   * The positions the panel explains are the rows the ranked table actually renders: the
   * rank buttons in the host table are the single source, so a position can never point at a
   * candidate with no row beside it (and a change to the table's row window is followed here
   * automatically).
   */
  const rankPillElements = () => [...host.querySelectorAll("[data-explain]")];
  const tableRanks = rankPillElements()
    .map((button) => Number(button.dataset.explain))
    .filter((rank) => Number.isFinite(rank))
    .sort((left, right) => left - right);
  const positions = tableRanks.length ? tableRanks : candidates.map((candidate) => candidate.rank);

  const candidateAt = (rank) => candidates.find((item) => item.rank === rank) ?? null;

  function context() {
    const candidate = state.rank === null ? null : candidateAt(state.rank);
    if (!candidate) return { candidate: null, geometry: null, losses: null, cap: null, checks: null, parts: [], partId: null, requirements };
    const geometry = deriveGeometry({
      tower: candidate.tower,
      fan: candidate.fan,
      fillDepthM: candidate.fillDepthM,
      motor: candidate.motor,
      nozzle: candidate.nozzle
    });
    const losses = pressureLosses(candidate);
    const cap = cooling(candidate, requirements);
    const checks = limitChecks(candidate, requirements, state.spec ? { maxDriftPpm: 5, minimumThermalMarginC: 0.4 } : {});
    return {
      candidate,
      geometry,
      losses,
      cap,
      checks,
      parts: geometry.parts,
      partId: state.partId,
      requirements,
      spec: state.spec ? { maxDriftPpm: 5, minimumThermalMarginC: 0.4 } : {}
    };
  }

  /* ---- chrome ------------------------------------------------------ */

  function renderRankSelect() {
    rankSelect.innerHTML = positions
      .map((position) => `<option value="${position}"${state.rank === position ? " selected" : ""}>${position} · ${escapeHtml(candidateAt(position).fill.id)} + ${escapeHtml(candidateAt(position).driftEliminator.id)} · ${fmt(candidateAt(position).thermal.coldWaterC, 3)} °C</option>`)
      .join("");
    rankSelect.value = String(state.rank);
  }

  rankSelect.addEventListener("change", () => {
    const value = rankSelect.value;
    if (value === "none") return;
    setRank(Number(value));
  });

  function renderViewChips() {
    panel.querySelector("#pp-view").innerHTML = `
      <button class="pp-chip" type="button" data-view="exploded" aria-pressed="${state.exploded}">Exploded</button>
      <button class="pp-chip" type="button" data-view="assembled" aria-pressed="${!state.exploded}">Assembled</button>`;
    for (const button of panel.querySelectorAll("#pp-view button")) {
      button.addEventListener("click", () => setExploded(button.dataset.view === "exploded"));
    }
  }

  function renderLayerChips() {
    panel.querySelector("#pp-layers").innerHTML = LAYER_TOGGLES.map((toggle) => `
      <button class="pp-chip" type="button" data-layer="${toggle.key}" aria-pressed="${state.layers[toggle.key]}" title="${escapeHtml(toggle.aria)}">${escapeHtml(toggle.label)}</button>`).join("");
    for (const button of panel.querySelectorAll("#pp-layers button")) {
      button.addEventListener("click", () => toggleLayer(button.dataset.layer));
    }
  }

  panel.querySelector("#pp-reset").addEventListener("click", () => viewer?.resetView());

  /* ---- rail -------------------------------------------------------- */

  function stateOfPart(ctx, partId) {
    const worst = ctx.checks.byPart(partId).reduce((acc, check) => (acc === null || check.utilization > acc.utilization ? check : acc), null);
    return worst ? worst.state : "pass";
  }

  function railExtra(ctx, part) {
    const attributed = attribution(ctx.losses).get(part.id);
    const sharePct = attributed ? (attributed.pa / ctx.losses.totalPa) * 100 : null;
    const checks = ctx.checks.byPart(part.id);
    const worst = checks.reduce((acc, check) => (acc === null || check.utilization > acc.utilization ? check : acc), null);
    const band = bandFor(sharePct ?? 0, Boolean(attributed));
    const recordId = partRecordId(ctx.candidate, part.id) ?? "no record";
    return {
      key: band.colour,
      name: part.label,
      record: sharePct === null
        ? `${escapeHtml(recordId)} · no air-side pressure term`
        : `${escapeHtml(recordId)} · ${fmt(sharePct, 2)} % of Δp · ${fmt(attributed.pa, 4)} Pa`,
      tags: [
        sharePct === null
          ? '<span class="pp-tag none">no pressure term</span>'
          : `<span class="pp-tag metric">${fmt(sharePct, 2)} % of Δp</span>`,
        worst && worst.state !== "pass" ? stateTag(worst.state) : ""
      ].filter(Boolean).join("")
    };
  }

  function renderRail(ctx) {
    if (!ctx.candidate) {
      dom.railCount.textContent = "";
      dom.railHint.textContent = "The rail lists the pickable assemblies of the candidate the panel is explaining.";
      dom.rail.innerHTML = '<li><div class="pp-empty"><span><span class="pp-empty-mark">Rail empty</span>Pick a position on the ranked table above to load its assemblies.</span></div></li>';
      return;
    }
    dom.railCount.textContent = `${ctx.parts.filter((part) => part.pickable !== false).length} of ${ctx.parts.length} pickable`;
    dom.railHint.textContent = "Selecting a part opens its numbers below. Every assembly maps 1:1 to a record the selection resolved — cell shell, fill bank, drift eliminator bank, fan, motor/gearbox, distribution header and basin.";
    dom.rail.innerHTML = ctx.parts.map((part, index) => {
      const extra = railExtra(ctx, part);
      const selected = part.id === state.partId;
      return `<li>
        <button class="pp-row" type="button" data-part="${part.id}" aria-current="${selected}"
          data-state="${stateOfPart(ctx, part.id)}">
          <span class="pp-row-key" style="background:${extra.key}"></span>
          <span class="pp-row-n">${String(index + 1).padStart(2, "0")}</span>
          <span class="pp-row-body">
            <span class="pp-row-name">${escapeHtml(extra.name)}</span>
            <span class="pp-row-record">${extra.record}</span>
          </span>
          <span class="pp-row-tags">${extra.tags}</span>
        </button>
      </li>`;
    }).join("");
    for (const button of dom.rail.querySelectorAll("button")) {
      button.addEventListener("click", () => selectPart(button.dataset.part, { frame: false, fromRail: true }));
      button.addEventListener("keydown", onRailKeydown);
    }
  }

  /** Roving arrow-key navigation inside the rail: a list of real buttons, reachable by keyboard. */
  function onRailKeydown(event) {
    const buttons = [...dom.rail.querySelectorAll("button")];
    const index = buttons.indexOf(event.currentTarget);
    let next = null;
    if (event.key === "ArrowDown") next = buttons[(index + 1) % buttons.length];
    if (event.key === "ArrowUp") next = buttons[(index - 1 + buttons.length) % buttons.length];
    if (event.key === "Home") next = buttons[0];
    if (event.key === "End") next = buttons[buttons.length - 1];
    if (!next) return;
    event.preventDefault();
    next.focus();
  }

  /* ---- stage ------------------------------------------------------- */

  function buildStage(ctx) {
    viewer?.dispose();
    viewer = null;
    dom.stage.innerHTML = "";
    if (!ctx.candidate) {
      dom.stage.innerHTML = `<div class="pp-empty">
        <span><span class="pp-empty-mark">No assembly loaded</span>
        The ranked table above is the primary surface. Pick a position to load that candidate's assembly into this panel.</span></div>`;
      markSettled();
      return;
    }
    dom.stage.innerHTML = `<canvas id="pp-canvas" tabindex="0" role="img"
        aria-label="Illustrative 3D section of the selected tower assembly. The same pickable parts are listed as buttons in the rail beside this view."></canvas>
      <div class="pp-labels" id="pp-labels" aria-hidden="true"></div>`;
    const canvas = panel.querySelector("#pp-canvas");
    const labelHost = panel.querySelector("#pp-labels");

    if (state.forceNoWebGL) {
      renderFallback(ctx, "WebGL was switched off for this view (?webgl=off) — the table-only fallback is being shown.");
      return;
    }
    try {
      viewer = createViewer({
        canvas,
        labelHost,
        geometry: ctx.geometry,
        colourFor: (part) => colourFor(part, ctx),
        reducedMotion,
        onPick: (partId) => { if (partId) selectPart(partId, { frame: false }); },
        onHover: () => {},
        onContextLost: (reason) => renderFallback(ctx, reason)
      });
      labels(viewer, ctx);
      /* On a phone the stage is too small to carry the numeric chips legibly, so those layers
         start off there and the anchored tables carry the same numbers. An explicit
         layer-… URL parameter (or a tap on the chip) overrides this. */
      if (viewer.stageWidth() < 540) {
        for (const toggle of LAYER_TOGGLES) {
          if (toggle.key === "parts" || params.has(`layer-${toggle.key}`)) continue;
          state.layers[toggle.key] = false;
        }
        renderLayerChips();
        const hint = panel.querySelector("#pp-view-hint");
        if (hint) hint.textContent = "drag to orbit · pinch to zoom · the chips are numbered like the rail below · grid 1 m";
      }
      for (const toggle of LAYER_TOGGLES) applyLayer(ctx, toggle.key, state.layers[toggle.key]);
      viewer.setExplode(state.exploded, { animate: false });
      viewer.select(state.partId);
    } catch (error) {
      state.errors.push(String(error.message ?? error));
      renderFallback(ctx, error.message ?? String(error));
      return;
    }
    markSettled();
  }

  function renderFallback(ctx, reason) {
    state.errors.push(reason);
    dom.stage.innerHTML = `<div class="pp-fallback" role="status">
        <div class="pp-fallback-head">
          <h4>3D view unavailable — table-only fallback</h4>
          <span class="status-badge warning">no live model</span>
        </div>
        <p>This surface is designed for a browser without WebGL: the ranked table above is untouched, every assembly is still
          listed as a button in the rail, and the numbers below are the same numbers the model annotates. Nothing is hidden
          behind the picture.</p>
        <p class="pp-fallback-reason">${escapeHtml(reason)}</p>
        <div class="table-scroll"><table class="worked-table" id="pp-fallback-dims">
          <thead><tr><th>Derived dimension</th><th>Formula</th><th>Value</th><th>Source</th></tr></thead>
          <tbody>${ctx.geometry.dims.map((dim) => `<tr>
            <td><strong>${escapeHtml(dim.label)}</strong></td>
            <td class="worked-why-cell">${escapeHtml(dim.formula)}</td>
            <td>${fmt(dim.value, 4)} ${escapeHtml(dim.unit)}</td>
            <td class="pp-source">${escapeHtml(dim.source)}</td>
          </tr>`).join("")}</tbody></table></div>
        <p class="pp-table-note">The geometry above is exactly what would have been drawn: the same
          <code>deriveGeometry()</code> call produced both, so the fallback is the model without the picture.</p>
      </div>`;
    markSettled();
  }

  function applyLayer(ctx, key, on) {
    state.layers[key] = on;
    if (!viewer) return;
    const ids = key === "parts"
      ? ctx.parts.filter((part) => part.pickable !== false).map((part) => `part-${part.id}`)
      : layerIds(ctx, key);
    for (const entry of viewer.labelEntries) {
      if (!ids.includes(entry.id)) continue;
      entry.visible = on;
      entry.node.style.display = on ? "" : "none";
    }
    viewer.requestRender();
  }

  function layerIds(ctx, key) {
    if (key === "pressure") return ctx.losses.rows.filter((row) => row.partId).map((row) => `loss-${row.key}`);
    if (key === "capability") return ["capability"];
    return [];
  }

  /* ---- colour and labels (variant A) -------------------------------- */

  function colourFor(part, ctx) {
    const attributed = attribution(ctx.losses).get(part.id);
    const sharePct = attributed ? (attributed.pa / ctx.losses.totalPa) * 100 : 0;
    return bandFor(sharePct, Boolean(attributed)).colour;
  }

  function labels(stage, ctx) {
    const { geometry, losses, cap, checks } = ctx;
    const attributed = attribution(losses);
    for (const [index, part] of geometry.parts.entries()) {
      if (part.pickable === false) {
        stage.addMarker({ position: part.anchor, colour: "#b9c4c9", size: 0.2 });
        stage.addLabel({
          id: `part-${part.id}`,
          kind: "loss",
          anchor: part.anchor,
          dx: (PART_LABEL_OFFSETS[part.id] ?? [0, 0])[0],
          dy: (PART_LABEL_OFFSETS[part.id] ?? [0, 0])[1],
          visible: true,
          html: `<span class="pp-label-part">${escapeHtml(part.label)}</span><span class="pp-label-value">context only</span>`
        });
        continue;
      }
      const share = attributed.get(part.id);
      const sharePct = share ? (share.pa / losses.totalPa) * 100 : null;
      const worst = checks.byPart(part.id).reduce((acc, check) => (acc === null || check.utilization > acc.utilization ? check : acc), null);
      const offsets = PART_LABEL_OFFSETS[part.id] ?? [0, 0];
      /* Every chip gets a pin, so no chip floats without saying what it belongs to. */
      stage.addMarker({ position: part.anchor, colour: sharePct === null ? "#87949a" : bandFor(sharePct, true).colour, size: 0.24 });
      stage.addLabel({
        id: `part-${part.id}`,
        kind: "part",
        anchor: part.anchor,
        dx: offsets[0],
        dy: offsets[1],
        visible: true,
        state: worst && worst.state !== "pass" ? worst.state : undefined,
        html: `<span class="pp-label-n">${String(index + 1).padStart(2, "0")}</span><span class="pp-label-part">${escapeHtml(part.label)}</span>`
          + (sharePct === null ? "" : `<span class="pp-label-value">${fmt(sharePct, 1)} % Δp</span>`)
      });
    }

    for (const [index, row] of losses.rows.entries()) {
      if (!row.partId) continue;
      const anchor = lossAnchor(geometry, row.key, index);
      if (!anchor) continue;
      stage.addMarker({ position: anchor.position, colour: bandFor(row.sharePct, true).colour });
      stage.addLabel({
        id: `loss-${row.key}`,
        kind: "loss",
        anchor: anchor.position,
        dx: anchor.dx,
        dy: anchor.dy,
        visible: true,
        html: `<span class="pp-label-part">Δp ${escapeHtml(row.label.toLowerCase())}</span><span class="pp-label-value">${fmt(row.pa, 2)} Pa</span>`
      });
    }

    /* The cooling chip: delivered against target, drawn where the cooling happens. */
    const capAnchor = {
      position: [-geometry.levels.planLenM / 2 - 0.2, geometry.levels.fillBottomM + geometry.levels.fillHeightM * 0.55, 0],
      dx: -34, dy: 6
    };
    stage.addMarker({ position: capAnchor.position, colour: "#3994a4", size: 0.26 });
    stage.addLabel({
      id: "capability",
      kind: "cooling",
      anchor: capAnchor.position,
      dx: capAnchor.dx,
      dy: capAnchor.dy,
      visible: true,
      html: `<span class="pp-label-part">Cooling</span><span class="pp-label-value">${fmt(cap.available, 3)} KaV/L · margin ${fmt(cap.marginC, 3)} °C</span>`
    });
  }

  /* ---- detail ------------------------------------------------------- */

  function renderDetail(ctx) {
    if (!ctx.candidate) {
      dom.detail.innerHTML = `<div class="pp-block"><div class="pp-block-head"><h4>No assembly loaded</h4></div>
        <p class="pp-table-note">The ranked table above stays the primary surface: pick a position — in the table or in the bar above —
        and this panel loads that candidate's assembly, its records and its limits.</p></div>`;
      return;
    }
    if (!ctx.partId) {
      dom.detail.innerHTML = `<div class="pp-block pp-block-wide">
        <div class="pp-block-head"><h4>No assembly selected</h4><span class="pp-source">${ctx.parts.filter((part) => part.pickable !== false).length} pickable assemblies</span></div>
        <p class="pp-table-note">The model is drawn whole. Choose a row in the rail — or click a part in the model — to open
          its numbers and its limits here. The rail is a list of buttons: tab to it and use the arrow keys if you are not
          using a mouse.</p>
        <p class="pp-table-note"><strong>Values of the candidate the panel is explaining</strong> (engine rank ${ctx.candidate.rank}):</p>
        ${keyValueRows([
          ["Candidate", `${escapeHtml(ctx.candidate.tower.id)} ranked ${ctx.candidate.rank} of ${candidates.length}`],
          ["Fill", `${escapeHtml(ctx.candidate.fill.id)} · ${fmt(ctx.candidate.fillDepthM, 2)} m`],
          ["Eliminator", escapeHtml(ctx.candidate.driftEliminator.id)],
          ["Fan", `${escapeHtml(ctx.candidate.fan.id)} @ ${fmt(ctx.candidate.speedRatio, 2)}×`],
          ["Cooling", `${fmt(ctx.cap.deliveredC, 3)} °C delivered · ${fmt(ctx.cap.marginC, 3)} °C margin`],
          ["Total air-side pressure", `${fmt(ctx.losses.totalPa, 2)} Pa`]
        ])}
      </div>`;
      return;
    }
    const part = ctx.parts.find((item) => item.id === ctx.partId);
    if (!part) {
      dom.detail.innerHTML = "";
      return;
    }
    const record = partRecord(ctx.candidate, ctx.partId, ctx.geometry);
    const sheet = partSheet(ctx.candidate, ctx.partId, ctx.geometry, requirements, ctx.losses, ctx.cap, ctx.checks);
    const tiles = partTiles(ctx.candidate, ctx.partId, ctx.geometry, ctx.losses, ctx.checks, ctx.cap);
    const partChecks = ctx.checks.byPart(ctx.partId);
    dom.detail.innerHTML = `
      <div class="pp-block">
        <div class="pp-block-head">
          <h4>${escapeHtml(part.label)}</h4>
          <span class="pp-source">${escapeHtml(record.recordPath)}</span>
        </div>
        <div class="pp-tiles">
          ${tiles.map((tile) => `<div class="pp-tile"><span class="label">${escapeHtml(tile.label)}</span><span class="value">${fmt(tile.value, tile.digits)} <span class="unit">${escapeHtml(tile.unit)}</span></span></div>`).join("")}
        </div>
      </div>
      <div class="pp-block">
        <div class="pp-block-head">
          <h4>Record behind the part</h4>
          <span class="pp-source">${escapeHtml(record.id)}</span>
        </div>
        <div class="pp-check-summary">${materialTag(record.material)}${partChecks.length ? partChecks.map((check) => stateTag(check.state)).filter((tag, index, all) => all.indexOf(tag) === index).join("") : '<span class="pp-tag none">no limit check on this assembly</span>'}</div>
        ${keyValueRows(record.fields.map(([label, value]) => [escapeHtml(label), escapeHtml(value)]))}
      </div>
      <div class="pp-block pp-block-wide">
        ${workedSheet(sheet, { open: true })}
      </div>
      ${partChecks.length ? `<div class="pp-block pp-block-wide">
        <div class="pp-block-head"><h4>Limits this assembly carries</h4><span class="pp-source">catalog and requirement limits, held against this selection's own numbers</span></div>
        ${checksTable({ checks: partChecks }, { stressed: Boolean(ctx.spec.maxDriftPpm || ctx.spec.minimumThermalMarginC) })}
      </div>` : ""}`;
    const specToggle = dom.detail.querySelector("#pp-spec-toggle") ?? panel.querySelector("#pp-spec-toggle");
    if (specToggle) {
      specToggle.addEventListener("change", () => setSpec(specToggle.checked));
    }
  }

  function renderStateLine(ctx) {
    if (!ctx.candidate) {
      dom.stateLine.innerHTML = "<strong>No assembly loaded</strong> — the ranked table above stays primary.";
      return;
    }
    const part = ctx.parts.find((item) => item.id === state.partId);
    const swatch = part ? colourFor(part, ctx) : "#cfd9dd";
    dom.stateLine.innerHTML = `<strong>Rank ${ctx.candidate.rank}</strong> · ${escapeHtml(ctx.candidate.tower.id)}
      + ${escapeHtml(ctx.candidate.fill.id)} ${fmt(ctx.candidate.fillDepthM, 1)} m
      + ${escapeHtml(ctx.candidate.driftEliminator.id)} + ${escapeHtml(ctx.candidate.fan.id)} @ ${fmt(ctx.candidate.speedRatio, 2)}×
      · ${state.exploded ? "exploded view" : "assembled view"} · ${part ? escapeHtml(part.label) : "no part selected"}
      <span class="pp-state-swatch" style="background:${swatch}"></span>`;
  }

  function renderLegend(ctx) {
    if (!ctx.candidate) {
      dom.legend.innerHTML = "";
      return;
    }
    const attributed = attribution(ctx.losses);
    const fill = attributed.get("fill");
    const shell = attributed.get("shell");
    const drift = attributed.get("drift");
    dom.legend.innerHTML = `<div class="pp-legend">
      <div class="pp-legend-block">
        <span class="pp-legend-title">Part tint = attributed share of Δp</span>
        <span class="pp-scale">${[...PRESSURE_BANDS].reverse().map((band) => `<span style="background:${band.colour}" title="${escapeHtml(band.label)}"></span>`).join("")}</span>
        <span class="pp-scale-caption">under 3 % → over 25 %</span>
      </div>
      <div class="pp-legend-block">
        <span class="pp-swatch">${fill ? `Fill ${fmt((fill.pa / ctx.losses.totalPa) * 100, 2)} %` : ""}
          ${shell ? `· Shell ${fmt((shell.pa / ctx.losses.totalPa) * 100, 2)} %` : ""}
          ${drift ? `· Eliminator ${fmt((drift.pa / ctx.losses.totalPa) * 100, 2)} %` : ""}</span>
        <span class="pp-badge-note">Attribution is a display choice: a term is shown on the assembly it physically sits on.</span>
      </div>
      <div class="pp-legend-block">
        <span class="pp-legend-title">Rail tags</span>
        <span class="pp-swatch">${stateTag("pass")}${stateTag("tight")}${stateTag("violation")}</span>
        <span class="pp-badge-note">row tag = limit state, exactly as the tags in the rail render them</span>
      </div>
      <div class="pp-legend-block">
        <span class="pp-legend-title">Paths and pins</span>
        <span class="pp-swatch"><i class="pp-line-key" style="border-top-color:#0b6073"></i>air</span>
        <span class="pp-swatch"><i class="pp-line-key" style="border-top-color:#c77a1b"></i>water</span>
        <span class="pp-swatch"><i style="background:#0b6073"></i>pressure pin</span>
        <span class="pp-swatch"><i style="background:#3994a4"></i>cooling pin</span>
        <span class="pp-badge-note">8 pressure terms · cooling balance on the fill bank · grid 1 m</span>
      </div>
    </div>`;
  }

  /* ---- anchored tables --------------------------------------------- */

  function pressureTableMarkup(losses) {
    const rows = losses.rows.map((row) => `<tr data-part="${row.partId ?? ""}">
        <td><strong>${escapeHtml(row.label)}</strong></td>
        <td class="worked-why-cell">${escapeHtml(row.why)}</td>
        <td class="pp-anchor-cell">${escapeHtml(row.where)}${row.partId ? "" : " — not drawn"}</td>
        <td>${fmt(row.pa, 4)}</td>
        <td><span class="worked-bar" style="--share:${Math.max(0, Math.min(100, row.sharePct))}%"></span>${fmt(row.sharePct, 2)} %</td>
      </tr>`).join("");
    return `<div class="table-scroll"><table class="worked-table">
      <thead><tr><th>Term</th><th>What it is</th><th>Shown at</th><th>Δp<br>Pa</th><th>Share</th></tr></thead>
      <tbody>${rows}
        <tr class="worked-total"><td colspan="3">Total air-side pressure the fan must overcome</td><td>${fmt(losses.totalPa, 4)}</td><td>100 %</td></tr>
      </tbody></table></div>
      <p class="pp-table-note">Share = Δp<sub>term</sub> / Δp<sub>total</sub> × 100, e.g. ${escapeHtml(losses.rows[0].shareSubstitution)}.
        Sum check: ${escapeHtml(losses.check.substitution)} → ${fmt(losses.check.delta, 12)} Pa residual against the total the selection records.</p>`;
  }

  function coolingTableMarkup(cap, requirements) {
    return `<div class="table-scroll"><table class="worked-table" id="pp-cooling">
        <thead><tr><th>Quantity</th><th>Value</th><th>Source</th></tr></thead>
        <tbody>
          <tr><td><strong>Available Merkel number</strong></td><td>${fmt(cap.available, 4)} KaV/L</td><td class="pp-source">candidate.airside.availableMerkelNumber</td></tr>
          <tr><td>of which the fill bank delivers</td><td>${fmt(cap.zones[0].value, 4)} KaV/L</td><td class="pp-source">candidate.airside.fillMerkelNumber</td></tr>
          <tr><td>spray zone</td><td>${fmt(cap.zones[1].value, 4)} KaV/L</td><td class="pp-source">candidate.airside.sprayZoneMerkelNumber</td></tr>
          <tr><td>rain zone</td><td>${fmt(cap.zones[2].value, 4)} KaV/L</td><td class="pp-source">candidate.airside.rainZoneMerkelNumber</td></tr>
          <tr><td><strong>Target cold water</strong></td><td>${fmt(cap.targetC, 1)} °C</td><td class="pp-source">requirements.targetColdWaterC</td></tr>
          <tr><td><strong>Delivered cold water</strong></td><td>${fmt(cap.deliveredC, 3)} °C</td><td class="pp-source">candidate.thermal.coldWaterC</td></tr>
          <tr><td><strong>Thermal margin</strong></td><td>${fmt(cap.marginC, 3)} °C</td><td class="pp-source">${escapeHtml(cap.marginCheck.substitution)}</td></tr>
          <tr><td><strong>L/G</strong></td><td>${fmt(cap.lg, 4)} ·</td><td class="pp-source">${escapeHtml(cap.lgCheck.substitution)}</td></tr>
          <tr><td><strong>Requirement held</strong></td><td>${fmt(requirements.minimumThermalMarginC, 1)} °C</td><td class="pp-source">requirements.minimumThermalMarginC</td></tr>
        </tbody></table></div>
      <p class="pp-table-note">The zone split is the engine's own: ${escapeHtml(cap.zoneCheck.substitution)} → ${fmt(cap.zoneCheck.delta, 12)} KaV/L residual.
        The temperature model for this candidate is ${escapeHtml(cap.temperatureModel)}. The recorded margin reproduces from the target minus the delivered
        value: ${escapeHtml(cap.marginCheck.substitution)} → ${fmt(cap.marginCheck.delta, 12)} °C residual.</p>`;
  }

  function renderTables(ctx) {
    if (!ctx.candidate) {
      dom.tables.innerHTML = "";
      return;
    }
    const stressed = Boolean(ctx.spec.maxDriftPpm || ctx.spec.minimumThermalMarginC);
    dom.tables.innerHTML = `
      <h3 class="pp-table-title">Where the ${fmt(ctx.losses.totalPa, 2)} Pa of air-side pressure goes</h3>
      ${pressureTableMarkup(ctx.losses)}
      <h3 class="pp-table-title">Where the cooling stands</h3>
      ${coolingTableMarkup(ctx.cap, requirements)}
      <h3 class="pp-table-title">Limit state for rank ${ctx.candidate.rank}</h3>
      <div class="pp-spec">
        <label><input type="checkbox" id="pp-spec-toggle" ${stressed ? "checked" : ""}> Hold the same numbers against a tighter spec</label>
        <span class="pp-source">Max drift ${fmt(stressed ? 5 : requirements.maxDriftPpm, 1)} ppm (requirement) · minimum thermal margin ${fmt(stressed ? 0.4 : requirements.minimumThermalMarginC, 1)} °C. The switch moves the threshold only — no number is recalculated.</span>
      </div>
      ${checksTable(ctx.checks, { stressed })}`;
    const specToggle = dom.tables.querySelector("#pp-spec-toggle");
    if (specToggle) specToggle.addEventListener("change", () => setSpec(specToggle.checked));
  }

  /* ---- host sync: the ranked table --------------------------------- */

  function syncHostRank() {
    for (const button of rankPillElements()) {
      const active = Number(button.dataset.explain) === state.rank;
      button.setAttribute("aria-current", String(active));
      const row = button.closest("tr");
      if (row) row.dataset.current = String(active);
    }
  }

  function wireHostRankPills() {
    for (const button of rankPillElements()) {
      button.addEventListener("click", () => setRank(Number(button.dataset.explain)));
    }
  }

  /* ---- setters ------------------------------------------------------ */

  function setRank(rank) {
    if (rank !== null && !positions.includes(rank)) return;
    state.rank = rank;
    render();
    panel.scrollIntoView({ block: "start", behavior: reducedMotion ? "auto" : "smooth" });
  }

  function setPart(partId) {
    selectPart(partId, { frame: false, fromRail: true });
  }

  function setExploded(exploded) {
    state.exploded = exploded;
    viewer?.setExplode(exploded);
    renderViewChips();
    renderStateLine(context());
    markSettled();
  }

  function setSpec(on) {
    state.spec = on;
    render();
  }

  function toggleLayer(key) {
    applyLayer(context(), key, !state.layers[key]);
    renderLayerChips();
  }

  /** Mesh → record and rail → mesh all funnel through here, so the two never disagree. */
  function selectPart(partId, { frame = false, fromRail = false } = {}) {
    state.partId = partId;
    viewer?.select(partId);
    const button = dom.rail.querySelector(`button[data-part="${partId}"]`);
    for (const row of dom.rail.querySelectorAll("button[data-part]")) row.setAttribute("aria-current", String(row === button));
    if (button && !fromRail) {
      button.focus({ preventScroll: true });
      button.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
    const ctx = context();
    renderDetail(ctx);
    renderStateLine(ctx);
    if (frame && viewer) viewer.framePart(partId);

    /* The table row follows the mesh: selecting a part of the explained candidate keeps that
       candidate's row current — picking never silently switches which row is explained. */
    syncHostRank();
    markSettled();
  }

  /* ---- render ------------------------------------------------------- */

  function render() {
    const ctx = context();
    renderRankSelect();
    renderViewChips();
    renderLayerChips();
    renderRail(ctx);
    renderDetail(ctx);
    renderStateLine(ctx);
    renderLegend(ctx);
    renderTables(ctx);
    buildStage(ctx);
    markSettled();
    syncHostRank();
    document.body.dataset.panelStance = stanceName();
  }

  function stanceName() {
    if (state.forceNoWebGL) return "fallback";
    if (state.partId) return "part";
    return state.exploded ? "exploded" : "assembled";
  }

  /* ---- readiness (for evidence capture and probes) ------------------ */

  let settledTimer = null;
  function markSettled() {
    api.ready = false;
    api.settled = false;
    clearTimeout(settledTimer);
    const check = () => {
      const stageOk = !viewer || (viewer.isReady() && viewer.isSettled());
      if (stageOk) {
        api.ready = true;
        api.settled = true;
        document.body.dataset.panel = "ready";
        return;
      }
      settledTimer = setTimeout(check, 40);
    };
    settledTimer = setTimeout(check, 40);
  }

  /* ---- inspector (deep links, probes, evidence) --------------------- */

  const api = {
    state: () => ({ ...state, stance: stanceName() }),
    partRecordId: (partId) => {
      const ctx = context();
      return ctx.candidate ? partRecordId(ctx.candidate, partId) : null;
    },
    selectPart: (partId) => setPart(partId),
    /** Same funnel a canvas click uses; returns the part now selected. */
    partFromMeshClick: (partId) => { selectPart(partId, { frame: false }); return state.partId; },
    probePick: (x, y) => (viewer ? (viewer.pickAtPoint(x, y)?.userData.partId ?? null) : null),
    canvasRect: () => {
      const canvas = panel.querySelector("canvas");
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      return { left: rect.left, top: rect.top, width: rect.width, height: rect.height, scrollY: window.scrollY };
    },
    labelBoxes: () => (viewer ? viewer.labelBoxes() : []),
    setRank,
    setExploded,
    setSpec,
    toggleLayer,
    reset: () => { viewer?.resetView(); },
    ready: false,
    settled: false,
    errors: state.errors,
    dom: () => ({
      stance: stanceName(),
      positions,
      railRows: [...dom.rail.querySelectorAll("button[data-part]")].map((button) => button.dataset.part),
      railSelected: dom.rail.querySelector('button[aria-current="true"]')?.dataset.part ?? null,
      railTags: [...dom.rail.querySelectorAll(".pp-tag")].map((tag) => tag.className.replace("pp-tag ", "")),
      railRecords: [...dom.rail.querySelectorAll(".pp-row-record")].map((node) => node.textContent),
      pickableParts: viewer ? viewer.pickableParts() : [],
      labels: viewer ? viewer.labelEntries.filter((entry) => entry.visible).map((entry) => entry.id) : [],
      markerCount: viewer ? viewer.markerGroup.children.length : 0,
      canvas: Boolean(panel.querySelector("canvas")),
      fallback: Boolean(panel.querySelector(".pp-fallback")),
      selectValue: rankSelect.value,
      keyValueRows: dom.detail.querySelectorAll("dl.key-value dt").length,
      stateLine: dom.stateLine.textContent.replace(/\s+/g, " ").trim(),
      tableCount: panel.querySelectorAll("table").length,
      legendText: dom.legend.textContent.replace(/\s+/g, " ").trim(),
      panelText: panel.innerText.replace(/\s+/g, " ")
    }),
    geometry: () => {
      const ctx = context();
      if (!ctx.geometry) return null;
      return {
        topology: ctx.geometry.topology,
        dims: ctx.geometry.dims.map((dim) => ({ key: dim.key, value: dim.value, unit: dim.unit, formula: dim.formula, substitution: dim.substitution, source: dim.source })),
        pickableIds: ctx.geometry.pickableIds,
        contextIds: ctx.geometry.contextIds,
        partSolids: ctx.geometry.parts.map((part) => ({ id: part.id, pickable: part.pickable !== false, bodies: part.solids.filter((solid) => solid.role === "body").length, details: part.solids.filter((solid) => solid.role === "detail").length })),
        notes: ctx.geometry.notes
      };
    },
    checks: () => {
      const ctx = context();
      return ctx.checks ? ctx.checks.checks.map((check) => ({ partId: check.partId, name: check.name, state: check.state, value: check.value, limit: check.limit, utilization: check.utilization })) : [];
    },
    stagePixel: () => {
      const canvas = panel.querySelector("canvas");
      return canvas ? { width: canvas.width, height: canvas.height, clientWidth: canvas.clientWidth, clientHeight: canvas.clientHeight } : null;
    }
  };

  wireHostRankPills();
  render();
  panel.__panel = api;
  if (!window.__panel) window.__panel = api;

  return { ...api, setRank, selectPart, context, panel };
}
