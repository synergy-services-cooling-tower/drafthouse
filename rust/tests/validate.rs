//! The catalog-record validator (issue #5): the machine schema over the five record types,
//! the refusal classes the fail-closed property is about, and the CLI gate — `select`
//! refuses an invalid catalog with exit 2, naming the record and the field, before anything
//! reaches the ranking.

use std::process::Command;
use synergy_drafthouse::validate::{schema, validate_record, FieldKind, RecordKind};

/* ---------------- records in the `ct-engine` spec encoding ---------------- */

/// The bundled catalog's `IDCF-064` tower, in the CLI's `key:value` encoding (`|` separates
/// list entries and a row's columns, `:` separates a curve point's columns).
fn bundled_tower() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "IDCF-064"),
        ("name", "Illustrative 64 m² Induced-Draft Counterflow Cell"),
        ("type", "counterflow"),
        ("draftType", "induced"),
        ("fillAreaM2", "64"),
        ("airFreeAreaM2", "64"),
        ("driftAreaM2", "61"),
        ("inletAreaM2", "32"),
        ("stackRecoveryFactor", "0.35"),
        ("fillDepthOptionsM", "1.2|1.5|1.8|2.1"),
        ("maxWaterMassFlowKgS", "265"),
        ("footprintM2", "78"),
        ("inletLossCoefficient", "3"),
        ("distributionLossCoefficient", "4.4"),
        ("supportLossCoefficient", "2.1"),
        ("plenumLossCoefficient", "0.38"),
        ("fixedPressureLossPa", "10"),
        ("sprayZoneHeightM", "0.6"),
        ("sprayZone", "0.16|3.0|2.0|-0.30|0.45"),
        ("rainZoneHeightM", "1.5"),
        ("rainZone", "0.13|3.0|2.0|-0.35|0.50"),
        ("compatibleFanIds", "AX-500|AX-600"),
    ]
}

/// The bundled catalog's `FILM-OF25` fill.
fn bundled_fill() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "FILM-OF25"),
        ("name", "Illustrative 25 mm Offset-Fluted Film Fill"),
        ("geometry", "offset-fluted film"),
        ("compatibleTowerTypes", "counterflow|crossflow"),
        ("allowedWaterQualityClasses", "clean|moderate"),
        ("thermal", "1.12|3.0|2.0|-0.31|0.44"),
        ("pressure", "61|3.0|2.0|0.14|1.72"),
        ("limits", "1.3|5.4|0.9|3.3|60"),
        ("material", "PP (illustrative)"),
    ]
}

/// The bundled catalog's `DE-3P-10` drift eliminator.
fn bundled_drift_eliminator() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "DE-3P-10"),
        ("name", "Illustrative Three-Pass 0.001% Eliminator"),
        ("material", "PVC (illustrative)"),
        ("maxWaterTemperatureC", "60"),
        (
            "curve",
            "1.0:4:10|1.5:6:18|2.0:9:31|2.5:14:49|3.0:23:72|3.5:38:101",
        ),
    ]
}

/// The bundled catalog's `AX-500` fan.
fn bundled_fan() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "AX-500"),
        ("name", "Illustrative 5.0 m Axial Fan"),
        ("stackAreaM2", "19.635"),
        ("pressureBasis", "total"),
        ("referenceDensityKgM3", "1.2"),
        ("allowedSpeedRatio", "0.70|1.13"),
        ("driveEfficiency", "0.96"),
        ("motorEfficiency", "0.95"),
        (
            "curve",
            "55:520:0.63|100:470:0.74|145:380:0.83|185:245:0.82|225:70:0.67",
        ),
    ]
}

/// The bundled catalog's `NZ-20` nozzle.
fn bundled_nozzle() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "NZ-20"),
        ("name", "Illustrative 20 mm Full-Cone Nozzle"),
        ("dischargeCoefficient", "0.72"),
        ("orificeDiameterM", "0.020"),
    ]
}

fn bundled_records() -> Vec<(RecordKind, Vec<(&'static str, &'static str)>)> {
    vec![
        (RecordKind::Tower, bundled_tower()),
        (RecordKind::Fill, bundled_fill()),
        (RecordKind::DriftEliminator, bundled_drift_eliminator()),
        (RecordKind::Fan, bundled_fan()),
        (RecordKind::Nozzle, bundled_nozzle()),
    ]
}

/// The refusal messages of a record that must not validate, and a panic when one does.
fn refusal(kind: RecordKind, label: &str, record: &[(&str, &str)]) -> Vec<String> {
    match validate_record(kind, label, record) {
        Ok(()) => panic!(
            "{} record {label:?} was accepted; expected a refusal",
            kind.as_str()
        ),
        Err(violations) => violations
            .iter()
            .map(|violation| violation.message())
            .collect(),
    }
}

/* ---------------- the schema ---------------- */

#[test]
fn every_record_kind_declares_a_unique_non_empty_schema() {
    for kind in RecordKind::ALL {
        let rules = schema(kind);
        assert!(!rules.is_empty(), "{} has no schema", kind.as_str());
        for (index, rule) in rules.iter().enumerate() {
            assert!(
                !rule.name.is_empty(),
                "{} has an unnamed field",
                kind.as_str()
            );
            assert!(
                !rules[..index].iter().any(|other| other.name == rule.name),
                "{} declares {:?} twice",
                kind.as_str(),
                rule.name
            );
            // Every kind is reachable through the checker; the match below is exhaustive.
            match rule.kind {
                FieldKind::Text
                | FieldKind::Number(_)
                | FieldKind::NumberList { .. }
                | FieldKind::TextList { .. }
                | FieldKind::Enum(_)
                | FieldKind::EnumList { .. }
                | FieldKind::RatioBracket
                | FieldKind::Row(_)
                | FieldKind::Points { .. } => {}
            }
        }
    }
}

/* ---------------- the bundled records ---------------- */

#[test]
fn the_bundled_record_shapes_validate_clean() {
    for (kind, record) in bundled_records() {
        if let Err(violations) = validate_record(kind, "record", &record) {
            let messages: Vec<String> = violations
                .iter()
                .map(|violation| violation.message())
                .collect();
            panic!("{} record refused: {messages:?}", kind.as_str());
        }
    }
}

#[test]
fn a_record_with_only_its_required_fields_also_validates_clean() {
    for (kind, record) in bundled_records() {
        let required_only: Vec<(&str, &str)> = record
            .iter()
            .copied()
            .filter(|(name, _)| {
                schema(kind)
                    .iter()
                    .any(|rule| rule.name == *name && rule.required)
            })
            .collect();
        assert!(
            validate_record(kind, "record", &required_only).is_ok(),
            "{} refused a required-fields-only record",
            kind.as_str()
        );
    }
}

/* ---------------- the refusals ---------------- */

#[test]
fn a_missing_required_field_is_refused_by_record_and_field() {
    let record: Vec<(&str, &str)> = bundled_tower()
        .into_iter()
        .filter(|(name, _)| *name != "fillAreaM2")
        .collect();
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].starts_with("tower record \"IDCF-064\""),
        "{}",
        messages[0]
    );
    assert!(messages[0].contains("\"fillAreaM2\""), "{}", messages[0]);
    assert!(messages[0].contains("required"), "{}", messages[0]);
}

#[test]
fn a_mistyped_field_name_is_refused_by_record_and_field() {
    let mut record = bundled_tower();
    record.push(("fillAreM2", "64"));
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("\"fillAreM2\"")
                && message.contains("not a declared field")),
        "{messages:?}"
    );
}

/// Requirement 2's decision, pinned: the machine schema does **not** declare the reference
/// catalog's commercial fields. The port's own guard
/// (`tests/selection.rs::no_money_ish_identifier_exists_anywhere_in_the_port`) fails if the
/// spelling appears anywhere under `rust/src`, so a schema that named them would break a
/// reviewed gate; a record carrying one is refused by name instead of being silently
/// ignored, and the bundled data itself is never edited (the reference keeps using them).
#[test]
fn a_reference_catalog_commercial_field_is_refused_as_undeclared() {
    let mut record = bundled_tower();
    record.push(("baseCost", "166000"));
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("\"baseCost\"")
                && message.contains("not a declared field")),
        "{messages:?}"
    );

    let mut fill = bundled_fill();
    fill.push(("costPerM3", "470"));
    let messages = refusal(RecordKind::Fill, "FILM-OF25", &fill);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("\"costPerM3\"")),
        "{messages:?}"
    );
}

#[test]
fn a_duplicated_field_is_refused() {
    let mut record = bundled_tower();
    record.push(("id", "IDCF-096"));
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(
        messages.iter().any(|message| message.contains("twice")),
        "{messages:?}"
    );
}

#[test]
fn non_finite_numbers_are_refused() {
    for raw in ["NaN", "Infinity", "-Infinity", "1e999"] {
        let record: Vec<(&str, &str)> = bundled_tower()
            .into_iter()
            .map(|(name, value)| {
                if name == "fillAreaM2" {
                    (name, raw)
                } else {
                    (name, value)
                }
            })
            .collect();
        let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
        assert_eq!(messages.len(), 1, "{raw}: {messages:?}");
        assert!(
            messages[0].contains("\"fillAreaM2\"") && messages[0].contains("must be finite"),
            "{raw}: {}",
            messages[0]
        );
    }
}

#[test]
fn a_non_numeric_value_is_refused() {
    let record: Vec<(&str, &str)> = bundled_tower()
        .into_iter()
        .map(|(name, value)| {
            if name == "fillAreaM2" {
                (name, "sixty-four")
            } else {
                (name, value)
            }
        })
        .collect();
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(
        messages[0].contains("\"fillAreaM2\"") && messages[0].contains("must be a number"),
        "{}",
        messages[0]
    );
}

#[test]
fn out_of_domain_and_out_of_set_values_are_refused() {
    // A negative area is out of its domain.
    let record: Vec<(&str, &str)> = bundled_tower()
        .into_iter()
        .map(|(name, value)| {
            if name == "fillAreaM2" {
                (name, "-64")
            } else {
                (name, value)
            }
        })
        .collect();
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(messages[0].contains("must be positive"), "{}", messages[0]);

    // An enumerated field outside its set.
    let record: Vec<(&str, &str)> = bundled_tower()
        .into_iter()
        .map(|(name, value)| {
            if name == "type" {
                (name, "counterflw")
            } else {
                (name, value)
            }
        })
        .collect();
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(
        messages[0].contains("\"type\"") && messages[0].contains("counterflow"),
        "{}",
        messages[0]
    );

    // An enumerated list with a class outside its set.
    let mut record = bundled_fill();
    record.retain(|(name, _)| *name != "allowedWaterQualityClasses");
    record.push(("allowedWaterQualityClasses", "clean|brackish"));
    let messages = refusal(RecordKind::Fill, "FILM-OF25", &record);
    assert!(
        messages[0].contains("brackish") && messages[0].contains("clean"),
        "{}",
        messages[0]
    );

    // A zone row with the wrong arity.
    let mut record = bundled_tower();
    record.retain(|(name, _)| *name != "sprayZone");
    record.push(("sprayZone", "0.16|3.0|2.0"));
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(
        messages[0].contains("\"sprayZone\"") && messages[0].contains("5 '|'-separated"),
        "{}",
        messages[0]
    );

    // A signed exponent column still has to be a number.
    let mut record = bundled_tower();
    record.retain(|(name, _)| *name != "rainZone");
    record.push(("rainZone", "0.13|3.0|2.0|oops|0.50"));
    let messages = refusal(RecordKind::Tower, "IDCF-064", &record);
    assert!(messages[0].contains("waterExponent"), "{}", messages[0]);

    // An inverted speed-ratio bracket.
    let mut record = bundled_fan();
    record.retain(|(name, _)| *name != "allowedSpeedRatio");
    record.push(("allowedSpeedRatio", "1.13|0.70"));
    let messages = refusal(RecordKind::Fan, "AX-500", &record);
    assert!(messages[0].contains("lower ratio first"), "{}", messages[0]);

    // A curve below the engine's own two-point minimum.
    let mut record = bundled_drift_eliminator();
    record.retain(|(name, _)| *name != "curve");
    record.push(("curve", "1.0:4:10"));
    let messages = refusal(RecordKind::DriftEliminator, "DE-3P-10", &record);
    assert!(
        messages[0].contains("at least 2") && messages[0].contains("\"curve\""),
        "{}",
        messages[0]
    );

    // A zero efficiency in a fan curve point.
    let mut record = bundled_fan();
    record.retain(|(name, _)| *name != "curve");
    record.push(("curve", "55:520:0.63|100:470:0"));
    let messages = refusal(RecordKind::Fan, "AX-500", &record);
    assert!(
        messages[0].contains("point 2") && messages[0].contains("efficiency"),
        "{}",
        messages[0]
    );
}

/* ---------------- the CLI gate ---------------- */

fn record(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{name}:{value}"))
        .collect::<Vec<String>>()
        .join(",")
}

/// A catalog with one record per type, required fields only, that the selector can run.
fn minimal_catalog_args() -> Vec<String> {
    vec![
        "--towers".to_string(),
        record(&[
            ("id", "IDCF-064"),
            ("type", "counterflow"),
            ("fillAreaM2", "64"),
            ("footprintM2", "78"),
            ("maxWaterMassFlowKgS", "265"),
            ("fillDepthOptionsM", "1.2|1.5"),
            ("compatibleFanIds", "AX-500"),
        ]),
        "--fills".to_string(),
        record(&[
            ("id", "FILM-OF25"),
            ("compatibleTowerTypes", "counterflow|crossflow"),
            ("allowedWaterQualityClasses", "clean|moderate"),
            ("thermal", "1.12|3.0|2.0|-0.31|0.44"),
            ("pressure", "61|3.0|2.0|0.14|1.72"),
            ("limits", "1.3|5.4|0.9|3.3|60"),
        ]),
        "--drift-eliminators".to_string(),
        record(&[
            ("id", "DE-3P-10"),
            ("maxWaterTemperatureC", "60"),
            ("curve", "1.0:4:10|1.5:6:18"),
        ]),
        "--fans".to_string(),
        record(&[
            ("id", "AX-500"),
            ("stackAreaM2", "19.635"),
            ("pressureBasis", "total"),
            ("referenceDensityKgM3", "1.2"),
            ("allowedSpeedRatio", "0.70|1.13"),
            ("driveEfficiency", "0.96"),
            ("motorEfficiency", "0.95"),
            ("curve", "55:520:0.63|100:470:0.74"),
        ]),
        "--nozzles".to_string(),
        record(&[
            ("id", "NZ-20"),
            ("name", "Illustrative 20 mm Full-Cone Nozzle"),
            ("dischargeCoefficient", "0.72"),
            ("orificeDiameterM", "0.020"),
        ]),
        "--quality-factors".to_string(),
        "clean:1:1;moderate:0.92:1.12".to_string(),
    ]
}

/// The same catalog with every declared-but-unread field present (`name`, `draftType`,
/// `geometry`, `material`, and the nozzle's optional reference density).
fn complete_catalog_args() -> Vec<String> {
    let mut args = minimal_catalog_args();
    let declare = |args: &mut Vec<String>, option: &str, extra: &str| {
        let index = args
            .iter()
            .position(|argument| argument == option)
            .expect("the option is present");
        args[index + 1] = format!("{},{}", args[index + 1], extra);
    };
    declare(
        &mut args,
        "--towers",
        "name:Illustrative 64 m2 Cell,draftType:induced",
    );
    declare(
        &mut args,
        "--fills",
        "name:Illustrative 25 mm Film Fill,geometry:offset-fluted film,material:PP",
    );
    declare(
        &mut args,
        "--drift-eliminators",
        "name:Illustrative Three-Pass Eliminator,material:PVC",
    );
    declare(&mut args, "--fans", "name:Illustrative 5.0 m Axial Fan");
    declare(&mut args, "--nozzles", "referenceWaterDensityKgM3:997");
    args
}

fn replace_towers(mut args: Vec<String>, tower: &str) -> Vec<String> {
    let index = args
        .iter()
        .position(|argument| argument == "--towers")
        .expect("--towers is present");
    args[index + 1] = tower.to_string();
    args
}

fn engine(command: &str, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ct-engine"))
        .arg(command)
        .args(args)
        .output()
        .expect("the CLI runs")
}

#[test]
fn select_runs_on_a_valid_catalog() {
    let output = engine("select", &minimal_catalog_args());
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn select_refuses_a_missing_required_field_with_exit_2() {
    let tower = record(&[
        ("id", "IDCF-064"),
        ("fillAreaM2", "64"),
        ("footprintM2", "78"),
        ("maxWaterMassFlowKgS", "265"),
        ("fillDepthOptionsM", "1.2|1.5"),
        ("compatibleFanIds", "AX-500"),
    ]);
    let output = engine("select", &replace_towers(minimal_catalog_args(), &tower));
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("tower record \"IDCF-064\""), "{stderr}");
    assert!(stderr.contains("\"type\""), "{stderr}");
    assert!(stderr.contains("required"), "{stderr}");
}

#[test]
fn select_refuses_a_non_finite_number_with_exit_2() {
    let tower = record(&[
        ("id", "IDCF-064"),
        ("type", "counterflow"),
        ("fillAreaM2", "NaN"),
        ("footprintM2", "78"),
        ("maxWaterMassFlowKgS", "265"),
        ("fillDepthOptionsM", "1.2|1.5"),
        ("compatibleFanIds", "AX-500"),
    ]);
    let output = engine("select", &replace_towers(minimal_catalog_args(), &tower));
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("tower record \"IDCF-064\""), "{stderr}");
    assert!(stderr.contains("\"fillAreaM2\""), "{stderr}");
    assert!(stderr.contains("must be finite"), "{stderr}");
}

#[test]
fn validate_catalog_accepts_a_complete_catalog_and_counts_it() {
    let output = engine("validate-catalog", &complete_catalog_args());
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let counts: serde_json::Value = serde_json::from_str(stdout.trim()).expect("JSON counts");
    for key in ["towers", "fills", "driftEliminators", "fans", "nozzles"] {
        assert_eq!(counts[key], 1, "{stdout}");
    }
    assert_eq!(counts["records"], 5, "{stdout}");
}

#[test]
fn validate_catalog_refuses_an_undeclared_field_with_exit_2() {
    let tower = format!(
        "{},baseCost:166000",
        record(&[
            ("id", "IDCF-064"),
            ("type", "counterflow"),
            ("fillAreaM2", "64"),
            ("footprintM2", "78"),
            ("maxWaterMassFlowKgS", "265"),
            ("fillDepthOptionsM", "1.2|1.5"),
            ("compatibleFanIds", "AX-500"),
        ])
    );
    let output = engine(
        "validate-catalog",
        &replace_towers(minimal_catalog_args(), &tower),
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("tower record \"IDCF-064\""), "{stderr}");
    assert!(stderr.contains("\"baseCost\""), "{stderr}");
    assert!(stderr.contains("not a declared field"), "{stderr}");
}
