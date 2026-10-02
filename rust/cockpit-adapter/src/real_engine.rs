//! [`RealEngine`] - the cockpit's [`Engine`] over the engine crate's own selection path.

use std::collections::HashMap;

use synergy_drafthouse::{
    check_fill_operating_envelope, fan_pressure_pa_at_flow, fan_speed_ratio_limit,
    layered_system_pressure_breakdown, psychrometric_state, run_selection, water_density_kg_m3,
    CatalogMetadata, DriftCurvePoint, DriftEliminatorRecord, FanCurvePoint, FanPressureBasis,
    FanRecord, FillLayer as EngineFillLayer, FillLayerResult, FillOperatingEnvelopeInput,
    FillStack, LayerTerms, LayeredBreakdownInput, NozzleRecord as EngineNozzleRecord,
    PsychrometricState, PsychrometricStateInput, SelectionCandidate, SelectionCatalog,
    SelectionDriftEliminator, SelectionFan, SelectionFill, SelectionInput, SelectionRequirements,
    SelectionRun, SelectionTower, SystemPressureBreakdown, TowerRecord as EngineTowerRecord,
    TowerType, WaterQualityFactor,
};

use crate::engine::{
    Engine, EngineError, EngineInput, EngineOutput, FanRecord as ContractFanRecord, FanSystemCurve,
    FillLayer, LayerResult, Limit, PressureZone, Provenance, Series, ThermalCurve, TowerRecord,
    WorkedStep, ZoneId, XY,
};

/// Millimetres of water gauge per pascal: the cockpit's second total-pressure unit.
///
/// `1 / 9.80665` is the cockpit baseline's own constant (`cockpit/src/fixture_engine.rs`); the
/// conversion is a display unit, not a second pressure model.
const MMWG_PER_PA: f64 = 1.0 / 9.80665;

/// The water flows the performance chart plots, in kg/s - the recorded sweep axis (140 to 260 in
/// 10 kg/s steps, `fixtures/engine-run.json`'s `sweeps.waterMassFlowKgS.values`). The chart plots
/// that axis converted to m3/hr at the engine's own water density.
fn performance_chart_water_mass_flow_kg_s() -> impl Iterator<Item = f64> {
    (140..=260).step_by(10).map(f64::from)
}

/// The air flows the resistance chart plots, in m3/s - the recorded system-curve axis (60 to 180 in
/// 5 m3/s steps, `fixtures/engine-run.json`'s `systemCurve`).
fn resistance_chart_flows_m3_s() -> impl Iterator<Item = f64> {
    (60..=180).step_by(5).map(f64::from)
}

/// The engine, as the cockpit's [`Engine`].
///
/// One instance holds the engine-side catalog the run reads from. Two of the cockpit's records have
/// no field in its contract and come from that catalog by id - the **fill record** (the input's
/// [`FillLayer`] names a fill and a depth; the thermal and pressure correlations live in the
/// catalog) and the **tower's spray/rain zone correlations** (catalog data the UI never edits).
/// Everything else - the tower's own dimensions and losses, the drift eliminator, the fan, the
/// nozzle, the duty - is the input's, because the cockpit edits its draft and an edit has to reach
/// the engine.
pub struct RealEngine {
    catalog: SelectionCatalog,
}

impl RealEngine {
    /// The adapter over an engine-side catalog. The catalog's own metadata becomes the run's
    /// provenance; its records are read-only (the adapter never edits one).
    pub fn new(catalog: SelectionCatalog) -> Self {
        Self { catalog }
    }

    /// The catalog this instance reads.
    pub fn catalog(&self) -> &SelectionCatalog {
        &self.catalog
    }

    fn tower(&self, id: &str) -> Option<&SelectionTower> {
        self.catalog.towers.iter().find(|tower| tower.id == id)
    }

    fn fill(&self, id: &str) -> Option<&SelectionFill> {
        self.catalog.fills.iter().find(|fill| fill.physics.id == id)
    }

    /// The run that produced no candidate at all: the engine's own rejection reasons, as [`Limit`]s.
    /// The contract reads a non-empty `validation` as "the engine refused: no headline numbers are
    /// valid", and the UI shows the messages instead of the numbers.
    ///
    /// A speed ratio outside the fan record's own validity band is named first, with the record's
    /// band on it: the engine refuses such a ratio rather than clamping it into the band and
    /// returning a number that looks evaluated. The band and the check are the engine's own
    /// ([`synergy_drafthouse::fan_speed_ratio_limit`]).
    fn refused_output(&self, input: &EngineInput, run: &SelectionRun) -> EngineOutput {
        let mut validation: Vec<Limit> = run
            .rejection_summary
            .iter()
            .map(|(reason, count)| Limit {
                field: "run".into(),
                message: format!(
                    "the engine refused {count} of the candidate combinations: {reason}"
                ),
                value: *count as f64,
                unit: "combinations".into(),
                min: None,
                max: None,
            })
            .collect();
        // The engine's own band check on the ratio this run asked for: named, with the record's
        // band, and never a ratio clamped into it.
        if let Some(limit) = fan_speed_ratio_limit(
            &input.fan.id,
            input.fan.allowed_speed_ratio,
            input.speed_ratio,
        ) {
            validation.insert(
                0,
                Limit {
                    field: "fan.speedRatio".into(),
                    message: format!(
                        "the run asked for speed ratio {:.3} on fan \"{}\", outside the record's own validity band {:.2}-{:.2} (allowedSpeedRatio): the engine returns no numbers instead of a ratio clamped into the band",
                        limit.speed_ratio, limit.fan_id, limit.min, limit.max
                    ),
                    value: limit.speed_ratio,
                    unit: "ratio".into(),
                    min: Some(limit.min),
                    max: Some(limit.max),
                },
            );
        }
        if validation.is_empty() {
            validation.push(Limit {
                field: "run".into(),
                message: format!(
                    "the engine returned no candidate for tower \"{}\" with the fill stack \"{}\" at speed ratio {}: its own compatibility, envelope and limit checks accepted none of the combinations",
                    input.tower.id,
                    stack_from(input).label(),
                    input.speed_ratio
                ),
                value: 0.0,
                unit: "candidates".into(),
                min: None,
                max: None,
            });
        }
        EngineOutput {
            capability_pct: 0.0,
            water_flow_m3_hr: input.duty.water_flow_m3_hr,
            cold_water_c: 0.0,
            range_c: 0.0,
            approach_c: 0.0,
            airflow_m3_s: 0.0,
            fan_power_kw: 0.0,
            total_pressure_pa: 0.0,
            total_pressure_mmwg: 0.0,
            evaporation_pct: 0.0,
            makeup_m3_hr: 0.0,
            kavl_total: 0.0,
            kavl_per_layer: Vec::new(),
            pressure_by_zone: Vec::new(),
            worked_steps: Vec::new(),
            thermal_curve: empty_thermal_curve(input),
            fan_system_curve: empty_fan_system_curve(),
            validation,
            provenance: self.provenance(run.warning.clone()),
        }
    }

    fn provenance(&self, warning: String) -> Provenance {
        let catalog: &CatalogMetadata = &self.catalog.metadata;
        Provenance {
            engine: self.name().to_string(),
            catalog_id: catalog.id.clone(),
            catalog_revision: catalog.revision.clone(),
            catalog_status: catalog.status.clone(),
            warning: if warning.is_empty() {
                // No run reported a warning (the refusal path): the engine's own constant stands in.
                synergy_drafthouse::SELECTION_WARNING.to_string()
            } else {
                warning
            },
        }
    }

    /// The contract's per-layer results, in the stack's order (index 0 = top): one [`LayerResult`]
    /// per engine layer, every number read from the engine's own layered result
    /// ([`synergy_drafthouse::FillLayerResult`]) - the adapter re-sums nothing.
    fn layer_results(
        &self,
        input: &EngineInput,
        candidate: &SelectionCandidate,
        kavl_total: f64,
    ) -> Result<Vec<LayerResult>, EngineError> {
        candidate
            .fill_layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let fill = self.fill(&layer.fill_id).ok_or_else(|| {
                    EngineError::Schema(format!(
                        "fill record \"{}\" is not in the engine's catalog",
                        layer.fill_id
                    ))
                })?;
                // The engine's own envelope check for this layer's fill, at the loadings the stack
                // sees - the same check the run's own candidate filter applied.
                let envelope = check_fill_operating_envelope(FillOperatingEnvelopeInput {
                    fill: &fill.physics,
                    water_loading_kg_m2_s: candidate.airside.water_loading_kg_m2_s,
                    dry_air_loading_kg_m2_s: candidate.airside.dry_air_loading_kg_m2_s,
                    hot_water_c: input.duty.hot_water_c,
                    water_quality_class: &input.duty.water_quality_class,
                });
                Ok(LayerResult {
                    index,
                    fill_id: layer.fill_id.clone(),
                    depth_m: layer.depth_m,
                    kavl: layer.merkel_number,
                    pressure_pa: layer.pressure_drop_pa,
                    cooling_share_pct: 100.0 * layer.merkel_number / kavl_total,
                    inside_envelope: envelope.ok,
                })
            })
            .collect()
    }

    /// The per-run catalog: one tower (carrying the input's own layer stack), one drift eliminator,
    /// one fan, one nozzle and every fill record the stack's layers name - the combination the
    /// cockpit's input declares.
    fn run_catalog(&self, input: &EngineInput) -> Result<SelectionCatalog, EngineError> {
        let catalog_tower = self.tower(&input.tower.id).ok_or_else(|| {
            EngineError::Schema(format!(
                "tower \"{}\" is not in the engine's catalog: the contract's tower record carries no \
                 spray/rain zone correlations, so the engine cannot evaluate an unknown tower \
                 without understating its transfer",
                input.tower.id
            ))
        })?;
        let mut fills: Vec<SelectionFill> = Vec::with_capacity(input.fill_layers.len());
        for layer in &input.fill_layers {
            let fill = self.fill(&layer.fill_id).ok_or_else(|| {
                EngineError::Schema(format!(
                    "fill record \"{}\" is not in the engine's catalog: the contract's fill layer names a \
                     fill and a depth; the thermal and pressure correlations are catalog records",
                    layer.fill_id
                ))
            })?;
            if !fills
                .iter()
                .any(|known| known.physics.id == fill.physics.id)
            {
                fills.push(fill.clone());
            }
        }
        let (run_thermal_multiplier, run_pressure_multiplier) = run_level_multipliers(input);

        let mut water_quality_factors: HashMap<String, WaterQualityFactor> = HashMap::new();
        water_quality_factors.insert(
            input.duty.water_quality_class.clone(),
            WaterQualityFactor {
                // The pair the stack's layers declare is the run's quality pair - see
                // [`run_level_multipliers`]; the cockpit's contract states it per layer and has no
                // run-level field of its own.
                thermal_multiplier: run_thermal_multiplier,
                pressure_multiplier: run_pressure_multiplier,
            },
        );

        Ok(SelectionCatalog {
            metadata: self.catalog.metadata.clone(),
            water_quality_factors,
            towers: vec![SelectionTower {
                id: input.tower.id.clone(),
                tower_type: TowerType::parse(&input.tower.tower_type).ok_or_else(|| {
                    EngineError::Schema(format!(
                        "tower \"{}\": unknown type \"{}\"",
                        input.tower.id, input.tower.tower_type
                    ))
                })?,
                physics: merged_tower(&input.tower, &catalog_tower.physics),
                footprint_m2: input.tower.footprint_m2,
                max_water_mass_flow_kg_s: input.tower.max_water_mass_flow_kg_s,
                // The declared stack replaces the single-fill depth enumeration: the engine selects
                // over the stack, so these options are not read. They stay the layers' own depths
                // so the field keeps naming the depths the input declared.
                fill_depth_options_m: input
                    .fill_layers
                    .iter()
                    .map(|layer| layer.depth_m)
                    .collect(),
                fill_stacks: vec![stack_from(input)],
                compatible_fan_ids: input.tower.compatible_fan_ids.clone(),
            }],
            fills,
            drift_eliminators: vec![SelectionDriftEliminator {
                physics: DriftEliminatorRecord {
                    id: input.drift.id.clone(),
                    curve: input
                        .drift
                        .curve
                        .iter()
                        .map(|point| DriftCurvePoint {
                            face_velocity_ms: point.face_velocity_m_s,
                            drift_ppm: point.drift_ppm,
                            pressure_drop_pa: point.pressure_drop_pa,
                        })
                        .collect(),
                },
                max_water_temperature_c: input.drift.max_water_temperature_c,
            }],
            fans: vec![SelectionFan {
                allowed_speed_ratio: input.fan.allowed_speed_ratio,
                // The record's own rated speed, carried into the engine's record: the rpm read-out
                // is `speed_ratio x nominal_rpm`, and `None` here means the record states none.
                nominal_rpm: input.fan.nominal_rpm,
                drive_efficiency: input.fan.drive_efficiency,
                motor_efficiency: input.fan.motor_efficiency,
                physics: engine_fan(&input.fan)?,
            }],
            nozzles: vec![EngineNozzleRecord {
                id: input.nozzle.id.clone(),
                name: input.nozzle.name.clone(),
                discharge_coefficient: input.nozzle.discharge_coefficient,
                orifice_diameter_m: input.nozzle.orifice_diameter_m,
                reference_water_density_kg_m3: input.nozzle.reference_water_density_kg_m3,
            }],
        })
    }
}

/// The run-level derating pair the engine's correlation form carries: one pair per run, multiplied
/// into every layer's own pair (and applied to the spray and rain zones above and below the stack).
///
/// The cockpit's contract states the pair **per layer** and carries no run-level pair. The stack's
/// top layer's declared pair is the pair the single-layer mapping already puts at the run level
/// (`0.92`/`1.12` for the fixture's moderate class), so a stack whose layers declare one pair - what
/// the cockpit's own water-quality control writes into every layer - maps exactly, and each other
/// layer's declared pair is carried relative to it ([`own_multipliers`]) so no layer's terms lose
/// the pair its own layer declares.
fn run_level_multipliers(input: &EngineInput) -> (f64, f64) {
    input
        .fill_layers
        .first()
        .map(|layer| (layer.thermal_multiplier, layer.pressure_multiplier))
        .unwrap_or((1.0, 1.0))
}

/// A declared layer's pair as the engine's own per-layer pair, relative to the run-level pair: the
/// engine multiplies the two together, so the product is exactly the pair the layer declares.
fn own_multipliers(layer: &FillLayer, run: (f64, f64)) -> (f64, f64) {
    (
        layer.thermal_multiplier / run.0,
        layer.pressure_multiplier / run.1,
    )
}

/// The cockpit's ordered layers as the engine's own fill stack: the same fills, depths and order
/// (index 0 = top), one layer per declared layer - nothing is reordered, collapsed or invented. The
/// multiplier mapping is [`run_level_multipliers`] / [`own_multipliers`].
fn stack_from(input: &EngineInput) -> FillStack {
    let run = run_level_multipliers(input);
    FillStack::new(
        input
            .fill_layers
            .iter()
            .map(|layer| {
                let (thermal, pressure) = own_multipliers(layer, run);
                EngineFillLayer::new(layer.fill_id.clone(), layer.depth_m)
                    .with_thermal_multiplier(thermal)
                    .with_pressure_multiplier(pressure)
            })
            .collect(),
    )
}

/// The input's layers resolved against the run catalog's own records, in the declared order: the
/// engine's layered-breakdown input. The catalog was built from these same layers, so every id
/// resolves; a missing one is still a named schema error rather than an assumption.
fn stack_terms<'a>(
    input: &EngineInput,
    catalog: &'a SelectionCatalog,
) -> Result<Vec<LayerTerms<'a>>, EngineError> {
    let run = run_level_multipliers(input);
    input
        .fill_layers
        .iter()
        .map(|layer| {
            let fill = catalog
                .fills
                .iter()
                .find(|fill| fill.physics.id == layer.fill_id)
                .ok_or_else(|| {
                    EngineError::Schema(format!(
                        "fill record \"{}\" is not in the engine's catalog",
                        layer.fill_id
                    ))
                })?;
            let (thermal, pressure) = own_multipliers(layer, run);
            Ok(LayerTerms {
                fill: &fill.physics,
                depth_m: layer.depth_m,
                thermal_multiplier: thermal,
                pressure_multiplier: pressure,
            })
        })
        .collect()
}

/// The input's tower record over the catalog record's zone correlations. The contract carries no
/// field for `sprayZone`/`rainZone`, so those two are the catalog's; every field it does carry is
/// the input's.
fn merged_tower(input: &TowerRecord, catalog: &EngineTowerRecord) -> EngineTowerRecord {
    EngineTowerRecord {
        fill_area_m2: input.fill_area_m2,
        // The contract states these as numbers; the engine takes them as options with its own
        // defaults. `Some` is what "the input states this" means.
        air_free_area_m2: Some(input.air_free_area_m2),
        drift_area_m2: Some(input.drift_area_m2),
        inlet_area_m2: Some(input.inlet_area_m2),
        plenum_area_m2: input.plenum_area_m2,
        fan_stack_area_m2: input.fan_stack_area_m2,
        stack_recovery_factor: Some(input.stack_recovery_factor),
        inlet_loss_coefficient: Some(input.inlet_loss_coefficient),
        distribution_loss_coefficient: Some(input.distribution_loss_coefficient),
        support_loss_coefficient: Some(input.support_loss_coefficient),
        plenum_loss_coefficient: Some(input.plenum_loss_coefficient),
        fixed_pressure_loss_pa: Some(input.fixed_pressure_loss_pa),
        spray_zone_height_m: Some(input.spray_zone_height_m),
        rain_zone_height_m: Some(input.rain_zone_height_m),
        spray_zone: catalog.spray_zone,
        rain_zone: catalog.rain_zone,
    }
}

/// The cockpit's fan record as the engine's. The cockpit's record carries every field the engine's
/// fan record does; the only translation is the pressure-basis spelling.
fn engine_fan(input: &ContractFanRecord) -> Result<FanRecord, EngineError> {
    Ok(FanRecord {
        id: input.id.clone(),
        stack_area_m2: Some(input.stack_area_m2),
        pressure_basis: Some(match input.pressure_basis.as_str() {
            "total" => FanPressureBasis::Total,
            "static" => FanPressureBasis::Static,
            other => {
                return Err(EngineError::Schema(format!(
                    "fan \"{}\": unknown pressure basis \"{other}\"",
                    input.id
                )))
            }
        }),
        // The contract states it as a number; the engine takes it as an option with its own default.
        reference_density_kg_m3: Some(input.reference_density_kg_m3),
        stack_recovery_factor: input.stack_recovery_factor,
        curve: input
            .curve
            .iter()
            .map(|point| FanCurvePoint {
                flow_m3_s: point.flow_m3_s,
                pressure_pa: point.pressure_pa,
                efficiency: point.efficiency,
            })
            .collect(),
    })
}

impl Engine for RealEngine {
    fn name(&self) -> &str {
        // Shown by the UI and carried in the provenance line. No claim beyond what it is: the Rust
        // engine crate's selection path over the ordered fill stack the input declares.
        "RealEngine (the Rust engine crate, over the ordered fill stack)"
    }

    /// One run: the cockpit's input -> the engine's selector -> the cockpit's output.
    ///
    /// The mapping, field by field:
    ///
    /// * **Records.** The tower (its own dimensions and losses) and every fill the stack's layers
    ///   name come from the catalog by id - the contract's tower record has no `sprayZone`/`rainZone`
    ///   and its fill layers carry no correlation fields; the drift eliminator, fan and nozzle are the
    ///   input's own records. An id the catalog does not carry is an [`EngineError::Schema`], like the
    ///   baseline's unknown-fill refusal.
    /// * **Water flow.** The cockpit's duty is m3/hr; the engine takes kg/s. The conversion uses the
    ///   engine's own water density at the duty's mean water temperature and salinity
    ///   ([`water_density_kg_m3`]) - never 1000 kg/m3.
    /// * **The fill stack.** The input's `fill_layers` are the engine's ordered stack, top first,
    ///   field for field: the same fills, the same depths, one layer per declared layer. Their
    ///   multipliers are the run's quality pair ([`run_level_multipliers`]) - the engine's correlation
    ///   form carries exactly one run-level pair, and the cockpit's contract states it per layer; the
    ///   catalog's per-class factors are not consulted.
    /// * **Everything else** the engine's [`SelectionRequirements`] defaults: the cockpit's contract
    ///   carries no drift-ppm, power, footprint, nozzle-drop or thermal-margin field, so the engine's
    ///   own defaults stand - except the thermal margin, which is 0.0 because the cockpit's duty is
    ///   the constraint at the temperature the contract states.
    /// * **Per-layer numbers.** One contract [`LayerResult`] per layer, in the stack's order, each
    ///   read from the engine's own layered result: `kavl` is the layer's `merkel_number`,
    ///   `pressure_pa` its `pressure_drop_pa`, `inside_envelope` the engine's own
    ///   [`check_fill_operating_envelope`] for that layer's fill, and `cooling_share_pct` its share of
    ///   the total available transfer (fill + spray + rain). Nothing is re-summed by the adapter.
    /// * **Zones.** The engine's air-side breakdown, in air-path order, with the baseline's labels:
    ///   one [`ZoneId::Fill`] zone per layer, lowest first (air rises), each carrying its layer's own
    ///   drop.
    /// * **Charts.** The performance series is the engine run at each water flow of the recorded axis;
    ///   the resistance series is the engine's layered air-side breakdown on the recorded flow grid;
    ///   the fan series is the engine's own [`fan_pressure_pa_at_flow`] at the run's speed ratio.
    /// * **Worked steps.** The quantities the engine's sheet names, read from the candidate it
    ///   returned - no second derivation of any of them.
    ///
    /// **What it refuses.** An empty stack, a non-positive depth or multiplier, a negative fan-curve
    /// point, or an id the catalog does not carry: [`EngineError::Schema`], naming the reason. A stack
    /// the engine's own compatibility, envelope and limit checks accept none of is reported as
    /// [`Limit`]s in `validation`, not as an error.
    fn run(&self, input: &EngineInput) -> Result<EngineOutput, EngineError> {
        check_layers(input)?;
        let water_mass_flow_kg_s = water_mass_flow_kg_s(input)?;
        let requirements = SelectionRequirements {
            water_mass_flow_kg_s,
            hot_water_c: input.duty.hot_water_c,
            target_cold_water_c: input.duty.target_cold_water_c,
            wet_bulb_c: input.duty.wet_bulb_c,
            dry_bulb_c: input.duty.dry_bulb_c,
            pressure_pa: input.duty.pressure_pa,
            salinity_g_kg: input.duty.salinity_g_kg,
            water_quality_class: input.duty.water_quality_class.clone(),
            cycles_of_concentration: input.duty.cycles_of_concentration,
            // The cockpit's duty is the constraint at the temperature it states: no extra margin.
            minimum_thermal_margin_c: 0.0,
            speed_ratios: vec![input.speed_ratio],
            ..SelectionRequirements::default()
        };
        let catalog = self.run_catalog(input)?;
        let run =
            run_selection(&SelectionInput::new(&catalog).with_requirements(requirements.clone()))
                .map_err(|error| EngineError::Schema(error.to_string()))?;

        let Some(candidate) = run.candidates.first().cloned() else {
            return Ok(self.refused_output(input, &run));
        };

        // The density the run itself used, between the two numbers the engine reported.
        let water_density = water_mass_flow_kg_s / candidate.water_volumetric_flow_m3_s;
        let inlet_air = psychrometric_state(
            PsychrometricStateInput::from_wet_bulb(input.duty.dry_bulb_c, input.duty.wet_bulb_c)
                .with_pressure(input.duty.pressure_pa),
        )
        .map_err(|error| EngineError::Schema(error.to_string()))?;
        let layers = stack_terms(input, &catalog)?;
        let charts = ChartContext {
            tower: &catalog.towers[0].physics,
            drift: &catalog.drift_eliminators[0].physics,
            fan: &catalog.fans[0].physics,
            layers: &layers,
            run_multipliers: run_level_multipliers(input),
            inlet_air: &inlet_air,
            water_mass_flow_kg_s,
        };

        let kavl_total = candidate.airside.available_merkel_number;
        let kavl_per_layer = self.layer_results(input, &candidate, kavl_total)?;

        Ok(EngineOutput {
            capability_pct: 100.0 * candidate.capability_ratio.unwrap_or(0.0),
            water_flow_m3_hr: candidate.water_volumetric_flow_m3_s * 3600.0,
            cold_water_c: candidate.thermal.cold_water_c,
            range_c: candidate.thermal.range_c,
            approach_c: candidate.thermal.approach_c,
            airflow_m3_s: candidate.fan_operating_point.flow_m3_s,
            fan_power_kw: candidate.fan_operating_point.shaft_power_kw,
            total_pressure_pa: candidate.airside.total_pa,
            total_pressure_mmwg: candidate.airside.total_pa * MMWG_PER_PA,
            evaporation_pct: 100.0 * candidate.water_balance.evaporation_kg_s
                / water_mass_flow_kg_s,
            makeup_m3_hr: candidate.water_balance.makeup_kg_s / water_density * 3600.0,
            kavl_total,
            kavl_per_layer,
            pressure_by_zone: pressure_zones(&candidate.airside, &candidate.fill_layers, input),
            worked_steps: worked_steps(input, &candidate, self.name(), water_mass_flow_kg_s),
            thermal_curve: charts.thermal_curve(
                input,
                &catalog,
                &requirements,
                candidate.thermal.cold_water_c,
                water_density,
            )?,
            fan_system_curve: charts.fan_system_curve(&candidate)?,
            validation: Vec::new(),
            provenance: self.provenance(run.warning.clone()),
        })
    }
}

/// The stack the cockpit declared, checked before any physics: at least one layer, and every layer's
/// depth and multipliers positive and finite. Each refusal names the layer it is about. (The fan
/// record's own non-negativity check rides along here, where it always did.)
fn check_layers(input: &EngineInput) -> Result<(), EngineError> {
    if input.fill_layers.is_empty() {
        return Err(EngineError::Schema(
            "fill stack is empty: at least one fill layer is required".into(),
        ));
    }
    for layer in &input.fill_layers {
        if !(layer.depth_m > 0.0) {
            return Err(EngineError::Schema(format!(
                "fill layer {}: depth_m must be positive, got {}",
                layer.fill_id, layer.depth_m
            )));
        }
        if !(layer.thermal_multiplier.is_finite() && layer.thermal_multiplier > 0.0) {
            return Err(EngineError::Schema(format!(
                "fill layer {}: thermal_multiplier must be positive and finite, got {}",
                layer.fill_id, layer.thermal_multiplier
            )));
        }
        if !(layer.pressure_multiplier.is_finite() && layer.pressure_multiplier > 0.0) {
            return Err(EngineError::Schema(format!(
                "fill layer {}: pressure_multiplier must be positive and finite, got {}",
                layer.fill_id, layer.pressure_multiplier
            )));
        }
    }
    for point in &input.fan.curve {
        if !(point.pressure_pa >= 0.0) || !(point.flow_m3_s >= 0.0) {
            return Err(EngineError::Schema(format!(
                "fan record {}: curve points must be non-negative",
                input.fan.id
            )));
        }
    }
    Ok(())
}

/// The run's water mass flow in kg/s: the cockpit's duty converted at the engine's own water density
/// for the duty's mean water temperature.
fn water_mass_flow_kg_s(input: &EngineInput) -> Result<f64, EngineError> {
    let mean_water_c = (input.duty.hot_water_c + input.duty.target_cold_water_c) / 2.0;
    let density = water_density_kg_m3(mean_water_c, input.duty.salinity_g_kg)
        .map_err(|error| EngineError::Schema(error.to_string()))?;
    Ok(input.duty.water_flow_m3_hr / 3600.0 * density)
}

/// The engine-side records one run's charts read.
struct ChartContext<'a> {
    tower: &'a EngineTowerRecord,
    drift: &'a DriftEliminatorRecord,
    fan: &'a FanRecord,
    /// The run's resolved layers: the same terms the selector swept.
    layers: &'a [LayerTerms<'a>],
    /// The run-level derating pair ([`run_level_multipliers`]) - the same pair the run catalog
    /// carried, so the chart's resistance curve is the curve the operating point was solved on.
    run_multipliers: (f64, f64),
    inlet_air: &'a PsychrometricState,
    water_mass_flow_kg_s: f64,
}

impl ChartContext<'_> {
    /// The engine's layered air-side breakdown at one air flow, at the run's water flow.
    fn breakdown(
        &self,
        volumetric_air_flow_m3_s: f64,
    ) -> Result<SystemPressureBreakdown, EngineError> {
        Ok(layered_system_pressure_breakdown(LayeredBreakdownInput {
            tower: self.tower,
            layers: self.layers,
            drift_eliminator: self.drift,
            fan: Some(self.fan),
            volumetric_air_flow_m3_s,
            dry_air_density_kg_m3: self.inlet_air.dry_air_density_kg_m3,
            moist_air_density_kg_m3: self.inlet_air.moist_air_density_kg_m3,
            water_mass_flow_kg_s: self.water_mass_flow_kg_s,
            thermal_multiplier: self.run_multipliers.0,
            pressure_multiplier: self.run_multipliers.1,
        })
        .map_err(|error| EngineError::Schema(error.to_string()))?
        .breakdown)
    }

    /// The tower's resistance on the recorded flow grid.
    fn system_series(&self) -> Result<Series, EngineError> {
        let mut points = Vec::new();
        for flow_m3_s in resistance_chart_flows_m3_s() {
            points.push(XY {
                x: flow_m3_s,
                y: self.breakdown(flow_m3_s)?.total_pa,
            });
        }
        Ok(Series {
            label: "system - tower resistance".into(),
            points,
        })
    }

    /// The fan's curve at the run's speed ratio, from the engine's own curve evaluation.
    fn fan_series(&self, speed_ratio: f64) -> Result<Series, EngineError> {
        let mut points = Vec::new();
        for point in &self.fan.curve {
            let flow_m3_s = point.flow_m3_s * speed_ratio;
            points.push(XY {
                x: flow_m3_s,
                y: fan_pressure_pa_at_flow(
                    self.fan,
                    flow_m3_s,
                    speed_ratio,
                    self.inlet_air.moist_air_density_kg_m3,
                )
                .map_err(|error| EngineError::Schema(error.to_string()))?,
            });
        }
        Ok(Series {
            label: format!("fan {} at speed ratio {:.2}", self.fan.id, speed_ratio),
            points,
        })
    }

    fn fan_system_curve(
        &self,
        candidate: &SelectionCandidate,
    ) -> Result<FanSystemCurve, EngineError> {
        Ok(FanSystemCurve {
            x_label: "airflow (m3/s)".into(),
            y_label: "pressure (Pa)".into(),
            fan: self.fan_series(candidate.speed_ratio)?,
            system: self.system_series()?,
            operating_point: XY {
                x: candidate.fan_operating_point.flow_m3_s,
                y: candidate.fan_operating_point.fan_pressure_pa,
            },
        })
    }

    /// Cold water vs water flow: the engine run at each water flow of the recorded axis, converted to
    /// m3/hr at the run's own water density. A flow the engine refuses (no candidate at all)
    /// contributes no point - the recorded sweep marks the same condition `infeasible`.
    fn thermal_curve(
        &self,
        input: &EngineInput,
        catalog: &SelectionCatalog,
        requirements: &SelectionRequirements,
        operating_cold_water_c: f64,
        water_density: f64,
    ) -> Result<ThermalCurve, EngineError> {
        let mut points = Vec::new();
        for water_mass_flow_kg_s in performance_chart_water_mass_flow_kg_s() {
            let mut sweep_requirements = requirements.clone();
            sweep_requirements.water_mass_flow_kg_s = water_mass_flow_kg_s;
            let Ok(run) =
                run_selection(&SelectionInput::new(catalog).with_requirements(sweep_requirements))
            else {
                continue;
            };
            let Some(sweep_candidate) = run.candidates.first() else {
                continue;
            };
            points.push(XY {
                x: water_mass_flow_kg_s / water_density * 3600.0,
                y: sweep_candidate.thermal.cold_water_c,
            });
        }
        let first = points.first().map(|point| point.x).unwrap_or(0.0);
        let last = points.last().map(|point| point.x).unwrap_or(0.0);
        Ok(ThermalCurve {
            x_label: "water flow (m3/hr)".into(),
            y_label: "cold water (C)".into(),
            performance: Series {
                label: "performance - cold water the tower reaches".into(),
                points,
            },
            demand: Series {
                label: "demand - target cold water".into(),
                points: vec![
                    XY {
                        x: first,
                        y: input.duty.target_cold_water_c,
                    },
                    XY {
                        x: last,
                        y: input.duty.target_cold_water_c,
                    },
                ],
            },
            operating_point: XY {
                x: input.duty.water_flow_m3_hr,
                y: operating_cold_water_c,
            },
        })
    }
}

/// The engine's air-side breakdown as the cockpit's zones, in air-path order, with the baseline's
/// labels. The air rises, so the fill zones run **lowest layer first** - the baseline implementation
/// orders them the same way - and each carries its layer's index into `EngineInput::fill_layers`.
fn pressure_zones(
    airside: &SystemPressureBreakdown,
    layers: &[FillLayerResult],
    input: &EngineInput,
) -> Vec<PressureZone> {
    let mut zones = vec![
        zone(
            ZoneId::Inlet,
            None,
            "inlet louvres".into(),
            airside.inlet_pa,
            airside.total_pa,
        ),
        zone(
            ZoneId::Rain,
            None,
            "rain zone + supports".into(),
            airside.support_pa,
            airside.total_pa,
        ),
    ];
    for layer in layers.iter().rev() {
        zones.push(zone(
            ZoneId::Fill,
            Some(layer.position - 1),
            format!("fill {} ({} m)", layer.fill_id, trim(layer.depth_m)),
            layer.pressure_drop_pa,
            airside.total_pa,
        ));
    }
    zones.extend([
        zone(
            ZoneId::Spray,
            None,
            "spray / distribution".into(),
            airside.distribution_pa,
            airside.total_pa,
        ),
        zone(
            ZoneId::Drift,
            None,
            format!("drift eliminator {}", input.drift.id),
            airside.drift_pa,
            airside.total_pa,
        ),
        zone(
            ZoneId::Plenum,
            None,
            "plenum".into(),
            airside.plenum_pa,
            airside.total_pa,
        ),
        zone(
            ZoneId::Stack,
            None,
            "fan stack".into(),
            airside.fan_stack_pa,
            airside.total_pa,
        ),
        zone(
            ZoneId::Fixed,
            None,
            "fixed losses".into(),
            airside.fixed_pa,
            airside.total_pa,
        ),
    ]);
    zones
}

fn zone(
    zone: ZoneId,
    layer: Option<usize>,
    label: String,
    pressure_pa: f64,
    total_pa: f64,
) -> PressureZone {
    PressureZone {
        zone,
        layer,
        label,
        pressure_pa,
        share_pct: if total_pa > 0.0 {
            100.0 * pressure_pa / total_pa
        } else {
            0.0
        },
    }
}

/// The quantities the engine's own worked sheet names, read from the candidate it returned. No value
/// here is derived a second time: each is the engine's field for that quantity (or the sum of two of
/// them, where the sheet names one).
fn worked_steps(
    input: &EngineInput,
    candidate: &SelectionCandidate,
    engine_name: &str,
    water_mass_flow_kg_s: f64,
) -> Vec<WorkedStep> {
    let step = |label: &str, why: &str, value: f64, unit: &str| WorkedStep {
        label: label.into(),
        why: why.into(),
        formula: None,
        substitution: None,
        value: Some(value),
        unit: unit.into(),
        reference: None,
        kind: "calc".into(),
    };
    vec![
        WorkedStep {
            label: "Real engine".into(),
            why: "This run comes from the Rust engine crate's own selection path: its Merkel solve, \
                  psychrometrics, air-side breakdown and fan balance. The contract's input is mapped \
                  onto that path, and every number below is a field of the candidate it returned."
                .into(),
            formula: None,
            substitution: None,
            value: None,
            unit: String::new(),
            reference: Some(engine_name.into()),
            kind: "note".into(),
        },
        step(
            "Duty water flow",
            "The circulating water the duty asks for, at the engine's water density.",
            water_mass_flow_kg_s,
            "kg/s",
        ),
        step(
            "Range",
            "Hot water minus the target cold water.",
            input.duty.hot_water_c - input.duty.target_cold_water_c,
            "K",
        ),
        step(
            "Heat the water gives up",
            "Water mass flow times specific heat times the range.",
            candidate.thermal.heat_transfer_kw,
            "kW",
        ),
        step(
            "Dry-air mass flow",
            "The air the fan balance moves, from the engine's own breakdown.",
            candidate.airside.dry_air_mass_flow_kg_s,
            "kg/s",
        ),
        step(
            "Fill transfer demand",
            "KaV/L the fill supplies at this loading and depth.",
            candidate.airside.fill_merkel_number,
            "-",
        ),
        step(
            "Spray and rain zone demand",
            "KaV/L the spray and rain zones supply.",
            candidate.airside.spray_zone_merkel_number + candidate.airside.rain_zone_merkel_number,
            "-",
        ),
        step(
            "Total demand",
            "Fill, spray and rain together: the tower's available transfer.",
            candidate.airside.available_merkel_number,
            "-",
        ),
        step(
            "Available transfer",
            "The same total the thermal solve ran against.",
            candidate.airside.available_merkel_number,
            "-",
        ),
        step(
            "Capability ratio",
            "Available transfer over the demand of the duty.",
            candidate.capability_ratio.unwrap_or(0.0),
            "-",
        ),
        step(
            "Cold-water temperature, solved",
            "The water temperature the tower reaches at this duty.",
            candidate.thermal.cold_water_c,
            "degC",
        ),
        step(
            "Air-side pressure demand",
            "Every air-path loss at the operating point, summed.",
            candidate.airside.total_pa,
            "Pa",
        ),
        step(
            "Fan operating point",
            "The flow where the speed-scaled fan curve meets the tower's resistance.",
            candidate.fan_operating_point.flow_m3_s,
            "m3/s",
        ),
        step(
            "Fan shaft power",
            "Flow times pressure over the efficiency at that point.",
            candidate.fan_operating_point.shaft_power_kw,
            "kW",
        ),
        step(
            "Evaporation",
            "The water the air carries off, from the dry-air mass balance.",
            candidate.water_balance.evaporation_kg_s,
            "kg/s",
        ),
        step(
            "Make-up water",
            "Evaporation plus bleed and drift.",
            candidate.water_balance.makeup_kg_s,
            "kg/s",
        ),
        step(
            "Capacity",
            "The water flow the unit holds at the required cold water.",
            candidate.capacity_kg_s.unwrap_or(0.0),
            "kg/s",
        ),
    ]
}

fn empty_thermal_curve(input: &EngineInput) -> ThermalCurve {
    ThermalCurve {
        x_label: "water flow (m3/hr)".into(),
        y_label: "cold water (C)".into(),
        performance: Series {
            label: "performance - cold water the tower reaches".into(),
            points: Vec::new(),
        },
        demand: Series {
            label: "demand - target cold water".into(),
            points: vec![
                XY {
                    x: input.duty.water_flow_m3_hr,
                    y: input.duty.target_cold_water_c,
                },
                XY {
                    x: input.duty.water_flow_m3_hr,
                    y: input.duty.target_cold_water_c,
                },
            ],
        },
        operating_point: XY {
            x: input.duty.water_flow_m3_hr,
            y: 0.0,
        },
    }
}

fn empty_fan_system_curve() -> FanSystemCurve {
    FanSystemCurve {
        x_label: "airflow (m3/s)".into(),
        y_label: "pressure (Pa)".into(),
        fan: Series {
            label: String::new(),
            points: Vec::new(),
        },
        system: Series {
            label: "system - tower resistance".into(),
            points: Vec::new(),
        },
        operating_point: XY { x: 0.0, y: 0.0 },
    }
}

/// The baseline's own two-decimal trim, so a label reads `1.5 m` and not `1.50 m`.
fn trim(value: f64) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
