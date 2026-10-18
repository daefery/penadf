//! Light look with a red accent, close to what Windows users expect from a PDF tool.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(196, 30, 58);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(253, 236, 239);
pub const CANVAS: Color32 = Color32::from_rgb(232, 234, 237);
pub const MUTED: Color32 = Color32::from_rgb(96, 102, 112);
pub const ADDED: Color32 = Color32::from_rgb(220, 245, 226);
pub const REMOVED: Color32 = Color32::from_rgb(253, 226, 226);

/// The system's own UI font file: Segoe UI on Windows, Helvetica Neue on macOS.
fn system_font() -> Option<Vec<u8>> {
    let windir = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into());
    let candidates = [
        windir.join("Fonts").join("segoeui.ttf"),
        // macOS: a single-face TrueType file that egui can read (SF Pro ships as a collection).
        "/System/Library/Fonts/Supplemental/Arial.ttf".into(),
    ];
    candidates.iter().find_map(|p| std::fs::read(p).ok())
}

/// Uses the system's UI font when present, keeping egui's bundled fonts as fallback for
/// symbols. Read from disk at start-up, so it adds nothing to the app size.
fn use_system_font(ctx: &egui::Context) {
    let Some(bytes) = system_font() else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "segoe".into(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        list.insert(0, "segoe".into());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    use_system_font(ctx);
    let mut v = Visuals::light();
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(240, 241, 243);
    v.widgets.active.weak_bg_fill = ACCENT_SOFT;
    v.window_corner_radius = CornerRadius::same(8);
    v.panel_fill = Color32::from_rgb(250, 250, 251);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 26.0;
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(19.0));
    });
}
