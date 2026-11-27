use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;
use drafthouse_engine::WaterBalanceInput;

pub(crate) fn balance(cli: &Cli) -> Result<String, Failure> {
    let mut input = WaterBalanceInput::new(cli.required_number("--evaporation")?);
    if let Some(drift_kg_s) = cli.number("--drift")? {
        input = input.with_drift_kg_s(drift_kg_s);
    }
    if let Some(cycles_of_concentration) = cli.number("--cycles")? {
        input = input.with_cycles_of_concentration(cycles_of_concentration);
    }
    let result = drafthouse_engine::cooling_tower_water_balance(&input)?;
    Ok(object(&[
        ("evaporationKgS", number(result.evaporation_kg_s)),
        ("driftKgS", number(result.drift_kg_s)),
        ("blowdownKgS", number(result.blowdown_kg_s)),
        ("makeupKgS", number(result.makeup_kg_s)),
        (
            "cyclesOfConcentration",
            number(result.cycles_of_concentration),
        ),
    ]))
}
