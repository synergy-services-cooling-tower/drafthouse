# `drafthouse-engine` · `psychrometrics`

Moist-air properties — port of `src/core/psychrometrics.js`.

Conventions, mirrored from the JavaScript reference (see `docs/ENGINEERING_METHODS.md`):

* ASHRAE Fundamentals saturation-pressure correlation, liquid branch above 0.01 °C and
  ice branch at or below it, limited to -100…200 °C.
* The water-vapour **enhancement factor** `f = 1.0007 + 3.46e-6 P[hPa]` (pressure-only
  Buck 1981) is applied by default; the simplified ASHRAE set is available by turning it
  off. ASHRAE's published tables include this real-mixture effect.
* Out-of-domain inputs return [`DomainError`]; nothing is clamped to a value.

## Notes carried from the source comments

### `top`

J/(kg dry air · K)

### `struct PsychrometricOptions`

Port of `DEFAULT_PSYCHROMETRIC_OPTIONS`:
`enhancementFactor` selects the real-mixture treatment.

### `fn water_vapor_enhancement_factor`

Port of `waterVaporEnhancementFactor`.

The true saturation partial pressure of water in moist air is `f * pws(t)`, with `f`
slightly greater than 1; ignoring it leaves humidity ratios about 0.3-0.6 % low near
ambient conditions.

### `fn saturation_vapor_pressure_pa`

Port of `saturationVaporPressurePa` — ASHRAE Fundamentals, Chapter 1.

### `fn effective_saturation_pressure_pa`

Port of `effectiveSaturationPressurePa`: the quantity that should drive every
humidity-ratio calculation.

### `fn humidity_ratio_from_vapor_pressure`

Port of `humidityRatioFromVaporPressure`.

### `fn vapor_pressure_from_humidity_ratio`

Port of `vaporPressureFromHumidityRatio`.

### `fn saturation_humidity_ratio`

Port of `saturationHumidityRatio`.

### `fn humidity_ratio_from_relative_humidity`

Port of `humidityRatioFromRelativeHumidity`. `relative_humidity` is a fraction, 0…1.

### `fn relative_humidity_from_humidity_ratio`

Port of `relativeHumidityFromHumidityRatio` (clamped to 0…1, as the reference does).

### `fn humidity_ratio_from_wet_bulb`

Port of `humidityRatioFromWetBulb`, including the sub-zero relation.

### `fn wet_bulb_from_humidity_ratio`

Port of `wetBulbFromHumidityRatio`. The residual tolerance is scaled to the humidity
ratio being matched: sub-zero air holds about 6e-4, where a fixed 1e-7 would be a 1e-4
*relative* error.

### `fn dew_point_from_humidity_ratio`

Port of `dewPointFromHumidityRatio`.

### `fn moist_air_enthalpy_kj_kg_dry_air`

Port of `moistAirEnthalpyKJkgDryAir`.

### `fn saturated_air_enthalpy_kj_kg_dry_air`

Port of `saturatedAirEnthalpyKJkgDryAir`.

### `fn dry_air_specific_volume_m3_kg`

Port of `dryAirSpecificVolumeM3kg`.

### `fn dry_air_density_kg_m3`

Port of `dryAirDensityKgM3`.

### `fn moist_air_density_kg_m3`

Port of `moistAirDensityKgM3`.

### `struct PsychrometricStateInput`

Inputs for [`psychrometric_state`] — the port of the JavaScript
`psychrometricState({ dryBulbC, wetBulbC, relativeHumidity, pressurePa, enhancementFactor })`
call. When both `wet_bulb_c` and `relative_humidity` are present the wet bulb wins, as in
the reference.

### `fn from_wet_bulb`

Dry bulb + wet bulb at standard atmospheric pressure.

### `fn from_relative_humidity`

Dry bulb + relative humidity (fraction) at standard atmospheric pressure.

### `struct PsychrometricState`

Port of the `psychrometricState` result object.

### `fn psychrometric_state`

Port of `psychrometricState`.
