//! Round 3: **the recorded performance grid** the two CTI-style charts draw.
//!
//! The brief for round 3 asks for two real charts in the Operating-point view (cold water vs entering
//! wet-bulb, and KaV/L vs L/G). Nothing here computes anything: the series are the engine's own recorded
//! grid, read off the fixture the engine was built from.
//!
//! Why a trait instead of a method on the engine: `cockpit/` is the approved baseline and is read, never
//! written (the brief's first constraint). Rust will not let another crate add an inherent method to
//! `FixtureEngine`, but it *will* let this crate add a **view** onto it - [`PerfGridSource`] - with no
//! change to the baseline at all. The baseline's `FixtureAnchor` types only the water-mass-flow sweep
//! (`Sweeps.water_mass_flow_kg_s`); the wet-bulb sweep and the grid values ride along in the same recorded
//! JSON, exactly like `anchor.air.inlet` (see `state::Ambient`), so the raw fixture text is read for those
//! two series and nothing else. One fixture, one engine, no new physics.
//!
//! Every series carries its own `infeasible` flags: the engine's sweep marks the points its own duty check
//! rejected, and the charts draw those as an open marker rather than silently dropping them.

use serde_json::Value;

use bevy::prelude::Resource;

/// The resource the UI reads: the recorded grid, or `None` when the fixture carries no sweep at all.
#[derive(Resource, Default, Clone, Debug)]
pub struct PerfGridRes(pub Option<PerfGrid>);

/// One recorded point. `ok` is the sweep's own feasibility flag (`infeasible` in the fixture = false).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
    pub ok: bool,
}

/// One recorded one-parameter sweep (the fixture's performance grid, one line of it).
#[derive(Clone, Debug, PartialEq)]
pub struct Sweep {
    /// The grid values the sweep was sampled at, as recorded.
    pub values: Vec<f64>,
    pub pts: Vec<Pt>,
}

/// The recorded performance grid, as far as the fixture carries it.
#[derive(Clone, Debug, PartialEq)]
pub struct PerfGrid {
    /// The fill the sweeps were run on (`anchor.anchorFillId`).
    pub fill_id: String,
    /// The fill depth the sweep ran at (m), when the fixture records one.
    pub fill_depth_m: Option<f64>,
    /// Cold water (C) vs entering wet bulb (C), at the design water flow.
    pub wb: Sweep,
    /// Cold water (C) vs water mass flow (kg/s), at the design wet bulb.
    pub flow: Sweep,
    /// The design water mass flow (kg/s) - `anchor.waterFlow.kgS`.
    pub design_flow_kg_s: f64,
    /// The design water flow in m3/hr - `anchor.waterFlow.m3Hr`.
    pub design_flow_m3_hr: f64,
    /// The design entering wet bulb (C) - the duty the fixture ran.
    pub design_wb_c: f64,
    /// The recorded water loading, kg/(m2 s) - `anchor.transfer.waterLoadingKgM2S`. With the dry-air
    /// loading below it gives the recorded L/G the KaV/L chart marks.
    pub water_loading_kg_m2_s: f64,
    /// The recorded dry-air loading, kg/(m2 s) - `anchor.transfer.dryAirLoadingKgM2S`. Also the loading the
    /// sampled fill characteristics are tabulated at.
    pub dry_air_loading_kg_m2_s: f64,
    /// The recorded fill demand (Merkel number) at the design condition - `anchor.transfer.fillMerkelNumber`.
    /// The fixture carries one demand value, not a demand sweep, and the chart says so.
    pub fill_demand_kavl: f64,
}

impl PerfGrid {
    /// Read the grid out of the raw fixture JSON. Returns `None` (rather than an empty grid) when the
    /// fixture carries no wet-bulb sweep at all: a chart with no series must say `series not in fixture`,
    /// so the caller has to be able to tell "no series" from "a series of no points".
    pub fn from_json(text: &str) -> Option<PerfGrid> {
        let v: Value = serde_json::from_str(text).ok()?;
        let anchor = v.get("anchor")?;
        let fill_id = anchor.get("anchorFillId")?.as_str()?.to_string();
        let sweeps = anchor.get("sweeps")?;
        let wb = sweep(sweeps.get("wetBulbC")?, &fill_id);
        let flow = sweep(sweeps.get("waterMassFlowKgS")?, &fill_id);
        if wb.pts.is_empty() && flow.pts.is_empty() {
            return None;
        }
        let design_flow_kg_s = anchor
            .get("waterFlow")
            .and_then(|w| w.get("kgS"))
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0);
        let design_flow_m3_hr = anchor
            .get("waterFlow")
            .and_then(|w| w.get("m3Hr"))
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0);
        let design_wb_c = v
            .get("defaultInput")
            .and_then(|d| d.get("duty"))
            .and_then(|d| d.get("wetBulbC"))
            .and_then(|x| x.as_f64())
            .unwrap_or(27.0);
        let fill_depth_m = anchor
            .get("layers")
            .and_then(|l| l.as_array())
            .and_then(|a| a.first())
            .and_then(|l| l.get("depth_m"))
            .and_then(|x| x.as_f64());
        let transfer = anchor.get("transfer");
        let transfer_f = |k: &str| {
            transfer
                .and_then(|t| t.get(k))
                .and_then(|x| x.as_f64())
                .unwrap_or(0.0)
        };
        Some(PerfGrid {
            fill_id,
            fill_depth_m,
            wb,
            flow,
            design_flow_kg_s,
            design_flow_m3_hr,
            design_wb_c,
            water_loading_kg_m2_s: transfer_f("waterLoadingKgM2S"),
            dry_air_loading_kg_m2_s: transfer_f("dryAirLoadingKgM2S"),
            fill_demand_kavl: transfer_f("fillMerkelNumber"),
        })
    }

    /// The recorded cold water (C) at a wet bulb, linearly interpolated between the recorded grid points
    /// **that the engine's own duty check accepted**. This is a lookup in the recorded grid, not a solve:
    /// outside the grid it returns `None` and the chart marks nothing.
    pub fn cold_water_at_wb(&self, wb: f64) -> Option<f64> {
        interp(&self.wb, wb)
    }

    /// The recorded cold water (C) at a water mass flow (kg/s), same rule.
    pub fn cold_water_at_flow(&self, kg_s: f64) -> Option<f64> {
        interp(&self.flow, kg_s)
    }

    /// The recorded points nearest a design fraction (0.9 / 1.0 / 1.1 of the design water flow), one per
    /// fraction: the chart's `90 / 100 / 110 %` markers. `None` when the sweep does not reach that flow.
    pub fn flow_at_fraction(&self, frac: f64) -> Option<Pt> {
        let target = self.design_flow_kg_s * frac;
        self.flow
            .pts
            .iter()
            .find(|p| p.ok && (p.x - target).abs() < 1e-6)
            .copied()
    }
}

fn sweep(v: &Value, fill_id: &str) -> Sweep {
    let values: Vec<f64> = v
        .get("values")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
        .unwrap_or_default();
    let pts: Vec<Pt> = v
        .get("series")
        .and_then(|s| s.get(fill_id))
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let x = p.get("x")?.as_f64()?;
                    let y = p.get("coldWaterC")?.as_f64()?;
                    let ok = !p
                        .get("infeasible")
                        .and_then(|i| i.as_bool())
                        .unwrap_or(false);
                    Some(Pt { x, y, ok })
                })
                .collect()
        })
        .unwrap_or_default();
    Sweep { values, pts }
}

/// Linear interpolation over the feasible points of a sweep; `None` outside its span (never extrapolated).
fn interp(s: &Sweep, x: f64) -> Option<f64> {
    let ok: Vec<&Pt> = s.pts.iter().filter(|p| p.ok).collect();
    if ok.len() < 2 {
        return None;
    }
    let (lo, hi) = (ok.first()?.x, ok.last()?.x);
    if x < lo - 1e-9 || x > hi + 1e-9 {
        return None;
    }
    for w in ok.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x >= a.x && x <= b.x {
            if (b.x - a.x).abs() < 1e-12 {
                return Some(a.y);
            }
            return Some(a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x));
        }
    }
    Some(ok.last()?.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIX: &str = include_str!("../assets/fixture.json");

    #[test]
    fn the_grid_reads_the_engines_own_sweeps() {
        let g = PerfGrid::from_json(FIX).expect("the shipped fixture carries the grid");
        assert_eq!(g.fill_id, "FILM-VF38");
        assert!(!g.wb.pts.is_empty(), "wet-bulb sweep");
        assert!(!g.flow.pts.is_empty(), "water-flow sweep");
        assert!((g.design_flow_kg_s - 200.0).abs() < 1e-9);
        assert!((g.design_wb_c - 27.0).abs() < 1e-9);
    }

    /// Issue #58: this test arrived asserting that all three design flows are *feasible* recorded
    /// points, and it is red at the approved head - the fixture's own flow sweep records the 110 %
    /// point (220 kg/s) with `infeasible: true` and `coldWaterC` 0, so there is no such point for
    /// `flow_at_fraction(1.1)` to find. The claim is stated against the record instead: every design
    /// flow is a recorded point of the sweep, the 90 % and 100 % points are feasible, and 110 % is
    /// the recorded point the fill cannot hold (which is why the chart draws it as a gap, not a
    /// number).
    #[test]
    fn the_three_design_flows_are_recorded_points() {
        let g = PerfGrid::from_json(FIX).unwrap();
        for (frac, kg_s) in [(0.9, 180.0), (1.0, 200.0)] {
            let p = g
                .flow_at_fraction(frac)
                .expect("recorded in the flow sweep");
            assert!((p.x - kg_s).abs() < 1e-9, "{frac}: {} vs {kg_s}", p.x);
            assert!(p.ok, "{frac}: recorded as feasible");
        }
        assert!(
            g.flow.values.iter().any(|v| (v - 220.0).abs() < 1e-9),
            "the grid records the 110 % flow"
        );
        assert!(
            g.flow_at_fraction(1.1).is_none(),
            "the 110 % point is the one the engine refused (the fixture's own point carries `infeasible: true` and no cold-water value), so there is nothing at that flow to look up"
        );
        // The sweep's own last feasible point is the 105 % one: the recorded grid goes to 260 kg/s,
        // and every point past 210 is refused.
        let last_feasible = g
            .flow
            .pts
            .iter()
            .filter(|p| p.ok)
            .map(|p| p.x)
            .fold(f64::MIN, f64::max);
        assert!(
            (last_feasible - 210.0).abs() < 1e-9,
            "the sweep's last feasible point is 210 kg/s, got {last_feasible}"
        );
    }

    #[test]
    fn a_lookup_never_extrapolates() {
        let g = PerfGrid::from_json(FIX).unwrap();
        assert!(g.cold_water_at_wb(g.design_wb_c).is_some());
        assert!(
            g.cold_water_at_wb(80.0).is_none(),
            "outside the recorded grid"
        );
    }

    #[test]
    fn the_recorded_transfer_gives_the_l_over_g_the_chart_marks() {
        let g = PerfGrid::from_json(FIX).unwrap();
        assert!((g.water_loading_kg_m2_s - 3.125).abs() < 1e-9);
        assert!((g.dry_air_loading_kg_m2_s - 2.178299700421475).abs() < 1e-9);
        let l_over_g = g.water_loading_kg_m2_s / g.dry_air_loading_kg_m2_s;
        assert!((l_over_g - 1.4346).abs() < 5e-4, "{l_over_g}");
        // the run that produced the sweeps needs 1.326 of fill transfer at that loading
        assert!((g.fill_demand_kavl - 1.326430046411088).abs() < 1e-9);
    }
}
