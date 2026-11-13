//! The game side (Armored Core VI 1.09.1): hands queued texts to the game's own system announce, from the announce
//! window's per-frame update (the game thread). Background: ..\..\docs\how-it-works.md.

use std::sync::OnceLock;

use retour::RawDetour;

type UpdateFn = unsafe extern "system" fn(usize, f32, usize, usize);
type PushFn = unsafe extern "system" fn(usize, *const u16) -> bool;

/// `01_SystemAnnounce` controller update (controller, dt): when the banner is idle, plays the next queued entry.
const UPDATE_ADDR: usize = 0x140fd61a0;
const UPDATE_PROLOGUE: [u8; 8] = [0x40, 0x57, 0x48, 0x83, 0xec, 0x50, 0x48, 0xc7];
/// Queues a copy of a text (model, wchar*) -> false when 9 are already queued.
const PUSH_ADDR: usize = 0x140e79dd0;
const PUSH_PROLOGUE: [u8; 8] = [0x48, 0x89, 0x54, 0x24, 0x10, 0x48, 0x83, 0xec];
/// Controller field: the announce model ([CSMenuMan+0x6f0]).
const CONTROLLER_MODEL: usize = 0x5b8;

static ORIG_UPDATE: OnceLock<UpdateFn> = OnceLock::new();

unsafe extern "system" fn update_hook(controller: usize, dt: f32, a: usize, b: usize) {
    unsafe {
        let model = std::ptr::read((controller + CONTROLLER_MODEL) as *const usize);
        if model != 0 {
            let push: PushFn = std::mem::transmute(PUSH_ADDR);
            crate::drain_pending(|text| push(model, text.as_ptr()));
        }
        ORIG_UPDATE.get().unwrap()(controller, dt, a, b);
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
    check_code(PUSH_ADDR, &PUSH_PROLOGUE)?;
    check_code(UPDATE_ADDR, &UPDATE_PROLOGUE)?;
    unsafe {
        let detour = RawDetour::new(UPDATE_ADDR as *const (), update_hook as *const ()).map_err(|e| e.to_string())?;
        let _ = ORIG_UPDATE.set(std::mem::transmute::<&(), UpdateFn>(detour.trampoline()));
        detour.enable().map_err(|e| e.to_string())?;
        std::mem::forget(detour);
    }
    Ok(())
}
