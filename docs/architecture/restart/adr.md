# ADR: the Drafthouse restart

## Summary

Decision: Keep the Rust engine and its recorded validation as they are and delete the Bevy cockpit. Build a small engine-only WASM with three typed operations (open a catalog, size, rate), a public site on Blocks with a Three.js tower drawn from the engine's geometry and loaded only on demand, and one self-contained embeddable package per release. A rating runs in its own worker, so the tower follows every edit while a sizing is still running. All 299 recorded steps are replayed natively and through a test-only WASM build on every PR. This repo becomes the only source.

Slices: foundation with every gate on; engine carried over; typed API and WASM; geometry; the engine client, 3D tower and wizard packages; web app; release; then synergy-apps (deploy, pins, real catalog mapping, admin embed). Every slice leaves the whole tree building and every gate green.

Open questions: (1) who supplies real tower cells and water-quality factors for the host's catalog; (2) the admin embed is written as new work, not a replacement: is that the intent; (3) sizing over several cells: now or after v1.0.0; (4) should any cockpit-only feature come back (PDF, saved projects, compare, native installers); (5) the hygiene check fails on `LICENSE` and `NOTICE`, which must keep their text: change the fleet's list or allow those files.

## The question

The engine is sound and validated, but its only face is a 7.58 MB Bevy app that is too heavy for the web, and
the repo is a snapshot written elsewhere. How do we rebuild the product around the engine so that it loads fast,
shows a live tower and every calc step, can be installed inside another app with that app's catalog, and is
written here under strict gates, without moving one recorded engine result?

## What is fixed

- The engine's numbers do not change. The 299 recorded steps and the Rust tests are the proof.
- The tree builds and every gate is green after every issue. A file is deleted only with its last user.
- Speed budgets, gated on every PR: first-load JavaScript at most 120 KB gzip, WASM at most 150 KB gzip,
  Lighthouse mobile at least 95; Three.js and the WASM load only on demand.
- Hosting stays on Azure SWA through the same pins. No new vendor, no spend.
- Real catalog data never enters this repo.
- The facts in [README.md](README.md), last section.

## Options and choices

| # | Question | Options | Chosen | Why |
|---|---|---|---|---|
| 1 | The WASM boundary | (a) keep the command-line text ABI and a binding that turns a catalog into `key:value` text; (b) `wasm-bindgen` with generated glue; (c) typed JSON operations over today's three C exports | c | (a) is untyped at the one boundary every caller crosses, and it links the 3,153-line command module into the WASM. (b) adds JavaScript glue and a build tool for three calls. (c) keeps the proven ABI, gives one typed contract and generated TypeScript. |
| 2 | Where JSON is parsed | (a) hand-written, as the output is today; (b) `serde` in the physics crate; (c) `serde` in a separate `api` crate | c | The physics crate stays dependency-free. The wire types become the contract instead of a mirror of internal structs. |
| 3 | The text commands | (a) delete them and re-record the baseline as typed calls; (b) keep them as a native-only replay tool | b | The baseline is frozen evidence recorded as command lines. Rewriting its inputs to fit new code would weaken it. The tool is not shipped and its command list is frozen. |
| 4 | Who owns geometry | (a) JavaScript, as `web/geometry.js` does; (b) the physics crate; (c) its own Rust crate, returned by `rate` | c | The issue asks for the engine's own geometry. A separate crate keeps the parity-gated numerics untouched. |
| 5 | What geometry says | (a) triangles; (b) boxes and cylinders, as today; (c) named parts with dimensions | c | The engine owns sizes and levels. The scene owns look, detail and motion, and can change them without a WASM build. |
| 6 | The site's UI | (a) Blocks screens built in Rust and WASM; (b) React with Blocks' React adapter; (c) Blocks' plain shell with TypeScript builders, prerendered | c | (a) puts a second WASM of about 131 KB gzip in the first load. (b) adds React and Blocks' 60 KB adapter to a budget the shell already fills to two thirds. (c) paints from static HTML. |
| 7 | How the tower sits in a screen | (a) a canvas beside Blocks, wired by hand; (b) a Blocks island | b | The island gives props in, events out and a fallback until its code loads. That is the lazy-load contract. |
| 8 | The embed | (a) three packages on a registry; (b) React components; (c) one self-contained tarball on the release, with mounts that take an element | c | It needs no registry, so no new vendor. A mount function needs neither Blocks nor Three.js from the host and works in any framework. |
| 9 | How admin installs it | (a) a URL in `package.json` with lockfile integrity; (b) fetched by pin, verified by digest, then installed | b | The issue asks for the same sha-pinned pins, and a pin is per environment while a `package.json` entry is per branch. This is a proposal to synergy-apps; S4 settles it there. |
| 10 | The site's release name | (a) keep publishing it as `drafthouse-cockpit-*`; (b) a new `site` piece, with the deploy accepting both during cut-over | b | (a) needs no change in synergy-apps but leaves a wrong name for good. (b) costs one small PR there and keeps rollback to v0.4.3. |
| 11 | Old code under the new gates | (a) all of it clean in the first commit; (b) a baseline file; (c) a small legacy set with its own gate, deleted issue by issue | c | (a) is one PR nobody can review. (b) breaks the empty-baseline rule. (c) keeps the baseline empty and ends at E3. The legacy set builds after every issue: [cutover.md](cutover.md) section 1 lists what each issue leaves and the run that proves the first step. |
| 12 | A sizing takes seconds | (a) run it on the main thread; (b) one worker, newest call wins; (c) two workers, one for `rate` and one for `size`, and the wizard cancels the running `size` on every edit; (d) change the selector now | c | (a) freezes the page. In (b) a rating waits behind a running sizing, so the tower lags by seconds. (d) risks recorded results inside a restart. (c) answers a rating in tens of milliseconds whatever the sizing does, for the price of a second WASM instance; R2 then makes the sizing itself fast, with its own baseline. |
| 13 | Package manager | (a) pnpm; (b) npm workspaces | b | Lane hosts have npm. pnpm would be a host install. |
| 14 | WASM parity once the shipped WASM drops the text commands | (a) compare only the typed operations, which covers 7 of the 299 recorded steps; (b) keep the text commands in the shipped WASM; (c) a second, test-only WASM build of the same crates that answers the text commands | c | (a) leaves the psychrometrics, Merkel, fan, crossflow and the other sections unproven in WASM. (b) costs the size budget. (c) replays all 299 steps through WASM, as today, and ships none of it. |
| 15 | Where the C ABI lives | (a) in `api`; (b) its own crate, `wasm`, that picks its handler by a build feature | b | The test-only build needs the ABI over the text commands, and `cli` already depends on `api`. One ABI crate keeps the `unsafe` code in one file and `api` safe Rust. |

## The decision

1. **Engine.** `rust/` becomes five crates: `engine` (physics, unchanged, standard library only), `geometry`,
   `api` (wire types, `open`, `size`, `rate`, the worked sheet), `cli` (the text commands, for replay) and `wasm`
   (the C ABI: `ct_alloc`, `ct_call`, `ct_free`), plus two small tool crates, `gen` and `blocks`.
   `engine.wasm` is `wasm` built for size over `api`. `replay.wasm` is the same crate built with the test-only
   feature `replay` over `cli`, under the same build profile; no release artifact holds it.
2. **Contract.** Three operations and their types as in [shapes.md](shapes.md). TypeScript types are generated
   from the Rust source by `just gen`.
3. **Geometry.** `rate` returns `TowerGeometry`: cells, each with basin, shell, inlets, fill, nozzles, drift
   eliminators, plenum, fan and drive, plus flow paths and every dimension with its formula.
4. **Packages.** `packages/engine` (two workers and the session), `packages/tower3d` (the scene),
   `packages/wizard` (state and Blocks screens), with the one-way rules of [boundaries.md](boundaries.md).
5. **Site.** `apps/web`: Blocks' plain shell, every route prerendered, the tower a lazy island, the synthetic
   catalog only.
6. **Embed.** `packages/embed` bundles the three packages, Blocks' shell and Three.js into one tarball with no
   dependencies. It exports `mountWizard`, `mountTower`, `loadEngine` and the types.
7. **Release.** A `v*` tag publishes `drafthouse-site-<version>.tar.gz` and `drafthouse-embed-<version>.tgz`
   with digests. The first is `v1.0.0`. synergy-apps pins both by sha256.
8. **Cut-over.** The steps of [cutover.md](cutover.md) section 3. The cockpit keeps serving from the existing
   releases until each pin moves.
9. **Strictness.** [cutover.md](cutover.md) section 4, from F0.
10. **Live update.** `rate` and `size` run in separate workers. An edit cancels the running `size` and rates the
    current design at once; the ranking follows 250 ms after the last edit ([tower3d.md](tower3d.md) section 2).
11. **Parity.** Three legs on every PR: the 299 recorded steps natively, the same 299 through `replay.wasm`
    within 1e-12 of native, and the typed operations of `engine.wasm` against native
    ([cutover.md](cutover.md) section 2).

## Consequences

**Gained**
- The first load drops from 7.58 MB to at most 120 KB of script and no WASM.
- One typed contract serves the site, the embed and the tests. A refusal is a value with a kind.
- A catalog is checked once, at `open`. Nothing downstream can run on an unchecked catalog.
- A visual change needs no engine build. An engine change cannot alter the look by accident.
- The validated numerics keep their results, and the same recorded evidence guards them.

**Costs**
- The cockpit's features end: PDF report, saved project files, variant compare, notes, the native installers.
  None is rebuilt here.
- E1 is large: the carried engine must meet pedantic clippy, the size and complexity limits and the strict
  comment policy with no baseline. Its comments, which hold provenance, move to `docs/engine/`.
- The physics crate stays dependency-free, but the repo no longer is. See the list below.
- Two WASM instances run in the page once a sizing is asked for, and the catalog is parsed in both. The file
  is fetched and compiled once.
- CI builds a second WASM, `replay.wasm`, on every PR. It is tested and thrown away.
- Two engine surfaces exist: the three operations (shipped) and the frozen text commands (replay only).
- Until `v1.0.0` is tagged, no release can be cut from this repo. Rollback is the existing v0.4.x assets.
- The WASM, first-load and Lighthouse gates run in CI only; a lane host has neither `wasm32` nor a browser.

**Risks, and what limits each**
- **The WASM may not fit in 150 KB.** It is 624,593 bytes raw today and its gzip size is unknown. E2 measures
  in its first commit, and asks the owner rather than raise the limit.
- **A sizing takes 2.5 s natively.** The tower and the numbers follow every edit through the rate worker; only
  the ranking waits. R2 owns the fix, with a baseline, and starts as soon as P1 is merged.
- **The size build may move a float.** Today's WASM is built for speed and its worst spread from native is
  1.28e-13 over the 299 steps. E2 switches to the size profile and replays all 299 again; a step outside 1e-12
  stops E2 for the owner.
- **Blocks' shell is 77.6 KB of the 120 KB.** A Blocks bump can fail the gate. The pin moves on purpose.
- **Prerender then take-over.** Blocks documents take-over of a static page for server streams, not for a
  client-built source. W1 proves it or files a Blocks issue. Until then `mount` redraws the same markup.
- **The host's catalog must hold every list the engine needs.** S3 cannot finish before open question 1.
- **Lighthouse varies between runs.** The gate takes the median of three.

**Dependencies this ADR approves** (each gets its reason line in `.gatekeeper/deps.toml`)

| Where | Dependency | Why |
|---|---|---|
| `crates/api`, `crates/geometry` | `serde`, `serde_json` | the JSON boundary |
| tests of `crates/engine` and `crates/cli` | `serde_json`, as a dev-dependency | they read the recorded vectors and the commands' JSON replies, as today |
| `crates/gen` | `ts-rs`; `blocks` at the Blocks pin | TypeScript types from the Rust source; prerender |
| `crates/blocks` | `blocks-cli` at the Blocks pin | `blocks check` |
| `packages/*`, `apps/web` | `@jirathip-dev/blocks` at the same pin; `three` | the UI; the tower |
| build and test only | `vite`, `typescript`, `vitest`, `playwright`, `lighthouse` | build, tests, the speed gates |

`crates/engine` has none outside its tests. Nothing is fetched at run time from a third party. Hosting, CI and releases stay on
what is used today.

## Open questions for the owner

1. **Real tower cells.** The engine's `Catalog` needs tower cells and water-quality factors beside fills,
   drift eliminators, fans and nozzles. What I read of the host's catalog is in the lane report, not in this
   public repo. Recommended: the host's catalog supplies every list from approved records (S3). The alternative
   is real components on the synthetic cells, which sizes towers nobody sells.
2. **The admin embed is new work.** The issue says admin frames the cockpit today. S4 and S5 are written as an
   addition to admin, a sizing section that mounts the embed; they replace nothing there. The lane report says
   why. Is that the intent?
3. **Cells.** The issue lists "cells". The engine sizes one cell per tower today, and this ADR keeps that. The
   geometry already carries `cells` as a list. Should sizing over several cells be part of the restart, or its
   own issue after `v1.0.0` (recommended)?
4. **Cockpit features.** PDF report, saved projects, variant compare, notes and the native installers end with
   the cockpit. Should any of them come back as an issue after `v1.0.0`?
5. **Hygiene.** `drover hygiene check` fails on today's tree. The hits in the cockpit go in F0, but `LICENSE`
   and `NOTICE` must keep their text, and the check stops on it. With every gate on, F0 cannot be green until
   either the fleet's list changes for this repo (recommended: the repo is public and the licence requires the
   notice) or the gate allows those two files. Both are the owner's to change.

## The `.drover/arch.toml` lines

This is the file as it stands after R1. F0 creates it. Every other entry is added by the issue named beside or
above it, in the PR that creates the files it names.

```toml
# F0
generate = "just gen"
generated = [
  "packages/engine/src/gen/**",       # E2
  "crates/api/src/worked/mod.rs",     # E2
  "crates/geometry/src/parts/mod.rs", # E3
  "packages/wizard/screens/**",       # P3
]
frozen = ["crates/cli/src/commands.rs"] # E1

[comments]
policy = "strict"
except = ["rust/**", "web/geometry.js", "scripts/parity/**"] # the legacy set: E1, E2 and E3 each delete their entry with its files

[[ban]]
pattern = "TODO"
paths = ["crates/**", "packages/**", "apps/**", "tools/**"]
reason = "no TODO comment, with or without an issue number: unfinished work is an issue"

[[collect]]
dir = ".gatekeeper/gates"
glob = "*.toml"

[[ban]]
pattern = "unsafe "
paths = ["crates/*/src/**"]
except = ["crates/wasm/src/lib.rs"]
reason = "the ABI is the only unsafe code"

[[ban]]
pattern = "serde"
paths = ["crates/engine/src/**"]
reason = "the physics has no dependency; its tests may read JSON with serde_json"

[[ban]]
pattern = "from \"three"
paths = ["packages/**", "apps/**"]
except = ["packages/tower3d/**"]
reason = "Three.js lives in the tower chunk only"

[[ban]]
pattern = "from \"@drafthouse/tower3d"
paths = ["packages/**", "apps/**"]
except = ["packages/tower3d/**"]
reason = "the tower is imported on demand: import(), never a static import"

[[ban]]
pattern = "Math."
paths = ["packages/wizard/src/**"]
reason = "the wizard shows the engine's numbers and computes none"

# E1
[[layer]]
name = "engine"
paths = ["crates/engine/src/**"]
mods = ["drafthouse_engine"]
may_use = []

[[layer]]
name = "cli"
paths = ["crates/cli/src/**"]
mods = ["drafthouse_cli"]
may_use = ["engine", "api"]

[[layer]]
name = "wasm"
paths = ["crates/wasm/src/**"]
may_use = ["cli", "api"] # `api` from E2

# E2
[[layer]]
name = "api"
paths = ["crates/api/src/**"]
mods = ["drafthouse_api"]
may_use = ["engine", "geometry"]

[[layer]]
name = "engine-client"
paths = ["packages/engine/src/**"]
mods = ["@drafthouse/engine"]
may_use = []

[[collect]]
dir = "crates/api/src/ops"
glob = "*.rs"

[[collect]]
dir = "crates/api/src/worked"
glob = "*.rs"

# E3
[[layer]]
name = "geometry"
paths = ["crates/geometry/src/**"]
mods = ["drafthouse_geometry"]
may_use = ["engine"]

[[collect]]
dir = "crates/geometry/src/parts"
glob = "*.rs"

# P2
[[layer]]
name = "tower3d"
paths = ["packages/tower3d/src/**"]
mods = ["@drafthouse/tower3d"]
may_use = ["engine-client"]

[[collect]]
dir = "packages/tower3d/src/parts"
glob = "*.ts"

# P3
[[layer]]
name = "wizard"
paths = ["packages/wizard/src/**"]
mods = ["@drafthouse/wizard"]
may_use = ["engine-client"]

[[collect]]
dir = "packages/wizard/src/steps"
glob = "*.ts"

# W1
[[layer]]
name = "web"
paths = ["apps/web/src/**"]
may_use = ["engine-client", "wizard", "tower3d"]

# R1
[[layer]]
name = "embed"
paths = ["packages/embed/src/**"]
may_use = ["engine-client", "wizard", "tower3d"]
```

How each hot file is avoided:

| Would be a hot file | Instead | Old list |
|---|---|---|
| the worked sheet, one 300-line function in `rust/src/cli.rs` | `crates/api/src/worked/`, one file per section; `mod.rs` written by `just gen` | deleted with `cli.rs` in E2 |
| the geometry derivation, one 560-line function in `web/geometry.js` | `crates/geometry/src/parts/` and `packages/tower3d/src/parts/`, one file per part | deleted in E3 |
| the command match in `rust/src/cli.rs` | new behaviour is a file in `crates/api/src/ops/` | `crates/cli/src/commands.rs`, frozen |
| the re-export list in `rust/src/lib.rs` | none: callers name the module path | moved as it is in E1, deleted in E2 with its last user, the adapter |
| a wizard step list | `packages/wizard/src/steps/`, read by a glob | never created |
| hand-written TypeScript wire types | `packages/engine/src/gen/`, written by `just gen` | none: `rust/wasm/binding.mjs` becomes the replay's raw caller in `tools/parity/` |
| one CI workflow with a job per check | `.gatekeeper/gates/`, one file per gate | `validate.yml`, deleted in F0 |
| one architecture or API doc | `docs/engine/`, `docs/web/`, `docs/release/` | deleted in F0 |
| the workspace member lists | `crates/*`, `packages/*`, `apps/*` globs | never created |
