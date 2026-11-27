use crate::numeric::{
    assert_positive, find_root_by_scan, interpolate_1d, DomainError, RootPreference,
    RootScanOptions,
};

/// Flow ∝ n: the fan-affinity exponent of the recorded curve's **flow** leg.
pub const FLOW_AFFINITY_EXPONENT: f64 = 1.0;

/// Pressure ∝ n²: the fan-affinity exponent of the recorded curve's **pressure** leg.
pub const PRESSURE_AFFINITY_EXPONENT: f64 = 2.0;

/// Shaft power ∝ n³: the exponent the two legs above imply for the **power** leg — `Q(n)·p(n) = n³·Q·p`, and the efficiency is read off the same recorded point.
pub const SHAFT_POWER_AFFINITY_EXPONENT: f64 = 3.0;

/// How a fan curve is published, mirroring the JavaScript `pressureBasis` string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanPressureBasis {
    Total,
    Static,
}

impl FanPressureBasis {
    /// The JavaScript spelling of this basis.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            FanPressureBasis::Total => "total",
            FanPressureBasis::Static => "static",
        }
    }
}

/// One point of a fan curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanCurvePoint {
    pub flow_m3_s: f64,
    pub pressure_pa: f64,
    pub efficiency: f64,
}

/// A validated fan catalog record, reduced to the fields this slice reads.
#[derive(Clone, Debug, PartialEq)]
pub struct FanRecord {
    pub id: String,
    pub stack_area_m2: Option<f64>,
    pub pressure_basis: Option<FanPressureBasis>,
    pub reference_density_kg_m3: Option<f64>,
    pub stack_recovery_factor: Option<f64>,
    pub curve: Vec<FanCurvePoint>,
}

fn validate_fan(fan: &FanRecord) -> Result<(), DomainError> {
    if fan.curve.len() < 2 {
        return Err(DomainError::new(
            "Fan must contain at least two curve points.",
        ));
    }
    assert_positive(
        fan.reference_density_kg_m3.unwrap_or(1.2),
        "fan.referenceDensityKgM3",
    )?;
    Ok(())
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// The operating range of a fan at one speed ratio, mirroring the JavaScript return object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanOperatingLimits {
    pub min_flow_m3_s: f64,
    pub max_flow_m3_s: f64,
}

/// # Errors `DomainError` when the input is out of domain. Port of `fanOperatingLimits`.
pub fn fan_operating_limits(
    fan: &FanRecord,
    speed_ratio: f64,
) -> Result<FanOperatingLimits, DomainError> {
    validate_fan(fan)?;
    let mut flows = fan
        .curve
        .iter()
        .map(|point| point.flow_m3_s * speed_ratio.powf(FLOW_AFFINITY_EXPONENT));
    let Some(first) = flows.next() else {
        return Err(DomainError::new("A fan curve needs at least two points."));
    };
    let (min_flow_m3_s, max_flow_m3_s) = flows.fold((first, first), |(min, max), flow| {
        (js_min(min, flow), js_max(max, flow))
    });
    Ok(FanOperatingLimits {
        min_flow_m3_s,
        max_flow_m3_s,
    })
}

/// # Errors `DomainError` when the input is out of domain. Port of `fanPressurePaAtFlow`: the curve is read at the speed-corrected reference flow and scaled by the density ratio and the affinity law's pressure exponent ([`PRESSURE_AFFINITY_EXPONENT`], the square of the speed ratio).
pub fn fan_pressure_pa_at_flow(
    fan: &FanRecord,
    flow_m3_s: f64,
    speed_ratio: f64,
    air_density_kg_m3: f64,
) -> Result<f64, DomainError> {
    validate_fan(fan)?;
    assert_positive(speed_ratio, "speedRatio")?;
    assert_positive(air_density_kg_m3, "airDensityKgM3")?;
    let reference_flow = flow_m3_s / speed_ratio.powf(FLOW_AFFINITY_EXPONENT);
    let reference_pressure = interpolate_1d(
        &fan.curve,
        reference_flow,
        |point| point.flow_m3_s,
        |point| point.pressure_pa,
        false,
    )?;
    Ok(reference_pressure
        * (air_density_kg_m3 / fan.reference_density_kg_m3.unwrap_or(1.2))
        * speed_ratio.powf(PRESSURE_AFFINITY_EXPONENT))
}

/// # Errors `DomainError` when the input is out of domain. Port of `fanEfficiencyAtFlow`: the curve is read at the speed-corrected reference flow and clamped at the ends (an efficiency curve outside its range is not extrapolated).
pub fn fan_efficiency_at_flow(
    fan: &FanRecord,
    flow_m3_s: f64,
    speed_ratio: f64,
) -> Result<f64, DomainError> {
    validate_fan(fan)?;
    let reference_flow = flow_m3_s / speed_ratio.powf(FLOW_AFFINITY_EXPONENT);
    interpolate_1d(
        &fan.curve,
        reference_flow,
        |point| point.flow_m3_s,
        |point| point.efficiency,
        true,
    )
}

/// # Errors `DomainError` when the input is out of domain. Port of `fanShaftPowerKW`.
pub fn fan_shaft_power_kw(
    fan: &FanRecord,
    flow_m3_s: f64,
    pressure_pa: f64,
    speed_ratio: f64,
) -> Result<f64, DomainError> {
    let efficiency = fan_efficiency_at_flow(fan, flow_m3_s, speed_ratio)?;
    if efficiency <= 0.0 {
        return Err(DomainError::new("Fan efficiency must be positive."));
    }
    Ok(flow_m3_s * pressure_pa / efficiency / 1000.0)
}

/// Input to [`solve_fan_system_intersection`]; `speed_ratio` and `minimum_flow_fraction` keep their JavaScript defaults through [`FanSystemIntersectionInput::new`].
#[derive(Clone, Copy, Debug)]
pub struct FanSystemIntersectionInput<'a> {
    pub fan: &'a FanRecord,
    pub air_density_kg_m3: f64,
    pub speed_ratio: f64,
    pub minimum_flow_fraction: f64,
}

impl<'a> FanSystemIntersectionInput<'a> {
    /// The required inputs; defaults to the JavaScript `speedRatio: 1` and `minimumFlowFraction: 0.05`.
    #[must_use]
    pub fn new(fan: &'a FanRecord, air_density_kg_m3: f64) -> Self {
        Self {
            fan,
            air_density_kg_m3,
            speed_ratio: 1.0,
            minimum_flow_fraction: 0.05,
        }
    }

    #[must_use]
    pub fn with_speed_ratio(mut self, speed_ratio: f64) -> Self {
        self.speed_ratio = speed_ratio;
        self
    }

    #[must_use]
    pub fn with_minimum_flow_fraction(mut self, minimum_flow_fraction: f64) -> Self {
        self.minimum_flow_fraction = minimum_flow_fraction;
        self
    }
}

/// The balanced operating point, mirroring the JavaScript return object field for field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanSystemIntersection {
    pub flow_m3_s: f64,
    pub fan_pressure_pa: f64,
    pub system_pressure_pa: f64,
    pub efficiency: f64,
    pub shaft_power_kw: f64,
    pub speed_ratio: f64,
    pub residual_pa: f64,
}

/// # Errors `DomainError` when the input is out of domain. Port of `solveFanSystemIntersection`.
pub fn solve_fan_system_intersection<F>(
    input: &FanSystemIntersectionInput<'_>,
    system_pressure_fn: F,
) -> Result<FanSystemIntersection, DomainError>
where
    F: Fn(f64) -> Result<f64, DomainError>,
{
    let limits = fan_operating_limits(input.fan, input.speed_ratio)?;
    let lower = js_max(
        limits.min_flow_m3_s,
        limits.max_flow_m3_s * input.minimum_flow_fraction,
    );
    let upper = limits.max_flow_m3_s;
    let residual = |flow_m3_s: f64| -> Result<f64, DomainError> {
        Ok(fan_pressure_pa_at_flow(
            input.fan,
            flow_m3_s,
            input.speed_ratio,
            input.air_density_kg_m3,
        )? - system_pressure_fn(flow_m3_s)?)
    };
    let flow_m3_s = find_root_by_scan(
        residual,
        lower,
        upper,
        RootScanOptions {
            samples: 300,
            tolerance: 1e-7,
            max_iterations: 160,
            prefer: RootPreference::Last,
        },
    )?;
    let fan_pressure_pa = fan_pressure_pa_at_flow(
        input.fan,
        flow_m3_s,
        input.speed_ratio,
        input.air_density_kg_m3,
    )?;
    let system_pressure_pa = system_pressure_fn(flow_m3_s)?;
    let efficiency = fan_efficiency_at_flow(input.fan, flow_m3_s, input.speed_ratio)?;
    let shaft_power_kw =
        fan_shaft_power_kw(input.fan, flow_m3_s, fan_pressure_pa, input.speed_ratio)?;
    Ok(FanSystemIntersection {
        flow_m3_s,
        fan_pressure_pa,
        system_pressure_pa,
        efficiency,
        shaft_power_kw,
        speed_ratio: input.speed_ratio,
        residual_pa: fan_pressure_pa - system_pressure_pa,
    })
}

/// Input to [`estimate_airflow_from_fan_power`].
#[derive(Clone, Copy, Debug)]
pub struct AirflowEstimateInput {
    pub reference_volumetric_flow_m3_s: f64,
    pub reference_fan_power_kw: f64,
    pub actual_fan_power_kw: f64,
    pub reference_air_density_kg_m3: f64,
    pub actual_air_density_kg_m3: f64,
}

/// # Errors `DomainError` when the input is out of domain. Port of `estimateAirflowFromFanPower`: the fan-affinity estimate `Q = Q_ref * (P/P_ref * rho_ref/rho)^(1/3)`.
pub fn estimate_airflow_from_fan_power(input: AirflowEstimateInput) -> Result<f64, DomainError> {
    assert_positive(
        input.reference_volumetric_flow_m3_s,
        "referenceVolumetricFlowM3S",
    )?;
    assert_positive(input.reference_fan_power_kw, "referenceFanPowerKW")?;
    assert_positive(input.actual_fan_power_kw, "actualFanPowerKW")?;
    assert_positive(input.reference_air_density_kg_m3, "referenceAirDensityKgM3")?;
    assert_positive(input.actual_air_density_kg_m3, "actualAirDensityKgM3")?;
    Ok(input.reference_volumetric_flow_m3_s
        * ((input.actual_fan_power_kw / input.reference_fan_power_kw)
            * (input.reference_air_density_kg_m3 / input.actual_air_density_kg_m3))
            .powf(1.0 / 3.0))
}

/// The IEC standard motor sizes the reference selects from, `standardSizesKW` in `fan.js`.
pub const STANDARD_MOTOR_SIZES_KW: [f64; 24] = [
    0.75, 1.1, 1.5, 2.2, 3.0, 4.0, 5.5, 7.5, 11.0, 15.0, 18.5, 22.0, 30.0, 37.0, 45.0, 55.0, 75.0,
    90.0, 110.0, 132.0, 160.0, 200.0, 250.0, 315.0,
];

/// The motor selection, mirroring the JavaScript return object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotorSelection {
    pub required_motor_output_kw: f64,
    /// `None` when the required output is above the largest standard size.
    pub selected_motor_kw: Option<f64>,
    pub drive_efficiency: f64,
    pub service_factor: f64,
}

/// # Errors `DomainError` when the input is out of domain. Port of `chooseStandardMotor`.
pub fn choose_standard_motor(
    shaft_power_kw: f64,
    drive_efficiency: f64,
    service_factor: f64,
) -> Result<MotorSelection, DomainError> {
    assert_positive(shaft_power_kw, "shaftPowerKW")?;
    let required_motor_output_kw = shaft_power_kw / drive_efficiency * service_factor;
    let selected_motor_kw = STANDARD_MOTOR_SIZES_KW
        .iter()
        .copied()
        .find(|size| *size >= required_motor_output_kw);
    Ok(MotorSelection {
        required_motor_output_kw,
        selected_motor_kw,
        drive_efficiency,
        service_factor,
    })
}
