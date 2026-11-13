# How ac6_toast works

Addresses are for AC6 1.09.1. The full research, with call sites and how each part was found, is in `research-notes.md`.

## The game's toast

- `menu\01_systemtoast.gfx` is a single line of text (`ItemList/Text_0`) with FadeIn / Loop / FadeOut labels. C++
  drives it.
- CSMenuMan (`[0x144e15530]`) builds its window at startup and keeps a small **toast model** at `+0x6f8`:
  - +0x08: the requested text, a MenuString;
  - +0x58: a copy of it;
  - +0x98: requested last frame;
  - +0x99: shown;
  - +0x9c: hold timer.
- Every frame, CSMenuMan's update calls the model update `0x140e976b0(model, dt)`, then **clears the text**. A toast
  is therefore up while something sets the text each frame (`0x140dc25e0(MenuString*)`). The game keeps it for
  1.0 s after the text stops (float `0x144241500`), then fades it out. There is no queue.
- The window's controller fades on `*[model+0x48]` (it points at +0x99) and writes the text into Text_0.
- Vanilla only uses it for online Versus Room status (system messages 5000, 6000, 6100, 6200), so offline the slot
  is free.

## Our text in the game's string

A MenuString (0x40 bytes) is laid out as:
- +0x00: a direct `wchar*`;
- +0x08: an owned DLString.

The game's copy keeps the direct pointer as is. So the DLL builds one empty MenuString (constructor `0x140442520`)
and, each frame, points +0x00 at the active toast's UTF-16 text before calling the set-text function.

The game reads that pointer on later frames (+0x08, +0x58, the controller). Text replaced by `update`, or belonging
to a finished toast, is therefore kept for 120 more updates before it's freed.

The same rule means a MenuString the game made (e.g. an FMG text) must not be kept across frames. Its direct pointer
points into message data that the game replaces on load. A probe that kept one showed mission text in the garage.

## The DLL

- **One hook**, on the model update. Before the original runs, it ticks the queue with `dt` and, if a toast is
  active, sets its text.
- **Queue** (`dll\src\queue.rs`, unit tested):
  - The active toast's text is set for its duration minus the game's 1 s hold.
  - The next toast starts once the game reports the model hidden (+0x99 = 0), plus 0.4 s for the FadeOut animation.
  - Sticky toasts (`seconds <= 0`) stay until dismissed.
- **Exports** lock one mutex. Strings are copied in, so callers own nothing afterwards.
