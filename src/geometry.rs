//! Maps between what the user sees on screen and PDF page coordinates,
//! honouring the page's crop box and /Rotate.

use crate::ops::{PagePoint, PageRect};
use anyhow::Result;
use pdfium_render::prelude::*;

/// The visible box of one page and how it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeom {
    pub crop: PageRect,
    /// Clockwise quarter turns, 0..=3.
    pub quarters: u8,
}

impl PageGeom {
    /// Width and height as displayed, in points.
    pub fn display_size(&self) -> (f32, f32) {
        let (w, h) = (
            self.crop.right - self.crop.left,
            self.crop.top - self.crop.bottom,
        );
        if self.quarters % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// From display fractions (u right, v down, both 0..1) to page coordinates.
    pub fn to_page(&self, u: f32, v: f32) -> PagePoint {
        let PageRect {
            left: l,
            bottom: b,
            right: r,
            top: t,
        } = self.crop;
        let (w, h) = (r - l, t - b);
        match self.quarters % 4 {
            0 => PagePoint {
                x: l + u * w,
                y: t - v * h,
            },
            1 => PagePoint {
                x: l + v * w,
                y: b + u * h,
            },
            2 => PagePoint {
                x: r - u * w,
                y: b + v * h,
            },
            _ => PagePoint {
                x: r - v * w,
                y: t - u * h,
            },
        }
    }

    /// From page coordinates to display fractions (inverse of [`to_page`](Self::to_page)).
    pub fn to_display(&self, p: PagePoint) -> (f32, f32) {
        let PageRect {
            left: l,
            bottom: b,
            right: r,
            top: t,
        } = self.crop;
        let (w, h) = ((r - l).max(1e-6), (t - b).max(1e-6));
        match self.quarters % 4 {
            0 => ((p.x - l) / w, (t - p.y) / h),
            1 => ((p.y - b) / h, (p.x - l) / w),
            2 => ((r - p.x) / w, (p.y - b) / h),
            _ => ((t - p.y) / h, (r - p.x) / w),
        }
    }

    /// A page rectangle as display fractions (min u, min v, max u, max v).
    pub fn rect_to_display(&self, rect: PageRect) -> (f32, f32, f32, f32) {
        let (u1, v1) = self.to_display(PagePoint {
            x: rect.left,
            y: rect.bottom,
        });
        let (u2, v2) = self.to_display(PagePoint {
            x: rect.right,
            y: rect.top,
        });
        (u1.min(u2), v1.min(v2), u1.max(u2), v1.max(v2))
    }
}

/// Geometry of every page of an open document.
pub fn page_geoms(doc: &PdfDocument) -> Result<Vec<PageGeom>> {
    doc.pages()
        .iter()
        .map(|page| {
            let crop = page
                .boundaries()
                .crop()
                .map(|b| b.bounds)
                .or_else(|_| page.boundaries().media().map(|b| b.bounds));
            let crop = crop.map(PageRect::from_pdf).unwrap_or(PageRect {
                left: 0.0,
                bottom: 0.0,
                right: page.width().value,
                top: page.height().value,
            });
            let quarters = match page.rotation()? {
                PdfPageRenderRotation::None => 0,
                PdfPageRenderRotation::Degrees90 => 1,
                PdfPageRenderRotation::Degrees180 => 2,
                PdfPageRenderRotation::Degrees270 => 3,
            };
            Ok(PageGeom { crop, quarters })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_in_every_rotation() {
        let crop = PageRect {
            left: 10.0,
            bottom: 20.0,
            right: 210.0,
            top: 320.0,
        };
        for quarters in 0..4 {
            let g = PageGeom { crop, quarters };
            for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.25, 0.75), (1.0, 1.0)] {
                let p = g.to_page(u, v);
                let (u2, v2) = g.to_display(p);
                assert!(
                    (u - u2).abs() < 1e-5 && (v - v2).abs() < 1e-5,
                    "q{quarters} ({u},{v}) -> {p:?} -> ({u2},{v2})"
                );
            }
        }
        assert_eq!(
            PageGeom { crop, quarters: 1 }.display_size(),
            (300.0, 200.0)
        );
    }
}
