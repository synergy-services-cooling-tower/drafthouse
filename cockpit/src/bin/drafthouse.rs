//! `drafthouse` - the native desktop binary (issue #71): a thin `main`.
//!
//! The CLI, the desktop window and the run loop live in the lib ([`drafthouse_cockpit::cli`]) with
//! their tests, which are what `cargo test` runs; this target exists so the crate ships a binary,
//! and it carries `test = false` in `Cargo.toml` so `cargo test` does not link a Bevy-sized test
//! harness around it (issue #71 fix round - the runner's linker died on exactly that link).

use std::process::ExitCode;

fn main() -> ExitCode {
    drafthouse_cockpit::cli::main(std::env::args().skip(1).collect())
}
