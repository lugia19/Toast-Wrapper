//! ac6_toast: one-line notifications ("toasts") for Armored Core VI mods, shown in the game's own `01_SystemToast`
//! window (top left). Other mods call the exports below (see README.md and client\ac6_toast_client.rs).
//!
//! Toasts play one at a time, in order. Each call copies the text, so callers' strings can be freed right away.
//! All exports are thread-safe and work before the game is up (toasts wait in the queue).
//!
//! API stability: an existing export's signature never changes; new behaviour gets a new export and a higher
//! `API_VERSION`, so a client only has to check the version is at least the one it was written for.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

mod game;
#[cfg(feature = "debug-hotkey")]
mod hotkey;
mod queue;
#[cfg(feature = "socket")]
mod socket;

const API_VERSION: u32 = 1;
/// Longest text kept, in UTF-16 units (the toast fits about 85 characters per line anyway).
const MAX_TEXT: usize = 512;

static QUEUE: Mutex<queue::Queue> = Mutex::new(queue::Queue::new());
/// The game hook is installed; without it toasts would only fill the queue.
static HOOKED: AtomicBool = AtomicBool::new(false);
static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

pub(crate) fn log(line: &str) {
    if let Some(file) = LOG.get()
        && let Ok(mut f) = file.lock()
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Converted before taking the queue lock (the game thread takes it every frame).
fn to_text(text: &str) -> queue::Text {
    let mut wide = queue::to_wide(text);
    if wide.len() > MAX_TEXT + 1 {
        wide = queue::to_wide(&String::from_utf16_lossy(&wide[..MAX_TEXT]));
    }
    wide
}

// ---- API (also used by the hotkey and socket features) ---------------------------------------------------------------

/// Queues a toast shown for `seconds` (0 or less: until dismissed). Returns its id, or 0 if the queue is full or
/// the game hook isn't installed.
pub(crate) fn show(text: &str, seconds: f32) -> u64 {
    if !HOOKED.load(Ordering::Relaxed) {
        return 0;
    }
    let text = to_text(text);
    QUEUE.lock().unwrap().show(text, seconds)
}

/// Replaces a toast's text. False if the id is unknown or the toast is finished.
pub(crate) fn update(id: u64, text: &str) -> bool {
    let text = to_text(text);
    QUEUE.lock().unwrap().update(id, text)
}

/// Ends a toast on screen (it fades out normally) or drops a queued one.
pub(crate) fn dismiss(id: u64) -> bool {
    QUEUE.lock().unwrap().dismiss(id)
}

/// UTF-8 from a caller's pointer and length (invalid sequences become U+FFFD).
unsafe fn text_arg<'a>(text: *const u8, len: usize) -> std::borrow::Cow<'a, str> {
    if text.is_null() {
        return "".into();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(text, len) })
}

// ---- exports -----------------------------------------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ac6_toast_api_version() -> u32 {
    API_VERSION
}

/// # Safety
/// `text` points at `len` bytes (UTF-8), or is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ac6_toast_show(text: *const u8, len: usize, seconds: f32) -> u64 {
    show(&unsafe { text_arg(text, len) }, seconds)
}

/// # Safety
/// `text` points at `len` bytes (UTF-8), or is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ac6_toast_update(id: u64, text: *const u8, len: usize) -> bool {
    update(id, &unsafe { text_arg(text, len) })
}

#[unsafe(no_mangle)]
pub extern "C" fn ac6_toast_dismiss(id: u64) -> bool {
    dismiss(id)
}

// ---- setup -------------------------------------------------------------------------------------------------------------

#[unsafe(no_mangle)]
extern "system" fn DllMain(_module: *mut std::ffi::c_void, reason: u32, _reserved: *mut std::ffi::c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        let log_dir = std::env::current_exe().map(|p| p.with_file_name("debug-logs")).unwrap_or("debug-logs".into());
        let _ = std::fs::create_dir_all(&log_dir);
        if let Ok(file) = std::fs::File::create(log_dir.join("ac6_toast.log")) {
            let _ = LOG.set(Mutex::new(file));
        }
        match game::install() {
            Ok(()) => {
                HOOKED.store(true, Ordering::Relaxed);
                log(&format!("ac6_toast {} (API {API_VERSION}): hooked", env!("CARGO_PKG_VERSION")));
            }
            Err(e) => log(&format!("ac6_toast: not hooked, toasts are refused: {e}")),
        }
        #[cfg(feature = "debug-hotkey")]
        hotkey::start();
        #[cfg(feature = "socket")]
        socket::start();
    }
    1
}
