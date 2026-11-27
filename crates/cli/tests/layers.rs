use drafthouse_cli as cli;
use drafthouse_engine::{
    check_stack_operating_envelope, fill_pressure_drop_pa, fill_thermal_merkel_number,
    layered_system_pressure_breakdown, resolve_fill_layers, system_pressure_breakdown,
    DriftCurvePoint, DriftEliminatorRecord, FanCurvePoint, FanPressureBasis, FanRecord, FillLayer,
    FillLayerResult, FillLimits, FillPressureCorrelation, FillRecord, FillStack, LayeredBreakdown,
    LayeredBreakdownInput, SystemPressureBreakdown, SystemPressureBreakdownInput, TowerRecord,
    ZoneCorrelation,
};
use serde_json::Value;

const VECTOR_TOLERANCE: f64 = 1e-6;

const DRY_AIR_DENSITY: f64 = 1.116_703_643_908_769;
const MOIST_AIR_DENSITY: f64 = 1.139_290_537_378_018_4;

fn agrees(actual: f64, expected: f64, label: &str) {
    let scale = expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() / scale < VECTOR_TOLERANCE,
        "{label}: got {actual}, vector says {expected}"
    );
}

fn close(actual: f64, expected: f64, label: &str) {
    let scale = expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() / scale < 1e-12,
        "{label}: got {actual}, expected {expected}"
    );
}

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
        spray_zone: Some(zone(0.16, 3.0, 2.0, -0.30, 0.45)),
        rain_zone_height_m: Some(1.5),
        rain_zone: Some(zone(0.13, 3.0, 2.0, -0.35, 0.50)),
    }
}

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

fn film_of25() -> FillRecord {
    FillRecord {
        id: "FILM-OF25".to_string(),
        thermal: Some(zone(1.12, 3.0, 2.0, -0.31, 0.44)),
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

fn film_vf38() -> FillRecord {
    FillRecord {
        id: "FILM-VF38".to_string(),
        thermal: Some(zone(0.94, 3.0, 2.0, -0.27, 0.39)),
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

fn de_3p_10() -> DriftEliminatorRecord {
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

fn ax_500() -> FanRecord {
    FanRecord {
        id: "AX-500".to_string(),
        stack_area_m2: Some(19.635),
        pressure_basis: Some(FanPressureBasis::Total),
        reference_density_kg_m3: Some(1.2),
        stack_recovery_factor: None,
        curve: [
            (55.0, 520.0, 0.63),
            (100.0, 470.0, 0.74),
            (145.0, 380.0, 0.83),
            (185.0, 245.0, 0.82),
            (225.0, 70.0, 0.67),
        ]
        .into_iter()
        .map(|(flow_m3_s, pressure_pa, efficiency)| FanCurvePoint {
            flow_m3_s,
            pressure_pa,
            efficiency,
        })
        .collect(),
    }
}

fn single_fill(
    fill: &FillRecord,
    depth_m: f64,
    flow_m3_s: f64,
    water_kg_s: f64,
    thermal_multiplier: f64,
    pressure_multiplier: f64,
) -> SystemPressureBreakdown {
    system_pressure_breakdown(SystemPressureBreakdownInput {
        tower: &idcf_064(),
        fill,
        fill_depth_m: depth_m,
        drift_eliminator: &de_3p_10(),
        fan: Some(&ax_500()),
        volumetric_air_flow_m3_s: flow_m3_s,
        dry_air_density_kg_m3: DRY_AIR_DENSITY,
        moist_air_density_kg_m3: MOIST_AIR_DENSITY,
        water_mass_flow_kg_s: water_kg_s,
        thermal_multiplier,
        pressure_multiplier,
    })
    .expect("the single-fill fixture computes")
}

fn stacked(
    stack: &FillStack,
    fills: &[FillRecord],
    flow_m3_s: f64,
    water_kg_s: f64,
    thermal_multiplier: f64,
    pressure_multiplier: f64,
) -> LayeredBreakdown {
    let layers = resolve_fill_layers(stack, fills).expect("the fixture stack resolves");
    layered_system_pressure_breakdown(LayeredBreakdownInput {
        tower: &idcf_064(),
        layers: &layers,
        drift_eliminator: &de_3p_10(),
        fan: Some(&ax_500()),
        volumetric_air_flow_m3_s: flow_m3_s,
        dry_air_density_kg_m3: DRY_AIR_DENSITY,
        moist_air_density_kg_m3: MOIST_AIR_DENSITY,
        water_mass_flow_kg_s: water_kg_s,
        thermal_multiplier,
        pressure_multiplier,
    })
    .expect("the fixture stack computes")
}

fn one_layer(fill: &str, depth_m: f64) -> FillStack {
    FillStack::single(fill, depth_m)
}

#[test]
fn a_one_layer_stack_reproduces_the_single_fill_breakdown_bit_for_bit() {
    let fill = film_of25();
    for flow_m3_s in [100.0, 130.0, 160.0] {
        let single = single_fill(&fill, 1.5, flow_m3_s, 200.0, 1.0, 1.0);
        let layered = stacked(
            &one_layer("FILM-OF25", 1.5),
            std::slice::from_ref(&fill),
            flow_m3_s,
            200.0,
            1.0,
            1.0,
        );
        assert_eq!(
            single, layered.breakdown,
            "a one-layer stack must be the single-fill breakdown field for field at {flow_m3_s} m³/s"
        );
        assert_eq!(layered.fill_layers.len(), 1);
        let layer = &layered.fill_layers[0];
        assert_eq!(layer.position, 1);
        assert_eq!(layer.fill_id, "FILM-OF25");
        assert_eq!(layer.depth_m, 1.5);
        assert_eq!(layer.pressure_drop_pa, single.fill_pa);
        assert_eq!(layer.merkel_number, single.fill_merkel_number);
        assert_eq!(layer.cumulative_pressure_drop_pa, single.fill_pa);
        assert_eq!(layer.cumulative_merkel_number, single.fill_merkel_number);
        assert_eq!(layer.volumetric_air_flow_m3_s, flow_m3_s);
        assert_eq!(layer.water_loading_kg_m2_s, single.water_loading_kg_m2_s);
        assert_eq!(
            layer.dry_air_loading_kg_m2_s,
            single.dry_air_loading_kg_m2_s
        );
        assert_eq!(layer.fill_velocity_ms, single.fill_velocity_ms);
        assert_eq!(layer.limits, fill.limits);
        assert_eq!(layer.thermal, *fill.thermal.as_ref().unwrap());
        assert_eq!(layer.pressure, *fill.pressure.as_ref().unwrap());
    }
}

#[test]
fn a_one_layer_stack_reproduces_the_recorded_airside_vector() {
    let fill = film_of25();
    let result = stacked(
        &one_layer("FILM-OF25", 1.5),
        &[fill],
        160.0,
        200.0,
        1.0,
        1.0,
    );
    let breakdown = &result.breakdown;
    agrees(breakdown.total_pa, 332.801_889, "airside-160 totalPa");
    agrees(breakdown.fill_pa, 163.320_834, "airside-160 fillPa");
    agrees(
        breakdown.fill_merkel_number,
        1.921_084,
        "airside-160 fillMerkelNumber",
    );
    agrees(
        breakdown.available_merkel_number,
        2.258_391,
        "airside-160 availableMerkelNumber",
    );
    assert_eq!(result.fill_layers[0].pressure_drop_pa, breakdown.fill_pa);
    assert_eq!(
        result.fill_layers[0].merkel_number,
        breakdown.fill_merkel_number
    );
}

#[test]
fn two_layers_of_the_same_fill_split_the_single_fill_terms_by_depth() {
    let fill = film_of25();
    let single = single_fill(&fill, 1.5, 160.0, 200.0, 1.0, 1.0);
    let stack = FillStack::new(vec![
        FillLayer::new("FILM-OF25", 0.6),
        FillLayer::new("FILM-OF25", 0.9),
    ]);
    let result = stacked(&stack, &[fill], 160.0, 200.0, 1.0, 1.0);
    assert_eq!(result.fill_layers.len(), 2);
    agrees(result.breakdown.fill_pa, 163.320_834, "split fillPa total");
    agrees(
        result.breakdown.fill_merkel_number,
        1.921_084,
        "split fillMerkelNumber total",
    );
    let [upper, lower] = &result.fill_layers[..] else {
        panic!("two layers");
    };
    assert_eq!(upper.position, 1);
    assert_eq!(lower.position, 2);
    assert_eq!(upper.depth_m, 0.6);
    assert_eq!(lower.depth_m, 0.9);
    let ratio = upper.merkel_number / lower.merkel_number;
    assert!(
        (ratio - 0.6 / 0.9).abs() / (0.6 / 0.9) < 1e-12,
        "the split must follow the depths: {ratio}"
    );
    assert_eq!(
        upper.pressure_drop_pa + lower.pressure_drop_pa,
        result.breakdown.fill_pa
    );
    assert_eq!(
        upper.merkel_number + lower.merkel_number,
        result.breakdown.fill_merkel_number
    );
    assert_eq!(upper.cumulative_pressure_drop_pa, upper.pressure_drop_pa);
    assert_eq!(lower.cumulative_pressure_drop_pa, result.breakdown.fill_pa);
    assert_eq!(
        lower.cumulative_merkel_number,
        result.breakdown.fill_merkel_number
    );
    for layer in &result.fill_layers {
        assert_eq!(layer.volumetric_air_flow_m3_s, 160.0);
        assert_eq!(layer.water_loading_kg_m2_s, single.water_loading_kg_m2_s);
        assert_eq!(
            layer.dry_air_loading_kg_m2_s,
            single.dry_air_loading_kg_m2_s
        );
        assert_eq!(layer.dry_air_mass_flow_kg_s, single.dry_air_mass_flow_kg_s);
        assert_eq!(layer.water_mass_flow_kg_s, 200.0);
    }
}

#[test]
fn a_mixed_stack_sums_its_layers_in_physical_order() {
    let fills = [film_of25(), film_vf38()];
    let stack = FillStack::new(vec![
        FillLayer::new("FILM-OF25", 0.45),
        FillLayer::new("FILM-VF38", 0.9),
    ]);
    let result = stacked(&stack, &fills, 160.0, 200.0, 1.0, 1.0);
    let breakdown = &result.breakdown;
    let [upper, lower] = &result.fill_layers[..] else {
        panic!("two layers");
    };
    assert_mixed_stack_identity(&stack, upper, lower);
    let expected_pressure = |fill: &FillRecord, depth_m: f64| {
        fill_pressure_drop_pa(
            fill,
            depth_m,
            breakdown.water_loading_kg_m2_s,
            breakdown.dry_air_loading_kg_m2_s,
            1.0,
        )
        .expect("the primitive computes")
    };
    let expected_merkel = |fill: &FillRecord, depth_m: f64| {
        fill_thermal_merkel_number(
            fill,
            depth_m,
            breakdown.water_loading_kg_m2_s,
            breakdown.dry_air_loading_kg_m2_s,
            1.0,
        )
        .expect("the primitive computes")
    };
    assert_eq!(upper.pressure_drop_pa, expected_pressure(&fills[0], 0.45));
    assert_eq!(lower.pressure_drop_pa, expected_pressure(&fills[1], 0.9));
    assert_eq!(upper.merkel_number, expected_merkel(&fills[0], 0.45));
    assert_eq!(lower.merkel_number, expected_merkel(&fills[1], 0.9));

    assert_eq!(
        breakdown.fill_pa,
        expected_pressure(&fills[0], 0.45) + expected_pressure(&fills[1], 0.9)
    );
    assert_eq!(
        breakdown.fill_merkel_number,
        expected_merkel(&fills[0], 0.45) + expected_merkel(&fills[1], 0.9)
    );
    assert_eq!(upper.cumulative_merkel_number, upper.merkel_number);
    assert_eq!(lower.cumulative_merkel_number, breakdown.fill_merkel_number);
    assert_eq!(
        breakdown.available_merkel_number,
        breakdown.fill_merkel_number
            + breakdown.spray_zone_merkel_number
            + breakdown.rain_zone_merkel_number
    );
    assert_eq!(upper.thermal, *fills[0].thermal.as_ref().unwrap());
    assert_eq!(lower.thermal, *fills[1].thermal.as_ref().unwrap());
    assert_eq!(upper.limits, fills[0].limits);
    assert_eq!(lower.limits, fills[1].limits);
}

fn assert_mixed_stack_identity(
    stack: &FillStack,
    upper: &FillLayerResult,
    lower: &FillLayerResult,
) {
    assert_eq!(upper.fill_id, "FILM-OF25");
    assert_eq!(upper.depth_m, 0.45);
    assert_eq!(lower.fill_id, "FILM-VF38");
    assert_eq!(lower.depth_m, 0.9);
    assert_eq!(stack.label(), "FILM-OF25@0.45+FILM-VF38@0.9");
    assert_eq!(stack.total_depth_m(), 1.35);
    assert_eq!(stack.identity(), (stack.label(), 1.35));
}

#[test]
fn a_layer_multiplier_composes_with_the_run_multiplier() {
    let fill = film_of25();
    let stack = FillStack::new(vec![FillLayer::new("FILM-OF25", 0.9)
        .with_thermal_multiplier(0.9)
        .with_pressure_multiplier(1.08)]);
    let result = stacked(&stack, std::slice::from_ref(&fill), 160.0, 200.0, 0.8, 0.5);
    let layer = &result.fill_layers[0];
    assert_eq!(layer.thermal_multiplier, 0.9);
    assert_eq!(layer.pressure_multiplier, 1.08);
    assert_eq!(layer.effective_thermal_multiplier, 0.9 * 0.8);
    assert_eq!(layer.effective_pressure_multiplier, 1.08 * 0.5);
    assert_eq!(
        layer.merkel_number,
        fill_thermal_merkel_number(
            &fill,
            0.9,
            result.breakdown.water_loading_kg_m2_s,
            result.breakdown.dry_air_loading_kg_m2_s,
            0.9 * 0.8,
        )
        .expect("the primitive computes")
    );
    assert_eq!(
        layer.pressure_drop_pa,
        fill_pressure_drop_pa(
            &fill,
            0.9,
            result.breakdown.water_loading_kg_m2_s,
            result.breakdown.dry_air_loading_kg_m2_s,
            1.08 * 0.5,
        )
        .expect("the primitive computes")
    );
    let single = single_fill(&fill, 0.9, 160.0, 200.0, 0.8, 0.5);
    assert_eq!(
        result.breakdown.spray_zone_merkel_number,
        single.spray_zone_merkel_number
    );
    assert_eq!(
        result.breakdown.rain_zone_merkel_number,
        single.rain_zone_merkel_number
    );
}

#[test]
fn an_empty_stack_is_refused() {
    let error = resolve_fill_layers(&FillStack::default(), &[film_of25()])
        .expect_err("an empty stack has no layer to compute");
    assert_eq!(
        error.message(),
        "A fill stack must carry at least one layer."
    );
}

#[test]
fn a_layer_whose_fill_is_not_in_the_catalog_is_refused_by_name() {
    let stack = FillStack::new(vec![
        FillLayer::new("FILM-OF25", 0.45),
        FillLayer::new("NOT-A-FILL", 0.9),
    ]);
    let error =
        resolve_fill_layers(&stack, &[film_of25()]).expect_err("a layer needs a fill record");
    assert_eq!(
        error.message(),
        "Layer 2 (NOT-A-FILL@0.9): no fill record in the catalog carries that id."
    );
}

#[test]
fn a_non_positive_depth_or_multiplier_is_refused_by_name_not_clamped() {
    let fills = [film_of25()];
    let zero_depth = FillStack::new(vec![FillLayer::new("FILM-OF25", 0.0)]);
    assert_eq!(
        resolve_fill_layers(&zero_depth, &fills)
            .expect_err("zero depth is refused")
            .message(),
        "Layer 1 (FILM-OF25@0): depthM must be positive, got 0."
    );
    let negative_thermal = FillStack::new(vec![
        FillLayer::new("FILM-OF25", 0.45),
        FillLayer::new("FILM-OF25", 0.9).with_thermal_multiplier(-1.0),
    ]);
    assert_eq!(
        resolve_fill_layers(&negative_thermal, &fills)
            .expect_err("a negative multiplier is refused")
            .message(),
        "Layer 2 (FILM-OF25@0.9): thermalMultiplier must be positive, got -1."
    );
    let zero_pressure = FillStack::new(vec![
        FillLayer::new("FILM-OF25", 0.45).with_pressure_multiplier(0.0)
    ]);
    assert_eq!(
        resolve_fill_layers(&zero_pressure, &fills)
            .expect_err("a zero multiplier is refused")
            .message(),
        "Layer 1 (FILM-OF25@0.45): pressureMultiplier must be positive, got 0."
    );
}

#[test]
fn a_fill_without_the_characteristic_its_layer_needs_is_refused_by_name() {
    let mut no_thermal = film_of25();
    no_thermal.thermal = None;
    let stack = FillStack::single("FILM-OF25", 1.5);
    assert_eq!(
        resolve_fill_layers(&stack, &[no_thermal])
            .expect_err("a thermal layer needs a thermal correlation")
            .message(),
        "Layer 1 (FILM-OF25@1.5): Fill thermal correlation is missing."
    );
    let mut no_pressure = film_of25();
    no_pressure.pressure = None;
    assert_eq!(
        resolve_fill_layers(&stack, &[no_pressure])
            .expect_err("a pressure layer needs a pressure correlation")
            .message(),
        "Layer 1 (FILM-OF25@1.5): Fill pressure correlation is missing."
    );
}

#[test]
fn a_layer_outside_its_fills_limits_is_refused_by_name() {
    let fills = [film_of25(), film_vf38()];
    let stack = FillStack::new(vec![
        FillLayer::new("FILM-OF25", 1.5),
        FillLayer::new("FILM-VF38", 1.5),
    ]);
    let terms = resolve_fill_layers(&stack, &fills).expect("the fixture stack resolves");
    let water_loading = 5.8;
    let failures =
        check_stack_operating_envelope(&terms, water_loading, 1.744_849_443_607_451_5, None, None);
    assert_eq!(
        failures.len(),
        1,
        "only the first fill's limit is exceeded: {failures:?}"
    );
    assert_eq!(
        failures[0],
        "Layer 1 (FILM-OF25@1.5): Water loading 5.800 kg/(m²·s) is outside 1.3–5.4."
    );
    let with_claims = check_stack_operating_envelope(
        &terms,
        water_loading,
        1.744_849_443_607_451_5,
        Some(70.0),
        Some("dirty"),
    );
    assert!(
        with_claims.iter().any(|failure| failure
            .starts_with("Layer 1 (FILM-OF25@1.5): Hot-water temperature 70.0 °C exceeds 60 °C.")),
        "{with_claims:?}"
    );
    assert!(
        with_claims.iter().any(|failure| failure
            .starts_with("Layer 1 (FILM-OF25@1.5): Fill is not approved in the sample catalog")),
        "{with_claims:?}"
    );
    assert!(
        !with_claims
            .iter()
            .any(|failure| failure.starts_with("Layer 2 (FILM-VF38@1.5): Water loading")),
        "{with_claims:?}"
    );
}

fn recorded_stack(spelling: &str) -> FillStack {
    FillStack::new(
        spelling
            .split('+')
            .map(|layer| {
                let (fill_id, depth_m) = layer
                    .split_once('@')
                    .unwrap_or_else(|| panic!("{layer:?} is not <fillId>@<depthM>"));
                FillLayer::new(
                    fill_id,
                    depth_m
                        .parse::<f64>()
                        .unwrap_or_else(|error| panic!("{depth_m:?}: {error}")),
                )
            })
            .collect(),
    )
}

fn recorded_vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../validation/test-vectors.json"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse: {error}"))
}

#[test]
fn the_recorded_fill_layer_vectors_reproduce() {
    let file = recorded_vectors();
    let cases = file["families"]["fillLayers"]
        .as_array()
        .expect("the fillLayers family is an array");
    assert!(!cases.is_empty(), "the family carries the layered cases");
    let mut seen = Vec::new();
    for case in cases {
        let id = case["id"].as_str().expect("id");
        seen.push(id.to_string());
        let stack = recorded_stack(case["stack"].as_str().expect("stack"));
        let inputs = &case["inputs"];
        let result = stacked(
            &stack,
            &[film_of25()],
            inputs["volumetricAirFlowM3S"].as_f64().expect("flow"),
            inputs["waterMassFlowKgS"].as_f64().expect("water"),
            1.0,
            1.0,
        );
        let expected = &case["expected"];
        agrees(
            result.breakdown.fill_pa,
            expected["fillPa"].as_f64().expect("fillPa"),
            &format!("{id} fillPa"),
        );
        agrees(
            result.breakdown.fill_merkel_number,
            expected["fillMerkelNumber"]
                .as_f64()
                .expect("fillMerkelNumber"),
            &format!("{id} fillMerkelNumber"),
        );
        agrees(
            result.breakdown.available_merkel_number,
            expected["availableMerkelNumber"]
                .as_f64()
                .expect("availableMerkelNumber"),
            &format!("{id} availableMerkelNumber"),
        );
        check_recorded_layers(id, expected, &result);
        agrees(
            result
                .fill_layers
                .last()
                .expect("a layer")
                .cumulative_merkel_number,
            expected["fillMerkelNumber"]
                .as_f64()
                .expect("fillMerkelNumber"),
            &format!("{id} cumulative total"),
        );
    }
    assert_eq!(
        seen,
        vec![
            "fillLayers-single-160".to_string(),
            "fillLayers-split-160".to_string()
        ],
        "the recorded layered cases, in the file's order"
    );
}

fn check_recorded_layers(id: &str, expected: &Value, result: &LayeredBreakdown) {
    let layers = expected["layers"].as_array().expect("layers");
    assert_eq!(layers.len(), result.fill_layers.len(), "{id}");
    for (index, layer) in layers.iter().enumerate() {
        let got = &result.fill_layers[index];
        assert_eq!(
            got.position,
            usize::try_from(layer["position"].as_u64().expect("position")).expect("position")
        );
        assert_eq!(got.fill_id, layer["fillId"].as_str().expect("fillId"));
        assert_eq!(got.depth_m, layer["depthM"].as_f64().expect("depthM"));
        agrees(
            got.pressure_drop_pa,
            layer["pressureDropPa"].as_f64().expect("pressureDropPa"),
            &format!("{id} layer {} pressureDropPa", got.position),
        );
        agrees(
            got.merkel_number,
            layer["merkelNumber"].as_f64().expect("merkelNumber"),
            &format!("{id} layer {} merkelNumber", got.position),
        );
    }
}

const TOWER_SPEC: &str = "fillAreaM2:64,airFreeAreaM2:64,driftAreaM2:61,inletAreaM2:32,\
stackRecoveryFactor:0.35,inletLossCoefficient:3,distributionLossCoefficient:4.4,\
supportLossCoefficient:2.1,plenumLossCoefficient:0.38,fixedPressureLossPa:10,\
sprayZoneHeightM:0.6,rainZoneHeightM:1.5";
const FILL_THERMAL: &str = "coefficientPerM:1.12,referenceWaterLoadingKgM2S:3,\
referenceDryAirLoadingKgM2S:2,waterExponent:-0.31,airExponent:0.44";
const FILL_PRESSURE: &str = "coefficientPaPerM:61,referenceWaterLoadingKgM2S:3,\
referenceDryAirLoadingKgM2S:2,waterExponent:0.14,airExponent:1.72";
const FILL_LIMITS: &str = "minWaterLoadingKgM2S:1.3,maxWaterLoadingKgM2S:5.4,\
minDryAirLoadingKgM2S:0.9,maxDryAirLoadingKgM2S:3.3,maxWaterTemperatureC:60";
const DRIFT_CURVE: &str = "1:4:10,1.5:6:18,2:9:31,2.5:14:49,3:23:72,3.5:38:101";
const FAN_CURVE: &str = "55:520:0.63,100:470:0.74,145:380:0.83,185:245:0.82,225:70:0.67";
const SPRAY: &str = "coefficientPerM:0.16,referenceWaterLoadingKgM2S:3,\
referenceDryAirLoadingKgM2S:2,waterExponent:-0.3,airExponent:0.45";
const RAIN: &str = "coefficientPerM:0.13,referenceWaterLoadingKgM2S:3,\
referenceDryAirLoadingKgM2S:2,waterExponent:-0.35,airExponent:0.5";
const FILLS: &str = "id:FILM-OF25,compatibleTowerTypes:counterflow|crossflow,\
allowedWaterQualityClasses:clean|moderate,thermal:1.12|3|2|-0.31|0.44,\
pressure:61|3|2|0.14|1.72,limits:1.3|5.4|0.9|3.3|60;\
id:FILM-VF38,compatibleTowerTypes:counterflow|crossflow,\
allowedWaterQualityClasses:clean|moderate|dirty,thermal:0.94|3|2|-0.27|0.39,\
pressure:43|3|2|0.1|1.67,limits:1.1|6.2|0.8|3.5|75";

fn common_args() -> Vec<String> {
    [
        "--tower",
        TOWER_SPEC,
        "--drift-curve",
        DRIFT_CURVE,
        "--flow",
        "160",
        "--water",
        "200",
        "--dry-air-density",
        "1.116703643908769",
        "--moist-air-density",
        "1.1392905373780184",
        "--spray",
        SPRAY,
        "--rain",
        RAIN,
        "--fan-curve",
        FAN_CURVE,
        "--fan-basis",
        "total",
        "--fan-stack-area",
        "19.635",
        "--fan-ref-density",
        "1.2",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

fn run(args: &[&str]) -> Value {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let json = cli::run(&args).expect("the command answers");
    serde_json::from_str(&json).expect("a JSON reply")
}

#[test]
fn the_rate_command_reports_every_layer_and_the_combined_total() {
    let mut args = vec![
        "layers".to_string(),
        "--fills".to_string(),
        FILLS.to_string(),
    ];
    args.extend(common_args());
    args.extend([
        "--fill-stack".to_string(),
        "FILM-OF25@0.45+FILM-VF38@0.9".to_string(),
    ]);
    let json = cli::run(&args).expect("the stack computes");
    let reply: Value = serde_json::from_str(&json).expect("a JSON reply");

    assert_eq!(reply["stackLabel"], "FILM-OF25@0.45+FILM-VF38@0.9");
    assert_eq!(reply["totalDepthM"], 1.35);
    let layers = reply["layers"].as_array().expect("layers is an array");
    assert_rate_layers(layers);
    let layer_sum =
        layers[0]["merkelNumber"].as_f64().unwrap() + layers[1]["merkelNumber"].as_f64().unwrap();
    let layer_pressure_sum = layers[0]["pressureDropPa"].as_f64().unwrap()
        + layers[1]["pressureDropPa"].as_f64().unwrap();
    close(
        reply["layerTotals"]["merkelNumber"].as_f64().unwrap(),
        layer_sum,
        "layerTotals.merkelNumber",
    );
    close(
        reply["airside"]["fillMerkelNumber"].as_f64().unwrap(),
        layer_sum,
        "airside.fillMerkelNumber",
    );
    close(
        reply["layerTotals"]["pressureDropPa"].as_f64().unwrap(),
        layer_pressure_sum,
        "layerTotals.pressureDropPa",
    );
    assert_eq!(
        reply["airside"]["fillPa"], reply["layerTotals"]["pressureDropPa"],
        "the breakdown's fill term is the layers' own total, byte for byte"
    );
    close(
        layers[1]["cumulativeMerkelNumber"].as_f64().unwrap(),
        layer_sum,
        "the last cumulative value",
    );
    assert!(reply["airside"]["sprayZoneMerkelNumber"].as_f64().unwrap() > 0.0);
    assert!(reply["airside"]["rainZoneMerkelNumber"].as_f64().unwrap() > 0.0);
    assert!(reply["airside"]["driftPa"].as_f64().unwrap() > 0.0);
}

fn assert_rate_layers(layers: &[Value]) {
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0]["position"], 1);
    assert_eq!(layers[0]["label"], "FILM-OF25@0.45");
    assert_eq!(layers[0]["fillId"], "FILM-OF25");
    assert_eq!(layers[0]["depthM"], 0.45);
    assert_eq!(layers[1]["position"], 2);
    assert_eq!(layers[1]["label"], "FILM-VF38@0.9");
    assert_eq!(layers[1]["fillId"], "FILM-VF38");
    assert_eq!(layers[1]["depthM"], 0.9);
    assert_eq!(layers[1]["thermal"]["coefficientPerM"], 0.94);
    assert_eq!(layers[1]["pressure"]["coefficientPaPerM"], 43.0);
    assert_eq!(layers[1]["limits"]["maxWaterLoadingKgM2S"], 6.2);
    assert_eq!(layers[0]["thermalMultiplier"], 1.0);
    assert_eq!(layers[0]["effectivePressureMultiplier"], 1.0);
    assert_eq!(
        layers[0]["volumetricAirFlowM3S"],
        layers[1]["volumetricAirFlowM3S"]
    );
    assert_eq!(
        layers[0]["waterLoadingKgM2S"],
        layers[1]["waterLoadingKgM2S"]
    );
    assert_eq!(
        layers[0]["dryAirLoadingKgM2S"],
        layers[1]["dryAirLoadingKgM2S"]
    );
}

#[test]
fn the_rate_command_reproduces_the_single_fill_command_for_a_one_layer_stack() {
    let single = run(&[
        "airside",
        "--fill-thermal",
        FILL_THERMAL,
        "--fill-pressure",
        FILL_PRESSURE,
        "--fill-limits",
        FILL_LIMITS,
        "--fill-quality",
        "clean,moderate",
        "--fill-depth",
        "1.5",
    ]
    .into_iter()
    .chain(common_args().iter().map(String::as_str))
    .collect::<Vec<&str>>());

    let layered = run(
        &["layers", "--fills", FILLS, "--fill-stack", "FILM-OF25@1.5"]
            .into_iter()
            .chain(common_args().iter().map(String::as_str))
            .collect::<Vec<&str>>(),
    );

    assert_eq!(
        layered["airside"], single,
        "the one-layer stack's breakdown must be the single-fill command's own reply"
    );
    assert_eq!(layered["layers"].as_array().unwrap().len(), 1);
    assert_eq!(layered["layers"][0]["fillId"], "FILM-OF25");
    assert_eq!(layered["stackLabel"], "FILM-OF25@1.5");
}

#[test]
fn the_rate_command_refuses_a_layer_outside_its_limits_and_names_it() {
    let mut args = vec![
        "layers".to_string(),
        "--fills".to_string(),
        FILLS.to_string(),
    ];
    args.extend(common_args());
    args.extend(["--fill-stack".to_string(), "FILM-OF25@1.5".to_string()]);
    let water_index = args
        .iter()
        .position(|argument| argument == "--water")
        .expect("--water is in the shared options");
    args[water_index + 1] = "400".to_string();
    let error = cli::run(&args).expect_err("an out-of-limit layer is refused");
    let cli::Failure::Domain(error) = error else {
        panic!("expected a domain refusal, got {error:?}");
    };
    assert_eq!(
        error.message(),
        "Layer 1 (FILM-OF25@1.5): Water loading 6.250 kg/(m²·s) is outside 1.3–5.4."
    );

    let water_index = args
        .iter()
        .position(|argument| argument == "--water")
        .expect("--water is in the shared options");
    args[water_index + 1] = "200".to_string();
    assert!(cli::run(&args).is_ok());
}

#[test]
fn a_layer_stack_spelling_round_trips_through_the_command_surface() {
    let mut args = vec![
        "layers".to_string(),
        "--fills".to_string(),
        FILLS.to_string(),
    ];
    args.extend(common_args());
    args.extend([
        "--fill-stack".to_string(),
        "FILM-VF38@0.9+FILM-OF25@0.45".to_string(),
    ]);
    let reply: Value =
        serde_json::from_str(&cli::run(&args).expect("the stack computes")).expect("a JSON reply");
    assert_eq!(reply["stackLabel"], "FILM-VF38@0.9+FILM-OF25@0.45");
    assert_eq!(reply["layers"][0]["fillId"], "FILM-VF38");
    assert_eq!(reply["layers"][1]["fillId"], "FILM-OF25");

    let mut broken = vec![
        "layers".to_string(),
        "--fills".to_string(),
        FILLS.to_string(),
    ];
    broken.extend(common_args());
    broken.extend(["--fill-stack".to_string(), "FILM-OF25".to_string()]);
    let error = cli::run(&broken).expect_err("a layer needs a depth");
    let cli::Failure::Usage(message) = error else {
        panic!("expected a usage refusal");
    };
    assert!(message.contains("fillId@depthM"), "{message}");
}
