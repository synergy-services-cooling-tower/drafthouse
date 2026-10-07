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
use cockpit::fixture_engine::{FillDepth, FixtureCatalog};
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
    /// The tower's own single-fill total-depth list (`fillDepthOptionsM`), metres.
    fill_depth_options_m: Vec<f64>,
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
    build(head.catalog)
}

/// The same catalog, built from a **catalog revision's** `records` block (issue #74). The revision's
/// records are the fixture's own `catalog` object, so this is the same builder - an imported revision and
/// the shipped one cannot drift apart in how they are read.
pub fn catalog_from_records(records: &Value) -> Result<SelectionCatalog, String> {
    let block: CatalogBlock = serde_json::from_value(records.clone())
        .map_err(|e| format!("catalog revision records: {e}"))?;
    build(block)
}

/// Issue #136: each catalog fill's own height spec, read through the contract's schema (so a fill
/// without one is refused here as well, by name) - what the real engine holds a run's layers to.
pub fn fill_depths_from_fixture(fixture_text: &str) -> Result<HashMap<String, FillDepth>, String> {
    #[derive(Deserialize)]
    struct Head {
        catalog: FixtureCatalog,
    }
    let head: Head = serde_json::from_str(fixture_text).map_err(|e| format!("fixture: {e}"))?;
    Ok(head.catalog.fill_depths)
}

fn build(catalog: CatalogBlock) -> Result<SelectionCatalog, String> {
    let catalog = &catalog;

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
            // The tower record's own depth list (metres), never an area (issue #136). The adapter's
            // per-run stack depths come from the input's own layers, each held to its fill's spec.
            fill_depth_options_m: tower.fill_depth_options_m,
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

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    /// Issue #136, AC 5: every tower's `fill_depth_options_m` is the record's own `fillDepthOptionsM`
    /// (metres) - never its `fillAreaM2` (the area that used to be assigned here).
    #[test]
    fn a_tower_s_depth_list_is_its_own_never_an_area() {
        let v: Value = serde_json::from_str(FIXTURE).unwrap();
        let cat = catalog_from_fixture(FIXTURE).expect("the catalog builds");
        assert_eq!(cat.towers.len(), 4);
        for t in &cat.towers {
            let rec = v["catalog"]["towers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == t.id.as_str())
                .unwrap();
            let own: Vec<f64> = serde_json::from_value(rec["fillDepthOptionsM"].clone()).unwrap();
            let area = rec["fillAreaM2"].as_f64().unwrap();
            assert_eq!(
                t.fill_depth_options_m, own,
                "{}: the record's own list",
                t.id
            );
            assert!(
                !t.fill_depth_options_m.contains(&area),
                "{}: {area} m2 is an area, not a depth",
                t.id
            );
        }
    }

    /// Issue #136, AC 4 (selector, by hand): the single-fill selector builds one candidate per tower x
    /// compatible fill x tower depth option. Every one of those is one of its fill's own heights, so
    /// the whole-catalog selection never builds a candidate the engine would refuse. Hand-checked:
    /// 1.2/1.5/1.8/2.1 m are 8/10/12/14 x 0.15 m (film, 0.30-2.10) and 4/5/6/7 x 0.30 m (VF38/trickle
    /// 0.60-2.40, splash 0.90-3.00).
    #[test]
    fn every_single_fill_candidate_is_one_of_its_fill_s_heights() {
        let cat = catalog_from_fixture(FIXTURE).expect("the catalog builds");
        let depths = fill_depths_from_fixture(FIXTURE).expect("the specs read");
        let mut n = 0;
        for t in &cat.towers {
            for f in cat
                .fills
                .iter()
                .filter(|f| f.compatible_tower_types.contains(&t.tower_type))
            {
                let spec = &depths[&f.physics.id];
                for d in &t.fill_depth_options_m {
                    assert!(spec.admits(*d), "{} x {} @ {d} m", t.id, f.physics.id);
                    n += 1;
                }
            }
        }
        // 3 counterflow towers x 7 fills x (3 + 4 + 4) depths, 1 crossflow x 4 fills x 3 depths.
        assert_eq!(
            n,
            7 * (3 + 4 + 4) + 4 * 3,
            "the hand count of single-fill candidates"
        );
        assert!(
            !depths["FILM-MF20"].admits(1.25),
            "and an off-module depth is not admitted"
        );
    }
}
