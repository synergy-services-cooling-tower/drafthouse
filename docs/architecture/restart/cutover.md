# Cut-over: what moves, what is retired, and the gates

## 1. What stays, moves or is deleted

| Today | Fate | By issue |
|---|---|---|
| `rust/src/*.rs`, the physics modules | move to `crates/engine/src/`. Numerics unchanged. Comments cut to the policy; module notes move to `docs/engine/`. `lib.rs` keeps `pub mod` lines only. | E1 |
| `rust/src/cli.rs`, `rust/src/bin/ct-engine.rs` | move to `crates/cli/`, one file per command under `src/cmd/`. Native only. The command list, `src/commands.rs`, is frozen: the text surface replays recorded evidence and does not grow. | E1 |
| `rust/tests/*.rs` | move to `crates/engine/tests/` with their assertions unchanged | E1 |
| `rust/src/wasm.rs` | moves to `crates/api/src/abi.rs`. The three exports stay; the payload becomes JSON. | E2 |
| `rust/wasm/binding.mjs` | deleted. `packages/engine` replaces it, starting with its raw ABI caller, which the parity harness uses. | E2 |
| `rust/cockpit-adapter/src/engine.rs` (a byte copy of the cockpit's contract), `fixture_engine.rs` | deleted | E2 |
| `rust/cockpit-adapter/src/real_engine.rs` | its one-design run becomes `rate` in `crates/api` | E2 |
| `rust/cockpit-adapter/tests/parity.rs` and `tests/data/` | move to `crates/api/tests/`, re-pointed at `rate` with the same recorded numbers | E2 |
| the worked sheet inside `cli.rs` | moves to `crates/api/src/worked/`, one file per section. `ct-engine select` prints it from there. | E2 |
| `cockpit/` | deleted, except `assets/revision-illustrative-catalog-v0.1.json`, which becomes `catalog/synthetic.json` | F0 |
| `web/geometry.js` | ported to `crates/geometry`, then deleted. Its output for the four synthetic towers is recorded first as test vectors. | E3 |
| `web/` the rest, with the vendored Three.js | deleted. Three.js comes from the lockfile. | F0 |
| `src/data/` | deleted. It carries money fields, and the tests already quote the records they use. | F0 |
| `bundles/`, `deploy-manifest.txt` | deleted. The release manifests replace the pins. | F0 |
| `scripts/parity/run.mjs` | moves to `tools/parity/`. The `--engine wasm` leg is re-pointed at the three operations. | E1, E2 |
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

Until E3 merges, a small legacy set stays where it is: `rust/` (its own Cargo workspace, as the cockpit was),
`web/geometry.js`, `scripts/parity/` and `validation/`. The legacy set has one gate of its own, `local/legacy`
(today's `cargo fmt`, `clippy -D warnings`, `cargo test` and the native parity run). Each issue deletes the
legacy part it replaces. No baseline file is written for it at any point.

## 2. Validation and parity carried over

| Evidence | Carried as | Proof that results are unchanged |
|---|---|---|
| 299 recorded steps, `validation/regression-baseline.json` | `tools/parity/run.mjs` against `crates/cli` | same steps, same tolerances, zero edits to `validation/`; the gate pins the file's sha256 |
| `validation/test-vectors.json` and the 14 Rust test files | `crates/engine/tests/` | assertions unchanged |
| WASM against native, 1e-12 relative | the same harness over every recorded `open`, `size` and `rate` case | worst deviation printed, as today |
| The cockpit's recorded run (adapter parity) | `crates/api/tests/` on `rate` | the eleven headline numbers, the zones and the layers against the record |
| No money field anywhere (a Rust test and a script) | the same Rust test, walking `crates/*/src`; and `local/tree` opens `catalog/synthetic.json` through the engine's schema, which refuses an undeclared field | every PR |
| The catalog is synthetic (a `grep` in the release workflow) | gate `local/tree`: every catalog file's status is `SYNTHETIC / NOT VENDOR DATA` | every PR |
| No developer words on screen (`tests/dev-text.test.js`) | gate `local/guard`: `blocks check` on the screen fixtures, plus the word list over their text | every PR |
| `VALIDATION_RESULTS.md` matches reality (a drift test) | the file is gone | nothing left to drift |

## 3. Retiring the snapshot pipeline

`public-snapshot.mjs` is run by hand in `drafthouse-private`. Its last run is v0.4.3 from b7decd2, which is
still the head of that repo's `staging`. Nothing is waiting there to be published.

| # | Step | Who | Effect on the live sites |
|---|---|---|---|
| 1 | Protect `main` and `staging` here: PR and one review required, no force push, no deletion. | owner | none. A snapshot push is now refused, which is what stops the pipeline. |
| 2 | Merge F0: the first commit written here. It removes the "generated by the snapshot builder" workflows and says in the README that this repo is the source. The owner then adds the required checks, `gates / done` and `drover/review`, which exist from this merge. | lane, then owner | none. The deploy reads release assets, and v0.4.0 and v0.4.3 stay published. |
| 3 | Re-file here what is still wanted of the 9 open issues in `drafthouse-private`; close the rest there. | owner or orchestrator | none |
| 4 | Archive `drafthouse-private` on GitHub. That makes it read-only and can be undone. | owner | none |
| 5 | synergy-apps S1: the deploy accepts a `drafthouse-site-*` archive (`piece: "site"`) as well as the cockpit one. | synergy-apps lane | none: the pins still name v0.4.x |
| 6 | Tag `v1.0.0` here after R1. `release.yml` publishes the site archive and the embed package and verifies both from a fresh download. | owner | none |
| 7 | synergy-apps S2: the staging pin moves to `v1.0.0`. | synergy-apps lane | staging serves the new site |
| 8 | synergy-apps S6: after staging is proven, the prod pin moves. | owner | production serves the new site |
| 9 | synergy-apps S7: the deploy drops the cockpit piece. | synergy-apps lane | none |

- **Rollback** is a pin edit back to the tag it had (`v0.4.3` on staging, `v0.4.0` on production), at any time
  before step 9.
- **Between step 2 and step 6 no release can be cut here.** The old workflow cannot build a tree without the
  cockpit. Production and staging keep serving the cockpit from the existing releases.
- **The SWA config needs no change for the new site.** Hashed files sit under `/assets/`, already immutable
  there. Every route is a file. The CSP already allows same-origin scripts, workers and `wasm-unsafe-eval`.
  S1 confirms this on staging.
- **The version is the git tag.** `release.yml` stamps it into the artifacts. No file mirrors it, so the
  version check goes.

## 4. Strictness from the first new commit (F0)

| Rule | How it is switched on |
|---|---|
| Every gatekeeper check on | `.gatekeeper/config.toml`: `packs = ["core", "rust", "ts"]`, `integration = "staging"`, a pinned version |
| Empty baseline | no file under `.gatekeeper/baseline/`, no `.drover/arch-baseline.txt` |
| No line cap | `[off] "core/cap" = "no line cap: drafthouse#1"`. This is the only `[off]` entry. No `ci/cap.toml`. |
| Comments | `[comments] policy = "strict"` in `.drover/arch.toml`: only `// SAFETY:`, `// TODO(#N)` and one `///` line on a public item, in every file |
| `drover arch-check` | `.drover/arch.toml` with layers, bans, collected folders and generated files ([adr.md](adr.md), last section) |
| `unsafe` | forbidden in every crate but `drafthouse-api`, named in `[rust] unsafe_crates` |
| Dependencies | `.gatekeeper/deps.toml`: each direct dependency with its reason |
| CI | `.github/workflows/ci.yml` calls gatekeeper's reusable `gates.yml` at a pinned sha. One required check: `gates / done`. |
| Reviews and checks | on `main` and `staging`: PR required, one review, `gates / done` required; on `staging` also `drover/review`. The owner sets this; `drover protection` checks it. |
| Recipes | `.drover/design.md` (HTML mockups on Blocks, `docs/design/<slug>/`, 1440 and 390) and `.drover/verify.md` (start the built site, drive the wizard, screenshots at 1440 and 390) |

`.drover/rules.md` starts with these lines:

```
- don't write a comment other than `// SAFETY:`, `// TODO(#N)` or one `///` line on a public item (enforced by: core/arch comments)
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
- don't use `unsafe` outside crates/api/src/abi.rs (enforced by: rust/bans)
- don't add a dependency without its reason (enforced by: core/deps)
- don't add a ct-engine command; new behaviour is an operation in crates/api (enforced by: core/arch frozen)
```

The cost of an empty baseline falls on E1. The carried engine must pass pedantic clippy, the function-size and
complexity limits and the strict comment policy as it arrives. That is a large, mechanical PR on validated
code, and the parity run is its proof.
