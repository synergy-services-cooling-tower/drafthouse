use crate::args::{parse_number, record_list, Cli, Spec};
use crate::Failure;
use drafthouse_engine::{
    CatalogMetadata, DriftCurvePoint, DriftEliminatorRecord, FanCurvePoint, FanPressureBasis,
    FanRecord, FillLayer, FillPressureCorrelation, FillRecord, FillStack, MerkelInput,
    NozzleRecord, RecordKind, SelectionCatalog, SelectionDriftEliminator, SelectionFan,
    SelectionFill, SelectionTower, TowerRecord, TowerType, WaterQualityFactor, ZoneCorrelation,
};
use std::collections::HashMap;

pub(crate) fn fill_stack(option: &str, raw: &str) -> Result<FillStack, Failure> {
    let mut layers = Vec::new();
    for entry in raw.split('+') {
        let (fill_id, rest) = entry.split_once('@').ok_or_else(|| {
            Failure::Usage(format!(
                "{option} expects fillId@depthM[@thermalMultiplier[@pressureMultiplier]] joined with +, got {entry:?}"
            ))
        })?;
        if fill_id.is_empty() {
            return Err(Failure::Usage(format!(
                "{option} needs a fill id before @, got {entry:?}"
            )));
        }
        let mut parts = rest.split('@');
        let depth = parts.next().unwrap_or_default();
        let thermal = parts.next();
        let pressure = parts.next();
        if depth.is_empty() || parts.next().is_some() {
            return Err(Failure::Usage(format!(
                "{option} expects fillId@depthM[@thermalMultiplier[@pressureMultiplier]] joined with +, got {entry:?}"
            )));
        }
        let mut layer = FillLayer::new(fill_id, parse_number(depth, option)?);
        if let Some(raw) = thermal {
            layer.thermal_multiplier = parse_number(raw, option)?;
        }
        if let Some(raw) = pressure {
            layer.pressure_multiplier = parse_number(raw, option)?;
        }
        layers.push(layer);
    }
    Ok(FillStack::new(layers))
}

pub(crate) fn rate_fill_records(cli: &Cli) -> Result<Vec<FillRecord>, Failure> {
    record_list(cli, "--fills")?
        .iter()
        .map(rate_fill_record)
        .collect()
}

pub(crate) fn rate_fill_record(spec: &Spec) -> Result<FillRecord, Failure> {
    spec.ensure_known(&SELECT_FILL_FIELDS)?;
    fill_physics(spec)
}

pub(crate) const ZONE_FIELDS: [&str; 5] = [
    "coefficientPerM",
    "referenceWaterLoadingKgM2S",
    "referenceDryAirLoadingKgM2S",
    "waterExponent",
    "airExponent",
];

pub(crate) const TOWER_FIELDS: [&str; 14] = [
    "fillAreaM2",
    "airFreeAreaM2",
    "driftAreaM2",
    "inletAreaM2",
    "plenumAreaM2",
    "fanStackAreaM2",
    "stackRecoveryFactor",
    "inletLossCoefficient",
    "distributionLossCoefficient",
    "supportLossCoefficient",
    "plenumLossCoefficient",
    "fixedPressureLossPa",
    "sprayZoneHeightM",
    "rainZoneHeightM",
];

pub(crate) const FILL_LIMIT_FIELDS: [&str; 5] = [
    "minWaterLoadingKgM2S",
    "maxWaterLoadingKgM2S",
    "minDryAirLoadingKgM2S",
    "maxDryAirLoadingKgM2S",
    "maxWaterTemperatureC",
];

pub(crate) const PRESSURE_FIELDS: [&str; 5] = [
    "coefficientPaPerM",
    "referenceWaterLoadingKgM2S",
    "referenceDryAirLoadingKgM2S",
    "waterExponent",
    "airExponent",
];

pub(crate) fn zone_correlation(spec: &Spec) -> Result<ZoneCorrelation, Failure> {
    spec.ensure_known(&ZONE_FIELDS)?;
    Ok(ZoneCorrelation {
        coefficient_per_m: spec.required("coefficientPerM")?,
        reference_water_loading_kg_m2_s: spec.required("referenceWaterLoadingKgM2S")?,
        reference_dry_air_loading_kg_m2_s: spec.required("referenceDryAirLoadingKgM2S")?,
        water_exponent: spec.required("waterExponent")?,
        air_exponent: spec.required("airExponent")?,
    })
}

pub(crate) fn tower_record(cli: &Cli) -> Result<TowerRecord, Failure> {
    let spec = Spec::parse(cli, "--tower")?;
    spec.ensure_known(&TOWER_FIELDS)?;
    let spray = Spec::parse(cli, "--spray")?;
    let rain = Spec::parse(cli, "--rain")?;
    let spray_zone = if spray.pairs.is_empty() {
        None
    } else {
        Some(zone_correlation(&spray)?)
    };
    let rain_zone = if rain.pairs.is_empty() {
        None
    } else {
        Some(zone_correlation(&rain)?)
    };
    Ok(TowerRecord {
        fill_area_m2: spec.required("fillAreaM2")?,
        air_free_area_m2: spec.number("airFreeAreaM2")?,
        drift_area_m2: spec.number("driftAreaM2")?,
        inlet_area_m2: spec.number("inletAreaM2")?,
        plenum_area_m2: spec.number("plenumAreaM2")?,
        fan_stack_area_m2: spec.number("fanStackAreaM2")?,
        stack_recovery_factor: spec.number("stackRecoveryFactor")?,
        inlet_loss_coefficient: spec.number("inletLossCoefficient")?,
        distribution_loss_coefficient: spec.number("distributionLossCoefficient")?,
        support_loss_coefficient: spec.number("supportLossCoefficient")?,
        plenum_loss_coefficient: spec.number("plenumLossCoefficient")?,
        fixed_pressure_loss_pa: spec.number("fixedPressureLossPa")?,
        spray_zone_height_m: spec.number("sprayZoneHeightM")?,
        spray_zone,
        rain_zone_height_m: spec.number("rainZoneHeightM")?,
        rain_zone,
    })
}

pub(crate) fn fill_record(cli: &Cli) -> Result<FillRecord, Failure> {
    let thermal_spec = Spec::parse(cli, "--fill-thermal")?;
    let pressure_spec = Spec::parse(cli, "--fill-pressure")?;
    let limits_spec = Spec::parse(cli, "--fill-limits")?;
    limits_spec.ensure_known(&FILL_LIMIT_FIELDS)?;
    let thermal = if thermal_spec.pairs.is_empty() {
        None
    } else {
        Some(zone_correlation(&thermal_spec)?)
    };
    let pressure = if pressure_spec.pairs.is_empty() {
        None
    } else {
        pressure_spec.ensure_known(&PRESSURE_FIELDS)?;
        Some(FillPressureCorrelation {
            coefficient_pa_per_m: pressure_spec.required("coefficientPaPerM")?,
            reference_water_loading_kg_m2_s: pressure_spec
                .required("referenceWaterLoadingKgM2S")?,
            reference_dry_air_loading_kg_m2_s: pressure_spec
                .required("referenceDryAirLoadingKgM2S")?,
            water_exponent: pressure_spec.required("waterExponent")?,
            air_exponent: pressure_spec.required("airExponent")?,
        })
    };
    let allowed_water_quality_classes = match cli.values.get("--fill-quality") {
        None => Vec::new(),
        Some(raw) => raw.split(',').map(str::to_string).collect(),
    };
    Ok(FillRecord {
        id: cli
            .values
            .get("--fill-id")
            .cloned()
            .unwrap_or_else(|| "fill".to_string()),
        thermal,
        pressure,
        limits: drafthouse_engine::FillLimits {
            min_water_loading_kg_m2_s: limits_spec.required("minWaterLoadingKgM2S")?,
            max_water_loading_kg_m2_s: limits_spec.required("maxWaterLoadingKgM2S")?,
            min_dry_air_loading_kg_m2_s: limits_spec.required("minDryAirLoadingKgM2S")?,
            max_dry_air_loading_kg_m2_s: limits_spec.required("maxDryAirLoadingKgM2S")?,
            max_water_temperature_c: limits_spec.required("maxWaterTemperatureC")?,
        },
        allowed_water_quality_classes,
    })
}

pub(crate) fn drift_record(cli: &Cli) -> Result<DriftEliminatorRecord, Failure> {
    let raw = cli
        .values
        .get("--drift-curve")
        .ok_or_else(|| Failure::Usage("airside needs --drift-curve".to_string()))?;
    let mut curve = Vec::new();
    for point in raw.split(',') {
        let fields: Vec<&str> = point.split(':').collect();
        let [face_velocity_ms, drift_ppm, pressure_drop_pa] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--drift-curve expects faceVelocityMS:driftPpm:pressureDropPa, got {point:?}"
            )));
        };
        curve.push(DriftCurvePoint {
            face_velocity_ms: parse_number(face_velocity_ms, "--drift-curve")?,
            drift_ppm: parse_number(drift_ppm, "--drift-curve")?,
            pressure_drop_pa: parse_number(pressure_drop_pa, "--drift-curve")?,
        });
    }
    Ok(DriftEliminatorRecord {
        id: cli
            .values
            .get("--drift-id")
            .cloned()
            .unwrap_or_else(|| "drift".to_string()),
        curve,
    })
}

pub(crate) fn fan_record(cli: &Cli) -> Result<Option<FanRecord>, Failure> {
    let Some(raw) = cli.values.get("--fan-curve") else {
        return Ok(None);
    };
    let mut curve = Vec::new();
    for point in raw.split(',') {
        let fields: Vec<&str> = point.split(':').collect();
        let [flow_m3_s, pressure_pa, efficiency] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--fan-curve expects flowM3S:pressurePa:efficiency, got {point:?}"
            )));
        };
        curve.push(FanCurvePoint {
            flow_m3_s: parse_number(flow_m3_s, "--fan-curve")?,
            pressure_pa: parse_number(pressure_pa, "--fan-curve")?,
            efficiency: parse_number(efficiency, "--fan-curve")?,
        });
    }
    let pressure_basis = match cli.values.get("--fan-basis").map(String::as_str) {
        None => None,
        Some("total") => Some(FanPressureBasis::Total),
        Some("static") => Some(FanPressureBasis::Static),
        Some(other) => {
            return Err(Failure::Usage(format!(
                "--fan-basis expects total or static, got {other:?}"
            )))
        }
    };
    Ok(Some(FanRecord {
        id: cli
            .values
            .get("--fan-id")
            .cloned()
            .unwrap_or_else(|| "fan".to_string()),
        stack_area_m2: cli.number("--fan-stack-area")?,
        pressure_basis,
        reference_density_kg_m3: cli.number("--fan-ref-density")?,
        stack_recovery_factor: cli.number("--fan-stack-recovery")?,
        curve,
    }))
}

pub(crate) const SELECT_TOWER_FIELDS: [&str; 23] = [
    "id",
    "type",
    "fillAreaM2",
    "airFreeAreaM2",
    "driftAreaM2",
    "inletAreaM2",
    "plenumAreaM2",
    "fanStackAreaM2",
    "stackRecoveryFactor",
    "inletLossCoefficient",
    "distributionLossCoefficient",
    "supportLossCoefficient",
    "plenumLossCoefficient",
    "fixedPressureLossPa",
    "sprayZoneHeightM",
    "rainZoneHeightM",
    "footprintM2",
    "maxWaterMassFlowKgS",
    "fillDepthOptionsM",
    "fillStacks",
    "compatibleFanIds",
    "sprayZone",
    "rainZone",
];

pub(crate) const SELECT_FILL_FIELDS: [&str; 6] = [
    "id",
    "compatibleTowerTypes",
    "allowedWaterQualityClasses",
    "thermal",
    "pressure",
    "limits",
];

pub(crate) const SELECT_DRIFT_FIELDS: [&str; 3] = ["id", "maxWaterTemperatureC", "curve"];

pub(crate) const SELECT_FAN_FIELDS: [&str; 10] = [
    "id",
    "stackAreaM2",
    "pressureBasis",
    "referenceDensityKgM3",
    "stackRecoveryFactor",
    "allowedSpeedRatio",
    "nominalRpm",
    "driveEfficiency",
    "motorEfficiency",
    "curve",
];

pub(crate) const SELECT_NOZZLE_FIELDS: [&str; 5] = [
    "id",
    "name",
    "dischargeCoefficient",
    "orificeDiameterM",
    "referenceWaterDensityKgM3",
];

pub(crate) fn zone_from_value(
    option: &str,
    key: &str,
    raw: &str,
) -> Result<ZoneCorrelation, Failure> {
    let parts: Vec<&str> = raw.split('|').collect();
    let [coefficient, water_loading, dry_air_loading, water_exponent, air_exponent] = parts[..]
    else {
        return Err(Failure::Usage(format!(
            "{option} {key} expects coefficientPerM|referenceWaterLoadingKgM2S|referenceDryAirLoadingKgM2S|waterExponent|airExponent, got {raw:?}"
        )));
    };
    Ok(ZoneCorrelation {
        coefficient_per_m: parse_number(coefficient, option)?,
        reference_water_loading_kg_m2_s: parse_number(water_loading, option)?,
        reference_dry_air_loading_kg_m2_s: parse_number(dry_air_loading, option)?,
        water_exponent: parse_number(water_exponent, option)?,
        air_exponent: parse_number(air_exponent, option)?,
    })
}

pub(crate) fn optional_zone(spec: &Spec, key: &str) -> Result<Option<ZoneCorrelation>, Failure> {
    match spec.get(key) {
        None => Ok(None),
        Some(raw) => Ok(Some(zone_from_value(&spec.option, key, raw)?)),
    }
}

pub(crate) fn selection_tower(spec: &Spec) -> Result<SelectionTower, Failure> {
    spec.ensure_known(&SELECT_TOWER_FIELDS)?;
    let tower_type = spec.get("type").and_then(TowerType::parse).ok_or_else(|| {
        Failure::Usage("--towers needs type counterflow or crossflow".to_string())
    })?;
    Ok(SelectionTower {
        id: spec.required_text("id")?,
        tower_type,
        physics: TowerRecord {
            fill_area_m2: spec.required("fillAreaM2")?,
            air_free_area_m2: spec.number("airFreeAreaM2")?,
            drift_area_m2: spec.number("driftAreaM2")?,
            inlet_area_m2: spec.number("inletAreaM2")?,
            plenum_area_m2: spec.number("plenumAreaM2")?,
            fan_stack_area_m2: spec.number("fanStackAreaM2")?,
            stack_recovery_factor: spec.number("stackRecoveryFactor")?,
            inlet_loss_coefficient: spec.number("inletLossCoefficient")?,
            distribution_loss_coefficient: spec.number("distributionLossCoefficient")?,
            support_loss_coefficient: spec.number("supportLossCoefficient")?,
            plenum_loss_coefficient: spec.number("plenumLossCoefficient")?,
            fixed_pressure_loss_pa: spec.number("fixedPressureLossPa")?,
            spray_zone_height_m: spec.number("sprayZoneHeightM")?,
            spray_zone: optional_zone(spec, "sprayZone")?,
            rain_zone_height_m: spec.number("rainZoneHeightM")?,
            rain_zone: optional_zone(spec, "rainZone")?,
        },
        footprint_m2: spec.required("footprintM2")?,
        max_water_mass_flow_kg_s: spec.required("maxWaterMassFlowKgS")?,
        fill_depth_options_m: spec
            .required_list("fillDepthOptionsM")?
            .iter()
            .map(|part| parse_number(part, "--towers fillDepthOptionsM"))
            .collect::<Result<Vec<f64>, Failure>>()?,
        fill_stacks: spec
            .list("fillStacks")
            .iter()
            .map(|entry| fill_stack("--towers fillStacks", entry))
            .collect::<Result<Vec<FillStack>, Failure>>()?,
        compatible_fan_ids: spec.required_list("compatibleFanIds")?,
    })
}

pub(crate) fn selection_fill(spec: &Spec) -> Result<SelectionFill, Failure> {
    spec.ensure_known(&SELECT_FILL_FIELDS)?;
    let compatible_tower_types = spec
        .required_list("compatibleTowerTypes")?
        .iter()
        .map(|raw| {
            TowerType::parse(raw).ok_or_else(|| {
                Failure::Usage(format!(
                    "--fills compatibleTowerTypes expects counterflow or crossflow, got {raw:?}"
                ))
            })
        })
        .collect::<Result<Vec<TowerType>, Failure>>()?;
    let physics = fill_physics(spec)?;
    Ok(SelectionFill {
        physics,
        compatible_tower_types,
    })
}

pub(crate) fn fill_physics(spec: &Spec) -> Result<FillRecord, Failure> {
    let limits = spec.required_text("limits")?;
    let limits_parts: Vec<&str> = limits.split('|').collect();
    let [min_water, max_water, min_dry_air, max_dry_air, max_temperature] = limits_parts[..] else {
        return Err(Failure::Usage(format!(
            "{} limits expects minWaterLoadingKgM2S|maxWaterLoadingKgM2S|minDryAirLoadingKgM2S|maxDryAirLoadingKgM2S|maxWaterTemperatureC, got {limits:?}",
            spec.option
        )));
    };
    let pressure = spec.required_text("pressure")?;
    let pressure_parts: Vec<&str> = pressure.split('|').collect();
    let [coefficient, water_loading, dry_air_loading, water_exponent, air_exponent] =
        pressure_parts[..]
    else {
        return Err(Failure::Usage(format!(
            "{} pressure expects coefficientPaPerM|referenceWaterLoadingKgM2S|referenceDryAirLoadingKgM2S|waterExponent|airExponent, got {pressure:?}",
            spec.option
        )));
    };
    let pressure_option = format!("{} pressure", spec.option);
    let limits_option = format!("{} limits", spec.option);
    Ok(FillRecord {
        id: spec.required_text("id")?,
        thermal: Some(zone_from_value(
            &spec.option,
            "thermal",
            &spec.required_text("thermal")?,
        )?),
        pressure: Some(FillPressureCorrelation {
            coefficient_pa_per_m: parse_number(coefficient, &pressure_option)?,
            reference_water_loading_kg_m2_s: parse_number(water_loading, &pressure_option)?,
            reference_dry_air_loading_kg_m2_s: parse_number(dry_air_loading, &pressure_option)?,
            water_exponent: parse_number(water_exponent, &pressure_option)?,
            air_exponent: parse_number(air_exponent, &pressure_option)?,
        }),
        limits: drafthouse_engine::FillLimits {
            min_water_loading_kg_m2_s: parse_number(min_water, &limits_option)?,
            max_water_loading_kg_m2_s: parse_number(max_water, &limits_option)?,
            min_dry_air_loading_kg_m2_s: parse_number(min_dry_air, &limits_option)?,
            max_dry_air_loading_kg_m2_s: parse_number(max_dry_air, &limits_option)?,
            max_water_temperature_c: parse_number(max_temperature, &limits_option)?,
        },
        allowed_water_quality_classes: spec.list("allowedWaterQualityClasses"),
    })
}

pub(crate) fn selection_drift_eliminator(spec: &Spec) -> Result<SelectionDriftEliminator, Failure> {
    spec.ensure_known(&SELECT_DRIFT_FIELDS)?;
    let mut curve = Vec::new();
    for point in spec.required_list("curve")? {
        let fields: Vec<&str> = point.split(':').collect();
        let [face_velocity_ms, drift_ppm, pressure_drop_pa] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--drift-eliminators curve expects faceVelocityMS:driftPpm:pressureDropPa, got {point:?}"
            )));
        };
        curve.push(DriftCurvePoint {
            face_velocity_ms: parse_number(face_velocity_ms, "--drift-eliminators curve")?,
            drift_ppm: parse_number(drift_ppm, "--drift-eliminators curve")?,
            pressure_drop_pa: parse_number(pressure_drop_pa, "--drift-eliminators curve")?,
        });
    }
    Ok(SelectionDriftEliminator {
        physics: DriftEliminatorRecord {
            id: spec.required_text("id")?,
            curve,
        },
        max_water_temperature_c: spec.required("maxWaterTemperatureC")?,
    })
}

pub(crate) fn selection_fan(spec: &Spec) -> Result<SelectionFan, Failure> {
    spec.ensure_known(&SELECT_FAN_FIELDS)?;
    let mut curve = Vec::new();
    for point in spec.required_list("curve")? {
        let fields: Vec<&str> = point.split(':').collect();
        let [flow_m3_s, pressure_pa, efficiency] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--fans curve expects flowM3S:pressurePa:efficiency, got {point:?}"
            )));
        };
        curve.push(FanCurvePoint {
            flow_m3_s: parse_number(flow_m3_s, "--fans curve")?,
            pressure_pa: parse_number(pressure_pa, "--fans curve")?,
            efficiency: parse_number(efficiency, "--fans curve")?,
        });
    }
    let speed_ratio = spec.required_list("allowedSpeedRatio")?;
    let [lower_speed_ratio, upper_speed_ratio] = speed_ratio.as_slice() else {
        return Err(Failure::Usage(format!(
            "--fans allowedSpeedRatio expects lower|upper, got {:?}",
            speed_ratio.join("|")
        )));
    };
    let pressure_basis = match spec.get("pressureBasis") {
        None => None,
        Some("total") => Some(FanPressureBasis::Total),
        Some("static") => Some(FanPressureBasis::Static),
        Some(other) => {
            return Err(Failure::Usage(format!(
                "--fans pressureBasis expects total or static, got {other:?}"
            )))
        }
    };
    Ok(SelectionFan {
        physics: FanRecord {
            id: spec.required_text("id")?,
            stack_area_m2: spec.number("stackAreaM2")?,
            pressure_basis,
            reference_density_kg_m3: spec.number("referenceDensityKgM3")?,
            stack_recovery_factor: spec.number("stackRecoveryFactor")?,
            curve,
        },
        allowed_speed_ratio: [
            parse_number(lower_speed_ratio, "--fans allowedSpeedRatio")?,
            parse_number(upper_speed_ratio, "--fans allowedSpeedRatio")?,
        ],
        nominal_rpm: spec.number("nominalRpm")?,
        drive_efficiency: spec.required("driveEfficiency")?,
        motor_efficiency: spec.required("motorEfficiency")?,
    })
}

pub(crate) fn selection_nozzle(spec: &Spec) -> Result<NozzleRecord, Failure> {
    spec.ensure_known(&SELECT_NOZZLE_FIELDS)?;
    Ok(NozzleRecord {
        id: spec.required_text("id")?,
        name: spec.required_text("name")?,
        discharge_coefficient: spec.required("dischargeCoefficient")?,
        orifice_diameter_m: spec.required("orificeDiameterM")?,
        reference_water_density_kg_m3: spec.number("referenceWaterDensityKgM3")?,
    })
}

pub(crate) fn quality_factors(cli: &Cli) -> Result<HashMap<String, WaterQualityFactor>, Failure> {
    let raw = cli
        .values
        .get("--quality-factors")
        .ok_or_else(|| Failure::Usage("select needs --quality-factors".to_string()))?;
    let mut factors = HashMap::new();
    for entry in raw.split(';') {
        let fields: Vec<&str> = entry.split(':').collect();
        let [name, thermal_multiplier, pressure_multiplier] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--quality-factors expects name:thermalMultiplier:pressureMultiplier, got {entry:?}"
            )));
        };
        factors.insert(
            name.to_string(),
            WaterQualityFactor {
                thermal_multiplier: parse_number(thermal_multiplier, "--quality-factors")?,
                pressure_multiplier: parse_number(pressure_multiplier, "--quality-factors")?,
            },
        );
    }
    Ok(factors)
}

pub(crate) struct CatalogList {
    pub(crate) option: &'static str,
    pub(crate) key: &'static str,
    pub(crate) kind: RecordKind,
}

pub(crate) const CATALOG_LISTS: [CatalogList; 5] = [
    CatalogList {
        option: "--towers",
        key: "towers",
        kind: RecordKind::Tower,
    },
    CatalogList {
        option: "--fills",
        key: "fills",
        kind: RecordKind::Fill,
    },
    CatalogList {
        option: "--drift-eliminators",
        key: "driftEliminators",
        kind: RecordKind::DriftEliminator,
    },
    CatalogList {
        option: "--fans",
        key: "fans",
        kind: RecordKind::Fan,
    },
    CatalogList {
        option: "--nozzles",
        key: "nozzles",
        kind: RecordKind::Nozzle,
    },
];

pub(crate) fn validate_catalog_records(cli: &Cli) -> Result<[usize; 5], Failure> {
    let mut counts = [0_usize; 5];
    let mut violations = Vec::new();
    for (count, list) in counts.iter_mut().zip(CATALOG_LISTS.iter()) {
        let specs = record_list(cli, list.option)?;
        *count = specs.len();
        for (position, spec) in specs.iter().enumerate() {
            let label = spec
                .get("id")
                .map_or_else(|| format!("#{}", position + 1), str::to_string);
            let pairs: Vec<(&str, &str)> = spec
                .pairs
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            if let Err(mut refused) = drafthouse_engine::validate_record(list.kind, &label, &pairs)
            {
                violations.append(&mut refused);
            }
        }
    }
    if violations.is_empty() {
        Ok(counts)
    } else {
        Err(Failure::Usage(
            violations
                .iter()
                .map(drafthouse_engine::Violation::message)
                .collect::<Vec<String>>()
                .join("\n"),
        ))
    }
}

pub(crate) fn selection_catalog(cli: &Cli) -> Result<SelectionCatalog, Failure> {
    let towers = record_list(cli, "--towers")?
        .iter()
        .map(selection_tower)
        .collect::<Result<Vec<SelectionTower>, Failure>>()?;
    let fills = record_list(cli, "--fills")?
        .iter()
        .map(selection_fill)
        .collect::<Result<Vec<SelectionFill>, Failure>>()?;
    let drift_eliminators = record_list(cli, "--drift-eliminators")?
        .iter()
        .map(selection_drift_eliminator)
        .collect::<Result<Vec<SelectionDriftEliminator>, Failure>>()?;
    let fans = record_list(cli, "--fans")?
        .iter()
        .map(selection_fan)
        .collect::<Result<Vec<SelectionFan>, Failure>>()?;
    let nozzles = record_list(cli, "--nozzles")?
        .iter()
        .map(selection_nozzle)
        .collect::<Result<Vec<NozzleRecord>, Failure>>()?;
    Ok(SelectionCatalog {
        metadata: CatalogMetadata {
            id: cli.values.get("--catalog-id").cloned().unwrap_or_default(),
            revision: cli
                .values
                .get("--catalog-revision")
                .cloned()
                .unwrap_or_default(),
            status: cli
                .values
                .get("--catalog-status")
                .cloned()
                .unwrap_or_default(),
        },
        water_quality_factors: quality_factors(cli)?,
        towers,
        fills,
        drift_eliminators,
        fans,
        nozzles,
    })
}

pub(crate) fn merkel_input(cli: &Cli) -> Result<MerkelInput, Failure> {
    let base = MerkelInput::new(
        cli.required_number("--hot")?,
        cli.required_number("--cold")?,
        cli.required_number("--wb")?,
        cli.required_number("--db")?,
        cli.required_number("--lg")?,
    );
    Ok(MerkelInput {
        pressure_pa: cli.number_or("--p", 101_325.0)?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        integration: cli.integration()?,
        segments: cli.integer("--segments", base.segments)?,
        inlet_enthalpy_convention: cli.convention()?,
        ..base
    })
}
