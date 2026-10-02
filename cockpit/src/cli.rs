//! The native host's half of the binary (issue #71): the CLI, the desktop window and the run loop.
//!
//! Same crate, same `App` as the web plane: [`crate::bootstrap::assemble`] is what the wasm entry
//! point calls too, and everything here is the *host* half - a desktop window instead of a canvas,
//! and CLI flags instead of `index.html`'s query string. No part of the instrument is forked.
//!
//! Every staging flag mirrors the page's parameter one for one (`--view` = `?view=`, `--duty-open` =
//! `?duty-open=`, ...) with the same names and the same defaults. An unknown flag, an unknown value
//! or a missing value exits 2 with a message naming the flag - never a silent fallback.
//!
//! This lives in the lib, not in the binary, so its tests run in the lib's test harness: the binary
//! target itself carries `test = false` (see `Cargo.toml`) because `cargo test` linking a
//! Bevy-sized test harness for a thin `main` is a cost with no test behind it (issue #71 fix round).

use std::process::ExitCode;

use crate::bootstrap;
use crate::state::StartOptions;
use bevy::window::{PresentMode, Window, WindowResizeConstraints, WindowResolution};
use cockpit::host::{Branding, HostConfig};

/// The page's own catalog label (`cockpit/index.html`); the same string on both hosts.
const CATALOG_LABEL: &str = "illustrative-catalog-v0.1";
/// `--no-window` without `--frames`: how many updates the smoke runs before it exits 0.
const DEFAULT_SMOKE_FRAMES: u32 = 3;

const USAGE: &str = "\
drafthouse - the Synergy Drafthouse cockpit as a native desktop app (issue #71).

USAGE:
    drafthouse [FLAGS] [--host public|internal] [--engine real|fixture|unavailable]
    drafthouse --no-window [--frames N]     # build the same App, run N updates, exit

WINDOW (default: a 1440x900 resizable window titled \"Synergy Drafthouse\", minimum 900x600):
    --no-window          build the App without a window and run --frames updates (the CI smoke)
    --frames N           exit after N updates; without it the app runs until the window is closed

HOST AND ENGINE:
    --host public|internal        which host config to run as (default: public)
    --engine real|fixture|unavailable
                                  which engine to run (default: this build's own default)
                                  `unavailable` stages a missing asset, as `?engine=unavailable` does
    --help                       this text

STAGING - the web plane's query parameters, same names, same defaults:
    --view <slug>                ?view=          the view to open (cockpit|curves|seams)
    --focus <slug>               ?focus=         the bay/part the keyboard is focused on
    --rpm <number>               ?rpm=           the fan speed, through the record's rated speed
    --spacing <metres>           ?spacing=       nozzle spacing (0.2 - 2.5 m)
    --pattern <slug>             ?pattern=       the nozzle arrangement
    --drag <class>:<id>          ?drag=          stage a drag in progress
    --over <slot>                ?over=          the bay the staged drag is over
    --drop <class>:<id>@<slot>   ?drop=          stage a drop into a bay
    --move up|down:<index>       ?move=          move a fill layer
    --remove <index>             ?remove=        remove a fill layer
    --add <class>:<id>           ?add=           add a fill layer
    --layer <index>              ?layer=         select a fill layer
    --grid 0|1                   ?grid=          the grid overlay
    --frozen 0|1                 ?frozen=        freeze the animations (byte-comparable frames)
    --reduced_motion 0|1         ?reduced_motion= reduced-motion mode
    --bay <slot>                 ?bay=           focus a bay
    --picker <slot>              ?picker=        open a bay's picker
    --picker-row <row>           ?picker-row=    the picker's keyboard cursor
    --cam <yaw>,<pitch>,<dist>   ?cam=           the 3D camera (the 3D view is off in this build)
    --rail 0|1                   ?rail=          the parts rail collapsed / open
    --legend 0|1                 ?legend=        the honesty legend dismissed / shown
    --form <class>               ?form=          open the custom-part form (fan|drift|fill|nozzle)
    --form-fields <k=v,...>      ?form-fields=   seed the form's fields
    --form-rows <v,v;v,v>        ?form-rows=     seed the form's first point table
    --form-companion <value>     ?form-companion= the point table's companion scalar
    --form-save 0|1              ?form-save=     press Save (the same call the button makes)
    --duty <k=v;k=v>             ?duty=          edit the working duty before the first run
    --hover <class>:<id>         ?hover=         show the parameter card (the phone long-press path)
    --hover-long 0|1             ?hover-long=    the staged card is the long-press one
    --hover-bay 0|1              ?hover-bay=     the staged card is the fitted bay's
    --duty-open 0|1              ?duty-open=     the DUTY section expanded (default) / collapsed
    --water-open 0|1             ?water-open=    the WATER QUALITY section collapsed (default) / expanded
    --limits-open 0|1            ?limits-open=   the LIMITS section collapsed (default) / expanded
";

/// The parsed command line.
#[derive(Debug, Default)]
struct Args {
    options: StartOptions,
    help: bool,
    no_window: bool,
    frames: Option<u32>,
}

/// The whole command line: its arguments in, the process exit code out. `src/bin/drafthouse.rs`'s
/// `main` is one call of this.
pub fn main(argv: Vec<String>) -> ExitCode {
    let args = match parse(&argv) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("drafthouse: {message}");
            eprintln!("drafthouse: `drafthouse --help` lists every flag");
            return ExitCode::from(2);
        }
    };
    if args.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    run(args)
}

/// The host configs the page injects (`cockpit/index.html`), the same two shapes.
fn public_host() -> HostConfig {
    HostConfig {
        public: true,
        branding: None,
        catalog_label: Some(CATALOG_LABEL.to_string()),
    }
}

fn internal_host() -> HostConfig {
    HostConfig {
        public: false,
        branding: Some(Branding {
            name: "Synergy Services".to_string(),
            site_url: "https://synergyservices.co.th".to_string(),
            notice: "Engine and source under PolyForm Noncommercial 1.0.0".to_string(),
            mark: "SS".to_string(),
            accent: Some("#2fb3c9".to_string()),
        }),
        catalog_label: Some(CATALOG_LABEL.to_string()),
    }
}

/// The desktop window: the same 1440x900 the canvas is sized to, resizable, with a 900x600 floor.
/// Present mode and backends are the defaults (wgpu: Metal / Vulkan / DX12; `WGPU_BACKEND` still
/// selects, as Bevy's own settings read the environment).
fn desktop_window() -> Window {
    Window {
        title: "Synergy Drafthouse".to_string(),
        resolution: WindowResolution::new(1440, 900),
        resizable: true,
        resize_constraints: WindowResizeConstraints {
            min_width: 900.0,
            min_height: 600.0,
            ..Default::default()
        },
        present_mode: PresentMode::AutoVsync,
        ..Default::default()
    }
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut index = 0;
    while index < argv.len() {
        let raw = argv[index].as_str();
        if !raw.starts_with('-') {
            return Err(format!(
                "unexpected argument `{raw}` - every drafthouse argument is a `--flag`"
            ));
        }
        let (flag, inline) = match raw.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value)),
            _ => (raw, None),
        };
        match flag {
            "--help" | "-h" => {
                no_value(flag, inline)?;
                args.help = true;
                index += 1;
                continue;
            }
            "--no-window" => {
                no_value(flag, inline)?;
                args.no_window = true;
                index += 1;
                continue;
            }
            _ => {}
        }
        // Every other flag takes a value: `--view curves` or `--view=curves`.
        let value = match inline {
            Some(value) => value,
            None => {
                index += 1;
                match argv.get(index).map(String::as_str) {
                    Some(value) if !value.starts_with("--") => value,
                    _ => return Err(format!("`{flag}` expects a value")),
                }
            }
        };
        apply(&mut args, flag, value)?;
        index += 1;
    }
    Ok(args)
}

/// Apply one flag, or refuse it by name.
fn apply(args: &mut Args, flag: &str, value: &str) -> Result<(), String> {
    let options = &mut args.options;
    match flag {
        "--host" => {
            options.host = match value {
                "public" => public_host(),
                "internal" => internal_host(),
                other => {
                    return Err(format!(
                        "`--host`: expected `public` or `internal`, got `{other}`"
                    ))
                }
            }
        }
        "--engine" => {
            options.engine = Some(match value {
                "real" | "fixture" | "unavailable" => value.to_string(),
                other => {
                    return Err(format!(
                        "`--engine`: expected `real`, `fixture` or `unavailable`, got `{other}`"
                    ))
                }
            });
        }
        "--frames" => args.frames = Some(count::<u32>(flag, value)?),
        "--view" => options.view = Some(value.to_string()),
        "--focus" => options.focus = Some(value.to_string()),
        "--rpm" => options.rpm = Some(number(flag, value)?),
        "--spacing" => options.spacing = Some(number(flag, value)?),
        "--pattern" => options.pattern = Some(value.to_string()),
        "--drag" => options.drag = Some(value.to_string()),
        "--over" => options.over = Some(value.to_string()),
        "--drop" => options.drop = Some(value.to_string()),
        "--move" => options.shift = Some(value.to_string()),
        "--remove" => options.remove = Some(count::<usize>(flag, value)?),
        "--add" => options.add = Some(value.to_string()),
        "--layer" => options.layer = Some(count::<usize>(flag, value)?),
        "--grid" => options.grid = Some(switch(flag, value)?),
        "--frozen" => options.frozen = Some(switch(flag, value)?),
        "--reduced_motion" => options.reduced_motion = switch(flag, value)?,
        "--bay" => options.bay = Some(value.to_string()),
        "--picker" => options.picker = Some(value.to_string()),
        "--picker-row" => options.picker_row = Some(count::<usize>(flag, value)?),
        "--cells" => options.cells = Some(count::<u32>(flag, value)?),
        "--cutaway" => options.cutaway = Some(switch(flag, value)?),
        "--cell" => options.cell = Some(count::<usize>(flag, value)?),
        "--cam" => options.cam = Some(value.to_string()),
        "--rail" => options.rail = Some(switch(flag, value)?),
        "--legend" => options.legend = Some(switch(flag, value)?),
        "--form" => options.form = Some(value.to_string()),
        "--form-fields" => options.form_fields = Some(value.to_string()),
        "--form-rows" => options.form_rows = Some(value.to_string()),
        "--form-companion" => options.form_companion = Some(value.to_string()),
        "--form-save" => options.form_save = Some(switch(flag, value)?),
        "--duty" => options.duty = Some(value.to_string()),
        "--hover" => options.hover = Some(value.to_string()),
        "--hover-long" => options.hover_long = Some(switch(flag, value)?),
        "--hover-bay" => options.hover_bay = Some(switch(flag, value)?),
        "--duty-open" => options.duty_open = Some(switch(flag, value)?),
        "--water-open" => options.water_open = Some(switch(flag, value)?),
        "--limits-open" => options.limits_open = Some(switch(flag, value)?),
        other => return Err(format!("unknown flag `{other}`")),
    }
    Ok(())
}

fn no_value(flag: &str, inline: Option<&str>) -> Result<(), String> {
    match inline {
        Some(value) => Err(format!("`{flag}` takes no value, got `{value}`")),
        None => Ok(()),
    }
}

fn number(flag: &str, value: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|_| format!("`{flag}`: expected a number, got `{value}`"))
}

fn count<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value
        .parse::<T>()
        .map_err(|_| format!("`{flag}`: expected a whole number, got `{value}`"))
}

fn switch(flag: &str, value: &str) -> Result<bool, String> {
    match value {
        "1" => Ok(true),
        "0" => Ok(false),
        other => Err(format!("`{flag}`: expected `1` or `0`, got `{other}`")),
    }
}

fn run(args: Args) -> ExitCode {
    let assets = bootstrap::assets_root();
    if !std::path::Path::new(&assets).is_dir() {
        eprintln!("drafthouse: no assets directory at `{assets}`");
        eprintln!(
            "drafthouse: `cargo build` seeds it beside the binary (build.rs); a shipped app carries \
             `assets/` next to the executable"
        );
        return ExitCode::from(3);
    }
    eprintln!("drafthouse: assets `{assets}`");

    // `--no-window` alone still has to stop by itself; the CI smoke relies on that.
    let frames = args
        .frames
        .or(args.no_window.then_some(DEFAULT_SMOKE_FRAMES));
    let blank = args.no_window;
    let window = (!blank).then(desktop_window);
    let mut app = bootstrap::assemble(args.options, window, frames);

    if blank {
        // No event loop to drive the app: step it by hand, exactly as Bevy's own `run_once` does.
        app.finish();
        app.cleanup();
        for _ in 0..frames.unwrap_or(DEFAULT_SMOKE_FRAMES) {
            app.update();
        }
        eprintln!(
            "drafthouse: smoke OK - {} update(s) of the built App, no window",
            frames.unwrap_or(DEFAULT_SMOKE_FRAMES)
        );
        return ExitCode::SUCCESS;
    }
    match app.run() {
        bevy::app::AppExit::Success => ExitCode::SUCCESS,
        bevy::app::AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn parsed(list: &[&str]) -> Args {
        parse(&argv(list)).expect("these flags parse")
    }

    #[test]
    fn host_defaults_to_public_and_takes_the_page_s_two_shapes() {
        let default = parsed(&[]);
        assert!(default.options.host.public);
        assert!(default.options.host.branding.is_none());

        let internal = parsed(&["--host", "internal"]);
        assert!(!internal.options.host.public);
        assert_eq!(
            internal
                .options
                .host
                .branding
                .expect("internal branding")
                .name,
            "Synergy Services"
        );
        assert!(parsed(&["--host", "public"]).options.host.public);
    }

    #[test]
    fn engine_defaults_to_the_build_s_own_and_keeps_the_page_s_three_values() {
        assert!(parsed(&[]).options.engine.is_none());
        for value in ["real", "fixture", "unavailable"] {
            assert_eq!(
                parsed(&["--engine", value]).options.engine.as_deref(),
                Some(value)
            );
        }
    }

    #[test]
    fn staging_flags_land_in_the_same_fields_the_query_string_fills() {
        let args = parsed(&[
            "--view",
            "curves",
            "--focus",
            "fan",
            "--rpm",
            "219",
            "--spacing",
            "1.25",
            "--grid",
            "0",
            "--frozen",
            "1",
            "--reduced_motion",
            "1",
            "--duty",
            "waterMassFlowKgS=200",
            "--duty-open",
            "0",
            "--form-fields",
            "stackAreaM2=12,heightM=3",
            "--picker-row",
            "2",
            "--cells",
            "4",
            "--remove",
            "1",
            "--move",
            "up:0",
            "--drop",
            "fan:AX-420@fan",
        ]);
        let options = &args.options;
        assert_eq!(options.view.as_deref(), Some("curves"));
        assert_eq!(options.focus.as_deref(), Some("fan"));
        assert_eq!(options.rpm, Some(219.0));
        assert_eq!(options.spacing, Some(1.25));
        assert_eq!(options.grid, Some(false));
        assert_eq!(options.frozen, Some(true));
        assert!(options.reduced_motion);
        assert_eq!(options.duty.as_deref(), Some("waterMassFlowKgS=200"));
        assert_eq!(options.duty_open, Some(false));
        // A value carrying `=` keeps everything after the first one.
        assert_eq!(
            options.form_fields.as_deref(),
            Some("stackAreaM2=12,heightM=3")
        );
        assert_eq!(options.picker_row, Some(2));
        assert_eq!(options.cells, Some(4));
        assert_eq!(options.remove, Some(1));
        assert_eq!(options.shift.as_deref(), Some("up:0"));
        assert_eq!(options.drop.as_deref(), Some("fan:AX-420@fan"));
    }

    #[test]
    fn the_inline_form_is_the_same_flag() {
        let args = parsed(&["--view=curves", "--grid=1", "--frames=7"]);
        assert_eq!(args.options.view.as_deref(), Some("curves"));
        assert_eq!(args.options.grid, Some(true));
        assert_eq!(args.frames, Some(7));
    }

    #[test]
    fn no_window_runs_a_bounded_smoke_by_default() {
        let args = parsed(&["--no-window"]);
        assert!(args.no_window);
        assert_eq!(args.frames, None);
        assert_eq!(
            args.frames
                .or(args.no_window.then_some(DEFAULT_SMOKE_FRAMES)),
            Some(DEFAULT_SMOKE_FRAMES)
        );
        assert_eq!(parsed(&["--no-window", "--frames", "0"]).frames, Some(0));
    }

    #[test]
    fn an_unknown_flag_is_refused_by_name() {
        let error = parse(&argv(&["--nope", "1"])).expect_err("unknown flags are refused");
        assert!(error.contains("`--nope`"), "{error}");
        let positional = parse(&argv(&["7"])).expect_err("positional arguments are refused");
        assert!(positional.contains('7'), "{positional}");
    }

    #[test]
    fn a_bad_value_is_refused_naming_its_flag() {
        for (flag, value) in [
            ("--host", "staging"),
            ("--engine", "virtual"),
            ("--grid", "2"),
            ("--frozen", "yes"),
            ("--rpm", "fast"),
            ("--frames", "-1"),
            ("--picker-row", "1.5"),
        ] {
            let error = parse(&argv(&[flag, value])).expect_err("bad values are refused");
            assert!(error.contains(flag), "`{flag} {value}` -> {error}");
        }
    }

    #[test]
    fn a_missing_value_is_refused_naming_its_flag() {
        let error = parse(&argv(&["--view"])).expect_err("a value is required");
        assert!(error.contains("`--view`"), "{error}");
    }

    #[test]
    fn flags_without_values_refuse_an_inline_one() {
        for line in ["--help=1", "--no-window=1"] {
            let error = parse(&argv(&[line])).expect_err("these flags take no value");
            assert!(error.contains("takes no value"), "{error}");
        }
    }

    #[test]
    fn every_flag_the_help_lists_is_a_flag_the_parser_knows() {
        for line in USAGE.lines() {
            let line = line.trim_start();
            let Some(first) = line.split_whitespace().next() else {
                continue;
            };
            let flag = first.trim_end_matches(',');
            if !flag.starts_with("--") {
                continue;
            }
            if let Err(error) = parse(&argv(&[flag, "1"])) {
                assert!(
                    !error.contains("unknown flag"),
                    "`{flag}` is documented but not parsed: {error}"
                );
            }
        }
    }
}
