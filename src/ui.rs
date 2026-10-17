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
/// brand-tinted background for selected rows and items
pub const SELECTED: Color32 = Color32::from_rgb(253, 239, 222);

/// Phosphor icons right after the text font (so icons resolve before emoji), and a system
/// font with Urdu/Arabic glyphs as the last fallback since egui's bundled fonts have none.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("icons".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Phosphor.ttf"))));
    let windir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
    let urdu = ["segoeui.ttf", "tahoma.ttf", "arial.ttf"].iter().find_map(|f| std::fs::read(windir.join("Fonts").join(f)).ok());
    if let Some(bytes) = urdu {
        fonts.font_data.insert("urdu".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    }
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        list.insert(list.len().min(1), "icons".into());
        if fonts.font_data.contains_key("urdu") {
            list.push("urdu".into());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn setup(ctx: &egui::Context) {
    setup_fonts(ctx);
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    v.panel_fill = Color32::WHITE;
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(250, 250, 250);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = BRAND;
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
        w.corner_radius = CornerRadius::same(6);
    }
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(28) };
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(22) };
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    v.selection.bg_fill = SELECTED;
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.menu_margin = egui::Margin::same(6);
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
            job.append(&tr(label), 0.0, egui::TextFormat { color, font_id: egui::FontId::proportional(13.0), ..Default::default() });
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
    let r = ui.add(egui::Button::new(RichText::new(tr(text)).color(Color32::WHITE).strong()).fill(BRAND).stroke(Stroke::NONE).corner_radius(CornerRadius::same(6)).min_size(Vec2::new(64.0, 28.0)));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

pub fn link_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(tr(text)).color(BRAND)).frame(false))
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(tr(text)).color(Color32::from_rgb(110, 110, 110)).size(12.0));
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
    let value_label = &tr(if cfg.value_label.is_empty() { "Value" } else { cfg.value_label });
    let full = ui.available_width();
    let name_w = (full * 0.22).clamp(120.0, 260.0);
    let desc_w = if cfg.description { (full * 0.22).clamp(100.0, 260.0) } else { 0.0 };
    let extra_w = if cfg.multipart { 190.0 } else { 0.0 };
    let value_w = (full - name_w - desc_w - extra_w - 90.0).max(140.0);
    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
        egui::Grid::new(id).num_columns(6).spacing(Vec2::new(8.0, 4.0)).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(tr("Name")).strong().size(12.0));
            if cfg.multipart {
                ui.label(RichText::new(tr("Type")).strong().size(12.0));
            }
            ui.label(RichText::new(value_label).strong().size(12.0));
            if cfg.description {
                ui.label(RichText::new(tr("Description")).strong().size(12.0));
            }
            ui.label("");
            ui.end_row();
            for (i, p) in rows.iter_mut().enumerate() {
                if cfg.toggle {
                    changed |= ui.checkbox(&mut p.enabled, "").changed();
                } else {
                    ui.label("");
                }
                changed |= ui.add_sized([name_w, 20.0], TextEdit::singleline(&mut p.key).hint_text(tr("Name")).frame(egui::Frame::NONE)).changed();
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
                    changed |= ui.add_sized([desc_w, 20.0], TextEdit::singleline(&mut p.description).hint_text(tr("Description")).frame(egui::Frame::NONE)).changed();
                }
                if ui.add(egui::Button::new(RichText::new(crate::icons::TRASH).color(MUTED)).frame(false)).on_hover_text(tr("Delete")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
            // trailing "new row"
            let ghost_id = ui.scope_id().with((id, "ghost"));
            let (mut gk, mut gv): (String, String) = ui.data_mut(|d| d.get_temp(ghost_id).unwrap_or_default());
            ui.label("");
            let k = ui.add_sized([name_w, 20.0], TextEdit::singleline(&mut gk).hint_text(tr("Name")).frame(egui::Frame::NONE));
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

fn bulk_text(rows: &[Pair]) -> String {
    rows.iter().map(|p| format!("{}{}: {}", if p.enabled { "" } else { "//" }, p.key, p.value)).collect::<Vec<_>>().join("\n")
}

/// Bruno's bulk format: one `name: value` per line, `//` disables a row.
pub fn parse_bulk(text: &str, old: &[Pair]) -> Vec<Pair> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (enabled, l) = match l.trim_start().strip_prefix("//") {
                Some(rest) => (false, rest),
                None => (true, l),
            };
            let (k, v) = l.split_once(':').unwrap_or((l, ""));
            let (k, v) = (k.trim(), v.trim());
            let keep = old.iter().find(|p| p.key == k).cloned().unwrap_or_default();
            Pair { key: k.into(), value: v.into(), enabled, ..keep }
        })
        .collect()
}

/// Key/value table with a "Bulk Edit" text mode toggle (state kept per table id).
pub fn kv_editor(ui: &mut Ui, id: &str, rows: &mut Vec<Pair>, cfg: TableCfg) -> bool {
    let mode_id = ui.scope_id().with((id, "bulk"));
    let mut bulk: bool = ui.data(|d| d.get_temp(mode_id).unwrap_or(false));
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if link_button(ui, if bulk { "Key/Value Edit" } else { "Bulk Edit" }).clicked() {
                bulk = !bulk;
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(mode_id, bulk));
    if !bulk {
        return kv_table(ui, id, rows, cfg);
    }
    let text_id = ui.scope_id().with((id, "bulk-text"));
    // keep the user's text (blank lines, half-typed rows) while it still parses to the rows
    let mut text: String = ui.data(|d| d.get_temp::<String>(text_id)).filter(|t| parse_bulk(t, rows) == *rows).unwrap_or_else(|| bulk_text(rows));
    let changed = code_editor(ui, &mut text, 10);
    if changed {
        *rows = parse_bulk(&text, rows);
    }
    ui.data_mut(|d| d.insert_temp(text_id, text));
    ui.label(RichText::new(tr("One `name: value` per line; prefix a line with // to disable it.")).color(MUTED).size(11.5));
    changed
}

/// Context-menu entry with a leading icon.
pub fn menu_item(ui: &mut Ui, icon: &str, label: &str) -> egui::Response {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::TextStyle::Button.resolve(ui.style());
    job.append(icon, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(font.size + 1.5), color: MUTED, valign: egui::Align::Center, ..Default::default() });
    job.append(&tr(label), 8.0, egui::TextFormat { font_id: font, color: TEXT, valign: egui::Align::Center, ..Default::default() });
    ui.add(egui::Button::new(job).frame_when_inactive(false).min_size(Vec2::new(170.0, 0.0)))
}

/// Flat icon button that turns brand-colored on hover.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> egui::Response {
    let id = ui.next_auto_id();
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    ui.add(egui::Button::new(RichText::new(icon).size(15.0).color(if hovered { BRAND } else { MUTED })).frame(false)).on_hover_text(tip)
}

/// Minimal Markdown preview for docs: headings, lists, quotes, rules, fenced code, **bold**, `code`.
pub fn markdown(ui: &mut Ui, text: &str) {
    let mut in_code = false;
    let mut code = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_code {
                egui::Frame::new().fill(Color32::from_rgb(246, 246, 246)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(rtl(code.trim_end())).monospace());
                });
                code.clear();
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            code.push_str(line);
            code.push('\n');
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            ui.add_space(4.0);
        } else if let Some(h) = ["### ", "## ", "# "].iter().find_map(|p| t.strip_prefix(p).map(|r| (p.len(), r))) {
            let size = [0.0, 0.0, 22.0, 18.0, 15.5][h.0];
            let layout = if h.1.chars().any(crate::rtl::is_rtl) { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
            ui.horizontal(|ui| ui.with_layout(layout, |ui| ui.label(RichText::new(rtl(h.1)).size(size).strong())));
        } else if t == "---" || t == "***" {
            ui.separator();
        } else if t.chars().any(crate::rtl::is_rtl) && (t.starts_with("- ") || t.starts_with("* ") || t.starts_with("> ")) {
            // the bullet / quote bar leads an RTL line, so it is drawn on the right
            let color = if t.starts_with('>') { MUTED } else { TEXT };
            inline_md(ui, &format!("{} {}", if t.starts_with('>') { "▍" } else { "•" }, &t[2..]), color);
        } else if let Some(r) = t.strip_prefix("> ") {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("▍").color(BORDER));
                inline_md(ui, r, MUTED);
            });
        } else if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            ui.horizontal_wrapped(|ui| {
                ui.label("•");
                inline_md(ui, r, TEXT);
            });
        } else {
            ui.horizontal_wrapped(|ui| inline_md(ui, t, TEXT));
        }
    }
    if in_code && !code.is_empty() {
        ui.label(RichText::new(rtl(&code)).monospace());
    }
}

fn inline_md(ui: &mut Ui, text: &str, color: Color32) {
    if text.chars().any(crate::rtl::is_rtl) {
        // an RTL paragraph is one label (pieces laid out left-to-right would be out of order)
        let plain = text.replace("**", "").replace('`', "");
        ui.horizontal(|ui| ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(rtl(&plain)).color(color))));
        return;
    }
    ui.spacing_mut().item_spacing.x = 0.0;
    let mut bold = false;
    for (i, seg) in text.split('`').enumerate() {
        if i % 2 == 1 {
            ui.label(RichText::new(seg).monospace().background_color(Color32::from_rgb(242, 242, 242)));
            continue;
        }
        for (j, part) in seg.split("**").enumerate() {
            if j > 0 {
                bold = !bold;
            }
            if !part.is_empty() {
                let r = RichText::new(rtl(part)).color(color);
                ui.label(if bold { r.strong() } else { r });
            }
        }
    }
}

pub fn code_editor(ui: &mut Ui, text: &mut String, rows: usize) -> bool {
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, w: f32| rtl_layout(ui, buf.as_str(), w, egui::TextStyle::Monospace);
    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        ui.add(TextEdit::multiline(text).code_editor().desired_rows(rows).desired_width(f32::INFINITY).frame(egui::Frame::NONE).layouter(&mut layouter)).changed()
    })
    .inner
}

pub fn code_view(ui: &mut Ui, text: &str) {
    let mut t = text;
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, w: f32| rtl_layout(ui, buf.as_str(), w, egui::TextStyle::Monospace);
    ui.add(TextEdit::multiline(&mut t).code_editor().desired_width(f32::INFINITY).frame(egui::Frame::NONE).layouter(&mut layouter));
}

/// Galley for editable text: Urdu/Arabic is shaped and ordered right-to-left one char per char,
/// so cursor indices still match the buffer.
pub fn rtl_layout(ui: &Ui, text: &str, wrap_width: f32, style: egui::TextStyle) -> std::sync::Arc<egui::Galley> {
    let font = style.resolve(ui.style());
    let shown = crate::rtl::visual(text);
    let mut job = egui::text::LayoutJob::simple(shown.into_owned(), font, ui.visuals().override_text_color.unwrap_or(TEXT), wrap_width);
    job.wrap.max_width = wrap_width;
    ui.fonts_mut(|f| f.layout_job(job))
}

/// Display form of text that may contain Urdu/Arabic (labels, names).
pub fn rtl(text: &str) -> String {
    crate::rtl::visual(text).into_owned()
}

/// UI string in the selected language (Urdu strings come back shaped for display).
pub fn tr(text: &str) -> String {
    crate::i18n::tr(text)
}

pub fn human_size(n: usize) -> String {
    if n < 1024 { format!("{n}B") } else if n < 1024 * 1024 { format!("{:.1}KB", n as f64 / 1024.0) } else { format!("{:.1}MB", n as f64 / 1048576.0) }
}

/// Layout for a URL with `{{variables}}` shown in green, like Bruno.
pub fn highlight_vars(ui: &Ui, text: &str, wrap_width: f32) -> std::sync::Arc<egui::Galley> {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let fmt = |color| egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    let shaped = crate::rtl::visual(text);
    let mut rest = shaped.as_ref();
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
