//! Continuous page view with zoom, search highlights and the on-page tools.

use super::document::Document;
use super::theme;
use crate::geometry::PageGeom;
use crate::ops::markup::Layer;
use crate::ops::text::Selection;
use crate::ops::{PagePoint, PageRect, SearchHit};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

const GAP: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Highlight,
    Underline,
    StrikeOut,
    Note,
    Draw,
    AddText,
    AddImage,
    Sign,
    SignImage,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Highlight => "Highlight",
            Tool::Underline => "Underline",
            Tool::StrikeOut => "Strike out",
            Tool::Note => "Sticky note",
            Tool::Draw => "Draw",
            Tool::AddText => "Add text",
            Tool::AddImage => "Add image",
            Tool::Sign => "Sign",
            Tool::SignImage => "Sign with image",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => {
                "Drag over text to select it, then Ctrl+C to copy. Ctrl + mouse wheel zooms."
            }
            Tool::Highlight | Tool::Underline | Tool::StrikeOut => {
                "Drag across the text you want to mark."
            }
            Tool::Note => "Click where the note should go.",
            Tool::Draw => {
                "Draw with the mouse. Press Enter or pick another tool to save the drawing."
            }
            Tool::AddText => "Click where the text should start.",
            Tool::AddImage => "Drag a box where the image should go.",
            Tool::Sign => "Drag a box where your signature should go.",
            Tool::SignImage => {
                "Drag a box for your signature image, then move, resize and layer it before applying."
            }
        }
    }

    fn drags_box(self) -> bool {
        matches!(
            self,
            Tool::Highlight
                | Tool::Underline
                | Tool::StrikeOut
                | Tool::AddImage
                | Tool::Sign
                | Tool::SignImage
        )
    }
}

/// Something the user did on a page that the app must turn into an operation.
#[derive(Clone, Debug)]
pub enum PageAction {
    Box {
        page: usize,
        tool: Tool,
        rect: PageRect,
    },
    Click {
        page: usize,
        tool: Tool,
        at: PagePoint,
    },
    InkDone {
        page: usize,
        strokes: Vec<Vec<PagePoint>>,
    },
    /// "Apply" pressed on an image being placed.
    PlaceImage {
        page: usize,
        rect: PageRect,
        layer: Layer,
        image: std::rc::Rc<Vec<u8>>,
    },
}

/// An image (a signature) shown on a page as a box the user can move, resize and layer
/// before it is written into the PDF.
pub struct Placement {
    pub page: usize,
    pub rect: PageRect,
    pub layer: Layer,
    /// Keep width/height at the image's proportions while resizing (hold Shift to stretch).
    pub aspect: f32,
    pub image: std::rc::Rc<Vec<u8>>,
    texture: egui::TextureHandle,
    /// Corner being dragged (0 bottom-left, 1 bottom-right, 2 top-right, 3 top-left in
    /// screen order) or None while moving the whole box.
    grab: Option<Option<usize>>,
    last_pointer: Option<PagePoint>,
}

impl Placement {
    /// Starts placing `image` (encoded PNG/JPEG...) inside `rect`, keeping its proportions.
    pub fn new(
        ctx: &egui::Context,
        page: usize,
        rect: PageRect,
        image: Vec<u8>,
        layer: Layer,
    ) -> anyhow::Result<Self> {
        let img = image::load_from_memory(&image)?.into_rgba8();
        let aspect = img.width() as f32 / img.height().max(1) as f32;
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        );
        let texture = ctx.load_texture("placement", color, egui::TextureOptions::LINEAR);
        Ok(Self {
            page,
            rect: crate::ops::markup::fit_keeping_aspect(rect, aspect),
            layer,
            aspect,
            image: std::rc::Rc::new(image),
            texture,
            grab: None,
            last_pointer: None,
        })
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.left += dx;
        self.rect.right += dx;
        self.rect.bottom += dy;
        self.rect.top += dy;
    }
}

enum PlacementOutcome {
    Apply,
    Cancel,
}

pub struct Viewer {
    pub zoom: f32,
    pub tool: Tool,
    /// Page scrolled into view on the next frame.
    pub goto: Option<usize>,
    pub current_page: usize,
    drag_start: Option<(usize, Pos2)>,
    ink_page: Option<usize>,
    ink: Vec<Vec<PagePoint>>,
    pending_fit: Option<Fit>,
    /// Selected text (Select tool). Cleared when the document changes.
    pub selection: Option<Selection>,
    /// Set while a text-selection drag is in progress.
    selecting: bool,
    /// "Copy" chosen in the right-click menu; the app reads it with `take_copy_request`.
    copy_requested: bool,
    /// An image being positioned on a page (Sign with image).
    pub placement: Option<Placement>,
}

#[derive(Clone, Copy)]
pub enum Fit {
    Width,
    Page,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            tool: Tool::Select,
            goto: None,
            current_page: 0,
            drag_start: None,
            ink_page: None,
            ink: Vec::new(),
            pending_fit: Some(Fit::Width),
            selection: None,
            selecting: false,
            copy_requested: false,
            placement: None,
        }
    }
}

impl Viewer {
    pub fn fit(&mut self, fit: Fit) {
        self.pending_fit = Some(fit);
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.1, 8.0);
    }

    pub fn has_ink(&self) -> bool {
        !self.ink.is_empty()
    }

    /// Ends an ink drawing, returning it if there is one.
    pub fn take_ink(&mut self) -> Option<PageAction> {
        let page = self.ink_page.take()?;
        let strokes: Vec<_> = std::mem::take(&mut self.ink)
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        (!strokes.is_empty()).then_some(PageAction::InkDone { page, strokes })
    }

    pub fn reset_for_new_doc(&mut self) {
        *self = Self {
            tool: self.tool,
            ..Self::default()
        };
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut Document,
        hits: &[SearchHit],
        active_hit: Option<usize>,
    ) -> Option<PageAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();

        // Ctrl + wheel zoom.
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if (zoom_delta - 1.0).abs() > 1e-3 && ui.rect_contains_pointer(ui.max_rect()) {
            self.set_zoom(self.zoom * zoom_delta);
        }

        if let Some(fit) = self.pending_fit.take() {
            let avail = ui.available_size() - Vec2::splat(2.0 * GAP);
            if let Some(g) = doc
                .geoms
                .get(self.current_page.min(doc.page_count().saturating_sub(1)))
            {
                let (w, h) = g.display_size();
                self.zoom = match fit {
                    Fit::Width => avail.x / w,
                    Fit::Page => (avail.x / w).min(avail.y / h),
                }
                .clamp(0.1, 8.0);
            }
        }

        let zoom = self.zoom;
        let ppp = ctx.pixels_per_point();
        let sizes: Vec<Vec2> = doc
            .geoms
            .iter()
            .map(|g| {
                let (w, h) = g.display_size();
                Vec2::new(w, h) * zoom
            })
            .collect();
        let max_w = sizes.iter().map(|s| s.x).fold(0.0, f32::max);
        let total_h: f32 = sizes.iter().map(|s| s.y + GAP).sum::<f32>() + GAP;

        let mut scroll = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .id_salt("pages");
        if let Some(page) = self.goto.take() {
            let y: f32 = GAP + sizes.iter().take(page).map(|s| s.y + GAP).sum::<f32>();
            scroll = scroll.vertical_scroll_offset((y - GAP).max(0.0));
        }

        let tool = self.tool;
        let geoms = doc.geoms.clone();
        scroll.show_viewport(ui, |ui, viewport| {
            let width = (max_w + 2.0 * GAP).max(ui.available_width());
            let (canvas, _) = ui.allocate_exact_size(Vec2::new(width, total_h), Sense::hover());
            let painter = ui.painter_at(canvas);
            painter.rect_filled(canvas, 0.0, theme::CANVAS);

            let mut y = canvas.top() + GAP;
            let mut best_visible = (0.0f32, self.current_page);
            for (index, size) in sizes.iter().enumerate() {
                let rect =
                    Rect::from_min_size(Pos2::new(canvas.center().x - size.x / 2.0, y), *size);
                y += size.y + GAP;
                let visible = rect.translate(-canvas.min.to_vec2()).intersect(viewport);
                if visible.height() > best_visible.0 {
                    best_visible = (visible.height(), index);
                }
                if !viewport
                    .expand(400.0)
                    .intersects(rect.translate(-canvas.min.to_vec2()))
                {
                    continue;
                }
                painter.rect_filled(
                    rect.translate(Vec2::new(0.0, 2.0)),
                    2.0,
                    Color32::from_black_alpha(30),
                );
                painter.rect_filled(rect, 0.0, Color32::WHITE);
                if let Some(tex) = doc.page_texture(&ctx, index, zoom * ppp) {
                    painter.image(
                        tex.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                let geom = geoms[index];
                let to_screen = |r: PageRect| {
                    let (u1, v1, u2, v2) = geom.rect_to_display(r);
                    Rect::from_min_max(
                        rect.lerp_inside(Vec2::new(u1, v1)),
                        rect.lerp_inside(Vec2::new(u2, v2)),
                    )
                };
                for (hi, hit) in hits.iter().enumerate().filter(|(_, h)| h.page == index) {
                    let color = if Some(hi) == active_hit {
                        Color32::from_rgba_unmultiplied(255, 140, 0, 110)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 220, 0, 90)
                    };
                    for r in &hit.rects {
                        painter.rect_filled(to_screen(*r).expand(1.0), 1.0, color);
                    }
                }

                let response = ui.interact(
                    rect,
                    ui.scope_id().with(("page", index)),
                    Sense::click_and_drag(),
                );
                if tool != Tool::Select {
                    let cursor = if tool == Tool::Draw || tool.drags_box() {
                        egui::CursorIcon::Crosshair
                    } else {
                        egui::CursorIcon::PointingHand
                    };
                    response.clone().on_hover_cursor(cursor);
                }
                if tool == Tool::Select {
                    self.select_text(&response, index, rect, geom, doc);
                    // Right-click menu with the usual text actions.
                    let has_sel = self.selection.is_some();
                    response.context_menu(|ui| {
                        if ui.add_enabled(has_sel, egui::Button::new("Copy")).clicked() {
                            self.copy_requested = true;
                            ui.close();
                        }
                        if ui.button("Select all on this page").clicked() {
                            self.selection = doc.text_layout(index).select_all(index);
                            ui.close();
                        }
                    });
                    if let Some(sel) = self.selection.filter(|s| s.page == index) {
                        let layout = doc.text_layout(index);
                        for r in layout.highlight(&sel) {
                            painter.rect_filled(
                                to_screen(r).expand(0.5),
                                1.0,
                                Color32::from_rgba_unmultiplied(29, 78, 216, 70),
                            );
                        }
                    }
                } else if self.placement.is_some() {
                    // While placing an image, the page only moves and resizes that image.
                } else if let Some(a) = self.handle_tool(&response, index, rect, geom, &painter) {
                    action = Some(a);
                }
                if self.placement.as_ref().is_some_and(|p| p.page == index)
                    && let Some(a) = self.edit_placement(ui, index, rect, geom, &painter)
                {
                    action = a;
                }
                // Ink drawn so far on this page.
                if self.ink_page == Some(index) {
                    for stroke in &self.ink {
                        let pts: Vec<Pos2> = stroke
                            .iter()
                            .map(|p| {
                                let (u, v) = geom.to_display(*p);
                                rect.lerp_inside(Vec2::new(u, v))
                            })
                            .collect();
                        painter.add(egui::Shape::line(
                            pts,
                            Stroke::new(2.0 * zoom.max(0.5), theme::ACCENT),
                        ));
                    }
                }
            }
            self.current_page = best_visible.1;
        });
        action
    }

    /// Select tool: press on text and drag to select; double-click selects a word; a click
    /// on empty space clears the selection. Over text the cursor becomes a text cursor.
    fn select_text(
        &mut self,
        response: &egui::Response,
        page: usize,
        rect: Rect,
        geom: PageGeom,
        doc: &mut Document,
    ) {
        let to_page = |p: Pos2| {
            let uv = (p - rect.min) / rect.size();
            geom.to_page(uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0))
        };
        // Tolerance of a few screen pixels, in page points.
        let slack = 4.0 / self.zoom.max(0.1);
        let over_text =
            |doc: &mut Document, pos: Pos2| doc.text_layout(page).hit(to_page(pos), slack);

        if let Some(pos) = response.hover_pos()
            && over_text(doc, pos).is_some()
        {
            response.ctx.set_cursor_icon(egui::CursorIcon::Text);
        }
        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos()
                && let Some(i) = over_text(doc, pos)
            {
                self.selection = doc.text_layout(page).word_at(page, i);
            }
            return;
        }
        if response.drag_started() {
            // egui reports a drag after a few pixels of movement; start where the press was.
            let origin = response.ctx.input(|i| i.pointer.press_origin());
            let start = origin.or(response.interact_pointer_pos());
            match start.and_then(|p| over_text(doc, p)) {
                Some(i) => {
                    self.selection = Some(Selection {
                        page,
                        anchor: i,
                        focus: i,
                    });
                    self.selecting = true;
                }
                None => self.selecting = false,
            }
        }
        if self.selecting
            && response.dragged()
            && let (Some(pos), Some(sel)) =
                (response.interact_pointer_pos(), self.selection.as_mut())
            && sel.page == page
            && let Some(i) = doc.text_layout(page).nearest(to_page(pos))
        {
            sel.focus = i;
        }
        if response.drag_stopped() {
            self.selecting = false;
        }
        if response.clicked() {
            self.selection = None;
        }
    }

    /// Text currently selected, ready for the clipboard.
    pub fn selected_text(&self, doc: &mut Document) -> Option<String> {
        let sel = self.selection?;
        let text = doc.text_layout(sel.page).text(&sel);
        (!text.is_empty()).then_some(text)
    }

    /// Selects all text on the page being read.
    pub fn select_all(&mut self, doc: &mut Document) {
        self.selection = doc
            .text_layout(self.current_page)
            .select_all(self.current_page);
    }

    /// True once after "Copy" was chosen in the page's right-click menu.
    pub fn take_copy_request(&mut self) -> bool {
        std::mem::take(&mut self.copy_requested)
    }

    /// True while a text selection drag is under way (the page view should not scroll then).
    pub fn is_selecting(&self) -> bool {
        self.selecting
    }

    /// Draws the image being placed with its frame and corner handles, and lets the user
    /// move it (drag inside), resize it (drag a corner; Shift stretches), nudge it (arrow
    /// keys; Shift for bigger steps), pick its layer, and apply (Enter) or cancel (Esc).
    /// Returns `Some(action)` when the placement ends.
    fn edit_placement(
        &mut self,
        ui: &mut egui::Ui,
        page: usize,
        page_rect: Rect,
        geom: PageGeom,
        painter: &egui::Painter,
    ) -> Option<Option<PageAction>> {
        let to_page = |p: Pos2| {
            let uv = (p - page_rect.min) / page_rect.size();
            geom.to_page(uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0))
        };
        let to_screen = |r: PageRect| {
            let (u1, v1, u2, v2) = geom.rect_to_display(r);
            Rect::from_min_max(
                page_rect.lerp_inside(Vec2::new(u1, v1)),
                page_rect.lerp_inside(Vec2::new(u2, v2)),
            )
        };
        let p = self.placement.as_mut()?;
        let screen = to_screen(p.rect);

        // Corner handles first in id order but interacted last, so they win over the body.
        let body = ui.interact(
            screen,
            ui.scope_id().with(("placement", page)),
            Sense::drag(),
        );
        let corners = [
            screen.left_bottom(),
            screen.right_bottom(),
            screen.right_top(),
            screen.left_top(),
        ];
        let handle_size = Vec2::splat(10.0);
        let mut handle_hover = None;
        for (i, c) in corners.iter().enumerate() {
            let r = ui.interact(
                Rect::from_center_size(*c, handle_size + Vec2::splat(6.0)),
                ui.scope_id().with(("placement-corner", page, i)),
                Sense::drag(),
            );
            if r.hovered() || r.dragged() {
                handle_hover = Some(i);
                let icon = if i % 2 == 0 {
                    egui::CursorIcon::ResizeNeSw
                } else {
                    egui::CursorIcon::ResizeNwSe
                };
                ui.ctx().set_cursor_icon(icon);
            }
            if r.drag_started() {
                p.grab = Some(Some(i));
            }
            if r.dragged()
                && p.grab == Some(Some(i))
                && let Some(pos) = r.interact_pointer_pos()
            {
                let stretch = ui.input(|inp| inp.modifiers.shift);
                let opposite = to_page(corners[(i + 2) % 4]);
                let mut moved = to_page(pos);
                if !stretch {
                    // Keep proportions: grow the box from the fixed corner along the
                    // larger of the two pointer movements.
                    let (dx, dy) = (moved.x - opposite.x, moved.y - opposite.y);
                    let w = dx.abs().max(dy.abs() * p.aspect);
                    let h = w / p.aspect;
                    moved = PagePoint {
                        x: opposite.x + w.copysign(dx),
                        y: opposite.y + h.copysign(dy),
                    };
                }
                let r = PageRect::from_corners(opposite, moved);
                if r.right - r.left >= 4.0 && r.top - r.bottom >= 4.0 {
                    p.rect = r;
                }
            }
            if r.drag_stopped() {
                p.grab = None;
            }
        }
        if handle_hover.is_none() && (body.hovered() || body.dragged()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if body.drag_started() && p.grab.is_none() {
            p.grab = Some(None);
            p.last_pointer = body.interact_pointer_pos().map(to_page);
        }
        if body.dragged()
            && p.grab == Some(None)
            && let Some(pos) = body.interact_pointer_pos()
        {
            // Unclamped page point so the box can be dragged to the page edge smoothly.
            let uv = (pos - page_rect.min) / page_rect.size();
            let now = geom.to_page(uv.x, uv.y);
            if let Some(prev) = p.last_pointer {
                p.translate(now.x - prev.x, now.y - prev.y);
            }
            p.last_pointer = Some(now);
        }
        if body.drag_stopped() && p.grab == Some(None) {
            p.grab = None;
            p.last_pointer = None;
        }

        // Keyboard: arrows nudge by 1 pt (10 pt with Shift), on screen directions.
        let mut outcome = None;
        if !ui.ctx().egui_wants_keyboard_input() {
            ui.input_mut(|inp| {
                let step = if inp.modifiers.shift { 10.0 } else { 1.0 };
                for (key, du, dv) in [
                    (egui::Key::ArrowLeft, -1.0, 0.0),
                    (egui::Key::ArrowRight, 1.0, 0.0),
                    (egui::Key::ArrowUp, 0.0, -1.0),
                    (egui::Key::ArrowDown, 0.0, 1.0),
                ] {
                    if inp.consume_key(inp.modifiers, key) {
                        let a = geom.to_page(0.5, 0.5);
                        let (w, h) = geom.display_size();
                        let b = geom.to_page(0.5 + du * step / w, 0.5 + dv * step / h);
                        p.translate(b.x - a.x, b.y - a.y);
                    }
                }
                if inp.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    outcome = Some(PlacementOutcome::Apply);
                }
                if inp.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    outcome = Some(PlacementOutcome::Cancel);
                }
            });
        }

        // Keep the box on the page.
        let crop = geom.crop;
        let (w, h) = (p.rect.right - p.rect.left, p.rect.top - p.rect.bottom);
        let dx = (crop.left - p.rect.left).max(0.0) - (p.rect.right - crop.right).max(0.0);
        let dy = (crop.bottom - p.rect.bottom).max(0.0) - (p.rect.top - crop.top).max(0.0);
        if w <= crop.right - crop.left && h <= crop.top - crop.bottom {
            p.translate(dx, dy);
        }

        let screen = to_screen(p.rect);
        painter.image(
            p.texture.id(),
            screen,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.rect_stroke(
            screen,
            0.0,
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Outside,
        );
        for c in [
            screen.left_bottom(),
            screen.right_bottom(),
            screen.right_top(),
            screen.left_top(),
        ] {
            let r = Rect::from_center_size(c, handle_size);
            painter.rect_filled(r, 2.0, Color32::WHITE);
            painter.rect_stroke(r, 2.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
        }

        // Controls under the box (above it when there is no room below).
        let below = screen.left_bottom() + Vec2::new(0.0, 10.0);
        let pos = if below.y + 120.0 > ui.clip_rect().bottom() {
            screen.left_top() - Vec2::new(0.0, 130.0)
        } else {
            below
        };
        egui::Area::new(ui.scope_id().with("placement-controls"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Layer");
                        egui::ComboBox::from_id_salt("placement-layer")
                            .selected_text(p.layer.label())
                            .width(260.0)
                            .show_ui(ui, |ui| {
                                for l in Layer::ALL {
                                    ui.selectable_value(&mut p.layer, l, l.label());
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        let mut w = p.rect.right - p.rect.left;
                        let mut h = p.rect.top - p.rect.bottom;
                        ui.label("Width");
                        let cw = ui.add(
                            egui::DragValue::new(&mut w)
                                .range(4.0..=2000.0)
                                .suffix(" pt")
                                .speed(1.0),
                        );
                        ui.label("Height");
                        let ch = ui.add(
                            egui::DragValue::new(&mut h)
                                .range(4.0..=2000.0)
                                .suffix(" pt")
                                .speed(1.0),
                        );
                        if cw.changed() {
                            p.rect.right = p.rect.left + w;
                            let new_h = w / p.aspect;
                            p.rect.bottom = p.rect.top - new_h;
                        } else if ch.changed() {
                            p.rect.bottom = p.rect.top - h;
                            p.rect.right = p.rect.left + h * p.aspect;
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Drag to move, drag a corner to resize (Shift stretches), arrow keys nudge.",
                        )
                        .small()
                        .color(theme::MUTED),
                    );
                    ui.horizontal(|ui| {
                        let apply = ui.add(
                            egui::Button::new(
                                egui::RichText::new("Apply (Enter)").color(Color32::WHITE),
                            )
                            .fill(theme::ACCENT),
                        );
                        if apply.clicked() {
                            outcome = Some(PlacementOutcome::Apply);
                        }
                        if ui.button("Cancel (Esc)").clicked() {
                            outcome = Some(PlacementOutcome::Cancel);
                        }
                    });
                });
            });
        match outcome? {
            PlacementOutcome::Cancel => {
                self.placement = None;
                Some(None)
            }
            PlacementOutcome::Apply => {
                let p = self.placement.take()?;
                Some(Some(PageAction::PlaceImage {
                    page: p.page,
                    rect: p.rect,
                    layer: p.layer,
                    image: p.image,
                }))
            }
        }
    }

    fn handle_tool(
        &mut self,
        response: &egui::Response,
        page: usize,
        rect: Rect,
        geom: PageGeom,
        painter: &egui::Painter,
    ) -> Option<PageAction> {
        let to_page = |p: Pos2| {
            let uv = (p - rect.min) / rect.size();
            geom.to_page(uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0))
        };
        match self.tool {
            Tool::Select => None, // handled by select_text
            t if t.drags_box() => {
                if response.drag_started() {
                    // egui reports a drag only after the pointer has moved a few pixels;
                    // start the box where the button went down, not where it is now.
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    self.drag_start = origin
                        .or(response.interact_pointer_pos())
                        .map(|p| (page, p));
                }
                let start = self
                    .drag_start
                    .filter(|(p, _)| *p == page)
                    .map(|(_, pos)| pos);
                if let (Some(a), Some(b)) = (start, response.interact_pointer_pos()) {
                    let r = Rect::from_two_pos(a, b).intersect(rect);
                    let fill = if t == Tool::Highlight {
                        Color32::from_rgba_unmultiplied(255, 220, 0, 60)
                    } else {
                        Color32::from_rgba_unmultiplied(196, 30, 58, 25)
                    };
                    painter.rect(
                        r,
                        2.0,
                        fill,
                        Stroke::new(1.0, theme::ACCENT),
                        StrokeKind::Inside,
                    );
                    if response.drag_stopped() {
                        self.drag_start = None;
                        if r.width() > 3.0 && r.height() > 3.0 {
                            return Some(PageAction::Box {
                                page,
                                tool: t,
                                rect: PageRect::from_corners(to_page(r.min), to_page(r.max)),
                            });
                        }
                    }
                }
                None
            }
            Tool::Draw => {
                if self.ink_page.is_some_and(|p| p != page) && response.drag_started() {
                    // Started drawing on another page: save what is there first.
                    let done = self.take_ink();
                    self.ink_page = Some(page);
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    self.ink.push(origin.map(to_page).into_iter().collect());
                    return done;
                }
                if response.drag_started() {
                    self.ink_page = Some(page);
                    // Begin the stroke at the press point so its first few pixels are not lost.
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    self.ink.push(origin.map(to_page).into_iter().collect());
                }
                if response.dragged()
                    && self.ink_page == Some(page)
                    && let (Some(pos), Some(stroke)) =
                        (response.interact_pointer_pos(), self.ink.last_mut())
                {
                    let p = to_page(pos);
                    if stroke
                        .last()
                        .is_none_or(|l| (l.x - p.x).abs() + (l.y - p.y).abs() > 0.5)
                    {
                        stroke.push(p);
                    }
                }
                None
            }
            t => {
                if response.clicked() {
                    return response
                        .interact_pointer_pos()
                        .map(|pos| PageAction::Click {
                            page,
                            tool: t,
                            at: to_page(pos),
                        });
                }
                None
            }
        }
    }
}
