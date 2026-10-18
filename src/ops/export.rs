//! Change file types: PDF pages to PNG/JPEG images, PDF text to TXT, PDF to HTML.

use crate::engine;
use anyhow::{Context, Result};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
        }
    }
}

/// Renders one page to RGBA at `dpi`. Also used by the viewer.
pub fn render_page(doc: &PdfDocument, page: usize, dpi: f32) -> Result<image::RgbaImage> {
    let page = doc.pages().get(page as i32)?;
    let config = PdfRenderConfig::new()
        .scale_page_by_factor(dpi / 72.0)
        .render_form_data(true);
    let bitmap = page.render_with_config(&config)?;
    let (w, h) = (bitmap.width() as u32, bitmap.height() as u32);
    image::RgbaImage::from_raw(w, h, bitmap.as_rgba_bytes()).context("bad bitmap size")
}

/// Writes one image per listed page into `dir` as `<stem>-<n>.<ext>`. Returns the paths.
pub fn to_images(
    bytes: &[u8],
    pw: Option<&str>,
    pages: &[usize],
    dpi: f32,
    format: ImageFormat,
    dir: &Path,
    stem: &str,
) -> Result<Vec<PathBuf>> {
    let doc = engine::open(bytes.to_vec(), pw)?;
    let mut written = Vec::new();
    for &p in pages {
        let img = render_page(&doc, p, dpi)?;
        let path = dir.join(format!("{stem}-{}.{}", p + 1, format.ext()));
        match format {
            ImageFormat::Png => img.save_with_format(&path, image::ImageFormat::Png)?,
            ImageFormat::Jpeg => image::DynamicImage::ImageRgba8(img)
                .into_rgb8()
                .save_with_format(&path, image::ImageFormat::Jpeg)?,
        }
        written.push(path);
    }
    Ok(written)
}

/// Plain text of the whole document, pages separated by form feeds (as pdftotext does).
pub fn to_text(bytes: &[u8], pw: Option<&str>) -> Result<String> {
    Ok(super::page_texts(bytes, pw)?.join("\n\u{c}\n"))
}

/// A self-contained, searchable HTML page: each PDF page as an embedded image
/// with its text in a selectable layer underneath for search and screen readers.
pub fn to_html(bytes: &[u8], pw: Option<&str>, title: &str) -> Result<String> {
    let doc = engine::open(bytes.to_vec(), pw)?;
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("<title>{}</title>\n", escape(title)));
    html.push_str("<style>body{margin:0;background:#eee;font-family:system-ui,sans-serif}main{max-width:900px;margin:auto;padding:16px}section{background:#fff;margin:0 0 16px;box-shadow:0 1px 4px #0003}section img{width:100%;display:block}section details{padding:8px 16px}pre{white-space:pre-wrap;font:inherit}</style>\n</head><body><main>\n");
    for index in 0..doc.pages().len() as usize {
        let img = render_page(&doc, index, 110.0)?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img).into_rgb8().write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Jpeg,
        )?;
        let text = doc.pages().get(index as i32)?.text()?.all();
        html.push_str(&format!(
            "<section id=\"page-{n}\"><img alt=\"Page {n}\" src=\"data:image/jpeg;base64,{b64}\"><details><summary>Text of page {n}</summary><pre>{t}</pre></details></section>\n",
            n = index + 1,
            b64 = base64(&png),
            t = escape(&text)
        ));
    }
    html.push_str("</main></body></html>\n");
    Ok(html)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn html_escape() {
        assert_eq!(
            escape("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }
}
