//! The visual pass's honest halves, in one dependency-light crate:
//!
//! 1. **The data-seam registry** ([`SEAMS`]) - every visual effect in the pass, the fixture value that
//!    drives it today, the *illustrative* or *definitional* rule where no engine field exists yet, and the
//!    engine field that will replace that rule. `VISUAL_DATA_SEAMS.md` is generated from this table; the
//!    running app renders the same table in its **Data seams** panel, so the document cannot drift.
//! 2. **The illustrative mapping** ([`mapping`]) - the few numbers the visual pass has to invent (an rpm
//!    for a fan record that only carries a speed ratio, a cone angle for a nozzle record that only carries
//!    an orifice, an animation speed for an airflow) plus the round-4 unit conversions and the ISA
//!    pressure/altitude definition. Pure functions, unit-tested, documented as invented or defined.
//! 3. **Round 4's three rule sets**: [`custom`] (the custom-part field list, the value model, the
//!    validation that refuses a value and names its range, and the record a save builds), [`fields`] (the
//!    parameter card's rows: every field of a record with its unit) and [`duty`] (the duty & site panel's
//!    seeds, the recorded limits, the psychrometric reference and the evidence gate that prints
//!    `out of fixture range` instead of a number the fixture cannot evidence).
//!
//! This crate contains **no cooling-tower physics** and no engine implementation. Every engineering number
//! the pass shows comes from `Engine::run`, reached through the contract crate in `cockpit/contract`:
//! the repository's real engine by default (issue #58), or the approved baseline's recorded
//! `FixtureEngine` replay when the cockpit is built with the `fixture-engine` feature instead.
//!
//! Required UI copy, verbatim:
//! `Illustrative visualisation — engineering values will come from the Rust engine.`

// The four modules are the approved pass's files, imported as they are; `#[rustfmt::skip]` keeps a
// future `cargo fmt` from rewriting them (the table `docs/COCKPIT_SEAMS.md` is generated from them).
#[rustfmt::skip]
pub mod custom;
#[rustfmt::skip]
pub mod duty;
#[rustfmt::skip]
pub mod fields;
#[rustfmt::skip]
pub mod mapping;

pub use mapping::*;

/// The copy the pass must carry wherever illustrative values are shown.
pub const REQUIRED_COPY: &str =
    "Illustrative visualisation — engineering values will come from the Rust engine.";

/// The four honest statuses a seam can have.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// The value comes from `Engine::run` / the engine's own input contract today.
    Engine,
    /// The value is a fixture-catalog fact (compatibility, geometry, ratings) - data, not physics.
    Fixture,
    /// No engine field exists yet: the visual pass invents a *look* and says so.
    Illustrative,
    /// A stated *definition* (a unit convention or a standard relation) that is neither an engine result nor
    /// fixture data - the pass names it and never presents it as a calculated engineering value.
    Definition,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Engine => "engine contract",
            Status::Fixture => "fixture data",
            Status::Illustrative => "illustrative",
            Status::Definition => "definition",
        }
    }

    /// One word for a chip.
    pub fn chip(self) -> &'static str {
        self.label()
    }
}

/// One binding between a visual effect and the data that drives it.
#[derive(Clone, Copy, Debug)]
pub struct Seam {
    /// Stable id (`area.effect`), used by the panel and the markdown.
    pub id: &'static str,
    /// The visual effect this seam drives, in the words of the scene.
    pub drives: &'static str,
    /// Where the value comes from today (fixture file path or `Engine::run` output field).
    pub source: &'static str,
    /// The rule that turns that value into pixels. Says `as returned` when nothing is invented.
    pub rule: &'static str,
    /// The engine field/output that will replace today's source.
    pub engine_field: &'static str,
    /// Where the binding lives in Rust, `file::symbol`.
    pub code: &'static str,
    pub status: Status,
}

/// Every visual effect of the pass and its binding. Order = the order the panel lists them: the tower
/// from the air path out (fan, then the stack, then the fill, then the water), then round 4's authoring
/// seams (a custom record, the parameter card, the duty & site panel and the recorded limits).
pub const SEAMS: [Seam; 38] = [
    Seam {
        id: "fan.speed_ratio",
        drives: "fan rpm read-out, blade rotation rate, tachometer needle",
        source: "EngineInput.speed_ratio x FanRecord.nominalRpm (the record's own rated speed; the range from catalog.fans[<id>].allowedSpeedRatio)",
        rule: "rpm = speed_ratio x nominalRpm (the record's own datum; `-` when the record states none); blade turns/s = rpm / 60",
        engine_field: "FanRecord.nominal_rpm - the ratio is already the engine's input and the rated speed is the record's own field",
        code: "drafthouse_cockpit::scene::plan (fan blades) / drafthouse_cockpit::ui::tacho / drafthouse_cockpit_seams::mapping::blade_turn_hz",
        status: Status::Engine,
    },
    Seam {
        id: "fan.airflow",
        drives: "airflow chevron speed and count in every zone, airflow read-out",
        source: "EngineOutput.airflow_m3_s (Engine::run)",
        rule: "animation factor = airflow / 124.84 (the anchor run), clamped 0.15..2.2",
        engine_field: "same field - already engine output",
        code: "drafthouse_cockpit::scene::plan (flow map chevrons)",
        status: Status::Engine,
    },
    Seam {
        id: "fan.curve",
        drives: "fan curve polyline on the operating-point instrument",
        source: "EngineOutput.fan_system_curve.fan (the fan record's points at the current speed ratio)",
        rule: "as returned - plotted in m3/s against Pa",
        engine_field: "same field",
        code: "drafthouse_cockpit::ui::plot",
        status: Status::Engine,
    },
    Seam {
        id: "fan.operating_point",
        drives: "operating-point marker on the tower and on the fan/system instrument",
        source: "EngineOutput.fan_system_curve.operating_point (Engine::run)",
        rule: "as returned - the marker sits at (flow_m3_s, pressure_pa)",
        engine_field: "same field - the engine's own fan/system crossing",
        code: "drafthouse_cockpit::scene::plan (operating-point rail) / drafthouse_cockpit::ui::plot",
        status: Status::Engine,
    },
    Seam {
        id: "pressure.zone",
        drives: "pressure rail segments, per-zone Pa labels, zone tints on the section",
        source: "EngineOutput.pressure_by_zone[] { zone, layer, pressure_pa, share_pct }",
        rule: "segment height = share_pct; label = pressure_pa; air-path order bottom-up",
        engine_field: "same field",
        code: "drafthouse_cockpit::scene::plan (pressure rail) / drafthouse_cockpit::ui::rail (zone table)",
        status: Status::Engine,
    },
    Seam {
        id: "fill.layer_geometry",
        drives: "drawn layer bands, the depth ruler, the mixed-stack order",
        source: "EngineInput.fill_layers[].depth_m; tower.fillDepthOptionsM for the ruler",
        rule: "band height = depth / sum(depths) x fill band, minimum 14 px",
        engine_field: "fill_layers as the engine validated them (order + depths)",
        code: "drafthouse_cockpit::scene::layout",
        status: Status::Fixture,
    },
    Seam {
        id: "fill.layer_identity",
        drives: "layer tint, per-layer KaV/L, pressure and cooling share in the stack read-out",
        source: "EngineOutput.kavl_per_layer[] { fill_id, kavl, pressure_pa, cooling_share_pct, inside_envelope }",
        rule: "as returned - the layer is tinted by fill_id",
        engine_field: "same field",
        code: "drafthouse_cockpit::ui::rail (layer rows)",
        status: Status::Engine,
    },
    Seam {
        id: "drift.haze",
        drives: "drift-eliminator haze band, its share label",
        source: "EngineOutput.pressure_by_zone[zone = Drift].share_pct",
        rule: "haze alpha = 0.25 + 0.75 x share; the haze *look* is illustrative, the share is not",
        engine_field: "drift zone pressure + DriftRecord curve at the engine's face velocity",
        code: "drafthouse_cockpit::scene::plan (drift haze)",
        status: Status::Engine,
    },
    Seam {
        id: "spray.cones",
        drives: "spray cone count, spacing and cone geometry above each fill layer",
        source: "nozzle arrangement authored in the editor (count/spacing/pattern) + catalog.nozzles[<id>].orificeDiameterM",
        rule: "half-angle = 24 deg + 0.4 deg per mm of orifice (illustrative); radius = height x tan(half-angle)",
        engine_field: "NozzleArrangement input + the engine's spray-zone distribution model",
        code: "drafthouse_cockpit::scene::plan (spray cones) / drafthouse_cockpit_seams::mapping::spray_half_angle_deg",
        status: Status::Illustrative,
    },
    Seam {
        id: "spray.coverage",
        drives: "coverage band on each fill layer and the coverage % on the label",
        source: "none - geometric construction from spacing and cone radius",
        rule: "coverage = min(1, 2 x radius / spacing): the flat-area overlap of two adjacent cones",
        engine_field: "EngineOutput.nozzle_coverage_pct + distribution uniformity (not implemented)",
        code: "drafthouse_cockpit::scene::plan (coverage band) / drafthouse_cockpit_seams::mapping::coverage_fraction",
        status: Status::Illustrative,
    },
    Seam {
        id: "water.rain",
        drives: "falling-water particle density in the rain zone and the basin flow label",
        source: "EngineOutput.water_flow_m3_hr",
        rule: "drop count = clamp(round(flow / 90), 3, 9); the droplet *look* is illustrative",
        engine_field: "same field + a droplet/liquid-loading model (not implemented)",
        code: "drafthouse_cockpit::scene::plan (rain drops) / drafthouse_cockpit_seams::mapping::rain_drop_count",
        status: Status::Engine,
    },
    Seam {
        id: "water.temperature",
        drives: "the falling water's colour, from the spray header down to the basin",
        source: "EngineOutput.cold_water_c and EngineOutput.range_c (the hot end is their recorded sum)",
        rule: "tint = a three-stop walk (basin blue, sand, spray terracotta - a two-stop blue/orange blend passes through violet) from the spray down to the water surface, with how far down the hot end reaches set by the run's own spread (`mapping::water_ramp`, reference 8 C): a 1 C run shows a short hot band under the spray, the fixture's 5 C run stays warm most of the way. The endpoints are the run's temperatures; the walk between them is the pass's look",
        engine_field: "same fields + a spray-to-basin temperature profile (not implemented)",
        code: "drafthouse_cockpit::ui::scene_overlay (falling water) / drafthouse_cockpit::theme::water_tint / drafthouse_cockpit_seams::mapping::water_ramp",
        status: Status::Engine,
    },
    Seam {
        id: "ambient.inlet_air",
        drives: "inlet-air callout (dry bulb, wet bulb, relative humidity, humidity ratio)",
        source: "anchor.air.inlet in the fixture file (the recorded run's psychrometrics)",
        rule: "printed as recorded - no rounding beyond the display format",
        engine_field: "EngineOutput.air.inlet once psychrometrics are part of the output contract",
        code: "drafthouse_cockpit::ui::scene_overlay (inlet-air call-out)",
        status: Status::Fixture,
    },
    Seam {
        id: "parts.slot_validity",
        drives: "valid / invalid drop states, slot contents, the part detail strip",
        source: "catalog compatibility: tower.compatibleFanIds, fill.compatibleTowerTypes, fill.allowedWaterQualityClasses, drift.maxWaterTemperatureC",
        rule: "catalog lookup only - a slot accepts or refuses by data, never by an estimated number",
        engine_field: "the engine's own catalog revision + its validation envelope (EngineOutput.validation)",
        code: "drafthouse_cockpit::state::check_drop / drafthouse_cockpit::ui::scene_overlay (bay verdict)",
        status: Status::Fixture,
    },
    Seam {
        id: "layers.mix_share",
        drives: "per-layer share bar in the fill stack read-out",
        source: "EngineOutput.kavl_per_layer[].cooling_share_pct",
        rule: "as returned - bar width = share of the total available transfer",
        engine_field: "same field",
        code: "drafthouse_cockpit::ui::rail (layer rows)",
        status: Status::Engine,
    },
    Seam {
        id: "run.provenance",
        drives: "the provenance strip: engine id, catalog revision, status, warning",
        source: "EngineOutput.provenance (Engine::run)",
        rule: "printed as returned",
        engine_field: "same field",
        code: "drafthouse_cockpit::ui::rail (provenance line)",
        status: Status::Engine,
    },
    // ============================ round 4, change 2: a part the user authors ============================
    Seam {
        id: "parts.custom_record",
        drives: "the `+ custom` chip at the end of every rail section, the form it opens, the amber dot on the saved chip and every state that chip can reach (drag, bay verdict, picker, fitted bay)",
        source: "cockpit/assets/custom-fields.json - the recorded descriptor (the generator that produced it is not part of this import): the field names and their order are the fixture's, each range is the span the bundled catalog records for that field",
        rule: "the form offers exactly those fields and no way to add one; a value outside its recorded range is refused and the range is named; the saved record is built into the engine's own FanRecord / DriftRecord / FixtureFill / NozzleRecord (drafthouse_cockpit_seams::custom) and appended to the session catalog, so it takes the SAME drop path as a catalog card - no new physics, no new field, no estimate",
        engine_field: "ServerCommand::SaveCustomPart { class, id, values } - the future server command that would persist a custom part; it is not in the shipped ServerCommand enum yet, so today the record is session-only",
        code: "drafthouse_cockpit_seams::custom (field list, parse, validate, to_fan/to_drift/to_fill/to_nozzle) / drafthouse_cockpit::form (the form) / drafthouse_cockpit::state::Catalog::add_custom",
        status: Status::Fixture,
    },
    // ============================ round 4, change 3: the parameter card =================================
    Seam {
        id: "parts.parameter_card",
        drives: "the hover (desktop) / long-press (phone) parameter card on a rail chip, a picker card or a fitted bay; the sparkline of a curve field",
        source: "the record itself (catalog / custom / fixture) + EngineOutput for the fitted part: kavl_per_layer, pressure_by_zone, fan_system_curve, airflow_m3_s, fan_power_kw",
        rule: "one row per record field, with its unit and its source (`catalog` / `custom` / `fixture`); a curve-typed field draws a sparkline of its recorded points instead of a scalar; the card is placed on the side of the hovered rect that leaves it uncovered",
        engine_field: "same fields - the card is a view of the record and of Engine::run, and adds no value of its own",
        code: "drafthouse_cockpit_seams::fields (fan_rows / drift_rows / fill_rows / nozzle_rows / custom_rows) / drafthouse_cockpit::hover",
        status: Status::Engine,
    },
    // ============================ round 4, change 4: duty & site =======================================
    Seam {
        id: "duty.water_flow",
        drives: "the editable water-flow row, the kg/s it converts to, and the engine run that follows",
        source: "provenance.duty.waterMassFlowKgS (200 kg/s) -> m3/hr at 1000 kg/m3; anchor.waterFlow.m3Hr (724.81) is the engine's own pair at its own density 993.36 kg/m3",
        rule: "m3/hr = kg/s / 1000 x 3600 (mapping::m3_hr_from_kg_s, the brief's convention) - both spellings shown, and the engine's own density-quoted pair printed beside them",
        engine_field: "Duty.water_flow_m3_hr - the engine's own input field, already consumed",
        code: "drafthouse_cockpit::duty_panel (water flow row) / drafthouse_cockpit_seams::mapping::m3_hr_from_kg_s",
        status: Status::Engine,
    },
    Seam {
        id: "duty.hot_water",
        drives: "the editable hot-water row and, through it, the engine re-run",
        source: "provenance.duty.hotWaterC (42 C)",
        rule: "typed into Duty.hot_water_c; the engine re-runs on every change and the read-out moves with it",
        engine_field: "Duty.hot_water_c",
        code: "drafthouse_cockpit::duty_panel (hot water row)",
        status: Status::Engine,
    },
    Seam {
        id: "duty.target_cold_water",
        drives: "the editable target-cold-water row, the demand line on the performance chart and the approach the panel derives",
        source: "provenance.duty.targetColdWaterC (32 C)",
        rule: "typed into Duty.target_cold_water_c; the charts' demand series and the engine's own approach check read it",
        engine_field: "Duty.target_cold_water_c",
        code: "drafthouse_cockpit::duty_panel (target cold water row)",
        status: Status::Engine,
    },
    Seam {
        id: "duty.range",
        drives: "the read-only range the panel shows live while the duty is edited",
        source: "derived from the two rows above: the duty's own design pair",
        rule: "range = hot water - target cold water (the engine's own worked sheet computes the same row from the same two fields; its `range_c` output uses the SOLVED cold water instead, and the read-out shows that one)",
        engine_field: "EngineOutput.range_c (solved cold water) - a different, also-recorded reading of the same idea",
        code: "drafthouse_cockpit::duty_panel (derived range row)",
        status: Status::Definition,
    },
    Seam {
        id: "duty.approach",
        drives: "the read-only approach the panel shows live, and the margin its own validation names",
        source: "derived from target cold water and entering wet bulb",
        rule: "approach = target cold water - entering wet bulb; the panel's own pre-check requires >= 0.5 C (mapping::APPROACH_MARGIN_MIN_C) and names the limit when it is not met. The fixture engine's own rule is weaker (`target_cold_water_c <= wet_bulb_c` refused) and both are shown",
        engine_field: "EngineOutput.approach_c (solved cold water - wet bulb) + EngineOutput.validation",
        code: "drafthouse_cockpit::duty_panel (derived approach row and its validation)",
        status: Status::Definition,
    },
    Seam {
        id: "duty.wet_bulb",
        drives: "the editable entering-wet-bulb row; the evidence gate that decides whether a cold-water number may be printed at all",
        source: "provenance.duty.wetBulbC (27 C); the recorded sweep domain 21-27 C (anchor.sweeps.wetBulbC, the points the engine accepted)",
        rule: "typed into Duty.wet_bulb_c; outside the recorded sweep the read-outs show `out of fixture range` in amber instead of a number",
        engine_field: "Duty.wet_bulb_c",
        code: "drafthouse_cockpit::duty_panel / drafthouse_cockpit_seams::duty::Evidence",
        status: Status::Engine,
    },
    Seam {
        id: "duty.dry_bulb",
        drives: "the editable dry-bulb row and the relative-humidity row derived from it",
        source: "provenance.duty.dryBulbC (33 C)",
        rule: "typed into Duty.dry_bulb_c (the engine's own requirement field). The fixture engine's re-expression does not consume it, and the panel says so rather than pretending the read-out moves",
        engine_field: "Duty.dry_bulb_c",
        code: "drafthouse_cockpit::duty_panel (dry bulb row)",
        status: Status::Engine,
    },
    Seam {
        id: "duty.relative_humidity",
        drives: "the read-only relative-humidity row beside the dry bulb (the pair's derived half) and the recorded inlet RH printed next to it",
        source: "anchor.air.inlet.humidityRatio + anchor.air.saturation (the fixture's own saturation table) + anchor.air.inlet.relativeHumidity (the engine's recorded value)",
        rule: "derived RH = recorded inlet humidity ratio / the fixture's saturation humidity ratio at the entered dry bulb (linear interpolation in the recorded table, no extrapolation). At the recorded inlet that reads 0.6193 against the engine's recorded 0.6313 - both are printed, because this is a defined display value and not the engine's psychrometrics",
        engine_field: "EngineOutput.air.inlet.relative_humidity (recorded; psychrometrics are not in the output contract yet)",
        code: "drafthouse_cockpit_seams::duty::Psychro::relative_humidity_at / drafthouse_cockpit::duty_panel",
        status: Status::Definition,
    },
    Seam {
        id: "duty.barometric_pressure",
        drives: "the editable barometric-pressure row and the altitude derived from it",
        source: "provenance.duty.pressurePa (101325 Pa)",
        rule: "typed into Duty.pressure_pa",
        engine_field: "Duty.pressure_pa",
        code: "drafthouse_cockpit::duty_panel (pressure row)",
        status: Status::Engine,
    },
    Seam {
        id: "duty.site_altitude",
        drives: "the derived site-altitude row, and the same row as the editable half when the panel is switched to altitude",
        source: "derived from the barometric pressure (the fixture records no altitude at all)",
        rule: "ISA standard troposphere: z = (T0/L) x (1 - (p/p0)^(1/5.2558774)) with p0 = 101325 Pa, T0 = 288.15 K, L = 0.0065 K/m (mapping::altitude_m_from_pressure_pa and its inverse). A definition, not a tower calculation; it feeds no engine field",
        engine_field: "none - a site field the engine's requirement record does not carry yet; the panel labels it `definition`",
        code: "drafthouse_cockpit_seams::mapping::altitude_m_from_pressure_pa / drafthouse_cockpit::duty_panel",
        status: Status::Definition,
    },
    Seam {
        id: "duty.evidence_range",
        drives: "the amber `out of fixture range` the read-out shows instead of a number, and the panel's own row naming which entry left the recorded domain",
        source: "anchor.sweeps.waterMassFlowKgS (feasible 140-210 kg/s -> 504-756 m3/hr at 1000 kg/m3) and anchor.sweeps.wetBulbC (feasible 21-27 C) in the bundled fixture",
        rule: "inside the recorded domain the engine interpolates the recorded run and the numbers are printed; outside it the pass prints `out of fixture range` and never an extrapolated value",
        engine_field: "the real engine's own envelope: EngineOutput.validation carries the limits it applied",
        code: "drafthouse_cockpit_seams::duty::Evidence::verdict / drafthouse_cockpit::ui::rail (the gated read-out)",
        status: Status::Fixture,
    },
    Seam {
        id: "water.quality_class",
        drives: "the water-quality-class row (the engine's own enum) and every fill drop verdict that reads it",
        source: "provenance.fixed.waterQualityClass (\"moderate\"); the vocabulary is catalog.waterQualityFactors' own keys (clean / moderate / dirty)",
        rule: "the row offers the engine's own values only; check_drop refuses a fill whose allowedWaterQualityClasses do not list the chosen class, naming the field",
        engine_field: "Duty.water_quality_class",
        code: "drafthouse_cockpit::duty_panel (class row) / drafthouse_cockpit::state::check_drop",
        status: Status::Engine,
    },
    Seam {
        id: "water.salinity",
        drives: "the editable salinity row",
        source: "provenance.fixed.salinityGKg (0 g/kg)",
        rule: "typed into Duty.salinity_g_kg and carried to the engine; the fixture engine's re-expression does not consume it, which the panel states",
        engine_field: "Duty.salinity_g_kg",
        code: "drafthouse_cockpit::duty_panel (salinity row)",
        status: Status::Engine,
    },
    Seam {
        id: "water.cycles_of_concentration",
        drives: "the editable cycles row and the makeup-water read-out that the engine derives from it",
        source: "provenance.fixed.cyclesOfConcentration (4)",
        rule: "typed into Duty.cycles_of_concentration; the engine's own makeup line reads it (`n / (n - 1)` blowdown factor, clamped at 1.01)",
        engine_field: "Duty.cycles_of_concentration -> EngineOutput.makeup_m3_hr",
        code: "drafthouse_cockpit::duty_panel (cycles row)",
        status: Status::Engine,
    },
    Seam {
        id: "water.tds_chloride_ph",
        drives: "the display-only TDS / chloride / pH note row under the water-quality section",
        source: "none - the fixture's water-quality record carries the class, the salinity and the cycles of concentration and nothing else",
        rule: "the row is printed as `not recorded` with the label `recorded, not used by the engine yet`: the pass invents no value and no effect for it (no scaling, no limit, no claim)",
        engine_field: "the engine's water-quality record once it carries those fields",
        code: "drafthouse_cockpit::duty_panel (display-only rows)",
        status: Status::Fixture,
    },
    Seam {
        id: "limits.max_drift",
        drives: "the recorded max-drift limit row",
        source: "provenance.fixed.maxDriftPpm (30 ppm)",
        rule: "printed as recorded, quoted with the drift the engine reports for the fitted eliminator where it exists; the pass adds no comparison it cannot evidence",
        engine_field: "the engine's own limit record (EngineOutput.validation)",
        code: "drafthouse_cockpit::duty_panel (limits rows)",
        status: Status::Fixture,
    },
    Seam {
        id: "limits.max_electrical_input",
        drives: "the recorded max-electrical-input limit row",
        source: "provenance.fixed.maxElectricalInputKW (75 kW)",
        rule: "printed as recorded beside the engine's own fan_power_kw for the current run; no money field, no tariff, no comparison the fixture cannot evidence",
        engine_field: "the engine's own limit record (EngineOutput.validation)",
        code: "drafthouse_cockpit::duty_panel (limits rows)",
        status: Status::Fixture,
    },
    Seam {
        id: "limits.max_footprint",
        drives: "the recorded max-footprint limit row",
        source: "provenance.fixed.maxFootprintM2 (130 m2)",
        rule: "printed as recorded beside the fitted tower's own footprintM2; the pass neither selects nor scores anything",
        engine_field: "the engine's own limit record (EngineOutput.validation)",
        code: "drafthouse_cockpit::duty_panel (limits rows)",
        status: Status::Fixture,
    },
    Seam {
        id: "limits.minimum_thermal_margin",
        drives: "the recorded minimum-thermal-margin limit row",
        source: "provenance.fixed.minimumThermalMarginC (0 C)",
        rule: "printed as recorded next to the engine's own solved cold water against the target; the pass recomputes nothing",
        engine_field: "the engine's own limit record (EngineOutput.validation)",
        code: "drafthouse_cockpit::duty_panel (limits rows)",
        status: Status::Fixture,
    },
    Seam {
        id: "limits.nozzle_pressure_drop",
        drives: "the recorded nozzle-pressure-drop limit row",
        source: "rate.inputs.requirements.nozzlePressureDropPa (65000 Pa) - recorded only in that block of fixtures/engine-run.json",
        rule: "printed as recorded with the block it came from named, because the bundled fixture's provenance block does not carry it",
        engine_field: "the engine's own limit record (EngineOutput.validation)",
        code: "drafthouse_cockpit::duty_panel (limits rows)",
        status: Status::Fixture,
    },
];

/// The seam counts by status - used by the panel header, the docs and the tests.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Counts {
    pub engine: usize,
    pub fixture: usize,
    pub illustrative: usize,
    pub definition: usize,
}

impl Counts {
    pub fn total(&self) -> usize {
        self.engine + self.fixture + self.illustrative + self.definition
    }
    /// `4 engine contract · 3 fixture data · 2 illustrative · 1 definition`, for a header line.
    pub fn line(&self) -> String {
        format!(
            "{} engine contract · {} fixture data · {} illustrative · {} definition",
            self.engine, self.fixture, self.illustrative, self.definition
        )
    }
}

/// Count the seams by status.
pub fn status_counts() -> Counts {
    let mut c = Counts::default();
    for s in SEAMS.iter() {
        match s.status {
            Status::Engine => c.engine += 1,
            Status::Fixture => c.fixture += 1,
            Status::Illustrative => c.illustrative += 1,
            Status::Definition => c.definition += 1,
        }
    }
    c
}

/// `VISUAL_DATA_SEAMS.md`, generated from [`SEAMS`] + [`mapping`] so the document cannot drift.
pub fn markdown() -> String {
    let c = status_counts();
    let mut s = String::new();
    s.push_str("# VISUAL_DATA_SEAMS - the Synergy Drafthouse cockpit (Bevy), visual pass\n\n");
    s.push_str("Every visual effect in `cockpit/` binds to data through the table below. The table is **generated** from\n");
    s.push_str("`cockpit/seams/src/lib.rs` (`cockpit/tools/gen-seams.sh`), and the running app renders the same table in its\n");
    s.push_str("**Data seams** panel, so this document cannot drift from the code that draws the scene.\n\n");
    s.push_str("Status legend: **engine contract** = the value already comes from `Engine::run` or the engine's own\n");
    s.push_str("input record; **fixture data** = a catalog/geometry fact or a recorded requirement (data, not physics);\n");
    s.push_str("**illustrative** = no engine field exists yet, so the pass invents a *look* and labels it;\n");
    s.push_str("**definition** = a stated convention or standard relation (a unit conversion, the ISA pressure/altitude\n");
    s.push_str("relation) that is neither an engine result nor fixture data, printed as a definition and never as a\n");
    s.push_str("calculated engineering value.\n\n");
    s.push_str(&format!("{} seams: {}.\n\n", SEAMS.len(), c.line()));
    s.push_str("> ");
    s.push_str(REQUIRED_COPY);
    s.push_str("\n\n**This pass adds no physics.** No CFD, no bypass claim, no CTI/MRL validation or certification claim,\n");
    s.push_str("no money field. The engine is reached through the contract crate in `cockpit/contract` - the\n");
    s.push_str("repository's real engine by default, the approved baseline's recorded `FixtureEngine` replay behind\n");
    s.push_str("the `fixture-engine` feature - one engine, two view layers.\n\n");

    s.push_str("## The seams\n\n");
    s.push_str("| # | seam | visual effect it drives | source today | rule | engine field that replaces it | seam in code | status |\n");
    s.push_str("|---|---|---|---|---|---|---|---|\n");
    for (n, seam) in SEAMS.iter().enumerate() {
        s.push_str(&format!(
            "| {} | `{}` | {} | `{}` | {} | `{}` | `{}` | {} |\n",
            n + 1,
            seam.id,
            seam.drives,
            seam.source,
            seam.rule,
            seam.engine_field,
            seam.code,
            seam.status.label(),
        ));
    }

    s.push_str("\n## The illustrative mapping (the numbers this pass invents)\n\n");
    s.push_str("Everything below is a *visual* rule. None of it is an engineering result, and none of it is presented as\n");
    s.push_str(
        "one: the scene says `illustrative` next to the flow map and the spray coverage.\n\n",
    );
    s.push_str("| constant | value | what it turns (fake) into what (seen) | why it is needed |\n");
    s.push_str("|---|---|---|---|\n");
    s.push_str(&format!(
        "| `ANCHOR_AIRFLOW_M3_S` | {:.2} m3/s | airflow -> chevron animation speed | the recorded run's airflow is the 1x reference for the pass |\n",
        mapping::ANCHOR_AIRFLOW_M3_S
    ));
    s.push_str(&format!(
        "| `SPRAY_BASE_HALF_ANGLE_DEG` | {:.0} deg | nozzle orifice -> spray cone half-angle | the nozzle records carry an orifice and a discharge coefficient, no spray angle |\n",
        mapping::SPRAY_BASE_HALF_ANGLE_DEG
    ));
    s.push_str(&format!(
        "| `SPRAY_HALF_ANGLE_PER_MM_DEG` | {:.2} deg/mm | orifice -> cone width | same - the cone model is not implemented |\n",
        mapping::SPRAY_HALF_ANGLE_PER_MM_DEG
    ));
    s.push_str(&format!(
        "| `WATER_RAMP_REFERENCE_C` | {:.0} C | the run's hot-cold spread -> how much of the warm end the falling water shows | the walk between the engine's two temperatures is a look; 8 C is the spread at which it reads as a full warm-to-cool fall |\n",
        mapping::WATER_RAMP_REFERENCE_C
    ));
    s.push_str(&format!(
        "| `MIN_BAND_PX` | {:.0} px | a 0.45 m layer -> a visible band | a 0.45 m layer is ~15 px on a 900 px screen at true scale |\n",
        mapping::MIN_BAND_PX
    ));
    s.push_str(&format!(
        "| `SPRAY_STAGGER_PITCH_FACTOR` | 1/sqrt(2) = {:.4} | a staggered bank -> its effective nearest-neighbour pitch | a staggered grid holds more nozzles per unit area; equal-area equivalent is pitch/sqrt(2) |\n",
        std::f64::consts::FRAC_1_SQRT_2
    ));
    s.push_str(&format!(
        "| `MIN_CHEVRONS` / `MAX_CHEVRONS` | {}, {} | airflow -> chevron count | arrow density has to stay readable, not proportional |\n",
        mapping::MIN_CHEVRONS, mapping::MAX_CHEVRONS
    ));
    s.push_str(&format!(
        "| `MIN_STREAMLINES` / `MAX_STREAMLINES` | {}, {} | airflow -> streamlines per side (2D) and inside the 3D cut cell | same reason: density stays readable, and both views clamp identically |\n",
        mapping::MIN_STREAMLINES, mapping::MAX_STREAMLINES
    ));
    s.push_str(&format!(
        "| `MIN_WATER_STREAKS` / `MAX_WATER_STREAKS` | {}, {} | water flow -> falling streaks | the streak count is a look; the label carries the engine's own `water_flow_m3_hr` |\n",
        mapping::MIN_WATER_STREAKS, mapping::MAX_WATER_STREAKS
    ));
    s.push_str(&format!(
        "| `CAM_DEFAULT` | yaw {} deg, pitch {} deg, dist {} x row width | the 3D view's opening camera (round 4: behind the `three-d` cargo feature, off by default) | a still evidence frame needs a stated camera; the orbit control moves it from here |\n",
        mapping::CAM_DEFAULT.0 as i64, mapping::CAM_DEFAULT.1 as i64, mapping::CAM_DEFAULT.2
    ));
    s.push_str(&format!(
        "| `STACK_HEIGHT_FACTOR` | {} x stack diameter | stack area -> a drawn stack height | the fan record carries a stack **area** and the tower a recovery factor, not a height |\n",
        mapping::STACK_HEIGHT_FACTOR
    ));
    s.push_str(&format!(
        "| `CUTAWAY_CASING_T_M`, `DRIFT_BANK_T_M`, `BASIN_DEPTH_M`, `CELL_GAP_M` | {} m, {} m, {} m, {} m | the 3D tower's wall, drift-bank, basin and service-lane dimensions | the fixture geometry stops at areas, depths and heights |\n",
        mapping::CUTAWAY_CASING_T_M, mapping::DRIFT_BANK_T_M, mapping::BASIN_DEPTH_M, mapping::CELL_GAP_M
    ));
    s.push_str("\n### Round 4: the defined (not invented) relations\n\n");
    s.push_str("A *definition* is a convention or a standard relation, not a look and not a result. The pass prints these\n");
    s.push_str(
        "with the relation that produced them and never as a calculated engineering value.\n\n",
    );
    s.push_str("| definition | value | what it turns into what | where it is stated |\n");
    s.push_str("|---|---|---|---|\n");
    s.push_str(&format!(
        "| `DUTY_WATER_DENSITY_KG_M3` | {} kg/m3 | duty water flow: kg/s <-> m3/hr (the brief's convention; the engine itself converted at the water's own density) | `mapping::m3_hr_from_kg_s`, `mapping::kg_s_from_m3_hr`; both spellings are printed in the panel |\n",
        mapping::DUTY_WATER_DENSITY_KG_M3 as i64
    ));
    s.push_str(&format!(
        "| ISA standard troposphere | p0 = {} Pa, T0 = {} K, L = {} K/m, exponent {} | barometric pressure <-> site altitude (the fixture records no altitude) | `mapping::altitude_m_from_pressure_pa`, `mapping::pressure_pa_from_altitude_m` |\n",
        mapping::ISA_SEA_LEVEL_PA as i64, mapping::ISA_SEA_LEVEL_K, mapping::ISA_LAPSE_K_PER_M, mapping::ISA_EXPONENT
    ));
    s.push_str(
        "| derived relative humidity | RH = recorded inlet humidity ratio / the fixture's saturation humidity ratio at the entered dry bulb | the dry-bulb / relative-humidity pair: one editable, the other derived | `duty::Psychro::relative_humidity_at`; the panel prints the engine's recorded RH beside it |\n",
    );
    s.push_str(&format!(
        "| `APPROACH_MARGIN_MIN_C` | {} C | the duty panel's own approach pre-check (cold water clears the wet bulb by at least this) | `mapping::APPROACH_MARGIN_MIN_C`; the fixture engine's own rule (`approach > 0`) is printed beside it |\n",
        mapping::APPROACH_MARGIN_MIN_C
    ));
    s.push_str("\nFunctions: `mapping::rpm`, `mapping::rpm_text`, `mapping::ratio_from_rpm`, `mapping::blade_turn_hz`, `mapping::flow_factor`,\n");
    s.push_str("`mapping::chevron_count`, `mapping::spray_half_angle_deg`, `mapping::spray_cone_radius_m`,\n");
    s.push_str(
        "`mapping::coverage_fraction`, `mapping::coverage_width_m`, `mapping::nozzle_count`,\n",
    );
    s.push_str("`mapping::rain_drop_count`, `mapping::op_marker_fraction`, `mapping::rail_segment_fraction`,\n");
    s.push_str(
        "`mapping::streamline_count`, `mapping::water_streak_count`, `mapping::cell_plan_m`,\n",
    );
    s.push_str(
        "`mapping::stack_diameter_m`, `mapping::stack_height_m`, `mapping::cell_row_width_m`,\n",
    );
    s.push_str(
        "`mapping::cell_center_x_m`, `mapping::m3_hr_from_kg_s`, `mapping::kg_s_from_m3_hr`,\n",
    );
    s.push_str("`mapping::altitude_m_from_pressure_pa`, `mapping::pressure_pa_from_altitude_m`.\n");
    s.push_str("All are unit-tested in `seams/src/lib.rs` and `seams/src/mapping.rs` (`cargo test --manifest-path seams/Cargo.toml --lib`).\n\n");

    // Issue #74 (D26): the file model, and the server commands that will sit on top of it when the
    // connect issue lands. The mapping is stated here because this document is generated - a table that
    // drifted from the code would be worse than no table.
    s.push_str("## Saving: one file model, and the server commands behind it (issue #74, D26)\n\n");
    s.push_str("**Owner decision.** Native and web share **one persistence model - file-based first, optional connect\n");
    s.push_str("later**. A project is a `.drafthouse` file (`docs/PROJECT_FORMAT.md`); a catalog revision is an\n");
    s.push_str("immutable file object whose declared `sha256` is checked on **every** read; the custom parts a user\n");
    s.push_str("authors live in that file, not in a page's memory. **Nothing in `cockpit/` talks to a network.**\n\n");
    s.push_str("The internal host's stub commands map onto the file model like this - each row names what the\n");
    s.push_str("command would carry once a server exists, and what the cockpit already writes locally:\n\n");
    s.push_str("| `ServerCommand` | what it maps to in the file model |\n");
    s.push_str("|---|---|\n");
    s.push_str("| `SaveRevision { project, note }` | **push the project** (`File > Save` on native, *download project* on the internal host) to the server as a new revision, with the note as its label |\n");
    s.push_str("| `ExportReportPdf { project }` | the server's own **report pipeline**, taking the project file as its input (the cockpit prints no PDF) |\n");
    s.push_str("| `ExportJson { project }` / `ExportCsv { project }` | the results export this cockpit writes **locally** today (`File > Export results…`), from the same snapshot the project carries |\n");
    s.push_str("| `LoadCatalogRevision { revision }` | the revision **id** a project pins (`catalogRevisionId`); the file itself is imported through `Revision::read`, digest checked |\n");
    s.push_str("| `CompareLater { project, against_revision }` | a project file plus that pinned revision id - enough to recompute the comparison later |\n\n");
    s.push_str("Every row is a label on a stub: no request is made, and the public host neither draws these commands\n");
    s.push_str(
        "nor answers the file commands at all (it can author, and it cannot save or export).\n\n",
    );

    s.push_str("## What the pass does not claim\n\n");
    s.push_str("- Animated arrows are a **flow map illustration**, not a CFD or network solution, and the scene says so on\n");
    s.push_str("  the frame: `illustrative flow map`.\n");
    s.push_str("- Spray coverage is a flat-area overlap of two adjacent cones - **not** a nozzle distribution model.\n");
    s.push_str("  It is labelled `illustrative` in place.\n");
    s.push_str("- The rpm read-out is the record's own rated speed (`nominalRpm`) times the speed ratio - **not** a\n  fan rating, and `-` when the record states no rated speed.\n");
    s.push_str("- A **custom part** is the user's own data, checked only against the ranges the bundled catalog records.\n");
    s.push_str("  The pass validates its *shape and range*, never its engineering adequacy; the fixture engine then\n");
    s.push_str("  interpolates its tables exactly as it does for a catalog record, and refuses it by the same rules.\n");
    s.push_str("- A duty outside the fixture's recorded sweep prints `out of fixture range` - **never** an extrapolated\n");
    s.push_str("  number, and never a silently clamped one.\n");
    s.push_str("- The relative humidity and the site altitude are **definitions** (the recorded-table ratio and the ISA\n");
    s.push_str("  relation), printed with the relation and never as engine output.\n");
    s.push_str("- The pass reads the same `EngineOutput` the baseline result screen reads. Nothing in `cockpit/` computes a\n");
    s.push_str("  cooling-tower quantity; the scene draws whatever engine the build selected produced, with\n");
    s.push_str("  no change to the view layer (that is what the seams are for).\n");
    s.push_str("- **Known gap (issue #71): the HTML mirror has no native equivalent.** On the web the live text a\n");
    s.push_str("  screen reader announces is written into `index.html` every frame by `cockpit/src/bridge.rs`\n");
    s.push_str("  (`#mirror-*` and the `data-*` runtime markers). That module compiles on wasm32 only, so the\n");
    s.push_str("  native desktop binary has **no announced-text surface yet**; the follow-up is native AccessKit.\n");
    s.push_str(
        "  Keyboard navigation is identical on both hosts - the shortcuts are the app's own.\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_seam_is_filled_in_and_unique() {
        let mut ids: Vec<&str> = SEAMS.iter().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "seam ids must be unique");
        for s in SEAMS.iter() {
            assert!(s.id.contains('.'), "{}: id is area.effect", s.id);
            assert!(s.drives.len() > 12, "{}: drives is a sentence", s.id);
            assert!(
                s.source.len() > 8,
                "{}: source names where the value comes from",
                s.id
            );
            assert!(
                s.rule.len() > 8,
                "{}: rule says what turns the value into pixels",
                s.id
            );
            assert!(
                s.engine_field.len() > 4,
                "{}: names the replacing engine field",
                s.id
            );
            assert!(
                s.code.contains("::"),
                "{}: code points at file::symbol",
                s.id
            );
        }
    }

    #[test]
    fn the_seven_required_seams_exist() {
        // The brief names these seven binding points; each must have a seam of its own.
        for id in [
            "fan.speed_ratio",
            "fan.curve",
            "fill.layer_geometry",
            "fan.airflow",
            "pressure.zone",
            "spray.cones",
            "spray.coverage",
        ] {
            assert!(SEAMS.iter().any(|s| s.id == id), "missing seam {id}");
        }
    }

    #[test]
    fn round_fours_own_seams_exist() {
        // Change 2: a seam for the custom record. Change 3: one for the parameter card. Change 4: one per
        // duty & site field the brief names, plus the evidence gate that decides what may be printed.
        for id in [
            "parts.custom_record",
            "parts.parameter_card",
            "duty.water_flow",
            "duty.hot_water",
            "duty.target_cold_water",
            "duty.range",
            "duty.approach",
            "duty.wet_bulb",
            "duty.dry_bulb",
            "duty.relative_humidity",
            "duty.barometric_pressure",
            "duty.site_altitude",
            "duty.evidence_range",
            "water.quality_class",
            "water.salinity",
            "water.cycles_of_concentration",
            "water.tds_chloride_ph",
            "limits.max_drift",
            "limits.max_electrical_input",
            "limits.max_footprint",
            "limits.minimum_thermal_margin",
            "limits.nozzle_pressure_drop",
        ] {
            assert!(SEAMS.iter().any(|s| s.id == id), "missing seam {id}");
        }
    }

    #[test]
    fn the_markdown_carries_the_required_copy_and_every_id() {
        let md = markdown();
        assert!(
            md.contains(REQUIRED_COPY),
            "the generated doc must carry the required copy"
        );
        assert!(md.contains("adds no physics"));
        assert!(md.contains("no money field"));
        assert!(md.contains("out of fixture range"));
        assert!(md.contains("ServerCommand::SaveCustomPart"));
        for s in SEAMS.iter() {
            assert!(md.contains(s.id), "{} missing from the generated doc", s.id);
            assert!(
                md.contains(s.engine_field),
                "{} engine field missing from the doc",
                s.id
            );
        }
    }

    #[test]
    fn the_counts_match_the_table() {
        let c = status_counts();
        assert_eq!(c.total(), SEAMS.len());
        assert_eq!(
            c.illustrative, 2,
            "exactly two illustrative seams: the spray cones and the coverage"
        );
        assert!(
            c.definition >= 4,
            "the round-4 definitions are stated as definitions: {}",
            c.line()
        );
        assert!(c.engine > c.illustrative && c.fixture > 0);
    }
}
