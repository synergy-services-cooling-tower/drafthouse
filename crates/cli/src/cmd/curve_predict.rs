use crate::args::Cli;
use crate::curves::{performance_curve_records, range_pair};
use crate::json::{number, object};
use crate::Failure;

pub(crate) fn curve_predict(cli: &Cli) -> Result<String, Failure> {
    let records = performance_curve_records(cli)?;
    let prediction = drafthouse_engine::predict_cold_water_from_performance_curves(
        &records,
        cli.required_number("--wb")?,
        cli.required_number("--range")?,
        cli.required_number("--water")?,
    )?;
    Ok(object(&[
        ("coldWaterC", number(prediction.cold_water_c)),
        (
            "brackets",
            object(&[
                ("wetBulbC", range_pair(prediction.wet_bulb_bracket_c)),
                ("rangeC", range_pair(prediction.range_bracket_c)),
            ]),
        ),
        (
            "extrapolated",
            if prediction.extrapolated {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ]))
}
