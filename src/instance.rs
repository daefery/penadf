//! One window per user: when PenaDF is already running, a new launch (for example
//! double-clicking a PDF in Explorer) hands its files to the running window, which opens
//! them as tabs, and then exits.
//!
//! Uses a local socket (a named pipe on Windows). Creating the listener fails if another
//! process already owns the name, so there is no race between two launches.

use interprocess::local_socket::{
    GenericNamespaced, ListenerNonblockingMode, ListenerOptions, Stream, ToNsName, prelude::*,
};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// Message sent when a second launch had no files: just bring the window forward.
const FOCUS: &str = "\u{1}focus";

/// What a launch should do.
pub enum Launch {
    /// This is the first instance. File paths from other launches arrive on the receiver;
    /// an empty list means "show the window".
    Primary(Receiver<Vec<PathBuf>>),
    /// Another instance took the files; this process should exit.
    Forwarded,
}

/// Name unique per user, so two people on one PC each get their own window.
pub fn default_name() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    format!(
        "penadf-{}.sock",
        user.to_lowercase()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    )
}

/// Becomes the primary instance, or forwards `files` to it.
///
/// `wake` is called from a background thread after files arrive, so the UI can repaint.
/// If the socket cannot be used at all, the app still starts as its own window.
pub fn start(name: &str, files: &[PathBuf], wake: impl Fn() + Send + Sync + 'static) -> Launch {
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    // Several tries: the old primary may be closing just as we start.
    for _ in 0..3 {
        if send(name, files).is_ok() {
            return Launch::Forwarded;
        }
        if let Some(rx) = listen(name, wake.clone()) {
            return Launch::Primary(rx);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Launch::Primary(channel().1)
}

fn listen(name: &str, wake: Arc<dyn Fn() + Send + Sync>) -> Option<Receiver<Vec<PathBuf>>> {
    let ns = name.to_ns_name::<GenericNamespaced>().ok()?;
    // On macOS the name is a socket file in /tmp. A crash can leave it behind; since `send`
    // just failed, nobody is listening on it, so replacing it is safe. (Windows named pipes
    // vanish with their process, so this has no effect there.)
    let listener = ListenerOptions::new()
        .name(ns)
        .nonblocking(ListenerNonblockingMode::Neither)
        .try_overwrite(true)
        .create_sync()
        .ok()?;
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("penadf-instance".into())
        .spawn(move || {
            for conn in listener.incoming().flatten() {
                let mut paths = Vec::new();
                for line in BufReader::new(conn).lines().map_while(Result::ok) {
                    if line == FOCUS {
                        continue;
                    }
                    if !line.is_empty() {
                        paths.push(PathBuf::from(line));
                    }
                }
                if tx.send(paths).is_err() {
                    break;
                }
                wake();
            }
        })
        .ok()?;
    Some(rx)
}

/// Sends files to a running instance. Fails if none is listening.
pub fn send(name: &str, files: &[PathBuf]) -> std::io::Result<()> {
    let ns = name.to_ns_name::<GenericNamespaced>()?;
    let mut conn = Stream::connect(ns)?;
    let mut msg = String::new();
    if files.is_empty() {
        msg.push_str(FOCUS);
        msg.push('\n');
    }
    for f in files {
        // Absolute paths: the other process has a different working directory.
        let abs = std::path::absolute(f).unwrap_or_else(|_| f.clone());
        msg.push_str(&abs.to_string_lossy());
        msg.push('\n');
    }
    conn.write_all(msg.as_bytes())?;
    conn.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn second_launch_forwards_files_to_the_first() {
        let name = format!("penadf-test-{}.sock", std::process::id());
        let woke = Arc::new(AtomicUsize::new(0));
        let w = woke.clone();
        let Launch::Primary(rx) = start(&name, &[], move || {
            w.fetch_add(1, Ordering::SeqCst);
        }) else {
            panic!("first launch must become primary");
        };

        let files = vec![PathBuf::from("a b.pdf"), PathBuf::from("/x/c.pdf")];
        assert!(matches!(start(&name, &files, || {}), Launch::Forwarded));
        let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got[0].is_absolute() && got[0].ends_with("a b.pdf"));
        assert!(got[1].ends_with("c.pdf"));

        // A launch with no files just asks to show the window.
        assert!(matches!(start(&name, &[], || {}), Launch::Forwarded));
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_empty());
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(woke.load(Ordering::SeqCst), 2);
    }

    // macOS only: there the name is a file in /tmp. (Linux uses the abstract namespace,
    // which leaves nothing behind; Windows named pipes vanish with their process.)
    #[cfg(target_os = "macos")]
    #[test]
    fn a_stale_socket_file_from_a_crash_does_not_block_the_next_start() {
        let name = format!("penadf-stale-{}.sock", std::process::id());
        // A leftover socket file with nobody listening, as a crashed run leaves behind.
        let path = std::path::Path::new("/tmp").join(&name);
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(matches!(start(&name, &[], || {}), Launch::Primary(_)));
        assert!(
            matches!(start(&name, &[], || {}), Launch::Forwarded),
            "the new primary listens"
        );
    }

    #[test]
    fn name_is_safe_and_per_user() {
        let n = default_name();
        assert!(n.starts_with("penadf-") && n.ends_with(".sock"));
        assert!(
            n.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        );
    }
}
