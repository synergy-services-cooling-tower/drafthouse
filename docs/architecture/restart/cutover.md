# Cut-over: what moves, what is retired, and the gates

## 1. What stays, moves or is deleted

| Today | Fate | By issue |
|---|---|---|
| `rust/src/*.rs`, the physics modules | move to `crates/engine/src/`. Numerics unchanged. Comments cut to the policy; module notes move to `docs/engine/`. `lib.rs` moves as it is; its re-export list goes in E2 with its last user. | E1, E2 |
| `rust/src/cli.rs`, `rust/src/bin/ct-engine.rs` | move to `crates/cli/`, a library and the bin, one file per command under `src/cmd/`. The command list, `src/commands.rs`, is frozen: the text surface replays recorded evidence and does not grow. | E1 |
| `rust/tests/*.rs` | move to `crates/engine/tests/` or `crates/cli/tests/`, by what each drives, with their assertions unchanged. `serde_json` stays their dev-dependency. | E1 |
| `rust/src/wasm.rs` | moves to `crates/wasm/src/lib.rs`, still answering the text commands. E2 makes JSON the shipped payload and the text commands the test-only feature `replay`. | E1, E2 |
| `rust/wasm/binding.mjs` | moves to `tools/parity/` as the raw ABI caller of the replay. `packages/engine` is written new. | E1 |
| `rust/cockpit-adapter/` | stays where it is until `rate` replaces it. F0 moves `cockpit/src/numfmt.rs`, which it compiles by path, into its `src/`. E1 re-points its engine dependency at `crates/engine`, source untouched. E2 deletes it. | F0, E1, E2 |
| `rust/cockpit-adapter/src/real_engine.rs` | its one-design run becomes `rate` in `crates/api` | E2 |
| `rust/cockpit-adapter/tests/parity.rs` and `tests/data/` | move to `crates/api/tests/`, re-pointed at `rate` with the same recorded numbers | E2 |
| the worked sheet inside `cli.rs` | moves with `cli.rs` in E1, then to `crates/api/src/worked/`, one file per section. `ct-engine select` prints it from there. | E1, E2 |
| `cockpit/` | deleted, except two files: `assets/revision-illustrative-catalog-v0.1.json` becomes `catalog/synthetic.json`, and `src/numfmt.rs` moves into the adapter | F0 |
| `web/geometry.js` | stays untouched until E3. It imports nothing. E3 records its output for the four synthetic towers as test vectors, ports it to `crates/geometry`, then deletes it. | E3 |
| `web/` the rest, with the vendored Three.js | deleted. Three.js comes from the lockfile. | F0 |
| `src/data/` | deleted. It carries money fields, and the tests already quote the records they use. | F0 |
| `bundles/`, `deploy-manifest.txt` | deleted. The release manifests replace the pins. | F0 |
| `scripts/parity/run.mjs` | moves to `tools/parity/`, re-pointed at the new crates. Both legs keep all 299 steps. E2 adds the `--engine api` leg. | E1, E2 |
| `scripts/release-archive.mjs`, `release-verify.mjs` | move to `tools/release/` | R1 |
| `scripts/` the rest: cockpit, native installers, deploy and deploytest, bundle, retirement, static, version, serve, smoke | deleted | F0 |
| `tests/*.test.js` | deleted with what they guarded. Three rules live on as structure: section 2. | F0 |
| `validation/` | stays, byte-identical | never edited |
| `docs/ENGINEERING_METHODS.md`, `VALIDATION.md`, `LIMITATIONS.md`, `REFERENCES.md`, `CATALOG_SCHEMA.md`, `FILL_LAYERS.md`, `TRACEABILITY_MATRIX.md`, `CTI_MRL_MAPPING.md`, `CALCULATION_EXAMPLE.md` | move to `docs/engine/` | E1 |
| `VALIDATION_RESULTS.md`, both copies | deleted. They restate a run's output and went stale before; the parity gate's kept report is the result. | E1 |
| `docs/API.md`, `ARCHITECTURE.md`, `WASM_ENGINE.md`, `COCKPIT_SEAMS.md`, `USER_GUIDE.md`, `ROADMAP.md` | deleted. This folder replaces them. | F0 |
| `.github/workflows/validate.yml`, `release.yml` | replaced by `ci.yml` (F0) and a new `release.yml` (R1), both written here | F0, R1 |
| `LICENSE`, `NOTICE` | stay | none |
| `README.md`, `CONTRIBUTING.md`, `THIRD_PARTY_NOTICES.md`, `package.json` | rewritten | F0, R1 |

### The legacy set, and what each issue leaves building

The legacy set is old code that still runs and is not yet under the new gates. It has its own gate,
`local/legacy`, and no baseline file. One rule keeps it building: **an issue deletes or moves a file only in the
PR that also deletes or re-points every file that builds from it.** `gates / done` builds and tests the whole
tree on every PR, the legacy set included, so a dangling path fails the PR that makes it.

| After | The legacy set | What `local/legacy` runs | Parity, on every PR |
|---|---|---|---|
| F0 | `rust/` whole (the engine, `ct-engine`, the WASM ABI, the adapter with `numfmt.rs` moved in), `scripts/parity/`, `web/geometry.js` | today's checks: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` | 299 steps native, 299 through WASM |
| E1 | `rust/cockpit-adapter/` alone, depending on `crates/engine` by path under the old crate name; `web/geometry.js` | `cargo fmt --check` and `cargo test` on the adapter | the same, from `tools/parity/` |
| E2 | `web/geometry.js` | the pin on `web/geometry.js` only | the same, plus the typed operations |
| E3 | nothing. `local/legacy` is deleted. | | the same |

- **F0 was run** on 2026-11-26, on a copy of 766fb98 with the cockpit, `web/` (but `geometry.js`), `src/`,
  `bundles/`, `tests/` and the dead scripts deleted and `numfmt.rs` moved: `cargo fmt --check` exit 0;
  `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --workspace` exit 0 with 197 tests passed;
  `node scripts/parity/run.mjs --no-build` exit 0 with 11,524 of 11,524 quantities passing, on a `ct-engine`
  built from the same `rust/src`. The WASM leg was not run: the lane host has no `wasm32` target. The adapter
  names `numfmt.rs` by a `#[path]` into the cockpit, which is why it cannot compile without the move.
- **E1 rests on two facts that were read**: the adapter uses the engine only through the names `lib.rs`
  re-exports, and nothing in the physics modules uses `cli`. So the adapter builds against `crates/engine` with
  a change to its manifest only (the dependency's path, and a workspace table of its own), as long as `lib.rs`
  keeps its list until the adapter goes.
- `web/geometry.js` imports nothing and nothing builds from it. `local/legacy` pins its sha256 from F0 until E3
  ports and deletes it, so E3 ports the v0.4.3 file.
- The adapter is never under clippy: it fails today's clippy with 3 findings and CI never ran it there. It is
  deleted in E2, not fixed.
- The root Cargo workspace needs a member to build. F0 creates it with `crates/blocks`; E1 adds the first
  engine crates.

## 2. Validation and parity carried over

| Evidence | Carried as | Proof that results are unchanged |
|---|---|---|
| 299 recorded steps and 11,524 quantities, `validation/regression-baseline.json` | `tools/parity/run.mjs` against `crates/cli` | same steps, same tolerances, zero edits to `validation/`; the gate pins the file's sha256 and fails unless 299 steps ran |
| `validation/test-vectors.json` and the 14 Rust test files | `crates/engine/tests/`, `crates/cli/tests/` | assertions unchanged |
| WASM against native over the same 299 steps (today's `--engine wasm`) | the same leg against `replay.wasm`: `crates/wasm` built with the test-only feature `replay`, under the build profile of the shipped file | every step within its recorded tolerance and within 1e-12 of native, the worst spread printed (1.28e-13 today); the gate fails unless 299 steps ran |
| The typed operations of the shipped `engine.wasm` (new) | `--engine api`: each recorded `select` and `validate-catalog` step (7) as `size` and `open`, and the recorded rating as `rate` | WASM against native to 1e-12, and against the same recorded values |
| The cockpit's recorded run (adapter parity) | `crates/api/tests/` on `rate` | the eleven headline numbers, the zones and the layers against the record |
| No money field anywhere (a Rust test and a script) | the same Rust test, walking `crates/*/src`; and `local/tree` opens `catalog/synthetic.json` through the engine's schema, which refuses an undeclared field | every PR |
| The catalog is synthetic (a `grep` in the release workflow) | gate `local/tree`: every catalog file's status is `SYNTHETIC / NOT VENDOR DATA` | every PR |
| No developer words on screen (`tests/dev-text.test.js`) | gate `local/guard`: `blocks check` on the screen fixtures, plus the word list over their text | every PR |
| `VALIDATION_RESULTS.md` matches reality (a drift test) | the file is gone | nothing left to drift |

Why two WASM legs. The typed operations reach only 7 of the 299 recorded steps; the other 292 are commands the
shipped file does not have. `replay.wasm` is the same crates, compiled by the same toolchain under the same
profile, with the text commands as its entry, so it proves the physics in WASM step by step. The `api` leg
proves the JSON boundary of the file that ships. `replay.wasm` is in no release: the release verifier refuses an
archive that holds it, and checks that the shipped file answers a text command with a `usage` refusal.

## 3. Retiring the snapshot pipeline

`public-snapshot.mjs` is run by hand in `drafthouse-private`. Its last run published v0.4.3 from what is still
the head of that repo's `staging`. Nothing is waiting there to be published.

| # | Step | Who | Effect on the live sites |
|---|---|---|---|
| 1 | Protect `main` and `staging` here: PR and one review required, no force push, no deletion. | owner | none. A snapshot push is now refused, which is what stops the pipeline. |
| 2 | Merge F0: the first commit written here. It removes the "generated by the snapshot builder" workflows and says in the README that this repo is the source. The owner then adds the required checks, `gates / done` and `drover/review`, which exist from this merge. | lane, then owner | none. The deploy reads release assets, and the existing releases stay published. |
| 3 | Re-file here what is still wanted of the open issues in `drafthouse-private`; close the rest there. | owner or orchestrator | none |
| 4 | Archive `drafthouse-private` on GitHub. That makes it read-only and can be undone. | owner | none |
| 5 | synergy-apps S1: the deploy accepts a `drafthouse-site-*` archive (`piece: "site"`) as well as the cockpit one. | synergy-apps lane | none: the pins still name v0.4.x |
| 6 | Tag `v1.0.0` here after R1. `release.yml` publishes the site archive and the embed package and verifies both from a fresh download. | owner | none |
| 7 | synergy-apps S2: the staging pin moves to `v1.0.0`. | synergy-apps lane | staging serves the new site |
| 8 | synergy-apps S6: after staging is proven, the prod pin moves. | owner | production serves the new site |
| 9 | synergy-apps S7: the deploy drops the cockpit piece. | synergy-apps lane | none |

- **Rollback** is a pin edit back to the tag it had, at any time before step 9.
- **Between step 2 and step 6 no release can be cut here.** The old workflow cannot build a tree without the
  cockpit. Production and staging keep serving the cockpit from the existing releases.
- **What the new site needs of its host.** Hashed files under `/assets/` served as immutable; every route a
  file, so no rewrite rule; a page that may run same-origin scripts and workers and compile WebAssembly. The
  cockpit needed the same, so no change is expected. S1 confirms it on staging.
- **The version is the git tag.** `release.yml` stamps it into the artifacts. No file mirrors it, so the
  version check goes.

## 4. Strictness from the first new commit (F0)

| Rule | How it is switched on |
|---|---|
| Every gatekeeper check on | `.gatekeeper/config.toml`: `packs = ["core", "rust", "ts"]`, `integration = "staging"`, a pinned version |
| Empty baseline | no file under `.gatekeeper/baseline/`, no `.drover/arch-baseline.txt` |
| No line cap | `[off] "core/cap" = "no line cap: drafthouse#1"`. This is the only `[off]` entry. No `ci/cap.toml`. |
| Comments | `[comments] policy = "strict"` in `.drover/arch.toml`: only `// SAFETY:` and one `///` line on a public item, in every file. No `TODO`, with or without an issue number: a `[[ban]]` refuses the word, because the stock policy would let `TODO(#N)` through. Unfinished work is an issue. |
| `drover arch-check` | `.drover/arch.toml` with layers, bans, collected folders and generated files ([adr.md](adr.md), last section) |
| `unsafe` | forbidden in every crate but `drafthouse-wasm`, named in `[rust] unsafe_crates` |
| Dependencies | `.gatekeeper/deps.toml`: each direct dependency with its reason. `serde` is banned from the engine's `src/` only; a test that reads JSON keeps `serde_json` as a dev-dependency. |
| Green between issues | `gates / done` builds and tests the whole tree, legacy set included, on every PR; section 1 lists what each issue leaves |
| CI | `.github/workflows/ci.yml` calls gatekeeper's reusable `gates.yml` at a pinned sha. One required check: `gates / done`. |
| Reviews and checks | on `main` and `staging`: PR required, one review, `gates / done` required; on `staging` also `drover/review`. The owner sets this; `drover protection` checks it. |
| Recipes | `.drover/design.md` (HTML mockups on Blocks, `docs/design/<slug>/`, 1440 and 390, light) and `.drover/verify.md` (start the built site, drive the wizard, screenshots at 1440 and 390) |

`.drover/rules.md` is created by this ADR's PR with the last line below, which the owner decided; F0 adds the
others above it:

```
- don't write a comment other than `// SAFETY:` or one `///` line on a public item (enforced by: core/arch comments)
- don't write `TODO`, with or without an issue number; file an issue (enforced by: core/arch ban)
- don't compute a shown number in TypeScript; show the engine's (enforced by: core/arch ban on `Math.` in packages/wizard)
- don't import Three.js outside packages/tower3d (enforced by: core/arch ban)
- don't import the tower statically from the site or the embed (enforced by: core/arch ban, local/first-load)
- don't fetch the engine or the tower chunk before the user asks (enforced by: local/first-load)
- don't pass 120 KB of first-load JavaScript or 150 KB of WASM, gzip (enforced by: local/first-load, local/wasm)
- don't drop Lighthouse mobile performance under 95 (enforced by: local/lighthouse)
- don't edit validation/ (enforced by: local/parity, sha256 pin)
- don't name a money field (enforced by: cargo test `no_money_ish_identifier_exists_anywhere_in_the_port`, local/tree)
- don't put a record in catalog/ that is not marked synthetic (enforced by: local/tree)
- don't add a model, texture or environment file (enforced by: local/tree)
- don't hand-edit a generated file; run `just gen` (enforced by: core/arch generated)
- don't use `unsafe` outside crates/wasm/src/lib.rs (enforced by: rust/bans)
- don't use serde in crates/engine/src; tests may (enforced by: core/arch ban)
- don't put replay.wasm or a text command in a released file (enforced by: tools/release verifier, local/wasm)
- don't run rate and size in one worker (enforced by: local/live)
- don't delete a file another file still builds from (enforced by: gates / done, local/legacy)
- don't add a dependency without its reason (enforced by: core/deps)
- don't add a ct-engine command; new behaviour is a command in crates/api (enforced by: core/arch frozen)
- don't add an app action outside a crates/api command (enforced by: core/arch generated command union, ban on `postMessage` outside packages/engine)
```

The cost of an empty baseline falls on E1. The carried engine must pass pedantic clippy, the function-size and
complexity limits and the strict comment policy as it arrives. That is a large, mechanical PR on validated
code, and the parity run is its proof.
