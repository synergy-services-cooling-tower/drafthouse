use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;
use drafthouse_engine::ColdWaterTemperatureInput;

pub(crate) fn inverse(cli: &Cli) -> Result<String, Failure> {
    let input = ColdWaterTemperatureInput {
        hot_water_c: cli.required_number("--hot")?,
        wet_bulb_c: cli.required_number("--wb")?,
        dry_bulb_c: cli.required_number("--db")?,
        water_to_dry_air_ratio: cli.required_number("--lg")?,
        available_merkel_number: cli.required_number("--kavl")?,
        pressure_pa: cli.number_or("--p", 101_325.0)?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        integration: cli.integration()?,
        inlet_enthalpy_convention: cli.convention()?,
    };
    let result = drafthouse_engine::solve_cold_water_temperature(&input)?;
    Ok(object(&[
        ("coldWaterC", number(result.cold_water_c)),
        ("approachC", number(result.approach_c)),
        ("rangeC", number(result.range_c)),
        (
            "requiredMerkelNumber",
            number(result.required_merkel_number),
        ),
        ("cpWaterKJkgK", number(result.cp_water_kj_kg_k)),
    ]))
}
