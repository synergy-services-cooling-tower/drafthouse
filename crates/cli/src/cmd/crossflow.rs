use crate::args::Cli;
use crate::json::{number, object, optional_number};
use crate::{json_string, Failure};
use drafthouse_engine::CrossflowGridInput;

pub(crate) fn crossflow_input(cli: &Cli) -> Result<CrossflowGridInput, Failure> {
    let mut input = CrossflowGridInput::new(
        cli.required_number("--hot")?,
        cli.required_number("--db")?,
        cli.required_number("--wb")?,
        cli.required_number("--water")?,
        cli.required_number("--dry-air")?,
        cli.required_number("--kavl")?,
    );
    if let Some(pressure_pa) = cli.number("--p")? {
        input = input.with_pressure(pressure_pa);
    }
    if let Some(salinity_g_kg) = cli.number("--salinity")? {
        input = input.with_salinity(salinity_g_kg);
    }
    let air_cells = cli.integer("--cells", input.air_cells)?;
    let water_cells = cli.integer("--water-cells", air_cells)?;
    input = input.with_cells(air_cells, water_cells);
    if cli.has("--no-richardson") {
        input = input.with_richardson(false);
    }
    Ok(input)
}

pub(crate) fn crossflow(cli: &Cli) -> Result<String, Failure> {
    let result = drafthouse_engine::solve_crossflow_grid(&crossflow_input(cli)?)?;
    Ok(object(&[
        ("model", json_string(result.model())),
        ("coldWaterC", number(result.cold_water_c)),
        ("rangeC", number(result.range_c)),
        ("approachC", number(result.approach_c)),
        ("heatTransferKW", number(result.heat_transfer_kw)),
        ("waterEnergyKW", number(result.water_energy_kw)),
        ("cpWaterKJkgK", number(result.cp_water_kj_kg_k)),
        (
            "outletAirDryBulbC",
            number(result.outlet_air_state.dry_bulb_c),
        ),
        (
            "outletAirHumidityRatio",
            number(result.outlet_air_state.humidity_ratio),
        ),
        (
            "outletAirEnthalpyKJkgDryAir",
            number(result.outlet_air_state.enthalpy_kj_kg_dry_air),
        ),
        (
            "coarseColdWaterC",
            number(result.grid_convergence.coarse_cold_water_c),
        ),
        (
            "fineColdWaterC",
            optional_number(result.grid_convergence.fine_cold_water_c),
        ),
        (
            "richardsonColdWaterC",
            number(result.grid_convergence.richardson_cold_water_c),
        ),
        (
            "estimatedDiscretizationErrorC",
            optional_number(result.grid_convergence.estimated_discretization_error_c),
        ),
    ]))
}
