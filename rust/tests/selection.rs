//! Component selection — port of `src/core/selection.js` with the engineering-objective
//! ranking of issue #1, tested against the reference engine's recorded behaviour.
//!
//! Sources, quoted per test:
//!
//! * `tests/selection.test.js` — 'default synthetic catalog returns feasible constrained
//!   selections'.
//! * `src/core/selection.js` — the candidate generation, the rejection labels and the
//!   per-candidate physics (the bundled catalog records below are quoted verbatim from
//!   `src/data/sampleCatalog.js`; all coefficients are illustrative synthetic data).
//! * issue #1's measured table (18 September 2026) — the four-objective anchors on the
//!   bundled catalog; the capacity and capability-ratio numbers were measured from the
//!   reference engine's own functions at this commit (`scripts/parity/run.mjs` recomputes
//!   them independently on every run).
//!
//! The JavaScript engine is the specification; nothing here re-derives a physics value.

use serde_json::Value;
use synergy_drafthouse::{
    cli, default_selection_requirements, rank_candidates, run_selection,
    select_cooling_tower_components, CatalogMetadata, DriftCurvePoint, DriftEliminatorRecord,
    FanCurvePoint, FanPressureBasis, FanRecord, FillLayer, FillLimits, FillPressureCorrelation,
    FillRecord, FillStack, NozzleRecord, Objective, SelectionCandidate, SelectionCatalog,
    SelectionDriftEliminator, SelectionFan, SelectionFill, SelectionInput, SelectionRequirements,
    SelectionTower, SystemPressureBreakdown, TowerRecord, TowerType, WaterQualityFactor,
    ZoneCorrelation,
};

/* ---------------- the bundled sample catalog, quoted verbatim ---------------- */

fn zone(
    coefficient_per_m: f64,
    reference_water_loading_kg_m2_s: f64,
    reference_dry_air_loading_kg_m2_s: f64,
    water_exponent: f64,
    air_exponent: f64,
) -> ZoneCorrelation {
    ZoneCorrelation {
        coefficient_per_m,
        reference_water_loading_kg_m2_s,
        reference_dry_air_loading_kg_m2_s,
        water_exponent,
        air_exponent,
    }
}

fn spray_zone() -> ZoneCorrelation {
    zone(0.16, 3.0, 2.0, -0.30, 0.45)
}

fn rain_zone() -> ZoneCorrelation {
    zone(0.13, 3.0, 2.0, -0.35, 0.50)
}

fn idcf_040() -> SelectionTower {
    SelectionTower {
        id: "IDCF-040".to_string(),
        tower_type: TowerType::Counterflow,
        physics: TowerRecord {
            fill_area_m2: 40.0,
            air_free_area_m2: Some(40.0),
            drift_area_m2: Some(38.0),
            inlet_area_m2: Some(22.0),
            plenum_area_m2: None,
            fan_stack_area_m2: None,
            stack_recovery_factor: Some(0.35),
            inlet_loss_coefficient: Some(3.2),
            distribution_loss_coefficient: Some(4.6),
            support_loss_coefficient: Some(2.2),
            plenum_loss_coefficient: Some(0.4),
            fixed_pressure_loss_pa: Some(10.0),
            spray_zone_height_m: Some(0.6),
            spray_zone: Some(spray_zone()),
            rain_zone_height_m: Some(1.4),
            rain_zone: Some(rain_zone()),
        },
        footprint_m2: 52.0,
        max_water_mass_flow_kg_s: 155.0,
        fill_depth_options_m: vec![1.2, 1.5, 1.8],
        fill_stacks: Vec::new(),
        compatible_fan_ids: vec!["AX-420".to_string(), "AX-500".to_string()],
    }
}

fn idcf_064() -> SelectionTower {
    SelectionTower {
        id: "IDCF-064".to_string(),
        tower_type: TowerType::Counterflow,
        physics: TowerRecord {
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
            spray_zone: Some(spray_zone()),
            rain_zone_height_m: Some(1.5),
            rain_zone: Some(rain_zone()),
        },
        footprint_m2: 78.0,
        max_water_mass_flow_kg_s: 265.0,
        fill_depth_options_m: vec![1.2, 1.5, 1.8, 2.1],
        fill_stacks: Vec::new(),
        compatible_fan_ids: vec!["AX-500".to_string(), "AX-600".to_string()],
    }
}

fn idcf_096() -> SelectionTower {
    SelectionTower {
        id: "IDCF-096".to_string(),
        tower_type: TowerType::Counterflow,
        physics: TowerRecord {
            fill_area_m2: 96.0,
            air_free_area_m2: Some(96.0),
            drift_area_m2: Some(92.0),
            inlet_area_m2: Some(43.0),
            plenum_area_m2: None,
            fan_stack_area_m2: None,
            stack_recovery_factor: Some(0.35),
            inlet_loss_coefficient: Some(2.8),
            distribution_loss_coefficient: Some(4.2),
            support_loss_coefficient: Some(2.0),
            plenum_loss_coefficient: Some(0.36),
            fixed_pressure_loss_pa: Some(10.0),
            spray_zone_height_m: Some(0.7),
            spray_zone: Some(spray_zone()),
            rain_zone_height_m: Some(1.6),
            rain_zone: Some(rain_zone()),
        },
        footprint_m2: 113.0,
        max_water_mass_flow_kg_s: 410.0,
        fill_depth_options_m: vec![1.2, 1.5, 1.8, 2.1],
        fill_stacks: Vec::new(),
        compatible_fan_ids: vec!["AX-600".to_string(), "AX-700".to_string()],
    }
}

fn idxf_080() -> SelectionTower {
    SelectionTower {
        id: "IDXF-080".to_string(),
        tower_type: TowerType::Crossflow,
        physics: TowerRecord {
            fill_area_m2: 80.0,
            air_free_area_m2: Some(72.0),
            drift_area_m2: Some(76.0),
            inlet_area_m2: Some(88.0),
            plenum_area_m2: None,
            fan_stack_area_m2: None,
            stack_recovery_factor: Some(0.25),
            inlet_loss_coefficient: Some(1.2),
            distribution_loss_coefficient: Some(2.6),
            support_loss_coefficient: Some(1.6),
            plenum_loss_coefficient: Some(0.34),
            fixed_pressure_loss_pa: Some(8.0),
            spray_zone_height_m: Some(0.5),
            spray_zone: Some(spray_zone()),
            rain_zone_height_m: Some(0.9),
            rain_zone: Some(rain_zone()),
        },
        footprint_m2: 118.0,
        max_water_mass_flow_kg_s: 310.0,
        fill_depth_options_m: vec![1.2, 1.5, 1.8],
        fill_stacks: Vec::new(),
        compatible_fan_ids: vec!["AX-500".to_string(), "AX-600".to_string()],
    }
}

fn fill(
    id: &str,
    compatible_tower_types: &[TowerType],
    allowed_water_quality_classes: &[&str],
    thermal: ZoneCorrelation,
    pressure: FillPressureCorrelation,
    limits: FillLimits,
) -> SelectionFill {
    SelectionFill {
        physics: FillRecord {
            id: id.to_string(),
            thermal: Some(thermal),
            pressure: Some(pressure),
            limits,
            allowed_water_quality_classes: allowed_water_quality_classes
                .iter()
                .map(|class| class.to_string())
                .collect(),
        },
        compatible_tower_types: compatible_tower_types.to_vec(),
    }
}

fn limits(
    min_water: f64,
    max_water: f64,
    min_dry_air: f64,
    max_dry_air: f64,
    max_water_temperature_c: f64,
) -> FillLimits {
    FillLimits {
        min_water_loading_kg_m2_s: min_water,
        max_water_loading_kg_m2_s: max_water,
        min_dry_air_loading_kg_m2_s: min_dry_air,
        max_dry_air_loading_kg_m2_s: max_dry_air,
        max_water_temperature_c,
    }
}

fn pressure(
    coefficient_pa_per_m: f64,
    water_exponent: f64,
    air_exponent: f64,
) -> FillPressureCorrelation {
    FillPressureCorrelation {
        coefficient_pa_per_m,
        reference_water_loading_kg_m2_s: 3.0,
        reference_dry_air_loading_kg_m2_s: 2.0,
        water_exponent,
        air_exponent,
    }
}

fn fills() -> Vec<SelectionFill> {
    let counterflow = [TowerType::Counterflow];
    let both = [TowerType::Counterflow, TowerType::Crossflow];
    vec![
        fill(
            "FILM-CF19",
            &counterflow,
            &["clean"],
            zone(1.28, 3.0, 2.0, -0.34, 0.46),
            pressure(78.0, 0.18, 1.78),
            limits(1.5, 5.0, 1.0, 3.2, 55.0),
        ),
        fill(
            "FILM-OF25",
            &both,
            &["clean", "moderate"],
            zone(1.12, 3.0, 2.0, -0.31, 0.44),
            pressure(61.0, 0.14, 1.72),
            limits(1.3, 5.4, 0.9, 3.3, 60.0),
        ),
        fill(
            "FILM-VF38",
            &both,
            &["clean", "moderate", "dirty"],
            zone(0.94, 3.0, 2.0, -0.27, 0.39),
            pressure(43.0, 0.10, 1.67),
            limits(1.1, 6.2, 0.8, 3.5, 75.0),
        ),
        fill(
            "TRICKLE-50",
            &both,
            &["moderate", "dirty"],
            zone(0.78, 3.0, 2.0, -0.22, 0.34),
            pressure(31.0, 0.08, 1.58),
            limits(0.9, 7.0, 0.7, 3.7, 85.0),
        ),
        fill(
            "SPLASH-GRID",
            &both,
            &["moderate", "dirty"],
            zone(0.62, 3.0, 2.0, -0.18, 0.31),
            pressure(22.0, 0.06, 1.52),
            limits(0.7, 8.0, 0.6, 4.0, 90.0),
        ),
    ]
}

fn drift_curve(points: &[(f64, f64, f64)]) -> Vec<DriftCurvePoint> {
    points
        .iter()
        .map(
            |(face_velocity_ms, drift_ppm, pressure_drop_pa)| DriftCurvePoint {
                face_velocity_ms: *face_velocity_ms,
                drift_ppm: *drift_ppm,
                pressure_drop_pa: *pressure_drop_pa,
            },
        )
        .collect()
}

fn drift_eliminators() -> Vec<SelectionDriftEliminator> {
    vec![
        SelectionDriftEliminator {
            physics: DriftEliminatorRecord {
                id: "DE-2P-LP".to_string(),
                curve: drift_curve(&[
                    (1.0, 35.0, 7.0),
                    (1.5, 45.0, 13.0),
                    (2.0, 62.0, 23.0),
                    (2.5, 88.0, 37.0),
                    (3.0, 130.0, 55.0),
                    (3.5, 200.0, 78.0),
                ]),
            },
            max_water_temperature_c: 60.0,
        },
        SelectionDriftEliminator {
            physics: DriftEliminatorRecord {
                id: "DE-3P-10".to_string(),
                curve: drift_curve(&[
                    (1.0, 4.0, 10.0),
                    (1.5, 6.0, 18.0),
                    (2.0, 9.0, 31.0),
                    (2.5, 14.0, 49.0),
                    (3.0, 23.0, 72.0),
                    (3.5, 38.0, 101.0),
                ]),
            },
            max_water_temperature_c: 60.0,
        },
        SelectionDriftEliminator {
            physics: DriftEliminatorRecord {
                id: "DE-4P-ULTRA".to_string(),
                curve: drift_curve(&[
                    (1.0, 1.2, 15.0),
                    (1.5, 1.8, 27.0),
                    (2.0, 2.8, 45.0),
                    (2.5, 4.8, 69.0),
                    (3.0, 8.5, 100.0),
                    (3.5, 15.0, 138.0),
                ]),
            },
            max_water_temperature_c: 80.0,
        },
    ]
}

fn fan_curve(points: &[(f64, f64, f64)]) -> Vec<FanCurvePoint> {
    points
        .iter()
        .map(|(flow_m3_s, pressure_pa, efficiency)| FanCurvePoint {
            flow_m3_s: *flow_m3_s,
            pressure_pa: *pressure_pa,
            efficiency: *efficiency,
        })
        .collect()
}

fn fan(
    id: &str,
    stack_area_m2: f64,
    allowed_speed_ratio: [f64; 2],
    drive_efficiency: f64,
    motor_efficiency: f64,
    curve: Vec<FanCurvePoint>,
) -> SelectionFan {
    SelectionFan {
        physics: FanRecord {
            id: id.to_string(),
            stack_area_m2: Some(stack_area_m2),
            pressure_basis: Some(FanPressureBasis::Total),
            reference_density_kg_m3: Some(1.2),
            stack_recovery_factor: None,
            curve,
        },
        allowed_speed_ratio,
        // These fixtures predate the record's rated speed; the rpm read-out is not what they test.
        nominal_rpm: None,
        drive_efficiency,
        motor_efficiency,
    }
}

fn fans() -> Vec<SelectionFan> {
    vec![
        fan(
            "AX-420",
            13.854,
            [0.72, 1.12],
            0.95,
            0.94,
            fan_curve(&[
                (35.0, 390.0, 0.61),
                (65.0, 350.0, 0.72),
                (95.0, 275.0, 0.81),
                (120.0, 185.0, 0.80),
                (145.0, 65.0, 0.66),
            ]),
        ),
        fan(
            "AX-500",
            19.635,
            [0.70, 1.13],
            0.96,
            0.95,
            fan_curve(&[
                (55.0, 520.0, 0.63),
                (100.0, 470.0, 0.74),
                (145.0, 380.0, 0.83),
                (185.0, 245.0, 0.82),
                (225.0, 70.0, 0.67),
            ]),
        ),
        fan(
            "AX-600",
            28.274,
            [0.68, 1.14],
            0.97,
            0.955,
            fan_curve(&[
                (85.0, 650.0, 0.65),
                (145.0, 585.0, 0.76),
                (210.0, 465.0, 0.85),
                (270.0, 300.0, 0.84),
                (325.0, 85.0, 0.69),
            ]),
        ),
        fan(
            "AX-700",
            38.485,
            [0.66, 1.15],
            0.97,
            0.96,
            fan_curve(&[
                (120.0, 760.0, 0.66),
                (205.0, 680.0, 0.78),
                (295.0, 535.0, 0.86),
                (375.0, 340.0, 0.85),
                (450.0, 95.0, 0.70),
            ]),
        ),
    ]
}

fn nozzles() -> Vec<NozzleRecord> {
    vec![
        NozzleRecord {
            id: "NZ-20".to_string(),
            name: "Illustrative 20 mm Full-Cone Nozzle".to_string(),
            discharge_coefficient: 0.72,
            orifice_diameter_m: 0.020,
            reference_water_density_kg_m3: None,
        },
        NozzleRecord {
            id: "NZ-25".to_string(),
            name: "Illustrative 25 mm Full-Cone Nozzle".to_string(),
            discharge_coefficient: 0.74,
            orifice_diameter_m: 0.025,
            reference_water_density_kg_m3: None,
        },
        NozzleRecord {
            id: "NZ-32".to_string(),
            name: "Illustrative 32 mm Full-Cone Nozzle".to_string(),
            discharge_coefficient: 0.76,
            orifice_diameter_m: 0.032,
            reference_water_density_kg_m3: None,
        },
        NozzleRecord {
            id: "NZ-40".to_string(),
            name: "Illustrative 40 mm Full-Cone Nozzle".to_string(),
            discharge_coefficient: 0.77,
            orifice_diameter_m: 0.040,
            reference_water_density_kg_m3: None,
        },
    ]
}

fn quality_factors() -> Vec<(&'static str, WaterQualityFactor)> {
    vec![
        (
            "clean",
            WaterQualityFactor {
                thermal_multiplier: 1.0,
                pressure_multiplier: 1.0,
            },
        ),
        (
            "moderate",
            WaterQualityFactor {
                thermal_multiplier: 0.92,
                pressure_multiplier: 1.12,
            },
        ),
        (
            "dirty",
            WaterQualityFactor {
                thermal_multiplier: 0.80,
                pressure_multiplier: 1.35,
            },
        ),
    ]
}

fn bundled_catalog() -> SelectionCatalog {
    SelectionCatalog {
        metadata: CatalogMetadata {
            id: "illustrative-catalog-v0.1".to_string(),
            revision: "2026-08-13".to_string(),
            status: "SYNTHETIC / NOT VENDOR DATA".to_string(),
        },
        water_quality_factors: quality_factors()
            .into_iter()
            .map(|(name, factor)| (name.to_string(), factor))
            .collect(),
        towers: vec![idcf_040(), idcf_064(), idcf_096(), idxf_080()],
        fills: fills(),
        drift_eliminators: drift_eliminators(),
        fans: fans(),
        nozzles: nozzles(),
    }
}

/// The single-cell catalog the focused tests run on: `IDCF-064` with `FILM-OF25`, `DE-3P-10`
/// and `AX-500`.
fn small_catalog() -> SelectionCatalog {
    let mut catalog = bundled_catalog();
    catalog.towers.retain(|tower| tower.id == "IDCF-064");
    catalog.fills.retain(|fill| fill.physics.id == "FILM-OF25");
    catalog
        .drift_eliminators
        .retain(|drift| drift.physics.id == "DE-3P-10");
    catalog.fans.retain(|fan| fan.physics.id == "AX-500");
    catalog
}

fn assert_close(actual: f64, expected: f64, relative: f64, label: &str) {
    let difference = (actual - expected).abs();
    assert!(
        difference <= relative * expected.abs(),
        "{label}: got {actual}, expected {expected} (|Δ| {difference})"
    );
}

fn identity(candidate: &synergy_drafthouse::SelectionCandidate) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        candidate.tower_id,
        candidate.fill_id,
        candidate.drift_eliminator_id,
        candidate.fan_id,
        candidate.fill_depth_m,
        candidate.speed_ratio
    )
}

/* ---------------- focused tests on the single-cell catalog ---------------- */

#[test]
fn the_reference_defaults_are_pinned() {
    let requirements = default_selection_requirements();
    assert_eq!(requirements.water_mass_flow_kg_s, 200.0);
    assert_eq!(requirements.hot_water_c, 42.0);
    assert_eq!(requirements.target_cold_water_c, 32.0);
    assert_eq!(requirements.wet_bulb_c, 27.0);
    assert_eq!(requirements.dry_bulb_c, 33.0);
    assert_eq!(requirements.pressure_pa, 101_325.0);
    assert_eq!(requirements.salinity_g_kg, 0.0);
    assert_eq!(requirements.water_quality_class, "moderate");
    assert_eq!(requirements.cycles_of_concentration, 4.0);
    assert_eq!(requirements.max_drift_ppm, 30.0);
    assert_eq!(requirements.max_electrical_input_kw, 75.0);
    assert_eq!(requirements.max_footprint_m2, 130.0);
    assert_eq!(requirements.minimum_thermal_margin_c, 0.3);
    assert_eq!(requirements.nozzle_pressure_drop_pa, 65_000.0);
    assert_eq!(requirements.speed_ratios, [0.78, 0.88, 0.98, 1.08]);
    assert_eq!(Objective::default(), Objective::LeastOverCapacity);
}

/// `tests/selection.test.js` — 'default synthetic catalog returns feasible constrained
/// selections' — on the single-cell catalog, every constraint checked on the winner.
#[test]
fn a_small_catalog_returns_feasible_constrained_candidates() {
    let catalog = small_catalog();
    let input = SelectionInput::new(&catalog);
    let result = select_cooling_tower_components(&input).expect("the selection runs");
    assert!(result.feasible_candidate_count > 0);
    assert_eq!(result.results.len(), result.feasible_candidate_count);
    assert_eq!(result.results[0].rank, 1);
    for (index, candidate) in result.results.iter().enumerate() {
        assert_eq!(candidate.rank, index + 1);
        assert!(candidate.thermal.cold_water_c <= result.requirements.target_cold_water_c);
        assert!(candidate.thermal_margin_c >= result.requirements.minimum_thermal_margin_c);
        assert!(candidate.airside.drift_ppm <= result.requirements.max_drift_ppm);
        assert!(candidate.electrical_input_kw <= result.requirements.max_electrical_input_kw);
        assert!(candidate.nozzle.count > 0.0);
        assert!(
            candidate.capacity_kg_s.is_some(),
            "capacity resolves for every feasible candidate"
        );
        assert!(candidate.capability_ratio.is_some());
        assert_eq!(candidate.provenance_status, catalog.metadata.status);
    }
    // The winner is the smallest unit: its capacity is the minimum over the feasible set.
    let smallest = result
        .results
        .iter()
        .map(|candidate| candidate.capacity_kg_s.expect("resolved"))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(result.results[0].capacity_kg_s, Some(smallest));
}

/// The inverse solve, pinned. The case: the single-cell catalog's least-over-capacity winner
/// (`IDCF-064 / FILM-OF25 / DE-3P-10 / AX-500`, depth 1.5 m, speed ratio 0.78) at the
/// reference's default duty. The expected 225.32138116657734 was measured with the reference
/// engine's own functions (2026-09-19, `solveFanSystemIntersection` + `solveColdWaterTemperature`
/// per flow, then `solveBracketedRoot` on water mass flow, tolerance 1e-9 / xTolerance 1e-6 —
/// the same bracketed bisection `scripts/parity/run.mjs` performs), and the harness reproduces
/// it for every candidate of the bundled catalog. The winner's identity is reproducible from
/// the selection itself.
#[test]
fn the_capacity_inverse_solve_is_pinned_and_self_consistent() {
    let catalog = small_catalog();
    let input = SelectionInput::new(&catalog);
    let result = select_cooling_tower_components(&input).expect("the selection runs");
    let winner = &result.results[0];
    assert_eq!(
        identity(winner),
        "IDCF-064|FILM-OF25|DE-3P-10|AX-500|1.5|0.78"
    );
    let capacity = winner.capacity_kg_s.expect("capacity resolves");
    assert_close(capacity, 225.321_381_166_577_34, 1e-6, "capacityKgS");

    // Forward check in the port itself: re-evaluating the unit at the capacity must reproduce
    // the required cold-water temperature to the solver's own floor.
    let residual = re_evaluate_unit(winner, &result.requirements, capacity)
        - result.requirements.target_cold_water_c;
    assert!(
        residual.abs() <= 1e-6,
        "the forward evaluation at the capacity missed the target by {residual} K"
    );
    // And the capability ratio is the reported available KaV/L over the required demand at the
    // required cold-water temperature.
    let demand = synergy_drafthouse::merkel_demand(&synergy_drafthouse::MerkelInput {
        pressure_pa: result.requirements.pressure_pa,
        salinity_g_kg: result.requirements.salinity_g_kg,
        integration: synergy_drafthouse::Integration::Chebyshev4,
        ..synergy_drafthouse::MerkelInput::new(
            result.requirements.hot_water_c,
            result.requirements.target_cold_water_c,
            result.requirements.wet_bulb_c,
            result.requirements.dry_bulb_c,
            result.requirements.water_mass_flow_kg_s / winner.airside.dry_air_mass_flow_kg_s,
        )
    })
    .expect("the demand evaluates");
    assert_close(
        winner.capability_ratio.expect("ratio resolves"),
        winner.airside.available_merkel_number / demand.merkel_number,
        1e-9,
        "capabilityRatio",
    );
}

/// Re-evaluate one candidate's unit at one water flow, using only the public ported
/// primitives — the same call sequence `selection.js` makes, built independently here.
fn re_evaluate_unit(
    candidate: &synergy_drafthouse::SelectionCandidate,
    requirements: &SelectionRequirements,
    water_mass_flow_kg_s: f64,
) -> f64 {
    let catalog = bundled_catalog();
    let tower = catalog
        .towers
        .iter()
        .find(|tower| tower.id == candidate.tower_id)
        .expect("the winner's tower is in the catalog");
    let fill = catalog
        .fills
        .iter()
        .find(|fill| fill.physics.id == candidate.fill_id)
        .expect("the winner's fill is in the catalog");
    let drift = catalog
        .drift_eliminators
        .iter()
        .find(|drift| drift.physics.id == candidate.drift_eliminator_id)
        .expect("the winner's drift eliminator is in the catalog");
    let fan = catalog
        .fans
        .iter()
        .find(|fan| fan.physics.id == candidate.fan_id)
        .expect("the winner's fan is in the catalog");
    let quality = catalog
        .water_quality_factors
        .get(&requirements.water_quality_class)
        .expect("the quality class is in the catalog");
    let inlet_air = synergy_drafthouse::psychrometric_state(
        synergy_drafthouse::PsychrometricStateInput::from_wet_bulb(
            requirements.dry_bulb_c,
            requirements.wet_bulb_c,
        )
        .with_pressure(requirements.pressure_pa),
    )
    .expect("the inlet state solves");
    let breakdown =
        |flow_m3_s: f64| -> Result<SystemPressureBreakdown, synergy_drafthouse::DomainError> {
            synergy_drafthouse::system_pressure_breakdown(
                synergy_drafthouse::SystemPressureBreakdownInput {
                    tower: &tower.physics,
                    fill: &fill.physics,
                    fill_depth_m: candidate.fill_depth_m,
                    drift_eliminator: &drift.physics,
                    fan: Some(&fan.physics),
                    volumetric_air_flow_m3_s: flow_m3_s,
                    dry_air_density_kg_m3: inlet_air.dry_air_density_kg_m3,
                    moist_air_density_kg_m3: inlet_air.moist_air_density_kg_m3,
                    water_mass_flow_kg_s,
                    thermal_multiplier: quality.thermal_multiplier,
                    pressure_multiplier: quality.pressure_multiplier,
                },
            )
        };
    let point = synergy_drafthouse::solve_fan_system_intersection(
        &synergy_drafthouse::FanSystemIntersectionInput::new(
            &fan.physics,
            inlet_air.moist_air_density_kg_m3,
        )
        .with_speed_ratio(candidate.speed_ratio),
        |flow_m3_s| Ok(breakdown(flow_m3_s)?.total_pa),
    )
    .expect("the fan balance solves");
    let airside = breakdown(point.flow_m3_s).expect("the breakdown evaluates");
    synergy_drafthouse::solve_cold_water_temperature(
        &synergy_drafthouse::ColdWaterTemperatureInput {
            pressure_pa: requirements.pressure_pa,
            salinity_g_kg: requirements.salinity_g_kg,
            integration: synergy_drafthouse::Integration::Chebyshev4,
            ..synergy_drafthouse::ColdWaterTemperatureInput::new(
                requirements.hot_water_c,
                requirements.wet_bulb_c,
                requirements.dry_bulb_c,
                water_mass_flow_kg_s / airside.dry_air_mass_flow_kg_s,
                airside.available_merkel_number,
            )
        },
    )
    .expect("the thermal solve runs")
    .cold_water_c
}

/// Duty and thermal margin are constraints, never sort keys: the candidate that ranks first
/// under `LowestElectricalInput` while the margin floor is 0.3 K is refused under every one of
/// the four objectives once the floor rises above its margin.
#[test]
fn a_candidate_below_the_required_margin_cannot_rank_under_any_objective() {
    let catalog = small_catalog();

    let baseline = select_cooling_tower_components(&SelectionInput::new(&catalog))
        .expect("the baseline selection runs");
    let promoted = baseline
        .results
        .iter()
        .min_by(|a, b| a.electrical_input_kw.total_cmp(&b.electrical_input_kw))
        .expect("there is a feasible set");
    let promoted_identity = identity(promoted);
    let promoted_margin = promoted.thermal_margin_c;
    assert!(
        (0.3..0.6).contains(&promoted_margin),
        "the case must straddle the floor"
    );

    // Raise the floor above that candidate's margin. It must leave every order, and the count
    // of 'thermal duty' refusals must rise by exactly one.
    let stricter = SelectionRequirements {
        minimum_thermal_margin_c: 0.6,
        ..default_selection_requirements()
    };
    let baseline_refusals = baseline
        .rejection_summary
        .iter()
        .find(|(label, _)| label == "thermal duty")
        .map_or(0, |(_, count)| *count);
    for objective in Objective::ALL {
        let result = select_cooling_tower_components(
            &SelectionInput::new(&catalog)
                .with_requirements(stricter.clone())
                .with_objective(objective),
        )
        .expect("the strict selection runs");
        let stricter_refusals = result
            .rejection_summary
            .iter()
            .find(|(label, _)| label == "thermal duty")
            .map_or(0, |(_, count)| *count);
        assert_eq!(
            stricter_refusals,
            baseline_refusals + 1,
            "the below-margin candidate is refused exactly once more under {objective:?}"
        );
        assert!(
            !result
                .results
                .iter()
                .any(|candidate| identity(candidate) == promoted_identity),
            "{promoted_identity} ranked under {objective:?} below the required margin"
        );
        if let Some(first) = result.results.first() {
            assert!(
                first.thermal_margin_c >= 0.6,
                "the {objective:?} winner sits below the margin floor"
            );
        }
    }
}

#[test]
fn the_temperature_order_and_the_quality_class_are_refused() {
    let catalog = small_catalog();
    let inverted = SelectionRequirements {
        target_cold_water_c: 45.0,
        ..default_selection_requirements()
    };
    let error =
        select_cooling_tower_components(&SelectionInput::new(&catalog).with_requirements(inverted))
            .expect_err("hot water must exceed the target cold water");
    assert_eq!(
        error.message(),
        "Selection temperatures must satisfy hot water > target cold water > wet bulb."
    );
    let unknown = SelectionRequirements {
        water_quality_class: "brackish".to_string(),
        ..default_selection_requirements()
    };
    let error =
        select_cooling_tower_components(&SelectionInput::new(&catalog).with_requirements(unknown))
            .expect_err("the class must be known");
    assert_eq!(error.message(), "Unknown water-quality class.");
}

/* ---------------- the bundled catalog: issue #1's anchors ---------------- */

/// The four-objective anchors on the bundled catalog, matching issue #1's measured table
/// (least over-capacity and lowest electrical input) and the reference engine's own values
/// for the other two. Also checks that every order is a total order built from the declared
/// objective plus the documented tie-break chain, and that the margin floor holds in each.
///
/// The full-bundled-catalog run computes the capacity and capability ratio of all 202 feasible
/// candidates, so this test is the slow one (tens of seconds in a debug build).
#[test]
fn the_bundled_catalog_winners_are_the_issue_1_anchors() {
    let catalog = bundled_catalog();
    let run = run_selection(&SelectionInput::new(&catalog)).expect("the selection runs");
    assert_eq!(run.feasible_candidate_count(), 202);
    assert_eq!(
        run.rejection_summary,
        vec![
            ("power limit".to_string(), 249),
            ("drift limit".to_string(), 235),
            ("thermal duty".to_string(), 153),
            ("drift velocity range".to_string(), 125),
            ("fill operating envelope".to_string(), 92),
            ("fill water-quality compatibility".to_string(), 2),
            ("tower water-flow limit".to_string(), 1),
        ]
    );

    let anchors = [
        (
            Objective::LeastOverCapacity,
            "IDCF-064|TRICKLE-50|DE-4P-ULTRA|AX-500|1.8|0.78",
        ),
        (
            Objective::LowestElectricalInput,
            "IDCF-064|TRICKLE-50|DE-3P-10|AX-500|1.8|0.78",
        ),
        (
            Objective::LowestMakeupWater,
            "IDCF-064|TRICKLE-50|DE-4P-ULTRA|AX-500|1.8|0.78",
        ),
        (
            Objective::LowestTotalAirSidePressure,
            "IDCF-096|SPLASH-GRID|DE-3P-10|AX-600|1.5|0.78",
        ),
    ];
    for (objective, expected) in anchors {
        let order = run.order_for(objective);
        assert_eq!(
            identity(&run.candidates[order[0]]),
            expected,
            "{objective:?} winner"
        );
        // Every order is the documented total order: the objective's own metric, then the
        // fixed tie-break chain, then the identity — recomputed here independently.
        let mut reference = order.clone();
        reference.sort_by(|&a, &b| {
            compare_documented(&run.candidates[a], &run.candidates[b], objective)
        });
        assert_eq!(
            order, reference,
            "{objective:?} order is not the documented one"
        );
        assert!(
            order.iter().all(|&index| {
                run.candidates[index].thermal_margin_c >= run.requirements.minimum_thermal_margin_c
            }),
            "the margin floor holds in every order"
        );
    }

    let over_capacity = run.candidates[run.order_for(Objective::LeastOverCapacity)[0]].clone();
    assert_close(
        over_capacity.capacity_kg_s.expect("resolved"),
        216.859_373_822_808_27,
        1e-6,
        "least-over-capacity capacityKgS",
    );
    assert_close(
        over_capacity.capability_ratio.expect("resolved"),
        1.107_910_430_164_819,
        1e-6,
        "least-over-capacity capabilityRatio (issue #1 records 1.108)",
    );
    assert_close(
        over_capacity.electrical_input_kw,
        31.578_740_358_710_32,
        1e-6,
        "least-over-capacity electricalInputKW (issue #1 records 31.58 kW)",
    );

    let electrical = run.candidates[run.order_for(Objective::LowestElectricalInput)[0]].clone();
    assert_close(
        electrical.electrical_input_kw,
        31.131_126,
        1e-6,
        "lowest-electrical-input electricalInputKW (issue #1 records 31.13 kW)",
    );
    assert_close(
        electrical.capability_ratio.expect("resolved"),
        1.140_135_271_982_506_4,
        1e-6,
        "lowest-electrical-input capabilityRatio (issue #1 records 1.140)",
    );

    let pressure = run.candidates[run.order_for(Objective::LowestTotalAirSidePressure)[0]].clone();
    assert_close(
        pressure.airside.total_pa,
        181.496_5,
        1e-4,
        "lowest-total-air-side-pressure totalPa",
    );
}

/// The documented tie-break chain, recomputed in the test: objective metric, then electrical
/// input, make-up water, total air-side pressure, capability ratio, capacity, identity.
fn compare_documented(
    a: &synergy_drafthouse::SelectionCandidate,
    b: &synergy_drafthouse::SelectionCandidate,
    objective: Objective,
) -> std::cmp::Ordering {
    let optional = |left: Option<f64>, right: Option<f64>| match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };
    optional(a.objective_metric(objective), b.objective_metric(objective))
        .then_with(|| a.electrical_input_kw.total_cmp(&b.electrical_input_kw))
        .then_with(|| {
            a.water_balance
                .makeup_kg_s
                .total_cmp(&b.water_balance.makeup_kg_s)
        })
        .then_with(|| a.airside.total_pa.total_cmp(&b.airside.total_pa))
        .then_with(|| optional(a.capability_ratio, b.capability_ratio))
        .then_with(|| optional(a.capacity_kg_s, b.capacity_kg_s))
        .then_with(|| {
            let (a_tower, a_fill, a_drift, a_fan, a_depth, a_speed) = a.identity();
            let (b_tower, b_fill, b_drift, b_fan, b_depth, b_speed) = b.identity();
            a_tower
                .cmp(b_tower)
                .then_with(|| a_fill.cmp(b_fill))
                .then_with(|| a_drift.cmp(b_drift))
                .then_with(|| a_fan.cmp(b_fan))
                .then_with(|| a_depth.total_cmp(&b_depth))
                .then_with(|| a_speed.total_cmp(&b_speed))
        })
}

/// `rank_candidates` is the public seam the CLI's `--all-orders` uses; it must agree with the
/// order `run_selection` reports for the same objective.
#[test]
fn rank_candidates_agrees_with_the_run_order() {
    let catalog = small_catalog();
    let run = run_selection(&SelectionInput::new(&catalog)).expect("the run works");
    let mut candidates = run.candidates.clone();
    rank_candidates(&mut candidates, Objective::LowestMakeupWater);
    let ranked: Vec<String> = candidates.iter().map(identity).collect();
    let expected: Vec<String> = run
        .order_for(Objective::LowestMakeupWater)
        .iter()
        .map(|&index| identity(&run.candidates[index]))
        .collect();
    assert_eq!(ranked, expected);
}

/* ---------------- the tie-break chain's interior links ---------------- */

/// The base both exact-tie catalogs start from: the single-cell catalog at one fill depth, so
/// the order is the tie-break chain's and not a depth or speed grid's.
fn tie_base_catalog() -> SelectionCatalog {
    let mut catalog = small_catalog();
    catalog.towers[0].fill_depth_options_m = vec![1.5];
    catalog
}

/// The catalog of the **electrical-input** case. `AX-500-ECO` is `AX-500` record for record
/// except its **motor efficiency** — the fan curve, the stack area, the pressure basis, the
/// drive efficiency, the speed window and every physics input are identical. `motorEfficiency`
/// enters `electricalInputKW` and nothing else (`chooseStandardMotor` is called with the drive
/// efficiency), so the two units tie on the objective metric, on the air-side solve, on the
/// thermal solve, on make-up water and on the capability ratio: they differ on electrical input
/// alone.
///
/// The new id sorts *after* the base one (`AX-500` < `AX-500-ECO`) while its electrical input is
/// the lower of the two, so the identity tail orders the pair against the chain. A case that
/// dropped the interior links fails rather than passes.
fn efficiency_tie_catalog() -> SelectionCatalog {
    let mut catalog = tie_base_catalog();
    catalog.towers[0]
        .compatible_fan_ids
        .push("AX-500-ECO".to_string());
    let mut eco = catalog
        .fans
        .iter()
        .find(|fan| fan.physics.id == "AX-500")
        .expect("the single-cell catalog carries AX-500")
        .clone();
    eco.physics.id = "AX-500-ECO".to_string();
    eco.motor_efficiency = 0.98;
    catalog.fans.push(eco);
    catalog
}

/// The catalog of the **make-up water** case. `DE-3P-10-LP` is `DE-3P-10` record for record
/// except the **drift-ppm column** of its curve; the pressure-drop column — the only one the
/// fan/system balance reads — is identical, so the operating point, the electrical input, the
/// capacity, the total air-side pressure and the capability ratio are bit-identical and the pair
/// differs on make-up water alone.
///
/// The make-up water is drift-sensitive only where the balance's blowdown term floors at zero
/// (`coolingTowerWaterBalance`: `blowdown = evaporation / (cycles − 1) − drift`, floored;
/// `makeup = evaporation + drift + blowdown`, in which the drift cancels algebraically while
/// blowdown stays positive). The case therefore runs at 3000 cycles of concentration, where the
/// base eliminator's blowdown floors and the halved one's does not — the two make-up values
/// below are pinned, so the case fails loudly if that premise ever stops holding.
fn drift_tie_catalog() -> SelectionCatalog {
    let mut catalog = tie_base_catalog();
    let mut low_drift = catalog.drift_eliminators[0].clone();
    low_drift.physics.id = "DE-3P-10-LP".to_string();
    for point in &mut low_drift.physics.curve {
        point.drift_ppm /= 2.0;
    }
    catalog.drift_eliminators.push(low_drift);
    catalog
}

/// The chain's **first** interior link, on a synthetic catalog with an exact objective-metric
/// tie: `efficiency_tie_catalog()`'s two units tie on the objective metric — the capacities
/// compared below are the same `f64`, not merely close — and on every link after the decisive
/// one, and differ on electrical input alone. The order asserted is the electrical link's, and
/// it is the reverse of the identity order.
#[test]
fn the_electrical_link_orders_an_exact_objective_metric_tie() {
    let catalog = efficiency_tie_catalog();
    let requirements = SelectionRequirements {
        speed_ratios: vec![0.78],
        ..SelectionRequirements::default()
    };
    let input = SelectionInput::new(&catalog).with_requirements(requirements);
    let result = select_cooling_tower_components(&input).expect("the selection runs");

    assert_eq!(
        result.results.iter().map(identity).collect::<Vec<String>>(),
        vec![
            "IDCF-064|FILM-OF25|DE-3P-10|AX-500-ECO|1.5|0.78",
            "IDCF-064|FILM-OF25|DE-3P-10|AX-500|1.5|0.78",
        ],
        "the electrical link orders the tie, against the identity order"
    );

    let eco = &result.results[0];
    let base = &result.results[1];
    assert_eq!(eco.capacity_kg_s, base.capacity_kg_s);
    assert_eq!(
        eco.water_balance.makeup_kg_s,
        base.water_balance.makeup_kg_s
    );
    assert_eq!(eco.airside.total_pa, base.airside.total_pa);
    assert_eq!(eco.capability_ratio, base.capability_ratio);
    assert!(eco.electrical_input_kw < base.electrical_input_kw);
    // The identity tail reads `AX-500` before `AX-500-ECO`, the opposite of the order the chain
    // produces: the case is evidence about the chain, not about the names.
    assert!("AX-500" < "AX-500-ECO");
}

/// The chain's **second** interior link, the same way: `drift_tie_catalog()`'s two units tie on
/// the objective metric, on electrical input, on total air-side pressure and on the capability
/// ratio, and differ on make-up water alone. The lower make-up water ranks first; the identity
/// order is again the reverse.
#[test]
fn the_makeup_link_orders_an_exact_objective_metric_tie() {
    let catalog = drift_tie_catalog();
    let requirements = SelectionRequirements {
        speed_ratios: vec![0.78],
        cycles_of_concentration: 3000.0,
        ..SelectionRequirements::default()
    };
    let input = SelectionInput::new(&catalog).with_requirements(requirements);
    let result = select_cooling_tower_components(&input).expect("the selection runs");

    assert_eq!(
        result.results.iter().map(identity).collect::<Vec<String>>(),
        vec![
            "IDCF-064|FILM-OF25|DE-3P-10-LP|AX-500|1.5|0.78",
            "IDCF-064|FILM-OF25|DE-3P-10|AX-500|1.5|0.78",
        ],
        "the make-up link orders the tie, against the identity order"
    );

    let low = &result.results[0];
    let base = &result.results[1];
    assert_eq!(low.capacity_kg_s, base.capacity_kg_s);
    assert_eq!(low.electrical_input_kw, base.electrical_input_kw);
    assert_eq!(low.airside.total_pa, base.airside.total_pa);
    assert_eq!(low.capability_ratio, base.capability_ratio);
    assert_eq!(
        low.water_balance.evaporation_kg_s,
        base.water_balance.evaporation_kg_s
    );
    assert!(low.water_balance.makeup_kg_s < base.water_balance.makeup_kg_s);

    // The premise of the case, pinned: the base eliminator's blowdown floors at zero (which is
    // what makes any make-up value drift-sensitive at all) and the halved one's does not.
    assert_eq!(base.water_balance.blowdown_kg_s, 0.0);
    assert!(low.water_balance.blowdown_kg_s > 0.0);
    assert!("DE-3P-10" < "DE-3P-10-LP");
}

/// One row of the link-by-link discrimination table: two candidates whose metrics differ on
/// exactly the named links, and the order the documented chain must produce.
struct LinkCase {
    /// What the row discriminates, for the failure message.
    label: &'static str,
    objective: Objective,
    /// `(electricalInputKW, makeupKgS, totalPa, capabilityRatio, capacityKgS)`
    a: [f64; 5],
    b: [f64; 5],
    /// Which side the documented chain must rank first.
    expected_first: &'static str,
}

/// A candidate of the row's pair: a real candidate from `small_catalog()` with its tie-break
/// metrics replaced by the row's values. The comparator reads those metrics as fields, and its
/// contract is the order it imposes on them; the rows below are that contract, one link at a
/// time. The tower ids (`TIE-B` for `a`, `TIE-A` for `b`) put the identity tail *against* the
/// row's expected order, so no row passes on the identity comparison.
fn engineered_candidate(
    base: &SelectionCandidate,
    tower_id: &str,
    fill_id: &str,
    metrics: [f64; 5],
) -> SelectionCandidate {
    let mut candidate = base.clone();
    candidate.tower_id = tower_id.to_string();
    candidate.fill_id = fill_id.to_string();
    candidate.electrical_input_kw = metrics[0];
    candidate.water_balance.makeup_kg_s = metrics[1];
    candidate.airside.total_pa = metrics[2];
    candidate.capability_ratio = Some(metrics[3]);
    candidate.capacity_kg_s = Some(metrics[4]);
    candidate
}

/// The rows: for each of the chain's five interior links, a pair that ties on every *other*
/// metric and differs on that one; and for each adjacent pair of links, a pair whose two
/// metrics are anti-correlated, so the links' **order** is exercised as well as their
/// direction. `a` is always the side the documented chain must rank first.
fn link_cases() -> Vec<LinkCase> {
    vec![
        // Direction, link 1 of the chain: electrical input, ascending.
        LinkCase {
            label: "electrical input ascending",
            objective: Objective::LeastOverCapacity,
            a: [9.0, 1.0, 100.0, 2.0, 300.0],
            b: [11.0, 1.0, 100.0, 2.0, 300.0],
            expected_first: "A",
        },
        // Direction, link 2: make-up water, ascending.
        LinkCase {
            label: "make-up water ascending",
            objective: Objective::LeastOverCapacity,
            a: [10.0, 1.0, 100.0, 2.0, 300.0],
            b: [10.0, 2.0, 100.0, 2.0, 300.0],
            expected_first: "A",
        },
        // Direction, link 3: total air-side pressure, ascending.
        LinkCase {
            label: "total air-side pressure ascending",
            objective: Objective::LeastOverCapacity,
            a: [10.0, 1.0, 99.0, 2.0, 300.0],
            b: [10.0, 1.0, 101.0, 2.0, 300.0],
            expected_first: "A",
        },
        // Direction, link 4: capability ratio, ascending.
        LinkCase {
            label: "capability ratio ascending",
            objective: Objective::LeastOverCapacity,
            a: [10.0, 1.0, 100.0, 1.9, 300.0],
            b: [10.0, 1.0, 100.0, 2.1, 300.0],
            expected_first: "A",
        },
        // Direction, link 5: capacity, ascending. Its own objective would make it the primary
        // metric, so this row ranks under an objective it does not carry.
        LinkCase {
            label: "capacity ascending",
            objective: Objective::LowestMakeupWater,
            a: [10.0, 1.0, 100.0, 2.0, 299.0],
            b: [10.0, 1.0, 100.0, 2.0, 301.0],
            expected_first: "A",
        },
        // Order, links 1-2: electrical input and make-up water anti-correlated, so the pair is
        // ordered by which link the chain consults first.
        LinkCase {
            label: "electrical input before make-up water",
            objective: Objective::LeastOverCapacity,
            a: [9.0, 2.0, 100.0, 2.0, 300.0],
            b: [11.0, 1.0, 100.0, 2.0, 300.0],
            expected_first: "A",
        },
        // Order, links 2-3: make-up water before total air-side pressure.
        LinkCase {
            label: "make-up water before total air-side pressure",
            objective: Objective::LeastOverCapacity,
            a: [10.0, 1.0, 101.0, 2.0, 300.0],
            b: [10.0, 2.0, 99.0, 2.0, 300.0],
            expected_first: "A",
        },
        // Order, links 3-4: total air-side pressure before capability ratio.
        LinkCase {
            label: "total air-side pressure before capability ratio",
            objective: Objective::LeastOverCapacity,
            a: [10.0, 1.0, 99.0, 2.1, 300.0],
            b: [10.0, 1.0, 101.0, 1.9, 300.0],
            expected_first: "A",
        },
        // Order, links 4-5: capability ratio before capacity.
        LinkCase {
            label: "capability ratio before capacity",
            objective: Objective::LowestMakeupWater,
            a: [10.0, 1.0, 100.0, 1.9, 301.0],
            b: [10.0, 1.0, 100.0, 2.1, 299.0],
            expected_first: "A",
        },
    ]
}

/// Every interior link of the chain, discriminated: each row ties on the objective metric and
/// on every other link and is ordered by its own. A link whose direction or position moved
/// flips its row (and for the anti-correlated rows, the link *order* flips them too).
#[test]
fn the_tie_break_chain_discriminates_every_interior_link() {
    let catalog = small_catalog();
    let run = run_selection(&SelectionInput::new(&catalog)).expect("the run works");
    let base = &run.candidates[0];
    for case in link_cases() {
        let mut pair = [
            engineered_candidate(base, "TIE-B", "FILL-A", case.a),
            engineered_candidate(base, "TIE-A", "FILL-Z", case.b),
        ];
        rank_candidates(&mut pair, case.objective);
        let first = if pair[0].tower_id == "TIE-B" {
            "A"
        } else {
            "B"
        };
        assert_eq!(first, case.expected_first, "{}", case.label);
    }
}

/// The identity tail on a full metric tie: every metric is equal, so the tail decides — and it
/// compares the tower before the fill, which matters because the two candidates disagree on the
/// two axes in opposite directions (`TIE-A`/`FILL-Z` against `TIE-B`/`FILL-A`).
#[test]
fn the_identity_tail_orders_a_full_metric_tie() {
    let catalog = small_catalog();
    let run = run_selection(&SelectionInput::new(&catalog)).expect("the run works");
    let metrics = [10.0, 1.0, 100.0, 2.0, 300.0];
    let mut pair = [
        engineered_candidate(&run.candidates[0], "TIE-A", "FILL-Z", metrics),
        engineered_candidate(&run.candidates[0], "TIE-B", "FILL-A", metrics),
    ];
    rank_candidates(&mut pair, Objective::LeastOverCapacity);
    assert_eq!(pair[0].tower_id, "TIE-A");
    assert_eq!(pair[1].tower_id, "TIE-B");
}

/* ---------------- the fail-closed money guard ---------------- */

/// The same fail-closed spirit as `tests/injection.test.js`: a money-ish identifier must not
/// reappear anywhere in the port's source, and the selector's own result must not carry one.
/// The guard is demonstrated to bite in a scratch tree (see the lane report).
#[test]
fn no_money_ish_identifier_exists_anywhere_in_the_port() {
    const TOKENS: [&str; 10] = [
        "cost",
        "price",
        "currency",
        "capex",
        "discount",
        "lifecycle",
        "money",
        "usd",
        "thb",
        "penalty",
    ];
    let mut sources = Vec::new();
    collect_sources(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    );
    assert!(
        sources.len() >= 10,
        "the port's source tree must be walked, found {}",
        sources.len()
    );
    for (path, source) in &sources {
        let lowered = source.to_lowercase();
        for token in TOKENS {
            assert!(
                !lowered.contains(token),
                "{path} contains the money-ish token {token:?}"
            );
        }
    }

    // The result the CLI actually prints must be just as clean.
    let (args, tower_spec) = small_catalog_args();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ct-engine"))
        .args(&args)
        .output()
        .expect("the CLI runs");
    assert!(output.status.success(), "the CLI refused a valid selection");
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    for token in TOKENS {
        assert!(
            !stdout.contains(token),
            "the CLI's selection result contains the money-ish token {token:?}"
        );
    }
    assert!(
        stdout.contains("capabilityratio"),
        "the CLI result must carry the new quantities"
    );

    // And a cost-shaped field is a usage error, never a silently ignored value.
    let poisoned = args
        .iter()
        .map(|argument| {
            if argument == &tower_spec {
                format!("{argument},baseCost:118000")
            } else {
                argument.clone()
            }
        })
        .collect::<Vec<String>>();
    let refused = std::process::Command::new(env!("CARGO_BIN_EXE_ct-engine"))
        .args(&poisoned)
        .output()
        .expect("the CLI runs");
    assert_eq!(
        refused.status.code(),
        Some(2),
        "a cost field must be a usage error"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("does not know the field"),
        "the refusal must name the field: {stderr}"
    );
}

/// Walk `directory` for `.rs` sources, as `(path, text)`.
fn collect_sources(directory: &std::path::Path, sink: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(directory).expect("the source directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            collect_sources(&path, sink);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path).expect("the source reads");
            sink.push((path.display().to_string(), text));
        }
    }
}

/// The `ct-engine select` arguments of the single-cell catalog, with a representative
/// requirements set. Returns the argument list and the tower spec it contains (so a test can
/// poison that exact spec).
fn small_catalog_args() -> (Vec<String>, String) {
    let catalog = small_catalog();
    let tower = &catalog.towers[0];
    let fill = &catalog.fills[0];
    let drift = &catalog.drift_eliminators[0];
    let fan = &catalog.fans[0];
    let zone_spec = |zone: &ZoneCorrelation| {
        format!(
            "{}|{}|{}|{}|{}",
            zone.coefficient_per_m,
            zone.reference_water_loading_kg_m2_s,
            zone.reference_dry_air_loading_kg_m2_s,
            zone.water_exponent,
            zone.air_exponent
        )
    };
    let tower_spec = format!(
        "id:{},type:{},fillAreaM2:{},airFreeAreaM2:{},driftAreaM2:{},inletAreaM2:{},stackRecoveryFactor:{},inletLossCoefficient:{},distributionLossCoefficient:{},supportLossCoefficient:{},plenumLossCoefficient:{},fixedPressureLossPa:{},sprayZoneHeightM:{},rainZoneHeightM:{},footprintM2:{},maxWaterMassFlowKgS:{},fillDepthOptionsM:{},compatibleFanIds:{},sprayZone:{},rainZone:{}",
        tower.id,
        tower.tower_type.as_str(),
        tower.physics.fill_area_m2,
        tower.physics.air_free_area_m2.expect("set"),
        tower.physics.drift_area_m2.expect("set"),
        tower.physics.inlet_area_m2.expect("set"),
        tower.physics.stack_recovery_factor.expect("set"),
        tower.physics.inlet_loss_coefficient.expect("set"),
        tower.physics.distribution_loss_coefficient.expect("set"),
        tower.physics.support_loss_coefficient.expect("set"),
        tower.physics.plenum_loss_coefficient.expect("set"),
        tower.physics.fixed_pressure_loss_pa.expect("set"),
        tower.physics.spray_zone_height_m.expect("set"),
        tower.physics.rain_zone_height_m.expect("set"),
        tower.footprint_m2,
        tower.max_water_mass_flow_kg_s,
        tower
            .fill_depth_options_m
            .iter()
            .map(f64::to_string)
            .collect::<Vec<String>>()
            .join("|"),
        tower.compatible_fan_ids.join("|"),
        zone_spec(tower.physics.spray_zone.as_ref().expect("set")),
        zone_spec(tower.physics.rain_zone.as_ref().expect("set")),
    );
    let fill_spec = format!(
        "id:{},compatibleTowerTypes:{},allowedWaterQualityClasses:{},thermal:{},pressure:{}|{}|{}|{}|{},limits:{}|{}|{}|{}|{}",
        fill.physics.id,
        fill.compatible_tower_types
            .iter()
            .map(|tower_type| tower_type.as_str())
            .collect::<Vec<&str>>()
            .join("|"),
        fill.physics.allowed_water_quality_classes.join("|"),
        zone_spec(fill.physics.thermal.as_ref().expect("set")),
        fill.physics.pressure.as_ref().expect("set").coefficient_pa_per_m,
        fill.physics.pressure.as_ref().expect("set").reference_water_loading_kg_m2_s,
        fill.physics.pressure.as_ref().expect("set").reference_dry_air_loading_kg_m2_s,
        fill.physics.pressure.as_ref().expect("set").water_exponent,
        fill.physics.pressure.as_ref().expect("set").air_exponent,
        fill.physics.limits.min_water_loading_kg_m2_s,
        fill.physics.limits.max_water_loading_kg_m2_s,
        fill.physics.limits.min_dry_air_loading_kg_m2_s,
        fill.physics.limits.max_dry_air_loading_kg_m2_s,
        fill.physics.limits.max_water_temperature_c,
    );
    let drift_spec = format!(
        "id:{},maxWaterTemperatureC:{},curve:{}",
        drift.physics.id,
        drift.max_water_temperature_c,
        drift
            .physics
            .curve
            .iter()
            .map(|point| format!(
                "{}:{}:{}",
                point.face_velocity_ms, point.drift_ppm, point.pressure_drop_pa
            ))
            .collect::<Vec<String>>()
            .join("|"),
    );
    let fan_spec = format!(
        "id:{},stackAreaM2:{},pressureBasis:total,referenceDensityKgM3:{},allowedSpeedRatio:{}|{},driveEfficiency:{},motorEfficiency:{},curve:{}",
        fan.physics.id,
        fan.physics.stack_area_m2.expect("set"),
        fan.physics.reference_density_kg_m3.expect("set"),
        fan.allowed_speed_ratio[0],
        fan.allowed_speed_ratio[1],
        fan.drive_efficiency,
        fan.motor_efficiency,
        fan.physics
            .curve
            .iter()
            .map(|point| format!("{}:{}:{}", point.flow_m3_s, point.pressure_pa, point.efficiency))
            .collect::<Vec<String>>()
            .join("|"),
    );
    let nozzle_specs = catalog
        .nozzles
        .iter()
        .map(|nozzle| {
            format!(
                "id:{},name:{},dischargeCoefficient:{},orificeDiameterM:{}",
                nozzle.id, nozzle.name, nozzle.discharge_coefficient, nozzle.orifice_diameter_m
            )
        })
        .collect::<Vec<String>>()
        .join(";");
    let args = vec![
        "select".to_string(),
        "--towers".to_string(),
        tower_spec.clone(),
        "--fills".to_string(),
        fill_spec,
        "--drift-eliminators".to_string(),
        drift_spec,
        "--fans".to_string(),
        fan_spec,
        "--nozzles".to_string(),
        nozzle_specs,
        "--quality-factors".to_string(),
        "clean:1:1;moderate:0.92:1.12;dirty:0.8:1.35".to_string(),
        "--catalog-id".to_string(),
        catalog.metadata.id.clone(),
        "--catalog-revision".to_string(),
        catalog.metadata.revision.clone(),
        "--catalog-status".to_string(),
        catalog.metadata.status.clone(),
    ];
    (args, tower_spec)
}

/* ---------------- issue #54: complete stack variants ---------------- */

/// The bundled catalog reduced to the records a mixed stack needs: `IDCF-064` on `AX-500`
/// through `DE-3P-10`, with the two counterflow/crossflow fills the variant cases below use.
fn pair_catalog() -> SelectionCatalog {
    let mut catalog = bundled_catalog();
    catalog.towers.retain(|tower| tower.id == "IDCF-064");
    catalog
        .fills
        .retain(|fill| fill.physics.id == "FILM-OF25" || fill.physics.id == "FILM-VF38");
    catalog
        .drift_eliminators
        .retain(|drift| drift.physics.id == "DE-3P-10");
    catalog.fans.retain(|fan| fan.physics.id == "AX-500");
    catalog
}

/// The reduced duty the recorded `select` case uses, which keeps both variants feasible.
fn variant_requirements() -> SelectionRequirements {
    SelectionRequirements {
        water_mass_flow_kg_s: 180.0,
        hot_water_c: 40.0,
        target_cold_water_c: 30.0,
        wet_bulb_c: 26.0,
        dry_bulb_c: 31.0,
        ..SelectionRequirements::default()
    }
}

/// The mixed stack issue #54 names, over the single-layer variant of the same tower.
fn variant_stacks() -> Vec<FillStack> {
    vec![
        FillStack::new(vec![
            FillLayer::new("FILM-OF25", 0.45),
            FillLayer::new("FILM-VF38", 0.9),
        ]),
        FillStack::new(vec![FillLayer::new("FILM-OF25", 1.35)]),
    ]
}

#[test]
fn a_tower_without_declared_stacks_keeps_the_single_fill_enumeration() {
    // Acceptance criterion 1's regression pin: a tower that declares no stacks is enumerated
    // exactly as before — one layer per compatible fill × depth option — and every candidate
    // carries that one layer, so the identity is the fill's own.
    let catalog = small_catalog();
    let run =
        run_selection(&SelectionInput::new(&catalog).with_requirements(variant_requirements()))
            .expect("the bundled catalog selects");
    assert!(!run.candidates.is_empty());
    for candidate in &run.candidates {
        assert_eq!(candidate.fill_layers.len(), 1);
        let layer = &candidate.fill_layers[0];
        assert_eq!(layer.position, 1);
        assert_eq!(layer.fill_id, candidate.fill_id);
        assert_eq!(layer.depth_m, candidate.fill_depth_m);
        assert_eq!(layer.limits, catalog.fills[0].physics.limits);
        assert_eq!(
            layer.merkel_number, candidate.airside.fill_merkel_number,
            "a one-layer stack's transfer number is the breakdown's fill term"
        );
    }
}

#[test]
fn a_tower_selects_over_its_complete_stack_variants() {
    // Acceptance criterion 6: the declared variants are the identities compared — a mixed
    // stack and a single-layer one — each carrying its own layers.
    let mut catalog = pair_catalog();
    let stacks = variant_stacks();
    catalog.towers[0].fill_stacks = stacks.clone();
    let run =
        run_selection(&SelectionInput::new(&catalog).with_requirements(variant_requirements()))
            .expect("the declared variants select");
    assert_eq!(
        run.candidates.len(),
        2,
        "one feasible candidate per variant"
    );

    let mixed = run
        .candidates
        .iter()
        .find(|candidate| candidate.fill_layers.len() == 2)
        .expect("the mixed variant is feasible at this duty");
    assert_eq!(mixed.fill_id, stacks[0].label());
    assert_eq!(mixed.fill_id, "FILM-OF25@0.45+FILM-VF38@0.9");
    assert_eq!(mixed.fill_depth_m, 1.35);
    assert_eq!(mixed.fill_layers[0].fill_id, "FILM-OF25");
    assert_eq!(mixed.fill_layers[0].depth_m, 0.45);
    assert_eq!(mixed.fill_layers[1].fill_id, "FILM-VF38");
    assert_eq!(mixed.fill_layers[1].depth_m, 0.9);
    // The candidate's fill term is its layers' own total, and the layers ride on the stack.
    assert_eq!(
        mixed.airside.fill_merkel_number,
        mixed.fill_layers[0].merkel_number + mixed.fill_layers[1].merkel_number
    );
    assert_eq!(
        mixed.airside.fill_pa,
        mixed.fill_layers[0].pressure_drop_pa + mixed.fill_layers[1].pressure_drop_pa
    );

    let single = run
        .candidates
        .iter()
        .find(|candidate| candidate.fill_layers.len() == 1)
        .expect("the single-layer variant is feasible at this duty");
    assert_eq!(
        single.fill_id, "FILM-OF25",
        "a one-layer stack keeps the fill identity"
    );
    assert_eq!(single.fill_depth_m, 1.35);
    assert_eq!(single.fill_layers[0].fill_id, "FILM-OF25");
    assert_eq!(single.fill_layers[0].depth_m, 1.35);
}

#[test]
fn a_declared_variant_whose_fill_is_not_in_the_catalog_is_refused_by_label() {
    let mut catalog = pair_catalog();
    catalog.towers[0].fill_stacks = vec![FillStack::new(vec![FillLayer::new("NOT-A-FILL", 1.5)])];
    let run = run_selection(&SelectionInput::new(&catalog)).expect("the run itself answers");
    assert!(run.candidates.is_empty());
    assert!(
        run.rejection_summary
            .iter()
            .any(|(label, count)| label == "fill not in the catalog" && *count == 1),
        "{:?}",
        run.rejection_summary
    );
}

#[test]
fn the_selection_surface_carries_the_layers_into_the_worked_steps() {
    // Acceptance criterion 7: the data-only surface carries the layers through — the reply's
    // `fillLayers` and the worked sheet's per-layer steps name the same layers in the same
    // top-first order.
    let (mut args, tower_spec) = small_catalog_args();
    let tower_index = args
        .iter()
        .position(|argument| argument == "--towers")
        .expect("--towers")
        + 1;
    args[tower_index] =
        format!("{tower_spec},fillStacks:FILM-OF25@0.45+FILM-VF38@0.9|FILM-OF25@1.35");
    let fills_index = args
        .iter()
        .position(|argument| argument == "--fills")
        .expect("--fills")
        + 1;
    args[fills_index] = format!(
        "{};id:FILM-VF38,compatibleTowerTypes:counterflow|crossflow,\
allowedWaterQualityClasses:clean|moderate|dirty,thermal:0.94|3|2|-0.27|0.39,\
pressure:43|3|2|0.1|1.67,limits:1.1|6.2|0.8|3.5|75",
        args[fills_index]
    );
    for (option, value) in [
        ("--water", "180"),
        ("--hot", "40"),
        ("--target-cold", "30"),
        ("--wb", "26"),
        ("--db", "31"),
    ] {
        args.push(option.to_string());
        args.push(value.to_string());
    }
    let reply: Value =
        serde_json::from_str(&cli::run(&args).expect("the variants select")).expect("a JSON reply");
    let results = reply["results"].as_array().expect("results");
    assert_eq!(results.len(), 2, "one feasible candidate per variant");
    // The mixed stack is the recommended unit at this duty (least over-capacity); the
    // single-layer variant is the other feasible candidate. Both are complete stacks.
    assert_eq!(results[0]["rank"], 1);
    assert_eq!(results[1]["rank"], 2);
    assert_eq!(results[1]["fillId"], "FILM-OF25");
    assert_eq!(results[1]["fillDepthM"], 1.35);
    let candidate = &results[0];
    assert_eq!(candidate["fillId"], "FILM-OF25@0.45+FILM-VF38@0.9");
    assert_eq!(candidate["fillDepthM"], 1.35);
    let layers = candidate["fillLayers"].as_array().expect("fillLayers");
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0]["fillId"], "FILM-OF25");
    assert_eq!(layers[0]["depthM"], 0.45);
    assert_eq!(layers[1]["fillId"], "FILM-VF38");
    assert_eq!(layers[1]["depthM"], 0.9);
    assert_eq!(layers[1]["position"], 2);

    let steps = reply["worked"]["steps"].as_array().expect("worked steps");
    let layer_steps: Vec<&Value> = steps
        .iter()
        .filter(|step| {
            step["label"]
                .as_str()
                .unwrap_or_default()
                .starts_with("Fill layer ")
        })
        .collect();
    assert_eq!(layer_steps.len(), layers.len(), "{steps:#?}");
    for (index, step) in layer_steps.iter().enumerate() {
        let layer = &layers[index];
        let label = step["label"].as_str().unwrap_or_default();
        assert!(
            label.contains(layer["fillId"].as_str().unwrap_or_default()),
            "{label} must name {}",
            layer["fillId"]
        );
        assert_eq!(
            step["value"], layer["merkelNumber"],
            "the step's value is the layer's own transfer number, read from the result"
        );
        assert_eq!(step["unit"], "-");
        assert_eq!(step["kind"], "calc");
    }
}
