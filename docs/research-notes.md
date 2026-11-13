# Research notes: how `01_SystemToast` is driven

Armored Core VI 1.09.1, November 2026. Static analysis was done in Ghidra on a de-Arxaned copy of the exe; runtime
checks used the probe DLL in `probe-dll\`. `docs\how-it-works.md` is the short version; this file keeps the details
and addresses.

## The movie

`menu\01_systemtoast.gfx` (3 KB): one `ItemList` clip with a `Text_0` field (MenuFont_01, 21 px, #cccccc), frame
labels `FadeIn`, `Loop`, `FadeOut`. No ActionScript logic beyond `stop()`: C++ drives it. Siblings worth a look later:
`01_systemannounce.gfx`, `02_tutorial_toast.gfx`, `02_onlinearena_roomannounce.gfx`.

## Static findings (Ghidra)

- **Window**: CSMenuMan (`[0x144e15530]`) builds it in `0x14101b770` (layer 0x1e, name `01_SystemToast`), through
  `0x140cce4e0`, from CSMenuMan's init `0x140cd2e70` and its frame update `0x140cd4290` (when `CSMenuMan+0x715` is
  set). The builder passes `CSMenuMan+0x6f8` (the **toast model**) to the factory.
- **Factory** `0x1439c0bf0` (std::function-style vtable; call slot +0x10 = `0x14101bad0`) allocates a 0x670 controller
  and constructs it with `0x14101b550`: vtable `0x1439c0af8`, show/hide animation proxy at +0x4f0, `ItemList/Text_0`
  text proxy at +0x558, model pointer at +0x668.
- **Controller update** `0x14101b8b0` (vtable slot +0xa0): plays FadeIn/FadeOut on `*[model+0x48]` (`0x140ce2020`),
  and when the model text (+0x08) is non-empty writes it into Text_0 (`0x140c6e1f0`). (The `0x14166bee0` float
  timers in it are Arxan check scaffolding, not logic.)
- **Model** (plain struct, no vtable; zeroed pointer in CSMenuMan's constructor around `0x140ccf690`, freed with
  `0x140ccff90` in the destructor `0x140ccfae0`):
  - +0x08 MenuString: requested text. +0x48 pointer to the show bool (+0x99, confirmed at runtime).
    +0x58 MenuString: copy of the text, made each update.
  - +0x98 requested last frame, +0x99 shown, +0x9c hold timer.
  - Update `0x140e976b0(model, dt)`, called from CSMenuMan's update: requested = text non-empty. Rising: shown = 1.
    Falling: shown stays on until the hold timer reaches 1.0 s (float `0x144241500`), then 0.
  - CSMenuMan's update then **clears the text every frame** (assigns an empty MenuString to model+0x08), so a toast
    lasts only while someone sets the text each frame, plus the 1 s hold.
- **MenuString** (0x40): +0x00 direct `wchar*` (FMG data, or null), +0x08 owned DLString (allocator +0x08, buffer
  +0x10, length +0x20, capacity +0x28, encoding +0x30), +0x38 extra. Copy `0x14044d830` copies the direct pointer as
  is. Empty constructor `0x140442520`. `0x140cb1ec0(out, id)` = System_Message FMG (category 0xcb, MsgRepository
  `[0x144e256d8]`).
- **Set text** `0x140dc25e0(MenuString*)` = copy into `[CSMenuMan+0x6f8]+0x08`. Three callers, all online
  "Versus Room" (arena matchmaking) status:
  - `0x140c65340` (+ its tail `0x140c6540a`, via `0x14635e931`): when the object's state (vfunc +0x50) is 3,
    shows the text from the online singleton `[0x144f2fe20]` (`0x141718d60`: system message 6200 "Cannot join Versus
    Room until you confirm or abandon edits." when its last state is 0x2712, else empty), falling back to 5000
    "Waiting to join Versus Room...". Gate `0x140c66410` (CSMenuMan+0x12, +0x6c8, online checks).
  - `0x141008980`, a menu's update (`0x141007710`, menu object with a MenuString at +0x2158): 6000 "Aborted joining
    Versus Room." (state 1) or 6100 "Cannot join Versus Room without closing dialog." (state 2), then sets whatever
    +0x2158 holds while it's non-empty.
  - `0x140c861af`, inside generic menu controller code (no function in Ghidra): 5000 again.

## Confirmed at runtime (probe, Arxan on)

- `model+0x48` points at `model+0x99` (the shown flag): the controller fades on the model's own latch.
- System message 5000 set every frame from the model-update hook: the toast shows top-left, a dark gradient strip
  with the text, even over the title screen's dialog (`screens\stock_5000.png`).
- An empty MenuString with its direct pointer at our own UTF-16 buffer: our text shows the same way
  (`screens\custom_text.png`). Any text works, no FMG edit needed.
- Stop setting the text: requested 0 at once, shown stays 1 for exactly 1.0 s, then FadeOut and gone.
- Model dump at boot: +0x08 and +0x58 MenuStrings start with a direct pointer to a static empty string
  (`0x1442344b8`), allocator `0x144fc6ae0`, capacity 7; `model+0x00` points at `model+0x50` (unknown).
- **Never keep a game-made MenuString past its frame.** Its direct pointer goes into FMG data that the game
  replaces on load: a stock string built at the title screen showed "troy the energy refinery plant" (mission text)
  in the garage. The probe fetches it every frame (and frees it like the game does).
- Long text wraps to a second line at about 85 characters, below the strip. Only glyphs in the menu font render.
- Not seen: a natural toast (needs an online Versus Room join), and toasts in missions / AC TEST (the window
  survives a mission load, so they most likely work).

## Probe

`probe-dll\` (`cargo build --release`), loaded by `probe.me3` (alt save `SystemToast.sl2`). It hooks the toast
model update, toast_wrapper the announce controller update, so they can be loaded together. Log `Game\debug-logs\system_toast_probe.log`: every text set (with callers,
on change), model state changes, a model dump on first update. F7 = custom toast (our own UTF-16 text through the
MenuString direct pointer), F8 = stock toast (system message 5000), F9 = model dump, F10 = push a system announce.

## System announce: the system both games share (2026-11-13)

Elden Ring has no `01_SystemToast`, but both games have the same queued **system announce** (ER RTTI names
`CS::FeSystemAnnounceViewModel` / `CS::FeSystemAnnounceView`; fromsoftware-rs documents ER's layout and play states).

| | Elden Ring (Steam build 25827938) | AC6 1.09.1 |
|---|---|---|
| Push `(model, const wchar_t*) -> bool` | `0x140843000` | `0x140e79dd0` |
| Model | `[CSMenuManImp(0x143d72840)+0x860]` | `[CSMenuMan(0x144e15530)+0x6f0]` |
| Queue (DLDeque) / count | model+0x10 / +0x38 | model+0x08 / +0x30 |
| Window, controller | `FeSystemAnnounceView` (0xb68; model +0xb00, state +0xb50) | `01_SystemAnnounce` (factory `0x1438fd7b0`, ctor `0x140fd5ed0`, 0x628; model +0x5b8, state +0x610), binds `SystemAnnounce/Text` |
| Vanilla caller | `0x1401cca70`: server announcement strings (UTF-8 -> UTF-16) | `0x1403c1870`, same shape |

- Push copies the text into a new entry's MenuString (game-owned) and refuses when 9 are queued (`count + 1 < 10`).
  The view plays them one by one (fade in, scroll if wider than the screen, fade out, dequeue).
- AC6 runtime (probe F10, Arxan on): push returns 1. At the title screen nothing plays and the queue only grows
  (the window isn't running there); in the garage the queued entries play: an orange line in a full-width strip at
  the bottom, just above the key guide. The test line (95 characters) fit without scrolling.
- AC6 in a mission: plays fine too (checked by lugia19). The text starts flush against the left screen edge (the
  view sets the text field's x itself, for scrolling), so toast_wrapper adds 18 leading spaces, which line it up
  with the key guide.
- toast_wrapper (this repo's DLL, from v0.2.0) uses this system: it hooks the controller update `0x140fd61a0`
  (controller, dt) and pushes waiting texts from there (game thread). Each short entry stays up about 13 s; a
  150-character line still fit without scrolling. The controller only plays while `CSMenuMan+0x6c8` is 0.
- Not yet checked: whether an entry can be changed or removed while it plays.

## Elden Ring (2026-11-13, toast_wrapper v0.3.0)

- Window update: `FeSystemAnnounceView` vtable (`0x142aecc28`) slot +0x58 = `0x1408c5e10(view, dt)`, called by the
  generic update at slot +0x10 (`0x140746870`). Same shape as AC6's: when `[CSMenuManImp+0x798]` is 0 and the view
  is idle (+0xb10 == 0) with entries queued ([view+0xb00]+0x38), it takes the front one (`0x140842fa0`) and plays it.
- ER's exe has the dynamic-base flag (DllCharacteristics 0x8160; AC6's is 0x8120, fixed at 0x140000000), so the DLL
  uses offsets from the exe base in both games.
- Runtime (Arxan on, game 1.17.2 = the analyzed Steam build 25827938): hooked, pushes play. The banner is a strip
  near the top under the compass, already with a left margin (no padding). An entry stays up about 15 s. Nothing
  plays on the title screen. The alt save (me3 `savefile`) loaded the real save's copy; the real `ER0000.sl2` was
  backed up first as `ER0000.sl2.before-toast-wrapper-2026-11-13`.

## Open questions

- How the tutorial toast / room announce movies are driven.
