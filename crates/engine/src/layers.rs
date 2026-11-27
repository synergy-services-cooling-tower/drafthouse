//! Ordered fill layers — the layered fill-stack contract (issue #54).
//!
//! A tower's fill stack is an **ordered list of layers**, top first. Each layer names the fill
//! record it is built from, its own depth, and the multipliers that apply to that fill's
//! thermal and pressure characteristics. The stack's result carries **one entry per layer** —
//! the airflow and loadings the layer sees, its own pressure drop and thermal contribution —
//! plus the stack's combined totals.
//!
//! The layer is a contract, not a second physics model. A layer's thermal contribution is the
//! ported `fillThermalMerkelNumber` at that layer's depth and multipliers, and its pressure
//! drop is the ported `fillPressureDropPa`; the stack's totals are those terms summed in
//! physical order ([`crate::layered_system_pressure_breakdown`]). A one-layer stack is
//! therefore the single-fill contract itself — the same functions, the same operand order —
//! which is what makes the existing published numbers the one-layer case of this path.
//!
//! **Refusals name the layer.** An empty stack, a layer whose fill record is not in the
//! catalog, a non-positive depth or multiplier, a fill record without the characteristic the
//! layer needs, and a layer outside its fill's own operating limits are all refusals that name
//! the layer's position, fill id and depth. Nothing here clamps a value into range.
//!
//! These records are synthetic fixtures (`src/data/`, and the specs a caller writes); nothing
//! in this module reads or claims a vendor or project report.

use crate::airside::{
    envelope_failures, FillLimits, FillPressureCorrelation, FillRecord, ZoneCorrelation,
};
use crate::numeric::DomainError;

/// One layer of a tower's fill stack.
///
/// `thermal_multiplier` and `pressure_multiplier` are the layer's own multipliers — a
/// manufacturer's derating for that layer, or 1 for a layer that contributes its fill record's
/// characteristics unmodified. The run-level multipliers (the water-quality factor in the
/// selector, or the CLI's `--thermal-multiplier`) are a separate input to
/// [`resolve_fill_layers`], so a layer's result can report both.
#[derive(Clone, Debug, PartialEq)]
pub struct FillLayer {
    pub fill_id: String,
    pub depth_m: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

impl FillLayer {
    /// A layer at both multipliers 1: it contributes exactly its fill record's characteristics
    /// over `depth_m`. Non-positive depths and multipliers are refused at resolution, not here.
    pub fn new(fill_id: impl Into<String>, depth_m: f64) -> Self {
        FillLayer {
            fill_id: fill_id.into(),
            depth_m,
            thermal_multiplier: 1.0,
            pressure_multiplier: 1.0,
        }
    }

    /// The layer's own thermal multiplier.
    pub fn with_thermal_multiplier(mut self, thermal_multiplier: f64) -> Self {
        self.thermal_multiplier = thermal_multiplier;
        self
    }

    /// The layer's own pressure multiplier.
    pub fn with_pressure_multiplier(mut self, pressure_multiplier: f64) -> Self {
        self.pressure_multiplier = pressure_multiplier;
        self
    }

    /// `<fillId>@<depthM>` — the layer's name, in the result and in the record specs the CLI
    /// reads (`format!`'s shortest round-trip form of the depth parses back to the same value).
    pub fn label(&self) -> String {
        format!("{}@{}", self.fill_id, self.depth_m)
    }
}

/// A complete fill stack: the layers in physical order, **top first**.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FillStack {
    pub layers: Vec<FillLayer>,
}

impl FillStack {
    pub fn new(layers: Vec<FillLayer>) -> Self {
        FillStack { layers }
    }

    /// The single-fill contract as the one-layer case of the stack contract.
    pub fn single(fill_id: impl Into<String>, depth_m: f64) -> Self {
        FillStack::new(vec![FillLayer::new(fill_id, depth_m)])
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// The top-to-bottom total depth of the stack.
    pub fn total_depth_m(&self) -> f64 {
        self.layers.iter().map(|layer| layer.depth_m).sum()
    }

    /// `<layer>+<layer>+…` — the stack's name, used as a candidate's fill identity and echoed
    /// in the layered results. A one-layer stack's label is that layer's own label.
    pub fn label(&self) -> String {
        self.layers
            .iter()
            .map(FillLayer::label)
            .collect::<Vec<String>>()
            .join("+")
    }

    /// The stack's identity: `(fill id, depth)` for a **one-layer** stack — the single-fill
    /// candidate's own identity, byte for byte — and `(stack name, total depth)` for a mixed
    /// stack, whose identity is the stack itself rather than any one of its fills.
    pub fn identity(&self) -> (String, f64) {
        match self.layers.as_slice() {
            [layer] => (layer.fill_id.clone(), layer.depth_m),
            _ => (self.label(), self.total_depth_m()),
        }
    }
}

/// The refusal an empty stack earns, wherever it is raised (resolution, or the breakdown's own
/// backstop guard).
pub fn empty_stack_error() -> DomainError {
    DomainError::new("A fill stack must carry at least one layer.")
}

/// One layer resolved against the catalog: the fill record it is built from, its depth and its
/// own multipliers. The run-level multipliers are the breakdown input's
/// ([`crate::LayeredBreakdownInput`]), so one stack cannot mix two runs' factors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerTerms<'a> {
    pub fill: &'a FillRecord,
    pub depth_m: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
}

impl LayerTerms<'_> {
    /// A layer at both of its own multipliers 1: it contributes exactly its fill record's
    /// characteristics over `depth_m`. This is the single-fill contract as one layer.
    pub fn of_fill(fill: &FillRecord, depth_m: f64) -> LayerTerms<'_> {
        LayerTerms {
            fill,
            depth_m,
            thermal_multiplier: 1.0,
            pressure_multiplier: 1.0,
        }
    }

    /// The layer's own name, as the caller declared it.
    pub fn label(&self) -> String {
        FillLayer {
            fill_id: self.fill.id.clone(),
            depth_m: self.depth_m,
            thermal_multiplier: self.thermal_multiplier,
            pressure_multiplier: self.pressure_multiplier,
        }
        .label()
    }
}

/// `Layer <position> (<fillId>@<depthM>)` — the head every layered refusal carries.
fn layer_head(position: usize, layer: &FillLayer) -> String {
    format!("Layer {position} ({})", layer.label())
}

/// Resolve a stack against the catalog, in physical order.
///
/// Refusals, all naming the layer they are about: an empty stack; a fill id no record in the
/// catalog carries; a depth or a multiplier that is not positive (or not finite); and a fill
/// record missing the thermal or pressure characteristic its layers need. The reference's own
/// guard texts are reused (`depthM must be positive.`, `Fill thermal correlation is missing.`),
/// each prefixed with the layer's identity so a seven-layer stack cannot report a bare `depthM`.
pub fn resolve_fill_layers<'a>(
    stack: &FillStack,
    fills: &'a [FillRecord],
) -> Result<Vec<LayerTerms<'a>>, DomainError> {
    if stack.is_empty() {
        return Err(empty_stack_error());
    }
    let mut terms = Vec::with_capacity(stack.layers.len());
    for (index, layer) in stack.layers.iter().enumerate() {
        let head = layer_head(index + 1, layer);
        let fill = fills
            .iter()
            .find(|fill| fill.id == layer.fill_id)
            .ok_or_else(|| {
                DomainError::new(format!(
                    "{head}: no fill record in the catalog carries that id."
                ))
            })?;
        if layer.depth_m.is_nan() || layer.depth_m <= 0.0 {
            return Err(DomainError::new(format!(
                "{head}: depthM must be positive, got {}.",
                layer.depth_m
            )));
        }
        if layer.thermal_multiplier.is_nan() || layer.thermal_multiplier <= 0.0 {
            return Err(DomainError::new(format!(
                "{head}: thermalMultiplier must be positive, got {}.",
                layer.thermal_multiplier
            )));
        }
        if layer.pressure_multiplier.is_nan() || layer.pressure_multiplier <= 0.0 {
            return Err(DomainError::new(format!(
                "{head}: pressureMultiplier must be positive, got {}.",
                layer.pressure_multiplier
            )));
        }
        if fill.thermal.is_none() {
            return Err(DomainError::new(format!(
                "{head}: Fill thermal correlation is missing."
            )));
        }
        if fill.pressure.is_none() {
            return Err(DomainError::new(format!(
                "{head}: Fill pressure correlation is missing."
            )));
        }
        terms.push(LayerTerms {
            fill,
            depth_m: layer.depth_m,
            thermal_multiplier: layer.thermal_multiplier,
            pressure_multiplier: layer.pressure_multiplier,
        });
    }
    Ok(terms)
}

/// One layer's results, in physical order.
///
/// `position` is 1-based with the **top** layer first — the order the stack was declared in.
/// `cumulative_*` are the running totals through this layer inclusive, so the last layer's
/// cumulative value is the stack total.
#[derive(Clone, Debug, PartialEq)]
pub struct FillLayerResult {
    pub position: usize,
    pub fill_id: String,
    pub depth_m: f64,
    pub thermal_multiplier: f64,
    pub pressure_multiplier: f64,
    pub effective_thermal_multiplier: f64,
    pub effective_pressure_multiplier: f64,
    /// The thermal characteristic the layer's contribution was computed from.
    pub thermal: ZoneCorrelation,
    /// The pressure characteristic the layer's drop was computed from.
    pub pressure: FillPressureCorrelation,
    /// The limits the layer was checked against.
    pub limits: FillLimits,
    pub volumetric_air_flow_m3_s: f64,
    pub dry_air_mass_flow_kg_s: f64,
    pub water_mass_flow_kg_s: f64,
    pub water_loading_kg_m2_s: f64,
    pub dry_air_loading_kg_m2_s: f64,
    pub fill_velocity_ms: f64,
    pub pressure_drop_pa: f64,
    pub merkel_number: f64,
    pub cumulative_pressure_drop_pa: f64,
    pub cumulative_merkel_number: f64,
}

impl FillLayerResult {
    /// The layer's name in a result: `<fillId>@<depthM>`.
    pub fn label(&self) -> String {
        format!("{}@{}", self.fill_id, self.depth_m)
    }
}

/// Where a stack's layers stand against their own fills' limits at one operating point.
///
/// The loadings are the **stack's**: layers sit in series in both the air path and the water
/// path, so every layer sees the same water loading and the same dry-air loading. The water
/// temperature and the water-quality class are checked only when the run declares them (`None`
/// means the run is not making that claim). Every failure names its layer; an empty vector
/// means no layer is outside the limits it echoes.
pub fn check_stack_operating_envelope(
    terms: &[LayerTerms<'_>],
    water_loading_kg_m2_s: f64,
    dry_air_loading_kg_m2_s: f64,
    hot_water_c: Option<f64>,
    water_quality_class: Option<&str>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for (index, term) in terms.iter().enumerate() {
        let head = format!(
            "Layer {} ({})",
            index + 1,
            FillLayer {
                fill_id: term.fill.id.clone(),
                depth_m: term.depth_m,
                thermal_multiplier: term.thermal_multiplier,
                pressure_multiplier: term.pressure_multiplier,
            }
            .label()
        );
        for failure in envelope_failures(
            term.fill,
            water_loading_kg_m2_s,
            dry_air_loading_kg_m2_s,
            hot_water_c,
            water_quality_class,
        ) {
            failures.push(format!("{head}: {failure}"));
        }
    }
    failures
}
