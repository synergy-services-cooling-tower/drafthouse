//! Recorded regression vectors — the ported families of `validation/test-vectors.json`.
//!
//! Mirrors `tests/vectors.test.js`. The vector file declares itself **unvalidated**: it
//! records what the reference engine computes, not physical truth. This test only proves the
//! Rust port reproduces it at the same 1e-6 relative tolerance the JavaScript suite uses.
//!
//! The file's `naturalDraft` and `performanceCurve` families were added by issue #40 slice 1
//! (additions only; every pre-existing entry is byte-unchanged) together with the extra
//! `capability` case, so the three concerns that had no Rust counterpart are now checked
//! against recorded vectors as well as against the live JavaScript engine.

use serde_json::Value;
use synergy_drafthouse::{
    evaluate_characteristic_capability, evaluate_performance_curve_capability, merkel_demand,
    performance_curve_bounds, predict_cold_water_from_performance_curves,
    predict_water_flow_from_performance_curves, psychrometric_state,
    saturated_air_enthalpy_kj_kg_dry_air, saturation_humidity_ratio, saturation_vapor_pressure_pa,
    solve_cold_water_temperature, solve_natural_draft_counterflow, CapabilityCondition,
    CharacteristicCapabilityInput, ColdWaterTemperatureInput, DriftCurvePoint,
    DriftEliminatorRecord, FillLimits, FillPressureCorrelation, FillRecord,
    InletEnthalpyConvention, Integration, MerkelInput, NaturalDraftInput, PerformanceCurveRecord,
    PsychrometricOptions, PsychrometricStateInput, TowerRecord, ZoneCorrelation,
};

/// The tolerance `tests/vectors.test.js` uses.
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
        "/../validation/test-vectors.json"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {path}: {error}"))
}

fn number(value: &Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key} is not a number in {value}"))
}

fn id(case: &Value) -> &str {
    case["id"].as_str().expect("id")
}

#[test]
fn the_vector_file_declares_itself_unvalidated() {
    // Guards against someone quietly promoting regression vectors to "reference data".
    let file = vectors();
    assert!(file["status"].as_str().unwrap().contains("UNVALIDATED"));
    assert!(file["warning"].as_str().unwrap().contains("CTI ToolKit"));
}

#[test]
fn psychrometric_vectors_reproduce() {
    let file = vectors();
    let options = PsychrometricOptions::default();
    for case in file["families"]["psychrometrics"].as_array().unwrap() {
        let dry_bulb_c = number(case, "dryBulbC");
        let pressure_pa = number(case, "pressurePa");
        let expected = &case["expected"];
        let state = psychrometric_state(
            PsychrometricStateInput::from_wet_bulb(dry_bulb_c, number(case, "wetBulbC"))
                .with_pressure(pressure_pa),
        )
        .unwrap();
        let id = id(case);
        agrees(
            state.humidity_ratio,
            number(expected, "humidityRatio"),
            &format!("{id} humidityRatio"),
        );
        agrees(
            state.relative_humidity,
            number(expected, "relativeHumidity"),
            &format!("{id} relativeHumidity"),
        );
        agrees(
            state.enthalpy_kj_kg_dry_air,
            number(expected, "enthalpyKJkgDryAir"),
            &format!("{id} enthalpy"),
        );
        agrees(
            state.dew_point_c,
            number(expected, "dewPointC"),
            &format!("{id} dewPoint"),
        );
        agrees(
            state.moist_air_density_kg_m3,
            number(expected, "moistAirDensityKgM3"),
            &format!("{id} moistAirDensity"),
        );
        agrees(
            saturation_vapor_pressure_pa(dry_bulb_c).unwrap(),
            number(expected, "saturationVaporPressurePa"),
            &format!("{id} pws"),
        );
        agrees(
            saturation_humidity_ratio(dry_bulb_c, pressure_pa, options).unwrap(),
            number(expected, "saturationHumidityRatio"),
            &format!("{id} Ws"),
        );
        agrees(
            saturated_air_enthalpy_kj_kg_dry_air(dry_bulb_c, pressure_pa, options).unwrap(),
            number(expected, "saturatedAirEnthalpyKJkgDryAir"),
            &format!("{id} hs"),
        );
    }
}

/// The four recorded combinations of convention × quadrature.
const COMBINATIONS: [(InletEnthalpyConvention, Integration, &str); 4] = [
    (
        InletEnthalpyConvention::Bulk,
        Integration::Simpson,
        "bulk.simpson",
    ),
    (
        InletEnthalpyConvention::Bulk,
        Integration::Chebyshev4,
        "bulk.chebyshev4",
    ),
    (
        InletEnthalpyConvention::CtiSaturatedWetBulb,
        Integration::Simpson,
        "cti-saturated-wetbulb.simpson",
    ),
    (
        InletEnthalpyConvention::CtiSaturatedWetBulb,
        Integration::Chebyshev4,
        "cti-saturated-wetbulb.chebyshev4",
    ),
];

#[test]
fn merkel_vectors_reproduce_under_both_conventions_and_both_quadratures() {
    let file = vectors();
    for case in file["families"]["merkel"].as_array().unwrap() {
        let inputs = &case["inputs"];
        let pressure_pa = number(inputs, "pressurePa");
        let base = MerkelInput::new(
            number(inputs, "hotWaterC"),
            number(inputs, "coldWaterC"),
            number(inputs, "wetBulbC"),
            number(inputs, "dryBulbC"),
            number(inputs, "waterToDryAirRatio"),
        );
        let id = id(case);

        for (convention, integration, key) in COMBINATIONS {
            let input = MerkelInput {
                pressure_pa,
                integration,
                inlet_enthalpy_convention: convention,
                ..base
            };
            match case["expected"][key].as_f64() {
                Some(expected) => {
                    let actual = merkel_demand(&input).unwrap().merkel_number;
                    agrees(actual, expected, &format!("{id} {key}"));
                }
                // The vector records an infeasible duty; the port must refuse it too.
                None => assert!(
                    merkel_demand(&input).is_err(),
                    "{id} {key} should still be infeasible"
                ),
            }
        }

        if let Some(expected_inverse) = case["expected"]["inverse.coldWaterC"].as_f64() {
            let available = merkel_demand(&MerkelInput {
                pressure_pa,
                integration: Integration::Chebyshev4,
                ..base
            })
            .unwrap()
            .merkel_number;
            let back = solve_cold_water_temperature(&ColdWaterTemperatureInput {
                pressure_pa,
                available_merkel_number: available,
                integration: Integration::Chebyshev4,
                ..ColdWaterTemperatureInput::new(
                    base.hot_water_c,
                    base.wet_bulb_c,
                    base.dry_bulb_c,
                    base.water_to_dry_air_ratio,
                    available,
                )
            })
            .unwrap();
            agrees(
                back.cold_water_c,
                expected_inverse,
                &format!("{id} inverse"),
            );
        }
    }
}

#[test]
fn the_cti_convention_is_consistently_the_higher_demand() {
    // If this ever flips, the conventions have been swapped somewhere.
    let file = vectors();
    let mut compared = 0;
    for case in file["families"]["merkel"].as_array().unwrap() {
        let cti = case["expected"]["cti-saturated-wetbulb.chebyshev4"].as_f64();
        let bulk = case["expected"]["bulk.chebyshev4"].as_f64();
        if let (Some(cti), Some(bulk)) = (cti, bulk) {
            assert!(
                cti > bulk,
                "{}: CTI convention {cti} should exceed bulk {bulk}",
                id(case)
            );
            compared += 1;
        }
    }
    assert!(compared > 10, "only {compared} feasible cases compared");
}

/* ---------------- issue #40 slice 1: the three added families ---------------- */

/// An optional numeric key: absent means the reference's default argument applies.
fn number_or(value: &Value, key: &str, default: f64) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(default)
}

/// The `capability` family's condition objects.
fn vector_condition(value: &Value) -> CapabilityCondition {
    CapabilityCondition {
        water_mass_flow_kg_s: number(value, "waterMassFlowKgS"),
        dry_air_mass_flow_kg_s: number(value, "dryAirMassFlowKgS"),
        hot_water_c: number(value, "hotWaterC"),
        cold_water_c: number(value, "coldWaterC"),
        wet_bulb_c: number(value, "wetBulbC"),
        dry_bulb_c: number(value, "dryBulbC"),
        pressure_pa: value.get("pressurePa").and_then(Value::as_f64),
    }
}

#[test]
fn capability_vectors_reproduce() {
    let file = vectors();
    let mut compared = 0;
    for case in file["families"]["capability"].as_array().unwrap() {
        let inputs = &case["inputs"];
        let mut input = CharacteristicCapabilityInput::new(
            vector_condition(&inputs["design"]),
            vector_condition(&inputs["test"]),
        );
        if let Some(exponent) = inputs.get("characteristicExponent").and_then(Value::as_f64) {
            input = input.with_characteristic_exponent(exponent);
        }
        if let Some(salinity) = inputs.get("salinityGKg").and_then(Value::as_f64) {
            input = input.with_salinity_g_kg(salinity);
        }
        if let Some(convention) = inputs
            .get("inletEnthalpyConvention")
            .and_then(Value::as_str)
        {
            input = input.with_inlet_enthalpy_convention(match convention {
                "cti-saturated-wetbulb" => InletEnthalpyConvention::CtiSaturatedWetBulb,
                _ => InletEnthalpyConvention::Bulk,
            });
        }
        let expected = &case["expected"];
        let result = evaluate_characteristic_capability(&input)
            .unwrap_or_else(|error| panic!("{}: {error}", id(case)));
        let id = id(case);
        agrees(
            result.capability_pct,
            number(expected, "capabilityPct"),
            &format!("{id} capabilityPct"),
        );
        agrees(
            result.test_merkel_number,
            number(expected, "testMerkelNumber"),
            &format!("{id} testMerkelNumber"),
        );
        agrees(
            result.test_characteristic_coefficient,
            number(expected, "testCharacteristicCoefficient"),
            &format!("{id} testCharacteristicCoefficient"),
        );
        compared += 1;
    }
    assert!(compared >= 5, "only {compared} capability vectors compared");
}

/// `sampleCatalog.fills` / `FILM-VF38`.
fn vector_fill_vf38() -> FillRecord {
    FillRecord {
        id: "FILM-VF38".to_string(),
        thermal: Some(ZoneCorrelation {
            coefficient_per_m: 0.94,
            reference_water_loading_kg_m2_s: 3.0,
            reference_dry_air_loading_kg_m2_s: 2.0,
            water_exponent: -0.27,
            air_exponent: 0.39,
        }),
        pressure: Some(FillPressureCorrelation {
            coefficient_pa_per_m: 43.0,
            reference_water_loading_kg_m2_s: 3.0,
            reference_dry_air_loading_kg_m2_s: 2.0,
            water_exponent: 0.10,
            air_exponent: 1.67,
        }),
        limits: FillLimits {
            min_water_loading_kg_m2_s: 1.1,
            max_water_loading_kg_m2_s: 6.2,
            min_dry_air_loading_kg_m2_s: 0.8,
            max_dry_air_loading_kg_m2_s: 3.5,
            max_water_temperature_c: 75.0,
        },
        allowed_water_quality_classes: vec!["clean".to_string()],
    }
}

/// `sampleCatalog.towers` / `IDCF-064`.
fn vector_tower_idcf_064() -> TowerRecord {
    TowerRecord {
        fill_area_m2: 64.0,
        air_free_area_m2: Some(64.0),
        drift_area_m2: Some(61.0),
        inlet_area_m2: Some(32.0),
        plenum_area_m2: None,
        fan_stack_area_m2: None,
        stack_recovery_factor: Some(0.35),
        inlet_loss_coefficient: Some(3.0),
        distribution_loss_coefficient: Some(4.4),
        support_loss_coefficient: Some(2.1),
        plenum_loss_coefficient: Some(0.38),
        fixed_pressure_loss_pa: Some(10.0),
        spray_zone_height_m: Some(0.6),
        spray_zone: Some(ZoneCorrelation {
            coefficient_per_m: 0.16,
            reference_water_loading_kg_m2_s: 3.0,
            reference_dry_air_loading_kg_m2_s: 2.0,
            water_exponent: -0.30,
            air_exponent: 0.45,
        }),
        rain_zone_height_m: Some(1.5),
        rain_zone: Some(ZoneCorrelation {
            coefficient_per_m: 0.13,
            reference_water_loading_kg_m2_s: 3.0,
            reference_dry_air_loading_kg_m2_s: 2.0,
            water_exponent: -0.35,
            air_exponent: 0.50,
        }),
    }
}

/// `sampleCatalog.driftEliminators` / `DE-3P-10`.
fn vector_drift_de_3p_10() -> DriftEliminatorRecord {
    DriftEliminatorRecord {
        id: "DE-3P-10".to_string(),
        curve: [
            (1.0, 4.0, 10.0),
            (1.5, 6.0, 18.0),
            (2.0, 9.0, 31.0),
            (2.5, 14.0, 49.0),
            (3.0, 23.0, 72.0),
            (3.5, 38.0, 101.0),
        ]
        .into_iter()
        .map(
            |(face_velocity_ms, drift_ppm, pressure_drop_pa)| DriftCurvePoint {
                face_velocity_ms,
                drift_ppm,
                pressure_drop_pa,
            },
        )
        .collect(),
    }
}

#[test]
fn natural_draft_vectors_reproduce() {
    let file = vectors();
    let tower = vector_tower_idcf_064();
    let fill = vector_fill_vf38();
    let drift_eliminator = vector_drift_de_3p_10();
    for case in file["families"]["naturalDraft"].as_array().unwrap() {
        let inputs = &case["inputs"];
        let result = solve_natural_draft_counterflow(&NaturalDraftInput {
            tower: &tower,
            effective_draft_height_m: number(inputs, "effectiveDraftHeightM"),
            fill: &fill,
            fill_depth_m: number(inputs, "fillDepthM"),
            drift_eliminator: &drift_eliminator,
            hot_water_c: number(inputs, "hotWaterC"),
            dry_bulb_c: number(inputs, "dryBulbC"),
            wet_bulb_c: number(inputs, "wetBulbC"),
            pressure_pa: number(inputs, "pressurePa"),
            water_mass_flow_kg_s: number(inputs, "waterMassFlowKgS"),
            salinity_g_kg: number_or(inputs, "salinityGKg", 0.0),
            thermal_multiplier: number_or(inputs, "thermalMultiplier", 1.0),
            pressure_multiplier: number_or(inputs, "pressureMultiplier", 1.0),
            min_face_velocity_ms: number_or(inputs, "minFaceVelocityMS", 0.2),
            max_face_velocity_ms: number_or(inputs, "maxFaceVelocityMS", 6.0),
        })
        .unwrap_or_else(|error| panic!("{}: {error}", id(case)));
        let expected = &case["expected"];
        let id = id(case);
        for (actual, key) in [
            (result.volumetric_air_flow_m3_s, "volumetricAirFlowM3S"),
            (result.water_to_dry_air_ratio, "waterToDryAirRatio"),
            (result.plume_density_kg_m3, "plumeDensityKgM3"),
            (result.draft_pressure_pa, "draftPressurePa"),
            (result.residual_pa, "residualPa"),
            (result.thermal.cold_water_c, "coldWaterC"),
            (result.outlet.dry_bulb_c, "outletDryBulbC"),
            (result.evaporation_kg_s, "evaporationKgS"),
        ] {
            agrees(actual, number(expected, key), &format!("{id} {key}"));
        }
    }
}

#[test]
fn performance_curve_vectors_reproduce() {
    let file = vectors();
    for case in file["families"]["performanceCurve"].as_array().unwrap() {
        let inputs = &case["inputs"];
        let records: Vec<PerformanceCurveRecord> = inputs["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| PerformanceCurveRecord {
                wet_bulb_c: number(record, "wetBulbC"),
                range_c: number(record, "rangeC"),
                water_flow_kg_s: number(record, "waterFlowKgS"),
                cold_water_c: number(record, "coldWaterC"),
            })
            .collect();
        let expected = &case["expected"];
        let id = id(case);
        // The bounds are recorded implicitly through the probes; the probes themselves are the
        // recorded cases: a direct prediction, and the inverse solve with the capability ratio.
        if let Some(expected_cold_water) = expected["coldWaterC"].as_f64() {
            let prediction = predict_cold_water_from_performance_curves(
                &records,
                number(inputs, "wetBulbC"),
                number(inputs, "rangeC"),
                number(inputs, "waterFlowKgS"),
            )
            .unwrap_or_else(|error| panic!("{id}: {error}"));
            agrees(
                prediction.cold_water_c,
                expected_cold_water,
                &format!("{id} coldWaterC"),
            );
            assert_eq!(
                prediction.extrapolated,
                expected["extrapolated"].as_bool().unwrap(),
                "{id} extrapolated"
            );
            assert!(performance_curve_bounds(&records).is_ok());
        }
        if let Some(expected_flow) = expected["waterFlowKgS"].as_f64() {
            let solved = predict_water_flow_from_performance_curves(
                &records,
                number(inputs, "wetBulbC"),
                number(inputs, "rangeC"),
                number(inputs, "coldWaterC"),
            )
            .unwrap_or_else(|error| panic!("{id}: {error}"));
            agrees(
                solved.water_flow_kg_s,
                expected_flow,
                &format!("{id} waterFlowKgS"),
            );
            agrees(
                solved.predicted_cold_water_c,
                number(expected, "predictedColdWaterC"),
                &format!("{id} predictedColdWaterC"),
            );
            let capability = evaluate_performance_curve_capability(
                &records,
                number(inputs, "wetBulbC"),
                number(inputs, "rangeC"),
                number(inputs, "coldWaterC"),
                number(inputs, "adjustedTestWaterFlowKgS"),
            )
            .unwrap_or_else(|error| panic!("{id}: {error}"));
            agrees(
                capability.capability_pct,
                number(expected, "capabilityPct"),
                &format!("{id} capabilityPct"),
            );
            agrees(
                capability.predicted_cold_water_at_adjusted_flow_c,
                number(expected, "predictedColdWaterAtAdjustedFlowC"),
                &format!("{id} predictedColdWaterAtAdjustedFlowC"),
            );
            agrees(
                capability.leaving_water_deviation_c,
                number(expected, "leavingWaterDeviationC"),
                &format!("{id} leavingWaterDeviationC"),
            );
        }
    }
}
