use crate::fan::{FanPressureBasis, FanRecord};
use crate::layers::{FillLayerResult, LayerTerms};
use crate::numeric::{assert_positive, interpolate_1d, DomainError};

/// One heat/mass-transfer correlation, the shared shape of `tower.sprayZone`, `tower.rainZone` and `fill.thermal`:
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneCorrelation {
    pub coefficient_per_m: f64,
    pub reference_water_loading_kg_m2_s: f64,
    pub reference_dry_air_loading_kg_m2_s: f64,
    pub water_exponent: f64,
    pub air_exponent: f64,
}

/// The fill pressure-drop correlation (`fill.pressure`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillPressureCorrelation {
    pub coefficient_pa_per_m: f64,
    pub reference_water_loading_kg_m2_s: f64,
    pub reference_dry_air_loading_kg_m2_s: f64,
    pub water_exponent: f64,
    pub air_exponent: f64,
}

/// The fill operating limits (`fill.limits`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillLimits {
    pub min_water_loading_kg_m2_s: f64,
    pub max_water_loading_kg_m2_s: f64,
    pub min_dry_air_loading_kg_m2_s: f64,
    pub max_dry_air_loading_kg_m2_s: f64,
    pub max_water_temperature_c: f64,
}

/// A validated fill catalog record, reduced to the fields this slice reads.
#[derive(Clone, Debug, PartialEq)]
pub struct FillRecord {
    pub id: String,
    pub thermal: Option<ZoneCorrelation>,
    pub pressure: Option<FillPressureCorrelation>,
    pub limits: FillLimits,
    pub allowed_water_quality_classes: Vec<String>,
}

/// A validated tower catalog record, reduced to the fields this slice reads.
#[derive(Clone, Debug, PartialEq)]
pub struct TowerRecord {
    pub fill_area_m2: f64,
    pub air_free_area_m2: Option<f64>,
    pub drift_area_m2: Option<f64>,
    pub inlet_area_m2: Option<f64>,
    pub plenum_area_m2: Option<f64>,
    pub fan_stack_area_m2: Option<f64>,
    pub stack_recovery_factor: Option<f64>,
    pub inlet_loss_coefficient: Option<f64>,
    pub distribution_loss_coefficient: Option<f64>,
    pub support_loss_coefficient: Option<f64>,
    pub plenum_loss_coefficient: Option<f64>,
    pub fixed_pressure_loss_pa: Option<f64>,
    pub spray_zone_height_m: Option<f64>,
    pub spray_zone: Option<ZoneCorrelation>,
    pub rain_zone_height_m: Option<f64>,
    pub rain_zone: Option<ZoneCorrelation>,
}

/// One point of a drift-eliminator curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftCurvePoint {
    pub face_velocity_ms: f64,
    pub drift_ppm: f64,
    pub pressure_drop_pa: f64,
}

/// A validated drift-eliminator catalog record.
#[derive(Clone, Debug, PartialEq)]
pub struct DriftEliminatorRecord {
    pub id: String,
    pub curve: Vec<DriftCurvePoint>,
}

/// The drift performance at one face velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftPerformance {
    pub drift_ppm: f64,
    pub pressure_drop_pa: f64,
}

/// The flow areas the airside model resolves, mirroring the JavaScript return object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirsideAreas {
    pub fill_area_m2: f64,
    pub air_free_area_m2: f64,
    pub drift_area_m2: f64,
    pub inlet_area_m2: f64,
    pub plenum_area_m2: f64,
    pub fan_stack_area_m2: Option<f64>,
}

/// Port of `velocityPressurePa`.
pub fn velocity_pressure_pa(air_density_kg_m3: f64, velocity_ms: f64) -> f64 {
    0.5 * air_density_kg_m3 * velocity_ms.powi(2)
}

/// Port of `minorLossPressurePa`.
pub fn minor_loss_pressure_pa(
    loss_coefficient: f64,
    air_density_kg_m3: f64,
    velocity_ms: f64,
) -> f64 {
    loss_coefficient * velocity_pressure_pa(air_density_kg_m3, velocity_ms)
}

/// Port of `zoneMerkelNumber`.
pub fn zone_merkel_number(
    zone: Option<&ZoneCorrelation>,
    height_m: f64,
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
    thermal_multiplier: f64,
) -> Result<f64, DomainError> {
    let Some(zone) = zone else {
        return Ok(0.0);
    };
    assert_positive(height_m, "zone heightM")?;
    let per_meter = zone.coefficient_per_m
        * (water_loading_kg_m2_s / zone.reference_water_loading_kg_m2_s).powf(zone.water_exponent)
        * (dry_air_loading_kg_m2_s / zone.reference_dry_air_loading_kg_m2_s)
            .powf(zone.air_exponent);
    Ok(per_meter * height_m * thermal_multiplier)
}

/// Port of `fillThermalMerkelNumber`.
pub fn fill_thermal_merkel_number(
    fill: &FillRecord,
    depth_m: f64,
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
    thermal_multiplier: f64,
) -> Result<f64, DomainError> {
    assert_positive(depth_m, "depthM")?;
    let thermal = fill
        .thermal
        .as_ref()
        .ok_or_else(|| DomainError::new("Fill thermal correlation is missing."))?;
    zone_merkel_number(
        Some(thermal),
        depth_m,
        water_loading_kg_m2_s,
        dry_air_loading_kg_m2_s,
        thermal_multiplier,
    )
}

/// Port of `fillPressureDropPa`.
pub fn fill_pressure_drop_pa(
    fill: &FillRecord,
    depth_m: f64,
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
    pressure_multiplier: f64,
) -> Result<f64, DomainError> {
    assert_positive(depth_m, "depthM")?;
    let pressure = fill
        .pressure
        .as_ref()
        .ok_or_else(|| DomainError::new("Fill pressure correlation is missing."))?;
    let per_meter_pa = pressure.coefficient_pa_per_m
        * (water_loading_kg_m2_s / pressure.reference_water_loading_kg_m2_s)
            .powf(pressure.water_exponent)
        * (dry_air_loading_kg_m2_s / pressure.reference_dry_air_loading_kg_m2_s)
            .powf(pressure.air_exponent);
    Ok(per_meter_pa * depth_m * pressure_multiplier)
}

/// Port of `driftPerformanceAtVelocity`: both curve readings are extrapolated linearly outside the tabulated range (`clampEnds: false`), so a velocity beyond the curve is not an error.
pub fn drift_performance_at_velocity(
    drift_eliminator: &DriftEliminatorRecord,
    face_velocity_ms: f64,
) -> Result<DriftPerformance, DomainError> {
    let drift_ppm = interpolate_1d(
        &drift_eliminator.curve,
        face_velocity_ms,
        |point| point.face_velocity_ms,
        |point| point.drift_ppm,
        false,
    )?;
    let pressure_drop_pa = interpolate_1d(
        &drift_eliminator.curve,
        face_velocity_ms,
        |point| point.face_velocity_ms,
        |point| point.pressure_drop_pa,
        false,
    )?;
    Ok(DriftPerformance {
        drift_ppm,
        pressure_drop_pa,
    })
}

/// Port of `resolveAirsideAreas`.
pub fn resolve_airside_areas(tower: &TowerRecord, fan: Option<&FanRecord>) -> AirsideAreas {
    let fill_area_m2 = tower.fill_area_m2;
    let air_free_area_m2 = tower.air_free_area_m2.unwrap_or(fill_area_m2);
    let fan_stack_area_m2 = fan
        .and_then(|fan| fan.stack_area_m2)
        .or(tower.fan_stack_area_m2);
    AirsideAreas {
        fill_area_m2,
        air_free_area_m2,
        drift_area_m2: tower.drift_area_m2.unwrap_or(fill_area_m2),
        inlet_area_m2: tower.inlet_area_m2.unwrap_or(air_free_area_m2),
        plenum_area_m2: tower
            .plenum_area_m2
            .or(fan_stack_area_m2)
            .unwrap_or(air_free_area_m2),
        fan_stack_area_m2,
    }
}

/// Input to [`fan_stack_discharge_pressure_pa`].
#[derive(Clone, Copy, Debug)]
pub struct FanStackDischargeInput {
    pub volumetric_air_flow_m3_s: f64,
    pub fan_stack_area_m2: Option<f64>,
    pub moist_air_density_kg_m3: f64,
    /// 0 is a plain cylindrical stack; 0.3–0.5 is typical for a velocity-recovery stack.
    pub stack_recovery_factor: f64,
    /// `'total'` (the default) charges the outlet velocity pressure against the curve; `'static'` has already netted it out and returns zero.
    pub fan_pressure_basis: FanPressureBasis,
}

/// The discharge term, mirroring the JavaScript return object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanStackDischarge {
    pub pressure_pa: f64,
    /// `None` when the curve is declared static, so no stack velocity is resolved.
    pub stack_velocity_ms: Option<f64>,
    pub basis: FanPressureBasis,
}

/// Port of `fanStackDischargePressurePa`.
pub fn fan_stack_discharge_pressure_pa(
    input: FanStackDischargeInput,
) -> Result<FanStackDischarge, DomainError> {
    if input.fan_pressure_basis == FanPressureBasis::Static {
        return Ok(FanStackDischarge {
            pressure_pa: 0.0,
            stack_velocity_ms: None,
            basis: input.fan_pressure_basis,
        });
    }
    let Some(fan_stack_area_m2) = input.fan_stack_area_m2 else {
        return Err(missing_fan_stack_area());
    };
    if fan_stack_area_m2.is_nan() || fan_stack_area_m2 <= 0.0 {
        return Err(missing_fan_stack_area());
    }
    if input.stack_recovery_factor < 0.0 || input.stack_recovery_factor >= 1.0 {
        return Err(DomainError::new("stackRecoveryFactor must be in [0, 1)."));
    }
    let stack_velocity_ms = input.volumetric_air_flow_m3_s / fan_stack_area_m2;
    Ok(FanStackDischarge {
        pressure_pa: (1.0 - input.stack_recovery_factor)
            * velocity_pressure_pa(input.moist_air_density_kg_m3, stack_velocity_ms),
        stack_velocity_ms: Some(stack_velocity_ms),
        basis: input.fan_pressure_basis,
    })
}

fn missing_fan_stack_area() -> DomainError {
    DomainError::new(
        "A fan-stack area is required to charge the discharge velocity pressure against a \
         total-pressure fan curve. Supply tower.fanStackAreaM2 or fan.stackAreaM2, or declare \
         the curve as fanPressureBasis \"static\".",
    )
}

/// Input to [`system_pressure_breakdown`], mirroring the JavaScript options object.
pub struct SystemPressureBreakdownInput<'a> {
    pub tower: &'a TowerRecord,
    pub fill: &'a FillRecord,
    pub fill_depth_m: f64,
    pub drift_eliminator: &'a DriftEliminatorRecord,
    pub fan: Option<&'a FanRecord>,
    pub volumetric_air_flow_m3_s: f64,
    pub dry_air_density_kg_m3: f64,
    pub moist_air_density_kg_m3: f64,
    pub water_mass_flow_kg_s: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

/// The full pressure breakdown, mirroring the JavaScript return object field for field.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemPressureBreakdown {
    pub total_pa: f64,
    pub fill_pa: f64,
    pub drift_pa: f64,
    pub inlet_pa: f64,
    pub distribution_pa: f64,
    pub support_pa: f64,
    pub plenum_pa: f64,
    pub fan_stack_pa: f64,
    pub fixed_pa: f64,
    pub drift_ppm: f64,
    pub water_loading_kg_m2_s: f64,
    pub dry_air_loading_kg_m2_s: f64,
    pub dry_air_mass_flow_kg_s: f64,
    pub fill_velocity_ms: f64,
    pub drift_velocity_ms: f64,
    pub inlet_velocity_ms: f64,
    pub plenum_velocity_ms: f64,
    pub fan_stack_velocity_ms: Option<f64>,
    pub fan_pressure_basis: FanPressureBasis,
    pub areas: AirsideAreas,
    pub fill_merkel_number: f64,
    pub spray_zone_merkel_number: f64,
    pub rain_zone_merkel_number: f64,
    pub available_merkel_number: f64,
}

/// Input to [`layered_system_pressure_breakdown`]: the air-side machinery plus the **resolved layers**, in physical order (top first).
pub struct LayeredBreakdownInput<'a> {
    pub tower: &'a TowerRecord,
    pub layers: &'a [LayerTerms<'a>],
    pub drift_eliminator: &'a DriftEliminatorRecord,
    pub fan: Option<&'a FanRecord>,
    pub volumetric_air_flow_m3_s: f64,
    pub dry_air_density_kg_m3: f64,
    pub moist_air_density_kg_m3: f64,
    pub water_mass_flow_kg_s: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

/// The layered result: the breakdown [`system_pressure_breakdown`] reports — its fill terms are the stack's totals — plus one entry per layer, in physical order.
#[derive(Clone, Debug, PartialEq)]
pub struct LayeredBreakdown {
    pub breakdown: SystemPressureBreakdown,
    pub fill_layers: Vec<FillLayerResult>,
}

/// Port of `systemPressureBreakdown` for an **ordered fill stack**.
pub fn layered_system_pressure_breakdown(
    input: LayeredBreakdownInput<'_>,
) -> Result<LayeredBreakdown, DomainError> {
    assert_positive(input.volumetric_air_flow_m3_s, "volumetricAirFlowM3S")?;
    if input.layers.is_empty() {
        return Err(crate::layers::empty_stack_error());
    }
    let areas = resolve_airside_areas(input.tower, input.fan);
    let AirsideAreas {
        fill_area_m2,
        air_free_area_m2,
        drift_area_m2,
        inlet_area_m2,
        plenum_area_m2,
        fan_stack_area_m2,
    } = areas;

    let water_loading_kg_m2_s = input.water_mass_flow_kg_s / fill_area_m2;
    let dry_air_mass_flow_kg_s = input.volumetric_air_flow_m3_s * input.dry_air_density_kg_m3;
    let dry_air_loading_kg_m2_s = dry_air_mass_flow_kg_s / air_free_area_m2;
    let fill_velocity_ms = input.volumetric_air_flow_m3_s / air_free_area_m2;
    let drift_velocity_ms = input.volumetric_air_flow_m3_s / drift_area_m2;
    let inlet_velocity_ms = input.volumetric_air_flow_m3_s / inlet_area_m2;
    let plenum_velocity_ms = input.volumetric_air_flow_m3_s / plenum_area_m2;

    let mut fill_pa = 0.0;
    let mut layer_pressure_drop_pa = Vec::with_capacity(input.layers.len());
    for terms in input.layers {
        let drop_pa = fill_pressure_drop_pa(
            terms.fill,
            terms.depth_m,
            water_loading_kg_m2_s,
            dry_air_loading_kg_m2_s,
            terms.pressure_multiplier * input.pressure_multiplier,
        )?;
        fill_pa += drop_pa;
        layer_pressure_drop_pa.push(drop_pa);
    }
    let drift = drift_performance_at_velocity(input.drift_eliminator, drift_velocity_ms)?;

    let inlet_pa = minor_loss_pressure_pa(
        input.tower.inlet_loss_coefficient.unwrap_or(1.2),
        input.moist_air_density_kg_m3,
        inlet_velocity_ms,
    );
    let distribution_pa = minor_loss_pressure_pa(
        input.tower.distribution_loss_coefficient.unwrap_or(0.8),
        input.moist_air_density_kg_m3,
        fill_velocity_ms,
    );
    let support_pa = minor_loss_pressure_pa(
        input.tower.support_loss_coefficient.unwrap_or(0.6),
        input.moist_air_density_kg_m3,
        fill_velocity_ms,
    );
    let plenum_pa = minor_loss_pressure_pa(
        input.tower.plenum_loss_coefficient.unwrap_or(0.7),
        input.moist_air_density_kg_m3,
        plenum_velocity_ms,
    );
    let stack = fan_stack_discharge_pressure_pa(FanStackDischargeInput {
        volumetric_air_flow_m3_s: input.volumetric_air_flow_m3_s,
        fan_stack_area_m2,
        moist_air_density_kg_m3: input.moist_air_density_kg_m3,
        stack_recovery_factor: input
            .tower
            .stack_recovery_factor
            .or(input.fan.and_then(|fan| fan.stack_recovery_factor))
            .unwrap_or(0.0),
        fan_pressure_basis: match input.fan {
            Some(fan) => fan.pressure_basis.unwrap_or(FanPressureBasis::Total),
            None => FanPressureBasis::Static,
        },
    })?;
    let fixed_pa = input.tower.fixed_pressure_loss_pa.unwrap_or(0.0);
    let total_pa = fill_pa
        + drift.pressure_drop_pa
        + inlet_pa
        + distribution_pa
        + support_pa
        + plenum_pa
        + stack.pressure_pa
        + fixed_pa;

    let mut fill_merkel_number = 0.0;
    let mut layer_merkel_number = Vec::with_capacity(input.layers.len());
    for terms in input.layers {
        let merkel_number = fill_thermal_merkel_number(
            terms.fill,
            terms.depth_m,
            water_loading_kg_m2_s,
            dry_air_loading_kg_m2_s,
            terms.thermal_multiplier * input.thermal_multiplier,
        )?;
        fill_merkel_number += merkel_number;
        layer_merkel_number.push(merkel_number);
    }
    let spray_zone_merkel_number = zone_merkel_number(
        input.tower.spray_zone.as_ref(),
        input.tower.spray_zone_height_m.unwrap_or(0.0),
        water_loading_kg_m2_s,
        dry_air_loading_kg_m2_s,
        input.thermal_multiplier,
    )?;
    let rain_zone_merkel_number = zone_merkel_number(
        input.tower.rain_zone.as_ref(),
        input.tower.rain_zone_height_m.unwrap_or(0.0),
        water_loading_kg_m2_s,
        dry_air_loading_kg_m2_s,
        input.thermal_multiplier,
    )?;

    let mut cumulative_pressure_drop_pa = 0.0;
    let mut cumulative_merkel_number = 0.0;
    let mut fill_layers = Vec::with_capacity(input.layers.len());
    for (index, terms) in input.layers.iter().enumerate() {
        cumulative_pressure_drop_pa += layer_pressure_drop_pa[index];
        cumulative_merkel_number += layer_merkel_number[index];
        fill_layers.push(FillLayerResult {
            position: index + 1,
            fill_id: terms.fill.id.clone(),
            depth_m: terms.depth_m,
            thermal_multiplier: terms.thermal_multiplier,
            pressure_multiplier: terms.pressure_multiplier,
            effective_thermal_multiplier: terms.thermal_multiplier * input.thermal_multiplier,
            effective_pressure_multiplier: terms.pressure_multiplier * input.pressure_multiplier,
            thermal: terms
                .fill
                .thermal
                .expect("fill_thermal_merkel_number resolves the thermal correlation"),
            pressure: terms
                .fill
                .pressure
                .expect("fill_pressure_drop_pa resolves the pressure correlation"),
            limits: terms.fill.limits,
            volumetric_air_flow_m3_s: input.volumetric_air_flow_m3_s,
            dry_air_mass_flow_kg_s,
            water_mass_flow_kg_s: input.water_mass_flow_kg_s,
            water_loading_kg_m2_s,
            dry_air_loading_kg_m2_s,
            fill_velocity_ms,
            pressure_drop_pa: layer_pressure_drop_pa[index],
            merkel_number: layer_merkel_number[index],
            cumulative_pressure_drop_pa,
            cumulative_merkel_number,
        });
    }

    Ok(LayeredBreakdown {
        breakdown: SystemPressureBreakdown {
            total_pa,
            fill_pa,
            drift_pa: drift.pressure_drop_pa,
            inlet_pa,
            distribution_pa,
            support_pa,
            plenum_pa,
            fan_stack_pa: stack.pressure_pa,
            fixed_pa,
            drift_ppm: drift.drift_ppm,
            water_loading_kg_m2_s,
            dry_air_loading_kg_m2_s,
            dry_air_mass_flow_kg_s,
            fill_velocity_ms,
            drift_velocity_ms,
            inlet_velocity_ms,
            plenum_velocity_ms,
            fan_stack_velocity_ms: stack.stack_velocity_ms,
            fan_pressure_basis: stack.basis,
            areas,
            fill_merkel_number,
            spray_zone_merkel_number,
            rain_zone_merkel_number,
            available_merkel_number: fill_merkel_number
                + spray_zone_merkel_number
                + rain_zone_merkel_number,
        },
        fill_layers,
    })
}

/// Port of `systemPressureBreakdown`.
pub fn system_pressure_breakdown(
    input: SystemPressureBreakdownInput<'_>,
) -> Result<SystemPressureBreakdown, DomainError> {
    let layers = [LayerTerms::of_fill(input.fill, input.fill_depth_m)];
    Ok(layered_system_pressure_breakdown(LayeredBreakdownInput {
        tower: input.tower,
        layers: &layers,
        drift_eliminator: input.drift_eliminator,
        fan: input.fan,
        volumetric_air_flow_m3_s: input.volumetric_air_flow_m3_s,
        dry_air_density_kg_m3: input.dry_air_density_kg_m3,
        moist_air_density_kg_m3: input.moist_air_density_kg_m3,
        water_mass_flow_kg_s: input.water_mass_flow_kg_s,
        thermal_multiplier: input.thermal_multiplier,
        pressure_multiplier: input.pressure_multiplier,
    })?
    .breakdown)
}

/// Input to [`check_fill_operating_envelope`].
pub struct FillOperatingEnvelopeInput<'a> {
    pub fill: &'a FillRecord,
    pub water_loading_kg_m2_s: f64,
    pub dry_air_loading_kg_m2_s: f64,
    pub hot_water_c: f64,
    pub water_quality_class: &'a str,
}

/// The envelope verdict, mirroring the JavaScript `{ ok, failures }` object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillOperatingEnvelope {
    pub ok: bool,
    pub failures: Vec<String>,
}

/// Port of `checkFillOperatingEnvelope`.
pub fn check_fill_operating_envelope(
    input: FillOperatingEnvelopeInput<'_>,
) -> FillOperatingEnvelope {
    let failures = envelope_failures(
        input.fill,
        input.water_loading_kg_m2_s,
        input.dry_air_loading_kg_m2_s,
        Some(input.hot_water_c),
        Some(input.water_quality_class),
    );
    FillOperatingEnvelope {
        ok: failures.is_empty(),
        failures,
    }
}

/// The ported envelope's own failure texts, with the two dimensions a run may not be making a claim about optional: `None` skips that check instead of inventing a value for it (a run that declares no hot-water temperature has nothing to compare against the fill's limit).
pub fn envelope_failures(
    fill: &FillRecord,
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
    hot_water_c: Option<f64>,
    water_quality_class: Option<&str>,
) -> Vec<String> {
    let limits = fill.limits;
    let mut failures = Vec::new();
    if water_loading_kg_m2_s < limits.min_water_loading_kg_m2_s
        || water_loading_kg_m2_s > limits.max_water_loading_kg_m2_s
    {
        failures.push(format!(
            "Water loading {} kg/(m²·s) is outside {}–{}.",
            to_fixed(water_loading_kg_m2_s, 3),
            limits.min_water_loading_kg_m2_s,
            limits.max_water_loading_kg_m2_s
        ));
    }
    if dry_air_loading_kg_m2_s < limits.min_dry_air_loading_kg_m2_s
        || dry_air_loading_kg_m2_s > limits.max_dry_air_loading_kg_m2_s
    {
        failures.push(format!(
            "Dry-air loading {} kg/(m²·s) is outside {}–{}.",
            to_fixed(dry_air_loading_kg_m2_s, 3),
            limits.min_dry_air_loading_kg_m2_s,
            limits.max_dry_air_loading_kg_m2_s
        ));
    }
    if let Some(hot_water_c) = hot_water_c {
        if hot_water_c > limits.max_water_temperature_c {
            failures.push(format!(
                "Hot-water temperature {} °C exceeds {} °C.",
                to_fixed(hot_water_c, 1),
                limits.max_water_temperature_c
            ));
        }
    }
    if let Some(water_quality_class) = water_quality_class {
        if !fill
            .allowed_water_quality_classes
            .iter()
            .any(|class| class == water_quality_class)
        {
            failures.push(format!(
                "Fill is not approved in the sample catalog for water-quality class “{}”.",
                water_quality_class
            ));
        }
    }
    failures
}

fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    let exact = format!("{:.*}", digits + 25, value.abs());
    let (integer_part, fraction) = exact.split_once('.').expect("a fixed-format decimal");
    let mut digits_text = String::with_capacity(integer_part.len() + digits);
    digits_text.push_str(integer_part);
    digits_text.push_str(&fraction[..digits]);
    if fraction.as_bytes()[digits] >= b'5' {
        increment_decimal(&mut digits_text);
    }
    let split = digits_text.len() - digits;
    let mut text = digits_text[..split].to_string();
    if digits > 0 {
        text.push('.');
        text.push_str(&digits_text[split..]);
    }
    if value < 0.0 {
        text.insert(0, '-');
    }
    text
}

fn increment_decimal(digits: &mut String) {
    let mut bytes = digits.clone().into_bytes();
    for index in (0..bytes.len()).rev() {
        if bytes[index] == b'9' {
            bytes[index] = b'0';
        } else {
            bytes[index] += 1;
            *digits = String::from_utf8(bytes).expect("ascii digits");
            return;
        }
    }
    bytes.insert(0, b'1');
    *digits = String::from_utf8(bytes).expect("ascii digits");
}
