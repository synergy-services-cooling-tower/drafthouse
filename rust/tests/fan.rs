//! Fans — the JavaScript suite's expectations for `src/core/fan.js`, ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `tests/airside.test.js` — the balanced-operating-point expectation (the fan/system
//!   solver is exercised there, not in a file of its own).
//! * `validation/test-vectors.json` (family `fan`) — the three recorded operating points,
//!   reproduced against the same `35 + 0.0085 Q²` system curve and 1.14 kg/m³ air,
//!   at the same 1e-6 relative tolerance `tests/vectors.test.js` uses.
//! * `src/core/fan.js` — the formulas, defaults (`speedRatio: 1`,
//!   `minimumFlowFraction: 0.05`, the IEC `standardSizesKW` table) and guard messages.
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    choose_standard_motor, estimate_airflow_from_fan_power, fan_efficiency_at_flow,
    fan_operating_limits, fan_pressure_pa_at_flow, fan_shaft_power_kw, fan_speed_ratio_limit,
    solve_fan_system_intersection, AirflowEstimateInput, FanCurvePoint, FanRecord,
    FanSpeedRatioLimit, FanSystemIntersection, FanSystemIntersectionInput, SelectionFan,
    PRESSURE_AFFINITY_EXPONENT, SHAFT_POWER_AFFINITY_EXPONENT,
};

/// `sampleCatalog.fans` / `AX-500` (illustrative synthetic data, quoted verbatim).
fn ax_500() -> FanRecord {
    FanRecord {
        id: "AX-500".to_string(),
        stack_area_m2: Some(19.635),
        pressure_basis: None,
        reference_density_kg_m3: Some(1.2),
        stack_recovery_factor: None,
        curve: vec![
            FanCurvePoint {
                flow_m3_s: 55.0,
                pressure_pa: 520.0,
                efficiency: 0.63,
            },
            FanCurvePoint {
                flow_m3_s: 100.0,
                pressure_pa: 470.0,
                efficiency: 0.74,
            },
            FanCurvePoint {
                flow_m3_s: 145.0,
                pressure_pa: 380.0,
                efficiency: 0.83,
            },
            FanCurvePoint {
                flow_m3_s: 185.0,
                pressure_pa: 245.0,
                efficiency: 0.82,
            },
            FanCurvePoint {
                flow_m3_s: 225.0,
                pressure_pa: 70.0,
                efficiency: 0.67,
            },
        ],
    }
}

/// The `35 + 0.0085 Q²` system curve of `tests/airside.test.js` and the recorded vectors.
fn documented_system_curve(flow_m3_s: f64) -> f64 {
    35.0 + 0.0085 * flow_m3_s.powi(2)
}

fn solve(fan: &FanRecord, air_density_kg_m3: f64, speed_ratio: f64) -> FanSystemIntersection {
    solve_fan_system_intersection(
        &FanSystemIntersectionInput::new(fan, air_density_kg_m3).with_speed_ratio(speed_ratio),
        |flow_m3_s| Ok(documented_system_curve(flow_m3_s)),
    )
    .unwrap()
}

fn agrees(actual: f64, expected: f64, label: &str) {
    let scale = expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() / scale < 1e-6,
        "{label}: got {actual}, vector says {expected}"
    );
}

/* ---------------- recorded vectors ---------------- */

/// `validation/test-vectors.json`, family `fan`.
#[test]
fn the_recorded_fan_vectors_reproduce() {
    let fan = ax_500();
    let cases: [(f64, [f64; 4]); 3] = [
        (0.78, [129.727648, 178.048732, 0.824671, 28.008568]),
        (0.90, [151.814388, 230.904670, 0.824079, 42.537957]),
        (1.00, [170.031780, 280.741854, 0.823742, 57.949011]),
    ];
    for (speed_ratio, expected) in cases {
        let result = solve(&fan, 1.14, speed_ratio);
        agrees(
            result.flow_m3_s,
            expected[0],
            &format!("{speed_ratio} flowM3S"),
        );
        agrees(
            result.fan_pressure_pa,
            expected[1],
            &format!("{speed_ratio} fanPressurePa"),
        );
        agrees(
            result.efficiency,
            expected[2],
            &format!("{speed_ratio} efficiency"),
        );
        agrees(
            result.shaft_power_kw,
            expected[3],
            &format!("{speed_ratio} shaftPowerKW"),
        );
        // The reference reports the residual at the returned point; it must vanish.
        agrees(
            result.fan_pressure_pa - result.system_pressure_pa,
            result.residual_pa,
            &format!("{speed_ratio} residualPa"),
        );
        assert!(
            result.residual_pa.abs() < 1e-6,
            "residual {} must vanish",
            result.residual_pa
        );
    }
}

/* ---------------- tests/airside.test.js, ported ---------------- */

#[test]
fn the_fan_system_solver_returns_a_balanced_positive_operating_point() {
    let fan = ax_500();
    let result = solve_fan_system_intersection(
        &FanSystemIntersectionInput::new(&fan, 1.15).with_speed_ratio(0.9),
        |flow_m3_s| Ok(documented_system_curve(flow_m3_s)),
    )
    .unwrap();
    assert!(result.flow_m3_s > 0.0);
    assert!(result.shaft_power_kw > 0.0);
    assert!(result.residual_pa.abs() < 1e-4);
}

/* ---------------- curve evaluation ---------------- */

#[test]
fn operating_limits_scale_with_the_speed_ratio_and_are_not_validated_there() {
    let fan = ax_500();
    let limits = fan_operating_limits(&fan, 1.0).unwrap();
    assert_eq!(limits.min_flow_m3_s, 55.0);
    assert_eq!(limits.max_flow_m3_s, 225.0);
    let half = fan_operating_limits(&fan, 0.78).unwrap();
    assert_eq!(half.min_flow_m3_s, 42.9);
    assert_eq!(half.max_flow_m3_s, 175.5);
}

#[test]
fn a_curve_reading_uses_the_density_ratio_and_the_square_of_the_speed_ratio() {
    let fan = ax_500();
    // Values read from the reference at this point (tests/airside.test.js uses the same call).
    let pressure = fan_pressure_pa_at_flow(&fan, 130.0, 0.9, 1.14).unwrap();
    assert!((pressure - 293.265).abs() < 1e-9, "{pressure}");
    // The reference reads the curve at flow / speedRatio and scales by the affinity laws.
    let reference_flow = 130.0 / 0.9;
    let reference_pressure = 470.0 + (reference_flow - 100.0) / 45.0 * (380.0 - 470.0);
    assert!(
        (pressure - reference_pressure * (1.14 / 1.2) * 0.9f64.powi(2)).abs() < 1e-9,
        "{pressure}"
    );
}

#[test]
fn an_efficiency_reading_clamps_at_the_curve_ends() {
    let fan = ax_500();
    // Values read from the reference. 130/0.9 = 144.44… m³/s falls between the 100 and 145
    // points; a flow beyond the curve clamps to the last efficiency rather than extrapolating
    // (the pressure curve extrapolates — `clampEnds: false` — the efficiency curve does not).
    assert!((fan_efficiency_at_flow(&fan, 130.0, 0.9).unwrap() - 0.8288888888888888).abs() < 1e-12);
    assert_eq!(fan_efficiency_at_flow(&fan, 9999.0, 1.0).unwrap(), 0.67);
    assert_eq!(fan_efficiency_at_flow(&fan, 1.0, 1.0).unwrap(), 0.63);
}

#[test]
fn shaft_power_is_flow_times_pressure_over_efficiency() {
    let fan = ax_500();
    let shaft_power = fan_shaft_power_kw(&fan, 130.0, 200.0, 0.9).unwrap();
    assert!(
        (shaft_power - 31.367292225201076).abs() < 1e-9,
        "{shaft_power}"
    );
    let efficiency = fan_efficiency_at_flow(&fan, 130.0, 0.9).unwrap();
    assert_eq!(shaft_power, 130.0 * 200.0 / efficiency / 1000.0);
}

/* ---------------- affinity estimate and motor selection ---------------- */

#[test]
fn an_airflow_estimate_follows_the_affinity_law() {
    let estimate = estimate_airflow_from_fan_power(AirflowEstimateInput {
        reference_volumetric_flow_m3_s: 150.0,
        reference_fan_power_kw: 40.0,
        actual_fan_power_kw: 52.0,
        reference_air_density_kg_m3: 1.2,
        actual_air_density_kg_m3: 1.14,
    })
    .unwrap();
    // Value read from the reference.
    assert!((estimate - 166.5320550502368).abs() < 1e-9, "{estimate}");
    // Same power and density is the identity; more power at the same density raises the flow.
    let identity = estimate_airflow_from_fan_power(AirflowEstimateInput {
        reference_volumetric_flow_m3_s: 150.0,
        reference_fan_power_kw: 40.0,
        actual_fan_power_kw: 40.0,
        reference_air_density_kg_m3: 1.2,
        actual_air_density_kg_m3: 1.2,
    })
    .unwrap();
    assert_eq!(identity, 150.0);
    assert!(estimate > 150.0);
}

#[test]
fn motor_selection_walks_the_iec_size_table() {
    // Values read from the reference (`chooseStandardMotor` defaults).
    let small = choose_standard_motor(0.1, 0.96, 1.1).unwrap();
    assert!((small.required_motor_output_kw - 0.11458333333333334).abs() < 1e-15);
    assert_eq!(small.selected_motor_kw, Some(0.75));

    let mid = choose_standard_motor(30.0, 0.96, 1.1).unwrap();
    assert_eq!(mid.required_motor_output_kw, 34.375);
    assert_eq!(mid.selected_motor_kw, Some(37.0));

    // Above the largest standard size the reference returns null (`?? null`).
    let large = choose_standard_motor(300.0, 0.97, 1.15).unwrap();
    assert!((large.required_motor_output_kw - 355.67010309278345).abs() < 1e-9);
    assert_eq!(large.selected_motor_kw, None);
    let at_the_top = choose_standard_motor(275.0, 0.96, 1.1).unwrap();
    assert!(at_the_top.required_motor_output_kw > 315.0);
    assert_eq!(at_the_top.selected_motor_kw, None);

    // The chosen size always covers the requirement, and the table is the reference's.
    assert_eq!(
        synergy_drafthouse::STANDARD_MOTOR_SIZES_KW,
        [
            0.75, 1.1, 1.5, 2.2, 3.0, 4.0, 5.5, 7.5, 11.0, 15.0, 18.5, 22.0, 30.0, 37.0, 45.0,
            55.0, 75.0, 90.0, 110.0, 132.0, 160.0, 200.0, 250.0, 315.0
        ]
    );
    for shaft_power_kw in [1.0, 5.0, 20.0, 100.0, 250.0] {
        let selection = choose_standard_motor(shaft_power_kw, 0.96, 1.1).unwrap();
        if let Some(size) = selection.selected_motor_kw {
            assert!(size >= selection.required_motor_output_kw);
        }
    }
}

/* ---------------- refusal behaviour ---------------- */

#[test]
fn a_fan_curve_with_fewer_than_two_points_is_refused() {
    for curve in [
        Vec::new(),
        vec![FanCurvePoint {
            flow_m3_s: 100.0,
            pressure_pa: 400.0,
            efficiency: 0.8,
        }],
    ] {
        let fan = FanRecord { curve, ..ax_500() };
        let error = fan_operating_limits(&fan, 1.0).expect_err("must refuse");
        assert_eq!(
            error.message(),
            "Fan must contain at least two curve points."
        );
        assert!(fan_pressure_pa_at_flow(&fan, 100.0, 1.0, 1.2).is_err());
        assert!(fan_efficiency_at_flow(&fan, 100.0, 1.0).is_err());
    }
}

#[test]
fn a_non_positive_reference_density_speed_ratio_or_air_density_is_refused() {
    let mut fan = ax_500();
    fan.reference_density_kg_m3 = Some(0.0);
    assert_eq!(
        fan_pressure_pa_at_flow(&fan, 100.0, 1.0, 1.2)
            .unwrap_err()
            .message(),
        "fan.referenceDensityKgM3 must be positive."
    );
    fan.reference_density_kg_m3 = None; // the reference default is 1.2, which is accepted
    assert!(fan_pressure_pa_at_flow(&fan, 100.0, 1.0, 1.2).is_ok());

    let fan = ax_500();
    assert_eq!(
        fan_pressure_pa_at_flow(&fan, 100.0, 0.0, 1.2)
            .unwrap_err()
            .message(),
        "speedRatio must be positive."
    );
    assert_eq!(
        fan_pressure_pa_at_flow(&fan, 100.0, 1.0, 0.0)
            .unwrap_err()
            .message(),
        "airDensityKgM3 must be positive."
    );
    // fanEfficiencyAtFlow does not assert the speed ratio in the reference either: a zero
    // ratio is an infinite reference flow, which clamps to the last efficiency.
    assert_eq!(fan_efficiency_at_flow(&fan, 100.0, 0.0).unwrap(), 0.67);
}

#[test]
fn a_non_positive_efficiency_is_refused() {
    let fan = FanRecord {
        curve: vec![
            FanCurvePoint {
                flow_m3_s: 55.0,
                pressure_pa: 520.0,
                efficiency: 0.0,
            },
            FanCurvePoint {
                flow_m3_s: 225.0,
                pressure_pa: 70.0,
                efficiency: 0.67,
            },
        ],
        ..ax_500()
    };
    let error = fan_shaft_power_kw(&fan, 55.0, 200.0, 1.0).expect_err("must refuse");
    assert_eq!(error.message(), "Fan efficiency must be positive.");
}

#[test]
fn a_system_curve_the_fan_never_meets_is_refused() {
    let fan = ax_500();
    // The reference scans `[max(minFlow, maxFlow * 0.05), maxFlow]` and refuses when no
    // bracketed root is found, rather than returning a clamp.
    let error =
        solve_fan_system_intersection(&FanSystemIntersectionInput::new(&fan, 1.14), |_flow_m3_s| {
            Ok(10_000.0)
        })
        .expect_err("an unreachable system curve must be refused");
    assert_eq!(
        error.message(),
        "No bracketed root was found in the requested interval."
    );
    // A system curve that cannot be evaluated is refused as well, from inside the scan.
    let error =
        solve_fan_system_intersection(&FanSystemIntersectionInput::new(&fan, 1.14), |_flow_m3_s| {
            Err(synergy_drafthouse::DomainError::new("no system curve"))
        })
        .expect_err("a failing system curve must propagate");
    assert_eq!(error.message(), "no system curve");
}

#[test]
fn non_positive_estimate_inputs_are_refused_and_named() {
    let base = AirflowEstimateInput {
        reference_volumetric_flow_m3_s: 150.0,
        reference_fan_power_kw: 40.0,
        actual_fan_power_kw: 52.0,
        reference_air_density_kg_m3: 1.2,
        actual_air_density_kg_m3: 1.14,
    };
    for (input, name) in [
        (
            AirflowEstimateInput {
                reference_volumetric_flow_m3_s: 0.0,
                ..base
            },
            "referenceVolumetricFlowM3S must be positive.",
        ),
        (
            AirflowEstimateInput {
                reference_fan_power_kw: -1.0,
                ..base
            },
            "referenceFanPowerKW must be positive.",
        ),
        (
            AirflowEstimateInput {
                actual_fan_power_kw: 0.0,
                ..base
            },
            "actualFanPowerKW must be positive.",
        ),
        (
            AirflowEstimateInput {
                reference_air_density_kg_m3: f64::NAN,
                ..base
            },
            "referenceAirDensityKgM3 must be a finite number.",
        ),
        (
            AirflowEstimateInput {
                actual_air_density_kg_m3: 0.0,
                ..base
            },
            "actualAirDensityKgM3 must be positive.",
        ),
    ] {
        let error = estimate_airflow_from_fan_power(input).expect_err("must refuse");
        assert_eq!(error.message(), name);
    }
    // A non-positive shaft power is refused by the motor chooser.
    assert_eq!(
        choose_standard_motor(0.0, 0.96, 1.1).unwrap_err().message(),
        "shaftPowerKW must be positive."
    );
}

/* ---------------- issue #59: the record's rated speed, the affinity legs and the named limit ---- */

/// The AX-500 record as the selector carries it: the physics record above, the fixture's own speed
/// window `[0.70, 1.13]`, and the rated speed its recorded curve is published at — 233 rpm, the
/// speed its own stack diameter gives at one blade tip speed across the fan family.
fn ax_500_selection() -> SelectionFan {
    SelectionFan {
        physics: ax_500(),
        allowed_speed_ratio: [0.70, 1.13],
        nominal_rpm: Some(233.0),
        drive_efficiency: 0.96,
        motor_efficiency: 0.95,
    }
}

#[test]
fn ratio_one_reproduces_the_recorded_curve_exactly() {
    let fan = ax_500();
    // The recorded curve **is** the ratio-1.0 curve: at ratio 1.0 every reading is the tabulated
    // point itself, bit for bit - no scaling, no interpolation, no clamp.
    for point in &fan.curve {
        assert_eq!(
            fan_pressure_pa_at_flow(&fan, point.flow_m3_s, 1.0, 1.2).unwrap(),
            point.pressure_pa,
            "pressure at {}",
            point.flow_m3_s
        );
        assert_eq!(
            fan_efficiency_at_flow(&fan, point.flow_m3_s, 1.0).unwrap(),
            point.efficiency,
            "efficiency at {}",
            point.flow_m3_s
        );
        assert_eq!(
            fan_shaft_power_kw(&fan, point.flow_m3_s, point.pressure_pa, 1.0).unwrap(),
            point.flow_m3_s * point.pressure_pa / point.efficiency / 1000.0,
            "shaft power at {}",
            point.flow_m3_s
        );
    }
    let limits = fan_operating_limits(&fan, 1.0).unwrap();
    assert_eq!(limits.min_flow_m3_s, 55.0);
    assert_eq!(limits.max_flow_m3_s, 225.0);
}

#[test]
fn ratio_078_matches_the_fixtures_recorded_operating_point() {
    // The fixture's recorded run sits at speed ratio 0.78 on AX-500; its recorded operating point
    // is `validation/test-vectors.json`'s `fan` vector (the same numbers the cockpit's anchored run
    // carries), and the rpm behind it is the record's own rated speed times the ratio.
    let fan = ax_500();
    let result = solve(&fan, 1.14, 0.78);
    agrees(result.flow_m3_s, 129.727648, "0.78 flowM3S");
    agrees(result.fan_pressure_pa, 178.048732, "0.78 fanPressurePa");
    agrees(result.shaft_power_kw, 28.008568, "0.78 shaftPowerKW");
    let rpm = ax_500_selection()
        .rpm_at_speed_ratio(0.78)
        .expect("the record states a rated speed");
    assert!((rpm - 181.74).abs() < 1e-9, "{rpm}");
}

#[test]
fn the_power_leg_is_the_third_power_of_the_speed_ratio_from_the_recorded_curve() {
    let fan = ax_500();
    // The record stores no power: the engine computes it from the recorded curve, and along the
    // affinity family - one fixed reference flow - it carries the flow leg (n^1) times the pressure
    // leg (n^2): the third power. A compared value has to MOVE here for the mutation proof to mean
    // anything, so the assertion names the n^2 law it must not be.
    for reference_flow in [80.0, 130.0, 200.0] {
        let reference_pressure = fan_pressure_pa_at_flow(&fan, reference_flow, 1.0, 1.2).unwrap();
        let reference_power =
            fan_shaft_power_kw(&fan, reference_flow, reference_pressure, 1.0).unwrap();
        for ratio in [0.70, 0.78, 1.00, 1.13] {
            let flow = reference_flow * ratio;
            let pressure = fan_pressure_pa_at_flow(&fan, flow, ratio, 1.2).unwrap();
            let power = fan_shaft_power_kw(&fan, flow, pressure, ratio).unwrap();
            let expected = reference_power * ratio.powf(SHAFT_POWER_AFFINITY_EXPONENT);
            let relative = (power - expected).abs() / expected.abs().max(1e-12);
            assert!(
                relative < 1e-12,
                "flow {reference_flow} ratio {ratio}: power {power} != n^3 x {reference_power} = {expected}"
            );
            if ratio != 1.0 {
                let square_law = reference_power * ratio.powf(PRESSURE_AFFINITY_EXPONENT);
                assert!(
                    (power - square_law).abs() > 1e-6 * square_law.abs(),
                    "the power leg must not be the n^2 law"
                );
            }
        }
    }
}

#[test]
fn an_out_of_band_ratio_is_a_named_limit_and_the_rpm_is_the_records_own_datum() {
    let fan = ax_500_selection();
    // Inside the record's own band there is no limit to report.
    assert_eq!(fan.speed_ratio_limit(0.70), None);
    assert_eq!(fan.speed_ratio_limit(0.78), None);
    assert_eq!(fan.speed_ratio_limit(1.13), None);
    // Outside it the outcome is named - the fan, the ratio asked for, the record's band - never a
    // ratio clamped into the band and reported as if the engine had evaluated it.
    assert_eq!(
        fan.speed_ratio_limit(1.50),
        Some(FanSpeedRatioLimit {
            fan_id: "AX-500".to_string(),
            speed_ratio: 1.50,
            min: 0.70,
            max: 1.13,
        })
    );
    assert_eq!(
        fan.speed_ratio_limit(0.60),
        Some(FanSpeedRatioLimit {
            fan_id: "AX-500".to_string(),
            speed_ratio: 0.60,
            min: 0.70,
            max: 1.13,
        })
    );
    // The same check at the band level - the adapter's own call shape, no record rebuilt.
    assert!(fan_speed_ratio_limit("AX-500", [0.70, 1.13], 1.50).is_some());
    assert!(fan_speed_ratio_limit("AX-500", [0.70, 1.13], 1.00).is_none());

    // The rpm read-out is the record's datum times the ratio; a record without one has no rpm.
    assert!((fan.rpm_at_speed_ratio(1.0).unwrap() - 233.0).abs() < 1e-12);
    let mut no_datum = ax_500_selection();
    no_datum.nominal_rpm = None;
    assert_eq!(no_datum.rpm_at_speed_ratio(0.78), None);
    assert!(no_datum.speed_ratio_limit(1.50).is_some());
}
