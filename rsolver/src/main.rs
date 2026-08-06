//! rsolver — puzzle solver for "The Artisan of Glimmith".
//!
//! Reads puzzle JSON from stdin, writes solution JSON to stdout.
//! Usage: rsolver [{file}] [--batch]
//!   - `rsolver` / `rsolver {file}` : solve one puzzle (single JSON in, solution out).
//!   - `rsolver --batch`            : read one puzzle JSON per line from stdin, write
//!                                    one solution JSON per line to stdout.
//!
//! The JSON model, puzzle building and serialization live in [`io`]; this entry
//! point only reads stdin/argv and writes stdout.

mod constraints;
mod dlx;
mod grid;
mod io;
mod polyomino;
mod shapes;
mod solver;
mod types;

use std::io::{Read, Write};
use std::path::PathBuf;

use io::{parse_puzzle, solve_json_line, solution_to_json_text};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let batch = args.iter().any(|a| a == "--batch");

    let mut input = String::new();

    // Batch mode always reads stdin (one compact puzzle JSON per line).
    let file_arg = if batch {
        None
    } else if args.len() >= 2 {
        Some(&args[1])
    } else {
        None
    };
    if let Some(path) = file_arg {
        // Read from file argument
        let path = PathBuf::from(path);
        std::fs::File::open(&path)
            .unwrap_or_else(|e| {
                eprintln!("Error opening {}: {}", path.display(), e);
                std::process::exit(1);
            })
            .read_to_string(&mut input)
            .unwrap_or_else(|e| {
                eprintln!("Error reading {}: {}", path.display(), e);
                std::process::exit(1);
            });
    } else {
        // Read from stdin
        std::io::stdin()
            .read_to_string(&mut input)
            .unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(1);
            });
    }

    if batch {
        // Line-delimited puzzles in, line-delimited solutions out.  A malformed
        // line emits a `solved:false` line and the batch continues — keeping
        // the 1 input line → 1 output line invariant.  (Blank lines are
        // skipped; the Python caller never sends them.)
        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let output = solution_to_json_text(&solve_json_line(line));
            let _ = std::io::stdout().write_all(output.as_bytes());
            let _ = std::io::stdout().write_all(b"\n");
        }
        return;
    }

    let puzzle = match parse_puzzle(&input) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let solution = solver::solve(&puzzle, 30_000); // 30s timeout

    let output = solution_to_json_text(&solution);
    let _ = std::io::stdout().write_all(output.as_bytes());
    let _ = std::io::stdout().write_all(b"\n");
}
