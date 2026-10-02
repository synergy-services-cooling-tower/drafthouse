//! Air-side losses and fan-stack discharge — the JavaScript suite's expectations for
//! `src/core/airside.js`, ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `tests/airside.test.js` — the eight air-side expectations, against the bundled sample
//!   catalog records (`src/data/sampleCatalog.js`, quoted below verbatim; all coefficients
//!   are illustrative synthetic data).
//! * `validation/test-vectors.json` (family `airside`) — the recorded regression vectors,
//!   reproduced at the same 1e-6 relative tolerance `tests/vectors.test.js` uses.
//! * `src/core/airside.js` — the formulas, `??` defaults and guard messages.
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    check_fill_operating_envelope, drift_performance_at_velocity, fan_stack_discharge_pressure_pa,
    fill_pressure_drop_pa, fill_thermal_merkel_number, minor_loss_pressure_pa, psychrometric_state,
    resolve_airside_areas, system_pressure_breakdown, velocity_pressure_pa, zone_merkel_number,
    DriftCurvePoint, DriftEliminatorRecord, FanCurvePoint, FanPressureBasis, FanRecord,
    FanStackDischargeInput, FillLimits, FillOperatingEnvelopeInput, FillPressureCorrelation,
    FillRecord, PsychrometricStateInput, SystemPressureBreakdown, SystemPressureBreakdownInput,
    TowerRecord, ZoneCorrelation,
};

/* ---------------- the bundled sample catalog ---------------- */

/// `sampleCatalog.towers` / `IDCF-064`.
fn idcf_064() -> TowerRecord {
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

/// `sampleCatalog.fills` / `FILM-OF25`.
fn film_of25() -> FillRecord {
    FillRecord {
        id: "FILM-OF25".to_string(),
        thermal: Some(ZoneCorrelation {
            coefficient_per_m: 1.12,
            reference_water_loading_kg_m2_s: 3.0,
            reference_dry_air_loading_kg_m2_s: 2.0,
            water_exponent: -0.31,
            air_exponent: 0.44,
        }),
        pressure: Some(FillPressureCorrelation {
            coefficient_pa_per_m: 61.0,
            reference_water_loading_kg_m2_s: 3.0,
            reference_dry_air_loading_kg_m2_s: 2.0,
            water_exponent: 0.14,
            air_exponent: 1.72,
        }),
        limits: FillLimits {
            min_water_loading_kg_m2_s: 1.3,
            max_water_loading_kg_m2_s: 5.4,
            min_dry_air_loading_kg_m2_s: 0.9,
            max_dry_air_loading_kg_m2_s: 3.3,
            max_water_temperature_c: 60.0,
        },
        allowed_water_quality_classes: vec!["clean".to_string(), "moderate".to_string()],
    }
}

/// `sampleCatalog.fills` / `FILM-VF38` (used by the envelope cases only).
fn film_vf38() -> FillRecord {
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
        allowed_water_quality_classes: vec![
            "clean".to_string(),
            "moderate".to_string(),
            "dirty".to_string(),
        ],
    }
}

/// `sampleCatalog.driftEliminators` / `DE-3P-10`.
fn de_3p_10() -> DriftEliminatorRecord {
    DriftEliminatorRecord {
        id: "DE-3P-10".to_string(),
        curve: vec![
            DriftCurvePoint {
                face_velocity_ms: 1.0,
                drift_ppm: 4.0,
                pressure_drop_pa: 10.0,
            },
            DriftCurvePoint {
                face_velocity_ms: 1.5,
                drift_ppm: 6.0,
                pressure_drop_pa: 18.0,
            },
            DriftCurvePoint {
                face_velocity_ms: 2.0,
                drift_ppm: 9.0,
                pressure_drop_pa: 31.0,
            },
            DriftCurvePoint {
                face_velocity_ms: 2.5,
                drift_ppm: 14.0,
                pressure_drop_pa: 49.0,
            },
            DriftCurvePoint {
                face_velocity_ms: 3.0,
                drift_ppm: 23.0,
                pressure_drop_pa: 72.0,
            },
            DriftCurvePoint {
                face_velocity_ms: 3.5,
                drift_ppm: 38.0,
                pressure_drop_pa: 101.0,
            },
        ],
    }
}

/// `sampleCatalog.fans` / `AX-500`.
fn ax_500() -> FanRecord {
    FanRecord {
        id: "AX-500".to_string(),
        stack_area_m2: Some(19.635),
        pressure_basis: Some(FanPressureBasis::Total),
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

/// The `breakdownAt` helper of `tests/airside.test.js`: the catalog cell at the recorded
/// 33/27 °C condition, 1.5 m of fill and 200 kg/s of water.
fn breakdown_at(volumetric_air_flow_m3_s: f64, fan: Option<&FanRecord>) -> SystemPressureBreakdown {
    let air = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let tower = idcf_064();
    let fill = film_of25();
    let drift = de_3p_10();
    system_pressure_breakdown(SystemPressureBreakdownInput {
        tower: &tower,
        fill: &fill,
        fill_depth_m: 1.5,
        drift_eliminator: &drift,
        fan,
        volumetric_air_flow_m3_s,
        dry_air_density_kg_m3: air.dry_air_density_kg_m3,
        moist_air_density_kg_m3: air.moist_air_density_kg_m3,
        water_mass_flow_kg_s: 200.0,
        thermal_multiplier: 1.0,
        pressure_multiplier: 1.0,
    })
    .unwrap()
}

fn agrees(actual: f64, expected: f64, label: &str) {
    let scale = expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() / scale < 1e-6,
        "{label}: got {actual}, vector says {expected}"
    );
}

/* ---------------- the loss definitions ---------------- */

#[test]
fn velocity_pressure_and_minor_loss_follow_the_definition() {
    // dp = 0.5 rho v^2, and a minor loss is K times that velocity pressure.
    let density = 1.1392905373780184;
    let velocity = 4.0625;
    let velocity_pressure = velocity_pressure_pa(density, velocity);
    // The reference expression is `0.5 * rho * v ** 2`; keep the same association.
    assert_eq!(velocity_pressure, 0.5 * density * velocity.powi(2));
    assert_eq!(
        minor_loss_pressure_pa(3.0, density, velocity),
        3.0 * velocity_pressure
    );
    // Values pinned from the reference (src/core/airside.js).
    assert!((velocity_pressure - 9.40137211019947).abs() < 1e-12);
    assert!((minor_loss_pressure_pa(3.0, density, velocity) - 28.20411633059841).abs() < 1e-9);
}

/* ---------------- recorded vectors ---------------- */

/// `validation/test-vectors.json`, family `airside`, at the same 1e-6 relative tolerance
/// `tests/vectors.test.js` uses.
#[test]
fn the_recorded_airside_vectors_reproduce() {
    let cases: [(f64, [f64; 7]); 3] = [
        (
            100.0,
            [
                145.340817, 72.770477, 21.622951, 16.688826, 5.614696, 9.604086, 1.830925,
            ],
        ),
        (
            130.0,
            [
                229.193862, 114.271462, 35.721311, 28.204116, 9.488837, 16.230905, 2.058431,
            ],
        ),
        (
            160.0,
            [
                332.801889, 163.320834, 54.655738, 42.723395, 14.373623, 24.586460, 2.258391,
            ],
        ),
    ];
    let fan = ax_500();
    for (flow, expected) in cases {
        let result = breakdown_at(flow, Some(&fan));
        for (index, quantity) in [
            ("totalPa", result.total_pa),
            ("fillPa", result.fill_pa),
            ("driftPa", result.drift_pa),
            ("inletPa", result.inlet_pa),
            ("plenumPa", result.plenum_pa),
            ("fanStackPa", result.fan_stack_pa),
            ("availableMerkelNumber", result.available_merkel_number),
        ]
        .into_iter()
        .enumerate()
        {
            agrees(
                quantity.1,
                expected[index],
                &format!("{flow} m³/s {}", quantity.0),
            );
        }
        // The per-zone Merkel numbers are part of the recorded vector too.
        agrees(
            result.fill_merkel_number,
            [1.562189, 1.753351, 1.921084][[100.0, 130.0, 160.0]
                .iter()
                .position(|f| *f == flow)
                .unwrap()],
            &format!("{flow} m³/s fillMerkelNumber"),
        );
        agrees(
            result.spray_zone_merkel_number,
            [0.089183, 0.100359, 0.110188][[100.0, 130.0, 160.0]
                .iter()
                .position(|f| *f == flow)
                .unwrap()],
            &format!("{flow} m³/s sprayZoneMerkelNumber"),
        );
        agrees(
            result.rain_zone_merkel_number,
            [0.179553, 0.204722, 0.227119][[100.0, 130.0, 160.0]
                .iter()
                .position(|f| *f == flow)
                .unwrap()],
            &format!("{flow} m³/s rainZoneMerkelNumber"),
        );
    }
}

/* ---------------- tests/airside.test.js, ported ---------------- */

#[test]
fn each_minor_loss_is_referenced_to_its_own_section_velocity_not_the_fill_face() {
    let fan = ax_500();
    let result = breakdown_at(130.0, Some(&fan));
    let areas = resolve_airside_areas(&idcf_064(), Some(&fan));
    let air = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let velocity_pressure =
        |velocity_ms: f64| velocity_pressure_pa(air.moist_air_density_kg_m3, velocity_ms);

    assert!(
        (result.inlet_pa - 3.0 * velocity_pressure(130.0 / areas.inlet_area_m2)).abs() < 1e-9,
        "inlet loss must use the air-inlet face velocity"
    );
    assert!(
        (result.plenum_pa - 0.38 * velocity_pressure(130.0 / areas.plenum_area_m2)).abs() < 1e-9,
        "plenum loss must use the fan-throat velocity"
    );
    assert!(
        result.inlet_velocity_ms > result.fill_velocity_ms,
        "inlet face must be smaller than the fill face"
    );
    assert!(
        result.fan_stack_velocity_ms.unwrap() > result.fill_velocity_ms,
        "fan throat must be smaller than the fill face"
    );
}

#[test]
fn minor_losses_are_a_material_share_of_the_total_not_a_rounding_error() {
    let fan = ax_500();
    let result = breakdown_at(130.0, Some(&fan));
    let minor_pa = result.inlet_pa
        + result.distribution_pa
        + result.support_pa
        + result.plenum_pa
        + result.fan_stack_pa;
    let share = minor_pa / result.total_pa;
    assert!(
        share > 0.15 && share < 0.7,
        "minor losses are {share} of total"
    );
}

#[test]
fn fan_stack_discharge_velocity_pressure_is_charged_against_a_total_pressure_curve() {
    let fan = ax_500();
    let result = breakdown_at(130.0, Some(&fan));
    assert_eq!(result.fan_pressure_basis, FanPressureBasis::Total);
    assert!(
        result.fan_stack_pa > 0.0,
        "a total-pressure fan curve must be charged the outlet velocity pressure"
    );
    let air = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let expected = (1.0 - 0.35)
        * velocity_pressure_pa(
            air.moist_air_density_kg_m3,
            130.0 / fan.stack_area_m2.unwrap(),
        );
    assert!(
        (result.fan_stack_pa - expected).abs() < 1e-9,
        "{} vs {expected}",
        result.fan_stack_pa
    );
}

#[test]
fn a_static_pressure_fan_curve_is_not_charged_the_discharge_term_twice() {
    let mut static_fan = ax_500();
    static_fan.pressure_basis = Some(FanPressureBasis::Static);
    let total = breakdown_at(130.0, Some(&ax_500()));
    let static_basis = breakdown_at(130.0, Some(&static_fan));
    assert_eq!(static_basis.fan_stack_pa, 0.0);
    assert_eq!(static_basis.fan_stack_velocity_ms, None);
    assert_eq!(static_basis.fan_pressure_basis, FanPressureBasis::Static);
    assert!(
        ((total.total_pa - static_basis.total_pa) - total.fan_stack_pa).abs() < 1e-9,
        "the difference between the two curves must be exactly the discharge term"
    );
}

#[test]
fn a_velocity_recovery_stack_reduces_the_discharge_loss() {
    let air = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let fan = ax_500();
    let plain = fan_stack_discharge_pressure_pa(FanStackDischargeInput {
        volumetric_air_flow_m3_s: 130.0,
        fan_stack_area_m2: fan.stack_area_m2,
        moist_air_density_kg_m3: air.moist_air_density_kg_m3,
        stack_recovery_factor: 0.0,
        fan_pressure_basis: FanPressureBasis::Total,
    })
    .unwrap();
    let recovery = fan_stack_discharge_pressure_pa(FanStackDischargeInput {
        volumetric_air_flow_m3_s: 130.0,
        fan_stack_area_m2: fan.stack_area_m2,
        moist_air_density_kg_m3: air.moist_air_density_kg_m3,
        stack_recovery_factor: 0.4,
        fan_pressure_basis: FanPressureBasis::Total,
    })
    .unwrap();
    assert!(recovery.pressure_pa < plain.pressure_pa);
    assert!((recovery.pressure_pa - 0.6 * plain.pressure_pa).abs() < 1e-9);
}

#[test]
fn every_loss_scales_with_the_square_of_air_flow() {
    let fan = ax_500();
    let low = breakdown_at(100.0, Some(&fan));
    let high = breakdown_at(200.0, Some(&fan));
    for (key, low_value, high_value) in [
        ("inletPa", low.inlet_pa, high.inlet_pa),
        ("distributionPa", low.distribution_pa, high.distribution_pa),
        ("supportPa", low.support_pa, high.support_pa),
        ("plenumPa", low.plenum_pa, high.plenum_pa),
        ("fanStackPa", low.fan_stack_pa, high.fan_stack_pa),
    ] {
        assert!(
            (high_value / low_value - 4.0).abs() < 1e-9,
            "{key} did not scale quadratically"
        );
    }
}

#[test]
fn available_merkel_number_includes_the_spray_and_rain_zones() {
    let fan = ax_500();
    let result = breakdown_at(130.0, Some(&fan));
    assert!(result.spray_zone_merkel_number > 0.0);
    assert!(result.rain_zone_merkel_number > 0.0);
    assert!(
        (result.available_merkel_number
            - (result.fill_merkel_number
                + result.spray_zone_merkel_number
                + result.rain_zone_merkel_number))
            .abs()
            < 1e-12
    );
    let zone_share = (result.spray_zone_merkel_number + result.rain_zone_merkel_number)
        / result.available_merkel_number;
    assert!(
        zone_share > 0.05 && zone_share < 0.35,
        "spray + rain zones are {zone_share}"
    );
}

/* ---------------- areas ---------------- */

#[test]
fn resolve_airside_areas_falls_back_exactly_as_the_reference_does() {
    // `tower.airFreeAreaM2 ?? fillAreaM2` and the rest of the chain, verified against the
    // reference: with no fan the stack area is absent, and a fan with a zero stack area wins
    // over the tower's own non-zero field.
    let bare = TowerRecord {
        fill_area_m2: 64.0,
        air_free_area_m2: None,
        drift_area_m2: None,
        inlet_area_m2: None,
        plenum_area_m2: None,
        fan_stack_area_m2: None,
        stack_recovery_factor: None,
        inlet_loss_coefficient: None,
        distribution_loss_coefficient: None,
        support_loss_coefficient: None,
        plenum_loss_coefficient: None,
        fixed_pressure_loss_pa: None,
        spray_zone_height_m: None,
        spray_zone: None,
        rain_zone_height_m: None,
        rain_zone: None,
    };
    let areas = resolve_airside_areas(&bare, None);
    assert_eq!(areas.fill_area_m2, 64.0);
    assert_eq!(areas.air_free_area_m2, 64.0);
    assert_eq!(areas.drift_area_m2, 64.0);
    assert_eq!(areas.inlet_area_m2, 64.0);
    assert_eq!(areas.plenum_area_m2, 64.0);
    assert_eq!(areas.fan_stack_area_m2, None);

    let mut tower = idcf_064();
    tower.fan_stack_area_m2 = Some(99.0);
    let fan = ax_500();
    let areas = resolve_airside_areas(&tower, Some(&fan));
    assert_eq!(areas.fan_stack_area_m2, Some(19.635));
    assert_eq!(areas.plenum_area_m2, 19.635);

    let mut zero_stack_fan = ax_500();
    zero_stack_fan.stack_area_m2 = Some(0.0);
    let areas = resolve_airside_areas(&tower, Some(&zero_stack_fan));
    assert_eq!(areas.fan_stack_area_m2, Some(0.0));
    assert_eq!(areas.plenum_area_m2, 0.0);
}

/* ---------------- drift and fill correlations ---------------- */

#[test]
fn a_drift_performance_reading_interpolates_and_extrapolates_both_curves() {
    let drift = de_3p_10();
    let reading = drift_performance_at_velocity(&drift, 2.25).unwrap();
    assert_eq!(reading.drift_ppm, 11.5);
    assert_eq!(reading.pressure_drop_pa, 40.0);
    // `clampEnds: false`: outside the tabulated range the end pair is continued linearly, so
    // a velocity beyond the curve is a value, not a refusal. Values read from the reference.
    let above = drift_performance_at_velocity(&drift, 4.0).unwrap();
    assert_eq!(above.drift_ppm, 53.0);
    assert_eq!(above.pressure_drop_pa, 130.0);
    let below = drift_performance_at_velocity(&drift, 0.5).unwrap();
    assert_eq!(below.drift_ppm, 2.0);
    assert_eq!(below.pressure_drop_pa, 2.0);
}

#[test]
fn the_fill_correlations_reproduce_the_reference_values() {
    // Water/air loadings of the recorded 130 m³/s case (see the vector test above).
    let fill = film_of25();
    let water_loading = 3.125;
    let dry_air_loading = 2.268304276689687;
    let merkel =
        fill_thermal_merkel_number(&fill, 1.5, water_loading, dry_air_loading, 1.0).unwrap();
    let pressure = fill_pressure_drop_pa(&fill, 1.5, water_loading, dry_air_loading, 1.0).unwrap();
    assert!((merkel - 1.7533505058419276).abs() < 1e-12);
    assert!((pressure - 114.27146227407496).abs() < 1e-9);
    // The multipliers scale only their own term, as in the reference.
    let damped = fill_pressure_drop_pa(&fill, 1.5, water_loading, dry_air_loading, 1.35).unwrap();
    assert!((damped - 1.35 * pressure).abs() < 1e-9);
    let degraded =
        fill_thermal_merkel_number(&fill, 1.5, water_loading, dry_air_loading, 0.8).unwrap();
    assert!((degraded - 0.8 * merkel).abs() < 1e-12);
}

/* ---------------- the fill operating envelope ---------------- */

#[test]
fn the_envelope_accepts_a_clean_fill_inside_its_limits() {
    let fill = film_of25();
    let envelope = check_fill_operating_envelope(FillOperatingEnvelopeInput {
        fill: &fill,
        water_loading_kg_m2_s: 2.5,
        dry_air_loading_kg_m2_s: 2.0,
        hot_water_c: 40.0,
        water_quality_class: "clean",
    });
    assert!(envelope.ok);
    assert!(envelope.failures.is_empty());
}

/// The failure strings below were read from the reference (`checkFillOperatingEnvelope` in
/// `src/core/airside.js`) and are quoted verbatim, `toFixed` rounding included.
#[test]
fn the_envelope_rejection_messages_match_the_reference_byte_for_byte() {
    let fill = film_of25();
    let check = |water: f64, air: f64, hot: f64, class: &str| {
        check_fill_operating_envelope(FillOperatingEnvelopeInput {
            fill: &fill,
            water_loading_kg_m2_s: water,
            dry_air_loading_kg_m2_s: air,
            hot_water_c: hot,
            water_quality_class: class,
        })
        .failures
    };

    assert_eq!(
        check(1.0, 2.0, 40.0, "clean"),
        vec!["Water loading 1.000 kg/(m²·s) is outside 1.3–5.4.".to_string()]
    );
    assert_eq!(
        check(2.5, 3.9, 40.0, "clean"),
        vec!["Dry-air loading 3.900 kg/(m²·s) is outside 0.9–3.3.".to_string()]
    );
    assert_eq!(
        check(2.5, 2.0, 70.0, "clean"),
        vec!["Hot-water temperature 70.0 °C exceeds 60 °C.".to_string()]
    );
    assert_eq!(
        check(2.5, 2.0, 40.0, "dirty"),
        vec![
            "Fill is not approved in the sample catalog for water-quality class “dirty”."
                .to_string()
        ]
    );

    // All three loading/temperature limits fail at once, in the reference's order.
    let vf38 = film_vf38();
    let envelope = check_fill_operating_envelope(FillOperatingEnvelopeInput {
        fill: &vf38,
        water_loading_kg_m2_s: 0.5,
        dry_air_loading_kg_m2_s: 4.5,
        hot_water_c: 80.0,
        water_quality_class: "clean",
    });
    assert!(!envelope.ok);
    assert_eq!(
        envelope.failures,
        vec![
            "Water loading 0.500 kg/(m²·s) is outside 1.1–6.2.".to_string(),
            "Dry-air loading 4.500 kg/(m²·s) is outside 0.8–3.5.".to_string(),
            "Hot-water temperature 80.0 °C exceeds 75 °C.".to_string(),
        ]
    );
}

/// `Number.prototype.toFixed` rounds the exact value of the double, ties away from zero.
/// These cases were read from the reference; the ones that distinguish it from Rust's own
/// fixed-precision formatting are 1.0005 (below the tie) and 54.9995 (below the tie).
#[test]
fn the_envelope_messages_round_like_the_reference() {
    // A fill whose limits are all zero, so the water-loading message is always the first
    // failure and the value under test is the one printed.
    let mut zero_limits = film_of25();
    zero_limits.limits = FillLimits {
        min_water_loading_kg_m2_s: 0.0,
        max_water_loading_kg_m2_s: 0.0,
        min_dry_air_loading_kg_m2_s: 0.0,
        max_dry_air_loading_kg_m2_s: 0.0,
        max_water_temperature_c: 0.0,
    };
    let water_message = |water: f64| {
        check_fill_operating_envelope(FillOperatingEnvelopeInput {
            fill: &zero_limits,
            water_loading_kg_m2_s: water,
            dry_air_loading_kg_m2_s: 1.0,
            hot_water_c: 1.0,
            water_quality_class: "clean",
        })
        .failures[0]
            .clone()
    };
    assert!(water_message(1.0005).starts_with("Water loading 1.000 "));
    assert!(water_message(2.6745).starts_with("Water loading 2.675 "));
    assert!(water_message(0.0625).starts_with("Water loading 0.063 "));
    assert!(water_message(54.9995).starts_with("Water loading 54.999 "));
    assert!(water_message(0.0005).starts_with("Water loading 0.001 "));
    // Values just above and below the tie, read from the reference: 1.9995 and 0.9995 are
    // above their tie, 2.9995 is below.
    assert!(water_message(1.9995).starts_with("Water loading 2.000 "));
    assert!(water_message(2.9995).starts_with("Water loading 2.999 "));
    assert!(water_message(0.9995).starts_with("Water loading 1.000 "));

    // A fill that accepts the water and air loadings but not any hot water, so the
    // temperature message is the first (and only) failure.
    let mut temperature_limits = film_of25();
    temperature_limits.limits.max_water_temperature_c = 0.0;
    let fill = temperature_limits;
    let temperature_message = |hot: f64| {
        check_fill_operating_envelope(FillOperatingEnvelopeInput {
            fill: &fill,
            water_loading_kg_m2_s: 2.5,
            dry_air_loading_kg_m2_s: 2.0,
            hot_water_c: hot,
            water_quality_class: "clean",
        })
        .failures[0]
            .clone()
    };
    assert!(temperature_message(60.05).starts_with("Hot-water temperature 60.0 "));
    assert!(temperature_message(54.9995).starts_with("Hot-water temperature 55.0 "));
}

/* ---------------- refusal behaviour ---------------- */

#[test]
fn a_total_pressure_curve_without_a_stack_area_is_refused_with_the_reference_message() {
    let error = fan_stack_discharge_pressure_pa(FanStackDischargeInput {
        volumetric_air_flow_m3_s: 130.0,
        fan_stack_area_m2: None,
        moist_air_density_kg_m3: 1.14,
        stack_recovery_factor: 0.0,
        fan_pressure_basis: FanPressureBasis::Total,
    })
    .expect_err("a missing stack area must be refused");
    assert!(error.message().contains("A fan-stack area is required"));
    // Zero and negative areas are refused the same way (the reference guards `!(area > 0)`).
    for area in [Some(0.0), Some(-1.0), Some(f64::NAN)] {
        assert!(
            fan_stack_discharge_pressure_pa(FanStackDischargeInput {
                volumetric_air_flow_m3_s: 130.0,
                fan_stack_area_m2: area,
                moist_air_density_kg_m3: 1.14,
                stack_recovery_factor: 0.0,
                fan_pressure_basis: FanPressureBasis::Total,
            })
            .is_err(),
            "area {area:?} must be refused"
        );
    }
    // With no fan the breakdown declares the curve static, so no stack area is required.
    let fanless = breakdown_at(130.0, None);
    assert_eq!(fanless.fan_stack_pa, 0.0);
    assert_eq!(fanless.fan_stack_velocity_ms, None);
    assert_eq!(fanless.fan_pressure_basis, FanPressureBasis::Static);
}

#[test]
fn a_stack_recovery_factor_outside_zero_one_is_refused() {
    for recovery in [1.0, -0.1, 2.0] {
        let error = fan_stack_discharge_pressure_pa(FanStackDischargeInput {
            volumetric_air_flow_m3_s: 130.0,
            fan_stack_area_m2: Some(19.635),
            moist_air_density_kg_m3: 1.14,
            stack_recovery_factor: recovery,
            fan_pressure_basis: FanPressureBasis::Total,
        })
        .expect_err("out-of-range recovery factor must be refused");
        assert_eq!(error.message(), "stackRecoveryFactor must be in [0, 1).");
    }
    // The bounds themselves: 0 is accepted, 1 is not.
    assert!(fan_stack_discharge_pressure_pa(FanStackDischargeInput {
        volumetric_air_flow_m3_s: 130.0,
        fan_stack_area_m2: Some(19.635),
        moist_air_density_kg_m3: 1.14,
        stack_recovery_factor: 0.0,
        fan_pressure_basis: FanPressureBasis::Total,
    })
    .is_ok());
}

#[test]
fn a_fill_without_a_correlation_is_refused_and_a_missing_zone_contributes_nothing() {
    let mut fill = film_of25();
    fill.thermal = None;
    let error = fill_thermal_merkel_number(&fill, 1.5, 3.0, 2.0, 1.0)
        .expect_err("a missing thermal correlation must be refused");
    assert_eq!(error.message(), "Fill thermal correlation is missing.");
    fill.pressure = None;
    let error = fill_pressure_drop_pa(&fill, 1.5, 3.0, 2.0, 1.0)
        .expect_err("a missing pressure correlation must be refused");
    assert_eq!(error.message(), "Fill pressure correlation is missing.");

    // A missing zone is not an error: it contributes zero, and the reference only checks the
    // height once there is a zone to apply it to.
    assert_eq!(zone_merkel_number(None, 0.0, 3.0, 2.0, 1.0).unwrap(), 0.0);
    let zone = ZoneCorrelation {
        coefficient_per_m: 0.16,
        reference_water_loading_kg_m2_s: 3.0,
        reference_dry_air_loading_kg_m2_s: 2.0,
        water_exponent: -0.30,
        air_exponent: 0.45,
    };
    let error = zone_merkel_number(Some(&zone), 0.0, 3.0, 2.0, 1.0)
        .expect_err("a present zone with no height must be refused");
    assert_eq!(error.message(), "zone heightM must be positive.");
}

#[test]
fn non_positive_depths_and_air_flows_are_refused() {
    let fill = film_of25();
    assert_eq!(
        fill_thermal_merkel_number(&fill, 0.0, 3.0, 2.0, 1.0)
            .unwrap_err()
            .message(),
        "depthM must be positive."
    );
    assert_eq!(
        fill_pressure_drop_pa(&fill, -1.0, 3.0, 2.0, 1.0)
            .unwrap_err()
            .message(),
        "depthM must be positive."
    );

    let tower = idcf_064();
    let drift = de_3p_10();
    let fan = ax_500();
    let air = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let error = system_pressure_breakdown(SystemPressureBreakdownInput {
        tower: &tower,
        fill: &fill,
        fill_depth_m: 1.5,
        drift_eliminator: &drift,
        fan: Some(&fan),
        volumetric_air_flow_m3_s: 0.0,
        dry_air_density_kg_m3: air.dry_air_density_kg_m3,
        moist_air_density_kg_m3: air.moist_air_density_kg_m3,
        water_mass_flow_kg_s: 200.0,
        thermal_multiplier: 1.0,
        pressure_multiplier: 1.0,
    })
    .expect_err("a non-positive air flow must be refused");
    assert_eq!(error.message(), "volumetricAirFlowM3S must be positive.");
}

#[test]
fn the_defaults_of_the_breakdown_match_the_reference_option_defaults() {
    // A tower with no loss coefficients, no recovery factor, no fixed loss and no fan
    // resolves the reference defaults (1.2, 0.8, 0.6, 0.7, 0, static basis).
    let tower = TowerRecord {
        fill_area_m2: 64.0,
        air_free_area_m2: Some(64.0),
        drift_area_m2: None,
        inlet_area_m2: None,
        plenum_area_m2: None,
        fan_stack_area_m2: None,
        stack_recovery_factor: None,
        inlet_loss_coefficient: None,
        distribution_loss_coefficient: None,
        support_loss_coefficient: None,
        plenum_loss_coefficient: None,
        fixed_pressure_loss_pa: None,
        spray_zone_height_m: None,
        spray_zone: None,
        rain_zone_height_m: None,
        rain_zone: None,
    };
    let fill = film_of25();
    let drift = de_3p_10();
    let air = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let result = system_pressure_breakdown(SystemPressureBreakdownInput {
        tower: &tower,
        fill: &fill,
        fill_depth_m: 1.5,
        drift_eliminator: &drift,
        fan: None,
        volumetric_air_flow_m3_s: 130.0,
        dry_air_density_kg_m3: air.dry_air_density_kg_m3,
        moist_air_density_kg_m3: air.moist_air_density_kg_m3,
        water_mass_flow_kg_s: 200.0,
        thermal_multiplier: 1.0,
        pressure_multiplier: 1.0,
    })
    .unwrap();
    let velocity_pressure =
        |velocity_ms: f64| velocity_pressure_pa(air.moist_air_density_kg_m3, velocity_ms);
    assert_eq!(
        result.inlet_pa,
        1.2 * velocity_pressure(130.0 / result.areas.inlet_area_m2)
    );
    assert_eq!(
        result.distribution_pa,
        0.8 * velocity_pressure(result.fill_velocity_ms)
    );
    assert_eq!(
        result.support_pa,
        0.6 * velocity_pressure(result.fill_velocity_ms)
    );
    assert_eq!(
        result.plenum_pa,
        0.7 * velocity_pressure(result.plenum_velocity_ms)
    );
    assert_eq!(result.fixed_pa, 0.0);
    assert_eq!(result.fan_stack_pa, 0.0);
    assert_eq!(result.spray_zone_merkel_number, 0.0);
    assert_eq!(result.rain_zone_merkel_number, 0.0);
    assert_eq!(result.available_merkel_number, result.fill_merkel_number);
}
