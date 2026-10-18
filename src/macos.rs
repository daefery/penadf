//! macOS: PDFs opened from Finder (double-click, "Open With", dropping on the Dock icon).
//!
//! Windows passes the file as a command-line argument. macOS instead sends the running app an
//! "open URLs" message, addressed to the application delegate. winit installs its own delegate
//! and does not handle that message, so we add an `application:openURLs:` method to the
//! delegate's class.
//!
//! Timing: when a double-click starts PenaDF, macOS delivers the file right after launching, before
//! `applicationDidFinishLaunching`. The method is therefore added when AppKit posts
//! `NSApplicationWillFinishLaunchingNotification`: the delegate exists by then, and no open
//! request has been delivered yet.
//!
//! Paths go into the same channel as files handed over by a second launch (`instance.rs`), so
//! they open as tabs in the existing window.

use std::ffi::{CString, c_void};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::{class, ffi, msg_send};
use objc2_foundation::{NSArray, NSNotification, NSNotificationCenter, NSString, NSURL};

static SENDER: OnceLock<Sender<Vec<PathBuf>>> = OnceLock::new();
static WAKE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// `- (void)application:(NSApplication *)app openURLs:(NSArray<NSURL *> *)urls`
extern "C" fn open_urls(
    _this: *mut AnyObject,
    _cmd: *const c_void,
    _app: *mut AnyObject,
    urls: *mut NSArray<NSURL>,
) {
    let Some(urls) = (unsafe { urls.as_ref() }) else {
        return;
    };
    let paths: Vec<PathBuf> = (0..urls.count())
        .filter_map(|i| unsafe { urls.objectAtIndex(i).path() })
        .map(|p| PathBuf::from(p.to_string()))
        .collect();
    if paths.is_empty() {
        return;
    }
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(paths);
    }
    if let Some(wake) = WAKE.get() {
        wake();
    }
}

/// Adds `application:openURLs:` to the class of the current application delegate.
fn add_open_urls_to_delegate() {
    let selector = CString::new("application:openURLs:").unwrap();
    // v = void return, @ = self, : = selector, @ = NSApplication, @ = NSArray
    let types = CString::new("v@:@@").unwrap();
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let delegate: *mut AnyObject = msg_send![app, delegate];
        if delegate.is_null() {
            return;
        }
        let cls = ffi::object_getClass(delegate.cast()) as *mut ffi::objc_class;
        let sel = ffi::sel_registerName(selector.as_ptr());
        let imp: ffi::IMP = Some(std::mem::transmute::<
            extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject, *mut NSArray<NSURL>),
            unsafe extern "C" fn(),
        >(open_urls));
        // Adds the method, or replaces it should winit ever add its own.
        ffi::class_replaceMethod(cls, sel, imp, types.as_ptr());
    }
}

/// Forwards Finder "open" requests into `tx`. Call once, on the main thread, before
/// `eframe::run_native`.
pub fn forward_open_events(tx: Sender<Vec<PathBuf>>, wake: impl Fn() + Send + Sync + 'static) {
    let _ = SENDER.set(tx);
    let _ = WAKE.set(Box::new(wake));
    let block = RcBlock::new(|_note: NonNull<NSNotification>| add_open_urls_to_delegate());
    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        // The notification's name is this exact string (AppKit's
        // NSApplicationWillFinishLaunchingNotification).
        let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
        let token =
            center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block);
        // The observer lives for the whole app run.
        std::mem::forget(token);
    }
}
