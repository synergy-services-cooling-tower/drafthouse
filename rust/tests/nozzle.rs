//! Nozzle flow and arrangement — the JavaScript suite's expectations for `src/core/nozzle.js`,
//! ported test for test.
//!
//! Sources, quoted per test:
//!
//! * `tests/water-nozzle.test.js` — the nozzle-selector count and flow-balance expectation,
//!   against the bundled sample-catalog nozzles (quoted below verbatim; illustrative
//!   synthetic data).
//! * `src/core/nozzle.js` — the formula, the `?? 997` density fallback, the `[4, 4000]`
//!   count range and the refusal message. The expected numbers were measured from the
//!   reference engine at this commit.
//!
//! The JavaScript engine is the specification; nothing here re-derives a value.

use synergy_drafthouse::{
    nozzle_flow_m3_s, select_nozzle_arrangement, NozzleArrangementInput, NozzleRecord,
    NOZZLE_WATER_DENSITY_KG_M3,
};

fn sample_nozzles() -> Vec<NozzleRecord> {
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

fn assert_close(actual: f64, expected: f64, relative: f64, label: &str) {
    let difference = (actual - expected).abs();
    assert!(
        difference <= relative * expected.abs(),
        "{label}: got {actual}, expected {expected} (|Δ| {difference})"
    );
}

/// Measured from `src/core/nozzle.js` at this commit: `nozzleFlowM3S({ dischargeCoefficient:
/// 0.72, orificeDiameterM: 0.02, pressureDropPa: 65000 })` with the default density 997.
#[test]
fn nozzle_flow_uses_the_reference_default_density() {
    let flow = nozzle_flow_m3_s(0.72, 0.020, 65_000.0, NOZZLE_WATER_DENSITY_KG_M3)
        .expect("the nozzle flows");
    assert_close(flow, 0.002_582_893_302_367_790_3, 1e-12, "NZ-20 at 65 kPa");
    // The reference's `waterDensityKgM3` argument is not used by this formula; a caller that
    // passes one gets the reference's own number for it.
    let denser = nozzle_flow_m3_s(0.72, 0.020, 65_000.0, 1000.0).expect("the nozzle flows");
    assert_close(
        denser,
        0.002_579_016_052_292_451,
        1e-12,
        "NZ-20 at 1000 kg/m3",
    );
}

/// `tests/water-nozzle.test.js` — 'nozzle selector returns count and flow balance' — plus the
/// reference's own numbers for the same call.
#[test]
fn the_arrangement_list_is_the_reference_order_and_count() {
    let nozzles = sample_nozzles();
    let options = select_nozzle_arrangement(&NozzleArrangementInput::new(0.2, &nozzles, 65_000.0))
        .expect("an arrangement fits");
    let summary: Vec<(&str, f64)> = options
        .iter()
        .map(|option| (option.nozzle_id.as_str(), option.count))
        .collect();
    assert_eq!(
        summary,
        [
            ("NZ-20", 78.0),
            ("NZ-32", 29.0),
            ("NZ-25", 49.0),
            ("NZ-40", 19.0)
        ]
    );
    assert!(options[0].actual_total_flow_m3_s >= 0.2);
    assert_close(
        options[0].flow_per_nozzle_m3_s,
        0.002_582_893_302_367_790_3,
        1e-12,
        "NZ-20 flow",
    );
    assert_close(
        options[0].actual_total_flow_m3_s,
        0.201_465_677_584_687_64,
        1e-12,
        "NZ-20 total",
    );
    assert_close(
        options[0].excess_flow_pct,
        0.732_838_792_343_815_1,
        1e-9,
        "NZ-20 excess",
    );
    assert!(options.iter().all(|option| option.valid_count));
}

#[test]
fn the_count_range_option_filters_and_an_empty_result_is_refused() {
    let nozzles = sample_nozzles();
    let empty = select_nozzle_arrangement(
        &NozzleArrangementInput::new(0.2, &nozzles, 65_000.0)
            .with_target_count_range([1000.0, 2000.0]),
    )
    .expect_err("no arrangement fits a 1000–2000 count window");
    assert_eq!(
        empty.message(),
        "No sample nozzle arrangement fits the requested count range."
    );
    // The default range's lower end: a trickle of flow asks for a single nozzle.
    assert!(
        select_nozzle_arrangement(&NozzleArrangementInput::new(1e-6, &nozzles, 65_000.0)).is_err()
    );
}

#[test]
fn non_positive_inputs_are_refused() {
    for (coefficient, diameter, pressure) in [
        (0.0, 0.02, 65_000.0),
        (0.72, 0.0, 65_000.0),
        (0.72, 0.02, 0.0),
    ] {
        assert!(
            nozzle_flow_m3_s(coefficient, diameter, pressure, 997.0).is_err(),
            "a non-positive input must refuse"
        );
    }
}
