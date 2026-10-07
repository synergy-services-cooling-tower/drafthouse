# Synergy Drafthouse — the cooling-tower engine and its cockpit

**Cooling-tower engineering, every step shown.**

The cooling-tower engine is a dependency-free Rust crate (`rust/`): thermal rating, CTI-style
evaluation mechanics, fan/system balance, water balance and catalog-driven component selection. It is
built twice — the native `ct-engine` binary and a `wasm32-unknown-unknown` build behind a thin,
data-only binding — and the product UI (`cockpit/`, an illustrative engineering instrument drawn with
Bevy) runs that build in the browser over a synthetic catalog. The older bounded JavaScript
demonstration was retired in issue #60 (the maintainers' private decision record, kept out of this tree): the product UI is the served
surface, and nothing in the tree references the retired one (a gate holds that). It is
**ported and drift-guarded, not validated**: the parity harness diffs it
against a recorded regression baseline, and the JavaScript reference the port was checked against while
it existed was retired in issue #40 slice 4 (its last commit is in this repository's history).

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

## Show every step

The demonstration surface renders the engine's own **"Show the worked steps"** panel under the
result. Each step gives:

1. why the step exists, in plain language;
2. the symbolic formula;
3. the same formula with this run's numbers substituted in;
4. the answer and its unit;
5. a source reference where one applies.

The step list is the engine's own output (the `worked` field of a reply), so it cannot drift from what
the engine computed. Programmatically, every command's reply carries it:

```js
import { readFileSync } from 'node:fs';
import { createEngine } from './rust/wasm/binding.mjs';

const engine = createEngine(readFileSync('rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm'));
const run = engine.select({ catalog, requirements: { waterMassFlowKgS: 200 } });
for (const step of run.worked.steps) {
  console.log(step.label, '|', step.formula, '|', step.substitution, '=', step.value, step.unit);
}
```

## Run locally

Requirements: Node.js 20 or newer and the Rust toolchain `rust/rust-toolchain.toml` pins; no package
installation is required. The cockpit is a wasm build, so build it once first (wasm-pack and the
`wasm32-unknown-unknown` target are required — the build refuses without them and gates the payload's
gzip size at 8 MiB, `COCKPIT_GZIP_LIMIT_BYTES`):

```bash
cockpit/tools/build-web.sh release     # writes cockpit/pkg/{drafthouse_cockpit.js, drafthouse_cockpit_bg.wasm}
```

Then start the repository's own static server:

```bash
npm start
```

`npm start` (`scripts/serve.mjs`) serves the **cockpit at `/`** — its root is `cockpit/`, the directory
the release publishes — on **port 4173** (`PORT=<n>` overrides the port). `http://localhost:4173/` is the
cockpit document; `/pkg/**` and `/assets/**` are the plane's own paths.

**Public vs internal host.** The page reads one query parameter, `?host=`:

- `http://localhost:4173/` — **public** (the default): brand-free, no save/export, the comparable-metrics
  label always visible;
- `http://localhost:4173/?host=internal` — **internal**: the page injects the host's own branding
  (default name `Synergy Services`, mark `SS`, accent, notice line) and the INTERNAL chip.

Everything else on the cockpit is staged the same way (view, bay, duty, engine selection); the
parameters are listed in `cockpit/index.html`.

### Native (macOS, Windows, Linux)

The same crate also builds a **desktop binary** — the same `App`, no wasm, no wasm-pack (issue #71):

```bash
cd cockpit
cargo run -p drafthouse-cockpit --bin drafthouse
```

The window is titled `Synergy Drafthouse`, opens at 1440x900 (minimum 900x600), and the app exits when
it is closed. The staging parameters are CLI flags with the same names and defaults as the page's query
parameters, plus the host and the engine selection:

```bash
cargo run --bin drafthouse -- --host internal --engine real --view curves --duty waterMassFlowKgS=200
cargo run --bin drafthouse -- --help     # every flag, with its default
```

`--engine unavailable` stages the same missing asset `?engine=unavailable` does, and `--no-window`
builds the app without a window, runs a few updates and exits 0 — that is the headless smoke the
`cockpit-native` CI job runs. An unknown flag or an unknown value exits 2 with a message naming the
flag; nothing falls back silently.

The binary resolves `assets/` **relative to the executable**, never relative to the directory it is
launched from: `build.rs` copies `cockpit/assets/` beside every binary cargo builds
(`cockpit/target/<profile>/assets/`), and a shipped app is the binary with that folder next to it. (From
the repository root the same run is `cargo run --manifest-path cockpit/Cargo.toml --bin drafthouse`.)
The installers the release builds from this binary are below; macOS is Developer-ID signed and
notarized on a tag push, Windows and Linux v1 ship unsigned.

The engine's own tests run in both workspaces:

```bash
cd rust && cargo test --workspace        # the engine crate + the cockpit adapter
cd cockpit && cargo test --workspace     # the cockpit's three crates (contract, seams, UI)
```

## Install

Each release tag publishes four installers beside the engine bundle and the cockpit plane
(issue #72); a dry run of the release workflow publishes the same four as workflow artifacts. Take
them from the Release — `v<version>` on the releases page — together with their `.sha256` sidecars,
`native-<version>.manifest.json` and `native-<version>.provenance.json`.

| platform | artifact |
| --- | --- |
| macOS (Apple silicon **and** Intel) | `Synergy-Drafthouse-<version>-macos-universal.dmg` |
| Windows x64 | `Synergy-Drafthouse-<version>-windows-x64.msi` |
| Linux x64, portable | `Synergy-Drafthouse-<version>-linux-x64.AppImage` |
| Linux x64, Debian/Ubuntu | `synergy-drafthouse_<version>_amd64.deb` |

Verify the bytes before installing anything. Each artifact has a sidecar carrying its SHA-256
(`<file>.sha256`, the name beside the digest), the manifest lists all four with their sizes and
digests, and the whole set verifies in one run:

```bash
node scripts/native-verify.mjs --dir <the directory you downloaded into> --expect-mode tag
```

**macOS.** `Synergy-Drafthouse-<version>-macos-universal.dmg` is a universal binary (arm64 + x86_64),
signed with the project's Developer ID certificate, notarized by Apple and stapled — the release job
assesses the dmg with `spctl` before it publishes it, and Gatekeeper applies the same assessment when
a user opens it: drag the app to Applications. The record the run leaves (`native-<version>.manifest.json`
and the provenance) names the signing state, and because the ticket is stapled to the image, a machine
that is offline can still verify it.

**Windows.** `Synergy-Drafthouse-<version>-windows-x64.msi` is **unsigned for v1**, so Windows
SmartScreen will warn about it — "Windows protected your PC" / *More info* → *Run anyway* is the
expected flow, and the warning is the correct behaviour for a binary with no code-signing
certificate behind it. Check the `.sha256` first; that is the integrity claim this release makes.

**Linux.** The AppImage is portable: `chmod +x Synergy-Drafthouse-<version>-linux-x64.AppImage &&
./Synergy-Drafthouse-<version>-linux-x64.AppImage`. It is built on Ubuntu 22.04, and the runtime
floor recorded for it is **glibc ≥ 2.31** (issue #72); it needs a working GL/Vulkan stack (it renders
through wgpu). The `.deb` declares what it needs, so `apt` resolves the rest:

```bash
sudo apt install ./synergy-drafthouse_<version>_amd64.deb   # X11/Wayland client libraries + the Vulkan loader
```

Neither Linux package is signed for v1: the `.sha256` sidecar is the integrity claim, and the
AppImage is run directly on the user's machine.

## Validate the repository

Run the complete validation command:

```bash
npm run validate
```

This runs the surviving JavaScript suite (the deployment-shaped serving contract, the deploy set, the
retirement gate and the documentation drift gate — `npm test`), the deterministic smoke case through the
pinned engine piece (`npm run smoke`), a live static-server HTTP smoke test of the served surface
(`npm run server-smoke`), the deployment readiness check (`deploy-check`), the wasm drift leg
(`parity-wasm`), the pinned bundles (`bundle-check`) and the retirement of the JavaScript surface
(`retirement-check`).

The engine itself is checked by the crate's own suites and the drift harness:

```bash
cd rust && cargo test --workspace
node scripts/parity/run.mjs --reference frozen   # the engine against the recorded regression baseline
```

For catalog revision `illustrative-catalog-v0.1`, the smoke case currently:

- recovers a known 32.000 °C cold-water condition from its calculated Merkel demand;
- returns approximately 100.000% capability when design and test points are identical;
- finds 202 feasible combinations under the bundled default selection constraints;
- recommends an illustrative 64 m² counterflow cell first, under the engine's own default objective.

The recommendation is only a software demonstration because every product and cost record is synthetic.
Note that the engine reads no commercial field, so it does not rank by lifecycle cost (the retired
JavaScript reference did). See [Validation Results](VALIDATION_RESULTS.md) for the full record.

## Project structure

```text
synergy-drafthouse/
├── rust/                              The engine: crate, binary, wasm build, tests
│   ├── src/
│   │   ├── psychrometrics.rs          Moist-air properties
│   │   ├── water.rs                   Water properties and tower duty
│   │   ├── merkel.rs                  Demand, characteristic, CWT solver
│   │   ├── capability.rs              Characteristic capability and uncertainty
│   │   ├── performance_curve.rs       Curve interpolation/inversion
│   │   ├── airside.rs                 Fill, drift, and system resistance
│   │   ├── fan.rs                     Fan curves, power, operating point
│   │   ├── nozzle.rs                  Pressurized nozzle selection
│   │   ├── water_balance.rs           Evaporation, drift, blowdown, makeup
│   │   ├── crossflow.rs               Simplified 2-D crossflow solver
│   │   ├── natural_draft.rs           Coupled buoyancy/resistance solver
│   │   ├── selection.rs               Coupled component search and ranking
│   │   ├── numeric.rs                 Roots, interpolation, quadrature, RNG
│   │   └── bin/ct-engine.rs           The command-line surface of the engine
│   ├── wasm/binding.mjs               The data-only JavaScript binding
│   └── tests/                         The crate's suites (anchors, vectors, behaviour)
├── cockpit/                           The product UI (issue #58): the Bevy/wasm cockpit, its contract and seams
├── web/                               The `visuals` piece (geometry, panel, viewer) — pinned and published
├── bundles/                           The pinned pieces' manifests (engine + visuals)
├── src/data/                          The synthetic catalog and curve records (the source files)
├── validation/                        Frozen evidence: test-vectors.json, regression-baseline.json
├── tests/                             The surviving JavaScript suite
├── scripts/                           serve, smoke, parity harness, deploy/retirement/release tooling
├── docs/                              Methods, architecture, schema, validation, decisions
├── VALIDATION_RESULTS.md              Validation result summary (single source)
├── CONTRIBUTING.md                    Contribution posture
├── THIRD_PARTY_NOTICES.md             External IP and data boundary
└── LICENSE                            PolyForm Noncommercial 1.0.0 licence
```

## Deploy

The served surface is the product UI's **cockpit plane**: `cockpit/index.html`, its committed assets
(`cockpit/assets/**` — the record, the generated field lists and the fonts the app fetches at run
time) and its built module (`cockpit/pkg/**`, a build product: `cockpit/tools/build-web.sh release`).
A host copies the plane — the directory is the whole deployment — and serves it at a path.

The plane's **committed half** is recorded, generated, never hand-typed:

```bash
npm run deploy-check       # derives the committed set, verifies deploy-manifest.txt
npm run deploy-manifest    # records a deliberate change to that set
```

`deploy-manifest.txt` lists the plane's document and every file under `cockpit/assets/`, derived by
[`scripts/deploy-check.mjs`](scripts/deploy-check.mjs) — the directory is the declaration, because the
app fetches those files at run time and no static walk can see that. The built module is **not** in
that record: its bytes are a build product, so the plane artifact's own manifest covers them.

Two requirements on a host:

- the site's `script-src` needs `'wasm-unsafe-eval'` for the engine to start (approved by the owner
  on 20 September 2026, not yet shipped), and the `.wasm` must be served as `application/wasm`;
- serve from a path (`/tools/cooling-tower/`), not a subdomain.

The pinned `engine` and `visuals` pieces are a **separate, distributable** product (`bundle-<version>`
releases): a consumer deployment places their bytes at the served paths its own surface fetches.

The prototype's own page (`index.html`, `styles.css`, `src/app.js`, `src/core/**`) was deleted in
issue #40 slice 4 (the reference is in this repository's history), and the
bounded JavaScript demonstration that once served the pinned pieces from the tree was retired in
issue #60 (D22), together with the packager that vendored them. `web/README.md` describes what the
`visuals` piece is now.

## Regression vectors

The recorded vectors are frozen evidence: `validation/test-vectors.json` was generated by
`scripts/generate-vectors.mjs`, which was retired with the JavaScript reference, and is replayed by
`rust/tests/vectors.rs` and the parity harness. It is not regenerated here.

**These record what this software computes. They are not validated truth** — nothing has been
checked against CTI ToolKit, published benchmarks, or measured tower data. The prepared
`validation/cti-comparison-matrix.csv` exists to be filled in during that comparison; use the
`cti_convention` column, not the `bulk` one, or you will see a built-in ~1.1 % offset and mistake it
for an error.

## Using the engine

The engine's surfaces are the `ct-engine` binary and the wasm build behind the binding; the
JavaScript reference API this README used to document was retired with the reference
(`docs/API.md` keeps it as the port's record). From the command line:

```bash
cargo build --manifest-path rust/Cargo.toml
rust/target/debug/ct-engine merkel --hot 42 --cold 32 --wb 27 --db 33 --lg 1.5 --p 101325
rust/target/debug/ct-engine select --towers "$TOWERS" --fills "$FILLS" --drift-eliminators "$DRIFT" \
  --fans "$FANS" --nozzles "$NOZZLES" --water 200 --hot 42 --target-cold 32 --wb 27 --db 33 # see rust/README.md
```

From JavaScript (Node or browser), through the binding — the shape a consumer surface uses:

```js
import { readFileSync } from 'node:fs';
import { createEngine } from './rust/wasm/binding.mjs';

const engine = createEngine(readFileSync('rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm'));
const reply = engine.call(['merkel', '--hot', '42', '--cold', '32', '--wb', '27', '--db', '33', '--lg', '1.5', '--p', '101325']);
if (!reply.ok) throw new Error(reply.error.message);
console.log(reply.value.merkelNumber);

const selection = engine.select({ catalog, requirements: { waterMassFlowKgS: 200, hotWaterC: 42, targetColdWaterC: 32, wetBulbC: 27, dryBulbC: 33 } });
console.log(selection.metrics, selection.candidates[0]);
```

See [`rust/README.md`](rust/README.md) for the module-by-module port map and
[`docs/WASM_ENGINE.md`](docs/WASM_ENGINE.md) for the published wasm piece.

## Replacing synthetic component data

Do not change only product names. A production record should include:

- product ID, manufacturer, model, revision, geometry, and orientation;
- thermal curve/correlation and wet pressure-drop curve/correlation;
- tested air and water loading domains;
- material, temperature, chemistry, UV, and fire limits;
- drift efficiency and pressure drop versus face velocity;
- fan pressure, efficiency, power, density, speed, pitch, and permitted operating range;
- nozzle hydraulic curve, spray geometry, and turndown;
- source document, laboratory, date, uncertainty, license status, and interpolation/extrapolation policy;
- installation, sealing, bypass, support, and maintenance requirements.

See [Component Catalog Schema](docs/CATALOG_SCHEMA.md).

## Documentation

- [User Guide](docs/USER_GUIDE.md)
- [The Engine (crate, binary, wasm build)](rust/README.md)
- [Engineering Methods and Equations](docs/ENGINEERING_METHODS.md)
- [Architecture and Solver Flow](docs/ARCHITECTURE.md)
- [Worked Synthetic Example](docs/CALCULATION_EXAMPLE.md)
- [CTI Toolkit and MRL Mapping](docs/CTI_MRL_MAPPING.md)
- [JavaScript Reference API (retired)](docs/API.md)
- [Component Catalog Schema](docs/CATALOG_SCHEMA.md)
- [Traceability Matrix](docs/TRACEABILITY_MATRIX.md)
- [Validation and Uncertainty Plan](docs/VALIDATION.md)
- [Validation Results](VALIDATION_RESULTS.md)
- [Production Roadmap](docs/ROADMAP.md)
- [References and Provenance](docs/REFERENCES.md)
- [Limitations and Production Gaps](docs/LIMITATIONS.md)

## License and external material

This repository's own source is covered by the [PolyForm Noncommercial License 1.0.0](LICENSE): study, research, teaching, personal and other noncommercial use are all permitted, and the terms travel with any copy or changed version. **Commercial use needs a licence from Synergy Services Co., Ltd.** — contact <jirathip@synergyservices.co.th>. See [Third-Party Notices](THIRD_PARTY_NOTICES.md) for the boundary around CTI, MRL, ASHRAE, PsychroLib, IAPWS, AMCA, and manufacturer data. No paid standard, proprietary MRL source code, or manufacturer performance curve is bundled.
