//! Test key (feature `debug-hotkey`), polled every 50 ms: F7 queues a numbered test text (press it a few times to
//! see the queue).

use std::time::Duration;

unsafe extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
}

const VK_F7: i32 = 0x76;

pub fn start() {
    crate::log("debug-hotkey: F7 test text");
    std::thread::spawn(|| {
        let mut was_down = false;
        let mut count = 0;
        loop {
            let down = unsafe { GetAsyncKeyState(VK_F7) } as u16 & 0x8000 != 0;
            if down && !was_down {
                count += 1;
                crate::show(&format!("toast_wrapper test {count}"));
            }
            was_down = down;
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}
