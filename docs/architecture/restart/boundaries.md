# Module boundaries

## 1. The tree

```
crates/
  engine/      the physics, selection and catalog schema. Standard library only. Numerics unchanged.
  geometry/    tower geometry from catalog records and a rated design. Layout arithmetic, no physics.
  api/         wire types, the commands (one per cockpit action, with JSON schemas), the worked sheet. Safe Rust.
  cli/         the text commands the recorded baseline replays, and the ct-engine bin. Never shipped.
  wasm/        the C ABI, the only unsafe code. Builds engine.wasm over api, and with the test-only
               feature `replay` builds replay.wasm over cli.
  gen/         just gen (TypeScript types, module lists, screen fixtures) and the site build's prerender step
  blocks/      the three-line bin that runs blocks check
tools/
  parity/      the baseline replay (native and replay.wasm), the typed-operation comparison, the raw ABI caller
  release/     the deterministic archive, the manifest, the verifier
  budgets/     the first-load, WASM size and Lighthouse measurements
packages/
  engine/      @drafthouse/engine: loader, the rate and size workers, Session, generated types, the tower
               island's props type. No DOM.
  tower3d/     @drafthouse/tower3d: the Three.js scene and the <drafthouse-tower> element
  wizard/      @drafthouse/wizard: wizard state and its Blocks screens
  embed/       @drafthouse/embed: the released bundle of the three above
apps/
  web/         the public site: landing page, routes, synthetic catalog, prerender
catalog/       synthetic.json, the one synthetic catalog
validation/    the recorded vectors and baseline, byte-identical to today
docs/          engine/, web/, release/, architecture/: one folder per area
```

Cargo members are `crates/*`; npm workspaces are `packages/*` and `apps/*`. A new crate or package
edits no shared list.

## 2. Who may use whom

```mermaid
flowchart LR
  subgraph Rust
    wasm --> api --> geometry --> engine
    wasm -. feature replay, test only .-> cli
    cli --> api
    api --> engine
    cli --> engine
    gen[crates/gen] --> api
  end
  subgraph TypeScript
    web[apps/web] --> wizard --> pe[packages/engine]
    web --> pe
    web -. import on demand .-> tower3d
    embed --> wizard
    embed --> pe
    embed -. import on demand .-> tower3d
    tower3d -. types only .-> pe
  end
  pe -. loads engine.wasm .-> wasm
```

| Module | May use | Must not |
|---|---|---|
| `crates/engine` | nothing in `src/`; its tests read JSON with `serde_json` | any dependency or `serde` in `src/`; geometry or wire types |
| `crates/geometry` | `engine` record and result types | solve or correct a physical quantity |
| `crates/api` | `engine`, `geometry` | hold physics of its own; any `unsafe`; an action that is not a command |
| `crates/cli` | `engine`, `api` (for the worked sheet) | be linked into `engine.wasm`; gain a command |
| `crates/wasm` | `api`; `cli` only under the feature `replay` | hold anything but the ABI; put `replay` in a released file |
| `packages/engine` | the generated types | the DOM, Blocks, Three.js; run `rate` and `size` in one worker |
| `packages/tower3d` | Three.js; `packages/engine` types | call the engine; import Blocks or the wizard |
| `packages/wizard` | `packages/engine`; Blocks core | import Three.js or `tower3d`; compute a number it shows; hold a rule the engine owns (snapping, pins, cell split) |
| `packages/embed`, `apps/web` | `engine`, `wizard`; `tower3d` through one dynamic import | a static import of `tower3d` or Three.js |

The wizard names the tower by its tag, `drafthouse-tower`, inside a Blocks island. It never imports it. That is
what keeps Three.js out of the first load.

Each row is a `[[layer]]` or `[[ban]]` entry in `.drover/arch.toml` ([adr.md](adr.md), last section). Two rows
have a test instead: the release verifier refuses an archive that holds `replay.wasm` or a WASM that answers a
text command, and a `packages/engine` test rates while a sizing runs.

## 3. What crosses each boundary

| Boundary | What crosses | Checked where |
|---|---|---|
| Host or site to `packages/engine` | a `Catalog`, a `Duty`, a `Design` | the engine's schema, in `open`; nothing downstream checks again |
| `packages/engine` to its two workers | the JSON call and reply of [shapes.md](shapes.md) sections 1 and 2: one command each | types generated from the Rust source |
| Wizard to tower | island props `{ geometry, view }`; events `pick` and `view`. The props type lives in `packages/engine`, so neither side imports the other. | the tower refuses an unknown `schema` |
| Release to synergy-apps | two archives and their digests | the host's pin, by sha256 |

Real catalog data never enters this repo. The mapping from synergy-apps' records to `Catalog` lives in synergy-apps.
