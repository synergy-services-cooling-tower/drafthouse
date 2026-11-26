# Drafthouse restart: architecture

## Summary

Decision: Keep the Rust engine and its recorded validation as they are and delete the Bevy cockpit. Build a small engine-only WASM with three typed operations (open a catalog, size, rate), a public site on Blocks with a Three.js tower drawn from the engine's geometry and loaded only on demand, and one self-contained embeddable package per release. This repo becomes the only source.

Slices: foundation with every gate on; engine carried over; typed API and WASM; geometry; the engine client, 3D tower and wizard packages; web app; release; then synergy-apps (deploy, pins, real catalog mapping, admin embed).

Open questions: (1) who supplies real tower cells: the synergy-apps catalog has none; (2) admin does not frame the cockpit today, so the embed is new work: is that the intent; (3) sizing over several cells: now or after v1.0.0; (4) should any cockpit-only feature come back (PDF, saved projects, compare, native installers); (5) the hygiene check fails on the licensor's name in `LICENSE` and `NOTICE`: change the fleet's list or allow those files.

## Where each acceptance criterion is answered

| Issue AC | File |
|---|---|
| ADR: question, options, decision, consequences | [adr.md](adr.md) |
| Engine WASM API; fate of `rust/wasm` and `rust/cockpit-adapter` | [shapes.md](shapes.md) sections 1 and 2, [cutover.md](cutover.md) section 1 |
| Package boundaries; what the embed exports | [boundaries.md](boundaries.md), [usage.md](usage.md) section 3 |
| 3D geometry contract, scene mapping, live update | [shapes.md](shapes.md) section 3, [tower3d.md](tower3d.md) |
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

These facts shaped the decisions. Two of them differ from the issue text.

| Fact | Source |
|---|---|
| The cockpit payload is 7.58 MB gzip. | `cockpit/wasm-budget.json` |
| The engine WASM is 624,593 bytes raw at `opt-level = 3`, with the text command parser linked in. Its gzip size is not recorded, and the lane host has no `wasm32` target to measure it. | `bundles/engine.manifest.json` |
| One sizing of the synthetic catalog at the default duty takes 2.5 s natively: 202 feasible designs, about 12 ms each, and a 753 KB reply. | `ct-engine select` replayed from `validation/regression-baseline.json` |
| The engine has no geometry. `web/geometry.js` derives it in JavaScript from catalog records. | `web/geometry.js`, `docs/WASM_ENGINE.md` |
| The Blocks web shell is 77,643 bytes gzip and its CSS 19,058. Blocks draws a foreign widget as an "island": a custom element with props in and events out. It renders a screen to static HTML in Rust. | Blocks `staging` at 38f4bf8: `web/dist`, `docs/extending.md`, `crates/blocks/src/site` |
| **Differs from the issue.** synergy-apps admin does not frame the cockpit. There is no `DrafthouseShellFrame` component, only a test of that name for the admin header. Admin holds a catalog editor. The cockpit is its own public site. | synergy-apps `staging`: `apps/admin/src` |
| **Differs from the issue.** The real catalog there has four kinds: fill, drift eliminator, fan, nozzle. It has no tower cells and no water-quality factors, and its records are not in the engine's field shape. | synergy-apps `packages/data/src/drafthouse/drafthouseCatalog.ts` |
| The SWA deploy reads only a pin (`ref`, `repo`, `tag`, `sha256`) and the release assets. It accepts exactly one `drafthouse-cockpit-<version>.tar.gz` with a `.sha256` and a `.manifest.json`. | synergy-apps `scripts/lib/drafthousePin.mjs` |
| Admin is Vite and React 19 behind nginx. Its CSP has no `wasm-unsafe-eval`. It has no Blocks and no Three.js. | synergy-apps `apps/admin/package.json`, `infra/nginx/csp-spa.*.inc` |
| The snapshot is run by hand. `drafthouse-private` `staging` is at b7decd2, the commit v0.4.3 was published from, so nothing there is unpublished. It has 9 open issues and no open PR. | `gh api` on the private repo |
| `main` and `staging` here are not protected. | `drover protection` |
| `drover hygiene check` exits 1 on today's tree: 13 hits. Most are in the cockpit. Two files must keep theirs: `LICENSE` and `NOTICE` name the licensor. | `drover hygiene check drafthouse` |
