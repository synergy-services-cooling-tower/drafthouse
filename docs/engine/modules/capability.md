# `drafthouse-engine` · `capability`

Characteristic-curve capability — port of `src/core/capability.js`.

Two functions: the capability projection of a test condition against a design condition
(`evaluateCharacteristicCapability`), and the seeded Monte-Carlo propagation of input
standard uncertainties through it (`monteCarloCharacteristicCapability`).

The JavaScript engine is the specification. Mirrored here as it is there: the guard order
(design, then test, then the exponent), the default exponent −0.6, the 720-sample scan with
`prefer: 'last'`, the `−1e12` residual that stands in for an infeasible design demand, the
curve sampling range and the `Number.isFinite` filter over it, and — in the Monte-Carlo —
the Box–Muller draws in the reference's own order, the `samples * 0.5` (floor 20)
acceptance rule and the `n − 1` sample variance.

`towerCharacteristic` and `characteristicCoefficient` — the two `merkel.js` functions this
projection needs — are ported in [`crate::merkel`], the module that mirrors their file.

Known divergence, in a corner the parity harness does not compare: the reference perturbs
a field only when the base object carries it (`Number.isFinite(undefined)` is false), so a
sigma for a top-level scalar (`characteristicExponent`, `salinityGKg`) is skipped when the
caller omitted that key. The Rust input always carries a resolved value, so a sigma for it
always applies. Every compared case passes both keys explicitly.

The perturbation order is the caller's list order, which is the reference's
`Object.entries` order: it decides which field consumes which Gaussian draw, so the two
engines agree only when the lists are in the same order (the harness passes them so).

## Notes carried from the source comments

### `struct CapabilityCondition`

One condition object — the design or the test condition of the reference call. `pressurePa`
is optional because the reference reads it through `?? 101325` at each use site; every other
field is checked finite by [`validate_condition`] before use.

### `fn new`

A condition at the reference's default pressure (the `?? 101325` site).

### `fn pressure`

The `pressurePa ?? 101325` the reference applies at every use site.

### `fn validate_condition`

Port of `validateCondition`: the six required numbers, in the reference's own order, then
the temperature ordering `hot > cold > wet bulb`.

### `struct CharacteristicCapabilityInput`

The inputs of [`evaluate_characteristic_capability`], carrying the reference's default
arguments: exponent −0.6, salinity 0 g/kg, `chebyshev4` integration, 80 curve points and
the `bulk` enthalpy convention.

### `fn new`

`evaluateCharacteristicCapability({ design, test })` — the two required conditions and
every default argument.

### `fn with_characteristic_exponent`

The reference's `characteristicExponent` argument.

### `fn with_salinity_g_kg`

The reference's `salinityGKg` argument.

### `fn with_integration`

The reference's `integration` argument.

### `fn with_curve_points`

The reference's `curvePoints` argument.

### `fn with_inlet_enthalpy_convention`

The reference's `inletEnthalpyConvention` argument.

### `fn demand_input`

The `merkelDemand` call the reference makes for one of the two conditions.

### `struct CapabilityCurvePoint`

One point of the reported design-demand / test-characteristic curve pair.

### `struct CharacteristicCapability`

The object `evaluateCharacteristicCapability` returns, field for field (minus the two the
harness reads as text: `method` and `disclaimer` are in `crates/cli/src/lib.rs`, which renders
them, so this struct carries the physics).

### `fn evaluate_characteristic_capability`

Port of `evaluateCharacteristicCapability`: the test demand sets the characteristic
coefficient, and the capability is the design-condition L/G at which the design demand
meets that characteristic — the last such crossing in the scan interval.

The reference's own stand-in for "infeasible at this L/G".

### `enum ConditionField`

The condition fields a perturbation can carry, in the order the reference's condition
objects declare them.

### `struct CapabilityUncertainty`

Port of the reference's `standardUncertainty` object: one sigma per named field, in the
caller's order (that order is what decides which field draws which Gaussian, so it matters).

### `pub characteristic_exponent: Option<f64>,`

The top-level `characteristicExponent` sigma, when the reference object carries one.

### `pub salinity_g_kg: Option<f64>,`

The top-level `salinityGKg` sigma, when the reference object carries one.

### `struct MonteCarloInput`

The inputs of [`monte_carlo_characteristic_capability`], carrying the reference's defaults
(1000 samples, seed 20260813).

### `fn new`

`monteCarloCharacteristicCapability({ baseInput, standardUncertainty })`.

### `fn with_samples`

The reference's `samples` argument.

### `fn with_seed`

The reference's `seed` argument.

### `struct MonteCarloResult`

The object `monteCarloCharacteristicCapability` returns, field for field.

### `fn perturb_value`

The reference's `target[key] += sigma * gaussianRandom(random)`, which skips a target the
base object does not carry as a finite number.

### `fn perturb_condition`

The reference's recursive walk over one nested object, over a typed condition here.

### `fn monte_carlo_characteristic_capability`

Port of `monteCarloCharacteristicCapability`: perturb the base input `samples` times with
the seeded Gaussian stream, evaluate each draw, and report the accepted capability
distribution. A run that accepts fewer than `max(20, samples / 2)` draws is refused, as in
the reference.
