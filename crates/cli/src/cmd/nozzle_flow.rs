use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;

pub(crate) fn nozzle_flow(cli: &Cli) -> Result<String, Failure> {
    let flow_m3_s = drafthouse_engine::nozzle_flow_m3_s(
        cli.required_number("--discharge-coefficient")?,
        cli.required_number("--orifice-diameter")?,
        cli.required_number("--dp")?,
        cli.number_or("--density", drafthouse_engine::NOZZLE_WATER_DENSITY_KG_M3)?,
    )?;
    Ok(object(&[("nozzleFlowM3S", number(flow_m3_s))]))
}
