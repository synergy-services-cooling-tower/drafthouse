# `drafthouse-engine` · `water_balance`

Water balance — port of `src/core/waterBalance.js`.

Three functions, mirrored line for line: the formulas, the guard order, the two default
arguments (`driftKgS` 0, `cyclesOfConcentration` 4), the `Math.max(0, …)` floors and the
refusals are the reference's. The JavaScript engine is the specification: a reference
quirk found here is a finding to report, not something to improve on.

The refusals, input for input, both engines (issue #16 changed both sides of the port at
once, in the reference first):

* a flow (`dryAirMassFlowKgS`, `circulatingWaterMassFlowKgS`) is positive;
* the two losses the balance sums (`evaporationKgS`, `driftKgS`) and the two humidity
  ratios of the air-mass balance are non-negative and finite — zero is legitimate (a
  no-load balance, a drift-free design, dry air), a negative or non-finite one is refused;
* `driftPpm` is finite and not negative.

Before issue #16 this module documented the opposite as a known reference quirk:
`coolingTowerWaterBalance` asserted neither `evaporationKgS` nor `driftKgS` (so a negative
evaporation returned a negative makeup), the humidity ratios of
`evaporationFromAirMassBalance` were unvalidated, and `driftLossKgS`'s `< 0` ppm check let
a NaN through (`NaN < 0.0` is false). The remaining leniency is stated where it lives:
`NaN <= 1.0` is false, so a NaN `cyclesOfConcentration` still passes the cycles guard and
propagates as NaN.

## Notes carried from the source comments

### `fn floor_at_zero`

Port of the reference's two `Math.max(0, value)` floors.

`Math.max(0, NaN)` is `NaN`, where Rust's `value.max(0.0)` would return `0.0` and quietly
swallow it; the floor must not turn a NaN into a zero.

### `fn evaporation_from_air_mass_balance`

Port of `evaporationFromAirMassBalance`: dry air is conserved through the tower, so the
water lost to evaporation is the dry-air flow times the rise in humidity ratio. The floor
at zero is the reference's: condensation (`outlet < inlet`) is not credited back.

The humidity ratios are non-negative and finite (issue #16): a humidity ratio is a mass
ratio, refused as negative wherever else the engine takes one
(`vaporPressureFromHumidityRatio`), and a NaN used to reach the subtraction unchecked.
Zero is dry air and is not refused.

### `fn drift_loss_kg_s`

Port of `driftLossKgS`: the circulating flow times the drift rating in parts per million.

The reference asserts the flow and refuses a negative ppm; a zero ppm is a legitimate
(drift-free) design and is not refused. The ppm is checked finite first (issue #16):
`drift_ppm < 0.0` is false for a NaN, which is how a NaN used to propagate out of the
engine as if it were a rating.

### `struct WaterBalanceInput`

The inputs of [`cooling_tower_water_balance`], carrying the reference's default arguments:
`driftKgS` defaults to 0 and `cyclesOfConcentration` to 4, applied where the reference
applies them (before the function body, so before its guard).

### `fn new`

`coolingTowerWaterBalance({ evaporationKgS })` — both default arguments apply.

### `fn with_drift_kg_s`

The reference's `driftKgS` argument, whose default is 0.

### `fn with_cycles_of_concentration`

The reference's `cyclesOfConcentration` argument, whose default is 4.

### `struct WaterBalance`

The object `coolingTowerWaterBalance` returns, field for field.

### `fn cooling_tower_water_balance`

Port of `coolingTowerWaterBalance`: the cycles guard comes first, before any arithmetic,
then the two losses are asserted non-negative and finite, then blowdown at
`evaporation / (cycles − 1) − drift` floored at zero, then makeup as the sum of the three
losses.

The two losses are refused when negative or non-finite (issue #16) — the reference used to
return a negative makeup for a negative evaporation, and a negative `driftKgS` was
accepted while `driftLossKgS` refused a negative ppm, two paths into the same quantity
disagreeing. Zero stays legitimate on both: the no-load balance, and `driftKgS`'s
documented default.
