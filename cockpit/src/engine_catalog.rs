//! The fixture's `catalog` block as the engine's **own** selection catalog (issue #58).
//!
//! The real engine is constructed with a [`SelectionCatalog`], not with a file. The cockpit's
//! fixture asset already carries the engine's own records (`assets/fixture.json` → `catalog`), so
//! the engine's catalog is built from that same text at run time, in the browser - one fixture,
//! one set of records, no second copy of the data.
//!
//! The mapping is the one `rust/cockpit-adapter/tests/parity.rs` (`engine_catalog`) uses to build
//! the engine catalog out of this fixture file: same JSON shapes, same fields, same two deliberate
//! empty lists (`fill_stacks`: the fixture's towers declare no stack variants, so the adapter
//! selects over the declared layers; `nozzles`: the adapter takes the nozzle from the input).
//! The adapter's parity gate is what says the numbers the real engine returns for these records
//! agree with the recorded run; this module only builds the records.
//!
//! A record the mapping cannot read is an error naming the record: nothing here invents a field.

use std::collections::HashMap;

// The engine's own record types, through the adapter's re-export (no second path dependency).
use cockpit_adapter::synergy_drafthouse::{
    CatalogMetadata, DriftCurvePoint, FanCurvePoint, FanPressureBasis, FillLimits, FillRecord,
    SelectionCatalog, SelectionDriftEliminator, SelectionFan, SelectionFill, SelectionTower,
    TowerRecord as EngineTowerRecord, TowerType, WaterQualityFactor, ZoneCorrelation,
};
use serde::Deserialize;
use serde_json::Value;

/// The fixture text's `catalog` block, exactly as `assets/fixture.json` carries it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureHead {
    catalog: CatalogBlock,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogBlock {
    metadata: JsonMeta,
    water_quality_factors: HashMap<String, JsonQualityFactor>,
    towers: Vec<Value>,
    fills: Vec<Value>,
    drift_eliminators: Vec<Value>,
    fans: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonMeta {
    id: String,
    revision: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonQualityFactor {
    thermal_multiplier: f64,
    pressure_multiplier: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonTower {
    id: String,
    #[serde(rename = "type")]
    tower_type: String,
    fill_area_m2: f64,
    air_free_area_m2: Option<f64>,
    drift_area_m2: Option<f64>,
    inlet_area_m2: Option<f64>,
    plenum_area_m2: Option<f64>,
    fan_stack_area_m2: Option<f64>,
    stack_recovery_factor: Option<f64>,
    inlet_loss_coefficient: Option<f64>,
    distribution_loss_coefficient: Option<f64>,
    support_loss_coefficient: Option<f64>,
    plenum_loss_coefficient: Option<f64>,
    fixed_pressure_loss_pa: Option<f64>,
    spray_zone_height_m: Option<f64>,
    spray_zone: Option<JsonZone>,
    rain_zone_height_m: Option<f64>,
    rain_zone: Option<JsonZone>,
    footprint_m2: f64,
    max_water_mass_flow_kg_s: f64,
    compatible_fan_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonZone {
    coefficient_per_m: f64,
    reference_water_loading_kg_m2_s: f64,
    reference_dry_air_loading_kg_m2_s: f64,
    water_exponent: f64,
    air_exponent: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFill {
    id: String,
    thermal: Option<JsonZone>,
    pressure: Option<JsonPressure>,
    limits: JsonLimits,
    allowed_water_quality_classes: Vec<String>,
    compatible_tower_types: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonPressure {
    coefficient_pa_per_m: f64,
    reference_water_loading_kg_m2_s: f64,
    reference_dry_air_loading_kg_m2_s: f64,
    water_exponent: f64,
    air_exponent: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonLimits {
    min_water_loading_kg_m2_s: f64,
    max_water_loading_kg_m2_s: f64,
    min_dry_air_loading_kg_m2_s: f64,
    max_dry_air_loading_kg_m2_s: f64,
    max_water_temperature_c: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonDrift {
    id: String,
    max_water_temperature_c: f64,
    curve: Vec<JsonDriftPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonDriftPoint {
    #[serde(rename = "faceVelocityMS")]
    face_velocity_ms: f64,
    drift_ppm: f64,
    pressure_drop_pa: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFan {
    id: String,
    pressure_basis: String,
    stack_area_m2: Option<f64>,
    reference_density_kg_m3: Option<f64>,
    stack_recovery_factor: Option<f64>,
    allowed_speed_ratio: [f64; 2],
    /// The speed the record's recorded curve is published at (speed ratio 1.0), in rpm; absent in a
    /// record that states none.
    #[serde(default)]
    nominal_rpm: Option<f64>,
    drive_efficiency: f64,
    motor_efficiency: f64,
    curve: Vec<JsonFanPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFanPoint {
    flow_m3_s: f64,
    pressure_pa: f64,
    efficiency: f64,
}

/// One record out of the catalog block; a failure names the record it was reading.
fn record<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, String> {
    let id = value.get("id").and_then(Value::as_str).unwrap_or("(no id)");
    serde_json::from_value(value.clone()).map_err(|e| format!("catalog record {id}: {e}"))
}

fn zone_correlation(zone: &JsonZone) -> ZoneCorrelation {
    ZoneCorrelation {
        coefficient_per_m: zone.coefficient_per_m,
        reference_water_loading_kg_m2_s: zone.reference_water_loading_kg_m2_s,
        reference_dry_air_loading_kg_m2_s: zone.reference_dry_air_loading_kg_m2_s,
        water_exponent: zone.water_exponent,
        air_exponent: zone.air_exponent,
    }
}

/// The engine-side catalog the real engine reads, built from the fixture text.
pub fn catalog_from_fixture(fixture_text: &str) -> Result<SelectionCatalog, String> {
    let head: FixtureHead =
        serde_json::from_str(fixture_text).map_err(|e| format!("fixture: {e}"))?;
    let catalog = &head.catalog;

    let mut water_quality_factors: HashMap<String, WaterQualityFactor> = HashMap::new();
    for (class, factor) in &catalog.water_quality_factors {
        water_quality_factors.insert(
            class.clone(),
            WaterQualityFactor {
                thermal_multiplier: factor.thermal_multiplier,
                pressure_multiplier: factor.pressure_multiplier,
            },
        );
    }

    let mut towers: Vec<SelectionTower> = Vec::new();
    for value in &catalog.towers {
        let tower: JsonTower = record(value)?;
        let tower_type = TowerType::parse(&tower.tower_type)
            .ok_or_else(|| format!("catalog record {}: unknown tower type", tower.id))?;
        towers.push(SelectionTower {
            id: tower.id,
            tower_type,
            physics: EngineTowerRecord {
                fill_area_m2: tower.fill_area_m2,
                air_free_area_m2: tower.air_free_area_m2,
                drift_area_m2: tower.drift_area_m2,
                inlet_area_m2: tower.inlet_area_m2,
                plenum_area_m2: tower.plenum_area_m2,
                fan_stack_area_m2: tower.fan_stack_area_m2,
                stack_recovery_factor: tower.stack_recovery_factor,
                inlet_loss_coefficient: tower.inlet_loss_coefficient,
                distribution_loss_coefficient: tower.distribution_loss_coefficient,
                support_loss_coefficient: tower.support_loss_coefficient,
                plenum_loss_coefficient: tower.plenum_loss_coefficient,
                fixed_pressure_loss_pa: tower.fixed_pressure_loss_pa,
                spray_zone_height_m: tower.spray_zone_height_m,
                spray_zone: tower.spray_zone.as_ref().map(zone_correlation),
                rain_zone_height_m: tower.rain_zone_height_m,
                rain_zone: tower.rain_zone.as_ref().map(zone_correlation),
            },
            footprint_m2: tower.footprint_m2,
            max_water_mass_flow_kg_s: tower.max_water_mass_flow_kg_s,
            // Unused: the adapter's per-run fill depth comes from the input's own layers.
            fill_depth_options_m: vec![tower.fill_area_m2],
            // The fixture's towers declare no stack variants: the adapter carries the input's own
            // ordered stack per run, and this catalog supplies the spray/rain correlations.
            fill_stacks: Vec::new(),
            compatible_fan_ids: tower.compatible_fan_ids,
        });
    }

    let mut fills: Vec<SelectionFill> = Vec::new();
    for value in &catalog.fills {
        let fill: JsonFill = record(value)?;
        let mut compatible_tower_types = Vec::new();
        for raw in &fill.compatible_tower_types {
            compatible_tower_types.push(
                TowerType::parse(raw)
                    .ok_or_else(|| format!("catalog record {}: unknown tower type", fill.id))?,
            );
        }
        fills.push(SelectionFill {
            physics: FillRecord {
                id: fill.id,
                thermal: fill.thermal.as_ref().map(zone_correlation),
                pressure: fill.pressure.as_ref().map(|pressure| {
                    cockpit_adapter::synergy_drafthouse::FillPressureCorrelation {
                        coefficient_pa_per_m: pressure.coefficient_pa_per_m,
                        reference_water_loading_kg_m2_s: pressure.reference_water_loading_kg_m2_s,
                        reference_dry_air_loading_kg_m2_s: pressure
                            .reference_dry_air_loading_kg_m2_s,
                        water_exponent: pressure.water_exponent,
                        air_exponent: pressure.air_exponent,
                    }
                }),
                limits: FillLimits {
                    min_water_loading_kg_m2_s: fill.limits.min_water_loading_kg_m2_s,
                    max_water_loading_kg_m2_s: fill.limits.max_water_loading_kg_m2_s,
                    min_dry_air_loading_kg_m2_s: fill.limits.min_dry_air_loading_kg_m2_s,
                    max_dry_air_loading_kg_m2_s: fill.limits.max_dry_air_loading_kg_m2_s,
                    max_water_temperature_c: fill.limits.max_water_temperature_c,
                },
                allowed_water_quality_classes: fill.allowed_water_quality_classes,
            },
            compatible_tower_types,
        });
    }

    let mut drift_eliminators: Vec<SelectionDriftEliminator> = Vec::new();
    for value in &catalog.drift_eliminators {
        let drift: JsonDrift = record(value)?;
        drift_eliminators.push(SelectionDriftEliminator {
            physics: cockpit_adapter::synergy_drafthouse::DriftEliminatorRecord {
                id: drift.id,
                curve: drift
                    .curve
                    .iter()
                    .map(|point| DriftCurvePoint {
                        face_velocity_ms: point.face_velocity_ms,
                        drift_ppm: point.drift_ppm,
                        pressure_drop_pa: point.pressure_drop_pa,
                    })
                    .collect(),
            },
            max_water_temperature_c: drift.max_water_temperature_c,
        });
    }

    let mut fans: Vec<SelectionFan> = Vec::new();
    for value in &catalog.fans {
        let fan: JsonFan = record(value)?;
        let pressure_basis = match fan.pressure_basis.as_str() {
            "total" => FanPressureBasis::Total,
            "static" => FanPressureBasis::Static,
            other => {
                return Err(format!(
                    "catalog record {}: unknown fan pressure basis {other}",
                    fan.id
                ))
            }
        };
        fans.push(SelectionFan {
            physics: cockpit_adapter::synergy_drafthouse::FanRecord {
                id: fan.id,
                stack_area_m2: fan.stack_area_m2,
                pressure_basis: Some(pressure_basis),
                reference_density_kg_m3: fan.reference_density_kg_m3,
                stack_recovery_factor: fan.stack_recovery_factor,
                curve: fan
                    .curve
                    .iter()
                    .map(|point| FanCurvePoint {
                        flow_m3_s: point.flow_m3_s,
                        pressure_pa: point.pressure_pa,
                        efficiency: point.efficiency,
                    })
                    .collect(),
            },
            allowed_speed_ratio: fan.allowed_speed_ratio,
            // The record's own rated speed, into the engine's record (issue #59).
            nominal_rpm: fan.nominal_rpm,
            drive_efficiency: fan.drive_efficiency,
            motor_efficiency: fan.motor_efficiency,
        });
    }

    Ok(SelectionCatalog {
        metadata: CatalogMetadata {
            id: catalog.metadata.id.clone(),
            revision: catalog.metadata.revision.clone(),
            status: catalog.metadata.status.clone(),
        },
        water_quality_factors,
        towers,
        fills,
        drift_eliminators,
        fans,
        // The adapter takes the nozzle from the input (the cockpit's nozzle record is complete),
        // so this catalog carries none - and the adapter never reads this list.
        nozzles: Vec::new(),
    })
}
