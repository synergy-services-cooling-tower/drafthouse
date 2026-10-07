# Traceability Matrix

| Engineering requirement | Implementation | Automated evidence | Current status |
|---|---|---|---|
| Saturation pressure and moist-air state | `rust/src/psychrometrics.rs` (ported from the retired `src/core/psychrometrics.js`) | `rust/tests/{psychrometrics,anchors,vectors}.rs` | Reference-point and internal-consistency checks pass in the crate's suite |
| Range, approach, effectiveness, heat rejection | `rust/src/water.rs` | `rust/tests/selection.rs` and the parity harness | Ported; dedicated duty tests still recommended |
| Merkel demand | `rust/src/merkel.rs` (`merkel_demand`) | `rust/tests/{merkel,vectors}.rs` | Simpson/Tchebycheff agreement and the recorded vectors pass in the crate's suite |
| CWT from available `KaV/L` | `rust/src/merkel.rs` (`solve_cold_water_temperature`) | `rust/tests/merkel.rs` | Known-point recovery and monotonicity checked in the crate's suite |
| Whole-tower characteristic | `rust/src/capability.rs` | `rust/tests/capability.rs` | The characteristic fit is exercised through the capability path |
| Characteristic capability | `rust/src/capability.rs` | `rust/tests/capability.rs`, parity harness | Identical design/test gives 100% |
| Capability uncertainty | `rust/src/capability.rs` (`capability-mc`) | `rust/tests/capability.rs`, recorded baseline rows | Seed repeatability; not code-prescribed uncertainty |
| Performance-grid interpolation | `rust/src/performance_curve.rs` | `rust/tests/performance_curve.rs` | Exact grid point checked |
| Performance-grid inverse flow | `rust/src/performance_curve.rs` | `rust/tests/performance_curve.rs` | Known flow recovery checked |
| Performance-curve capability | `rust/src/performance_curve.rs` | `rust/tests/performance_curve.rs` | Matching point gives 100% |
| Fill thermal correlation | `rust/src/airside.rs` | `rust/tests/airside.rs` | Synthetic correlation only |
| Fill wet pressure correlation | `rust/src/airside.rs` | `rust/tests/airside.rs` | Pressure sum checked; synthetic correlation only |
| Drift interpolation | `rust/src/airside.rs` | `rust/tests/airside.rs` | Interpolation and both curve ends checked |
| Minor/system pressure | `rust/src/airside.rs` (`system_pressure_breakdown`) | `rust/tests/airside.rs` | Component sum closes to total |
| Fan curve and system intersection | `rust/src/fan.rs` (`solve_fan_system_intersection`) | `rust/tests/fan.rs` | Residual checked below tolerance |
| Motor selection | `rust/src/fan.rs` (`choose_standard_motor`) | `rust/tests/fan.rs` | Size-table walk checked |
| Nozzle selection | `rust/src/nozzle.rs` | `rust/tests/nozzle.rs` | Flow capacity and count checked |
| Drift/water balance | `rust/src/water_balance.rs` | `rust/tests/water_balance.rs` | Mass balance closes |
| Crossflow topology | `rust/src/crossflow.rs` | `rust/tests/crossflow.rs` | Energy balance closes; model not externally validated |
| Natural-draft coupling | `rust/src/natural_draft.rs` | `rust/tests/natural_draft.rs` | Draft/system residual checked |
| Coupled catalog selection | `rust/src/selection.rs` | `rust/tests/selection.rs`, parity harness | Feasible results satisfy hard constraints |
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

See `docs/REFERENCES.md` for links and provenance notes.
