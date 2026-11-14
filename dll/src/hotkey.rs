//! Test keys (feature `debug-hotkey`), polled every 50 ms: F7 queues a numbered test text with the game's own timing,
//! F8 one that stays 3 s (press them a few times to see the queue).

use std::time::Duration;

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
    crate::log("debug-hotkey: F7 test text, F8 3-second test text");
    std::thread::spawn(|| {
        let (mut f7_down, mut f8_down) = (false, false);
        let mut count = 0;
        loop {
            if pressed(VK_F7, &mut f7_down) {
                count += 1;
                crate::show(&format!("toast_wrapper test {count}"), None);
            }
            if pressed(VK_F8, &mut f8_down) {
                count += 1;
                crate::show(&format!("toast_wrapper test {count} (3 s)"), Some(3.0));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}
