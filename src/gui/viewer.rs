//! Continuous page view with zoom, search highlights and the on-page tools.

use super::document::Document;
use super::theme;
use crate::geometry::PageGeom;
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
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Scroll to read. Ctrl + mouse wheel zooms.",
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
        }
    }

    fn drags_box(self) -> bool {
        matches!(
            self,
            Tool::Highlight | Tool::Underline | Tool::StrikeOut | Tool::AddImage | Tool::Sign
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
                if let Some(a) = self.handle_tool(&response, index, rect, geom, &painter) {
                    action = Some(a);
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
            Tool::Select => None,
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
