//! Custom durations (game-free, unit tested).
//! Banner states (AC6 and ER): 1 load, 2 fade in, short text 5 set timer / 6 wait, scrolling 3-8 passes /
//! 9 repeat check, then 10 hide, 11 fade out, 12 dequeue, 0 idle. With a duration we push the game's own end
//! away (long wait, many passes) and jump to 10 when time is up.

use std::collections::VecDeque;

const IDLE: u8 = 0;
const FADE_IN: u8 = 2;
const WAIT: u8 = 6;
const REPEAT_CHECK: u8 = 9;
const HIDE: u8 = 10;
// Small enough that dt still changes it: elapsed only counts frames where the banner moved.
const HOLD_SECONDS: f32 = 3600.0;
const MANY_PASSES: i32 = 1000; // game: 3 in AC6, a param in ER

/// Same layout in both games, from the state byte.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Banner {
    pub state: u8,
    pub timer: f32,
    pub passes: i32,
    pub scrolls: bool,
    /// Only read to tell whether the banner moved (nothing else changes while scrolling).
    pub scroll_offset: i32,
}

impl Banner {
    pub const TIMER: usize = 0x4;
    pub const PASSES: usize = 0x8;
    pub const SCROLLS: usize = 0xc;
    pub const SCROLL_OFFSET: usize = 0x10;
}

pub struct Playback {
    /// Mirrors the game's queue length, so entries it drops drop here too.
    queued: VecDeque<Option<f32>>,
    elapsed: f32,
}

impl Playback {
    pub const fn new() -> Self {
        Playback { queued: VecDeque::new(), elapsed: 0.0 }
    }

    pub fn pushed(&mut self, duration: Option<f32>) {
        self.queued.push_back(duration);
    }

    /// `game_queued` includes the playing entry. Returns the fields to write back.
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

    /// Simplified short-text state machine, 15 s wait.
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
            b = p.after_update(n, b, b, FRAME); // game paused
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
        p.after_update(0, idle, idle, FRAME); // game dropped both
        p.pushed(None);
        let mut n = 1;
        assert!(run(&mut p, &mut n, 10_000) > 800);
    }

    #[test]
    fn scrolling_text_gets_many_passes() {
        let (mut p, n) = queue(&[Some(5.0)]);
        let before = Banner { scrolls: true, ..Default::default() };
        let b = p.after_update(n, before, Banner { state: FADE_IN, passes: 3, ..before }, FRAME);
        assert_eq!(b.passes, MANY_PASSES);
    }
}
