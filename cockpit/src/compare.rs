//! drafthouse#89: the comparison surface's model - saved project variants, opened and recomputed.
//!
//! A comparison is two or three **saved `.drafthouse` project files**, each opened through #74's
//! reader ([`crate::project::Project::read`] + the catalog's own record lookups, exactly the path
//! [`crate::files::open_text`] takes) and then run by the engine on the file's own inputs. Nothing
//! here edits an in-memory copy of the draft: every number a comparison shows is the engine's own
//! output for that file, recomputed on open - the file's saved results are never trusted.
//!
//! The model is UI-free and engine-agnostic: [`open`] takes any [`Engine`], the metric readers are
//! the engine's own fields (drift is the same eliminator-curve call the Water screen makes), and the
//! diff/highlight rules live in [`changed`]/[`best_column`] so the painter and the tests read the
//! same predicates.
//!
//! What a comparison does not claim is written down in `docs/LIMITATIONS.md` - in particular what
//! "better" means for each engineering objective, and that a variant is a single design file, not a
//! selection run.

use cockpit::engine::{Engine, EngineError, EngineInput, EngineOutput};

use crate::state::Catalog;

/// How many variants a comparison holds: two or three (the issue's own range).
pub const MAX_VARIANTS: usize = 3;

/// One loaded variant: a saved project file, its digest, its recomputed input and run.
#[derive(Clone, Debug)]
pub struct Variant {
    /// The file's name as the surface shows it.
    pub name: String,
    /// SHA-256 of the file's bytes - the variant's identity, whatever it is called.
    pub digest: String,
    /// The complete engine input the file declares (records resolved through the catalog).
    pub input: EngineInput,
    /// The engine's own run of that input. A refused run is still the engine's answer.
    pub out: Result<EngineOutput, String>,
}

impl Variant {
    /// The engine's output, when the run returned one.
    pub fn output(&self) -> Option<&EngineOutput> {
        self.out.as_ref().ok()
    }
}

/// Open one saved project file as a comparison variant.
///
/// The file is read by #74's reader; the tower, fan, drift eliminator and nozzle ids are resolved
/// through `cat` (named refusals, the same rule [`crate::files::open_text`] applies); the duty,
/// fill stack and speed ratio come from the file. The engine then runs the resolved input - the
/// variant's numbers are that run's, never the file's saved results.
pub fn open(text: &str, name: &str, cat: &Catalog, engine: &dyn Engine) -> Result<Variant, String> {
    let project = crate::project::Project::read(text)?;
    if !project.catalog_revision_id.is_empty() && project.catalog_revision_id != cat.catalog_id {
        return Err(format!(
            "`{name}` was written against catalog revision `{}`; this session carries `{}` - import \
             that revision and open the file again",
            project.catalog_revision_id, cat.catalog_id
        ));
    }
    if !project.custom_parts.is_empty() {
        return Err(format!(
            "`{name}` carries {} custom record(s): resolving those would need the session's own \
             catalog, which a comparison never touches - open the file as the draft instead",
            project.custom_parts.len()
        ));
    }
    let tower = cat.tower(&project.tower).ok_or_else(|| {
        format!(
            "`{name}`: tower `{}` is not in the catalog revision",
            project.tower
        )
    })?;
    let fan = cat.fan(&project.fitted.fan).ok_or_else(|| {
        format!(
            "`{name}`: fan `{}` is not in the catalog revision",
            project.fitted.fan
        )
    })?;
    let drift = cat.drift(&project.fitted.drift).ok_or_else(|| {
        format!(
            "`{name}`: drift `{}` is not in the catalog revision",
            project.fitted.drift
        )
    })?;
    let nozzle = cat.nozzle(&project.fitted.nozzle).ok_or_else(|| {
        format!(
            "`{name}`: nozzle `{}` is not in the catalog revision",
            project.fitted.nozzle
        )
    })?;
    // The file carries every field of the input: the nine duty fields, the stack, the ratio and the
    // four fitted ids. `write_into` fills the duty and the stack; the records above are the
    // resolved ids. The placeholder duty below is fully overwritten, field for field, by
    // `write_into` - it is not a fallback value, and a file that omitted a duty field would have
    // failed `Project::read`'s own schema first (the block is `serde(default)`, so an absent field
    // reads as 0 - the reader's rule, not this module's).
    let mut input = EngineInput {
        duty: cockpit::engine::Duty {
            water_flow_m3_hr: 0.0,
            hot_water_c: 0.0,
            target_cold_water_c: 0.0,
            wet_bulb_c: 0.0,
            dry_bulb_c: 0.0,
            pressure_pa: 0.0,
            salinity_g_kg: 0.0,
            water_quality_class: String::new(),
            cycles_of_concentration: 0.0,
        },
        tower: tower.clone(),
        fill_layers: Vec::new(),
        drift: drift.clone(),
        fan: fan.clone(),
        speed_ratio: 0.0,
        nozzle: nozzle.clone(),
    };
    project.write_into(&mut input);
    if input.fill_layers.is_empty() {
        return Err(format!(
            "`{name}` declares no fill layers - the engine needs at least one"
        ));
    }
    let out = engine.run(&input).map_err(|e| match e {
        EngineError::Unavailable(why) => format!("the engine is not loaded: {why}"),
        EngineError::Schema(why) => why,
    });
    Ok(Variant {
        name: name.to_string(),
        digest: crate::sha256::hex(text.as_bytes()),
        input,
        out,
    })
}

// ===================================================================================== the metrics

/// The better direction of an engineering objective: which way is an improvement.
///
/// `Airflow` is deliberately absent: more air is not "better" without the objective it serves, and
/// a comparison that painted a winner there would be inventing one. `docs/LIMITATIONS.md` carries
/// the full statement, one row per objective.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Better {
    /// The smaller value serves the objective (pressure drop, power, water lost).
    Lower,
    /// The larger value serves the objective (duty capability).
    Higher,
}

/// One key-result row of the comparison: the issue's engineering headlines, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Metric {
    ColdWater,
    Airflow,
    TotalPressure,
    FanPower,
    Drift,
    WaterUse,
    Capability,
    Approach,
    Kavl,
}

impl Metric {
    /// Every row, in display order.
    pub const ALL: [Metric; 9] = [
        Metric::ColdWater,
        Metric::Airflow,
        Metric::TotalPressure,
        Metric::FanPower,
        Metric::Drift,
        Metric::WaterUse,
        Metric::Capability,
        Metric::Approach,
        Metric::Kavl,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Metric::ColdWater => "cold water",
            Metric::Airflow => "airflow",
            Metric::TotalPressure => "total pressure",
            Metric::FanPower => "fan power",
            Metric::Drift => "drift",
            Metric::WaterUse => "water use",
            Metric::Capability => "capability",
            Metric::Approach => "approach",
            Metric::Kavl => "KaV/L",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Metric::ColdWater => "°C",
            Metric::Airflow => "m³/s",
            Metric::TotalPressure => "Pa",
            Metric::FanPower => "kW",
            Metric::Drift => "ppm",
            Metric::WaterUse => "m³/h",
            Metric::Capability => "%",
            Metric::Approach => "K",
            Metric::Kavl => "fill",
        }
    }

    /// The decimals the surface prints (and therefore the diff tolerance).
    pub fn decimals(self) -> usize {
        match self {
            Metric::ColdWater => 2,
            Metric::Airflow => 1,
            Metric::TotalPressure => 1,
            Metric::FanPower => 1,
            Metric::Drift => 1,
            Metric::WaterUse => 2,
            Metric::Capability => 1,
            Metric::Approach => 2,
            Metric::Kavl => 3,
        }
    }

    /// The engineering direction, or `None` where the row carries no objective of its own.
    pub fn better(self) -> Option<Better> {
        match self {
            Metric::ColdWater => Some(Better::Lower),
            Metric::TotalPressure => Some(Better::Lower),
            Metric::FanPower => Some(Better::Lower),
            Metric::Drift => Some(Better::Lower),
            Metric::WaterUse => Some(Better::Lower),
            Metric::Capability => Some(Better::Higher),
            Metric::Approach => Some(Better::Lower),
            Metric::Airflow | Metric::Kavl => None,
        }
    }

    /// One value of this row from the engine's own output.
    ///
    /// Drift is the same call the Water screen and the answer card make: the draft eliminator's own
    /// curve at the run's face velocity (airflow over the drift area), through the engine crate's
    /// `drift_performance_at_velocity`. The rest are fields of the run itself.
    pub fn read(self, input: &EngineInput, out: &EngineOutput) -> Option<f64> {
        match self {
            Metric::ColdWater => Some(out.cold_water_c),
            Metric::Airflow => Some(out.airflow_m3_s),
            Metric::TotalPressure => Some(out.total_pressure_pa),
            Metric::FanPower => Some(out.fan_power_kw),
            Metric::Drift => crate::screens::data::drift_ppm_at(input, out),
            Metric::WaterUse => Some(out.makeup_m3_hr),
            Metric::Capability => Some(out.capability_pct),
            Metric::Approach => Some(out.approach_c),
            Metric::Kavl => Some(out.kavl_total),
        }
    }
}

/// One variant's value for a row: `None` when the run was refused.
pub fn value(metric: Metric, v: &Variant) -> Option<f64> {
    v.output().and_then(|o| metric.read(&v.input, o))
}

/// Every variant's value for one row, in column order.
pub fn values(metric: Metric, variants: &[Variant]) -> Vec<Option<f64>> {
    variants.iter().map(|v| value(metric, v)).collect()
}

/// Format a value at the row's display precision - the one place the print rule lives, so the
/// screen and the export cannot disagree.
pub fn display(metric: Metric, value: f64) -> String {
    format!("{:.*}", metric.decimals(), value)
}

// ================================================================================== the highlight

/// Half of the last digit a row prints: the smallest difference the displayed value can show.
pub fn display_tol(decimals: usize) -> f64 {
    0.5 * 10f64.powi(-(decimals as i32))
}

/// The highlighter: does `value` differ from the baseline enough to be seen, at the row's own
/// display precision? Both directions of the rule - a difference above the tolerance is lit, and a
/// difference at or below it is not.
pub fn changed(decimals: usize, baseline: Option<f64>, value: Option<f64>) -> bool {
    match (baseline, value) {
        (Some(base), Some(x)) => (x - base).abs() > display_tol(decimals),
        // A refused run has no value to compare; its cell states the refusal instead.
        _ => false,
    }
}

/// The best column for a row: the extreme in the row's own direction, when the row has a direction
/// and its values differ by more than the displayed tolerance. `None` when nothing is marked.
pub fn best_column(metric: Metric, row: &[Option<f64>]) -> Option<usize> {
    let direction = metric.better()?;
    let present: Vec<f64> = row.iter().flatten().copied().collect();
    let spread = present.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - present.iter().copied().fold(f64::INFINITY, f64::min);
    if present.len() < 2 || spread <= display_tol(metric.decimals()) {
        return None;
    }
    let pick = |a: &f64, b: &f64| match direction {
        Better::Lower => a.total_cmp(b),
        Better::Higher => b.total_cmp(a),
    };
    row.iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|x| (i, x)))
        .min_by(|a, b| pick(&a.1, &b.1))
        .map(|(i, _)| i)
}

/// What changed between a baseline file's input and another's - the names the surface (and its
/// section drawing) stand on. Site-recorded metadata is not an engine input and never appears here.
pub fn inputs_changed(baseline: &EngineInput, other: &EngineInput) -> Vec<&'static str> {
    let mut changed = Vec::new();
    if baseline.duty != other.duty {
        changed.push("the duty");
    }
    if baseline.tower.id != other.tower.id {
        changed.push("the tower");
    }
    if baseline.fill_layers != other.fill_layers {
        changed.push("the fill stack");
    }
    if baseline.fan.id != other.fan.id {
        changed.push("the fan");
    }
    if baseline.speed_ratio != other.speed_ratio {
        changed.push("fan speed");
    }
    if baseline.drift.id != other.drift.id {
        changed.push("the drift eliminator");
    }
    if baseline.nozzle.id != other.nozzle.id {
        changed.push("the nozzle");
    }
    changed
}

// ============================================================================== zones and verdicts

/// One variant's per-zone pressure split, in the engine's own air-path order: `(label, Pa, share)`.
pub fn zones(v: &Variant) -> Vec<(String, f64, f64)> {
    v.output()
        .map(|o| {
            o.pressure_by_zone
                .iter()
                .map(|z| (z.label.clone(), z.pressure_pa, z.share_pct))
                .collect()
        })
        .unwrap_or_default()
}

/// The zone labels across every variant, first-seen order: the shared rows of the split.
pub fn zone_labels(variants: &[Variant]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for v in variants {
        for (label, _, _) in zones(v) {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
    }
    labels
}

/// One variant's cell in a zone row: `(Pa, share %)`.
pub fn zone_of(v: &Variant, label: &str) -> Option<(f64, f64)> {
    zones(v)
        .into_iter()
        .find(|(l, _, _)| l == label)
        .map(|(_, pa, share)| (pa, share))
}

/// The engine's verdict for a variant - its own words, never a second opinion.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// The run returned output with no validation limits.
    Accepted,
    /// The engine refused: the schema/unavailable error, or the run's own `validation` limits.
    Refused(Vec<String>),
}

impl Verdict {
    pub fn declined(&self) -> bool {
        matches!(self, Verdict::Refused(_))
    }

    /// One line for the surface: `accepted`, or the reasons joined.
    pub fn line(&self) -> String {
        match self {
            Verdict::Accepted => "accepted".to_string(),
            Verdict::Refused(reasons) => format!("refused: {}", reasons.join(" · ")),
        }
    }
}

/// The engine's own verdict for a variant.
///
/// The contract's rule: a non-empty `validation` means the engine refused and no headline number
/// is valid; an `Err` is the schema/unavailable refusal. Both are the engine's words here.
pub fn verdict(v: &Variant) -> Verdict {
    match &v.out {
        Err(why) => Verdict::Refused(vec![why.clone()]),
        Ok(o) if !o.validation.is_empty() => {
            Verdict::Refused(o.validation.iter().map(|l| l.message.clone()).collect())
        }
        Ok(_) => Verdict::Accepted,
    }
}

// ==================================================================================== the export

/// A comparison sheet: the PDF bytes and the file name they travel under.
pub struct Sheet {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The comparison as a one-page A4 sheet on the report's own writer ([`crate::pdf`]), delivered by
/// the report's own path (`report_pdf::deliver`); the DEMO account gets the watermark. The sheet's
/// columns are the same variants the surface shows at the same display precision and with the same
/// changed/best rules, so a reader can hold the printout against the screen.
///
/// Where this sits relative to #85's calc sheet: the calc sheet is one draft's worked report, this
/// is the multi-variant comparison - a sheet of its own. Folding it into the calc sheet's page
/// plan would restructure that plan, which this lane deliberately does not do (see
/// `docs/LIMITATIONS.md`).
pub fn export_pdf(variants: &[Variant], demo: bool) -> Result<Sheet, String> {
    use crate::pdf::{self, Font, Pdf, A4_H, A4_W};

    const INK: [u8; 3] = [26, 28, 32];
    const INK2: [u8; 3] = [70, 74, 82];
    const MUTED: [u8; 3] = [130, 134, 142];
    const TEAL: [u8; 3] = [26, 116, 140];
    const RULE: [u8; 3] = [203, 208, 214];
    const LITBG: [u8; 3] = [232, 243, 247];
    const DANGER: [u8; 3] = [150, 52, 52];
    const OK: [u8; 3] = [52, 120, 80];
    const WATERMARK: [u8; 3] = [188, 192, 198];

    if variants.len() < 2 {
        return Err("a comparison sheet needs at least two variants".into());
    }
    let mut combined = String::new();
    for v in variants {
        combined.push_str(&v.digest);
    }
    let digest = crate::sha256::hex(combined.as_bytes());
    let name = format!("drafthouse-comparison-{}.pdf", &digest[..8]);

    let mut pdf = Pdf::new("drafthouse comparison");
    if demo {
        pdf.text_rot(150.0, 700.0, 96.0, Font::Bold, WATERMARK, 32.0, "DEMO");
    }
    let m = 40.0;
    let w = A4_W - 2.0 * m;
    let mut y = A4_H - 42.0;
    pdf.text(m, y, 8.2, Font::Bold, TEAL, "DRAFTHOUSE · COMPARISON SHEET");
    pdf.text_right(
        A4_W - m,
        y,
        7.6,
        Font::Mono,
        MUTED,
        &format!(
            "{} saved variants · each recomputed by the engine on open",
            variants.len()
        ),
    );
    y -= 20.0;
    pdf.text(m, y, 15.0, Font::Bold, INK, "Variants compared");
    y -= 12.0;
    for line in pdf::wrap(
        Font::Sans,
        9.0,
        "Each column was opened from a saved .drafthouse project file through the project reader \
         and recomputed by the engine; every number below is that run's own output, at the \
         precision the surface shows. The first column is the baseline.",
        w,
    ) {
        pdf.text(m, y, 9.0, Font::Sans, INK2, &line);
        y -= 11.0;
    }

    // the variant header block: name, the file's own digest, how it was read
    let label_w = 148.0;
    let gutter = 10.0;
    let col_w = (w - label_w - gutter * (variants.len() - 1) as f64) / variants.len() as f64;
    let col_x = |i: usize| m + label_w + (col_w + gutter) * i as f64;
    y -= 4.0;
    let header_y = y;
    let mut lowest = y;
    for (i, v) in variants.iter().enumerate() {
        let x = col_x(i);
        pdf.text_fit(x, y, 9.5, Font::Bold, INK, &v.name, col_w);
        let mut dy = y - 10.0;
        pdf.text(
            x,
            dy,
            7.4,
            Font::Sans,
            MUTED,
            "as saved, recomputed on open",
        );
        dy -= 8.0;
        pdf.text(x, dy, 7.2, Font::Mono, MUTED, "sha256");
        dy -= 7.2;
        for line in pdf::wrap(Font::Mono, 6.6, &v.digest, col_w) {
            pdf.text(x, dy, 6.6, Font::Mono, INK2, &line);
            dy -= 7.2;
        }
        lowest = lowest.min(dy);
    }
    y = lowest - 6.0;
    pdf.line(m, y, A4_W - m, y, 0.6, RULE);
    let _ = header_y;
    y -= 13.0;

    // the key rows: the same metric readers, precision and rules the surface uses
    let values: Vec<Vec<Option<f64>>> =
        Metric::ALL.iter().map(|mm| values(*mm, variants)).collect();
    for (ri, mm) in Metric::ALL.iter().enumerate() {
        let base_v = value(*mm, &variants[0]);
        let best = best_column(*mm, &values[ri]);
        pdf.text(m, y, 9.0, Font::Sans, INK2, mm.label());
        pdf.text_right(m + label_w - 8.0, y, 7.4, Font::Mono, MUTED, mm.unit());
        for (col, cell_v) in values[ri].iter().enumerate() {
            let Some(x) = *cell_v else {
                pdf.text(col_x(col), y, 9.0, Font::Sans, DANGER, "refused");
                continue;
            };
            let lit = col > 0 && changed(mm.decimals(), base_v, Some(x));
            let mut cell = display(*mm, x);
            if lit {
                cell.push_str(" *");
            }
            if best == Some(col) {
                cell.push_str(" · best");
                pdf.rect(
                    col_x(col) - 2.0,
                    y - 3.2,
                    col_w + 4.0,
                    12.0,
                    Some(LITBG),
                    None,
                );
            }
            pdf.text(
                col_x(col),
                y,
                9.5,
                Font::Mono,
                if lit { TEAL } else { INK },
                &cell,
            );
        }
        y -= 14.2;
    }
    pdf.text(
        m,
        y,
        7.4,
        Font::Sans,
        MUTED,
        "* differs from the baseline column · · best = the better value in the row's own engineering direction (airflow and KaV/L carry none)",
    );
    y -= 16.0;

    // the per-zone pressure split, the engine's own air path
    pdf.line(m, y + 8.0, A4_W - m, y + 8.0, 0.6, RULE);
    pdf.text(
        m,
        y - 2.0,
        9.0,
        Font::Bold,
        INK,
        "Pressure by zone · Pa · share %",
    );
    y -= 15.0;
    let z_labels = zone_labels(variants);
    for label in &z_labels {
        let base_pa = zone_of(&variants[0], label).map(|(pa, _)| pa);
        pdf.text_fit(m, y, 8.4, Font::Sans, INK2, label, label_w - 10.0);
        for (col, v) in variants.iter().enumerate() {
            match zone_of(v, label) {
                Some((pa, share)) => {
                    let lit = col > 0 && changed(1, base_pa, Some(pa));
                    let cell = format!("{pa:.1} · {share:.0}%{}", if lit { " *" } else { "" });
                    pdf.text(
                        col_x(col),
                        y,
                        8.8,
                        Font::Mono,
                        if lit { TEAL } else { INK },
                        &cell,
                    );
                }
                None => {
                    pdf.text(col_x(col), y, 8.8, Font::Sans, MUTED, "-");
                }
            }
        }
        y -= 11.4;
    }
    y -= 8.0;

    // the per-variant status: the engine's own verdict, verbatim
    pdf.line(m, y + 6.0, A4_W - m, y + 6.0, 0.6, RULE);
    pdf.text(m, y - 2.0, 9.0, Font::Bold, INK, "Status");
    y -= 15.0;
    for (col, v) in variants.iter().enumerate() {
        let (line, rgb) = match verdict(v) {
            Verdict::Accepted => ("accepted".to_string(), OK),
            Verdict::Refused(why) => (format!("refused · {}", why.join(" · ")), DANGER),
        };
        let x = col_x(col);
        pdf.text_fit(x, y, 8.2, Font::Sans, rgb, &v.name, col_w);
        let mut dy = y - 10.0;
        for line in pdf::wrap(Font::Sans, 8.2, &line, col_w) {
            pdf.text(x, dy, 8.2, Font::Sans, rgb, &line);
            dy -= 9.6;
        }
    }

    // the footer: the exact bytes behind the columns
    let mut fy = m + 26.0;
    pdf.line(m, fy + 14.0, A4_W - m, fy + 14.0, 0.6, RULE);
    for line in pdf::wrap(
        Font::Mono,
        6.6,
        &format!("files sha256 (in column order): {combined}"),
        w,
    ) {
        pdf.text(m, fy, 6.6, Font::Mono, MUTED, &line);
        fy -= 7.2;
    }
    pdf.text(
        m,
        fy,
        6.8,
        Font::Sans,
        MUTED,
        "drafthouse · comparison sheet · the comparison surface's export",
    );

    Ok(Sheet {
        name,
        bytes: pdf.finish(),
    })
}

/// Test support (#89): the committed variant files, opened through [`open`] exactly as the surface
/// would open them - the screen tests draw and measure the same variants the model tests assert on.
#[cfg(test)]
pub fn fixture_variants(names: &[&str]) -> Vec<Variant> {
    use cockpit::fixture_engine::FixtureEngine;

    let fixture = include_str!("../assets/fixture.json");
    let engine = crate::engine_select::build(fixture, None).expect("the build's engine");
    let fx = FixtureEngine::from_json(fixture).expect("the fixture parses");
    let cat = Catalog::from_fixture(&fx);
    names
        .iter()
        .map(|name| {
            let path: std::path::PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets", "variants", name]
                .iter()
                .collect();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            open(&text, name, &cat, engine.as_ref()).unwrap_or_else(|e| panic!("open {name}: {e}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::engine::Engine;
    use cockpit::fixture_engine::FixtureEngine;
    use std::path::PathBuf;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    /// The committed variant files, read from disk at test time - real files, byte for byte what
    /// `cargo test` opens through the reader.
    fn variant_file(name: &str) -> String {
        let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets", "variants", name]
            .iter()
            .collect();
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn engine() -> Box<dyn Engine> {
        crate::engine_select::build(FIXTURE, None).expect("the build's engine")
    }

    fn catalog() -> Catalog {
        let fx = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        Catalog::from_fixture(&fx)
    }

    /// The fixture's own recorded default, via the engine that ships with the build: the base every
    /// variant file is a saved state of.
    fn default_input() -> EngineInput {
        FixtureEngine::from_json(FIXTURE)
            .expect("the fixture parses")
            .default_input()
    }

    // A note on what a "separate run" is in these tests: the app's own file path is
    // `files::open_text` (the File menu's open) into a session draft, and then the engine's run of
    // that draft. The comparison must report exactly those numbers, per file - so the reference
    // below is a different code path from `compare::open` even though both end in `Engine::run`.

    /// Issue #89 AC 1: two fixture variants, each shown with the values a separate run of the same
    /// file produces. `files::open_text` is the app's own open; its run must equal the comparison's
    /// column exactly, field for field, and the two variants must actually differ (or the equality
    /// would hold for a surface that copied one column into both).
    #[test]
    fn two_fixture_variants_show_their_own_separate_runs() {
        let engine = engine();
        let cat = catalog();
        let a_text = variant_file("a-base.drafthouse");
        let b_text = variant_file("b-fan-faster.drafthouse");
        let a = open(&a_text, "a-base.drafthouse", &cat, engine.as_ref()).expect("A opens");
        let b = open(&b_text, "b-fan-faster.drafthouse", &cat, engine.as_ref()).expect("B opens");

        // The separate runs: the File menu's own open path, into a scratch session.
        let (mut draft_a, mut cat_a, mut session_a, mut slot_a) = scratch();
        crate::files::open_text(
            &a_text,
            scratch_ctx(),
            &mut draft_a,
            &mut cat_a,
            &mut slot_a,
            &mut session_a,
        )
        .expect("A opens through the app's own path");
        let (mut draft_b, mut cat_b, mut session_b, mut slot_b) = scratch();
        crate::files::open_text(
            &b_text,
            scratch_ctx(),
            &mut draft_b,
            &mut cat_b,
            &mut slot_b,
            &mut session_b,
        )
        .expect("B opens through the app's own path");
        let run_a = engine.run(&draft_a).expect("A's separate run");
        let run_b = engine.run(&draft_b).expect("B's separate run");

        // One value at a time: the comparison's column equals the separate run's output, whole.
        assert_eq!(a.out.as_ref().expect("A ran"), &run_a);
        assert_eq!(b.out.as_ref().expect("B ran"), &run_b);

        // The pair is a real pair: the files' inputs differ in the fan speed alone, and the runs
        // do not agree where the inputs differ - so a surface that copied one column into the other
        // cannot pass the per-column equality above.
        assert_eq!(
            inputs_changed(&a.input, &b.input),
            vec!["fan speed"],
            "the fixtures differ in exactly one engine input"
        );
        assert_ne!(
            a.output().unwrap().cold_water_c,
            b.output().unwrap().cold_water_c,
            "the pair's runs differ where its inputs do"
        );

        // And per row, the surfaces' values ARE the separate engine's: recompute each row from the
        // separate run and compare with what the comparison reports.
        let variants = vec![a, b];
        for metric in Metric::ALL {
            let shown = values(metric, &variants);
            let separate = vec![metric.read(&draft_a, &run_a), metric.read(&draft_b, &run_b)];
            assert_eq!(
                shown,
                separate,
                "{}: shown vs separate runs",
                metric.label()
            );
        }
        // Drift is the one row with an engine call of its own; pin that it is the same numbers the
        // Water screen reads (not `None` under a real-engine build).
        #[cfg(feature = "real-engine")]
        assert!(
            values(Metric::Drift, &variants).iter().all(|v| v.is_some()),
            "the drift row carries the engine's own ppm"
        );
    }

    /// Issue #89 AC 2, the sharp direction: a pair that differs in exactly ONE value lights exactly
    /// that row and leaves its neighbours alone. (A test that passes when everything is lit would
    /// fail the neighbour assertions here.)
    #[test]
    fn one_differing_value_lights_exactly_its_row() {
        let metrics = Metric::ALL;
        let baseline: Vec<Option<f64>> = vec![
            Some(31.65),
            Some(124.8),
            Some(190.1),
            Some(28.7),
            Some(4.2),
            Some(15.03),
            Some(112.0),
            Some(4.65),
            Some(1.602),
        ];
        // identical everywhere but the third row by more than its displayed tolerance
        let mut neighbour = baseline.clone();
        neighbour[2] = Some(190.9);
        let lit: Vec<bool> = metrics
            .iter()
            .zip(&baseline)
            .zip(&neighbour)
            .map(|((m, b), v)| changed(m.decimals(), *b, *v))
            .collect();
        assert_eq!(
            lit,
            vec![false, false, true, false, false, false, false, false, false],
            "exactly the third row is lit"
        );
        // ...and a difference the display cannot show is not lit (the same value at both ends of
        // the tolerance)
        assert!(!changed(2, Some(31.65), Some(31.6549)), "under half a cent");
        assert!(changed(2, Some(31.65), Some(31.6551)), "over half a cent");
        // in the other direction, the lower-value change is lit all the same
        let mut lower = baseline.clone();
        lower[2] = Some(189.5);
        assert!(changed(metrics[2].decimals(), baseline[2], lower[2]));
        assert!(baseline
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || !changed(metrics[i].decimals(), *b, lower[i])));
    }

    /// Issue #89 AC 2, both directions on the committed files: A vs B light exactly the rows whose
    /// runs differ (nothing else), and A vs C - files that differ only in recorded site rows the
    /// engine does not read - light nothing at all.
    #[test]
    fn the_lit_rows_are_exactly_the_rows_that_moved() {
        let engine = engine();
        let cat = catalog();
        let a_text = variant_file("a-base.drafthouse");
        let b_text = variant_file("b-fan-faster.drafthouse");
        let c_text = variant_file("c-recorded.drafthouse");
        let a = open(&a_text, "a-base.drafthouse", &cat, engine.as_ref()).expect("A opens");
        let b = open(&b_text, "b-fan-faster.drafthouse", &cat, engine.as_ref()).expect("B opens");
        let c = open(&c_text, "c-recorded.drafthouse", &cat, engine.as_ref()).expect("C opens");

        let lit = |base: &Variant, other: &Variant| -> Vec<(Metric, bool)> {
            Metric::ALL
                .into_iter()
                .map(|m| (m, changed(m.decimals(), value(m, base), value(m, other))))
                .collect()
        };

        let for_base = lit(&a, &a);
        assert!(
            for_base.iter().all(|(_, lit)| !lit),
            "the baseline itself is not highlighted"
        );

        let a_vs_b = lit(&a, &b);
        assert!(
            a_vs_b.iter().any(|(_, lit)| *lit),
            "the fan edit moves at least one row"
        );
        for (metric, is_lit) in &a_vs_b {
            let (va, vb) = (value(*metric, &a), value(*metric, &b));
            let moved = match (va, vb) {
                (Some(x), Some(y)) => (y - x).abs() > display_tol(metric.decimals()),
                _ => false,
            };
            assert_eq!(
                *is_lit,
                moved,
                "{}: lit must be exactly 'the value moved'",
                metric.label()
            );
        }

        let a_vs_c = lit(&a, &c);
        assert!(
            a_vs_c.iter().all(|(_, lit)| !lit),
            "C differs only in recorded site rows: nothing is highlighted"
        );
        assert_ne!(
            a.digest, c.digest,
            "the two files do differ (the recorded row is in the bytes)"
        );
        assert_eq!(
            inputs_changed(&a.input, &c.input),
            Vec::<&'static str>::new(),
            "and the difference is not an engine input"
        );
    }

    /// The best column is the extreme in the row's own direction, and it is only marked when the
    /// values differ by more than the display can show; a row with no objective is never marked.
    #[test]
    fn the_best_column_follows_the_rows_own_direction() {
        let cold = values_row(Metric::ColdWater, [Some(31.65), Some(31.12), Some(31.90)]);
        assert_eq!(
            best_column(Metric::ColdWater, &cold),
            Some(1),
            "lowest cools"
        );
        let power = values_row(Metric::FanPower, [Some(41.1), Some(28.7), Some(29.1)]);
        assert_eq!(best_column(Metric::FanPower, &power), Some(1));
        let cap = values_row(Metric::Capability, [Some(112.0), Some(130.7), Some(125.6)]);
        assert_eq!(
            best_column(Metric::Capability, &cap),
            Some(1),
            "highest first"
        );
        let air = values_row(Metric::Airflow, [Some(124.8), Some(141.9), Some(121.7)]);
        assert_eq!(best_column(Metric::Airflow, &air), None, "no direction");
        let flat = values_row(Metric::FanPower, [Some(28.70), Some(28.70), Some(28.70)]);
        assert_eq!(
            best_column(Metric::FanPower, &flat),
            None,
            "nothing differs"
        );
        let refused = values_row(Metric::FanPower, [Some(28.70), None, Some(25.1)]);
        assert_eq!(best_column(Metric::FanPower, &refused), Some(2));
    }

    fn values_row(metric: Metric, v: [Option<f64>; 3]) -> Vec<Option<f64>> {
        let _ = metric;
        v.to_vec()
    }

    /// Regenerate the committed variant fixtures.
    ///
    /// Deliberate runs only: `cd cockpit && cargo test --lib \
    /// compare::tests::regenerate_the_committed_fixtures -- --ignored`. The files are written by
    /// `files::snapshot_text` - the app's own writer - from the fixture's own recorded default:
    /// A as saved, B with the fan speed moved (0.78 -> 0.81, inside AX-500's 0.70..1.13 band and
    /// no other input touched), C = A with one recorded site reading (display-only, the engine
    /// does not read it) filled in. The always-running tests below then read them back from disk.
    #[test]
    #[ignore = "writes the committed fixture files; run deliberately when the fixture's inputs move"]
    fn regenerate_the_committed_fixtures() {
        use crate::state::StartOptions;

        let fixture_engine = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        let engine = engine();
        let cat = catalog();
        let base = fixture_engine.default_input();
        let spec = crate::state::DutySpecRes::load().spec;
        let session = crate::files::FileSession {
            revision_id: crate::revision::SHIPPED_ID.to_string(),
            ..crate::files::FileSession::default()
        };
        let options = StartOptions::default();

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/variants");
        std::fs::create_dir_all(&dir).expect("the variants dir");
        let write = |name: &str, text: &str| {
            std::fs::write(dir.join(name), text).unwrap_or_else(|e| panic!("write {name}: {e}"));
        };

        // A: the fixture's default duty, results = its own run.
        let run_a = engine.run(&base).expect("the fixture runs");
        let a_text = crate::files::snapshot_text(
            &crate::files::Ctx {
                options: &options,
                host_label: crate::files::plane_label(),
                spec: spec.as_ref(),
                output: Some(&run_a),
                fixture_text: FIXTURE,
            },
            &session,
            &base,
            &cat,
            Some(engine.as_ref()),
        );
        write("a-base.drafthouse", &a_text);

        // B: the fan speed moved one step, saved (like the app would save it) before any run.
        let mut b = base.clone();
        b.speed_ratio = 0.81;
        let b_text = crate::files::snapshot_text(
            &crate::files::Ctx {
                options: &options,
                host_label: crate::files::plane_label(),
                spec: spec.as_ref(),
                output: None,
                fixture_text: FIXTURE,
            },
            &session,
            &b,
            &cat,
            Some(engine.as_ref()),
        );
        write("b-fan-faster.drafthouse", &b_text);

        // C: A with one recorded site reading filled in - display-only rows the engine does not
        // read. The bytes differ from A; the engine's run cannot.
        let mut c = crate::project::Project::read(&a_text).expect("A reads back");
        c.site
            .water_quality_recorded
            .insert("tds".to_string(), Some(350.0));
        write("c-recorded.drafthouse", &c.write());
    }

    /// Issue #117: the native `--compare` route's exported sheet, written for the web parity test
    /// (`tests/compare-web-parity.test.js`).
    ///
    /// This is the native side of the parity claim, driven through the route's own pieces: the
    /// commands are built by the same function the native file work uses
    /// ([`crate::files::compare_file_command`]), each is parsed exactly as the screens' command
    /// channel parses them (`screens::apply`), opened through the same [`open`] the screen's frame
    /// calls, and exported through the same [`export_pdf`] the screen's button calls. The sheet
    /// lands as `<out>/native-sheet.pdf` and the readback facts as `<out>/native-facts.json`;
    /// `DRAFTHOUSE_COMPARE_PARITY_OUT` names the directory. `#[ignore]`d: it writes files, so it is
    /// run by the parity test, or deliberately with the variable set.
    #[test]
    #[ignore = "writes the native route's comparison sheet for tests/compare-web-parity.test.js; run through that test"]
    fn native_compare_sheet_for_parity() {
        let out = std::env::var("DRAFTHOUSE_COMPARE_PARITY_OUT").expect(
            "DRAFTHOUSE_COMPARE_PARITY_OUT must name the directory the sheet is written to",
        );
        let out = PathBuf::from(out);
        std::fs::create_dir_all(&out).expect("the parity out directory");

        // The native route: one command per file, built where the file system is.
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/variants");
        let paths = [
            assets.join("a-base.drafthouse"),
            assets.join("b-fan-faster.drafthouse"),
        ];
        let commands: Vec<String> = paths
            .iter()
            .map(|path| {
                crate::files::compare_file_command(path)
                    .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
                    .1
            })
            .collect();

        // The command channel's own parse (`screens::apply`): everything after the verb and the
        // name's first `:` is the file's text.
        let engine = engine();
        let cat = catalog();
        let variants: Vec<Variant> = commands
            .iter()
            .map(|command| {
                let rest = command
                    .strip_prefix("compare:open:")
                    .expect("the route's own verb");
                let (name, text) = rest.split_once(':').expect("compare:open:<name>:<text>");
                open(text, name, &cat, engine.as_ref())
                    .unwrap_or_else(|e| panic!("open {name}: {e}"))
            })
            .collect();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].name, "a-base.drafthouse");
        assert_eq!(variants[1].name, "b-fan-faster.drafthouse");
        assert_ne!(
            variants[0].digest, variants[1].digest,
            "the two files are two variants"
        );

        // The button's own call: the sheet, at the default DEMO account (`screens::Account::Demo`).
        let sheet = export_pdf(&variants, true).expect("the native sheet exports");
        std::fs::write(out.join("native-sheet.pdf"), &sheet.bytes).expect("write the sheet");

        // The facts the web leg checks its own screen against: the same names, every row's
        // displayed string (the painter's own `display`), the sheet's name and digest.
        let rows: Vec<serde_json::Value> = Metric::ALL
            .iter()
            .map(|m| {
                serde_json::json!([
                    m.label(),
                    variants
                        .iter()
                        .map(|v| match value(*m, v) {
                            Some(x) => display(*m, x),
                            None => "refused".to_string(),
                        })
                        .collect::<Vec<String>>(),
                ])
            })
            .collect();
        let facts = serde_json::json!({
            "files": variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
            "digests": variants.iter().map(|v| v.digest.clone()).collect::<Vec<_>>(),
            "sheet_name": sheet.name,
            "sheet_bytes": sheet.bytes.len(),
            "sheet_sha256": crate::sha256::hex(&sheet.bytes),
            "demo": true,
            "rows": rows,
        });
        std::fs::write(
            out.join("native-facts.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&facts).expect("facts serialize")
            ),
        )
        .expect("write the facts");
        println!(
            "native comparison sheet: {} ({} bytes, sha256 {}) for {}",
            sheet.name,
            sheet.bytes.len(),
            crate::sha256::hex(&sheet.bytes),
            variants
                .iter()
                .map(|v| v.name.as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        );
    }

    /// The verdict is the engine's own channel: a clean run is accepted; a run with validation
    /// limits, or an error, is refused naming the engine's reasons - no second opinion is formed.
    #[test]
    fn the_verdict_is_the_engines_own() {
        let engine = engine();
        let cat = catalog();
        let a_text = variant_file("a-base.drafthouse");
        let a = open(&a_text, "a-base.drafthouse", &cat, engine.as_ref()).expect("A opens");
        assert_eq!(verdict(&a), Verdict::Accepted);
        assert!(!verdict(&a).declined());

        let mut refused = a.clone();
        let mut out = a.output().unwrap().clone();
        out.validation = vec![cockpit::engine::Limit {
            field: "run".into(),
            message: "the engine refused 12 of the candidate combinations: fill envelope".into(),
            value: 12.0,
            unit: "combinations".into(),
            min: None,
            max: None,
        }];
        refused.out = Ok(out);
        match verdict(&refused) {
            Verdict::Refused(reasons) => {
                assert_eq!(reasons.len(), 1);
                assert!(reasons[0].contains("12 of the candidate combinations"));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }

        let mut broken = a.clone();
        broken.out = Err("fill record `nope` is not in the engine's catalog".into());
        match verdict(&broken) {
            Verdict::Refused(reasons) => {
                assert!(reasons[0].contains("not in the engine's catalog"))
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The zone split comes from the engine's own `pressure_by_zone`, in its order, and the labels
    /// line up across the variants that share the tower.
    #[test]
    fn the_zone_split_is_the_engines_own() {
        let engine = engine();
        let cat = catalog();
        let a_text = variant_file("a-base.drafthouse");
        let b_text = variant_file("b-fan-faster.drafthouse");
        let a = open(&a_text, "a-base.drafthouse", &cat, engine.as_ref()).expect("A opens");
        let b = open(&b_text, "b-fan-faster.drafthouse", &cat, engine.as_ref()).expect("B opens");
        let za = zones(&a);
        assert!(!za.is_empty(), "the engine reports zones");
        assert_eq!(
            za,
            a.output()
                .unwrap()
                .pressure_by_zone
                .iter()
                .map(|z| (z.label.clone(), z.pressure_pa, z.share_pct))
                .collect::<Vec<_>>(),
            "the model copies the engine's own zones, nothing else"
        );
        let labels = zone_labels(&[a.clone(), b.clone()]);
        for label in &labels {
            assert!(zone_of(&a, label).is_some(), "{label} in A");
            assert!(zone_of(&b, label).is_some(), "{label} in B");
        }
        // The shares are shares of the same total: each variant's sum is ~100 %.
        for v in [&a, &b] {
            let sum: f64 = zones(v).iter().map(|(_, _, share)| share).sum();
            assert!((sum - 100.0).abs() < 0.5, "shares sum to ~100: {sum}");
        }
        // A refused variant has no zone rows rather than invented ones.
        let mut refused = a.clone();
        refused.out = Err("schema".into());
        assert!(zones(&refused).is_empty());
    }

    // ------------------------------------------------------------------ scratch session plumbing

    /// A scratch draft/catalog/session/slot for `files::open_text`, all defaulted from the fixture.
    #[allow(clippy::type_complexity)]
    fn scratch() -> (
        EngineInput,
        Catalog,
        crate::files::FileSession,
        crate::app::EngineSlot,
    ) {
        (
            default_input(),
            catalog(),
            crate::files::FileSession {
                revision_id: crate::revision::SHIPPED_ID.to_string(),
                ..crate::files::FileSession::default()
            },
            Default::default(),
        )
    }

    fn scratch_ctx() -> &'static crate::files::Ctx<'static> {
        Box::leak(Box::new(crate::files::Ctx {
            options: Box::leak(Box::new(crate::state::StartOptions::default())),
            host_label: crate::files::HOST_WEB_INTERNAL,
            spec: None,
            output: None,
            fixture_text: FIXTURE,
        }))
    }
}
