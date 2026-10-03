//! Issue #84's load-bearing test: the crossflow fixture the UI computes equals the native engine's
//! own output, at **exact f64 bits** - no tolerance, no band.
//!
//! The same shape as issue #83's [`super::fixture_equals_cli`]. The UI path is the screen's real
//! one: [`data::Cache`] stepped to completion (the same lazy `step_size`/`solve_xf` calls the wasm
//! app makes). The CLI path is `rust/src/cli.rs`'s [`eng::cli::run`] - the function
//! `rust/src/bin/ct-engine.rs` is a shell over - driven with the fixture's own duty and the winning
//! crossflow candidate the UI's selection run found, spelled as `crossflow` / `convergence`
//! arguments. Both paths run the same engine functions on the same grid inputs; the arguments going
//! in are Rust's shortest round-trip decimals and the answers coming back are read out of the bytes
//! the CLI printed ([`Cli`]'s walker, via [`cli`]), compared with `f64::to_bits`.
//!
//! What each path pins:
//! * `crossflow --cells {XF_CELLS}`: the whole scalar answer (cold water, range, approach, heat
//!   transfer, water energy, cp, the outlet air state) *and* the convergence block - the raw grid,
//!   the doubled grid, the Richardson value and the engine's discretisation-error estimate (the
//!   numbers issue #84's AC 2 shows on the surface).
//! * `convergence --base-cells 12 --safety-factor 1.25`: the three-grid study the panel shows.
//! * the drawing's own grid: the mean of the UI grid's last row is the CLI's `fineColdWaterC`, so
//!   the painted cells are the same sweep the CLI solved.
//!
//! A difference is a real difference, not a tolerance to widen.

use cockpit::engine::EngineInput;
use cockpit::fixture_engine::FixtureEngine;

use super::data::{self, XfGrid};
use super::fixture_equals_cli::{cli, n, Cli, Step, Step::Key};

const FIXTURE: &str = include_str!("../../assets/fixture.json");

/// The UI path: the screen's lazy cache, stepped (one frame's budget at a time, the shell's own
/// cadence) until the crossflow run is in it.
fn ui_grid(draft: &EngineInput) -> XfGrid {
    let mut cache = data::Cache::default();
    let mut steps = 0;
    // the size step and the grid step are separate frames' work, so keep stepping until the run lands
    while cache.xf.is_none() {
        cache.step(data::Want::Crossflow, FIXTURE, draft, None, None);
        steps += 1;
        assert!(steps < 10_000, "the crossflow run never finished");
    }
    cache
        .xf
        .expect("the crossflow run is in the cache")
        .unwrap_or_else(|error| panic!("the UI's crossflow run refused: {error}"))
}

/// The arguments both subcommands share: the duty, the cockpit's own water mass flow
/// ([`data::water_kg_s`], the spelling the screen's solve uses) and the winning candidate's dry-air
/// flow and available KaV/L.
fn shared_args(sub: &str, grid: &XfGrid, draft: &EngineInput) -> Vec<String> {
    vec![
        sub.into(),
        "--hot".into(),
        n(draft.duty.hot_water_c),
        "--db".into(),
        n(draft.duty.dry_bulb_c),
        "--wb".into(),
        n(draft.duty.wet_bulb_c),
        "--water".into(),
        n(data::water_kg_s(draft)),
        "--dry-air".into(),
        n(grid.cand.dry_air_kg_s),
        "--kavl".into(),
        n(grid.cand.available_merkel),
        "--p".into(),
        n(draft.duty.pressure_pa),
        "--salinity".into(),
        n(draft.duty.salinity_g_kg),
    ]
}

/// The UI's number and the CLI's printed number are the same `f64` bits.
fn same(what: &str, ui: f64, run: &Cli, path: &[Step]) {
    let cli_value = run.number(path);
    assert_eq!(
        ui.to_bits(),
        cli_value.to_bits(),
        "{what}: the UI's {ui:?} is not the CLI's {cli_value:?}"
    );
}

fn fixture_draft() -> EngineInput {
    FixtureEngine::from_json(FIXTURE)
        .expect("the fixture parses")
        .default_input()
}

/// The grid solve the UI paints, against `crossflow` on the same inputs - every scalar the engine
/// prints, bit for bit, and the printed bytes of the headline value.
#[test]
fn crossflow_grid_equals_the_native_cli() {
    let draft = fixture_draft();
    let grid = ui_grid(&draft);
    assert!(
        grid.cand.crossflow,
        "the winner at this duty is the crossflow tower"
    );
    let mut args = shared_args("crossflow", &grid, &draft);
    args.push("--cells".into());
    args.push(data::XF_CELLS.to_string());
    let run = cli(&args);

    same("cold water", grid.cold_c, &run, &[Key("coldWaterC")]);
    same("range", grid.range_c, &run, &[Key("rangeC")]);
    same("approach", grid.approach_c, &run, &[Key("approachC")]);
    same(
        "heat transfer",
        grid.heat_kw,
        &run,
        &[Key("heatTransferKW")],
    );
    same("water energy", grid.water_kw, &run, &[Key("waterEnergyKW")]);
    same("cp water", grid.cp_kj_kg_k, &run, &[Key("cpWaterKJkgK")]);
    same(
        "outlet air dry bulb",
        grid.outlet_db,
        &run,
        &[Key("outletAirDryBulbC")],
    );
    same(
        "outlet air humidity ratio",
        grid.outlet_hr,
        &run,
        &[Key("outletAirHumidityRatio")],
    );
    same(
        "outlet air enthalpy",
        grid.outlet_h,
        &run,
        &[Key("outletAirEnthalpyKJkgDryAir")],
    );
    same(
        "the raw grid",
        grid.coarse_cold_c,
        &run,
        &[Key("coarseColdWaterC")],
    );
    same(
        "the doubled grid",
        grid.fine_cold_c.expect("the doubled grid ran"),
        &run,
        &[Key("fineColdWaterC")],
    );
    same(
        "the Richardson value",
        grid.richardson_cold_c,
        &run,
        &[Key("richardsonColdWaterC")],
    );
    same(
        "the discretisation-error estimate",
        grid.error_c.expect("Richardson carries the estimate"),
        &run,
        &[Key("estimatedDiscretizationErrorC")],
    );

    // the printed bytes themselves: the CLI's spelling of the headline answer is Rust's shortest
    // round trip of the number the UI holds (the #83 reading discipline)
    assert_eq!(
        run.raw_at(&[Key("coldWaterC")]),
        n(grid.cold_c),
        "the CLI's own spelling of the cold water"
    );
    assert_eq!(
        run.raw_at(&[Key("richardsonColdWaterC")]),
        n(grid.richardson_cold_c),
        "the CLI's own spelling of the Richardson value"
    );

    // the drawing's grid: the UI paints the doubled sweep the engine solved - its last row's mean
    // is the CLI's own `fineColdWaterC` (the engine's `cold_water_c` is that mean, crossflow.rs)
    let last = grid.water.last().expect("the grid has a last row");
    let mean = last.iter().sum::<f64>() / last.len() as f64;
    assert_eq!(
        mean.to_bits(),
        run.number(&[Key("fineColdWaterC")]).to_bits(),
        "the painted grid's last row is the CLI's fine sweep"
    );
    // and the reported answer is exactly the engine's extrapolation of the two grids
    let fine = grid.fine_cold_c.expect("the doubled grid ran");
    assert_eq!(
        grid.cold_c.to_bits(),
        (2.0 * fine - grid.coarse_cold_c).to_bits(),
        "the reported cold water is 2*fine - coarse, the engine's own extrapolation"
    );
}

/// The optional three-grid study the panel shows, against `convergence` on the same grid inputs and
/// its own `n`/`2n`/`4n` ladder.
#[test]
fn the_convergence_study_equals_the_native_cli() {
    let draft = fixture_draft();
    let grid = ui_grid(&draft);
    let mut args = shared_args("convergence", &grid, &draft);
    // the study solves its own ladder; the screen's cell count is not part of it (the engine
    // overrides the grid per rung). The defaults are spelled so a changed default is a visible
    // difference, not a silent one.
    args.push("--base-cells".into());
    args.push("12".into());
    args.push("--safety-factor".into());
    args.push(n(1.25));
    let run = cli(&args);

    let study = &grid.study;
    assert_eq!(
        study.cells,
        [12, 24, 48],
        "the study ran the reference ladder"
    );
    same(
        "study coarse",
        study.cold[0],
        &run,
        &[Key("coarseCellsColdWaterC")],
    );
    same(
        "study medium",
        study.cold[1],
        &run,
        &[Key("mediumCellsColdWaterC")],
    );
    same(
        "study fine",
        study.cold[2],
        &run,
        &[Key("fineCellsColdWaterC")],
    );
    same(
        "study extrapolated",
        study.extrapolated,
        &run,
        &[Key("extrapolatedColdWaterC")],
    );
    same(
        "study fine-grid error",
        study.fine_error,
        &run,
        &[Key("fineGridErrorEstimateC")],
    );
    same(
        "study GCI",
        study.gci_pct,
        &run,
        &[Key("gridConvergenceIndexPct")],
    );
    let order = study.order.expect("the fixture's study has an order");
    assert_eq!(
        order.to_bits(),
        run.number(&[Key("observedOrder")]).to_bits(),
        "the observed order"
    );
    assert_eq!(
        run.raw_at(&[Key("observedOrder")]),
        n(order),
        "the CLI's own spelling of the observed order"
    );
}
