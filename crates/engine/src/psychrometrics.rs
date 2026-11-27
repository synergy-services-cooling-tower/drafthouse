//! Moist-air properties — port of `src/core/psychrometrics.js`.
//!
//! Conventions, mirrored from the JavaScript reference (see `docs/ENGINEERING_METHODS.md`):
//!
//! * ASHRAE Fundamentals saturation-pressure correlation, liquid branch above 0.01 °C and
//!   ice branch at or below it, limited to -100…200 °C.
//! * The water-vapour **enhancement factor** `f = 1.0007 + 3.46e-6 P[hPa]` (pressure-only
//!   Buck 1981) is applied by default; the simplified ASHRAE set is available by turning it
//!   off. ASHRAE's published tables include this real-mixture effect.
//! * Out-of-domain inputs return [`DomainError`]; nothing is clamped to a value.

use crate::numeric::{
    assert_finite_number, clamp, solve_bracketed_root, BracketedRootOptions, DomainError,
};

const R_DA: f64 = 287.042; // J/(kg dry air · K)
const MIN_HUM_RATIO: f64 = 1e-7;
const EPSILON: f64 = 0.621945;

/// Port of `DEFAULT_PSYCHROMETRIC_OPTIONS`:
/// `enhancementFactor` selects the real-mixture treatment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PsychrometricOptions {
    pub enhancement_factor: bool,
}

impl Default for PsychrometricOptions {
    fn default() -> Self {
        Self {
            enhancement_factor: true,
        }
    }
}

/// Port of `waterVaporEnhancementFactor`.
///
/// The true saturation partial pressure of water in moist air is `f * pws(t)`, with `f`
/// slightly greater than 1; ignoring it leaves humidity ratios about 0.3-0.6 % low near
/// ambient conditions.
pub fn water_vapor_enhancement_factor(pressure_pa: f64, options: PsychrometricOptions) -> f64 {
    if !options.enhancement_factor {
        return 1.0;
    }
    1.0007 + 3.46e-6 * (pressure_pa / 100.0)
}

/// Port of `saturationVaporPressurePa` — ASHRAE Fundamentals, Chapter 1.
pub fn saturation_vapor_pressure_pa(dry_bulb_c: f64) -> Result<f64, DomainError> {
    assert_finite_number(dry_bulb_c, "temperature")?;
    let t_k = dry_bulb_c + 273.15;
    if !(-100.0..=200.0).contains(&dry_bulb_c) {
        return Err(DomainError::new(
            "ASHRAE saturation-pressure correlation is limited here to -100 to 200 °C.",
        ));
    }

    let ln_pws = if dry_bulb_c <= 0.01 {
        let (c1, c2, c3, c4, c5, c6, c7) = (
            -5.6745359e3,
            6.3925247,
            -9.677843e-3,
            6.2215701e-7,
            2.0747825e-9,
            -9.484024e-13,
            4.1635019,
        );
        c1 / t_k
            + c2
            + c3 * t_k
            + c4 * t_k.powi(2)
            + c5 * t_k.powi(3)
            + c6 * t_k.powi(4)
            + c7 * t_k.ln()
    } else {
        let (c8, c9, c10, c11, c12, c13) = (
            -5.8002206e3,
            1.3914993,
            -4.8640239e-2,
            4.1764768e-5,
            -1.4452093e-8,
            6.5459673,
        );
        c8 / t_k + c9 + c10 * t_k + c11 * t_k.powi(2) + c12 * t_k.powi(3) + c13 * t_k.ln()
    };
    Ok(ln_pws.exp())
}

/// Port of `effectiveSaturationPressurePa`: the quantity that should drive every
/// humidity-ratio calculation.
pub fn effective_saturation_pressure_pa(
    temperature_c: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    Ok(water_vapor_enhancement_factor(pressure_pa, options)
        * saturation_vapor_pressure_pa(temperature_c)?)
}

/// Port of `humidityRatioFromVaporPressure`.
pub fn humidity_ratio_from_vapor_pressure(
    vapor_pressure_pa: f64,
    pressure_pa: f64,
) -> Result<f64, DomainError> {
    if vapor_pressure_pa < 0.0 || vapor_pressure_pa >= pressure_pa {
        return Err(DomainError::new(
            "Vapor pressure must be non-negative and below total pressure.",
        ));
    }
    Ok(MIN_HUM_RATIO.max(EPSILON * vapor_pressure_pa / (pressure_pa - vapor_pressure_pa)))
}

/// Port of `vaporPressureFromHumidityRatio`.
pub fn vapor_pressure_from_humidity_ratio(
    humidity_ratio: f64,
    pressure_pa: f64,
) -> Result<f64, DomainError> {
    if humidity_ratio < 0.0 {
        return Err(DomainError::new("Humidity ratio cannot be negative."));
    }
    Ok(pressure_pa * humidity_ratio / (EPSILON + humidity_ratio))
}

/// Port of `saturationHumidityRatio`.
pub fn saturation_humidity_ratio(
    temperature_c: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    let pws = effective_saturation_pressure_pa(temperature_c, pressure_pa, options)?;
    if pws >= pressure_pa {
        return Err(DomainError::new(
            "Saturation pressure equals or exceeds atmospheric pressure.",
        ));
    }
    humidity_ratio_from_vapor_pressure(pws, pressure_pa)
}

/// Port of `humidityRatioFromRelativeHumidity`. `relative_humidity` is a fraction, 0…1.
pub fn humidity_ratio_from_relative_humidity(
    dry_bulb_c: f64,
    relative_humidity: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    if !(0.0..=1.0).contains(&relative_humidity) {
        return Err(DomainError::new(
            "Relative humidity must be between 0 and 1.",
        ));
    }
    humidity_ratio_from_vapor_pressure(
        relative_humidity * effective_saturation_pressure_pa(dry_bulb_c, pressure_pa, options)?,
        pressure_pa,
    )
}

/// Port of `relativeHumidityFromHumidityRatio` (clamped to 0…1, as the reference does).
pub fn relative_humidity_from_humidity_ratio(
    dry_bulb_c: f64,
    humidity_ratio: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    let vapor_pressure = vapor_pressure_from_humidity_ratio(humidity_ratio, pressure_pa)?;
    let saturation_pressure = effective_saturation_pressure_pa(dry_bulb_c, pressure_pa, options)?;
    Ok(clamp(vapor_pressure / saturation_pressure, 0.0, 1.0))
}

/// Port of `humidityRatioFromWetBulb`, including the sub-zero relation.
pub fn humidity_ratio_from_wet_bulb(
    dry_bulb_c: f64,
    wet_bulb_c: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    if wet_bulb_c > dry_bulb_c {
        return Err(DomainError::new(
            "Wet-bulb temperature cannot exceed dry-bulb temperature.",
        ));
    }
    let saturated_at_wet_bulb = saturation_humidity_ratio(wet_bulb_c, pressure_pa, options)?;
    let ratio = if wet_bulb_c >= 0.0 {
        ((2501.0 - 2.326 * wet_bulb_c) * saturated_at_wet_bulb - 1.006 * (dry_bulb_c - wet_bulb_c))
            / (2501.0 + 1.86 * dry_bulb_c - 4.186 * wet_bulb_c)
    } else {
        ((2830.0 - 0.24 * wet_bulb_c) * saturated_at_wet_bulb - 1.006 * (dry_bulb_c - wet_bulb_c))
            / (2830.0 + 1.86 * dry_bulb_c - 2.1 * wet_bulb_c)
    };
    Ok(MIN_HUM_RATIO.max(ratio))
}

/// Port of `wetBulbFromHumidityRatio`. The residual tolerance is scaled to the humidity
/// ratio being matched: sub-zero air holds about 6e-4, where a fixed 1e-7 would be a 1e-4
/// *relative* error.
pub fn wet_bulb_from_humidity_ratio(
    dry_bulb_c: f64,
    humidity_ratio: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    let lower = (-100.0_f64).max(dew_point_from_humidity_ratio(
        humidity_ratio,
        pressure_pa,
        options,
    )?);
    solve_bracketed_root(
        |wet_bulb_c| {
            Ok(
                humidity_ratio_from_wet_bulb(dry_bulb_c, wet_bulb_c, pressure_pa, options)?
                    - humidity_ratio,
            )
        },
        lower,
        dry_bulb_c,
        BracketedRootOptions {
            tolerance: 1e-12_f64.max(1e-7 * humidity_ratio),
            x_tolerance: Some(1e-9),
            ..BracketedRootOptions::default()
        },
    )
}

/// Port of `dewPointFromHumidityRatio`.
pub fn dew_point_from_humidity_ratio(
    humidity_ratio: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    let vapor_pressure = vapor_pressure_from_humidity_ratio(humidity_ratio, pressure_pa)?;
    solve_bracketed_root(
        |temperature_c| {
            Ok(
                effective_saturation_pressure_pa(temperature_c, pressure_pa, options)?
                    - vapor_pressure,
            )
        },
        -100.0,
        100.01,
        BracketedRootOptions {
            tolerance: 1e-6,
            ..BracketedRootOptions::default()
        },
    )
}

/// Port of `moistAirEnthalpyKJkgDryAir`.
pub fn moist_air_enthalpy_kj_kg_dry_air(dry_bulb_c: f64, humidity_ratio: f64) -> f64 {
    1.006 * dry_bulb_c + humidity_ratio * (2501.0 + 1.86 * dry_bulb_c)
}

/// Port of `saturatedAirEnthalpyKJkgDryAir`.
pub fn saturated_air_enthalpy_kj_kg_dry_air(
    temperature_c: f64,
    pressure_pa: f64,
    options: PsychrometricOptions,
) -> Result<f64, DomainError> {
    Ok(moist_air_enthalpy_kj_kg_dry_air(
        temperature_c,
        saturation_humidity_ratio(temperature_c, pressure_pa, options)?,
    ))
}

/// Port of `dryAirSpecificVolumeM3kg`.
pub fn dry_air_specific_volume_m3_kg(
    dry_bulb_c: f64,
    humidity_ratio: f64,
    pressure_pa: f64,
) -> f64 {
    let t_k = dry_bulb_c + 273.15;
    R_DA * t_k * (1.0 + 1.607858 * humidity_ratio) / pressure_pa
}

/// Port of `dryAirDensityKgM3`.
pub fn dry_air_density_kg_m3(dry_bulb_c: f64, humidity_ratio: f64, pressure_pa: f64) -> f64 {
    1.0 / dry_air_specific_volume_m3_kg(dry_bulb_c, humidity_ratio, pressure_pa)
}

/// Port of `moistAirDensityKgM3`.
pub fn moist_air_density_kg_m3(dry_bulb_c: f64, humidity_ratio: f64, pressure_pa: f64) -> f64 {
    (1.0 + humidity_ratio) * dry_air_density_kg_m3(dry_bulb_c, humidity_ratio, pressure_pa)
}

/// Inputs for [`psychrometric_state`] — the port of the JavaScript
/// `psychrometricState({ dryBulbC, wetBulbC, relativeHumidity, pressurePa, enhancementFactor })`
/// call. When both `wet_bulb_c` and `relative_humidity` are present the wet bulb wins, as in
/// the reference.
#[derive(Clone, Copy, Debug)]
pub struct PsychrometricStateInput {
    pub dry_bulb_c: f64,
    pub wet_bulb_c: Option<f64>,
    pub relative_humidity: Option<f64>,
    pub pressure_pa: f64,
    pub enhancement_factor: bool,
}

impl PsychrometricStateInput {
    /// Dry bulb + wet bulb at standard atmospheric pressure.
    pub fn from_wet_bulb(dry_bulb_c: f64, wet_bulb_c: f64) -> Self {
        Self {
            dry_bulb_c,
            wet_bulb_c: Some(wet_bulb_c),
            relative_humidity: None,
            pressure_pa: 101_325.0,
            enhancement_factor: true,
        }
    }

    /// Dry bulb + relative humidity (fraction) at standard atmospheric pressure.
    pub fn from_relative_humidity(dry_bulb_c: f64, relative_humidity: f64) -> Self {
        Self {
            dry_bulb_c,
            wet_bulb_c: None,
            relative_humidity: Some(relative_humidity),
            pressure_pa: 101_325.0,
            enhancement_factor: true,
        }
    }

    pub fn with_pressure(mut self, pressure_pa: f64) -> Self {
        self.pressure_pa = pressure_pa;
        self
    }

    pub fn with_enhancement_factor(mut self, enhancement_factor: bool) -> Self {
        self.enhancement_factor = enhancement_factor;
        self
    }
}

/// Port of the `psychrometricState` result object.
#[derive(Clone, Copy, Debug)]
pub struct PsychrometricState {
    pub dry_bulb_c: f64,
    pub wet_bulb_c: f64,
    pub relative_humidity: f64,
    pub pressure_pa: f64,
    pub humidity_ratio: f64,
    pub enthalpy_kj_kg_dry_air: f64,
    pub dry_air_density_kg_m3: f64,
    pub moist_air_density_kg_m3: f64,
    pub dew_point_c: f64,
    pub enhancement_factor: f64,
}

/// Port of `psychrometricState`.
pub fn psychrometric_state(
    input: PsychrometricStateInput,
) -> Result<PsychrometricState, DomainError> {
    assert_finite_number(input.dry_bulb_c, "dryBulbC")?;
    let options = PsychrometricOptions {
        enhancement_factor: input.enhancement_factor,
    };

    let humidity_ratio;
    let wet_bulb_c;
    let relative_humidity;
    if let Some(wet_bulb) = input.wet_bulb_c {
        humidity_ratio =
            humidity_ratio_from_wet_bulb(input.dry_bulb_c, wet_bulb, input.pressure_pa, options)?;
        wet_bulb_c = wet_bulb;
        relative_humidity = relative_humidity_from_humidity_ratio(
            input.dry_bulb_c,
            humidity_ratio,
            input.pressure_pa,
            options,
        )?;
    } else if let Some(relative_humidity_input) = input.relative_humidity {
        humidity_ratio = humidity_ratio_from_relative_humidity(
            input.dry_bulb_c,
            relative_humidity_input,
            input.pressure_pa,
            options,
        )?;
        wet_bulb_c = wet_bulb_from_humidity_ratio(
            input.dry_bulb_c,
            humidity_ratio,
            input.pressure_pa,
            options,
        )?;
        relative_humidity = relative_humidity_input;
    } else {
        return Err(DomainError::new("Provide wetBulbC or relativeHumidity."));
    }

    Ok(PsychrometricState {
        dry_bulb_c: input.dry_bulb_c,
        wet_bulb_c,
        relative_humidity,
        pressure_pa: input.pressure_pa,
        humidity_ratio,
        enthalpy_kj_kg_dry_air: moist_air_enthalpy_kj_kg_dry_air(input.dry_bulb_c, humidity_ratio),
        dry_air_density_kg_m3: dry_air_density_kg_m3(
            input.dry_bulb_c,
            humidity_ratio,
            input.pressure_pa,
        ),
        moist_air_density_kg_m3: moist_air_density_kg_m3(
            input.dry_bulb_c,
            humidity_ratio,
            input.pressure_pa,
        ),
        dew_point_c: dew_point_from_humidity_ratio(humidity_ratio, input.pressure_pa, options)?,
        enhancement_factor: water_vapor_enhancement_factor(input.pressure_pa, options),
    })
}
