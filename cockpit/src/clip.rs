//! Round 5: **the layout measurement surface** - the clip probe and the counters the layout frames read.
//!
//! The owner's round-4 report was precise: *the right column clips*, and a two-value row loses its second
//! value and unit off the right edge. A picture can show that, but a picture cannot *prove* it is gone, so
//! round 5 makes the instrument measure itself:
//!
//! - [`ClipProbe`] collects one entry per text unit the right column (or the phone's stack) draws, with the
//!   rect it was drawn in, the width it had to lay out in, and the natural width of its text. The set is
//!   published every frame to `#mirror-clip` in the HTML mirror (JSON), so `tools/clip-check.mjs` can assert,
//!   per evidence frame, that **no text element overflows its container** and that a row that needed more
//!   than one line actually wrapped (`lines >= 2`) instead of clipping.
//! - [`LayoutInfo`] carries the round-5 counts a frame reads as `data-*`: how many bay label blocks exist,
//!   whether two of them overlap, whether a block left its bay, the selected-part hint's state, how many
//!   read-out rows show a value with its unit, and whether the status strip had to truncate.
//!
//! Both are measurement only: nothing here changes what the instrument draws.

use bevy::prelude::*;
use bevy_egui::egui;

/// The font a text unit was drawn with, so the probe can measure the same shaping the frame will show.
pub fn measure(painter: &egui::Painter, text: &str, font: &egui::FontId) -> egui::Vec2 {
    painter
        .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE)
        .size()
}

/// Half a point of tolerance: egui rounds rects to the physical pixel grid, and a row that ends exactly on
/// the container's edge is inside it.
pub const EPS: f32 = 0.5;

/// One text unit: what it says, where it was drawn, how wide it could have been and how wide it wanted to be.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub id: String,
    pub text: String,
    /// The drawn rect of the text unit, in canvas points.
    pub rect: [f32; 4],
    /// The width the unit had to lay its text out in (the row's available width).
    pub avail: f32,
    /// The natural (unwrapped) width of the text, measured with the same font.
    pub need: f32,
    /// How many lines the unit drew in.
    pub lines: u32,
    /// Horizontal overflow past the container's right edge; 0 when inside. (`over_x`)
    pub over: f32,
    /// How far past the container's left edge the unit started; 0 when inside.
    pub over_left: f32,
}

impl Entry {
    pub fn inside(&self) -> bool {
        self.over <= EPS && self.over_left <= EPS
    }
    /// A one-line unit whose text did not fit its row would have been clipped by the painter.
    pub fn wrapped_when_needed(&self) -> bool {
        self.lines >= 2 || self.need <= self.avail + EPS
    }
}

/// The probe: the right column's inner rect plus one [`Entry`] per text unit it drew this frame.
#[derive(Resource, Default)]
pub struct ClipProbe {
    /// The container every entry is checked against: the right column's inner rect (desktop) or the phone
    /// stack's own column (a phone has no right column - the brief's item 5 checks the same panel there).
    pub column: [f32; 4],
    pub entries: Vec<Entry>,
    pub frame: u64,
}

impl ClipProbe {
    /// Start a frame: the container, and an empty entry list.
    pub fn begin(&mut self, column: egui::Rect) {
        self.frame += 1;
        self.column = [column.min.x, column.min.y, column.width(), column.height()];
        self.entries.clear();
    }

    pub fn column_rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(self.column[0], self.column[1]),
            egui::vec2(self.column[2], self.column[3]),
        )
    }

    /// Record one text unit. `lines` is what the unit actually drew in; `need` is measured with the same
    /// font the unit was drawn with. Pass `need = 0.0` for a unit whose text is *truncated by design* (a
    /// one-line summary with an ellipsis, a wrapped paragraph): the check then reduces to "inside the
    /// container", which is the whole claim such a unit makes.
    pub fn push(
        &mut self,
        id: impl Into<String>,
        text: impl Into<String>,
        rect: egui::Rect,
        avail: f32,
        need: f32,
        lines: u32,
    ) {
        let col = self.column_rect();
        let over = (rect.max.x - col.max.x).max(0.0);
        let over_left = (col.min.x - rect.min.x).max(0.0);
        self.entries.push(Entry {
            id: id.into(),
            text: text.into(),
            rect: [rect.min.x, rect.min.y, rect.width(), rect.height()],
            avail,
            need,
            lines,
            over,
            over_left,
        });
    }

    /// Convenience for a unit whose natural width is known from the same painter.
    pub fn push_measured(
        &mut self,
        id: impl Into<String>,
        text: &str,
        rect: egui::Rect,
        avail: f32,
        painter: &egui::Painter,
        font: egui::FontId,
    ) {
        let need = measure(painter, text, &font).x;
        let lines = if rect.height() > font.size * 1.9 {
            2
        } else {
            1
        };
        self.push(id, text.to_owned(), rect, avail, need, lines);
    }

    pub fn overflow_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.inside()).count()
    }

    pub fn clip_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| !e.wrapped_when_needed())
            .count()
    }

    /// Units the column's bottom edge **cuts**: the rect starts inside the column and ends past its
    /// bottom - the band the status strip's own panel owns, so a unit counted here is one the strip is
    /// drawn over (issue #58, the owner's layout nit 1: at 1440x900 the read-out's last row was the one
    /// the edge cut).
    ///
    /// A unit *entirely* below the column is **not** counted: that is content scrolled out of a viewport
    /// shorter than its content, which is what a scroll area is for. The frame reads the count
    /// (`data-clip-below`) instead of eyeballing a picture.
    pub fn below_count(&self) -> usize {
        let column = self.column_rect();
        self.entries
            .iter()
            .filter(|e| e.rect[1] < column.max.y && e.rect[1] + e.rect[3] > column.max.y + EPS)
            .count()
    }

    /// The JSON the HTML mirror carries - parsed by `tools/clip-check.mjs`.
    pub fn json(&self) -> String {
        let mut s = String::with_capacity(256 + self.entries.len() * 160);
        s.push_str(&format!(
            "{{\"frame\":{},\"column\":[{:.2},{:.2},{:.2},{:.2}],\"entries\":[",
            self.frame, self.column[0], self.column[1], self.column[2], self.column[3]
        ));
        for (i, e) in self.entries.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"id\":\"{}\",\"text\":\"{}\",\"rect\":[{:.2},{:.2},{:.2},{:.2}],\"avail\":{:.2},\"need\":{:.2},\"lines\":{},\"over\":{:.2},\"overLeft\":{:.2}}}",
                json_escape(&e.id),
                json_escape(&e.text),
                e.rect[0],
                e.rect[1],
                e.rect[2],
                e.rect[3],
                e.avail,
                e.need,
                e.lines,
                e.over,
                e.over_left
            ));
        }
        s.push_str(&format!(
            "],\"overflows\":{},\"clips\":{}}}",
            self.overflow_count(),
            self.clip_count()
        ));
        s
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The round-5 layout counters the frames read (published as `data-*` by [`crate::bridge`]).
#[derive(Resource, Default, Debug, Clone)]
pub struct LayoutInfo {
    /// The right column's own width this frame (0 on a phone or in the seams view).
    pub right_w: f32,
    /// How many bay label blocks were drawn (one per bay: four).
    pub bay_labels: u32,
    /// Bay label blocks that are not fully inside their own bay.
    pub bay_label_outside: u32,
    /// Bay label blocks whose text had to be cut to fit their bay.
    pub bay_label_truncated: u32,
    /// Bay label blocks that dropped their leading piece (the record's id) to fit their bay - the phone's
    /// nozzle bay, whose band is 18 px tall.
    pub bay_label_dropped: u32,
    /// Which bay's block needed which step of the ladder, as `<slot>:<step>` pairs (`fan:cut`, `nozzle:dropped`).
    pub bay_label_notes: String,
    /// The three collapsible headers, as drawn: `<name>:<x>,<y>,<w>,<h>,<open>` joined by `;`.
    pub section_rects: String,
    /// Pairs of bay label blocks that overlap each other.
    pub bay_label_overlaps: u32,
    /// Pairs of annotation plates (bay blocks, zone call-outs, legend) that overlap each other.
    pub plate_overlaps: u32,
    /// Every annotation plate drawn in the section this frame.
    pub plates: u32,
    /// The plates' own rects (`x,y,w,h` joined by `;`) - the section's annotation layer, as drawn.
    pub plate_rects: String,
    /// The selected-part strip's hint state: `none`, `dragging` or `fitted`.
    pub hint_state: &'static str,
    /// The one hint string the strip shows in that state.
    pub hint: String,
    /// Read-out rows that show a value with its unit.
    pub readout_rows: u32,
    /// The same rows, counted as (value, unit) pairs - they must be equal: every value keeps its unit.
    pub readout_units: u32,
    /// The footer's `staged:` text as drawn (empty when nothing was staged).
    pub staged_text: String,
    /// Did the status strip have to truncate it?
    pub staged_truncated: bool,
    /// Overlapping pairs among the status strip's own items.
    pub footer_overlaps: u32,
    /// The three collapsible sections, and the one-line summary each shows when collapsed.
    pub duty_open: bool,
    pub water_open: bool,
    pub limits_open: bool,
    pub duty_summary: String,
    pub water_summary: String,
    pub limits_summary: String,
}

impl LayoutInfo {
    /// Start a frame: clear the counters, and record the right column's own width (0 on a phone or in the
    /// seams view). Everything else is set by the drawer that produces it.
    pub fn reset(&mut self, right_w: f32) {
        *self = LayoutInfo {
            right_w,
            ..LayoutInfo::default()
        };
    }
}

/// Issue #91: **the on-screen text inventory.** Every text shape the frame actually painted (any visible
/// egui layer, clipped to its own clip rect and to the window), read back from egui's own paint lists at
/// the end of the pass. The word counts in `docs/design/usability-r1/README.md` are computed from this,
/// so "the screen shows less text" is a measurement, not a claim. Measurement only: nothing is drawn.
#[derive(Resource, Default)]
pub struct ScreenText {
    /// `(text, [x, y, w, h])`, sorted top-to-bottom then left-to-right.
    pub items: Vec<(String, [f32; 4])>,
}

impl ScreenText {
    pub fn collect(&mut self, ctx: &egui::Context) {
        let screen = ctx.viewport_rect();
        let layers = ctx.memory(|m| m.areas().visible_layer_ids());
        let mut out: Vec<(String, [f32; 4])> = Vec::new();
        ctx.graphics(|g| {
            for layer in layers.iter() {
                if let Some(list) = g.get(*layer) {
                    for cs in list.all_entries() {
                        walk(&cs.shape, cs.clip_rect, screen, &mut out);
                    }
                }
            }
        });
        out.sort_by(|a, b| (a.1[1] as i32, a.1[0] as i32).cmp(&(b.1[1] as i32, b.1[0] as i32)));
        self.items = out;
    }

    pub fn json(&self) -> String {
        let strings: Vec<&str> = self.items.iter().map(|(s, _)| s.as_str()).collect();
        let words: usize = strings
            .iter()
            .map(|s| {
                s.split_whitespace()
                    .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
                    .count()
            })
            .sum();
        serde_json::json!({
            "words": words,
            "strings": strings,
            "rects": self.items.iter().map(|(_, r)| r.iter().map(|v| v.round()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

fn walk(
    shape: &egui::Shape,
    clip: egui::Rect,
    screen: egui::Rect,
    out: &mut Vec<(String, [f32; 4])>,
) {
    match shape {
        egui::Shape::Text(ts) => {
            let r = ts.visual_bounding_rect();
            let seen = r.intersect(clip).intersect(screen);
            if seen.width() > 1.0 && seen.height() > 1.0 && ts.opacity_factor > 0.05 {
                let text = ts
                    .galley
                    .text()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.is_empty() {
                    out.push((text, [r.min.x, r.min.y, r.width(), r.height()]));
                }
            }
        }
        egui::Shape::Vec(v) => {
            for s in v {
                walk(s, clip, screen, out);
            }
        }
        _ => {}
    }
}

/// The mark a collapsible section header draws when the section is open / collapsed. The bundled Plex
/// subset carries no geometric triangles (U+25BE / U+25B8 are absent from its cmap, and a missing glyph
/// draws as tofu), so the pair is the same one the fill-stack rows already use for reorder: `v` and `>`.
pub const CARET_OPEN: &str = "v";
pub const CARET_CLOSED: &str = ">";
