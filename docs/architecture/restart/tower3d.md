# The 3D tower

The engine emits `TowerGeometry` ([shapes.md](shapes.md) section 3). `packages/tower3d` turns it into a Three.js
scene. There is no model file, no texture file and no environment file: every mesh is built from numbers.

## 1. How the scene maps the geometry

One geometry field, one file in `packages/tower3d/src/parts/`, one scene object.

| Geometry | Scene object | Technique | Motion |
|---|---|---|---|
| `cell.basin` | floor slab, water surface | box; a plane with a small ripple shader | ripple |
| `cell.shell` | four casing walls | boxes with the cutaway clipping plane, open at the inlets | none |
| `cell.inlets` | louver slats | one instanced mesh per face | none |
| `cell.fill` | corrugated sheets per block and layer | one instanced mesh; colour by `recordId` | none |
| `cell.nozzles` | header pipes, nozzle cones | cylinders; one instanced mesh of `count` cones on the `rows` by `columns` grid | spray |
| `cell.drift` | chevron blades per panel | one instanced mesh | none |
| `cell.plenum` | nothing solid | it bounds the air particles | none |
| `cell.fan` | hub, `bladeCount` blades, the flared stack | extruded blade shape; lathe for the stack | spins at `rpm`, capped for the eye |
| `cell.drive` | gearbox, motor | cylinder and box | none |
| `flows.water` | water particles | one points cloud along each path; colour from `fromC` to `toC` | falls at `speedMS` |
| `flows.air` | air particles | one points cloud along each path | moves at `speedMS` |
| `dimensions` | labels and the part card | HTML over the canvas, not in the scene | none |
| `size` | camera framing and orbit limits | fit the box, keep the user's angle | eased |

- **Cutaway.** One clipping plane through the tower, moved by a slider from 0 (closed) to 1 (half open). Walls,
  fill and drift are clipped; fan, nozzles and particles are not, so the inside reads at once.
- **One renderer, one loop.** WebGL only. The loop stops when the tower is off screen or the tab is hidden.
- **Quality.** Three tiers by device: particle counts, pixel ratio (capped at 2) and shadows. With
  `prefers-reduced-motion` the fans and particles stand still.
- **Picking.** A tap on a part sends the island event `pick` with its `recordId`. The wizard shows the part card.

## 2. The live-update path

```mermaid
sequenceDiagram
  participant U as User
  participant W as Wizard (Blocks local source)
  participant R as Rate worker
  participant S as Size worker
  participant T as drafthouse-tower (island)
  U->>W: edits a duty input
  W->>S: abort the running size, if any
  W->>R: rate(duty, current design)
  R-->>W: Rating (tens of ms)
  W->>T: props.geometry
  T->>T: update each part, tween 300 ms
  Note over U,T: every further edit repeats these steps. The size worker is never in their way.
  W->>S: size(duty), 250 ms after the last edit
  Note over S: seconds today, under one after R2
  S-->>W: Sizing
  W->>R: rate(duty, kept design or rank 1)
  R-->>W: Rating
  W->>T: props.geometry
  T->>T: morph to the new design
```

- **Every edit reaches the tower through the rate worker alone.** Fan speed, air and water particles, the
  numbers and any dimension that follows the duty move within tens of milliseconds, whether a sizing is
  running, cancelled or not started.
- **An edit cancels the running sizing at once**, not after the pause, so no stale ranking can land and the
  size worker is free when the pause ends.
- A change of design (the user picks another row, or a new ranking puts another design first) reshapes the
  tower: each part's `update` moves and resizes the objects it already has. A part is rebuilt only when a count
  changes (blades, nozzles, layers, panels). Picking a row is one `rate` call, so it is as fast as an edit.
- While `size` runs, the tower shows the current design at the new duty and the wizard marks the ranking as
  updating.
- The tower never calls the engine. It draws what its props say.

## 3. States of the tower view

```mermaid
stateDiagram-v2
  [*] --> Poster: the page is parsed. The island shows its fallback image
  Poster --> Loading: first input near the tower, or the tower is opened
  Loading --> Live: the chunk is defined and a geometry is in props
  Loading --> Fallback: no WebGL, or the chunk failed to load
  Live --> Morphing: a new geometry arrives
  Morphing --> Live: the tween ends
  Morphing --> Morphing: a newer geometry arrives / tween from where it is
  Live --> Paused: off screen, or the tab is hidden
  Paused --> Live: back in view
  Live --> Fallback: the WebGL context is lost and does not come back
  Fallback --> Loading: the user retries
```

`Fallback` shows the poster and the dimension table. The numbers never depend on WebGL.

## 4. Speed of the update

| Step | Today | Target | Held by |
|---|---|---|---|
| An edit to the tower following it | one `rate`: about 12 ms natively | under 100 ms in the browser, with or without a sizing running | `local/live`, from P1 |
| An edit to a new ranking | one `size`: 2.5 s natively, more in WASM | under 1 s | R2, with a baseline; it starts when P1 is merged |

The first row is the design's: two workers and the cancel. The second is R2's. Two routes are open for it
without a change to the API: split the catalog by tower across workers, and remove repeated work inside the
capacity solve. Neither may move a recorded number.
