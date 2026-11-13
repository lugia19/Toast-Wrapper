# Toast-Wrapper (toast_wrapper.dll)

One-line notifications for Armored Core VI mods (1.09.1, loaded with [me3](https://github.com/garyttierney/me3)).
They play in the game's own system announce banner, a strip at the bottom of the screen, with its fade in and out.
One DLL owns the banner; any number of mods call it.

![A text in the announce banner, in the garage](docs/screens/announce_garage.png)

## Using it

1. Build: `cd dll && cargo build --release` → `dll\target\release\toast_wrapper.dll` (or take it from the releases).
2. Add it to your me3 profile:
   ```toml
   [[natives]]
   path = '...\toast_wrapper.dll'
   ```
3. In a Rust mod, copy `client\toast_wrapper_client.rs` into your crate:
   ```rust
   mod toast_wrapper_client;

   toast_wrapper_client::show("Saved");
   ```
   The client finds the DLL when it's called, so load order doesn't matter. Without the DLL, `show` does nothing and
   returns false.

Other languages call the exports (UTF-8 text, copied on call; thread-safe; callable before the game is up):

```c
uint32_t toast_wrapper_api_version(void);                  // 2
bool     toast_wrapper_show(const char *text, size_t len); // false = not hooked (wrong game version) or 64 waiting
```

`bool` is C's 1-byte `_Bool`; declare it that way in ctypes / C#. An export's signature never changes: new features
get new exports and a higher API version, so check `toast_wrapper_api_version() >=` the version you wrote against
(the Rust client does).

## Behaviour and limits

- Texts play one after another, in order. The game decides how long each stays up: about 13 s for a short line.
- The banner plays in the garage and in missions, not on the title screen: texts sent there wait and play later.
- Text gets 18 leading spaces, which line it up with the key guide below it. A line wider than the screen
  scrolls; about 150 characters still fit.
- Only characters in the game's menu font render (accented Latin works; symbols like ✓ show as a box).
- No durations, updating or dismissing: the game owns the banner's timing.

## Testing

Build `dll\` with `cargo build --release --features debug-hotkey,socket` and `tests\client-test\` with
`cargo build --release`, then `me3 launch -p toast-wrapper-test.me3`:
- F7: test text.
- `python tools\send_toast.py hello`, through 127.0.0.1:48700.

The profile also loads `tests\client-test` ("client OK" after 20 s). `cargo test` in `dll\` checks the hand-off to
the game's queue. Log: `Game\debug-logs\toast_wrapper.log`.

How it works, and the research behind it: `docs\how-it-works.md`, `docs\research-notes.md`.
