use crate::args::Cli;
use crate::objects::airside_object;
use crate::records::{drift_record, fan_record, fill_record, tower_record};
use crate::Failure;
use drafthouse_engine::SystemPressureBreakdownInput;

pub(crate) fn airside(cli: &Cli) -> Result<String, Failure> {
    let tower = tower_record(cli)?;
    let fill = fill_record(cli)?;
    let drift_eliminator = drift_record(cli)?;
    let fan = fan_record(cli)?;
    let input = SystemPressureBreakdownInput {
        tower: &tower,
        fill: &fill,
        fill_depth_m: cli.required_number("--fill-depth")?,
        drift_eliminator: &drift_eliminator,
        fan: fan.as_ref(),
        volumetric_air_flow_m3_s: cli.required_number("--flow")?,
        dry_air_density_kg_m3: cli.required_number("--dry-air-density")?,
        moist_air_density_kg_m3: cli.required_number("--moist-air-density")?,
        water_mass_flow_kg_s: cli.required_number("--water")?,
        thermal_multiplier: cli.number_or("--thermal-multiplier", 1.0)?,
        pressure_multiplier: cli.number_or("--pressure-multiplier", 1.0)?,
    };
    let result = drafthouse_engine::system_pressure_breakdown(input)?;
    Ok(airside_object(&result))
}
