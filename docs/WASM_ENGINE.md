# The wasm engine (issue #24)

The engine's second build: the same Rust crate, compiled for `wasm32-unknown-unknown`, plus a
thin JavaScript binding that speaks data only. Together they are the publishable **`engine`**
piece of the ratified distribution plan — one engine, published as a pinned bundle for consuming
surfaces; the product UI carries the same engine compiled in through the cockpit adapter (issue
#58), and the JavaScript preview surface that once consumed the piece from the tree was retired
in issue #60 (D22). The JavaScript reference the port was checked against while it existed was
retired in issue #40 slice 4; the last commit that contains it is recorded in the private
decision record (D17).

**What this is not:** geometry, rendering and markup live elsewhere on purpose.
`web/geometry.js` (the `visuals` piece) stays JavaScript presentation-adjacent arithmetic, so
a new visual never triggers a wasm rebuild and a geometry tweak never touches the
parity-gated numerics. Nothing in the crate or in the binding draws anything.

## The build

```sh
cargo build --manifest-path rust/Cargo.toml \
    --target wasm32-unknown-unknown --profile wasm --lib
```

* artifact: `rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm`
* profile: `[profile.wasm]` in `rust/Cargo.toml` (release, `lto`, `panic = "abort"`,
  `strip = "debuginfo"`) — the artifact's bytes do not depend on a host's default release
  settings.
* crate type: `rlib` + `cdylib` (`rust/Cargo.toml`); the wasm exports themselves are
  `#[cfg(target_arch = "wasm32")]` (`rust/src/wasm.rs`), so a native build keeps an rlib and a
  binary and nothing else.
* dependencies: none. There is no binding generator: the ABI is three functions over the
  module's own memory, and the JavaScript side is a plain ES module.

Size and digest (what a pinned bundle records):

```sh
ls -l rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm
shasum -a 256 rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm
```

## The equivalence check

The parity harness is the gate both surfaces consume, and it can drive either build:

```sh
node scripts/parity/run.mjs              # the binary (native build, the default)
node scripts/parity/run.mjs --engine wasm  # the wasm artifact through the JS binding
```

`--engine wasm` builds both artifacts (skip with `--no-build`), then:

1. runs the **whole** recorded case set through the wasm build and compares it against the
   **recorded regression baseline** (`validation/regression-baseline.json`), quantity by
   quantity, with the harness's own named tolerances. The baseline is the comparison partner
   since issue #40 slice 4 retired the JavaScript reference; the binding's own surface is no
   longer part of the comparison (it had no engine reply behind it and so no recorded
   counterpart) and the run names that in its output;
2. cross-checks every reply against the native binary for the same arguments: the same reply
   shape, and numbers agreeing to the last few ulp. wasm32-unknown-unknown has no host libm,
   so the two builds evaluate `exp`/`ln`/`pow` in different implementations and the
   quadratures can land a few ulp apart; the worst spread measured over the recorded suite is
   `1.28e-13` relative — 0.128 of the `1e-12` tolerance, about 8x headroom, under one order —
   and a deviation beyond `1e-12` fails the run. This figure replaces an earlier "~3e-16 /
   three orders of headroom" estimate, which the #24 review falsified; the gate constant is
   unchanged from base. A coefficient change, a dropped term or a wasm-only regression cannot
   hide inside that.

Exit status is the harness's usual: `0` everything within tolerance, `1` something out (the
failures are listed), `2` the harness could not run.

## The ABI (`rust/src/wasm.rs`)

Three exports, no imports:

| export | meaning |
|---|---|
| `ct_alloc(len) -> ptr` | a buffer with room for `len` payload bytes; write them at `ptr + 4` |
| `ct_call(ptr) -> ptr` | run the command line in the buffer (takes ownership of it), return the reply |
| `ct_free(ptr)` | free a buffer this module produced |

Every buffer is length-prefixed: four little-endian bytes of payload length, then the payload.
The command line is UTF-8 with one argument per line — the arguments of a `ct-engine` command.
The reply is a JSON envelope:

```json
{"ok": true,  "value": { … the command's JSON, exactly what the binary prints … }}
{"ok": false, "status": 1, "error": {"error": "DomainError", "message": "…"}}
{"ok": false, "status": 2, "error": {"error": "usage",       "message": "…"}}
```

The status is the exit status the `ct-engine` binary reports for the same command, so a caller
that understands the binary understands the wasm surface, and the parity harness runs its
suite against either without a second mapping.

## The binding (`rust/wasm/binding.mjs`)

```js
import { readFileSync } from 'node:fs';
import { createEngine } from './rust/wasm/binding.mjs';

const engine = createEngine(readFileSync('rust/target/wasm32-unknown-unknown/wasm/synergy_drafthouse.wasm'));
```

* `engine.call(args)` — one `ct-engine` command, as data: `{ok: true, value}` or
  `{ok: false, status, error}`.
* `engine.select({ catalog, requirements, objective, maxResults, allOrders })` — the selection
  surface: the catalog as records, the requirements as fields, and back:

  | key | what it is |
  |---|---|
  | `inputs` | the values the run used: the resolved identity, the requirements, and the catalog records it read, dimensions and curve rows included |
  | `metrics` | the recommended candidate's headline numbers |
  | `candidates` | the ranked candidates |
  | `steps` | the worked step list, in the `worked.js` shape (`label`, `why`, `formula`, `substitution`, `value`, `unit`, `reference`, `kind`) |
  | `worked` | the whole worked sheet the steps belong to (`title`, `purpose`, `steps`, `result`) |

* `engine.recordFields()` — the record field names the engine accepts per catalog list
  (`ct-engine select-fields`). The binding projects a caller's catalog against this list
  before building record specs, because the selector refuses a field it does not read; the
  list has one source, the engine.

The binding renders nothing, derives no geometry and emits no HTML — it allocates a buffer,
writes a command line, reads a JSON reply and returns data.

## CI

`.github/workflows/validate.yml` job `wasm`: installs the pinned toolchain plus the
`wasm32-unknown-unknown` target, builds both artifacts, prints the wasm artifact's size and
`sha256sum`, and runs `node scripts/parity/run.mjs --engine wasm --no-build`. A change that
breaks the wasm path fails there instead of at a publisher.

The documentation contract for a step's data (issue #5's slice) is not settled yet; the sheet
this build emits follows `src/core/worked.js`'s shape field for field and is deliberately
minimal — it re-derives nothing, and every `value` in it is a number the selection already
computed.
