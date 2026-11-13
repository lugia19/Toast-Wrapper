//! system-toast probe (Armored Core VI 1.09.1, stock exe, me3 0.13.0): how `01_SystemToast` is driven.
//!
//! Static findings this checks (Ghidra, see docs\research-notes.md):
//! - CSMenuMan (`[0x144e15530]`) +0x6f8 = toast model. Its text (a MenuString at model+0x08) is set through
//!   0x140dc25e0(MenuString*), and CSMenuMan's frame update ticks the model (0x140e976b0(model, dt)) and then clears the
//!   text, so a toast only stays while someone sets the text every frame.
//! - Model update: requested = text non-empty; +0x98 = last request, +0x99 = shown, +0x9c = hold timer (1 s after
//!   the text stops); then copies the text to model+0x58. The window controller reads `*[model+0x48]` (= +0x99) as
//!   its show / hide bool (FadeIn / FadeOut) and the text at model+0x08 for `ItemList/Text_0`.
//! - MenuString: +0x00 direct wchar pointer (FMG data, or null), +0x08 owned DLString (allocator +0x08, buffer
//!   +0x10, length +0x20, capacity +0x28), +0x38 extra. 0x140cb1ec0(out, id) = System_Message FMG text (category 0xcb).
//!   Never keep a game-made MenuString past its frame: its direct pointer goes into FMG data that the game replaces
//!   when it loads (a stock string kept from the title screen showed mission text in the garage).
//!
//! Keys (polled every 50 ms; never F6): F7 toggles a custom toast (our own text), F8 toggles a stock one (system
//! message 5000), F9 dumps the model. Log: Game\debug-logs\system_toast_probe.log.

use std::ffi::c_void;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use retour::RawDetour;

const CS_MENU_MAN: usize = 0x144e15530;

/// Sets the toast text: copies a MenuString into [CSMenuMan+0x6f8]+0x08.
const SET_TEXT_ADDR: usize = 0x140dc25e0;
const SET_TEXT_PROLOGUE: [u8; 6] = [0x48, 0x8b, 0xd1, 0x48, 0x8b, 0x0d];
/// Toast model per-frame update (model, dt).
const MODEL_UPDATE_ADDR: usize = 0x140e976b0;
const MODEL_UPDATE_PROLOGUE: [u8; 8] = [0x40, 0x57, 0x48, 0x83, 0xec, 0x50, 0x48, 0xc7];
/// MenuString default constructor (empty) and System_Message lookup (out, id).
const MENU_STRING_EMPTY: usize = 0x140442520;
const SYSTEM_MESSAGE: usize = 0x140cb1ec0;

const MODEL_TEXT: usize = 0x08;
const MODEL_SHOW_PTR: usize = 0x48;
const MODEL_SHOWN_TEXT: usize = 0x58;
const MODEL_REQUESTED: usize = 0x98;
const MODEL_SHOWN: usize = 0x99;
const MODEL_HOLD: usize = 0x9c;

const STOCK_MESSAGE_ID: i32 = 5000; // "Waiting to join Versus Room..."
const CUSTOM_TEXT: &str = "system-toast probe: custom toast text";

const VK_F7: i32 = 0x76;
const VK_F8: i32 = 0x77;
const VK_F9: i32 = 0x78;

const INJECT_OFF: u8 = 0;
const INJECT_CUSTOM: u8 = 1;
const INJECT_STOCK: u8 = 2;

static INJECT: AtomicU8 = AtomicU8::new(INJECT_OFF);
static DUMP_PENDING: AtomicBool = AtomicBool::new(false);
static ORIG_SET_TEXT: AtomicUsize = AtomicUsize::new(0);
static ORIG_MODEL_UPDATE: AtomicUsize = AtomicUsize::new(0);

/// Room for a MenuString (0x40 bytes; twice that in case the layout is off).
#[repr(C, align(16))]
struct MenuStringBuf([u8; 0x80]);

/// Our custom MenuString (empty, direct pointer to our own UTF-16 text), built on the game thread on first use and
/// leaked with its text, so the pointer stays valid as long as the game may read it.
static CUSTOM: OnceLock<usize> = OnceLock::new();

/// Last logged text set (text, callers) and model state (model, (requested, shown, show flag)), to log changes only.
static LAST_SET: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
static LAST_STATE: Mutex<(usize, (u8, u8, Option<u8>))> = Mutex::new((0, (0, 0, None)));

static START: OnceLock<Instant> = OnceLock::new();
static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

unsafe extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
    fn RtlCaptureStackBackTrace(skip: u32, count: u32, frames: *mut usize, hash: *mut u32) -> u16;
}

fn log(line: &str) {
    let ms = START.get().map(|s| s.elapsed().as_millis()).unwrap_or(0);
    if let Some(file) = LOG.get() {
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(f, "{ms:>8} {line}");
            let _ = f.flush();
        }
    }
}

/// Return addresses of the hook's callers inside the game exe.
fn callers() -> String {
    let mut frames = [0usize; 10];
    let n = unsafe { RtlCaptureStackBackTrace(2, frames.len() as u32, frames.as_mut_ptr(), std::ptr::null_mut()) };
    frames[..n as usize]
        .iter()
        .filter(|&&a| (0x140000000..0x147000000).contains(&a))
        .take(6)
        .map(|a| format!("{a:#x}"))
        .collect::<Vec<_>>()
        .join(" <- ")
}

unsafe fn read<T: Copy>(addr: usize) -> T {
    unsafe { std::ptr::read_unaligned(addr as *const T) }
}

unsafe fn wide_str(ptr: *const u16, max: usize) -> String {
    if ptr.is_null() {
        return "<null>".into();
    }
    let mut len = 0;
    while len < max && unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// A MenuString's text: the direct pointer if set, else the DLString's buffer (inline up to 7 chars).
unsafe fn menu_string(s: usize) -> String {
    unsafe {
        let direct: usize = read(s);
        if direct != 0 {
            return wide_str(direct as *const u16, 512);
        }
        let capacity: usize = read(s + 0x28);
        let buffer = if capacity > 7 { read::<usize>(s + 0x10) } else { s + 0x10 };
        wide_str(buffer as *const u16, 512)
    }
}

/// Frees a game-made MenuString's DLString heap buffer, like the game's inlined destructor: when capacity > 7, the
/// allocator (+0x08) frees the buffer (+0x10) through vfunc +0x68.
unsafe fn free_menu_string(s: usize) {
    unsafe {
        if read::<usize>(s + 0x28) > 7 {
            let allocator: usize = read(s + 0x08);
            let free: unsafe extern "system" fn(usize, usize) =
                std::mem::transmute(read::<usize>(read::<usize>(allocator) + 0x68));
            free(allocator, read(s + 0x10));
        }
    }
}

/// The model's show bool, `*[model+0x48]` (None while the pointer is null).
unsafe fn show_flag(model: usize) -> Option<u8> {
    unsafe {
        let ptr: usize = read(model + MODEL_SHOW_PTR);
        (ptr != 0).then(|| read(ptr))
    }
}

/// Calls the game's set-text function, skipping our logging detour.
unsafe fn set_text(menu_string: usize) {
    unsafe {
        let orig: unsafe extern "system" fn(usize) = std::mem::transmute(ORIG_SET_TEXT.load(Ordering::Relaxed));
        orig(menu_string);
    }
}

unsafe fn hex(addr: usize, len: usize) -> String {
    (0..len)
        .step_by(16)
        .map(|o| {
            let bytes: Vec<String> = (0..16).map(|i| format!("{:02x}", unsafe { read::<u8>(addr + o + i) })).collect();
            format!("    +{o:03x}: {}", bytes.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

unsafe fn dump_model(model: usize) {
    unsafe {
        let show_ptr: usize = read(model + MODEL_SHOW_PTR);
        log(&format!(
            "model {model:#x}: text {:?}, shown text {:?}, requested {}, shown {}, hold {:.2}; show ptr {show_ptr:#x}{} -> {:?}\n{}",
            menu_string(model + MODEL_TEXT),
            menu_string(model + MODEL_SHOWN_TEXT),
            read::<u8>(model + MODEL_REQUESTED),
            read::<u8>(model + MODEL_SHOWN),
            read::<f32>(model + MODEL_HOLD),
            if show_ptr == model + MODEL_SHOWN { " (= model+0x99)" } else { "" },
            show_flag(model),
            hex(model, 0xa0)
        ));
    }
}

unsafe extern "system" fn set_text_hook(text: usize) {
    unsafe {
        let current = (menu_string(text), callers());
        let changed = {
            let mut last = LAST_SET.lock().unwrap();
            let changed = *last != current;
            if changed {
                *last = current.clone();
            }
            changed
        };
        if changed {
            log(&format!("set text {:?} (direct ptr {:#x}) from {}", current.0, read::<usize>(text), current.1));
        }
        set_text(text);
    }
}

unsafe fn custom_menu_string() -> usize {
    *CUSTOM.get_or_init(|| unsafe {
        let empty: unsafe extern "system" fn(usize) -> usize = std::mem::transmute(MENU_STRING_EMPTY);
        let s = Box::leak(Box::new(MenuStringBuf([0; 0x80]))).0.as_mut_ptr() as usize;
        empty(s);
        let text: &'static [u16] = CUSTOM_TEXT.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>().leak();
        std::ptr::write_unaligned(s as *mut usize, text.as_ptr() as usize);
        log(&format!("built custom MenuString: {:?}", menu_string(s)));
        s
    })
}

/// Sets our text for this frame. The game's own toasts are online-only (Versus Room), and modded play is never
/// online, so there's nothing to share the slot with.
unsafe fn inject() {
    unsafe {
        let mode = INJECT.load(Ordering::Relaxed);
        if mode == INJECT_OFF {
            return;
        }
        if mode == INJECT_CUSTOM {
            set_text(custom_menu_string());
        } else {
            // Fetched every frame, like the game does (see the module comment).
            let system_message: unsafe extern "system" fn(usize, i32) -> usize = std::mem::transmute(SYSTEM_MESSAGE);
            let mut temp = MenuStringBuf([0; 0x80]);
            let s = temp.0.as_mut_ptr() as usize;
            system_message(s, STOCK_MESSAGE_ID);
            set_text(s);
            free_menu_string(s);
        }
    }
}

/// System announce (the same queued system as Elden Ring's FeSystemAnnounceViewModel): 0x140e79dd0(model, wchar*)
/// pushes a copy of the text if fewer than 9 are queued; model = [CSMenuMan+0x6f0] (deque at +0x08, count +0x30).
const ANNOUNCE_PUSH: usize = 0x140e79dd0;
const MENU_MAN_ANNOUNCE_MODEL: usize = 0x6f0;
const ANNOUNCE_TEXT: &str = "system-toast probe: system announce test, a longer line to see whether it scrolls across the banner";
static ANNOUNCE_PENDING: AtomicBool = AtomicBool::new(false);

unsafe fn announce() {
    unsafe {
        let model: usize = read(read::<usize>(CS_MENU_MAN) + MENU_MAN_ANNOUNCE_MODEL);
        if model == 0 {
            return log("F10: no announce model");
        }
        let text: Vec<u16> = ANNOUNCE_TEXT.encode_utf16().chain(std::iter::once(0)).collect();
        let push: unsafe extern "system" fn(usize, *const u16) -> u8 = std::mem::transmute(ANNOUNCE_PUSH);
        let pushed = push(model, text.as_ptr());
        log(&format!("F10: announce model {model:#x}, push -> {pushed}, queued {}", read::<usize>(model + 0x30)));
    }
}

unsafe extern "system" fn model_update_hook(model: usize, dt: f32, a: usize, b: usize) {
    unsafe {
        inject();
        if ANNOUNCE_PENDING.swap(false, Ordering::Relaxed) {
            announce();
        }
        if DUMP_PENDING.swap(false, Ordering::Relaxed) {
            log("F9 dump, before update:");
            dump_model(model);
        }
        let orig: unsafe extern "system" fn(usize, f32, usize, usize) =
            std::mem::transmute(ORIG_MODEL_UPDATE.load(Ordering::Relaxed));
        orig(model, dt, a, b);

        let state = (read::<u8>(model + MODEL_REQUESTED), read::<u8>(model + MODEL_SHOWN), show_flag(model));
        let mut last = LAST_STATE.lock().unwrap();
        if last.0 != model {
            log(&format!("first model update, CSMenuMan {:#x}:", read::<usize>(CS_MENU_MAN)));
            dump_model(model);
        }
        if last.1 != state {
            log(&format!(
                "state: requested {}, shown {}, *[+0x48] {:?}, hold {:.2}, text {:?}",
                state.0,
                state.1,
                state.2,
                read::<f32>(model + MODEL_HOLD),
                menu_string(model + MODEL_SHOWN_TEXT)
            ));
        }
        *last = (model, state);
    }
}

unsafe fn hook(addr: usize, prologue: &[u8], detour: *const (), orig: &AtomicUsize) -> Result<(), String> {
    let actual = unsafe { std::slice::from_raw_parts(addr as *const u8, prologue.len()) };
    if actual != prologue {
        return Err(format!("prologue at {addr:#x} doesn't match (not 1.09.1?): {actual:02x?}"));
    }
    let d = unsafe { RawDetour::new(addr as *const (), detour) }.map_err(|e| e.to_string())?;
    orig.store(d.trampoline() as *const () as usize, Ordering::Relaxed);
    unsafe { d.enable() }.map_err(|e| e.to_string())?;
    std::mem::forget(d);
    Ok(())
}

fn toggle_injection(mode: u8, key: &str) {
    let new = if INJECT.load(Ordering::Relaxed) == mode { INJECT_OFF } else { mode };
    INJECT.store(new, Ordering::Relaxed);
    let what = match new {
        INJECT_CUSTOM => "custom text",
        INJECT_STOCK => "stock message",
        _ => "off",
    };
    log(&format!("{key}: injection {what}"));
}

fn key_loop() {
    let mut was_down = [false; 4];
    let mut pressed = |i: usize, key: i32| {
        let down = unsafe { GetAsyncKeyState(key) } as u16 & 0x8000 != 0;
        let edge = down && !was_down[i];
        was_down[i] = down;
        edge
    };
    loop {
        if pressed(0, VK_F7) {
            toggle_injection(INJECT_CUSTOM, "F7");
        }
        if pressed(1, VK_F8) {
            toggle_injection(INJECT_STOCK, "F8");
        }
        if pressed(2, VK_F9) {
            DUMP_PENDING.store(true, Ordering::Relaxed);
            log("F9: model dump at the next update");
        }
        if pressed(3, 0x79) {
            ANNOUNCE_PENDING.store(true, Ordering::Relaxed);
            log("F10: system announce at the next update");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn init() -> Result<(), String> {
    unsafe {
        hook(SET_TEXT_ADDR, &SET_TEXT_PROLOGUE, set_text_hook as *const (), &ORIG_SET_TEXT)?;
        hook(MODEL_UPDATE_ADDR, &MODEL_UPDATE_PROLOGUE, model_update_hook as *const (), &ORIG_MODEL_UPDATE)?;
    }
    std::thread::spawn(key_loop);
    Ok(())
}

fn log_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("debug-logs")))
        .unwrap_or_else(|| "debug-logs".into())
}

#[unsafe(no_mangle)]
extern "system" fn DllMain(_module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        let dir = log_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = START.set(Instant::now());
        if let Ok(file) = std::fs::File::create(dir.join("system_toast_probe.log")) {
            let _ = LOG.set(Mutex::new(file));
        }
        match init() {
            Ok(()) => log("hooked"),
            Err(e) => log(&format!("not hooked: {e}")),
        }
    }
    1
}
