//! Modal windows. Each returns what the user chose; the app performs the work.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use penadf::gui::theme;
use penadf::ops::compare::{Change, Report};
use penadf::ops::forms::{Field, FieldKind};
use penadf::ops::markup::Signature;
use penadf::ops::protect::Allowed;
use penadf::ops::{PagePoint, PageRect};

pub enum Outcome<T> {
    Open,
    Cancel,
    Done(T),
}

fn modal<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
    let response = egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.heading(title);
        ui.add_space(6.0);
        add(ui)
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let close = response.should_close() || escape;
    (response.inner, close)
}

fn buttons(ui: &mut egui::Ui, ok_label: &str, ok_enabled: bool) -> (bool, bool) {
    ui.add_space(8.0);
    let mut ok = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        ok = ui
            .add_enabled(
                ok_enabled,
                egui::Button::new(egui::RichText::new(ok_label).color(Color32::WHITE))
                    .fill(theme::ACCENT),
            )
            .clicked();
        cancel = ui.button("Cancel").clicked();
    });
    (ok, cancel)
}

fn enter(ui: &egui::Ui) -> bool {
    ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
}

// ---------- Password ----------

#[derive(Default)]
pub struct PasswordDialog {
    pub file_name: String,
    pub password: String,
    pub wrong: bool,
}

impl PasswordDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<String> {
        let ((ok, cancel), close) = modal(ctx, "pw", "Password required", |ui| {
            ui.label(format!(
                "\"{}\" is protected. Enter its password to open it.",
                self.file_name
            ));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.password)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
            edit.request_focus();
            if self.wrong {
                ui.colored_label(theme::ACCENT, "That password is not right. Try again.");
            }
            let (ok, cancel) = buttons(ui, "Open", !self.password.is_empty());
            (ok || (enter(ui) && !self.password.is_empty()), cancel)
        });
        if ok {
            Outcome::Done(self.password.clone())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}

// ---------- Text entry (sticky note, add text) ----------

pub struct TextDialog {
    pub title: &'static str,
    pub page: usize,
    pub at: PagePoint,
    pub text: String,
    pub size: f32,
    pub with_size: bool,
}

impl TextDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let ((ok, cancel), close) = modal(ctx, "text", self.title, |ui| {
            let edit = ui.add(
                egui::TextEdit::multiline(&mut self.text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            if !edit.has_focus() && self.text.is_empty() {
                edit.request_focus();
            }
            if self.with_size {
                ui.horizontal(|ui| {
                    ui.label("Size");
                    ui.add(egui::Slider::new(&mut self.size, 6.0..=48.0).suffix(" pt"));
                });
            }
            ui.label(
                egui::RichText::new("Shift+Enter for a new line.")
                    .color(theme::MUTED)
                    .small(),
            );
            let ready = !self.text.trim().is_empty();
            let (ok, cancel) = buttons(ui, "Add", ready);
            (ok || (enter(ui) && ready), cancel)
        });
        if ok {
            self.text = self.text.trim_end().to_string();
            Outcome::Done(())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}

// ---------- Signature pad ----------

pub struct SignatureDialog {
    pub signature: Signature,
    pub remember: bool,
    /// Where to place it once drawn (set when opened from the Sign tool).
    pub target: Option<(usize, PageRect)>,
    drawing: bool,
}

impl SignatureDialog {
    pub fn new(existing: Option<Signature>, target: Option<(usize, PageRect)>) -> Self {
        Self {
            signature: existing.unwrap_or_default(),
            remember: true,
            target,
            drawing: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let ((ok, cancel), close) = modal(ctx, "sig", "Your signature", |ui| {
            ui.label("Sign in the box with your mouse, pen or finger.");
            let size = Vec2::new(420.0, 160.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, Color32::WHITE);
            painter.rect_stroke(
                rect,
                6.0,
                Stroke::new(1.0, Color32::from_gray(190)),
                egui::StrokeKind::Inside,
            );
            let base = rect.bottom() - 36.0;
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 20.0, base),
                    Pos2::new(rect.right() - 20.0, base),
                ],
                Stroke::new(1.0, Color32::from_gray(210)),
            );

            // Store in a unit box with y up, so it scales to any placement.
            let to_unit = |p: Pos2| PagePoint {
                x: (p.x - rect.left()) / rect.height(),
                y: (rect.bottom() - p.y) / rect.height(),
            };
            let to_screen = |p: &PagePoint| {
                Pos2::new(
                    rect.left() + p.x * rect.height(),
                    rect.bottom() - p.y * rect.height(),
                )
            };
            if response.drag_started() {
                self.signature.strokes.push(Vec::new());
                self.drawing = true;
            }
            if self.drawing
                && let (Some(pos), Some(stroke)) = (
                    response.interact_pointer_pos(),
                    self.signature.strokes.last_mut(),
                )
                && rect.contains(pos)
            {
                stroke.push(to_unit(pos));
            }
            if response.drag_stopped() {
                self.drawing = false;
            }
            // Strokes being drawn use the pad's own coordinates; a saved signature may come
            // from a different pad size, so show it fitted inside the pad (with a margin).
            let shown = if self.drawing {
                self.signature.strokes.clone()
            } else {
                let h = rect.height();
                self.signature.fit(PageRect {
                    left: 0.06,
                    bottom: 0.08,
                    right: rect.width() / h - 0.06,
                    top: 0.92,
                })
            };
            for s in &shown {
                let pts: Vec<Pos2> = s.iter().map(to_screen).collect();
                painter.add(egui::Shape::line(
                    pts,
                    Stroke::new(2.2, Color32::from_rgb(16, 24, 64)),
                ));
            }
            ui.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    self.signature = Signature::default();
                }
                ui.checkbox(&mut self.remember, "Save this signature on this PC");
            });
            let label = if self.target.is_some() {
                "Place signature"
            } else {
                "Save"
            };
            buttons(ui, label, !self.signature.is_empty())
        });
        if ok {
            Outcome::Done(())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}

// ---------- Protect ----------

pub struct ProtectDialog {
    pub require_open_password: bool,
    pub open_password: String,
    pub owner_password: String,
    pub confirm: String,
    pub allowed: Allowed,
}

impl Default for ProtectDialog {
    fn default() -> Self {
        Self {
            require_open_password: true,
            open_password: String::new(),
            owner_password: String::new(),
            confirm: String::new(),
            allowed: Allowed::default(),
        }
    }
}

impl ProtectDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let ((ok, cancel), close) = modal(ctx, "protect", "Protect PDF", |ui| {
            ui.checkbox(
                &mut self.require_open_password,
                "Require a password to open the file",
            );
            if self.require_open_password {
                ui.label("Open password");
                ui.add(
                    egui::TextEdit::singleline(&mut self.open_password)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
            }
            ui.add_space(4.0);
            ui.label("Permissions password (needed to remove protection later)");
            ui.add(
                egui::TextEdit::singleline(&mut self.owner_password)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
            ui.label("Type it again");
            ui.add(
                egui::TextEdit::singleline(&mut self.confirm)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);
            ui.label("People who open the file may:");
            ui.checkbox(&mut self.allowed.print, "Print");
            ui.checkbox(&mut self.allowed.copy, "Copy text and images");
            ui.checkbox(&mut self.allowed.edit, "Edit and organize pages");
            ui.checkbox(
                &mut self.allowed.comment_and_fill,
                "Comment, fill forms and sign",
            );

            let problem = if self.owner_password.is_empty() {
                Some("Choose a permissions password.")
            } else if self.owner_password != self.confirm {
                Some("The permissions passwords do not match.")
            } else if self.require_open_password && self.open_password.is_empty() {
                Some("Choose an open password, or untick that option.")
            } else if self.require_open_password && self.open_password == self.owner_password {
                Some("Use a different open password and permissions password.")
            } else {
                None
            };
            if let Some(p) = problem {
                ui.colored_label(theme::MUTED, p);
            }
            ui.label(egui::RichText::new("Encryption: AES-256. If you forget the permissions password, protection cannot be removed.").small().color(theme::MUTED));
            buttons(ui, "Protect", problem.is_none())
        });
        if ok {
            Outcome::Done(())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }

    pub fn open_password(&self) -> &str {
        if self.require_open_password {
            &self.open_password
        } else {
            ""
        }
    }
}

// ---------- Simple single-field prompt ----------

pub struct Prompt {
    pub id: &'static str,
    pub title: &'static str,
    pub label: String,
    pub value: String,
    pub ok: &'static str,
    pub secret: bool,
}

impl Prompt {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<String> {
        let ((ok, cancel), close) = modal(ctx, self.id, self.title, |ui| {
            ui.label(&self.label);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.value)
                    .password(self.secret)
                    .desired_width(f32::INFINITY),
            );
            if !edit.has_focus() && self.value.is_empty() {
                edit.request_focus();
            }
            let (ok, cancel) = buttons(ui, self.ok, !self.value.trim().is_empty());
            (ok || (enter(ui) && !self.value.trim().is_empty()), cancel)
        });
        if ok {
            Outcome::Done(self.value.trim().to_string())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}

// ---------- Crop ----------

pub struct CropDialog {
    pub pages: String,
    pub margins_mm: [f32; 4],
}

impl CropDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let ((ok, cancel), close) = modal(ctx, "crop", "Crop pages", |ui| {
            ui.horizontal(|ui| {
                ui.label("Pages");
                ui.text_edit_singleline(&mut self.pages);
            });
            ui.label(
                egui::RichText::new("For example: all, 1-3, 5")
                    .small()
                    .color(theme::MUTED),
            );
            egui::Grid::new("margins").show(ui, |ui| {
                for (label, i) in [("Left", 0), ("Bottom", 1), ("Right", 2), ("Top", 3)] {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.margins_mm[i])
                            .range(0.0..=200.0)
                            .suffix(" mm")
                            .speed(0.5),
                    );
                    ui.end_row();
                }
            });
            buttons(ui, "Crop", self.margins_mm.iter().any(|m| *m > 0.0))
        });
        if ok {
            Outcome::Done(())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }

    pub fn margins_pt(&self) -> [f32; 4] {
        self.margins_mm.map(|mm| mm * 72.0 / 25.4)
    }
}

// ---------- Export images ----------

pub struct ExportImagesDialog {
    pub pages: String,
    pub dpi: f32,
    pub png: bool,
}

impl ExportImagesDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let ((ok, cancel), close) = modal(ctx, "export-img", "Export pages as images", |ui| {
            ui.horizontal(|ui| {
                ui.label("Pages");
                ui.text_edit_singleline(&mut self.pages);
            });
            ui.horizontal(|ui| {
                ui.label("Format");
                ui.selectable_value(&mut self.png, true, "PNG");
                ui.selectable_value(&mut self.png, false, "JPEG");
            });
            ui.horizontal(|ui| {
                ui.label("Quality");
                ui.add(egui::Slider::new(&mut self.dpi, 72.0..=300.0).suffix(" dpi"));
            });
            buttons(ui, "Choose folder…", true)
        });
        if ok {
            Outcome::Done(())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}

// ---------- Fill form ----------

pub struct FormDialog {
    pub fields: Vec<Field>,
    pub flatten: bool,
}

impl FormDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let ((ok, cancel), close) = modal(ctx, "form", "Fill out form", |ui| {
            if self.fields.is_empty() {
                ui.label("This PDF has no fillable fields. Use Add text to type on it, or Sign to sign it.");
                return buttons(ui, "OK", false);
            }
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .show(ui, |ui| {
                    egui::Grid::new("fields")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for f in &mut self.fields {
                                ui.label(format!("{}  (p.{})", f.name, f.page + 1));
                                match f.kind {
                                    FieldKind::Text => {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut f.value)
                                                .desired_width(220.0),
                                        );
                                    }
                                    FieldKind::Checkbox => {
                                        let mut on = f.value == "true";
                                        if ui.checkbox(&mut on, "").changed() {
                                            f.value = on.to_string();
                                        }
                                    }
                                    FieldKind::Other(ref k) => {
                                        ui.label(
                                            egui::RichText::new(format!("{k} (not editable here)"))
                                                .color(theme::MUTED),
                                        );
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
            ui.checkbox(
                &mut self.flatten,
                "Lock answers (flatten) so they cannot be changed",
            );
            buttons(ui, "Apply", true)
        });
        if ok {
            Outcome::Done(())
        } else if cancel || close {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}

// ---------- Compare report ----------

pub struct CompareWindow {
    pub old_name: String,
    pub new_name: String,
    pub report: Report,
    pub only_changes: bool,
}

impl CompareWindow {
    /// Returns false when closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        egui::Window::new("Compare files")
            .open(&mut open)
            .default_size([640.0, 520.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Old: {}  ({} pages)",
                    self.old_name, self.report.old_pages
                ));
                ui.label(format!(
                    "New: {}  ({} pages)",
                    self.new_name, self.report.new_pages
                ));
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(22, 128, 61),
                        format!("+{} added", self.report.added),
                    );
                    ui.colored_label(theme::ACCENT, format!("−{} removed", self.report.removed));
                    ui.checkbox(&mut self.only_changes, "Show only changes");
                });
                if self.report.added + self.report.removed == 0 {
                    ui.label("No text differences found.");
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for line in &self.report.lines {
                            if self.only_changes && line.change == Change::Same {
                                continue;
                            }
                            let (bg, sign) = match line.change {
                                Change::Same => (Color32::TRANSPARENT, " "),
                                Change::Added => (theme::ADDED, "+"),
                                Change::Removed => (theme::REMOVED, "−"),
                            };
                            egui::Frame::new()
                                .fill(bg)
                                .inner_margin(egui::Margin::symmetric(6, 1))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{sign} p.{:<3} {}",
                                            line.page, line.text
                                        ))
                                        .monospace(),
                                    );
                                });
                        }
                    });
            });
        open
    }
}

// ---------- Help / About ----------

pub fn about(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("About PenaDF").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            logo(ui, 48.0);
            ui.vertical(|ui| {
                ui.heading(format!("PenaDF {}", env!("CARGO_PKG_VERSION")));
                ui.label("Open, sign, comment on and organize PDFs, without the internet.");
            });
        });
        ui.add_space(6.0);
        ui.label("Your files never leave this PC. No account, ads or tracking.");
        ui.label("Free and open source under the Apache License 2.0.");
        ui.add_space(6.0);
        ui.label(egui::RichText::new("PDF engine: PDFium (Google). Full third-party licences: THIRD-PARTY-NOTICES.txt in the install folder.").small().color(theme::MUTED));
    });
}

pub fn shortcuts(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Keyboard shortcuts")
        .open(open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).show(ui, |ui| {
                for (k, v) in [
                    ("Ctrl+O", "Open (in a new tab)"),
                    ("Ctrl+W", "Close tab"),
                    ("Ctrl+Tab / Ctrl+Shift+Tab", "Next / previous tab"),
                    ("Ctrl+S", "Save"),
                    ("Ctrl+Shift+S", "Save as"),
                    ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
                    ("Ctrl+F", "Find"),
                    ("Ctrl+P", "Print"),
                    ("Ctrl + / Ctrl −", "Zoom in / out"),
                    ("Ctrl+0", "Fit width"),
                    ("Page Up / Page Down", "Previous / next page"),
                    ("Esc", "Back to Select tool"),
                ] {
                    ui.label(egui::RichText::new(k).monospace());
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
}

/// Paints a small circle-in-rect icon. Kept here so all drawing helpers sit together.
/// The PenaDF logo (same artwork as the app icon), drawn at `size` points.
fn logo(ui: &mut egui::Ui, size: f32) {
    let tex = ui
        .ctx()
        .data_mut(|d| d.get_temp::<egui::TextureHandle>(egui::Id::unique("penadf-logo")))
        .unwrap_or_else(|| {
            let img = image::load_from_memory(include_bytes!("../../assets/penadf-128.png"))
                .map(|i| i.into_rgba8());
            let color = match img {
                Ok(i) => egui::ColorImage::from_rgba_unmultiplied(
                    [i.width() as usize, i.height() as usize],
                    i.as_raw(),
                ),
                Err(_) => egui::ColorImage::new([1, 1], vec![Color32::TRANSPARENT]),
            };
            let t = ui
                .ctx()
                .load_texture("penadf-logo", color, egui::TextureOptions::LINEAR);
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("penadf-logo"), t.clone()));
            t
        });
    ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::splat(size)));
}

pub fn empty_state(ui: &mut egui::Ui) -> bool {
    let mut open = false;
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.25);
        logo(ui, 96.0);
        ui.add_space(12.0);
        ui.heading("Open a PDF to get started");
        ui.label(
            egui::RichText::new(
                "Or drop PDFs, images, text or Office files here to turn them into a PDF.",
            )
            .color(theme::MUTED),
        );
        ui.add_space(10.0);
        open = ui
            .add(
                egui::Button::new(egui::RichText::new("Open file…").color(Color32::WHITE))
                    .fill(theme::ACCENT)
                    .min_size(Vec2::new(140.0, 32.0)),
            )
            .clicked();
    });
    open
}
