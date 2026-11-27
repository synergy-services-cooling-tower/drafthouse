use crate::args::Cli;
use crate::curves::{performance_curve_records, range_pair};
use crate::json::object;
use crate::Failure;

pub(crate) fn curve_bounds(cli: &Cli) -> Result<String, Failure> {
    let bounds = drafthouse_engine::performance_curve_bounds(&performance_curve_records(cli)?)?;
    Ok(object(&[
        ("wetBulbC", range_pair(bounds.wet_bulb_c)),
        ("rangeC", range_pair(bounds.range_c)),
        ("waterFlowKgS", range_pair(bounds.water_flow_kg_s)),
        ("coldWaterC", range_pair(bounds.cold_water_c)),
    ]))
}
