# Callers' usage

How the code that uses each piece will read. Names are final; bodies are sketches. Blocks builder calls are
illustrative and follow Blocks' own docs.

## 1. The engine client (`packages/engine`)

A catalog is opened once. Only an opened catalog can be sized or rated, so an unchecked catalog cannot reach the
physics. A refusal is a value, never an exception.

```ts
import { loadEngine, type Catalog, type Duty } from "@drafthouse/engine";

const engine = await loadEngine();                    // starts the worker, fetches engine.wasm
const opened = await engine.open(catalog);            // checks every record against the engine's schema
if (!opened.ok) return showViolations(opened.error.violations);
const session = opened.value;

const sizing = await session.size(duty, { objective: "least-over-capacity", limit: 30 });
if (!sizing.ok) return showRefusal(sizing.error);
const best = sizing.value.candidates[0];

const rating = await session.rate(duty, best.design); // numbers, calc steps and geometry of one design
```

A newer `size` call replaces a running one. The older call resolves `{ ok: false, error: { kind: "superseded" } }`.

## 2. The public site (`apps/web`)

The landing page is static HTML. The tower is a Blocks island with a poster inside it. One file names the tower
chunk, and it is imported only when the user asks for it:

```ts
// apps/web/src/tower-island.ts
import { whenWanted } from "./when-wanted";           // first input near the tower, or the tower opened

whenWanted("drafthouse-tower", async () => {
  const { defineTower } = await import("@drafthouse/tower3d");   // Three.js is only in this chunk
  defineTower();                                      // the island upgrades and the poster fades out
});
```

The landing page needs no WASM to draw its tower: the site build runs the engine once on the default design
and writes the geometry into the page as the island's props.

The wizard is a Blocks local source: state in, screen out, action in, state out.

```ts
// apps/web/src/routes/size.ts
import { createWizard } from "@drafthouse/wizard";

const wizard = createWizard({ engine: () => loadEngine(), catalog: syntheticCatalog });
mount({ root, layout, sources: { wizard: wizard.source } });
```

One wizard step is one file. A glob collects the folder, so two PRs adding steps never meet in a list.

```ts
// packages/wizard/src/steps/duty.ts
export const step: Step = {
  id: "duty",
  order: 10,
  screen: (state) => page("Duty", [
    slider("waterFlowM3Hr", state.duty.waterFlowM3Hr),
    slider("hotWaterC", state.duty.hotWaterC),
    island("drafthouse-tower", { geometry: state.rating?.geometry ?? null }),
  ]),
  act: (state, action) => withDuty(state, action),
};
```

A duty edit updates the tower in two beats: the current design is rated at once, then the ranking follows.

```ts
// packages/wizard/src/run.ts
async function dutyChanged(duty: Duty) {
  const quick = await session.rate(duty, state.design);          // tens of ms: tower and numbers follow the input
  if (quick.ok) show(quick.value);
  const sizing = await session.size(duty, { objective });        // seconds today, in the worker, newest call wins
  if (!sizing.ok) return sizing.error.kind === "superseded" ? undefined : refuse(sizing.error);
  const design = keepOrFirst(state.design, sizing.value.candidates);
  show(await session.rate(duty, design));
}
```

## 3. A host app (synergy-apps admin)

The embed package has no React and no runtime dependency. It mounts on an element the host gives it. The host
owns the wrapper and passes its own catalog, already mapped to the engine's record fields.

```tsx
import { mountWizard, type Catalog } from "@drafthouse/embed";
import "@drafthouse/embed/embed.css";

export function DrafthouseSizing({ catalog }: { catalog: Catalog }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const wizard = mountWizard(ref.current!, { catalog, onRefused: report });
    return () => wizard.destroy();
  }, [catalog]);
  return <div ref={ref} />;
}
```

The tower alone, for a host that has a rating already:

```ts
import { mountTower } from "@drafthouse/embed";

const tower = mountTower(element, { geometry: rating.geometry });
tower.show(nextRating.geometry);                      // reshapes in place
tower.destroy();
```

## 4. The 3D tower (`packages/tower3d`)

```ts
const view = createTowerView(canvasHost, { quality: "auto" });
view.show(geometry);                                  // first call builds, later calls morph over 300 ms
view.set({ cutaway: 0.5, air: true, water: true, labels: false });
view.dispose();
```

One part kind is one file with one shape, collected by a glob:

```ts
// packages/tower3d/src/parts/fan.ts
export const part: Part<Cell["fan"]> = {
  kind: "fan",
  build: (fan) => fanGroup(fan),                      // hub and blades from diameter, hub diameter, blade count
  update: (group, fan) => resizeFan(group, fan),      // same objects, new transforms
  tick: (group, fan, seconds) => spin(group, fan.rpm, seconds),
};
```

## 5. Inside the Rust API (`crates/api`)

An operation takes the opened catalog and a typed request and returns a typed reply or a refusal.

```rust
pub fn rate(catalog: &OpenCatalog, request: &RateRequest) -> Result<Rating, Refusal> {
    let run = one_design_run(catalog, &request.duty, &request.design)?;
    let candidate = run.candidates.first().ok_or_else(|| Refusal::infeasible(&run))?;
    Ok(Rating {
        headline: headline(&run, candidate),
        steps: worked::sheet(&run, candidate),
        geometry: drafthouse_geometry::tower(catalog.records_of(candidate), candidate),
        ..rating_tables(&run, candidate)
    })
}
```

## 6. The recorded evidence

The replay of the recorded baseline reads the same as today, from the new paths:

```sh
node tools/parity/run.mjs                 # 299 recorded steps against the native ct-engine
node tools/parity/run.mjs --engine wasm   # every open, size and rate case: WASM against native, to 1e-12
```
