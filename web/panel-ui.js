/**
 * Renderers in the host tool's own vocabulary.
 *
 * These mirror the markup src/app.js emits for keyValueRows(), workedSheet() and its worked
 * tables, so the panel's numbers read exactly like the numbers elsewhere in the tool. The
 * class names come from styles.css and are not redefined anywhere.
 *
 * The only extension to that vocabulary: a worked step may be kind "given" (a value READ from
 * the selection result) as well as kind "math" (arithmetic shown here). The two tags stay
 * Formula / With your numbers for arithmetic; a read step is tagged Field / As selected so the
 * two are never confused. The worksheet block itself is unchanged.
 */

export function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

export function keyValueRows(entries) {
  return `<dl class="key-value">${entries.map(([key, value]) => `<dt>${key}</dt><dd>${value}</dd>`).join("")}</dl>`;
}

let workedSheetCounter = 0;

/**
 * "64 x 1.5 = 96 m³" + a result row of "= 96 m³" is the same number twice. If the substitution
 * ends in the step's own value, keep the operands and let the result row state the answer.
 */
function trimDuplicatedResult(substitution, value) {
  if (typeof substitution !== "string" || !Number.isFinite(value)) return substitution;
  const match = /^(.*?)\s*=\s*(-?[\d.,]+)\s*([^\s=]*)\s*$/.exec(substitution);
  if (!match) return substitution;
  const tail = Number(match[2].replace(/,/g, ""));
  if (!Number.isFinite(tail)) return substitution;
  const tolerance = Math.max(Math.abs(value) * 1e-6, 1e-9);
  if (Math.abs(tail - value) > tolerance) return substitution;
  return match[1];
}

function workedStepRow(step, index) {
  if (step.kind === "note") {
    return `<div class="worked-step worked-note">
      <div class="worked-step-head"><span class="worked-step-n">i</span><span class="worked-step-label">${escapeHtml(step.label)}</span></div>
      <p class="worked-why">${escapeHtml(step.why)}</p>
      ${step.reference ? `<p class="worked-ref">Source: ${escapeHtml(step.reference)}</p>` : ""}
    </div>`;
  }
  const valueCell = step.value === null || step.value === undefined
    ? ""
    : `<div class="worked-result"><span class="worked-eq">=</span><span class="worked-value">${new Intl.NumberFormat("en-US", { maximumFractionDigits: step.digits ?? 5 }).format(step.value)}</span> <span class="unit">${escapeHtml(step.unit)}</span></div>`;
  const substitution = trimDuplicatedResult(step.substitution, step.value);
  const formulaTag = step.kind === "given" ? "Field" : "Formula";
  const substitutionTag = step.kind === "given" ? "As selected" : "With your numbers";
  return `<div class="worked-step${step.kind === "given" ? " worked-given" : ""}">
    <div class="worked-step-head"><span class="worked-step-n">${index}</span><span class="worked-step-label">${escapeHtml(step.label)}</span></div>
    <p class="worked-why">${escapeHtml(step.why)}</p>
    <div class="worked-math">
      <div class="worked-formula"><span class="worked-tag">${formulaTag}</span><code>${escapeHtml(step.formula)}</code></div>
      <div class="worked-substitution"><span class="worked-tag">${substitutionTag}</span><code>${escapeHtml(substitution)}</code></div>
      ${valueCell}
    </div>
    ${step.reference ? `<p class="worked-ref">Source: ${escapeHtml(step.reference)}</p>` : ""}
  </div>`;
}

export function workedSheet(sheet, { open = false } = {}) {
  workedSheetCounter += 1;
  let stepNumber = 0;
  const steps = sheet.steps.map((step) => {
    if (step.kind !== "note") stepNumber += 1;
    return workedStepRow(step, stepNumber);
  }).join("");
  return `<details class="worked" ${open ? "open" : ""} data-sheet="${workedSheetCounter}">
    <summary><span class="worked-summary-title">Show every step — ${escapeHtml(sheet.title)}</span></summary>
    <div class="worked-body">
      <p class="worked-purpose">${escapeHtml(sheet.purpose)}</p>
      <div class="worked-steps">${steps}</div>
    </div>
  </details>`;
}

const STATE_LABEL = { pass: "within limit", tight: "close to limit", violation: "over limit" };

export function stateTag(state) {
  return `<span class="pp-tag state-${state}">${escapeHtml(STATE_LABEL[state] ?? state)}</span>`;
}

export function materialTag(family) {
  const value = family ?? "undeclared";
  const label = family ? family : "material not declared";
  return `<span class="pp-tag material" data-family="${escapeHtml(value)}"${family ? "" : ' title="This catalog record carries no material field."'}>${escapeHtml(label)}</span>`;
}

/** Every limit check in one table, with the values and the arithmetic that produced them. */
export function checksTable(result, { stressed = false } = {}) {
  const rows = result.checks.map((check) => `<tr data-part="${check.partId}" data-state="${check.state}">
      <td><strong>${escapeHtml(check.name)}</strong></td>
      <td>${escapeHtml(check.unit)}</td>
      <td>${new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 }).format(check.value)}</td>
      <td class="worked-why-cell">${escapeHtml(check.limitLabel)}</td>
      <td>${new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 }).format(check.utilization)}</td>
      <td>${stateTag(check.state)}</td>
    </tr>`).join("");
  const counts = result.checks.reduce((acc, check) => {
    acc[check.state] = (acc[check.state] ?? 0) + 1;
    return acc;
  }, {});
  return `<div class="pp-check-summary">
      <strong>${result.checks.length} checks</strong>
      <span class="pp-tag state-pass">${counts.pass ?? 0} within limit</span>
      <span class="pp-tag state-tight">${counts.tight ?? 0} close to limit</span>
      <span class="pp-tag state-violation">${counts.violation ?? 0} over limit</span>
      <span class="pp-badge-note">utilisation ≥ 0.9 reads as “close to limit”; &gt; 1.0 is over it${stressed ? " — spec switch on" : ""}</span>
    </div>
    <div class="table-scroll"><table class="worked-table">
      <thead><tr><th>Check</th><th>Unit</th><th>Value</th><th>Limit held against</th><th>Utilisation</th><th>State</th></tr></thead>
      <tbody>${rows}</tbody></table></div>`;
}
