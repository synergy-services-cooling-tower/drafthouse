//! Crossflow grid — the JavaScript suite's and the recorded vectors' expectations for
//! `src/core/crossflow.js`, ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `validation/test-vectors.json` (family `crossflow`) — the recorded regression vectors,
//!   reproduced at the same 1e-6 relative tolerance `tests/vectors.test.js` uses.
//! * `tests/crossflow-natural.test.js` — the grid-convergence, monotonicity and
//!   thermodynamic-limit expectations.
//! * `src/core/crossflow.js` — the formulas, the `??` defaults and the refusal message.
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    crossflow_convergence_study, saturated_temperature_from_enthalpy, solve_crossflow_grid,
    CrossflowGridInput, CrossflowStudyInput,
};

/// The `CASE` of `tests/crossflow-natural.test.js`.
fn case_input() -> CrossflowGridInput {
    CrossflowGridInput::new(42.0, 33.0, 27.0, 200.0, 150.0, 1.6)
}

/// The same `agrees` rule `tests/vectors.test.js` uses for recorded vectors: the relative
/// tolerance with an absolute floor of 1, so a value recorded as 0.098917 is allowed its last
/// digit of rounding.
fn assert_close(actual: f64, expected: f64, relative: f64, label: &str) {
    let scale = expected.abs().max(1.0);
    let difference = (actual - expected).abs();
    assert!(
        difference / scale < relative,
        "{label}: got {actual}, expected {expected} (|Δ| {difference})"
    );
}

/// The four recorded vectors of `validation/test-vectors.json`, family `crossflow`.
#[test]
fn recorded_crossflow_vectors_reproduce() {
    let vectors = [
        (
            "xf-base",
            CrossflowGridInput::new(42.0, 33.0, 27.0, 200.0, 150.0, 1.6),
            32.333352,
            8088.067,
            36.38799,
            0.098917,
        ),
        (
            "xf-low-transfer",
            CrossflowGridInput::new(40.0, 30.0, 24.0, 200.0, 150.0, 0.8),
            32.395153,
            6362.4565,
            32.611451,
            0.059272,
        ),
        (
            "xf-high-transfer",
            CrossflowGridInput::new(45.0, 35.0, 28.0, 250.0, 200.0, 2.4),
            32.269144,
            13316.4595,
            38.710359,
            0.154465,
        ),
        (
            "xf-temperate",
            CrossflowGridInput::new(32.0, 20.0, 14.0, 180.0, 160.0, 1.4),
            23.492129,
            6401.6276,
            25.638455,
            0.066315,
        ),
    ];
    for (id, input, cold_water_c, heat_transfer_kw, outlet_dry_bulb_c, error_c) in vectors {
        let result = solve_crossflow_grid(&input).expect("the vector must solve");
        assert_close(
            result.cold_water_c,
            cold_water_c,
            1e-6,
            &format!("{id} coldWaterC"),
        );
        assert_close(
            result.heat_transfer_kw,
            heat_transfer_kw,
            1e-6,
            &format!("{id} heatTransferKW"),
        );
        assert_close(
            result.outlet_air_state.dry_bulb_c,
            outlet_dry_bulb_c,
            1e-6,
            &format!("{id} outletDryBulbC"),
        );
        assert_close(
            result
                .grid_convergence
                .estimated_discretization_error_c
                .expect("Richardson extrapolation is on by default"),
            error_c,
            1e-6,
            &format!("{id} estimatedDiscretizationErrorC"),
        );
    }
}

#[test]
fn the_default_grid_is_grid_converged_against_a_much_finer_one() {
    // tests/crossflow-natural.test.js: 'crossflow cold-water temperature is grid-converged at
    // the default cell count'.
    let result = solve_crossflow_grid(&case_input()).expect("default grid solves");
    let reference =
        solve_crossflow_grid(&case_input().with_cells(96, 96)).expect("fine grid solves");
    assert!(
        (result.cold_water_c - reference.cold_water_c).abs() < 0.01,
        "default grid {} vs fine grid {}",
        result.cold_water_c,
        reference.cold_water_c
    );
    let error = result
        .grid_convergence
        .estimated_discretization_error_c
        .expect("Richardson extrapolation is on by default");
    assert!(error < 0.15, "reported discretisation error {error}");
}

#[test]
fn the_discretisation_error_falls_as_the_grid_is_refined() {
    // tests/crossflow-natural.test.js: 'crossflow discretisation error falls as the grid is
    // refined'.
    let errors: Vec<f64> = [6usize, 12, 24, 48]
        .into_iter()
        .map(|cells| {
            solve_crossflow_grid(&case_input().with_cells(cells, cells))
                .expect("refinement solves")
                .grid_convergence
                .estimated_discretization_error_c
                .expect("Richardson extrapolation is on by default")
        })
        .collect();
    for pair in errors.windows(2) {
        assert!(
            pair[1] < pair[0],
            "error did not fall from {} to {}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn the_crossflow_model_respects_its_thermodynamic_limits() {
    // tests/crossflow-natural.test.js: 'crossflow respects its thermodynamic limits'.
    let infinite = solve_crossflow_grid(&CrossflowGridInput {
        available_merkel_number: 80.0,
        ..case_input().with_cells(24, 24)
    })
    .expect("infinite area solves");
    assert!(
        infinite.cold_water_c > 27.0,
        "water cannot be cooled below the entering wet bulb"
    );
    assert!(infinite.cold_water_c < 42.0);

    let negligible = solve_crossflow_grid(&CrossflowGridInput {
        available_merkel_number: 1e-4,
        ..case_input().with_cells(24, 24)
    })
    .expect("negligible area solves");
    assert!(
        42.0 - negligible.cold_water_c < 0.05,
        "with no transfer area the water must leave at inlet temperature, got {}",
        negligible.cold_water_c
    );
}

#[test]
fn cooling_increases_monotonically_with_available_transfer() {
    // tests/crossflow-natural.test.js: 'crossflow cooling increases monotonically with
    // available transfer'.
    let mut previous = f64::INFINITY;
    for available_merkel_number in [0.4, 0.8, 1.2, 1.6, 2.4] {
        let cold_water_c = solve_crossflow_grid(&CrossflowGridInput {
            available_merkel_number,
            ..case_input().with_cells(20, 20)
        })
        .expect("the sweep solves")
        .cold_water_c;
        assert!(
            cold_water_c < previous,
            "Me={available_merkel_number} gave {cold_water_c}, not colder than {previous}"
        );
        previous = cold_water_c;
    }
}

#[test]
fn the_convergence_study_sees_a_first_order_scheme() {
    // tests/crossflow-natural.test.js: 'crossflow discretisation is first order, as the scheme
    // implies'.
    let study = crossflow_convergence_study(&CrossflowStudyInput::new(case_input()))
        .expect("the study solves");
    let observed_order = study.observed_order.expect("the gaps are not degenerate");
    assert!(
        observed_order > 0.85 && observed_order < 1.25,
        "observed order {observed_order} is not consistent with a first-order scheme"
    );
    assert!(
        study.grid_convergence_index_pct < 1.0,
        "GCI {} %",
        study.grid_convergence_index_pct
    );
    let reported = solve_crossflow_grid(&case_input())
        .expect("the reported solve works")
        .cold_water_c;
    assert!(
        (reported - study.extrapolated_cold_water_c).abs() < 0.02,
        "{reported} vs {}",
        study.extrapolated_cold_water_c
    );
    assert_eq!(study.cells, [12, 24, 48]);
}

#[test]
fn the_grid_and_the_inputs_are_each_refused_out_of_domain() {
    let too_few_cells =
        solve_crossflow_grid(&case_input().with_cells(1, 18)).expect_err("must refuse");
    assert_eq!(
        too_few_cells.message(),
        "Crossflow grid requires at least 2 × 2 cells."
    );
    for bad in [
        CrossflowGridInput {
            water_mass_flow_kg_s: 0.0,
            ..case_input()
        },
        CrossflowGridInput {
            dry_air_mass_flow_kg_s: -1.0,
            ..case_input()
        },
        CrossflowGridInput {
            available_merkel_number: 0.0,
            ..case_input()
        },
    ] {
        assert!(
            solve_crossflow_grid(&bad).is_err(),
            "a non-positive input must refuse"
        );
    }
}

/// `saturatedTemperatureFromEnthalpy` — the psychrometrics remainder this slice ports — can
/// be checked directly: it inverts the already-ported saturated enthalpy. The bounds are the
/// ones the crossflow and selection call sites use; a bound at or above the point where
/// saturation pressure reaches atmospheric pressure is refused by the saturation correlation,
/// exactly as the reference refuses it.
#[test]
fn the_saturated_temperature_inversion_round_trips() {
    use synergy_drafthouse::{saturated_air_enthalpy_kj_kg_dry_air, PsychrometricOptions};
    for temperature_c in [5.0, 20.0, 33.0, 60.0] {
        let enthalpy = saturated_air_enthalpy_kj_kg_dry_air(
            temperature_c,
            101_325.0,
            PsychrometricOptions::default(),
        )
        .expect("the enthalpy evaluates");
        let inverted = saturated_temperature_from_enthalpy(enthalpy, 101_325.0, [-50.0, 80.0])
            .expect("the inversion solves");
        assert_close(inverted, temperature_c, 1e-6, "round trip");
    }
    // The reference's own default bounds are [-50, 100]; at 100 °C saturated air carries
    // atmospheric pressure, and the saturation correlation refuses (the reference refuses it
    // too — `solveBracketedRoot` evaluates every bound before it starts).
    let enthalpy_at_40 =
        saturated_air_enthalpy_kj_kg_dry_air(40.0, 101_325.0, PsychrometricOptions::default())
            .expect("the enthalpy evaluates");
    assert!(
        saturated_temperature_from_enthalpy(enthalpy_at_40, 101_325.0, [-50.0, 100.0]).is_err()
    );
}
