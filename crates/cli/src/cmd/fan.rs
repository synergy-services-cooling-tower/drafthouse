use crate::args::Cli;
use crate::json::{number, object};
use crate::records::{drift_record, fan_record, fill_record, tower_record};
use crate::Failure;
use drafthouse_engine::{FanSystemIntersectionInput, SystemPressureBreakdownInput};

pub(crate) fn fan(cli: &Cli) -> Result<String, Failure> {
    let fan = fan_record(cli)?.ok_or_else(|| Failure::Usage("fan needs --curve".to_string()))?;
    let air_density_kg_m3 = cli.required_number("--air-density")?;
    let mut input = FanSystemIntersectionInput::new(&fan, air_density_kg_m3);
    if let Some(speed_ratio) = cli.number("--speed-ratio")? {
        input = input.with_speed_ratio(speed_ratio);
    }
    if let Some(minimum_flow_fraction) = cli.number("--min-flow-fraction")? {
        input = input.with_minimum_flow_fraction(minimum_flow_fraction);
    }
    let limits = drafthouse_engine::fan_operating_limits(&fan, input.speed_ratio)?;
    let system_pressure_fn: Box<dyn Fn(f64) -> Result<f64, drafthouse_engine::DomainError>> =
        if cli.has("--system-breakdown") {
            let tower = tower_record(cli)?;
            let fill = fill_record(cli)?;
            let drift_eliminator = drift_record(cli)?;
            let fill_depth_m = cli.required_number("--fill-depth")?;
            let water_mass_flow_kg_s = cli.required_number("--water")?;
            let dry_air_density_kg_m3 = cli.required_number("--dry-air-density")?;
            let moist_air_density_kg_m3 = cli.required_number("--moist-air-density")?;
            let fan = fan.clone();
            Box::new(move |flow_m3_s: f64| {
                Ok(
                    drafthouse_engine::system_pressure_breakdown(SystemPressureBreakdownInput {
                        tower: &tower,
                        fill: &fill,
                        fill_depth_m,
                        drift_eliminator: &drift_eliminator,
                        fan: Some(&fan),
                        volumetric_air_flow_m3_s: flow_m3_s,
                        dry_air_density_kg_m3,
                        moist_air_density_kg_m3,
                        water_mass_flow_kg_s,
                        thermal_multiplier: 1.0,
                        pressure_multiplier: 1.0,
                    })?
                    .total_pa,
                )
            })
        } else {
            let constant_pa = cli.number_or("--system-a", 0.0)?;
            let quadratic_pa = cli.number_or("--system-b", 0.0)?;
            Box::new(move |flow_m3_s: f64| Ok(constant_pa + quadratic_pa * flow_m3_s.powi(2)))
        };
    let result = drafthouse_engine::solve_fan_system_intersection(&input, &*system_pressure_fn)?;
    Ok(object(&[
        ("minFlowM3S", number(limits.min_flow_m3_s)),
        ("maxFlowM3S", number(limits.max_flow_m3_s)),
        ("flowM3S", number(result.flow_m3_s)),
        ("fanPressurePa", number(result.fan_pressure_pa)),
        ("systemPressurePa", number(result.system_pressure_pa)),
        ("efficiency", number(result.efficiency)),
        ("shaftPowerKW", number(result.shaft_power_kw)),
        ("residualPa", number(result.residual_pa)),
        ("speedRatio", number(result.speed_ratio)),
    ]))
}
