//! Test keys (feature `debug-hotkey`), polled every 50 ms: F7 queues a numbered 4 s toast (press it a few times to
//! see the queue), F8 starts / stops a status toast whose text changes every second.

use std::time::{Duration, Instant};

unsafe extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
}

const VK_F7: i32 = 0x76;
const VK_F8: i32 = 0x77;

/// True on the poll where `key` goes down.
fn pressed(key: i32, was_down: &mut bool) -> bool {
    let down = unsafe { GetAsyncKeyState(key) } as u16 & 0x8000 != 0;
    let edge = down && !*was_down;
    *was_down = down;
    edge
}

pub fn start() {
    crate::log("debug-hotkey: F7 test toast, F8 status toast");
    std::thread::spawn(|| {
        let (mut f7_down, mut f8_down) = (false, false);
        let mut count = 0;
        let mut status: Option<(u64, Instant)> = None;
        loop {
            if pressed(VK_F7, &mut f7_down) {
                count += 1;
                crate::show(&format!("ac6_toast test toast {count}"), 4.0);
            }
            if pressed(VK_F8, &mut f8_down) {
                status = match status {
                    Some((id, _)) => {
                        crate::dismiss(id);
                        None
                    }
                    None => Some((crate::show("ac6_toast status: 0 s", 0.0), Instant::now())),
                };
            }
            if let Some((id, since)) = status {
                // Same text most polls: the queue ignores unchanged updates.
                crate::update(id, &format!("ac6_toast status: {} s", since.elapsed().as_secs()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}
