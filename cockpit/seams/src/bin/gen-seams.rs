//! gen-seams - print `VISUAL_DATA_SEAMS.md` from the seam registry.
//!
//!   cargo run --manifest-path seams/Cargo.toml --bin gen-seams > VISUAL_DATA_SEAMS.md
//!   (tools/gen-seams.sh does exactly this and then verifies the file is current)
//!
//! Deterministic: the document is a pure function of `seams/src/lib.rs`, so a diff means the code changed.

fn main() {
    print!("{}", drafthouse_cockpit_seams::markdown());
}
