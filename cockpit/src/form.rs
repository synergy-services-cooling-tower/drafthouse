//! Round 4, item 2: **the custom-part form.**
//!
//! Every rail section ends with a `+ custom` chip. Tapping it opens this form for a NEW record of that
//! class, whose fields come from the class's field list in the generated descriptor
//! (`viz/assets/custom-fields.json`, built from `../fixtures-fields.json` + the bundled catalog). The user
//! enters VALUES; there is no way to add a field. Required fields are marked, an out-of-range value is
//! refused with the range named, a curve-typed field gets a point-table editor with add/remove, and the
//! footer carries the session-only note.
//!
//! The form owns no rules of its own: parse, validate and the record a save builds all live in
//! `drafthouse_cockpit_seams::custom` and are unit-tested there. This module is the widget layer.

use bevy::prelude::*;
use bevy_egui::egui;
use egui::{Color32, Rect, RichText, Stroke};

use drafthouse_cockpit_seams::custom::{
    self, ClassSpec, Field, Fields, Kind, PERSISTENCE_INTERNAL, PERSISTENCE_PUBLIC,
};

use crate::app::HitMap;
use crate::state::{Catalog, Class, Flash, Visual};
use crate::theme as t;

/// The form's state: which class, and one [`custom::Input`] per field.
#[derive(Resource, Clone, Debug, Default)]
pub struct CustomForm {
    pub class: Option<Class>,
    pub inputs: std::collections::HashMap<String, custom::Input>,
    /// The refusals the last Save produced (empty after a successful save).
    pub errors: Vec<String>,
    /// The line the strip shows. The app writes it into `Visual::flash` too.
    pub note: String,
    /// True when a frame staged this form (the evidence says which path opened it).
    pub staged: bool,
}

impl CustomForm {
    /// Open the form for a class: blank inputs, one empty row per point table.
    pub fn open(&mut self, class: Class, fields: &Fields, staged: bool) {
        self.class = Some(class);
        self.errors.clear();
        self.note.clear();
        self.staged = staged;
        self.inputs.clear();
        if let Some(spec) = fields.class(class.slug()) {
            for f in spec.fields.iter() {
                let mut input = custom::Input::default();
                if f.kind == Kind::Points {
                    input.rows.push(vec![String::new(); f.columns.len().max(1)]);
                }
                self.inputs.insert(f.key.clone(), input);
            }
        }
    }

    pub fn input_mut(&mut self, key: &str) -> &mut custom::Input {
        self.inputs.entry(key.to_string()).or_default()
    }

    pub fn input(&self, key: &str) -> Option<&custom::Input> {
        self.inputs.get(key)
    }

    /// Set a point table's companion scalar (it lives in the table's own input, where the editor writes it).
    pub fn set_companion(&mut self, field_key: &str, value: &str) {
        self.input_mut(field_key).companion = value.trim().to_string();
    }

    /// Set a text value (used by the URL staging and by the reset button).
    pub fn set_text(&mut self, key: &str, text: &str) {
        self.input_mut(key).text = text.to_string();
    }

    /// Set the first point-table field's rows (URL staging: `?form-rows=55,520,0.63;145,380,0.83`).
    pub fn set_rows(&mut self, key: &str, rows: &[Vec<String>], fields: &Fields, class: Class) {
        if let Some(spec) = fields.class(class.slug()) {
            if let Some(f) = spec.fields.iter().find(|f| f.key == key) {
                if f.kind == Kind::Points {
                    let input = self.input_mut(key);
                    input.rows = rows.to_vec();
                }
            }
        }
    }

    /// Run the Save the button runs: parse + validate every field through `parse_field_entries` (the same
    /// code the tests exercise), build the engine record, and append it to the session catalog - and to the
    /// engine's own catalog, so the record can actually be run and dropped.
    pub fn save(
        &mut self,
        fields: &Fields,
        catalog: &mut Catalog,
        engine: Option<&mut (dyn cockpit::engine::Engine + '_)>,
    ) -> Result<String, Vec<String>> {
        let Some(class) = self.class else {
            return Err(vec!["no class selected".into()]);
        };
        let Some(spec) = fields.class(class.slug()) else {
            return Err(vec![format!("no field list for {}", class.slug())]);
        };
        let mut values: custom::Values = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        for f in spec.fields.iter() {
            let mut input = self.inputs.get(&f.key).cloned().unwrap_or_default();
            if let Some(c) = &f.companion {
                // A companion typed as its own row (the URL staging's own key) lands in the table's input.
                if input.companion.trim().is_empty() {
                    if let Some(v) = self.inputs.get(&c.key) {
                        input.companion = v.text.clone();
                    }
                }
            }
            match custom::parse_field_entries(f, &input) {
                Ok(entries) => values.extend(entries),
                Err(e) => errors.push(e),
            }
        }
        if !errors.is_empty() {
            self.errors = errors;
            return Err(self.errors.clone());
        }
        let part = custom::CustomPart::new(class.slug(), values);
        match catalog.add_custom(&part, fields, engine) {
            Ok(line) => {
                self.errors.clear();
                self.note = line.clone();
                Ok(line)
            }
            Err(e) => {
                self.errors = vec![e.clone()];
                Err(self.errors.clone())
            }
        }
    }
}

/// The `+ custom` chip at the end of a rail section. It is dim and inert when the generated field list did
/// not load: there is no field list to build a form from, and the pass says so rather than inventing one.
pub fn add_chip(
    ui: &mut egui::Ui,
    class: Class,
    vis: &mut Visual,
    form: &mut CustomForm,
    hits: &mut HitMap,
    fields: Option<&Fields>,
) {
    let (fg, line) = if fields.is_some() {
        (t::AMBER, t::with_alpha(t::AMBER, 150))
    } else {
        (t::MUTED, t::LINE_SOFT)
    };
    let resp = t::chip_frame(t::PANEL, line)
        .show(ui, |ui| {
            ui.label(
                RichText::new("+ custom")
                    .size(10.0)
                    .color(fg)
                    .family(t::family_semi())
                    .extra_letter_spacing(0.4),
            );
        })
        .response;
    let r = resp.rect;
    hits.0.push((
        format!("rail:custom:{}", class.slug()),
        [r.min.x, r.min.y, r.width(), r.height()],
    ));
    let resp = match fields {
        Some(_) => resp.on_hover_text(format!(
            "author a new {} from the fixture's own field list: names, units, types and ranges all come from it",
            class.name()
        )),
        None => resp.on_hover_text("the generated field list did not load - no authoring surface"),
    };
    if resp.clicked() {
        if let Some(fields) = fields {
            form.open(class, fields, false);
            vis.selected_slot = class.slot();
            vis.flash = Some(Flash {
                ok: true,
                text: format!("new {} form open", class.name()),
            });
        }
    }
}

/// Draw the form. Returns true while it is open (the caller keeps the frame's `data-form` marker).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ctx: &egui::Context,
    form: &mut CustomForm,
    fields: &Fields,
    catalog: &mut Catalog,
    engine: Option<&mut (dyn cockpit::engine::Engine + '_)>,
    vis: &mut Visual,
    hits: &mut HitMap,
    screen: Rect,
    phone: bool,
    host_public: bool,
) {
    let Some(class) = form.class else { return };
    let Some(spec) = fields.class(class.slug()) else {
        form.class = None;
        return;
    };
    let spec: &ClassSpec = spec;

    // ---- placement: a right-hand panel on a desktop (over the read-out column, never over the bay being
    // dragged to), a bottom sheet on a phone - the same two shapes the picker uses.
    let (pos, width, max_h) = if phone {
        (
            egui::pos2(
                screen.left() + 6.0,
                (screen.top() + 54.0).max(screen.bottom() - 6.0 - screen.height() * 0.66),
            ),
            screen.width() - 12.0,
            screen.height() * 0.66,
        )
    } else {
        let w = 340.0_f32.min(screen.width() - 40.0);
        (
            egui::pos2(
                (screen.right() - w - 10.0).max(screen.left() + 8.0),
                screen.top() + 104.0,
            ),
            w,
            screen.bottom() - (screen.top() + 104.0) - 40.0,
        )
    };

    let mut save = false;
    let mut cancel = false;
    let area = egui::Area::new(egui::Id::new("viz.custom-form"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_max_width(width);
            ui.set_max_height(max_h);
            egui::Frame::new()
                .fill(t::PANEL_RAISED)
                .stroke(Stroke::new(1.0, t::AMBER))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("new {}", class.name()))
                                .size(13.0)
                                .color(t::INK)
                                .family(t::family_semi()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_sized(egui::vec2(24.0, 22.0), egui::Button::new("x"))
                                .on_hover_text("close (esc)")
                                .clicked()
                            {
                                cancel = true;
                            }
                            t::chip_frame(t::AMBER_SOFT, t::with_alpha(t::AMBER, 140)).show(ui, |ui| {
                                ui.label(
                                    RichText::new("custom")
                                        .size(10.0)
                                        .color(t::AMBER)
                                        .family(t::family_semi()),
                                );
                            });
                        });
                    });
                    ui.label(
                        RichText::new(format!(
                            "fields generated from fixtures-fields.json · {} field(s) · required fields marked * · no way to add a field",
                            spec.fields.len()
                        ))
                        .size(9.5)
                        .color(t::MUTED),
                    );
                    ui.add_space(4.0);
                    let scroll_h = (max_h - 132.0).max(120.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .max_height(scroll_h)
                        .show(ui, |ui| {
                            for f in spec.fields.iter() {
                                field_row(ui, f, form, hits, phone);
                            }
                        });
                    ui.add_space(4.0);
                    ui.separator();
                    // ---- the refusals, with the range named (they come from the seams crate)
                    for e in form.errors.iter().take(4) {
                        ui.label(RichText::new(e).size(10.0).color(t::DANGER));
                    }
                    if form.errors.len() > 4 {
                        ui.label(
                            RichText::new(format!("… and {} more refusal(s)", form.errors.len() - 4))
                                .size(9.5)
                                .color(t::DANGER),
                        );
                    }
                    if !form.note.is_empty() {
                        ui.label(RichText::new(&form.note).size(10.0).color(t::VALID));
                    }
                    ui.label(
                        RichText::new(if host_public {
                            PERSISTENCE_PUBLIC
                        } else {
                            PERSISTENCE_INTERNAL
                        })
                        .size(9.0)
                        .color(t::MUTED),
                    );
                    ui.horizontal(|ui| {
                        if hit(
                            hits,
                            "ctl:form-save",
                            ui.add_sized(egui::vec2(92.0, 24.0), egui::Button::new("save")),
                        )
                        .clicked()
                        {
                            save = true;
                        }
                        if hit(
                            hits,
                            "ctl:form-cancel",
                            ui.add_sized(egui::vec2(84.0, 24.0), egui::Button::new("cancel")),
                        )
                        .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
        });
    let _ = area;

    // Esc closes, exactly like the picker.
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            cancel = true;
        }
    });

    if save {
        let class_slug = form.class.map(|c| c.slug().to_string());
        match form.save(fields, catalog, engine) {
            Ok(line) => {
                // The new record sits at the end of its own rail section; ask the rail to bring its chip
                // into view so the save is visible where the chip appears (not only in the flash line).
                if let Some(slug) = class_slug.clone() {
                    let id = line
                        .split_whitespace()
                        .find(|w| w.starts_with("CP-"))
                        .unwrap_or_default()
                        .to_string();
                    if !id.is_empty() {
                        vis.scroll_to = Some(format!("{slug}:{id}"));
                    }
                }
                vis.flash = Some(Flash {
                    ok: true,
                    text: line,
                });
                form.class = None;
            }
            Err(errors) => {
                vis.flash = Some(Flash {
                    ok: false,
                    text: format!(
                        "refused: {}",
                        errors
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "invalid value".into())
                    ),
                });
            }
        }
    } else if cancel {
        form.class = None;
        form.errors.clear();
        vis.flash = Some(Flash {
            ok: true,
            text: "form closed".into(),
        });
    }
}

/// One field of the form: the engine's own key, its unit, its recorded range and the control.
fn field_row(ui: &mut egui::Ui, f: &Field, form: &mut CustomForm, hits: &mut HitMap, phone: bool) {
    let hint = custom::range_hint(f);
    let input = form.input_mut(&f.key);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&f.key)
                .size(11.0)
                .color(t::INK)
                .family(t::family_mono_med()),
        );
        if f.required {
            ui.label(
                RichText::new("*")
                    .size(11.0)
                    .color(t::AMBER)
                    .family(t::family_semi()),
            );
        }
        if f.unit != "-" && !f.unit.is_empty() {
            ui.label(
                RichText::new(&f.unit)
                    .size(9.5)
                    .color(t::MUTED)
                    .family(t::family_mono_med()),
            );
        }
    });
    if !hint.is_empty() {
        ui.label(RichText::new(hint).size(9.0).color(t::MUTED));
    }
    let w = if phone { 240.0 } else { 300.0 };
    match f.kind {
        Kind::Points => {
            // The point-table editor: one row of one-line cells per point, add/remove.
            let cols = f.columns.clone();
            let mut remove: Option<usize> = None;
            // The row count is read before the mutable iteration: a "+ point" that arrives while the rows
            // are being drawn lands in the same frame, and a removal is applied after the loop.
            let row_count = input.rows.len();
            for (i, row) in input.rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    for (c, col) in cols.iter().enumerate() {
                        let width = (w / cols.len().max(1) as f32).max(56.0) - 6.0;
                        if row.len() <= c {
                            row.resize(c + 1, String::new());
                        }
                        let r = ui.add_sized(
                            egui::vec2(width, 20.0),
                            egui::TextEdit::singleline(&mut row[c]).hint_text(&col.key),
                        );
                        hits.0.push((
                            format!("form:{}:{}:{}:{}", f.key, i, c, col.key),
                            [r.rect.min.x, r.rect.min.y, r.rect.width(), r.rect.height()],
                        ));
                    }
                    if row_count > 1
                        && ui
                            .add_sized(egui::vec2(20.0, 20.0), egui::Button::new("x"))
                            .on_hover_text("remove this point")
                            .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                input.rows.remove(i);
            }
            ui.horizontal(|ui| {
                if hit(
                    hits,
                    &format!("form:{}:add-row", f.key),
                    ui.add_sized(egui::vec2(72.0, 20.0), egui::Button::new("+ point")),
                )
                .clicked()
                {
                    input.rows.push(vec![String::new(); cols.len().max(1)]);
                }
                if let Some(c) = &f.companion {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&c.key)
                                .size(10.0)
                                .color(t::INK_2)
                                .family(t::family_mono_med()),
                        );
                        let r = ui.add_sized(
                            egui::vec2(96.0, 20.0),
                            egui::TextEdit::singleline(&mut input.companion)
                                .hint_text(c.unit.clone()),
                        );
                        hits.0.push((
                            format!("form:{}:companion", c.key),
                            [r.rect.min.x, r.rect.min.y, r.rect.width(), r.rect.height()],
                        ));
                        ui.label(
                            RichText::new("required for this table")
                                .size(9.0)
                                .color(t::MUTED),
                        );
                    });
                }
            });
        }
        _ => {
            let r = ui.add_sized(
                egui::vec2(w, 20.0),
                egui::TextEdit::singleline(&mut input.text).hint_text(match f.kind {
                    Kind::Number => "value",
                    Kind::Range2 => "lo,hi",
                    Kind::Object => "comma-separated",
                    Kind::TextList => "comma-separated",
                    _ => "value",
                }),
            );
            hits.0.push((
                format!("form:{}", f.key),
                [r.rect.min.x, r.rect.min.y, r.rect.width(), r.rect.height()],
            ));
            if !f.one_of.is_empty() {
                ui.label(
                    RichText::new(format!("one of: {}", f.one_of.join(" | ")))
                        .size(9.0)
                        .color(t::MUTED),
                );
            }
            if !f.note.is_empty() && f.note != "may be left blank" {
                ui.label(RichText::new(&f.note).size(9.0).color(t::MUTED));
            }
        }
    }
}

/// The `data-form-*` markers the frame reads (the same idea as every other `data-*` in the pass).
pub fn markers(form: &CustomForm, fields: Option<&Fields>) -> (String, String, String, String) {
    let class = form.class.map(|c| c.slug().to_string()).unwrap_or_default();
    let field_count = form
        .class
        .and_then(|c| fields.and_then(|f| f.class(c.slug())))
        .map(|s| s.fields.len())
        .unwrap_or(0);
    let errors = form.errors.len();
    let first_error = form.errors.first().cloned().unwrap_or_default();
    (
        class,
        field_count.to_string(),
        errors.to_string(),
        first_error,
    )
}

fn hit(hits: &mut HitMap, key: &str, resp: egui::Response) -> egui::Response {
    let r = resp.rect;
    hits.0
        .push((key.to_string(), [r.min.x, r.min.y, r.width(), r.height()]));
    resp
}

/// The amber dot a custom chip carries (the same colour the form's own chip uses, so the two read as one
/// state: this record was authored here, and it lives in this session only).
pub const CUSTOM_DOT: Color32 = t::AMBER;
