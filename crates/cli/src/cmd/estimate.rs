use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;
use drafthouse_engine::AirflowEstimateInput;

pub(crate) fn estimate(cli: &Cli) -> Result<String, Failure> {
    let result = drafthouse_engine::estimate_airflow_from_fan_power(AirflowEstimateInput {
        reference_volumetric_flow_m3_s: cli.required_number("--ref-flow")?,
        reference_fan_power_kw: cli.required_number("--ref-power")?,
        actual_fan_power_kw: cli.required_number("--actual-power")?,
        reference_air_density_kg_m3: cli.required_number("--ref-density")?,
        actual_air_density_kg_m3: cli.required_number("--actual-density")?,
    })?;
    Ok(object(&[("airflowM3S", number(result))]))
}
