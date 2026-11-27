use crate::args::Cli;
use crate::curves::performance_curve_records;
use crate::json::{number, object};
use crate::Failure;

pub(crate) fn curve_flow(cli: &Cli) -> Result<String, Failure> {
    let records = performance_curve_records(cli)?;
    let prediction = drafthouse_engine::predict_water_flow_from_performance_curves(
        &records,
        cli.required_number("--wb")?,
        cli.required_number("--range")?,
        cli.required_number("--cold")?,
    )?;
    Ok(object(&[
        ("waterFlowKgS", number(prediction.water_flow_kg_s)),
        (
            "predictedColdWaterC",
            number(prediction.predicted_cold_water_c),
        ),
    ]))
}
