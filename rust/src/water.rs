//! Water properties — port of the part of `src/core/water.js` the slice needs.

/// Port of `waterSpecificHeatKJkgK`.
///
/// An engineering approximation (the reference says so too; IAPWS/TEOS-10 would be
/// required for contractual seawater work), floored at 3.7 kJ/(kg·K).
pub fn water_specific_heat_kj_kg_k(temperature_c: f64, salinity_g_kg: f64) -> f64 {
    let temperature_correction = 0.00035 * (temperature_c - 30.0);
    let salinity_correction = 0.0042 * salinity_g_kg.max(0.0);
    3.7_f64.max(4.181 + temperature_correction - salinity_correction)
}
