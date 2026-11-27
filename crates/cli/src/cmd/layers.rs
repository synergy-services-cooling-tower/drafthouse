use crate::args::Cli;
use crate::json::{json_array, number, object};
use crate::objects::{airside_object, layer_object};
use crate::records::{drift_record, fan_record, fill_stack, rate_fill_records, tower_record};
use crate::{json_string, Failure};
use drafthouse_engine::{
    layered_system_pressure_breakdown, FillStack, LayeredBreakdown, LayeredBreakdownInput,
};

pub(crate) fn layers(cli: &Cli) -> Result<String, Failure> {
    let tower = tower_record(cli)?;
    let fills = rate_fill_records(cli)?;
    let raw = cli
        .values
        .get("--fill-stack")
        .ok_or_else(|| Failure::Usage("layers needs --fill-stack".to_string()))?;
    let stack = fill_stack("--fill-stack", raw)?;
    let terms = drafthouse_engine::resolve_fill_layers(&stack, &fills)?;
    let drift_eliminator = drift_record(cli)?;
    let fan = fan_record(cli)?;
    let result = layered_system_pressure_breakdown(LayeredBreakdownInput {
        tower: &tower,
        layers: &terms,
        drift_eliminator: &drift_eliminator,
        fan: fan.as_ref(),
        volumetric_air_flow_m3_s: cli.required_number("--flow")?,
        dry_air_density_kg_m3: cli.required_number("--dry-air-density")?,
        moist_air_density_kg_m3: cli.required_number("--moist-air-density")?,
        water_mass_flow_kg_s: cli.required_number("--water")?,
        thermal_multiplier: cli.number_or("--thermal-multiplier", 1.0)?,
        pressure_multiplier: cli.number_or("--pressure-multiplier", 1.0)?,
    })?;
    let failures = drafthouse_engine::check_stack_operating_envelope(
        &terms,
        result.breakdown.water_loading_kg_m2_s,
        result.breakdown.dry_air_loading_kg_m2_s,
        cli.number("--hot")?,
        cli.values.get("--quality-class").map(String::as_str),
    );
    if !failures.is_empty() {
        return Err(Failure::Domain(drafthouse_engine::DomainError::new(
            failures.join("\n"),
        )));
    }
    Ok(layers_object(&stack, &result))
}

pub(crate) fn layers_object(stack: &FillStack, result: &LayeredBreakdown) -> String {
    object(&[
        ("stackLabel", json_string(&stack.label())),
        ("totalDepthM", number(stack.total_depth_m())),
        (
            "layers",
            json_array(
                &result
                    .fill_layers
                    .iter()
                    .map(layer_object)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "layerTotals",
            object(&[
                ("pressureDropPa", number(result.breakdown.fill_pa)),
                ("merkelNumber", number(result.breakdown.fill_merkel_number)),
                (
                    "availableMerkelNumber",
                    number(result.breakdown.available_merkel_number),
                ),
            ]),
        ),
        ("airside", airside_object(&result.breakdown)),
    ])
}
