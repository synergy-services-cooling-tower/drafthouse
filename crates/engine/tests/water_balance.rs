use drafthouse_engine::{
    cooling_tower_water_balance, drift_loss_kg_s, evaporation_from_air_mass_balance,
    psychrometric_state, PsychrometricStateInput, WaterBalanceInput,
};
use serde_json::Value;

const VECTOR_TOLERANCE: f64 = 1e-6;

fn agrees(actual: f64, expected: f64, label: &str) {
    let scale = expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() / scale < VECTOR_TOLERANCE,
        "{label}: got {actual}, vector says {expected}"
    );
}

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../validation/test-vectors.json"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {path}: {error}"))
}

fn number(value: &Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key} is not a number in {value}"))
}

#[test]
fn the_recorded_water_balance_vectors_reproduce() {
    let file = vectors();
    for case in file["families"]["waterBalance"].as_array().unwrap() {
        let inputs = &case["inputs"];
        let id = case["id"].as_str().expect("id");
        let inlet = psychrometric_state(PsychrometricStateInput::from_wet_bulb(
            number(inputs, "inletDb"),
            number(inputs, "inletWb"),
        ))
        .unwrap();
        let evaporation_kg_s = evaporation_from_air_mass_balance(
            number(inputs, "dryAirMassFlowKgS"),
            inlet.humidity_ratio,
            number(inputs, "outletHumidityRatio"),
        )
        .unwrap();
        let drift_kg_s = drift_loss_kg_s(
            number(inputs, "circulatingWaterMassFlowKgS"),
            number(inputs, "driftPpm"),
        )
        .unwrap();
        let balance = cooling_tower_water_balance(
            &WaterBalanceInput::new(evaporation_kg_s)
                .with_drift_kg_s(drift_kg_s)
                .with_cycles_of_concentration(number(inputs, "cyclesOfConcentration")),
        )
        .unwrap();
        let expected = &case["expected"];
        let fields = [
            ("evaporationKgS", balance.evaporation_kg_s),
            ("driftKgS", balance.drift_kg_s),
            ("blowdownKgS", balance.blowdown_kg_s),
            ("makeupKgS", balance.makeup_kg_s),
        ];
        for (label, actual) in fields {
            agrees(actual, number(expected, label), &format!("{id} {label}"));
        }
    }
}

#[test]
fn water_balance_conserves_makeup_equals_evaporation_plus_drift_plus_blowdown() {
    let drift_kg_s = drift_loss_kg_s(200.0, 10.0).unwrap();
    let result = cooling_tower_water_balance(
        &WaterBalanceInput::new(4.5)
            .with_drift_kg_s(drift_kg_s)
            .with_cycles_of_concentration(4.0),
    )
    .unwrap();
    assert!(
        (result.makeup_kg_s - (result.evaporation_kg_s + result.drift_kg_s + result.blowdown_kg_s))
            .abs()
            < 1e-12
    );
    assert_eq!(drift_kg_s, 0.002);
}

#[test]
fn the_worked_water_balance_of_the_design_cell_closes() {
    const OUTLET_HUMIDITY_RATIO: f64 = 0.042_764_593_386_655_496;
    let inlet = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    assert_eq!(inlet.humidity_ratio, 0.020_226_399_002_504_4);
    let evaporation_kg_s =
        evaporation_from_air_mass_balance(200.0 / 1.5, inlet.humidity_ratio, OUTLET_HUMIDITY_RATIO)
            .unwrap();
    agrees(
        evaporation_kg_s,
        3.005_092_584_553_479_7,
        "worked evaporationKgS",
    );
    let balance = cooling_tower_water_balance(
        &WaterBalanceInput::new(evaporation_kg_s)
            .with_drift_kg_s(drift_loss_kg_s(200.0, 10.0).unwrap())
            .with_cycles_of_concentration(4.0),
    )
    .unwrap();
    agrees(
        balance.blowdown_kg_s,
        0.999_697_528_184_493_3,
        "worked blowdownKgS",
    );
    agrees(
        balance.makeup_kg_s,
        4.006_790_112_737_972,
        "worked makeupKgS",
    );
    let evaporation_pct = 100.0 * balance.evaporation_kg_s / 200.0;
    assert!(
        evaporation_pct > 1.0 && evaporation_pct < 2.5,
        "evaporation is {evaporation_pct} % of circulating flow"
    );
}

#[test]
fn the_reference_default_arguments_apply_when_the_field_is_absent() {
    let defaults = WaterBalanceInput::new(4.5);
    assert_eq!(defaults.drift_kg_s, 0.0);
    assert_eq!(defaults.cycles_of_concentration, 4.0);
    let balance = cooling_tower_water_balance(&defaults).unwrap();
    assert_eq!(balance.evaporation_kg_s, 4.5);
    assert_eq!(balance.drift_kg_s, 0.0);
    assert_eq!(balance.blowdown_kg_s, 1.5);
    assert_eq!(balance.makeup_kg_s, 6.0);
    let explicit = cooling_tower_water_balance(
        &WaterBalanceInput::new(4.5)
            .with_drift_kg_s(0.0)
            .with_cycles_of_concentration(4.0),
    )
    .unwrap();
    assert_eq!(balance, explicit);
    let drift_only =
        cooling_tower_water_balance(&WaterBalanceInput::new(4.5).with_drift_kg_s(0.002)).unwrap();
    assert_eq!(drift_only.drift_kg_s, 0.002);
    assert_eq!(drift_only.cycles_of_concentration, 4.0);
    let cycles_only =
        cooling_tower_water_balance(&WaterBalanceInput::new(4.5).with_cycles_of_concentration(8.0))
            .unwrap();
    assert_eq!(cycles_only.drift_kg_s, 0.0);
    assert_eq!(cycles_only.blowdown_kg_s, 4.5 / 7.0);
    assert_eq!(cycles_only.makeup_kg_s, 4.5 + 4.5 / 7.0);
}

#[test]
fn blowdown_is_floored_at_zero_while_makeup_still_sums_the_losses() {
    let balance = cooling_tower_water_balance(
        &WaterBalanceInput::new(0.5)
            .with_drift_kg_s(1.0)
            .with_cycles_of_concentration(4.0),
    )
    .unwrap();
    assert_eq!(balance.blowdown_kg_s, 0.0);
    assert_eq!(balance.makeup_kg_s, 1.5);
}

#[test]
fn the_floors_do_not_swallow_a_nan() {
    let condensed = evaporation_from_air_mass_balance(1.0, 0.02, 0.01).unwrap();
    assert_eq!(condensed, 0.0);
    assert!(condensed.is_sign_positive());
    let balance = cooling_tower_water_balance(
        &WaterBalanceInput::new(4.5).with_cycles_of_concentration(f64::NAN),
    )
    .unwrap();
    assert!(balance.blowdown_kg_s.is_nan());
    assert!(balance.makeup_kg_s.is_nan());
}

#[test]
fn a_non_positive_or_non_finite_dry_air_mass_flow_is_refused() {
    for (flow, message) in [
        (0.0, "dryAirMassFlowKgS must be positive."),
        (-1.0, "dryAirMassFlowKgS must be positive."),
        (f64::NAN, "dryAirMassFlowKgS must be a finite number."),
        (f64::INFINITY, "dryAirMassFlowKgS must be a finite number."),
    ] {
        let error = evaporation_from_air_mass_balance(flow, 0.01, 0.02).expect_err("must refuse");
        assert_eq!(error.message(), message);
    }
}

#[test]
fn a_non_positive_circulating_flow_is_refused_before_the_drift_ppm_check() {
    let error = drift_loss_kg_s(0.0, -1.0).expect_err("must refuse");
    assert_eq!(
        error.message(),
        "circulatingWaterMassFlowKgS must be positive."
    );
    assert_eq!(
        drift_loss_kg_s(f64::NAN, 10.0).unwrap_err().message(),
        "circulatingWaterMassFlowKgS must be a finite number."
    );
    assert_eq!(
        drift_loss_kg_s(200.0, -1.0).unwrap_err().message(),
        "Drift ppm cannot be negative."
    );
    assert_eq!(drift_loss_kg_s(200.0, 0.0).unwrap(), 0.0);
    assert_eq!(drift_loss_kg_s(200.0, 10.0).unwrap(), 0.002);
}

#[test]
fn cycles_of_concentration_at_or_below_one_is_refused_first() {
    for cycles in [1.0, 0.5, 0.0, -4.0] {
        let error = cooling_tower_water_balance(
            &WaterBalanceInput::new(4.5).with_cycles_of_concentration(cycles),
        )
        .expect_err("must refuse");
        assert_eq!(error.message(), "Cycles of concentration must exceed 1.");
    }
    let error = cooling_tower_water_balance(
        &WaterBalanceInput::new(f64::NAN).with_cycles_of_concentration(1.0),
    )
    .expect_err("must refuse");
    assert_eq!(error.message(), "Cycles of concentration must exceed 1.");
}

#[test]
fn a_non_finite_or_negative_humidity_ratio_is_refused_while_zero_is_not() {
    for ratio in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            evaporation_from_air_mass_balance(10.0, ratio, 0.02)
                .unwrap_err()
                .message(),
            "inletHumidityRatio must be a finite number.",
            "inlet humidity ratio {ratio}"
        );
        assert_eq!(
            evaporation_from_air_mass_balance(10.0, 0.02, ratio)
                .unwrap_err()
                .message(),
            "outletHumidityRatio must be a finite number.",
            "outlet humidity ratio {ratio}"
        );
    }
    for ratio in [-0.01, -1.0] {
        assert_eq!(
            evaporation_from_air_mass_balance(10.0, ratio, 0.02)
                .unwrap_err()
                .message(),
            "inletHumidityRatio must be non-negative.",
            "inlet humidity ratio {ratio}"
        );
        assert_eq!(
            evaporation_from_air_mass_balance(10.0, 0.02, ratio)
                .unwrap_err()
                .message(),
            "outletHumidityRatio must be non-negative.",
            "outlet humidity ratio {ratio}"
        );
    }
    agrees(
        evaporation_from_air_mass_balance(10.0, 0.0, 0.02).unwrap(),
        0.2,
        "dry air to 0.02",
    );
    assert_eq!(
        evaporation_from_air_mass_balance(10.0, 0.02, 0.0).unwrap(),
        0.0
    );
    assert_eq!(
        evaporation_from_air_mass_balance(0.0, f64::NAN, 0.02)
            .unwrap_err()
            .message(),
        "dryAirMassFlowKgS must be positive."
    );
}

#[test]
fn a_non_finite_drift_ppm_is_refused_while_zero_is_not() {
    for ppm in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            drift_loss_kg_s(200.0, ppm).unwrap_err().message(),
            "driftPpm must be a finite number.",
            "drift ppm {ppm}"
        );
    }
    assert_eq!(
        drift_loss_kg_s(200.0, -1.0).unwrap_err().message(),
        "Drift ppm cannot be negative."
    );
    assert_eq!(drift_loss_kg_s(200.0, 0.0).unwrap(), 0.0);
    assert_eq!(drift_loss_kg_s(200.0, 10.0).unwrap(), 0.002);
}

#[test]
fn a_negative_or_non_finite_balance_loss_is_refused_while_zero_is_not() {
    for evaporation in [-5.0, -0.001] {
        assert_eq!(
            cooling_tower_water_balance(&WaterBalanceInput::new(evaporation))
                .unwrap_err()
                .message(),
            "evaporationKgS must be non-negative.",
            "evaporation {evaporation}"
        );
    }
    for evaporation in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            cooling_tower_water_balance(&WaterBalanceInput::new(evaporation))
                .unwrap_err()
                .message(),
            "evaporationKgS must be a finite number.",
            "evaporation {evaporation}"
        );
    }
    assert_eq!(
        cooling_tower_water_balance(&WaterBalanceInput::new(-5.0).with_drift_kg_s(-1.0))
            .unwrap_err()
            .message(),
        "evaporationKgS must be non-negative."
    );
    assert_eq!(
        cooling_tower_water_balance(&WaterBalanceInput::new(4.5).with_drift_kg_s(-1.0))
            .unwrap_err()
            .message(),
        "driftKgS must be non-negative."
    );
    assert_eq!(
        cooling_tower_water_balance(&WaterBalanceInput::new(4.5).with_drift_kg_s(f64::NAN))
            .unwrap_err()
            .message(),
        "driftKgS must be a finite number."
    );

    let no_load = cooling_tower_water_balance(&WaterBalanceInput::new(0.0)).unwrap();
    assert_eq!(no_load.blowdown_kg_s, 0.0);
    assert_eq!(no_load.makeup_kg_s, 0.0);
    let zero_drift =
        cooling_tower_water_balance(&WaterBalanceInput::new(4.5).with_drift_kg_s(0.0)).unwrap();
    assert_eq!(zero_drift.blowdown_kg_s, 1.5);
    assert_eq!(zero_drift.makeup_kg_s, 6.0);
    let drift_only = cooling_tower_water_balance(
        &WaterBalanceInput::new(0.0).with_drift_kg_s(drift_loss_kg_s(200.0, 10.0).unwrap()),
    )
    .unwrap();
    assert_eq!(drift_only.blowdown_kg_s, 0.0);
    assert_eq!(drift_only.makeup_kg_s, 0.002);
}
