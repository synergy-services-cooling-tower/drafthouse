//! Characteristic-curve capability — the JavaScript suite's expectations for
//! `src/core/capability.js`, ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `tests/capability.test.js` — identical design and test conditions evaluate to 100 %
//!   capability, and a seeded Monte Carlo is repeatable for the same seed.
//! * `validation/test-vectors.json` (family `capability`) — the recorded cases are replayed in
//!   `rust/tests/vectors.rs`, next to the two JavaScript vector families.
//! * `src/core/capability.js` — the guard order and messages, the default exponent, the
//!   `-1e12` infeasible-demand residual, the curve sampling range and filter, and the
//!   Monte-Carlo acceptance floor.
//! * `tests/vectors.test.js` — the seeded generator's first draws, quoted in `rand_js_draws`
//!   (produced by `createSeededRandom(17)` in the JavaScript engine itself).
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    create_seeded_random, evaluate_characteristic_capability, gaussian_random,
    monte_carlo_characteristic_capability, percentile, CapabilityCondition, CapabilityUncertainty,
    CharacteristicCapabilityInput, ConditionField, InletEnthalpyConvention, Integration,
    MonteCarloInput,
};

/// The condition of `tests/capability.test.js`.
fn condition() -> CapabilityCondition {
    CapabilityCondition::new(200.0, 133.3333333, 42.0, 32.0, 27.0, 33.0).with_pressure(101_325.0)
}

/// `evaluateCharacteristicCapability({ design, test, characteristicExponent: -0.6 })`.
fn capability_input(
    design: CapabilityCondition,
    test: CapabilityCondition,
) -> CharacteristicCapabilityInput {
    CharacteristicCapabilityInput::new(design, test).with_characteristic_exponent(-0.6)
}

fn message(error: &synergy_drafthouse::DomainError) -> String {
    error.message().to_string()
}

#[test]
fn identical_design_and_test_conditions_evaluate_to_one_hundred_percent() {
    // tests/capability.test.js: `Math.abs(capabilityPct - 100) < 0.001`.
    let result = evaluate_characteristic_capability(&capability_input(condition(), condition()))
        .expect("the documented case evaluates");
    assert!(
        (result.capability_pct - 100.0).abs() < 0.001,
        "capabilityPct {}",
        result.capability_pct
    );
    assert_eq!(result.design_water_to_dry_air_ratio, 200.0 / 133.3333333);
    assert_eq!(result.test_water_to_dry_air_ratio, 200.0 / 133.3333333);
}

#[test]
fn the_default_exponent_applies_when_the_input_omits_it() {
    // src/core/capability.js: `characteristicExponent = -0.6`. `CharacteristicCapabilityInput::new`
    // applies the reference's default; `with_characteristic_exponent` overrides it.
    let input = CharacteristicCapabilityInput::new(condition(), condition());
    assert_eq!(input.characteristic_exponent, -0.6);
    let result = evaluate_characteristic_capability(&input).expect("defaults evaluate");
    assert_eq!(result.characteristic_exponent, -0.6);
    assert_eq!(
        result.inlet_enthalpy_convention,
        InletEnthalpyConvention::Bulk
    );
    // The reference's own test asserts within 0.001: the capability is root-solved, so an
    // identical design and test condition lands on 100 % to within the scan's stopping width.
    assert!(
        (result.capability_pct - 100.0).abs() < 0.001,
        "capabilityPct {}",
        result.capability_pct
    );
}

#[test]
fn a_warmer_test_cold_water_declines_and_a_cooler_one_gains_capability() {
    // The recorded vectors (`cap-worse` 88.7939 %, `cap-better` 111.605272 %) pin the
    // direction; this test only asserts the direction, not the recorded number.
    let worse = evaluate_characteristic_capability(&capability_input(
        condition(),
        CapabilityCondition::new(200.0, 133.3333333, 42.0, 33.0, 27.0, 33.0),
    ))
    .expect("evaluates");
    let better = evaluate_characteristic_capability(&capability_input(
        condition(),
        CapabilityCondition::new(200.0, 133.3333333, 42.0, 31.0, 27.0, 33.0),
    ))
    .expect("evaluates");
    assert!(worse.capability_pct < 100.0, "{}", worse.capability_pct);
    assert!(better.capability_pct > 100.0, "{}", better.capability_pct);
}

#[test]
fn a_positive_exponent_is_refused_after_both_conditions_are_validated() {
    // The reference validates `design`, then `test`, then the exponent.
    let error = evaluate_characteristic_capability(
        &capability_input(condition(), condition()).with_characteristic_exponent(0.5),
    )
    .expect_err("a non-negative exponent must be refused");
    assert_eq!(
        message(&error),
        "A whole-tower characteristic exponent must be negative."
    );
    // A bad condition is reported before the exponent.
    let error = evaluate_characteristic_capability(
        &capability_input(condition(), condition()).with_characteristic_exponent(0.5),
    )
    .expect_err("refused");
    assert!(message(&error).starts_with("A whole-tower"));
    let error = evaluate_characteristic_capability(&CharacteristicCapabilityInput::new(
        condition(),
        CapabilityCondition::new(200.0, 133.3333333, 42.0, 27.0, 27.0, 33.0),
    ))
    .expect_err("the temperature ordering is checked before the exponent");
    assert_eq!(
        message(&error),
        "test temperatures must satisfy hot water > cold water > wet bulb."
    );
}

#[test]
fn a_condition_with_a_non_finite_field_is_refused_naming_it() {
    let error = evaluate_characteristic_capability(&capability_input(
        CapabilityCondition::new(200.0, 133.3333333, 42.0, 32.0, f64::NAN, 33.0),
        condition(),
    ))
    .expect_err("NaN wet bulb must be refused");
    assert_eq!(message(&error), "design.wetBulbC is required.");
    let error = evaluate_characteristic_capability(&capability_input(
        condition(),
        CapabilityCondition::new(f64::INFINITY, 133.3333333, 42.0, 32.0, 27.0, 33.0),
    ))
    .expect_err("infinite flow must be refused");
    assert_eq!(message(&error), "test.waterMassFlowKgS is required.");
}

#[test]
fn the_curves_span_the_reference_window_and_carry_both_ordinates() {
    // src/core/capability.js: `range(max(0.05, min(...) * 0.55), max(...) * 1.55, curvePoints)`,
    // filtered to the points whose design demand is finite. The documented condition is
    // feasible at the low end of the window (the approach to the wet bulb pinches only at
    // higher L/G), so every point survives here.
    let result = evaluate_characteristic_capability(&capability_input(condition(), condition()))
        .expect("evaluates");
    assert_eq!(result.curves.len(), 80);
    let design_ratio = result.design_water_to_dry_air_ratio;
    let test_ratio = result.test_water_to_dry_air_ratio;
    let capability_ratio = result.capability_water_to_dry_air_ratio;
    let start = 0.05_f64.max(design_ratio.min(test_ratio).min(capability_ratio) * 0.55);
    let stop = design_ratio.max(test_ratio).max(capability_ratio) * 1.55;
    assert_eq!(result.curves[0].water_to_dry_air_ratio, start);
    assert_eq!(
        result.curves[79].water_to_dry_air_ratio,
        start + (stop - start)
    );
    // The characteristic ordinate follows `coefficient * L/G ** exponent` exactly.
    for point in &result.curves {
        let expected = result.test_characteristic_coefficient
            * point
                .water_to_dry_air_ratio
                .powf(result.characteristic_exponent);
        assert_eq!(point.test_characteristic_merkel, expected);
    }
}

#[test]
fn a_case_with_a_pinch_in_the_curve_window_drops_the_infeasible_points() {
    // A design condition whose demand pinches inside the sampling window loses those curve
    // points to the reference's `Number.isFinite` filter; 80 sampled points come back as 78
    // (measured in the reference for this pair of conditions).
    let design = CapabilityCondition::new(200.0, 133.3333333, 42.0, 32.5, 27.0, 33.0);
    let test = CapabilityCondition::new(200.0, 133.3333333, 42.0, 32.0, 27.0, 33.0);
    let result = evaluate_characteristic_capability(&capability_input(design, test))
        .expect("the projection itself evaluates");
    assert_eq!(result.curves.len(), 78);
    assert!(result
        .curves
        .iter()
        .all(|point| point.design_demand_merkel.is_finite()));
    // The dropped points are the high-L/G end: the surviving window starts where it started.
    assert_eq!(
        result.curves[0].water_to_dry_air_ratio,
        0.05_f64.max(design.water_mass_flow_kg_s / design.dry_air_mass_flow_kg_s * 0.55)
    );
}

/* ---------------- the seeded stream ---------------- */

#[test]
fn the_seeded_generator_draws_the_javascript_stream() {
    // Measured in the reference engine itself (`createSeededRandom(17)`): the generator is
    // 32-bit wrapping arithmetic, so these draws are exact, not approximate.
    let expected = [
        0.6771502960473299_f64,
        0.19265692122280598,
        0.5313839064911008,
        0.1654723146930337,
        0.08778517786413431,
    ];
    let mut random = create_seeded_random(17.0);
    for (index, value) in expected.iter().enumerate() {
        assert_eq!(random.next_value(), *value, "draw {index}");
    }
    // `gaussianRandom` over that stream (measured in the reference in the same place).
    let mut random = create_seeded_random(17.0);
    let expected_gaussian = [
        0.31131072930854836_f64,
        0.5695513541368098,
        0.14988441715048012,
    ];
    for (index, value) in expected_gaussian.iter().enumerate() {
        let draw = gaussian_random(&mut || random.next_value());
        let scale = value.abs().max(1.0);
        assert!(
            (draw - value).abs() / scale < 1e-12,
            "gaussian {index}: got {draw}, reference {value}"
        );
    }
}

#[test]
fn percentile_matches_the_reference_definition() {
    // `percentile([3, 1, 4, 1, 5, 9, 2, 6], p)` in the reference: sorted, position clamped,
    // then linearly interpolated between the neighbours.
    let values = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
    assert_eq!(percentile(&values, 0.025), 1.0);
    assert_eq!(percentile(&values, 0.5), 3.5);
    assert_eq!(percentile(&values, 0.975), 8.475000000000001);
    assert!(percentile(&[], 0.5).is_nan());
    // A position outside [0, 1] clamps to the ends.
    assert_eq!(percentile(&values, -1.0), 1.0);
    assert_eq!(percentile(&values, 2.0), 9.0);
}

/* ---------------- Monte Carlo ---------------- */

/// The uncertainty of `tests/capability.test.js`: five perturbed test fields, in its order.
fn documented_uncertainty() -> CapabilityUncertainty {
    CapabilityUncertainty {
        design: Vec::new(),
        test: vec![
            (ConditionField::WaterMassFlowKgS, 1.0),
            (ConditionField::DryAirMassFlowKgS, 1.0),
            (ConditionField::HotWaterC, 0.03),
            (ConditionField::ColdWaterC, 0.03),
            (ConditionField::WetBulbC, 0.05),
        ],
        characteristic_exponent: None,
        salinity_g_kg: None,
    }
}

#[test]
fn a_seeded_monte_carlo_is_repeatable() {
    // tests/capability.test.js: the same input twice gives the same p50, and p2_5 < p97_5.
    let input = MonteCarloInput::new(
        capability_input(condition(), condition()),
        documented_uncertainty(),
    )
    .with_samples(80)
    .with_seed(17.0);
    let first = monte_carlo_characteristic_capability(&input).expect("runs");
    let second = monte_carlo_characteristic_capability(&input).expect("runs again");
    assert_eq!(first.p50, second.p50);
    assert!(first.p2_5 < first.p97_5);
    // Every sample is either accepted or rejected, and the seed is echoed.
    assert_eq!(first.samples_requested, 80);
    assert_eq!(first.samples_accepted + first.rejected_samples, 80);
    assert_eq!(first.seed, 17.0);
    assert_eq!(
        first.expanded_uncertainty_approx_pct_points,
        2.0 * first.standard_deviation_pct_points
    );
}

#[test]
fn a_different_seed_draws_a_different_sample_set() {
    let base = MonteCarloInput::new(
        capability_input(condition(), condition()),
        documented_uncertainty(),
    )
    .with_samples(80);
    let first = monte_carlo_characteristic_capability(&base.clone().with_seed(17.0)).expect("runs");
    let second = monte_carlo_characteristic_capability(&base.with_seed(18.0)).expect("runs");
    assert_ne!(first.p50, second.p50);
    assert_eq!(first.samples_accepted, second.samples_accepted);
}

#[test]
fn a_run_that_cannot_clear_the_acceptance_floor_is_refused() {
    // The reference refuses when fewer than `max(20, samples * 0.5)` samples are accepted, so
    // a request for ten samples can never clear its own floor.
    let input = MonteCarloInput::new(
        capability_input(condition(), condition()),
        documented_uncertainty(),
    )
    .with_samples(10);
    let error = monte_carlo_characteristic_capability(&input).expect_err("must refuse");
    assert_eq!(
        message(&error),
        "Too many Monte Carlo samples were invalid; review uncertainty inputs."
    );
}

#[test]
fn the_perturbation_order_decides_which_field_draws_which_gaussian() {
    // The reference walks `Object.entries(standardUncertainty)`: two fields whose sigmas are
    // swapped between orders produce different samples, so the order is part of the contract.
    let swapped = CapabilityUncertainty {
        design: Vec::new(),
        test: vec![
            (ConditionField::WetBulbC, 0.05),
            (ConditionField::ColdWaterC, 0.03),
        ],
        characteristic_exponent: None,
        salinity_g_kg: None,
    };
    let base = CharacteristicCapabilityInput::new(condition(), condition());
    let first = monte_carlo_characteristic_capability(
        &MonteCarloInput::new(base, documented_uncertainty()).with_samples(40),
    )
    .expect("runs");
    let second = monte_carlo_characteristic_capability(
        &MonteCarloInput::new(
            base,
            CapabilityUncertainty {
                design: Vec::new(),
                test: vec![
                    (ConditionField::WaterMassFlowKgS, 1.0),
                    (ConditionField::DryAirMassFlowKgS, 1.0),
                    (ConditionField::HotWaterC, 0.03),
                    (ConditionField::ColdWaterC, 0.03),
                    (ConditionField::WetBulbC, 0.05),
                ],
                characteristic_exponent: None,
                salinity_g_kg: None,
            },
        )
        .with_samples(40),
    )
    .expect("runs");
    assert_eq!(first.p50, second.p50);
    let reordered = monte_carlo_characteristic_capability(
        &MonteCarloInput::new(base, swapped).with_samples(40),
    )
    .expect("runs");
    assert_ne!(first.p50, reordered.p50);
}

#[test]
fn the_uncertainty_walks_both_conditions_and_the_optional_scalars() {
    let uncertainty = CapabilityUncertainty {
        design: vec![(ConditionField::ColdWaterC, 0.02)],
        test: vec![(ConditionField::WetBulbC, 0.05)],
        characteristic_exponent: Some(0.01),
        salinity_g_kg: None,
    };
    let result = monte_carlo_characteristic_capability(
        &MonteCarloInput::new(
            CharacteristicCapabilityInput::new(condition(), condition())
                .with_integration(Integration::Chebyshev4),
            uncertainty,
        )
        .with_samples(40)
        .with_seed(7.0),
    )
    .expect("runs");
    assert_eq!(result.samples_requested, 40);
    assert!(result.samples_accepted >= 20);
}
