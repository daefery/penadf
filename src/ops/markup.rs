//! Comment and mark up: highlight, underline, strike out, sticky notes, freehand ink.
//! Edit: add text and images to the page. Sign: place a drawn or image signature.
//!
//! Markup tools create standard PDF annotations, so other readers (Acrobat, Edge, Chrome)
//! show and can edit them. "Add text", "Add image" and "Sign" write into the page content,
//! so they print and cannot be moved by accident later.

use super::{PagePoint, PageRect};
use crate::engine;
use anyhow::{Result, bail};
use pdfium_render::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMarkup {
    Highlight,
    Underline,
    StrikeOut,
}

/// RGBA colour, 0-255 per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const YELLOW: Rgba = Rgba(255, 221, 0, 255);
    pub const RED: Rgba = Rgba(220, 38, 38, 255);
    pub const BLUE: Rgba = Rgba(29, 78, 216, 255);
    pub const INK: Rgba = Rgba(16, 24, 64, 255);

    fn pdf(self) -> PdfColor {
        PdfColor::new(self.0, self.1, self.2, self.3)
    }
}

/// Finds the text lines under a dragged rectangle and returns one rectangle per line,
/// hugging the characters. Empty if no text is there.
pub fn text_lines_in(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    area: PageRect,
) -> Result<Vec<PageRect>> {
    let doc = engine::open(bytes.to_vec(), pw)?;
    let page = doc.pages().get(page as i32)?;
    let text = page.text()?;
    let mut boxes: Vec<PageRect> = Vec::new();
    for ch in text.chars().iter() {
        if ch.unicode_char().is_some_and(char::is_whitespace) {
            continue;
        }
        let Ok(b) = ch.loose_bounds() else { continue };
        let r = PageRect::from_pdf(b);
        let (cx, cy) = ((r.left + r.right) / 2.0, (r.bottom + r.top) / 2.0);
        if cx >= area.left && cx <= area.right && cy >= area.bottom && cy <= area.top {
            boxes.push(r);
        }
    }
    Ok(group_into_lines(boxes))
}

/// Merges character boxes that share a line (vertical centres within half a line height).
pub fn group_into_lines(mut boxes: Vec<PageRect>) -> Vec<PageRect> {
    boxes.sort_by(|a, b| b.top.total_cmp(&a.top).then(a.left.total_cmp(&b.left)));
    let mut lines: Vec<PageRect> = Vec::new();
    for b in boxes {
        let cy = (b.bottom + b.top) / 2.0;
        let same_line = lines.iter_mut().find(|l| {
            let h = (l.top - l.bottom).max(b.top - b.bottom);
            (cy - (l.bottom + l.top) / 2.0).abs() <= h / 2.0
        });
        match same_line {
            Some(l) => {
                l.left = l.left.min(b.left);
                l.right = l.right.max(b.right);
                l.bottom = l.bottom.min(b.bottom);
                l.top = l.top.max(b.top);
            }
            None => lines.push(b),
        }
    }
    lines
}

/// Adds a highlight / underline / strike-out annotation covering `lines` (one rect per line).
pub fn add_text_markup(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    kind: TextMarkup,
    lines: &[PageRect],
    color: Rgba,
) -> Result<Vec<u8>> {
    if lines.is_empty() {
        bail!("no text under the selection");
    }
    engine::edit(bytes, pw, |doc| {
        let mut page = doc.pages().get(page as i32)?;
        let annots = page.annotations_mut();
        let bounds = lines.iter().fold(lines[0], |acc, r| PageRect {
            left: acc.left.min(r.left),
            bottom: acc.bottom.min(r.bottom),
            right: acc.right.max(r.right),
            top: acc.top.max(r.top),
        });
        macro_rules! fill {
            ($a:expr) => {{
                let mut a = $a;
                for r in lines {
                    a.attachment_points_mut()
                        .create_attachment_point_at_end(quad(*r))?;
                }
                a.set_bounds(bounds.to_pdf())?;
                a.set_stroke_color(color.pdf())?;
                a.set_creator(&author())?;
            }};
        }
        match kind {
            TextMarkup::Highlight => fill!(annots.create_highlight_annotation()?),
            TextMarkup::Underline => fill!(annots.create_underline_annotation()?),
            TextMarkup::StrikeOut => fill!(annots.create_strikeout_annotation()?),
        }
        Ok(())
    })
}

/// Adds a sticky note whose icon sits at `at`.
pub fn add_note(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    at: PagePoint,
    text: &str,
) -> Result<Vec<u8>> {
    if text.trim().is_empty() {
        bail!("the note is empty");
    }
    engine::edit(bytes, pw, |doc| {
        let mut page = doc.pages().get(page as i32)?;
        let mut note = page.annotations_mut().create_text_annotation(text)?;
        note.set_bounds(PdfRect::new_from_values(
            at.y - 20.0,
            at.x,
            at.y,
            at.x + 20.0,
        ))?;
        note.set_stroke_color(Rgba::YELLOW.pdf())?;
        note.set_creator(&author())?;
        Ok(())
    })
}

/// Adds a freehand ink annotation made of `strokes` (each a polyline in page points).
pub fn add_ink(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    strokes: &[Vec<PagePoint>],
    color: Rgba,
    width: f32,
) -> Result<Vec<u8>> {
    let strokes: Vec<&Vec<PagePoint>> = strokes.iter().filter(|s| !s.is_empty()).collect();
    if strokes.is_empty() {
        bail!("nothing drawn");
    }
    engine::edit(bytes, pw, |doc| {
        let mut paths = Vec::new();
        for s in &strokes {
            paths.push(polyline(doc, s, color, width)?);
        }
        let mut page = doc.pages().get(page as i32)?;
        let mut ink = page.annotations_mut().create_ink_annotation()?;
        let all = strokes.iter().flat_map(|s| s.iter());
        let pad = width;
        let (mut l, mut b, mut r, mut t) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in all {
            l = l.min(p.x);
            b = b.min(p.y);
            r = r.max(p.x);
            t = t.max(p.y);
        }
        ink.set_bounds(PdfRect::new_from_values(b - pad, l - pad, t + pad, r + pad))?;
        ink.set_stroke_color(color.pdf())?;
        ink.set_creator(&author())?;
        for path in paths {
            ink.objects_mut().add_path_object(path)?;
        }
        Ok(())
    })
}

/// Writes text into the page content at `at` (baseline-left), Helvetica.
pub fn add_text(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    at: PagePoint,
    text: &str,
    size: f32,
    color: Rgba,
) -> Result<Vec<u8>> {
    if text.trim().is_empty() {
        bail!("the text is empty");
    }
    engine::edit(bytes, pw, |doc| {
        let font = doc.fonts_mut().helvetica();
        let mut page = doc.pages().get(page as i32)?;
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let y = at.y - i as f32 * size * 1.2;
            let mut obj = page.objects_mut().create_text_object(
                PdfPoints::new(at.x),
                PdfPoints::new(y),
                line,
                font,
                PdfPoints::new(size),
            )?;
            obj.set_fill_color(color.pdf())?;
        }
        Ok(())
    })
}

/// Places an image (PNG/JPEG/...) into the page content, filling `rect` while keeping
/// its aspect ratio, centred.
pub fn add_image(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    rect: PageRect,
    image_bytes: &[u8],
) -> Result<Vec<u8>> {
    let img = image::load_from_memory(image_bytes)?;
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let (rw, rh) = (rect.right - rect.left, rect.top - rect.bottom);
    if rw < 1.0 || rh < 1.0 {
        bail!("the area is too small");
    }
    let scale = (rw / iw).min(rh / ih);
    let (w, h) = (iw * scale, ih * scale);
    let (x, y) = (rect.left + (rw - w) / 2.0, rect.bottom + (rh - h) / 2.0);
    engine::edit(bytes, pw, |doc| {
        let mut page = doc.pages().get(page as i32)?;
        page.objects_mut().create_image_object(
            PdfPoints::new(x),
            PdfPoints::new(y),
            &img,
            Some(PdfPoints::new(w)),
            Some(PdfPoints::new(h)),
        )?;
        Ok(())
    })
}

/// A signature drawn as strokes in its own unit box (0..1 in both axes, y up).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Signature {
    pub strokes: Vec<Vec<PagePoint>>,
}

impl Signature {
    pub fn is_empty(&self) -> bool {
        self.strokes.iter().all(|s| s.len() < 2)
    }

    /// Width / height of the inked area, for placing without distortion.
    pub fn aspect(&self) -> f32 {
        let (l, b, r, t) = self.extent();
        ((r - l) / (t - b).max(1e-3)).clamp(0.2, 10.0)
    }

    fn extent(&self) -> (f32, f32, f32, f32) {
        let (mut l, mut b, mut r, mut t) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in self.strokes.iter().flatten() {
            l = l.min(p.x);
            b = b.min(p.y);
            r = r.max(p.x);
            t = t.max(p.y);
        }
        if l > r {
            (0.0, 0.0, 1.0, 1.0)
        } else {
            (l, b, r, t)
        }
    }

    /// Maps strokes into `rect` on the page, keeping aspect ratio.
    pub fn fit(&self, rect: PageRect) -> Vec<Vec<PagePoint>> {
        let (l, b, r, t) = self.extent();
        let (sw, sh) = ((r - l).max(1e-3), (t - b).max(1e-3));
        let (rw, rh) = (rect.right - rect.left, rect.top - rect.bottom);
        let scale = (rw / sw).min(rh / sh);
        let (ox, oy) = (
            rect.left + (rw - sw * scale) / 2.0,
            rect.bottom + (rh - sh * scale) / 2.0,
        );
        self.strokes
            .iter()
            .map(|s| {
                s.iter()
                    .map(|p| PagePoint {
                        x: ox + (p.x - l) * scale,
                        y: oy + (p.y - b) * scale,
                    })
                    .collect()
            })
            .collect()
    }

    /// Serialises to a compact text form for saving between sessions.
    pub fn to_text(&self) -> String {
        self.strokes
            .iter()
            .map(|s| {
                s.iter()
                    .map(|p| format!("{:.4},{:.4}", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn from_text(text: &str) -> Self {
        let strokes = text
            .lines()
            .map(|line| {
                line.split_whitespace()
                    .filter_map(|pair| {
                        let (x, y) = pair.split_once(',')?;
                        Some(PagePoint {
                            x: x.parse().ok()?,
                            y: y.parse().ok()?,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|s: &Vec<PagePoint>| !s.is_empty())
            .collect();
        Self { strokes }
    }
}

/// Signs: writes the signature strokes into the page content inside `rect`.
pub fn sign(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    rect: PageRect,
    signature: &Signature,
) -> Result<Vec<u8>> {
    if signature.is_empty() {
        bail!("draw a signature first");
    }
    let strokes = signature.fit(rect);
    let width = ((rect.top - rect.bottom) / 40.0).clamp(0.8, 2.5);
    engine::edit(bytes, pw, |doc| {
        let mut paths = Vec::new();
        for s in strokes.iter().filter(|s| s.len() >= 2) {
            paths.push(polyline(doc, s, Rgba::INK, width)?);
        }
        let mut page = doc.pages().get(page as i32)?;
        for p in paths {
            page.objects_mut().add_path_object(p)?;
        }
        Ok(())
    })
}

/// Lists existing comments (notes, highlights, ink...) for the comments panel.
#[derive(Clone, Debug)]
pub struct Comment {
    pub page: usize,
    pub index: usize,
    pub kind: String,
    pub text: String,
    pub author: String,
    pub rect: PageRect,
}

const COMMENT_KINDS: &[PdfPageAnnotationType] = &[
    PdfPageAnnotationType::Text,
    PdfPageAnnotationType::FreeText,
    PdfPageAnnotationType::Highlight,
    PdfPageAnnotationType::Underline,
    PdfPageAnnotationType::Squiggly,
    PdfPageAnnotationType::Strikeout,
    PdfPageAnnotationType::Ink,
    PdfPageAnnotationType::Square,
    PdfPageAnnotationType::Circle,
    PdfPageAnnotationType::Line,
    PdfPageAnnotationType::Stamp,
];

pub fn comments(bytes: &[u8], pw: Option<&str>) -> Result<Vec<Comment>> {
    let doc = engine::open(bytes.to_vec(), pw)?;
    let mut out = Vec::new();
    for (page_index, page) in doc.pages().iter().enumerate() {
        for (index, a) in page.annotations().iter().enumerate() {
            let t = a.annotation_type();
            if !COMMENT_KINDS.contains(&t) {
                continue;
            }
            out.push(Comment {
                page: page_index,
                index,
                kind: format!("{t:?}"),
                text: a.contents().unwrap_or_default(),
                author: a.creator().unwrap_or_default(),
                rect: a.bounds().map(PageRect::from_pdf).unwrap_or(PageRect {
                    left: 0.0,
                    bottom: 0.0,
                    right: 0.0,
                    top: 0.0,
                }),
            });
        }
    }
    Ok(out)
}

/// Deletes the annotation at `index` on `page` (as listed by [`comments`]).
pub fn delete_comment(
    bytes: &[u8],
    pw: Option<&str>,
    page: usize,
    index: usize,
) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        let mut page = doc.pages().get(page as i32)?;
        let annotations = page.annotations_mut();
        let annotation = annotations.get(index)?;
        annotations.delete_annotation(annotation)?;
        Ok(())
    })
}

fn polyline(
    doc: &PdfDocument<'static>,
    pts: &[PagePoint],
    color: Rgba,
    width: f32,
) -> Result<PdfPagePathObject<'static>> {
    let first = pts[0];
    let mut path = PdfPagePathObject::new(
        doc,
        PdfPoints::new(first.x),
        PdfPoints::new(first.y),
        Some(color.pdf()),
        Some(PdfPoints::new(width)),
        None,
    )?;
    if pts.len() == 1 {
        // A dot: a tiny segment so round caps draw a circle.
        path.line_to(PdfPoints::new(first.x + 0.01), PdfPoints::new(first.y))?;
    }
    for p in &pts[1..] {
        path.line_to(PdfPoints::new(p.x), PdfPoints::new(p.y))?;
    }
    path.set_line_cap(PdfPageObjectLineCap::Round)?;
    path.set_line_join(PdfPageObjectLineJoin::Round)?;
    Ok(path)
}

/// QuadPoints in the order Acrobat and PDFium draw them: top-left, top-right, bottom-left,
/// bottom-right. (`PdfQuadPoints::from_rect` goes round the rectangle instead, which
/// renders as a thin bow-tie rather than a filled box.)
fn quad(r: PageRect) -> PdfQuadPoints {
    let p = PdfPoints::new;
    PdfQuadPoints::new(
        p(r.left),
        p(r.top),
        p(r.right),
        p(r.top),
        p(r.left),
        p(r.bottom),
        p(r.right),
        p(r.bottom),
    )
}

fn author() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "PenaDF".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(l: f32, b: f32, rt: f32, t: f32) -> PageRect {
        PageRect {
            left: l,
            bottom: b,
            right: rt,
            top: t,
        }
    }

    #[test]
    fn groups_chars_into_lines_top_to_bottom() {
        let lines = group_into_lines(vec![
            r(10., 90., 15., 100.),
            r(30., 50., 35., 60.),
            r(16., 91., 20., 100.),
        ]);
        assert_eq!(lines, vec![r(10., 90., 20., 100.), r(30., 50., 35., 60.)]);
    }

    #[test]
    fn signature_fits_inside_rect_keeping_aspect() {
        let sig = Signature {
            strokes: vec![vec![
                PagePoint { x: 0.0, y: 0.0 },
                PagePoint { x: 2.0, y: 1.0 },
            ]],
        };
        let placed = sig.fit(r(0., 0., 100., 100.));
        assert_eq!(placed[0][0], PagePoint { x: 0.0, y: 25.0 });
        assert_eq!(placed[0][1], PagePoint { x: 100.0, y: 75.0 });
        assert!((sig.aspect() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn signature_text_round_trip() {
        let sig = Signature {
            strokes: vec![
                vec![PagePoint { x: 0.25, y: 0.5 }, PagePoint { x: 1.0, y: 0.0 }],
                vec![PagePoint { x: 0.5, y: 0.5 }],
            ],
        };
        assert_eq!(Signature::from_text(&sig.to_text()), sig);
        assert!(Signature::from_text("").is_empty());
    }
}
