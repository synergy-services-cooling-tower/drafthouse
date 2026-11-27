# Drafthouse restart: architecture

## Summary

Decision: Keep the Rust engine and its recorded validation as they are and delete the Bevy cockpit. Build a small engine-only WASM whose only entry is a set of typed `crates/api` commands, one per cockpit action, each with JSON input and output schemas, so the UI holds no rating or rule logic and later MCP tools call the same commands; a public site on Blocks with a Three.js tower drawn from the engine's geometry and loaded only on demand, and one self-contained embeddable package per release. A rating runs in its own worker, so the tower follows every edit while a sizing is still running. All 299 recorded steps are replayed natively and through a test-only WASM build on every PR. This repo becomes the only source.

Slices: foundation with every gate on; engine carried over; typed API and WASM; geometry; the engine client, 3D tower and wizard packages; web app; release; then synergy-apps (deploy, pins, real catalog mapping, admin embed). Every slice leaves the whole tree building and every gate green.

Open questions: (1) who supplies real tower cells and water-quality factors for the host's catalog; (2) the admin embed is written as new work, not a replacement: is that the intent; (3) sizing over several cells: now or after v1.0.0; (4) should any cockpit-only feature come back (PDF, saved projects, compare, native installers); (5) the hygiene check fails on `LICENSE` and `NOTICE`, which must keep their text: change the fleet's list or allow those files.

## Revision 2: the owner's notes and where each is answered

| Note | Answer | Where |
|---|---|---|
| 1. Green between every issue | F0 moves the one cockpit file the adapter compiles into the adapter. E1 re-points the adapter at the moved engine in the same PR. A table lists what each issue leaves and what proves it. F0's step was run. | [cutover.md](cutover.md) section 1, [issues.md](issues.md) |
| 2. WASM parity over all 299 steps | A test-only build, `replay.wasm`, answers the text commands under the shipped build profile. All 299 steps are replayed through it on every PR, from F0 on. | [cutover.md](cutover.md) section 2, [boundaries.md](boundaries.md) |
| 3. Rating must not wait behind sizing | Both: `rate` has its own worker, and the wizard cancels the running `size` on every edit. | [loading.md](loading.md) section 3, [tower3d.md](tower3d.md) section 2 |
| 4. No TODO comments; serde in tests | Only `SAFETY:` and one doc line; a ban on `TODO`. The serde ban covers the engine's `src/` only. | [cutover.md](cutover.md) section 4, [adr.md](adr.md) last section |
| 5. No internals of the private apps repo | Removed from the tree: its paths, its security policy file, its commit ids and what its code holds. The first commit of this branch still carries them: see the lane report. | every file |
| Agent forward: every cockpit action is one `crates/api` command | Thirteen typed commands with JSON schemas generated from one folder; the UI and a later MCP server send the same calls. `.drover/rules.md` carries the line from this PR on. | [shapes.md](shapes.md) section 2, [adr.md](adr.md) option 16, [cutover.md](cutover.md) section 4 |
| Scope: no dark theme | Superseded by the design amendments: one cockpit screen, dark by default with a light toggle, drawn at 1440 and 390. | [screens.md](screens.md) |

## Where each acceptance criterion is answered

| Issue AC | File |
|---|---|
| ADR: question, options, decision, consequences | [adr.md](adr.md) |
| Engine WASM API; fate of `rust/wasm` and `rust/cockpit-adapter` | [shapes.md](shapes.md) sections 1 to 3, [cutover.md](cutover.md) section 1 |
| Package boundaries; what the embed exports | [boundaries.md](boundaries.md), [usage.md](usage.md) section 3 |
| 3D geometry contract, scene mapping, live update | [shapes.md](shapes.md) section 4, [tower3d.md](tower3d.md) |
| Lazy-load plan and the three CI gates | [loading.md](loading.md) |
| Pipeline retirement and the SWA cut-over | [cutover.md](cutover.md) section 3 |
| Screens and actions for the design lane | [screens.md](screens.md) |
| What stays, moves or is deleted; tests carried over | [cutover.md](cutover.md) sections 1 and 2 |
| Strictness from the first new commit | [cutover.md](cutover.md) section 4, [adr.md](adr.md) last section |
| Phased issue list | [issues.md](issues.md) |

For the owner: this page and [adr.md](adr.md) are the decision. The other files are what the design and impl
lanes build from; [usage.md](usage.md) is the place to start there, because it shows how the calling code reads.

## Words used here

- **Cell**: one tower record of the catalog. The engine sizes one cell per tower today.
- **Duty**: what the tower must do: water flow, hot and cold water temperatures, wet and dry bulb, pressure.
- **Design**: one cell with its fill layers, drift eliminator, fan and fan speed.
- **Sizing**: every feasible design for a duty, ranked by one objective.
- **Rating**: what one design does at one duty: the numbers, the calc steps and the geometry.
- **First load**: every script a cold visit fetches before the user does anything.

## What was read and measured (2026-11-26, at 766fb98)

These facts shaped the decisions.

| Fact | Source |
|---|---|
| The cockpit payload is 7.58 MB gzip. | `cockpit/wasm-budget.json` |
| The engine WASM is 624,593 bytes raw at `opt-level = 3`, with the text command parser linked in. Its gzip size is not recorded, and the lane host has no `wasm32` target to measure it. | `bundles/engine.manifest.json` |
| One sizing of the synthetic catalog at the default duty takes 2.5 s natively: 202 feasible designs, about 12 ms each, and a 753 KB reply. | `ct-engine select` replayed from `validation/regression-baseline.json` |
| The recorded baseline is 299 steps and 11,524 quantities. Only 7 steps are `select` or `validate-catalog`; the other 292 are commands the typed operations do not have (58 `psy`, 107 `merkel`, 25 `inverse` and so on). Today's WASM parity run replays all 299, and its worst spread from native is 1.28e-13. | `validation/regression-baseline.json`, `scripts/parity/run.mjs` |
| `rust/cockpit-adapter` compiles one file of the cockpit by path, `cockpit/src/numfmt.rs`, and uses the engine only through the re-export list of `rust/src/lib.rs`. The Rust tests read nothing outside `rust/` but `validation/`. | `rust/cockpit-adapter/src/lib.rs`, `rust/tests/` |
| The adapter fails `cargo clippy -D warnings` today (3 findings). CI never ran clippy on it. | `cargo clippy --workspace` in `rust/` |
| The engine's tests use `serde_json` as a dev-dependency (6 test files). | `rust/Cargo.toml` |
| The engine has no geometry. `web/geometry.js` derives it in JavaScript from catalog records, and imports nothing. | `web/geometry.js`, `docs/WASM_ENGINE.md` |
| The Blocks web shell is 77,643 bytes gzip and its CSS 19,058. Blocks draws a foreign widget as an "island": a custom element with props in and events out. It renders a screen to static HTML in Rust. | Blocks `staging`, read 2026-11-26 |
| The deploy takes this repo's release assets by pin: today one `drafthouse-cockpit-<version>.tar.gz` with a `.sha256` and a `.manifest.json`. | `.github/workflows/release.yml`, the issue |
| What the issue says of the host app (how admin shows the cockpit, what the real catalog holds) differs from what was read there. The details are in the lane report and behind open questions 1 and 2. | the lane report |
| The snapshot is run by hand. v0.4.3 was published from the head of `drafthouse-private`'s `staging`, so nothing there is unpublished. | the private repo |
| `main` and `staging` here are not protected. | `drover protection` |
| `drover hygiene check` exits 1 on today's tree. Most hits are in the cockpit. Two files must keep theirs: `LICENSE` and `NOTICE`. | `drover hygiene check drafthouse` |
