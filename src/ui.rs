//! Bruno's look: light theme, orange brand, method colors, tab strips and key/value tables.

use crate::model::Pair;
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, TextEdit, Ui, Vec2};

pub const BRAND: Color32 = Color32::from_rgb(211, 127, 23);
pub const TEXT: Color32 = Color32::from_rgb(52, 52, 52);
pub const MUTED: Color32 = Color32::from_rgb(155, 155, 155);
pub const SIDEBAR: Color32 = Color32::from_rgb(248, 248, 248);
pub const BORDER: Color32 = Color32::from_rgb(229, 229, 229);
pub const HOVER: Color32 = Color32::from_rgb(236, 236, 236);
pub const GREEN: Color32 = Color32::from_rgb(46, 138, 84);
pub const RED: Color32 = Color32::from_rgb(206, 79, 59);
pub const PURPLE: Color32 = Color32::from_rgb(141, 67, 177);

pub fn setup(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    v.panel_fill = Color32::WHITE;
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(250, 250, 250);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = BRAND;
    v.selection.bg_fill = Color32::from_rgb(250, 226, 196);
    v.selection.stroke = Stroke::new(1.0, BRAND);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = Color32::WHITE;
    v.widgets.inactive.weak_bg_fill = Color32::WHITE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(204, 204, 204));
    v.widgets.active.weak_bg_fill = HOVER;
    v.widgets.open.weak_bg_fill = HOVER;
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.window_corner_radius = CornerRadius::same(8);
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.interact_size.y = 24.0;
    for (ts, size) in [(egui::TextStyle::Body, 13.5), (egui::TextStyle::Button, 13.0), (egui::TextStyle::Monospace, 13.0), (egui::TextStyle::Small, 11.0), (egui::TextStyle::Heading, 17.0)] {
        if let Some(f) = style.text_styles.get_mut(&ts) {
            f.size = size;
        }
    }
    ctx.set_global_style(style);
}

pub fn method_color(m: &str) -> Color32 {
    match m.to_lowercase().as_str() {
        "get" => GREEN,
        "post" | "patch" => PURPLE,
        "put" => Color32::from_rgb(198, 121, 16),
        "delete" => RED,
        "options" => Color32::from_rgb(46, 138, 135),
        "head" => Color32::from_rgb(48, 137, 166),
        "grpc" => Color32::from_rgb(58, 124, 196),
        "ws" => Color32::from_rgb(198, 121, 16),
        "graphql" => Color32::from_rgb(200, 60, 140),
        _ => MUTED,
    }
}

/// Short sidebar/tab label like Bruno's (DEL, OPT, gRPC, WS, GQL).
pub fn method_label(method: &str, kind: &str) -> String {
    match kind {
        "grpc" => return "gRPC".into(),
        "ws" => return "WS".into(),
        "graphql" => return "GQL".into(),
        _ => {}
    }
    match method.to_lowercase().as_str() {
        "delete" => "DEL".into(),
        "options" => "OPT".into(),
        m => m.to_uppercase(),
    }
}

pub fn method_tag(method: &str, kind: &str) -> RichText {
    let key = if kind == "http" || kind.is_empty() { method } else { kind };
    RichText::new(method_label(method, kind)).size(10.5).color(method_color(key)).strong()
}

/// Bruno-style tab row: grey labels, active one dark with an orange underline, counts as superscripts.
pub fn tab_strip<T: PartialEq + Copy>(ui: &mut Ui, items: &[(T, &str, usize)], current: &mut T) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for (value, label, count) in items {
            let active = *current == *value;
            let mut job = egui::text::LayoutJob::default();
            let color = if active { TEXT } else { MUTED };
            job.append(label, 0.0, egui::TextFormat { color, font_id: egui::FontId::proportional(13.0), ..Default::default() });
            if *count > 0 {
                job.append(&count.to_string(), 2.0, egui::TextFormat { color: if active { BRAND } else { MUTED }, font_id: egui::FontId::proportional(9.5), valign: egui::Align::TOP, ..Default::default() });
            }
            let r = ui.add(egui::Label::new(job).sense(Sense::click()));
            if r.clicked() {
                *current = *value;
            }
            if active {
                let rect = r.rect;
                ui.painter().hline(rect.x_range(), rect.bottom() + 3.0, Stroke::new(2.0, BRAND));
            } else if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    });
    ui.add_space(6.0);
}

pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(BRAND).stroke(Stroke::NONE).corner_radius(CornerRadius::same(5)))
}

pub fn link_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(BRAND)).frame(false))
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(Color32::from_rgb(110, 110, 110)).size(12.0));
}

#[derive(Clone, Copy, Default)]
pub struct TableCfg<'a> {
    pub value_label: &'a str,
    pub description: bool,
    /// multipart rows: text/file type selector and content type
    pub multipart: bool,
    /// show the enable checkbox
    pub toggle: bool,
}

/// Name | Value | Description table; typing in the trailing empty row appends an entry.
pub fn kv_table(ui: &mut Ui, id: &str, rows: &mut Vec<Pair>, cfg: TableCfg) -> bool {
    let mut changed = false;
    let mut remove = None;
    let value_label = if cfg.value_label.is_empty() { "Value" } else { cfg.value_label };
    let full = ui.available_width();
    let name_w = (full * 0.22).clamp(120.0, 260.0);
    let desc_w = if cfg.description { (full * 0.22).clamp(100.0, 260.0) } else { 0.0 };
    let extra_w = if cfg.multipart { 190.0 } else { 0.0 };
    let value_w = (full - name_w - desc_w - extra_w - 90.0).max(140.0);
    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
        egui::Grid::new(id).num_columns(6).spacing(Vec2::new(8.0, 4.0)).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Name").strong().size(12.0));
            if cfg.multipart {
                ui.label(RichText::new("Type").strong().size(12.0));
            }
            ui.label(RichText::new(value_label).strong().size(12.0));
            if cfg.description {
                ui.label(RichText::new("Description").strong().size(12.0));
            }
            ui.label("");
            ui.end_row();
            for (i, p) in rows.iter_mut().enumerate() {
                if cfg.toggle {
                    changed |= ui.checkbox(&mut p.enabled, "").changed();
                } else {
                    ui.label("");
                }
                changed |= ui.add_sized([name_w, 20.0], TextEdit::singleline(&mut p.key).hint_text("Name").frame(egui::Frame::NONE)).changed();
                if cfg.multipart {
                    ui.horizontal(|ui| {
                        let before = p.is_file;
                        egui::ComboBox::from_id_salt((id, i, "type")).width(60.0).selected_text(if p.is_file { "File" } else { "Text" }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut p.is_file, false, "Text");
                            ui.selectable_value(&mut p.is_file, true, "File");
                        });
                        if before != p.is_file {
                            p.value.clear();
                            changed = true;
                        }
                        changed |= ui.add(TextEdit::singleline(&mut p.content_type).hint_text("content-type").desired_width(110.0).frame(egui::Frame::NONE)).changed();
                    });
                }
                if cfg.multipart && p.is_file {
                    ui.horizontal(|ui| {
                        let shown = if p.value.is_empty() { "No file selected".to_string() } else { p.value.replace('|', ", ") };
                        ui.add_sized([value_w - 70.0, 20.0], egui::Label::new(RichText::new(shown).color(MUTED)).truncate());
                        if ui.small_button("Browse…").clicked()
                            && let Some(files) = rfd::FileDialog::new().pick_files()
                        {
                            p.value = files.iter().map(|f| f.to_string_lossy().to_string()).collect::<Vec<_>>().join("|");
                            changed = true;
                        }
                    });
                } else {
                    changed |= ui.add_sized([value_w, 20.0], TextEdit::singleline(&mut p.value).hint_text(value_label).frame(egui::Frame::NONE).font(egui::TextStyle::Monospace)).changed();
                }
                if cfg.description {
                    changed |= ui.add_sized([desc_w, 20.0], TextEdit::singleline(&mut p.description).hint_text("Description").frame(egui::Frame::NONE)).changed();
                }
                if ui.add(egui::Button::new(RichText::new("🗑").color(MUTED)).frame(false)).on_hover_text("Delete").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
            // trailing "new row"
            let ghost_id = ui.scope_id().with((id, "ghost"));
            let (mut gk, mut gv): (String, String) = ui.data_mut(|d| d.get_temp(ghost_id).unwrap_or_default());
            ui.label("");
            let k = ui.add_sized([name_w, 20.0], TextEdit::singleline(&mut gk).hint_text("Name").frame(egui::Frame::NONE));
            if cfg.multipart {
                ui.label("");
            }
            let v = ui.add_sized([value_w, 20.0], TextEdit::singleline(&mut gv).hint_text(value_label).frame(egui::Frame::NONE).font(egui::TextStyle::Monospace));
            if cfg.description {
                ui.label("");
            }
            ui.label("");
            ui.end_row();
            if !gk.is_empty() || !gv.is_empty() {
                rows.push(Pair::new(&gk, &gv));
                changed = true;
                // keep typing in the row that was just created
                let focus = if k.has_focus() { k.id } else { v.id };
                ui.data_mut(|d| d.insert_temp(ghost_id, (String::new(), String::new())));
                ui.ctx().memory_mut(|m| m.request_focus(focus));
            } else {
                ui.data_mut(|d| d.insert_temp(ghost_id, (gk, gv)));
            }
        });
    });
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    changed
}

pub fn code_editor(ui: &mut Ui, text: &mut String, rows: usize) -> bool {
    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        ui.add(TextEdit::multiline(text).code_editor().desired_rows(rows).desired_width(f32::INFINITY).frame(egui::Frame::NONE)).changed()
    })
    .inner
}

pub fn code_view(ui: &mut Ui, text: &str) {
    let mut t = text;
    ui.add(TextEdit::multiline(&mut t).code_editor().desired_width(f32::INFINITY).frame(egui::Frame::NONE));
}

pub fn human_size(n: usize) -> String {
    if n < 1024 { format!("{n}B") } else if n < 1024 * 1024 { format!("{:.1}KB", n as f64 / 1024.0) } else { format!("{:.1}MB", n as f64 / 1048576.0) }
}

/// Layout for a URL with `{{variables}}` shown in green, like Bruno.
pub fn highlight_vars(ui: &Ui, text: &str, wrap_width: f32) -> std::sync::Arc<egui::Galley> {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let fmt = |color| egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let end = rest[start..].find("}}").map(|e| start + e + 2).unwrap_or(rest.len());
        job.append(&rest[..start], 0.0, fmt(TEXT));
        job.append(&rest[start..end], 0.0, fmt(GREEN));
        rest = &rest[end..];
    }
    job.append(rest, 0.0, fmt(TEXT));
    job.wrap.max_width = wrap_width;
    ui.fonts_mut(|f| f.layout_job(job))
}

pub fn status_color(status: u16) -> Color32 {
    match status {
        200..=299 => GREEN,
        300..=399 => Color32::from_rgb(198, 121, 16),
        _ => RED,
    }
}
