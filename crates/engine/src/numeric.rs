use std::cmp::Ordering;
use std::fmt;

/// Port of the JavaScript `DomainError`: a refusal to produce a physically meaningless number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainError {
    message: String,
}

impl DomainError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DomainError {}

/// # Errors `DomainError` when the input is out of domain. Port of `assertFiniteNumber`.
pub fn assert_finite_number(value: f64, name: &str) -> Result<f64, DomainError> {
    if !value.is_finite() {
        return Err(DomainError::new(format!("{name} must be a finite number.")));
    }
    Ok(value)
}

/// # Errors `DomainError` when the input is out of domain. Port of `assertPositive`; the JavaScript `allowZero` variant is [`assert_non_negative`].
pub fn assert_positive(value: f64, name: &str) -> Result<f64, DomainError> {
    assert_finite_number(value, name)?;
    if value <= 0.0 {
        return Err(DomainError::new(format!("{name} must be positive.")));
    }
    Ok(value)
}

/// # Errors `DomainError` when the input is out of domain. Port of the JavaScript `assertPositive(value, name, { allowZero: true })`: a finite, non-negative quantity.
pub fn assert_non_negative(value: f64, name: &str) -> Result<f64, DomainError> {
    assert_finite_number(value, name)?;
    if value < 0.0 {
        return Err(DomainError::new(format!("{name} must be non-negative.")));
    }
    Ok(value)
}

/// Port of `clamp`; returns NaN for a NaN input, like the JavaScript `Math.min/Math.max` pair it replaces.
#[must_use]
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

/// Port of two-argument `Math.min`: a NaN operand propagates, where Rust's own `f64::min` returns the other operand and would quietly turn a NaN into a plausible number.
#[must_use]
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// Port of two-argument `Math.max`, with the same NaN rule as [`js_min`].
#[must_use]
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// `value as f64` for a count, exact below 2^32.
#[must_use]
pub fn count_as_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

/// `value as u64`: truncation toward zero, saturating, NaN to zero.
#[must_use]
pub fn truncate_to_u64(value: f64) -> u64 {
    if value.is_nan() || value < 1.0 {
        return 0;
    }
    if value >= 18_446_744_073_709_551_616.0 {
        return u64::MAX;
    }
    let bits = value.to_bits();
    let exponent = (bits >> 52) & 0x7ff;
    let mantissa = (bits & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000;
    if exponent >= 1075 {
        mantissa << (exponent - 1075)
    } else {
        mantissa >> (1075 - exponent)
    }
}

fn same(a: f64, b: f64) -> bool {
    a.partial_cmp(&b) == Some(Ordering::Equal)
}

fn inverse_lerp(a: f64, b: f64, value: f64) -> f64 {
    if same(a, b) {
        0.0
    } else {
        (value - a) / (b - a)
    }
}

/// # Errors `DomainError` when the input is out of domain. Port of `interpolate1D`: linear interpolation over an unsorted table of records.
pub fn interpolate_1d<P, X, Y>(
    points: &[P],
    x: f64,
    x_of: X,
    y_of: Y,
    clamp_ends: bool,
) -> Result<f64, DomainError>
where
    X: Fn(&P) -> f64,
    Y: Fn(&P) -> f64,
{
    let mut sorted: Vec<(f64, f64)> = points
        .iter()
        .map(|point| (x_of(point), y_of(point)))
        .collect();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let (&[first, second, ..], &[.., previous, last]) = (sorted.as_slice(), sorted.as_slice())
    else {
        return Err(DomainError::new(
            "At least two interpolation points are required.",
        ));
    };
    if x <= first.0 {
        if clamp_ends {
            return Ok(first.1);
        }
        return Ok(lerp(first.1, second.1, inverse_lerp(first.0, second.0, x)));
    }
    if x >= last.0 {
        if clamp_ends {
            return Ok(last.1);
        }
        return Ok(lerp(
            previous.1,
            last.1,
            inverse_lerp(previous.0, last.0, x),
        ));
    }
    for pair in sorted.windows(2) {
        if let &[a, b] = pair {
            if x >= a.0 && x <= b.0 {
                return Ok(lerp(a.1, b.1, inverse_lerp(a.0, b.0, x)));
            }
        }
    }
    Err(DomainError::new("Interpolation failed unexpectedly."))
}

fn math_sign(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

/// # Errors `DomainError` when the input is out of domain. Port of `integrateSimpson`: composite Simpson's rule with the segment count rounded up to an even number (minimum 2).
pub fn integrate_simpson<F>(
    function: F,
    lower: f64,
    upper: f64,
    segments: usize,
) -> Result<f64, DomainError>
where
    F: Fn(f64) -> Result<f64, DomainError>,
{
    assert_finite_number(lower, "integration lower bound")?;
    assert_finite_number(upper, "integration upper bound")?;
    if same(upper, lower) {
        return Ok(0.0);
    }
    let mut n = segments.max(2);
    if !n.is_multiple_of(2) {
        n += 1;
    }
    let h = (upper - lower) / count_as_f64(n);
    let mut sum = function(lower)? + function(upper)?;
    for i in 1..n {
        let weight = if i % 2 == 0 { 2.0 } else { 4.0 };
        sum += weight * function(lower + count_as_f64(i) * h)?;
    }
    Ok((h / 3.0) * sum)
}

/// # Errors `DomainError` when the input is out of domain. Port of `integrateChebyshev4`: four-point equal-weight Tchebycheff quadrature, the form used in cooling-tower worksheets.
pub fn integrate_chebyshev4<F>(function: F, lower: f64, upper: f64) -> Result<f64, DomainError>
where
    F: Fn(f64) -> Result<f64, DomainError>,
{
    const FRACTIONS: [f64; 4] = [
        0.102_672_763_854,
        0.406_203_762_957,
        0.593_796_237_043,
        0.897_327_236_146,
    ];
    if same(upper, lower) {
        return Ok(0.0);
    }
    let span = upper - lower;
    let mut total = 0.0;
    for fraction in FRACTIONS {
        total += function(lower + fraction * span)?;
    }
    Ok((span / 4.0) * total)
}

/// Options for [`solve_bracketed_root`].
#[derive(Clone, Copy, Debug)]
pub struct BracketedRootOptions {
    pub tolerance: f64,
    pub x_tolerance: Option<f64>,
    pub max_iterations: usize,
}

impl Default for BracketedRootOptions {
    fn default() -> Self {
        Self {
            tolerance: 1e-8,
            x_tolerance: None,
            max_iterations: 160,
        }
    }
}

/// # Errors `DomainError` when the input is out of domain. Port of `solveBracketedRoot`: bisection on a bracketed root, stopping on either the residual `tolerance` or the bracket width `x_tolerance`.
pub fn solve_bracketed_root<F>(
    function: F,
    lower: f64,
    upper: f64,
    options: BracketedRootOptions,
) -> Result<f64, DomainError>
where
    F: Fn(f64) -> Result<f64, DomainError>,
{
    let x_tolerance = options.x_tolerance.unwrap_or(options.tolerance);
    let f_lower = function(lower)?;
    let f_upper = function(upper)?;
    if !f_lower.is_finite() || !f_upper.is_finite() {
        return Err(DomainError::new(
            "Root endpoints must evaluate to finite values.",
        ));
    }
    if f_lower == 0.0 {
        return Ok(lower);
    }
    if f_upper == 0.0 {
        return Ok(upper);
    }
    if math_sign(f_lower) == math_sign(f_upper) {
        return Err(DomainError::new("Root is not bracketed."));
    }

    let mut lo = lower;
    let mut hi = upper;
    let mut f_lo = f_lower;
    for _ in 0..options.max_iterations {
        let midpoint = f64::midpoint(lo, hi);
        let f_mid = function(midpoint)?;
        if !f_mid.is_finite() {
            return Err(DomainError::new(
                "Root function returned a non-finite value.",
            ));
        }
        if f_mid.abs() <= options.tolerance || (hi - lo).abs() <= x_tolerance {
            return Ok(midpoint);
        }
        if math_sign(f_lo) == math_sign(f_mid) {
            lo = midpoint;
            f_lo = f_mid;
        } else {
            hi = midpoint;
        }
    }
    Ok(f64::midpoint(lo, hi))
}

/// Which root to return when a scan finds several, mirroring the JavaScript `prefer` option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootPreference {
    First,
    Last,
}

/// Options for [`find_root_by_scan`].
#[derive(Clone, Copy, Debug)]
pub struct RootScanOptions {
    pub samples: usize,
    pub tolerance: f64,
    pub max_iterations: usize,
    pub prefer: RootPreference,
}

impl Default for RootScanOptions {
    fn default() -> Self {
        Self {
            samples: 240,
            tolerance: 1e-8,
            max_iterations: 160,
            prefer: RootPreference::First,
        }
    }
}

/// # Errors `DomainError` when the input is out of domain. Port of `findRootByScan`: sample the interval, bracket every sign change, and refine it with bisection.
pub fn find_root_by_scan<F>(
    function: F,
    lower: f64,
    upper: f64,
    options: RootScanOptions,
) -> Result<f64, DomainError>
where
    F: Fn(f64) -> Result<f64, DomainError>,
{
    if upper <= lower {
        return Err(DomainError::new(
            "Root scan upper bound must exceed lower bound.",
        ));
    }
    let mut roots: Vec<f64> = Vec::new();
    let mut previous_x = lower;
    let mut previous_f = function(previous_x)?;

    for i in 1..=options.samples {
        let x = lower + ((upper - lower) * count_as_f64(i)) / count_as_f64(options.samples);
        let f_x = function(x)?;
        if previous_f.is_finite() && f_x.is_finite() {
            if previous_f == 0.0 {
                roots.push(previous_x);
            }
            if math_sign(previous_f) != math_sign(f_x) {
                roots.push(solve_bracketed_root(
                    &function,
                    previous_x,
                    x,
                    BracketedRootOptions {
                        tolerance: options.tolerance,
                        x_tolerance: None,
                        max_iterations: options.max_iterations,
                    },
                )?);
            }
        }
        previous_x = x;
        previous_f = f_x;
    }

    let chosen = match options.prefer {
        RootPreference::Last => roots.last(),
        RootPreference::First => roots.first(),
    };
    chosen
        .copied()
        .ok_or_else(|| DomainError::new("No bracketed root was found in the requested interval."))
}

/// # Errors `Err` when the input is out of domain. Port of `bracketValues`: the two sorted unique values that bracket `target`.
pub fn bracket_values(values: &[f64], target: f64) -> Result<(f64, f64), DomainError> {
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    sorted.dedup();
    let (Some(&first), Some(&last)) = (sorted.first(), sorted.last()) else {
        return Err(DomainError::new("Cannot bracket an empty value set."));
    };
    if target <= first {
        return Ok((first, first));
    }
    if target >= last {
        return Ok((last, last));
    }
    for pair in sorted.windows(2) {
        if let &[low, high] = pair {
            if target >= low && target <= high {
                return Ok((low, high));
            }
        }
    }
    Ok((first, last))
}

/// Port of `bilinear`: linear interpolation across the first axis, then across the second.
#[must_use]
pub fn bilinear(q11: f64, q21: f64, q12: f64, q22: f64, tx: f64, ty: f64) -> f64 {
    let bottom = lerp(q11, q21, tx);
    let top = lerp(q12, q22, tx);
    lerp(bottom, top, ty)
}

/// Port of `range`: `count` values from `start` to `stop` inclusive.
#[must_use]
pub fn range(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count < 2 {
        return vec![start];
    }
    (0..count)
        .map(|index| lerp(start, stop, count_as_f64(index) / count_as_f64(count - 1)))
        .collect()
}

/// Port of `createSeededRandom`: the reference's xorshift-style 32-bit generator, state and all, so the same seed draws the same stream.
#[derive(Clone, Copy, Debug)]
pub struct SeededRandom {
    state: u32,
}

impl SeededRandom {
    #[must_use]
    pub fn new(seed: f64) -> Self {
        Self {
            state: u32::try_from(truncate_to_u64(seed.rem_euclid(4_294_967_296.0)))
                .unwrap_or(u32::MAX),
        }
    }

    /// Port of the returned closure's one draw: a `f64` in `[0, 1)`.
    pub fn next_value(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3D));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// Port of `createSeededRandom(seed)` — the same object, spelled the way the reference names it.
#[must_use]
pub fn create_seeded_random(seed: f64) -> SeededRandom {
    SeededRandom::new(seed)
}

/// Port of `gaussianRandom`: the Box–Muller transform, drawing again while a draw is exactly zero, exactly as the reference does.
pub fn gaussian_random<F: FnMut() -> f64>(random: &mut F) -> f64 {
    let mut u = 0.0;
    while u == 0.0 {
        u = random();
    }
    let mut v = 0.0;
    while v == 0.0 {
        v = random();
    }
    (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
}

/// Port of `percentile`: the linear interpolation between the two nearest order statistics of a sorted copy, with the position clamped to the sample range.
#[must_use]
pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let index = clamp(p, 0.0, 1.0) * count_as_f64(sorted.len() - 1);
    let lower = index.floor();
    let upper = index.ceil();
    let at = |position: f64| {
        usize::try_from(truncate_to_u64(position))
            .ok()
            .and_then(|slot| sorted.get(slot).copied())
            .unwrap_or(f64::NAN)
    };
    lerp(at(lower), at(upper), index - lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Point {
        x: f64,
        y: f64,
    }

    fn curve(points: &[(f64, f64)]) -> Vec<Point> {
        points.iter().map(|(x, y)| Point { x: *x, y: *y }).collect()
    }

    fn interpolate(points: &[(f64, f64)], x: f64, clamp_ends: bool) -> Result<f64, DomainError> {
        interpolate_1d(
            &curve(points),
            x,
            |point| point.x,
            |point| point.y,
            clamp_ends,
        )
    }

    fn value(points: &[(f64, f64)], x: f64, clamp_ends: bool) -> f64 {
        interpolate(points, x, clamp_ends).expect("expected a value")
    }

    #[test]
    fn a_mid_table_input_interpolates_between_its_bracketing_points() {
        assert_eq!(value(&[(0.0, 0.0), (10.0, 100.0)], 5.0, true), 50.0);
        assert_eq!(value(&[(0.0, 0.0), (10.0, 100.0)], 5.0, false), 50.0);
        assert_eq!(
            value(
                &[(0.0, 10.0), (2.5, 6.0), (5.0, 9.0), (7.5, 14.0)],
                6.25,
                true
            ),
            11.5
        );
    }

    #[test]
    fn an_input_below_the_first_abscissa_clamps_or_extrapolates() {
        assert_eq!(value(&[(0.0, 0.0), (10.0, 100.0)], -5.0, true), 0.0);
        assert_eq!(value(&[(0.0, 0.0), (10.0, 100.0)], -5.0, false), -50.0);
    }

    #[test]
    fn an_input_above_the_last_abscissa_clamps_or_extrapolates() {
        assert_eq!(value(&[(0.0, 0.0), (10.0, 100.0)], 15.0, true), 100.0);
        assert_eq!(value(&[(0.0, 0.0), (10.0, 100.0)], 15.0, false), 150.0);
        assert_eq!(
            value(&[(0.0, 0.0), (10.0, 100.0)], f64::INFINITY, true),
            100.0
        );
    }

    #[test]
    fn an_unsorted_table_interpolates_as_if_it_had_been_sorted() {
        assert_eq!(value(&[(10.0, 100.0), (0.0, 0.0)], 5.0, true), 50.0);
        assert_eq!(
            value(
                &[(7.5, 14.0), (0.0, 10.0), (5.0, 9.0), (2.5, 6.0)],
                6.25,
                true
            ),
            11.5
        );
    }

    #[test]
    fn a_table_with_fewer_than_two_points_is_refused() {
        for points in [vec![], vec![(1.0, 1.0)]] {
            let error = interpolate(&points, 1.0, true).expect_err("must refuse");
            assert_eq!(
                error.message(),
                "At least two interpolation points are required."
            );
        }
    }

    #[test]
    fn duplicate_abscissae_resolve_to_the_first_ordinate_of_the_pair() {
        assert_eq!(
            value(&[(5.0, 10.0), (5.0, 20.0), (10.0, 30.0)], 5.0, true),
            10.0
        );
        assert_eq!(
            value(&[(5.0, 10.0), (5.0, 20.0), (10.0, 30.0)], 5.0, false),
            10.0
        );
        assert_eq!(value(&[(5.0, 10.0), (5.0, 20.0)], 5.0, false), 10.0);
        assert_eq!(
            value(
                &[(0.0, 0.0), (5.0, 10.0), (5.0, 20.0), (10.0, 30.0)],
                5.0,
                true
            ),
            10.0
        );
        assert_eq!(
            value(
                &[(0.0, 0.0), (5.0, 10.0), (5.0, 20.0), (10.0, 30.0)],
                7.0,
                true
            ),
            24.0
        );
        assert_eq!(
            value(&[(0.0, 0.0), (10.0, 10.0), (10.0, 20.0)], 10.0, true),
            20.0
        );
        assert_eq!(
            value(&[(0.0, 0.0), (10.0, 10.0), (10.0, 20.0)], 10.0, false),
            10.0
        );
    }

    #[test]
    fn an_input_that_cannot_be_bracketed_is_refused_not_returned_as_nan() {
        let error = interpolate(&[(0.0, 0.0), (10.0, 100.0)], f64::NAN, true)
            .expect_err("NaN must be refused");
        assert_eq!(error.message(), "Interpolation failed unexpectedly.");
        assert!(interpolate(&[(0.0, 0.0), (10.0, 100.0)], f64::NAN, false).is_err());
        assert!(interpolate(&[(f64::NAN, 5.0), (10.0, 100.0)], 5.0, false).is_err());
        assert!(interpolate(&[(f64::NAN, 5.0), (10.0, 100.0)], 5.0, true).is_err());
    }
}
