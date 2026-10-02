//! Seeds `assets/` beside the binary cargo builds (issue #71).
//!
//! The native binary resolves its assets relative to the **executable** (`bootstrap::assets_root`),
//! never relative to the current directory: a desktop app launched from `/`, `C:\` or Finder must
//! find its own `assets/`. Cargo puts the binary in `<target>/<profile>/`, so this script copies the
//! crate's `assets/` to `<target>/<profile>/assets/` - `cargo run --bin drafthouse` then works from
//! any directory, and a shipped app is the binary with that folder beside it.
//!
//! `OUT_DIR` is `<target>/<profile>/build/<pkg>-<hash>/out`; three parents up is the profile
//! directory. (`--target <triple>` adds a level above the profile directory, not inside it, so the
//! depth is the same.)

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source = manifest.join("assets");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let profile_dir = out
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is <target>/<profile>/build/<pkg>/out")
        .to_path_buf();
    let dest = profile_dir.join("assets");
    println!("cargo:rerun-if-changed=assets");
    if let Err(error) = copy_tree(&source, &dest) {
        // Never silent: a binary without its `assets/` can only draw the engine's load failure.
        panic!(
            "copying {} to {}: {error}",
            source.display(),
            dest.display()
        );
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
