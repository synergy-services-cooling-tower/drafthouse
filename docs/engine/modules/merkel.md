# `drafthouse-engine` · `merkel`

Counterflow Merkel demand — port of `src/core/merkel.js` (the slice excludes
`estimateOutletAirState` and `generateDemandCurve`: the first is ported with the selector,
`selection.rs`, and the second is still unported — nothing in the ported slices needs it).

`KaV/L = ∫ cp dTw / (hs(Tw) - ha(Tw))`, with the entering-air enthalpy taken either as
the true moist-air enthalpy of the measured dry bulb/wet bulb pair (`bulk`, the default)
or as saturated air at the entering wet bulb (`cti-saturated-wetbulb`). The two
conventions differ by roughly 1 % at a typical design point; the difference is real and
is recorded in the vectors.

## Notes carried from the source comments

### `enum Integration`

Which quadrature to use, mirroring the JavaScript `integration` option.

### `enum InletEnthalpyConvention`

Entering-air enthalpy convention, mirroring the JavaScript
`inletEnthalpyConvention` option.

### `Bulk,`

True moist-air enthalpy of the entering dry bulb/wet bulb pair (default).

### `CtiSaturatedWetBulb,`

Enthalpy of saturated air at the entering wet bulb; the CTI demand-curve convention.

### `fn validate_thermal_temperatures`

Port of `validateThermalTemperatures`.

`!(a > b)`, written so it stays true for NaN (the JavaScript comparison does too).

### `fn inlet_air_enthalpy_kj_kg_dry_air`

Port of `inletAirEnthalpyKJkgDryAir`.

### `fn air_enthalpy_operating_line_kj_kg`

Port of `airEnthalpyOperatingLineKJkg`.

### `struct MerkelInput`

Inputs for [`merkel_demand`] — the port of the JavaScript `merkelDemand({...})` call.

### `pub pressure_pa: f64,`

Default 101 325 Pa.

### `pub salinity_g_kg: f64,`

Default 0 g/kg.

### `pub integration: Integration,`

Default [`Integration::Simpson`].

### `pub segments: usize,`

Simpson segment count. Default 240.

### `pub minimum_enthalpy_potential: f64,`

Default 1e-7.

### `pub inlet_enthalpy_convention: InletEnthalpyConvention,`

Default [`InletEnthalpyConvention::Bulk`].

### `fn new`

The five physically required inputs; everything else takes the reference default.

### `struct MerkelResult`

Port of the `merkelDemand` result object.

### `fn merkel_demand`

Port of `merkelDemand`.

### `fn safe_merkel_demand`

Port of `safeMerkelDemand`: a pinch returns positive infinity (as the reference does)
instead of an error; every other refusal still propagates.

### `fn tower_characteristic`

Port of `towerCharacteristic`: the whole-tower characteristic `coefficient · L/G^exponent`
the capability projection compares the design demand against. Both the ratio and the
coefficient are asserted positive, as in the reference.

### `fn characteristic_coefficient`

Port of `characteristicCoefficient`: the coefficient that makes the characteristic pass
through a measured demand at a measured L/G — the test-condition anchor of the capability
projection.

### `struct ColdWaterTemperatureInput`

Inputs for [`solve_cold_water_temperature`] — the port of the JavaScript
`solveColdWaterTemperature({...})` call (which takes no segment count).

### `pub pressure_pa: f64,`

Default 101 325 Pa.

### `pub salinity_g_kg: f64,`

Default 0 g/kg.

### `pub integration: Integration,`

Default [`Integration::Simpson`].

### `pub inlet_enthalpy_convention: InletEnthalpyConvention,`

Default [`InletEnthalpyConvention::Bulk`].

### `fn new`

The five physically required inputs; everything else takes the reference default.

### `struct ColdWaterTemperatureResult`

Port of the `solveColdWaterTemperature` result object.

### `fn solve_cold_water_temperature`

Port of `solveColdWaterTemperature`.
