//! The game side (Armored Core VI 1.09.1): one hook on the toast model's update, which sets the queue's text each
//! frame. Background: ..\..\docs\how-it-works.md.

use std::sync::OnceLock;

use retour::RawDetour;

use crate::QUEUE;

type ModelUpdateFn = unsafe extern "system" fn(usize, f32, usize, usize);
type SetTextFn = unsafe extern "system" fn(*mut MenuString);
type MenuStringEmptyFn = unsafe extern "system" fn(*mut MenuString) -> *mut MenuString;

/// Toast model update (model = [CSMenuMan+0x6f8], dt). CSMenuMan calls it every frame, then clears the text.
const MODEL_UPDATE_ADDR: usize = 0x140e976b0;
const MODEL_UPDATE_PROLOGUE: [u8; 8] = [0x40, 0x57, 0x48, 0x83, 0xec, 0x50, 0x48, 0xc7];
/// Sets the toast text: copies a MenuString into the model (+0x08).
const SET_TEXT_ADDR: usize = 0x140dc25e0;
const SET_TEXT_PROLOGUE: [u8; 6] = [0x48, 0x8b, 0xd1, 0x48, 0x8b, 0x0d];
/// MenuString constructor (empty string).
const MENU_STRING_EMPTY_ADDR: usize = 0x140442520;
/// Model byte: the game shows the toast (set with the text, cleared 1 s after it stops).
const MODEL_SHOWN: usize = 0x99;

/// The game's MenuString (0x40 bytes: +0x00 direct wchar pointer, +0x08 owned DLString); twice the size as a margin.
#[repr(C, align(16))]
pub struct MenuString {
    direct: *const u16,
    rest: [u8; 0x78],
}

static ORIG_MODEL_UPDATE: OnceLock<ModelUpdateFn> = OnceLock::new();

/// Our MenuString, built on the game thread on first use and leaked; each frame its direct pointer is set to the
/// active toast's text, which the game copies as is.
struct OurMenuString(*mut MenuString);
unsafe impl Send for OurMenuString {}
unsafe impl Sync for OurMenuString {}
static MENU_STRING: OnceLock<OurMenuString> = OnceLock::new();

fn menu_string() -> *mut MenuString {
    MENU_STRING
        .get_or_init(|| unsafe {
            let empty: MenuStringEmptyFn = std::mem::transmute(MENU_STRING_EMPTY_ADDR);
            let s = Box::leak(Box::new(MenuString { direct: std::ptr::null(), rest: [0; 0x78] }));
            empty(s);
            OurMenuString(s)
        })
        .0
}

unsafe extern "system" fn model_update_hook(model: usize, dt: f32, a: usize, b: usize) {
    unsafe {
        let shown = std::ptr::read((model + MODEL_SHOWN) as *const u8) != 0;
        let text = QUEUE.lock().unwrap().tick(dt, shown);
        if let Some(text) = text {
            let s = menu_string();
            (*s).direct = text;
            let set_text: SetTextFn = std::mem::transmute(SET_TEXT_ADDR);
            set_text(s);
        }
        ORIG_MODEL_UPDATE.get().unwrap()(model, dt, a, b);
    }
}

/// The game's code at `addr`, or another mod's hook on it (a `jmp`, which our detour chains after).
fn check_code(addr: usize, prologue: &[u8]) -> Result<(), String> {
    let actual = unsafe { std::slice::from_raw_parts(addr as *const u8, prologue.len()) };
    if actual != prologue && actual[0] != 0xe9 {
        return Err(format!("unexpected code at {addr:#x} (not AC6 1.09.1?): {actual:02x?}"));
    }
    Ok(())
}

pub fn install() -> Result<(), String> {
    check_code(SET_TEXT_ADDR, &SET_TEXT_PROLOGUE)?;
    check_code(MODEL_UPDATE_ADDR, &MODEL_UPDATE_PROLOGUE)?;
    unsafe {
        let detour = RawDetour::new(MODEL_UPDATE_ADDR as *const (), model_update_hook as *const ())
            .map_err(|e| e.to_string())?;
        let _ = ORIG_MODEL_UPDATE.set(std::mem::transmute::<&(), ModelUpdateFn>(detour.trampoline()));
        detour.enable().map_err(|e| e.to_string())?;
        std::mem::forget(detour);
    }
    Ok(())
}
