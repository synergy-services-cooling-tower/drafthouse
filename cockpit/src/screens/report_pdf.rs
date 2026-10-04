//! Issue #85: the calculation-report export - the fixture/current project and workspace as a real
//! **PDF calc sheet**, built on [`crate::pdf`] (the dependency-free writer [the fence on
//! `cockpit/Cargo.toml` demands](../pdf.rs)).
//!
//! The document is the Report screen's sheet, page for page: cover (project, tower, date, revision,
//! author), inputs, **every** worked step (the screen truncates with "continue on the next sheet";
//! the PDF continues onto real pages), results including the air-path split, vector charts
//! (fan-vs-system, the air-path split, cold-water-vs-wet-bulb, the candidate scatter, the water
//! balance, the section schematic), and the validation + limitations page. The validation-status
//! statement is emitted by [`validation_statement`] and called unconditionally by [`document`]:
//! there is no flag, account, draft mode or code path that produces a document without it (the
//! module's own test mutates the call away to prove the test bites).
//!
//! Two delivery paths, one byte stream: [`deliver`] is a download on wasm and a save on native
//! (`native: ~/Downloads/<name>`), and the native binary's `--export-pdf <path>` drives the same
//! [`document`] headless.
//!
//! **The revision row ([`Revision`]).** When a saved or opened `.drafthouse` project document backs
//! the export, the row carries the document's own digest: SHA-256 over the document's canonical
//! bytes, produced at export time through the session's own writer (issue #74's `files::snapshot_text`
//! - the same call the File menu's save makes), so the value describes the state the sheet was built
//! from. Project writes are deterministic (write -> read -> write is byte-equal), so for a project
//! saved and exported unchanged that digest IS the saved file's own, byte for byte, and a second
//! export reproduces it; a project modified after its save carries the working document's digest and
//! says so. A sheet exported from a draft with **no** document behind it carries the clearly-labelled
//! [`state_hash`] fallback instead - a state hash, never presented as a document revision. Adding a
//! self-digest field to the project file was considered and is deliberately not done: the digest is
//! computed from the saved bytes at export time, and the file format is a shared contract.

use cockpit::engine::{EngineInput, EngineOutput, Limit, WorkedStep};

use super::data;
use super::kit;
use super::DocumentBytes;
use crate::pdf::{self, Font, Pdf};

// ======================================================================================= palette

const INK: [u8; 3] = [0x1d, 0x26, 0x2c];
const INK2: [u8; 3] = [0x4b, 0x58, 0x60];
const MUTED: [u8; 3] = [0x77, 0x82, 0x89];
const RULE: [u8; 3] = [0xd6, 0xd0, 0xc4];
const PAPER: [u8; 3] = [0xf4, 0xf1, 0xea];
const TEAL: [u8; 3] = [0x0b, 0x60, 0x73];
const OK_SOFT: [u8; 3] = [0xdf, 0xee, 0xe5];
const DANGER: [u8; 3] = [0xb0, 0x30, 0x30];
const DANGER_SOFT: [u8; 3] = [0xf6, 0xdd, 0xdb];
const WATERMARK: [u8; 3] = [0xe0, 0xd9, 0xcb];

// ======================================================================================= geometry

/// The sheet's margins (the screen's `sheet` paints a 40 pt margin on A4).
const M: f64 = 40.0;
/// Usable width.
const W: f64 = pdf::A4_W - 2.0 * M;
/// Where a page's body starts (below the running header rule).
const TOP: f64 = 104.0;
/// Where a page's body must end (above the footer).
const BOTTOM: f64 = 782.0;

/// The numbers the screen's `sheet` prints (kept here so the extraction test and the PDF share one
/// spelling - the on-screen figure is the same `{:.2}`/`{:.1}` formatting of the same engine field).
fn f2(v: f64) -> String {
    format!("{v:.2}")
}
fn f1(v: f64) -> String {
    format!("{v:.1}")
}
fn f0(v: f64) -> String {
    format!("{v:.0}")
}
fn f3(v: f64) -> String {
    format!("{v:.3}")
}

// ======================================================================================= meta

/// The cover's provenance fields. `project` and `report_no` are the cockpit's STUB constants (the
/// `.drafthouse` format carries no name field); `state_hash` is [`state_hash`]'s output (the
/// fallback identity of a draft with no document behind it) and `revision` is what the revision row
/// actually carries - see [`Revision`].
#[derive(Clone, Debug)]
pub struct Meta {
    pub project: String,
    pub report_no: String,
    pub date: String,
    pub author: String,
    pub state_hash: String,
    /// The revision row's value and caption ([`Meta::carry_document`] replaces the draft fallback
    /// when the export is built from a saved project document).
    pub revision: Revision,
    pub demo: bool,
}

/// What the sheet's revision row carries.
///
/// [`Revision::Document`] is the project document's own identity: SHA-256 over the `.drafthouse`
/// document's canonical bytes - the bytes the writer produces ([`crate::files::snapshot_text`],
/// issue #74) from the state the sheet was built from. Project writes are deterministic (write ->
/// read -> write is byte-equal), so a project saved and exported unchanged carries the saved file's
/// own digest, byte for byte; a project modified after its save carries the working document's
/// digest and is labelled as such.
///
/// [`Revision::DraftState`] is the honest fallback for a sheet exported from a draft with **no**
/// document behind it: [`state_hash`]'s output over the draft input, labelled a state hash. It is
/// never presented as a document revision.
#[derive(Clone, Debug, PartialEq)]
pub enum Revision {
    /// A saved or opened project document backs the sheet.
    Document {
        /// SHA-256 (lowercase hex) of the document's canonical bytes.
        sha256: String,
        /// The session reports no unsaved changes against the document it last saved or opened, so
        /// this digest is that document's own bytes.
        unchanged: bool,
    },
    /// No project document backs the sheet: the draft-input state hash.
    DraftState {
        /// [`state_hash`]'s output over the exported draft input.
        state_hash: String,
    },
}

impl Revision {
    /// The 64-character lowercase hex the revision row prints.
    pub fn value(&self) -> &str {
        match self {
            Revision::Document { sha256, .. } => sha256,
            Revision::DraftState { state_hash } => state_hash,
        }
    }

    /// The short name the page footers carry for what the value is.
    pub fn short_name(&self) -> &'static str {
        match self {
            Revision::Document {
                unchanged: true, ..
            } => "project document",
            Revision::Document {
                unchanged: false, ..
            } => "working document",
            Revision::DraftState { .. } => "project state hash",
        }
    }

    /// The cover's caption under the revision row: what the value is and which document it names.
    pub fn caption(&self) -> String {
        match self {
            Revision::Document { unchanged: true, .. } => {
                "project document - sha-256 of the .drafthouse bytes this sheet was built from; \
                 the project writer is deterministic, so this is the saved file's own digest"
                    .to_string()
            }
            Revision::Document { unchanged: false, .. } => {
                "working document - sha-256 of the .drafthouse bytes this sheet was built from; \
                 the working state has unsaved changes, so it differs from the file's digest"
                    .to_string()
            }
            Revision::DraftState { .. } => {
                "project state hash - sha-256 of this draft's input; no saved project document \
                 backs this sheet (save the project and export again to carry the document's digest)"
                    .to_string()
            }
        }
    }

    /// The validation page's sentence for the revision row - what the value is, in full.
    pub fn note(&self) -> String {
        match self {
            Revision::Document {
                sha256,
                unchanged: true,
            } => format!(
                "Revision {sha256}: the sha-256 of the .drafthouse project document this sheet was \
                 built from - the saved file's own digest. Project writes are deterministic (write, \
                 read, write are byte-equal), so a second export of the same document carries the \
                 same revision."
            ),
            Revision::Document {
                sha256,
                unchanged: false,
            } => format!(
                "Revision {sha256}: the sha-256 of the .drafthouse project document this sheet was \
                 built from. The working state has unsaved changes, so this digest is not the saved \
                 file's - save the project to pin the sheet's revision to a file."
            ),
            Revision::DraftState { state_hash } => format!(
                "Revision {state_hash}: a project state hash, not a project document's digest - \
                 this sheet was exported from a draft with no saved document behind it. Save the \
                 project (.drafthouse) and export again to carry the document's sha-256."
            ),
        }
    }
}

impl Meta {
    /// Issue #85 completion: carry the project document's digest in the revision row. Called by the
    /// export surfaces that hold the session's document (the Report screen's button); a path with
    /// no session (the headless `--export-pdf`) keeps the labelled draft fallback.
    pub fn carry_document(&mut self, document: &DocumentBytes) {
        self.revision = Revision::Document {
            sha256: sha256_hex(document.text.as_bytes()),
            unchanged: document.unchanged,
        };
    }
}

/// The export metadata for the app's own account state: the stubs, the wall-clock date (UTC), a
/// freshly computed state hash, and the draft fallback revision ([`Meta::carry_document`] replaces
/// it when the export is built from a saved project document). `staff` is the signed-in account (the
/// stub identity); `false` is the DEMO account, whose export is watermarked.
pub fn meta_for_export(staff: bool, draft: &EngineInput) -> Meta {
    let hash = state_hash(draft);
    Meta {
        project: data::STUB_PROJECT.to_string(),
        report_no: data::STUB_REPORT_NO.to_string(),
        date: wall_date_utc(),
        author: if staff {
            data::STUB_USER.to_string()
        } else {
            "not signed in (DEMO sample)".to_string()
        },
        state_hash: hash.clone(),
        revision: Revision::DraftState { state_hash: hash },
        demo: !staff,
    }
}

/// The suggested file name: the state hash's first 8 hex characters identify the export.
pub fn file_name(state_hash: &str) -> String {
    format!("drafthouse-calc-sheet-{}.pdf", &state_hash[..8])
}

// ======================================================================================= hash

/// A deterministic content hash of the exported project state (the draft), SHA-256 over a versioned
/// canonical form: the bytes `"drafthouse project state v1\n"` followed by the draft's compact
/// JSON. Same draft, same hash; any edited field, a different one.
///
/// This is the **fallback** identity for a sheet exported from a draft with no saved document behind
/// it ([`Revision::DraftState`]): a hash of the draft *input*, not of a `.drafthouse` document, and
/// the document never presents it as a document revision. When a saved project document backs the
/// export, the revision row carries that document's own digest instead ([`Revision::Document`],
/// through [`Meta::carry_document`]).
pub fn state_hash(draft: &EngineInput) -> String {
    let mut bytes = b"drafthouse project state v1\n".to_vec();
    if let Ok(json) = serde_json::to_vec(draft) {
        bytes.extend_from_slice(&json);
    }
    sha256_hex(&bytes)
}

/// SHA-256, hex. Written here because the crate's dependencies are fenced and std has no digest.
/// Pinned against the standard vectors by the module's tests.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for t in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[t])
                .wrapping_add(w[t]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

/// The wall-clock date, UTC. The sheet's clock is process-relative, so this reads the system clock
/// (`js_sys::Date::now()` on wasm; `SystemTime` on native) and names its scale (UTC) so no reader
/// mistakes it for local time.
pub fn wall_date_utc() -> String {
    let ms = wall_ms();
    let (y, m, d) = civil_from_days((ms / 86_400_000.0).floor() as i64);
    format!("{y:04}-{m:02}-{d:02} (UTC)")
}

#[cfg(target_arch = "wasm32")]
fn wall_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn wall_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's `civil_from_days`, the inverse of the
/// well-known `days_from_civil`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ======================================================================================= the statement

/// The validation-status statement - **the one that cannot be turned off**. [`document`] emits it on
/// every document it builds; the wording is the screen's (clean vs refused, and the refused case
/// names how many limits the run reported).
pub fn validation_statement(o: &EngineOutput) -> String {
    let fails = o.validation.iter().filter(|l| outside(l)).count();
    if fails == 0 {
        "The engine refused nothing: every record range this run passed through held.".to_string()
    } else {
        format!(
            "The engine refused this run: {} limit(s) outside their range.",
            o.validation.len()
        )
    }
}

/// The limitations statement the sheet always carries (the export's counterpart of
/// `docs/LIMITATIONS.md` §1/§3/§4, in one paragraph).
pub const LIMITATIONS: &str =
    "This sheet is an engineering calculation record, not a rating. It is \
not certified by the Cooling Technology Institute: using Merkel-equation or curve-intersection \
methods does not make a program certified, and the output must not be used as a contractual \
acceptance determination. Every tower, fill, drift, fan and nozzle record in this build is \
synthetic/illustrative, not vendor data; the selection it drives must not influence procurement, \
guarantees or field modifications. The method assumes uniform water and dry-air loading, uniform \
fill depth, one inlet-air state and components in design condition; real towers maldistribute, \
foul, recirculate and bypass, and this sheet models none of that.";

/// The notes the export adds about itself, below the limitations: the revision row's own sentence
/// ([`Revision::note`] - what the carried value is, per the document state, with no placeholder
/// language left to retire).
pub fn export_notes(meta: &Meta) -> Vec<String> {
    let mut notes = vec![meta.revision.note()];
    if meta.demo {
        notes.push(
            "DEMO sample: the document is watermarked and unsigned; the staff export carries the \
             same pages without the watermark."
                .to_string(),
        );
    }
    notes
}

/// A limit is outside its recorded band (the screen's `outside`, on paper).
fn outside(l: &Limit) -> bool {
    l.min.map(|m| l.value < m).unwrap_or(false) || l.max.map(|m| l.value > m).unwrap_or(false)
}

// ======================================================================================= steps plan

/// One stage of a step's block: `n` wrapped lines at `line_h`.
struct StepPart {
    text: String,
    font: Font,
    size: f64,
    line_h: f64,
    rgb: [u8; 3],
}

fn step_parts(step: &WorkedStep) -> Vec<StepPart> {
    let mut parts = Vec::new();
    match (&step.formula, step.why.is_empty()) {
        (Some(formula), _) => parts.push(StepPart {
            text: formula.clone(),
            font: Font::Mono,
            size: 8.0,
            line_h: 10.0,
            rgb: TEAL,
        }),
        (None, false) => parts.push(StepPart {
            text: step.why.clone(),
            font: Font::Sans,
            size: 8.0,
            line_h: 10.0,
            rgb: INK2,
        }),
        _ => {}
    }
    if let Some(substitution) = &step.substitution {
        parts.push(StepPart {
            text: substitution.clone(),
            font: Font::Mono,
            size: 7.5,
            line_h: 9.5,
            rgb: INK2,
        });
    }
    if let Some(reference) = &step.reference {
        parts.push(StepPart {
            text: format!("ref · {reference}"),
            font: Font::Mono,
            size: 7.0,
            line_h: 9.0,
            rgb: MUTED,
        });
    }
    parts
}

/// A step's block height, measured with the writer's own metric - the plan and the paint share it,
/// so the page count the screen's card prints cannot drift from the pages the PDF has.
fn step_height(step: &WorkedStep) -> f64 {
    let body_w = W - 18.0;
    let mut h = 16.0;
    for part in step_parts(step) {
        h += pdf::wrap(part.font, part.size, &part.text, body_w).len() as f64 * part.line_h;
    }
    h + 6.0
}

/// Which steps land on which steps page. `step_height` measures; a page takes steps until the next
/// block would cross [`BOTTOM`]. At least one step per page (an over-tall block overflows rather
/// than loops).
pub fn plan_steps(steps: &[WorkedStep]) -> Vec<Vec<usize>> {
    let avail = BOTTOM - (TOP + 26.0);
    let mut pages: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (i, step) in steps.iter().enumerate() {
        let h = step_height(step);
        if !pages.last().expect("one page").is_empty() && used + h > avail {
            pages.push(Vec::new());
            used = 0.0;
        }
        used += h;
        pages.last_mut().expect("one page").push(i);
    }
    pages
}

/// The document's page count: cover + inputs + the step pages + results + charts + validation.
pub fn page_count(steps: &[WorkedStep]) -> usize {
    5 + plan_steps(steps).len()
}

// ======================================================================================= document

/// Build the whole calc sheet. Every section of the Report screen is carried; the validation
/// statement is emitted on the validation page unconditionally. The bytes are deterministic for the
/// same inputs (no clock reads: the date is in [`Meta`]).
pub fn document(d: &EngineInput, o: &EngineOutput, cache: &data::Cache, meta: &Meta) -> Vec<u8> {
    let plan = plan_steps(&o.worked_steps);
    let first_steps = 3usize;
    let page_of_results = 3 + plan.len();
    let page_of_charts = 4 + plan.len();
    let page_of_validation = 5 + plan.len();
    let total = 5 + plan.len();

    let mut pdf = Pdf::new("Drafthouse calculation sheet");
    cover(
        &mut pdf,
        d,
        o,
        meta,
        total,
        page_of_results,
        page_of_charts,
        page_of_validation,
    );
    inputs_page(&mut pdf, d, o, meta, 2, total);
    for (i, page) in plan.iter().enumerate() {
        steps_page(&mut pdf, o, meta, i, page, first_steps, total);
    }
    results_page(&mut pdf, o, meta, page_of_results, total);
    charts_page(&mut pdf, d, o, cache, meta, page_of_charts, total);
    validation_page(&mut pdf, d, o, meta, page_of_validation, total);
    pdf.finish()
}

// ------------------------------------------------------------------ page furniture

fn page_open(pdf: &mut Pdf, meta: &Meta, no: usize, total: usize) {
    pdf.rect(0.0, 0.0, pdf::A4_W, pdf::A4_H, Some(PAPER), None);
    if meta.demo {
        pdf.text_rot(150.0, 700.0, 96.0, Font::Bold, WATERMARK, 32.0, "DEMO");
    }
    pdf.text(M, 46.0, 8.0, Font::Bold, TEAL, "DRAFTHOUSE · CALC SHEET");
    pdf.text_right(
        M + W,
        46.0,
        8.0,
        Font::Mono,
        INK2,
        &format!("{} · {no} / {total}", meta.report_no),
    );
    pdf.line(M, 57.0, M + W, 57.0, 0.8, RULE);
}

fn page_footer(pdf: &mut Pdf, o: &EngineOutput, meta: &Meta) {
    pdf.line(M, 800.0, M + W, 800.0, 0.5, RULE);
    pdf.text(
        M,
        810.0,
        7.0,
        Font::Mono,
        INK2,
        &format!(
            "{} · {}",
            o.provenance.catalog_id, o.provenance.catalog_status
        ),
    );
    pdf.text_right(
        M + W,
        810.0,
        7.0,
        Font::Mono,
        INK2,
        &format!(
            "{} {}…",
            meta.revision.short_name(),
            &meta.revision.value()[..12]
        ),
    );
}

/// A section head: "1  Inputs" and a rule under it.
fn section_head(pdf: &mut Pdf, title: &str) {
    pdf.text(M, TOP, 13.0, Font::Bold, INK, title);
    pdf.line(M, TOP + 8.0, M + W, TOP + 8.0, 1.0, RULE);
}

/// One label-left / value-right row with a hairline under it; `u` empty for a bare value.
fn row(pdf: &mut Pdf, y: f64, label: &str, value: &str, u: &str) {
    pdf.text_fit(M, y, 9.5, Font::Sans, INK2, label, W * 0.45);
    let vs = if u.is_empty() {
        value.to_string()
    } else {
        format!("{value} {u}")
    };
    pdf.text_right(M + W, y, 9.5, Font::Mono, INK, &vs);
    pdf.line(M, y + 6.0, M + W, y + 6.0, 0.5, RULE);
}

/// A sub-head inside a page.
fn sub_head(pdf: &mut Pdf, y: f64, title: &str) {
    pdf.text(M, y, 9.5, Font::Bold, TEAL, title);
}

// ------------------------------------------------------------------ cover

#[allow(clippy::too_many_arguments)]
fn cover(
    pdf: &mut Pdf,
    d: &EngineInput,
    o: &EngineOutput,
    meta: &Meta,
    total: usize,
    page_of_results: usize,
    page_of_charts: usize,
    page_of_validation: usize,
) {
    page_open(pdf, meta, 1, total);
    pdf.text(M, 132.0, 23.0, Font::Bold, INK, "Thermal & airside");
    pdf.text(M, 164.0, 23.0, Font::Bold, INK, "calculation sheet");

    // project + tower
    pdf.text(M, 216.0, 12.0, Font::Sans, INK2, &meta.project);
    pdf.text(
        M,
        232.0,
        7.5,
        Font::Mono,
        MUTED,
        "(stub: the project name and report number are placeholders - the .drafthouse format carries \
         no name field)",
    );
    let tower = if d.tower.name.is_empty() {
        d.tower.id.clone()
    } else {
        format!("{} · {}", d.tower.name, d.tower.tower_type)
    };
    pdf.text(M, 262.0, 11.0, Font::Sans, INK2, &tower);
    pdf.text(
        M + 300.0,
        262.0,
        9.0,
        Font::Mono,
        INK2,
        &format!("tower {}", d.tower.id),
    );

    // date / revision / author
    let mut y = 296.0;
    pdf.text(M, y, 9.0, Font::Sans, INK2, "date");
    pdf.text(M + 90.0, y, 8.5, Font::Mono, INK, &meta.date);
    y += 18.0;
    pdf.text(M, y, 9.0, Font::Sans, INK2, "revision");
    pdf.text(M + 90.0, y, 8.0, Font::Mono, INK, meta.revision.value());
    let note = pdf::wrap(Font::Mono, 6.5, &meta.revision.caption(), W - 90.0);
    for (i, line) in note.iter().enumerate() {
        pdf.text(
            M + 90.0,
            y + 10.0 + i as f64 * 8.0,
            6.5,
            Font::Mono,
            MUTED,
            line,
        );
    }
    y += 10.0 + note.len() as f64 * 8.0 + 8.0;
    pdf.text(M, y, 9.0, Font::Sans, INK2, "author");
    pdf.text(M + 90.0, y, 8.5, Font::Mono, INK, &meta.author);

    // the headline, big
    let hy = 400.0;
    pdf.line(M, hy, M + W, hy, 1.2, INK);
    let cw = W / 3.0;
    for (i, (label, value, unit)) in [
        ("cold water", f2(o.cold_water_c), "°C"),
        ("capability", f1(o.capability_pct), "%"),
        ("fan power", f1(o.fan_power_kw), "kW"),
    ]
    .iter()
    .enumerate()
    {
        let x = M + cw * i as f64;
        pdf.text(x, hy + 18.0, 9.0, Font::Sans, INK2, label);
        pdf.text(x, hy + 46.0, 21.0, Font::Bold, INK, value);
        let w = pdf::width(Font::Bold, 21.0, value);
        pdf.text(x + w + 4.0, hy + 46.0, 9.0, Font::Sans, INK2, unit);
    }

    // contents
    let mut cy = 502.0;
    sub_head(pdf, cy, "Contents");
    cy += 20.0;
    let steps_label = if page_of_results - 3 == 1 {
        format!("{}", 3)
    } else {
        format!("3-{}", page_of_results - 1)
    };
    let toc: [(String, String, String); 5] = [
        (
            "1".into(),
            "Inputs".into(),
            format!(
                "duty, tower, {} fill layers, fan, eliminator, nozzle",
                d.fill_layers.len()
            ),
        ),
        (
            "2".into(),
            "Worked steps".into(),
            format!(
                "{} steps, each with its formula, substitution and reference",
                o.worked_steps.len()
            ),
        ),
        (
            "3".into(),
            "Results".into(),
            "cold water, capability, airside, KaV/L by layer, air-path split".into(),
        ),
        (
            "4".into(),
            "Charts".into(),
            "section, air-path split, fan vs system, cold water vs wet bulb, scatter, water balance"
                .into(),
        ),
        (
            "5".into(),
            "Validation & limitations".into(),
            "the engine's refusals, the limitations statement, this export's own notes".into(),
        ),
    ];
    let pages = [
        "2".to_string(),
        steps_label,
        format!("{page_of_results}"),
        format!("{page_of_charts}"),
        format!("{page_of_validation}"),
    ];
    for (i, (no, title, sub)) in toc.iter().enumerate() {
        pdf.text(M, cy, 9.0, Font::Mono, INK2, no);
        pdf.text(M + 18.0, cy, 10.0, Font::Bold, INK, title);
        pdf.text_fit(M + 18.0, cy + 12.0, 8.0, Font::Sans, INK2, sub, W - 60.0);
        pdf.text_right(M + W, cy, 9.0, Font::Mono, INK2, &pages[i]);
        cy += 30.0;
    }

    // sign-off boxes
    let by = 690.0;
    let bw = (W - 16.0) / 3.0;
    for (i, label) in ["Prepared", "Checked", "Approved"].iter().enumerate() {
        let bx = M + (bw + 8.0) * i as f64;
        pdf.rect(bx, by, bw, 56.0, None, Some((0.7, RULE)));
        pdf.text(bx + 6.0, by + 12.0, 8.0, Font::Bold, INK2, label);
        pdf.text(bx + 6.0, by + 48.0, 7.5, Font::Sans, MUTED, "name · date");
    }

    // provenance
    let mut py = 762.0;
    for (label, value) in [
        ("engine", o.provenance.engine.as_str()),
        ("catalog", o.provenance.catalog_id.as_str()),
        ("revision", o.provenance.catalog_revision.as_str()),
        ("status", o.provenance.catalog_status.as_str()),
    ] {
        pdf.text(M, py, 8.0, Font::Sans, INK2, label);
        pdf.text_fit(M + 60.0, py, 8.0, Font::Mono, INK, value, W - 60.0);
        py += 13.0;
    }

    page_footer(pdf, o, meta);
}

// ------------------------------------------------------------------ inputs

fn results_rows(o: &EngineOutput) -> Vec<(&'static str, String, &'static str)> {
    vec![
        ("cold water", f2(o.cold_water_c), "°C"),
        ("range", f2(o.range_c), "K"),
        ("approach", f2(o.approach_c), "K"),
        ("capability", f1(o.capability_pct), "%"),
        ("KaV/L", f3(o.kavl_total), ""),
        ("airflow", f2(o.airflow_m3_s), "m³/s"),
        ("total pressure", f1(o.total_pressure_pa), "Pa"),
        ("fan power", f2(o.fan_power_kw), "kW"),
        ("evaporation", f2(o.evaporation_pct), "%"),
        ("make-up", f2(o.makeup_m3_hr), "m³/h"),
    ]
}

#[allow(clippy::too_many_arguments)]
fn steps_page(
    pdf: &mut Pdf,
    o: &EngineOutput,
    meta: &Meta,
    page_index: usize,
    step_indices: &[usize],
    first_steps: usize,
    total: usize,
) {
    let no = first_steps + page_index;
    pdf.page();
    page_open(pdf, meta, no, total);
    let first = step_indices.first().map(|i| i + 1).unwrap_or(0);
    let last = step_indices.last().map(|i| i + 1).unwrap_or(0);
    section_head(
        pdf,
        &format!(
            "2  Worked steps ({first}-{last} of {})",
            o.worked_steps.len()
        ),
    );
    let mut y = TOP + 26.0;
    for &i in step_indices {
        let step = &o.worked_steps[i];
        pdf.text(M, y, 8.0, Font::Mono, INK2, &format!("{}", i + 1));
        pdf.text_fit(
            M + 18.0,
            y,
            9.5,
            Font::Bold,
            INK,
            &step.label,
            W - 18.0 - 110.0,
        );
        if let Some(value) = step.value {
            let vs = if step.unit.is_empty() {
                kit::fmt2(value)
            } else {
                format!("{} {}", kit::fmt2(value), step.unit)
            };
            pdf.text_right(M + W, y, 9.5, Font::Mono, INK, &vs);
        }
        y += 12.0;
        for part in step_parts(step) {
            for line in pdf::wrap(part.font, part.size, &part.text, W - 18.0) {
                pdf.text(M + 18.0, y, part.size, part.font, part.rgb, &line);
                y += part.line_h;
            }
        }
        y += 2.0;
        pdf.line(M, y, M + W, y, 0.5, RULE);
        y += 6.0;
    }
    page_footer(pdf, o, meta);
}

// ------------------------------------------------------------------ inputs

fn inputs_page(
    pdf: &mut Pdf,
    d: &EngineInput,
    o: &EngineOutput,
    meta: &Meta,
    no: usize,
    total: usize,
) {
    pdf.page();
    page_open(pdf, meta, no, total);
    section_head(pdf, "1  Inputs");
    let mut y = TOP + 26.0;
    sub_head(pdf, y, "Duty");
    y += 18.0;
    let duty: [(&str, String, &str); 9] = [
        ("water flow", f1(d.duty.water_flow_m3_hr), "m³/h"),
        ("hot water", f2(d.duty.hot_water_c), "°C"),
        ("target cold water", f2(d.duty.target_cold_water_c), "°C"),
        ("wet bulb", f2(d.duty.wet_bulb_c), "°C"),
        ("dry bulb", f2(d.duty.dry_bulb_c), "°C"),
        ("pressure", f0(d.duty.pressure_pa), "Pa"),
        ("cycles", f1(d.duty.cycles_of_concentration), "×"),
        (
            "water quality class",
            d.duty.water_quality_class.clone(),
            "",
        ),
        ("salinity", f2(d.duty.salinity_g_kg), "g/kg"),
    ];
    for (label, value, unit) in &duty {
        row(pdf, y, label, value, unit);
        y += 19.0;
    }
    y += 14.0;
    sub_head(pdf, y, "Tower & equipment");
    y += 18.0;
    row(pdf, y, "tower", &d.tower.id, "");
    y += 19.0;
    row(pdf, y, "fill area", &f1(d.tower.fill_area_m2), "m²");
    y += 19.0;
    for (i, l) in d.fill_layers.iter().enumerate() {
        row(
            pdf,
            y,
            &format!("fill layer {}", i + 1),
            &format!("{} · {}", l.fill_id, f2(l.depth_m)),
            "m",
        );
        y += 19.0;
    }
    row(
        pdf,
        y,
        "fan · speed",
        &format!("{} · {}", d.fan.id, f2(d.speed_ratio)),
        "",
    );
    y += 19.0;
    row(pdf, y, "drift eliminator", &d.drift.id, "");
    y += 19.0;
    row(pdf, y, "nozzle", &d.nozzle.id, "");
    y += 30.0;
    // the same provenance the cover carries, in full, because the inputs page is what a reviewer
    // reads with the numbers
    sub_head(pdf, y, "Recorded provenance");
    y += 18.0;
    for (label, value) in [
        ("engine", o.provenance.engine.as_str()),
        ("catalog", o.provenance.catalog_id.as_str()),
        ("cat. revision", o.provenance.catalog_revision.as_str()),
        ("catalog status", o.provenance.catalog_status.as_str()),
    ] {
        row(pdf, y, label, value, "");
        y += 19.0;
    }
    page_footer(pdf, o, meta);
}

// ------------------------------------------------------------------ results

fn results_page(pdf: &mut Pdf, o: &EngineOutput, meta: &Meta, no: usize, total: usize) {
    pdf.page();
    page_open(pdf, meta, no, total);
    section_head(pdf, "3  Results");
    let mut y = TOP + 26.0;
    for (label, value, unit) in results_rows(o) {
        row(pdf, y, label, &value, unit);
        y += 19.0;
    }
    y += 14.0;
    sub_head(pdf, y, "KaV/L by layer");
    y += 18.0;
    for l in &o.kavl_per_layer {
        row(
            pdf,
            y,
            &format!("{} · {} m", l.fill_id, f2(l.depth_m)),
            &format!("{}  ({}%)", f3(l.kavl), f1(l.cooling_share_pct)),
            "",
        );
        y += 19.0;
    }
    let fill: f64 = o.kavl_per_layer.iter().map(|l| l.kavl).sum();
    let share: f64 = o.kavl_per_layer.iter().map(|l| l.cooling_share_pct).sum();
    row(
        pdf,
        y,
        "spray + rain zones",
        &format!("{}  ({}%)", f3(o.kavl_total - fill), f1(100.0 - share)),
        "",
    );
    y += 19.0;
    row(
        pdf,
        y,
        "total",
        &format!("{}  (100%)", f3(o.kavl_total)),
        "",
    );
    y += 30.0;
    sub_head(pdf, y, "Air path (pressure by zone)");
    y += 18.0;
    for z in &o.pressure_by_zone {
        row(
            pdf,
            y,
            &z.label,
            &format!("{} Pa", f1(z.pressure_pa)),
            &format!("({}%)", f1(z.share_pct)),
        );
        y += 19.0;
    }
    page_footer(pdf, o, meta);
}

// ------------------------------------------------------------------ charts

/// A rectangle in the writer's y-down space.
#[derive(Clone, Copy)]
struct B {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl B {
    fn left(&self) -> f64 {
        self.x
    }
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn top(&self) -> f64 {
        self.y
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

fn tick(v: f64, step: f64) -> String {
    let prec = if step < 1.0 { 1 } else { 0 };
    format!("{v:.prec$}")
}

/// The plot frame: grid, ticks, axis names; returns the plot box.
#[allow(clippy::too_many_arguments)]
fn axes(
    pdf: &mut Pdf,
    r: B,
    x: (f64, f64),
    y: (f64, f64),
    xs: f64,
    ys: f64,
    xl: &str,
    yl: &str,
) -> B {
    let plot = B {
        x: r.left() + 32.0,
        y: r.top() + 12.0,
        w: r.w - 38.0,
        h: r.h - 36.0,
    };
    let px = |v: f64| plot.left() + (v - x.0) / (x.1 - x.0) * plot.w;
    let py = |v: f64| plot.bottom() - (v - y.0) / (y.1 - y.0) * plot.h;
    pdf.rect(plot.x, plot.y, plot.w, plot.h, None, Some((0.5, RULE)));
    let mut v = (x.0 / xs).ceil() * xs;
    while v <= x.1 + 1e-9 {
        pdf.line(px(v), plot.top(), px(v), plot.bottom(), 0.3, RULE);
        pdf.text_center(
            px(v),
            plot.bottom() + 8.0,
            6.0,
            Font::Mono,
            INK2,
            &tick(v, xs),
        );
        v += xs;
    }
    let mut v = (y.0 / ys).ceil() * ys;
    while v <= y.1 + 1e-9 {
        pdf.line(plot.left(), py(v), plot.right(), py(v), 0.3, RULE);
        pdf.text_right(
            plot.left() - 3.0,
            py(v) + 2.0,
            6.0,
            Font::Mono,
            INK2,
            &tick(v, ys),
        );
        v += ys;
    }
    pdf.text_center(
        plot.left() + plot.w / 2.0,
        plot.bottom() + 17.0,
        6.5,
        Font::Sans,
        INK2,
        xl,
    );
    pdf.text(r.left(), r.top() + 2.0, 6.5, Font::Sans, INK2, yl);
    plot
}

fn map(plot: B, x: (f64, f64), y: (f64, f64), q: (f64, f64)) -> (f64, f64) {
    (
        plot.left() + (q.0 - x.0) / (x.1 - x.0) * plot.w,
        plot.bottom() - (q.1 - y.0) / (y.1 - y.0) * plot.h,
    )
}

fn dashed(pdf: &mut Pdf, a: (f64, f64), b: (f64, f64), w: f64, rgb: [u8; 3]) {
    let len = ((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1)).sqrt();
    let steps = (len / 6.0).max(1.0);
    let (dx, dy) = ((b.0 - a.0) / steps, (b.1 - a.1) / steps);
    let mut i = 0.0;
    while i + 0.5 < steps {
        let from = (a.0 + dx * i, a.1 + dy * i);
        let to = (a.0 + dx * (i + 0.5), a.1 + dy * (i + 0.5));
        pdf.line(from.0, from.1, to.0, to.1, w, rgb);
        i += 1.0;
    }
}

fn cell_title(pdf: &mut Pdf, r: B, title: &str) -> B {
    pdf.text(r.left(), r.top() + 8.0, 8.0, Font::Bold, TEAL, title);
    B {
        y: r.top() + 12.0,
        h: r.h - 12.0,
        ..r
    }
}

fn pending(pdf: &mut Pdf, r: B, what: &str) {
    let (cx, cy) = r.center();
    pdf.text_center(cx, cy, 7.5, Font::Sans, INK2, &format!("{what}: computing"));
}

#[allow(clippy::too_many_arguments)]
fn chart_fan_system(pdf: &mut Pdf, r: B, o: &EngineOutput) {
    let r = cell_title(pdf, r, "Fan vs system (operating point)");
    let fsc = &o.fan_system_curve;
    if fsc.fan.points.len() < 2 || fsc.system.points.len() < 2 {
        return pending(pdf, r, "the fan curve");
    }
    let x1 = fsc
        .fan
        .points
        .iter()
        .chain(fsc.system.points.iter())
        .map(|q| q.x)
        .fold(0.0, f64::max);
    let y1 = fsc
        .fan
        .points
        .iter()
        .chain(fsc.system.points.iter())
        .map(|q| q.y)
        .fold(0.0, f64::max);
    if !(x1 > 0.0 && y1 > 0.0) {
        return pending(pdf, r, "the fan curve");
    }
    let x = (0.0, x1 * 1.04);
    let y = (0.0, y1 * 1.08);
    let plot = axes(
        pdf,
        r,
        x,
        y,
        super::rate::nice(x1 / 4.0),
        super::rate::nice(y1 / 4.0),
        "airflow m³/s",
        "Pa",
    );
    let fan: Vec<(f64, f64)> = fsc
        .fan
        .points
        .iter()
        .map(|q| map(plot, x, y, (q.x, q.y)))
        .collect();
    let sys: Vec<(f64, f64)> = fsc
        .system
        .points
        .iter()
        .map(|q| map(plot, x, y, (q.x, q.y)))
        .collect();
    pdf.polyline(&sys, 0.9, INK2);
    pdf.polyline(&fan, 1.3, TEAL);
    let op = map(plot, x, y, (fsc.operating_point.x, fsc.operating_point.y));
    pdf.circle(op.0, op.1, 3.0, 1.1, INK);
    pdf.text(
        op.0 + 6.0,
        op.1 - 9.0,
        6.5,
        Font::Mono,
        INK,
        &format!(
            "{} m³/s · {} Pa",
            f1(fsc.operating_point.x),
            f0(fsc.operating_point.y)
        ),
    );
}

fn chart_air_path(pdf: &mut Pdf, r: B, o: &EngineOutput) {
    let r = cell_title(pdf, r, "Air-path split (pressure by zone)");
    let zones = &o.pressure_by_zone;
    if zones.is_empty() {
        return pending(pdf, r, "the air path");
    }
    let total: f64 = zones.iter().map(|z| z.pressure_pa).sum::<f64>().max(1e-9);
    let mut y = r.top() + 8.0;
    let bar_x = r.left() + 96.0;
    let bar_w = r.w - 96.0 - 66.0;
    for z in zones {
        pdf.text_fit(r.left(), y, 6.0, Font::Mono, INK2, &z.label, 92.0);
        let len = bar_w * z.pressure_pa / total;
        pdf.rect(bar_x, y - 5.0, len.max(0.8), 6.0, Some(TEAL), None);
        pdf.text_right(
            r.right(),
            y,
            6.0,
            Font::Mono,
            INK,
            &format!("{} Pa ({}%)", f1(z.pressure_pa), f1(z.share_pct)),
        );
        y += 14.0;
    }
}

fn chart_cold_vs_wb(pdf: &mut Pdf, r: B, d: &EngineInput, cache: &data::Cache) {
    let r = cell_title(pdf, r, "Cold water vs wet bulb (80 / 100 / 120 % flow)");
    let Some(cd) = cache.curves.as_ref().filter(|c| !c.recs.is_empty()) else {
        return pending(pdf, r, "the curves");
    };
    let want = d.duty.hot_water_c - d.duty.target_cold_water_c;
    let range = data::CURVE_RANGE
        .iter()
        .copied()
        .min_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()))
        .unwrap_or(10.0);
    let recs: Vec<&data::CurveRec> = cd
        .recs
        .iter()
        .filter(|x| (x.range - range).abs() < 1e-6)
        .collect();
    if recs.is_empty() {
        return pending(pdf, r, "the curves");
    }
    let (y0, y1) = recs
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, x| {
            (a.0.min(x.cold), a.1.max(x.cold))
        });
    let x = (data::CURVE_WB[0], data::CURVE_WB[data::CURVE_WB.len() - 1]);
    let y = ((y0 - 0.5).floor(), (y1 + 0.5).ceil());
    let plot = axes(
        pdf,
        r,
        x,
        y,
        2.0,
        super::rate::nice((y1 - y0 + 1.0) / 4.0),
        &format!("wet bulb °C · {range:.0} K range"),
        "cold °C",
    );
    let cols = [INK2, TEAL, INK];
    for (fi, fpct) in data::CURVE_FLOW_PCT.iter().enumerate() {
        let flow = cd.design_flow_kg_s * fpct / 100.0;
        let mut pts: Vec<(f64, f64)> = recs
            .iter()
            .filter(|rec| (rec.flow_kg_s - flow).abs() < 1e-6)
            .map(|rec| map(plot, x, y, (rec.wb, rec.cold)))
            .collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        pdf.polyline(&pts, if fi == 1 { 1.3 } else { 0.8 }, cols[fi]);
        if let Some(q) = pts.last() {
            pdf.text(
                q.0 + 2.0,
                q.1 - 2.0,
                6.0,
                Font::Mono,
                cols[fi],
                &format!("{fpct:.0}%"),
            );
        }
    }
    let dp = map(plot, x, y, (d.duty.wet_bulb_c, d.duty.target_cold_water_c));
    pdf.circle(dp.0, dp.1, 2.6, 1.1, INK);
}

fn chart_scatter(pdf: &mut Pdf, r: B, d: &EngineInput, cache: &data::Cache) {
    let r = cell_title(pdf, r, "Feasible candidates: fan power vs cold water");
    let Some(sz) = cache.size.as_ref().filter(|x| !x.towers.is_empty()) else {
        return pending(pdf, r, "the selection");
    };
    let list = sz.ranked(None);
    if list.is_empty() {
        return pending(pdf, r, "the selection");
    }
    let (x0, x1, y1) = list.iter().fold(
        (f64::INFINITY, d.duty.target_cold_water_c, 0.0f64),
        |a, c| (a.0.min(c.cold_c), a.1.max(c.cold_c), a.2.max(c.power_kw)),
    );
    let x = (x0 - 0.1, x1 + 0.1);
    let y = (0.0, y1 * 1.1);
    let plot = axes(
        pdf,
        r,
        x,
        y,
        super::rate::nice((x1 - x0 + 0.2) / 4.0),
        super::rate::nice(y1 / 4.0),
        "cold water °C",
        "kW",
    );
    let target = map(plot, x, y, (d.duty.target_cold_water_c, y.1));
    dashed(pdf, target, (target.0, plot.bottom()), 0.6, INK2);
    for (i, c) in list.iter().enumerate().rev() {
        let q = map(plot, x, y, (c.cold_c, c.power_kw));
        let col = if i == 0 { TEAL } else { INK2 };
        if c.crossflow {
            pdf.square(q.0, q.1, 2.0, col);
        } else {
            pdf.disc(q.0, q.1, 1.5, col);
        }
    }
    // the count and the marker legend (drawn as shapes - the ●/■ glyphs are not in WinAnsi) share
    // the plot's top line, clear of the axis title underneath the plot
    let ly = plot.top() + 4.0;
    let label = format!("{} feasible", list.len());
    pdf.text(plot.left() + 3.0, ly, 6.0, Font::Mono, INK2, &label);
    let mut lx = plot.left() + 3.0 + 3.6 * label.chars().count() as f64 + 12.0;
    pdf.disc(lx, ly - 2.0, 1.5, INK2);
    pdf.text(lx + 3.5, ly, 5.5, Font::Sans, INK2, "counterflow");
    lx += 3.5 + 30.0 + 10.0;
    pdf.square(lx, ly - 2.0, 2.0, TEAL);
    pdf.text(lx + 4.0, ly, 5.5, Font::Sans, INK2, "crossflow");
}

fn chart_water(pdf: &mut Pdf, r: B, d: &EngineInput, o: &EngineOutput) {
    let r = cell_title(pdf, r, "Water balance at the duty's cycles");
    match data::water(d, o, d.duty.cycles_of_concentration) {
        Err(e) => {
            let (cx, cy) = r.center();
            pdf.text_center(cx, cy, 7.0, Font::Sans, INK2, &e);
        }
        Ok(w) => {
            let to = |kg: f64| kg / w.density * 3600.0;
            let parts = [
                ("evaporation", to(w.evaporation_kg_s), TEAL),
                ("drift", to(w.drift_kg_s), INK),
                ("blowdown", to(w.blowdown_kg_s), INK2),
            ];
            let total = to(w.makeup_kg_s).max(1e-9);
            let bar = B {
                x: r.left(),
                y: r.top() + 14.0,
                w: r.w,
                h: 14.0,
            };
            let mut x = bar.left();
            for (_, v, col) in parts.iter() {
                let seg = (bar.w * (v / total)).max(0.6);
                pdf.rect(x, bar.top(), seg, bar.h, Some(*col), None);
                x += seg;
            }
            let mut y = bar.bottom() + 14.0;
            for (name, v, col) in parts.iter() {
                pdf.rect(r.left(), y - 5.0, 6.0, 6.0, Some(*col), None);
                pdf.text(r.left() + 10.0, y, 7.5, Font::Sans, INK2, name);
                pdf.text_right(r.right(), y, 7.5, Font::Mono, INK, &format!("{v:.2} m³/h"));
                y += 14.0;
            }
            pdf.line(r.left(), y - 4.0, r.right(), y - 4.0, 0.5, RULE);
            y += 4.0;
            pdf.text(r.left() + 10.0, y, 8.0, Font::Bold, INK, "make-up");
            pdf.text_right(
                r.right(),
                y,
                8.0,
                Font::Mono,
                INK,
                &format!("{total:.2} m³/h"),
            );
            pdf.text(
                r.left(),
                y + 14.0,
                7.0,
                Font::Sans,
                INK2,
                &format!("at {} cycles of concentration", f1(w.cycles)),
            );
        }
    }
}

fn chart_section(pdf: &mut Pdf, r: B, d: &EngineInput) {
    let r = cell_title(pdf, r, "Tower section (schematic)");
    let (cx, _) = r.center();
    let tower = B {
        x: cx - r.w * 0.22,
        y: r.top() + 16.0,
        w: r.w * 0.44,
        h: r.h - 40.0,
    };
    pdf.rect(tower.x, tower.y, tower.w, tower.h, None, Some((0.8, INK)));
    // the fill stack, top first (the draft order)
    let n = d.fill_layers.len().max(1);
    let band_h = (tower.h * 0.5) / n as f64;
    for (i, l) in d.fill_layers.iter().enumerate() {
        let by = tower.top() + tower.h * 0.2 + band_h * i as f64;
        pdf.rect(
            tower.left() + 3.0,
            by,
            tower.w - 6.0,
            band_h * 0.8,
            Some([0xd9, 0xd2, 0xc2]),
            None,
        );
        pdf.text_fit(
            tower.left() + 5.0,
            by + band_h * 0.55,
            6.0,
            Font::Mono,
            INK2,
            &format!("{} {}", l.fill_id, f2(l.depth_m)),
            tower.w - 10.0,
        );
    }
    // fan + stack
    pdf.circle(cx, tower.top() + 8.0, 7.0, 0.9, TEAL);
    pdf.text_center(cx, tower.top() + 11.0, 6.0, Font::Mono, TEAL, "fan");
    // air in / water in / out
    pdf.line(
        tower.left() - 24.0,
        tower.bottom() - 6.0,
        tower.left() - 2.0,
        tower.bottom() - 6.0,
        0.9,
        TEAL,
    );
    pdf.text(
        tower.left() - 24.0,
        tower.bottom() - 12.0,
        6.0,
        Font::Sans,
        TEAL,
        "air in",
    );
    pdf.line(cx, tower.top() - 16.0, cx, tower.top() - 2.0, 0.9, INK2);
    pdf.text(
        cx + 4.0,
        tower.top() - 14.0,
        6.0,
        Font::Sans,
        INK2,
        "air out",
    );
    pdf.line(
        tower.right() + 2.0,
        tower.top() + 14.0,
        tower.right() + 24.0,
        tower.top() + 14.0,
        0.9,
        INK,
    );
    pdf.text(
        tower.right() + 2.0,
        tower.top() + 8.0,
        6.0,
        Font::Sans,
        INK,
        "water in",
    );
    pdf.line(
        tower.right() + 2.0,
        tower.bottom() - 2.0,
        tower.right() + 24.0,
        tower.bottom() - 2.0,
        0.9,
        INK,
    );
    pdf.text(
        tower.right() + 2.0,
        tower.bottom() - 9.0,
        6.0,
        Font::Sans,
        INK,
        "water out",
    );
    pdf.text(
        r.left(),
        r.bottom() - 2.0,
        6.0,
        Font::Sans,
        INK2,
        "not to scale; the engine model is 1-D along the air path",
    );
}

#[allow(clippy::too_many_arguments)]
fn charts_page(
    pdf: &mut Pdf,
    d: &EngineInput,
    o: &EngineOutput,
    cache: &data::Cache,
    meta: &Meta,
    no: usize,
    total: usize,
) {
    pdf.page();
    page_open(pdf, meta, no, total);
    section_head(pdf, "4  Charts");
    let g = B {
        x: M,
        y: TOP + 24.0,
        w: W,
        h: BOTTOM - (TOP + 24.0) - 14.0,
    };
    let gap = 18.0;
    let cw = (g.w - gap) / 2.0;
    let ch = (g.h - 2.0 * gap) / 3.0;
    let cell = |k: usize| B {
        x: g.x + (cw + gap) * (k % 2) as f64,
        y: g.y + (ch + gap) * (k / 2) as f64,
        w: cw,
        h: ch,
    };
    chart_fan_system(pdf, cell(0), o);
    chart_air_path(pdf, cell(1), o);
    chart_cold_vs_wb(pdf, cell(2), d, cache);
    chart_scatter(pdf, cell(3), d, cache);
    chart_water(pdf, cell(4), d, o);
    chart_section(pdf, cell(5), d);
    page_footer(pdf, o, meta);
}

// ------------------------------------------------------------------ validation

fn validation_page(
    pdf: &mut Pdf,
    d: &EngineInput,
    o: &EngineOutput,
    meta: &Meta,
    no: usize,
    total: usize,
) {
    pdf.page();
    page_open(pdf, meta, no, total);
    section_head(pdf, "5  Validation & limitations");
    let stmt = validation_statement(o);
    let fails = o.validation.iter().filter(|l| outside(l)).count();
    let mut y = TOP + 26.0;
    let lines = pdf::wrap(Font::Bold, 9.5, &stmt, W - 20.0);
    let banner_h = 14.0 + lines.len() as f64 * 13.0;
    pdf.rect(
        M,
        y - 12.0,
        W,
        banner_h,
        Some(if fails == 0 { OK_SOFT } else { DANGER_SOFT }),
        None,
    );
    for (i, line) in lines.iter().enumerate() {
        pdf.text(M + 10.0, y + i as f64 * 13.0, 9.5, Font::Bold, INK, line);
    }
    y += banner_h + 6.0;

    if o.validation.is_empty() {
        pdf.text(
            M,
            y,
            9.0,
            Font::Sans,
            INK2,
            "The engine reported no limit outside its recorded range for this run.",
        );
        y += 20.0;
    } else {
        for l in &o.validation {
            let range = match (l.min, l.max) {
                (Some(a), Some(b)) => format!(">= {} and <= {}", kit::fmt2(a), kit::fmt2(b)),
                (Some(a), None) => format!(">= {}", kit::fmt2(a)),
                (None, Some(b)) => format!("<= {}", kit::fmt2(b)),
                _ => "-".into(),
            };
            let bad = outside(l);
            pdf.text_fit(M, y, 9.0, Font::Sans, INK2, &l.field, W * 0.42);
            pdf.text(
                M + W * 0.46,
                y,
                9.0,
                Font::Mono,
                if bad { DANGER } else { INK },
                &format!("{} {}", kit::fmt2(l.value), l.unit),
            );
            pdf.text_right(M + W, y, 8.5, Font::Mono, INK2, &range);
            pdf.line(M, y + 6.0, M + W, y + 6.0, 0.5, RULE);
            y += 19.0;
        }
    }

    // limitations + export notes
    y += 10.0;
    sub_head(pdf, y, "Limitations");
    y += 16.0;
    for line in pdf::wrap(Font::Sans, 8.0, LIMITATIONS, W) {
        pdf.text(M, y, 8.0, Font::Sans, INK2, &line);
        y += 10.0;
    }
    y += 8.0;
    for note in export_notes(meta) {
        for line in pdf::wrap(Font::Mono, 7.0, &note, W) {
            pdf.text(M, y, 7.0, Font::Mono, MUTED, &line);
            y += 8.5;
        }
        y += 4.0;
    }

    // provenance, the screen's own foot line
    y += 6.0;
    pdf.line(M, y, M + W, y, 1.0, INK);
    y += 12.0;
    pdf.text_fit(
        M,
        y,
        8.5,
        Font::Mono,
        INK,
        &format!(
            "Computed by {} on catalog {} rev {}.",
            o.provenance.engine, o.provenance.catalog_id, o.provenance.catalog_revision
        ),
        W,
    );
    y += 12.0;
    if !o.provenance.warning.is_empty() {
        for line in pdf::wrap(Font::Sans, 8.0, &o.provenance.warning, W) {
            pdf.text(M, y, 8.0, Font::Sans, INK2, &line);
            y += 10.0;
        }
    }
    pdf.text(
        M,
        y,
        7.5,
        Font::Mono,
        MUTED,
        &format!(
            "draft: tower {} · fan {} · {:.2}x speed",
            d.tower.id, d.fan.id, d.speed_ratio
        ),
    );
    page_footer(pdf, o, meta);
}

// ======================================================================================= delivery

/// Deliver the bytes on the host this build runs on: a **download** on the web (a `data:` URL on a
/// `download` anchor - the same bytes), a **save** on native (`~/Downloads/<name>` when there is a
/// Downloads directory, the working directory otherwise). Returns what happened, for the toast.
#[cfg(target_arch = "wasm32")]
pub fn deliver(bytes: &[u8], name: &str) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    let win = web_sys::window().ok_or("no window")?;
    let doc = win.document().ok_or("no document")?;
    let anchor = doc
        .create_element("a")
        .map_err(|_| "could not create the download anchor".to_string())?;
    let _ = anchor.set_attribute(
        "href",
        &format!("data:application/pdf;base64,{}", base64(bytes)),
    );
    let _ = anchor.set_attribute("download", name);
    let html: web_sys::HtmlElement = anchor
        .dyn_into()
        .map_err(|_| "the download anchor is not an element".to_string())?;
    html.click();
    Ok(format!("download started: {name}"))
}

/// Native: write the file and say where it went.
#[cfg(not(target_arch = "wasm32"))]
pub fn deliver(bytes: &[u8], name: &str) -> Result<String, String> {
    let dir = std::env::var("HOME")
        .ok()
        .map(|home| std::path::Path::new(&home).join("Downloads"))
        .filter(|downloads| downloads.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .ok_or("no directory to save into".to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(format!("saved: {}", path.display()))
}

/// Standard base64 with padding (the wasm download's `data:` URL).
pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ======================================================================================= tests

#[cfg(test)]
mod tests {
    use super::*;
    use cockpit::fixture_engine::FixtureEngine;

    const FIXTURE: &str = include_str!("../../assets/fixture.json");

    /// The fixture project: the recorded default input, run on the build's own engine (the app's
    /// path - `engine_select::build`, exactly what the cockpit boots with), and the Charts caches
    /// stepped to completion the way the Report screen steps them. Built once per test process
    /// (`OnceLock`): the engine run and the cache stepping are the expensive part and every test
    /// here only reads the result.
    fn fixture_case() -> &'static (EngineInput, EngineOutput, data::Cache) {
        static CASE: std::sync::OnceLock<(EngineInput, EngineOutput, data::Cache)> =
            std::sync::OnceLock::new();
        CASE.get_or_init(|| {
            let draft = FixtureEngine::from_json(FIXTURE)
                .expect("the fixture parses")
                .default_input();
            let engine = crate::engine_select::build(FIXTURE, None).expect("the build's engine");
            let out = engine.run(&draft).expect("the fixture duty runs");
            let mut cache = data::Cache::default();
            let mut steps = 0;
            while cache.step(
                data::Want::Report,
                FIXTURE,
                &draft,
                Some(&out),
                Some(engine.as_ref()),
            ) {
                steps += 1;
                assert!(steps < 10_000, "the Charts caches never finished");
            }
            (draft, out, cache)
        })
    }

    fn staff_meta(draft: &EngineInput) -> Meta {
        let mut meta = meta_for_export(true, draft);
        meta.date = "2026-10-03 (UTC)".to_string();
        meta
    }

    /// Issue #85 completion: a file session holding the fixture project, saved the way the app's
    /// save path saves it - `files::snapshot_text` into a real file - so "save a project, export the
    /// report" runs against the session's own writer and a real file on disk, nothing re-encoded
    /// for the test.
    struct SavedProject {
        session: crate::files::FileSession,
        catalog: crate::state::Catalog,
        options: crate::state::StartOptions,
        engine: Box<dyn cockpit::engine::Engine>,
        /// The bytes the save wrote - the file's own text.
        text: String,
        path: std::path::PathBuf,
    }

    impl SavedProject {
        /// What the session's writer produces for `input` right now: the same call the File menu's
        /// save and the Report export's document source both make.
        fn written(&self, input: &EngineInput, output: Option<&EngineOutput>) -> String {
            let ctx = crate::files::Ctx {
                options: &self.options,
                host_label: crate::files::plane_label(),
                spec: None,
                output,
                fixture_text: FIXTURE,
            };
            crate::files::snapshot_text(
                &ctx,
                &self.session,
                input,
                &self.catalog,
                Some(self.engine.as_ref()),
            )
        }

        /// The [DocumentBytes] the export's document source would hand the report for `input`: the
        /// session's own writer, and the session's own unsaved-changes verdict.
        fn document(&self, input: &EngineInput, output: Option<&EngineOutput>) -> DocumentBytes {
            DocumentBytes {
                text: self.written(input, output),
                unchanged: !self.session.is_dirty(input, &self.catalog),
            }
        }
    }

    /// Save the fixture project: write the document, then put the session in the state a save
    /// leaves it in (`files::save`: path, loaded, saved signature).
    fn save_project(draft: &EngineInput, out: &EngineOutput) -> SavedProject {
        let fixture = FixtureEngine::from_json(FIXTURE).expect("the fixture parses");
        let catalog = crate::state::Catalog::from_fixture(&fixture);
        let options = crate::state::StartOptions::default();
        let engine = crate::engine_select::build(FIXTURE, None).expect("the build's engine");
        let mut session = crate::files::FileSession {
            revision_id: crate::revision::SHIPPED_ID.to_string(),
            ..crate::files::FileSession::default()
        };
        let text = {
            let ctx = crate::files::Ctx {
                options: &options,
                host_label: crate::files::plane_label(),
                spec: None,
                output: Some(out),
                fixture_text: FIXTURE,
            };
            crate::files::snapshot_text(&ctx, &session, draft, &catalog, Some(engine.as_ref()))
        };
        let path = std::env::temp_dir().join(format!(
            "drafthouse-85-{}-{}.drafthouse",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a wall clock")
                .as_nanos()
        ));
        std::fs::write(&path, &text).expect("the project file is written");
        session.path = Some(path.clone());
        session.loaded = Some(Box::new(
            crate::project::Project::read(&text).expect("the file reads back"),
        ));
        session.saved_signature = crate::files::signature(draft, &catalog, &session.revision_id);
        SavedProject {
            session,
            catalog,
            options,
            engine,
            text,
            path,
        }
    }

    /// The wrapped lines of a text the sheet paints, for checking the document really carries them.
    fn painted_lines(font: Font, size: f64, s: &str, width: f64) -> Vec<String> {
        pdf::wrap(font, size, s, width)
    }

    /// The staff export of the fixture project, as the bytes the button delivers.
    fn fixture_pdf() -> Vec<u8> {
        let (draft, out, cache) = fixture_case();
        document(draft, out, cache, &staff_meta(draft))
    }

    /// The label/value pair has to appear as two consecutive extracted lines (the writer emits the
    /// label, then the value).
    fn pair(lines: &[String], label: &str, value: &str) -> bool {
        lines.windows(2).any(|w| w[0] == label && w[1] == value)
    }

    fn parsed_value(line: &str) -> f64 {
        line.split(' ')
            .next()
            .expect("a number")
            .parse()
            .expect("the first token is the number")
    }

    /// AC 1's load-bearing test: every number the sheet shows for the fixture project is the
    /// engine's - extracted back out of the produced PDF bytes by the minimal extractor (no
    /// `pdftotext`, no dev-dependency, no constants typed twice: the expected strings are the engine
    /// fields through the same format calls the sheet uses).
    #[test]
    fn fixture_pdf_text_equals_the_engine_values() {
        let pdf_bytes = fixture_pdf();
        let (draft, out, _cache) = fixture_case();
        let lines = pdf::extract_text(&pdf_bytes);
        assert!(
            lines.len() > 100,
            "the extraction found {} lines",
            lines.len()
        );
        println!(
            "\n--- extracted text, {} lines ---\n{}\n---",
            lines.len(),
            lines.join("\n")
        );

        // the headline results: label / formatted value + unit, and the number parsed back equals
        // the engine's field to its printed precision
        for (label, value, unit, engine_value, decimals) in [
            (
                "cold water",
                f2(out.cold_water_c),
                "°C",
                out.cold_water_c,
                2,
            ),
            ("range", f2(out.range_c), "K", out.range_c, 2),
            ("approach", f2(out.approach_c), "K", out.approach_c, 2),
            (
                "capability",
                f1(out.capability_pct),
                "%",
                out.capability_pct,
                1,
            ),
            ("KaV/L", f3(out.kavl_total), "", out.kavl_total, 3),
            ("airflow", f2(out.airflow_m3_s), "m³/s", out.airflow_m3_s, 2),
            (
                "total pressure",
                f1(out.total_pressure_pa),
                "Pa",
                out.total_pressure_pa,
                1,
            ),
            ("fan power", f2(out.fan_power_kw), "kW", out.fan_power_kw, 2),
            (
                "evaporation",
                f2(out.evaporation_pct),
                "%",
                out.evaporation_pct,
                2,
            ),
            ("make-up", f2(out.makeup_m3_hr), "m³/h", out.makeup_m3_hr, 2),
        ] {
            let shown = if unit.is_empty() {
                value.clone()
            } else {
                format!("{value} {unit}")
            };
            assert!(
                pair(&lines, label, &shown),
                "the sheet does not show {label:?} / {shown:?} (engine value {engine_value})"
            );
            let back = parsed_value(&shown);
            // the extracted number round-trips to the printed string at the field's own precision,
            // and sits within half of the last printed unit of the engine's value
            assert_eq!(
                format!("{back:.prec$}", prec = decimals),
                value,
                "{label}: extracted {back} does not print back as {value:?}"
            );
            let half_unit = 0.5 * 10f64.powi(-(decimals as i32)) + 1e-9;
            assert!(
                (back - engine_value).abs() <= half_unit,
                "{label}: extracted {back} vs engine {engine_value}"
            );
        }

        // every worked step with a value carries it, formatted the screen's way
        for step in &out.worked_steps {
            if let Some(v) = step.value {
                let shown = if step.unit.is_empty() {
                    kit::fmt2(v)
                } else {
                    format!("{} {}", kit::fmt2(v), step.unit)
                };
                assert!(
                    pair(&lines, &step.label, &shown),
                    "step {:?} does not show {shown:?}",
                    step.label
                );
            }
        }

        // the inputs are the draft's own numbers
        for (label, value, unit) in [
            ("water flow", f1(draft.duty.water_flow_m3_hr), "m³/h"),
            ("hot water", f2(draft.duty.hot_water_c), "°C"),
            (
                "target cold water",
                f2(draft.duty.target_cold_water_c),
                "°C",
            ),
            ("wet bulb", f2(draft.duty.wet_bulb_c), "°C"),
            ("dry bulb", f2(draft.duty.dry_bulb_c), "°C"),
            ("pressure", f0(draft.duty.pressure_pa), "Pa"),
            ("cycles", f1(draft.duty.cycles_of_concentration), "×"),
            ("salinity", f2(draft.duty.salinity_g_kg), "g/kg"),
            ("fill area", f1(draft.tower.fill_area_m2), "m²"),
        ] {
            assert!(
                pair(&lines, label, &format!("{value} {unit}")),
                "the inputs page does not show {label:?} / {value} {unit}"
            );
        }

        // the KaV/L by-layer block: the engine's per-layer values, the remainder and the total
        for l in &out.kavl_per_layer {
            assert!(
                pair(
                    &lines,
                    &format!("{} · {} m", l.fill_id, f2(l.depth_m)),
                    &format!("{}  ({}%)", f3(l.kavl), f1(l.cooling_share_pct))
                ),
                "layer {} is not in the results table",
                l.fill_id
            );
        }
        let fill: f64 = out.kavl_per_layer.iter().map(|l| l.kavl).sum();
        assert!(pair(
            &lines,
            "spray + rain zones",
            &format!(
                "{}  ({}%)",
                f3(out.kavl_total - fill),
                f1(100.0
                    - out
                        .kavl_per_layer
                        .iter()
                        .map(|l| l.cooling_share_pct)
                        .sum::<f64>())
            )
        ));
        assert!(pair(
            &lines,
            "total",
            &format!("{}  (100%)", f3(out.kavl_total))
        ));

        // and the air-path split carries every zone
        for z in &out.pressure_by_zone {
            assert!(
                pair(
                    &lines,
                    &z.label,
                    &format!("{} Pa ({:.1}%)", f1(z.pressure_pa), z.share_pct)
                ),
                "zone {} is not in the air-path split",
                z.label
            );
        }

        // the revision row: for this draft (no saved document) it is the state hash, labelled for
        // what it is - and it must not claim the document digest it does not have
        let meta = staff_meta(draft);
        let text = lines.join("\n");
        assert_eq!(
            meta.revision,
            Revision::DraftState {
                state_hash: meta.state_hash.clone()
            },
            "the draft fallback is the state hash"
        );
        assert!(
            text.contains(&meta.state_hash),
            "the cover carries the state hash"
        );
        assert!(
            text.contains("project state hash"),
            "the hash is labelled a project state hash"
        );
        // the note really paints, wrapped the way the sheet wraps it - so the fallback's own
        // sentence ("not a project document's digest") is in the finished document
        for line in painted_lines(Font::Mono, 7.0, &meta.revision.note(), W) {
            assert!(
                text.contains(&line),
                "the validation page's revision note line {line:?} is not in the document"
            );
        }
        assert!(
            meta.revision
                .note()
                .contains("not a project document's digest"),
            "the fallback says it is not a document digest: {}",
            meta.revision.note()
        );
        assert!(
            !text.contains("the saved file's own digest"),
            "an unsaved draft must not claim the saved document's digest"
        );
        assert!(
            !text.contains("issue #74") && !text.contains("has not landed"),
            "no placeholder language remains anywhere in the document"
        );
    }

    /// AC 3: the validation statement is in every document, whatever the settings - account (demo
    /// watermark or staff), empty or full Charts caches, a clean or refused run. Removing the
    /// statement's emission from `document` reds this test (the mutation is recorded in the
    /// evidence).
    #[test]
    fn every_document_carries_the_validation_statement() {
        // clean run, staff, full caches
        let (draft, out, cache) = fixture_case();
        let doc = document(draft, out, cache, &staff_meta(draft));
        let text = pdf::extract_text(&doc).join("\n");
        assert!(
            text.contains(&validation_statement(out)),
            "the clean-run statement is missing"
        );

        // demo account (watermark on) and empty caches: the statement is still there
        let mut demo = meta_for_export(false, draft);
        demo.date = "2026-10-03 (UTC)".to_string();
        assert!(demo.demo);
        let doc = document(draft, out, &data::Cache::default(), &demo);
        let text = pdf::extract_text(&doc).join("\n");
        assert!(
            text.contains(&validation_statement(out)),
            "the demo/no-cache statement is missing"
        );

        // a refused run: the statement names the refusal and is still present. The refusal is the
        // engine's own refusal path - a speed ratio outside the fan record's validity band, which
        // the engine reports as a limit carrying the band rather than clamping the ratio into it.
        let mut refused_draft = draft.clone();
        refused_draft.speed_ratio = refused_draft.fan.allowed_speed_ratio[1] * 2.0 + 5.0;
        let engine = crate::engine_select::build(FIXTURE, None).expect("the build's engine");
        let refused = engine
            .run(&refused_draft)
            .expect("a refusal is reported as limits, not as an error");
        for l in &refused.validation {
            eprintln!(
                "refusal: {} - {} (value {}, band {:?}..{:?})",
                l.field, l.message, l.value, l.min, l.max
            );
        }
        assert!(
            !refused.validation.is_empty(),
            "the off-band ratio has to trigger a refusal"
        );
        assert!(
            refused.validation.iter().filter(|l| outside(l)).count() > 0,
            "the refusal has to name a limit outside its band: {:?}..{:?}",
            refused.validation.first().map(|l| l.min),
            refused.validation.first().map(|l| l.max)
        );
        let doc = document(&refused_draft, &refused, cache, &staff_meta(&refused_draft));
        let text = pdf::extract_text(&doc).join("\n");
        assert!(
            text.contains(&validation_statement(&refused)),
            "the refused-run statement is missing"
        );
        assert!(
            validation_statement(&refused).contains("refused this run"),
            "the refused run must read as refused: {}",
            validation_statement(&refused)
        );
        assert_eq!(
            validation_statement(out),
            "The engine refused nothing: every record range this run passed through held."
        );
    }

    #[test]
    fn the_state_hash_is_deterministic_and_moves_with_the_draft() {
        // the standard vectors pin the digest itself
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let draft = FixtureEngine::from_json(FIXTURE)
            .expect("the fixture parses")
            .default_input();
        let a = state_hash(&draft);
        let b = state_hash(&draft.clone());
        assert_eq!(a, b, "the same draft hashes the same");
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        let mut edited = draft.clone();
        edited.duty.wet_bulb_c += 0.001;
        assert_ne!(a, state_hash(&edited), "an edit moves the hash");
    }

    /// Issue #85 completion, AC 2's load-bearing test: **a saved project exports a sheet whose
    /// revision row carries the saved file's own sha-256** - and the equality is not arranged, the
    /// test reads the file back off disk and hashes what it finds. The writer's determinism is
    /// asserted in place (the unchanged state, written again, IS the file's bytes), and a second
    /// export reproduces the revision.
    #[test]
    fn a_saved_project_exports_the_saved_files_digest_and_reproduces_it() {
        let (draft, out, cache) = fixture_case();
        let saved = save_project(draft, out);

        // The premise, computed and shown here rather than cited: the file on disk is the writer's
        // bytes, and writing the unchanged state again reproduces them exactly.
        let on_disk = std::fs::read(&saved.path).expect("the saved file reads back");
        assert_eq!(
            saved.text.as_bytes(),
            on_disk.as_slice(),
            "the file on disk is the writer's bytes"
        );
        assert_eq!(
            saved.written(draft, Some(out)),
            saved.text,
            "write -> save -> write is byte-equal"
        );
        // The file's digest, computed with issue #74's own sha-256 implementation (`crate::sha256`,
        // the one the project's revision objects are built on) - independent of the writer's own
        // (`sha256_hex`) that the report carries - and shown equal, so the equality below cannot be
        // one implementation agreeing with itself.
        let file_sha = crate::sha256::hex(&on_disk);
        assert_eq!(
            file_sha,
            sha256_hex(&on_disk),
            "the two sha-256 implementations agree on the file's bytes"
        );

        // The two ids, side by side, and shown equal.
        let mut meta = staff_meta(draft);
        meta.carry_document(&saved.document(draft, Some(out)));
        let first = document(draft, out, cache, &meta);
        let text = pdf::extract_text(&first).join("\n");
        println!("saved file sha256 = {file_sha}");
        println!("report  revision  = {}", meta.revision.value());
        assert_eq!(
            meta.revision.value(),
            file_sha,
            "the report's revision is the saved file's sha-256"
        );
        assert!(
            text.contains(&file_sha),
            "the finished document carries the saved file's digest"
        );

        // The caption really paints, wrapped the way the sheet wraps it, and the value is labelled
        // the project document's own digest - with the placeholder language gone.
        assert_eq!(
            meta.revision,
            Revision::Document {
                sha256: file_sha.clone(),
                unchanged: true
            }
        );
        for line in painted_lines(Font::Mono, 6.5, &meta.revision.caption(), W - 90.0) {
            assert!(
                text.contains(&line),
                "the cover's revision caption line {line:?} is not in the document"
            );
        }
        for line in painted_lines(Font::Mono, 7.0, &meta.revision.note(), W) {
            assert!(
                text.contains(&line),
                "the validation page's revision note line {line:?} is not in the document"
            );
        }
        assert!(
            meta.revision
                .caption()
                .contains("the saved file's own digest"),
            "the caption names what the value is: {}",
            meta.revision.caption()
        );
        assert!(
            text.contains(&format!("project document {}…", &file_sha[..12])),
            "every footer carries the project document id"
        );
        assert!(
            !text.contains("issue #74") && !text.contains("has not landed"),
            "the placeholder language is gone for a document built after a save"
        );

        // A second export, unchanged, reproduces the revision - and the bytes.
        let mut meta2 = staff_meta(draft);
        meta2.carry_document(&saved.document(draft, Some(out)));
        let second = document(draft, out, cache, &meta2);
        assert_eq!(
            first, second,
            "the second export is byte-for-byte identical"
        );
        assert_eq!(meta2.revision.value(), file_sha, "the revision reproduces");
        assert!(
            pdf::extract_text(&second).join("\n").contains(&file_sha),
            "the second export carries the same revision"
        );

        std::fs::remove_file(&saved.path).ok();
    }

    /// Issue #85 completion: a project **saved and then modified** and exported moves the revision -
    /// the sheet describes the state it was built from (the working document), and its caption says
    /// the value differs from the file's, rather than presenting the stale file digest.
    #[test]
    fn a_project_modified_after_its_save_moves_the_revision() {
        let (draft, out, cache) = fixture_case();
        let saved = save_project(draft, out);
        let file_sha =
            crate::sha256::hex(&std::fs::read(&saved.path).expect("the saved file reads back"));

        // The save is the clean baseline: no unsaved changes, and the row is the file's digest.
        assert!(
            !saved.session.is_dirty(draft, &saved.catalog),
            "just saved: no unsaved changes"
        );
        let mut before = staff_meta(draft);
        before.carry_document(&saved.document(draft, Some(out)));
        assert_eq!(before.revision.value(), file_sha);

        // Modify the state the way a user does, and run the engine on it (the app's frame loop runs
        // the current draft every frame, so the export's numbers are the modified run's).
        let mut modified = draft.clone();
        modified.speed_ratio += 0.05;
        let moved_out = saved.engine.run(&modified).expect("the modified duty runs");
        assert!(
            saved.session.is_dirty(&modified, &saved.catalog),
            "the edit is unsaved changes against the saved file"
        );

        let mut moved = staff_meta(&modified);
        moved.carry_document(&saved.document(&modified, Some(&moved_out)));
        let moved_pdf = document(&modified, &moved_out, cache, &moved);
        let text = pdf::extract_text(&moved_pdf).join("\n");
        println!("saved file      sha256 = {file_sha}");
        println!("working document sha256 = {}", moved.revision.value());
        assert_ne!(
            moved.revision.value(),
            file_sha,
            "the revision must change when the state the sheet describes has changed"
        );
        assert!(
            text.contains(moved.revision.value()),
            "the working document's digest is carried"
        );
        assert!(
            text.contains(&format!(
                "working document {}…",
                &moved.revision.value()[..12]
            )),
            "the footer names the working document"
        );
        assert!(
            moved.revision.caption().contains("unsaved changes"),
            "the caption says why it differs from the file: {}",
            moved.revision.caption()
        );
        assert!(
            !text.contains(&file_sha),
            "the stale saved-file digest must not be presented as this sheet's revision"
        );
        for line in painted_lines(Font::Mono, 6.5, &moved.revision.caption(), W - 90.0) {
            assert!(
                text.contains(&line),
                "the cover's revision caption line {line:?} is not in the document"
            );
        }

        std::fs::remove_file(&saved.path).ok();
    }

    #[test]
    fn the_same_inputs_build_the_same_bytes() {
        let (draft, out, cache) = fixture_case();
        let one = document(draft, out, cache, &staff_meta(draft));
        let two = document(draft, out, cache, &staff_meta(draft));
        assert_eq!(one, two, "the export is deterministic");
    }

    #[test]
    fn the_steps_plan_covers_every_step_and_sets_the_page_count() {
        let bytes = fixture_pdf();
        let (_, out, _) = fixture_case();
        let plan = plan_steps(&out.worked_steps);
        let covered: Vec<usize> = plan.iter().flatten().copied().collect();
        assert_eq!(covered.len(), out.worked_steps.len());
        assert_eq!(covered, (0..out.worked_steps.len()).collect::<Vec<_>>());
        assert_eq!(page_count(&out.worked_steps), 5 + plan.len());
        // the PDF really has that many pages
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(&format!("/Count {}", page_count(&out.worked_steps))));
        let slots = bytes.windows(11).filter(|w| w == b"/Type /Page").count();
        assert_eq!(
            slots - 1,
            page_count(&out.worked_steps),
            "one page object per page plus the pages tree itself"
        );
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn dates_are_utc_and_named() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert!(wall_date_utc().ends_with(" (UTC)"));
    }

    #[test]
    fn delivery_writes_the_bytes_natively() {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let bytes = fixture_pdf();
            let name = format!("drafthouse-test-{}.pdf", std::process::id());
            let report = deliver(&bytes, &name).expect("the native save works");
            assert!(report.starts_with("saved: "), "{report}");
            let path = report.trim_start_matches("saved: ");
            assert_eq!(
                std::fs::read(path).expect("the file is there"),
                bytes,
                "the saved bytes are the document"
            );
            std::fs::remove_file(path).ok();
        }
    }
}
