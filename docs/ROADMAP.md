# Production Roadmap

## Phase 1 — Freeze the engineering basis

- Obtain and control the current licensed ATC-105 and ATC-140 documents required by the intended workflow.
- Define supported tower types, units, pressure conventions, reference planes, and result labels.
- Establish software quality, review, approval, and data-governance procedures.
- Convert public-prototype assumptions into an approved engineering specification.

Exit criterion: signed calculation specification and traceability matrix.

## Phase 2 — Reference properties and numerical validation

- Validate psychrometrics against ASHRAE reference cases across the intended pressure/temperature range.
- Replace approximate water/seawater properties with an approved IAPWS/TEOS-10-compatible implementation.
- Add numerical convergence studies for quadrature, root scans, crossflow grids, and natural-draft iteration.
- Add unit conversion and rounding controls.

Exit criterion: bounded error against controlled reference values.

## Phase 3 — Complete CTI acceptance workflows

- Implement current ATC-105 test validity, corrections, averaging, crossplot, reporting, and uncertainty requirements.
- Add instrument records, calibration data, time-series ingestion, outlier policy, and audit trail.
- Compare every supported method against licensed CTI Toolkit cases.
- Keep characteristic and performance-curve methods as separate report types.

Exit criterion: independently reviewed agreement with approved benchmark cases.

## Phase 4 — Controlled component databases

- Import supplier fill thermal and wet-pressure data with tested domains and uncertainty.
- Import drift curves with test configuration, sealing/bypass requirements, and pressure loss.
- Import fan pressure, efficiency, power, speed, pitch, and stable-range data.
- Import nozzle hydraulic and spray-distribution data.
- Implement versioning, approval, source rights, and no-extrapolation enforcement.

Exit criterion: each selectable record has approved provenance and regression cases.

## Phase 5 — Mechanical-draft rating and selection

- Add explicit induced- versus forced-draft geometry and recirculation treatment.
- Reconcile fan static/total pressure and system reference planes.
- Add multiple cells, cell staging, VFD control, turndown, fan-off operation, and annual weather simulation.
- Add structural, material, fire, sound, vibration, access, and maintainability constraints.
- Replace synthetic economics with project-specific THB/USD cost models.

Exit criterion: validated counterflow selection for controlled product families.

## Phase 6 — Crossflow and natural draft

- Validate crossflow local transfer equations and water/air maldistribution.
- Add gravity distribution basins and orifice/header balance.
- Add natural-draft vertical/radial zones, shell losses, rain zone, wind, and multiple fills/rings.
- Compare with licensed MRL cases and available test data at intermediate as well as final variables.

Exit criterion: documented model validity and error by tower family.

## Phase 7 — Productization

- Add authenticated catalog administration, revision approval, and audit logs.
- Add persistent projects, scenarios, reports, JSON/CSV imports, and controlled exports.
- Add worker-thread/server execution for large searches and uncertainty runs.
- Add CI regression packs, browser tests, security review, backups, and deployment documentation.
- Add explicit report watermarks for prototype, internal engineering, contractual, and certified contexts.

Exit criterion: release approval under the company’s engineering software process.
