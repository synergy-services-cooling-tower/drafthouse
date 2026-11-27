use crate::args::Cli;
use crate::json::{number, object};
use crate::objects::{air_state_object, airside_object};
use crate::records::{drift_record, fill_record, tower_record};
use crate::{json_string, Failure};
use drafthouse_engine::NaturalDraftInput;

pub(crate) const MODEL_NATURAL_DRAFT: &str =
    "simplified coupled natural-draft counterflow (demonstration only)";

pub(crate) const CAVEAT_NATURAL_DRAFT: &str = "A production natural-draft model requires vertical/radial density integration, shell loss, wind effects, rain zone, and validated fill-zone correlations.";

pub(crate) fn natural_draft(cli: &Cli) -> Result<String, Failure> {
    let tower = tower_record(cli)?;
    let fill = fill_record(cli)?;
    let drift_eliminator = drift_record(cli)?;
    let input = NaturalDraftInput {
        tower: &tower,
        effective_draft_height_m: cli.required_number("--draft-height")?,
        fill: &fill,
        fill_depth_m: cli.required_number("--fill-depth")?,
        drift_eliminator: &drift_eliminator,
        hot_water_c: cli.required_number("--hot")?,
        dry_bulb_c: cli.required_number("--db")?,
        wet_bulb_c: cli.required_number("--wb")?,
        pressure_pa: cli.number_or("--p", 101_325.0)?,
        water_mass_flow_kg_s: cli.required_number("--water")?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        thermal_multiplier: cli.number_or("--thermal-multiplier", 1.0)?,
        pressure_multiplier: cli.number_or("--pressure-multiplier", 1.0)?,
        min_face_velocity_ms: cli.number_or("--min-face-velocity", 0.2)?,
        max_face_velocity_ms: cli.number_or("--max-face-velocity", 6.0)?,
    };
    let result = drafthouse_engine::solve_natural_draft_counterflow(&input)?;
    Ok(object(&[
        ("model", json_string(MODEL_NATURAL_DRAFT)),
        (
            "volumetricAirFlowM3S",
            number(result.volumetric_air_flow_m3_s),
        ),
        ("waterToDryAirRatio", number(result.water_to_dry_air_ratio)),
        ("plumeDensityKgM3", number(result.plume_density_kg_m3)),
        ("draftPressurePa", number(result.draft_pressure_pa)),
        ("residualPa", number(result.residual_pa)),
        ("airside", airside_object(&result.airside)),
        (
            "thermal",
            object(&[
                ("coldWaterC", number(result.thermal.cold_water_c)),
                ("approachC", number(result.thermal.approach_c)),
                ("rangeC", number(result.thermal.range_c)),
                (
                    "requiredMerkelNumber",
                    number(result.thermal.required_merkel_number),
                ),
                ("cpWaterKJkgK", number(result.thermal.cp_water_kj_kg_k)),
            ]),
        ),
        (
            "outlet",
            object(&[
                ("dryBulbC", number(result.outlet.dry_bulb_c)),
                ("wetBulbC", number(result.outlet.wet_bulb_c)),
                ("relativeHumidity", number(result.outlet.relative_humidity)),
                ("humidityRatio", number(result.outlet.humidity_ratio)),
                (
                    "enthalpyKJkgDryAir",
                    number(result.outlet.enthalpy_kj_kg_dry_air),
                ),
                ("pressurePa", number(result.outlet.pressure_pa)),
            ]),
        ),
        ("inletAirState", air_state_object(&result.inlet_air_state)),
        ("evaporationKgS", number(result.evaporation_kg_s)),
        ("caveat", json_string(CAVEAT_NATURAL_DRAFT)),
    ]))
}
