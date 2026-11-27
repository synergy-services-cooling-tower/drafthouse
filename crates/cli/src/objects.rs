use crate::json::{number, object, optional_number};
use crate::json_string;
use drafthouse_engine::{
    FillPressureCorrelation, InletEnthalpyConvention, PsychrometricState, ZoneCorrelation,
};

pub(crate) fn airside_object(result: &drafthouse_engine::SystemPressureBreakdown) -> String {
    let areas = result.areas;
    object(&[
        ("totalPa", number(result.total_pa)),
        ("fillPa", number(result.fill_pa)),
        ("driftPa", number(result.drift_pa)),
        ("inletPa", number(result.inlet_pa)),
        ("distributionPa", number(result.distribution_pa)),
        ("supportPa", number(result.support_pa)),
        ("plenumPa", number(result.plenum_pa)),
        ("fanStackPa", number(result.fan_stack_pa)),
        ("fixedPa", number(result.fixed_pa)),
        ("driftPpm", number(result.drift_ppm)),
        ("waterLoadingKgM2S", number(result.water_loading_kg_m2_s)),
        ("dryAirLoadingKgM2S", number(result.dry_air_loading_kg_m2_s)),
        ("dryAirMassFlowKgS", number(result.dry_air_mass_flow_kg_s)),
        ("fillVelocityMS", number(result.fill_velocity_ms)),
        ("driftVelocityMS", number(result.drift_velocity_ms)),
        ("inletVelocityMS", number(result.inlet_velocity_ms)),
        ("plenumVelocityMS", number(result.plenum_velocity_ms)),
        (
            "fanStackVelocityMS",
            optional_number(result.fan_stack_velocity_ms),
        ),
        (
            "fanPressureBasis",
            json_string(result.fan_pressure_basis.as_str()),
        ),
        (
            "areas",
            object(&[
                ("fillAreaM2", number(areas.fill_area_m2)),
                ("airFreeAreaM2", number(areas.air_free_area_m2)),
                ("driftAreaM2", number(areas.drift_area_m2)),
                ("inletAreaM2", number(areas.inlet_area_m2)),
                ("plenumAreaM2", number(areas.plenum_area_m2)),
                ("fanStackAreaM2", optional_number(areas.fan_stack_area_m2)),
            ]),
        ),
        ("fillMerkelNumber", number(result.fill_merkel_number)),
        (
            "sprayZoneMerkelNumber",
            number(result.spray_zone_merkel_number),
        ),
        (
            "rainZoneMerkelNumber",
            number(result.rain_zone_merkel_number),
        ),
        (
            "availableMerkelNumber",
            number(result.available_merkel_number),
        ),
    ])
}

pub(crate) fn layer_object(layer: &drafthouse_engine::FillLayerResult) -> String {
    object(&[
        ("position", layer.position.to_string()),
        ("label", json_string(&layer.label())),
        ("fillId", json_string(&layer.fill_id)),
        ("depthM", number(layer.depth_m)),
        ("thermalMultiplier", number(layer.thermal_multiplier)),
        ("pressureMultiplier", number(layer.pressure_multiplier)),
        (
            "effectiveThermalMultiplier",
            number(layer.effective_thermal_multiplier),
        ),
        (
            "effectivePressureMultiplier",
            number(layer.effective_pressure_multiplier),
        ),
        ("thermal", zone_object(&layer.thermal)),
        ("pressure", pressure_object(&layer.pressure)),
        ("limits", limits_object(&layer.limits)),
        (
            "volumetricAirFlowM3S",
            number(layer.volumetric_air_flow_m3_s),
        ),
        ("dryAirMassFlowKgS", number(layer.dry_air_mass_flow_kg_s)),
        ("waterMassFlowKgS", number(layer.water_mass_flow_kg_s)),
        ("waterLoadingKgM2S", number(layer.water_loading_kg_m2_s)),
        ("dryAirLoadingKgM2S", number(layer.dry_air_loading_kg_m2_s)),
        ("fillVelocityMS", number(layer.fill_velocity_ms)),
        ("pressureDropPa", number(layer.pressure_drop_pa)),
        ("merkelNumber", number(layer.merkel_number)),
        (
            "cumulativePressureDropPa",
            number(layer.cumulative_pressure_drop_pa),
        ),
        (
            "cumulativeMerkelNumber",
            number(layer.cumulative_merkel_number),
        ),
    ])
}

pub(crate) fn zone_object(zone: &ZoneCorrelation) -> String {
    object(&[
        ("coefficientPerM", number(zone.coefficient_per_m)),
        (
            "referenceWaterLoadingKgM2S",
            number(zone.reference_water_loading_kg_m2_s),
        ),
        (
            "referenceDryAirLoadingKgM2S",
            number(zone.reference_dry_air_loading_kg_m2_s),
        ),
        ("waterExponent", number(zone.water_exponent)),
        ("airExponent", number(zone.air_exponent)),
    ])
}

pub(crate) fn pressure_object(pressure: &FillPressureCorrelation) -> String {
    object(&[
        ("coefficientPaPerM", number(pressure.coefficient_pa_per_m)),
        (
            "referenceWaterLoadingKgM2S",
            number(pressure.reference_water_loading_kg_m2_s),
        ),
        (
            "referenceDryAirLoadingKgM2S",
            number(pressure.reference_dry_air_loading_kg_m2_s),
        ),
        ("waterExponent", number(pressure.water_exponent)),
        ("airExponent", number(pressure.air_exponent)),
    ])
}

pub(crate) fn limits_object(limits: &drafthouse_engine::FillLimits) -> String {
    object(&[
        (
            "minWaterLoadingKgM2S",
            number(limits.min_water_loading_kg_m2_s),
        ),
        (
            "maxWaterLoadingKgM2S",
            number(limits.max_water_loading_kg_m2_s),
        ),
        (
            "minDryAirLoadingKgM2S",
            number(limits.min_dry_air_loading_kg_m2_s),
        ),
        (
            "maxDryAirLoadingKgM2S",
            number(limits.max_dry_air_loading_kg_m2_s),
        ),
        (
            "maxWaterTemperatureC",
            number(limits.max_water_temperature_c),
        ),
    ])
}

pub(crate) fn air_state_object(state: &PsychrometricState) -> String {
    object(&[
        ("dryBulbC", number(state.dry_bulb_c)),
        ("wetBulbC", number(state.wet_bulb_c)),
        ("relativeHumidity", number(state.relative_humidity)),
        ("pressurePa", number(state.pressure_pa)),
        ("humidityRatio", number(state.humidity_ratio)),
        ("enthalpyKJkgDryAir", number(state.enthalpy_kj_kg_dry_air)),
        ("dryAirDensityKgM3", number(state.dry_air_density_kg_m3)),
        ("moistAirDensityKgM3", number(state.moist_air_density_kg_m3)),
        ("dewPointC", number(state.dew_point_c)),
        ("enhancementFactor", number(state.enhancement_factor)),
    ])
}

pub(crate) fn convention_name(convention: InletEnthalpyConvention) -> &'static str {
    match convention {
        InletEnthalpyConvention::Bulk => "bulk",
        InletEnthalpyConvention::CtiSaturatedWetBulb => "cti-saturated-wetbulb",
    }
}
