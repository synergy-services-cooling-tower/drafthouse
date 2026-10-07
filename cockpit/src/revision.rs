//! The catalog revision object: `{ id, publishedAt, sha256, status, records… }`, an **immutable** file.
//!
//! Owner decision (2026-09-23, D26): a catalog revision is an immutable file object. The records the
//! instrument selects over are not a mutable session catalog any more - they are a published revision,
//! identified by its id, dated, carrying its own status line, and sealed with a digest of its own bytes.
//!
//! * The first revision is **shipped in the binary and the wasm module** ([`shipped`]):
//!   `illustrative-catalog-v0.1`, the records `cockpit/assets/fixture.json` already carries. Shipping it
//!   is what makes the default state a *revision* rather than a file-less default.
//! * `File > Import catalog revision…` loads another one from disk; the internal host's equivalent goes
//!   through the page's upload path and the same [`Revision::read`] call.
//! * A run pins the revision's id in the project file (`catalogRevisionId`).
//!
//! # Immutability
//!
//! The object declares `sha256`, and it is checked on **every** read, not on demand: a file whose bytes do
//! not hash to the declared digest is **refused by name**, both digests printed. What the digest covers is
//! stated exactly - [`hash_input`] - so it is reproducible in another language:
//!
//! > the published bytes, with the `sha256` value emptied
//!
//! The rule is a byte substitution, not a re-serialisation. That is deliberate: any byte of the file - a
//! reformatting, one digit inside one record - moves the digest away from the declared one, which is what
//! "immutable" has to mean for a file object. A record edited by hand cannot be blessed by re-hashing
//! either, because re-hashing is not an operation this crate offers: to change a revision you publish a
//! new revision id (see `docs/PROJECT_FORMAT.md`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::sha256;

/// The revision schema version this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// The shipped revision's id.
pub const SHIPPED_ID: &str = "illustrative-catalog-v0.1";
/// The revision shipped in this build, as text (the generator that wrote it,
/// `gen-revision.mjs`, is kept with the maintainers).
pub const SHIPPED_TEXT: &str = include_str!("../assets/revision-illustrative-catalog-v0.1.json");

/// One catalog revision: the records, who published them, and the digest that seals them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Revision {
    pub format_version: u32,
    /// The revision's id - what a run pins in a project file (`catalogRevisionId`).
    pub id: String,
    /// The catalog's own revision date (`catalog.metadata.revision` of the records it carries).
    pub published_at: String,
    /// The declared digest of this file's bytes ([`hash_input`]).
    pub sha256: String,
    /// The status line, verbatim - `SYNTHETIC / NOT VENDOR DATA` for the illustrative catalog.
    pub status: String,
    /// The catalog block, exactly as the engine's catalog builder reads it.
    pub records: Value,
    /// Unknown keys, kept verbatim (the same forward-compatibility rule the project file follows).
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Default for Revision {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            id: String::new(),
            published_at: String::new(),
            sha256: String::new(),
            status: String::new(),
            records: Value::Null,
            rest: Map::new(),
        }
    }
}

/// The bytes the declared digest covers: the published text with the `sha256` value emptied.
///
/// The scan is deliberate and small so a second implementation (the generator) can be checked against it:
/// find the first `"sha256"` key, keep everything up to and including its `:`, write `""` where the value
/// stood, keep the rest of the file byte for byte. Whitespace **inside** the `sha256` field's own value
/// span is the only thing normalised away, so a file that has been reformatted elsewhere still verifies -
/// and any other byte change does not.
pub fn hash_input(text: &str) -> Result<String, String> {
    let key = "\"sha256\"";
    let at = text
        .find(key)
        .ok_or_else(|| "no `sha256` field to verify against".to_string())?;
    let colon = text[at + key.len()..]
        .find(':')
        .ok_or_else(|| "the `sha256` field carries no `:`".to_string())?
        + at
        + key.len();
    let after = &text[colon + 1..];
    let value_start = after.len() - after.trim_start().len();
    let quoted = &after[value_start..];
    let quoted = quoted
        .strip_prefix('"')
        .ok_or_else(|| "the `sha256` value is not a string".to_string())?;
    let end = quoted
        .find('"')
        .ok_or_else(|| "the `sha256` value is not closed".to_string())?;
    Ok(format!("{}\"\"{}", &text[..colon + 1], &quoted[end + 1..]))
}

/// The digest a revision file with `text`'s bytes declares.
pub fn digest_of(text: &str) -> Result<String, String> {
    Ok(sha256::hex(hash_input(text)?.as_bytes()))
}

/// The refusal. It names the revision, the mismatch, **both** digests, and what to do instead.
fn refusal(id: &str, declared: &str, computed: &str) -> String {
    format!(
        "catalog revision `{id}` refused: sha256 mismatch - the file declares {declared} but its bytes \
         hash to {computed}. Revisions are immutable: import the file exactly as it was published, or \
         publish a new revision id (this build offers no way to re-seal a revision, deliberately)"
    )
}

impl Revision {
    /// Read a revision from a file's text: parse, then **verify the declared digest**.
    pub fn read(text: &str) -> Result<Revision, String> {
        let revision: Revision =
            serde_json::from_str(text).map_err(|e| format!("catalog revision: {e}"))?;
        if revision.format_version == 0 {
            return Err("catalog revision: no `formatVersion`".to_string());
        }
        if revision.format_version > FORMAT_VERSION {
            return Err(format!(
                "catalog revision: formatVersion {} is newer than this build's {FORMAT_VERSION}",
                revision.format_version
            ));
        }
        if revision.id.trim().is_empty() {
            return Err("catalog revision: no `id`".to_string());
        }
        let computed = digest_of(text)?;
        if !computed.eq_ignore_ascii_case(revision.sha256.trim()) {
            return Err(refusal(&revision.id, revision.sha256.trim(), &computed));
        }
        Ok(revision)
    }

    /// Read and verify a revision from disk. The only file-touching entry point, and the one
    /// `File > Import catalog revision…` calls (the web-internal host hands the page's upload to
    /// [`Revision::read`] instead).
    pub fn from_file(path: &Path) -> Result<Revision, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("catalog revision `{}`: {e}", path.display()))?;
        Revision::read(&text).map_err(|e| format!("{} (file: {})", e, path.display()))
    }

    /// The records as the engine's catalog builder reads them.
    pub fn records(&self) -> &Value {
        &self.records
    }

    /// The record counts one line, for a status line or a report.
    pub fn counts_line(&self) -> String {
        let count = |key: &str| {
            self.records
                .get(key)
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        };
        format!(
            "{}: {} towers, {} fills, {} drift eliminators, {} fans, {} nozzle banks",
            self.id,
            count("towers"),
            count("fills"),
            count("driftEliminators"),
            count("fans"),
            count("nozzles")
        )
    }

    /// Every record id the revision carries, by list - what an import check can compare against.
    pub fn record_ids(&self) -> BTreeMap<String, Vec<String>> {
        let mut out = BTreeMap::new();
        for key in ["towers", "fills", "driftEliminators", "fans", "nozzles"] {
            let ids = self
                .records
                .get(key)
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            out.insert(key.to_string(), ids);
        }
        out
    }
}

/// The revision this build ships, parsed and verified **once**, on first use.
///
/// `Err` is a build-time defect (the shipped bytes do not verify), not a user error: the app shows it in
/// the same place a failed fixture load shows up rather than panicking a wasm module.
pub fn shipped() -> &'static Result<Revision, String> {
    use std::sync::LazyLock;
    static SHIPPED: LazyLock<Result<Revision, String>> =
        LazyLock::new(|| Revision::read(SHIPPED_TEXT));
    &SHIPPED
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../assets/fixture.json");

    /// The shipped object is a revision: it verifies, it says what it is, and it carries the whole catalog.
    #[test]
    fn the_shipped_revision_verifies_and_keeps_the_synthetic_status() {
        let revision = shipped().as_ref().expect("the shipped bytes verify");
        assert_eq!(revision.id, SHIPPED_ID);
        assert_eq!(revision.published_at, "2026-08-13");
        assert_eq!(revision.status, "SYNTHETIC / NOT VENDOR DATA");
        assert_eq!(revision.format_version, FORMAT_VERSION);
        assert_eq!(
            revision.sha256.len(),
            64,
            "a declared digest is 64 hex chars"
        );
        assert!(revision.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            revision.sha256,
            digest_of(SHIPPED_TEXT).expect("the rule reads")
        );
        assert_eq!(
            digest_of(SHIPPED_TEXT).expect("the rule reads"),
            revision.sha256,
            "the declared digest is the file's own"
        );
        let ids = revision.record_ids();
        assert_eq!(ids["towers"].len(), 4);
        assert_eq!(ids["fills"].len(), 7);
        assert!(ids["fills"].iter().any(|id| id == "FILM-MF20"));
        assert!(revision.counts_line().contains("4 towers"));
    }

    /// The shipped revision carries exactly the fixture's `catalog` block - not a second, drifting copy.
    #[test]
    fn the_shipped_records_are_the_fixture_s_own_catalog_block() {
        let revision = shipped().as_ref().expect("the shipped bytes verify");
        let fixture: Value = serde_json::from_str(FIXTURE).expect("the fixture parses");
        assert_eq!(
            revision.records(),
            fixture
                .get("catalog")
                .expect("the fixture carries a catalog"),
            "the shipped revision's records are the fixture's catalog, record for record"
        );
    }

    /// The digest rule, byte for byte: everything but the declared value, and nothing else.
    #[test]
    fn the_digest_rule_covers_every_byte_but_the_declared_value() {
        let a = "{\"id\":\"x\",\"sha256\":\"00\",\"records\":[1]}";
        let b = "{\"id\":\"x\",\"sha256\":\"ffee00aa\",\"records\":[1]}";
        assert_eq!(
            digest_of(a).expect("reads"),
            digest_of(b).expect("reads"),
            "the declared value's own bytes are what the rule drops"
        );
        let record_moved = "{\"id\":\"x\",\"sha256\":\"00\",\"records\":[2]}";
        assert_ne!(
            digest_of(a).expect("reads"),
            digest_of(record_moved).expect("reads"),
            "one digit inside a record moves the digest"
        );
        let reformatted = "{\n  \"id\": \"x\",\n  \"sha256\": \"00\",\n  \"records\": [1]\n}\n";
        assert_ne!(
            digest_of(a).expect("reads"),
            digest_of(reformatted).expect("reads"),
            "the digest covers the published bytes, whitespace included"
        );
        assert_eq!(
            hash_input("{\"id\":\"x\",\"sha256\":\"00\",\"records\":[1]}").expect("reads"),
            "{\"id\":\"x\",\"sha256\":\"\",\"records\":[1]}"
        );
        assert!(
            Revision::read("{\"formatVersion\":1,\"id\":\"x\"}").is_err(),
            "a revision with no sha256 field cannot be verified at all"
        );
    }

    /// RED/GREEN, in memory: a file with one hand-edited id is refused, and the refusal names the
    /// revision, the mismatch and both digests. Untouched bytes verify.
    #[test]
    fn a_record_edited_by_hand_is_refused_naming_both_digests() {
        let good = SHIPPED_TEXT;
        let declared = shipped().as_ref().expect("verifies").sha256.clone();
        assert_eq!(
            good.matches("FILM-MF20").count(),
            2,
            "one fill record, one note"
        );
        let tampered = good.replace("FILM-MF20", "FILM-MF21");
        assert_ne!(tampered, good, "the mutation has to land");

        let error = Revision::read(&tampered).expect_err("a tampered revision is refused");
        assert!(
            error.contains(SHIPPED_ID),
            "the refusal names the revision: {error}"
        );
        assert!(
            error.contains("sha256 mismatch"),
            "the refusal names the failure: {error}"
        );
        assert!(
            error.contains(&declared),
            "the refusal names the declared digest: {error}"
        );
        let computed = digest_of(&tampered).expect("the rule reads");
        assert!(
            error.contains(&computed),
            "the refusal names the digest the bytes actually hash to: {error}"
        );
        assert!(error.contains("immutable"), "the refusal says why: {error}");
        assert!(
            !tampered.is_empty() && tampered.len() == good.len(),
            "the tamper moved no length, only bytes - so a size check could never have caught it"
        );

        // GREEN again on the untouched bytes: the refusal is about the file, not about a poisoned cache.
        let revision = Revision::read(good).expect("the untouched bytes verify");
        assert_eq!(revision.sha256, declared);
    }

    /// The same refusal through the file path the native menu uses, with the restore proved byte-identically.
    #[test]
    fn a_tampered_copy_on_disk_is_refused_then_restored_byte_identically() {
        let dir = std::env::temp_dir().join("drafthouse-revision-tamper-74");
        std::fs::create_dir_all(&dir).expect("a temp directory to tamper in");
        let path = dir.join("revision-illustrative-catalog-v0.1.json");
        std::fs::write(&path, SHIPPED_TEXT).expect("write the published bytes");
        assert_eq!(
            std::fs::read(&path).expect("read back"),
            SHIPPED_TEXT.as_bytes(),
            "the copy starts byte-identical to the shipped object"
        );
        Revision::from_file(&path).expect("the published bytes verify from disk");

        let tampered = SHIPPED_TEXT.replace("SYNTHETIC / NOT VENDOR DATA", "VENDOR DATA");
        assert_ne!(tampered, SHIPPED_TEXT, "the mutation has to land");
        std::fs::write(&path, &tampered).expect("write the tampered copy");
        let error = Revision::from_file(&path).expect_err("the tampered copy is refused");
        assert!(error.contains("sha256 mismatch"), "{error}");
        assert!(
            error.contains(&path.display().to_string()),
            "the refusal names the file it read: {error}"
        );

        std::fs::write(&path, SHIPPED_TEXT).expect("restore the published bytes");
        assert_eq!(
            std::fs::read(&path).expect("read back"),
            SHIPPED_TEXT.as_bytes(),
            "restored byte-identically"
        );
        Revision::from_file(&path).expect("the restored bytes verify");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
