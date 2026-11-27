use crate::args::Cli;
use crate::curves::{
    performance_curve_records, DISCLAIMER_CURVE, METHOD_CURVE, SIGN_CONVENTION_CURVE,
};
use crate::json::{number, object};
use crate::{json_string, Failure};

pub(crate) fn curve_capability(cli: &Cli) -> Result<String, Failure> {
    let records = performance_curve_records(cli)?;
    let result = drafthouse_engine::evaluate_performance_curve_capability(
        &records,
        cli.required_number("--wb")?,
        cli.required_number("--range")?,
        cli.required_number("--cold")?,
        cli.required_number("--adjusted-flow")?,
    )?;
    Ok(object(&[
        ("method", json_string(METHOD_CURVE)),
        (
            "predictedWaterFlowKgS",
            number(result.predicted_water_flow_kg_s),
        ),
        (
            "adjustedTestWaterFlowKgS",
            number(result.adjusted_test_water_flow_kg_s),
        ),
        ("capabilityPct", number(result.capability_pct)),
        (
            "predictedColdWaterAtAdjustedFlowC",
            number(result.predicted_cold_water_at_adjusted_flow_c),
        ),
        (
            "leavingWaterDeviationC",
            number(result.leaving_water_deviation_c),
        ),
        ("signConvention", json_string(SIGN_CONVENTION_CURVE)),
        ("disclaimer", json_string(DISCLAIMER_CURVE)),
    ]))
}
