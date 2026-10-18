//! Binds PDFium once per process and loads documents from bytes.
//!
//! PDFium is loaded at run time from `pdfium.dll` (Windows) or `libpdfium.so` (Linux).
//! Search order: next to the executable, the macOS app bundle's `Frameworks` folder,
//! `PENADF_PDFIUM_DIR`, then the working directory.
//!
//! Threading: PDFium is not thread-safe. `pdfium-render` serialises single calls, but a
//! sequence of calls (open a document, then read its pages) must not overlap with another
//! thread's. Use every function here from one thread at a time (the app uses the UI thread).

use anyhow::{Context, Result, anyhow};
use pdfium_render::prelude::*;
use std::path::PathBuf;
use std::sync::OnceLock;

struct Engine {
    pdfium: Pdfium,
    /// A second handle on the same loaded library, for the few PDFium calls that
    /// `pdfium-render` does not wrap (moving pages inside one document).
    raw: Box<dyn PdfiumLibraryBindings>,
}

static ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        // PenaDF.app/Contents/MacOS/PenaDF -> PenaDF.app/Contents/Frameworks/libpdfium.dylib
        dirs.push(dir.join("../Frameworks"));
        dirs.push(dir);
    }
    if let Some(dir) = std::env::var_os("PENADF_PDFIUM_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(PathBuf::from("."));
    dirs
}

fn bind() -> std::result::Result<Engine, String> {
    let mut tried = Vec::new();
    for dir in candidate_dirs() {
        let path = Pdfium::pdfium_platform_library_name_at_path(&dir);
        // Both handles must be created before `Pdfium::new` publishes the global bindings.
        match (
            Pdfium::bind_to_library(&path),
            Pdfium::bind_to_library(&path),
        ) {
            (Ok(main), Ok(raw)) => {
                return Ok(Engine {
                    pdfium: Pdfium::new(main),
                    raw,
                });
            }
            (Err(e), _) | (_, Err(e)) => tried.push(format!("{}: {e}", path.display())),
        }
    }
    Err(format!(
        "PDFium library not found. Tried:\n{}",
        tried.join("\n")
    ))
}

fn engine() -> Result<&'static Engine> {
    ENGINE
        .get_or_init(bind)
        .as_ref()
        .map_err(|e| anyhow!(e.clone()))
}

/// The process-wide PDFium instance.
pub fn pdfium() -> Result<&'static Pdfium> {
    Ok(&engine()?.pdfium)
}

/// True when the error means "this file needs a (different) password".
pub fn is_password_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<PdfiumError>(),
            Some(PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::PasswordError
            ))
        )
    })
}

/// Opens a document from bytes. The document owns its copy of the bytes.
pub fn open(bytes: Vec<u8>, password: Option<&str>) -> Result<PdfDocument<'static>> {
    pdfium()?
        .load_pdf_from_byte_vec(bytes, password)
        .context("could not open PDF")
}

/// Opens a document, lets `edit` change it in place, and returns the saved bytes.
/// Encryption, bookmarks and form data of the original are kept.
pub fn edit(
    bytes: &[u8],
    password: Option<&str>,
    edit: impl FnOnce(&mut PdfDocument<'static>) -> Result<()>,
) -> Result<Vec<u8>> {
    let mut doc = open(bytes.to_vec(), password)?;
    edit(&mut doc)?;
    doc.save_to_bytes().context("could not save PDF")
}

/// Raw permission bits of an open document (PDF 32000 table 22): bit 3 print, 4 modify,
/// 5 copy, 6 annotate, 9 fill forms, 10 accessibility, 11 assemble, 12 high-quality print.
/// `0xFFFF_FFFF` when unprotected or opened with the owner password.
/// Read directly because `pdfium-render`'s helpers reject AES-256 (revision 6) files.
pub fn permission_bits(doc: &PdfDocument) -> Result<u32> {
    let raw = &engine()?.raw;
    Ok(unsafe { raw.FPDF_GetDocPermissions(raw.get_handle_from_document(doc)) } as u32)
}

/// True if the open document uses a password security handler (revision 2-6).
pub fn is_protected(doc: &PdfDocument) -> Result<bool> {
    let raw = &engine()?.raw;
    Ok(unsafe { raw.FPDF_GetSecurityHandlerRevision(raw.get_handle_from_document(doc)) } > 0)
}

/// Moves the pages at `indices` (zero-based, in that order) so they start at `dest`
/// in the resulting order. No page of `doc` may be open while this runs.
pub fn move_pages(doc: &PdfDocument, indices: &[i32], dest: i32) -> Result<()> {
    let raw = &engine()?.raw;
    let ok = unsafe {
        let handle = raw.get_handle_from_document(doc);
        raw.FPDF_MovePages(handle, indices.as_ptr(), indices.len() as _, dest)
    };
    if raw.is_true(ok) {
        Ok(())
    } else {
        Err(anyhow!("PDFium could not move the pages"))
    }
}
