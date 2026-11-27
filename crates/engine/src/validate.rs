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
    Finite,
    Positive,
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
    Text,
    Number(NumberDomain),
    NumberList {
        domain: NumberDomain,
        min_len: usize,
    },
    TextList {
        min_len: usize,
    },
    Enum(&'static [&'static str]),
    EnumList {
        allowed: &'static [&'static str],
        min_len: usize,
    },
    RatioBracket,
    Row(&'static [Column]),
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
    /// `«kind» record «label»`, e.g.
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

const CURVE_MIN_POINTS: usize = 2;

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
    FieldRule::optional("fillStacks", FieldKind::TextList { min_len: 1 }),
    FieldRule::optional("name", FieldKind::Text),
    FieldRule::optional("draftType", FieldKind::Text),
    FieldRule::optional(
        "effectiveDraftHeightM",
        FieldKind::Number(NumberDomain::Positive),
    ),
];

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

const FAN_FIELDS: [FieldRule; 11] = [
    FieldRule::required("id", FieldKind::Text),
    FieldRule::required("allowedSpeedRatio", FieldKind::RatioBracket),
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
