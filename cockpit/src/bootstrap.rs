//! The one `App` composition both entry points share (issue #71).
//!
//! The wasm entry ([`crate::bridge::viz_start`]) and the native binary (`src/bin/drafthouse.rs`)
//! both call [`assemble`]: same `DefaultPlugins` configuration, same [`crate::app::VizPlugin`],
//! same asset plugin. Only the *host* differs - the web page hands in a canvas window and the
//! binary a desktop window (or none, for the `--no-window` smoke) - so the instrument itself is
//! never forked for the native build.

use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin};

use crate::app::VizPlugin;
use crate::state::StartOptions;

/// Build the app the host is about to run.
///
/// * `window`: `Some` for a real window (the page's canvas / the desktop window), `None` to build
///   the same app without one - the smoke path, which disables the winit plugin because there is
///   no event loop to create.
/// * `frames`: `Some(n)` asks the app to exit cleanly once `n` updates have run (the smoke's own
///   bound); `None` runs until the host's normal exit (the window closes).
pub fn assemble(options: StartOptions, window: Option<Window>, frames: Option<u32>) -> App {
    let windowless = window.is_none();
    let mut plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: window,
            ..default()
        })
        .set(bevy::log::LogPlugin {
            level: bevy::log::Level::WARN,
            ..default()
        })
        // The fixture ships as a plain file, so no `.meta` side-car is written for it; without this
        // the asset reader probes `<file>.meta` and logs a 404 on every load (harmless but wrong in
        // a console an evidence frame is read from). The path is the host's: the page's own
        // directory on wasm, the executable's own `assets/` on native ([`assets_root`]).
        .set(bevy::asset::AssetPlugin {
            meta_check: bevy::asset::AssetMetaCheck::Never,
            file_path: assets_root(),
            ..default()
        });
    if windowless {
        plugins = plugins.disable::<bevy::winit::WinitPlugin>();
    }

    let mut app = App::new();
    app.add_plugins(plugins);
    app.add_plugins(VizPlugin { options });
    if let Some(frames) = frames {
        // A bounded run is a smoke: its frame count must not depend on whether the window has focus.
        // Bevy's default winit settings update a focused window continuously and an unfocused one
        // only on events, so `--frames N` would take unbounded wall time behind another window.
        app.insert_resource(bevy::winit::WinitSettings::continuous());
        app.add_systems(Update, exit_after_frames(frames));
    }
    app
}

/// `--frames N`: raise `AppExit` once `N` updates have run, so a smoke can start the real app and
/// still stop by itself (the windowed form closes the window loop the same way a close click does).
fn exit_after_frames(frames: u32) -> impl FnMut(Local<u32>, MessageWriter<AppExit>) {
    move |mut tick: Local<u32>, mut exit: MessageWriter<AppExit>| {
        *tick += 1;
        if *tick >= frames {
            exit.write(AppExit::Success);
        }
    }
}

/// Where the asset server reads from.
///
/// * **native** - `<the executable's own directory>/assets`. A desktop binary must find its assets
///   next to itself, never in whatever directory it was launched from; `build.rs` seeds that folder
///   beside every binary cargo builds, and a shipped app is the binary with `assets/` beside it.
/// * **wasm** - `assets`, relative to the page, exactly as before (the served plane serves it).
#[cfg(not(target_arch = "wasm32"))]
pub fn assets_root() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("assets")))
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_else(|| "assets".to_string())
}

/// Where the asset server reads from: the page's own directory, exactly as before issue #71.
#[cfg(target_arch = "wasm32")]
pub fn assets_root() -> String {
    "assets".to_string()
}
