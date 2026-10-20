//! Document operations. Each takes the current PDF bytes and returns new bytes,
//! so the app gets undo/redo by keeping earlier byte versions.

pub mod compare;
pub mod create;
pub mod export;
pub mod forms;
pub mod markup;
pub mod pages;
pub mod protect;
pub mod text;

use crate::engine;
use anyhow::Result;
use pdfium_render::prelude::*;

/// Point on a page in PDF user space (points, origin bottom-left of the unrotated page).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PagePoint {
    pub x: f32,
    pub y: f32,
}

/// Rectangle on a page in PDF user space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageRect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl PageRect {
    /// Builds a normalised rectangle from any two corners.
    pub fn from_corners(a: PagePoint, b: PagePoint) -> Self {
        Self {
            left: a.x.min(b.x),
            right: a.x.max(b.x),
            bottom: a.y.min(b.y),
            top: a.y.max(b.y),
        }
    }

    pub fn to_pdf(self) -> PdfRect {
        PdfRect::new_from_values(self.bottom, self.left, self.top, self.right)
    }

    pub fn from_pdf(r: PdfRect) -> Self {
        Self {
            left: r.left().value,
            bottom: r.bottom().value,
            right: r.right().value,
            top: r.top().value,
        }
    }
}

/// Facts about a document shown in the UI.
#[derive(Clone, Debug, Default)]
pub struct DocInfo {
    pub page_count: usize,
    pub title: String,
    pub author: String,
    pub encrypted: bool,
    pub has_form: bool,
}

pub fn info(bytes: &[u8], password: Option<&str>) -> Result<DocInfo> {
    let doc = engine::open(bytes.to_vec(), password)?;
    let tag = |t| {
        doc.metadata()
            .get(t)
            .map(|m| m.value().to_string())
            .unwrap_or_default()
    };
    Ok(DocInfo {
        page_count: doc.pages().len() as usize,
        title: tag(PdfDocumentMetadataTagType::Title),
        author: tag(PdfDocumentMetadataTagType::Author),
        encrypted: engine::is_protected(&doc)?,
        has_form: doc.form().is_some(),
    })
}

/// Extracts the plain text of every page.
pub fn page_texts(bytes: &[u8], password: Option<&str>) -> Result<Vec<String>> {
    let doc = engine::open(bytes.to_vec(), password)?;
    doc.pages()
        .iter()
        .map(|page| Ok(page.text()?.all()))
        .collect()
}

/// One search hit: the page and the rectangles covering the matched text.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub page: usize,
    pub rects: Vec<PageRect>,
}

/// Case-insensitive text search across the whole document.
pub fn search(bytes: &[u8], password: Option<&str>, needle: &str) -> Result<Vec<SearchHit>> {
    if needle.trim().is_empty() {
        return Ok(Vec::new());
    }
    let doc = engine::open(bytes.to_vec(), password)?;
    let options = PdfSearchOptions::new();
    let mut hits = Vec::new();
    for (index, page) in doc.pages().iter().enumerate() {
        let text = page.text()?;
        let search = text.search(needle, &options)?;
        while let Some(segments) = search.find_next() {
            let rects = segments
                .iter()
                .map(|s| PageRect::from_pdf(s.bounds()))
                .collect();
            hits.push(SearchHit { page: index, rects });
        }
    }
    Ok(hits)
}
