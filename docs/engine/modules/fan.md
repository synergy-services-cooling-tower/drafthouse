# `drafthouse-engine` · `fan`

Fan operating limits, curve evaluation, the fan/system balance and motor selection —
port of `src/core/fan.js`.

The fan curve is the reference for every quantity here: pressures and efficiencies are
interpolated from it (`clampEnds: false` for pressure, so the curve is continued linearly
outside its tabulated range; `clampEnds: true` for efficiency), then corrected for the
air density and the affinity laws at the operating speed ratio. The recorded curve **is** the
ratio-1.0 curve; the affinity exponents this module applies are named constants
([`FLOW_AFFINITY_EXPONENT`], [`PRESSURE_AFFINITY_EXPONENT`], and the shaft power they imply,
[`SHAFT_POWER_AFFINITY_EXPONENT`]), and the engineering statement with its source is recorded
in `docs/CTI_MRL_MAPPING.md`.

Out-of-domain input returns a [`DomainError`] rather than a clamped value: a curve with
fewer than two points, a non-positive reference density, speed ratio or air density, and a
non-positive interpolated efficiency are all refused.

## Notes carried from the source comments

### `const FLOW_AFFINITY_EXPONENT`

Flow ∝ n: the fan-affinity exponent of the recorded curve's **flow** leg.

One turn of the same curve at speed ratio `n` carries `n` times the flow. Source: the AMCA
references in `docs/REFERENCES.md`, recorded against the engine in `docs/CTI_MRL_MAPPING.md`.

### `const PRESSURE_AFFINITY_EXPONENT`

Pressure ∝ n²: the fan-affinity exponent of the recorded curve's **pressure** leg.

### `const SHAFT_POWER_AFFINITY_EXPONENT`

Shaft power ∝ n³: the exponent the two legs above imply for the **power** leg —
`Q(n)·p(n) = n³·Q·p`, and the efficiency is read off the same recorded point.

### `enum FanPressureBasis`

How a fan curve is published, mirroring the JavaScript `pressureBasis` string. Rust models
it as an enum, so an unknown spelling cannot be constructed at the library seam; the CLI
reports an unrecognised spelling as a usage error (see the crate README).

### `Total,`

The curve includes the outlet velocity pressure; the fan-stack discharge term must be
charged against it (the default).

### `Static,`

The outlet velocity pressure is already netted out of the curve.

### `fn as_str`

The JavaScript spelling of this basis.

### `struct FanCurvePoint`

One point of a fan curve.

### `struct FanRecord`

A validated fan catalog record, reduced to the fields this slice reads.

### `fn validate_fan`

Port of the module-private `validateFan`.

### `fn js_max`

Port of `Math.max`, including its NaN behaviour: `Math.max(NaN, x)` is `NaN`, where Rust's
`f64::max` would return `x`.

### `fn js_min`

Port of `Math.min`, including its NaN behaviour.

### `struct FanOperatingLimits`

The operating range of a fan at one speed ratio, mirroring the JavaScript return object.

### `fn fan_operating_limits`

Port of `fanOperatingLimits`.

### `fn fan_pressure_pa_at_flow`

Port of `fanPressurePaAtFlow`: the curve is read at the speed-corrected reference flow and
scaled by the density ratio and the affinity law's pressure exponent
([`PRESSURE_AFFINITY_EXPONENT`], the square of the speed ratio).

### `fn fan_efficiency_at_flow`

Port of `fanEfficiencyAtFlow`: the curve is read at the speed-corrected reference flow and
clamped at the ends (an efficiency curve outside its range is not extrapolated).

### `fn fan_shaft_power_kw`

Port of `fanShaftPowerKW`.

The power leg is the recorded curve's own airflow and pressure at the same operating point,
divided by the recorded efficiency there: flow carries [`FLOW_AFFINITY_EXPONENT`] and pressure
[`PRESSURE_AFFINITY_EXPONENT`], so the power this returns carries their sum,
[`SHAFT_POWER_AFFINITY_EXPONENT`], along the affinity family (an engine computation from the
recorded curve — no power value is stored in the record).

### `struct FanSystemIntersectionInput`

Input to [`solve_fan_system_intersection`]; `speed_ratio` and `minimum_flow_fraction` keep
their JavaScript defaults through [`FanSystemIntersectionInput::new`].

### `fn new`

The required inputs; defaults to the JavaScript `speedRatio: 1` and
`minimumFlowFraction: 0.05`.

### `struct FanSystemIntersection`

The balanced operating point, mirroring the JavaScript return object field for field.

### `fn solve_fan_system_intersection`

Port of `solveFanSystemIntersection`.

The root is found by the same scan-then-bisect as the reference: 300 samples over
`[max(minFlow, maxFlow * minimumFlowFraction), maxFlow]`, residual tolerance `1e-7`, and the
**last** bracketed root preferred. The system curve is a fallible closure, so a domain
refusal raised inside it propagates exactly as a JavaScript `throw` does.

### `struct AirflowEstimateInput`

Input to [`estimate_airflow_from_fan_power`].

### `fn estimate_airflow_from_fan_power`

Port of `estimateAirflowFromFanPower`: the fan-affinity estimate
`Q = Q_ref * (P/P_ref * rho_ref/rho)^(1/3)`.

### `const STANDARD_MOTOR_SIZES_KW`

The IEC standard motor sizes the reference selects from, `standardSizesKW` in `fan.js`.

### `struct MotorSelection`

The motor selection, mirroring the JavaScript return object.

### `pub selected_motor_kw: Option<f64>,`

`None` when the required output is above the largest standard size (the JavaScript
`?? null`).

### `fn choose_standard_motor`

Port of `chooseStandardMotor`.

The reference's `standardSizesKW` parameter is not exposed: no JavaScript caller overrides
it. `driveEfficiency` and `serviceFactor` keep their JavaScript call-site values.
