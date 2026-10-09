//! Round 4, change 3: **the parameter card's rows** - every field of a record, with its unit.
//!
//! Pure Rust, no Bevy. The card the app draws for a hovered rail chip, picker card or fitted bay is built
//! from these functions, so what the card claims and what the record holds cannot drift: the keys are the
//! engine's own record fields (`cockpit/src/engine.rs`), and the units are the same suffix-derived units
//! the custom-part form uses (`tools/gen-custom-fields.mjs`).
//!
//! `source` is the honest provenance of the value:
//! - `catalog`  - a record from the bundled fixture catalog,
//! - `custom`   - a record the user authored in this session,
//! - `fixture`  - a value read straight out of the fixture the engine was built from.
//!
//! A curve-typed field carries its points instead of a scalar, so the card can draw a sparkline.

use cockpit::engine::{DriftRecord, FanRecord, NozzleRecord};
use cockpit::fixture_engine::FixtureFill;

use crate::custom::trim;

/// One row of the parameter card.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The engine's own field name.
    pub key: String,
    /// The value, formatted for reading (a curve field reads `n point(s)`).
    pub value: String,
    pub unit: String,
    pub source: &'static str,
    /// A curve field's points, `(x, y)`: the card draws a sparkline and no scalar.
    pub curve: Option<Vec<(f64, f64)>>,
}

impl Row {
    pub fn new(key: impl Into<String>, value: impl Into<String>, unit: impl Into<String>, source: &'static str) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            unit: unit.into(),
            source,
            curve: None,
        }
    }
    pub fn curve(key: impl Into<String>, pts: Vec<(f64, f64)>, unit: impl Into<String>, source: &'static str) -> Self {
        Self {
            key: key.into(),
            value: format!("{} point(s)", pts.len()),
            unit: unit.into(),
            source,
            curve: Some(pts),
        }
    }
}

fn n(key: &str, v: f64, unit: &str, source: &'static str) -> Row {
    Row::new(key, trim(v), unit, source)
}

fn opt(v: Option<f64>, unit: &str) -> (String, String) {
    match v {
        Some(x) => (trim(x), unit.to_string()),
        None => ("not recorded".to_string(), unit.to_string()),
    }
}

/// Every field of a fan record, in the catalog's own field order (`fields.fans`).
pub fn fan_rows(r: &FanRecord, source: &'static str) -> Vec<Row> {
    let mut rows = vec![
        n("id", 0.0, "-", source),
        Row::new("name", r.name.clone(), "-", source),
        n("stackAreaM2", r.stack_area_m2, "m2", source),
        Row::new("pressureBasis", r.pressure_basis.clone(), "-", source),
        n("referenceDensityKgM3", r.reference_density_kg_m3, "kg/m3", source),
    ];
    rows[0] = Row::new("id", r.id.clone(), "-", source);
    let (sr, su) = opt(r.stack_recovery_factor, "-");
    rows.push(Row::new("stackRecoveryFactor", sr, su, source));
    rows.push(Row::new(
        "allowedSpeedRatio",
        format!("{} .. {}", trim(r.allowed_speed_ratio[0]), trim(r.allowed_speed_ratio[1])),
        "-",
        source,
    ));
    // Issue #59: the rated speed the recorded curve is published at (speed ratio 1.0). An absent
    // one says so rather than showing a zero - and the rpm read-out then shows nothing.
    let (nominal, nominal_unit) = opt(r.nominal_rpm, "rpm");
    rows.push(Row::new("nominalRpm", nominal, nominal_unit, source));
    rows.push(n("driveEfficiency", r.drive_efficiency, "-", source));
    rows.push(n("motorEfficiency", r.motor_efficiency, "-", source));
    rows.push(Row::curve(
        "curve",
        r.curve.iter().map(|p| (p.flow_m3_s, p.pressure_pa)).collect(),
        "m3/s -> Pa",
        source,
    ));
    rows
}

/// Every field of a drift-eliminator record (`fields.driftEliminators`).
pub fn drift_rows(r: &DriftRecord, source: &'static str) -> Vec<Row> {
    vec![
        Row::new("id", r.id.clone(), "-", source),
        Row::new("name", r.name.clone(), "-", source),
        n("maxWaterTemperatureC", r.max_water_temperature_c, "C", source),
        Row::curve(
            "curve",
            r.curve.iter().map(|p| (p.face_velocity_m_s, p.drift_ppm)).collect(),
            "m/s -> ppm",
            source,
        ),
    ]
}

/// Every field of a fill record (`fields.fills`) plus its sampled characteristic (the curve field).
pub fn fill_rows(f: &FixtureFill, source: &'static str) -> Vec<Row> {
    let rec = &f.record;
    let mut rows = vec![
        Row::new("id", rec.id.clone(), "-", source),
        Row::new("name", rec.name.clone(), "-", source),
        Row::new("geometry", rec.geometry.clone(), "-", source),
        Row::new(
            "compatibleTowerTypes",
            rec.compatible_tower_types.join(", "),
            "-",
            source,
        ),
        Row::new(
            "allowedWaterQualityClasses",
            rec.allowed_water_quality_classes.join(", "),
            "-",
            source,
        ),
        n("thermal.coefficientPerM", rec.thermal.coefficient_per_m, "1/m", source),
        n(
            "thermal.referenceWaterLoadingKgM2S",
            rec.thermal.reference_water_loading_kg_m2_s,
            "kg/(m2 s)",
            source,
        ),
        n(
            "thermal.referenceDryAirLoadingKgM2S",
            rec.thermal.reference_dry_air_loading_kg_m2_s,
            "kg/(m2 s)",
            source,
        ),
        n("thermal.waterExponent", rec.thermal.water_exponent, "-", source),
        n("thermal.airExponent", rec.thermal.air_exponent, "-", source),
        n("pressure.coefficientPaPerM", rec.pressure.coefficient_pa_per_m, "Pa/m", source),
        n("pressure.waterExponent", rec.pressure.water_exponent, "-", source),
        n("pressure.airExponent", rec.pressure.air_exponent, "-", source),
        n(
            "limits.minWaterLoadingKgM2S",
            rec.limits.min_water_loading_kg_m2_s,
            "kg/(m2 s)",
            source,
        ),
        n(
            "limits.maxWaterLoadingKgM2S",
            rec.limits.max_water_loading_kg_m2_s,
            "kg/(m2 s)",
            source,
        ),
        n(
            "limits.minDryAirLoadingKgM2S",
            rec.limits.min_dry_air_loading_kg_m2_s,
            "kg/(m2 s)",
            source,
        ),
        n(
            "limits.maxDryAirLoadingKgM2S",
            rec.limits.max_dry_air_loading_kg_m2_s,
            "kg/(m2 s)",
            source,
        ),
        n("limits.maxWaterTemperatureC", rec.limits.max_water_temperature_c, "C", source),
    ];
    rows.push(Row::new(
        "characteristic.atDryAirLoadingKgM2S",
        trim(f.characteristic.at_dry_air_loading_kg_m2_s),
        "kg/(m2 s)",
        source,
    ));
    rows.push(Row::curve(
        "characteristic.points",
        f.characteristic
            .points
            .iter()
            .map(|p| (p.water_loading_kg_m2_s, p.kavl_per_m))
            .collect(),
        "kg/(m2 s) -> 1/m",
        source,
    ));
    rows
}

/// Every field of a nozzle record (`fields.nozzles`).
pub fn nozzle_rows(r: &NozzleRecord, source: &'static str) -> Vec<Row> {
    let (dr, du) = opt(r.reference_water_density_kg_m3, "kg/m3");
    vec![
        Row::new("id", r.id.clone(), "-", source),
        Row::new("name", r.name.clone(), "-", source),
        n("dischargeCoefficient", r.discharge_coefficient, "-", source),
        n("orificeDiameterM", r.orifice_diameter_m, "m", source),
        Row::new("referenceWaterDensityKgM3", dr, du, source),
    ]
}

/// One row per field of a custom record, read from the values the user entered and the generated
/// descriptor's units - so a custom record's card and a catalog record's card read the same way.
pub fn custom_rows(part: &crate::custom::CustomPart, fields: &crate::custom::Fields) -> Vec<Row> {
    let Some(spec) = fields.class(&part.class) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for f in spec.fields.iter() {
        let Some(v) = crate::custom::get(&part.values, &f.key) else {
            continue;
        };
        let row = match v {
            crate::custom::Value::Points(pts) => Row::curve(
                &f.key,
                pts.iter()
                    .filter(|r| r.len() >= 2)
                    .map(|r| (r[0], r[1]))
                    .collect(),
                &f.unit,
                "custom",
            ),
            other => Row::new(&f.key, other.summary(), &f.unit, "custom"),
        };
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::{Fields, Value};
    use cockpit::engine::{DriftPoint, FanPoint};

    fn fan() -> FanRecord {
        FanRecord {
            id: "AX-500".into(),
            name: "Illustrative 5.0 m Axial Fan".into(),
            stack_area_m2: 19.635,
            pressure_basis: "total".into(),
            reference_density_kg_m3: 1.2,
            stack_recovery_factor: None,
            allowed_speed_ratio: [0.7, 1.13],
            // The record's own rated speed (issue #59): 233 rpm holds this fan's 5.000 m stack
            // (19.635 m2) at the cited 12 000 ft/min tip-speed practice - the rule holds for
            // the other three fans too.
            nominal_rpm: Some(233.0),
            drive_efficiency: 0.96,
            motor_efficiency: 0.95,
            curve: vec![
                FanPoint { flow_m3_s: 55.0, pressure_pa: 520.0, efficiency: 0.63 },
                FanPoint { flow_m3_s: 100.0, pressure_pa: 470.0, efficiency: 0.74 },
            ],
        }
    }

    #[test]
    fn a_fan_card_carries_every_field_name_the_engine_has() {
        let rows = fan_rows(&fan(), "catalog");
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        for want in [
            "id",
            "stackAreaM2",
            "pressureBasis",
            "referenceDensityKgM3",
            "stackRecoveryFactor",
            "allowedSpeedRatio",
            "driveEfficiency",
            "motorEfficiency",
            "curve",
        ] {
            assert!(keys.contains(&want), "{want} missing from {keys:?}");
        }
        let id = rows.iter().find(|r| r.key == "id").unwrap();
        assert_eq!(id.value, "AX-500");
        assert_eq!(id.source, "catalog");
        // An absent optional field says so rather than showing a zero.
        let srf = rows.iter().find(|r| r.key == "stackRecoveryFactor").unwrap();
        assert_eq!(srf.value, "not recorded");
        // The curve is points, not a scalar, and it is paired x -> y.
        let curve = rows.iter().find(|r| r.key == "curve").unwrap();
        assert_eq!(curve.value, "2 point(s)");
        assert_eq!(curve.curve.as_ref().unwrap()[1], (100.0, 470.0));
    }

    #[test]
    fn a_drift_card_pairs_face_velocity_with_drift_and_a_nozzle_card_is_scalar_only() {
        let d = DriftRecord {
            id: "DE-3P-10".into(),
            name: "Three-pass".into(),
            max_water_temperature_c: 60.0,
            curve: vec![
                DriftPoint { face_velocity_m_s: 1.0, drift_ppm: 4.0, pressure_drop_pa: 10.0 },
                DriftPoint { face_velocity_m_s: 1.5, drift_ppm: 6.0, pressure_drop_pa: 18.0 },
            ],
        };
        let rows = drift_rows(&d, "catalog");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[2].value, "60");
        assert_eq!(rows[3].curve.as_ref().unwrap()[0], (1.0, 4.0));
        let nz = NozzleRecord {
            id: "NZ-20".into(),
            name: "Full cone".into(),
            discharge_coefficient: 0.72,
            orifice_diameter_m: 0.02,
            reference_water_density_kg_m3: None,
        };
        let rows = nozzle_rows(&nz, "catalog");
        assert!(rows.iter().all(|r| r.curve.is_none()));
        assert_eq!(rows.iter().find(|r| r.key == "orificeDiameterM").unwrap().value, "0.02");
        assert_eq!(
            rows.iter().find(|r| r.key == "referenceWaterDensityKgM3").unwrap().value,
            "not recorded"
        );
    }

    #[test]
    fn a_custom_records_card_reads_the_same_field_names_as_the_form() {
        let fields = Fields::from_json(include_str!("../../assets/custom-fields.json")).unwrap();
        let part = crate::custom::CustomPart {
            class: "fan".into(),
            values: vec![
                crate::custom::Entry { key: "id".into(), value: Value::Text("MY-FAN".into()) },
                crate::custom::Entry { key: "stackAreaM2".into(), value: Value::Number(18.0) },
                crate::custom::Entry {
                    key: "curve".into(),
                    value: Value::Points(vec![vec![10.0, 300.0, 0.6], vec![20.0, 250.0, 0.7]]),
                },
            ],
        };
        let rows = custom_rows(&part, &fields);
        assert!(rows.iter().any(|r| r.key == "id" && r.value == "MY-FAN" && r.source == "custom"));
        assert!(rows.iter().any(|r| r.key == "stackAreaM2" && r.value == "18"));
        let c = rows.iter().find(|r| r.key == "curve").unwrap();
        assert_eq!(c.curve.as_ref().unwrap().len(), 2);
        // The unit comes from the generated descriptor, the same one the form uses.
        let spec_unit = Fields::from_json(include_str!("../../assets/custom-fields.json"))
            .unwrap()
            .class("fan")
            .unwrap()
            .fields
            .iter()
            .find(|f| f.key == "stackAreaM2")
            .unwrap()
            .unit
            .clone();
        assert_eq!(
            rows.iter().find(|r| r.key == "stackAreaM2").unwrap().unit,
            spec_unit
        );
    }
}
