//! toast_wrapper: one-line notifications for Armored Core VI mods, shown by the game's own system announce banner
//! (a strip at the bottom of the screen). Other mods call the exports below (see README.md and
//! client\toast_wrapper_client.rs).
//!
//! The game queues and plays the texts itself (fade in, scroll when too wide, fade out), one after another. This DLL
//! only copies texts in and hands them to the game from the game thread, a few at a time as the game's queue (9
//! entries) has room.
//!
//! API stability: an existing export's signature never changes; new behaviour gets a new export and a higher
//! `API_VERSION`, so a client only has to check the version is at least the one it was written for.

use std::collections::VecDeque;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

mod game;
#[cfg(feature = "debug-hotkey")]
mod hotkey;
#[cfg(feature = "socket")]
mod socket;

const API_VERSION: u32 = 2;
/// The banner's text starts flush against the left screen edge; 18 spaces line it up with the key guide below it.
const PADDING: &str = "                  "; // 18 spaces
/// Longest text kept, in UTF-16 units.
const MAX_TEXT: usize = 512;
/// Most texts waiting for room in the game's queue.
const MAX_PENDING: usize = 64;

/// Texts not yet handed to the game: UTF-16, null-terminated, padded.
static PENDING: Mutex<VecDeque<Vec<u16>>> = Mutex::new(VecDeque::new());
/// The game hook is installed; without it texts would only pile up.
static HOOKED: AtomicBool = AtomicBool::new(false);
static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

pub(crate) fn log(line: &str) {
    if let Some(file) = LOG.get()
        && let Ok(mut f) = file.lock()
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Queues a text for the banner. False if the DLL isn't hooked or too many texts are waiting.
pub(crate) fn show(text: &str) -> bool {
    if !HOOKED.load(Ordering::Relaxed) {
        return false;
    }
    let mut wide: Vec<u16> = PADDING.encode_utf16().chain(text.encode_utf16()).take(MAX_TEXT).collect();
    wide.iter_mut().filter(|c| **c == 0).for_each(|c| *c = b' ' as u16);
    wide.push(0);
    let mut pending = PENDING.lock().unwrap();
    if pending.len() >= MAX_PENDING {
        return false;
    }
    pending.push_back(wide);
    true
}

/// Game thread: hands waiting texts to `push` (the game copies them) until it reports its queue full.
pub(crate) fn drain_pending(mut push: impl FnMut(&[u16]) -> bool) {
    let mut pending = PENDING.lock().unwrap();
    while let Some(text) = pending.front() {
        if !push(text) {
            break;
        }
        pending.pop_front();
    }
}

// ---- exports -----------------------------------------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn toast_wrapper_api_version() -> u32 {
    API_VERSION
}

/// Queues `len` bytes of UTF-8 text for the banner (invalid sequences become U+FFFD). Returns false if the DLL
/// couldn't hook the game or 64 texts are already waiting.
///
/// # Safety
/// `text` points at `len` bytes, or is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn toast_wrapper_show(text: *const u8, len: usize) -> bool {
    let bytes = if text.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(text, len) } };
    show(&String::from_utf8_lossy(bytes))
}

// ---- setup -------------------------------------------------------------------------------------------------------------

#[unsafe(no_mangle)]
extern "system" fn DllMain(_module: *mut std::ffi::c_void, reason: u32, _reserved: *mut std::ffi::c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        let log_dir = std::env::current_exe().map(|p| p.with_file_name("debug-logs")).unwrap_or("debug-logs".into());
        let _ = std::fs::create_dir_all(&log_dir);
        if let Ok(file) = std::fs::File::create(log_dir.join("toast_wrapper.log")) {
            let _ = LOG.set(Mutex::new(file));
        }
        match game::install() {
            Ok(()) => {
                HOOKED.store(true, Ordering::Relaxed);
                log(&format!("toast_wrapper {} (API {API_VERSION}): hooked", env!("CARGO_PKG_VERSION")));
            }
            Err(e) => log(&format!("toast_wrapper: not hooked, texts are refused: {e}")),
        }
        #[cfg(feature = "debug-hotkey")]
        hotkey::start();
        #[cfg(feature = "socket")]
        socket::start();
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_until_the_game_queue_is_full() {
        HOOKED.store(true, Ordering::Relaxed);
        for i in 0..12 {
            assert!(show(&format!("t{i}")));
        }
        let mut game_queue: Vec<String> = Vec::new();
        let mut push = |text: &[u16]| {
            if game_queue.len() >= 9 {
                return false;
            }
            game_queue.push(String::from_utf16_lossy(&text[..text.len() - 1]));
            true
        };
        drain_pending(&mut push);
        assert_eq!(game_queue.len(), 9);
        assert_eq!(game_queue[0], format!("{PADDING}t0"));
        assert_eq!(PENDING.lock().unwrap().len(), 3);
    }
}
