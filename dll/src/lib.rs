//! toast_wrapper: one-line notifications for Armored Core VI and Elden Ring mods, shown by the game's own system
//! announce banner. Other mods call the exports below (see README.md and client\toast_wrapper_client.rs).
//! game.rs holds the per-game parts.
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
mod playback;
#[cfg(feature = "socket")]
mod socket;

/// 2: show. 3: show_for (custom durations).
const API_VERSION: u32 = 3;
/// Longest text kept, in UTF-16 units.
const MAX_TEXT: usize = 512;
/// Most texts waiting for room in the game's queue.
const MAX_PENDING: usize = 64;

/// A text not yet handed to the game.
pub(crate) struct Pending {
    /// UTF-16, null-terminated, padded.
    pub text: Vec<u16>,
    /// Seconds on screen; None = the game's own timing.
    pub duration: Option<f32>,
}

static PENDING: Mutex<VecDeque<Pending>> = Mutex::new(VecDeque::new());
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

/// Queues a text for the banner, on screen for `seconds` (None: the game's own timing). False if the DLL isn't hooked
/// or too many texts are waiting.
pub(crate) fn show(text: &str, seconds: Option<f32>) -> bool {
    if !HOOKED.load(Ordering::Relaxed) {
        return false;
    }
    let padding = std::iter::repeat_n(b' ' as u16, game::padding());
    let mut wide: Vec<u16> = padding.chain(text.encode_utf16()).take(MAX_TEXT).collect();
    wide.iter_mut().filter(|c| **c == 0).for_each(|c| *c = b' ' as u16);
    wide.push(0);
    let mut pending = PENDING.lock().unwrap();
    if pending.len() >= MAX_PENDING {
        return false;
    }
    pending.push_back(Pending { text: wide, duration: seconds.filter(|s| *s > 0.0) });
    true
}

/// Game thread: hands waiting texts to `push` (the game copies them) until it reports its queue full.
pub(crate) fn drain_pending(mut push: impl FnMut(&Pending) -> bool) {
    let mut pending = PENDING.lock().unwrap();
    while let Some(next) = pending.front() {
        if !push(next) {
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
    show(&unsafe { text_arg(text, len) }, None)
}

/// Like `toast_wrapper_show`, but the text stays on screen for `seconds` (plus the fade in and out), longer or
/// shorter than the game's own timing. 0 or less: the game's own timing. API version 3.
///
/// # Safety
/// `text` points at `len` bytes, or is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn toast_wrapper_show_for(text: *const u8, len: usize, seconds: f32) -> bool {
    show(&unsafe { text_arg(text, len) }, Some(seconds))
}

/// UTF-8 from a caller's pointer and length (invalid sequences become U+FFFD).
unsafe fn text_arg<'a>(text: *const u8, len: usize) -> std::borrow::Cow<'a, str> {
    let bytes = if text.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(text, len) } };
    String::from_utf8_lossy(bytes)
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
            Ok(game) => {
                HOOKED.store(true, Ordering::Relaxed);
                log(&format!("toast_wrapper {} (API {API_VERSION}): hooked {}", env!("CARGO_PKG_VERSION"), game.name));
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
            assert!(show(&format!("t{i}"), None));
        }
        let mut game_queue: Vec<String> = Vec::new();
        let mut push = |pending: &Pending| {
            if game_queue.len() >= 9 {
                return false;
            }
            game_queue.push(String::from_utf16_lossy(&pending.text[..pending.text.len() - 1]));
            true
        };
        drain_pending(&mut push);
        assert_eq!(game_queue.len(), 9);
        assert_eq!(game_queue[0], "t0"); // no game detected in tests: no padding
        assert_eq!(PENDING.lock().unwrap().len(), 3);
    }
}
