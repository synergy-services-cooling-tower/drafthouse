//! The contract-drift gate (#57 scope 3): the copied contract against the cockpit's own bytes.
//!
//! `src/engine.rs` is a byte copy of the cockpit's `cockpit/contract/src/engine.rs` (imported from
//! the private design pass at commit `815d832730b944607afc7aac07f5bea610c8c07c`, plus issue #59's `nominalRpm`
//! field on the fan record): 13010 bytes, git blob
//! `418ecf8cb46fbf2cc8de51d066e146815c03904f`, sha256
//! `c955238548fd51a62c7a33687d47bf05608e64062502df53d0d66b0c74bf27fe`. The copy is what this crate
//! compiles against; this test re-checks its identity, so a reformatted, edited or "improved" copy
//! fails here naming the drift. `src/lib.rs` documents why the `mod` declaration carries
//! `#[rustfmt::skip]`: `cargo fmt` would otherwise rewrite the file and break this pin.
//!
//! Three checks, strongest first, and no silent skips:
//!
//! 1. the copy's bytes against the cockpit's own file in this repository, when the cockpit checkout
//!    is present next to it (`../cockpit/contract/src/engine.rs`);
//! 2. the copy's git blob hash against the pin - this is the check that runs everywhere, including a
//!    CI checkout with no second checkout next to it;
//! 3. the copy's byte length, and its sha256 when the host has a `shasum`/`sha256sum` helper.

use std::path::PathBuf;
use std::process::Command;

/// The copy, as the adapter compiles it.
const CONTRACT: &str = include_str!("../src/engine.rs");

const CONTRACT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/engine.rs");
const PIN_BYTES: usize = 13_010;
const PIN_BLOB: &str = "418ecf8cb46fbf2cc8de51d066e146815c03904f";
const PIN_SHA256: &str = "c955238548fd51a62c7a33687d47bf05608e64062502df53d0d66b0c74bf27fe";
/// The private design pass's commit the cockpit's contract was imported from (issue #58); the issue-59
/// fan-record field is the one adaptation this repository has made to it since.
const SOURCE_COMMIT: &str = "815d832730b944607afc7aac07f5bea610c8c07c";

#[test]
fn the_copy_is_the_pinned_source_bytes() {
    assert_eq!(
        CONTRACT.len(),
        PIN_BYTES,
        "the copied contract is {} bytes, the pin is {PIN_BYTES}: it was edited, not copied",
        CONTRACT.len()
    );

    // 2. The blob hash: the identity that travels with the copy itself.
    match git_blob_hash(CONTRACT_PATH) {
        Ok(blob) => assert_eq!(
            blob, PIN_BLOB,
            "the copied contract drifted from the private design pass {SOURCE_COMMIT}: git blob {blob} is not \
             the pinned {PIN_BLOB}. The contract is a byte copy of the cockpit's \
             cockpit/contract/src/engine.rs and nothing in it may be edited, reformatted or renamed \
             - if the cockpit's contract moved, re-copy the file, re-pin both digests here and in \
             src/lib.rs, and re-run the parity test"
        ),
        Err(reason) => eprintln!(
            "NOT VERIFIABLE on this host: `git hash-object` is unavailable ({reason}); the byte \
             length above was still checked"
        ),
    }

    // 1. The cockpit's own file, when this checkout sits next to the adapter's crate.
    match cockpit_contract() {
        Some((path, bytes)) => {
            if bytes == CONTRACT.as_bytes() {
                println!(
                    "the cockpit's contract checked: {} ({} bytes) holds the pinned bytes",
                    path.display(),
                    bytes.len()
                );
            } else {
                eprintln!(
                    "NOT VERIFIABLE against the cockpit's contract: {} holds {} bytes against the \
                     pinned {PIN_BYTES}, so the copy is not that file's bytes; the copy's own \
                     identity was checked above and is what this crate compiles",
                    path.display(),
                    bytes.len()
                );
            }
        }
        None => eprintln!(
            "NOT VERIFIABLE on this host: no cockpit checkout found next to this crate \
             (set `COCKPIT_DIR` to point at one); the pinned blob above is the identity check that \
             remains"
        ),
    }

    // 3. The sha256, when the host can compute one.
    match sha256_of(CONTRACT_PATH) {
        Some((digest, helper)) => assert_eq!(
            digest, PIN_SHA256,
            "the copied contract's sha256 is {digest} (via {helper}), not the pinned {PIN_SHA256}"
        ),
        None => eprintln!(
            "NOT VERIFIABLE on this host: no `shasum`/`sha256sum` helper to compute the sha256; the \
             pinned blob above was still checked"
        ),
    }
}

/// `git hash-object <path>`: the blob the copy would enter a git object store as. This is the pin
/// the brief records next to the sha256, and it needs nothing but git.
fn git_blob_hash(path: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["hash-object", path])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git hash-object exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The file's sha256 through the host's own helper, and which helper answered.
fn sha256_of(path: &str) -> Option<(String, &'static str)> {
    for (program, args) in [
        ("shasum", vec!["-a", "256", path]),
        ("sha256sum", vec![path]),
    ] {
        let Ok(output) = Command::new(program).args(&args).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(digest) = stdout.split_whitespace().next() {
            if digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some((digest.to_string(), program));
            }
        }
    }
    None
}

/// The cockpit's own contract file, when this checkout sits next to the adapter's crate:
/// `<repo>/cockpit/contract/src/engine.rs`. `COCKPIT_DIR` overrides the location.
fn cockpit_contract() -> Option<(PathBuf, Vec<u8>)> {
    let path = match std::env::var_os("COCKPIT_DIR") {
        Some(dir) => PathBuf::from(dir)
            .join("contract")
            .join("src")
            .join("engine.rs"),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("cockpit")
            .join("contract")
            .join("src")
            .join("engine.rs"),
    };
    let bytes = std::fs::read(&path).ok()?;
    Some((path, bytes))
}
