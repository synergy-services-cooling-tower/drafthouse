use crate::args::Cli;
use crate::cmd::crossflow::crossflow_input;
use crate::json::{number, object, optional_number};
use crate::Failure;
use drafthouse_engine::numeric::truncate_to_u64;
use drafthouse_engine::CrossflowStudyInput;

pub(crate) fn convergence(cli: &Cli) -> Result<String, Failure> {
    let mut input = CrossflowStudyInput::new(crossflow_input(cli)?);
    if let Some(base_cells) = cli.number("--base-cells")? {
        input = input
            .with_base_cells(usize::try_from(truncate_to_u64(base_cells)).unwrap_or(usize::MAX));
    }
    if let Some(safety_factor) = cli.number("--safety-factor")? {
        input = input.with_safety_factor(safety_factor);
    }
    let study = drafthouse_engine::crossflow_convergence_study(&input)?;
    Ok(object(&[
        ("observedOrder", optional_number(study.observed_order)),
        (
            "extrapolatedColdWaterC",
            number(study.extrapolated_cold_water_c),
        ),
        (
            "fineGridErrorEstimateC",
            number(study.fine_grid_error_estimate_c),
        ),
        (
            "gridConvergenceIndexPct",
            number(study.grid_convergence_index_pct),
        ),
        ("coarseCellsColdWaterC", number(study.cold_water_c[0])),
        ("mediumCellsColdWaterC", number(study.cold_water_c[1])),
        ("fineCellsColdWaterC", number(study.cold_water_c[2])),
    ]))
}
