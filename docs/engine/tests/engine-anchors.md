# `engine` tests · `anchors`

External anchors for the ported slice — the same anchors, at the same tolerances, that
the JavaScript suite asserts. Every reference value below is a published value or an
independent integration, never a value produced by this crate.

Sources, quoted per test:

* `tests/psychrometrics-ashrae.test.js` — ASHRAE Handbook, Fundamentals, Chapter 1
  (psychrometrics), thermodynamic properties of moist air at standard atmospheric
  pressure (101.325 kPa).
* `tests/psychrometrics-range.test.js` — ASHRAE over-ice saturation pressures, ICAO
  standard-atmosphere pressures, and the range/refusal behaviour.
* `tests/psychrometrics.test.js` — the simplified (enhancement-factor-off) cross-check
  against PsychroLib / the ASHRAE simplified equation set.
* `tests/merkel-accuracy.test.js` — an independent reference integration of the Merkel
  demand written as a flat trapezoid loop that shares no code path with the quadrature
  under test.
* `tests/merkel.test.js` — quadrature agreement and the cold-water inverse closure.

## Notes carried from the source comments

### `fn close`

/* ---------------- ASHRAE saturation pressure, liquid branch ---------------- */

### `const ASHRAE_SATURATION_PRESSURE_PA`

ASHRAE Handbook — Fundamentals, Chapter 1: saturation pressure of water at standard
atmospheric pressure.

### `const ASHRAE_SATURATED_AIR`

ASHRAE saturated-air properties at 101.325 kPa: temperature °C, humidity ratio
kg/kg dry air, enthalpy kJ/kg dry air.

### `const ASHRAE_OVER_ICE_PA`

ASHRAE Handbook — Fundamentals, saturation pressure over ice.

### `const ALTITUDES`

ICAO standard-atmosphere pressures.

### `fn the_ice_and_liquid_correlation_branches_meet_without_a_step`

The implementation switches branch at 0.01 °C; a discontinuity there would be a
genuine defect, putting a kink in every downstream property.

### `fn saturation_pressure_is_monotonic_across_the_freezing_point`

/* ---------------- ASHRAE saturated-air tables ---------------- */

### `fn omitting_the_enhancement_factor_measurably_degrades_table_agreement`

Guards against the enhancement factor being silently dropped again: the simplified
treatment must stay visibly worse against the published tables.

/* ---------------- documented inputs ---------------- */

### `fn the_simplified_path_at_30_c_and_50_rh_is_internally_consistent`

Values pinned in tests/psychrometrics.test.js from PsychroLib / the ASHRAE simplified
equation set, which omits the enhancement factor.

### `fn humidity_ratio_and_relative_humidity_round_trip`

/* ---------------- below freezing and away from standard pressure ---------------- */

### `fn humidity_ratio_rises_as_total_pressure_falls_at_fixed_temperature`

W = 0.621945 f·pws / (P - f·pws); pws depends only on temperature, so thinner air
holds more water per kilogram of dry air.

### `fn a_saturation_pressure_at_or_above_total_pressure_is_refused_not_returned`

At 70 kPa water boils near 90 °C; beyond that there is no moist-air state to compute.

/* ---------------- Merkel demand against an independent integration ---------------- */

### `const MERKEL_CASES`

The same five conditions as `tests/merkel-accuracy.test.js`.

### `fn reference_merkel`

Independent reference integration of the Merkel demand:

```text
KaV/L = integral over Tc..Th of cp dTw / (hs(Tw) - ha(Tw))
```

Written as a flat trapezoid loop that shares no code path with this crate's quadrature —
the same deliberately independent reference the JavaScript suite uses
(`tests/merkel-accuracy.test.js`).

### `fn the_cti_wet_bulb_enthalpy_convention_is_available_and_demands_more_transfer`

A higher entering-air enthalpy shrinks the driving potential, so demand must rise.

### `fn simpson_and_four_point_tchebycheff_agree_closely`

/* ---------------- cold-water inverse solver ---------------- */

### `fn more_available_merkel_capability_produces_colder_water`

/* ---------------- refusal behaviour ---------------- */

### `fn an_enthalpy_pinch_is_refused_rather_than_clamped`

The recorded infeasible vector merkel-02: an L/G of 1.5 cannot meet a 5 K approach
against a 10 °C wet bulb.

### `fn physically_impossible_inputs_are_refused_and_named`

Wet bulb above dry bulb.

Relative humidity outside 0…1.

Capacity must be positive.

Temperatures must be ordered hot > cold > wet bulb.

### `fn inputs_outside_the_correlations_are_refused_and_named`

Saturation-pressure correlation limits.

A vapor pressure at or above the total pressure has no humidity-ratio solution.

A state needs one of wet bulb or relative humidity.
