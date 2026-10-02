# Third-Party Notices and Intellectual-Property Boundary

## Bundled code

The prototype is dependency-free and does not bundle CTI Toolkit, MRL software, PsychroLib, IAPWS source code, manufacturer selection code, or supplier product curves.

The psychrometric implementation was independently written from published ASHRAE-style equations and checked conceptually against the open-source PsychroLib documentation. See `docs/REFERENCES.md`.

### three.js (the 3D companion panel)

The parts panel renders with **three.js r186 (`0.186.0`)**, vendored as committed local ES modules and imported natively — no CDN, no npm dependency and no build step:

- `web/vendor/three.module.js` — SHA-256 `9052042d676cb0fdc1ddfefe193053f34b7ac0513a616fdac4535d49987812ea`
- `web/vendor/three.core.js` — SHA-256 `9edde002b066a9a05676a6127f67735b62baf399bdea529f2f7e31657da769e6`
- `web/vendor/three-LICENSE.txt` — SHA-256 `8b378ebe60e2fe500158cb0ac71cb5e8b7d92953c2abcc63a0eb90499653b5bc`

Licence: **MIT**, Copyright © 2010-2026 three.js authors. The MIT text is vendored alongside the files as `web/vendor/three-LICENSE.txt`. `three.module.js` imports `./three.core.js`, so both files ship together.

The panel's own modules (`web/panel.js`, `web/geometry.js`, `web/numbers.js`, `web/panel-ui.js`, `web/viewer.js`, `web/panel.css`) are original to this repository and carry the same PolyForm Noncommercial 1.0.0 licence as the rest of it (see `LICENSE`).

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
