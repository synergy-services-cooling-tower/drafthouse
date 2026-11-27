# Synergy Drafthouse

**Cooling-tower engineering, every step shown.**

A cooling-tower engine in Rust: thermal rating, CTI-style evaluation mechanics, fan and system balance, water
balance and catalog-driven component selection, each answer with its worked steps. This repository is its only
source. It is being rebuilt around that engine as a fast public site with a live 3D tower and an embeddable
package; the plan is the restart ADR in [docs/architecture/restart/](docs/architecture/restart/README.md). The
engine is **ported and drift-guarded, not validated**: every pull request replays the 299 recorded steps of
`validation/regression-baseline.json` natively and through WebAssembly.

> **Validation status:** this software has been checked against ASHRAE property tables and for
> internal consistency. It has **not** been compared against CTI ToolKit, published cooling-tower
> benchmarks, or measured tower data. The air-side model and the component catalog have never met
> reality. See [`docs/VALIDATION.md`](docs/VALIDATION.md) §8 and
> [`VALIDATION_RESULTS.md`](VALIDATION_RESULTS.md) for what is and is not established.

> **Engineering status:** educational/prototyping software. It is not CTI Toolkit, does not reproduce paid CTI standards, is not MRL source code, and is not a CTI-certified rating program. The bundled tower, fill, drift-eliminator, fan, nozzle, material, and cost records are synthetic. Contractual acceptance, certified ratings, guarantees, procurement, and field modifications require licensed standards, controlled manufacturer data, and independent validation.

## Included calculation families

### 1. Thermal rating and design

- ASHRAE-style moist-air properties from dry bulb/wet bulb or dry bulb/relative humidity, including the water-vapour enhancement factor (real-mixture treatment) with the simplified equation set available for cross-checks.
- Range, approach, effectiveness, water-side heat rejection, water density, and water heat capacity.
- Counterflow Merkel demand using composite Simpson or four-point equal-weight Tchebycheff quadrature, with a selectable entering-air enthalpy convention (`bulk` or `cti-saturated-wetbulb`).
- Cold-water temperature solved from available `KaV/L`.
- Whole-tower characteristic `KaV/L = C(L/G)^m`.
- Saturated outlet-air estimate, evaporation, blowdown, makeup, and cycles of concentration.
- Simplified 2-D finite-volume crossflow solver, Richardson-extrapolated with a reported discretisation-error estimate and an optional three-grid convergence study.
- Simplified coupled natural-draft counterflow solver.

### 2. CTI-style evaluation mechanics

- Characteristic-curve capability by fitting a test-point tower characteristic and intersecting it with the design demand curve.
- Rectangular performance-curve interpolation and inverse water-flow lookup.
- Leaving-water-temperature deviation with an explicit sign convention.
- Seeded Monte Carlo propagation for user-specified characteristic-capability uncertainties.

These functions demonstrate publicly described mechanics only. They do **not** implement every current ATC-105 rule, correction, instrumentation requirement, validity criterion, or contractual procedure.

### 3. Tower and part selection

- Tower geometry and fill-depth enumeration.
- Independent fill thermal and wet-pressure correlations, plus spray-zone and rain-zone transfer.
- Air-side losses referenced to their own section velocities, with an explicit fan-stack discharge term for total-pressure fan curves.
- Fill operating-envelope and water-quality checks.
- Drift-eliminator drift/pressure interpolation versus face velocity.
- Fan curve interpolation, density/speed scaling, and fan/system intersection.
- Fan shaft power, drive/motor input, and standard motor selection.
- Nozzle count at a specified pressure drop.
- Constraints for CWT, drift, electrical input, footprint, loading domains, materials, and fan speed.
- Lifecycle ranking from synthetic CAPEX, energy, water, maintenance, and risk terms.
- Rejection summary showing why combinations failed.


## Intent

- Keep the engine's numbers: the recorded validation is the proof, and no restart step may move a result.
- Show every step: each number on screen is the engine's, with the formula and the substitution behind it.
- Load fast: a first paint from static HTML; the engine and the 3D tower load only when the user asks.
- One typed surface: every app action is one typed API command with JSON schemas, called the same way by the
  site, the embed's host and later MCP tools.
- Strict from the first commit: every gatekeeper pack on, no baseline file, the tree green after every issue.

## Non-goals

- Not a certified rating program, and no real vendor data: the catalog in `catalog/` is synthetic.
- No prices, costs or other money fields anywhere.
- No accounts, server-side storage or spend: hosting stays on the same static pins.
- None of the retired cockpit's features (PDF report, saved project files, native installers) come back here
  unless an issue asks for them.
- No model, texture or environment files: the tower is drawn from the engine's geometry.

## The tree today

| Path | What it is |
|---|---|
| `rust/` | the engine crate, the `ct-engine` binary, the WASM build and the cockpit adapter: the legacy set, moved into `crates/` issue by issue |
| `crates/` | the root Cargo workspace; `crates/blocks` runs `blocks check` at the Blocks pin |
| `scripts/parity/run.mjs` | the replay of the recorded baseline, native and WASM |
| `web/geometry.js` | the tower geometry in JavaScript, kept byte for byte until the engine owns it |
| `catalog/synthetic.json` | the one synthetic catalog |
| `validation/` | the recorded vectors and baseline, never edited |
| `docs/` | the engine's methods and validation, and the restart's architecture and design |
| `.gatekeeper/` | the pinned gate kit and this repo's own gates |

## Check it

```bash
just check                 # every gate CI runs, the same as a lane's done check
just gate local/parity     # one gate: here the 299 recorded steps
```

Requirements: Node.js 20 or newer and Rust; `rust/rust-toolchain.toml` and `rust-toolchain.toml` pin the two
toolchains. The WASM leg of `local/parity` needs the `wasm32-unknown-unknown` target and always runs in CI.

## Documentation

- [The engine (crate, binary, WASM build)](rust/README.md)
- [Engineering methods and equations](docs/ENGINEERING_METHODS.md)
- [Worked synthetic example](docs/CALCULATION_EXAMPLE.md)
- [CTI Toolkit and MRL mapping](docs/CTI_MRL_MAPPING.md)
- [Component catalog schema](docs/CATALOG_SCHEMA.md)
- [Traceability matrix](docs/TRACEABILITY_MATRIX.md)
- [Validation and uncertainty plan](docs/VALIDATION.md)
- [References and provenance](docs/REFERENCES.md)
- [Limitations and production gaps](docs/LIMITATIONS.md)
- [The restart: architecture](docs/architecture/restart/README.md) and [design](docs/design/restart/README.md)

## License and external material

This repository's own source is covered by the [PolyForm Noncommercial License 1.0.0](LICENSE): study, research, teaching, personal and other noncommercial use are all permitted, and the terms travel with any copy or changed version. **Commercial use needs a licence from Synergy Services Co., Ltd.** — contact <jirathip@synergyservices.co.th>. See [Third-Party Notices](THIRD_PARTY_NOTICES.md) for the boundary around CTI, MRL, ASHRAE, PsychroLib, IAPWS, AMCA, and manufacturer data. No paid standard, proprietary MRL source code, or manufacturer performance curve is bundled.
