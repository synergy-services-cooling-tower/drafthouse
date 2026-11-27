use crate::numeric::{assert_finite_number, assert_non_negative, assert_positive, DomainError};

fn floor_at_zero(value: f64) -> f64 {
    if value > 0.0 {
        value
    } else if value.is_nan() {
        f64::NAN
    } else {
        0.0
    }
}

/// Port of `evaporationFromAirMassBalance`: dry air is conserved through the tower, so the water lost to evaporation is the dry-air flow times the rise in humidity ratio.
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

/// Port of `coolingTowerWaterBalance`: the cycles guard comes first, before any arithmetic, then the two losses are asserted non-negative and finite, then blowdown at `evaporation / (cycles − 1) − drift` floored at zero, then makeup as the sum of the three
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
