//! The main window: toolbar, tool strip, page thumbnails, viewer and side panels.

use super::dialogs::{self, Outcome};
use anyhow::{Context, Result, bail};
use eframe::egui::{self, Color32, RichText, Vec2};
use penadf::gui::document::Document;
use penadf::gui::tabs::{self, TabBarAction, TabLabel, TabStrip};
use penadf::gui::theme;
use penadf::gui::viewer::{Fit, PageAction, Tool, Viewer};
use penadf::ops::{
    self, PageRect, SearchHit, compare, create, export, forms, markup, pages, protect,
};
use penadf::{engine, page_range, winshell};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePanel {
    None,
    Comments,
    Organize,
}

enum Dialog {
    None,
    Password {
        path: Option<PathBuf>,
        name: String,
        bytes: Vec<u8>,
        ui: dialogs::PasswordDialog,
    },
    Text {
        tool: Tool,
        ui: dialogs::TextDialog,
    },
    Signature(dialogs::SignatureDialog),
    Protect(dialogs::ProtectDialog),
    Unprotect(dialogs::Prompt),
    Split(dialogs::Prompt),
    Extract(dialogs::Prompt),
    Crop(dialogs::CropDialog),
    ExportImages(dialogs::ExportImagesDialog),
    Form(dialogs::FormDialog),
}

/// Everything that belongs to one tab. The active tab's copy lives directly in
/// [`PenaApp`] (fields of the same names); the others are parked in `tabs`.
#[derive(Default)]
struct TabState {
    doc: Option<Document>,
    viewer: Viewer,
    search: String,
    hits: Vec<SearchHit>,
    active_hit: Option<usize>,
    page_field: String,
    comments_cache: Option<Vec<markup::Comment>>,
}

pub struct PenaApp {
    // Active tab (see TabState).
    doc: Option<Document>,
    viewer: Viewer,
    search: String,
    hits: Vec<SearchHit>,
    active_hit: Option<usize>,
    page_field: String,
    comments_cache: Option<Vec<markup::Comment>>,
    /// All open tabs; the active one's slot is empty while its state sits in the fields above.
    tabs: TabStrip<TabState>,

    dialog: Dialog,
    compare: Option<dialogs::CompareWindow>,
    side: SidePanel,
    search_open: bool,
    toast: Option<(String, bool, f64)>,
    signature: Option<markup::Signature>,
    show_about: bool,
    show_keys: bool,
    allow_close: bool,
    /// Files handed over by later launches (double-clicking a PDF while the app runs).
    incoming: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    /// Hide the "make PenaDF your default" bar for good.
    default_hint_dismissed: bool,
}

impl PenaApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        open: Vec<PathBuf>,
        incoming: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let signature = signature_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| markup::Signature::from_text(&t))
            .filter(|s| !s.is_empty());
        let mut app = Self {
            doc: None,
            viewer: Viewer::default(),
            dialog: Dialog::None,
            compare: None,
            side: SidePanel::None,
            search: String::new(),
            search_open: false,
            hits: Vec::new(),
            active_hit: None,
            toast: None,
            signature,
            show_about: false,
            show_keys: false,
            allow_close: false,
            page_field: String::new(),
            comments_cache: None,
            tabs: TabStrip::default(),
            incoming,
            default_hint_dismissed: settings_flag(DEFAULT_HINT_FLAG),
        };
        if let Err(e) = engine::pdfium() {
            app.error(format!("{e}"));
        }
        app.open_paths(open);
        #[cfg(feature = "screenshot")]
        app.screenshot_setup();
        app
    }

    // ---------- messages ----------

    fn info(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), false, 0.0));
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), true, 0.0));
    }

    fn report<T>(&mut self, r: Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.error(format!("{e:#}"));
                None
            }
        }
    }

    // ---------- opening and saving ----------

    fn open_dialog(&mut self) {
        let files = rfd::FileDialog::new()
            .add_filter("PDF and convertible files", &all_exts())
            .add_filter("PDF", &["pdf"])
            .add_filter("Images", create::IMAGE_EXTS)
            .add_filter("Office documents", create::OFFICE_EXTS)
            .pick_files();
        if let Some(files) = files {
            self.open_paths(files);
        }
    }

    /// Each PDF opens in its own tab (or switches to it if already open).
    /// Other files are converted and combined into one new PDF tab.
    fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let (pdfs, others): (Vec<PathBuf>, Vec<PathBuf>) = paths
            .into_iter()
            .partition(|p| create::kind_of(p) == Some(create::SourceKind::Pdf));
        for path in pdfs {
            if self.switch_to_open_file(&path) {
                continue;
            }
            match std::fs::read(&path) {
                Ok(bytes) => self.load(Some(path.clone()), file_name(&path), bytes, None),
                Err(e) => self.error(format!("Could not read {}: {e}", path.display())),
            }
            // A password prompt pauses the queue; later files would hide behind it.
            if !matches!(self.dialog, Dialog::None) {
                break;
            }
        }
        if !others.is_empty() {
            self.create_from(others);
        }
    }

    /// Converts files (images, text, Office, or PDFs) into one new, unsaved PDF tab.
    fn create_from(&mut self, paths: Vec<PathBuf>) {
        let result = self.combine_files(&paths);
        if let Some(bytes) = self.report(result) {
            let name = if paths.len() == 1 {
                format!("{}.pdf", stem(&paths[0]))
            } else {
                "Combined.pdf".to_string()
            };
            self.load(None, name, bytes, None);
            if let Some(d) = &mut self.doc {
                d.dirty = true;
            }
            self.info(format!(
                "Created a PDF from {} file(s). Save it with Ctrl+S.",
                paths.len()
            ));
        }
    }

    fn combine_files(&self, paths: &[PathBuf]) -> Result<Vec<u8>> {
        // Consecutive images become one PDF with a page per image.
        let all_images = paths
            .iter()
            .all(|p| create::kind_of(p) == Some(create::SourceKind::Image));
        if all_images {
            let images = paths
                .iter()
                .map(std::fs::read)
                .collect::<std::io::Result<Vec<_>>>()?;
            return create::from_images(&images);
        }
        let mut parts = Vec::new();
        for p in paths {
            let bytes = create::from_file(p).with_context(|| file_name(p).to_string())?;
            if protect::is_encrypted(&bytes) {
                bail!(
                    "{} is password protected. Open it on its own first and remove the protection.",
                    file_name(p)
                );
            }
            parts.push((bytes, None));
        }
        pages::merge(&parts)
    }

    fn load(
        &mut self,
        path: Option<PathBuf>,
        name: String,
        bytes: Vec<u8>,
        password: Option<String>,
    ) {
        match Document::open(path.clone(), name.clone(), bytes.clone(), password.clone()) {
            Ok(doc) => {
                self.open_tab(doc);
                self.dialog = Dialog::None;
            }
            Err(e) if engine::is_password_error(&e) => {
                let wrong = password.is_some();
                self.dialog = Dialog::Password {
                    path,
                    name: name.clone(),
                    bytes,
                    ui: dialogs::PasswordDialog {
                        file_name: name,
                        password: String::new(),
                        wrong,
                    },
                };
            }
            Err(e) => self.error(format!("Could not open {name}: {e:#}")),
        }
    }

    fn save(&mut self, save_as: bool) {
        let Some(doc) = &mut self.doc else { return };
        let path = match (&doc.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(&doc.name)
                .save_file()
                .map(|p| with_ext(p, "pdf")),
        };
        let Some(path) = path else { return };
        // Write to a temp file first so a failed save never destroys the original.
        let tmp = path.with_extension("pdf.penadf-tmp");
        let result = std::fs::write(&tmp, &doc.bytes).and_then(|_| std::fs::rename(&tmp, &path));
        match result {
            Ok(()) => {
                doc.path = Some(path.clone());
                doc.name = file_name(&path);
                doc.dirty = false;
                self.info(format!("Saved {}", path.display()));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.error(format!("Could not save: {e}"));
            }
        }
    }

    fn confirm_discard(&mut self) -> bool {
        match &self.doc {
            Some(d) if d.dirty => {
                let answer = rfd::MessageDialog::new()
                    .set_title("Unsaved changes")
                    .set_description(format!("Save changes to \"{}\" first?", d.name))
                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                    .show();
                match answer {
                    rfd::MessageDialogResult::Yes => {
                        self.save(false);
                        !self.doc.as_ref().is_some_and(|d| d.dirty)
                    }
                    rfd::MessageDialogResult::No => true,
                    _ => false,
                }
            }
            _ => true,
        }
    }

    // ---------- document changes ----------

    /// Runs an operation on the open document and keeps the result (with undo).
    fn change(&mut self, what: &str, op: impl FnOnce(&[u8], Option<&str>) -> Result<Vec<u8>>) {
        let Some(doc) = &mut self.doc else { return };
        let result = op(&doc.bytes, doc.pw()).and_then(|b| doc.apply(b));
        match result {
            Ok(()) => {
                self.comments_cache = None;
                self.hits.clear();
                self.info(what.to_string());
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    fn require(&mut self, allowed: bool, what: &str) -> bool {
        if !allowed {
            self.error(format!("The owner of this PDF does not allow {what}. Open it with the permissions password to do this."));
        }
        allowed
    }

    fn selected_pages(&self, spec: &str) -> Result<Vec<usize>> {
        let n = self.doc.as_ref().map_or(0, Document::page_count);
        page_range::parse(spec, n)
    }

    fn run_page_action(&mut self, action: PageAction) {
        let Some(doc) = &self.doc else { return };
        let can_comment = doc.allowed.comment_and_fill || doc.allowed.edit;
        let can_edit = doc.allowed.edit;
        match action {
            PageAction::Box { page, tool, rect } => match tool {
                Tool::Highlight | Tool::Underline | Tool::StrikeOut => {
                    if !self.require(can_comment, "comments") {
                        return;
                    }
                    let kind = match tool {
                        Tool::Highlight => markup::TextMarkup::Highlight,
                        Tool::Underline => markup::TextMarkup::Underline,
                        _ => markup::TextMarkup::StrikeOut,
                    };
                    let color = match kind {
                        markup::TextMarkup::Highlight => markup::Rgba::YELLOW,
                        markup::TextMarkup::Underline => markup::Rgba::BLUE,
                        markup::TextMarkup::StrikeOut => markup::Rgba::RED,
                    };
                    self.change(&format!("{} added", tool.label()), |b, pw| {
                        let lines = markup::text_lines_in(b, pw, page, rect)?;
                        if lines.is_empty() {
                            bail!("There is no selectable text there. Scanned pages need text recognition first; use Draw to mark them instead.");
                        }
                        markup::add_text_markup(b, pw, page, kind, &lines, color)
                    });
                }
                Tool::AddImage => {
                    if !self.require(can_edit, "editing") {
                        return;
                    }
                    let file = rfd::FileDialog::new()
                        .add_filter("Images", create::IMAGE_EXTS)
                        .pick_file();
                    if let Some(file) = file {
                        let result = std::fs::read(&file).map_err(anyhow::Error::from);
                        if let Some(img) = self.report(result) {
                            self.change("Image added", |b, pw| {
                                markup::add_image(b, pw, page, rect, &img)
                            });
                        }
                    }
                }
                Tool::Sign => {
                    if !self.require(doc.allowed.comment_and_fill || can_edit, "signing") {
                        return;
                    }
                    match self.signature.clone() {
                        Some(sig) => self.place_signature(page, rect, &sig),
                        None => {
                            self.dialog = Dialog::Signature(dialogs::SignatureDialog::new(
                                None,
                                Some((page, rect)),
                            ))
                        }
                    }
                }
                _ => {}
            },
            PageAction::Click { page, tool, at } => {
                let (title, with_size, allowed) = match tool {
                    Tool::Note => ("Sticky note", false, can_comment),
                    Tool::AddText => ("Add text", true, can_edit),
                    _ => return,
                };
                if !self.require(
                    allowed,
                    if tool == Tool::Note {
                        "comments"
                    } else {
                        "editing"
                    },
                ) {
                    return;
                }
                self.dialog = Dialog::Text {
                    tool,
                    ui: dialogs::TextDialog {
                        title,
                        page,
                        at,
                        text: String::new(),
                        size: 12.0,
                        with_size,
                    },
                };
            }
            PageAction::InkDone { page, strokes } => {
                if !self.require(can_comment, "comments") {
                    return;
                }
                self.change("Drawing added", |b, pw| {
                    markup::add_ink(b, pw, page, &strokes, markup::Rgba::RED, 2.0)
                });
            }
        }
    }

    fn place_signature(&mut self, page: usize, rect: PageRect, sig: &markup::Signature) {
        // Keep the signature's proportions inside the dragged box.
        self.change("Signed", |b, pw| markup::sign(b, pw, page, rect, sig));
        self.viewer.tool = Tool::Select;
    }

    fn set_tool(&mut self, tool: Tool) {
        if let Some(a) = self.viewer.take_ink() {
            self.run_page_action(a);
        }
        self.viewer.tool = tool;
    }

    fn print(&mut self) {
        let Some(allowed) = self.doc.as_ref().map(|d| d.allowed.print) else {
            return;
        };
        if !self.require(allowed, "printing") {
            return;
        }
        let Some(doc) = &self.doc else { return };
        let path = std::env::temp_dir().join(format!("penadf-print-{}.pdf", std::process::id()));
        let result = std::fs::write(&path, &doc.bytes)
            .map_err(anyhow::Error::from)
            .and_then(|_| print_file(&path));
        if self.report(result).is_some() {
            self.info("Sent to the default PDF printer handler.");
        }
    }

    // ---------- tools menu actions ----------

    fn merge_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .add_filter("PDF and convertible files", &all_exts())
            .set_title("Choose files to combine (in order)")
            .pick_files()
        else {
            return;
        };
        let Some(doc) = &self.doc else {
            self.create_from(files);
            return;
        };
        let at = doc.page_count();
        let result = self.combine_files(&files);
        if let Some(extra) = self.report(result) {
            self.change(
                &format!("Added {} file(s) at the end", files.len()),
                |b, pw| pages::insert_document(b, pw, &extra, None, at),
            );
        }
    }

    fn insert_file(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .add_filter("PDF and convertible files", &all_exts())
            .pick_file()
        else {
            return;
        };
        let at = self.viewer.current_page + 1;
        let result = create::from_file(&file);
        if let Some(extra) = self.report(result) {
            self.change(&format!("Inserted after page {at}"), |b, pw| {
                pages::insert_document(b, pw, &extra, None, at)
            });
        }
    }

    fn compare_with(&mut self) {
        let Some(doc) = &self.doc else { return };
        let Some(file) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_title("Choose the other version")
            .pick_file()
        else {
            return;
        };
        let old_name = doc.name.clone();
        let result = std::fs::read(&file)
            .map_err(anyhow::Error::from)
            .and_then(|other| compare::compare(&doc.bytes, doc.pw(), &other, None));
        if let Some(report) = self.report(result) {
            self.compare = Some(dialogs::CompareWindow {
                old_name,
                new_name: file_name(&file),
                report,
                only_changes: true,
            });
        }
    }

    fn export_text(&mut self) {
        let Some(allowed) = self.doc.as_ref().map(|d| d.allowed.copy) else {
            return;
        };
        if !self.require(allowed, "copying text") {
            return;
        }
        let Some(doc) = &self.doc else { return };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name(format!("{}.txt", stem(Path::new(&doc.name))))
            .save_file()
        else {
            return;
        };
        let result =
            export::to_text(&doc.bytes, doc.pw()).and_then(|t| Ok(std::fs::write(&path, t)?));
        if self.report(result).is_some() {
            self.info(format!("Saved {}", path.display()));
        }
    }

    fn export_html(&mut self) {
        let Some(allowed) = self.doc.as_ref().map(|d| d.allowed.copy) else {
            return;
        };
        if !self.require(allowed, "copying content") {
            return;
        }
        let Some(doc) = &self.doc else { return };
        let name = stem(Path::new(&doc.name));
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Web page", &["html"])
            .set_file_name(format!("{name}.html"))
            .save_file()
        else {
            return;
        };
        let result = export::to_html(&doc.bytes, doc.pw(), &name)
            .and_then(|t| Ok(std::fs::write(&path, t)?));
        if self.report(result).is_some() {
            self.info(format!("Saved {}", path.display()));
        }
    }

    fn save_parts(&mut self, parts: Vec<Vec<u8>>, base: &str) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Choose a folder for the new files")
            .pick_folder()
        else {
            return;
        };
        let mut written = 0;
        for (i, part) in parts.iter().enumerate() {
            let path = dir.join(format!("{base}-part{}.pdf", i + 1));
            if let Err(e) = std::fs::write(&path, part) {
                self.error(format!("Could not write {}: {e}", path.display()));
                return;
            }
            written += 1;
        }
        self.info(format!("Saved {written} file(s) to {}", dir.display()));
    }

    // ---------- UI ----------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut as K, Modifiers as M};
        let has_dialog = !matches!(self.dialog, Dialog::None);
        let pressed = |k: K| ctx.input_mut(|i| i.consume_shortcut(&k));
        if pressed(K::new(M::COMMAND, Key::O)) {
            self.open_dialog();
        }
        if self.doc.is_none() || has_dialog {
            return;
        }
        if pressed(K::new(M::COMMAND, Key::W)) {
            self.close_tab();
            return;
        }
        if pressed(K::new(M::COMMAND | M::SHIFT, Key::Tab)) {
            self.cycle_tab(false);
        }
        if pressed(K::new(M::COMMAND, Key::Tab)) {
            self.cycle_tab(true);
        }
        if pressed(K::new(M::COMMAND | M::SHIFT, Key::S)) {
            self.save(true);
        }
        if pressed(K::new(M::COMMAND, Key::S)) {
            self.save(false);
        }
        if pressed(K::new(M::COMMAND, Key::Z)) {
            self.undo();
        }
        if pressed(K::new(M::COMMAND, Key::Y)) || pressed(K::new(M::COMMAND | M::SHIFT, Key::Z)) {
            self.redo();
        }
        if pressed(K::new(M::COMMAND, Key::F)) {
            self.search_open = true;
        }
        if pressed(K::new(M::COMMAND, Key::P)) {
            self.print();
        }
        if pressed(K::new(M::COMMAND, Key::Plus)) || pressed(K::new(M::COMMAND, Key::Equals)) {
            self.viewer.set_zoom(self.viewer.zoom * 1.2);
        }
        if pressed(K::new(M::COMMAND, Key::Minus)) {
            self.viewer.set_zoom(self.viewer.zoom / 1.2);
        }
        if pressed(K::new(M::COMMAND, Key::Num0)) {
            self.viewer.fit(Fit::Width);
        }
        let n = self.doc.as_ref().map_or(0, Document::page_count);
        if pressed(K::new(M::NONE, Key::PageDown)) {
            self.viewer.goto = Some((self.viewer.current_page + 1).min(n.saturating_sub(1)));
        }
        if pressed(K::new(M::NONE, Key::PageUp)) {
            self.viewer.goto = Some(self.viewer.current_page.saturating_sub(1));
        }
        if pressed(K::new(M::NONE, Key::Escape)) {
            self.set_tool(Tool::Select);
            self.search_open = false;
        }
        if self.viewer.has_ink()
            && pressed(K::new(M::NONE, Key::Enter))
            && let Some(a) = self.viewer.take_ink()
        {
            self.run_page_action(a);
        }
    }

    fn undo(&mut self) {
        if let Some(d) = &mut self.doc {
            let r = d.undo();
            self.comments_cache = None;
            self.report(r);
        }
    }

    fn redo(&mut self) {
        if let Some(d) = &mut self.doc {
            let r = d.redo();
            self.comments_cache = None;
            self.report(r);
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…  (Ctrl+O)").clicked() {
                        self.open_dialog();
                    }
                    if ui
                        .button("Create PDF from files…")
                        .on_hover_text("Images, text, Word, Excel or PowerPoint files")
                        .clicked()
                        && let Some(files) = rfd::FileDialog::new()
                            .add_filter("Convertible files", &all_exts())
                            .pick_files()
                    {
                        self.create_from(files);
                    }
                    ui.separator();
                    let has = self.doc.is_some();
                    if ui
                        .add_enabled(has, egui::Button::new("Save  (Ctrl+S)"))
                        .clicked()
                    {
                        self.save(false);
                    }
                    if ui.add_enabled(has, egui::Button::new("Save as…")).clicked() {
                        self.save(true);
                    }
                    if ui
                        .add_enabled(has, egui::Button::new("Print…  (Ctrl+P)"))
                        .clicked()
                    {
                        self.print();
                    }
                    ui.separator();
                    ui.menu_button("Export", |ui| {
                        if ui
                            .add_enabled(has, egui::Button::new("Pages as images (PNG/JPEG)…"))
                            .clicked()
                        {
                            self.dialog = Dialog::ExportImages(dialogs::ExportImagesDialog {
                                pages: "all".into(),
                                dpi: 150.0,
                                png: true,
                            });
                        }
                        if ui
                            .add_enabled(has, egui::Button::new("Text (.txt)…"))
                            .clicked()
                        {
                            self.export_text();
                        }
                        if ui
                            .add_enabled(has, egui::Button::new("Web page (.html)…"))
                            .clicked()
                        {
                            self.export_html();
                        }
                    });
                    ui.separator();
                    if ui
                        .add_enabled(has, egui::Button::new("Close tab  (Ctrl+W)"))
                        .clicked()
                    {
                        self.close_tab();
                    }
                    if ui.button("Exit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let (u, r) = self
                        .doc
                        .as_ref()
                        .map_or((false, false), |d| (d.can_undo(), d.can_redo()));
                    if ui
                        .add_enabled(u, egui::Button::new("Undo  (Ctrl+Z)"))
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(r, egui::Button::new("Redo  (Ctrl+Y)"))
                        .clicked()
                    {
                        self.redo();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.doc.is_some(), egui::Button::new("Find…  (Ctrl+F)"))
                        .clicked()
                    {
                        self.search_open = true;
                    }
                    if ui
                        .add_enabled(self.doc.is_some(), egui::Button::new("Copy all text"))
                        .clicked()
                    {
                        let allowed = self.doc.as_ref().is_some_and(|d| d.allowed.copy);
                        if self.require(allowed, "copying text")
                            && let Some(d) = &self.doc
                        {
                            let r = ops::page_texts(&d.bytes, d.pw()).map(|t| t.join("\n\n"));
                            if let Some(t) = self.report(r) {
                                ui.ctx().copy_text(t);
                                self.info("Text copied to the clipboard");
                            }
                        }
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Fit width  (Ctrl+0)").clicked() {
                        self.viewer.fit(Fit::Width);
                    }
                    if ui.button("Fit page").clicked() {
                        self.viewer.fit(Fit::Page);
                    }
                    if ui.button("Actual size").clicked() {
                        self.viewer.set_zoom(1.0);
                    }
                    ui.separator();
                    ui.radio_value(&mut self.side, SidePanel::None, "No side panel");
                    ui.radio_value(&mut self.side, SidePanel::Organize, "Organize pages");
                    ui.radio_value(&mut self.side, SidePanel::Comments, "Comments");
                });
                ui.menu_button("Tools", |ui| self.tools_menu(ui));
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        self.show_keys = true;
                    }
                    if ui.button("About PenaDF").clicked() {
                        self.show_about = true;
                    }
                });
            });
        });
    }

    fn tools_menu(&mut self, ui: &mut egui::Ui) {
        let has = self.doc.is_some();
        if cfg!(windows) && ui.button("Make PenaDF the default PDF app…").clicked() {
            self.open_default_apps();
        }
        ui.separator();
        ui.label(
            RichText::new("Create & combine")
                .color(theme::MUTED)
                .small(),
        );
        if ui.button("Combine files into one PDF…").clicked() {
            self.merge_files();
        }
        if ui
            .add_enabled(has, egui::Button::new("Insert file after this page…"))
            .clicked()
        {
            self.insert_file();
        }
        ui.separator();
        ui.label(RichText::new("Organize").color(theme::MUTED).small());
        if ui
            .add_enabled(has, egui::Button::new("Split into several files…"))
            .clicked()
        {
            self.dialog = Dialog::Split(dialogs::Prompt {
                id: "split",
                title: "Split PDF",
                label: "One file per group. Example: 1-3, 4-6, 7-".into(),
                value: String::new(),
                ok: "Split",
                secret: false,
            });
        }
        if ui
            .add_enabled(has, egui::Button::new("Extract pages to a new file…"))
            .clicked()
        {
            self.dialog = Dialog::Extract(dialogs::Prompt {
                id: "extract",
                title: "Extract pages",
                label: "Pages to copy into a new PDF. Example: 1, 3-5".into(),
                value: String::new(),
                ok: "Extract",
                secret: false,
            });
        }
        if ui
            .add_enabled(has, egui::Button::new("Crop pages…"))
            .clicked()
        {
            self.dialog = Dialog::Crop(dialogs::CropDialog {
                pages: "all".into(),
                margins_mm: [10.0; 4],
            });
        }
        if ui
            .add_enabled(has, egui::Button::new("Organize pages panel"))
            .clicked()
        {
            self.side = SidePanel::Organize;
        }
        ui.separator();
        ui.label(
            RichText::new("Forms & signatures")
                .color(theme::MUTED)
                .small(),
        );
        if ui
            .add_enabled(has, egui::Button::new("Fill out form…"))
            .clicked()
            && let Some(d) = &self.doc
        {
            let r = forms::fields(&d.bytes, d.pw());
            if let Some(fields) = self.report(r) {
                self.dialog = Dialog::Form(dialogs::FormDialog {
                    fields,
                    flatten: false,
                });
            }
        }
        if ui.button("My signature…").clicked() {
            self.dialog =
                Dialog::Signature(dialogs::SignatureDialog::new(self.signature.clone(), None));
        }
        if ui
            .add_enabled(has, egui::Button::new("Flatten (lock comments and fields)"))
            .clicked()
        {
            self.change(
                "Flattened: comments and fields are now part of the page",
                forms::flatten,
            );
        }
        ui.separator();
        ui.label(
            RichText::new("Review & protect")
                .color(theme::MUTED)
                .small(),
        );
        if ui
            .add_enabled(has, egui::Button::new("Compare with another version…"))
            .clicked()
        {
            self.compare_with();
        }
        let encrypted = self.doc.as_ref().is_some_and(|d| d.info.encrypted);
        if ui
            .add_enabled(
                has && !encrypted,
                egui::Button::new("Protect with password…"),
            )
            .clicked()
        {
            self.dialog = Dialog::Protect(dialogs::ProtectDialog::default());
        }
        if ui
            .add_enabled(encrypted, egui::Button::new("Remove protection…"))
            .clicked()
        {
            self.dialog = Dialog::Unprotect(dialogs::Prompt {
                id: "unprotect",
                title: "Remove protection",
                label: "Permissions password".into(),
                value: String::new(),
                ok: "Remove",
                secret: true,
            });
        }
    }

    fn tool_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("tools")
            .frame(
                egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let has = self.doc.is_some();
                    if ui.button("Open").on_hover_text("Ctrl+O").clicked() {
                        self.open_dialog();
                    }
                    let dirty = self.doc.as_ref().is_some_and(|d| d.dirty);
                    if ui
                        .add_enabled(dirty, egui::Button::new("Save"))
                        .on_hover_text("Ctrl+S")
                        .clicked()
                    {
                        self.save(false);
                    }
                    if ui.add_enabled(has, egui::Button::new("Print")).clicked() {
                        self.print();
                    }
                    ui.separator();
                    ui.add_enabled_ui(has, |ui| {
                        for tool in [
                            Tool::Select,
                            Tool::Highlight,
                            Tool::Underline,
                            Tool::StrikeOut,
                            Tool::Note,
                            Tool::Draw,
                            Tool::AddText,
                            Tool::AddImage,
                            Tool::Sign,
                        ] {
                            let selected = self.viewer.tool == tool;
                            let text = if selected {
                                RichText::new(tool.label()).color(theme::ACCENT).strong()
                            } else {
                                RichText::new(tool.label())
                            };
                            if ui
                                .selectable_label(selected, text)
                                .on_hover_text(tool.hint())
                                .clicked()
                            {
                                self.set_tool(tool);
                            }
                        }
                        ui.separator();
                        if ui
                            .button("Rotate left")
                            .on_hover_text("Rotate this page left")
                            .clicked()
                        {
                            let p = self.viewer.current_page;
                            self.rotate(vec![p], -1);
                        }
                        if ui
                            .button("Rotate right")
                            .on_hover_text("Rotate this page right")
                            .clicked()
                        {
                            let p = self.viewer.current_page;
                            self.rotate(vec![p], 1);
                        }
                        ui.separator();
                        if ui.button("−").on_hover_text("Zoom out").clicked() {
                            self.viewer.set_zoom(self.viewer.zoom / 1.2);
                        }
                        ui.label(format!("{:.0}%", self.viewer.zoom * 100.0));
                        if ui.button("+").on_hover_text("Zoom in").clicked() {
                            self.viewer.set_zoom(self.viewer.zoom * 1.2);
                        }
                        if ui.button("Fit").on_hover_text("Fit width").clicked() {
                            self.viewer.fit(Fit::Width);
                        }
                        ui.separator();
                        if ui.selectable_label(self.search_open, "Find").clicked() {
                            self.search_open = !self.search_open;
                        }
                    });
                });
            });
    }

    fn rotate(&mut self, pages_to_turn: Vec<usize>, quarters: i32) {
        let Some(doc) = &self.doc else { return };
        if !self.require(doc.allowed.edit, "page changes") {
            return;
        }
        self.change("Rotated", |b, pw| {
            pages::rotate(b, pw, &pages_to_turn, quarters)
        });
    }

    fn search_bar(&mut self, ui: &mut egui::Ui) {
        if !self.search_open || self.doc.is_none() {
            return;
        }
        egui::Panel::top("search").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find");
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .desired_width(260.0)
                        .hint_text("text in this PDF"),
                );
                if self.hits.is_empty() && self.search.is_empty() {
                    edit.request_focus();
                }
                let go = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if edit.changed() {
                    self.hits.clear();
                    self.active_hit = None;
                }
                if go || ui.button("Next").clicked() {
                    if self.hits.is_empty() {
                        if let Some(d) = &self.doc {
                            let r = ops::search(&d.bytes, d.pw(), &self.search);
                            self.hits = self.report(r).unwrap_or_default();
                        }
                        self.active_hit = (!self.hits.is_empty()).then_some(0);
                    } else {
                        self.active_hit = self.active_hit.map(|i| (i + 1) % self.hits.len());
                    }
                    if let Some(h) = self.active_hit.and_then(|i| self.hits.get(i)) {
                        self.viewer.goto = Some(h.page);
                    }
                    edit.request_focus();
                }
                if ui
                    .add_enabled(!self.hits.is_empty(), egui::Button::new("Previous"))
                    .clicked()
                {
                    let n = self.hits.len();
                    self.active_hit = self.active_hit.map(|i| (i + n - 1) % n);
                    if let Some(h) = self.active_hit.and_then(|i| self.hits.get(i)) {
                        self.viewer.goto = Some(h.page);
                    }
                }
                match (
                    self.active_hit,
                    self.hits.len(),
                    self.search.trim().is_empty(),
                ) {
                    (_, _, true) => {}
                    (Some(i), n, _) => {
                        ui.label(format!("{} of {n}", i + 1));
                    }
                    (None, _, _) => {
                        ui.label(RichText::new("Press Enter to search").color(theme::MUTED));
                    }
                }
                if ui.button("×").clicked() {
                    self.search_open = false;
                    self.hits.clear();
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(d) = &self.doc {
                    let n = d.page_count();
                    ui.label("Page");
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.page_field)
                            .desired_width(36.0)
                            .hint_text(format!("{}", self.viewer.current_page + 1)),
                    );
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Ok(p) = self.page_field.trim().parse::<usize>()
                            && (1..=n).contains(&p)
                        {
                            self.viewer.goto = Some(p - 1);
                        }
                        self.page_field.clear();
                    }
                    ui.label(format!("of {n}"));
                    ui.separator();
                    let mut flags = Vec::new();
                    if d.info.encrypted {
                        flags.push("Protected");
                    }
                    if d.info.has_form {
                        flags.push("Form");
                    }
                    if d.dirty {
                        flags.push("Unsaved changes");
                    }
                    ui.label(RichText::new(flags.join("  ·  ")).color(theme::MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(self.viewer.tool.hint()).color(theme::MUTED));
                    });
                } else {
                    ui.label(RichText::new("No file open").color(theme::MUTED));
                }
            });
        });
    }

    fn thumbnails(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &mut self.doc else { return };
        let mut action: Option<ThumbAction> = None;
        let organize = self.side == SidePanel::Organize;
        let current = self.viewer.current_page;
        egui::Panel::left("thumbs")
            .resizable(true)
            .default_size(150.0)
            .size_range(110.0..=260.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let width = ui.available_width() - 16.0;
                        ui.add_space(6.0); // room for the selected page's outline
                        for i in 0..doc.page_count() {
                            let (w, h) = doc.geoms[i].display_size();
                            let size = Vec2::new(width, width * h / w);
                            ui.vertical_centered(|ui| {
                                let (rect, resp) =
                                    ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                                let p = ui.painter();
                                p.rect_filled(rect, 0.0, Color32::WHITE);
                                if let Some(tex) = doc.thumb_texture(ui.ctx(), i) {
                                    p.image(
                                        tex.id(),
                                        rect,
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        Color32::WHITE,
                                    );
                                }
                                let stroke = if i == current {
                                    egui::Stroke::new(2.5, theme::ACCENT)
                                } else {
                                    egui::Stroke::new(1.0, Color32::from_gray(200))
                                };
                                p.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
                                if resp.clicked() {
                                    action = Some(ThumbAction::Goto(i));
                                }
                                // Drag a thumbnail onto another to move the page there.
                                if organize {
                                    resp.dnd_set_drag_payload(i);
                                    if let Some(from) = resp.dnd_release_payload::<usize>() {
                                        action = Some(ThumbAction::Move(*from, i));
                                    }
                                    if resp.dnd_hover_payload::<usize>().is_some() {
                                        p.rect_stroke(
                                            rect,
                                            0.0,
                                            egui::Stroke::new(3.0, Color32::from_rgb(29, 78, 216)),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("Rotate right").clicked() {
                                        action = Some(ThumbAction::Rotate(i, 1));
                                    }
                                    if ui.button("Rotate left").clicked() {
                                        action = Some(ThumbAction::Rotate(i, -1));
                                    }
                                    if ui.button("Insert blank page after").clicked() {
                                        action = Some(ThumbAction::Blank(i + 1));
                                    }
                                    if ui.button("Extract this page…").clicked() {
                                        action = Some(ThumbAction::Extract(i));
                                    }
                                    if ui.button("Delete page").clicked() {
                                        action = Some(ThumbAction::Delete(i));
                                    }
                                });
                                ui.label(
                                    RichText::new(format!("{}", i + 1))
                                        .small()
                                        .color(theme::MUTED),
                                );
                            });
                            ui.add_space(4.0);
                        }
                    });
            });
        if let Some(a) = action {
            self.thumb_action(a);
        }
    }

    fn thumb_action(&mut self, a: ThumbAction) {
        let allowed = self.doc.as_ref().is_some_and(|d| d.allowed.edit);
        if !matches!(a, ThumbAction::Goto(_) | ThumbAction::Extract(_))
            && !self.require(allowed, "page changes")
        {
            return;
        }
        match a {
            ThumbAction::Goto(i) => self.viewer.goto = Some(i),
            ThumbAction::Move(from, to) if from != to => self.change(
                &format!("Moved page {} to position {}", from + 1, to + 1),
                |b, pw| pages::move_page(b, pw, from, to),
            ),
            ThumbAction::Move(..) => {}
            ThumbAction::Rotate(i, q) => self.rotate(vec![i], q),
            ThumbAction::Blank(at) => self.change("Blank page inserted", |b, pw| {
                pages::insert_blank(b, pw, at)
            }),
            ThumbAction::Delete(i) => self.change(&format!("Page {} deleted", i + 1), |b, pw| {
                pages::delete(b, pw, &[i])
            }),
            ThumbAction::Extract(i) => self.extract_to_file(vec![i]),
        }
    }

    fn extract_to_file(&mut self, list: Vec<usize>) {
        let Some(d) = &self.doc else { return };
        let r = pages::extract(&d.bytes, d.pw(), &list);
        let name = format!("{}-pages.pdf", stem(Path::new(&d.name)));
        if let Some(bytes) = self.report(r)
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(name)
                .save_file()
        {
            let r = std::fs::write(with_ext(path.clone(), "pdf"), bytes);
            if self.report(r.map_err(anyhow::Error::from)).is_some() {
                self.info(format!("Saved {}", path.display()));
            }
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        match self.side {
            SidePanel::None => {}
            SidePanel::Organize => {
                egui::Panel::right("organize").default_size(240.0).show(ui, |ui| {
                    ui.heading("Organize pages");
                    ui.label(RichText::new("Drag thumbnails on the left onto another page to reorder. Right-click a thumbnail for more.").color(theme::MUTED));
                    ui.separator();
                    let p = self.viewer.current_page;
                    ui.label(format!("This page: {}", p + 1));
                    if ui.button("Rotate right").clicked() {
                        self.rotate(vec![p], 1);
                    }
                    if ui.button("Rotate all pages right").clicked() {
                        let n = self.doc.as_ref().map_or(0, Document::page_count);
                        self.rotate((0..n).collect(), 1);
                    }
                    if ui.button("Delete this page").clicked() {
                        self.thumb_action(ThumbAction::Delete(p));
                    }
                    if ui.button("Insert blank page after").clicked() {
                        self.thumb_action(ThumbAction::Blank(p + 1));
                    }
                    if ui.button("Insert file after this page…").clicked() {
                        self.insert_file();
                    }
                    ui.separator();
                    if ui.button("Split into several files…").clicked() {
                        self.dialog = Dialog::Split(dialogs::Prompt { id: "split", title: "Split PDF", label: "One file per group. Example: 1-3, 4-6, 7-".into(), value: String::new(), ok: "Split", secret: false });
                    }
                    if ui.button("Extract pages…").clicked() {
                        self.dialog = Dialog::Extract(dialogs::Prompt { id: "extract", title: "Extract pages", label: "Pages to copy into a new PDF. Example: 1, 3-5".into(), value: String::new(), ok: "Extract", secret: false });
                    }
                    if ui.button("Crop pages…").clicked() {
                        self.dialog = Dialog::Crop(dialogs::CropDialog { pages: "all".into(), margins_mm: [10.0; 4] });
                    }
                });
            }
            SidePanel::Comments => {
                if self.comments_cache.is_none()
                    && let Some(d) = &self.doc
                {
                    self.comments_cache =
                        Some(markup::comments(&d.bytes, d.pw()).unwrap_or_default());
                }
                let mut delete = None;
                let mut goto = None;
                egui::Panel::right("comments")
                    .default_size(260.0)
                    .show(ui, |ui| {
                        ui.heading("Comments");
                        let list = self.comments_cache.clone().unwrap_or_default();
                        if list.is_empty() {
                            ui.label(
                                RichText::new(
                                    "No comments yet. Use Highlight, Sticky note or Draw.",
                                )
                                .color(theme::MUTED),
                            );
                        }
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for c in &list {
                                egui::Frame::group(ui.style()).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&c.kind).strong());
                                        ui.label(
                                            RichText::new(format!("p.{}", c.page + 1))
                                                .color(theme::MUTED),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("Delete")
                                                    .on_hover_text("Delete")
                                                    .clicked()
                                                {
                                                    delete = Some((c.page, c.index));
                                                }
                                                if ui.small_button("Go").clicked() {
                                                    goto = Some(c.page);
                                                }
                                            },
                                        );
                                    });
                                    if !c.text.is_empty() {
                                        ui.label(&c.text);
                                    }
                                    if !c.author.is_empty() {
                                        ui.label(
                                            RichText::new(&c.author).small().color(theme::MUTED),
                                        );
                                    }
                                });
                            }
                        });
                    });
                if let Some(p) = goto {
                    self.viewer.goto = Some(p);
                }
                if let Some((page, index)) = delete {
                    self.change("Comment deleted", |b, pw| {
                        markup::delete_comment(b, pw, page, index)
                    });
                }
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let dialog = std::mem::replace(&mut self.dialog, Dialog::None);
        self.dialog = match dialog {
            Dialog::None => Dialog::None,
            Dialog::Password {
                path,
                name,
                bytes,
                mut ui,
            } => match ui.show(ctx) {
                Outcome::Open => Dialog::Password {
                    path,
                    name,
                    bytes,
                    ui,
                },
                Outcome::Cancel => Dialog::None,
                Outcome::Done(pw) => {
                    self.load(path, name, bytes, Some(pw));
                    std::mem::replace(&mut self.dialog, Dialog::None)
                }
            },
            Dialog::Text { tool, mut ui } => match ui.show(ctx) {
                Outcome::Open => Dialog::Text { tool, ui },
                Outcome::Cancel => Dialog::None,
                Outcome::Done(()) => {
                    let (page, at, text, size) = (ui.page, ui.at, ui.text.clone(), ui.size);
                    if tool == Tool::Note {
                        self.change("Note added", |b, pw| {
                            markup::add_note(b, pw, page, at, &text)
                        });
                    } else {
                        // Click marks the top-left of the text; PDF text is placed by baseline.
                        let at = ops::PagePoint {
                            x: at.x,
                            y: at.y - size,
                        };
                        self.change("Text added", |b, pw| {
                            markup::add_text(b, pw, page, at, &text, size, markup::Rgba::INK)
                        });
                    }
                    Dialog::None
                }
            },
            Dialog::Signature(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Signature(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(()) => {
                    let sig = ui.signature.clone();
                    if ui.remember
                        && let Some(path) = signature_file()
                    {
                        let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
                        let _ = std::fs::write(path, sig.to_text());
                    }
                    self.signature = Some(sig.clone());
                    match ui.target {
                        Some((page, rect)) => self.place_signature(page, rect, &sig),
                        None => {
                            self.info(
                                "Signature saved. Pick the Sign tool and drag a box to place it.",
                            );
                        }
                    }
                    Dialog::None
                }
            },
            Dialog::Protect(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Protect(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(()) => {
                    let (open_pw, owner_pw, allowed) = (
                        ui.open_password().to_string(),
                        ui.owner_password.clone(),
                        ui.allowed,
                    );
                    if let Some(d) = &mut self.doc {
                        let r = protect::protect(&d.bytes, d.pw(), &open_pw, &owner_pw, allowed);
                        match r {
                            Ok(bytes) => {
                                // From now on, open it with the owner password so editing keeps working.
                                d.password = Some(owner_pw);
                                let r = d.apply(bytes);
                                if self.report(r).is_some() {
                                    self.info(
                                        "Protected with AES-256. Save to keep the protection.",
                                    );
                                }
                            }
                            Err(e) => self.error(format!("{e:#}")),
                        }
                    }
                    Dialog::None
                }
            },
            Dialog::Unprotect(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Unprotect(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(pw) => {
                    if let Some(d) = &mut self.doc {
                        match protect::unprotect(&d.bytes, &pw) {
                            Ok(bytes) => {
                                d.password = None;
                                let r = d.apply(bytes);
                                if self.report(r).is_some() {
                                    self.info("Protection removed. Save to keep the change.");
                                }
                            }
                            Err(e) => self.error(format!("{e:#}")),
                        }
                    }
                    Dialog::None
                }
            },
            Dialog::Split(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Split(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(spec) => {
                    if let Some(d) = &self.doc {
                        let base = stem(Path::new(&d.name));
                        let r = page_range::parse_groups(&spec, d.page_count())
                            .and_then(|g| pages::split(&d.bytes, d.pw(), &g));
                        if let Some(parts) = self.report(r) {
                            self.save_parts(parts, &base);
                        }
                    }
                    Dialog::None
                }
            },
            Dialog::Extract(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Extract(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(spec) => {
                    let r = self.selected_pages(&spec);
                    if let Some(list) = self.report(r) {
                        self.extract_to_file(list);
                    }
                    Dialog::None
                }
            },
            Dialog::Crop(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Crop(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(()) => {
                    let r = self.selected_pages(&ui.pages);
                    if let Some(list) = self.report(r) {
                        let m = ui.margins_pt();
                        self.change("Cropped", |b, pw| pages::crop(b, pw, &list, m));
                    }
                    Dialog::None
                }
            },
            Dialog::ExportImages(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::ExportImages(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(()) => {
                    let r = self.selected_pages(&ui.pages);
                    if let (Some(list), Some(d)) = (self.report(r), &self.doc)
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        let fmt = if ui.png {
                            export::ImageFormat::Png
                        } else {
                            export::ImageFormat::Jpeg
                        };
                        let base = stem(Path::new(&d.name));
                        let r =
                            export::to_images(&d.bytes, d.pw(), &list, ui.dpi, fmt, &dir, &base);
                        if let Some(files) = self.report(r) {
                            self.info(format!(
                                "Saved {} image(s) to {}",
                                files.len(),
                                dir.display()
                            ));
                        }
                    }
                    Dialog::None
                }
            },
            Dialog::Form(mut ui) => match ui.show(ctx) {
                Outcome::Open => Dialog::Form(ui),
                Outcome::Cancel => Dialog::None,
                Outcome::Done(()) => {
                    let (fields, flatten) = (ui.fields.clone(), ui.flatten);
                    self.change(
                        if flatten {
                            "Form filled and locked"
                        } else {
                            "Form filled"
                        },
                        |b, pw| {
                            let filled = forms::fill(b, pw, &fields)?;
                            if flatten {
                                forms::flatten(&filled, pw)
                            } else {
                                Ok(filled)
                            }
                        },
                    );
                    Dialog::None
                }
            },
        };
    }

    fn toast(&mut self, ctx: &egui::Context) {
        let Some((msg, is_error, started)) = &mut self.toast else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if *started == 0.0 {
            *started = now;
        }
        let ttl = if *is_error { 8.0 } else { 3.5 };
        if now - *started > ttl {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let (bg, fg) = if *is_error {
            (theme::ACCENT, Color32::WHITE)
        } else {
            (Color32::from_rgb(32, 36, 44), Color32::WHITE)
        };
        let msg = msg.clone();
        let mut dismiss = false;
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -40.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(bg)
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(560.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(msg).color(fg));
                            if ui
                                .add(egui::Button::new(RichText::new("×").color(fg)).frame(false))
                                .clicked()
                            {
                                dismiss = true;
                            }
                        });
                    });
            });
        if dismiss {
            self.toast = None;
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            let any_pdf = dropped
                .iter()
                .any(|p| create::kind_of(p) == Some(create::SourceKind::Pdf));
            // PDFs open in tabs; images or Office files dropped on an open PDF are added to it.
            if self.doc.is_some()
                && !any_pdf
                && dropped.iter().all(|p| create::kind_of(p).is_some())
            {
                // With a document open, dropped files are added at the end.
                let result = self.combine_files(&dropped);
                if let Some(extra) = self.report(result) {
                    let at = self.doc.as_ref().map_or(0, Document::page_count);
                    self.change(
                        &format!("Added {} file(s) at the end", dropped.len()),
                        |b, pw| pages::insert_document(b, pw, &extra, None, at),
                    );
                }
            } else {
                self.open_paths(dropped);
            }
        }
    }

    // ---------- tabs ----------

    fn take_active(&mut self) -> TabState {
        TabState {
            doc: self.doc.take(),
            viewer: std::mem::take(&mut self.viewer),
            search: std::mem::take(&mut self.search),
            hits: std::mem::take(&mut self.hits),
            active_hit: self.active_hit.take(),
            page_field: std::mem::take(&mut self.page_field),
            comments_cache: self.comments_cache.take(),
        }
    }

    fn put_active(&mut self, t: TabState) {
        let tool = self.viewer.tool; // the chosen tool follows you between tabs
        self.doc = t.doc;
        self.viewer = t.viewer;
        self.viewer.tool = tool;
        self.search = t.search;
        self.hits = t.hits;
        self.active_hit = t.active_hit;
        self.page_field = t.page_field;
        self.comments_cache = t.comments_cache;
    }

    /// Shows `doc` in a new tab right after the current one.
    fn open_tab(&mut self, doc: Document) {
        if let Some(a) = self.viewer.take_ink() {
            self.run_page_action(a);
        }
        let new = TabState {
            doc: Some(doc),
            ..TabState::default()
        };
        let mut cur = self.take_active();
        self.tabs.open(&mut cur, new);
        self.put_active(cur);
    }

    fn activate_tab(&mut self, index: usize) {
        if index == self.tabs.active() {
            return;
        }
        if let Some(a) = self.viewer.take_ink() {
            self.run_page_action(a);
        }
        let mut cur = self.take_active();
        self.tabs.activate(&mut cur, index);
        self.put_active(cur);
    }

    fn cycle_tab(&mut self, forward: bool) {
        let n = self.tabs.len();
        if n > 1 {
            let a = self.tabs.active();
            self.activate_tab(if forward {
                (a + 1) % n
            } else {
                (a + n - 1) % n
            });
        }
    }

    /// Closes the active tab, offering to save it first if it has unsaved changes.
    fn close_tab(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        let mut cur = self.take_active();
        self.tabs.close_active(&mut cur);
        self.put_active(cur);
    }

    /// Closes tab `index`. Closing a background tab leaves you on the tab you were reading
    /// (it is shown only briefly if it needs a "save changes?" answer).
    fn close_tab_at(&mut self, index: usize) {
        let was = self.tabs.active();
        if index == was {
            self.close_tab();
            return;
        }
        self.activate_tab(index);
        let before = self.tabs.len();
        self.close_tab();
        if self.tabs.len() < before {
            // Indices after the closed tab moved left by one.
            self.activate_tab(if was > index { was - 1 } else { was });
        }
    }

    fn tab_doc(&mut self, index: usize) -> Option<&Document> {
        if index == self.tabs.active() {
            self.doc.as_ref()
        } else {
            self.tabs.parked_mut(index)?.doc.as_ref()
        }
    }

    /// If `path` is already open in a tab, switches to it.
    fn switch_to_open_file(&mut self, path: &Path) -> bool {
        let want = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        for i in 0..self.tabs.len() {
            let open_path = self.tab_doc(i).and_then(|d| d.path.clone());
            if open_path.is_some_and(|p| std::fs::canonicalize(&p).unwrap_or(p) == want) {
                self.activate_tab(i);
                return true;
            }
        }
        false
    }

    /// Before quitting: offers to save every tab with unsaved changes.
    fn confirm_discard_all(&mut self) -> bool {
        for i in 0..self.tabs.len() {
            if self.tab_doc(i).is_some_and(|d| d.dirty) {
                self.activate_tab(i);
                if !self.confirm_discard() {
                    return false;
                }
            }
        }
        true
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            return;
        }
        let active = self.tabs.active();
        let labels: Vec<TabLabel> = (0..self.tabs.len())
            .map(|i| match self.tab_doc(i) {
                Some(d) => TabLabel {
                    title: d.name.clone(),
                    tooltip: d
                        .path
                        .as_ref()
                        .map_or_else(|| "Not saved yet".to_string(), |p| p.display().to_string()),
                    dirty: d.dirty,
                },
                None => TabLabel {
                    title: "New tab".into(),
                    tooltip: String::new(),
                    dirty: false,
                },
            })
            .collect();
        let frame = egui::Frame::new()
            .fill(theme::CANVAS)
            .inner_margin(egui::Margin {
                left: 6,
                right: 6,
                top: 5,
                bottom: 0,
            });
        let action = egui::Panel::top("tab-bar")
            .frame(frame)
            .show(ui, |ui| tabs::tab_bar(ui, &labels, active))
            .inner;
        match action {
            Some(TabBarAction::Close(i)) => self.close_tab_at(i),
            Some(TabBarAction::Activate(i)) => self.activate_tab(i),
            Some(TabBarAction::OpenNew) => self.open_dialog(),
            None => {}
        }
    }

    /// Files from a second launch (for example double-clicking a PDF in Explorer).
    fn receive_from_other_launches(&mut self, ctx: &egui::Context) {
        let mut batches = Vec::new();
        if let Some(rx) = &self.incoming {
            while let Ok(paths) = rx.try_recv() {
                batches.push(paths);
            }
        }
        for paths in batches {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.open_paths(paths);
        }
    }

    fn open_default_apps(&mut self) {
        if !winshell::is_registered() {
            self.error(
                "Windows only lists PenaDF as a PDF app after it is installed with the installer.",
            );
        } else if !winshell::open_default_apps_settings() {
            self.error("Could not open Settings. Go to Settings → Apps → Default apps → PenaDF.");
        } else {
            self.info("In Settings, click the box next to .pdf and choose PenaDF.");
        }
    }

    /// A slim bar inviting the user to make PenaDF the default PDF app. Shown on Windows
    /// when PenaDF is installed but not chosen; "Don't ask again" hides it for good.
    fn default_app_bar(&mut self, ui: &mut egui::Ui) {
        if self.default_hint_dismissed || !cfg!(windows) {
            return;
        }
        // Re-read the registry at most every 2 s (the user may change it in Settings).
        let now = ui.input(|i| i.time);
        let key = egui::Id::unique("default-app-check");
        let cached: Option<(f64, bool)> = ui.ctx().data(|d| d.get_temp(key));
        let show = match cached {
            Some((t, v)) if now - t < 2.0 => v,
            _ => {
                let v = winshell::is_registered() && !winshell::is_default_pdf_app();
                ui.ctx().data_mut(|d| d.insert_temp(key, (now, v)));
                v
            }
        };
        if !show {
            return;
        }
        let mut make_default = false;
        let mut dismiss = false;
        egui::Panel::top("default-app")
            .frame(
                egui::Frame::new()
                    .fill(theme::ACCENT_SOFT)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Open PDFs with PenaDF when you double-click them?");
                    make_default = ui.button("Make default…").clicked();
                    dismiss = ui
                        .add(
                            egui::Button::new(RichText::new("Don't ask again").color(theme::MUTED))
                                .frame(false),
                        )
                        .clicked();
                });
            });
        if make_default {
            self.open_default_apps();
        }
        if dismiss {
            self.default_hint_dismissed = true;
            set_settings_flag(DEFAULT_HINT_FLAG);
        }
    }

    /// Dev only: `PENADF_SHOW` opens a view for README screenshots.
    /// Values: comments, organize, search:<text>, sign, protect, compare:<file>, tab:<n>, tool:<name>.
    #[cfg(feature = "screenshot")]
    fn screenshot_setup(&mut self) {
        for item in std::env::var("PENADF_SHOW").unwrap_or_default().split(',') {
            let (key, val) = item.split_once(':').unwrap_or((item, ""));
            match key {
                "comments" => self.side = SidePanel::Comments,
                "organize" => self.side = SidePanel::Organize,
                "protect" => self.dialog = Dialog::Protect(dialogs::ProtectDialog::default()),
                "sign" => {
                    self.dialog = Dialog::Signature(dialogs::SignatureDialog::new(
                        self.signature.clone(),
                        None,
                    ))
                }
                "tab" => {
                    if let Ok(i) = val.parse::<usize>() {
                        self.activate_tab(i);
                    }
                }
                "tool" => {
                    let tool = [
                        Tool::Highlight,
                        Tool::Note,
                        Tool::Draw,
                        Tool::Sign,
                        Tool::AddText,
                    ]
                    .into_iter()
                    .find(|t| t.label().eq_ignore_ascii_case(val));
                    if let Some(t) = tool {
                        self.viewer.tool = t;
                    }
                }
                "search" => {
                    self.search_open = true;
                    self.search = val.to_string();
                    if let Some(d) = &self.doc {
                        self.hits = ops::search(&d.bytes, d.pw(), val).unwrap_or_default();
                        self.active_hit = (!self.hits.is_empty()).then_some(0);
                    }
                }
                "compare" => {
                    if let Some(d) = &self.doc
                        && let Ok(report) = std::fs::read(val)
                            .map_err(anyhow::Error::from)
                            .and_then(|o| compare::compare(&d.bytes, d.pw(), &o, None))
                    {
                        self.compare = Some(dialogs::CompareWindow {
                            old_name: d.name.clone(),
                            new_name: file_name(Path::new(val)),
                            report,
                            only_changes: true,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> String {
        match &self.doc {
            Some(d) => format!("{}{} - PenaDF", if d.dirty { "• " } else { "" }, d.name),
            None => "PenaDF".to_string(),
        }
    }
}

enum ThumbAction {
    Goto(usize),
    Move(usize, usize),
    Rotate(usize, i32),
    Blank(usize),
    Delete(usize),
    Extract(usize),
}

impl eframe::App for PenaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

        // Ask before closing with unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.confirm_discard_all() {
                self.allow_close = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        self.receive_from_other_launches(&ctx);
        self.handle_drops(&ctx);
        self.shortcuts(&ctx);
        self.top_bar(ui);
        self.tab_bar(ui);
        self.tool_bar(ui);
        self.default_app_bar(ui);
        self.search_bar(ui);
        self.status_bar(ui);
        if self.doc.is_some() {
            self.thumbnails(ui);
            self.side_panel(ui);
        }

        let mut page_action = None;
        let mut open_clicked = false;
        egui::CentralPanel::no_frame().show(ui, |ui| match &mut self.doc {
            Some(doc) => page_action = self.viewer.show(ui, doc, &self.hits, self.active_hit),
            None => {
                egui::Frame::new().fill(theme::CANVAS).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    open_clicked = dialogs::empty_state(ui);
                });
            }
        });
        if open_clicked {
            self.open_dialog();
        }
        if let Some(a) = page_action {
            self.run_page_action(a);
        }

        if let Some(w) = &mut self.compare
            && !w.show(&ctx)
        {
            self.compare = None;
        }
        self.dialogs(&ctx);
        dialogs::about(&ctx, &mut self.show_about);
        dialogs::shortcuts(&ctx, &mut self.show_keys);
        self.toast(&ctx);

        // Dragging files over the window.
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("drop"),
            ));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(120));
            let text = if self.doc.is_some() {
                "Drop PDFs to open them in tabs. Images and Office files are added to this PDF."
            } else {
                "Drop to open (images and Office files are combined into one PDF)"
            };
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(22.0),
                Color32::WHITE,
            );
        }
    }
}

// ---------- helpers ----------

fn all_exts() -> Vec<&'static str> {
    let mut v = vec!["pdf"];
    v.extend_from_slice(create::IMAGE_EXTS);
    v.extend_from_slice(create::TEXT_EXTS);
    v.extend_from_slice(create::OFFICE_EXTS);
    v
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document.pdf".into())
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into())
}

fn with_ext(p: PathBuf, ext: &str) -> PathBuf {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
        p
    } else {
        p.with_extension(ext)
    }
}

/// Per-user settings folder: %APPDATA%\PenaDF on Windows.
fn settings_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("PenaDF"))
}

/// Where the saved signature lives.
fn signature_file() -> Option<PathBuf> {
    Some(settings_dir()?.join("signature.txt"))
}

const DEFAULT_HINT_FLAG: &str = "default-hint-dismissed";

/// On/off settings are empty marker files in the settings folder.
fn settings_flag(name: &str) -> bool {
    settings_dir().is_some_and(|d| d.join(name).exists())
}

fn set_settings_flag(name: &str) {
    if let Some(d) = settings_dir() {
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::write(d.join(name), b"");
    }
}

/// Hands the file to Windows' "print" verb for PDFs (the default PDF app prints it).
#[cfg(windows)]
fn print_file(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let script = format!(
        "Start-Process -FilePath '{}' -Verb Print",
        path.display().to_string().replace('\'', "''")
    );
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "Windows has no app registered to print PDFs. Save the file and print it from another PDF reader."
        )
    }
}

#[cfg(not(windows))]
fn print_file(path: &Path) -> Result<()> {
    let status = std::process::Command::new("lp").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("printing failed")
    }
}
