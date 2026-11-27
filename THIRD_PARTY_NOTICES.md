# Third-Party Notices and Intellectual-Property Boundary

## Bundled code

The engine crate is dependency-free and does not bundle CTI Toolkit, MRL software, PsychroLib, IAPWS source code, manufacturer selection code, or supplier product curves.

The psychrometric implementation was independently written from published ASHRAE-style equations and checked conceptually against the open-source PsychroLib documentation. See `docs/REFERENCES.md`.

### Build and gate tools

`crates/blocks` builds the `blocks check` tool from the Blocks repository (MIT) at a pinned tag. It is a check
run in CI and is in no shipped file. `.gatekeeper/run` fetches the pinned gatekeeper binary and checks its sha256.

## Referenced standards and software

The following names identify external products, standards or organizations and remain the property of their respective owners:

- Cooling Technology Institute (CTI), CTI Toolkit, ATC-105, ATC-140 and STD-201;
- MRL cooling-tower software and Richard Aull Cooling Tower Consulting;
- ASHRAE Handbook — Fundamentals;
- PsychroLib;
- IAPWS formulations;
- AMCA standards and educational material; and
- all manufacturer fill, fan, drift-eliminator and nozzle data.

No licence to reproduce paid standards, proprietary software or commercial data is granted by this repository.

## Data notice

All bundled tower, fill, drift, fan, motor, nozzle, price and degradation records are synthetic. Similarity to any real product is coincidental. Replace them only with data that the organization is legally permitted to use and that has completed engineering approval.
