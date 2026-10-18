// No console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod ui;

use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

fn main() -> eframe::Result {
    // Files passed on the command line (also how "Open with" and double-click arrive).
    let files: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();

    // If PenaDF is already open, hand the files to it (they open as tabs) and quit.
    // The window wakes through `ctx`, which only exists once eframe starts, so the
    // listener's wake-up goes through a slot filled in below.
    let ctx_slot: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
    let wake_slot = ctx_slot.clone();
    penadf::winshell::allow_other_to_take_focus();
    let incoming =
        match penadf::instance::start(&penadf::instance::default_name(), &files, move || {
            if let Some(ctx) = wake_slot.get() {
                ctx.request_repaint();
            }
        }) {
            penadf::instance::Launch::Forwarded => return Ok(()),
            penadf::instance::Launch::Primary(rx) => rx,
        };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PenaDF")
            .with_app_id("PenaDF")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([640.0, 420.0])
            .with_drag_and_drop(true)
            .with_icon(icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "PenaDF",
        options,
        Box::new(move |cc| {
            let _ = ctx_slot.set(cc.egui_ctx.clone());
            Ok(Box::new(ui::app::PenaApp::new(cc, files, Some(incoming))))
        }),
    )
}

/// The window and taskbar icon: the same artwork as the .exe icon, 64x64 RGBA,
/// produced by scripts/make_icon.py.
fn icon() -> egui::IconData {
    const N: u32 = 64;
    let rgba = include_bytes!("../assets/penadf-64.rgba").to_vec();
    debug_assert_eq!(rgba.len(), (N * N * 4) as usize);
    egui::IconData {
        rgba,
        width: N,
        height: N,
    }
}
