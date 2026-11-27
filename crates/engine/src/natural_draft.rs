use crate::airside::{
    system_pressure_breakdown, DriftEliminatorRecord, FillRecord, SystemPressureBreakdown,
    SystemPressureBreakdownInput, TowerRecord,
};
use crate::crossflow::OutletAirState;
use crate::merkel::{
    solve_cold_water_temperature, ColdWaterTemperatureInput, ColdWaterTemperatureResult,
    Integration,
};
use crate::numeric::{find_root_by_scan, DomainError, RootPreference, RootScanOptions};
use crate::psychrometrics::{
    moist_air_density_kg_m3, psychrometric_state, PsychrometricState, PsychrometricStateInput,
};
use crate::selection::estimate_outlet_air_state;
use crate::water_balance::evaporation_from_air_mass_balance;

const G: f64 = 9.80665;

/// Port of `naturalDraftPressurePa`: the buoyancy of a column of ambient air against a column of warmer, lighter plume air.
pub fn natural_draft_pressure_pa(
    effective_height_m: f64,
    ambient_density_kg_m3: f64,
    plume_density_kg_m3: f64,
) -> f64 {
    G * effective_height_m * (ambient_density_kg_m3 - plume_density_kg_m3)
}

/// The inputs of [`solve_natural_draft_counterflow`], carrying the reference's default arguments: pressure 101 325 Pa, salinity 0 g/kg, both multipliers 1, and face velocities 0.2–6 m/s.
pub struct NaturalDraftInput<'a> {
    pub tower: &'a TowerRecord,
    /// The reference's `tower.effectiveDraftHeightM`, the field the draft solve is built on.
    pub effective_draft_height_m: f64,
    pub fill: &'a FillRecord,
    pub fill_depth_m: f64,
    pub drift_eliminator: &'a DriftEliminatorRecord,
    pub hot_water_c: f64,
    pub dry_bulb_c: f64,
    pub wet_bulb_c: f64,
    pub pressure_pa: f64,
    pub water_mass_flow_kg_s: f64,
    pub salinity_g_kg: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
    pub min_face_velocity_ms: f64,
    pub max_face_velocity_ms: f64,
}

impl<'a> NaturalDraftInput<'a> {
    /// The arguments the reference requires — the three records and the four condition values — with every default argument applied.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tower: &'a TowerRecord,
        effective_draft_height_m: f64,
        fill: &'a FillRecord,
        fill_depth_m: f64,
        drift_eliminator: &'a DriftEliminatorRecord,
        hot_water_c: f64,
        dry_bulb_c: f64,
        wet_bulb_c: f64,
        water_mass_flow_kg_s: f64,
    ) -> Self {
        Self {
            tower,
            effective_draft_height_m,
            fill,
            fill_depth_m,
            drift_eliminator,
            hot_water_c,
            dry_bulb_c,
            wet_bulb_c,
            pressure_pa: 101_325.0,
            water_mass_flow_kg_s,
            salinity_g_kg: 0.0,
            thermal_multiplier: 1.0,
            pressure_multiplier: 1.0,
            min_face_velocity_ms: 0.2,
            max_face_velocity_ms: 6.0,
        }
    }

    /// The reference's `pressurePa` argument.
    pub fn with_pressure_pa(mut self, pressure_pa: f64) -> Self {
        self.pressure_pa = pressure_pa;
        self
    }

    /// The reference's `salinityGKg` argument.
    pub fn with_salinity_g_kg(mut self, salinity_g_kg: f64) -> Self {
        self.salinity_g_kg = salinity_g_kg;
        self
    }

    /// The reference's `thermalMultiplier` argument.
    pub fn with_thermal_multiplier(mut self, thermal_multiplier: f64) -> Self {
        self.thermal_multiplier = thermal_multiplier;
        self
    }

    /// The reference's `pressureMultiplier` argument.
    pub fn with_pressure_multiplier(mut self, pressure_multiplier: f64) -> Self {
        self.pressure_multiplier = pressure_multiplier;
        self
    }

    /// The reference's `minFaceVelocityMS` argument.
    pub fn with_min_face_velocity_ms(mut self, min_face_velocity_ms: f64) -> Self {
        self.min_face_velocity_ms = min_face_velocity_ms;
        self
    }

    /// The reference's `maxFaceVelocityMS` argument.
    pub fn with_max_face_velocity_ms(mut self, max_face_velocity_ms: f64) -> Self {
        self.max_face_velocity_ms = max_face_velocity_ms;
        self
    }
}

/// Everything `calculateAtFlow` computes for one trial flow — the reference's own result object before the draft height and the ambient state are attached.
pub struct NaturalDraftFlowPoint {
    pub volumetric_air_flow_m3_s: f64,
    pub airside: SystemPressureBreakdown,
    pub water_to_dry_air_ratio: f64,
    pub thermal: ColdWaterTemperatureResult,
    pub outlet: OutletAirState,
    pub plume_density_kg_m3: f64,
    pub draft_pressure_pa: f64,
    pub residual_pa: f64,
}

/// The object `solveNaturalDraftCounterflow` returns, field for field (the `model` and `caveat` strings travel with the result the CLI renders; this struct carries the physics).
#[derive(Clone, Debug)]
pub struct NaturalDraftResult {
    pub volumetric_air_flow_m3_s: f64,
    pub water_to_dry_air_ratio: f64,
    pub plume_density_kg_m3: f64,
    pub draft_pressure_pa: f64,
    pub residual_pa: f64,
    pub airside: SystemPressureBreakdown,
    pub thermal: ColdWaterTemperatureResult,
    pub outlet: OutletAirState,
    pub inlet_air_state: PsychrometricState,
    pub evaporation_kg_s: f64,
}

/// Port of `solveNaturalDraftCounterflow`: sample the face-velocity interval, solve the flow whose draft balances the air-side resistance, then report the full coupled point at that flow, plus the evaporation the plume's humidity rise implies.
pub fn solve_natural_draft_counterflow(
    input: &NaturalDraftInput<'_>,
) -> Result<NaturalDraftResult, DomainError> {
    if input.effective_draft_height_m.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err(DomainError::new(
            "Natural-draft tower requires effectiveDraftHeightM.",
        ));
    }
    let inlet = psychrometric_state(
        PsychrometricStateInput::from_wet_bulb(input.dry_bulb_c, input.wet_bulb_c)
            .with_pressure(input.pressure_pa),
    )?;
    let area_m2 = input
        .tower
        .air_free_area_m2
        .unwrap_or(input.tower.fill_area_m2);

    let calculate_at_flow =
        |volumetric_air_flow_m3_s: f64| -> Result<NaturalDraftFlowPoint, DomainError> {
            let airside = system_pressure_breakdown(SystemPressureBreakdownInput {
                tower: input.tower,
                fill: input.fill,
                fill_depth_m: input.fill_depth_m,
                drift_eliminator: input.drift_eliminator,
                fan: None,
                volumetric_air_flow_m3_s,
                dry_air_density_kg_m3: inlet.dry_air_density_kg_m3,
                moist_air_density_kg_m3: inlet.moist_air_density_kg_m3,
                water_mass_flow_kg_s: input.water_mass_flow_kg_s,
                thermal_multiplier: input.thermal_multiplier,
                pressure_multiplier: input.pressure_multiplier,
            })?;
            let water_to_dry_air_ratio =
                input.water_mass_flow_kg_s / airside.dry_air_mass_flow_kg_s;
            let thermal = solve_cold_water_temperature(&ColdWaterTemperatureInput {
                pressure_pa: input.pressure_pa,
                salinity_g_kg: input.salinity_g_kg,
                integration: Integration::Chebyshev4,
                ..ColdWaterTemperatureInput::new(
                    input.hot_water_c,
                    input.wet_bulb_c,
                    input.dry_bulb_c,
                    water_to_dry_air_ratio,
                    airside.available_merkel_number,
                )
            })?;
            let outlet = estimate_outlet_air_state(
                &inlet,
                water_to_dry_air_ratio,
                input.hot_water_c,
                thermal.cold_water_c,
                thermal.cp_water_kj_kg_k,
                input.pressure_pa,
            )?;
            let plume_density_kg_m3 = moist_air_density_kg_m3(
                outlet.dry_bulb_c,
                outlet.humidity_ratio,
                input.pressure_pa,
            );
            let draft_pressure_pa = natural_draft_pressure_pa(
                input.effective_draft_height_m,
                inlet.moist_air_density_kg_m3,
                plume_density_kg_m3,
            );
            let residual_pa = draft_pressure_pa - airside.total_pa;
            Ok(NaturalDraftFlowPoint {
                volumetric_air_flow_m3_s,
                airside,
                water_to_dry_air_ratio,
                thermal,
                outlet,
                plume_density_kg_m3,
                draft_pressure_pa,
                residual_pa,
            })
        };

    let lower = area_m2 * input.min_face_velocity_ms;
    let upper = area_m2 * input.max_face_velocity_ms;
    let residual = |flow_m3_s: f64| -> Result<f64, DomainError> {
        match calculate_at_flow(flow_m3_s) {
            Ok(point) => Ok(point.residual_pa),
            Err(_) => Ok(f64::NAN),
        }
    };
    let volumetric_air_flow_m3_s = find_root_by_scan(
        residual,
        lower,
        upper,
        RootScanOptions {
            samples: 480,
            tolerance: 1e-6,
            prefer: RootPreference::First,
            ..RootScanOptions::default()
        },
    )?;
    let point = calculate_at_flow(volumetric_air_flow_m3_s)?;
    let evaporation_kg_s = evaporation_from_air_mass_balance(
        point.airside.dry_air_mass_flow_kg_s,
        inlet.humidity_ratio,
        point.outlet.humidity_ratio,
    )?;
    Ok(NaturalDraftResult {
        volumetric_air_flow_m3_s: point.volumetric_air_flow_m3_s,
        water_to_dry_air_ratio: point.water_to_dry_air_ratio,
        plume_density_kg_m3: point.plume_density_kg_m3,
        draft_pressure_pa: point.draft_pressure_pa,
        residual_pa: point.residual_pa,
        airside: point.airside,
        thermal: point.thermal,
        outlet: point.outlet,
        inlet_air_state: inlet,
        evaporation_kg_s,
    })
}
