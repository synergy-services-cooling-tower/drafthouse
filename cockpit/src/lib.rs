//! Synergy Drafthouse cockpit - **visual pass** (issue #55's visual brief).
//!
//! A visual-only pass over the approved baseline: the same tower, the same fixture engine, an instrument
//! instead of a form. Nothing here computes physics. See `VISUAL_DATA_SEAMS.md` for every binding.
//!
//! Module map:
//! - [`state`]   what the instrument edits and shows: slots, drag, focus, staging, the drop rules
//! - [`project`] issue #74: the `.drafthouse` project file - one reader/writer, deterministic bytes,
//!   unknown fields preserved, plus the results export
//! - [`revision`] issue #74: the catalog revision object - immutable, hash-checked on every read
//! - [`sha256`]  issue #74: the in-crate digest those two are built on
//! - [`clip`]    round 5: the clip probe + the layout counters `tools/clip-check.mjs` asserts on
//! - [`scene`]   the tower section drawn with Bevy sprites (fan, flow map, spray, rails, operating point)
//! - [`ui`]      the egui instrument: rail, bays, rpm dock, stack, nozzle arrangement, charts, seams
//! - [`perf`]    round 3: the recorded performance grid the two CTI-style charts draw (read-only view)
//! - [`form`]    round 4: the custom-part form (`+ custom` per rail section)
//! - [`hover`]   round 4: the parameter card (hover / long press)
//! - [`duty_panel`] rounds 4/5: the duty & site panel (three collapsible sections) and the evidence gate
//! - [`clip`]    round 5: the layout measurement surface - every right-column text unit's rect against the
//!   column it had to fit in, published to `#mirror-clip` and checked by `tools/clip-check.mjs`
//! - [`three`]   round 2/3: the parametric 3D tower - **behind the `three-d` cargo feature, off by
//!   default**: the default build has no 3D view, no `bevy_pbr` and no 3D state
//! - [`theme`]   the baseline's design tokens, extended for the instrument layer
//! - [`bootstrap`] the one `App` composition both entry points run (issue #71): the wasm entry and
//!   the native binary call the same function, so the native build is not a fork of the UI
//! - [`cli`]     the native host's half (issue #71): the flags that replace `index.html`'s query
//!   string, the desktop window and the run loop - with their tests. The binary target is a thin
//!   `main` over it, carrying `test = false`, so `cargo test` never links a Bevy-sized harness for it
//! - [`clock`]   the millisecond clock, read from `performance.now()` on wasm and from a monotonic
//!   `Instant` on native (issue #71)
//! - [`bridge`]  wasm-bindgen entry + the HTML mirror (live text + the `data-*` runtime markers);
//!   **compiled on wasm32 only** (issue #71) - the native binary has no DOM, and the web crates are
//!   not in its dependency tree (see the known gap in `docs/COCKPIT_SEAMS.md`)

/// Issue #58: the adapter's real engine behind **this** crate's contract (real-engine builds).
#[cfg(feature = "real-engine")]
pub mod adapter_bridge;
/// #91 round 2: the answer card, the source marks and the tap-detail.
pub mod answer;
pub mod app;
/// Issue #71: the one `App` composition both entry points run.
pub mod bootstrap;
/// Issue #71: the wasm-bindgen entry + the HTML mirror - wasm builds only (the native binary has
/// no DOM, and the web crates are not in its dependency tree; see `docs/COCKPIT_SEAMS.md`).
#[cfg(target_arch = "wasm32")]
pub mod bridge;
/// Issue #71: the native host's half - the CLI, the desktop window and the run loop - with its
/// tests; the binary target is a thin `main` over [`cli::main`] and carries `test = false`.
pub mod cli;
pub mod clip;
/// Issue #71: the app's millisecond clock, `performance.now()` on wasm and a monotonic `Instant` here.
pub mod clock;
/// Issue #89: the comparison surface's model - saved project variants opened through #74's reader,
/// recomputed by the engine, with the diff/highlight and best-per-objective rules.
pub mod compare;
pub mod duty_panel;
/// Issue #58: the fixture's catalog block as the engine's own selection catalog (real-engine builds).
#[cfg(feature = "real-engine")]
pub mod engine_catalog;
/// Issue #58: the engine selection - which `Engine` the instrument runs on.
pub mod engine_select;
/// Issue #74: the project session - the file being edited, the native dialogs, the page's
/// download/upload, and the catalog-revision import. The two planes share every function here.
pub mod files;
pub mod form;
pub mod hover;
/// Issue #91: the notes drawer - every explanatory sentence, behind the validation badge.
pub mod notes;
/// drafthouse#85: the dependency-free PDF 1.7 writer the report export is built on.
pub mod pdf;
pub mod perf;
/// Issue #74: the `.drafthouse` project file - **one** reader/writer for both planes, with the results
/// export. No Bevy, no file dialogs: the native menu and the page's download/upload both call this.
pub mod project;
/// Issue #74: the catalog revision object - an immutable file (`{ id, publishedAt, sha256, status,
/// records }`), verified on every read, with the shipped `illustrative-catalog-v0.1`.
pub mod revision;
pub mod scene;
/// drafthouse#91 Part B: the app shell and the new screens.
pub mod screens;
/// Issue #74: SHA-256, in crate: the revision's declared digest and the contract mark. A `sha2`
/// dependency would enter the wasm target's dependency graph, which the issue's fence forbids moving.
pub mod sha256;
pub mod state;
pub mod theme;
#[cfg(feature = "three-d")]
pub mod three;
pub mod ui;

/// Re-exported so the panel and the docs quote the same table.
pub use drafthouse_cockpit_seams as seams;
