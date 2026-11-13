# How toast_wrapper works

The full research, with call sites and how each part was found, is in `research-notes.md`.

## The game's system announce

Armored Core VI and Elden Ring share a queued one-line banner, the **system announce** (Elden Ring's RTTI names it
`CS::FeSystemAnnounceView` / `CS::FeSystemAnnounceViewModel`; AC6's window is `01_SystemAnnounce`). Vanilla uses it
for server announcements.

- The menu manager keeps its **model**: a queue of entries, each a flag and a game-owned MenuString.
- A push function `(model, const wchar_t*) -> bool` adds an entry with a copy of the text, unless 9 are already
  queued.
- The window's update `(window, dt)` takes the next entry when the banner is idle and plays it: fade in, scroll if
  wider than the screen, fade out. It only plays while a menu-manager field is 0, and the window isn't running on
  the title screens.

| | Armored Core VI 1.09.1 | Elden Ring (Steam build 25827938) |
|---|---|---|
| Push | `0x140e79dd0` | `0x140843000` (RVA `0x843000`) |
| Window update | `0x140fd61a0` (controller) | `0x1408c5e10` (`FeSystemAnnounceView`, vtable slot +0x58) |
| Model | `[CSMenuMan(0x144e15530)+0x6f0]`, window+0x5b8 | `[CSMenuManImp(0x143d72840)+0x860]`, window+0xb00 |
| Play gate | `CSMenuMan+0x6c8 == 0` | `CSMenuManImp+0x798 == 0` |

Elden Ring addresses are for the unrelocated exe; its exe can be moved by ASLR, so the DLL uses offsets from the
exe's base in both games.

## The DLL

- At load it reads the exe's name, picks that game's table in `dll\src\game.rs` (offsets, prologue checks, padding)
  and checks the code there before hooking; anything else is refused and logged.
- `toast_wrapper_show` copies the text (UTF-8 → UTF-16, the game's padding, 512 units at most) into a small waiting
  list (64 at most).
- **One hook**, on the window update: before the original runs, it pushes waiting texts into the game's queue until
  the game reports it full. The game copies each text, so the DLL frees its copy right away.
- Pushing happens on the game thread because the game reads and dequeues the same queue there.
- The earlier design drove AC6's `01_SystemToast` (top-left, one text slot cleared every frame) with its own queue
  and timing. It's kept in `research-notes.md` and the `probe-dll\` research probe; the announce banner replaced it
  because both games have it and it already queues and plays texts itself.
