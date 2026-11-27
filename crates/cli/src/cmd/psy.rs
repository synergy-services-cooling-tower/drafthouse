use crate::args::Cli;
use crate::json::{number, object};
use crate::Failure;
use drafthouse_engine::{PsychrometricOptions, PsychrometricStateInput};

pub(crate) fn psy(cli: &Cli) -> Result<String, Failure> {
    let dry_bulb_c = cli.required_number("--db")?;
    let pressure_pa = cli.number_or("--p", 101_325.0)?;
    let enhancement_factor = !cli.has("--no-enhancement");
    let input = match (cli.number("--wb")?, cli.number("--rh")?) {
        (Some(wet_bulb_c), _) => PsychrometricStateInput::from_wet_bulb(dry_bulb_c, wet_bulb_c),
        (None, Some(relative_humidity)) => {
            PsychrometricStateInput::from_relative_humidity(dry_bulb_c, relative_humidity)
        }
        (None, None) => return Err(Failure::Usage("psy needs --wb or --rh".to_string())),
    }
    .with_pressure(pressure_pa)
    .with_enhancement_factor(enhancement_factor);

    let state = drafthouse_engine::psychrometric_state(input)?;
    let options = PsychrometricOptions { enhancement_factor };
    Ok(object(&[
        ("dryBulbC", number(state.dry_bulb_c)),
        ("wetBulbC", number(state.wet_bulb_c)),
        ("relativeHumidity", number(state.relative_humidity)),
        ("pressurePa", number(state.pressure_pa)),
        ("humidityRatio", number(state.humidity_ratio)),
        ("enthalpyKJkgDryAir", number(state.enthalpy_kj_kg_dry_air)),
        ("dryAirDensityKgM3", number(state.dry_air_density_kg_m3)),
        ("moistAirDensityKgM3", number(state.moist_air_density_kg_m3)),
        ("dewPointC", number(state.dew_point_c)),
        ("enhancementFactor", number(state.enhancement_factor)),
        (
            "saturationVaporPressurePa",
            number(drafthouse_engine::saturation_vapor_pressure_pa(
                state.dry_bulb_c,
            )?),
        ),
        (
            "saturationHumidityRatio",
            number(drafthouse_engine::saturation_humidity_ratio(
                state.dry_bulb_c,
                state.pressure_pa,
                options,
            )?),
        ),
        (
            "saturatedAirEnthalpyKJkgDryAir",
            number(drafthouse_engine::saturated_air_enthalpy_kj_kg_dry_air(
                state.dry_bulb_c,
                state.pressure_pa,
                options,
            )?),
        ),
    ]))
}
