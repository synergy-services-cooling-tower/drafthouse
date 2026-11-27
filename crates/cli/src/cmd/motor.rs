use crate::args::Cli;
use crate::json::{number, object, optional_number};
use crate::Failure;

pub(crate) fn motor(cli: &Cli) -> Result<String, Failure> {
    let selection = drafthouse_engine::choose_standard_motor(
        cli.required_number("--shaft-power")?,
        cli.number_or("--drive-efficiency", 0.96)?,
        cli.number_or("--service-factor", 1.1)?,
    )?;
    Ok(object(&[
        (
            "requiredMotorOutputKW",
            number(selection.required_motor_output_kw),
        ),
        (
            "selectedMotorKW",
            optional_number(selection.selected_motor_kw),
        ),
        ("driveEfficiency", number(selection.drive_efficiency)),
        ("serviceFactor", number(selection.service_factor)),
    ]))
}
