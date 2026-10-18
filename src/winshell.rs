//! Windows file-association helpers.
//!
//! Since Windows 8, apps may not make themselves the default for a file type; the user
//! chooses in Settings (Microsoft: "The choice of default should be user driven",
//! learn.microsoft.com/windows/win32/shell/default-programs). The installer registers
//! PenaDF as a *candidate* for .pdf; these helpers tell whether the user picked it and
//! open the right Settings page.

/// ProgID the installer writes for .pdf (see installer/penadf.nsi).
pub const PROG_ID: &str = "PenaDF.Document";
/// Name under HKCU\Software\RegisteredApplications (see installer/penadf.nsi).
pub const REGISTERED_APP: &str = "PenaDF";

/// The URI that opens Windows Settings at PenaDF's default-apps page.
/// Windows 11 (2023-04 update or later) understands `registeredAppUser`; older builds
/// ignore it and show the general Default apps page.
pub fn default_apps_uri() -> String {
    format!(
        "ms-settings:defaultapps?registeredAppUser={}",
        uri_escape(REGISTERED_APP)
    )
}

fn uri_escape(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    use windows_sys::Win32::UI::Shell::{
        ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_EXECUTABLE, AssocQueryStringW, ShellExecuteW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, SW_SHOWNORMAL,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_hkcu(subkey: &str, value: &str) -> Option<String> {
        let (k, v) = (wide(subkey), wide(value));
        let mut buf = vec![0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                k.as_ptr(),
                v.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc != 0 {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
    }

    /// The ProgID the user chose for .pdf, if any.
    pub fn pdf_handler() -> Option<String> {
        read_hkcu(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pdf\UserChoice",
            "ProgId",
        )
    }

    /// The Microsoft Store package family name when PenaDF runs from its MSIX package.
    pub fn package_family() -> Option<String> {
        let mut len = 0u32;
        unsafe { GetCurrentPackageFamilyName(&mut len, std::ptr::null_mut()) };
        if len == 0 {
            return None; // not packaged (installer or portable build)
        }
        let mut buf = vec![0u16; len as usize];
        let rc = unsafe { GetCurrentPackageFamilyName(&mut len, buf.as_mut_ptr()) };
        (rc == 0).then(|| String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]))
    }

    /// True when Windows lists PenaDF as a PDF app: installed by the installer, or from the Store.
    pub fn is_registered() -> bool {
        package_family().is_some()
            || read_hkcu(r"Software\RegisteredApplications", REGISTERED_APP).is_some()
    }

    /// The program Windows starts for .pdf (works for both installer and Store builds).
    pub fn pdf_executable() -> Option<String> {
        let ext = wide(".pdf");
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let rc = unsafe {
            AssocQueryStringW(
                ASSOCF_INIT_IGNOREUNKNOWN,
                ASSOCSTR_EXECUTABLE,
                ext.as_ptr(),
                std::ptr::null(),
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        (rc == 0).then(|| String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]))
    }

    pub fn open_default_apps_settings() -> bool {
        // A Store install is identified by its app user model ID; an installer build by name.
        let uri = match package_family() {
            Some(family) => format!(
                "ms-settings:defaultapps?registeredAUMID={}",
                uri_escape(&format!("{family}!PenaDF"))
            ),
            None => default_apps_uri(),
        };
        let (verb, file) = (wide("open"), wide(&uri));
        let r = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        r as isize > 32
    }

    /// Lets the already-running window take focus when we hand it files.
    pub fn allow_other_to_take_focus() {
        unsafe {
            AllowSetForegroundWindow(ASFW_ANY);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn pdf_handler() -> Option<String> {
        None
    }
    pub fn is_registered() -> bool {
        false
    }
    pub fn package_family() -> Option<String> {
        None
    }
    pub fn pdf_executable() -> Option<String> {
        None
    }
    pub fn open_default_apps_settings() -> bool {
        false
    }
    pub fn allow_other_to_take_focus() {}
}

pub use imp::*;

/// True when the user picked PenaDF as the default for .pdf.
pub fn is_default_pdf_app() -> bool {
    if pdf_handler().is_some_and(|p| p.eq_ignore_ascii_case(PROG_ID)) {
        return true;
    }
    // Store installs use a generated ProgID; compare the program Windows would start instead.
    let ours = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    let theirs =
        pdf_executable().map(|p| p.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase());
    matches!((ours, theirs), (Some(a), Some(b)) if !a.is_empty() && a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_uri_escapes_the_app_name() {
        assert_eq!(
            default_apps_uri(),
            "ms-settings:defaultapps?registeredAppUser=PenaDF"
        );
    }
}
