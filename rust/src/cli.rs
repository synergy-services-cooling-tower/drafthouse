//! The engine's JSON command surface, shared by the `ct-engine` binary and the wasm export.
//!
//! `run` turns one command line into one JSON object; `Failure` splits the two ways a
//! request can be refused, and each carries the exit status the binary reports for it,
//! because the parity harness and the wasm binding both key off that status rather than off
//! a message.
//!
//! The parity harness (`scripts/parity/run.mjs`) drives this over the recorded
//! inputs and diffs its JSON against the JavaScript reference engine; with `--engine wasm`
//! it drives the same commands through the wasm build instead of the binary. JSON keys use
//! the JavaScript names on purpose: the harness compares the two engine outputs field by
//! field, so the names should be the ones both sides already agree on.
//!
//! Exit status of `ct-engine`, the binary over this module:
//!
//! * `0` — one JSON object on stdout.
//! * `1` — the engine refused an out-of-domain request. `{"error":"DomainError","message":…}`
//!   on stderr; stdout stays empty.
//! * `2` — usage error (unknown command, missing or unparsable option).

use crate::{
    layered_system_pressure_breakdown, AirflowEstimateInput, CapabilityCondition,
    CapabilityUncertainty, CatalogMetadata, CharacteristicCapabilityInput,
    ColdWaterTemperatureInput, ConditionField, CrossflowGridInput, CrossflowStudyInput,
    DriftCurvePoint, DriftEliminatorRecord, FanCurvePoint, FanPressureBasis, FanRecord,
    FanSystemIntersectionInput, FillLayer, FillOperatingEnvelopeInput, FillPressureCorrelation,
    FillRecord, FillStack, InletEnthalpyConvention, Integration, LayeredBreakdown,
    LayeredBreakdownInput, MerkelInput, MonteCarloInput, NaturalDraftInput, NozzleArrangement,
    NozzleArrangementInput, NozzleRecord, Objective, PerformanceCurveRecord, PsychrometricOptions,
    PsychrometricState, PsychrometricStateInput, RecordKind, SelectionCandidate, SelectionCatalog,
    SelectionDriftEliminator, SelectionFan, SelectionFill, SelectionInput, SelectionRequirements,
    SelectionTower, SystemPressureBreakdownInput, TowerRecord, TowerType, WaterBalanceInput,
    WaterQualityFactor, ZoneCorrelation,
};
use std::collections::{HashMap, HashSet};

pub const USAGE: &str = "usage: ct-engine <command> [options]

commands:
  psy      --db <C> (--wb <C> | --rh <0..1>) [--p <Pa>] [--no-enhancement]
  merkel   --hot <C> --cold <C> --wb <C> --db <C> --lg <kg/kg>
           [--p <Pa>] [--salinity <g/kg>] [--integration simpson|chebyshev4]
           [--segments <n>] [--convention bulk|cti-saturated-wetbulb]
  inverse  --hot <C> --wb <C> --db <C> --lg <kg/kg> --kavl <KaV/L>
           [--p <Pa>] [--salinity <g/kg>] [--integration simpson|chebyshev4]
           [--convention bulk|cti-saturated-wetbulb]
  airside  --tower <spec> --fill-thermal <spec> --fill-pressure <spec> --fill-limits <spec>
           [--fill-quality <a,b>] --drift-curve <v:ppm:pa,...> --fill-depth <m>
           --flow <m3/s> --water <kg/s> --dry-air-density <kg/m3> --moist-air-density <kg/m3>
           [--spray <spec>] [--rain <spec>] [--fan-curve <q:pa:eff,...>]
           [--fan-stack-area <m2>] [--fan-basis total|static] [--fan-ref-density <kg/m3>]
           [--thermal-multiplier <x>] [--pressure-multiplier <x>]
  layers   --tower <spec> --fills <rec;...> --fill-stack <layer[+layer...]>
           --drift-curve <v:ppm:pa,...> --flow <m3/s> --water <kg/s>
           --dry-air-density <kg/m3> --moist-air-density <kg/m3>
           [--spray <spec>] [--rain <spec>] [--fan-curve <q:pa:eff,...>]
           [--fan-stack-area <m2>] [--fan-basis total|static] [--fan-ref-density <kg/m3>]
           [--hot <C>] [--quality-class <name>]
           [--thermal-multiplier <x>] [--pressure-multiplier <x>]
  fan      --fan-curve <q:pa:eff,...> --air-density <kg/m3> [--speed-ratio <r>]
           [--min-flow-fraction <f>] [--fan-ref-density <kg/m3>]
           (--system-a <Pa> --system-b <Pa/(m3/s)^2> | --system-breakdown + the airside records)
  envelope --fill-limits <spec> --quality <class> --water-loading <x>
           --dry-air-loading <x> --hot-water <C>
  estimate --ref-flow <m3/s> --ref-power <kW> --actual-power <kW>
           --ref-density <kg/m3> --actual-density <kg/m3>
  motor    --shaft-power <kW> [--drive-efficiency <x>] [--service-factor <x>]
  evaporation --dry-air-mass-flow <kg/s> --inlet-humidity-ratio <kg/kg>
           --outlet-humidity-ratio <kg/kg>
  drift    --circulating-water <kg/s> --drift-ppm <ppm>
  balance  --evaporation <kg/s> [--drift <kg/s>] [--cycles <n>]
  capability --design <condition> --test <condition> [--exponent <-x>] [--salinity <g/kg>]
           [--integration simpson|chebyshev4] [--curve-points <n>]
           [--convention bulk|cti-saturated-wetbulb]
  capability-mc --design <condition> --test <condition> [--exponent <-x>] [--salinity <g/kg>]
           [--integration simpson|chebyshev4] [--curve-points <n>]
           [--convention bulk|cti-saturated-wetbulb]
           [--sigma-design <sigmas>] [--sigma-test <sigmas>] [--sigma-exponent <x>]
           [--sigma-salinity <x>] [--samples <n>] [--seed <n>]
  natural-draft --tower <spec> --draft-height <m> --fill-thermal <spec> --fill-pressure <spec>
           --fill-limits <spec> [--fill-quality <a,b>] --drift-curve <v:ppm:pa,...>
           --fill-depth <m> --hot <C> --db <C> --wb <C> --water <kg/s> [--p <Pa>]
           [--salinity <g/kg>] [--thermal-multiplier <x>] [--pressure-multiplier <x>]
           [--min-face-velocity <m/s>] [--max-face-velocity <m/s>]
  curve-bounds --records <rec;…>
  curve-predict --records <rec;…> --wb <C> --range <C> --water <kg/s>
  curve-flow --records <rec;…> --wb <C> --range <C> --cold <C>
  curve-capability --records <rec;…> --wb <C> --range <C> --cold <C> --adjusted-flow <kg/s>
  select   --towers <rec;...> --fills <rec;...> --drift-eliminators <rec;...>
           --fans <rec;...> --nozzles <rec;...> --quality-factors <name:thermal:pressure;...>
           [--catalog-id <id>] [--catalog-revision <rev>] [--catalog-status <status>]
           [--objective least-over-capacity|lowest-electrical-input|lowest-makeup-water|lowest-total-air-side-pressure]
           [--max-results <n>] [--all-orders]
           [--water <kg/s>] [--hot <C>] [--target-cold <C>] [--wb <C>] [--db <C>] [--p <Pa>]
           [--salinity <g/kg>] [--quality-class <name>] [--cycles <n>] [--max-drift-ppm <ppm>]
           [--max-power <kW>] [--max-footprint <m2>] [--min-margin <C>] [--nozzle-dp <Pa>]
           [--speed-ratios <r|r|r>]
  crossflow --hot <C> --db <C> --wb <C> --water <kg/s> --dry-air <kg/s> --kavl <KaV/L>
           [--p <Pa>] [--salinity <g/kg>] [--cells <n>] [--water-cells <n>] [--no-richardson]
  convergence --hot <C> --db <C> --wb <C> --water <kg/s> --dry-air <kg/s> --kavl <KaV/L>
           [--p <Pa>] [--salinity <g/kg>] [--base-cells <n>] [--safety-factor <x>]
  nozzle   --flow <m3/s> --dp <Pa> --nozzles <rec;...> [--count-range <lo|hi>]
  nozzle-flow --discharge-coefficient <x> --orifice-diameter <m> --dp <Pa> [--density <kg/m3>]
  validate-catalog --towers <rec;...> --fills <rec;...> --drift-eliminators <rec;...>
           --fans <rec;...> --nozzles <rec;...> [--catalog-id <id>]
           [--catalog-revision <rev>] [--catalog-status <status>]
  select-fields   the record field names `select` accepts, per catalog list

specs are `key:value` pairs joined with commas, using the JavaScript catalog field names,
e.g. --tower fillAreaM2:64,inletAreaM2:32,sprayZoneHeightM:0.6. The `select` catalog is a
`;`-separated list of such records, and a list-valued field (fillDepthOptionsM, fillStacks,
compatibleFanIds, allowedSpeedRatio, curve points, the five-field zone correlations) spells
its entries with `|`, e.g. --towers id:IDCF-064,type:counterflow,...,fillDepthOptionsM:1.2|1.5.

A fill STACK is an ordered list of layers, top first, spelled
<fillId>@<depthM>[@<thermalMultiplier>[@<pressureMultiplier>]] with its layers joined with
`+` — e.g. `FILM-OF25@0.45+FILM-CF19@0.9`. `--fill-stack` takes one such stack; the `select`
tower field `fillStacks` takes one or more stack VARIANT (`|`-separated), and a stack's own
name in a result is exactly this spelling. A tower that declares `fillStacks` is selected over
those variants — complete layer stacks, each with its own fill ids, depths and multipliers —
instead of over `fillDepthOptionsM`.
A `condition` is `waterMassFlowKgS:200,dryAirMassFlowKgS:160,hotWaterC:42,coldWaterC:32,`
`wetBulbC:27,dryBulbC:33` with an optional `,pressurePa:101325`; a `sigmas` spec uses the same
field names and carries the standard uncertainty of each one it perturbs. A performance-curve
`rec` is `wetBulbC:24,rangeC:8,waterFlowKgS:120,coldWaterC:26.1`; records are `;`-separated.
The selector carries no economics field of any kind: passing one is a usage error, not a
silently ignored value. Before anything is selected, `select` and `validate-catalog` check
every record against the machine schema in `rust/src/validate.rs` (docs/CATALOG_SCHEMA.md
as code): a missing required field, a field name the record type does not declare, a
non-finite number or a value outside its declared domain is a usage error that names the
record and the field, and `validate-catalog` reports the record counts instead of running
the selector.";

/// A request that cannot be computed, split by what the caller should do about it.
///
/// The variant carries the exit status the binary reports for it, so a caller that is not
/// the binary (the wasm export) can hand the same status back without a second mapping.
#[derive(Debug)]
pub enum Failure {
    /// The command line itself is wrong.
    Usage(String),
    /// The physics refuses this input.
    Domain(crate::DomainError),
}

impl From<crate::DomainError> for Failure {
    fn from(error: crate::DomainError) -> Self {
        Failure::Domain(error)
    }
}

pub fn run(args: &[String]) -> Result<String, Failure> {
    let (command, cli) = Cli::parse(args)?;
    match command.as_str() {
        "psy" => psy(&cli),
        "merkel" => merkel(&cli),
        "inverse" => inverse(&cli),
        "airside" => airside(&cli),
        "layers" => layers(&cli),
        "fan" => fan(&cli),
        "envelope" => envelope(&cli),
        "estimate" => estimate(&cli),
        "motor" => motor(&cli),
        "evaporation" => evaporation(&cli),
        "drift" => drift(&cli),
        "balance" => balance(&cli),
        "capability" => capability(&cli),
        "capability-mc" => capability_monte_carlo(&cli),
        "natural-draft" => natural_draft(&cli),
        "curve-bounds" => curve_bounds(&cli),
        "curve-predict" => curve_predict(&cli),
        "curve-flow" => curve_flow(&cli),
        "curve-capability" => curve_capability(&cli),
        "select" => select(&cli),
        "crossflow" => crossflow(&cli),
        "convergence" => convergence(&cli),
        "nozzle" => nozzle(&cli),
        "nozzle-flow" => nozzle_flow(&cli),
        "validate-catalog" => validate_catalog_command(&cli),
        "select-fields" => Ok(select_fields_object()),
        other => Err(Failure::Usage(format!("unknown command: {other}"))),
    }
}

/// Run one command and wrap the outcome as the reply envelope a caller that is not a
/// process reads — the wasm export (`rust/src/wasm.rs`, driven by `rust/wasm/binding.mjs`).
/// The value is the same JSON the binary prints; a refusal becomes the `status`/`error`
/// pair the binary reports through its exit status and stderr, so both surfaces agree on
/// what "refused" means.
pub fn reply(args: &[String]) -> String {
    match run(args) {
        Ok(value) => format!("{{\"ok\":true,\"value\":{value}}}"),
        Err(Failure::Usage(message)) => format!(
            "{{\"ok\":false,\"status\":2,\"error\":{{\"error\":\"usage\",\"message\":{}}}}}",
            json_string(&message)
        ),
        Err(Failure::Domain(error)) => format!(
            "{{\"ok\":false,\"status\":1,\"error\":{{\"error\":\"DomainError\",\"message\":{}}}}}",
            json_string(error.message())
        ),
    }
}

/// `--key value` options plus bare `--switches`.
struct Cli {
    values: HashMap<String, String>,
    switches: HashSet<String>,
}

impl Cli {
    fn parse(args: &[String]) -> Result<(String, Self), Failure> {
        let mut values = HashMap::new();
        let mut switches = HashSet::new();
        let mut command = None;
        let mut index = 0;
        while index < args.len() {
            let argument = &args[index];
            if let Some(key) = argument.strip_prefix("--") {
                let next = args.get(index + 1);
                match next {
                    Some(value) if !value.starts_with("--") => {
                        values.insert(format!("--{key}"), value.clone());
                        index += 2;
                    }
                    _ => {
                        switches.insert(format!("--{key}"));
                        index += 1;
                    }
                }
            } else if command.is_none() {
                command = Some(argument.clone());
                index += 1;
            } else {
                return Err(Failure::Usage(format!("unexpected argument: {argument}")));
            }
        }
        match command {
            Some(command) => Ok((command, Self { values, switches })),
            None => Err(Failure::Usage("no command given".to_string())),
        }
    }

    fn has(&self, name: &str) -> bool {
        self.switches.contains(name)
    }

    fn number(&self, name: &str) -> Result<Option<f64>, Failure> {
        match self.values.get(name) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<f64>()
                .map(Some)
                .map_err(|_| Failure::Usage(format!("{name} expects a number, got {raw:?}"))),
        }
    }

    fn integer(&self, name: &str, default: usize) -> Result<usize, Failure> {
        match self.values.get(name) {
            None => Ok(default),
            Some(raw) => raw
                .parse::<usize>()
                .map_err(|_| Failure::Usage(format!("{name} expects an integer, got {raw:?}"))),
        }
    }

    fn required_number(&self, name: &str) -> Result<f64, Failure> {
        self.number(name)?
            .ok_or_else(|| Failure::Usage(format!("missing required option {name}")))
    }

    fn number_or(&self, name: &str, default: f64) -> Result<f64, Failure> {
        Ok(self.number(name)?.unwrap_or(default))
    }

    fn integration(&self) -> Result<Integration, Failure> {
        self.integration_default(Integration::Simpson)
    }

    /// The `--integration` option with a caller-chosen default: `merkel.js`'s callers default
    /// to `simpson`, but the capability projection's own default is `chebyshev4`, and the
    /// harness compares both engines under whichever default the reference declares.
    fn integration_default(&self, default: Integration) -> Result<Integration, Failure> {
        match self.values.get("--integration").map(String::as_str) {
            None => Ok(default),
            Some("simpson") => Ok(Integration::Simpson),
            Some("chebyshev4") => Ok(Integration::Chebyshev4),
            Some(other) => Err(Failure::Usage(format!(
                "--integration expects simpson or chebyshev4, got {other:?}"
            ))),
        }
    }

    fn convention(&self) -> Result<InletEnthalpyConvention, Failure> {
        match self.values.get("--convention").map(String::as_str) {
            None | Some("bulk") => Ok(InletEnthalpyConvention::Bulk),
            Some("cti-saturated-wetbulb") => Ok(InletEnthalpyConvention::CtiSaturatedWetBulb),
            Some(other) => Err(Failure::Usage(format!(
                "--convention expects bulk or cti-saturated-wetbulb, got {other:?}"
            ))),
        }
    }
}

fn psy(cli: &Cli) -> Result<String, Failure> {
    let dry_bulb_c = cli.required_number("--db")?;
    let pressure_pa = cli.number_or("--p", 101_325.0)?;
    let enhancement_factor = !cli.has("--no-enhancement");
    let input = match (cli.number("--wb")?, cli.number("--rh")?) {
        (Some(wet_bulb_c), _) => PsychrometricStateInput::from_wet_bulb(dry_bulb_c, wet_bulb_c),
        (None, Some(relative_humidity)) => {
            PsychrometricStateInput::from_relative_humidity(dry_bulb_c, relative_humidity)
        }
        (None, None) => return Err(Failure::Usage("psy needs --wb or --rh".to_string())),
    }
    .with_pressure(pressure_pa)
    .with_enhancement_factor(enhancement_factor);

    let state = crate::psychrometric_state(input)?;
    let options = PsychrometricOptions { enhancement_factor };
    Ok(object(&[
        ("dryBulbC", number(state.dry_bulb_c)),
        ("wetBulbC", number(state.wet_bulb_c)),
        ("relativeHumidity", number(state.relative_humidity)),
        ("pressurePa", number(state.pressure_pa)),
        ("humidityRatio", number(state.humidity_ratio)),
        ("enthalpyKJkgDryAir", number(state.enthalpy_kj_kg_dry_air)),
        ("dryAirDensityKgM3", number(state.dry_air_density_kg_m3)),
        ("moistAirDensityKgM3", number(state.moist_air_density_kg_m3)),
        ("dewPointC", number(state.dew_point_c)),
        ("enhancementFactor", number(state.enhancement_factor)),
        (
            "saturationVaporPressurePa",
            number(crate::saturation_vapor_pressure_pa(state.dry_bulb_c)?),
        ),
        (
            "saturationHumidityRatio",
            number(crate::saturation_humidity_ratio(
                state.dry_bulb_c,
                state.pressure_pa,
                options,
            )?),
        ),
        (
            "saturatedAirEnthalpyKJkgDryAir",
            number(crate::saturated_air_enthalpy_kj_kg_dry_air(
                state.dry_bulb_c,
                state.pressure_pa,
                options,
            )?),
        ),
    ]))
}

fn merkel(cli: &Cli) -> Result<String, Failure> {
    let input = merkel_input(cli)?;
    let result = crate::merkel_demand(&input)?;
    Ok(object(&[
        ("merkelNumber", number(result.merkel_number)),
        (
            "inletAirEnthalpyKJkgDryAir",
            number(result.inlet_air_enthalpy_kj_kg_dry_air),
        ),
        ("cpWaterKJkgK", number(result.cp_water_kj_kg_k)),
        (
            "inletAirState",
            object(&[
                (
                    "humidityRatio",
                    number(result.inlet_air_state.humidity_ratio),
                ),
                (
                    "relativeHumidity",
                    number(result.inlet_air_state.relative_humidity),
                ),
                ("wetBulbC", number(result.inlet_air_state.wet_bulb_c)),
            ]),
        ),
    ]))
}

fn inverse(cli: &Cli) -> Result<String, Failure> {
    let input = ColdWaterTemperatureInput {
        hot_water_c: cli.required_number("--hot")?,
        wet_bulb_c: cli.required_number("--wb")?,
        dry_bulb_c: cli.required_number("--db")?,
        water_to_dry_air_ratio: cli.required_number("--lg")?,
        available_merkel_number: cli.required_number("--kavl")?,
        pressure_pa: cli.number_or("--p", 101_325.0)?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        integration: cli.integration()?,
        inlet_enthalpy_convention: cli.convention()?,
    };
    let result = crate::solve_cold_water_temperature(&input)?;
    Ok(object(&[
        ("coldWaterC", number(result.cold_water_c)),
        ("approachC", number(result.approach_c)),
        ("rangeC", number(result.range_c)),
        (
            "requiredMerkelNumber",
            number(result.required_merkel_number),
        ),
        ("cpWaterKJkgK", number(result.cp_water_kj_kg_k)),
    ]))
}

fn airside(cli: &Cli) -> Result<String, Failure> {
    let tower = tower_record(cli)?;
    let fill = fill_record(cli)?;
    let drift_eliminator = drift_record(cli)?;
    let fan = fan_record(cli)?;
    let input = SystemPressureBreakdownInput {
        tower: &tower,
        fill: &fill,
        fill_depth_m: cli.required_number("--fill-depth")?,
        drift_eliminator: &drift_eliminator,
        fan: fan.as_ref(),
        volumetric_air_flow_m3_s: cli.required_number("--flow")?,
        dry_air_density_kg_m3: cli.required_number("--dry-air-density")?,
        moist_air_density_kg_m3: cli.required_number("--moist-air-density")?,
        water_mass_flow_kg_s: cli.required_number("--water")?,
        thermal_multiplier: cli.number_or("--thermal-multiplier", 1.0)?,
        pressure_multiplier: cli.number_or("--pressure-multiplier", 1.0)?,
    };
    let result = crate::system_pressure_breakdown(input)?;
    Ok(airside_object(&result))
}

/// Every field the air-side breakdown reports, nested areas included — shared by the
/// `airside` command and the selector's per-candidate output.
fn airside_object(result: &crate::SystemPressureBreakdown) -> String {
    let areas = result.areas;
    object(&[
        ("totalPa", number(result.total_pa)),
        ("fillPa", number(result.fill_pa)),
        ("driftPa", number(result.drift_pa)),
        ("inletPa", number(result.inlet_pa)),
        ("distributionPa", number(result.distribution_pa)),
        ("supportPa", number(result.support_pa)),
        ("plenumPa", number(result.plenum_pa)),
        ("fanStackPa", number(result.fan_stack_pa)),
        ("fixedPa", number(result.fixed_pa)),
        ("driftPpm", number(result.drift_ppm)),
        ("waterLoadingKgM2S", number(result.water_loading_kg_m2_s)),
        ("dryAirLoadingKgM2S", number(result.dry_air_loading_kg_m2_s)),
        ("dryAirMassFlowKgS", number(result.dry_air_mass_flow_kg_s)),
        ("fillVelocityMS", number(result.fill_velocity_ms)),
        ("driftVelocityMS", number(result.drift_velocity_ms)),
        ("inletVelocityMS", number(result.inlet_velocity_ms)),
        ("plenumVelocityMS", number(result.plenum_velocity_ms)),
        (
            "fanStackVelocityMS",
            optional_number(result.fan_stack_velocity_ms),
        ),
        (
            "fanPressureBasis",
            json_string(result.fan_pressure_basis.as_str()),
        ),
        (
            "areas",
            object(&[
                ("fillAreaM2", number(areas.fill_area_m2)),
                ("airFreeAreaM2", number(areas.air_free_area_m2)),
                ("driftAreaM2", number(areas.drift_area_m2)),
                ("inletAreaM2", number(areas.inlet_area_m2)),
                ("plenumAreaM2", number(areas.plenum_area_m2)),
                ("fanStackAreaM2", optional_number(areas.fan_stack_area_m2)),
            ]),
        ),
        ("fillMerkelNumber", number(result.fill_merkel_number)),
        (
            "sprayZoneMerkelNumber",
            number(result.spray_zone_merkel_number),
        ),
        (
            "rainZoneMerkelNumber",
            number(result.rain_zone_merkel_number),
        ),
        (
            "availableMerkelNumber",
            number(result.available_merkel_number),
        ),
    ])
}

/// The layer spelling: `<fillId>@<depthM>[@<thermalMultiplier>[@<pressureMultiplier>]]`, with
/// layers joined with `+` in physical order (top first). This is the same spelling a stack's
/// own `label()` prints and the one the `select` tower field `fillStacks` carries, so a stack
/// read out of a result parses back into the stack that produced it.
fn fill_stack(option: &str, raw: &str) -> Result<FillStack, Failure> {
    let mut layers = Vec::new();
    for entry in raw.split('+') {
        let (fill_id, rest) = entry.split_once('@').ok_or_else(|| {
            Failure::Usage(format!(
                "{option} expects fillId@depthM[@thermalMultiplier[@pressureMultiplier]] joined with +, got {entry:?}"
            ))
        })?;
        if fill_id.is_empty() {
            return Err(Failure::Usage(format!(
                "{option} needs a fill id before @, got {entry:?}"
            )));
        }
        let mut parts = rest.split('@');
        let depth = parts.next().expect("split yields at least one part");
        let thermal = parts.next();
        let pressure = parts.next();
        if depth.is_empty() || parts.next().is_some() {
            return Err(Failure::Usage(format!(
                "{option} expects fillId@depthM[@thermalMultiplier[@pressureMultiplier]] joined with +, got {entry:?}"
            )));
        }
        let mut layer = FillLayer::new(fill_id, parse_number(depth, option)?);
        if let Some(raw) = thermal {
            layer.thermal_multiplier = parse_number(raw, option)?;
        }
        if let Some(raw) = pressure {
            layer.pressure_multiplier = parse_number(raw, option)?;
        }
        layers.push(layer);
    }
    Ok(FillStack::new(layers))
}

/// The `--fills` records `layers` resolves a stack against: the same spec encoding `select`
/// reads (a fill record authored for one is a fill record for the other), with the
/// tower-type compatibility `select` needs and a rating flow has no use for.
fn rate_fill_records(cli: &Cli) -> Result<Vec<FillRecord>, Failure> {
    record_list(cli, "--fills")?
        .iter()
        .map(rate_fill_record)
        .collect()
}

fn rate_fill_record(spec: &Spec) -> Result<FillRecord, Failure> {
    spec.ensure_known(&SELECT_FILL_FIELDS)?;
    fill_physics(spec)
}

/// `layers` — the **rating** flow: one selected stack, resolved in physical order against the
/// `--fills` records, with a result per layer and the stack's combined totals.
///
/// Every layer is checked against its own fill's limits at this operating point (the loadings
/// are the stack's — layers sit in series), so a layer outside them is a refusal naming the
/// layer and the limit; `--hot` and `--quality-class` add the two dimensions the caller may
/// declare. Nothing is clamped into range.
fn layers(cli: &Cli) -> Result<String, Failure> {
    let tower = tower_record(cli)?;
    let fills = rate_fill_records(cli)?;
    let raw = cli
        .values
        .get("--fill-stack")
        .ok_or_else(|| Failure::Usage("layers needs --fill-stack".to_string()))?;
    let stack = fill_stack("--fill-stack", raw)?;
    let terms = crate::resolve_fill_layers(&stack, &fills)?;
    let drift_eliminator = drift_record(cli)?;
    let fan = fan_record(cli)?;
    let result = layered_system_pressure_breakdown(LayeredBreakdownInput {
        tower: &tower,
        layers: &terms,
        drift_eliminator: &drift_eliminator,
        fan: fan.as_ref(),
        volumetric_air_flow_m3_s: cli.required_number("--flow")?,
        dry_air_density_kg_m3: cli.required_number("--dry-air-density")?,
        moist_air_density_kg_m3: cli.required_number("--moist-air-density")?,
        water_mass_flow_kg_s: cli.required_number("--water")?,
        thermal_multiplier: cli.number_or("--thermal-multiplier", 1.0)?,
        pressure_multiplier: cli.number_or("--pressure-multiplier", 1.0)?,
    })?;
    let failures = crate::check_stack_operating_envelope(
        &terms,
        result.breakdown.water_loading_kg_m2_s,
        result.breakdown.dry_air_loading_kg_m2_s,
        cli.number("--hot")?,
        cli.values.get("--quality-class").map(String::as_str),
    );
    if !failures.is_empty() {
        return Err(Failure::Domain(crate::DomainError::new(
            failures.join("\n"),
        )));
    }
    Ok(layers_object(&stack, &result))
}

/// The `layers` reply: the stack's own name and total depth, one result per layer in physical
/// order, the layers' combined totals, and the air-side breakdown `airside` reports (its fill
/// terms are the stack's totals). Data only — nothing here renders.
fn layers_object(stack: &FillStack, result: &LayeredBreakdown) -> String {
    object(&[
        ("stackLabel", json_string(&stack.label())),
        ("totalDepthM", number(stack.total_depth_m())),
        (
            "layers",
            json_array(
                &result
                    .fill_layers
                    .iter()
                    .map(layer_object)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "layerTotals",
            object(&[
                ("pressureDropPa", number(result.breakdown.fill_pa)),
                ("merkelNumber", number(result.breakdown.fill_merkel_number)),
                (
                    "availableMerkelNumber",
                    number(result.breakdown.available_merkel_number),
                ),
            ]),
        ),
        ("airside", airside_object(&result.breakdown)),
    ])
}

/// One layer's result, field for field: its identity and depth, the characteristic and
/// multipliers it was computed from, the limits it was checked against, what it sees
/// (airflow, loadings, velocity) and what it contributes (drop, transfer number) — plus the
/// running totals through it, whose last value is the stack total.
fn layer_object(layer: &crate::FillLayerResult) -> String {
    object(&[
        ("position", layer.position.to_string()),
        ("label", json_string(&layer.label())),
        ("fillId", json_string(&layer.fill_id)),
        ("depthM", number(layer.depth_m)),
        ("thermalMultiplier", number(layer.thermal_multiplier)),
        ("pressureMultiplier", number(layer.pressure_multiplier)),
        (
            "effectiveThermalMultiplier",
            number(layer.effective_thermal_multiplier),
        ),
        (
            "effectivePressureMultiplier",
            number(layer.effective_pressure_multiplier),
        ),
        ("thermal", zone_object(&layer.thermal)),
        ("pressure", pressure_object(&layer.pressure)),
        ("limits", limits_object(&layer.limits)),
        (
            "volumetricAirFlowM3S",
            number(layer.volumetric_air_flow_m3_s),
        ),
        ("dryAirMassFlowKgS", number(layer.dry_air_mass_flow_kg_s)),
        ("waterMassFlowKgS", number(layer.water_mass_flow_kg_s)),
        ("waterLoadingKgM2S", number(layer.water_loading_kg_m2_s)),
        ("dryAirLoadingKgM2S", number(layer.dry_air_loading_kg_m2_s)),
        ("fillVelocityMS", number(layer.fill_velocity_ms)),
        ("pressureDropPa", number(layer.pressure_drop_pa)),
        ("merkelNumber", number(layer.merkel_number)),
        (
            "cumulativePressureDropPa",
            number(layer.cumulative_pressure_drop_pa),
        ),
        (
            "cumulativeMerkelNumber",
            number(layer.cumulative_merkel_number),
        ),
    ])
}

/// One heat/mass-transfer correlation, as the record declares it.
fn zone_object(zone: &ZoneCorrelation) -> String {
    object(&[
        ("coefficientPerM", number(zone.coefficient_per_m)),
        (
            "referenceWaterLoadingKgM2S",
            number(zone.reference_water_loading_kg_m2_s),
        ),
        (
            "referenceDryAirLoadingKgM2S",
            number(zone.reference_dry_air_loading_kg_m2_s),
        ),
        ("waterExponent", number(zone.water_exponent)),
        ("airExponent", number(zone.air_exponent)),
    ])
}

/// One fill pressure-drop correlation, as the record declares it.
fn pressure_object(pressure: &FillPressureCorrelation) -> String {
    object(&[
        ("coefficientPaPerM", number(pressure.coefficient_pa_per_m)),
        (
            "referenceWaterLoadingKgM2S",
            number(pressure.reference_water_loading_kg_m2_s),
        ),
        (
            "referenceDryAirLoadingKgM2S",
            number(pressure.reference_dry_air_loading_kg_m2_s),
        ),
        ("waterExponent", number(pressure.water_exponent)),
        ("airExponent", number(pressure.air_exponent)),
    ])
}

/// A fill's own operating limits, as the record declares them.
fn limits_object(limits: &crate::FillLimits) -> String {
    object(&[
        (
            "minWaterLoadingKgM2S",
            number(limits.min_water_loading_kg_m2_s),
        ),
        (
            "maxWaterLoadingKgM2S",
            number(limits.max_water_loading_kg_m2_s),
        ),
        (
            "minDryAirLoadingKgM2S",
            number(limits.min_dry_air_loading_kg_m2_s),
        ),
        (
            "maxDryAirLoadingKgM2S",
            number(limits.max_dry_air_loading_kg_m2_s),
        ),
        (
            "maxWaterTemperatureC",
            number(limits.max_water_temperature_c),
        ),
    ])
}

fn fan(cli: &Cli) -> Result<String, Failure> {
    let fan = fan_record(cli)?.ok_or_else(|| Failure::Usage("fan needs --curve".to_string()))?;
    let air_density_kg_m3 = cli.required_number("--air-density")?;
    let mut input = FanSystemIntersectionInput::new(&fan, air_density_kg_m3);
    if let Some(speed_ratio) = cli.number("--speed-ratio")? {
        input = input.with_speed_ratio(speed_ratio);
    }
    if let Some(minimum_flow_fraction) = cli.number("--min-flow-fraction")? {
        input = input.with_minimum_flow_fraction(minimum_flow_fraction);
    }
    let limits = crate::fan_operating_limits(&fan, input.speed_ratio)?;
    let system_pressure_fn: Box<dyn Fn(f64) -> Result<f64, crate::DomainError>> =
        if cli.has("--system-breakdown") {
            let tower = tower_record(cli)?;
            let fill = fill_record(cli)?;
            let drift_eliminator = drift_record(cli)?;
            let fill_depth_m = cli.required_number("--fill-depth")?;
            let water_mass_flow_kg_s = cli.required_number("--water")?;
            let dry_air_density_kg_m3 = cli.required_number("--dry-air-density")?;
            let moist_air_density_kg_m3 = cli.required_number("--moist-air-density")?;
            let fan = fan.clone();
            Box::new(move |flow_m3_s: f64| {
                Ok(
                    crate::system_pressure_breakdown(SystemPressureBreakdownInput {
                        tower: &tower,
                        fill: &fill,
                        fill_depth_m,
                        drift_eliminator: &drift_eliminator,
                        fan: Some(&fan),
                        volumetric_air_flow_m3_s: flow_m3_s,
                        dry_air_density_kg_m3,
                        moist_air_density_kg_m3,
                        water_mass_flow_kg_s,
                        thermal_multiplier: 1.0,
                        pressure_multiplier: 1.0,
                    })?
                    .total_pa,
                )
            })
        } else {
            let constant_pa = cli.number_or("--system-a", 0.0)?;
            let quadratic_pa = cli.number_or("--system-b", 0.0)?;
            Box::new(move |flow_m3_s: f64| Ok(constant_pa + quadratic_pa * flow_m3_s.powi(2)))
        };
    let result = crate::solve_fan_system_intersection(&input, &*system_pressure_fn)?;
    Ok(object(&[
        ("minFlowM3S", number(limits.min_flow_m3_s)),
        ("maxFlowM3S", number(limits.max_flow_m3_s)),
        ("flowM3S", number(result.flow_m3_s)),
        ("fanPressurePa", number(result.fan_pressure_pa)),
        ("systemPressurePa", number(result.system_pressure_pa)),
        ("efficiency", number(result.efficiency)),
        ("shaftPowerKW", number(result.shaft_power_kw)),
        ("residualPa", number(result.residual_pa)),
        ("speedRatio", number(result.speed_ratio)),
    ]))
}

fn envelope(cli: &Cli) -> Result<String, Failure> {
    let fill = fill_record(cli)?;
    let water_quality_class = cli
        .values
        .get("--quality")
        .ok_or_else(|| Failure::Usage("envelope needs --quality".to_string()))?;
    let result = crate::check_fill_operating_envelope(FillOperatingEnvelopeInput {
        fill: &fill,
        water_loading_kg_m2_s: cli.required_number("--water-loading")?,
        dry_air_loading_kg_m2_s: cli.required_number("--dry-air-loading")?,
        hot_water_c: cli.required_number("--hot-water")?,
        water_quality_class,
    });
    Ok(object(&[
        ("ok", result.ok.to_string()),
        ("failureCount", result.failures.len().to_string()),
        ("failures", json_string_array(&result.failures)),
        ("qualityClass", json_string(water_quality_class)),
    ]))
}

fn estimate(cli: &Cli) -> Result<String, Failure> {
    let result = crate::estimate_airflow_from_fan_power(AirflowEstimateInput {
        reference_volumetric_flow_m3_s: cli.required_number("--ref-flow")?,
        reference_fan_power_kw: cli.required_number("--ref-power")?,
        actual_fan_power_kw: cli.required_number("--actual-power")?,
        reference_air_density_kg_m3: cli.required_number("--ref-density")?,
        actual_air_density_kg_m3: cli.required_number("--actual-density")?,
    })?;
    Ok(object(&[("airflowM3S", number(result))]))
}

fn motor(cli: &Cli) -> Result<String, Failure> {
    let selection = crate::choose_standard_motor(
        cli.required_number("--shaft-power")?,
        cli.number_or("--drive-efficiency", 0.96)?,
        cli.number_or("--service-factor", 1.1)?,
    )?;
    Ok(object(&[
        (
            "requiredMotorOutputKW",
            number(selection.required_motor_output_kw),
        ),
        (
            "selectedMotorKW",
            optional_number(selection.selected_motor_kw),
        ),
        ("driveEfficiency", number(selection.drive_efficiency)),
        ("serviceFactor", number(selection.service_factor)),
    ]))
}

/* ---------------- water balance ---------------- */

fn evaporation(cli: &Cli) -> Result<String, Failure> {
    let evaporation_kg_s = crate::evaporation_from_air_mass_balance(
        cli.required_number("--dry-air-mass-flow")?,
        cli.required_number("--inlet-humidity-ratio")?,
        cli.required_number("--outlet-humidity-ratio")?,
    )?;
    Ok(object(&[("evaporationKgS", number(evaporation_kg_s))]))
}

fn drift(cli: &Cli) -> Result<String, Failure> {
    let drift_kg_s = crate::drift_loss_kg_s(
        cli.required_number("--circulating-water")?,
        cli.required_number("--drift-ppm")?,
    )?;
    Ok(object(&[("driftKgS", number(drift_kg_s))]))
}

/// The reference's default arguments are applied by leaving `--drift` / `--cycles` out, the
/// way an absent property takes the default in JavaScript.
fn balance(cli: &Cli) -> Result<String, Failure> {
    let mut input = WaterBalanceInput::new(cli.required_number("--evaporation")?);
    if let Some(drift_kg_s) = cli.number("--drift")? {
        input = input.with_drift_kg_s(drift_kg_s);
    }
    if let Some(cycles_of_concentration) = cli.number("--cycles")? {
        input = input.with_cycles_of_concentration(cycles_of_concentration);
    }
    let result = crate::cooling_tower_water_balance(&input)?;
    Ok(object(&[
        ("evaporationKgS", number(result.evaporation_kg_s)),
        ("driftKgS", number(result.drift_kg_s)),
        ("blowdownKgS", number(result.blowdown_kg_s)),
        ("makeupKgS", number(result.makeup_kg_s)),
        (
            "cyclesOfConcentration",
            number(result.cycles_of_concentration),
        ),
    ]))
}

/* ---------------- capability, natural draft and performance curves ---------------- */

/// The fields of a `--design` / `--test` condition spec, in the reference condition object's
/// own field order.
const CONDITION_FIELDS: [&str; 7] = [
    "waterMassFlowKgS",
    "dryAirMassFlowKgS",
    "hotWaterC",
    "coldWaterC",
    "wetBulbC",
    "dryBulbC",
    "pressurePa",
];

/// The reference's own narrative strings, verbatim — the harness compares their text, because
/// a port that quietly rewords a method note is not the same result object.
const METHOD_CAPABILITY: &str = "CTI-style characteristic-curve educational implementation";
const DISCLAIMER_CAPABILITY: &str = "This reproduces the public characteristic-curve concept, not the complete licensed ATC-105 correction, validity, instrumentation, or contractual procedure.";
const MODEL_NATURAL_DRAFT: &str =
    "simplified coupled natural-draft counterflow (demonstration only)";
const CAVEAT_NATURAL_DRAFT: &str = "A production natural-draft model requires vertical/radial density integration, shell loss, wind effects, rain zone, and validated fill-zone correlations.";
const METHOD_CURVE: &str = "Performance-curve rectangular-grid interpolation";
const SIGN_CONVENTION_CURVE: &str =
    "Positive leaving-water deviation means the test water is warmer (worse) than the curve prediction.";
const DISCLAIMER_CURVE: &str = "Automatic CTI crossplot rules and test corrections must be verified against the licensed current ATC-105 and contractual curve set.";

/// One `condition` spec: the six required numbers plus the optional `pressurePa`, every one
/// under the field name the reference reads.
fn capability_condition(cli: &Cli, option: &str) -> Result<CapabilityCondition, Failure> {
    let spec = Spec::parse(cli, option)?;
    spec.ensure_known(&CONDITION_FIELDS)?;
    Ok(CapabilityCondition {
        water_mass_flow_kg_s: spec.required("waterMassFlowKgS")?,
        dry_air_mass_flow_kg_s: spec.required("dryAirMassFlowKgS")?,
        hot_water_c: spec.required("hotWaterC")?,
        cold_water_c: spec.required("coldWaterC")?,
        wet_bulb_c: spec.required("wetBulbC")?,
        dry_bulb_c: spec.required("dryBulbC")?,
        pressure_pa: spec.number("pressurePa")?,
    })
}

/// The field a sigma spec names, under the reference's field name.
fn condition_field(key: &str, option: &str) -> Result<ConditionField, Failure> {
    match key {
        "waterMassFlowKgS" => Ok(ConditionField::WaterMassFlowKgS),
        "dryAirMassFlowKgS" => Ok(ConditionField::DryAirMassFlowKgS),
        "hotWaterC" => Ok(ConditionField::HotWaterC),
        "coldWaterC" => Ok(ConditionField::ColdWaterC),
        "wetBulbC" => Ok(ConditionField::WetBulbC),
        "dryBulbC" => Ok(ConditionField::DryBulbC),
        "pressurePa" => Ok(ConditionField::PressurePa),
        other => Err(Failure::Usage(format!(
            "{option} does not know the field {other:?}"
        ))),
    }
}

/// One `sigmas` spec, in the order the caller wrote it: that order decides which field draws
/// which Gaussian, exactly as the reference's `Object.entries` order does.
fn condition_sigmas(cli: &Cli, option: &str) -> Result<Vec<(ConditionField, f64)>, Failure> {
    let spec = Spec::parse(cli, option)?;
    let mut sigmas = Vec::new();
    for (key, value) in &spec.pairs {
        sigmas.push((condition_field(key, option)?, parse_number(value, option)?));
    }
    Ok(sigmas)
}

/// The `capability` / `capability-mc` inputs: the two conditions and the reference's default
/// arguments (an exponent of −0.6, `chebyshev4`, 80 curve points, the bulk convention).
fn capability_input(cli: &Cli) -> Result<CharacteristicCapabilityInput, Failure> {
    Ok(CharacteristicCapabilityInput {
        design: capability_condition(cli, "--design")?,
        test: capability_condition(cli, "--test")?,
        characteristic_exponent: cli.number_or("--exponent", -0.6)?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        integration: cli.integration_default(Integration::Chebyshev4)?,
        curve_points: cli.integer("--curve-points", 80)?,
        inlet_enthalpy_convention: cli.convention()?,
    })
}

/// A full psychrometric state, in the reference's field names — the states the capability and
/// natural-draft results carry.
fn air_state_object(state: &PsychrometricState) -> String {
    object(&[
        ("dryBulbC", number(state.dry_bulb_c)),
        ("wetBulbC", number(state.wet_bulb_c)),
        ("relativeHumidity", number(state.relative_humidity)),
        ("pressurePa", number(state.pressure_pa)),
        ("humidityRatio", number(state.humidity_ratio)),
        ("enthalpyKJkgDryAir", number(state.enthalpy_kj_kg_dry_air)),
        ("dryAirDensityKgM3", number(state.dry_air_density_kg_m3)),
        ("moistAirDensityKgM3", number(state.moist_air_density_kg_m3)),
        ("dewPointC", number(state.dew_point_c)),
        ("enhancementFactor", number(state.enhancement_factor)),
    ])
}

/// The enthalpy convention's JavaScript spelling.
fn convention_name(convention: InletEnthalpyConvention) -> &'static str {
    match convention {
        InletEnthalpyConvention::Bulk => "bulk",
        InletEnthalpyConvention::CtiSaturatedWetBulb => "cti-saturated-wetbulb",
    }
}

/// The `evaluateCharacteristicCapability` result object, field for field.
fn capability_object(result: &crate::CharacteristicCapability) -> String {
    let curves = result
        .curves
        .iter()
        .map(|point| {
            object(&[
                ("waterToDryAirRatio", number(point.water_to_dry_air_ratio)),
                ("designDemandMerkel", number(point.design_demand_merkel)),
                (
                    "testCharacteristicMerkel",
                    number(point.test_characteristic_merkel),
                ),
            ])
        })
        .collect::<Vec<String>>();
    object(&[
        ("method", json_string(METHOD_CAPABILITY)),
        ("capabilityPct", number(result.capability_pct)),
        (
            "designWaterToDryAirRatio",
            number(result.design_water_to_dry_air_ratio),
        ),
        (
            "testWaterToDryAirRatio",
            number(result.test_water_to_dry_air_ratio),
        ),
        (
            "capabilityWaterToDryAirRatio",
            number(result.capability_water_to_dry_air_ratio),
        ),
        ("testMerkelNumber", number(result.test_merkel_number)),
        (
            "testCharacteristicCoefficient",
            number(result.test_characteristic_coefficient),
        ),
        (
            "characteristicExponent",
            number(result.characteristic_exponent),
        ),
        (
            "inletEnthalpyConvention",
            json_string(convention_name(result.inlet_enthalpy_convention)),
        ),
        ("designAirState", air_state_object(&result.design_air_state)),
        ("testAirState", air_state_object(&result.test_air_state)),
        ("curves", json_array(&curves)),
        ("disclaimer", json_string(DISCLAIMER_CAPABILITY)),
    ])
}

fn capability(cli: &Cli) -> Result<String, Failure> {
    let input = capability_input(cli)?;
    let result = crate::evaluate_characteristic_capability(&input)?;
    Ok(capability_object(&result))
}

/// The `monteCarloCharacteristicCapability` result object, field for field.
fn capability_monte_carlo(cli: &Cli) -> Result<String, Failure> {
    let uncertainty = CapabilityUncertainty {
        design: condition_sigmas(cli, "--sigma-design")?,
        test: condition_sigmas(cli, "--sigma-test")?,
        characteristic_exponent: cli.number("--sigma-exponent")?,
        salinity_g_kg: cli.number("--sigma-salinity")?,
    };
    let input = MonteCarloInput {
        base_input: capability_input(cli)?,
        uncertainty,
        samples: cli.integer("--samples", 1000)?,
        seed: cli.number_or("--seed", 20260813.0)?,
    };
    let result = crate::monte_carlo_characteristic_capability(&input)?;
    Ok(object(&[
        ("samplesRequested", result.samples_requested.to_string()),
        ("samplesAccepted", result.samples_accepted.to_string()),
        ("rejectedSamples", result.rejected_samples.to_string()),
        ("meanCapabilityPct", number(result.mean_capability_pct)),
        (
            "standardDeviationPctPoints",
            number(result.standard_deviation_pct_points),
        ),
        ("p2_5", number(result.p2_5)),
        ("p50", number(result.p50)),
        ("p97_5", number(result.p97_5)),
        (
            "expandedUncertaintyApproxPctPoints",
            number(result.expanded_uncertainty_approx_pct_points),
        ),
        ("seed", number(result.seed)),
    ]))
}

/// The `solveNaturalDraftCounterflow` result object, in the reference's own key names. The
/// draft height is its own option (`--draft-height`): the reference reads it off the tower
/// object, and every other reader of a tower record here takes the air-side fields only.
fn natural_draft(cli: &Cli) -> Result<String, Failure> {
    let tower = tower_record(cli)?;
    let fill = fill_record(cli)?;
    let drift_eliminator = drift_record(cli)?;
    let input = NaturalDraftInput {
        tower: &tower,
        effective_draft_height_m: cli.required_number("--draft-height")?,
        fill: &fill,
        fill_depth_m: cli.required_number("--fill-depth")?,
        drift_eliminator: &drift_eliminator,
        hot_water_c: cli.required_number("--hot")?,
        dry_bulb_c: cli.required_number("--db")?,
        wet_bulb_c: cli.required_number("--wb")?,
        pressure_pa: cli.number_or("--p", 101_325.0)?,
        water_mass_flow_kg_s: cli.required_number("--water")?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        thermal_multiplier: cli.number_or("--thermal-multiplier", 1.0)?,
        pressure_multiplier: cli.number_or("--pressure-multiplier", 1.0)?,
        min_face_velocity_ms: cli.number_or("--min-face-velocity", 0.2)?,
        max_face_velocity_ms: cli.number_or("--max-face-velocity", 6.0)?,
    };
    let result = crate::solve_natural_draft_counterflow(&input)?;
    Ok(object(&[
        ("model", json_string(MODEL_NATURAL_DRAFT)),
        (
            "volumetricAirFlowM3S",
            number(result.volumetric_air_flow_m3_s),
        ),
        ("waterToDryAirRatio", number(result.water_to_dry_air_ratio)),
        ("plumeDensityKgM3", number(result.plume_density_kg_m3)),
        ("draftPressurePa", number(result.draft_pressure_pa)),
        ("residualPa", number(result.residual_pa)),
        ("airside", airside_object(&result.airside)),
        (
            "thermal",
            object(&[
                ("coldWaterC", number(result.thermal.cold_water_c)),
                ("approachC", number(result.thermal.approach_c)),
                ("rangeC", number(result.thermal.range_c)),
                (
                    "requiredMerkelNumber",
                    number(result.thermal.required_merkel_number),
                ),
                ("cpWaterKJkgK", number(result.thermal.cp_water_kj_kg_k)),
            ]),
        ),
        (
            "outlet",
            object(&[
                ("dryBulbC", number(result.outlet.dry_bulb_c)),
                ("wetBulbC", number(result.outlet.wet_bulb_c)),
                ("relativeHumidity", number(result.outlet.relative_humidity)),
                ("humidityRatio", number(result.outlet.humidity_ratio)),
                (
                    "enthalpyKJkgDryAir",
                    number(result.outlet.enthalpy_kj_kg_dry_air),
                ),
                ("pressurePa", number(result.outlet.pressure_pa)),
            ]),
        ),
        ("inletAirState", air_state_object(&result.inlet_air_state)),
        ("evaporationKgS", number(result.evaporation_kg_s)),
        ("caveat", json_string(CAVEAT_NATURAL_DRAFT)),
    ]))
}

/// The fields of a performance-curve record spec.
const CURVE_RECORD_FIELDS: [&str; 4] = ["wetBulbC", "rangeC", "waterFlowKgS", "coldWaterC"];

/// The `--records` list: `;`-separated records of `wetBulbC,rangeC,waterFlowKgS,coldWaterC`.
fn performance_curve_records(cli: &Cli) -> Result<Vec<PerformanceCurveRecord>, Failure> {
    let raw = cli
        .values
        .get("--records")
        .ok_or_else(|| Failure::Usage("a curve command needs --records".to_string()))?;
    let mut records = Vec::new();
    for entry in raw.split(';') {
        let spec = Spec::parse_text("--records", entry)?;
        spec.ensure_known(&CURVE_RECORD_FIELDS)?;
        records.push(PerformanceCurveRecord {
            wet_bulb_c: spec.required("wetBulbC")?,
            range_c: spec.required("rangeC")?,
            water_flow_kg_s: spec.required("waterFlowKgS")?,
            cold_water_c: spec.required("coldWaterC")?,
        });
    }
    Ok(records)
}

/// One `[lo, hi]` axis range, as the reference returns it.
fn range_pair(limits: (f64, f64)) -> String {
    json_array(&[number(limits.0), number(limits.1)])
}

fn curve_bounds(cli: &Cli) -> Result<String, Failure> {
    let bounds = crate::performance_curve_bounds(&performance_curve_records(cli)?)?;
    Ok(object(&[
        ("wetBulbC", range_pair(bounds.wet_bulb_c)),
        ("rangeC", range_pair(bounds.range_c)),
        ("waterFlowKgS", range_pair(bounds.water_flow_kg_s)),
        ("coldWaterC", range_pair(bounds.cold_water_c)),
    ]))
}

fn curve_predict(cli: &Cli) -> Result<String, Failure> {
    let records = performance_curve_records(cli)?;
    let prediction = crate::predict_cold_water_from_performance_curves(
        &records,
        cli.required_number("--wb")?,
        cli.required_number("--range")?,
        cli.required_number("--water")?,
    )?;
    Ok(object(&[
        ("coldWaterC", number(prediction.cold_water_c)),
        (
            "brackets",
            object(&[
                ("wetBulbC", range_pair(prediction.wet_bulb_bracket_c)),
                ("rangeC", range_pair(prediction.range_bracket_c)),
            ]),
        ),
        (
            "extrapolated",
            if prediction.extrapolated {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ]))
}

fn curve_flow(cli: &Cli) -> Result<String, Failure> {
    let records = performance_curve_records(cli)?;
    let prediction = crate::predict_water_flow_from_performance_curves(
        &records,
        cli.required_number("--wb")?,
        cli.required_number("--range")?,
        cli.required_number("--cold")?,
    )?;
    Ok(object(&[
        ("waterFlowKgS", number(prediction.water_flow_kg_s)),
        (
            "predictedColdWaterC",
            number(prediction.predicted_cold_water_c),
        ),
    ]))
}

fn curve_capability(cli: &Cli) -> Result<String, Failure> {
    let records = performance_curve_records(cli)?;
    let result = crate::evaluate_performance_curve_capability(
        &records,
        cli.required_number("--wb")?,
        cli.required_number("--range")?,
        cli.required_number("--cold")?,
        cli.required_number("--adjusted-flow")?,
    )?;
    Ok(object(&[
        ("method", json_string(METHOD_CURVE)),
        (
            "predictedWaterFlowKgS",
            number(result.predicted_water_flow_kg_s),
        ),
        (
            "adjustedTestWaterFlowKgS",
            number(result.adjusted_test_water_flow_kg_s),
        ),
        ("capabilityPct", number(result.capability_pct)),
        (
            "predictedColdWaterAtAdjustedFlowC",
            number(result.predicted_cold_water_at_adjusted_flow_c),
        ),
        (
            "leavingWaterDeviationC",
            number(result.leaving_water_deviation_c),
        ),
        ("signConvention", json_string(SIGN_CONVENTION_CURVE)),
        ("disclaimer", json_string(DISCLAIMER_CURVE)),
    ]))
}

/* ---------------- selection ---------------- */

/// The selector, driven from record specs — its result JSON is the projection the parity
/// harness compares against the reference `selectCoolingTowerComponents`.
fn select(cli: &Cli) -> Result<String, Failure> {
    // Fail-closed: nothing invalid may reach the ranking. The schema gate runs before the
    // records are parsed into their types, so a mistyped, missing or non-finite field is a
    // named refusal rather than an `undefined`/`NaN` that a candidate would rank on.
    validate_catalog_records(cli)?;
    let catalog = selection_catalog(cli)?;
    let objective = match cli.values.get("--objective") {
        None => Objective::default(),
        Some(raw) => Objective::parse(raw).ok_or_else(|| {
            let allowed = Objective::ALL
                .iter()
                .map(|objective| objective.as_str())
                .collect::<Vec<_>>()
                .join("|");
            Failure::Usage(format!("--objective expects {allowed}, got {raw:?}"))
        })?,
    };
    let mut requirements = SelectionRequirements::default();
    if let Some(value) = cli.number("--water")? {
        requirements.water_mass_flow_kg_s = value;
    }
    if let Some(value) = cli.number("--hot")? {
        requirements.hot_water_c = value;
    }
    if let Some(value) = cli.number("--target-cold")? {
        requirements.target_cold_water_c = value;
    }
    if let Some(value) = cli.number("--wb")? {
        requirements.wet_bulb_c = value;
    }
    if let Some(value) = cli.number("--db")? {
        requirements.dry_bulb_c = value;
    }
    if let Some(value) = cli.number("--p")? {
        requirements.pressure_pa = value;
    }
    if let Some(value) = cli.number("--salinity")? {
        requirements.salinity_g_kg = value;
    }
    if let Some(value) = cli.values.get("--quality-class") {
        requirements.water_quality_class = value.clone();
    }
    if let Some(value) = cli.number("--cycles")? {
        requirements.cycles_of_concentration = value;
    }
    if let Some(value) = cli.number("--max-drift-ppm")? {
        requirements.max_drift_ppm = value;
    }
    if let Some(value) = cli.number("--max-power")? {
        requirements.max_electrical_input_kw = value;
    }
    if let Some(value) = cli.number("--max-footprint")? {
        requirements.max_footprint_m2 = value;
    }
    if let Some(value) = cli.number("--min-margin")? {
        requirements.minimum_thermal_margin_c = value;
    }
    if let Some(value) = cli.number("--nozzle-dp")? {
        requirements.nozzle_pressure_drop_pa = value;
    }
    if let Some(raw) = cli.values.get("--speed-ratios") {
        requirements.speed_ratios = raw
            .split('|')
            .map(|part| parse_number(part, "--speed-ratios"))
            .collect::<Result<Vec<f64>, Failure>>()?;
    }
    let max_results = cli.integer("--max-results", 30)?;

    let input = SelectionInput::new(&catalog)
        .with_objective(objective)
        .with_max_results(max_results)
        .with_requirements(requirements);
    let run = crate::run_selection(&input)?;

    let capacities_resolved = run
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.capacity_kg_s.is_some() && candidate.capability_ratio.is_some()
        })
        .count();

    let mut fields: Vec<(&str, String)> = vec![
        ("objective", json_string(run.objective.as_str())),
        ("requirements", requirements_object(&run.requirements)),
        ("catalog", catalog_object(&run.catalog_metadata)),
        (
            "inletAirState",
            inlet_air_state_object(&run.inlet_air_state),
        ),
        (
            "feasibleCandidateCount",
            run.feasible_candidate_count().to_string(),
        ),
        (
            "rejectionSummary",
            rejection_summary_object(&run.rejection_summary),
        ),
        ("capacitiesResolved", capacities_resolved.to_string()),
        ("warning", json_string(&run.warning)),
        (
            "results",
            json_array(
                &run.candidates
                    .iter()
                    .take(max_results)
                    .map(candidate_object)
                    .collect::<Vec<String>>(),
            ),
        ),
    ];
    if cli.has("--all-orders") {
        fields.push((
            "candidates",
            json_array(
                &run.candidates
                    .iter()
                    .map(compact_candidate_object)
                    .collect::<Vec<String>>(),
            ),
        ));
        fields.push(("orders", orders_object(&run)));
    }
    // Issue #24: the data a non-shell caller needs. Both keys are data only — the values
    // the recommended candidate actually used, and its worked sheet in the `worked.js`
    // shape. Nothing here renders, derives geometry or emits markup.
    if let Some(recommended) = run.candidates.first() {
        fields.push(("inputs", selection_inputs_object(cli, &run, recommended)));
        fields.push(("worked", selection_worked_object(&run, recommended)));
    }
    Ok(object(&fields))
}

/* ---------------- the selection's inputs and worked sheet (issue #24) ---------------- */

/// The input values the run used for the recommended candidate: the identity it resolved,
/// the requirements it ranked against, and the catalog records that candidate was built
/// from, dimension fields included. Record fields are echoed from the `key:value` specs the
/// caller passed — this module's parser is their only reader, so the echo is the parsed
/// value, not a second reading of the record.
fn selection_inputs_object(
    cli: &Cli,
    run: &crate::SelectionRun,
    candidate: &SelectionCandidate,
) -> String {
    object(&[
        ("objective", json_string(run.objective.as_str())),
        ("catalog", catalog_object(&run.catalog_metadata)),
        ("requirements", requirements_object(&run.requirements)),
        (
            "selected",
            object(&[
                ("towerId", json_string(&candidate.tower_id)),
                ("fillId", json_string(&candidate.fill_id)),
                (
                    "driftEliminatorId",
                    json_string(&candidate.drift_eliminator_id),
                ),
                ("fanId", json_string(&candidate.fan_id)),
                ("nozzleId", json_string(&candidate.nozzle.nozzle_id)),
                ("fillDepthM", number(candidate.fill_depth_m)),
                ("speedRatio", number(candidate.speed_ratio)),
            ]),
        ),
        (
            "records",
            object(&[
                (
                    "tower",
                    spec_record_object(cli.values.get("--towers"), &candidate.tower_id),
                ),
                (
                    "fill",
                    spec_record_object(cli.values.get("--fills"), &candidate.fill_id),
                ),
                (
                    "driftEliminator",
                    spec_record_object(
                        cli.values.get("--drift-eliminators"),
                        &candidate.drift_eliminator_id,
                    ),
                ),
                (
                    "fan",
                    spec_record_object(cli.values.get("--fans"), &candidate.fan_id),
                ),
                (
                    "nozzle",
                    spec_record_object(cli.values.get("--nozzles"), &candidate.nozzle.nozzle_id),
                ),
            ]),
        ),
    ])
}

/// One record out of a `;`-separated record list, matched on its `id` field, as a JSON
/// object of the values it carried — or `null` when the list does not hold that id (a
/// record the list omitted, which the selector cannot then have used).
fn spec_record_object(list: Option<&String>, id: &str) -> String {
    let Some(list) = list else {
        return "null".to_string();
    };
    for record in list.split(';') {
        let pairs = record
            .split(',')
            .filter_map(|pair| pair.split_once(':'))
            .collect::<Vec<(&str, &str)>>();
        if pairs
            .iter()
            .any(|(key, value)| *key == "id" && *value == id)
        {
            return object(
                &pairs
                    .iter()
                    .map(|(key, value)| (*key, spec_value(value)))
                    .collect::<Vec<(&str, String)>>(),
            );
        }
    }
    "null".to_string()
}

/// A spec field value at its own type: `1.2` is a number, `a|b` a list, `1:2:3|4:5:6` a
/// list of lists (a curve), anything else a string.
fn spec_value(raw: &str) -> String {
    if raw.contains('|') {
        json_array(&raw.split('|').map(spec_value).collect::<Vec<String>>())
    } else if raw.contains(':') {
        json_array(&raw.split(':').map(spec_value).collect::<Vec<String>>())
    } else if let Ok(value) = raw.parse::<f64>() {
        if value.is_finite() {
            number(value)
        } else {
            json_string(raw)
        }
    } else {
        json_string(raw)
    }
}

/// Where the per-layer steps go in the worked sheet: after the four setup steps (duty flow,
/// range, heat, dry-air mass flow) and before the fill's own total, so the sheet reads
/// stack → fill total → zones → demand.
const FILL_LAYER_STEP_POSITION: usize = 4;

/// One worked step per layer of the candidate's fill stack, top first.
///
/// The sheet names each layer by fill id and depth — the same identity the `fillLayers` block
/// carries — and shows the characteristic it was evaluated with, the loadings every layer of
/// the stack sees and the multiplier that reached it. The value is the layer's own transfer
/// number, read from the computed layer result rather than re-derived.
fn fill_layer_steps(candidate: &SelectionCandidate) -> Vec<String> {
    let count = candidate.fill_layers.len();
    candidate
        .fill_layers
        .iter()
        .map(|layer| {
            worked_step(
                &format!(
                    "Fill layer {} of {} — {} @ {} m",
                    layer.position,
                    count,
                    layer.fill_id,
                    fixed(layer.depth_m, 4)
                ),
                "One layer of the stack, in physical order. The layers sit in series in the air \
                 path and in the water path, so every layer is evaluated at the same water and \
                 dry-air loadings; this layer contributes its own fill characteristic over its \
                 own depth.",
                "KaV/L_layer = c * (L''/L''ref)^a * (G''/G''ref)^b * depth * multiplier",
                &format!(
                    "L'' {} vs {}, G'' {} vs {}, depth {} m, multiplier {}",
                    fixed(layer.water_loading_kg_m2_s, 4),
                    fixed(layer.thermal.reference_water_loading_kg_m2_s, 4),
                    fixed(layer.dry_air_loading_kg_m2_s, 4),
                    fixed(layer.thermal.reference_dry_air_loading_kg_m2_s, 4),
                    fixed(layer.depth_m, 4),
                    fixed(layer.effective_thermal_multiplier, 4)
                ),
                number(layer.merkel_number),
                "-",
                Some("Merkel (1925); ASHRAE Systems and Equipment, Cooling Towers"),
            )
        })
        .collect()
}

/// The recommended candidate's worked sheet, in the `worked.js` shape: `title`, `purpose`,
/// `steps` (each `label` / `why` / `formula` / `substitution` / `value` / `unit` /
/// `reference` / `kind`) and the `result` the steps resolve to. Every `value` is read from
/// the selection result the engine already computed; the sheet re-derives nothing, exactly
/// as `worked.js` calls the same core functions rather than restating their physics.
fn selection_worked_object(run: &crate::SelectionRun, candidate: &SelectionCandidate) -> String {
    let requirements = &run.requirements;
    let inlet = &run.inlet_air_state;
    let thermal = &candidate.thermal;
    let airside = &candidate.airside;
    let fan = &candidate.fan_operating_point;
    let balance = &candidate.water_balance;
    let range_c = requirements.hot_water_c - requirements.target_cold_water_c;
    let total_merkel = airside.fill_merkel_number
        + airside.spray_zone_merkel_number
        + airside.rain_zone_merkel_number;

    let mut steps: Vec<String> = vec![
        worked_step(
            "Duty water flow",
            "The flow the unit has to hold at the required cold-water temperature. It is a \
             constraint on every candidate, never a sort key.",
            "Q_w = duty flow",
            &format!(
                "{} kg/s (given)",
                fixed(requirements.water_mass_flow_kg_s, 4)
            ),
            number(requirements.water_mass_flow_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Range",
            "How far the water is cooled, which fixes the heat it gives up per kilogram.",
            "range = t_hot - t_cold,target",
            &format!(
                "{} - {}",
                fixed(requirements.hot_water_c, 4),
                fixed(requirements.target_cold_water_c, 4)
            ),
            number(range_c),
            "K",
            None,
        ),
        worked_step(
            "Heat the water gives up",
            "The water-side duty the air side has to match, at the water's own heat capacity \
             at the mean temperature.",
            "Q = Q_w * c_p,w * range",
            &format!(
                "{} * {} * {}",
                fixed(requirements.water_mass_flow_kg_s, 4),
                fixed(thermal.cp_water_kj_kg_k, 4),
                fixed(range_c, 4)
            ),
            number(thermal.heat_transfer_kw),
            "kW",
            None,
        ),
        worked_step(
            "Dry-air mass flow",
            "The air the fill actually passes at this duty; the air-side quantities below are \
             all set by it.",
            "G = rho_da * A_fill * v_fill",
            &format!(
                "{} * {} * {}",
                fixed(inlet.dry_air_density_kg_m3, 4),
                fixed(airside.areas.fill_area_m2, 4),
                fixed(airside.fill_velocity_ms, 4)
            ),
            number(airside.dry_air_mass_flow_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Fill transfer demand",
            "The transfer number the fill alone has to deliver over the range, integrated \
             against the air's saturation line.",
            "KaV/L_fill = integral c_p,w dt / (h_s(t) - h_a)",
            "evaluated over the fill height at the solved water path",
            number(airside.fill_merkel_number),
            "-",
            Some("Merkel (1925); ASHRAE Systems and Equipment, Cooling Towers"),
        ),
        worked_step(
            "Spray and rain zone demand",
            "The zones above and below the fill transfer heat too, on the same correlation \
             form with their own loadings.",
            "KaV/L_zones = KaV/L_spray + KaV/L_rain",
            &format!(
                "{} + {}",
                fixed(airside.spray_zone_merkel_number, 4),
                fixed(airside.rain_zone_merkel_number, 4)
            ),
            number(airside.spray_zone_merkel_number + airside.rain_zone_merkel_number),
            "-",
            None,
        ),
        worked_step(
            "Total demand",
            "Fill plus zones: what the tower must supply to cool the water to the solved \
             cold-water temperature.",
            "KaV/L_req = KaV/L_fill + KaV/L_spray + KaV/L_rain",
            &format!(
                "{} + {} + {}",
                fixed(airside.fill_merkel_number, 4),
                fixed(airside.spray_zone_merkel_number, 4),
                fixed(airside.rain_zone_merkel_number, 4)
            ),
            number(total_merkel),
            "-",
            None,
        ),
        worked_step(
            "Available transfer",
            "What this fill at this depth, this water loading and this speed ratio can supply.",
            "KaV/L_avail = C * (L/G)^m",
            &format!(
                "at L/G = {}, fill depth {} m",
                fixed(
                    requirements.water_mass_flow_kg_s / airside.dry_air_mass_flow_kg_s,
                    4
                ),
                fixed(candidate.fill_depth_m, 4)
            ),
            number(airside.available_merkel_number),
            "-",
            None,
        ),
        worked_step(
            "Capability ratio",
            "Available over required at the duty, before the margin is taken: above 1 the \
             unit has headroom, below 1 it misses the target.",
            "capability = KaV/L_avail,duty / KaV/L_req,duty",
            "solved at the target cold-water temperature, not at the solved one",
            optional_number(candidate.capability_ratio),
            "-",
            None,
        ),
        worked_step(
            "Cold-water temperature, solved",
            "The temperature the tower actually reaches: the available transfer is matched \
             against the demand by root solve.",
            "solve KaV/L_avail(t) = KaV/L_req(t)",
            &format!(
                "target {}, margin {}",
                fixed(requirements.target_cold_water_c, 4),
                fixed(candidate.thermal_margin_c, 4)
            ),
            number(thermal.cold_water_c),
            "degC",
            None,
        ),
        worked_step(
            "Air-side pressure demand",
            "Every loss the fan has to overcome, summed over the path.",
            "dp = fill + drift + inlet + distribution + support + plenum + fan stack + fixed",
            &format!(
                "{} + {} + {} + {} + {} + {} + {} + {}",
                fixed(airside.fill_pa, 4),
                fixed(airside.drift_pa, 4),
                fixed(airside.inlet_pa, 4),
                fixed(airside.distribution_pa, 4),
                fixed(airside.support_pa, 4),
                fixed(airside.plenum_pa, 4),
                fixed(airside.fan_stack_pa, 4),
                fixed(airside.fixed_pa, 4)
            ),
            number(airside.total_pa),
            "Pa",
            None,
        ),
        worked_step(
            "Fan operating point",
            "Where the fan curve and the system curve cross; the flow and pressure the fan \
             actually delivers.",
            "dp_fan(Q) = dp_system(Q)",
            &format!(
                "fan {} Pa vs system {} Pa at {} m3/s",
                fixed(fan.fan_pressure_pa, 4),
                fixed(fan.system_pressure_pa, 4),
                fixed(fan.flow_m3_s, 4)
            ),
            number(fan.flow_m3_s),
            "m3/s",
            None,
        ),
        worked_step(
            "Fan shaft power",
            "The shaft power at that operating point and the fan's efficiency there.",
            "P_shaft = Q * dp_fan / eta_fan",
            &format!(
                "{} * {} / {}",
                fixed(fan.flow_m3_s, 4),
                fixed(fan.fan_pressure_pa, 4),
                fixed(fan.efficiency, 4)
            ),
            number(fan.shaft_power_kw),
            "kW",
            Some("AMCA 210 fan total vs static pressure definitions"),
        ),
        worked_step(
            "Evaporation",
            "Water the air carries away, from the dry-air mass balance between inlet and \
             leaving humidity ratio.",
            "E = G * (W_out - W_in)",
            &format!(
                "{} * ({} - {})",
                fixed(airside.dry_air_mass_flow_kg_s, 4),
                fixed(thermal.outlet_air_state.humidity_ratio, 6),
                fixed(inlet.humidity_ratio, 6)
            ),
            number(balance.evaporation_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Make-up water",
            "Evaporation concentrated by the cycles of concentration, plus the drift that \
             leaves with the air.",
            "M = E * N/(N-1) + D",
            &format!(
                "{} * {}/({} - 1) + {}",
                fixed(balance.evaporation_kg_s, 4),
                fixed(requirements.cycles_of_concentration, 4),
                fixed(requirements.cycles_of_concentration, 4),
                fixed(balance.drift_kg_s, 6)
            ),
            number(balance.makeup_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Capacity",
            "The flow the unit holds at the required cold-water temperature, re-solving the \
             air side at each flow: the ranking metric of the default objective.",
            "Q_w,max = root on Q_w at t_cold(Q_w) = target",
            "bracketed root on the water mass flow",
            optional_number(candidate.capacity_kg_s),
            "kg/s",
            None,
        ),
        worked_note(
            "Ranking",
            "The candidate above is rank 1 under the run's declared objective. Duty flow and \
             the thermal margin constrain the feasible set; they are never sort keys.",
            None,
        ),
        worked_note(
            "Where these numbers come from",
            "Every value in this sheet is read from the selection result itself - the same \
             solve that ranked the candidate - so the sheet cannot drift from the engine.",
            None,
        ),
    ];
    // The stack's layers, immediately before the fill's own total: the sheet names every layer
    // by fill id and depth, in the same top-first order the `fillLayers` block reports, so the
    // worked steps and the data-only surface identify the same stack.
    steps.splice(
        FILL_LAYER_STEP_POSITION..FILL_LAYER_STEP_POSITION,
        fill_layer_steps(candidate),
    );
    let steps = json_array(&steps);

    object(&[
        ("title", json_string("Selection of the recommended unit")),
        (
            "purpose",
            json_string(
                "One duty in, one ranked unit out: the numbers the recommendation rests on, \
                 in the order they were computed.",
            ),
        ),
        ("steps", steps),
        (
            "result",
            object(&[
                ("rank", candidate.rank.to_string()),
                ("towerId", json_string(&candidate.tower_id)),
                ("fillId", json_string(&candidate.fill_id)),
                (
                    "driftEliminatorId",
                    json_string(&candidate.drift_eliminator_id),
                ),
                ("fanId", json_string(&candidate.fan_id)),
                ("nozzleId", json_string(&candidate.nozzle.nozzle_id)),
                ("fillDepthM", number(candidate.fill_depth_m)),
                ("speedRatio", number(candidate.speed_ratio)),
                ("coldWaterC", number(thermal.cold_water_c)),
                ("thermalMarginC", number(candidate.thermal_margin_c)),
                ("rangeC", number(thermal.range_c)),
                ("heatTransferKW", number(thermal.heat_transfer_kw)),
                ("electricalInputKW", number(candidate.electrical_input_kw)),
                (
                    "motorRequiredKW",
                    number(candidate.motor.required_motor_output_kw),
                ),
                (
                    "motorSelectedKW",
                    optional_number(candidate.motor.selected_motor_kw),
                ),
                ("capacityKgS", optional_number(candidate.capacity_kg_s)),
                (
                    "capabilityRatio",
                    optional_number(candidate.capability_ratio),
                ),
                ("totalPa", number(airside.total_pa)),
                ("fanFlowM3S", number(fan.flow_m3_s)),
                ("fanPressurePa", number(fan.fan_pressure_pa)),
                ("fanShaftPowerKW", number(fan.shaft_power_kw)),
                ("evaporationKgS", number(balance.evaporation_kg_s)),
                ("makeupKgS", number(balance.makeup_kg_s)),
            ]),
        ),
    ])
}

/// One `worked.js` step: the label, why it is computed, the formula, the numbers
/// substituted into it, the value, its unit, an optional source reference, and the kind.
#[allow(clippy::too_many_arguments)]
fn worked_step(
    label: &str,
    why: &str,
    formula: &str,
    substitution: &str,
    value: String,
    unit: &str,
    reference: Option<&str>,
) -> String {
    object(&[
        ("label", json_string(label)),
        ("why", json_string(why)),
        ("formula", json_string(formula)),
        ("substitution", json_string(substitution)),
        ("value", value),
        ("unit", json_string(unit)),
        (
            "reference",
            reference
                .map(json_string)
                .unwrap_or_else(|| "null".to_string()),
        ),
        ("kind", json_string("calc")),
    ])
}

/// A `worked.js` note: narrative with no value of its own.
fn worked_note(label: &str, why: &str, reference: Option<&str>) -> String {
    object(&[
        ("label", json_string(label)),
        ("why", json_string(why)),
        ("formula", "null".to_string()),
        ("substitution", "null".to_string()),
        ("value", "null".to_string()),
        ("unit", json_string("")),
        (
            "reference",
            reference
                .map(json_string)
                .unwrap_or_else(|| "null".to_string()),
        ),
        ("kind", json_string("note")),
    ])
}

/// A fixed-precision display of a quantity inside a step's `substitution` text — the job
/// `Number.prototype.toFixed` does in `worked.js`.
fn fixed(value: f64, digits: usize) -> String {
    format!("{value:.digits$}")
}

/// The record field names `select` accepts, per catalog list — the same constants its
/// record readers enforce, so a caller that builds record specs (the wasm binding) projects
/// a catalog against the engine's own intake instead of a mirrored list. A field the
/// selector does not read is a usage error, which is why the projection has to be exact.
fn select_fields_object() -> String {
    fn names(fields: &[&str]) -> String {
        json_array(
            &fields
                .iter()
                .map(|field| json_string(field))
                .collect::<Vec<String>>(),
        )
    }
    object(&[
        ("towers", names(&SELECT_TOWER_FIELDS)),
        ("fills", names(&SELECT_FILL_FIELDS)),
        ("driftEliminators", names(&SELECT_DRIFT_FIELDS)),
        ("fans", names(&SELECT_FAN_FIELDS)),
        ("nozzles", names(&SELECT_NOZZLE_FIELDS)),
    ])
}

fn catalog_object(metadata: &CatalogMetadata) -> String {
    object(&[
        ("id", json_string(&metadata.id)),
        ("revision", json_string(&metadata.revision)),
        ("status", json_string(&metadata.status)),
    ])
}

fn inlet_air_state_object(state: &PsychrometricState) -> String {
    object(&[
        ("dryBulbC", number(state.dry_bulb_c)),
        ("wetBulbC", number(state.wet_bulb_c)),
        ("humidityRatio", number(state.humidity_ratio)),
        ("enthalpyKJkgDryAir", number(state.enthalpy_kj_kg_dry_air)),
        ("dryAirDensityKgM3", number(state.dry_air_density_kg_m3)),
        ("moistAirDensityKgM3", number(state.moist_air_density_kg_m3)),
    ])
}

/// The resolved requirements, in the reference's field names minus the inputs that only fed
/// its economics (see `rust/README.md`).
fn requirements_object(requirements: &SelectionRequirements) -> String {
    let speed_ratios = requirements
        .speed_ratios
        .iter()
        .map(|speed_ratio| number(*speed_ratio))
        .collect::<Vec<String>>();
    object(&[
        (
            "waterMassFlowKgS",
            number(requirements.water_mass_flow_kg_s),
        ),
        ("hotWaterC", number(requirements.hot_water_c)),
        ("targetColdWaterC", number(requirements.target_cold_water_c)),
        ("wetBulbC", number(requirements.wet_bulb_c)),
        ("dryBulbC", number(requirements.dry_bulb_c)),
        ("pressurePa", number(requirements.pressure_pa)),
        ("salinityGKg", number(requirements.salinity_g_kg)),
        (
            "waterQualityClass",
            json_string(&requirements.water_quality_class),
        ),
        (
            "cyclesOfConcentration",
            number(requirements.cycles_of_concentration),
        ),
        ("maxDriftPpm", number(requirements.max_drift_ppm)),
        (
            "maxElectricalInputKW",
            number(requirements.max_electrical_input_kw),
        ),
        ("maxFootprintM2", number(requirements.max_footprint_m2)),
        (
            "minimumThermalMarginC",
            number(requirements.minimum_thermal_margin_c),
        ),
        (
            "nozzlePressureDropPa",
            number(requirements.nozzle_pressure_drop_pa),
        ),
        ("speedRatios", json_array(&speed_ratios)),
    ])
}

fn rejection_summary_object(summary: &[(String, usize)]) -> String {
    let pairs = summary
        .iter()
        .map(|(label, count)| (label.as_str(), count.to_string()))
        .collect::<Vec<(&str, String)>>();
    object(&pairs)
}

/// One feasible candidate, every physics field included.
fn candidate_object(candidate: &SelectionCandidate) -> String {
    let thermal = &candidate.thermal;
    let fan = &candidate.fan_operating_point;
    let balance = &candidate.water_balance;
    let nozzle = &candidate.nozzle;
    object(&[
        ("rank", candidate.rank.to_string()),
        ("towerId", json_string(&candidate.tower_id)),
        ("fillId", json_string(&candidate.fill_id)),
        ("driftId", json_string(&candidate.drift_eliminator_id)),
        ("fanId", json_string(&candidate.fan_id)),
        ("fillDepthM", number(candidate.fill_depth_m)),
        (
            "fillLayers",
            json_array(
                &candidate
                    .fill_layers
                    .iter()
                    .map(layer_object)
                    .collect::<Vec<String>>(),
            ),
        ),
        ("speedRatio", number(candidate.speed_ratio)),
        ("model", json_string(thermal.model)),
        ("coldWaterC", number(thermal.cold_water_c)),
        ("thermalMarginC", number(candidate.thermal_margin_c)),
        ("approachC", number(thermal.approach_c)),
        ("rangeC", number(thermal.range_c)),
        ("cpWaterKJkgK", number(thermal.cp_water_kj_kg_k)),
        (
            "outletAirHumidityRatio",
            number(thermal.outlet_air_state.humidity_ratio),
        ),
        ("heatTransferKW", number(thermal.heat_transfer_kw)),
        ("electricalInputKW", number(candidate.electrical_input_kw)),
        (
            "motorRequiredKW",
            number(candidate.motor.required_motor_output_kw),
        ),
        (
            "motorSelectedKW",
            optional_number(candidate.motor.selected_motor_kw),
        ),
        ("capacityKgS", optional_number(candidate.capacity_kg_s)),
        (
            "capabilityRatio",
            optional_number(candidate.capability_ratio),
        ),
        (
            "waterVolumetricFlowM3S",
            number(candidate.water_volumetric_flow_m3_s),
        ),
        (
            "nozzleOptionCount",
            candidate.nozzle_option_count.to_string(),
        ),
        ("airside", airside_object(&candidate.airside)),
        (
            "fanOperatingPoint",
            object(&[
                ("flowM3S", number(fan.flow_m3_s)),
                ("fanPressurePa", number(fan.fan_pressure_pa)),
                ("systemPressurePa", number(fan.system_pressure_pa)),
                ("efficiency", number(fan.efficiency)),
                ("shaftPowerKW", number(fan.shaft_power_kw)),
                ("residualPa", number(fan.residual_pa)),
            ]),
        ),
        (
            "waterBalance",
            object(&[
                ("evaporationKgS", number(balance.evaporation_kg_s)),
                ("driftKgS", number(balance.drift_kg_s)),
                ("blowdownKgS", number(balance.blowdown_kg_s)),
                ("makeupKgS", number(balance.makeup_kg_s)),
            ]),
        ),
        ("nozzle", nozzle_arrangement_object(nozzle)),
    ])
}

/// One nozzle arrangement, field for field.
fn nozzle_arrangement_object(nozzle: &NozzleArrangement) -> String {
    object(&[
        ("nozzleId", json_string(&nozzle.nozzle_id)),
        ("nozzleName", json_string(&nozzle.nozzle_name)),
        ("count", number(nozzle.count)),
        ("flowPerNozzleM3S", number(nozzle.flow_per_nozzle_m3_s)),
        ("actualTotalFlowM3S", number(nozzle.actual_total_flow_m3_s)),
        ("excessFlowPct", number(nozzle.excess_flow_pct)),
        ("validCount", nozzle.valid_count.to_string()),
    ])
}

/// One feasible candidate's identity and ranking metrics — the `candidates` table the
/// `--all-orders` switch emits so the harness can check every objective's order at once.
fn compact_candidate_object(candidate: &SelectionCandidate) -> String {
    object(&[
        ("towerId", json_string(&candidate.tower_id)),
        ("fillId", json_string(&candidate.fill_id)),
        ("driftId", json_string(&candidate.drift_eliminator_id)),
        ("fanId", json_string(&candidate.fan_id)),
        ("fillDepthM", number(candidate.fill_depth_m)),
        ("speedRatio", number(candidate.speed_ratio)),
        ("capacityKgS", optional_number(candidate.capacity_kg_s)),
        (
            "capabilityRatio",
            optional_number(candidate.capability_ratio),
        ),
        ("electricalInputKW", number(candidate.electrical_input_kw)),
        ("makeupKgS", number(candidate.water_balance.makeup_kg_s)),
        ("totalPa", number(candidate.airside.total_pa)),
        ("coldWaterC", number(candidate.thermal.cold_water_c)),
        ("thermalMarginC", number(candidate.thermal_margin_c)),
    ])
}

fn candidate_identity(candidate: &SelectionCandidate) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        candidate.tower_id,
        candidate.fill_id,
        candidate.drift_eliminator_id,
        candidate.fan_id,
        number(candidate.fill_depth_m),
        number(candidate.speed_ratio),
    )
}

/// The whole feasible set's order under each objective, as identity strings.
fn orders_object(run: &crate::SelectionRun) -> String {
    let entries = Objective::ALL
        .iter()
        .map(|objective| {
            let identities = run
                .order_for(*objective)
                .iter()
                .map(|index| json_string(&candidate_identity(&run.candidates[*index])))
                .collect::<Vec<String>>();
            format!(
                "{}:{}",
                json_string(objective.as_str()),
                json_array(&identities)
            )
        })
        .collect::<Vec<String>>();
    format!("{{{}}}", entries.join(","))
}

/* ---------------- crossflow, convergence and nozzles ---------------- */

/// The crossflow grid input every `crossflow` / `convergence` invocation shares.
fn crossflow_input(cli: &Cli) -> Result<CrossflowGridInput, Failure> {
    let mut input = CrossflowGridInput::new(
        cli.required_number("--hot")?,
        cli.required_number("--db")?,
        cli.required_number("--wb")?,
        cli.required_number("--water")?,
        cli.required_number("--dry-air")?,
        cli.required_number("--kavl")?,
    );
    if let Some(pressure_pa) = cli.number("--p")? {
        input = input.with_pressure(pressure_pa);
    }
    if let Some(salinity_g_kg) = cli.number("--salinity")? {
        input = input.with_salinity(salinity_g_kg);
    }
    let air_cells = cli.integer("--cells", input.air_cells)?;
    let water_cells = cli.integer("--water-cells", air_cells)?;
    input = input.with_cells(air_cells, water_cells);
    if cli.has("--no-richardson") {
        input = input.with_richardson(false);
    }
    Ok(input)
}

fn crossflow(cli: &Cli) -> Result<String, Failure> {
    let result = crate::solve_crossflow_grid(&crossflow_input(cli)?)?;
    Ok(object(&[
        ("model", json_string(result.model())),
        ("coldWaterC", number(result.cold_water_c)),
        ("rangeC", number(result.range_c)),
        ("approachC", number(result.approach_c)),
        ("heatTransferKW", number(result.heat_transfer_kw)),
        ("waterEnergyKW", number(result.water_energy_kw)),
        ("cpWaterKJkgK", number(result.cp_water_kj_kg_k)),
        (
            "outletAirDryBulbC",
            number(result.outlet_air_state.dry_bulb_c),
        ),
        (
            "outletAirHumidityRatio",
            number(result.outlet_air_state.humidity_ratio),
        ),
        (
            "outletAirEnthalpyKJkgDryAir",
            number(result.outlet_air_state.enthalpy_kj_kg_dry_air),
        ),
        (
            "coarseColdWaterC",
            number(result.grid_convergence.coarse_cold_water_c),
        ),
        (
            "fineColdWaterC",
            optional_number(result.grid_convergence.fine_cold_water_c),
        ),
        (
            "richardsonColdWaterC",
            number(result.grid_convergence.richardson_cold_water_c),
        ),
        (
            "estimatedDiscretizationErrorC",
            optional_number(result.grid_convergence.estimated_discretization_error_c),
        ),
    ]))
}

fn convergence(cli: &Cli) -> Result<String, Failure> {
    let mut input = CrossflowStudyInput::new(crossflow_input(cli)?);
    if let Some(base_cells) = cli.number("--base-cells")? {
        input = input.with_base_cells(base_cells as usize);
    }
    if let Some(safety_factor) = cli.number("--safety-factor")? {
        input = input.with_safety_factor(safety_factor);
    }
    let study = crate::crossflow_convergence_study(&input)?;
    Ok(object(&[
        ("observedOrder", optional_number(study.observed_order)),
        (
            "extrapolatedColdWaterC",
            number(study.extrapolated_cold_water_c),
        ),
        (
            "fineGridErrorEstimateC",
            number(study.fine_grid_error_estimate_c),
        ),
        (
            "gridConvergenceIndexPct",
            number(study.grid_convergence_index_pct),
        ),
        ("coarseCellsColdWaterC", number(study.cold_water_c[0])),
        ("mediumCellsColdWaterC", number(study.cold_water_c[1])),
        ("fineCellsColdWaterC", number(study.cold_water_c[2])),
    ]))
}

fn nozzle_flow(cli: &Cli) -> Result<String, Failure> {
    let flow_m3_s = crate::nozzle_flow_m3_s(
        cli.required_number("--discharge-coefficient")?,
        cli.required_number("--orifice-diameter")?,
        cli.required_number("--dp")?,
        cli.number_or("--density", crate::NOZZLE_WATER_DENSITY_KG_M3)?,
    )?;
    Ok(object(&[("nozzleFlowM3S", number(flow_m3_s))]))
}

fn nozzle(cli: &Cli) -> Result<String, Failure> {
    let nozzles = record_list(cli, "--nozzles")?
        .iter()
        .map(selection_nozzle)
        .collect::<Result<Vec<NozzleRecord>, Failure>>()?;
    let mut input = NozzleArrangementInput::new(
        cli.required_number("--flow")?,
        &nozzles,
        cli.required_number("--dp")?,
    );
    if let Some(raw) = cli.values.get("--count-range") {
        let parts: Vec<&str> = raw.split('|').collect();
        let [lower, upper] = parts[..] else {
            return Err(Failure::Usage(format!(
                "--count-range expects lower|upper, got {raw:?}"
            )));
        };
        input = input.with_target_count_range([
            parse_number(lower, "--count-range")?,
            parse_number(upper, "--count-range")?,
        ]);
    }
    let options = crate::select_nozzle_arrangement(&input)?;
    Ok(object(&[(
        "options",
        json_array(
            &options
                .iter()
                .map(nozzle_arrangement_object)
                .collect::<Vec<String>>(),
        ),
    )]))
}

/// A `key:value` record spec using the JavaScript catalog field names, e.g.
/// `--tower fillAreaM2:64,inletAreaM2:32,sprayZoneHeightM:0.6`.
struct Spec {
    pairs: Vec<(String, String)>,
    option: String,
}

impl Spec {
    fn parse(cli: &Cli, option: &str) -> Result<Self, Failure> {
        match cli.values.get(option) {
            None => Ok(Self {
                pairs: Vec::new(),
                option: option.to_string(),
            }),
            Some(raw) => Self::parse_text(option, raw),
        }
    }

    /// One record's `key:value` pairs, comma-separated.
    fn parse_text(option: &str, raw: &str) -> Result<Self, Failure> {
        let mut pairs = Vec::new();
        if !raw.is_empty() {
            for part in raw.split(',') {
                let (key, value) = part.split_once(':').ok_or_else(|| {
                    Failure::Usage(format!("{option} expects key:value pairs, got {part:?}"))
                })?;
                pairs.push((key.to_string(), value.to_string()));
            }
        }
        Ok(Self {
            pairs,
            option: option.to_string(),
        })
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// A `|`-separated field; absent is an empty list.
    fn list(&self, key: &str) -> Result<Vec<String>, Failure> {
        match self.get(key) {
            None => Ok(Vec::new()),
            Some(raw) => Ok(raw.split('|').map(str::to_string).collect()),
        }
    }

    /// A `|`-separated field that must carry at least one entry.
    fn required_list(&self, key: &str) -> Result<Vec<String>, Failure> {
        let values = self.list(key)?;
        if values.is_empty() {
            return Err(Failure::Usage(format!("{} needs {key}", self.option)));
        }
        Ok(values)
    }

    fn required_text(&self, key: &str) -> Result<String, Failure> {
        self.get(key)
            .map(str::to_string)
            .ok_or_else(|| Failure::Usage(format!("{} needs {key}", self.option)))
    }

    fn number(&self, key: &str) -> Result<Option<f64>, Failure> {
        match self.get(key) {
            None => Ok(None),
            Some(raw) => raw.parse::<f64>().map(Some).map_err(|_| {
                Failure::Usage(format!(
                    "{} {key} expects a number, got {raw:?}",
                    self.option
                ))
            }),
        }
    }

    fn required(&self, key: &str) -> Result<f64, Failure> {
        self.number(key)?
            .ok_or_else(|| Failure::Usage(format!("{} needs {key}", self.option)))
    }

    fn ensure_known(&self, allowed: &[&str]) -> Result<(), Failure> {
        for (key, _) in &self.pairs {
            if !allowed.contains(&key.as_str()) {
                return Err(Failure::Usage(format!(
                    "{} does not know the field {key:?}",
                    self.option
                )));
            }
        }
        Ok(())
    }
}

const ZONE_FIELDS: [&str; 5] = [
    "coefficientPerM",
    "referenceWaterLoadingKgM2S",
    "referenceDryAirLoadingKgM2S",
    "waterExponent",
    "airExponent",
];

const TOWER_FIELDS: [&str; 14] = [
    "fillAreaM2",
    "airFreeAreaM2",
    "driftAreaM2",
    "inletAreaM2",
    "plenumAreaM2",
    "fanStackAreaM2",
    "stackRecoveryFactor",
    "inletLossCoefficient",
    "distributionLossCoefficient",
    "supportLossCoefficient",
    "plenumLossCoefficient",
    "fixedPressureLossPa",
    "sprayZoneHeightM",
    "rainZoneHeightM",
];

const FILL_LIMIT_FIELDS: [&str; 5] = [
    "minWaterLoadingKgM2S",
    "maxWaterLoadingKgM2S",
    "minDryAirLoadingKgM2S",
    "maxDryAirLoadingKgM2S",
    "maxWaterTemperatureC",
];

const PRESSURE_FIELDS: [&str; 5] = [
    "coefficientPaPerM",
    "referenceWaterLoadingKgM2S",
    "referenceDryAirLoadingKgM2S",
    "waterExponent",
    "airExponent",
];

fn zone_correlation(spec: &Spec) -> Result<ZoneCorrelation, Failure> {
    spec.ensure_known(&ZONE_FIELDS)?;
    Ok(ZoneCorrelation {
        coefficient_per_m: spec.required("coefficientPerM")?,
        reference_water_loading_kg_m2_s: spec.required("referenceWaterLoadingKgM2S")?,
        reference_dry_air_loading_kg_m2_s: spec.required("referenceDryAirLoadingKgM2S")?,
        water_exponent: spec.required("waterExponent")?,
        air_exponent: spec.required("airExponent")?,
    })
}

fn tower_record(cli: &Cli) -> Result<TowerRecord, Failure> {
    let spec = Spec::parse(cli, "--tower")?;
    spec.ensure_known(&TOWER_FIELDS)?;
    let spray = Spec::parse(cli, "--spray")?;
    let rain = Spec::parse(cli, "--rain")?;
    let spray_zone = if spray.pairs.is_empty() {
        None
    } else {
        Some(zone_correlation(&spray)?)
    };
    let rain_zone = if rain.pairs.is_empty() {
        None
    } else {
        Some(zone_correlation(&rain)?)
    };
    Ok(TowerRecord {
        fill_area_m2: spec.required("fillAreaM2")?,
        air_free_area_m2: spec.number("airFreeAreaM2")?,
        drift_area_m2: spec.number("driftAreaM2")?,
        inlet_area_m2: spec.number("inletAreaM2")?,
        plenum_area_m2: spec.number("plenumAreaM2")?,
        fan_stack_area_m2: spec.number("fanStackAreaM2")?,
        stack_recovery_factor: spec.number("stackRecoveryFactor")?,
        inlet_loss_coefficient: spec.number("inletLossCoefficient")?,
        distribution_loss_coefficient: spec.number("distributionLossCoefficient")?,
        support_loss_coefficient: spec.number("supportLossCoefficient")?,
        plenum_loss_coefficient: spec.number("plenumLossCoefficient")?,
        fixed_pressure_loss_pa: spec.number("fixedPressureLossPa")?,
        spray_zone_height_m: spec.number("sprayZoneHeightM")?,
        spray_zone,
        rain_zone_height_m: spec.number("rainZoneHeightM")?,
        rain_zone,
    })
}

fn fill_record(cli: &Cli) -> Result<FillRecord, Failure> {
    let thermal_spec = Spec::parse(cli, "--fill-thermal")?;
    let pressure_spec = Spec::parse(cli, "--fill-pressure")?;
    let limits_spec = Spec::parse(cli, "--fill-limits")?;
    limits_spec.ensure_known(&FILL_LIMIT_FIELDS)?;
    let thermal = if thermal_spec.pairs.is_empty() {
        None
    } else {
        Some(zone_correlation(&thermal_spec)?)
    };
    let pressure = if pressure_spec.pairs.is_empty() {
        None
    } else {
        pressure_spec.ensure_known(&PRESSURE_FIELDS)?;
        Some(FillPressureCorrelation {
            coefficient_pa_per_m: pressure_spec.required("coefficientPaPerM")?,
            reference_water_loading_kg_m2_s: pressure_spec
                .required("referenceWaterLoadingKgM2S")?,
            reference_dry_air_loading_kg_m2_s: pressure_spec
                .required("referenceDryAirLoadingKgM2S")?,
            water_exponent: pressure_spec.required("waterExponent")?,
            air_exponent: pressure_spec.required("airExponent")?,
        })
    };
    let allowed_water_quality_classes = match cli.values.get("--fill-quality") {
        None => Vec::new(),
        Some(raw) => raw.split(',').map(str::to_string).collect(),
    };
    Ok(FillRecord {
        id: cli
            .values
            .get("--fill-id")
            .cloned()
            .unwrap_or_else(|| "fill".to_string()),
        thermal,
        pressure,
        limits: crate::FillLimits {
            min_water_loading_kg_m2_s: limits_spec.required("minWaterLoadingKgM2S")?,
            max_water_loading_kg_m2_s: limits_spec.required("maxWaterLoadingKgM2S")?,
            min_dry_air_loading_kg_m2_s: limits_spec.required("minDryAirLoadingKgM2S")?,
            max_dry_air_loading_kg_m2_s: limits_spec.required("maxDryAirLoadingKgM2S")?,
            max_water_temperature_c: limits_spec.required("maxWaterTemperatureC")?,
        },
        allowed_water_quality_classes,
    })
}

fn drift_record(cli: &Cli) -> Result<DriftEliminatorRecord, Failure> {
    let raw = cli
        .values
        .get("--drift-curve")
        .ok_or_else(|| Failure::Usage("airside needs --drift-curve".to_string()))?;
    let mut curve = Vec::new();
    for point in raw.split(',') {
        let fields: Vec<&str> = point.split(':').collect();
        let [face_velocity_ms, drift_ppm, pressure_drop_pa] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--drift-curve expects faceVelocityMS:driftPpm:pressureDropPa, got {point:?}"
            )));
        };
        curve.push(DriftCurvePoint {
            face_velocity_ms: parse_number(face_velocity_ms, "--drift-curve")?,
            drift_ppm: parse_number(drift_ppm, "--drift-curve")?,
            pressure_drop_pa: parse_number(pressure_drop_pa, "--drift-curve")?,
        });
    }
    Ok(DriftEliminatorRecord {
        id: cli
            .values
            .get("--drift-id")
            .cloned()
            .unwrap_or_else(|| "drift".to_string()),
        curve,
    })
}

fn fan_record(cli: &Cli) -> Result<Option<FanRecord>, Failure> {
    let Some(raw) = cli.values.get("--fan-curve") else {
        return Ok(None);
    };
    let mut curve = Vec::new();
    for point in raw.split(',') {
        let fields: Vec<&str> = point.split(':').collect();
        let [flow_m3_s, pressure_pa, efficiency] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--fan-curve expects flowM3S:pressurePa:efficiency, got {point:?}"
            )));
        };
        curve.push(FanCurvePoint {
            flow_m3_s: parse_number(flow_m3_s, "--fan-curve")?,
            pressure_pa: parse_number(pressure_pa, "--fan-curve")?,
            efficiency: parse_number(efficiency, "--fan-curve")?,
        });
    }
    let pressure_basis = match cli.values.get("--fan-basis").map(String::as_str) {
        None => None,
        Some("total") => Some(FanPressureBasis::Total),
        Some("static") => Some(FanPressureBasis::Static),
        Some(other) => {
            return Err(Failure::Usage(format!(
                "--fan-basis expects total or static, got {other:?}"
            )))
        }
    };
    Ok(Some(FanRecord {
        id: cli
            .values
            .get("--fan-id")
            .cloned()
            .unwrap_or_else(|| "fan".to_string()),
        stack_area_m2: cli.number("--fan-stack-area")?,
        pressure_basis,
        reference_density_kg_m3: cli.number("--fan-ref-density")?,
        stack_recovery_factor: cli.number("--fan-stack-recovery")?,
        curve,
    }))
}

/* ---------------- selection catalog from record specs ---------------- */

/// A `;`-separated list of `key:value,…` records for the `select` command, each record a
/// [`Spec`]; list-valued fields spell their entries with `|` (see the usage text).
fn record_list(cli: &Cli, option: &str) -> Result<Vec<Spec>, Failure> {
    let Some(raw) = cli.values.get(option) else {
        return Err(Failure::Usage(format!("select needs {option}")));
    };
    raw.split(';')
        .map(|entry| Spec::parse_text(option, entry))
        .collect()
}

const SELECT_TOWER_FIELDS: [&str; 23] = [
    "id",
    "type",
    "fillAreaM2",
    "airFreeAreaM2",
    "driftAreaM2",
    "inletAreaM2",
    "plenumAreaM2",
    "fanStackAreaM2",
    "stackRecoveryFactor",
    "inletLossCoefficient",
    "distributionLossCoefficient",
    "supportLossCoefficient",
    "plenumLossCoefficient",
    "fixedPressureLossPa",
    "sprayZoneHeightM",
    "rainZoneHeightM",
    "footprintM2",
    "maxWaterMassFlowKgS",
    "fillDepthOptionsM",
    "fillStacks",
    "compatibleFanIds",
    "sprayZone",
    "rainZone",
];

const SELECT_FILL_FIELDS: [&str; 6] = [
    "id",
    "compatibleTowerTypes",
    "allowedWaterQualityClasses",
    "thermal",
    "pressure",
    "limits",
];

const SELECT_DRIFT_FIELDS: [&str; 3] = ["id", "maxWaterTemperatureC", "curve"];

const SELECT_FAN_FIELDS: [&str; 10] = [
    "id",
    "stackAreaM2",
    "pressureBasis",
    "referenceDensityKgM3",
    "stackRecoveryFactor",
    "allowedSpeedRatio",
    "nominalRpm",
    "driveEfficiency",
    "motorEfficiency",
    "curve",
];

const SELECT_NOZZLE_FIELDS: [&str; 5] = [
    "id",
    "name",
    "dischargeCoefficient",
    "orificeDiameterM",
    "referenceWaterDensityKgM3",
];

/// The five-field zone correlation, in the reference's field order, `|`-separated.
fn zone_from_value(option: &str, key: &str, raw: &str) -> Result<ZoneCorrelation, Failure> {
    let parts: Vec<&str> = raw.split('|').collect();
    let [coefficient, water_loading, dry_air_loading, water_exponent, air_exponent] = parts[..]
    else {
        return Err(Failure::Usage(format!(
            "{option} {key} expects coefficientPerM|referenceWaterLoadingKgM2S|referenceDryAirLoadingKgM2S|waterExponent|airExponent, got {raw:?}"
        )));
    };
    Ok(ZoneCorrelation {
        coefficient_per_m: parse_number(coefficient, option)?,
        reference_water_loading_kg_m2_s: parse_number(water_loading, option)?,
        reference_dry_air_loading_kg_m2_s: parse_number(dry_air_loading, option)?,
        water_exponent: parse_number(water_exponent, option)?,
        air_exponent: parse_number(air_exponent, option)?,
    })
}

fn optional_zone(spec: &Spec, key: &str) -> Result<Option<ZoneCorrelation>, Failure> {
    match spec.get(key) {
        None => Ok(None),
        Some(raw) => Ok(Some(zone_from_value(&spec.option, key, raw)?)),
    }
}

fn selection_tower(spec: &Spec) -> Result<SelectionTower, Failure> {
    spec.ensure_known(&SELECT_TOWER_FIELDS)?;
    let tower_type = spec.get("type").and_then(TowerType::parse).ok_or_else(|| {
        Failure::Usage("--towers needs type counterflow or crossflow".to_string())
    })?;
    Ok(SelectionTower {
        id: spec.required_text("id")?,
        tower_type,
        physics: TowerRecord {
            fill_area_m2: spec.required("fillAreaM2")?,
            air_free_area_m2: spec.number("airFreeAreaM2")?,
            drift_area_m2: spec.number("driftAreaM2")?,
            inlet_area_m2: spec.number("inletAreaM2")?,
            plenum_area_m2: spec.number("plenumAreaM2")?,
            fan_stack_area_m2: spec.number("fanStackAreaM2")?,
            stack_recovery_factor: spec.number("stackRecoveryFactor")?,
            inlet_loss_coefficient: spec.number("inletLossCoefficient")?,
            distribution_loss_coefficient: spec.number("distributionLossCoefficient")?,
            support_loss_coefficient: spec.number("supportLossCoefficient")?,
            plenum_loss_coefficient: spec.number("plenumLossCoefficient")?,
            fixed_pressure_loss_pa: spec.number("fixedPressureLossPa")?,
            spray_zone_height_m: spec.number("sprayZoneHeightM")?,
            spray_zone: optional_zone(spec, "sprayZone")?,
            rain_zone_height_m: spec.number("rainZoneHeightM")?,
            rain_zone: optional_zone(spec, "rainZone")?,
        },
        footprint_m2: spec.required("footprintM2")?,
        max_water_mass_flow_kg_s: spec.required("maxWaterMassFlowKgS")?,
        fill_depth_options_m: spec
            .required_list("fillDepthOptionsM")?
            .iter()
            .map(|part| parse_number(part, "--towers fillDepthOptionsM"))
            .collect::<Result<Vec<f64>, Failure>>()?,
        fill_stacks: spec
            .list("fillStacks")?
            .iter()
            .map(|entry| fill_stack("--towers fillStacks", entry))
            .collect::<Result<Vec<FillStack>, Failure>>()?,
        compatible_fan_ids: spec.required_list("compatibleFanIds")?,
    })
}

fn selection_fill(spec: &Spec) -> Result<SelectionFill, Failure> {
    spec.ensure_known(&SELECT_FILL_FIELDS)?;
    let compatible_tower_types = spec
        .required_list("compatibleTowerTypes")?
        .iter()
        .map(|raw| {
            TowerType::parse(raw).ok_or_else(|| {
                Failure::Usage(format!(
                    "--fills compatibleTowerTypes expects counterflow or crossflow, got {raw:?}"
                ))
            })
        })
        .collect::<Result<Vec<TowerType>, Failure>>()?;
    let physics = fill_physics(spec)?;
    Ok(SelectionFill {
        physics,
        compatible_tower_types,
    })
}

/// One fill record's physics out of its spec — the reader `select` and `layers` share, so a
/// fill record authored for one is a fill record for the other.
///
/// `id`, `thermal`, `pressure` and `limits` are required: they are what a layer is computed
/// from. `allowedWaterQualityClasses` is read when present and empty otherwise — `select`'s
/// schema requires it, while a `layers` run that declares no `--quality-class` has nothing to
/// check it against. `compatibleTowerTypes` belongs to the selector's candidate generation and
/// is read by `selection_fill`, not here.
fn fill_physics(spec: &Spec) -> Result<FillRecord, Failure> {
    let limits = spec.required_text("limits")?;
    let limits_parts: Vec<&str> = limits.split('|').collect();
    let [min_water, max_water, min_dry_air, max_dry_air, max_temperature] = limits_parts[..] else {
        return Err(Failure::Usage(format!(
            "{} limits expects minWaterLoadingKgM2S|maxWaterLoadingKgM2S|minDryAirLoadingKgM2S|maxDryAirLoadingKgM2S|maxWaterTemperatureC, got {limits:?}",
            spec.option
        )));
    };
    let pressure = spec.required_text("pressure")?;
    let pressure_parts: Vec<&str> = pressure.split('|').collect();
    let [coefficient, water_loading, dry_air_loading, water_exponent, air_exponent] =
        pressure_parts[..]
    else {
        return Err(Failure::Usage(format!(
            "{} pressure expects coefficientPaPerM|referenceWaterLoadingKgM2S|referenceDryAirLoadingKgM2S|waterExponent|airExponent, got {pressure:?}",
            spec.option
        )));
    };
    let pressure_option = format!("{} pressure", spec.option);
    let limits_option = format!("{} limits", spec.option);
    Ok(FillRecord {
        id: spec.required_text("id")?,
        thermal: Some(zone_from_value(
            &spec.option,
            "thermal",
            &spec.required_text("thermal")?,
        )?),
        pressure: Some(FillPressureCorrelation {
            coefficient_pa_per_m: parse_number(coefficient, &pressure_option)?,
            reference_water_loading_kg_m2_s: parse_number(water_loading, &pressure_option)?,
            reference_dry_air_loading_kg_m2_s: parse_number(dry_air_loading, &pressure_option)?,
            water_exponent: parse_number(water_exponent, &pressure_option)?,
            air_exponent: parse_number(air_exponent, &pressure_option)?,
        }),
        limits: crate::FillLimits {
            min_water_loading_kg_m2_s: parse_number(min_water, &limits_option)?,
            max_water_loading_kg_m2_s: parse_number(max_water, &limits_option)?,
            min_dry_air_loading_kg_m2_s: parse_number(min_dry_air, &limits_option)?,
            max_dry_air_loading_kg_m2_s: parse_number(max_dry_air, &limits_option)?,
            max_water_temperature_c: parse_number(max_temperature, &limits_option)?,
        },
        allowed_water_quality_classes: spec.list("allowedWaterQualityClasses")?,
    })
}

fn selection_drift_eliminator(spec: &Spec) -> Result<SelectionDriftEliminator, Failure> {
    spec.ensure_known(&SELECT_DRIFT_FIELDS)?;
    let mut curve = Vec::new();
    for point in spec.required_list("curve")? {
        let fields: Vec<&str> = point.split(':').collect();
        let [face_velocity_ms, drift_ppm, pressure_drop_pa] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--drift-eliminators curve expects faceVelocityMS:driftPpm:pressureDropPa, got {point:?}"
            )));
        };
        curve.push(DriftCurvePoint {
            face_velocity_ms: parse_number(face_velocity_ms, "--drift-eliminators curve")?,
            drift_ppm: parse_number(drift_ppm, "--drift-eliminators curve")?,
            pressure_drop_pa: parse_number(pressure_drop_pa, "--drift-eliminators curve")?,
        });
    }
    Ok(SelectionDriftEliminator {
        physics: DriftEliminatorRecord {
            id: spec.required_text("id")?,
            curve,
        },
        max_water_temperature_c: spec.required("maxWaterTemperatureC")?,
    })
}

fn selection_fan(spec: &Spec) -> Result<SelectionFan, Failure> {
    spec.ensure_known(&SELECT_FAN_FIELDS)?;
    let mut curve = Vec::new();
    for point in spec.required_list("curve")? {
        let fields: Vec<&str> = point.split(':').collect();
        let [flow_m3_s, pressure_pa, efficiency] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--fans curve expects flowM3S:pressurePa:efficiency, got {point:?}"
            )));
        };
        curve.push(FanCurvePoint {
            flow_m3_s: parse_number(flow_m3_s, "--fans curve")?,
            pressure_pa: parse_number(pressure_pa, "--fans curve")?,
            efficiency: parse_number(efficiency, "--fans curve")?,
        });
    }
    let speed_ratio = spec.required_list("allowedSpeedRatio")?;
    if speed_ratio.len() != 2 {
        return Err(Failure::Usage(format!(
            "--fans allowedSpeedRatio expects lower|upper, got {:?}",
            speed_ratio.join("|")
        )));
    }
    let pressure_basis = match spec.get("pressureBasis") {
        None => None,
        Some("total") => Some(FanPressureBasis::Total),
        Some("static") => Some(FanPressureBasis::Static),
        Some(other) => {
            return Err(Failure::Usage(format!(
                "--fans pressureBasis expects total or static, got {other:?}"
            )))
        }
    };
    Ok(SelectionFan {
        physics: FanRecord {
            id: spec.required_text("id")?,
            stack_area_m2: spec.number("stackAreaM2")?,
            pressure_basis,
            reference_density_kg_m3: spec.number("referenceDensityKgM3")?,
            stack_recovery_factor: spec.number("stackRecoveryFactor")?,
            curve,
        },
        allowed_speed_ratio: [
            parse_number(&speed_ratio[0], "--fans allowedSpeedRatio")?,
            parse_number(&speed_ratio[1], "--fans allowedSpeedRatio")?,
        ],
        // The speed the recorded curve is published at. Optional: a record that states no rated
        // speed still selects, but it has no rpm read-out (nothing here invents one).
        nominal_rpm: spec.number("nominalRpm")?,
        drive_efficiency: spec.required("driveEfficiency")?,
        motor_efficiency: spec.required("motorEfficiency")?,
    })
}

fn selection_nozzle(spec: &Spec) -> Result<NozzleRecord, Failure> {
    spec.ensure_known(&SELECT_NOZZLE_FIELDS)?;
    Ok(NozzleRecord {
        id: spec.required_text("id")?,
        name: spec.required_text("name")?,
        discharge_coefficient: spec.required("dischargeCoefficient")?,
        orifice_diameter_m: spec.required("orificeDiameterM")?,
        reference_water_density_kg_m3: spec.number("referenceWaterDensityKgM3")?,
    })
}

/// `--quality-factors clean:1:1;moderate:0.92:1.12;…` — name, thermal multiplier, pressure
/// multiplier. There is no third field: the selector reads only these two.
fn quality_factors(cli: &Cli) -> Result<HashMap<String, WaterQualityFactor>, Failure> {
    let raw = cli
        .values
        .get("--quality-factors")
        .ok_or_else(|| Failure::Usage("select needs --quality-factors".to_string()))?;
    let mut factors = HashMap::new();
    for entry in raw.split(';') {
        let fields: Vec<&str> = entry.split(':').collect();
        let [name, thermal_multiplier, pressure_multiplier] = fields[..] else {
            return Err(Failure::Usage(format!(
                "--quality-factors expects name:thermalMultiplier:pressureMultiplier, got {entry:?}"
            )));
        };
        factors.insert(
            name.to_string(),
            WaterQualityFactor {
                thermal_multiplier: parse_number(thermal_multiplier, "--quality-factors")?,
                pressure_multiplier: parse_number(pressure_multiplier, "--quality-factors")?,
            },
        );
    }
    Ok(factors)
}

/* ---------------- the catalog schema gate ---------------- */

/// The five record lists of the `select` catalog, in `docs/CATALOG_SCHEMA.md`'s order: the
/// option the records arrive under, the JSON key `validate-catalog` reports them as, and the
/// record type each list must satisfy.
struct CatalogList {
    option: &'static str,
    key: &'static str,
    kind: RecordKind,
}

const CATALOG_LISTS: [CatalogList; 5] = [
    CatalogList {
        option: "--towers",
        key: "towers",
        kind: RecordKind::Tower,
    },
    CatalogList {
        option: "--fills",
        key: "fills",
        kind: RecordKind::Fill,
    },
    CatalogList {
        option: "--drift-eliminators",
        key: "driftEliminators",
        kind: RecordKind::DriftEliminator,
    },
    CatalogList {
        option: "--fans",
        key: "fans",
        kind: RecordKind::Fan,
    },
    CatalogList {
        option: "--nozzles",
        key: "nozzles",
        kind: RecordKind::Nozzle,
    },
];

/// Validate every record of the catalog against the machine schema in [`crate::validate`],
/// returning the per-list record counts. This is the fail-closed gate `select` runs before
/// it builds or ranks anything: every violation is a usage error naming the record and the
/// field, and all of them are reported at once.
fn validate_catalog_records(cli: &Cli) -> Result<[usize; 5], Failure> {
    let mut counts = [0_usize; 5];
    let mut violations = Vec::new();
    for (index, list) in CATALOG_LISTS.iter().enumerate() {
        let specs = record_list(cli, list.option)?;
        counts[index] = specs.len();
        for (position, spec) in specs.iter().enumerate() {
            let label = spec
                .get("id")
                .map(str::to_string)
                .unwrap_or_else(|| format!("#{}", position + 1));
            let pairs: Vec<(&str, &str)> = spec
                .pairs
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            if let Err(mut refused) = crate::validate_record(list.kind, &label, &pairs) {
                violations.append(&mut refused);
            }
        }
    }
    if violations.is_empty() {
        Ok(counts)
    } else {
        Err(Failure::Usage(
            violations
                .iter()
                .map(|violation| violation.message())
                .collect::<Vec<String>>()
                .join("\n"),
        ))
    }
}

/// `validate-catalog` — the schema over the records without running the selector. The
/// counts it prints are how the parity harness ties the machine schema to the bundled
/// catalog.
fn validate_catalog_command(cli: &Cli) -> Result<String, Failure> {
    let counts = validate_catalog_records(cli)?;
    let mut pairs: Vec<(&str, String)> = CATALOG_LISTS
        .iter()
        .enumerate()
        .map(|(index, list)| (list.key, counts[index].to_string()))
        .collect();
    pairs.push(("records", counts.iter().sum::<usize>().to_string()));
    Ok(object(&pairs))
}

fn selection_catalog(cli: &Cli) -> Result<SelectionCatalog, Failure> {
    let towers = record_list(cli, "--towers")?
        .iter()
        .map(selection_tower)
        .collect::<Result<Vec<SelectionTower>, Failure>>()?;
    let fills = record_list(cli, "--fills")?
        .iter()
        .map(selection_fill)
        .collect::<Result<Vec<SelectionFill>, Failure>>()?;
    let drift_eliminators = record_list(cli, "--drift-eliminators")?
        .iter()
        .map(selection_drift_eliminator)
        .collect::<Result<Vec<SelectionDriftEliminator>, Failure>>()?;
    let fans = record_list(cli, "--fans")?
        .iter()
        .map(selection_fan)
        .collect::<Result<Vec<SelectionFan>, Failure>>()?;
    let nozzles = record_list(cli, "--nozzles")?
        .iter()
        .map(selection_nozzle)
        .collect::<Result<Vec<NozzleRecord>, Failure>>()?;
    Ok(SelectionCatalog {
        metadata: CatalogMetadata {
            id: cli.values.get("--catalog-id").cloned().unwrap_or_default(),
            revision: cli
                .values
                .get("--catalog-revision")
                .cloned()
                .unwrap_or_default(),
            status: cli
                .values
                .get("--catalog-status")
                .cloned()
                .unwrap_or_default(),
        },
        water_quality_factors: quality_factors(cli)?,
        towers,
        fills,
        drift_eliminators,
        fans,
        nozzles,
    })
}

fn parse_number(raw: &str, option: &str) -> Result<f64, Failure> {
    raw.parse::<f64>()
        .map_err(|_| Failure::Usage(format!("{option} expects a number, got {raw:?}")))
}

fn merkel_input(cli: &Cli) -> Result<MerkelInput, Failure> {
    let base = MerkelInput::new(
        cli.required_number("--hot")?,
        cli.required_number("--cold")?,
        cli.required_number("--wb")?,
        cli.required_number("--db")?,
        cli.required_number("--lg")?,
    );
    Ok(MerkelInput {
        pressure_pa: cli.number_or("--p", 101_325.0)?,
        salinity_g_kg: cli.number_or("--salinity", 0.0)?,
        integration: cli.integration()?,
        segments: cli.integer("--segments", base.segments)?,
        inlet_enthalpy_convention: cli.convention()?,
        ..base
    })
}

/// JSON numbers use Rust's shortest round-trip float formatting, so the harness parses back
/// the exact `f64` the engine produced. Non-finite values cannot appear on a success path;
/// they would serialize as `null` rather than as invalid JSON.
fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value}")
    } else {
        "null".to_string()
    }
}

/// A field the engine may not resolve at all (`fanStackVelocityMS` on a static-pressure
/// curve, `fanStackAreaM2` on a tower with no stack): the JSON `null` the harness compares.
fn optional_number(value: Option<f64>) -> String {
    match value {
        Some(value) => number(value),
        None => "null".to_string(),
    }
}

fn json_string_array(values: &[String]) -> String {
    let body = values
        .iter()
        .map(|value| json_string(value))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

/// A JSON array of pre-rendered items.
fn json_array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

fn object(pairs: &[(&str, String)]) -> String {
    let body = pairs
        .iter()
        .map(|(key, value)| format!("\"{key}\":{value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

/// One JSON string literal, escaped the way both the binary's stderr and the wasm envelope
/// need it (the reference `JSON.stringify` set of escapes).
pub fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}
