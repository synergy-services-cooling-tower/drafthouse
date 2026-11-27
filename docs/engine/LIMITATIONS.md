# Limitations and Production Gaps

## Status summary

| Area | Prototype status | Required production work |
|---|---|---|
| Psychrometrics | ASHRAE-style equations and inversions | Full validation over supported temperature/pressure range and approved equation control |
| Water properties | Engineering approximations | Validated IAPWS/TEOS-10-compatible package and uncertainty |
| Counterflow thermal | Classical Merkel, uniform 1-D | Lewis-factor/Poppe option, varying liquid flow, rain/spray zones, maldistribution |
| Characteristic capability | Public curve concept | Complete licensed ATC-105 requirements and Toolkit benchmark agreement |
| Performance curves | Rectangular record interpolation/inversion | Licensed correction/crossplot rules, controlled curve imports, no-extrapolation policy |
| Fill | Synthetic power laws | Real tested multidimensional data, depth effects, provenance, uncertainty |
| Drift | Synthetic piecewise curves | Controlled test evidence, water-loading effects, bypass/sealing, droplet distribution |
| Fan | Piecewise pressure/efficiency curves and affinity scaling | Manufacturer validation, system effects, pressure definitions, stall/noise/vibration |
| Distribution | Orifice hydraulics/count only | Header/gravity-basin balance, spray coverage, turndown, clogging |
| Water balance | Saturated outlet approximation | Rigorous heat/mass model and chemistry integration |
| Crossflow | Simplified 2-D topology, solved in the cockpit with its grid, error estimate and 3-grid study shown | Validated local transfer correlations and nonuniform distributions |
| Natural draft | Uniform effective-height buoyancy | Vertical/radial zones, shell/rain losses, wind, multiple fills/rings |
| Selection | Synthetic lifecycle ranking | Controlled costs, constructability, structural, material, maintenance, risk constraints |

## 1. Certification and contractual use

The software is not certified by the Cooling Technology Institute. Using Merkel equations or a CTI-style curve intersection does not make a program CTI certified. Do not use its output as a contractual acceptance determination until the current licensed code has been implemented, verified, and independently reviewed.

## 2. MRL equivalence

The program is not an MRL replacement. Public information describes MRL program families but not source code, proprietary correlations, fill data, or exact iteration logic. Numerical agreement must be established case by case with licensed software.

## 3. Synthetic component data

All included tower, fill, drift, fan, nozzle, material, and cost records are fictional. The calculated “best candidate” only demonstrates a coupled search. It must not influence procurement, guarantees, or field modifications.

## 4. Uniform-flow assumptions

The counterflow selector assumes uniform:

- water loading;
- dry-air loading;
- fill depth;
- inlet-air state;
- component condition.

Real towers may have maldistribution, blocked/fouled fill, basin/nozzle imbalance, recirculation, leakage, bypass, structural blockage, fan inlet distortion, and cell interaction.

## 5. Merkel assumptions

The classical Merkel model combines sensible and latent transfer through an enthalpy potential. This implementation assumes:

- approximately constant circulating-water flow;
- a simplified operating line;
- saturated interface air;
- no explicit Lewis-factor variation;
- no droplet carryover heat transfer;
- no axial conduction.

A Poppe-type option should be added for more explicit evaporation and outlet-air-state work.

## 6. Water and psychrometric properties

The psychrometric equations are implemented independently and are checked against published ASHRAE
table values at standard pressure over 0–50 °C (saturation pressure within 0.05 %, humidity ratio
within 0.2 %, saturated enthalpy within 0.4 kJ/kg). They have **not** been certified over every
supported input, and the checked range is narrower than the −100 to 200 °C software domain.

Below freezing the ice branch (`dryBulbC <= 0.01`) is checked against the IAPWS R14-08(2011)
sublimation equation, Eq. (6), from −60 °C to 0 °C (worst case ≈0.03 %, inside that equation's own
uncertainty) and against ASHRAE over-ice table values; the below-freezing wet-bulb relation is
checked against an independent first-principles adiabatic-saturation derivation using published
constants (within 0.25 %, the residual being the handbook's rounded 2830 kJ/kg sublimation group
against 2834.4 = 2501 + 333.4) and against PsychroLib's recorded reference values (≤0.08 %). Those
are checks against published *relations and tables*, not against measured air: no sub-zero
measurement is held in this repository.

**The below-freezing wet-bulb relation cannot carry a large depression at very cold temperatures.**
It is a linearised adiabatic-saturation balance, and cold air holds little moisture to evaporate:
at 101 325 Pa the largest depression the relation can return a non-negative humidity ratio for is
about 3.35 K at −10 °C, 1.55 K at −20 °C, 0.62 K at −30 °C and 0.22 K at −40 °C. Beyond that limit
`humidityRatioFromWetBulb()` silently floors the result at its minimum humidity ratio (1e-7) and
`psychrometricState()` returns that state — −30 °C dry bulb with −31 °C wet bulb reports a humidity
ratio of 1e-7 and a relative humidity of 0.04 % instead of refusing the pair. The independent
derivation reproduces the same negative value, so the limit belongs to the published relation rather
than to this implementation, but silently returning an impossible state is a gap: the pair should be
refused. `rust/tests/anchors.rs` (and, before the reference retired, `tests/psychrometrics-cold-pressure.test.js`)
records the current behaviour so that any future change is a visible decision.

The water-vapour enhancement factor uses the pressure-only Buck (1981) approximation rather than the
full Hyland–Wexler/ASHRAE RP-1485 formulation, which also depends on temperature. It matches Buck's
published form exactly and an independent published pressure-only approximation (the WMO/Sonntag
form, doi:10.1175/2100.1) to within 0.07 % over 62–105 kPa — but **no external anchor for the
real-mixture treatment away from 1 atm is held here**, so that accuracy remains unverified.
Agreement also degrades above roughly 50 °C.

The ice branch applies the same pressure-only factor as the liquid branch. Buck (1981) publishes a
separate ice-surface form as well (1.0003 + 4.18e-6 P, P in hPa; see the NOAA PSL flux handbook,
Appendix A4.1); at 101 325 Pa it is 0.033 % higher than the form used here, and against the ASHRAE
0 °C saturated-humidity-ratio anchor the form used here is the closer of the two (+0.002 % against
+0.035 %). Recorded as an observation, not changed: it is a difference between two published
approximations, and no source held here decides which is right for ice in air.

The water density and heat-capacity approximations are not suitable for contractual seawater work.

## 7. Fill depth scaling

The sample characteristic and pressure drop scale linearly with fill depth. Real performance may not scale linearly because of entrance/exit regions, redistribution, support blockage, wetting, and test geometry. Use depth-specific test data where possible.

## 8. Pressure definitions

Fan and system curves must use compatible static/total pressure definitions and reference planes. The prototype now makes this explicit through `fan.pressureBasis` (`'total'` or `'static'`) and charges the fan-stack discharge velocity pressure only against a total-pressure curve. It still does not model all installation and system effects, and it evaluates every section at the entering air density rather than carrying density through the tower.

Loss coefficients are bound to named reference areas so that each `K` is evaluated at its own section velocity. This is a structural correction: an earlier revision referenced every minor loss to the fill-face velocity — the slowest section — which collapsed the combined inlet, distribution, support and plenum losses to under 6 Pa out of 162 Pa and materially understated fan power.

**The values remain synthetic.** Fixing where the terms are referenced does not make the coefficients real. Against ASHRAE 90.1's minimum open-circuit axial-fan efficiency, the bundled catalog still selects towers on the optimistic side of plausible. Loss coefficients, reference areas, stack recovery factors, and fan efficiencies must all come from the same validated test data and plane definitions before any of this output is used commercially.

## 9. Drift

Drift fraction is not reliably inferred from eliminator geometry alone. Bypass at seams and perimeter gaps can dominate. The prototype assumes the supplied curve represents the installed system, which is optimistic unless installation quality is controlled.

## 10. Water chemistry

The water-quality classes are qualitative placeholders. Real fill selection needs, at minimum:

- suspended solids and particle-size distribution;
- oil/grease;
- scaling indices and saturation margins;
- biological activity;
- treatment program;
- cycles of concentration;
- makeup chemistry;
- operating temperature;
- cleaning method and frequency.

## 11. Structural and mechanical constraints

The selector does not check:

- dry, wet, or fouled fill weight;
- support span and deflection;
- wind and seismic loads;
- fan shaft, gearbox, belt, and bearing life;
- vibration and resonance;
- motor starting current and electrical protection;
- fire classification;
- access, lifting, and replacement routes;
- basin, piping, pump, and NPSH limits.

## 12. Crossflow and natural draft

The crossflow grid demonstrates the correct directional topology but uses a simplified local effectiveness relation and uniform total `KaV` distribution. The natural-draft module uses one effective height and uniform plume density. Neither is a production-equivalent commercial rating model.

**What the cockpit's crossflow screen covers (issue #84).** The screen draws the section - side air inlets, a hot-water distribution basin over each fill pack, the plenum between them and the fan stack - and solves the engine's 2-D grid for the best crossflow candidate the selection run found at the duty. Its operating point is the selection chain's: the dry-air mass flow and the available `KaV/L` are that candidate's own airside values, with the duty's water flow and salinity. The panel prints the mesh it solved (14 × 14 cells, doubled to 28 × 28 for the Richardson pass), the engine's discretisation-error estimate and the optional three-grid (12/24/48 cells) convergence study (observed order, grid-convergence index, extrapolated cold water), so the solver's numerical quality sits next to its answer. The error estimate is the engine's conservative upper bound on the **fine grid** - not on the reported extrapolated value, which is normally closer.

**What it does not cover.** The grid is one fill pack's through-flow field: it does not model the tower's three-dimensional inlet and plenum distribution, fan-induced maldistribution, the spray zone above the fill, the rain zone below it, or basin mixing. It is not a rating - the candidate's capacity and the screen's selection remain the synthetic characteristic chain, and the study's observed order is a property of the cell scheme, not of the fill data. Natural draft stays out of this surface entirely (a separate module, and a separate later issue if wanted).

## 13. Economics

Costs are synthetic. The lifecycle objective excludes inflation, taxes, demand charges, carbon, discharge fees, downtime, spare parts, labor escalation, financing, exchange-rate risk, and project-specific construction risk.

## 14. Performance-curve data

The included performance grid is generated from a simple synthetic formula. It is not a manufacturer curve. Interpolation outside the supported wet-bulb/range domain may clamp to the nearest bracket, while water-flow interpolation may extrapolate; a production importer should reject unsupported extrapolation unless explicitly approved.

## 15. Numerical failure is meaningful

A rejected calculation may indicate:

- an enthalpy pinch;
- no cold-water root inside physical bounds;
- no fan/system or draft/system intersection;
- an incomplete performance grid;
- a candidate outside its curve domain.

Do not suppress these failures by extending curves without engineering review.


## 15. Numerical convergence and what the tests prove

The crossflow solver is first order in cell count. A raw 18 × 18 grid is about 0.18 °C optimistic; Richardson extrapolation from a doubled grid brings this under 0.01 °C, and `crossflowConvergenceStudy()` confirms the observed order is 1.04. The cockpit's crossflow panel runs that study (12/24/48 cells) beside its answer and prints the mesh and the extrapolation's error estimate (issue #84). Other solvers (Merkel quadrature, cold-water bisection, fan/system intersection, natural-draft balance) have **not** received an equivalent published convergence study — see `VALIDATION.md` §7 for the outstanding work.

Two earlier tests asserted algebraic identities rather than physics:

- "crossflow energy balance closes" — the water-side and air-side energy totals are equal by construction, so this passed at 1e-14 even on a 2 × 2 grid that was 2 °C wrong;
- "airside breakdown sums to total" — a sum of terms compared with the sum of the same terms.

Both have been replaced with checks that can fail: grid convergence against a fine reference, monotonic response, thermodynamic limiting cases, per-component reference-velocity verification, quadratic scaling with flow, and comparison against an independently written reference integration.

Passing tests still demonstrate software and numerical consistency only. They do not establish CTI compliance, MRL equivalence, manufacturer rating accuracy, or contractual validity.

## 16. The educational worked calculations

The step-by-step derivations the engine emits are teaching material. They explain what the implemented model does and why, using the same functions the engine's solvers use. They are not a substitute for the licensed standards, and the explanatory text necessarily simplifies: it describes this prototype's model, not the full state of cooling-tower practice.

## 17. The exported calculation sheet (issue #85)

The Report screen's export writes the sheet as a PDF built in this repository: the writer (issue #85, `cockpit/src/pdf.rs`) is a documented minimal PDF 1.7 subset with no third-party PDF library, and it uses the standard text fonts only (Helvetica / Helvetica-Bold / Courier, WinAnsi-encoded; the few glyphs WinAnsi has no code for are spelled in ASCII and the substitutions are pinned by the writer's own tests). Every chart on the sheet is vector paths and every number is selectable text.

**What the revision line carries.** When a saved project document (`.drafthouse`, the project files issue #74 landed) backs the export, the sheet's revision is that document's own digest: the SHA-256 of the canonical bytes the project writer produces from the exported state, computed at export time. Project writes are deterministic (write → read → write is byte-equal), so a project saved and exported unchanged carries **the saved file's own SHA-256** — the writer's test reads the file back off disk and pins the equality — and a project modified after its save carries the working document's digest, captioned "working document" rather than presented as the file's. For a report exported from a draft with **no** document behind it, the sheet carries a SHA-256 content hash of the exported draft state instead, labelled "project state hash" in the document itself: a hash of the draft input, never presented as a document revision. The `.drafthouse` file carries no self-digest field and the format is unchanged — the digest is computed from the saved bytes at export time; adding a self-digest field would be a shared-contract change and was deliberately not made.

The numbers on the sheet are checked by the writer's own inverse (the extractor test in `cockpit/src/screens/report_pdf.rs` decodes the content streams this repository wrote and compares them with the engine's fields). Like §15 and §16, that demonstrates software consistency only — it is not CTI compliance, MRL equivalence or contractual validity.

## 18. The comparison and its exported sheet (issue #89)

The Compare screen compares two or three **saved project files** (`.drafthouse`, issue #74's format). Each file is opened through the project reader and the engine is run on that file's own inputs when the file is handed to the comparison; the saved results a file may carry are never trusted or shown. A file whose catalog revision the session does not carry, or that holds custom record sets, is **refused with the reader's own reason** shown in the surface - the comparison never resolves records on the session's behalf. A variant the engine itself refuses shows as `refused` in its column and its status row, in the engine's own words. The **baseline column is a session choice** (tap a column's head); the project file format carries no comparison state, and adding a "baseline" field to it would be a shared-contract change that was deliberately not made. Two files whose engine inputs are identical compare as identical even when their bytes differ (the screen says `same inputs`); the digest shown in each column's head and carried on the exported sheet is the SHA-256 of the file's own bytes, so the files are still told apart.

Differences are lit at the **precision the surface shows**: a value that differs from the baseline by more than half of its last displayed digit is highlighted, and one that differs less is not - an intentional display rule, not a measurement. The best value per row is marked only where the engineering direction is defined (airflow and KaV/L carry no direction and are never marked best).

**The export is its own sheet.** The compare export writes a one-page A4 comparison sheet through the same PDF writer and the same delivery path as the calculation sheet (issue #85), but it is **not folded into the calculation sheet's page plan** - that plan is issue #85's surface and this lane deliberately left it untouched; a reader gets two documents, the worked calculation and the comparison. The sheet's numbers are the surface's numbers (same readers, same precision, same changed/best rules), its column digests are the full SHA-256 of each file's bytes, and the DEMO watermark applies exactly as on the calculation sheet.

**Loading.** The native binary takes the files with `--compare a.drafthouse,b.drafthouse[,c.drafthouse]`; on the web the page hands a file to the surface with a `compare:open:<name>:<text>` command. A comparison holds at most three variants; further files are refused with a named problem line. What the comparison does **not** do: it does not edit the draft, does not import or resolve custom records, and cannot open a file that the session's catalog does not cover.
