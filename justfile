# Every gate CI runs (the core, rust and ts packs and .gatekeeper/gates/), the same as drover turn done.
check:
    ./.gatekeeper/run check

# One gate by name, e.g. `just gate local/parity`.
gate name:
    ./.gatekeeper/run check --gate {{name}}
