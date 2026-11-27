# `drafthouse-engine` · `water`

Water properties — port of the part of `src/core/water.js` the slice needs.

## Notes carried from the source comments

### `fn water_specific_heat_kj_kg_k`

Port of `waterSpecificHeatKJkgK`.

An engineering approximation (the reference says so too; IAPWS/TEOS-10 would be
required for contractual seawater work), floored at 3.7 kJ/(kg·K).
