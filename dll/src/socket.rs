//! Command socket for external tools (feature `socket`): TCP 127.0.0.1:48700, one command per line, one reply line.
//!   show <seconds> <text>   -> id (0 = queue full)
//!   update <id> <text>      -> 1 / 0
//!   dismiss <id>            -> 1 / 0
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
        if writeln!(out, "{}", run(line.trim())).is_err() {
            return;
        }
    }
}

fn run(line: &str) -> String {
    let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
    let (first, text) = rest.split_once(' ').unwrap_or((rest, ""));
    let bad_id = || format!("error: bad id {first:?}");
    match command {
        "show" => match first.parse::<f32>() {
            Ok(seconds) => crate::show(text, seconds).to_string(),
            Err(_) => format!("error: bad seconds {first:?}"),
        },
        "update" => match first.parse::<u64>() {
            Ok(id) => (crate::update(id, text) as u8).to_string(),
            Err(_) => bad_id(),
        },
        "dismiss" => match first.parse::<u64>() {
            Ok(id) => (crate::dismiss(id) as u8).to_string(),
            Err(_) => bad_id(),
        },
        _ => "error: commands are show <seconds> <text>, update <id> <text>, dismiss <id>".into(),
    }
}
