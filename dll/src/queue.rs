//! The toast queue, game-free so it can be unit tested. The game hook calls `tick` once per toast-model update and
//! sets whatever text it returns.
//!
//! Timing: the game keeps a toast up for `GAME_HOLD` seconds after its text stops, then fades it out. A toast's
//! duration includes that hold, and the next toast waits until the game reports the previous one hidden, plus
//! `FADE_OUT`, so each toast gets its own fade-in.

use std::collections::VecDeque;

/// Seconds the game keeps showing a toast after its text stops (float at 0x144241500).
const GAME_HOLD: f32 = 1.0;
/// Shortest time a toast's text is set.
const MIN_TEXT_TIME: f32 = 0.1;
/// Seconds left for the FadeOut animation after the game hides a toast, before the next one starts (looked right
/// in game; not measured from the movie).
const FADE_OUT: f32 = 0.4;
/// Ticks a text buffer the game was given stays alive after it's replaced or its toast ends. The game copies our
/// pointer, not the text, and only holds it for about a frame (model+0x08 is cleared every frame, +0x58 is overwritten
/// by the next update). 120 is a generous safety margin over that, at the cost of at most ~120 small buffers.
const RETIRE_TICKS: u64 = 120;
/// Most toasts waiting at once.
const MAX_PENDING: usize = 64;

/// UTF-16, null-terminated (interior nulls become spaces).
pub type Text = Box<[u16]>;

pub fn to_wide(text: &str) -> Text {
    text.encode_utf16().map(|c| if c == 0 { b' ' as u16 } else { c }).chain(std::iter::once(0)).collect()
}

struct Toast {
    id: u64,
    text: Text,
    /// Seconds left to set the text (the game's hold already taken off); None = until dismissed.
    remaining: Option<f32>,
    /// The game was given `text` (it may still read it).
    handed_out: bool,
}

/// After a toast ends, before the next one starts.
enum Gap {
    /// The game still shows the finished toast (its hold).
    GameShowing,
    /// The game hid it; seconds left of the fade-out animation.
    FadingOut(f32),
}

struct Retired {
    #[expect(dead_code, reason = "only kept alive for the game to read")]
    text: Text,
    at_tick: u64,
}

pub struct Queue {
    next_id: u64,
    ticks: u64,
    pending: VecDeque<Toast>,
    active: Option<Toast>,
    gap: Option<Gap>,
    /// Buffers the game was given and may still read, oldest first.
    retired: VecDeque<Retired>,
}

impl Queue {
    pub const fn new() -> Self {
        Queue { next_id: 1, ticks: 0, pending: VecDeque::new(), active: None, gap: None, retired: VecDeque::new() }
    }

    /// Queues a toast on screen for `seconds` (0 or less: until dismissed); returns its id, or 0 when full.
    pub fn show(&mut self, text: Text, seconds: f32) -> u64 {
        if self.pending.len() >= MAX_PENDING {
            return 0;
        }
        let id = self.next_id;
        self.next_id += 1;
        let remaining = (seconds > 0.0).then(|| (seconds - GAME_HOLD).max(MIN_TEXT_TIME));
        self.pending.push_back(Toast { id, text, remaining, handed_out: false });
        id
    }

    /// Replaces a toast's text (in place when it's on screen). False if the id is unknown or finished.
    pub fn update(&mut self, id: u64, text: Text) -> bool {
        let toast = match self.active.as_mut().filter(|a| a.id == id) {
            Some(active) => active,
            None => match self.pending.iter_mut().find(|t| t.id == id) {
                Some(queued) => queued,
                None => return false,
            },
        };
        if toast.text != text {
            let old = std::mem::replace(&mut toast.text, text);
            if std::mem::take(&mut toast.handed_out) {
                self.retired.push_back(Retired { text: old, at_tick: self.ticks });
            }
        }
        true
    }

    /// Ends a toast on screen (the game's hold and fade follow) or drops a queued one.
    pub fn dismiss(&mut self, id: u64) -> bool {
        if self.active.as_ref().is_some_and(|a| a.id == id) {
            self.finish_active();
            return true;
        }
        let before = self.pending.len();
        self.pending.retain(|t| t.id != id);
        self.pending.len() != before
    }

    fn finish_active(&mut self) {
        if let Some(active) = self.active.take() {
            if active.handed_out {
                self.retired.push_back(Retired { text: active.text, at_tick: self.ticks });
            }
            self.gap = Some(Gap::GameShowing);
        }
    }

    /// One toast-model update: `dt` seconds passed, `shown` = the game currently shows a toast. Returns the text to
    /// set this frame (kept alive `RETIRE_TICKS` ticks after it's replaced).
    pub fn tick(&mut self, dt: f32, shown: bool) -> Option<*const u16> {
        self.ticks += 1;
        while self.retired.front().is_some_and(|r| self.ticks - r.at_tick > RETIRE_TICKS) {
            self.retired.pop_front();
        }

        if let Some(remaining) = self.active.as_mut().and_then(|a| a.remaining.as_mut()) {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.finish_active();
            }
        }
        self.gap = match self.gap.take() {
            Some(Gap::GameShowing) if !shown => Some(Gap::FadingOut(FADE_OUT)),
            Some(Gap::FadingOut(left)) if left - dt > 0.0 => Some(Gap::FadingOut(left - dt)),
            Some(Gap::FadingOut(_)) => None,
            other => other,
        };
        if self.active.is_none() && self.gap.is_none() {
            self.active = self.pending.pop_front();
        }
        self.active.as_mut().map(|a| {
            a.handed_out = true;
            a.text.as_ptr()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn text_of(ptr: Option<*const u16>) -> Option<String> {
        ptr.map(|p| unsafe {
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
        })
    }

    /// The game's side: shows a toast while text is set, and for GAME_HOLD seconds after.
    #[derive(Default)]
    struct FakeGame {
        shown: bool,
        since_text: f32,
    }

    impl FakeGame {
        fn step(&mut self, text_set: bool) {
            if text_set {
                (self.shown, self.since_text) = (true, 0.0);
            } else if self.shown {
                self.since_text += FRAME;
                self.shown = self.since_text < GAME_HOLD;
            }
        }
    }

    /// Runs `seconds` of frames; returns the texts set, in order, without repeats.
    fn run(queue: &mut Queue, seconds: f32, game: &mut FakeGame) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..(seconds / FRAME) as usize {
            let text = text_of(queue.tick(FRAME, game.shown));
            game.step(text.is_some());
            if let Some(t) = text.filter(|t| seen.last() != Some(t)) {
                seen.push(t);
            }
        }
        seen
    }

    fn show(q: &mut Queue, text: &str, seconds: f32) -> u64 {
        q.show(to_wide(text), seconds)
    }

    #[test]
    fn plays_in_order_with_gaps() {
        let mut q = Queue::new();
        show(&mut q, "a", 2.0);
        show(&mut q, "b", 2.0);
        let mut game = FakeGame::default();
        // a: text for 1 s, hold 1 s, fade 0.4 s, then b.
        assert_eq!(run(&mut q, 1.5, &mut game), ["a"]);
        assert_eq!(run(&mut q, 0.8, &mut game), Vec::<String>::new());
        assert_eq!(run(&mut q, 1.0, &mut game), ["b"]);
    }

    #[test]
    fn short_toast_still_shows() {
        let mut q = Queue::new();
        show(&mut q, "x", 0.2);
        assert_eq!(text_of(q.tick(FRAME, false)).as_deref(), Some("x"));
    }

    #[test]
    fn sticky_until_dismissed() {
        let mut q = Queue::new();
        let id = show(&mut q, "status", 0.0);
        run(&mut q, 30.0, &mut FakeGame::default());
        assert_eq!(text_of(q.tick(FRAME, true)).as_deref(), Some("status"));
        assert!(q.dismiss(id));
        assert_eq!(q.tick(FRAME, true), None);
        assert!(!q.dismiss(id));
    }

    #[test]
    fn update_active_and_queued() {
        let mut q = Queue::new();
        let a = show(&mut q, "a", 0.0);
        let b = show(&mut q, "b", 3.0);
        q.tick(FRAME, false);
        assert!(q.update(a, to_wide("a2")));
        assert!(q.update(b, to_wide("b2")));
        assert_eq!(text_of(q.tick(FRAME, true)).as_deref(), Some("a2"));
        q.dismiss(a);
        let mut game = FakeGame { shown: true, since_text: 0.0 };
        assert_eq!(run(&mut q, 3.0, &mut game), ["b2"]);
        assert!(!q.update(999, to_wide("nope")));
    }

    #[test]
    fn dismiss_queued() {
        let mut q = Queue::new();
        show(&mut q, "a", 1.5);
        let b = show(&mut q, "b", 1.5);
        show(&mut q, "c", 1.5);
        assert!(q.dismiss(b));
        assert_eq!(run(&mut q, 10.0, &mut FakeGame::default()), ["a", "c"]);
    }

    #[test]
    fn waits_while_game_still_shows() {
        let mut q = Queue::new();
        show(&mut q, "a", 1.1);
        show(&mut q, "b", 2.0);
        q.tick(FRAME, false);
        for _ in 0..30 {
            q.tick(FRAME, true); // "a" finishes after 0.1 s; the game keeps showing it
        }
        assert_eq!(q.tick(FRAME, true), None);
        for _ in 0..20 {
            assert_eq!(q.tick(FRAME, false), None); // fade allowance (0.4 s = 24 frames)
        }
        assert!((0..10).any(|_| q.tick(FRAME, false).is_some()));
    }

    #[test]
    fn handed_out_text_outlives_update() {
        let mut q = Queue::new();
        let id = show(&mut q, "old", 0.0);
        let old = q.tick(FRAME, false).unwrap();
        q.update(id, to_wide("new"));
        for _ in 0..RETIRE_TICKS {
            q.tick(FRAME, true);
        }
        assert_eq!(text_of(Some(old)).as_deref(), Some("old"));
        q.tick(FRAME, true);
        assert!(q.retired.is_empty());
    }

    #[test]
    fn updates_between_ticks_retire_one_buffer() {
        let mut q = Queue::new();
        let id = show(&mut q, "s 0", 0.0);
        q.tick(FRAME, false);
        for i in 1..1000 {
            q.update(id, to_wide(&format!("s {i}"))); // never handed out: freed at once
        }
        assert_eq!(q.retired.len(), 1);
        q.update(id, to_wide("s 999")); // unchanged: nothing to do
        assert_eq!(q.retired.len(), 1);
    }

    #[test]
    fn cap_and_ids() {
        let mut q = Queue::new();
        for i in 0..MAX_PENDING {
            assert_eq!(show(&mut q, "t", 1.0), i as u64 + 1);
        }
        assert_eq!(show(&mut q, "t", 1.0), 0);
    }
}
