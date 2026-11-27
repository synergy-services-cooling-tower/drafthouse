use crate::args::Cli;
use crate::json::{json_string_array, object};
use crate::records::fill_record;
use crate::{json_string, Failure};
use drafthouse_engine::FillOperatingEnvelopeInput;

pub(crate) fn envelope(cli: &Cli) -> Result<String, Failure> {
    let fill = fill_record(cli)?;
    let water_quality_class = cli
        .values
        .get("--quality")
        .ok_or_else(|| Failure::Usage("envelope needs --quality".to_string()))?;
    let result = drafthouse_engine::check_fill_operating_envelope(FillOperatingEnvelopeInput {
        fill: &fill,
        water_loading_kg_m2_s: cli.required_number("--water-loading")?,
        dry_air_loading_kg_m2_s: cli.required_number("--dry-air-loading")?,
        hot_water_c: cli.required_number("--hot-water")?,
        water_quality_class,
    });
    Ok(object(&[
        ("ok", result.ok.to_string()),
        ("failureCount", result.failures.len().to_string()),
        ("failures", json_string_array(&result.failures)),
        ("qualityClass", json_string(water_quality_class)),
    ]))
}
