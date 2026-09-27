//! `twister-mcp`: the MCP server binary. Newline-delimited JSON-RPC on
//! stdin/stdout; diagnostics on stderr, because anything else on stdout
//! corrupts the stream.

use std::io::{BufRead, Write};
use std::sync::Arc;

fn main() {
    let Ok(dir) = twister_lib::settings::data_dir() else {
        eprintln!("twister-mcp: no data directory");
        std::process::exit(1);
    };
    let db = match twister_lib::db::Db::open_at(&dir.join(twister_lib::db::DB_FILE)) {
        Ok(db) => Arc::new(db),
        Err(err) => {
            eprintln!("twister-mcp: {err}");
            std::process::exit(1);
        }
    };
    let session = twister_lib::mcp::Session::new(db);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(message) => session.handle(&message),
            Err(err) => {
                eprintln!("twister-mcp: unreadable frame: {err}");
                Some(twister_lib::mcp::parse_error())
            }
        };
        if let Some(answer) = answer {
            // A closed stdout means the host is gone.
            if writeln!(stdout, "{answer}")
                .and_then(|()| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    }
}
