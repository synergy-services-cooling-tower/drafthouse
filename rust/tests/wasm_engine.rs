//! The engine surface the wasm export adds (issue #24): the `inputs` and `worked` keys
//! `select` emits, and the reply envelope a caller that is not a process reads.
//!
//! `cli::run` is the engine both the `ct-engine` binary and the wasm export call, so this
//! data contract is testable natively, without a wasm runtime. The wasm-specific half — the
//! C ABI (`src/wasm.rs`), the artifact and the JS binding's catalog projection — is
//! exercised by the parity harness under `--engine wasm` (`scripts/parity/run.mjs`).
//!
//! The records below are one tower, fill, drift eliminator, fan and nozzle from the bundled
//! catalog, in the `key:value` spec form the CLI parses; the selection they produce is
//! feasible at the engine's default duty, which is what makes the worked sheet non-empty.

use serde_json::Value;
use synergy_drafthouse::cli::{self, Failure, USAGE};

const TOWER: &str = "id:IDCF-064,type:counterflow,fillAreaM2:64,airFreeAreaM2:64,driftAreaM2:61,inletAreaM2:32,stackRecoveryFactor:0.35,inletLossCoefficient:3,distributionLossCoefficient:4.4,supportLossCoefficient:2.1,plenumLossCoefficient:0.38,fixedPressureLossPa:10,sprayZoneHeightM:0.6,rainZoneHeightM:1.5,footprintM2:78,maxWaterMassFlowKgS:265,fillDepthOptionsM:1.2|1.5|1.8,compatibleFanIds:AX-500,sprayZone:0.16|3|2|-0.3|0.45,rainZone:0.13|3|2|-0.35|0.5";
const FILL: &str = "id:FILM-OF25,compatibleTowerTypes:counterflow|crossflow,allowedWaterQualityClasses:clean|moderate,thermal:1.12|3|2|-0.31|0.44,pressure:61|3|2|0.14|1.72,limits:1.3|5.4|0.9|3.3|60";
const DRIFT: &str =
    "id:DE-3P-10,maxWaterTemperatureC:60,curve:1:4:10|1.5:6:18|2:9:31|2.5:14:49|3:23:72|3.5:38:101";
const FAN: &str = "id:AX-500,stackAreaM2:19.635,pressureBasis:total,referenceDensityKgM3:1.2,allowedSpeedRatio:0.7|1.13,driveEfficiency:0.96,motorEfficiency:0.95,curve:55:520:0.63|100:470:0.74|145:380:0.83|185:245:0.82|225:70:0.67";
const NOZZLE: &str =
    "id:NZ-20,name:Illustrative 20 mm Full-Cone Nozzle,dischargeCoefficient:0.72,orificeDiameterM:0.02";
const QUALITY_FACTORS: &str = "clean:1:1;moderate:0.92:1.12";

/// The `worked.js` step's field set — the shape a step is data in, not markup.
const STEP_FIELDS: [&str; 8] = [
    "label",
    "why",
    "formula",
    "substitution",
    "value",
    "unit",
    "reference",
    "kind",
];

fn select_args() -> Vec<String> {
    [
        "select",
        "--towers",
        TOWER,
        "--fills",
        FILL,
        "--drift-eliminators",
        DRIFT,
        "--fans",
        FAN,
        "--nozzles",
        NOZZLE,
        "--quality-factors",
        QUALITY_FACTORS,
        "--max-results",
        "3",
    ]
    .iter()
    .map(|argument| argument.to_string())
    .collect()
}

fn select() -> Value {
    let args = select_args();
    let json = cli::run(&args).expect("the selection runs at the default duty");
    serde_json::from_str(&json).expect("the selection output is JSON")
}

fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    let mut current = value;
    for part in path.split('.') {
        current = match current {
            Value::Array(items) => items
                .get(part.parse::<usize>().expect("an array index"))
                .unwrap_or_else(|| panic!("{path} is out of range at {part}")),
            other => other
                .get(part)
                .unwrap_or_else(|| panic!("{path} is missing ({part})")),
        };
    }
    current
}

fn number(value: &Value, path: &str) -> f64 {
    field(value, path)
        .as_f64()
        .unwrap_or_else(|| panic!("{path} is not a number"))
}

fn text(value: &Value, path: &str) -> String {
    field(value, path)
        .as_str()
        .unwrap_or_else(|| panic!("{path} is not a string"))
        .to_string()
}

#[test]
fn select_reports_the_inputs_it_used_including_catalog_dimensions() {
    let run = select();
    let leader = field(&run, "results.0");

    // The identity the run resolved for the recommended candidate...
    let selected = field(&run, "inputs.selected");
    assert_eq!(
        text(selected, "towerId"),
        text(leader, "towerId"),
        "the inputs must name the candidate the sheet describes"
    );
    assert_eq!(text(selected, "fillId"), text(leader, "fillId"));
    assert_eq!(text(selected, "driftEliminatorId"), text(leader, "driftId"));
    assert_eq!(text(selected, "fanId"), text(leader, "fanId"));
    assert_eq!(number(selected, "fillDepthM"), number(leader, "fillDepthM"));
    assert_eq!(number(selected, "speedRatio"), number(leader, "speedRatio"));

    // ...and the catalog dimensions it read out of the records, as values, not as the spec
    // text they arrived in.
    let tower = field(&run, "inputs.records.tower");
    assert_eq!(text(tower, "id"), "IDCF-064");
    assert_eq!(number(tower, "fillAreaM2"), 64.0);
    assert_eq!(number(tower, "driftAreaM2"), 61.0);
    assert_eq!(number(tower, "sprayZoneHeightM"), 0.6);
    assert_eq!(
        tower
            .get("fillDepthOptionsM")
            .and_then(Value::as_array)
            .expect("a list")
            .len(),
        3
    );
    assert_eq!(
        field(&run, "inputs.records.fan.curve")
            .as_array()
            .expect("a list of curve rows")
            .len(),
        5
    );
    assert_eq!(
        field(&run, "inputs.records.driftEliminator.curve")
            .as_array()
            .expect("a list of curve rows")
            .len(),
        6
    );
    assert_eq!(
        number(&run, "inputs.records.nozzle.dischargeCoefficient"),
        0.72
    );

    // The requirements the run ranked against travel with the inputs.
    assert!(field(&run, "inputs.requirements.waterMassFlowKgS").is_number());
}

#[test]
fn worked_steps_follow_the_worked_js_shape_and_are_data_only() {
    let run = select();
    let steps = field(&run, "worked.steps").as_array().expect("a step list");
    assert!(
        steps.len() >= 10,
        "the sheet must trace the recommendation, found {} steps",
        steps.len()
    );

    for step in steps {
        let mut keys = step
            .as_object()
            .expect("a step is an object")
            .keys()
            .cloned()
            .collect::<Vec<String>>();
        keys.sort();
        let mut expected = STEP_FIELDS.to_vec();
        expected.sort();
        assert_eq!(
            keys, expected,
            "a step carries exactly the worked.js fields"
        );

        let kind = text(step, "kind");
        assert!(
            kind == "calc" || kind == "note",
            "unexpected step kind {kind:?}"
        );
        assert!(text(step, "label").len() > 1);
        assert!(text(step, "why").len() > 1);
        if kind == "calc" {
            assert!(step.get("value").is_some_and(|value| !value.is_null()));
            assert!(text(step, "formula").len() > 1);
            assert!(text(step, "substitution").len() > 1);
        } else {
            assert_eq!(step.get("value"), Some(&Value::Null));
            assert_eq!(step.get("formula"), Some(&Value::Null));
        }
    }

    // Data, not markup: nothing in the sheet may carry the characters that would make it
    // renderable, and nothing may build geometry either (the numbers are values, not paths).
    let serialised = serde_json::to_string(&run["worked"]).expect("serialises");
    for forbidden in ["<", ">", "&lt;", "&amp;", "class=", "style="] {
        assert!(
            !serialised.contains(forbidden),
            "the worked sheet carries {forbidden:?}; it must stay data"
        );
    }

    assert_eq!(
        text(&run, "worked.title"),
        "Selection of the recommended unit"
    );
    assert!(text(&run, "worked.purpose").len() > 1);
}

#[test]
fn worked_step_values_are_the_candidates_own_numbers() {
    let run = select();
    let leader = field(&run, "results.0");
    let steps = field(&run, "worked.steps").as_array().expect("a step list");
    let value_of = |label: &str| {
        steps
            .iter()
            .find(|step| step.get("label").and_then(Value::as_str) == Some(label))
            .unwrap_or_else(|| panic!("no step labelled {label:?}"))
            .get("value")
            .cloned()
            .unwrap_or(Value::Null)
    };

    for (label, path) in [
        ("Capacity", "capacityKgS"),
        ("Cold-water temperature, solved", "coldWaterC"),
        ("Heat the water gives up", "heatTransferKW"),
        ("Air-side pressure demand", "airside.totalPa"),
        ("Fan shaft power", "fanOperatingPoint.shaftPowerKW"),
        ("Make-up water", "waterBalance.makeupKgS"),
    ] {
        let reported = value_of(label);
        let computed = field(leader, path);
        assert_eq!(
            reported, *computed,
            "step {label:?} must report the candidate's own {path}"
        );
    }

    // The sheet's result block and the ranked candidate agree, field for field, on the
    // quantities both carry.
    for (result_path, candidate_path) in [
        ("capacityKgS", "capacityKgS"),
        ("coldWaterC", "coldWaterC"),
        ("electricalInputKW", "electricalInputKW"),
        ("totalPa", "airside.totalPa"),
    ] {
        assert_eq!(
            number(&run, &format!("worked.result.{result_path}")),
            number(leader, candidate_path),
            "worked.result.{result_path} must be the candidate's own number"
        );
    }
}

#[test]
fn the_reply_envelope_carries_the_status_the_binary_would_exit_with() {
    let success: Value = serde_json::from_str(&cli::reply(&[
        "psy".into(),
        "--db".into(),
        "30".into(),
        "--wb".into(),
        "20".into(),
    ]))
    .expect("a JSON envelope");
    assert_eq!(success["ok"], Value::Bool(true));
    assert_eq!(success["value"]["dryBulbC"].as_f64(), Some(30.0));

    let usage = cli::reply(&["nope".into()]);
    let usage: Value = serde_json::from_str(&usage).expect("a JSON envelope");
    assert_eq!(usage["ok"], Value::Bool(false));
    assert_eq!(
        usage["status"].as_u64(),
        Some(2),
        "a usage refusal is exit 2"
    );
    assert_eq!(usage["error"]["error"].as_str(), Some("usage"));

    let domain: Value = serde_json::from_str(&cli::reply(&[
        "psy".into(),
        "--db".into(),
        "27".into(),
        "--wb".into(),
        "33".into(),
    ]))
    .expect("a JSON envelope");
    assert_eq!(domain["ok"], Value::Bool(false));
    assert_eq!(
        domain["status"].as_u64(),
        Some(1),
        "an out-of-domain refusal is exit 1"
    );
    assert_eq!(domain["error"]["error"].as_str(), Some("DomainError"));

    // The envelope is the process's own classification, not a second opinion.
    assert!(matches!(cli::run(&["nope".into()]), Err(Failure::Usage(_))));
    assert!(matches!(
        cli::run(&[
            "psy".into(),
            "--db".into(),
            "27".into(),
            "--wb".into(),
            "33".into()
        ]),
        Err(Failure::Domain(_))
    ));
    assert!(!USAGE.is_empty());
}

#[test]
fn select_fields_lists_the_field_names_the_selector_accepts() {
    let json = cli::run(&["select-fields".into()]).expect("select-fields runs");
    let fields: Value = serde_json::from_str(&json).expect("select-fields emits JSON");

    let names = |list: &str| {
        fields[list]
            .as_array()
            .unwrap_or_else(|| panic!("{list} is missing"))
            .iter()
            .map(|name| name.as_str().expect("a field name").to_string())
            .collect::<Vec<String>>()
    };

    assert_eq!(
        names("driftEliminators"),
        vec!["id", "maxWaterTemperatureC", "curve"]
    );
    for expected in ["id", "type", "fillAreaM2", "sprayZone", "rainZone"] {
        assert!(
            names("towers").contains(&expected.to_string()),
            "the tower intake must list {expected}"
        );
    }
    for expected in ["id", "allowedSpeedRatio", "curve", "driveEfficiency"] {
        assert!(
            names("fans").contains(&expected.to_string()),
            "the fan intake must list {expected}"
        );
    }

    // Every field name the tower spec above carries must be in the list: the projection a
    // caller applies (the wasm binding's, and this test's) is exactly this intake.
    for pair in TOWER.split(',') {
        let key = pair.split(':').next().expect("a key");
        assert!(
            names("towers").contains(&key.to_string()),
            "the tower spec carries {key}, which the engine would refuse"
        );
    }
}
