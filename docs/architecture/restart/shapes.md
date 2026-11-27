# Data shapes

The Rust types in `crates/api` and `crates/geometry` are the source. `just gen` writes them as TypeScript to
`packages/engine/src/gen/types.ts` and as one JSON Schema per command input and output to `crates/api/schemas/`.
They are shown here in TypeScript because both callers read that.
Every number is SI and its name carries the unit (`KgS`, `C`, `Pa`, `M`, `M2`, `KW`).

## 1. The WASM boundary

The three exports of today's `rust/src/wasm.rs` stay, in `crates/wasm`: `ct_alloc(len)`, `ct_call(ptr)`,
`ct_free(ptr)`, over length-prefixed buffers, with no binding generator and no JavaScript glue. In `engine.wasm`
the payload changes from a command line to JSON:

```ts
type Call = { [C in Command]: { command: C; input: Input<C> } }[Command];   // generated from crates/api/src/commands/
type Reply<T> = { ok: true; value: T } | { ok: false; error: Refusal };
```

Every call is one command of section 2. One WASM instance holds one opened catalog. Any command but `open`,
`import` and `export` before `open` is a `usage` refusal. The client runs
two instances of the one compiled module, one for `rate` and one for `size`, and opens the catalog in both.

`replay.wasm` is the same crate built with the feature `replay`, under the same build profile. Its payload is a
recorded command line and its reply is today's envelope, so the parity harness replays all 299 recorded steps
through it unchanged. It is built in CI, tested and discarded. `engine.wasm` answers a command line with a
`usage` refusal.

## 2. The commands

Every cockpit action is one typed command in `crates/api`, with a JSON input and a JSON output and a schema for
each. The UI holds no rating or rule logic: it sends a command and shows the reply. A later MCP server lists the
same schemas as its tools and calls the same `drafthouse_api::call`, unchanged. One command is one file in
`crates/api/src/commands/`; `just gen` writes the `Command` union, the TypeScript types and the schemas from that
folder, so a command cannot exist on one side only.

The cockpit's whole state is a `Project`, and a command that edits it returns the new `Project` with its rating.
Undo and redo keep a stack of the `Project` values the engine returned; they compute nothing.

```ts
type Command =
  | "open" | "set-duty" | "set-fill-layers" | "set-cells" | "set-structure" | "pin" | "unpin"
  | "rate" | "size" | "structural-check" | "compare" | "export" | "import";

type Project = {
  schema: 1;
  duty: Duty; limits: Limits; objective: Objective;
  cells: number;                       // identical cells; total flow splits evenly, one is rated and multiplied
  structure: Structure;
  design: Design;
  pins: PartRef[];                     // parts the user dragged onto the tower; sizing ranks around them
};
type PartRef = { kind: "fill" | "drift" | "fan" | "nozzle" | "casing"; recordId: string };
type Structure = { frame: "frp" | "steel" | "concrete"; code: "thai-dpt" | "asce-7"; site: LoadSite };
type Edited = { project: Project; rating: Rating };
```

| Command | Input | Output | Refuses | Lands in |
|---|---|---|---|---|
| `open` | `Catalog` | `Opened` | `catalog` | E2 |
| `set-duty` | `{ project; duty; limits?; objective? }` | `Edited` | `domain`, `infeasible` | E2 |
| `set-fill-layers` | `{ project; layers: Design["fillLayers"] }` | `Edited`; each depth snapped to its fill's module step | `infeasible` | E2 |
| `rate` | `{ project }` | `Rating` | `domain`, `infeasible` | E2 |
| `size` | `{ project; limit? }` | `Sizing`, every candidate keeping the pins | `infeasible` | E2 |
| `compare` | `{ projects: Project[] }`, one to three | `{ rows: { design: Design; headline: Rating["headline"] }[] }` | `usage` over three | E2 |
| `export` | `{ project }` | `{ file: ProjectFile; link: string }`; the link carries the file in its fragment | none | E2 |
| `import` | `{ text: string }`, a file or a link | `ProjectFile` | `usage` on an unknown `schema` | E2 |
| `set-cells` | `{ project; cells: number }` | `Edited`; the geometry has `cells` side by side | `usage` under one | #2 |
| `set-structure` | `{ project; structure: Structure }` | `Edited`; the casing part and the frame line follow | `usage` | #3 |
| `structural-check` | `{ project }` | `{ loads; members; steps: WorkedSheet }` for the picked code and site | `domain` | #3 |
| `pin` | `{ project; part: PartRef }` | `Edited` with the part in `pins` and in the design | `infeasible`, in plain words | #4 |
| `unpin` | `{ project; part: PartRef }` | `Edited` | none | #4 |

`ProjectFile` is `{ schema: 1; name: string; project: Project }`, the saved scenario. `LoadSite`, `loads` and
`members` are #3's, and each load code has its own site fields. A command of #2, #3 or #4 is added as its file in
that issue; until then the union does not hold it and the panel that sends it is not built.

## 3. The operations

The commands `open`, `size` and `rate` carry the engine's own types:

```ts
type Catalog = {                       // record fields: docs/engine/CATALOG_SCHEMA.md, unchanged
  metadata: { id: string; revision: string; status: string };
  waterQualityFactors: Record<string, { thermalMultiplier: number; pressureMultiplier: number }>;
  towers: TowerRecord[]; fills: FillRecord[]; driftEliminators: DriftRecord[];
  fans: FanRecord[]; nozzles: NozzleRecord[];
};
type Opened = { metadata: Catalog["metadata"]; counts: Record<"towers" | "fills" | "driftEliminators" | "fans" | "nozzles", number> };

type Duty = {
  waterFlowM3Hr: number; hotWaterC: number; targetColdWaterC: number;
  wetBulbC: number; dryBulbC: number; pressurePa: number;
  salinityGKg: number; waterQualityClass: string; cyclesOfConcentration: number;
};
type Limits = Partial<{                // absent means the engine's default, as today
  maxDriftPpm: number; maxElectricalInputKW: number; maxFootprintM2: number;
  minimumThermalMarginC: number; nozzlePressureDropPa: number; speedRatios: number[];
}>;
type Objective = "least-over-capacity" | "lowest-electrical-input" | "lowest-makeup-water" | "lowest-total-air-side-pressure";
type Design = {
  towerId: string; driftId: string; fanId: string; speedRatio: number;
  fillLayers: { fillId: string; depthM: number; thermalMultiplier?: number; pressureMultiplier?: number }[];   // top first
};

type SizeRequest = { project: Project; limit?: number };
type SizeOptions = { limit?: number; signal?: AbortSignal };              // the client's: abort ends the sizing
type Sizing = {
  objective: Objective; feasibleCount: number; warning: string;
  candidates: Candidate[];             // ranked, at most `limit`
  rejections: { reason: string; count: number }[];
};
type Candidate = {
  rank: number; design: Design;
  coldWaterC: number; thermalMarginC: number; capacityKgS: number | null; capabilityRatio: number | null;
  electricalInputKW: number; makeupKgS: number; totalPressurePa: number; airflowM3S: number; footprintM2: number;
};

type RateRequest = { project: Project };
type Rating = {
  design: Design;
  headline: {                          // the eleven numbers the cockpit showed first
    capabilityPct: number; waterFlowM3Hr: number; coldWaterC: number; rangeC: number; approachC: number;
    airflowM3S: number; fanPowerKW: number; totalPressurePa: number; evaporationPct: number;
    makeupM3Hr: number; kavlTotal: number;
  };
  zones: { zone: "inlet" | "rain" | "fill" | "spray" | "drift" | "plenum" | "stack" | "fixed"; layer: number | null; pressurePa: number; sharePct: number }[];
  layers: { fillId: string; depthM: number; kavl: number; pressurePa: number; coolingSharePct: number; insideEnvelope: boolean }[];
  fanSystemCurve: { fan: Point[]; system: Point[]; operatingPoint: Point };   // x: airflow m3/s, y: Pa
  motor: { requiredKW: number; selectedKW: number | null };
  nozzle: { nozzleId: string; count: number; flowPerNozzleM3S: number; excessFlowPct: number };
  waterBalance: { evaporationKgS: number; driftKgS: number; blowdownKgS: number; makeupKgS: number };
  steps: WorkedSheet;
  geometry: TowerGeometry;
  provenance: { engine: string; catalog: Catalog["metadata"]; warning: string };
};

type WorkedSheet = { title: string; purpose: string; sections: { id: string; title: string; steps: WorkedStep[] }[] };
type WorkedStep = {                    // today's step fields, unchanged
  kind: "calc" | "note"; label: string; why: string;
  formula: string | null; substitution: string | null; value: number | null; unit: string; reference: string | null;
};

type Refusal =
  | { kind: "usage"; message: string }                                  // the caller's bug
  | { kind: "catalog"; message: string; violations: { list: string; recordId: string; field: string; why: string }[] }
  | { kind: "domain"; message: string }                                 // the physics refuses the duty
  | { kind: "infeasible"; message: string; limits: { field: string; message: string; value: number; unit: string; min: number | null; max: number | null }[] }
  | { kind: "superseded" }                                              // the client's: a newer call replaced this one, or it was aborted
  | { kind: "unavailable"; message: string };                           // the client's: the engine could not load
```

How the issue's four words map: **rating** is `rate`; **sizing and ranking** is `size`; **calc steps** is
`Rating.steps`; **geometry** is `Rating.geometry`. A catalog record with a field the schema does not declare is a
`catalog` refusal, as today: a money field can never ride along.

`size` returns ranked rows, not whole ratings. That cuts the reply from 753 KB to about 60 KB. `rate` reruns one
design, about 12 ms natively. A `rate` call never waits for a `size` call: each has its own worker
([loading.md](loading.md) section 3).

## 4. The geometry contract

The engine says what is where and how big. It says nothing about meshes, materials or motion. Metres,
right-handed, `y` up, `y = 0` at the basin floor, origin at the centre of the tower plan, cells along `+x`.

```ts
type Vec3 = [number, number, number];
type Box = { centre: Vec3; size: Vec3 };

type TowerGeometry = {
  schema: 1;
  topology: "counterflow" | "crossflow";
  size: Vec3;                          // the box that holds everything, for framing the camera
  cells: Cell[];                       // exactly one today
  flows: { air: FlowPath[]; water: FlowPath[] };
  dimensions: Dimension[];
};
type Cell = {
  recordId: string;
  basin: { box: Box; waterDepthM: number };
  shell: { box: Box; wallM: number };
  inlets: { face: "+x" | "-x" | "+z" | "-z"; bottomM: number; heightM: number; spanM: number }[];
  fill: { recordId: string; layer: number; box: Box; airAxis: "y" | "z" }[];   // counterflow: one stack; crossflow: two banks
  nozzles: { recordId: string; count: number; rows: number; columns: number; pitchM: number; headerYM: number; headerDiameterM: number };
  drift: { recordId: string; panels: Box[] };
  plenum: Box;
  fan: {
    recordId: string; centre: Vec3; diameterM: number; hubDiameterM: number; bladeCount: number;
    rpm: number | null;                // nominal rpm times speed ratio, when the record states one
    stack: { bottomM: number; topM: number; throatDiameterM: number; exitDiameterM: number };
  };
  drive: { motorKW: number | null; gearbox: Box };
};
type FlowPath = { points: Vec3[]; speedMS: number; fromC: number; toC: number };
type Dimension = { key: string; label: string; value: number; unit: string; formula: string; substitution: string; source: string };
```

- The plan rules, the level stack and the layout constants are those of today's `web/geometry.js`, ported to
  Rust without change. Each dimension keeps its formula, its substituted numbers and the record field it came
  from, so the part card can show them.
- `flows` carries real values: air speed from the rated face velocity, water temperatures from hot to cold.
- A new field is additive. A change of meaning raises `schema`, and the tower view refuses a schema it does not know.

## 5. What the embed package exports

```ts
export function loadEngine(assets?: EngineAssets): Promise<Engine>;
export function mountWizard(element: HTMLElement, options: {
  catalog: Catalog; duty?: Partial<Duty>; assets?: EngineAssets;       // one light theme; no theme option
  onRated?: (rating: Rating) => void; onRefused?: (refusal: Refusal) => void;
}): { setCatalog(catalog: Catalog): void; destroy(): void };
export function mountTower(element: HTMLElement, options: { geometry: TowerGeometry }):
  { show(geometry: TowerGeometry): void; set(view: Partial<TowerViewState>): void; destroy(): void };
export type EngineAssets = { wasmUrl: string; workerUrl: string };    // only for a host whose bundler cannot resolve them
export type TowerViewState = { cutaway: number; air: boolean; water: boolean; labels: boolean; paused: boolean };
export type { Command, Call, Project, Catalog, Duty, Limits, Objective, Design, Sizing, Candidate, Rating, TowerGeometry, Refusal };
export const version: string;
```

Files in the tarball: `index.js`, `index.d.ts`, a hashed tower chunk (Three.js inside), `engine.worker.js`,
`engine.wasm`, `embed.css` (Blocks' scoped stylesheet plus the tower's), `package.json`, `LICENSE`,
`THIRD_PARTY_NOTICES.md`. It has no `dependencies`.

What the embed needs of its host: a bundler that resolves ES modules and asset URLs (or `EngineAssets`), and a
page that may compile WebAssembly and start same-origin workers.

## 6. Release assets and the pin

| Asset of release `v<version>` | What it is |
|---|---|
| `drafthouse-site-<version>.tar.gz` | The built public site, in today's deterministic container |
| `drafthouse-site-<version>.tar.gz.sha256` | Its digest, as today |
| `drafthouse-site-<version>.manifest.json` | Today's manifest shape: `archive { name, bytes, sha256 }`, `piece: "site"`, `prefix: "site"`, `commit`, `files[] { path, bytes, sha256 }` |
| `drafthouse-embed-<version>.tgz` | The embed package, as `npm pack` writes it |
| `drafthouse-embed-<version>.tgz.sha256` | Its digest |

The pin in synergy-apps names a tag and the sha256 of the site archive, as today, and gains the sha256 of the
embed package. Its file and its other fields are the host repo's own.
