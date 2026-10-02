//! External anchors for the ported slice — the same anchors, at the same tolerances, that
//! the JavaScript suite asserts. Every reference value below is a published value or an
//! independent integration, never a value produced by this crate.
//!
//! Sources, quoted per test:
//!
//! * `tests/psychrometrics-ashrae.test.js` — ASHRAE Handbook, Fundamentals, Chapter 1
//!   (psychrometrics), thermodynamic properties of moist air at standard atmospheric
//!   pressure (101.325 kPa).
//! * `tests/psychrometrics-range.test.js` — ASHRAE over-ice saturation pressures, ICAO
//!   standard-atmosphere pressures, and the range/refusal behaviour.
//! * `tests/psychrometrics.test.js` — the simplified (enhancement-factor-off) cross-check
//!   against PsychroLib / the ASHRAE simplified equation set.
//! * `tests/merkel-accuracy.test.js` — an independent reference integration of the Merkel
//!   demand written as a flat trapezoid loop that shares no code path with the quadrature
//!   under test.
//! * `tests/merkel.test.js` — quadrature agreement and the cold-water inverse closure.

use synergy_drafthouse::{
    dew_point_from_humidity_ratio, humidity_ratio_from_relative_humidity,
    humidity_ratio_from_vapor_pressure, humidity_ratio_from_wet_bulb, merkel_demand,
    moist_air_density_kg_m3, moist_air_enthalpy_kj_kg_dry_air, psychrometric_state,
    saturated_air_enthalpy_kj_kg_dry_air, saturation_humidity_ratio, saturation_vapor_pressure_pa,
    solve_cold_water_temperature, validate_thermal_temperatures,
    vapor_pressure_from_humidity_ratio, water_specific_heat_kj_kg_k,
    water_vapor_enhancement_factor, wet_bulb_from_humidity_ratio, ColdWaterTemperatureInput,
    InletEnthalpyConvention, Integration, MerkelInput, PsychrometricOptions,
    PsychrometricStateInput,
};

fn options() -> PsychrometricOptions {
    PsychrometricOptions::default()
}

fn simplified() -> PsychrometricOptions {
    PsychrometricOptions {
        enhancement_factor: false,
    }
}

fn close(actual: f64, expected: f64, tolerance: f64, message: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{message} expected {expected}, got {actual}"
    );
}

/* ---------------- ASHRAE saturation pressure, liquid branch ---------------- */

/// ASHRAE Handbook — Fundamentals, Chapter 1: saturation pressure of water at standard
/// atmospheric pressure.
const ASHRAE_SATURATION_PRESSURE_PA: [(f64, f64); 7] = [
    (0.0, 611.2),
    (10.0, 1228.1),
    (20.0, 2339.3),
    (25.0, 3169.2),
    (30.0, 4246.0),
    (40.0, 7384.9),
    (50.0, 12351.9),
];

/// ASHRAE saturated-air properties at 101.325 kPa: temperature °C, humidity ratio
/// kg/kg dry air, enthalpy kJ/kg dry air.
const ASHRAE_SATURATED_AIR: [(f64, f64, f64); 7] = [
    (0.0, 0.003790, 9.473),
    (10.0, 0.007661, 29.348),
    (20.0, 0.014758, 57.555),
    (25.0, 0.020173, 76.504),
    (30.0, 0.027329, 100.006),
    (40.0, 0.049141, 166.346),
    (50.0, 0.086863, 275.65),
];

/// ASHRAE Handbook — Fundamentals, saturation pressure over ice.
const ASHRAE_OVER_ICE_PA: [(f64, f64); 5] = [
    (-20.0, 103.24),
    (-15.0, 165.30),
    (-10.0, 259.90),
    (-5.0, 401.76),
    (0.0, 611.15),
];

/// ICAO standard-atmosphere pressures.
const ALTITUDES: [(&str, f64); 5] = [
    ("sea level", 101_325.0),
    ("500 m", 95_461.0),
    ("1500 m", 84_556.0),
    ("3000 m", 70_121.0),
    ("4000 m", 61_660.0),
];

#[test]
fn ashrae_saturation_pressure_tracks_the_table_within_0_05_percent() {
    for (t, pws) in ASHRAE_SATURATION_PRESSURE_PA {
        let calculated = saturation_vapor_pressure_pa(t).expect("finite temperature");
        let error_pct = 100.0 * (calculated - pws).abs() / pws;
        assert!(
            error_pct < 0.05,
            "{t} °C: {calculated} Pa vs {pws} Pa ({error_pct:.3} %)"
        );
    }
}

#[test]
fn ashrae_saturation_pressure_over_ice_tracks_the_table_within_0_1_percent() {
    for (t, pws) in ASHRAE_OVER_ICE_PA {
        let calculated = saturation_vapor_pressure_pa(t).expect("finite temperature");
        let error_pct = 100.0 * (calculated - pws).abs() / pws;
        assert!(
            error_pct < 0.1,
            "{t} °C: {calculated:.3} Pa vs {pws} Pa ({error_pct:.4} %)"
        );
    }
}

#[test]
fn the_ice_and_liquid_correlation_branches_meet_without_a_step() {
    // The implementation switches branch at 0.01 °C; a discontinuity there would be a
    // genuine defect, putting a kink in every downstream property.
    let below = saturation_vapor_pressure_pa(0.009).unwrap();
    let above = saturation_vapor_pressure_pa(0.011).unwrap();
    let jump_pct = 100.0 * (above - below).abs() / below;
    assert!(
        jump_pct < 0.05,
        "branch discontinuity of {jump_pct:.4} % at the 0.01 °C switchover"
    );
}

#[test]
fn saturation_pressure_is_monotonic_across_the_freezing_point() {
    let mut previous = 0.0;
    let mut t = -30.0;
    while t <= 30.0 {
        let pws = saturation_vapor_pressure_pa(t).unwrap();
        assert!(
            pws > previous,
            "not monotonic at {t} °C: {pws} after {previous}"
        );
        previous = pws;
        t += 0.5;
    }
}

/* ---------------- ASHRAE saturated-air tables ---------------- */

#[test]
fn saturated_humidity_ratio_tracks_ashrae_tables_within_0_2_percent() {
    for (t, ws, _) in ASHRAE_SATURATED_AIR {
        let calculated = saturation_humidity_ratio(t, 101_325.0, options()).unwrap();
        let error_pct = 100.0 * (calculated - ws).abs() / ws;
        assert!(
            error_pct < 0.2,
            "{t} °C: {calculated} vs {ws} ({error_pct:.3} %)"
        );
    }
}

#[test]
fn saturated_air_enthalpy_tracks_ashrae_tables_within_0_4_kj_per_kg() {
    for (t, _, h) in ASHRAE_SATURATED_AIR {
        let calculated = saturated_air_enthalpy_kj_kg_dry_air(t, 101_325.0, options()).unwrap();
        assert!(
            (calculated - h).abs() < 0.4,
            "{t} °C: {calculated:.3} vs {h} kJ/kg ({:.3})",
            calculated - h
        );
    }
}

#[test]
fn omitting_the_enhancement_factor_measurably_degrades_table_agreement() {
    // Guards against the enhancement factor being silently dropped again: the simplified
    // treatment must stay visibly worse against the published tables.
    let worst = |options: PsychrometricOptions| {
        ASHRAE_SATURATED_AIR
            .iter()
            .map(|(t, ws, _)| {
                let calculated = saturation_humidity_ratio(*t, 101_325.0, options).unwrap();
                (calculated - ws).abs() / ws
            })
            .fold(0.0, f64::max)
    };
    let enhanced = worst(options());
    let simplified = worst(simplified());
    assert!(simplified > 3.0 * enhanced, "{simplified} vs {enhanced}");
}

/* ---------------- documented inputs ---------------- */

#[test]
fn the_simplified_path_at_30_c_and_50_rh_is_internally_consistent() {
    // Values pinned in tests/psychrometrics.test.js from PsychroLib / the ASHRAE simplified
    // equation set, which omits the enhancement factor.
    let humidity_ratio =
        humidity_ratio_from_relative_humidity(30.0, 0.5, 101_325.0, simplified()).unwrap();
    close(
        humidity_ratio,
        0.0133102,
        2e-7,
        "simplified humidity ratio:",
    );
    close(
        moist_air_enthalpy_kj_kg_dry_air(30.0, humidity_ratio),
        64.2115,
        0.01,
        "simplified enthalpy:",
    );
    let state = psychrometric_state(
        PsychrometricStateInput::from_relative_humidity(30.0, 0.5)
            .with_pressure(101_325.0)
            .with_enhancement_factor(false),
    )
    .unwrap();
    close(state.wet_bulb_c, 22.005, 0.02, "simplified wet bulb:");
    close(state.dew_point_c, 18.447, 0.02, "simplified dew point:");
    close(
        state.relative_humidity,
        0.5,
        1e-12,
        "simplified relative humidity:",
    );
}

#[test]
fn a_wet_bulb_input_resolves_relative_humidity() {
    let state = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    close(
        state.relative_humidity,
        0.6313,
        0.0005,
        "relative humidity:",
    );
    close(state.enthalpy_kj_kg_dry_air, 85.026, 0.02, "enthalpy:");
}

#[test]
fn the_enhancement_factor_raises_the_humidity_ratio() {
    assert!(water_vapor_enhancement_factor(101_325.0, options()) > 1.004);
    assert_eq!(water_vapor_enhancement_factor(101_325.0, simplified()), 1.0);
    let enhanced = psychrometric_state(PsychrometricStateInput::from_wet_bulb(33.0, 27.0)).unwrap();
    let plain = psychrometric_state(
        PsychrometricStateInput::from_wet_bulb(33.0, 27.0).with_enhancement_factor(false),
    )
    .unwrap();
    assert!(
        enhanced.humidity_ratio > plain.humidity_ratio,
        "real-mixture treatment must raise the humidity ratio"
    );
}

#[test]
fn humidity_ratio_and_relative_humidity_round_trip() {
    for dry_bulb_c in [5.0, 20.0, 33.0, 45.0] {
        for relative_humidity in [0.2, 0.5, 0.9] {
            let state = psychrometric_state(PsychrometricStateInput::from_relative_humidity(
                dry_bulb_c,
                relative_humidity,
            ))
            .unwrap();
            let back = psychrometric_state(PsychrometricStateInput::from_wet_bulb(
                dry_bulb_c,
                state.wet_bulb_c,
            ))
            .unwrap();
            close(
                back.relative_humidity,
                relative_humidity,
                5e-5,
                &format!("DB {dry_bulb_c} RH {relative_humidity}:"),
            );
        }
    }
}

/* ---------------- below freezing and away from standard pressure ---------------- */

#[test]
fn sub_zero_moist_air_states_resolve_and_stay_physical() {
    for (dry_bulb_c, wet_bulb_c) in [
        (0.0, -2.0),
        (-5.0, -6.0),
        (-10.0, -12.0),
        (-20.0, -21.0),
        (-2.0, -2.0),
    ] {
        let state = psychrometric_state(PsychrometricStateInput::from_wet_bulb(
            dry_bulb_c, wet_bulb_c,
        ))
        .unwrap();
        assert!(
            state.humidity_ratio > 0.0,
            "non-positive humidity ratio at {dry_bulb_c}/{wet_bulb_c}"
        );
        assert!(
            state.relative_humidity > 0.0 && state.relative_humidity <= 1.0,
            "RH out of range at {dry_bulb_c}/{wet_bulb_c}"
        );
        assert!(state.dew_point_c <= dry_bulb_c + 1e-6);
        assert!(state.dew_point_c <= wet_bulb_c + 1e-6);
        assert!(state.enthalpy_kj_kg_dry_air.is_finite());
    }
}

#[test]
fn the_sub_zero_wet_bulb_relation_round_trips() {
    for (dry_bulb_c, wet_bulb_c) in [(0.0, -3.0), (-5.0, -7.0), (-10.0, -13.0), (-15.0, -16.0)] {
        let humidity_ratio =
            humidity_ratio_from_wet_bulb(dry_bulb_c, wet_bulb_c, 101_325.0, options()).unwrap();
        close(
            wet_bulb_from_humidity_ratio(dry_bulb_c, humidity_ratio, 101_325.0, options()).unwrap(),
            wet_bulb_c,
            1e-4,
            &format!("DB {dry_bulb_c} WB {wet_bulb_c}:"),
        );
    }
}

#[test]
fn saturated_air_holds_less_moisture_as_it_gets_colder() {
    let mut previous = f64::INFINITY;
    for t in [10.0, 5.0, 0.0, -5.0, -10.0, -20.0] {
        let ws = saturation_humidity_ratio(t, 101_325.0, options()).unwrap();
        assert!(
            ws < previous,
            "saturation humidity ratio did not fall at {t} °C"
        );
        previous = ws;
    }
}

#[test]
fn humidity_ratio_rises_as_total_pressure_falls_at_fixed_temperature() {
    // W = 0.621945 f·pws / (P - f·pws); pws depends only on temperature, so thinner air
    // holds more water per kilogram of dry air.
    let mut previous = 0.0;
    for (name, pressure_pa) in ALTITUDES {
        let ws = saturation_humidity_ratio(25.0, pressure_pa, options()).unwrap();
        assert!(
            ws > previous,
            "saturation humidity ratio did not rise at {name}"
        );
        previous = ws;
    }
}

#[test]
fn moist_air_states_resolve_at_altitude_and_round_trip() {
    for (name, pressure_pa) in ALTITUDES {
        for (dry_bulb_c, wet_bulb_c) in [(30.0, 22.0), (25.0, 18.0), (15.0, 10.0)] {
            let state = psychrometric_state(
                PsychrometricStateInput::from_wet_bulb(dry_bulb_c, wet_bulb_c)
                    .with_pressure(pressure_pa),
            )
            .unwrap();
            assert!(state.relative_humidity > 0.0 && state.relative_humidity <= 1.0);
            assert!(state.humidity_ratio > 0.0);
            close(
                wet_bulb_from_humidity_ratio(
                    dry_bulb_c,
                    state.humidity_ratio,
                    pressure_pa,
                    options(),
                )
                .unwrap(),
                wet_bulb_c,
                1e-4,
                &format!("{name} DB {dry_bulb_c} WB {wet_bulb_c}:"),
            );
            close(
                dew_point_from_humidity_ratio(state.humidity_ratio, pressure_pa, options())
                    .unwrap(),
                state.dew_point_c,
                1e-4,
                &format!("{name} dew point:"),
            );
        }
    }
}

#[test]
fn air_gets_thinner_with_altitude() {
    let mut previous = f64::INFINITY;
    for (name, pressure_pa) in ALTITUDES {
        let state = psychrometric_state(
            PsychrometricStateInput::from_wet_bulb(25.0, 18.0).with_pressure(pressure_pa),
        )
        .unwrap();
        let density = moist_air_density_kg_m3(25.0, state.humidity_ratio, pressure_pa);
        assert!(density < previous, "density did not fall at {name}");
        previous = density;
    }
}

#[test]
fn a_saturation_pressure_at_or_above_total_pressure_is_refused_not_returned() {
    // At 70 kPa water boils near 90 °C; beyond that there is no moist-air state to compute.
    assert!(saturation_humidity_ratio(95.0, 70_121.0, options()).is_err());
    assert!(saturation_humidity_ratio(85.0, 70_121.0, options()).is_ok());
}

/* ---------------- Merkel demand against an independent integration ---------------- */

#[derive(Clone, Copy, Debug)]
struct MerkelCase {
    hot_water_c: f64,
    cold_water_c: f64,
    wet_bulb_c: f64,
    dry_bulb_c: f64,
    water_to_dry_air_ratio: f64,
}

const REFERENCE_STEPS: usize = 400_000;

/// The same five conditions as `tests/merkel-accuracy.test.js`.
const MERKEL_CASES: [MerkelCase; 5] = [
    MerkelCase {
        hot_water_c: 42.0,
        cold_water_c: 32.0,
        wet_bulb_c: 27.0,
        dry_bulb_c: 33.0,
        water_to_dry_air_ratio: 1.5,
    },
    MerkelCase {
        hot_water_c: 40.0,
        cold_water_c: 30.0,
        wet_bulb_c: 25.0,
        dry_bulb_c: 32.0,
        water_to_dry_air_ratio: 1.0,
    },
    MerkelCase {
        hot_water_c: 45.0,
        cold_water_c: 30.0,
        wet_bulb_c: 24.0,
        dry_bulb_c: 35.0,
        water_to_dry_air_ratio: 1.2,
    },
    MerkelCase {
        hot_water_c: 50.0,
        cold_water_c: 32.0,
        wet_bulb_c: 26.0,
        dry_bulb_c: 34.0,
        water_to_dry_air_ratio: 0.8,
    },
    MerkelCase {
        hot_water_c: 36.0,
        cold_water_c: 29.0,
        wet_bulb_c: 22.0,
        dry_bulb_c: 30.0,
        water_to_dry_air_ratio: 1.8,
    },
];

/// Independent reference integration of the Merkel demand:
///
/// ```text
/// KaV/L = integral over Tc..Th of cp dTw / (hs(Tw) - ha(Tw))
/// ```
///
/// Written as a flat trapezoid loop that shares no code path with this crate's quadrature —
/// the same deliberately independent reference the JavaScript suite uses
/// (`tests/merkel-accuracy.test.js`).
fn reference_merkel(case: MerkelCase, convention: InletEnthalpyConvention) -> f64 {
    let pressure_pa = 101_325.0;
    let cp = water_specific_heat_kj_kg_k((case.hot_water_c + case.cold_water_c) / 2.0, 0.0);
    let inlet_enthalpy = match convention {
        InletEnthalpyConvention::CtiSaturatedWetBulb => {
            saturated_air_enthalpy_kj_kg_dry_air(case.wet_bulb_c, pressure_pa, options()).unwrap()
        }
        InletEnthalpyConvention::Bulk => {
            let humidity_ratio = humidity_ratio_from_wet_bulb(
                case.dry_bulb_c,
                case.wet_bulb_c,
                pressure_pa,
                options(),
            )
            .unwrap();
            moist_air_enthalpy_kj_kg_dry_air(case.dry_bulb_c, humidity_ratio)
        }
    };
    let h = (case.hot_water_c - case.cold_water_c) / REFERENCE_STEPS as f64;
    let mut sum = 0.0;
    for i in 0..=REFERENCE_STEPS {
        let water_temperature_c = case.cold_water_c + i as f64 * h;
        let potential =
            saturated_air_enthalpy_kj_kg_dry_air(water_temperature_c, pressure_pa, options())
                .unwrap()
                - (inlet_enthalpy
                    + case.water_to_dry_air_ratio * cp * (water_temperature_c - case.cold_water_c));
        let weight = if i == 0 || i == REFERENCE_STEPS {
            0.5
        } else {
            1.0
        };
        sum += weight * cp / potential;
    }
    sum * h
}

fn merkel_case_input(case: MerkelCase, integration: Integration) -> MerkelInput {
    MerkelInput {
        integration,
        ..MerkelInput::new(
            case.hot_water_c,
            case.cold_water_c,
            case.wet_bulb_c,
            case.dry_bulb_c,
            case.water_to_dry_air_ratio,
        )
    }
}

#[test]
fn simpson_matches_an_independent_reference_integration_to_0_01_percent() {
    for case in MERKEL_CASES {
        let calculated = merkel_demand(&merkel_case_input(case, Integration::Simpson))
            .unwrap()
            .merkel_number;
        let reference = reference_merkel(case, InletEnthalpyConvention::Bulk);
        let error_pct = 100.0 * (calculated - reference).abs() / reference;
        assert!(
            error_pct < 0.01,
            "{case:?}: {calculated} vs {reference} ({error_pct:.5} %)"
        );
    }
}

#[test]
fn four_point_tchebycheff_stays_within_0_1_percent_of_the_reference_integration() {
    for case in MERKEL_CASES {
        let calculated = merkel_demand(&merkel_case_input(case, Integration::Chebyshev4))
            .unwrap()
            .merkel_number;
        let reference = reference_merkel(case, InletEnthalpyConvention::Bulk);
        let error_pct = 100.0 * (calculated - reference).abs() / reference;
        assert!(
            error_pct < 0.1,
            "{case:?}: {calculated} vs {reference} ({error_pct:.5} %)"
        );
    }
}

#[test]
fn the_cti_wet_bulb_enthalpy_convention_is_available_and_demands_more_transfer() {
    let case = MERKEL_CASES[0];
    let bulk = merkel_demand(&merkel_case_input(case, Integration::Simpson)).unwrap();
    let cti = merkel_demand(&MerkelInput {
        inlet_enthalpy_convention: InletEnthalpyConvention::CtiSaturatedWetBulb,
        ..merkel_case_input(case, Integration::Simpson)
    })
    .unwrap();

    assert_eq!(
        bulk.inlet_enthalpy_convention,
        InletEnthalpyConvention::Bulk
    );
    assert!(cti.inlet_air_enthalpy_kj_kg_dry_air > bulk.inlet_air_enthalpy_kj_kg_dry_air);
    // A higher entering-air enthalpy shrinks the driving potential, so demand must rise.
    assert!(cti.merkel_number > bulk.merkel_number);

    let spread_pct = 100.0 * (cti.merkel_number - bulk.merkel_number) / bulk.merkel_number;
    assert!(
        (0.3..4.0).contains(&spread_pct),
        "convention spread is {spread_pct:.3} %"
    );

    for convention in [
        InletEnthalpyConvention::Bulk,
        InletEnthalpyConvention::CtiSaturatedWetBulb,
    ] {
        let calculated = merkel_demand(&MerkelInput {
            inlet_enthalpy_convention: convention,
            ..merkel_case_input(case, Integration::Simpson)
        })
        .unwrap()
        .merkel_number;
        let reference = reference_merkel(case, convention);
        assert!(
            100.0 * (calculated - reference).abs() / reference < 0.01,
            "{convention:?} disagrees with the reference integration"
        );
    }
}

#[test]
fn simpson_and_four_point_tchebycheff_agree_closely() {
    let case = MERKEL_CASES[0];
    let simpson = merkel_demand(&merkel_case_input(case, Integration::Simpson))
        .unwrap()
        .merkel_number;
    let chebyshev = merkel_demand(&merkel_case_input(case, Integration::Chebyshev4))
        .unwrap()
        .merkel_number;
    assert!(simpson > 0.0);
    assert!(
        (chebyshev - simpson).abs() / simpson < 0.015,
        "{simpson} vs {chebyshev}"
    );
}

/* ---------------- cold-water inverse solver ---------------- */

#[test]
fn the_cold_water_solver_recovers_the_known_condition_from_its_merkel_demand() {
    let case = MERKEL_CASES[0];
    let demand = merkel_demand(&merkel_case_input(case, Integration::Simpson)).unwrap();
    let result = solve_cold_water_temperature(&ColdWaterTemperatureInput {
        available_merkel_number: demand.merkel_number,
        ..ColdWaterTemperatureInput::new(
            case.hot_water_c,
            case.wet_bulb_c,
            case.dry_bulb_c,
            case.water_to_dry_air_ratio,
            demand.merkel_number,
        )
    })
    .unwrap();
    assert!(
        (result.cold_water_c - case.cold_water_c).abs() < 0.002,
        "{}",
        result.cold_water_c
    );
}

#[test]
fn more_available_merkel_capability_produces_colder_water() {
    let solve = |available_merkel_number: f64| {
        solve_cold_water_temperature(&ColdWaterTemperatureInput::new(
            42.0,
            27.0,
            33.0,
            1.5,
            available_merkel_number,
        ))
        .unwrap()
    };
    let low = solve(1.2);
    let high = solve(1.8);
    assert!(high.cold_water_c < low.cold_water_c);
}

/* ---------------- refusal behaviour ---------------- */

#[test]
fn an_enthalpy_pinch_is_refused_rather_than_clamped() {
    // The recorded infeasible vector merkel-02: an L/G of 1.5 cannot meet a 5 K approach
    // against a 10 °C wet bulb.
    let input = MerkelInput::new(25.0, 15.0, 10.0, 15.0, 1.5);
    let error = merkel_demand(&input).expect_err("the pinch must be refused");
    assert!(
        error.message().contains("pinch"),
        "unexpected message: {error}"
    );
}

#[test]
fn physically_impossible_inputs_are_refused_and_named() {
    let base = MerkelInput::new(42.0, 32.0, 27.0, 33.0, 1.5);

    // Wet bulb above dry bulb.
    assert!(humidity_ratio_from_wet_bulb(27.0, 33.0, 101_325.0, options()).is_err());
    // Relative humidity outside 0…1.
    assert!(humidity_ratio_from_relative_humidity(30.0, 1.5, 101_325.0, options()).is_err());
    assert!(humidity_ratio_from_relative_humidity(30.0, -0.1, 101_325.0, options()).is_err());
    // Capacity must be positive.
    assert!(merkel_demand(&MerkelInput {
        water_to_dry_air_ratio: 0.0,
        ..base
    })
    .is_err());
    assert!(
        solve_cold_water_temperature(&ColdWaterTemperatureInput::new(42.0, 27.0, 33.0, 1.5, 0.0))
            .is_err()
    );
    // Temperatures must be ordered hot > cold > wet bulb.
    assert!(validate_thermal_temperatures(32.0, 42.0, 27.0).is_err());
    assert!(validate_thermal_temperatures(42.0, 27.0, 27.0).is_err());
    assert!(validate_thermal_temperatures(42.0, 32.0, 27.0).is_ok());
}

#[test]
fn inputs_outside_the_correlations_are_refused_and_named() {
    // Saturation-pressure correlation limits.
    assert!(saturation_vapor_pressure_pa(-101.0).is_err());
    assert!(saturation_vapor_pressure_pa(201.0).is_err());
    assert!(saturation_vapor_pressure_pa(f64::NAN).is_err());
    // A vapor pressure at or above the total pressure has no humidity-ratio solution.
    assert!(humidity_ratio_from_vapor_pressure(101_325.0, 101_325.0).is_err());
    assert!(humidity_ratio_from_vapor_pressure(-1.0, 101_325.0).is_err());
    assert!(vapor_pressure_from_humidity_ratio(-0.001, 101_325.0).is_err());
    // A state needs one of wet bulb or relative humidity.
    let bare = PsychrometricStateInput {
        dry_bulb_c: 33.0,
        wet_bulb_c: None,
        relative_humidity: None,
        pressure_pa: 101_325.0,
        enhancement_factor: true,
    };
    assert!(psychrometric_state(bare).is_err());
}
