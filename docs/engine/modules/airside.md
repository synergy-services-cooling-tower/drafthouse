# `drafthouse-engine` · `airside`

Air-side pressure losses and fan-stack discharge — port of `src/core/airside.js`.

Every formula, default and guard mirrors the JavaScript reference; where the reference
carries a `??` default (a loss coefficient, an area fallback, a stack recovery factor) the
Rust struct models the missing field as `Option` and applies the same default here.
Out-of-domain input returns a [`DomainError`] rather than a clamped value.

The functions that take a JavaScript options object take positional arguments or a struct
here; the JavaScript-to-Rust name mapping is in the crate README.

## Notes carried from the source comments

### `struct ZoneCorrelation`

One heat/mass-transfer correlation, the shared shape of `tower.sprayZone`, `tower.rainZone`
and `fill.thermal`:

```text
Me_zone = c * (L''/L''ref)^a * (G''/G''ref)^b * height
```

### `struct FillPressureCorrelation`

The fill pressure-drop correlation (`fill.pressure`).

### `struct FillLimits`

The fill operating limits (`fill.limits`).

### `struct FillRecord`

A validated fill catalog record, reduced to the fields this slice reads.

### `struct TowerRecord`

A validated tower catalog record, reduced to the fields this slice reads. The optional
fields are the ones the JavaScript reference reads through `??` fallbacks.

### `struct DriftCurvePoint`

One point of a drift-eliminator curve.

### `struct DriftEliminatorRecord`

A validated drift-eliminator catalog record.

### `struct DriftPerformance`

The drift performance at one face velocity.

### `struct AirsideAreas`

The flow areas the airside model resolves, mirroring the JavaScript return object.

### `fn velocity_pressure_pa`

Port of `velocityPressurePa`.

### `fn minor_loss_pressure_pa`

Port of `minorLossPressurePa`.

### `fn zone_merkel_number`

Port of `zoneMerkelNumber`. A missing zone contributes nothing and is not an error — the
height is only checked once there is a zone to apply it to, exactly as the reference does.

### `fn fill_thermal_merkel_number`

Port of `fillThermalMerkelNumber`. The depth is asserted before the correlation is looked
up, matching the reference's guard order.

### `fn fill_pressure_drop_pa`

Port of `fillPressureDropPa`.

### `fn drift_performance_at_velocity`

Port of `driftPerformanceAtVelocity`: both curve readings are extrapolated linearly outside
the tabulated range (`clampEnds: false`), so a velocity beyond the curve is not an error.

### `fn resolve_airside_areas`

Port of `resolveAirsideAreas`.

Each loss coefficient in this model is bound to a named area, because K is only meaningful
relative to a stated reference velocity: `dp = K * 0.5 * rho * v_ref^2`. The fan record
wins over the tower's own fan-stack area — pairing the same cell with a larger fan gives a
larger throat and a lower discharge velocity.

### `struct FanStackDischargeInput`

Input to [`fan_stack_discharge_pressure_pa`].

### `pub stack_recovery_factor: f64,`

0 is a plain cylindrical stack; 0.3–0.5 is typical for a velocity-recovery stack.

### `pub fan_pressure_basis: FanPressureBasis,`

`'total'` (the default) charges the outlet velocity pressure against the curve;
`'static'` has already netted it out and returns zero.

### `struct FanStackDischarge`

The discharge term, mirroring the JavaScript return object.

### `pub stack_velocity_ms: Option<f64>,`

`None` when the curve is declared static, so no stack velocity is resolved.

### `fn fan_stack_discharge_pressure_pa`

Port of `fanStackDischargePressurePa`.

Air leaving the stack carries kinetic energy `0.5 * rho * v^2` per unit volume out of the
control volume; that energy is supplied by the fan and is not recovered by the tower, so it
is a genuine load on the fan. Whether it must be charged to the *system* depends on how the
fan curve is published: a total-pressure curve includes it, a static-pressure curve
(`FSP = FTP - outlet velocity pressure`) already has it netted out.

The reference guards with `!(fanStackAreaM2 > 0)`, which also refuses a NaN area; an
explicit `is_nan` keeps that behaviour without a negated comparison clippy rejects.

### `struct SystemPressureBreakdownInput`

Input to [`system_pressure_breakdown`], mirroring the JavaScript options object. The
multipliers default to 1 and `fan` to `None`, as the reference's default arguments do.

### `struct SystemPressureBreakdown`

The full pressure breakdown, mirroring the JavaScript return object field for field.

### `struct LayeredBreakdownInput`

Input to [`layered_system_pressure_breakdown`]: the air-side machinery plus the **resolved
layers**, in physical order (top first). `layers` is never empty on a valid call — an empty
slice is refused, so a caller that loses its stack cannot silently get a no-fill tower.

`thermal_multiplier` and `pressure_multiplier` are the run's: they reach every layer of the
stack (multiplied by that layer's own) and the spray and rain zones above and below it,
exactly as the single-fill `SystemPressureBreakdownInput`'s do.

### `struct LayeredBreakdown`

The layered result: the breakdown [`system_pressure_breakdown`] reports — its fill terms are
the stack's totals — plus one entry per layer, in physical order.

### `fn layered_system_pressure_breakdown`

Port of `systemPressureBreakdown` for an **ordered fill stack**.

The layers sit in series in both the air path and the water path, so each layer sees the
stack's water loading, dry-air loading and airflow, and the stack's fill terms are its
layers' terms summed in physical order: `fillPa = Σ fillPressureDropPa(layer)` and
`fillMerkelNumber = Σ fillThermalMerkelNumber(layer)`. Everything else — the drift
performance, the minor losses, the discharge term, the spray and rain zones — is the
single-fill path unchanged, and the total keeps the single-fill sum's operand order.

With one layer every sum is its single term, which is why the published single-fill numbers
are this path's one-layer case rather than a parallel implementation of it.

The layers are walked in physical order; a refusal is the first layer (top-down) that
cannot be computed, and the accumulation order is the stack's own.

Each minor loss names the velocity its K is referenced to.

With no fan there is no fan stack, so there is no discharge term to charge against a
fan curve. Natural-draft shell exit losses are handled in the draft balance instead.

Both characteristics were resolved to compute this layer's terms, so the
unwraps below cannot fire: the calls above already refused a missing one.

### `fn system_pressure_breakdown`

Port of `systemPressureBreakdown`.

The total available Merkel number is the sum of every transfer zone the air passes through,
not the fill alone; omitting the spray and rain zones understates a real counterflow tower
by roughly 10–20 %.

This is [`layered_system_pressure_breakdown`] with a **one-layer stack**: the caller's fill,
depth and multipliers become a single layer whose own multipliers are 1, so the sums are
their single terms and the arithmetic is the single-fill arithmetic.

### `struct FillOperatingEnvelopeInput`

Input to [`check_fill_operating_envelope`].

### `struct FillOperatingEnvelope`

The envelope verdict, mirroring the JavaScript `{ ok, failures }` object.

### `fn check_fill_operating_envelope`

Port of `checkFillOperatingEnvelope`.

### `fn envelope_failures`

The ported envelope's own failure texts, with the two dimensions a run may not be making a
claim about optional: `None` skips that check instead of inventing a value for it (a run
that declares no hot-water temperature has nothing to compare against the fill's limit).

[`check_fill_operating_envelope`] passes both dimensions, so its verdicts are exactly the
ones the reference produces; [`crate::layers::check_stack_operating_envelope`] calls this
per layer and prefixes each text with the layer it belongs to, which keeps one copy of the
wording in the crate.

### `fn to_fixed`

Port of JavaScript `Number.prototype.toFixed` for the finite values these failure messages
carry.

The reference rounds the *exact* value of the double, ties away from zero. Rust's own
fixed-precision formatting rounds a different digit sequence (it prints `"1.001"` for
`1.0005`, where the reference prints `"1.000"`), so the rounding is done here instead: the
exact expansion is printed with extra digits and the first dropped digit decides — a tie is
a `5` followed by zeros, and ties round away from zero too, so `>= 5` is the whole rule.

A value of exactly zero prints unsigned, matching the reference's `x < 0` sign test.

### `fn increment_decimal`

Add one to a decimal digit string, carrying (`9.99` -> `10.00`).

All nines: the carry opens a new integer place, keeping the fractional digit count.
