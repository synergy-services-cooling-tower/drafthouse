//! The house number style (issue #137, item 3) - the ONE formatter every number on screen, in the Report, in
//! the PDF and in the worked steps goes through.
//!
//! This file is plain `std` Rust so that two crates compile the same source: the cockpit (`theme::num`,
//! which re-exports it as part of the design system) and the engine adapter
//! (`rust/cockpit-adapter/src/real_engine.rs` includes it by path for the worked steps' substitutions).
//! One file, one style: a substitution line and the screen's read-out cannot disagree on a digit.
//!
//! | quantity | style | example |
//! |---|---|---|
//! | temperature, temperature difference | 0.1 | `31.7 °C`, `4.7 K` |
//! | power | 0.1 kW | `28.7 kW` |
//! | flow (m³/h, m³/s, m³/day, kg/s, L/s) | 3 significant figures | `725 m³/h`, `4.15 kg/s`, `0.00682 m³/h` |
//! | KaV/L and other ratios | 2 decimals | `1.60` |
//! | percentage | whole | `112 %` (a non-zero value under 0.5 reads `<1 %`) |
//! | pressure | whole Pa, 0.1 under 10 Pa; kPa at 10 kPa and above | `190 Pa`, `4.6 Pa`, `101 kPa` |
//!
//! The minus sign is U+2212 (`−`), the multiplication sign U+00D7 (`×`); both are in the bundled
//! IBM Plex subsets. The PDF writer maps `−` to its WinAnsi hyphen.

/// The minus sign the house style prints.
pub const MINUS: char = '\u{2212}';

/// One house-style decimal: `-3.0` prints as `−3.0`, and a value that rounds to zero prints without a sign.
/// Public for the few quantities the table has no named style for (a depth in m, a face velocity).
pub fn fixed(v: f64, dp: usize) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    let s = format!("{:.*}", dp, v);
    let zero = s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.');
    if zero {
        return s.trim_start_matches('-').to_string();
    }
    match s.strip_prefix('-') {
        Some(rest) => format!("{MINUS}{rest}"),
        None => s,
    }
}

/// A temperature, °C, to 0.1.
pub fn temp(v: f64) -> String {
    fixed(v, 1)
}

/// A temperature difference, K, to 0.1.
pub fn kelvin(v: f64) -> String {
    fixed(v, 1)
}

/// A signed temperature difference (a margin), K, to 0.1: `+0.3`, `−1.2`.
pub fn kelvin_signed(v: f64) -> String {
    let s = fixed(v, 1);
    if s.starts_with(MINUS) || s.chars().all(|c| c == '0' || c == '.') {
        s
    } else {
        format!("+{s}")
    }
}

/// A power, kW, to 0.1.
pub fn power(v: f64) -> String {
    fixed(v, 1)
}

/// A KaV/L (or any dimensionless ratio), 2 decimals.
pub fn kavl(v: f64) -> String {
    fixed(v, 2)
}

/// A percentage, whole: `112`. A non-zero value that would round to 0 reads `<1` (never a false zero).
/// A house-style magnitude with its sign: `+` for a rise, U+2212 for a fall, none for zero (a change
/// "since" something, where the sign is the message).
pub fn with_sign(magnitude: String, v: f64) -> String {
    if v > 1e-9 {
        format!("+{magnitude}")
    } else if v < -1e-9 {
        format!("{MINUS}{magnitude}")
    } else {
        magnitude
    }
}

pub fn pct(v: f64) -> String {
    if v.is_finite() && v != 0.0 && v.abs() < 0.5 {
        return "<1".into();
    }
    fixed(v, 0)
}

/// A flow (m³/h, m³/s, m³/day, kg/s, L/s), 3 significant figures: `725`, `15.0`, `4.15`, `0.00682`, `1230`.
pub fn flow(v: f64) -> String {
    sig(v, 3)
}

/// `v` to `n` significant figures, never in exponent form.
pub fn sig(v: f64, n: i32) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let mag = v.abs().log10().floor() as i32;
    let dp = n - 1 - mag;
    if dp > 0 {
        let s = fixed(v, dp as usize);
        // a value that rounds up a decade (9.996 -> 10.00) keeps n figures, not n + 1
        let digits = s.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
        let significant = digits.trim_start_matches('0').len() as i32;
        if significant > n && dp > 0 {
            return fixed(v, (dp - 1) as usize);
        }
        s
    } else {
        let unit = 10f64.powi(-dp);
        fixed((v / unit).round() * unit, 0)
    }
}

/// A pressure in Pa: whole Pa, 0.1 Pa under 10 Pa.
pub fn pa(v: f64) -> String {
    if v.abs() < 10.0 {
        fixed(v, 1)
    } else {
        fixed(v, 0)
    }
}

/// A pressure with its unit: Pa below 10 kPa, kPa (3 significant figures) at and above it - `190 Pa`,
/// `101 kPa` for the site's barometric pressure.
pub fn pressure(v_pa: f64) -> String {
    if v_pa.abs() >= 10_000.0 {
        format!("{} kPa", sig(v_pa / 1000.0, 3))
    } else {
        format!("{} Pa", pa(v_pa))
    }
}

/// The number alone, styled by its unit. Units outside the table get 3 significant figures.
pub fn value(v: f64, unit: &str) -> String {
    match unit {
        "°C" | "K" => fixed(v, 1),
        "kW" => power(v),
        "m³/h" | "m³/s" | "m³/day" | "kg/s" | "L/s" | "kg/m²·s" => flow(v),
        "%" => pct(v),
        "Pa" => pa(v),
        "kPa" => sig(v, 3),
        "" | "×" => kavl(v),
        "m" | "m/s" => fixed(v, 2),
        "ppm" => sig(v, 3),
        "kJ/kg·K" | "kg/m³" | "kg/kg" => sig(v, 4),
        "rpm" => fixed(v, 0),
        _ => sig(v, 3),
    }
}

/// The number with its unit, one thin space between: `31.7 °C`, `725 m³/h`, `1.60` (a ratio has no unit).
pub fn with_unit(v: f64, unit: &str) -> String {
    let n = value(v, unit);
    if unit.is_empty() {
        n
    } else if unit == "×" {
        format!("{n}×")
    } else {
        format!("{n} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperatures_and_differences_to_a_tenth() {
        assert_eq!(temp(31.654595), "31.7");
        assert_eq!(kelvin(4.654595), "4.7");
        assert_eq!(kelvin_signed(0.345404), "+0.3");
        assert_eq!(kelvin_signed(-1.26), "−1.3");
        assert_eq!(kelvin_signed(0.04), "0.0");
        assert_eq!(with_unit(31.654595, "°C"), "31.7 °C");
    }

    #[test]
    fn flows_to_three_significant_figures() {
        assert_eq!(flow(724.8), "725");
        assert_eq!(flow(15.03), "15.0");
        assert_eq!(flow(4.148444), "4.15");
        assert_eq!(flow(0.0068153), "0.00682");
        assert_eq!(flow(1234.5), "1230");
        assert_eq!(flow(200.0), "200");
        assert_eq!(flow(9.996), "10.0");
        assert_eq!(flow(0.0), "0");
        assert_eq!(with_unit(139.41118, "kg/s"), "139 kg/s");
    }

    #[test]
    fn power_ratio_percent_pressure() {
        assert_eq!(power(28.730973), "28.7");
        assert_eq!(power(8655.77168), "8655.8");
        assert_eq!(kavl(1.601663), "1.60");
        assert_eq!(pct(111.97163), "112");
        assert_eq!(pct(0.32), "<1");
        assert_eq!(pct(0.0), "0");
        assert_eq!(pa(190.149), "190");
        assert_eq!(pa(4.5518), "4.6");
        assert_eq!(pressure(101_325.0), "101 kPa");
        assert_eq!(pressure(190.149), "190 Pa");
    }

    #[test]
    fn the_minus_sign_is_the_typographic_one_and_zero_has_no_sign() {
        assert_eq!(temp(-3.04), "−3.0");
        assert_eq!(temp(-0.04), "0.0");
        assert_eq!(kavl(-0.6), "−0.60");
    }
}
