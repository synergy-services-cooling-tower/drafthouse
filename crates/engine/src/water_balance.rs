//! Water balance — port of `src/core/waterBalance.js`.
//!
//! Three functions, mirrored line for line: the formulas, the guard order, the two default
//! arguments (`driftKgS` 0, `cyclesOfConcentration` 4), the `Math.max(0, …)` floors and the
//! refusals are the reference's. The JavaScript engine is the specification: a reference
//! quirk found here is a finding to report, not something to improve on.
//!
//! The refusals, input for input, both engines (issue #16 changed both sides of the port at
//! once, in the reference first):
//!
//! * a flow (`dryAirMassFlowKgS`, `circulatingWaterMassFlowKgS`) is positive;
//! * the two losses the balance sums (`evaporationKgS`, `driftKgS`) and the two humidity
//!   ratios of the air-mass balance are non-negative and finite — zero is legitimate (a
//!   no-load balance, a drift-free design, dry air), a negative or non-finite one is refused;
//! * `driftPpm` is finite and not negative.
//!
//! Before issue #16 this module documented the opposite as a known reference quirk:
//! `coolingTowerWaterBalance` asserted neither `evaporationKgS` nor `driftKgS` (so a negative
//! evaporation returned a negative makeup), the humidity ratios of
//! `evaporationFromAirMassBalance` were unvalidated, and `driftLossKgS`'s `< 0` ppm check let
//! a NaN through (`NaN < 0.0` is false). The remaining leniency is stated where it lives:
//! `NaN <= 1.0` is false, so a NaN `cyclesOfConcentration` still passes the cycles guard and
//! propagates as NaN.

use crate::numeric::{assert_finite_number, assert_non_negative, assert_positive, DomainError};

/// Port of the reference's two `Math.max(0, value)` floors.
///
/// `Math.max(0, NaN)` is `NaN`, where Rust's `value.max(0.0)` would return `0.0` and quietly
/// swallow it; the floor must not turn a NaN into a zero.
fn floor_at_zero(value: f64) -> f64 {
    if value > 0.0 {
        value
    } else if value.is_nan() {
        f64::NAN
    } else {
        0.0
    }
}

/// Port of `evaporationFromAirMassBalance`: dry air is conserved through the tower, so the
/// water lost to evaporation is the dry-air flow times the rise in humidity ratio. The floor
/// at zero is the reference's: condensation (`outlet < inlet`) is not credited back.
///
/// The humidity ratios are non-negative and finite (issue #16): a humidity ratio is a mass
/// ratio, refused as negative wherever else the engine takes one
/// (`vaporPressureFromHumidityRatio`), and a NaN used to reach the subtraction unchecked.
/// Zero is dry air and is not refused.
pub fn evaporation_from_air_mass_balance(
    dry_air_mass_flow_kg_s: f64,
    inlet_humidity_ratio: f64,
    outlet_humidity_ratio: f64,
) -> Result<f64, DomainError> {
    assert_positive(dry_air_mass_flow_kg_s, "dryAirMassFlowKgS")?;
    assert_non_negative(inlet_humidity_ratio, "inletHumidityRatio")?;
    assert_non_negative(outlet_humidity_ratio, "outletHumidityRatio")?;
    Ok(floor_at_zero(
        dry_air_mass_flow_kg_s * (outlet_humidity_ratio - inlet_humidity_ratio),
    ))
}

/// Port of `driftLossKgS`: the circulating flow times the drift rating in parts per million.
///
/// The reference asserts the flow and refuses a negative ppm; a zero ppm is a legitimate
/// (drift-free) design and is not refused. The ppm is checked finite first (issue #16):
/// `drift_ppm < 0.0` is false for a NaN, which is how a NaN used to propagate out of the
/// engine as if it were a rating.
pub fn drift_loss_kg_s(
    circulating_water_mass_flow_kg_s: f64,
    drift_ppm: f64,
) -> Result<f64, DomainError> {
    assert_positive(
        circulating_water_mass_flow_kg_s,
        "circulatingWaterMassFlowKgS",
    )?;
    assert_finite_number(drift_ppm, "driftPpm")?;
    if drift_ppm < 0.0 {
        return Err(DomainError::new("Drift ppm cannot be negative."));
    }
    Ok(circulating_water_mass_flow_kg_s * drift_ppm * 1e-6)
}

/// The inputs of [`cooling_tower_water_balance`], carrying the reference's default arguments:
/// `driftKgS` defaults to 0 and `cyclesOfConcentration` to 4, applied where the reference
/// applies them (before the function body, so before its guard).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterBalanceInput {
    pub evaporation_kg_s: f64,
    pub drift_kg_s: f64,
    pub cycles_of_concentration: f64,
}

impl WaterBalanceInput {
    /// `coolingTowerWaterBalance({ evaporationKgS })` — both default arguments apply.
    pub fn new(evaporation_kg_s: f64) -> Self {
        Self {
            evaporation_kg_s,
            drift_kg_s: 0.0,
            cycles_of_concentration: 4.0,
        }
    }

    /// The reference's `driftKgS` argument, whose default is 0.
    pub fn with_drift_kg_s(mut self, drift_kg_s: f64) -> Self {
        self.drift_kg_s = drift_kg_s;
        self
    }

    /// The reference's `cyclesOfConcentration` argument, whose default is 4.
    pub fn with_cycles_of_concentration(mut self, cycles_of_concentration: f64) -> Self {
        self.cycles_of_concentration = cycles_of_concentration;
        self
    }
}

/// The object `coolingTowerWaterBalance` returns, field for field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterBalance {
    pub evaporation_kg_s: f64,
    pub drift_kg_s: f64,
    pub blowdown_kg_s: f64,
    pub makeup_kg_s: f64,
    pub cycles_of_concentration: f64,
}

/// Port of `coolingTowerWaterBalance`: the cycles guard comes first, before any arithmetic,
/// then the two losses are asserted non-negative and finite, then blowdown at
/// `evaporation / (cycles − 1) − drift` floored at zero, then makeup as the sum of the three
/// losses.
///
/// The two losses are refused when negative or non-finite (issue #16) — the reference used to
/// return a negative makeup for a negative evaporation, and a negative `driftKgS` was
/// accepted while `driftLossKgS` refused a negative ppm, two paths into the same quantity
/// disagreeing. Zero stays legitimate on both: the no-load balance, and `driftKgS`'s
/// documented default.
pub fn cooling_tower_water_balance(input: &WaterBalanceInput) -> Result<WaterBalance, DomainError> {
    if input.cycles_of_concentration <= 1.0 {
        return Err(DomainError::new("Cycles of concentration must exceed 1."));
    }
    assert_non_negative(input.evaporation_kg_s, "evaporationKgS")?;
    assert_non_negative(input.drift_kg_s, "driftKgS")?;
    let blowdown_kg_s = floor_at_zero(
        input.evaporation_kg_s / (input.cycles_of_concentration - 1.0) - input.drift_kg_s,
    );
    let makeup_kg_s = input.evaporation_kg_s + input.drift_kg_s + blowdown_kg_s;
    Ok(WaterBalance {
        evaporation_kg_s: input.evaporation_kg_s,
        drift_kg_s: input.drift_kg_s,
        blowdown_kg_s,
        makeup_kg_s,
        cycles_of_concentration: input.cycles_of_concentration,
    })
}
