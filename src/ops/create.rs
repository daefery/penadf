//! Create PDF from other file types.
//!
//! Built in: images (PNG, JPEG, BMP, GIF, WebP, TIFF) and plain text.
//! Word, Excel and PowerPoint: converted by an office suite already installed on the PC
//! (Microsoft Office or LibreOffice). Nothing is uploaded anywhere.

use crate::engine;
use anyhow::{Context, Result, anyhow, bail};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

const A4: (f32, f32) = (595.0, 842.0);
const MARGIN: f32 = 36.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Pdf,
    Image,
    Text,
    Office,
}

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff"];
pub const TEXT_EXTS: &[&str] = &["txt", "md", "csv", "log"];
pub const OFFICE_EXTS: &[&str] = &[
    "doc", "docx", "rtf", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "odp",
];

pub fn kind_of(path: &Path) -> Option<SourceKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let ext = ext.as_str();
    if ext == "pdf" {
        Some(SourceKind::Pdf)
    } else if IMAGE_EXTS.contains(&ext) {
        Some(SourceKind::Image)
    } else if TEXT_EXTS.contains(&ext) {
        Some(SourceKind::Text)
    } else if OFFICE_EXTS.contains(&ext) {
        Some(SourceKind::Office)
    } else {
        None
    }
}

/// Converts any supported file into PDF bytes.
pub fn from_file(path: &Path) -> Result<Vec<u8>> {
    match kind_of(path) {
        Some(SourceKind::Pdf) => {
            std::fs::read(path).with_context(|| format!("could not read {}", path.display()))
        }
        Some(SourceKind::Image) => from_images(&[std::fs::read(path)?]),
        Some(SourceKind::Text) => from_text(&String::from_utf8_lossy(&std::fs::read(path)?)),
        Some(SourceKind::Office) => from_office(path),
        None => bail!("{} is not a supported file type", path.display()),
    }
}

/// One page per image. Each page is sized to the image (72 dpi, capped to A4 long side)
/// so nothing is cropped or stretched.
pub fn from_images(images: &[Vec<u8>]) -> Result<Vec<u8>> {
    if images.is_empty() {
        bail!("no images given");
    }
    let mut doc = engine::pdfium()?.create_new_pdf()?;
    for bytes in images {
        let img = decode_oriented(bytes)?;
        let (w, h) = (img.width() as f32, img.height() as f32);
        let scale = (A4.1 / w.max(h)).min(1.0);
        let (pw, ph) = (w * scale, h * scale);
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(pw),
                PdfPoints::new(ph),
            ))?;
        page.objects_mut().create_image_object(
            PdfPoints::ZERO,
            PdfPoints::ZERO,
            &img,
            Some(PdfPoints::new(pw)),
            Some(PdfPoints::new(ph)),
        )?;
    }
    Ok(doc.save_to_bytes()?)
}

/// Decodes an image and applies its EXIF orientation so phone photos stand upright.
fn decode_oriented(bytes: &[u8]) -> Result<image::DynamicImage> {
    use image::ImageDecoder;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder().context("unsupported image")?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).context("could not decode image")?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Plain text on A4 pages in Courier 10 pt, wrapped to the page width.
/// Courier is one of the 14 standard PDF fonts, so nothing needs embedding;
/// characters outside Latin-1 are replaced with '?'.
pub fn from_text(text: &str) -> Result<Vec<u8>> {
    const SIZE: f32 = 10.0;
    const LEADING: f32 = 12.0;
    let chars_per_line = ((A4.0 - 2.0 * MARGIN) / (SIZE * 0.6)) as usize;
    let lines_per_page = ((A4.1 - 2.0 * MARGIN) / LEADING) as usize;

    let lines = wrap(text, chars_per_line);
    let mut doc = engine::pdfium()?.create_new_pdf()?;
    let font = doc.fonts_mut().courier();
    for chunk in lines.chunks(lines_per_page.max(1)) {
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4())?;
        page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        for (i, line) in chunk.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let y = A4.1 - MARGIN - SIZE - i as f32 * LEADING;
            page.objects_mut().create_text_object(
                PdfPoints::new(MARGIN),
                PdfPoints::new(y),
                line,
                font,
                PdfPoints::new(SIZE),
            )?;
        }
        page.regenerate_content()?;
    }
    Ok(doc.save_to_bytes()?)
}

/// Hard-wraps text to `width` characters, keeping blank lines and expanding tabs.
/// Always returns at least one line so an empty file still gives one page.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for raw in text.replace("\r\n", "\n").split('\n') {
        let line: String = raw
            .replace('\t', "    ")
            .chars()
            .map(|c| {
                if (c as u32) < 256 && !c.is_control() {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in line.split_inclusive(' ') {
            if current.chars().count() + word.trim_end().chars().count() > width
                && !current.is_empty()
            {
                out.push(current.trim_end().to_string());
                current.clear();
            }
            let mut word = word.to_string();
            while word.chars().count() > width {
                let head: String = word.chars().take(width).collect();
                out.push(head);
                word = word.chars().skip(width).collect();
            }
            current.push_str(&word);
        }
        out.push(current.trim_end().to_string());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Which installed office suite can convert documents, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfficeConverter {
    LibreOffice(PathBuf),
    #[cfg(windows)]
    MicrosoftOffice,
}

pub fn find_office_converter() -> Option<OfficeConverter> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(
                PathBuf::from(base)
                    .join("LibreOffice")
                    .join("program")
                    .join("soffice.exe"),
            );
        }
    }
    #[cfg(not(windows))]
    candidates.extend(
        [
            "/usr/bin/soffice",
            "/usr/bin/libreoffice",
            "/usr/local/bin/soffice",
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        ]
        .map(PathBuf::from),
    );
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(OfficeConverter::LibreOffice(found));
    }
    #[cfg(windows)]
    if ms_office_installed() {
        return Some(OfficeConverter::MicrosoftOffice);
    }
    None
}

#[cfg(windows)]
fn ms_office_installed() -> bool {
    powershell(
        "foreach($p in 'Word.Application','Excel.Application','PowerPoint.Application'){ if([type]::GetTypeFromProgID($p)){ exit 0 } }; exit 1",
    )
    .map(|s| s.success())
    .unwrap_or(false)
}

#[cfg(windows)]
fn powershell(script: &str) -> std::io::Result<std::process::ExitStatus> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
}

fn from_office(path: &Path) -> Result<Vec<u8>> {
    let converter = find_office_converter().ok_or_else(|| {
        anyhow!("Converting Word, Excel and PowerPoint files needs Microsoft Office or the free LibreOffice installed on this PC.")
    })?;
    let out_dir = std::env::temp_dir().join(format!("penadf-convert-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir)?;
    let stem = path
        .file_stem()
        .ok_or_else(|| anyhow!("file has no name"))?;
    let target = out_dir.join(stem).with_extension("pdf");
    let _ = std::fs::remove_file(&target);

    let status = match &converter {
        OfficeConverter::LibreOffice(soffice) => {
            let mut cmd = Command::new(soffice);
            cmd.args([
                "--headless",
                "--norestore",
                "--convert-to",
                "pdf",
                "--outdir",
            ])
            .arg(&out_dir)
            .arg(path);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            cmd.status()?
        }
        #[cfg(windows)]
        OfficeConverter::MicrosoftOffice => powershell(&ms_office_script(path, &target))?,
    };
    let bytes = std::fs::read(&target);
    let _ = std::fs::remove_dir_all(&out_dir);
    match bytes {
        Ok(b) if status.success() || !b.is_empty() => Ok(b),
        _ => bail!("the office suite could not convert {}", path.display()),
    }
}

#[cfg(windows)]
fn ms_office_script(src: &Path, dst: &Path) -> String {
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    let (src, dst) = (q(src), q(dst));
    let ext = Path::new(&src)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // ExportAsFixedFormat / SaveAs constants: Word wdExportFormatPDF=17,
    // Excel xlTypePDF=0, PowerPoint ppSaveAsPDF=32.
    match ext.as_str() {
        "xls" | "xlsx" | "ods" => format!(
            "$a=New-Object -ComObject Excel.Application; $a.DisplayAlerts=$false; try {{ $d=$a.Workbooks.Open('{src}', 0, $true); $d.ExportAsFixedFormat(0, '{dst}'); $d.Close($false) }} finally {{ $a.Quit() }}"
        ),
        "ppt" | "pptx" | "odp" => format!(
            "$a=New-Object -ComObject PowerPoint.Application; try {{ $d=$a.Presentations.Open('{src}', $true, $false, $false); $d.SaveAs('{dst}', 32); $d.Close() }} finally {{ $a.Quit() }}"
        ),
        _ => format!(
            "$a=New-Object -ComObject Word.Application; $a.Visible=$false; try {{ $d=$a.Documents.Open('{src}', $false, $true); $d.ExportAsFixedFormat('{dst}', 17); $d.Close($false) }} finally {{ $a.Quit() }}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_long_lines_and_words() {
        assert_eq!(wrap("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_keeps_blank_lines_and_replaces_unsupported_chars() {
        assert_eq!(wrap("a\r\n\r\nb", 10), vec!["a", "", "b"]);
        assert_eq!(wrap("caf\u{e9} \u{4e2d}", 10), vec!["caf\u{e9} ?"]);
        assert_eq!(wrap("", 10), vec![""]);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(Path::new("a.PDF")), Some(SourceKind::Pdf));
        assert_eq!(kind_of(Path::new("a.jpeg")), Some(SourceKind::Image));
        assert_eq!(kind_of(Path::new("a.docx")), Some(SourceKind::Office));
        assert_eq!(kind_of(Path::new("a.txt")), Some(SourceKind::Text));
        assert_eq!(kind_of(Path::new("a.exe")), None);
        assert_eq!(kind_of(Path::new("noext")), None);
    }
}
