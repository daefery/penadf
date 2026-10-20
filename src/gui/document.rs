//! One open document: its bytes, undo history, page geometry and a render cache.

use crate::geometry::{self, PageGeom};
use crate::ops::{self, DocInfo};
use crate::{engine, ops::export};
use anyhow::Result;
use eframe::egui;
use pdfium_render::prelude::PdfDocument;
use std::collections::HashMap;
use std::path::PathBuf;

/// Undo keeps whole earlier versions; stop at this many steps or this much memory.
const MAX_UNDO_STEPS: usize = 30;
const MAX_UNDO_BYTES: usize = 400 * 1024 * 1024;

pub struct Document {
    pub path: Option<PathBuf>,
    pub name: String,
    pub bytes: Vec<u8>,
    pub password: Option<String>,
    pub info: DocInfo,
    pub geoms: Vec<PageGeom>,
    /// Allowed actions for the password used (all true when unprotected).
    pub allowed: ops::protect::Allowed,
    pub dirty: bool,
    /// Kept open so page rendering does not re-parse the file.
    live: Option<PdfDocument<'static>>,
    undo: Vec<Vec<u8>>,
    redo: Vec<Vec<u8>>,
    /// (page, zoom-bucket) -> texture. Cleared on every change.
    textures: HashMap<(usize, u32), egui::TextureHandle>,
    thumbs: HashMap<usize, egui::TextureHandle>,
    /// Character layout per page, read on first use for text selection.
    text_layouts: HashMap<usize, std::rc::Rc<crate::ops::text::TextLayout>>,
}

impl Document {
    pub fn open(
        path: Option<PathBuf>,
        name: String,
        bytes: Vec<u8>,
        password: Option<String>,
    ) -> Result<Self> {
        let mut doc = Self {
            path,
            name,
            bytes,
            password,
            info: DocInfo::default(),
            geoms: Vec::new(),
            allowed: ops::protect::Allowed::default(),
            dirty: false,
            live: None,
            undo: Vec::new(),
            redo: Vec::new(),
            textures: HashMap::new(),
            thumbs: HashMap::new(),
            text_layouts: HashMap::new(),
        };
        doc.refresh()?;
        Ok(doc)
    }

    pub fn pw(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn page_count(&self) -> usize {
        self.geoms.len()
    }

    fn refresh(&mut self) -> Result<()> {
        self.live = None;
        let doc = engine::open(self.bytes.clone(), self.pw())?;
        self.geoms = geometry::page_geoms(&doc)?;
        self.allowed = ops::protect::Allowed::from_bits(engine::permission_bits(&doc)?);
        self.live = Some(doc);
        self.info = ops::info(&self.bytes, self.pw())?;
        self.textures.clear();
        self.thumbs.clear();
        self.text_layouts.clear();
        Ok(())
    }

    /// Replaces the content with the result of an operation, keeping undo history.
    pub fn apply(&mut self, new_bytes: Vec<u8>) -> Result<()> {
        let old = std::mem::replace(&mut self.bytes, new_bytes);
        if let Err(e) = self.refresh() {
            self.bytes = old;
            self.refresh()?;
            return Err(e);
        }
        self.undo.push(old);
        while self.undo.len() > MAX_UNDO_STEPS
            || (self.undo.len() > 1
                && self.undo.iter().map(Vec::len).sum::<usize>() > MAX_UNDO_BYTES)
        {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Result<()> {
        if let Some(prev) = self.undo.pop() {
            let cur = std::mem::replace(&mut self.bytes, prev);
            self.redo.push(cur);
            self.dirty = true;
            self.refresh()?;
        }
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        if let Some(next) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.bytes, next);
            self.undo.push(cur);
            self.dirty = true;
            self.refresh()?;
        }
        Ok(())
    }

    /// Texture for a page at a given display scale (pixels per point).
    /// Rendering is rounded to 10% steps so zooming does not re-render every frame.
    pub fn page_texture(
        &mut self,
        ctx: &egui::Context,
        page: usize,
        pixels_per_point: f32,
    ) -> Option<egui::TextureHandle> {
        let bucket = ((pixels_per_point * 10.0).ceil() as u32).clamp(2, 60);
        if let Some(t) = self.textures.get(&(page, bucket)) {
            return Some(t.clone());
        }
        // Keep memory bounded: drop other zoom levels of far-away pages.
        if self.textures.len() > 24 {
            self.textures
                .retain(|(p, b), _| *b == bucket && p.abs_diff(page) < 6);
        }
        let tex = self.render(ctx, page, bucket as f32 / 10.0 * 72.0, "page")?;
        self.textures.insert((page, bucket), tex.clone());
        Some(tex)
    }

    /// The selectable text of `page` (cached until the document changes).
    pub fn text_layout(&mut self, page: usize) -> std::rc::Rc<crate::ops::text::TextLayout> {
        if let Some(l) = self.text_layouts.get(&page) {
            return l.clone();
        }
        let layout = self
            .live
            .as_ref()
            .and_then(|doc| crate::ops::text::layout(doc, page).ok())
            .unwrap_or_default();
        let layout = std::rc::Rc::new(layout);
        self.text_layouts.insert(page, layout.clone());
        layout
    }

    pub fn thumb_texture(
        &mut self,
        ctx: &egui::Context,
        page: usize,
    ) -> Option<egui::TextureHandle> {
        if let Some(t) = self.thumbs.get(&page) {
            return Some(t.clone());
        }
        let tex = self.render(ctx, page, 20.0, "thumb")?;
        self.thumbs.insert(page, tex.clone());
        Some(tex)
    }

    fn render(
        &self,
        ctx: &egui::Context,
        page: usize,
        dpi: f32,
        kind: &str,
    ) -> Option<egui::TextureHandle> {
        let doc = self.live.as_ref()?;
        let max_side = ctx.input(|i| i.max_texture_side) as f32;
        let (w, h) = self.geoms.get(page)?.display_size();
        let dpi = dpi.min(max_side / (w.max(h) / 72.0)).max(10.0);
        let img = export::render_page(doc, page, dpi).ok()?;
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        Some(ctx.load_texture(
            format!("{kind}-{page}-{dpi}"),
            color,
            egui::TextureOptions::LINEAR,
        ))
    }
}
