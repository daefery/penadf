//! Fill out forms: list fields, set values, optionally flatten so answers become page content.

use super::PageRect;
use crate::engine;
use anyhow::{Result, bail};
use pdfium_render::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Checkbox,
    Other(String),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub page: usize,
    pub annotation: usize,
    pub name: String,
    pub kind: FieldKind,
    /// Text value, or "true"/"false" for checkboxes.
    pub value: String,
    pub rect: PageRect,
}

pub fn fields(bytes: &[u8], pw: Option<&str>) -> Result<Vec<Field>> {
    let doc = engine::open(bytes.to_vec(), pw)?;
    let mut out = Vec::new();
    for (page_index, page) in doc.pages().iter().enumerate() {
        for (annotation, a) in page.annotations().iter().enumerate() {
            let Some(f) = a.as_form_field() else { continue };
            let (kind, value) = match f.field_type() {
                PdfFormFieldType::Text => (
                    FieldKind::Text,
                    f.as_text_field()
                        .and_then(|t| t.value())
                        .unwrap_or_default(),
                ),
                PdfFormFieldType::Checkbox => {
                    let on = f
                        .as_checkbox_field()
                        .and_then(|c| c.is_checked().ok())
                        .unwrap_or(false);
                    (FieldKind::Checkbox, on.to_string())
                }
                other => (FieldKind::Other(format!("{other:?}")), String::new()),
            };
            out.push(Field {
                page: page_index,
                annotation,
                name: f
                    .name()
                    .unwrap_or_else(|| format!("field {}", out.len() + 1)),
                kind,
                value,
                rect: a.bounds().map(PageRect::from_pdf).unwrap_or(PageRect {
                    left: 0.,
                    bottom: 0.,
                    right: 0.,
                    top: 0.,
                }),
            });
        }
    }
    Ok(out)
}

/// Writes the given values back (text and checkbox fields).
pub fn fill(bytes: &[u8], pw: Option<&str>, values: &[Field]) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        for v in values {
            let page = doc.pages().get(v.page as i32)?;
            let mut annotation = page.annotations().get(v.annotation)?;
            let Some(field) = annotation.as_form_field_mut() else {
                bail!("{} is no longer a form field", v.name)
            };
            match v.kind {
                FieldKind::Text => {
                    if let Some(t) = field.as_text_field_mut() {
                        t.set_value(&v.value)?;
                    }
                }
                FieldKind::Checkbox => {
                    if let Some(c) = field.as_checkbox_field_mut() {
                        c.set_checked(v.value == "true")?;
                    }
                }
                FieldKind::Other(_) => {}
            }
        }
        Ok(())
    })
}

/// Burns form fields and annotations into page content so they can no longer change.
pub fn flatten(bytes: &[u8], pw: Option<&str>) -> Result<Vec<u8>> {
    engine::edit(bytes, pw, |doc| {
        for mut page in doc.pages().iter() {
            page.flatten()?;
        }
        Ok(())
    })
}
