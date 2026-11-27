use crate::args::{Cli, Spec};
use crate::json::{json_array, number};
use crate::Failure;
use drafthouse_engine::PerformanceCurveRecord;

pub(crate) const METHOD_CURVE: &str = "Performance-curve rectangular-grid interpolation";

pub(crate) const SIGN_CONVENTION_CURVE: &str =
    "Positive leaving-water deviation means the test water is warmer (worse) than the curve prediction.";

pub(crate) const DISCLAIMER_CURVE: &str = "Automatic CTI crossplot rules and test corrections must be verified against the licensed current ATC-105 and contractual curve set.";

pub(crate) const CURVE_RECORD_FIELDS: [&str; 4] =
    ["wetBulbC", "rangeC", "waterFlowKgS", "coldWaterC"];

pub(crate) fn performance_curve_records(cli: &Cli) -> Result<Vec<PerformanceCurveRecord>, Failure> {
    let raw = cli
        .values
        .get("--records")
        .ok_or_else(|| Failure::Usage("a curve command needs --records".to_string()))?;
    let mut records = Vec::new();
    for entry in raw.split(';') {
        let spec = Spec::parse_text("--records", entry)?;
        spec.ensure_known(&CURVE_RECORD_FIELDS)?;
        records.push(PerformanceCurveRecord {
            wet_bulb_c: spec.required("wetBulbC")?,
            range_c: spec.required("rangeC")?,
            water_flow_kg_s: spec.required("waterFlowKgS")?,
            cold_water_c: spec.required("coldWaterC")?,
        });
    }
    Ok(records)
}

pub(crate) fn range_pair(limits: (f64, f64)) -> String {
    json_array(&[number(limits.0), number(limits.1)])
}
