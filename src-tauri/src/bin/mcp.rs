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
        let message: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("twister-mcp: unreadable frame: {err}");
                continue;
            }
        };
        if let Some(answer) = session.handle(&message) {
            let _ = writeln!(stdout, "{answer}");
            let _ = stdout.flush();
        }
    }
}
