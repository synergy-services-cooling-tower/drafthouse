# The `visuals` piece

This directory is the published **`visuals`** piece: the geometry derivation, the part-panel
modules, the vendored renderer and the stylesheet. It is pinned in
`bundles/visuals.manifest.json` (per-file SHA-256 and byte size), and its bytes are checked by
`npm run bundle-check`; a consumer deployment places the pinned bytes at the served paths
`scripts/bundle.mjs`'s `servedPath` defines (the private deployment procedure §2c). The packager that vendored
them into the retired JavaScript surface went with it (issue #60).

Two numbers rule, and they are why the piece is separate from the engine: a visual change never
triggers a wasm rebuild, and a geometry tweak never touches the parity-gated numerics. Nothing here
re-derives physics — every displayed number is read from the engine's own output (`numbers.js`'s
NUMERIC FIDELITY RULE).

## What this directory used to carry

Until issue #40 slice 4, a second surface lived on top of this piece: the prototype application
(`index.html`, `styles.css`, `src/app.js`, `src/core/**`), whose deployment entries, theming
conventions (`branding.themeTokens`) and provenance banner were documented here. That application —
and its public entry point `web/public-entry.js` — was deleted with the JavaScript reference; the
retirement is recorded in the private decision record (D17). The theming rationale it documented is kept in
the decision record; the piece itself is unchanged and still published, so the panels keep their
formula / substitution / result hierarchy and the warning colours (`--accent` amber, `--danger`
red) stay functional.

## Files

| file | role |
| --- | --- |
| `geometry.js` | the derivation — record fields in, drawable parts and dimensions out |
| `numbers.js` | the part panel's derived values (pressure breakdown, limits, tiles, sheets) |
| `panel-ui.js` | the panel's small render helpers (key/value rows, worked sheets, checks table) |
| `panel.css`, `panel.js` | the part panel's own styling and the rail/controls composition |
| `viewer.js` | the one renderer (three.js scene, labels, explode view) |
| `vendor/` | the vendored three.js build and its licence notice, part of the pin |
