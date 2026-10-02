//! Natural-draft coupling — the JavaScript suite's expectations for `src/core/naturalDraft.js`,
//! ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `tests/crossflow-natural.test.js` — the two natural-draft expectations: the documented
//!   balance on the 4200 m² shell, and the taller shell drawing more air. The tower there is a
//!   literal, quoted verbatim below.
//! * `validation/test-vectors.json` (family `naturalDraft`) — the two recorded catalog-cell
//!   cases are replayed in `rust/tests/vectors.rs`.
//! * `src/core/naturalDraft.js` — the draft-height guard and its message, the
//!   `airFreeAreaM2 ?? fillAreaM2` face area, the 480-sample scan (`prefer: 'first'`, 1e-6 Pa
//!   residual), the `chebyshev4` cold-water solve and the bare `catch` that lets a refusing
//!   sample fall out of the scan.
//! * The bundled sample catalog (`src/data/sampleCatalog.js`) — `FILM-VF38` and `DE-3P-10`,
//!   quoted below verbatim.
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    natural_draft_pressure_pa, solve_natural_draft_counterflow, DriftCurvePoint,
    DriftEliminatorRecord, FillLimits, FillPressureCorrelation, FillRecord, NaturalDraftInput,
    TowerRecord, ZoneCorrelation,
};

/// The tower of `tests/crossflow-natural.test.js` (the draft height travels beside the record
/// in this port: see `rust/src/natural_draft.rs`).
fn documented_tower() -> TowerRecord {
    TowerRecord {
        fill_area_m2: 4200.0,
        air_free_area_m2: Some(4200.0),
        drift_area_m2: Some(4200.0 * 0.96),
        inlet_area_m2: Some(4200.0 * 1.25),
        plenum_area_m2: None,
        fan_stack_area_m2: None,
        stack_recovery_factor: None,
        inlet_loss_coefficient: Some(0.55),
        distribution_loss_coefficient: Some(0.35),
        support_loss_coefficient: Some(0.35),
        plenum_loss_coefficient: Some(0.25),
        fixed_pressure_loss_pa: Some(4.0),
        spray_zone_height_m: None,
        spray_zone: None,
        rain_zone_height_m: None,
        rain_zone: None,
    }
}

/// `sampleCatalog.fills` / `FILM-VF38`.
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

/// The documented case of `tests/crossflow-natural.test.js`.
fn documented_input(
    _effective_draft_height_m: f64,
) -> (TowerRecord, FillRecord, DriftEliminatorRecord) {
    (documented_tower(), film_vf38(), de_3p_10())
}

#[test]
fn the_documented_case_balances_buoyancy_against_the_system_resistance() {
    // tests/crossflow-natural.test.js asserts: a positive flow, a balance within 0.01 Pa, a
    // plume genuinely lighter than the ambient air, and a positive draft.
    let (tower, fill, drift_eliminator) = documented_input(170.0);
    let result = solve_natural_draft_counterflow(&NaturalDraftInput {
        tower: &tower,
        effective_draft_height_m: 170.0,
        fill: &fill,
        fill_depth_m: 1.5,
        drift_eliminator: &drift_eliminator,
        hot_water_c: 40.0,
        dry_bulb_c: 30.0,
        wet_bulb_c: 24.0,
        pressure_pa: 101_325.0,
        water_mass_flow_kg_s: 5000.0,
        salinity_g_kg: 0.0,
        thermal_multiplier: 0.95,
        pressure_multiplier: 1.08,
        min_face_velocity_ms: 0.4,
        max_face_velocity_ms: 4.5,
    })
    .expect("the documented case solves");

    assert!(result.volumetric_air_flow_m3_s > 0.0);
    assert!(
        (result.draft_pressure_pa - result.airside.total_pa).abs() < 0.01,
        "draft {} vs resistance {}",
        result.draft_pressure_pa,
        result.airside.total_pa
    );
    assert!(result.plume_density_kg_m3 < result.inlet_air_state.moist_air_density_kg_m3);
    assert!(result.draft_pressure_pa > 0.0);
    // The flow the scan returns sits inside the face-velocity window the call declares.
    let area_m2 = tower.air_free_area_m2.unwrap_or(tower.fill_area_m2);
    assert!(result.volumetric_air_flow_m3_s >= area_m2 * 0.4);
    assert!(result.volumetric_air_flow_m3_s <= area_m2 * 4.5);
    // `evaporationFromAirMassBalance` floors at zero and is a mass balance of the dry air.
    let expected_evaporation = result.airside.dry_air_mass_flow_kg_s
        * (result.outlet.humidity_ratio - result.inlet_air_state.humidity_ratio);
    assert!((result.evaporation_kg_s - expected_evaporation).abs() < 1e-9);
}

#[test]
fn a_taller_shell_draws_more_air() {
    // tests/crossflow-natural.test.js: `run(200) > run(140)`.
    let flow = |height_m: f64| {
        let (tower, fill, drift_eliminator) = documented_input(height_m);
        solve_natural_draft_counterflow(&NaturalDraftInput {
            tower: &tower,
            effective_draft_height_m: height_m,
            fill: &fill,
            fill_depth_m: 1.5,
            drift_eliminator: &drift_eliminator,
            hot_water_c: 40.0,
            dry_bulb_c: 30.0,
            wet_bulb_c: 24.0,
            pressure_pa: 101_325.0,
            water_mass_flow_kg_s: 5000.0,
            salinity_g_kg: 0.0,
            thermal_multiplier: 1.0,
            pressure_multiplier: 1.0,
            min_face_velocity_ms: 0.4,
            max_face_velocity_ms: 4.5,
        })
        .expect("solves")
        .volumetric_air_flow_m3_s
    };
    assert!(flow(200.0) > flow(140.0));
}

#[test]
fn the_default_arguments_match_the_reference_declarations() {
    // `pressurePa` 101 325, `salinityGKg` 0, both multipliers 1, face velocities 0.2–6: a
    // `NaturalDraftInput::new` leaves every one of them at the reference's default, and the
    // solve still lands on a balanced point.
    let (tower, fill, drift_eliminator) = documented_input(170.0);
    let input = NaturalDraftInput::new(
        &tower,
        170.0,
        &fill,
        1.5,
        &drift_eliminator,
        40.0,
        30.0,
        24.0,
        5000.0,
    );
    assert_eq!(input.pressure_pa, 101_325.0);
    assert_eq!(input.salinity_g_kg, 0.0);
    assert_eq!(input.thermal_multiplier, 1.0);
    assert_eq!(input.pressure_multiplier, 1.0);
    assert_eq!(input.min_face_velocity_ms, 0.2);
    assert_eq!(input.max_face_velocity_ms, 6.0);
    let result = solve_natural_draft_counterflow(&input).expect("the defaults solve");
    assert!(
        result.residual_pa.abs() < 1e-3,
        "residual {}",
        result.residual_pa
    );
    let area_m2 = tower.air_free_area_m2.unwrap_or(tower.fill_area_m2);
    assert!(result.volumetric_air_flow_m3_s >= area_m2 * 0.2);
    assert!(result.volumetric_air_flow_m3_s <= area_m2 * 6.0);
    // The `??` fallbacks: with no `airFreeAreaM2` the face area is the fill area, so the
    // solve runs over the same window the fill area defines and still lands on a flow.
    let mut no_air_free = documented_tower();
    no_air_free.air_free_area_m2 = None;
    let fallback = solve_natural_draft_counterflow(&NaturalDraftInput::new(
        &no_air_free,
        170.0,
        &fill,
        1.5,
        &drift_eliminator,
        40.0,
        30.0,
        24.0,
        5000.0,
    ))
    .expect("solves");
    assert!(fallback.volumetric_air_flow_m3_s > 0.0);
}

#[test]
fn a_tower_without_a_draft_height_is_refused_before_any_physics() {
    // src/core/naturalDraft.js: `if (!(tower.effectiveDraftHeightM > 0)) throw …`.
    let (tower, fill, drift_eliminator) = documented_input(0.0);
    let error = solve_natural_draft_counterflow(&NaturalDraftInput::new(
        &tower,
        0.0,
        &fill,
        1.5,
        &drift_eliminator,
        40.0,
        30.0,
        24.0,
        5000.0,
    ))
    .expect_err("must refuse");
    assert_eq!(
        error.message(),
        "Natural-draft tower requires effectiveDraftHeightM."
    );
}

#[test]
fn the_draft_term_is_the_reference_buoyancy_product() {
    // `naturalDraftPressurePa` = G * height * (ambient - plume), with G = 9.80665. The two
    // literals are the reference's own output for these inputs (measured in the engine).
    assert_eq!(
        natural_draft_pressure_pa(170.0, 1.15, 1.13),
        33.34261000000003
    );
    assert_eq!(natural_draft_pressure_pa(0.0, 1.2, 1.1), 0.0);
    assert!(natural_draft_pressure_pa(100.0, 1.1, 1.2) < 0.0);
}

#[test]
fn the_salinity_reaches_the_cold_water_solve() {
    // `solveColdWaterTemperature(… salinityGKg …)`: the salinity moves the water heat capacity,
    // so two runs that differ only in salinity must not report the same cold water.
    let (tower, fill, drift_eliminator) = documented_input(170.0);
    let cold_water = |salinity_g_kg: f64| {
        solve_natural_draft_counterflow(
            &NaturalDraftInput::new(
                &tower,
                170.0,
                &fill,
                1.5,
                &drift_eliminator,
                40.0,
                30.0,
                24.0,
                5000.0,
            )
            .with_salinity_g_kg(salinity_g_kg),
        )
        .expect("solves")
        .thermal
        .cold_water_c
    };
    let fresh = cold_water(0.0);
    let saline = cold_water(50.0);
    assert_ne!(fresh, saline);
    assert!(
        (fresh - saline).abs() > 1e-9,
        "fresh {fresh} saline {saline}"
    );
    // A fresh-water solve is unchanged by the default argument being left at zero.
    let (tower, fill, drift_eliminator) = documented_input(170.0);
    let defaulted = solve_natural_draft_counterflow(&NaturalDraftInput::new(
        &tower,
        170.0,
        &fill,
        1.5,
        &drift_eliminator,
        40.0,
        30.0,
        24.0,
        5000.0,
    ))
    .expect("solves");
    assert_eq!(defaulted.thermal.cold_water_c, fresh);
}
