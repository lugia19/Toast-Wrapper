//! Command socket for external tools (feature `socket`): TCP 127.0.0.1:48700, one command per line, one reply line.
//!   show <text>   -> 1 (queued) / 0 (refused)
//! Anything else gets `error: <why>`. Test client: tools\send_toast.py.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

const ADDRESS: &str = "127.0.0.1:48700";

/// Listens on its own thread (binding initializes Winsock, which must not happen inside DllMain).
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
            Some(("show", text)) => (crate::show(text) as u8).to_string(),
            _ => "error: the command is show <text>".to_string(),
        };
        if writeln!(out, "{reply}").is_err() {
            return;
        }
    }
}
