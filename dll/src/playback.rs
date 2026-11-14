//! Custom durations, game-free so it can be unit tested. The game hook reads the banner's play state before and after
//! the game's own update, and `after_update` returns it adjusted for the text on screen.
//!
//! The banner plays each entry through a fixed state machine (same in AC6 and Elden Ring): 1 load, 2 fade in, then
//! either (short text) 5 set the wait timer, 6 wait, or (scrolling text) 3-8 scroll passes, 9 repeat check, then
//! 10 hide, 11 fade out, 12 dequeue, back to 0 (idle). For a text with a duration, the game's own end is pushed out
//! of the way (a very long wait, many scroll passes) and the DLL jumps to 10 once the duration is up, so the game's
//! normal fade-out follows.

use std::collections::VecDeque;

const IDLE: u8 = 0;
const FADE_IN: u8 = 2;
const WAIT: u8 = 6;
const REPEAT_CHECK: u8 = 9;
const HIDE: u8 = 10;
/// Wait timer for short texts with a duration: long enough to never end on its own, short enough that a frame's dt
/// still changes it (the DLL counts time only on frames where the banner moved).
const HOLD_SECONDS: f32 = 3600.0;
/// Scroll passes for scrolling texts with a duration (the game uses 3 in AC6, a param in Elden Ring).
const MANY_PASSES: i32 = 1000;

/// The banner's play-state fields, laid out the same way in both games from the state byte on (offsets below).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Banner {
    pub state: u8,
    /// Seconds left in the current wait.
    pub timer: f32,
    /// Scroll passes left.
    pub passes: i32,
    /// The text is wider than the screen and scrolls.
    pub scrolls: bool,
    /// Current scroll position, in pixels. Only read to tell whether the banner moved: while scrolling, nothing else
    /// changes.
    pub scroll_offset: i32,
}

impl Banner {
    pub const TIMER: usize = 0x4;
    pub const PASSES: usize = 0x8;
    pub const SCROLLS: usize = 0xc;
    pub const SCROLL_OFFSET: usize = 0x10;
}

pub struct Playback {
    /// Durations of the entries in the game's queue (None: the game's own timing), front = the one playing or next.
    /// Kept the same length as the game's queue, so entries the game drops (a cleared queue) drop out here too.
    queued: VecDeque<Option<f32>>,
    /// Seconds the banner has been moving through the front entry.
    elapsed: f32,
}

impl Playback {
    pub const fn new() -> Self {
        Playback { queued: VecDeque::new(), elapsed: 0.0 }
    }

    /// A text was pushed into the game's queue.
    pub fn pushed(&mut self, duration: Option<f32>) {
        self.queued.push_back(duration);
    }

    /// After the game's update: `game_queued` = entries in the game's queue (the playing one included, until the game
    /// dequeues it), `before` / `after` = the banner's fields around the update. Returns the fields to write back.
    pub fn after_update(&mut self, game_queued: usize, before: Banner, after: Banner, dt: f32) -> Banner {
        while self.queued.len() > game_queued {
            self.queued.pop_front();
            self.elapsed = 0.0;
        }
        let mut banner = after;
        let Some(Some(duration)) = self.queued.front().copied() else { return banner };
        if banner.state == IDLE {
            return banner;
        }
        if banner != before {
            self.elapsed += dt;
        }
        if banner.scrolls && before.state < FADE_IN && banner.state >= FADE_IN {
            banner.passes = MANY_PASSES;
        }
        if !banner.scrolls && banner.state == WAIT && before.state != WAIT {
            banner.timer = HOLD_SECONDS;
        }
        if self.elapsed >= duration && (FADE_IN..=REPEAT_CHECK).contains(&banner.state) {
            banner.state = HIDE;
        }
        banner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    /// A simplified short-text run of the game's state machine: load and fade in take a frame each, the wait is
    /// 15 s, then hide, fade out, and dequeue (which shrinks the game's queue).
    fn game_step(b: &mut Banner, game_queued: &mut usize) {
        match b.state {
            IDLE if *game_queued > 0 => b.state = FADE_IN,
            FADE_IN => b.state = 5,
            5 => {
                b.timer = 15.0;
                b.state = WAIT;
            }
            WAIT => {
                b.timer -= FRAME;
                if b.timer <= 0.0 {
                    b.state = 7;
                }
            }
            7..=REPEAT_CHECK => b.state += 1,
            HIDE | 11 => b.state += 1,
            12 => {
                *game_queued -= 1;
                b.state = IDLE;
            }
            _ => {}
        }
    }

    /// Frames until the front entry is dequeued.
    fn run(playback: &mut Playback, game_queued: &mut usize, max_frames: usize) -> usize {
        let mut b = Banner::default();
        let start = *game_queued;
        for frame in 0..max_frames {
            let before = b;
            game_step(&mut b, game_queued);
            b = playback.after_update(*game_queued, before, b, FRAME);
            if *game_queued < start {
                return frame;
            }
        }
        max_frames
    }

    fn queue(durations: &[Option<f32>]) -> (Playback, usize) {
        let mut p = Playback::new();
        durations.iter().for_each(|d| p.pushed(*d));
        (p, durations.len())
    }

    #[test]
    fn game_timing_without_a_duration() {
        let (mut p, mut n) = queue(&[None]);
        let frames = run(&mut p, &mut n, 10_000);
        assert!((890..910).contains(&frames), "{frames}");
    }

    #[test]
    fn shorter_duration_cuts_the_wait() {
        let (mut p, mut n) = queue(&[Some(3.0)]);
        let frames = run(&mut p, &mut n, 10_000);
        assert!((178..192).contains(&frames), "{frames}");
    }

    #[test]
    fn longer_duration_extends_the_wait() {
        let (mut p, mut n) = queue(&[Some(40.0)]);
        let frames = run(&mut p, &mut n, 10_000);
        assert!((2398..2412).contains(&frames), "{frames}");
    }

    #[test]
    fn paused_frames_dont_count() {
        let (mut p, n) = queue(&[Some(1.0)]);
        let before = Banner::default();
        let mut b = p.after_update(n, before, Banner { state: FADE_IN, ..before }, FRAME);
        for _ in 0..600 {
            b = p.after_update(n, b, b, FRAME); // the game didn't run its state machine
        }
        assert_eq!(b.state, FADE_IN);
    }

    #[test]
    fn durations_follow_the_game_queue() {
        let (mut p, mut n) = queue(&[Some(2.0), None]);
        let first = run(&mut p, &mut n, 10_000);
        let second = run(&mut p, &mut n, 10_000);
        assert!(first < 200 && second > 800, "{first} {second}");
    }

    #[test]
    fn a_cleared_game_queue_resyncs() {
        let (mut p, _) = queue(&[Some(2.0), Some(3.0)]);
        let idle = Banner::default();
        p.after_update(0, idle, idle, FRAME); // the game dropped both entries
        p.pushed(None);
        let mut n = 1;
        assert!(run(&mut p, &mut n, 10_000) > 800); // the new entry keeps the game's timing
    }

    #[test]
    fn scrolling_text_gets_many_passes() {
        let (mut p, n) = queue(&[Some(5.0)]);
        let before = Banner { scrolls: true, ..Default::default() };
        let b = p.after_update(n, before, Banner { state: FADE_IN, passes: 3, ..before }, FRAME);
        assert_eq!(b.passes, MANY_PASSES);
    }
}
