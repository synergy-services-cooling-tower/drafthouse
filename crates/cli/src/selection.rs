use crate::args::Cli;
use crate::json::{json_array, number, object, optional_number};
use crate::json_string;
use crate::objects::{airside_object, layer_object};
use drafthouse_engine::{
    CatalogMetadata, FanSystemIntersection, NozzleArrangement, Objective, PsychrometricState,
    SelectionCandidate, SelectionRequirements, WaterBalance,
};

pub(crate) fn selection_inputs_object(
    cli: &Cli,
    run: &drafthouse_engine::SelectionRun,
    candidate: &SelectionCandidate,
) -> String {
    object(&[
        ("objective", json_string(run.objective.as_str())),
        ("catalog", catalog_object(&run.catalog_metadata)),
        ("requirements", requirements_object(&run.requirements)),
        (
            "selected",
            object(&[
                ("towerId", json_string(&candidate.tower_id)),
                ("fillId", json_string(&candidate.fill_id)),
                (
                    "driftEliminatorId",
                    json_string(&candidate.drift_eliminator_id),
                ),
                ("fanId", json_string(&candidate.fan_id)),
                ("nozzleId", json_string(&candidate.nozzle.nozzle_id)),
                ("fillDepthM", number(candidate.fill_depth_m)),
                ("speedRatio", number(candidate.speed_ratio)),
            ]),
        ),
        (
            "records",
            object(&[
                (
                    "tower",
                    spec_record_object(cli.values.get("--towers"), &candidate.tower_id),
                ),
                (
                    "fill",
                    spec_record_object(cli.values.get("--fills"), &candidate.fill_id),
                ),
                (
                    "driftEliminator",
                    spec_record_object(
                        cli.values.get("--drift-eliminators"),
                        &candidate.drift_eliminator_id,
                    ),
                ),
                (
                    "fan",
                    spec_record_object(cli.values.get("--fans"), &candidate.fan_id),
                ),
                (
                    "nozzle",
                    spec_record_object(cli.values.get("--nozzles"), &candidate.nozzle.nozzle_id),
                ),
            ]),
        ),
    ])
}

pub(crate) fn spec_record_object(list: Option<&String>, id: &str) -> String {
    let Some(list) = list else {
        return "null".to_string();
    };
    for record in list.split(';') {
        let pairs = record
            .split(',')
            .filter_map(|pair| pair.split_once(':'))
            .collect::<Vec<(&str, &str)>>();
        if pairs
            .iter()
            .any(|(key, value)| *key == "id" && *value == id)
        {
            return object(
                &pairs
                    .iter()
                    .map(|(key, value)| (*key, spec_value(value)))
                    .collect::<Vec<(&str, String)>>(),
            );
        }
    }
    "null".to_string()
}

pub(crate) fn spec_value(raw: &str) -> String {
    if raw.contains('|') {
        json_array(&raw.split('|').map(spec_value).collect::<Vec<String>>())
    } else if raw.contains(':') {
        json_array(&raw.split(':').map(spec_value).collect::<Vec<String>>())
    } else if let Ok(value) = raw.parse::<f64>() {
        if value.is_finite() {
            number(value)
        } else {
            json_string(raw)
        }
    } else {
        json_string(raw)
    }
}

pub(crate) fn catalog_object(metadata: &CatalogMetadata) -> String {
    object(&[
        ("id", json_string(&metadata.id)),
        ("revision", json_string(&metadata.revision)),
        ("status", json_string(&metadata.status)),
    ])
}

pub(crate) fn inlet_air_state_object(state: &PsychrometricState) -> String {
    object(&[
        ("dryBulbC", number(state.dry_bulb_c)),
        ("wetBulbC", number(state.wet_bulb_c)),
        ("humidityRatio", number(state.humidity_ratio)),
        ("enthalpyKJkgDryAir", number(state.enthalpy_kj_kg_dry_air)),
        ("dryAirDensityKgM3", number(state.dry_air_density_kg_m3)),
        ("moistAirDensityKgM3", number(state.moist_air_density_kg_m3)),
    ])
}

pub(crate) fn requirements_object(requirements: &SelectionRequirements) -> String {
    let speed_ratios = requirements
        .speed_ratios
        .iter()
        .map(|speed_ratio| number(*speed_ratio))
        .collect::<Vec<String>>();
    object(&[
        (
            "waterMassFlowKgS",
            number(requirements.water_mass_flow_kg_s),
        ),
        ("hotWaterC", number(requirements.hot_water_c)),
        ("targetColdWaterC", number(requirements.target_cold_water_c)),
        ("wetBulbC", number(requirements.wet_bulb_c)),
        ("dryBulbC", number(requirements.dry_bulb_c)),
        ("pressurePa", number(requirements.pressure_pa)),
        ("salinityGKg", number(requirements.salinity_g_kg)),
        (
            "waterQualityClass",
            json_string(&requirements.water_quality_class),
        ),
        (
            "cyclesOfConcentration",
            number(requirements.cycles_of_concentration),
        ),
        ("maxDriftPpm", number(requirements.max_drift_ppm)),
        (
            "maxElectricalInputKW",
            number(requirements.max_electrical_input_kw),
        ),
        ("maxFootprintM2", number(requirements.max_footprint_m2)),
        (
            "minimumThermalMarginC",
            number(requirements.minimum_thermal_margin_c),
        ),
        (
            "nozzlePressureDropPa",
            number(requirements.nozzle_pressure_drop_pa),
        ),
        ("speedRatios", json_array(&speed_ratios)),
    ])
}

pub(crate) fn rejection_summary_object(summary: &[(String, usize)]) -> String {
    let pairs = summary
        .iter()
        .map(|(label, count)| (label.as_str(), count.to_string()))
        .collect::<Vec<(&str, String)>>();
    object(&pairs)
}

pub(crate) fn candidate_object(candidate: &SelectionCandidate) -> String {
    let thermal = &candidate.thermal;
    let nozzle = &candidate.nozzle;
    object(&[
        ("rank", candidate.rank.to_string()),
        ("towerId", json_string(&candidate.tower_id)),
        ("fillId", json_string(&candidate.fill_id)),
        ("driftId", json_string(&candidate.drift_eliminator_id)),
        ("fanId", json_string(&candidate.fan_id)),
        ("fillDepthM", number(candidate.fill_depth_m)),
        ("fillLayers", fill_layers_array(candidate)),
        ("speedRatio", number(candidate.speed_ratio)),
        ("model", json_string(thermal.model)),
        ("coldWaterC", number(thermal.cold_water_c)),
        ("thermalMarginC", number(candidate.thermal_margin_c)),
        ("approachC", number(thermal.approach_c)),
        ("rangeC", number(thermal.range_c)),
        ("cpWaterKJkgK", number(thermal.cp_water_kj_kg_k)),
        (
            "outletAirHumidityRatio",
            number(thermal.outlet_air_state.humidity_ratio),
        ),
        ("heatTransferKW", number(thermal.heat_transfer_kw)),
        ("electricalInputKW", number(candidate.electrical_input_kw)),
        (
            "motorRequiredKW",
            number(candidate.motor.required_motor_output_kw),
        ),
        (
            "motorSelectedKW",
            optional_number(candidate.motor.selected_motor_kw),
        ),
        ("capacityKgS", optional_number(candidate.capacity_kg_s)),
        (
            "capabilityRatio",
            optional_number(candidate.capability_ratio),
        ),
        (
            "waterVolumetricFlowM3S",
            number(candidate.water_volumetric_flow_m3_s),
        ),
        (
            "nozzleOptionCount",
            candidate.nozzle_option_count.to_string(),
        ),
        ("airside", airside_object(&candidate.airside)),
        (
            "fanOperatingPoint",
            fan_point_object(&candidate.fan_operating_point),
        ),
        (
            "waterBalance",
            water_balance_object(&candidate.water_balance),
        ),
        ("nozzle", nozzle_arrangement_object(nozzle)),
    ])
}

fn fill_layers_array(candidate: &SelectionCandidate) -> String {
    json_array(
        &candidate
            .fill_layers
            .iter()
            .map(layer_object)
            .collect::<Vec<String>>(),
    )
}

fn fan_point_object(fan: &FanSystemIntersection) -> String {
    object(&[
        ("flowM3S", number(fan.flow_m3_s)),
        ("fanPressurePa", number(fan.fan_pressure_pa)),
        ("systemPressurePa", number(fan.system_pressure_pa)),
        ("efficiency", number(fan.efficiency)),
        ("shaftPowerKW", number(fan.shaft_power_kw)),
        ("residualPa", number(fan.residual_pa)),
    ])
}

fn water_balance_object(balance: &WaterBalance) -> String {
    object(&[
        ("evaporationKgS", number(balance.evaporation_kg_s)),
        ("driftKgS", number(balance.drift_kg_s)),
        ("blowdownKgS", number(balance.blowdown_kg_s)),
        ("makeupKgS", number(balance.makeup_kg_s)),
    ])
}

pub(crate) fn nozzle_arrangement_object(nozzle: &NozzleArrangement) -> String {
    object(&[
        ("nozzleId", json_string(&nozzle.nozzle_id)),
        ("nozzleName", json_string(&nozzle.nozzle_name)),
        ("count", number(nozzle.count)),
        ("flowPerNozzleM3S", number(nozzle.flow_per_nozzle_m3_s)),
        ("actualTotalFlowM3S", number(nozzle.actual_total_flow_m3_s)),
        ("excessFlowPct", number(nozzle.excess_flow_pct)),
        ("validCount", nozzle.valid_count.to_string()),
    ])
}

pub(crate) fn compact_candidate_object(candidate: &SelectionCandidate) -> String {
    object(&[
        ("towerId", json_string(&candidate.tower_id)),
        ("fillId", json_string(&candidate.fill_id)),
        ("driftId", json_string(&candidate.drift_eliminator_id)),
        ("fanId", json_string(&candidate.fan_id)),
        ("fillDepthM", number(candidate.fill_depth_m)),
        ("speedRatio", number(candidate.speed_ratio)),
        ("capacityKgS", optional_number(candidate.capacity_kg_s)),
        (
            "capabilityRatio",
            optional_number(candidate.capability_ratio),
        ),
        ("electricalInputKW", number(candidate.electrical_input_kw)),
        ("makeupKgS", number(candidate.water_balance.makeup_kg_s)),
        ("totalPa", number(candidate.airside.total_pa)),
        ("coldWaterC", number(candidate.thermal.cold_water_c)),
        ("thermalMarginC", number(candidate.thermal_margin_c)),
    ])
}

pub(crate) fn candidate_identity(candidate: &SelectionCandidate) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        candidate.tower_id,
        candidate.fill_id,
        candidate.drift_eliminator_id,
        candidate.fan_id,
        number(candidate.fill_depth_m),
        number(candidate.speed_ratio),
    )
}

pub(crate) fn orders_object(run: &drafthouse_engine::SelectionRun) -> String {
    let entries = Objective::ALL
        .iter()
        .map(|objective| {
            let identities = run
                .order_for(*objective)
                .iter()
                .filter_map(|index| run.candidates.get(*index))
                .map(|candidate| json_string(&candidate_identity(candidate)))
                .collect::<Vec<String>>();
            format!(
                "{}:{}",
                json_string(objective.as_str()),
                json_array(&identities)
            )
        })
        .collect::<Vec<String>>();
    format!("{{{}}}", entries.join(","))
}
