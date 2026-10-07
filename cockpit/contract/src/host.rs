//! Host configuration: what the embedding page injects. Nothing here is hard-coded branding.

use serde::{Deserialize, Serialize};

/// Branding the host page supplies (decision 9 on #52 / decision 13 on #55). `None` = brand-free.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Branding {
    /// Provider name shown in the header, e.g. "Synergy Services".
    pub name: String,
    pub site_url: String,
    /// Footer line satisfying the notice obligation, e.g. "(c) 2026 ... - Engine and source under PolyForm Noncommercial 1.0.0".
    pub notice: String,
    /// Short mark drawn in the header (2-3 letters). Inline SVG logos are a host-page concern (the HTML shell).
    pub mark: String,
    /// Accent colour as "#rrggbb"; falls back to the cockpit primary when absent.
    pub accent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostConfig {
    /// Public host: authoring allowed, no save/export, the comparable-metrics label always visible.
    pub public: bool,
    pub branding: Option<Branding>,
    /// Catalog label the host wants shown next to the synthetic-data chip (e.g. "illustrative-catalog-v0.1").
    pub catalog_label: Option<String>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self { public: true, branding: None, catalog_label: None }
    }
}

impl HostConfig {
    pub fn host_name(&self) -> &'static str {
        if self.public { "PUBLIC" } else { "INTERNAL" }
    }
}

/// The public host's mandatory label: decision 12's obligation (comparable, not certified), in the
/// wording issue #137 set (conductor decision, 2026-10-06) - "Calculated by our engine" was developer
/// text. The badge's (i) opens the notes drawer, which carries the longer explanation
/// (`crate::host::NOTICE_EXPLAINED`).
pub const PUBLIC_LABEL: &str = "Comparable engineering metrics · not a certified CTI/MRL test";
/// The internal host's version of the same obligation.
pub const INTERNAL_LABEL: &str = "Internal host · comparable engineering metrics · not a certified CTI/MRL test";
/// The longer statement behind the badge's (i), shown first in the notes drawer.
pub const NOTICE_EXPLAINED: &str = "These are comparable engineering metrics: every result is calculated with the Merkel method from the recorded part data, so two designs can be compared on the same basis. They are not a certified CTI/MRL test - a certified figure needs a witnessed acceptance test of the built tower.";
pub const SYNTHETIC_LABEL: &str = "Synthetic data - not vendor data";

/// The line the host is obliged to keep visible with any numbers. Which line, and how loud, depends on
/// the host - the cockpit never invents either.
#[derive(Clone, Debug, PartialEq)]
pub struct PublicNotice {
    pub text: String,
    /// True on the public host (the label is the louder one there).
    pub public: bool,
}

impl HostConfig {
    pub fn notice(&self) -> PublicNotice {
        if self.public {
            PublicNotice { text: PUBLIC_LABEL.into(), public: true }
        } else {
            PublicNotice { text: INTERNAL_LABEL.into(), public: false }
        }
    }
}

/// Everything the internal host would send to its server. Stubbed: the UI enqueues these and shows them;
/// no network code exists in this crate (decision 5 - auth, storage, catalog revisions, report generation
/// stay on the server).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ServerCommand {
    SaveRevision { project: String, note: String },
    ExportReportPdf { project: String },
    ExportJson { project: String },
    ExportCsv { project: String },
    CompareLater { project: String, against_revision: Option<String> },
    LoadCatalogRevision { revision: String },
}

impl ServerCommand {
    pub fn label(&self) -> String {
        match self {
            ServerCommand::SaveRevision { project, .. } => format!("save revision of {project}"),
            ServerCommand::ExportReportPdf { .. } => "export PDF engineering report".into(),
            ServerCommand::ExportJson { .. } => "export JSON".into(),
            ServerCommand::ExportCsv { .. } => "export CSV".into(),
            ServerCommand::CompareLater { against_revision: Some(r), .. } => format!("compare against revision {r}"),
            ServerCommand::CompareLater { .. } => "pin this run for a later comparison".into(),
            ServerCommand::LoadCatalogRevision { revision } => format!("load catalog revision {revision}"),
        }
    }
}
