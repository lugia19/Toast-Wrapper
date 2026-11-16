//! Test socket (feature `socket`), one command per line, client tools\send_toast.py:
//!   show <text>               -> 1 queued / 0 refused
//!   showfor <seconds> <text>  -> same

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

const ADDRESS: &str = "127.0.0.1:48700";

// Own thread: binding initializes Winsock, not allowed in DllMain.
pub fn start() {
    std::thread::spawn(|| {
        let listener = match TcpListener::bind(ADDRESS) {
            Ok(l) => l,
            Err(e) => return crate::log(&format!("socket: can't listen on {ADDRESS}: {e}")),
        };
        crate::log(&format!("socket: listening on {ADDRESS}"));
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream));
        }
    });
}

fn serve(stream: TcpStream) {
    let Ok(mut out) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let reply = match line.trim().split_once(' ') {
            Some(("show", text)) => (crate::show(text, None) as u8).to_string(),
            Some(("showfor", rest)) => match rest.split_once(' ').map(|(s, text)| (s.parse::<f32>(), text)) {
                Some((Ok(seconds), text)) => (crate::show(text, Some(seconds)) as u8).to_string(),
                _ => "error: showfor <seconds> <text>".to_string(),
            },
            _ => "error: the commands are show <text>, showfor <seconds> <text>".to_string(),
        };
        if writeln!(out, "{reply}").is_err() {
            return;
        }
    }
}
