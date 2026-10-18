//! Protect PDF: password to open, and restrictions on printing, copying and editing.
//! Uses AES-256 (PDF 2.0 / security handler revision 6).

use anyhow::{Context, Result, anyhow, bail};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, LoadOptions, Permissions};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What people who open the file *without* the owner password may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowed {
    pub print: bool,
    pub copy: bool,
    pub edit: bool,
    pub comment_and_fill: bool,
}

impl Default for Allowed {
    fn default() -> Self {
        Self {
            print: true,
            copy: false,
            edit: false,
            comment_and_fill: true,
        }
    }
}

impl Allowed {
    fn permissions(self) -> Permissions {
        // Accessibility extraction must always be allowed (PDF 2.0, 7.6.4.2).
        let mut p = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.print {
            p |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.copy {
            p |= Permissions::COPYABLE;
        }
        if self.edit {
            p |= Permissions::MODIFIABLE | Permissions::ASSEMBLABLE;
        }
        if self.comment_and_fill {
            p |= Permissions::ANNOTABLE | Permissions::FILLABLE;
        }
        p
    }
}

impl Allowed {
    /// What an open document permits, from PDFium's raw permission bits.
    pub fn from_bits(bits: u32) -> Self {
        let has = |p: Permissions| bits as u64 & p.bits() == p.bits();
        Self {
            print: has(Permissions::PRINTABLE),
            copy: has(Permissions::COPYABLE),
            edit: has(Permissions::MODIFIABLE),
            comment_and_fill: has(Permissions::ANNOTABLE) || has(Permissions::FILLABLE),
        }
    }
}

/// True if the file carries an /Encrypt dictionary.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    // Cheap check first; a real parse confirms.
    if !bytes.windows(8).any(|w| w == b"/Encrypt") {
        return false;
    }
    match Document::load_mem(bytes) {
        Ok(doc) => doc.is_encrypted() || doc.was_encrypted(),
        Err(_) => true,
    }
}

/// Encrypts with AES-256. `open_password` may be empty (anyone can open, restrictions
/// still apply). `owner_password` must be non-empty and differ from `open_password`.
pub fn protect(
    bytes: &[u8],
    current_password: Option<&str>,
    open_password: &str,
    owner_password: &str,
    allowed: Allowed,
) -> Result<Vec<u8>> {
    if owner_password.is_empty() {
        bail!("an owner password is required to set restrictions");
    }
    if owner_password == open_password {
        bail!("the owner password must differ from the open password");
    }
    let mut doc = load_decrypted(bytes, current_password)?;

    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|e| anyhow!("no secure random source: {e}"))?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password: open_password,
        permissions: allowed.permissions(),
    };
    let state = EncryptionState::try_from(version)
        .map_err(|e| anyhow!("encryption setup failed: {e:?}"))?;
    doc.encrypt(&state).context("encryption failed")?;
    save(&mut doc)
}

/// Removes all security. Needs the password that opens the file with full rights.
pub fn unprotect(bytes: &[u8], password: &str) -> Result<Vec<u8>> {
    let mut doc = load_decrypted(bytes, Some(password))?;
    save(&mut doc)
}

fn load_decrypted(bytes: &[u8], password: Option<&str>) -> Result<Document> {
    let options = match password {
        Some(p) if !p.is_empty() => LoadOptions::with_password(p),
        _ => LoadOptions::default(),
    };
    let mut doc = Document::load_mem_with_options(bytes, options).map_err(|e| match e {
        lopdf::Error::InvalidPassword => anyhow!("wrong password"),
        other => anyhow!("could not read PDF: {other}"),
    })?;
    if doc.is_encrypted() {
        bail!("wrong password");
    }
    // The reader leaves the loaded objects decrypted; forget the old security handler.
    doc.encryption_state = None;
    Ok(doc)
}

fn save(doc: &mut Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_to(&mut out).context("could not write PDF")?;
    Ok(out)
}
