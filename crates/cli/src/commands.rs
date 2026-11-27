use crate::args::Cli;
use crate::cmd;
use crate::Failure;

pub(crate) fn dispatch(command: &str, cli: &Cli) -> Result<String, Failure> {
    match command {
        "psy" => cmd::psy::psy(cli),
        "merkel" => cmd::merkel::merkel(cli),
        "inverse" => cmd::inverse::inverse(cli),
        "airside" => cmd::airside::airside(cli),
        "layers" => cmd::layers::layers(cli),
        "fan" => cmd::fan::fan(cli),
        "envelope" => cmd::envelope::envelope(cli),
        "estimate" => cmd::estimate::estimate(cli),
        "motor" => cmd::motor::motor(cli),
        "evaporation" => cmd::evaporation::evaporation(cli),
        "drift" => cmd::drift::drift(cli),
        "balance" => cmd::balance::balance(cli),
        "capability" => cmd::capability::capability(cli),
        "capability-mc" => cmd::capability_mc::capability_monte_carlo(cli),
        "natural-draft" => cmd::natural_draft::natural_draft(cli),
        "curve-bounds" => cmd::curve_bounds::curve_bounds(cli),
        "curve-predict" => cmd::curve_predict::curve_predict(cli),
        "curve-flow" => cmd::curve_flow::curve_flow(cli),
        "curve-capability" => cmd::curve_capability::curve_capability(cli),
        "select" => cmd::select::select(cli),
        "crossflow" => cmd::crossflow::crossflow(cli),
        "convergence" => cmd::convergence::convergence(cli),
        "nozzle" => cmd::nozzle::nozzle(cli),
        "nozzle-flow" => cmd::nozzle_flow::nozzle_flow(cli),
        "validate-catalog" => cmd::validate_catalog::validate_catalog_command(cli),
        "select-fields" => Ok(cmd::select_fields::select_fields_object()),
        other => Err(Failure::Usage(format!("unknown command: {other}"))),
    }
}
