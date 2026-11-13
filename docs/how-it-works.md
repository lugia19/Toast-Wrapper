# How toast_wrapper works

Addresses are for AC6 1.09.1. The full research, with call sites and how each part was found, is in
`research-notes.md`.

## The game's system announce

- `01_SystemAnnounce` is a one-line banner at the bottom of the screen (`SystemAnnounce/Text`). Vanilla uses it for
  server announcements. Elden Ring has the same system (`CS::FeSystemAnnounceView` / `FeSystemAnnounceViewModel`).
- CSMenuMan (`[0x144e15530]`) keeps its **model** at `+0x6f0`: a queue (DLDeque at +0x08, count +0x30) of entries,
  each a flag and a game-owned MenuString.
- `0x140e79dd0(model, const wchar_t*) -> bool` pushes an entry with a copy of the text, unless 9 are already queued.
- The window's controller update `0x140fd61a0(controller, dt)` (model pointer at controller+0x5b8) takes the next
  entry when the banner is idle and plays it: fade in, scroll if wider than the screen, fade out. It only plays while
  `CSMenuMan+0x6c8` is 0, and the window isn't running on the title screen.

## The DLL

- `toast_wrapper_show` copies the text (UTF-8 → UTF-16, 18 spaces of padding, 512 units at most) into a small
  waiting list (64 at most).
- **One hook**, on the controller update: before the original runs, it pushes waiting texts into the game's queue
  until the game reports it full. The game copies each text, so the DLL frees its copy right away.
- Pushing happens on the game thread because the game reads and dequeues the same queue there.
- The earlier design drove `01_SystemToast` (top-left, one text slot cleared every frame) with its own queue and
  timing. It's kept in `research-notes.md` and the `probe-dll\` research probe; the announce banner replaced it
  because it exists in both AC6 and Elden Ring and already queues and plays texts itself.
