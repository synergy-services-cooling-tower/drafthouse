use crate::args::Cli;
use crate::cmd::capability::{capability_input, condition_sigmas};
use crate::json::{number, object};
use crate::Failure;
use drafthouse_engine::{CapabilityUncertainty, MonteCarloInput};

pub(crate) fn capability_monte_carlo(cli: &Cli) -> Result<String, Failure> {
    let uncertainty = CapabilityUncertainty {
        design: condition_sigmas(cli, "--sigma-design")?,
        test: condition_sigmas(cli, "--sigma-test")?,
        characteristic_exponent: cli.number("--sigma-exponent")?,
        salinity_g_kg: cli.number("--sigma-salinity")?,
    };
    let input = MonteCarloInput {
        base_input: capability_input(cli)?,
        uncertainty,
        samples: cli.integer("--samples", 1000)?,
        seed: cli.number_or("--seed", 20260813.0)?,
    };
    let result = drafthouse_engine::monte_carlo_characteristic_capability(&input)?;
    Ok(object(&[
        ("samplesRequested", result.samples_requested.to_string()),
        ("samplesAccepted", result.samples_accepted.to_string()),
        ("rejectedSamples", result.rejected_samples.to_string()),
        ("meanCapabilityPct", number(result.mean_capability_pct)),
        (
            "standardDeviationPctPoints",
            number(result.standard_deviation_pct_points),
        ),
        ("p2_5", number(result.p2_5)),
        ("p50", number(result.p50)),
        ("p97_5", number(result.p97_5)),
        (
            "expandedUncertaintyApproxPctPoints",
            number(result.expanded_uncertainty_approx_pct_points),
        ),
        ("seed", number(result.seed)),
    ]))
}
