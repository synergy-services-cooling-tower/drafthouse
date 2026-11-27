use crate::args::Cli;
use crate::json::{number, object};
use crate::records::merkel_input;
use crate::Failure;

pub(crate) fn merkel(cli: &Cli) -> Result<String, Failure> {
    let input = merkel_input(cli)?;
    let result = drafthouse_engine::merkel_demand(&input)?;
    Ok(object(&[
        ("merkelNumber", number(result.merkel_number)),
        (
            "inletAirEnthalpyKJkgDryAir",
            number(result.inlet_air_enthalpy_kj_kg_dry_air),
        ),
        ("cpWaterKJkgK", number(result.cp_water_kj_kg_k)),
        (
            "inletAirState",
            object(&[
                (
                    "humidityRatio",
                    number(result.inlet_air_state.humidity_ratio),
                ),
                (
                    "relativeHumidity",
                    number(result.inlet_air_state.relative_humidity),
                ),
                ("wetBulbC", number(result.inlet_air_state.wet_bulb_c)),
            ]),
        ),
    ]))
}
