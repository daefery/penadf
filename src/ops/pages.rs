//! Organize pages: rotate, delete, reorder, extract, insert, merge, split, crop.

use crate::engine;
use anyhow::{Result, bail};
use pdfium_render::prelude::*;

fn rotation_from_quarters(quarters: i32) -> PdfPageRenderRotation {
    match quarters.rem_euclid(4) {
        0 => PdfPageRenderRotation::None,
        1 => PdfPageRenderRotation::Degrees90,
        2 => PdfPageRenderRotation::Degrees180,
        _ => PdfPageRenderRotation::Degrees270,
    }
}

fn quarters(rotation: PdfPageRenderRotation) -> i32 {
    match rotation {
        PdfPageRenderRotation::None => 0,
        PdfPageRenderRotation::Degrees90 => 1,
        PdfPageRenderRotation::Degrees180 => 2,
        PdfPageRenderRotation::Degrees270 => 3,
    }
}

/// Rotates each listed page clockwise by `quarter_turns` x 90 degrees (negative = counter-clockwise).
pub fn rotate(
    bytes: &[u8],
    pw: Option<&str>,
    pages: &[usize],
    quarter_turns: i32,
) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        for &index in pages {
            let mut page = doc.pages().get(index as i32)?;
            let current = quarters(page.rotation()?);
            page.set_rotation(rotation_from_quarters(current + quarter_turns));
        }
        Ok(())
    })
}

/// Deletes the listed pages. Refuses to delete every page.
pub fn delete(bytes: &[u8], pw: Option<&str>, pages: &[usize]) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        let count = doc.pages().len() as usize;
        let mut sorted: Vec<usize> = pages.iter().copied().filter(|&p| p < count).collect();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() >= count {
            bail!("a PDF must keep at least one page");
        }
        for &index in sorted.iter().rev() {
            doc.pages().get(index as i32)?.delete()?;
        }
        Ok(())
    })
}

/// Moves one page from `from` to position `to` (both zero-based, `to` in the final order).
pub fn move_page(bytes: &[u8], pw: Option<&str>, from: usize, to: usize) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        let count = doc.pages().len() as usize;
        if from >= count || to >= count {
            bail!("page out of range");
        }
        if from != to {
            engine::move_pages(doc, &[from as i32], to as i32)?;
        }
        Ok(())
    })
}

/// A new document holding only the listed pages, in the order given.
pub fn extract(bytes: &[u8], pw: Option<&str>, pages: &[usize]) -> Result<Vec<u8>> {
    let pdfium = engine::pdfium()?;
    let source = engine::open(bytes.to_vec(), pw)?;
    let mut out = pdfium.create_new_pdf()?;
    for (dest, &index) in pages.iter().enumerate() {
        out.pages_mut()
            .copy_page_from_document(&source, index as i32, dest as i32)?;
    }
    Ok(out.save_to_bytes()?)
}

/// Joins several PDFs into one, in the order given.
pub fn merge(files: &[(Vec<u8>, Option<String>)]) -> Result<Vec<u8>> {
    if files.is_empty() {
        bail!("nothing to merge");
    }
    let pdfium = engine::pdfium()?;
    let mut out = pdfium.create_new_pdf()?;
    for (bytes, pw) in files {
        let source = engine::open(bytes.clone(), pw.as_deref())?;
        out.pages_mut().append(&source)?;
    }
    Ok(out.save_to_bytes()?)
}

/// One output file per page group.
pub fn split(bytes: &[u8], pw: Option<&str>, groups: &[Vec<usize>]) -> Result<Vec<Vec<u8>>> {
    groups.iter().map(|g| extract(bytes, pw, g)).collect()
}

/// Inserts all pages of `other` before page `at` (zero-based; `at == page_count` appends).
pub fn insert_document(
    bytes: &[u8],
    pw: Option<&str>,
    other: &[u8],
    other_pw: Option<&str>,
    at: usize,
) -> Result<Vec<u8>> {
    let source = engine::open(other.to_vec(), other_pw)?;
    engine::edit(bytes, pw, |doc| {
        let at = at.min(doc.pages().len() as usize) as i32;
        let last = source.pages().len() - 1;
        doc.pages_mut()
            .copy_page_range_from_document(&source, 0..=last, at)?;
        Ok(())
    })
}

/// Inserts a blank page at `at`, sized like the neighbouring page (A4 if none).
pub fn insert_blank(bytes: &[u8], pw: Option<&str>, at: usize) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        let count = doc.pages().len() as usize;
        let at = at.min(count);
        let size = if count == 0 {
            PdfPagePaperSize::a4()
        } else {
            let r = doc.pages().page_size(at.min(count - 1) as i32)?;
            PdfPagePaperSize::Custom(r.width(), r.height())
        };
        doc.pages_mut().create_page_at_index(size, at as i32)?;
        Ok(())
    })
}

/// Trims every listed page by the given margins in points (left, bottom, right, top).
pub fn crop(bytes: &[u8], pw: Option<&str>, pages: &[usize], margins: [f32; 4]) -> Result<Vec<u8>> {
    let [l, b, r, t] = margins;
    if margins.iter().any(|m| *m < 0.0) {
        bail!("margins cannot be negative");
    }
    engine::edit(bytes, pw, |doc| {
        for &index in pages {
            let mut page = doc.pages().get(index as i32)?;
            let current = page
                .boundaries()
                .crop()
                .map(|b| b.bounds)
                .unwrap_or_else(|_| page.page_size());
            let new = PdfRect::new_from_values(
                current.bottom().value + b,
                current.left().value + l,
                current.top().value - t,
                current.right().value - r,
            );
            if new.width().value < 10.0 || new.height().value < 10.0 {
                bail!("margins leave less than 10 pt of page {}", index + 1);
            }
            page.boundaries_mut().set_crop(new)?;
        }
        Ok(())
    })
}
