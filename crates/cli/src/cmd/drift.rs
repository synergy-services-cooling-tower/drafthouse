use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;

pub(crate) fn drift(cli: &Cli) -> Result<String, Failure> {
    let drift_kg_s = drafthouse_engine::drift_loss_kg_s(
        cli.required_number("--circulating-water")?,
        cli.required_number("--drift-ppm")?,
    )?;
    Ok(object(&[("driftKgS", number(drift_kg_s))]))
}
