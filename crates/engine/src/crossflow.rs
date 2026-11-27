//! Simplised finite-volume crossflow grid — port of `src/core/crossflow.js`, line for line.
//!
//! The explicit cell scheme is the reference's: water flows down the columns, air flows across
//! the rows, each cell applies a local enthalpy-potential effectiveness relation, the coarse
//! grid is Richardson-extrapolated against a doubled grid, and the water-side heat capacity is
//! iterated to the mean bulk temperature. The JavaScript engine is the specification; nothing
//! here re-derives a value, and a reference quirk found here is a finding to report.
//!
//! One function of `src/core/psychrometrics.js` is ported in this file, because the
//! psychrometrics slice explicitly left it out and this lane's file fence keeps that reviewed
//! module byte-identical: [`saturated_temperature_from_enthalpy`] (`saturatedTemperatureFrom
//! Enthalpy`), the inversion `crossflow.js` and the selection slice both need. It uses the
//! already-ported [`solve_bracketed_root`] machinery, so no new numeric primitive is introduced.

use crate::numeric::{assert_positive, solve_bracketed_root, BracketedRootOptions, DomainError};
use crate::psychrometrics::{
    psychrometric_state, saturated_air_enthalpy_kj_kg_dry_air, saturation_humidity_ratio,
    PsychrometricOptions, PsychrometricState, PsychrometricStateInput,
};
use crate::water::water_specific_heat_kj_kg_k;

/// `model` string `solveCrossflowGrid` returns.
pub const CROSSFLOW_MODEL: &str = "simplified finite-volume crossflow";

/// Port of the reference's `Math.max(a, b)`: a NaN operand wins, where `f64::max` would
/// return the other operand and quietly swallow it.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// Port of the reference's `Math.min(a, b)`, NaN-propagating for the same reason.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// Port of `saturatedTemperatureFromEnthalpy` from `src/core/psychrometrics.js`: the
/// temperature at which saturated air carries the given enthalpy, by bracketed root.
///
/// The JavaScript `options` argument is not ported (every call site in this slice uses the
/// default enhancement-factor-on option set), and the default bounds stay `[-50, 100]` when
/// the caller passes none.
pub fn saturated_temperature_from_enthalpy(
    enthalpy_kj_kg_dry_air: f64,
    pressure_pa: f64,
    bounds: [f64; 2],
) -> Result<f64, DomainError> {
    let options = PsychrometricOptions::default();
    let residual = |temperature_c: f64| -> Result<f64, DomainError> {
        Ok(
            saturated_air_enthalpy_kj_kg_dry_air(temperature_c, pressure_pa, options)?
                - enthalpy_kj_kg_dry_air,
        )
    };
    solve_bracketed_root(
        residual,
        bounds[0],
        bounds[1],
        BracketedRootOptions {
            tolerance: 1e-7,
            ..BracketedRootOptions::default()
        },
    )
}

/// `sweepCrossflowGrid` — one explicit sweep at a fixed cell count and water specific heat.
struct SweepInput {
    hot_water_c: f64,
    inlet_air_enthalpy_kj_kg_dry_air: f64,
    pressure_pa: f64,
    water_mass_flow_kg_s: f64,
    dry_air_mass_flow_kg_s: f64,
    available_merkel_number: f64,
    cp_water: f64,
    air_cells: usize,
    water_cells: usize,
}

struct Sweep {
    water: Vec<Vec<f64>>,
    air: Vec<Vec<f64>>,
    cold_water_c: f64,
    outlet_enthalpy: f64,
}

fn sweep_crossflow_grid(input: &SweepInput) -> Result<Sweep, DomainError> {
    let options = PsychrometricOptions::default();
    let water_by_column = input.water_mass_flow_kg_s / input.air_cells as f64;
    let air_by_row = input.dry_air_mass_flow_kg_s / input.water_cells as f64;
    let total_kav = input.available_merkel_number * input.water_mass_flow_kg_s;
    let kav_cell = total_kav / (input.air_cells * input.water_cells) as f64;

    let mut water = vec![vec![f64::NAN; input.air_cells]; input.water_cells + 1];
    let mut air = vec![vec![f64::NAN; input.air_cells + 1]; input.water_cells];
    for column in water[0].iter_mut() {
        *column = input.hot_water_c;
    }
    for row in air.iter_mut() {
        row[0] = input.inlet_air_enthalpy_kj_kg_dry_air;
    }

    // The reference's sweep also accumulates `totalHeatKW`; its caller never reads it, so the
    // port does not carry the dead field.
    for y in 0..input.water_cells {
        for x in 0..input.air_cells {
            let water_in_c = water[y][x];
            let air_in_h = air[y][x];
            let saturated_h =
                saturated_air_enthalpy_kj_kg_dry_air(water_in_c, input.pressure_pa, options)?;
            let potential = js_max(0.0, saturated_h - air_in_h);
            let cell_effectiveness = 1.0 - f64::exp(-kav_cell / air_by_row);
            let air_delta_h = cell_effectiveness * potential;
            let heat_kw = air_by_row * air_delta_h;
            water[y + 1][x] = water_in_c - heat_kw / (water_by_column * input.cp_water);
            air[y][x + 1] = air_in_h + heat_kw / air_by_row;
        }
    }

    let last_row = water.last().expect("water has water_cells + 1 rows");
    let cold_water_c = last_row.iter().sum::<f64>() / input.air_cells as f64;
    let outlet_enthalpy =
        air.iter().map(|row| row[input.air_cells]).sum::<f64>() / input.water_cells as f64;
    Ok(Sweep {
        water,
        air,
        cold_water_c,
        outlet_enthalpy,
    })
}

/// Inputs for [`solve_crossflow_grid`] — the port of the JavaScript
/// `solveCrossflowGrid({...})` call, with the reference's default arguments on
/// [`CrossflowGridInput::new`].
#[derive(Clone, Copy, Debug)]
pub struct CrossflowGridInput {
    pub hot_water_c: f64,
    pub dry_bulb_c: f64,
    pub wet_bulb_c: f64,
    /// Default 101 325 Pa.
    pub pressure_pa: f64,
    pub water_mass_flow_kg_s: f64,
    pub dry_air_mass_flow_kg_s: f64,
    pub available_merkel_number: f64,
    /// Default 0 g/kg.
    pub salinity_g_kg: f64,
    /// Default 18.
    pub air_cells: usize,
    /// Default 18.
    pub water_cells: usize,
    /// Default true.
    pub richardson: bool,
    /// Default 3.
    pub cp_iterations: usize,
}

impl CrossflowGridInput {
    /// The seven physically required inputs; everything else takes the reference default.
    pub fn new(
        hot_water_c: f64,
        dry_bulb_c: f64,
        wet_bulb_c: f64,
        water_mass_flow_kg_s: f64,
        dry_air_mass_flow_kg_s: f64,
        available_merkel_number: f64,
    ) -> Self {
        Self {
            hot_water_c,
            dry_bulb_c,
            wet_bulb_c,
            pressure_pa: 101_325.0,
            water_mass_flow_kg_s,
            dry_air_mass_flow_kg_s,
            available_merkel_number,
            salinity_g_kg: 0.0,
            air_cells: 18,
            water_cells: 18,
            richardson: true,
            cp_iterations: 3,
        }
    }

    /// The reference's `pressurePa` argument.
    pub fn with_pressure(mut self, pressure_pa: f64) -> Self {
        self.pressure_pa = pressure_pa;
        self
    }

    /// The reference's `salinityGKg` argument, default 0.
    pub fn with_salinity(mut self, salinity_g_kg: f64) -> Self {
        self.salinity_g_kg = salinity_g_kg;
        self
    }

    /// The reference's `airCells` / `waterCells` arguments, both defaulting to 18.
    pub fn with_cells(mut self, air_cells: usize, water_cells: usize) -> Self {
        self.air_cells = air_cells;
        self.water_cells = water_cells;
        self
    }

    /// The reference's `richardson` argument, default true.
    pub fn with_richardson(mut self, richardson: bool) -> Self {
        self.richardson = richardson;
        self
    }

    /// The reference's `cpIterations` argument, default 3.
    pub fn with_cp_iterations(mut self, cp_iterations: usize) -> Self {
        self.cp_iterations = cp_iterations;
        self
    }
}

/// The `gridConvergence` object `solveCrossflowGrid` returns.
#[derive(Clone, Copy, Debug)]
pub struct CrossflowGridConvergence {
    pub coarse_cells: [usize; 2],
    pub coarse_cold_water_c: f64,
    pub fine_cells: Option<[usize; 2]>,
    pub fine_cold_water_c: Option<f64>,
    pub richardson_cold_water_c: f64,
    pub estimated_discretization_error_c: Option<f64>,
    pub error_estimate_meaning: &'static str,
    pub note: &'static str,
}

/// The `outletAirState` object the crossflow solve returns (and the one
/// `estimateOutletAirState` builds in the selection slice).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutletAirState {
    pub dry_bulb_c: f64,
    pub wet_bulb_c: f64,
    pub relative_humidity: f64,
    pub enthalpy_kj_kg_dry_air: f64,
    pub humidity_ratio: f64,
    pub pressure_pa: f64,
}

/// The object `solveCrossflowGrid` returns, field for field.
#[derive(Clone, Debug)]
pub struct CrossflowGridResult {
    pub cold_water_c: f64,
    pub range_c: f64,
    pub approach_c: f64,
    pub heat_transfer_kw: f64,
    pub water_energy_kw: f64,
    pub cp_water_kj_kg_k: f64,
    pub grid_convergence: CrossflowGridConvergence,
    pub outlet_air_state: OutletAirState,
    pub water_temperature_grid_c: Vec<Vec<f64>>,
    pub air_enthalpy_grid_kj_kg_dry_air: Vec<Vec<f64>>,
    pub inlet_air_state: PsychrometricState,
}

impl CrossflowGridResult {
    /// The reference's `model` field.
    pub fn model(&self) -> &'static str {
        CROSSFLOW_MODEL
    }
}

/// One `solveAt` call: a sweep at `nx × ny` with the water-specific-heat iteration.
fn solve_at(
    input: &CrossflowGridInput,
    inlet_air: &PsychrometricState,
    seed_cp: Option<f64>,
    nx: usize,
    ny: usize,
) -> Result<(Sweep, f64), DomainError> {
    let mut cp_water = seed_cp
        .unwrap_or_else(|| water_specific_heat_kj_kg_k(input.hot_water_c, input.salinity_g_kg));
    let iterations = if seed_cp.is_none() {
        input.cp_iterations
    } else {
        1
    };
    let mut sweep = None;
    for _ in 0..iterations {
        let current = sweep_crossflow_grid(&SweepInput {
            hot_water_c: input.hot_water_c,
            inlet_air_enthalpy_kj_kg_dry_air: inlet_air.enthalpy_kj_kg_dry_air,
            pressure_pa: input.pressure_pa,
            water_mass_flow_kg_s: input.water_mass_flow_kg_s,
            dry_air_mass_flow_kg_s: input.dry_air_mass_flow_kg_s,
            available_merkel_number: input.available_merkel_number,
            cp_water,
            air_cells: nx,
            water_cells: ny,
        })?;
        let next_cp = water_specific_heat_kj_kg_k(
            (input.hot_water_c + current.cold_water_c) / 2.0,
            input.salinity_g_kg,
        );
        if (next_cp - cp_water).abs() < 1e-9 {
            cp_water = next_cp;
            sweep = Some(current);
            break;
        }
        cp_water = next_cp;
        sweep = Some(current);
    }
    Ok((sweep.expect("at least one iteration"), cp_water))
}

/// Port of `solveCrossflowGrid`.
pub fn solve_crossflow_grid(
    input: &CrossflowGridInput,
) -> Result<CrossflowGridResult, DomainError> {
    assert_positive(input.water_mass_flow_kg_s, "waterMassFlowKgS")?;
    assert_positive(input.dry_air_mass_flow_kg_s, "dryAirMassFlowKgS")?;
    assert_positive(input.available_merkel_number, "availableMerkelNumber")?;
    if input.air_cells < 2 || input.water_cells < 2 {
        return Err(DomainError::new(
            "Crossflow grid requires at least 2 × 2 cells.",
        ));
    }

    let inlet_air = psychrometric_state(
        PsychrometricStateInput::from_wet_bulb(input.dry_bulb_c, input.wet_bulb_c)
            .with_pressure(input.pressure_pa),
    )?;

    // The water cp is a property of the mean bulk temperature, which is not known until the
    // outlet temperature is known; iterate (or take the caller's converged seed).
    let (coarse, coarse_cp) =
        solve_at(input, &inlet_air, None, input.air_cells, input.water_cells)?;
    let fine = if input.richardson {
        Some(solve_at(
            input,
            &inlet_air,
            Some(coarse_cp),
            input.air_cells * 2,
            input.water_cells * 2,
        )?)
    } else {
        None
    };

    let extrapolated_cold_water_c = match &fine {
        Some((fine_sweep, _)) => 2.0 * fine_sweep.cold_water_c - coarse.cold_water_c,
        None => coarse.cold_water_c,
    };
    let extrapolated_outlet_enthalpy = match &fine {
        Some((fine_sweep, _)) => 2.0 * fine_sweep.outlet_enthalpy - coarse.outlet_enthalpy,
        None => coarse.outlet_enthalpy,
    };

    let (chosen, cp_water) = match &fine {
        Some((fine_sweep, fine_cp)) => (fine_sweep, *fine_cp),
        None => (&coarse, coarse_cp),
    };
    let cold_water_c = extrapolated_cold_water_c;
    let outlet_enthalpy = extrapolated_outlet_enthalpy;

    let outlet_dry_bulb_c = saturated_temperature_from_enthalpy(
        outlet_enthalpy,
        input.pressure_pa,
        [-50.0, js_min(120.0, input.hot_water_c + 40.0)],
    )?;
    let outlet_humidity_ratio = saturation_humidity_ratio(
        outlet_dry_bulb_c,
        input.pressure_pa,
        PsychrometricOptions::default(),
    )?;
    let heat_transfer_kw =
        input.water_mass_flow_kg_s * cp_water * (input.hot_water_c - cold_water_c);

    let error_estimate_meaning = if fine.is_some() {
        "Conservative upper bound: the estimated error of the raw fine grid, not of the reported extrapolated value."
    } else {
        "Richardson extrapolation disabled; the reported value carries the full first-order grid error."
    };
    let note = if fine.is_some() {
        "Cold-water temperature is Richardson-extrapolated from the coarse and doubled grids."
    } else {
        "Richardson extrapolation disabled; the reported value carries the raw first-order grid error."
    };

    Ok(CrossflowGridResult {
        cold_water_c,
        range_c: input.hot_water_c - cold_water_c,
        approach_c: cold_water_c - input.wet_bulb_c,
        heat_transfer_kw,
        water_energy_kw: heat_transfer_kw,
        cp_water_kj_kg_k: cp_water,
        grid_convergence: CrossflowGridConvergence {
            coarse_cells: [input.air_cells, input.water_cells],
            coarse_cold_water_c: coarse.cold_water_c,
            fine_cells: fine
                .as_ref()
                .map(|_| [input.air_cells * 2, input.water_cells * 2]),
            fine_cold_water_c: fine.as_ref().map(|(sweep, _)| sweep.cold_water_c),
            richardson_cold_water_c: extrapolated_cold_water_c,
            estimated_discretization_error_c: fine
                .as_ref()
                .map(|(sweep, _)| (sweep.cold_water_c - extrapolated_cold_water_c).abs()),
            error_estimate_meaning,
            note,
        },
        outlet_air_state: OutletAirState {
            dry_bulb_c: outlet_dry_bulb_c,
            wet_bulb_c: outlet_dry_bulb_c,
            relative_humidity: 1.0,
            enthalpy_kj_kg_dry_air: outlet_enthalpy,
            humidity_ratio: outlet_humidity_ratio,
            pressure_pa: input.pressure_pa,
        },
        water_temperature_grid_c: chosen.water.clone(),
        air_enthalpy_grid_kj_kg_dry_air: chosen.air.clone(),
        inlet_air_state: inlet_air,
    })
}

/// Inputs for [`crossflow_convergence_study`]: the reference's `{ baseCells, safetyFactor }`
/// plus the grid inputs it spreads into every solve.
#[derive(Clone, Copy, Debug)]
pub struct CrossflowStudyInput {
    /// Default 12.
    pub base_cells: usize,
    /// Default 1.25.
    pub safety_factor: f64,
    pub grid: CrossflowGridInput,
}

impl CrossflowStudyInput {
    /// The reference's defaults; `grid` carries the case's inputs.
    pub fn new(grid: CrossflowGridInput) -> Self {
        Self {
            base_cells: 12,
            safety_factor: 1.25,
            grid,
        }
    }

    /// The reference's `baseCells` argument.
    pub fn with_base_cells(mut self, base_cells: usize) -> Self {
        self.base_cells = base_cells;
        self
    }

    /// The reference's `safetyFactor` argument.
    pub fn with_safety_factor(mut self, safety_factor: f64) -> Self {
        self.safety_factor = safety_factor;
        self
    }
}

/// The object `crossflowConvergenceStudy` returns.
#[derive(Clone, Copy, Debug)]
pub struct CrossflowConvergenceStudy {
    pub cells: [usize; 3],
    pub cold_water_c: [f64; 3],
    pub observed_order: Option<f64>,
    pub extrapolated_cold_water_c: f64,
    pub fine_grid_error_estimate_c: f64,
    pub grid_convergence_index_pct: f64,
    pub interpretation: &'static str,
}

/// Port of `crossflowConvergenceStudy` — the three-grid (n, 2n, 4n) convergence study the
/// selector deliberately does not call.
pub fn crossflow_convergence_study(
    input: &CrossflowStudyInput,
) -> Result<CrossflowConvergenceStudy, DomainError> {
    let raw = |cells: usize| -> Result<f64, DomainError> {
        Ok(
            solve_crossflow_grid(&input.grid.with_cells(cells, cells).with_richardson(false))?
                .cold_water_c,
        )
    };
    let coarse = raw(input.base_cells)?;
    let medium = raw(input.base_cells * 2)?;
    let fine = raw(input.base_cells * 4)?;

    let coarse_gap = coarse - medium;
    let medium_gap = medium - fine;
    let observed_order = if medium_gap.abs() < 1e-12 {
        None
    } else {
        Some(f64::log2((coarse_gap / medium_gap).abs()))
    };
    let order = observed_order.unwrap_or(1.0);
    // f_exact ~ f_fine + (f_fine - f_medium) / (2^p - 1); medium_gap is (medium - fine), so it
    // enters with a negative sign.
    let extrapolated_cold_water_c = fine - medium_gap / (2f64.powf(order) - 1.0);
    let relative_error = (medium_gap / fine).abs();
    let grid_convergence_index_pct =
        100.0 * input.safety_factor * relative_error / (2f64.powf(order) - 1.0);

    Ok(CrossflowConvergenceStudy {
        cells: [input.base_cells, input.base_cells * 2, input.base_cells * 4],
        cold_water_c: [coarse, medium, fine],
        observed_order,
        extrapolated_cold_water_c,
        fine_grid_error_estimate_c: (extrapolated_cold_water_c - fine).abs(),
        grid_convergence_index_pct,
        interpretation: "An observed order near 1 confirms the expected first-order behaviour of the explicit cell scheme.",
    })
}
