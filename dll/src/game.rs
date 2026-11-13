//! The game side: hands queued texts to the game's own system announce, from the announce window's per-frame update
//! (the game thread). Armored Core VI and Elden Ring share this system; only the addresses below differ.
//! Background: ..\..\docs\how-it-works.md.

use std::sync::OnceLock;

use retour::RawDetour;

type UpdateFn = unsafe extern "system" fn(usize, f32, usize, usize);
type PushFn = unsafe extern "system" fn(usize, *const u16) -> bool;

/// One supported game build. Addresses are relative to the exe's base (Elden Ring's exe can be moved by ASLR).
pub struct Game {
    pub name: &'static str,
    /// The exe's file name, lowercase.
    exe: &'static str,
    /// Announce window update (window, dt): when the banner is idle, plays the next queued entry.
    update_rva: usize,
    update_prologue: [u8; 8],
    /// Queues a copy of a text (model, wchar*) -> false when 9 are already queued.
    push_rva: usize,
    push_prologue: [u8; 8],
    /// Window field: the announce model.
    window_model: usize,
    /// Leading spaces, so the text doesn't start flush against the screen edge.
    pub padding: usize,
}

const PUSH_PROLOGUE: [u8; 8] = [0x48, 0x89, 0x54, 0x24, 0x10, 0x48, 0x83, 0xec];

const GAMES: [Game; 2] = [
    // AC6 1.09.1: `01_SystemAnnounce` controller; model = [CSMenuMan+0x6f0]. 18 spaces line it up with the key guide.
    Game {
        name: "Armored Core VI 1.09.1",
        exe: "armoredcore6.exe",
        update_rva: 0xfd61a0,
        update_prologue: [0x40, 0x57, 0x48, 0x83, 0xec, 0x50, 0x48, 0xc7],
        push_rva: 0xe79dd0,
        push_prologue: PUSH_PROLOGUE,
        window_model: 0x5b8,
        padding: 18,
    },
    // Elden Ring (Steam build 25827938): CS::FeSystemAnnounceView (vtable slot +0x58); model =
    // [CSMenuManImp+0x860] (CS::FeSystemAnnounceViewModel).
    Game {
        name: "Elden Ring (Steam build 25827938)",
        exe: "eldenring.exe",
        update_rva: 0x8c5e10,
        update_prologue: [0x40, 0x53, 0x48, 0x83, 0xec, 0x30, 0x0f, 0x29],
        push_rva: 0x843000,
        push_prologue: PUSH_PROLOGUE,
        window_model: 0xb00,
        padding: 0,
    },
];

static GAME: OnceLock<&'static Game> = OnceLock::new();
static ORIG_UPDATE: OnceLock<UpdateFn> = OnceLock::new();
static PUSH: OnceLock<PushFn> = OnceLock::new();

unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> usize;
    fn GetModuleFileNameW(module: usize, buffer: *mut u16, size: u32) -> u32;
}

unsafe extern "system" fn update_hook(window: usize, dt: f32, a: usize, b: usize) {
    unsafe {
        let model = std::ptr::read((window + GAME.get().unwrap().window_model) as *const usize);
        if model != 0 {
            let push = PUSH.get().unwrap();
            crate::drain_pending(|text| push(model, text.as_ptr()));
        }
        ORIG_UPDATE.get().unwrap()(window, dt, a, b);
    }
}

/// The game's code at `addr`, or another mod's hook on it (a `jmp`, which our detour chains after).
fn check_code(game: &Game, addr: usize, prologue: &[u8]) -> Result<(), String> {
    let actual = unsafe { std::slice::from_raw_parts(addr as *const u8, prologue.len()) };
    if actual != prologue && actual[0] != 0xe9 {
        return Err(format!("unexpected code at {addr:#x} (not {}?): {actual:02x?}", game.name));
    }
    Ok(())
}

fn exe_name() -> String {
    let mut buffer = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(0, buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    let path = String::from_utf16_lossy(&buffer[..len]);
    path.rsplit(['\\', '/']).next().unwrap_or_default().to_lowercase()
}

/// Detects the game by its exe and hooks it.
pub fn install() -> Result<&'static Game, String> {
    let exe = exe_name();
    let game = GAMES.iter().find(|g| g.exe == exe).ok_or_else(|| format!("unsupported game ({exe})"))?;
    let base = unsafe { GetModuleHandleW(std::ptr::null()) };
    let (update, push) = (base + game.update_rva, base + game.push_rva);
    check_code(game, push, &game.push_prologue)?;
    check_code(game, update, &game.update_prologue)?;
    let _ = GAME.set(game);
    unsafe {
        let _ = PUSH.set(std::mem::transmute::<usize, PushFn>(push));
        let detour = RawDetour::new(update as *const (), update_hook as *const ()).map_err(|e| e.to_string())?;
        let _ = ORIG_UPDATE.set(std::mem::transmute::<&(), UpdateFn>(detour.trampoline()));
        detour.enable().map_err(|e| e.to_string())?;
        std::mem::forget(detour);
    }
    Ok(game)
}

/// Leading spaces for the detected game (0 before `install`).
pub fn padding() -> usize {
    GAME.get().map_or(0, |g| g.padding)
}
