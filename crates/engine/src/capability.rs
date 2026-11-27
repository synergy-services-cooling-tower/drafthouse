use crate::merkel::{
    characteristic_coefficient, merkel_demand, safe_merkel_demand, tower_characteristic,
    InletEnthalpyConvention, Integration, MerkelInput,
};
use crate::numeric::{
    count_as_f64, create_seeded_random, find_root_by_scan, gaussian_random, js_max, js_min,
    percentile, range, DomainError, RootPreference, RootScanOptions, SeededRandom,
};
use crate::psychrometrics::{psychrometric_state, PsychrometricState, PsychrometricStateInput};

/// One condition object — the design or the test condition of the reference call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapabilityCondition {
    pub water_mass_flow_kg_s: f64,
    pub dry_air_mass_flow_kg_s: f64,
    pub hot_water_c: f64,
    pub cold_water_c: f64,
    pub wet_bulb_c: f64,
    pub dry_bulb_c: f64,
    pub pressure_pa: Option<f64>,
}

impl CapabilityCondition {
    /// A condition at the reference's default pressure.
    #[must_use]
    pub fn new(
        water_mass_flow_kg_s: f64,
        dry_air_mass_flow_kg_s: f64,
        hot_water_c: f64,
        cold_water_c: f64,
        wet_bulb_c: f64,
        dry_bulb_c: f64,
    ) -> Self {
        Self {
            water_mass_flow_kg_s,
            dry_air_mass_flow_kg_s,
            hot_water_c,
            cold_water_c,
            wet_bulb_c,
            dry_bulb_c,
            pressure_pa: None,
        }
    }

    #[must_use]
    pub fn with_pressure(mut self, pressure_pa: f64) -> Self {
        self.pressure_pa = Some(pressure_pa);
        self
    }

    fn pressure(&self) -> f64 {
        self.pressure_pa.unwrap_or(101_325.0)
    }
}

fn validate_condition(condition: &CapabilityCondition, label: &str) -> Result<(), DomainError> {
    let required = [
        ("waterMassFlowKgS", condition.water_mass_flow_kg_s),
        ("dryAirMassFlowKgS", condition.dry_air_mass_flow_kg_s),
        ("hotWaterC", condition.hot_water_c),
        ("coldWaterC", condition.cold_water_c),
        ("wetBulbC", condition.wet_bulb_c),
        ("dryBulbC", condition.dry_bulb_c),
    ];
    for (field, value) in required {
        if !value.is_finite() {
            return Err(DomainError::new(format!("{label}.{field} is required.")));
        }
    }
    if !(condition.hot_water_c > condition.cold_water_c
        && condition.cold_water_c > condition.wet_bulb_c)
    {
        return Err(DomainError::new(format!(
            "{label} temperatures must satisfy hot water > cold water > wet bulb."
        )));
    }
    Ok(())
}

/// The inputs of [`evaluate_characteristic_capability`], carrying the reference's default arguments: exponent −0.6, salinity 0 g/kg, `chebyshev4` integration, 80 curve points and the `bulk` enthalpy convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacteristicCapabilityInput {
    pub design: CapabilityCondition,
    pub test: CapabilityCondition,
    pub characteristic_exponent: f64,
    pub salinity_g_kg: f64,
    pub integration: Integration,
    pub curve_points: usize,
    pub inlet_enthalpy_convention: InletEnthalpyConvention,
}

impl CharacteristicCapabilityInput {
    /// `evaluateCharacteristicCapability({ design, test })` — the two required conditions and every default argument.
    #[must_use]
    pub fn new(design: CapabilityCondition, test: CapabilityCondition) -> Self {
        Self {
            design,
            test,
            characteristic_exponent: -0.6,
            salinity_g_kg: 0.0,
            integration: Integration::Chebyshev4,
            curve_points: 80,
            inlet_enthalpy_convention: InletEnthalpyConvention::Bulk,
        }
    }

    /// The reference's `characteristicExponent` argument.
    #[must_use]
    pub fn with_characteristic_exponent(mut self, characteristic_exponent: f64) -> Self {
        self.characteristic_exponent = characteristic_exponent;
        self
    }

    /// The reference's `salinityGKg` argument.
    #[must_use]
    pub fn with_salinity_g_kg(mut self, salinity_g_kg: f64) -> Self {
        self.salinity_g_kg = salinity_g_kg;
        self
    }

    /// The reference's `integration` argument.
    #[must_use]
    pub fn with_integration(mut self, integration: Integration) -> Self {
        self.integration = integration;
        self
    }

    /// The reference's `curvePoints` argument.
    #[must_use]
    pub fn with_curve_points(mut self, curve_points: usize) -> Self {
        self.curve_points = curve_points;
        self
    }

    /// The reference's `inletEnthalpyConvention` argument.
    #[must_use]
    pub fn with_inlet_enthalpy_convention(
        mut self,
        inlet_enthalpy_convention: InletEnthalpyConvention,
    ) -> Self {
        self.inlet_enthalpy_convention = inlet_enthalpy_convention;
        self
    }

    fn demand_input(&self, condition: &CapabilityCondition, ratio: f64) -> MerkelInput {
        MerkelInput {
            pressure_pa: condition.pressure(),
            salinity_g_kg: self.salinity_g_kg,
            integration: self.integration,
            inlet_enthalpy_convention: self.inlet_enthalpy_convention,
            ..MerkelInput::new(
                condition.hot_water_c,
                condition.cold_water_c,
                condition.wet_bulb_c,
                condition.dry_bulb_c,
                ratio,
            )
        }
    }
}

/// One point of the reported design-demand / test-characteristic curve pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapabilityCurvePoint {
    pub water_to_dry_air_ratio: f64,
    pub design_demand_merkel: f64,
    pub test_characteristic_merkel: f64,
}

/// The object `evaluateCharacteristicCapability` returns, field for field (minus the two the harness reads as text: `method` and `disclaimer` are in `rust/src/cli.rs`, which renders them, so this struct carries the physics).
#[derive(Clone, Debug)]
pub struct CharacteristicCapability {
    pub capability_pct: f64,
    pub design_water_to_dry_air_ratio: f64,
    pub test_water_to_dry_air_ratio: f64,
    pub capability_water_to_dry_air_ratio: f64,
    pub test_merkel_number: f64,
    pub test_characteristic_coefficient: f64,
    pub characteristic_exponent: f64,
    pub inlet_enthalpy_convention: InletEnthalpyConvention,
    pub design_air_state: PsychrometricState,
    pub test_air_state: PsychrometricState,
    pub curves: Vec<CapabilityCurvePoint>,
}

/// # Errors `DomainError` when the input is out of domain. Port of `evaluateCharacteristicCapability`: the test demand sets the characteristic coefficient, and the capability is the design-condition L/G at which the design demand meets that characteristic — the last such crossing in the scan interval.
pub fn evaluate_characteristic_capability(
    input: &CharacteristicCapabilityInput,
) -> Result<CharacteristicCapability, DomainError> {
    validate_condition(&input.design, "design")?;
    validate_condition(&input.test, "test")?;
    if input.characteristic_exponent >= 0.0 {
        return Err(DomainError::new(
            "A whole-tower characteristic exponent must be negative.",
        ));
    }

    let design_water_to_air =
        input.design.water_mass_flow_kg_s / input.design.dry_air_mass_flow_kg_s;
    let test_water_to_air = input.test.water_mass_flow_kg_s / input.test.dry_air_mass_flow_kg_s;
    let test_demand = merkel_demand(&input.demand_input(&input.test, test_water_to_air))?;
    let test_coefficient = characteristic_coefficient(
        test_demand.merkel_number,
        test_water_to_air,
        input.characteristic_exponent,
    )?;

    let ratios = CapabilityRatios {
        design: design_water_to_air,
        test: test_water_to_air,
        test_coefficient,
    };
    let capability_water_to_air = capability_root(input, &ratios)?;
    let capability_pct = 100.0 * capability_water_to_air / design_water_to_air;
    let curves = capability_curves(input, &ratios, capability_water_to_air)?;

    Ok(CharacteristicCapability {
        capability_pct,
        design_water_to_dry_air_ratio: design_water_to_air,
        test_water_to_dry_air_ratio: test_water_to_air,
        capability_water_to_dry_air_ratio: capability_water_to_air,
        test_merkel_number: test_demand.merkel_number,
        test_characteristic_coefficient: test_coefficient,
        characteristic_exponent: input.characteristic_exponent,
        inlet_enthalpy_convention: input.inlet_enthalpy_convention,
        design_air_state: psychrometric_state(
            PsychrometricStateInput::from_wet_bulb(
                input.design.dry_bulb_c,
                input.design.wet_bulb_c,
            )
            .with_pressure(input.design.pressure()),
        )?,
        test_air_state: test_demand.inlet_air_state,
        curves,
    })
}

struct CapabilityRatios {
    design: f64,
    test: f64,
    test_coefficient: f64,
}

fn design_demand_at(input: &CharacteristicCapabilityInput, ratio: f64) -> Result<f64, DomainError> {
    safe_merkel_demand(&input.demand_input(&input.design, ratio))
}

fn capability_root(
    input: &CharacteristicCapabilityInput,
    ratios: &CapabilityRatios,
) -> Result<f64, DomainError> {
    let residual = |ratio: f64| -> Result<f64, DomainError> {
        let demand = design_demand_at(input, ratio)?;
        if !demand.is_finite() {
            return Ok(-1e12);
        }
        Ok(tower_characteristic(
            ratio,
            ratios.test_coefficient,
            input.characteristic_exponent,
        )? - demand)
    };
    let min_ratio = js_max(0.05, js_min(ratios.design, ratios.test) * 0.12);
    let max_ratio = js_max(ratios.design, ratios.test) * 8.0;
    find_root_by_scan(
        residual,
        min_ratio,
        max_ratio,
        RootScanOptions {
            samples: 720,
            tolerance: 1e-8,
            prefer: RootPreference::Last,
            ..RootScanOptions::default()
        },
    )
}

fn capability_curves(
    input: &CharacteristicCapabilityInput,
    ratios: &CapabilityRatios,
    capability_water_to_air: f64,
) -> Result<Vec<CapabilityCurvePoint>, DomainError> {
    let x_values = range(
        js_max(
            0.05,
            js_min(js_min(ratios.design, ratios.test), capability_water_to_air) * 0.55,
        ),
        js_max(js_max(ratios.design, ratios.test), capability_water_to_air) * 1.55,
        input.curve_points,
    );
    let mut curves = Vec::new();
    for ratio in x_values {
        let design_demand_merkel = design_demand_at(input, ratio)?;
        if !design_demand_merkel.is_finite() {
            continue;
        }
        curves.push(CapabilityCurvePoint {
            water_to_dry_air_ratio: ratio,
            design_demand_merkel,
            test_characteristic_merkel: tower_characteristic(
                ratio,
                ratios.test_coefficient,
                input.characteristic_exponent,
            )?,
        });
    }
    Ok(curves)
}

/// The condition fields a perturbation can carry, in the order the reference's condition objects declare them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionField {
    WaterMassFlowKgS,
    DryAirMassFlowKgS,
    HotWaterC,
    ColdWaterC,
    WetBulbC,
    DryBulbC,
    PressurePa,
}

/// Port of the reference's `standardUncertainty` object: one sigma per named field, in the caller's order (that order is what decides which field draws which Gaussian, so it matters).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CapabilityUncertainty {
    pub design: Vec<(ConditionField, f64)>,
    pub test: Vec<(ConditionField, f64)>,
    /// The top-level `characteristicExponent` sigma, when the reference object carries one.
    pub characteristic_exponent: Option<f64>,
    /// The top-level `salinityGKg` sigma, when the reference object carries one.
    pub salinity_g_kg: Option<f64>,
}

/// The inputs of [`monte_carlo_characteristic_capability`], carrying the reference's defaults (1000 samples, seed 20260813).
#[derive(Clone, Debug, PartialEq)]
pub struct MonteCarloInput {
    pub base_input: CharacteristicCapabilityInput,
    pub uncertainty: CapabilityUncertainty,
    pub samples: usize,
    pub seed: f64,
}

impl MonteCarloInput {
    /// `monteCarloCharacteristicCapability({ baseInput, standardUncertainty })`.
    #[must_use]
    pub fn new(
        base_input: CharacteristicCapabilityInput,
        uncertainty: CapabilityUncertainty,
    ) -> Self {
        Self {
            base_input,
            uncertainty,
            samples: 1000,
            seed: 20_260_813.0,
        }
    }

    /// The reference's `samples` argument.
    #[must_use]
    pub fn with_samples(mut self, samples: usize) -> Self {
        self.samples = samples;
        self
    }

    /// The reference's `seed` argument.
    #[must_use]
    pub fn with_seed(mut self, seed: f64) -> Self {
        self.seed = seed;
        self
    }
}

/// The object `monteCarloCharacteristicCapability` returns, field for field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonteCarloResult {
    pub samples_requested: usize,
    pub samples_accepted: usize,
    pub rejected_samples: usize,
    pub mean_capability_pct: f64,
    pub standard_deviation_pct_points: f64,
    pub p2_5: f64,
    pub p50: f64,
    pub p97_5: f64,
    pub expanded_uncertainty_approx_pct_points: f64,
    pub seed: f64,
}

fn perturb_value(target: &mut f64, sigma: f64, random: &mut SeededRandom) {
    if target.is_finite() {
        *target += sigma * gaussian_random(&mut || random.next_value());
    }
}

fn perturb_condition(
    condition: &mut CapabilityCondition,
    sigmas: &[(ConditionField, f64)],
    random: &mut SeededRandom,
) {
    for (field, sigma) in sigmas {
        if !sigma.is_finite() {
            continue;
        }
        match field {
            ConditionField::WaterMassFlowKgS => {
                perturb_value(&mut condition.water_mass_flow_kg_s, *sigma, random);
            }
            ConditionField::DryAirMassFlowKgS => {
                perturb_value(&mut condition.dry_air_mass_flow_kg_s, *sigma, random);
            }
            ConditionField::HotWaterC => {
                perturb_value(&mut condition.hot_water_c, *sigma, random);
            }
            ConditionField::ColdWaterC => {
                perturb_value(&mut condition.cold_water_c, *sigma, random);
            }
            ConditionField::WetBulbC => {
                perturb_value(&mut condition.wet_bulb_c, *sigma, random);
            }
            ConditionField::DryBulbC => {
                perturb_value(&mut condition.dry_bulb_c, *sigma, random);
            }
            ConditionField::PressurePa => {
                if let Some(pressure_pa) = condition.pressure_pa.as_mut() {
                    perturb_value(pressure_pa, *sigma, random);
                }
            }
        }
    }
}

/// # Errors `DomainError` when the input is out of domain. Port of `monteCarloCharacteristicCapability`: perturb the base input `samples` times with the seeded Gaussian stream, evaluate each draw, and report the accepted capability distribution.
pub fn monte_carlo_characteristic_capability(
    input: &MonteCarloInput,
) -> Result<MonteCarloResult, DomainError> {
    let mut random = create_seeded_random(input.seed);
    let mut capabilities: Vec<f64> = Vec::new();
    let mut rejected_samples = 0_usize;
    for _ in 0..input.samples {
        let mut sampled = input.base_input;
        perturb_condition(&mut sampled.design, &input.uncertainty.design, &mut random);
        perturb_condition(&mut sampled.test, &input.uncertainty.test, &mut random);
        if let Some(sigma) = input.uncertainty.characteristic_exponent {
            perturb_value(&mut sampled.characteristic_exponent, sigma, &mut random);
        }
        if let Some(sigma) = input.uncertainty.salinity_g_kg {
            perturb_value(&mut sampled.salinity_g_kg, sigma, &mut random);
        }
        match evaluate_characteristic_capability(&sampled) {
            Ok(result) if result.capability_pct.is_finite() => {
                capabilities.push(result.capability_pct);
            }
            Ok(_) | Err(_) => rejected_samples += 1,
        }
    }
    if count_as_f64(capabilities.len()) < js_max(20.0, count_as_f64(input.samples) * 0.5) {
        return Err(DomainError::new(
            "Too many Monte Carlo samples were invalid; review uncertainty inputs.",
        ));
    }
    let mean = capabilities.iter().sum::<f64>() / count_as_f64(capabilities.len());
    let variance = capabilities
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / count_as_f64((capabilities.len() - 1).max(1));
    Ok(MonteCarloResult {
        samples_requested: input.samples,
        samples_accepted: capabilities.len(),
        rejected_samples,
        mean_capability_pct: mean,
        standard_deviation_pct_points: variance.sqrt(),
        p2_5: percentile(&capabilities, 0.025),
        p50: percentile(&capabilities, 0.5),
        p97_5: percentile(&capabilities, 0.975),
        expanded_uncertainty_approx_pct_points: 2.0 * variance.sqrt(),
        seed: input.seed,
    })
}
