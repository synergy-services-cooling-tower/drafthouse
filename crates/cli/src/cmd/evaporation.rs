use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;

pub(crate) fn evaporation(cli: &Cli) -> Result<String, Failure> {
    let evaporation_kg_s = drafthouse_engine::evaporation_from_air_mass_balance(
        cli.required_number("--dry-air-mass-flow")?,
        cli.required_number("--inlet-humidity-ratio")?,
        cli.required_number("--outlet-humidity-ratio")?,
    )?;
    Ok(object(&[("evaporationKgS", number(evaporation_kg_s))]))
}
