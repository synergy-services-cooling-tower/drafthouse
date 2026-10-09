# Validation Results

**Prototype version:** 0.4.3
**Catalog revision:** `illustrative-catalog-v0.1` (revision 2026-08-13, status `SYNTHETIC / NOT VENDOR DATA`)
**Recorded:** 19 September 2026 on branch `impl/ct-validation`, re-derived 20 September 2026 on branch `impl/ct-retire` (issue #40 slice 4 and its fix round 1); every figure below was re-derived at this head after the JavaScript reference retired, and the legs it names are the ones that ran.
**Node.js:** v26.7.0 for the issue #37 recording; the figures below are this head's own runs under the CI pin v20.20.2.

This is the single source of validation results. `docs/VALIDATION_RESULTS.md` points here.

Chain (from `package.json`): static, version-check, native-icons-check, test, smoke, server-smoke, deploy-check, parity-wasm, bundle-check, retirement-check

The engine is the Rust engine (`rust/`), native and built for wasm — **ported and drift-guarded, not
validated**: the parity harness diffs it against the recorded regression baseline
(`validation/regression-baseline.json`), which is a drift check and not a statement about the physics.
The JavaScript reference the port was checked against while it existed was **retired** in issue #40
slice 4; the last commit that contains it is recorded in the private decision record (D17).
Figures whose only source was the JavaScript suite are now
carried by the Rust test suite or have been removed with a note.

`tests/validation-results-drift.test.js` keeps the gated figures honest. It re-derives the test count, the
chain, the prototype version, the catalog revision and the smoke figures from the repository itself and
fails when any of them stops matching this file. It never rewrites this file: the failure names both
numbers and the command to re-derive them. The recording date and the node version are *not* gated — they
describe the environment a run happened in, not a claim about the current one.

## Commands completed

`npm run validate` (the chain above, run in order) completed with **exit status 0**. Each leg is
listed with its headline output and the raw exit of its own run; the logs recorded for the slice
that retired the JavaScript surface are kept with the maintainers (the preconditions before the
deletion, the log at that retirement head, and the fix round that swept the surviving tree for
dangling references, deleted `web/public-entry.js` and reinstated the chain's `static` leg); issue
#60's own re-derivation is kept there too. The bundle figures quote the pieces and
their digests and omit the version string, which embeds the tree commit and therefore changes with
every commit.

| Leg | Command | Headline (this head's own run, raw exit 0) |
| --- | --- | --- |
| static | `npm run static` | `{"passed": true, "scannedFiles": 64, "syntaxChecked": 40, "javascriptFiles": 40, "htmlDocuments": 1, "markdownFiles": 23, "references": 55, "licenceChecks": 3}` — every surviving module parses (`node --check`), every relative reference the tree makes resolves (JS imports, the served document's subresources, markdown links) and the licence claims hold (the six modules issue #72 adds for the native installers' icons and release path are the difference from the 62/34/34/1/27/58/2 this row read before; at the issue #67 base the same figures were 75/44/44/2/29/79/2) |
| version-check | `npm run version-check` | `{"passed": true, "verified": false, "version": "0.2.0", "newestTag": null, …}` with `version-check: NOT VERIFIABLE — no v* tag is visible in this clone` on stderr — `package.json`'s version is the source of truth: the five `Cargo.toml`s and both `Cargo.lock`s record it, and it equals the newest `v*` tag or is strictly greater (a release tag whose version was never bumped fails, naming both; a published snapshot tree carries no `v*` tags of its own, so the guard prints `NOT VERIFIABLE` instead of passing silently — its own run above). Since issue #72 the guard also reads the packager metadata: `cockpit/Cargo.toml`'s `[package.metadata.packager]` carries **no** version of its own (so `cargo-packager`, which fills that field from the crate, is fed the guarded number), and a version written into it later must equal the source of truth or the guard fails naming both. The bite proof, with raw exits, is kept with the maintainers |
| native-icons-check | `npm run native-icons-check` | `{"passed": true, "source": "cockpit/icons/mark.svg", "pngs": ["32x32.png 32x32", "128x128.png 128x128", "128x128@2x.png 256x256", "icon.png 512x512"], "ico": [16, 32, 48, 64, 128, 256]}` — the installer icon set (issue #72) is re-derived from `cockpit/icons/mark.svg` and compared pixel for pixel: the committed PNGs and the PNG payloads inside `icon.ico` are exactly what the neutral mark draws at their sizes, and no file outside the set is present |
| test | `npm test` | 105 tests, 105 pass, 0 fail — see below |
| smoke | `npm run smoke` | the JSON block below: the documented duty through the pinned engine piece |
| server-smoke | `npm run server-smoke` | `{"passed": true, "results": [ ... ]}` — six cockpit paths (`/`, the wasm glue, the wasm module, the fixture record, the duty field list, a subset font), HTTP 200, expected MIME types, non-empty bodies (issue #58 re-pointed this leg at the surface that is served; a tree without a built plane prints the NOT VERIFIABLE diagnostic instead) |
| deploy-check | `npm run deploy-check` | `{"passed": true, "surface": "cockpit", "document": "cockpit/index.html", "fileCount": 10, "totalBytes": 295252, "assets": 9, "manifest": "deploy-manifest.txt", "manifestVerified": true}` (the deploy set is the cockpit plane: its document, the committed assets and the pins it vendors; issue #60 re-pointed this leg at the served surface; issue #73's rename moved the fixture's own bytes, 96851 -> 96797, and the sum with them; issue #74 added `cockpit/assets/revision-illustrative-catalog-v0.1.json` - the first catalog revision, shipped in the binary and the wasm module - and the page's internal-only file group, 240500 -> 295252) |
| parity-wasm | `npm run parity-wasm` | `compared 11524 quantities: 11524 pass, 0 fail` / `every quantity is within tolerance` / `wasm vs native: 299 replies compared — 223 identical to the last bit, 76 alike within 1e-12, 0 beyond` (the recorded regression baseline; the wasm binding section is not compared — see "Engine drift check") |
| bundle-check | `npm run bundle-check` | `{"passed": true, "pins": ["bundles/engine.manifest.json", "bundles/visuals.manifest.json"], "pieces": [{"piece": "engine", "files": 2, "bytes": 633427, "digest": "6d0b545c…"}, {"piece": "visuals", "files": 9, "bytes": 2289937, "digest": "9a4e0eaf…"}]}` (the pin checks file digests, not the commit label: the manifests were published from `6beb8bc`, while the repository's head is newer — `npm run bundle-check` prints both) |
| retirement-check | `npm run retirement-check` | passed — the retirement gate (issue #60, D22): the retired JavaScript surface is absent from the tree and no artifact the deployment ships references the retired module (the gate's raw JSON is kept with the maintainers) |

No external package installation: the chain has no install step and the repository has no runtime
dependencies.

## Automated test result

Reproduce with `npm test` (which runs `node --test tests/*.test.js`); raw exit status **0** at this
head. After issue #60 the suite is the four files whose subject survived the retired surface: the
deployment-shaped serving tests (`tests/deployment-serving.test.js`), the deploy-set check
(`tests/deploy-check.test.js`), the retirement gate's regressions (`tests/retirement-check.test.js`)
and the drift gate itself (`tests/validation-results-drift.test.js`); issue #92 adds the workflow
lint (`tests/workflow-lint.test.js`), which checks each workflow against its own shape — the private
CI's billed-minutes tier, and the generated public CI staying untiered; issue #81 adds the
small-screen geometry (`tests/small-screens.test.js`), which runs
`docs/design/small-screens-r1/tools/check.mjs` over the round's own frames and — the control the check
needs — over the frames captured before the fix, where it must fail naming the label overlap and the
dropped answer card. Issue #83 adds the money gate's regressions
(`tests/currency-check.test.js`), which run `cockpit/tools/currency-check.mjs` over a copy of the
delivered tree, then over planted currency strings (the plane document, the fixture record, the
app's own sources, a built module) and two fail-closed cases, and pin that the rule's own prose
(`cost`, `money`) is exempt while every other token stays literal. Issue #117 adds the comparison's
parity test (`tests/compare-web-parity.test.js`), which drives the web plane's compare picker in the
real headless browser and the native route's own helper through cargo over the same two saved files,
then requires the two exported sheets to be the same bytes; where the built plane, the built native
target or the browser is absent it reports NOT VERIFIABLE rather than skipping. Issue #72's fix
round adds the tamper self-test's own regressions (`tests/tamper-ident.test.js`), which build the
published native set and the published bundle out of the repository's own writers and hold both
verifiers to the contract that a tamper dispatch's RED names the self-test — while an unlabelled
mismatch keeps the plain, unqualified failure a real run must have. Issue #135 adds the click-sensing
audit (`tests/click-sensing.test.js`), which reads every `.rs` file under `cockpit/src`, traces each
`clicked()` / `double_clicked()` / `drag_started()` read back to the binding that built the response, and
fails on the two shapes that cannot sense a click (`dnd_drag_source`'s drag-only response and
`Frame::show(...).response`'s hover-only one) — with the pre-fix shape of each of the three sites it was
written for driven through the same scanner, so the check is shown to bite. Issue #137 adds the
no-developer-text test (`tests/dev-text.test.js`): the rule, every literal in the modules that paint
text, and the round's committed after captures - which must pass - and before captures, which must
fail; where `design/137` is absent it reports NOT VERIFIABLE. Issue #148 adds the plane's addressing guard (`tests/cockpit-plane.test.js`): two builds of
different bytes must share no payload URL, a rebuild of the same bytes must keep every URL, and a
packed plane's document references must resolve inside the plane it will be served from — with the
pre-fix identity (`addressPlane` returning flat names) driven through the same checks, where four of
the seven fail. The JavaScript
engine's own tests were the reference's, and went with it. Node's runner decorates these lines (`ℹ` in its spec reporter, `#` in
TAP, depending on where stdout goes); the decoration is dropped here, everything else is verbatim, and
the duration line is left out because it changes with the host:

```text
tests 105
suites 0
pass 105
fail 0
cancelled 0
skipped 0
todo 0
```

The suite is not build-free: `node --test tests/*.test.js` runs the smoke-drift gate, which executes the
recorded wasm engine build (`bundles/engine.manifest.json`) into `rust/target` through `scripts/smoke.mjs`
— the quick tier installs the wasm32 target and caches `rust/target` for it. No browser, no network is
required: issue #117's parity test uses a headless browser and a built plane where both exist, and
reports NOT VERIFIABLE where they do not. The
engine halves — the native binary and the wasm build — are exercised by `npm run smoke` (through the
pinned engine piece), the parity legs of the chain and `cargo test`.

For the record, the earlier recordings this file carried: the issue #37 run counted 157 tests; the
run that followed it counted **163 tests** and exited **1** — 162 passed and only the
documented-count gate failed because this record still said 157 — and the suite stood at **179**
before this slice removed the JavaScript-side files with the engine.

### External accuracy anchors

These compare against published reference values or relations, not against this code's own output.
"Independent implementation" means a separate codebase implementing the same published correlation;
it is a real check against transcription error, but it is not metrological data. Every anchor below is
carried by the crate's own suite (`rust/tests/anchors.rs`, run by `cargo test`); the JavaScript tests
that first established them were retired with the reference (issue #40 slice 4).

| Check | Reference | Tolerance | Result |
| --- | --- | --- | --- |
| Saturation vapour pressure, 0–50 °C, 101 325 Pa | ASHRAE published tables | 0.05 % | pass |
| Saturated humidity ratio, 0–50 °C | ASHRAE published tables | 0.2 % | pass |
| Saturated-air enthalpy, 0–50 °C | ASHRAE published tables | 0.4 kJ/kg | pass |
| Simplified-path state at 30 °C / 50 % RH | PsychroLib (independent implementation) | 2e-7 kg/kg | pass |
| Simpson Merkel quadrature, 5 conditions | independent reference integration | 0.01 % | pass |
| Four-point Tchebycheff, 5 conditions | independent reference integration | 0.1 % | pass |
| **Ice-branch saturation pressure, −60…0 °C** | **IAPWS R14-08(2011) Eq. (6)**, checked against the release's own Table 3 verification value first | 0.05 % | pass, worst 0.033 % |
| **Saturation pressure over ice, −20…0 °C** | ASHRAE over-ice table values | 0.1 % | pass, worst 0.020 % |
| **Saturated humidity ratio below freezing (enhanced)** | ASHRAE 0 °C table value; PsychroLib recorded values at −20/−5/−50 °C | 0.2 % / 0.25 % | pass, worst 0.002 % / 0.080 % |
| **Below-freezing wet-bulb relation** | **independent first-principles adiabatic-saturation derivation** with published constants (1.006, 1.86, 2501, 333.4, 2.1) | 0.3 % | pass, worst 0.248 %; the residual is the handbook's rounded 2830 kJ/kg sublimation group against 2834.4 = 2501 + 333.4 |
| **Simplified-path pressure scaling, 61 660–105 000 Pa** | published ASHRAE equation with published saturation pressures (temperature-only data) | 1e-12 relative, and 0.02 % against the tables' own rounding | pass, 1.8e-16 / 0.008 % |
| **Enhancement factor away from 1 atm** | Buck (1981) published form; WMO/Sonntag pressure-only form (doi:10.1175/2100.1) | exact / 0.1 % | pass, 0 / worst 0.070 % — **but see the open item below** |

The enhancement-factor treatment is also required to be measurably better than the simplified
equation set against the ASHRAE tables, so it cannot be silently dropped again.

**What is not externally anchored.** `validation/test-vectors.json` is not evidence of accuracy: its
`expected` values were generated by this repository's own code (`scripts/generate-vectors.mjs`, retired
with the JavaScript reference in issue #40 slice 4), and the file says so in its `provenance` block.
The `psy-sub-zero` vector (−10 °C dry bulb, −12 °C wet bulb) is one of those. It *is* exercised:
`rust/tests/vectors.rs` replays it against the engine and the parity harness replays it against the
native binary and the wasm build. What it proves is that the engine still produces the recorded values
and that the recorded behaviour has not moved; it cannot show they are right. The accuracy claims for
that range come from the anchored cases above, which use their own inputs.

### The two Tier-1 paths (`docs/VALIDATION.md` items 1.2 and 1.3)

**1.2 Sub-zero psychrometrics — covered as far as external anchors exist.**

- the ice branch (`dryBulbC <= 0.01`) agrees with the official IAPWS R14-08(2011) sublimation
  equation to 0.033 % worst case over −60…0 °C, inside that equation's own uncertainty (~0.04 % at
  −10 °C, ~0.28 % at −60 °C), and with ASHRAE over-ice table values to 0.020 %;
- the below-freezing wet-bulb relation agrees with an independent first-principles
  adiabatic-saturation derivation to 0.248 % worst case, with the residual explained by the
  handbook's rounded 2830 kJ/kg sublimation group;
- saturated humidity ratios below freezing agree with the ASHRAE 0 °C table value and with
  PsychroLib's recorded references to 0.080 % worst case;
- **still open**: the relation's low-temperature positivity limit and the engine's silent floor of
  an impossible input — documented in `LIMITATIONS.md` §6, reported not fixed; and any measured
  sub-zero air data, which this repository does not hold.

**1.3 Non-standard pressure — partly covered.**

- the simplified (enhancement-off) path is *exactly* pressure-scaled given the saturation pressures,
  which are anchored at standard pressure by the ASHRAE tables, so its accuracy at altitude follows
  from an anchored quantity and an exact equation;
- the enhancement factor matches Buck (1981) to the last bit and the independent WMO/Sonntag
  pressure-only approximation to 0.070 % over 62–105 kPa;
- **still open**: ASHRAE's full enhancement factor is a function of temperature *and* pressure
  (Hyland–Wexler / RP-1485). No external anchor for the real-mixture treatment away from 1 atm is
  held in this repository, so its accuracy away from 1 atm is **unverified**. Matching two published
  pressure-only approximations bounds how the approximation behaves; it does not measure it.

### Numerical convergence

Crossflow solver, reference case (42 °C hot water, 33/27 °C air, L = 200 kg/s, G = 150 kg/s,
Me = 1.6). Reproduce with the engine's own commands:

```bash
cargo build --manifest-path rust/Cargo.toml
rust/target/debug/ct-engine crossflow --hot 42 --db 33 --wb 27 --water 200 --dry-air 150 --kavl 1.6 --cells 18
rust/target/debug/ct-engine crossflow --hot 42 --db 33 --wb 27 --water 200 --dry-air 150 --kavl 1.6 --cells 18 --no-richardson
rust/target/debug/ct-engine crossflow --hot 42 --db 33 --wb 27 --water 200 --dry-air 150 --kavl 1.6 --cells 384 --no-richardson
rust/target/debug/ct-engine convergence --hot 42 --db 33 --wb 27 --water 200 --dry-air 150 --kavl 1.6 --base-cells 12
```

| Quantity | Value |
| --- | --- |
| Raw 18 × 18 grid | 32.13552 °C |
| Richardson-extrapolated 18 × 18 (the value the solver reports) | 32.33335 °C |
| Raw 384 × 384 reference | 32.32224 °C |
| Raw-grid error | −0.18672 °C |
| Reported-value error | +0.01111 °C |
| Observed order of convergence (12/24/48 study) | 1.03771 |
| Grid Convergence Index (12/24/48 study) | 0.27076 % |

The observed order confirms the expected first-order behaviour of the explicit cell scheme. The
same study at base 18 cells gives order 1.0243 and GCI 0.1822 %; both are consistent with first
order, and the study is not called by the selector (it costs about seven times a single sweep).

### Behavioural checks that can fail

Replacing the two identity assertions noted in `LIMITATIONS.md` §15, the crate's own suites
(`rust/tests/*.rs`, run by `cargo test`) check: monotonic cooling with available transfer and
thermodynamic limiting cases; discretisation error falling under refinement and first-order behaviour;
each minor loss evaluated at its own section velocity and a material share of the total; quadratic
scaling of every loss with air flow; the fan-stack term charged against a total-pressure curve and not
against a static one; velocity recovery reducing the discharge loss; spray and rain zones contributing
to the available Merkel number; natural-draft balance and its response to stack height; Merkel
quadrature against an independently written reference integration; refusal messages byte-for-byte
against the reference's; and the selection's feasibility, ranking and tie-break chains.

### Worked-calculation checks

The step-by-step sheets (the `worked` field of an engine reply) are the engine's own output: every
displayed number is a value the engine computed, and the quantities the harness compares include the
worked cases it recorded (for example the worked water balance, whose evaporation/drift/blowdown/makeup
figures are in the recorded set). A served surface rendered those steps directly until issue #60 retired
it; the numbers themselves are the engine's own and are unchanged. The HTML-rendering checks
for the deleted example sheet's renderer (balanced tags, leaked `undefined`/`NaN`, unevaluated
placeholders, escaping) went with that sheet and its tests; the served surface's pixels and requests
are covered by the browser audits (`npm run deployment-audit`, the hosted `deploytest-audit` runs)
and the pinned bytes by `npm run bundle-check`.

## Deterministic smoke result

Reproduce with `npm run smoke`; raw exit status **0**, byte-identical across runs. It runs the
documented duty through the pinned engine piece (`bundles/engine.manifest.json`), and every number below is
the engine's own reply:

```json
{
  "demandMerkel": 1.4926843129361054,
  "recoveredColdWaterC": 31.99999992370606,
  "identicalPointCapabilityPct": 100.00000034968058,
  "feasibleSelections": 202,
  "topSelection": {
    "tower": "IDCF-064",
    "fill": "TRICKLE-50",
    "drift": "DE-4P-ULTRA",
    "fan": "AX-500",
    "coldWaterC": 31.687489913940436,
    "electricalInputKW": 31.57874035871032
  }
}
```

The top selection is the engine's own default-objective recommendation, not the JavaScript reference's
lifecycle-cost ranking: the port deliberately reads no cost field, so the cost-ranked leader the
JavaScript engine reported (`FILM-VF38` / `DE-3P-10`, 31.503 kW) is no longer the first row.

## Static-server smoke result

Reproduce with `npm run server-smoke`; raw exit status **0**. The static server's root is the served
surface's directory (the cockpit plane), so the paths are the release's own: HTTP 200 with the expected
MIME type and a non-empty body for `/` (`cockpit/index.html`), `/pkg/drafthouse_cockpit.js`
(`text/javascript`), `/pkg/drafthouse_cockpit_bg.wasm` (`application/wasm`), the fixture record and
`/assets/fonts/subset/IBMPlexMono-Regular.ttf`. The counts come from the run's own JSON output.

## Engine drift check (the recorded regression baseline)

The parity harness (`scripts/parity/run.mjs`) diffs the engine under test — the native binary, or the
wasm build of the same engine — against the **recorded regression baseline**
(`validation/regression-baseline.json`), per case and per quantity, inside the tolerance that quantity
already carries. The live JavaScript comparison this harness once ran was retired with the reference
(issue #40 slice 4): `--reference live` is refused (exit 2) and `--write-baseline` is retired, so a live
comparison cannot run and cannot be skipped into a pass.

| Leg | Command | Result (raw exit 0) |
| --- | --- | --- |
| native, recorded baseline | `node scripts/parity/run.mjs --reference frozen` | `compared 11524 quantities: 11524 pass, 0 fail` |
| wasm, recorded baseline | `npm run parity-wasm` | `compared 11524 quantities: 11524 pass, 0 fail`; `wasm vs native: 299 replies compared — 223 identical to the last bit, 76 alike within 1e-12, 0 beyond` |
| the crate's own suites | `cd rust && cargo test` | the anchors, the recorded vectors, the behavioural checks and the wasm engine's tests: all suites green |

Two things the recorded baseline cannot hold, named rather than silently dropped: the **wasm binding
section** (rows that diffed `rust/wasm/binding.mjs`'s data-only JavaScript surface against the
reference — a JavaScript surface with no engine reply behind it, so it has no recorded counterpart; its
bytes are pinned and checked by `npm run bundle-check`, and its behaviour is
exercised by every wasm reply the cross-check above compares) and the **reference's own refusals** (the
recorded refusals assert the engine still refuses, with the same status and error kind).

The recorded baseline is **mutation-probed** (issue #40 slice 4): a
deliberate change to one Rust quantity turns the native leg red, restoring the file byte-identically
turns it green — so a green leg cannot mean "compared nothing".

The drift check is not accuracy: the baseline records what the two implementations agreed on at capture,
so a shared mistake would pass it. The engine is ported and drift-guarded, **not validated**.

## CTI comparison status

**No CTI ToolKit comparison has been made.** `validation/cti-comparison-matrix.csv` is the prepared
input matrix for that run, and its `CTI_ToolKit_result` and `delta_pct` columns are empty for every
row because no licensed CTI ToolKit run has filled them. The file carries that statement at its top.
No figure in this document implies a comparison that never happened.

## What changed since the 14 August 2026 record

The 14 August 2026 record claimed 51 tests. The suite has grown as slices landed (engineering
checks, the educational sheets, the deployment and serving contracts, the vectors replay), reached
**179** by issue #40 slice 3, and after slice 4 retired the JavaScript reference it is **76** — the
JavaScript-side files went with the engine (the per-file decisions are kept with the maintainers). The chain's `static` leg, which slice 4 dropped with the
prototype surface it checked, is back in the fix round: `scripts/static-check.mjs` now resolves every
relative reference the surviving tree makes — JavaScript imports (static and dynamic), the served
document's subresources, markdown links — and keeps the licence assertions the retired check carried,
so a dangling reference fails before any test runs. The checks that had no subject left to check (the
prototype's HTML ids, its exported API surface, the renderer's classes) are gone with that surface.
The model changes recorded earlier — the water-vapour enhancement factor applied by default, minor-loss
coefficients referenced to their own section velocities, the fan-stack discharge term charged against
total-pressure curves, spray- and rain-zone transfer included, the crossflow solver
Richardson-extrapolated, `minimumThermalMarginC` defaulting to 0.3 °C — are all still in place, and the
resulting figures (202 feasible candidates, 1.4927 Merkel demand, 31.579 kW top-candidate electrical
input under the engine's own default objective) reproduce in the smoke result above. The 31.503 kW /
`FILM-VF38` row this document used to quote was the JavaScript reference's lifecycle-cost leader, which
the engine does not rank on.

## Interpretation

These checks demonstrate software consistency, numerical convergence, agreement with published
psychrometric reference data, and agreement between the engine (native and wasm builds) and the recorded
regression baseline. They do **not** establish CTI Toolkit equivalence, MRL equivalence, ATC-105 or
ATC-140 compliance, CTI certification, manufacturer rating accuracy, component suitability, or
contractual validity.

The bundled catalog remains synthetic. Correcting *where* the pressure terms are referenced does not
make the coefficients real, and the bundled data still selects towers on the optimistic side of
plausible when compared against ASHRAE 90.1 minimum efficiencies. The external validation work
described in `docs/VALIDATION.md` remains necessary before engineering deployment.
