# Software and Solver Architecture

## 1. Design principles

- **SI internally:** unit conversion belongs at input/output boundaries.
- **Pure calculation modules:** core functions do not depend on the browser DOM.
- **Separated result classes:** acceptance-test capability, model rating and commercial selection are never represented by one generic percentage.
- **Data before optimization:** candidates are valid only inside controlled component domains.
- **Bounded numerical methods:** root solves scan and bracket physical intervals before bisection.
- **Traceability:** every output should retain solver, catalog and source revisions.

## 2. Runtime architecture

```mermaid
flowchart LR
    UI[Browser — the cockpit plane] --> COCKPIT[cockpit/ document and its built module]
    COCKPIT --> ENGINE[The engine, through the cockpit adapter — the same crate, compiled in]
    ENGINE --> PSY[Psychrometrics]
    ENGINE --> MERKEL[Merkel demand / inverse CWT]
    ENGINE --> CTI[Capability and performance curves]
    ENGINE --> SELECT[Component selector]
    ENGINE --> UTIL[Nozzle / water / natural draft]

    SELECT --> CATALOG[Synthetic catalog]
    SELECT --> AIR[Airside pressure and fill supply]
    SELECT --> FAN[Fan/system operating point]
    SELECT --> MERKEL
    SELECT --> XFLOW[Crossflow grid]
    SELECT --> WATER[Water balance]

    ENGINE -. published as pinned pieces .-> BUNDLE[bundle release — engine + visuals]
```

The public surface is the cockpit plane: `cockpit/` — a Bevy/WASM application whose document boots
its own built module (`cockpit/pkg/**`, built by `cockpit/tools/build-web.sh`), with the engine
compiled in through the cockpit adapter (issue #58). The plane's committed half is what
`deploy-manifest.txt` lists, and the release publishes it as its own artifact
(`scripts/cockpit-artifact.mjs`); the pinned `engine` and `visuals` pieces remain the repository's
distributable engine bundle (D16). The internal full application and the real catalog are out of
scope (the private release procedure, the private deployment procedure). The engine itself (`rust/`) has no runtime package
dependency: the native binary and the wasm build are the same crate, and the JavaScript reference the
port was checked against while it existed was retired in issue #40 slice 4 (last commit in it:
the private decision record D17); the retired JavaScript surface was the served surface until the cockpit became the product UI (issue #60, D22).

## 3. Mechanical-draft selection sequence

The selector's enumeration and solve sequence is unchanged by the port; the engine reads no commercial
field (issue #26's removal is kept), so candidates are ranked by the stated engineering objective and
its tie-break chain, not by lifecycle cost:

```mermaid
flowchart TD
    A[Read duty and constraints] --> B[Enumerate tower/fill/depth/drift/fan/speed]
    B --> C[Build wet system pressure function]
    C --> D[Solve fan pressure = system pressure]
    D --> E[Calculate dry-air flow and L/G]
    E --> F[Check fill and drift tested envelopes]
    F --> G[Calculate available Merkel number]
    G --> H{Tower topology}
    H -->|Counterflow| I[Solve required Merkel = available Merkel for CWT]
    H -->|Crossflow| J[Solve 2-D finite-volume grid]
    I --> K[Check CWT, drift, power, footprint]
    J --> K
    K -->|Rejected| L[Record reason]
    K -->|Feasible| M[Select motor and nozzles]
    M --> N[Calculate water use]
    N --> O[Rank by the objective and tie-break chain]
```

The present selector performs one fan/system solve using inlet density. A production implementation should add an outer convergence loop that updates outlet/plenum density and any density-dependent losses.

## 4. Characteristic-capability sequence

```mermaid
flowchart TD
    A[Test measurements] --> B[Test psychrometrics]
    B --> C[Compute test L/G]
    C --> D[Integrate test Merkel demand]
    D --> E[Fit characteristic through test point]
    F[Design condition] --> G[Generate design demand versus L/G]
    E --> H[Intersect characteristic and demand]
    G --> H
    H --> I[Capability = intersection L/G / design L/G]
    I --> J[Optional seeded Monte Carlo propagation]
```

The licensed ATC-105 procedure may require additional corrections, validity checks and contractual conventions. Those are deliberately outside the current prototype.

## 5. Natural-draft sequence

```mermaid
flowchart TD
    A[Guess face velocity] --> B[Compute air mass flow and L/G]
    B --> C[Compute wet resistance and fill Merkel supply]
    C --> D[Solve CWT]
    D --> E[Estimate saturated outlet/plume state]
    E --> F[Compute plume density and buoyancy]
    F --> G[Residual = draft pressure - resistance]
    G -->|Not zero| A
    G -->|Converged| H[Return airflow, CWT and pressure closure]
```

## 6. Data boundaries

The sample catalog is executable JavaScript for convenience. The production boundary should instead be a validated data service with:

- immutable catalog revisions;
- per-record hashes;
- source-document links;
- units and normalization definitions;
- tested operating envelopes;
- interpolation/extrapolation policy;
- engineering approval status; and
- access/licence controls.

## 7. Suggested service decomposition

A production web system can retain the same core modules while adding:

```text
web-client/
calculation-api/
  properties/
  thermal/
  cti-evaluation/
  airside/
  selection/
component-data-service/
report-service/
validation-suite/
audit-store/
```

The calculation API should be stateless. A project service should store immutable input/result snapshots rather than allowing old results to be recomputed silently with new catalogs.
