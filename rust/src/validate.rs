//! The fail-closed catalog-record validator (issue #5).
//!
//! `../docs/CATALOG_SCHEMA.md` describes the component catalog in prose. This module is that
//! prose as data: for every record type it declares every field, whether the field is
//! required, and what its value must be. The `ct-engine` CLI validates catalog records with
//! this table before the selector may rank anything; a future front end (the HTML side
//! through the WASM kernel) shares the same table instead of growing a second validator.
//!
//! Fail-closed means each of these is a refusal that names the record and the field:
//!
//! * a required field is missing from the record;
//! * a field name the record type does not declare appears — a mistyped name must fail
//!   loudly instead of becoming an ignored key, which is the defect this module closes;
//! * a value is not the declared type, or is a non-finite number (`NaN`, `Infinity`, or a
//!   literal that overflows to infinity), which is the value class that used to flow into a
//!   ranking;
//! * a number is outside its declared domain (positive, non-negative), an enumerated value
//!   is outside its declared set, or a row/list has the wrong length.
//!
//! The schema is the port's own record shape: the fields `selection.js` feeds the selector,
//! plus the catalog fields `../docs/CATALOG_SCHEMA.md` declares that the port does not read
//! (`name`, `draftType`, `effectiveDraftHeightM`, `geometry`, `material`). Fields the schema
//! does not declare are refused; `README.md` records that decision, including the reference
//! catalog's commercial fields.

/// The five record types `docs/CATALOG_SCHEMA.md` declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    Tower,
    Fill,
    DriftEliminator,
    Fan,
    Nozzle,
}

impl RecordKind {
    /// Every record type, in the schema's own order.
    pub const ALL: [RecordKind; 5] = [
        RecordKind::Tower,
        RecordKind::Fill,
        RecordKind::DriftEliminator,
        RecordKind::Fan,
        RecordKind::Nozzle,
    ];

    /// The spelling used in refusals and in the CLI's record options.
    pub fn as_str(self) -> &'static str {
        match self {
            RecordKind::Tower => "tower",
            RecordKind::Fill => "fill",
            RecordKind::DriftEliminator => "drift-eliminator",
            RecordKind::Fan => "fan",
            RecordKind::Nozzle => "nozzle",
        }
    }
}

/// What a numeric field's value must satisfy beyond being a finite number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberDomain {
    /// Any finite number (the schema's signed quantities: the fill power-law exponents).
    Finite,
    /// A finite number greater than zero.
    Positive,
    /// A finite number at or above zero.
    NonNegative,
}

/// One named numeric column of a `|`-separated row or a `:`-separated curve point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: &'static str,
    pub domain: NumberDomain,
}

/// The value shape a field declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKind {
    /// A non-empty string.
    Text,
    /// One number.
    Number(NumberDomain),
    /// A `|`-separated list of numbers.
    NumberList {
        domain: NumberDomain,
        min_len: usize,
    },
    /// A `|`-separated list of non-empty strings.
    TextList { min_len: usize },
    /// A string from a declared set.
    Enum(&'static [&'static str]),
    /// A `|`-separated list of strings from a declared set.
    EnumList {
        allowed: &'static [&'static str],
        min_len: usize,
    },
    /// Two `|`-separated ratios forming an ordered bracket: `lower|upper`, `lower <= upper`.
    RatioBracket,
    /// One `|`-separated row of numeric columns (`a|b|c`).
    Row(&'static [Column]),
    /// `|`-separated points of `:`-separated numeric columns (`a:b:c|a:b:c`).
    Points {
        columns: &'static [Column],
        min_len: usize,
    },
}

/// One field of one record type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldRule {
    pub name: &'static str,
    pub required: bool,
    pub kind: FieldKind,
}

impl FieldRule {
    const fn required(name: &'static str, kind: FieldKind) -> Self {
        Self {
            name,
            required: true,
            kind,
        }
    }

    const fn optional(name: &'static str, kind: FieldKind) -> Self {
        Self {
            name,
            required: false,
            kind,
        }
    }
}

/// One refused field: the record it belongs to, the field, and what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    /// `«kind» record «label»`, e.g. `tower record "IDCF-064"`.
    pub record: String,
    pub field: String,
    pub problem: String,
}

impl Violation {
    fn new(record: &str, field: &str, problem: String) -> Self {
        Self {
            record: record.to_string(),
            field: field.to_string(),
            problem,
        }
    }

    /// The refusal line: names the record and the field.
    pub fn message(&self) -> String {
        format!("{}: field {:?} {}", self.record, self.field, self.problem)
    }
}

const TOWER_TYPES: [&str; 2] = ["counterflow", "crossflow"];
const WATER_QUALITY_CLASSES: [&str; 3] = ["clean", "moderate", "dirty"];
const PRESSURE_BASES: [&str; 2] = ["total", "static"];

const ZONE_COLUMNS: [Column; 5] = [
    Column {
        name: "coefficientPerM",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "referenceWaterLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "referenceDryAirLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "waterExponent",
        domain: NumberDomain::Finite,
    },
    Column {
        name: "airExponent",
        domain: NumberDomain::Finite,
    },
];

const FILL_PRESSURE_COLUMNS: [Column; 5] = [
    Column {
        name: "coefficientPaPerM",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "referenceWaterLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "referenceDryAirLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "waterExponent",
        domain: NumberDomain::Finite,
    },
    Column {
        name: "airExponent",
        domain: NumberDomain::Finite,
    },
];

const FILL_LIMIT_COLUMNS: [Column; 5] = [
    Column {
        name: "minWaterLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "maxWaterLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "minDryAirLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "maxDryAirLoadingKgM2S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "maxWaterTemperatureC",
        domain: NumberDomain::Positive,
    },
];

const DRIFT_CURVE_COLUMNS: [Column; 3] = [
    Column {
        name: "faceVelocityMS",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "driftPpm",
        domain: NumberDomain::NonNegative,
    },
    Column {
        name: "pressureDropPa",
        domain: NumberDomain::NonNegative,
    },
];

/// `interpolate1D` refuses a curve with fewer than two points, and
/// `driftPerformanceAtVelocity` interpolates without clamping the ends, so a curve needs a
/// span: the same minimum the engine's own contract states.
const CURVE_MIN_POINTS: usize = 2;

/// The tower geometry record (schema section 3).
const TOWER_FIELDS: [FieldRule; 26] = [
    FieldRule::required("id", FieldKind::Text),
    FieldRule::required("type", FieldKind::Enum(&TOWER_TYPES)),
    FieldRule::required("fillAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional("airFreeAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional("driftAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional("inletAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional("plenumAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional("fanStackAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional(
        "stackRecoveryFactor",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "inletLossCoefficient",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "distributionLossCoefficient",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "supportLossCoefficient",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "plenumLossCoefficient",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "fixedPressureLossPa",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "sprayZoneHeightM",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional(
        "rainZoneHeightM",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional("sprayZone", FieldKind::Row(&ZONE_COLUMNS)),
    FieldRule::optional("rainZone", FieldKind::Row(&ZONE_COLUMNS)),
    FieldRule::required("footprintM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::required(
        "maxWaterMassFlowKgS",
        FieldKind::Number(NumberDomain::Positive),
    ),
    FieldRule::required(
        "fillDepthOptionsM",
        FieldKind::NumberList {
            domain: NumberDomain::Positive,
            min_len: 1,
        },
    ),
    FieldRule::required("compatibleFanIds", FieldKind::TextList { min_len: 1 }),
    // The ordered-layer contract (issue #54): zero or more complete `+`-joined stack variants,
    // `|`-separated. Each variant is an ordered list of `<fillId>@<depthM>[@<thermal>[@<pressure>]]`
    // layers, resolved by the CLI's own reader (a layer's fields are not this record's fields).
    FieldRule::optional("fillStacks", FieldKind::TextList { min_len: 1 }),
    FieldRule::optional("name", FieldKind::Text),
    FieldRule::optional("draftType", FieldKind::Text),
    FieldRule::optional(
        "effectiveDraftHeightM",
        FieldKind::Number(NumberDomain::Positive),
    ),
];

/// The fill record (schema section 4).
const FILL_FIELDS: [FieldRule; 9] = [
    FieldRule::required("id", FieldKind::Text),
    FieldRule::required(
        "compatibleTowerTypes",
        FieldKind::EnumList {
            allowed: &TOWER_TYPES,
            min_len: 1,
        },
    ),
    FieldRule::required(
        "allowedWaterQualityClasses",
        FieldKind::EnumList {
            allowed: &WATER_QUALITY_CLASSES,
            min_len: 1,
        },
    ),
    FieldRule::required("thermal", FieldKind::Row(&ZONE_COLUMNS)),
    FieldRule::required("pressure", FieldKind::Row(&FILL_PRESSURE_COLUMNS)),
    FieldRule::required("limits", FieldKind::Row(&FILL_LIMIT_COLUMNS)),
    FieldRule::optional("name", FieldKind::Text),
    FieldRule::optional("geometry", FieldKind::Text),
    FieldRule::optional("material", FieldKind::Text),
];

/// The drift-eliminator record (schema section 5).
const DRIFT_ELIMINATOR_FIELDS: [FieldRule; 5] = [
    FieldRule::required("id", FieldKind::Text),
    FieldRule::required(
        "maxWaterTemperatureC",
        FieldKind::Number(NumberDomain::Positive),
    ),
    FieldRule::required(
        "curve",
        FieldKind::Points {
            columns: &DRIFT_CURVE_COLUMNS,
            min_len: CURVE_MIN_POINTS,
        },
    ),
    FieldRule::optional("name", FieldKind::Text),
    FieldRule::optional("material", FieldKind::Text),
];

const FAN_CURVE_COLUMNS: [Column; 3] = [
    Column {
        name: "flowM3S",
        domain: NumberDomain::Positive,
    },
    Column {
        name: "pressurePa",
        domain: NumberDomain::NonNegative,
    },
    Column {
        name: "efficiency",
        domain: NumberDomain::Positive,
    },
];

/// The fan record (schema section 6).
const FAN_FIELDS: [FieldRule; 11] = [
    FieldRule::required("id", FieldKind::Text),
    FieldRule::required("allowedSpeedRatio", FieldKind::RatioBracket),
    // The speed the recorded curve is published at (speed ratio 1.0), in rpm. Optional: the
    // record's own datum, and one this schema never invents.
    FieldRule::optional("nominalRpm", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::required("driveEfficiency", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::required("motorEfficiency", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::required(
        "curve",
        FieldKind::Points {
            columns: &FAN_CURVE_COLUMNS,
            min_len: CURVE_MIN_POINTS,
        },
    ),
    FieldRule::optional("stackAreaM2", FieldKind::Number(NumberDomain::Positive)),
    FieldRule::optional("pressureBasis", FieldKind::Enum(&PRESSURE_BASES)),
    FieldRule::optional(
        "referenceDensityKgM3",
        FieldKind::Number(NumberDomain::Positive),
    ),
    FieldRule::optional(
        "stackRecoveryFactor",
        FieldKind::Number(NumberDomain::NonNegative),
    ),
    FieldRule::optional("name", FieldKind::Text),
];

/// The nozzle record (schema section 7).
const NOZZLE_FIELDS: [FieldRule; 5] = [
    FieldRule::required("id", FieldKind::Text),
    FieldRule::required("name", FieldKind::Text),
    FieldRule::required(
        "dischargeCoefficient",
        FieldKind::Number(NumberDomain::Positive),
    ),
    FieldRule::required(
        "orificeDiameterM",
        FieldKind::Number(NumberDomain::Positive),
    ),
    FieldRule::optional(
        "referenceWaterDensityKgM3",
        FieldKind::Number(NumberDomain::Positive),
    ),
];

/// The machine-enforced schema of one record type.
pub fn schema(kind: RecordKind) -> &'static [FieldRule] {
    match kind {
        RecordKind::Tower => &TOWER_FIELDS,
        RecordKind::Fill => &FILL_FIELDS,
        RecordKind::DriftEliminator => &DRIFT_ELIMINATOR_FIELDS,
        RecordKind::Fan => &FAN_FIELDS,
        RecordKind::Nozzle => &NOZZLE_FIELDS,
    }
}

/// Validate one record's `field: value` pairs against the schema of `kind`.
///
/// `label` names the record in every refusal — the CLI passes the record's `id` when it has
/// one, otherwise its position. `Ok(())` means every declared field is present and every
/// value is in its declared domain; `Err(violations)` names every refused field, so a
/// caller can report all of them at once.
pub fn validate_record(
    kind: RecordKind,
    label: &str,
    fields: &[(&str, &str)],
) -> Result<(), Vec<Violation>> {
    let rules = schema(kind);
    let record = format!("{} record {label:?}", kind.as_str());
    let mut violations = Vec::new();
    let mut seen: Vec<&str> = Vec::with_capacity(fields.len());

    for (name, value) in fields {
        if seen.contains(name) {
            violations.push(Violation::new(
                &record,
                name,
                "is declared twice in the record".to_string(),
            ));
            continue;
        }
        seen.push(name);
        match rules.iter().find(|rule| rule.name == *name) {
            None => violations.push(Violation::new(
                &record,
                name,
                format!(
                    "is not a declared field of the {} schema — the selector does not know the field",
                    kind.as_str()
                ),
            )),
            Some(rule) => check_value(rule, value, &record, &mut violations),
        }
    }

    for rule in rules {
        if rule.required && !seen.contains(&rule.name) {
            violations.push(Violation::new(
                &record,
                rule.name,
                "is required and was not provided".to_string(),
            ));
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

/* ---------------- the value checks ---------------- */

fn check_value(rule: &FieldRule, raw: &str, record: &str, sink: &mut Vec<Violation>) {
    let field = rule.name;
    match rule.kind {
        FieldKind::Text => {
            if raw.is_empty() {
                sink.push(Violation::new(
                    record,
                    field,
                    "must not be empty".to_string(),
                ));
            }
        }
        FieldKind::Number(domain) => {
            if let Some(problem) = number_problem(domain, raw) {
                sink.push(Violation::new(record, field, problem));
            }
        }
        FieldKind::NumberList { domain, min_len } => {
            let entries: Vec<&str> = raw.split('|').collect();
            if entries.len() < min_len {
                sink.push(Violation::new(
                    record,
                    field,
                    format!(
                        "expects at least {min_len} '|'-separated numbers, got {}",
                        entries.len()
                    ),
                ));
                return;
            }
            for (index, entry) in entries.iter().enumerate() {
                if let Some(problem) = number_problem(domain, entry) {
                    sink.push(Violation::new(
                        record,
                        field,
                        format!("entry {} {problem}", index + 1),
                    ));
                }
            }
        }
        FieldKind::TextList { min_len } => {
            let entries: Vec<&str> = raw.split('|').collect();
            if entries.len() < min_len {
                sink.push(Violation::new(
                    record,
                    field,
                    format!(
                        "expects at least {min_len} '|'-separated entries, got {}",
                        entries.len()
                    ),
                ));
                return;
            }
            for (index, entry) in entries.iter().enumerate() {
                if entry.is_empty() {
                    sink.push(Violation::new(
                        record,
                        field,
                        format!("entry {} must not be empty", index + 1),
                    ));
                }
            }
        }
        FieldKind::Enum(allowed) => {
            if !allowed.contains(&raw) {
                sink.push(Violation::new(
                    record,
                    field,
                    format!("must be one of {}, got {raw:?}", listed(allowed)),
                ));
            }
        }
        FieldKind::EnumList { allowed, min_len } => {
            let entries: Vec<&str> = raw.split('|').collect();
            if entries.len() < min_len {
                sink.push(Violation::new(
                    record,
                    field,
                    format!(
                        "expects at least {min_len} '|'-separated entries, got {}",
                        entries.len()
                    ),
                ));
                return;
            }
            for entry in &entries {
                if !allowed.contains(entry) {
                    sink.push(Violation::new(
                        record,
                        field,
                        format!("must be one of {}, got {entry:?}", listed(allowed)),
                    ));
                }
            }
        }
        FieldKind::RatioBracket => {
            let entries: Vec<&str> = raw.split('|').collect();
            if entries.len() != 2 {
                sink.push(Violation::new(
                    record,
                    field,
                    format!("expects lower|upper, got {raw:?}"),
                ));
                return;
            }
            let mut ratios = [0.0_f64; 2];
            let mut ok = true;
            for (index, entry) in entries.iter().enumerate() {
                match number_problem(NumberDomain::Positive, entry) {
                    None => ratios[index] = entry.parse().unwrap_or_default(),
                    Some(problem) => {
                        ok = false;
                        sink.push(Violation::new(
                            record,
                            field,
                            format!("{} {problem}", bracket_side(index)),
                        ));
                    }
                }
            }
            if ok && ratios[0] > ratios[1] {
                sink.push(Violation::new(
                    record,
                    field,
                    format!(
                        "expects the lower ratio first, got {}|{}",
                        entries[0], entries[1]
                    ),
                ));
            }
        }
        FieldKind::Row(columns) => {
            let entries: Vec<&str> = raw.split('|').collect();
            if entries.len() != columns.len() {
                sink.push(Violation::new(
                    record,
                    field,
                    format!(
                        "expects {} '|'-separated numbers ({}), got {}",
                        columns.len(),
                        column_names(columns),
                        entries.len()
                    ),
                ));
                return;
            }
            for (column, entry) in columns.iter().zip(&entries) {
                if let Some(problem) = number_problem(column.domain, entry) {
                    sink.push(Violation::new(
                        record,
                        field,
                        format!("column {:?} {problem}", column.name),
                    ));
                }
            }
        }
        FieldKind::Points { columns, min_len } => {
            let points: Vec<&str> = raw.split('|').collect();
            if points.len() < min_len {
                sink.push(Violation::new(
                    record,
                    field,
                    format!(
                        "expects at least {min_len} '|'-separated points ({}), got {}",
                        column_names(columns),
                        points.len()
                    ),
                ));
                return;
            }
            for (index, point) in points.iter().enumerate() {
                let entries: Vec<&str> = point.split(':').collect();
                if entries.len() != columns.len() {
                    sink.push(Violation::new(
                        record,
                        field,
                        format!(
                            "point {} expects {} ':'-separated numbers ({}), got {point:?}",
                            index + 1,
                            columns.len(),
                            column_names(columns)
                        ),
                    ));
                    continue;
                }
                for (column, entry) in columns.iter().zip(&entries) {
                    if let Some(problem) = number_problem(column.domain, entry) {
                        sink.push(Violation::new(
                            record,
                            field,
                            format!("point {} column {:?} {problem}", index + 1, column.name),
                        ));
                    }
                }
            }
        }
    }
}

fn bracket_side(index: usize) -> &'static str {
    if index == 0 {
        "the lower ratio"
    } else {
        "the upper ratio"
    }
}

fn listed(allowed: &[&str]) -> String {
    allowed
        .iter()
        .map(|entry| format!("{entry:?}"))
        .collect::<Vec<String>>()
        .join(", ")
}

fn column_names(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|column| column.name)
        .collect::<Vec<&str>>()
        .join(":")
}

/// What is wrong with `raw` as a value of `domain`, or `None` when the value is acceptable.
fn number_problem(domain: NumberDomain, raw: &str) -> Option<String> {
    let value: f64 = match raw.parse() {
        Ok(value) => value,
        Err(_) => return Some(format!("must be a number, got {raw:?}")),
    };
    if !value.is_finite() {
        return Some(format!("must be finite, got {raw:?}"));
    }
    match domain {
        NumberDomain::Finite => None,
        NumberDomain::Positive if value <= 0.0 => Some(format!("must be positive, got {value}")),
        NumberDomain::NonNegative if value < 0.0 => {
            Some(format!("must not be negative, got {value}"))
        }
        _ => None,
    }
}
