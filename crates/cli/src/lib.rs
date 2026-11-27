mod args;
mod cmd;
mod commands;
mod curves;
mod json;
mod objects;
mod records;
mod selection;
mod worked;

use args::Cli;

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
#[derive(Debug)]
pub enum Failure {
    Usage(String),
    Domain(drafthouse_engine::DomainError),
}

impl From<drafthouse_engine::DomainError> for Failure {
    fn from(error: drafthouse_engine::DomainError) -> Self {
        Failure::Domain(error)
    }
}

/// # Errors `Failure` when the command is unknown, an option is unusable, or the engine refuses.
pub fn run(args: &[String]) -> Result<String, Failure> {
    let (command, cli) = Cli::parse(args)?;
    commands::dispatch(&command, &cli)
}

/// Run one command and wrap the outcome as the reply envelope a caller that is not a process reads — the wasm export (`crates/wasm`).
#[must_use]
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

/// One JSON string literal, escaped the way both the binary's stderr and the wasm envelope need it (the reference `JSON.stringify` set of escapes).
#[must_use]
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
            character if u32::from(character) < 0x20 => {
                let code = u32::from(character);
                escaped.push_str("\\u00");
                escaped.push(char::from_digit(code >> 4, 16).unwrap_or('0'));
                escaped.push(char::from_digit(code & 0xf, 16).unwrap_or('0'));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}
