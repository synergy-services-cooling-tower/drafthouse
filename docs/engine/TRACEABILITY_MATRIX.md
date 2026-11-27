# Traceability Matrix

| Engineering requirement | Implementation | Automated evidence | Current status |
|---|---|---|---|
| Saturation pressure and moist-air state | `crates/engine/src/psychrometrics.rs` (ported from the retired `src/core/psychrometrics.js`) | `rust/tests/{psychrometrics,anchors,vectors}.rs` | Reference-point and internal-consistency checks pass in the crate's suite |
| Range, approach, effectiveness, heat rejection | `crates/engine/src/water.rs` | `crates/cli/tests/selection.rs` and the parity harness | Ported; dedicated duty tests still recommended |
| Merkel demand | `crates/engine/src/merkel.rs` (`merkel_demand`) | `rust/tests/{merkel,vectors}.rs` | Simpson/Tchebycheff agreement and the recorded vectors pass in the crate's suite |
| CWT from available `KaV/L` | `crates/engine/src/merkel.rs` (`solve_cold_water_temperature`) | `crates/engine/tests/anchors.rs` | Known-point recovery and monotonicity checked in the crate's suite |
| Whole-tower characteristic | `crates/engine/src/capability.rs` | `crates/engine/tests/capability.rs` | The characteristic fit is exercised through the capability path |
| Characteristic capability | `crates/engine/src/capability.rs` | `crates/engine/tests/capability.rs`, parity harness | Identical design/test gives 100% |
| Capability uncertainty | `crates/engine/src/capability.rs` (`capability-mc`) | `crates/engine/tests/capability.rs`, recorded baseline rows | Seed repeatability; not code-prescribed uncertainty |
| Performance-grid interpolation | `crates/engine/src/performance_curve.rs` | `crates/engine/tests/performance_curve.rs` | Exact grid point checked |
| Performance-grid inverse flow | `crates/engine/src/performance_curve.rs` | `crates/engine/tests/performance_curve.rs` | Known flow recovery checked |
| Performance-curve capability | `crates/engine/src/performance_curve.rs` | `crates/engine/tests/performance_curve.rs` | Matching point gives 100% |
| Fill thermal correlation | `crates/engine/src/airside.rs` | `crates/engine/tests/airside.rs` | Synthetic correlation only |
| Fill wet pressure correlation | `crates/engine/src/airside.rs` | `crates/engine/tests/airside.rs` | Pressure sum checked; synthetic correlation only |
| Drift interpolation | `crates/engine/src/airside.rs` | `crates/engine/tests/airside.rs` | Interpolation and both curve ends checked |
| Minor/system pressure | `crates/engine/src/airside.rs` (`system_pressure_breakdown`) | `crates/engine/tests/airside.rs` | Component sum closes to total |
| Fan curve and system intersection | `crates/engine/src/fan.rs` (`solve_fan_system_intersection`) | `crates/engine/tests/fan.rs` | Residual checked below tolerance |
| Motor selection | `crates/engine/src/fan.rs` (`choose_standard_motor`) | `crates/engine/tests/fan.rs` | Size-table walk checked |
| Nozzle selection | `crates/engine/src/nozzle.rs` | `crates/engine/tests/nozzle.rs` | Flow capacity and count checked |
| Drift/water balance | `crates/engine/src/water_balance.rs` | `crates/engine/tests/water_balance.rs` | Mass balance closes |
| Crossflow topology | `crates/engine/src/crossflow.rs` | `crates/engine/tests/crossflow.rs` | Energy balance closes; model not externally validated |
| Natural-draft coupling | `crates/engine/src/natural_draft.rs` | `crates/engine/tests/natural_draft.rs` | Draft/system residual checked |
| Coupled catalog selection | `crates/engine/src/selection.rs` | `crates/cli/tests/selection.rs`, parity harness | Feasible results satisfy hard constraints |
| Browser workflow | `cockpit/` — the product UI's plane (the engine through the cockpit adapter; the JavaScript surfaces were retired, issues #40 and #60) | `tests/deployment-serving.test.js`, the deploy/retirement checks and the deployment-shaped audits | Automated browser evidence exists for the product surface |
| CTI Toolkit equivalence | None claimed | Future licensed comparison matrix | Not implemented |
| MRL equivalence | None claimed | Future licensed case-by-case comparison | Not implemented |
| Real component selection | Schema and engine ready | Requires supplier data validation | Not production ready |

## Source-to-code map

| Public source area | Repository use |
|---|---|
| CTI Toolkit public feature description | Defines separation of psychrometrics, demand worksheet, and performance evaluator |
| ATC-105 public scope | Defines the boundary around characteristic/performance evaluation and licensed procedure requirements |
| ATC-140 public scope | Defines the boundary around controlled drift testing |
| MRL public program list | Motivates separate counterflow, crossflow, mechanical-draft, natural-draft, and curve-generation structures |
| ASHRAE psychrometric references | Basis for moist-air equations and validation targets |
| IAPWS releases | Production target for water/steam properties |
| AMCA fan references | Basis for affinity-law and fan-curve validation targets |

See `docs/engine/REFERENCES.md` for links and provenance notes.
