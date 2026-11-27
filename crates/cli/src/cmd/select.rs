use crate::args::{parse_number, Cli};
use crate::json::{json_array, object};
use crate::records::{selection_catalog, validate_catalog_records};
use crate::selection::{
    candidate_object, catalog_object, compact_candidate_object, inlet_air_state_object,
    orders_object, rejection_summary_object, requirements_object, selection_inputs_object,
};
use crate::worked::selection_worked_object;
use crate::{json_string, Failure};
use drafthouse_engine::{Objective, SelectionInput, SelectionRequirements, SelectionRun};

pub(crate) fn select(cli: &Cli) -> Result<String, Failure> {
    validate_catalog_records(cli)?;
    let catalog = selection_catalog(cli)?;
    let objective = objective(cli)?;
    let requirements = requirements(cli)?;
    let max_results = cli.integer("--max-results", 30)?;

    let input = SelectionInput::new(&catalog)
        .with_objective(objective)
        .with_max_results(max_results)
        .with_requirements(requirements);
    let run = drafthouse_engine::run_selection(&input)?;

    let mut fields = run_fields(&run, max_results);
    if cli.has("--all-orders") {
        fields.push((
            "candidates",
            json_array(
                &run.candidates
                    .iter()
                    .map(compact_candidate_object)
                    .collect::<Vec<String>>(),
            ),
        ));
        fields.push(("orders", orders_object(&run)));
    }
    if let Some(recommended) = run.candidates.first() {
        fields.push(("inputs", selection_inputs_object(cli, &run, recommended)));
        fields.push(("worked", selection_worked_object(&run, recommended)));
    }
    Ok(object(&fields))
}

fn objective(cli: &Cli) -> Result<Objective, Failure> {
    Ok(match cli.values.get("--objective") {
        None => Objective::default(),
        Some(raw) => Objective::parse(raw).ok_or_else(|| {
            let allowed = Objective::ALL
                .iter()
                .map(|objective| objective.as_str())
                .collect::<Vec<_>>()
                .join("|");
            Failure::Usage(format!("--objective expects {allowed}, got {raw:?}"))
        })?,
    })
}

fn requirements(cli: &Cli) -> Result<SelectionRequirements, Failure> {
    let mut requirements = SelectionRequirements::default();
    if let Some(value) = cli.number("--water")? {
        requirements.water_mass_flow_kg_s = value;
    }
    if let Some(value) = cli.number("--hot")? {
        requirements.hot_water_c = value;
    }
    if let Some(value) = cli.number("--target-cold")? {
        requirements.target_cold_water_c = value;
    }
    if let Some(value) = cli.number("--wb")? {
        requirements.wet_bulb_c = value;
    }
    if let Some(value) = cli.number("--db")? {
        requirements.dry_bulb_c = value;
    }
    if let Some(value) = cli.number("--p")? {
        requirements.pressure_pa = value;
    }
    if let Some(value) = cli.number("--salinity")? {
        requirements.salinity_g_kg = value;
    }
    if let Some(value) = cli.values.get("--quality-class") {
        requirements.water_quality_class.clone_from(value);
    }
    if let Some(value) = cli.number("--cycles")? {
        requirements.cycles_of_concentration = value;
    }
    if let Some(value) = cli.number("--max-drift-ppm")? {
        requirements.max_drift_ppm = value;
    }
    if let Some(value) = cli.number("--max-power")? {
        requirements.max_electrical_input_kw = value;
    }
    if let Some(value) = cli.number("--max-footprint")? {
        requirements.max_footprint_m2 = value;
    }
    if let Some(value) = cli.number("--min-margin")? {
        requirements.minimum_thermal_margin_c = value;
    }
    if let Some(value) = cli.number("--nozzle-dp")? {
        requirements.nozzle_pressure_drop_pa = value;
    }
    if let Some(raw) = cli.values.get("--speed-ratios") {
        requirements.speed_ratios = raw
            .split('|')
            .map(|part| parse_number(part, "--speed-ratios"))
            .collect::<Result<Vec<f64>, Failure>>()?;
    }
    Ok(requirements)
}

fn run_fields(run: &SelectionRun, max_results: usize) -> Vec<(&'static str, String)> {
    let capacities_resolved = run
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.capacity_kg_s.is_some() && candidate.capability_ratio.is_some()
        })
        .count();

    vec![
        ("objective", json_string(run.objective.as_str())),
        ("requirements", requirements_object(&run.requirements)),
        ("catalog", catalog_object(&run.catalog_metadata)),
        (
            "inletAirState",
            inlet_air_state_object(&run.inlet_air_state),
        ),
        (
            "feasibleCandidateCount",
            run.feasible_candidate_count().to_string(),
        ),
        (
            "rejectionSummary",
            rejection_summary_object(&run.rejection_summary),
        ),
        ("capacitiesResolved", capacities_resolved.to_string()),
        ("warning", json_string(&run.warning)),
        (
            "results",
            json_array(
                &run.candidates
                    .iter()
                    .take(max_results)
                    .map(candidate_object)
                    .collect::<Vec<String>>(),
            ),
        ),
    ]
}
