//! Embeds the icon and version information into the Windows .exe, so Explorer, the
//! taskbar and "Open with" show PenaDF properly.

fn main() {
    println!("cargo:rerun-if-changed=assets/penadf.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/penadf.ico")
        .set("ProductName", "PenaDF")
        .set("FileDescription", "PenaDF")
        .set("CompanyName", "PenaDF")
        .set("LegalCopyright", "MIT licence")
        .set("OriginalFilename", "PenaDF.exe")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);
    if let Err(e) = res.compile() {
        // Without the resource compiler the app still works; it only lacks its icon.
        println!("cargo:warning=could not embed the Windows icon: {e}");
    }
}
