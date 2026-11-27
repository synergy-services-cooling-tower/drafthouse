# `drafthouse-engine` · `numeric`

Numeric primitives — port of the parts of `src/core/numeric.js` the slice uses.

Two deliberate deviations from the JavaScript reference, both documented in the crate
README:

* The JavaScript `DomainError` carries a `details` object; here it carries the message
  only (the message is what callers and tests match on).
* Every solver takes a fallible closure (`Fn(f64) -> Result<f64, DomainError>`) so a
  domain failure raised *inside* an integrand or root function propagates exactly as
  the JavaScript `throw` does, instead of being flattened into a value.

## Notes carried from the source comments

### `struct DomainError`

Port of the JavaScript `DomainError`: a refusal to produce a physically meaningless
number. Inputs outside the supported domain return this, never a clamped value.

### `fn assert_finite_number`

Port of `assertFiniteNumber`.

### `fn assert_positive`

Port of `assertPositive`; the JavaScript `allowZero` variant is [`assert_non_negative`].

### `fn assert_non_negative`

Port of the JavaScript `assertPositive(value, name, { allowZero: true })`: a finite,
non-negative quantity. The finite check runs first, which is also what refuses a NaN —
`NaN < 0.0` is false, so the comparison alone cannot.

Ported for the water balance (issue #16), where zero is a legitimate quantity: a no-load
balance, a drift-free design, dry air.

### `fn clamp`

Port of `clamp`; returns NaN for a NaN input, like the JavaScript `Math.min/Math.max`
pair it replaces.

### `fn js_min`

Port of two-argument `Math.min`: a NaN operand propagates, where Rust's own `f64::min`
returns the other operand and would quietly turn a NaN into a plausible number.

### `fn js_max`

Port of two-argument `Math.max`, with the same NaN rule as [`js_min`].

### `fn lerp`

Port of the JavaScript `lerp`.

### `fn inverse_lerp`

Port of the JavaScript `inverseLerp`; a degenerate interval maps to 0.

### `fn interpolate_1d`

Port of `interpolate1D`: linear interpolation over an unsorted table of records.

The JavaScript version selects the abscissa and ordinate by key name (`xKey`, `yKey`);
here the caller passes accessors instead, so one primitive serves every record shape
(`driftEliminator.curve` and `fan.curve` in this slice).

The reference's edge behaviour is mirrored, not "fixed":

* Fewer than two points is refused (`At least two interpolation points are required.`).
* An input below the first or above the last abscissa is clamped when `clamp_ends` is
  set, and linearly extrapolated from the two end points when it is not.
* The table is copied and sorted by abscissa before use, so an unsorted table interpolates
  as if it had been sorted; a comparator result of NaN (a non-finite key) leaves the pair
  in place, as the JavaScript sort does.
* Duplicate abscissae are not merged: whichever pair the scan reaches first wins, and
  `inverse_lerp` returns 0 for a degenerate pair, so a duplicate pair resolves to its
  first ordinate.
* An input that cannot be bracketed at all (a NaN input, or a NaN abscissa in the table)
  is refused (`Interpolation failed unexpectedly.`) rather than silently returned as NaN.

### `fn math_sign`

Port of `Math.sign` for the finite values the solvers deal with. `Math.sign(-0)` is `-0`,
which compares equal to `0`, so both map to `0.0` here.

### `fn integrate_simpson`

Port of `integrateSimpson`: composite Simpson's rule with the segment count rounded up
to an even number (minimum 2).

### `fn integrate_chebyshev4`

Port of `integrateChebyshev4`: four-point equal-weight Tchebycheff quadrature, the form
used in cooling-tower worksheets.

### `struct BracketedRootOptions`

Options for [`solve_bracketed_root`]. `x_tolerance` defaults to `tolerance`, matching the
JavaScript option pair.

### `fn solve_bracketed_root`

Port of `solveBracketedRoot`: bisection on a bracketed root, stopping on either the
residual `tolerance` or the bracket width `x_tolerance`.

### `enum RootPreference`

Which root to return when a scan finds several, mirroring the JavaScript `prefer` option.

### `struct RootScanOptions`

Options for [`find_root_by_scan`].

### `fn find_root_by_scan`

Port of `findRootByScan`: sample the interval, bracket every sign change, and refine it
with bisection.

### `fn bracket_values`

Port of `bracketValues`: the two sorted unique values that bracket `target`.

The reference dedupes through `new Set(values)` and sorts numerically; here the values are
copied, sorted and deduped the same way for the finite values a performance-curve grid
carries (`NaN` compares unequal to itself under `PartialEq`, so a repeated `NaN` survives
the `dedup` where the `Set` would have kept one — non-finite grid values are out of the
ported behaviour). A target at or beyond either end collapses the bracket to that end, and
a target the loop cannot bracket (a NaN) falls through to the ends, as the reference does.

### `fn bilinear`

Port of `bilinear`: linear interpolation across the first axis, then across the second.

### `fn range`

Port of `range`: `count` values from `start` to `stop` inclusive. Fewer than two points
returns the start alone, exactly as the reference does.

### `struct SeededRandom`

Port of `createSeededRandom`: the reference's xorshift-style 32-bit generator, state and
all, so the same seed draws the same stream.

The JavaScript `state += K` is not truncated by the addition itself, but every later use
goes through `ToInt32`/`ToUint32`, so the whole sequence is 32-bit wrapping arithmetic —
which is what this reproduces. The seed is `seed >>> 0` (`ToUint32`).

### `fn new`

`ToUint32`: truncate toward zero, then modulo 2^32 (NaN becomes 0, as in JavaScript).

### `fn next_value`

Port of the returned closure's one draw: a `f64` in `[0, 1)`.

### `fn create_seeded_random`

Port of `createSeededRandom(seed)` — the same object, spelled the way the reference names it.

### `fn gaussian_random`

Port of `gaussianRandom`: the Box–Muller transform, drawing again while a draw is exactly
zero, exactly as the reference does.

### `fn percentile`

Port of `percentile`: the linear interpolation between the two nearest order statistics of
a sorted copy, with the position clamped to the sample range. An empty sample set is NaN,
as in the reference.

### `struct Point`

A keyed record, the shape the JavaScript `interpolate1D` takes.

### `fn interpolate`

The JavaScript `interpolate1D(points, x, 'x', 'y', { clampEnds })`, reduced to the
key names every call in this crate uses.

### `fn an_input_below_the_first_abscissa_clamps_or_extrapolates`

Measured against the reference: clampEnds -> sorted[0].y; otherwise the first pair
is extrapolated (a linear continuation, not a clamp).

### `fn an_input_above_the_last_abscissa_clamps_or_extrapolates`

An infinite input is "above the last abscissa" and clamps like any other.

### `fn duplicate_abscissae_resolve_to_the_first_ordinate_of_the_pair`

`inverseLerp(a, b, x)` is 0 when a === b, so the lower ordinate of the pair wins.
(Reference values measured from src/core/numeric.js::interpolate1D.)

Mid-table, the scan reaches the pair whose upper bound is the duplicate.

At the top end a clamped input returns the LAST table entry, not the first of the
duplicate pair; without clamping the last pair is extrapolated from.

### `fn an_input_that_cannot_be_bracketed_is_refused_not_returned_as_nan`

A non-finite abscissa in the table cannot be bracketed either.
