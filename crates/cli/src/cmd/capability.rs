use crate::args::{parse_number, Cli, Spec};
use crate::json::{json_array, number, object};
use crate::objects::{air_state_object, convention_name};
use crate::{json_string, Failure};
use drafthouse_engine::{
    CapabilityCondition, CharacteristicCapabilityInput, ConditionField, Integration,
};

pub(crate) const CONDITION_FIELDS: [&str; 7] = [
    "waterMassFlowKgS",
    "dryAirMassFlowKgS",
    "hotWaterC",
    "coldWaterC",
    "wetBulbC",
    "dryBulbC",
    "pressurePa",
];

pub(crate) const METHOD_CAPABILITY: &str =
    "CTI-style characteristic-curve educational implementation";

pub(crate) const DISCLAIMER_CAPABILITY: &str = "This reproduces the public characteristic-curve concept, not the complete licensed ATC-105 correction, validity, instrumentation, or contractual procedure.";

pub(crate) fn capability_condition(
    cli: &Cli,
    option: &str,
) -> Result<CapabilityCondition, Failure> {
    let spec = Spec::parse(cli, option)?;
    spec.ensure_known(&CONDITION_FIELDS)?;
    Ok(CapabilityCondition {
        water_mass_flow_kg_s: spec.required("waterMassFlowKgS")?,
        dry_air_mass_flow_kg_s: spec.required("dryAirMassFlowKgS")?,
        hot_water_c: spec.required("hotWaterC")?,
        cold_water_c: spec.required("coldWaterC")?,
        wet_bulb_c: spec.required("wetBulbC")?,
        dry_bulb_c: spec.required("dryBulbC")?,
        pressure_pa: spec.number("pressurePa")?,
    })
}

pub(crate) fn condition_field(key: &str, option: &str) -> Result<ConditionField, Failure> {
    match key {
        "waterMassFlowKgS" => Ok(ConditionField::WaterMassFlowKgS),
        "dryAirMassFlowKgS" => Ok(ConditionField::DryAirMassFlowKgS),
        "hotWaterC" => Ok(ConditionField::HotWaterC),
        "coldWaterC" => Ok(ConditionField::ColdWaterC),
        "wetBulbC" => Ok(ConditionField::WetBulbC),
        "dryBulbC" => Ok(ConditionField::DryBulbC),
        "pressurePa" => Ok(ConditionField::PressurePa),
        other => Err(Failure::Usage(format!(
            "{option} does not know the field {other:?}"
        ))),
    }
}

pub(crate) fn condition_sigmas(
    cli: &Cli,
    option: &str,
) -> Result<Vec<(ConditionField, f64)>, Failure> {
    let spec = Spec::parse(cli, option)?;
    let mut sigmas = Vec::new();
    for (key, value) in &spec.pairs {
        sigmas.push((condition_field(key, option)?, parse_number(value, option)?));
    }
    Ok(sigmas)
}

pub(crate) fn capability_input(cli: &Cli) -> Result<CharacteristicCapabilityInput, Failure> {
    Ok(CharacteristicCapabilityInput {
        design: capability_condition(cli, "--design")?,
        test: capability_condition(cli, "--test")?,
        characteristic_exponent: cli.number_or("--exponent", -0.6)?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        integration: cli.integration_default(Integration::Chebyshev4)?,
        curve_points: cli.integer("--curve-points", 80)?,
        inlet_enthalpy_convention: cli.convention()?,
    })
}

pub(crate) fn capability_object(result: &drafthouse_engine::CharacteristicCapability) -> String {
    let curves = result
        .curves
        .iter()
        .map(|point| {
            object(&[
                ("waterToDryAirRatio", number(point.water_to_dry_air_ratio)),
                ("designDemandMerkel", number(point.design_demand_merkel)),
                (
                    "testCharacteristicMerkel",
                    number(point.test_characteristic_merkel),
                ),
            ])
        })
        .collect::<Vec<String>>();
    object(&[
        ("method", json_string(METHOD_CAPABILITY)),
        ("capabilityPct", number(result.capability_pct)),
        (
            "designWaterToDryAirRatio",
            number(result.design_water_to_dry_air_ratio),
        ),
        (
            "testWaterToDryAirRatio",
            number(result.test_water_to_dry_air_ratio),
        ),
        (
            "capabilityWaterToDryAirRatio",
            number(result.capability_water_to_dry_air_ratio),
        ),
        ("testMerkelNumber", number(result.test_merkel_number)),
        (
            "testCharacteristicCoefficient",
            number(result.test_characteristic_coefficient),
        ),
        (
            "characteristicExponent",
            number(result.characteristic_exponent),
        ),
        (
            "inletEnthalpyConvention",
            json_string(convention_name(result.inlet_enthalpy_convention)),
        ),
        ("designAirState", air_state_object(&result.design_air_state)),
        ("testAirState", air_state_object(&result.test_air_state)),
        ("curves", json_array(&curves)),
        ("disclaimer", json_string(DISCLAIMER_CAPABILITY)),
    ])
}

pub(crate) fn capability(cli: &Cli) -> Result<String, Failure> {
    let input = capability_input(cli)?;
    let result = drafthouse_engine::evaluate_characteristic_capability(&input)?;
    Ok(capability_object(&result))
}
