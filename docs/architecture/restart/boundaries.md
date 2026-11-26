# Module boundaries

## 1. The tree

```
crates/
  engine/      the physics, selection and catalog schema. Standard library only. Numerics unchanged.
  geometry/    tower geometry from catalog records and a rated design. Layout arithmetic, no physics.
  api/         wire types, the three operations, the worked sheet, the C ABI. Builds engine.wasm.
  cli/         ct-engine: the text commands the recorded baseline replays. Native only, never shipped.
  gen/         just gen (TypeScript types, module lists, screen fixtures) and the site build's prerender step
  blocks/      the three-line bin that runs blocks check
tools/
  parity/      the baseline replay and the WASM-against-native comparison
  release/     the deterministic archive, the manifest, the verifier
  budgets/     the first-load, WASM size and Lighthouse measurements
packages/
  engine/      @drafthouse/engine: loader, worker, Session, generated types, the tower island's props type. No DOM.
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
    cli --> api --> geometry --> engine
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
  pe -. loads engine.wasm .-> api
```

| Module | May use | Must not |
|---|---|---|
| `crates/engine` | nothing | any dependency; `serde`; geometry or wire types |
| `crates/geometry` | `engine` record and result types | solve or correct a physical quantity |
| `crates/api` | `engine`, `geometry` | hold physics of its own; `unsafe` outside `abi.rs` |
| `crates/cli` | `engine`, `api` (for the worked sheet) | be linked into the WASM; gain a command |
| `packages/engine` | the generated types | the DOM, Blocks, Three.js |
| `packages/tower3d` | Three.js; `packages/engine` types | call the engine; import Blocks or the wizard |
| `packages/wizard` | `packages/engine`; Blocks core | import Three.js or `tower3d`; compute a number it shows |
| `packages/embed`, `apps/web` | `engine`, `wizard`; `tower3d` through one dynamic import | a static import of `tower3d` or Three.js |

The wizard names the tower by its tag, `drafthouse-tower`, inside a Blocks island. It never imports it. That is
what keeps Three.js out of the first load.

Each row is a `[[layer]]` or `[[ban]]` entry in `.drover/arch.toml` ([adr.md](adr.md), last section).

## 3. What crosses each boundary

| Boundary | What crosses | Checked where |
|---|---|---|
| Host or site to `packages/engine` | a `Catalog`, a `Duty`, a `Design` | the engine's schema, in `open`; nothing downstream checks again |
| `packages/engine` to the worker | the JSON call and reply of [shapes.md](shapes.md) section 1 | types generated from the Rust source |
| Wizard to tower | island props `{ geometry, view }`; events `pick` and `view`. The props type lives in `packages/engine`, so neither side imports the other. | the tower refuses an unknown `schema` |
| Release to synergy-apps | two archives and their digests | the pin, by sha256, in synergy-apps' fetch scripts |

Real catalog data never enters this repo. The mapping from synergy-apps' records to `Catalog` lives in synergy-apps.
