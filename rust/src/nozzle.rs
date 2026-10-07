//! Nozzle flow and arrangement — port of `src/core/nozzle.js`, line for line.
//!
//! The reference's `waterDensityKgM3 = 997` default and its
//! `nozzle.referenceWaterDensityKgM3 ?? 997` fallback are [`NOZZLE_WATER_DENSITY_KG_M3`]
//! here; the count stays an `f64` because the reference never truncates `Math.ceil` to an
//! integer, and the sort is the reference's two-key comparator
//! (`excessFlowPct`, then `count`), stable like `Array.prototype.sort`.

use crate::numeric::{assert_positive, DomainError};

/// The reference's prototype water density when neither the call nor the nozzle record
/// supplies one.
pub const NOZZLE_WATER_DENSITY_KG_M3: f64 = 997.0;

/// Port of `nozzleFlowM3S`.
pub fn nozzle_flow_m3_s(
    discharge_coefficient: f64,
    orifice_diameter_m: f64,
    pressure_drop_pa: f64,
    water_density_kg_m3: f64,
) -> Result<f64, DomainError> {
    assert_positive(discharge_coefficient, "dischargeCoefficient")?;
    assert_positive(orifice_diameter_m, "orificeDiameterM")?;
    assert_positive(pressure_drop_pa, "pressureDropPa")?;
    let area_m2 = std::f64::consts::PI * orifice_diameter_m.powi(2) / 4.0;
    Ok(discharge_coefficient * area_m2 * f64::sqrt(2.0 * pressure_drop_pa / water_density_kg_m3))
}

/// A nozzle record — the fields of `sampleCatalog.nozzles` the selector reads.
#[derive(Clone, Debug, PartialEq)]
pub struct NozzleRecord {
    pub id: String,
    pub name: String,
    pub discharge_coefficient: f64,
    pub orifice_diameter_m: f64,
    /// `?? 997` at the call site.
    pub reference_water_density_kg_m3: Option<f64>,
}

/// The object `selectNozzleArrangement` returns, field for field.
#[derive(Clone, Debug, PartialEq)]
pub struct NozzleArrangement {
    pub nozzle_id: String,
    pub nozzle_name: String,
    pub count: f64,
    pub flow_per_nozzle_m3_s: f64,
    pub actual_total_flow_m3_s: f64,
    pub excess_flow_pct: f64,
    pub valid_count: bool,
}

/// Inputs for [`select_nozzle_arrangement`] — the port of the JavaScript
/// `selectNozzleArrangement({...})` call.
#[derive(Clone, Copy, Debug)]
pub struct NozzleArrangementInput<'a> {
    pub total_volumetric_water_flow_m3_s: f64,
    pub nozzles: &'a [NozzleRecord],
    pub pressure_drop_pa: f64,
    /// Default `[4, 4000]`.
    pub target_count_range: [f64; 2],
}

impl<'a> NozzleArrangementInput<'a> {
    /// The three physically required inputs; the count range takes the reference default.
    pub fn new(
        total_volumetric_water_flow_m3_s: f64,
        nozzles: &'a [NozzleRecord],
        pressure_drop_pa: f64,
    ) -> Self {
        Self {
            total_volumetric_water_flow_m3_s,
            nozzles,
            pressure_drop_pa,
            target_count_range: [4.0, 4000.0],
        }
    }

    /// The reference's `targetCountRange` argument.
    pub fn with_target_count_range(mut self, target_count_range: [f64; 2]) -> Self {
        self.target_count_range = target_count_range;
        self
    }
}

/// The reference's comparator: `a.excessFlowPct - b.excessFlowPct || a.count - b.count`.
/// A zero difference and a NaN difference both fall through to the count, exactly as the
/// JavaScript `||` does.
fn compare_arrangements(a: &NozzleArrangement, b: &NozzleArrangement) -> std::cmp::Ordering {
    let difference = a.excess_flow_pct - b.excess_flow_pct;
    if difference == 0.0 || difference.is_nan() {
        a.count
            .partial_cmp(&b.count)
            .unwrap_or(std::cmp::Ordering::Equal)
    } else {
        difference
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Port of `selectNozzleArrangement`: one candidate per nozzle, filtered to the valid count
/// range, sorted by excess flow. An empty result is refused, as the reference refuses it.
pub fn select_nozzle_arrangement(
    input: &NozzleArrangementInput<'_>,
) -> Result<Vec<NozzleArrangement>, DomainError> {
    let mut candidates: Vec<NozzleArrangement> = input
        .nozzles
        .iter()
        .map(|nozzle| {
            let flow_per_nozzle_m3_s = nozzle_flow_m3_s(
                nozzle.discharge_coefficient,
                nozzle.orifice_diameter_m,
                input.pressure_drop_pa,
                nozzle
                    .reference_water_density_kg_m3
                    .unwrap_or(NOZZLE_WATER_DENSITY_KG_M3),
            )?;
            let count = f64::ceil(input.total_volumetric_water_flow_m3_s / flow_per_nozzle_m3_s);
            let actual_total_flow_m3_s = count * flow_per_nozzle_m3_s;
            Ok(NozzleArrangement {
                nozzle_id: nozzle.id.clone(),
                nozzle_name: nozzle.name.clone(),
                count,
                flow_per_nozzle_m3_s,
                actual_total_flow_m3_s,
                excess_flow_pct: 100.0
                    * (actual_total_flow_m3_s - input.total_volumetric_water_flow_m3_s)
                    / input.total_volumetric_water_flow_m3_s,
                valid_count: count >= input.target_count_range[0]
                    && count <= input.target_count_range[1],
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    candidates.retain(|candidate| candidate.valid_count);
    candidates.sort_by(compare_arrangements);

    if candidates.is_empty() {
        return Err(DomainError::new(
            "No sample nozzle arrangement fits the requested count range.",
        ));
    }
    Ok(candidates)
}
