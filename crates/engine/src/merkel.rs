//! Counterflow Merkel demand — port of `src/core/merkel.js` (the slice excludes
//! `estimateOutletAirState` and `generateDemandCurve`: the first is ported with the selector,
//! `selection.rs`, and the second is still unported — nothing in the ported slices needs it).
//!
//! `KaV/L = ∫ cp dTw / (hs(Tw) - ha(Tw))`, with the entering-air enthalpy taken either as
//! the true moist-air enthalpy of the measured dry bulb/wet bulb pair (`bulk`, the default)
//! or as saturated air at the entering wet bulb (`cti-saturated-wetbulb`). The two
//! conventions differ by roughly 1 % at a typical design point; the difference is real and
//! is recorded in the vectors.

use crate::numeric::{
    assert_positive, find_root_by_scan, integrate_chebyshev4, integrate_simpson, DomainError,
    RootPreference, RootScanOptions,
};
use crate::psychrometrics::{
    psychrometric_state, saturated_air_enthalpy_kj_kg_dry_air, PsychrometricOptions,
    PsychrometricState, PsychrometricStateInput,
};
use crate::water::water_specific_heat_kj_kg_k;
use std::cmp::Ordering;

/// Which quadrature to use, mirroring the JavaScript `integration` option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Integration {
    Simpson,
    Chebyshev4,
}

/// Entering-air enthalpy convention, mirroring the JavaScript
/// `inletEnthalpyConvention` option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InletEnthalpyConvention {
    /// True moist-air enthalpy of the entering dry bulb/wet bulb pair (default).
    Bulk,
    /// Enthalpy of saturated air at the entering wet bulb; the CTI demand-curve convention.
    CtiSaturatedWetBulb,
}

/// Port of `validateThermalTemperatures`.
pub fn validate_thermal_temperatures(
    hot_water_c: f64,
    cold_water_c: f64,
    wet_bulb_c: f64,
) -> Result<(), DomainError> {
    // `!(a > b)`, written so it stays true for NaN (the JavaScript comparison does too).
    if hot_water_c.partial_cmp(&cold_water_c) != Some(Ordering::Greater) {
        return Err(DomainError::new(
            "Hot-water temperature must exceed cold-water temperature.",
        ));
    }
    if cold_water_c.partial_cmp(&wet_bulb_c) != Some(Ordering::Greater) {
        return Err(DomainError::new(
            "Cold-water temperature must exceed entering-air wet bulb.",
        ));
    }
    Ok(())
}

/// Port of `inletAirEnthalpyKJkgDryAir`.
pub fn inlet_air_enthalpy_kj_kg_dry_air(
    inlet_air_state: &PsychrometricState,
    pressure_pa: f64,
    convention: InletEnthalpyConvention,
) -> Result<f64, DomainError> {
    match convention {
        InletEnthalpyConvention::CtiSaturatedWetBulb => saturated_air_enthalpy_kj_kg_dry_air(
            inlet_air_state.wet_bulb_c,
            pressure_pa,
            PsychrometricOptions::default(),
        ),
        InletEnthalpyConvention::Bulk => Ok(inlet_air_state.enthalpy_kj_kg_dry_air),
    }
}

/// Port of `airEnthalpyOperatingLineKJkg`.
pub fn air_enthalpy_operating_line_kj_kg(
    water_temperature_c: f64,
    cold_water_c: f64,
    inlet_air_enthalpy_kj_kg_dry_air: f64,
    water_to_dry_air_ratio: f64,
    cp_water_kj_kg_k: f64,
) -> f64 {
    inlet_air_enthalpy_kj_kg_dry_air
        + water_to_dry_air_ratio * cp_water_kj_kg_k * (water_temperature_c - cold_water_c)
}

/// Inputs for [`merkel_demand`] — the port of the JavaScript `merkelDemand({...})` call.
#[derive(Clone, Copy, Debug)]
pub struct MerkelInput {
    pub hot_water_c: f64,
    pub cold_water_c: f64,
    pub wet_bulb_c: f64,
    pub dry_bulb_c: f64,
    pub water_to_dry_air_ratio: f64,
    /// Default 101 325 Pa.
    pub pressure_pa: f64,
    /// Default 0 g/kg.
    pub salinity_g_kg: f64,
    /// Default [`Integration::Simpson`].
    pub integration: Integration,
    /// Simpson segment count. Default 240.
    pub segments: usize,
    /// Default 1e-7.
    pub minimum_enthalpy_potential: f64,
    /// Default [`InletEnthalpyConvention::Bulk`].
    pub inlet_enthalpy_convention: InletEnthalpyConvention,
}

impl MerkelInput {
    /// The five physically required inputs; everything else takes the reference default.
    pub fn new(
        hot_water_c: f64,
        cold_water_c: f64,
        wet_bulb_c: f64,
        dry_bulb_c: f64,
        water_to_dry_air_ratio: f64,
    ) -> Self {
        Self {
            hot_water_c,
            cold_water_c,
            wet_bulb_c,
            dry_bulb_c,
            water_to_dry_air_ratio,
            pressure_pa: 101_325.0,
            salinity_g_kg: 0.0,
            integration: Integration::Simpson,
            segments: 240,
            minimum_enthalpy_potential: 1e-7,
            inlet_enthalpy_convention: InletEnthalpyConvention::Bulk,
        }
    }
}

/// Port of the `merkelDemand` result object.
#[derive(Clone, Copy, Debug)]
pub struct MerkelResult {
    pub merkel_number: f64,
    pub inlet_air_state: PsychrometricState,
    pub inlet_air_enthalpy_kj_kg_dry_air: f64,
    pub inlet_enthalpy_convention: InletEnthalpyConvention,
    pub cp_water_kj_kg_k: f64,
    pub integration: Integration,
}

/// Port of `merkelDemand`.
pub fn merkel_demand(input: &MerkelInput) -> Result<MerkelResult, DomainError> {
    validate_thermal_temperatures(input.hot_water_c, input.cold_water_c, input.wet_bulb_c)?;
    assert_positive(input.water_to_dry_air_ratio, "waterToDryAirRatio")?;
    let options = PsychrometricOptions::default();
    let inlet = psychrometric_state(
        PsychrometricStateInput::from_wet_bulb(input.dry_bulb_c, input.wet_bulb_c)
            .with_pressure(input.pressure_pa),
    )?;
    let cp_water = water_specific_heat_kj_kg_k(
        (input.hot_water_c + input.cold_water_c) / 2.0,
        input.salinity_g_kg,
    );
    let inlet_enthalpy = inlet_air_enthalpy_kj_kg_dry_air(
        &inlet,
        input.pressure_pa,
        input.inlet_enthalpy_convention,
    )?;

    let integrand = |water_temperature_c: f64| -> Result<f64, DomainError> {
        let saturated_enthalpy =
            saturated_air_enthalpy_kj_kg_dry_air(water_temperature_c, input.pressure_pa, options)?;
        let air_enthalpy = air_enthalpy_operating_line_kj_kg(
            water_temperature_c,
            input.cold_water_c,
            inlet_enthalpy,
            input.water_to_dry_air_ratio,
            cp_water,
        );
        let potential = saturated_enthalpy - air_enthalpy;
        if potential <= input.minimum_enthalpy_potential {
            return Err(DomainError::new(
                "Thermal demand reached an enthalpy pinch; the specified condition is infeasible for this L/G.",
            ));
        }
        Ok(cp_water / potential)
    };

    let merkel_number = match input.integration {
        Integration::Chebyshev4 => {
            integrate_chebyshev4(integrand, input.cold_water_c, input.hot_water_c)?
        }
        Integration::Simpson => integrate_simpson(
            integrand,
            input.cold_water_c,
            input.hot_water_c,
            input.segments,
        )?,
    };

    Ok(MerkelResult {
        merkel_number,
        inlet_air_state: inlet,
        inlet_air_enthalpy_kj_kg_dry_air: inlet_enthalpy,
        inlet_enthalpy_convention: input.inlet_enthalpy_convention,
        cp_water_kj_kg_k: cp_water,
        integration: input.integration,
    })
}

/// Port of `safeMerkelDemand`: a pinch returns positive infinity (as the reference does)
/// instead of an error; every other refusal still propagates.
pub fn safe_merkel_demand(input: &MerkelInput) -> Result<f64, DomainError> {
    match merkel_demand(input) {
        Ok(result) => Ok(result.merkel_number),
        Err(error) if error.message().contains("pinch") => Ok(f64::INFINITY),
        Err(error) => Err(error),
    }
}

/// Port of `towerCharacteristic`: the whole-tower characteristic `coefficient · L/G^exponent`
/// the capability projection compares the design demand against. Both the ratio and the
/// coefficient are asserted positive, as in the reference.
pub fn tower_characteristic(
    water_to_dry_air_ratio: f64,
    coefficient: f64,
    exponent: f64,
) -> Result<f64, DomainError> {
    assert_positive(water_to_dry_air_ratio, "waterToDryAirRatio")?;
    assert_positive(coefficient, "coefficient")?;
    Ok(coefficient * water_to_dry_air_ratio.powf(exponent))
}

/// Port of `characteristicCoefficient`: the coefficient that makes the characteristic pass
/// through a measured demand at a measured L/G — the test-condition anchor of the capability
/// projection.
pub fn characteristic_coefficient(
    merkel_number: f64,
    water_to_dry_air_ratio: f64,
    exponent: f64,
) -> Result<f64, DomainError> {
    assert_positive(merkel_number, "merkelNumber")?;
    assert_positive(water_to_dry_air_ratio, "waterToDryAirRatio")?;
    Ok(merkel_number / water_to_dry_air_ratio.powf(exponent))
}

/// Inputs for [`solve_cold_water_temperature`] — the port of the JavaScript
/// `solveColdWaterTemperature({...})` call (which takes no segment count).
#[derive(Clone, Copy, Debug)]
pub struct ColdWaterTemperatureInput {
    pub hot_water_c: f64,
    pub wet_bulb_c: f64,
    pub dry_bulb_c: f64,
    pub water_to_dry_air_ratio: f64,
    pub available_merkel_number: f64,
    /// Default 101 325 Pa.
    pub pressure_pa: f64,
    /// Default 0 g/kg.
    pub salinity_g_kg: f64,
    /// Default [`Integration::Simpson`].
    pub integration: Integration,
    /// Default [`InletEnthalpyConvention::Bulk`].
    pub inlet_enthalpy_convention: InletEnthalpyConvention,
}

impl ColdWaterTemperatureInput {
    /// The five physically required inputs; everything else takes the reference default.
    pub fn new(
        hot_water_c: f64,
        wet_bulb_c: f64,
        dry_bulb_c: f64,
        water_to_dry_air_ratio: f64,
        available_merkel_number: f64,
    ) -> Self {
        Self {
            hot_water_c,
            wet_bulb_c,
            dry_bulb_c,
            water_to_dry_air_ratio,
            available_merkel_number,
            pressure_pa: 101_325.0,
            salinity_g_kg: 0.0,
            integration: Integration::Simpson,
            inlet_enthalpy_convention: InletEnthalpyConvention::Bulk,
        }
    }

    fn demand_input(&self, cold_water_c: f64) -> MerkelInput {
        MerkelInput {
            cold_water_c,
            ..MerkelInput::new(
                self.hot_water_c,
                cold_water_c,
                self.wet_bulb_c,
                self.dry_bulb_c,
                self.water_to_dry_air_ratio,
            )
        }
        .with_pressure(self.pressure_pa)
        .with_salinity(self.salinity_g_kg)
        .with_integration(self.integration)
        .with_inlet_enthalpy_convention(self.inlet_enthalpy_convention)
    }
}

impl MerkelInput {
    fn with_pressure(mut self, pressure_pa: f64) -> Self {
        self.pressure_pa = pressure_pa;
        self
    }

    fn with_salinity(mut self, salinity_g_kg: f64) -> Self {
        self.salinity_g_kg = salinity_g_kg;
        self
    }

    fn with_integration(mut self, integration: Integration) -> Self {
        self.integration = integration;
        self
    }

    fn with_inlet_enthalpy_convention(
        mut self,
        inlet_enthalpy_convention: InletEnthalpyConvention,
    ) -> Self {
        self.inlet_enthalpy_convention = inlet_enthalpy_convention;
        self
    }
}

/// Port of the `solveColdWaterTemperature` result object.
#[derive(Clone, Copy, Debug)]
pub struct ColdWaterTemperatureResult {
    pub cold_water_c: f64,
    pub approach_c: f64,
    pub range_c: f64,
    pub required_merkel_number: f64,
    pub inlet_air_state: PsychrometricState,
    pub cp_water_kj_kg_k: f64,
}

/// Port of `solveColdWaterTemperature`.
pub fn solve_cold_water_temperature(
    input: &ColdWaterTemperatureInput,
) -> Result<ColdWaterTemperatureResult, DomainError> {
    assert_positive(input.available_merkel_number, "availableMerkelNumber")?;
    let lower = input.wet_bulb_c + 0.01;
    let upper = input.hot_water_c - 0.01;

    let residual = |cold_water_c: f64| -> Result<f64, DomainError> {
        let demand = safe_merkel_demand(&input.demand_input(cold_water_c))?;
        if !demand.is_finite() {
            return Ok(1e12);
        }
        Ok(demand - input.available_merkel_number)
    };

    let cold_water_c = find_root_by_scan(
        residual,
        lower,
        upper,
        RootScanOptions {
            samples: 360,
            tolerance: 1e-7,
            prefer: RootPreference::Last,
            ..RootScanOptions::default()
        },
    )?;

    let demand = merkel_demand(&input.demand_input(cold_water_c))?;
    Ok(ColdWaterTemperatureResult {
        cold_water_c,
        approach_c: cold_water_c - input.wet_bulb_c,
        range_c: input.hot_water_c - cold_water_c,
        required_merkel_number: demand.merkel_number,
        inlet_air_state: demand.inlet_air_state,
        cp_water_kj_kg_k: demand.cp_water_kj_kg_k,
    })
}
