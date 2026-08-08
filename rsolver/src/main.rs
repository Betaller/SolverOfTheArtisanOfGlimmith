//! rsolver — puzzle solver for "The Artisan of Glimmith".
//!
//! Reads puzzle JSON from stdin, writes solution JSON to stdout.
//! Usage: rsolver [{file}] [--batch]
//!   - `rsolver` / `rsolver {file}` : solve one puzzle (single JSON in, solution out).
//!   - `rsolver --batch`            : read one puzzle JSON per line from stdin, write
//!                                    one solution JSON per line to stdout.
//!
//! Per-puzzle timeout is controlled by the `RSOLVER_TIMEOUT_MS` env var
//! (milliseconds, default 30 000).  It is a *unit budget*: each of aog /
//! pieces / backtrack / rose receives the full amount as its own deadline.
//! The Python `RustSolver` wrapper sets this env var from its `timeout`
//! argument so `--timeout` actually reaches the Rust search.  Values below
//! `MIN_TIMEOUT_MS` (1 000) are clamped up with a warning, so a stray
//! `RSOLVER_TIMEOUT_MS=0` cannot silently turn every puzzle into an instant
//! timeout.
//!
//! The JSON model, puzzle building and serialization live in [`io`]; this entry
//! point only reads stdin/argv and writes stdout.

mod dlx;
mod grid;
mod io;
mod polyomino;
mod shapes;
mod solver;
mod types;

use std::io::{Read, Write};
use std::path::PathBuf;

/// Cached `AOG_DEBUG` env-var check. The raw `std::env::var("AOG_DEBUG").is_ok()`
/// is a 50-100ns syscall + `OsString` heap allocation, called on hot DFS paths
/// (search.rs / core.rs / region_match.rs — 21 sites). This caches the result
/// in a `OnceLock` after the first read, so every subsequent call is a single
/// `AtomicUsize` load (~1ns). The env var cannot change mid-process, so caching
/// is sound. (White-捡 W1, doc 15 §1.)
pub fn aog_debug_enabled() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| std::env::var("AOG_DEBUG").is_ok())
}

use io::{parse_puzzle, solve_json_line, solution_to_json_text};

/// Default per-puzzle unit budget (ms) when `RSOLVER_TIMEOUT_MS` is unset or
/// unparseable.  Matches the historical hardcoded value.
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Floor for the resolved timeout — a stray `RSOLVER_TIMEOUT_MS=0` would
/// otherwise make every puzzle an instant timeout with no clear signal.
const MIN_TIMEOUT_MS: u64 = 1_000;

/// Read the per-puzzle timeout (ms) from `RSOLVER_TIMEOUT_MS`, falling back to
/// `DEFAULT_TIMEOUT_MS` on missing/unparseable input, and clamping up to
/// `MIN_TIMEOUT_MS`.  Used by both single-puzzle and `--batch` modes so the
/// Python caller's `--timeout` is honored end-to-end (was hardcoded 30s).
fn resolve_timeout_ms() -> u64 {
    match std::env::var("RSOLVER_TIMEOUT_MS") {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(ms) if ms < MIN_TIMEOUT_MS => {
                eprintln!(
                    "RSOLVER_TIMEOUT_MS={ms} below floor {MIN_TIMEOUT_MS}; clamping up"
                );
                MIN_TIMEOUT_MS
            }
            Ok(ms) => ms,
            Err(_) => DEFAULT_TIMEOUT_MS,
        },
        Err(_) => DEFAULT_TIMEOUT_MS,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let batch = args.iter().any(|a| a == "--batch");
    let timeout_ms = resolve_timeout_ms();

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
        // skipped; the Python caller never sends them.)  Each line is solved
        // with the same `timeout_ms` unit budget — `solve_json_line` honors it
        // via `solver::solve`'s deadline checks, so one runaway line can no
        // longer blow past the Python caller's per-puzzle budget (the old
        // hardcoded 30s × 3 parts = 90s cascading-timeout bug).
        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let output = solution_to_json_text(&solve_json_line(line, timeout_ms));
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
    let solution = solver::solve(&puzzle, timeout_ms);

    let output = solution_to_json_text(&solution);
    let _ = std::io::stdout().write_all(output.as_bytes());
    let _ = std::io::stdout().write_all(b"\n");
}

#[cfg(test)]
mod tests {
    use super::{resolve_timeout_ms, DEFAULT_TIMEOUT_MS, MIN_TIMEOUT_MS};

    /// Helper: run `resolve_timeout_ms` with a given env value set (or cleared).
    fn with_env(value: Option<&str>) -> u64 {
        // SAFETY on env mutation in tests: std::env::set_var/remove_var are
        // process-global; these tests are single-threaded and the value is
        // restored/cleared afterwards.  Other test threads must not call
        // resolve_timeout_ms concurrently (none do — it lives in main.rs).
        unsafe {
            match value {
                Some(v) => std::env::set_var("RSOLVER_TIMEOUT_MS", v),
                None => std::env::remove_var("RSOLVER_TIMEOUT_MS"),
            }
            let result = resolve_timeout_ms();
            std::env::remove_var("RSOLVER_TIMEOUT_MS");
            result
        }
    }

    #[test]
    fn default_when_unset() {
        assert_eq!(with_env(None), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn parses_valid_value() {
        assert_eq!(with_env(Some("40000")), 40_000);
    }

    #[test]
    fn parses_with_whitespace() {
        assert_eq!(with_env(Some("  25000  ")), 25_000);
    }

    #[test]
    fn falls_back_on_garbage() {
        assert_eq!(with_env(Some("abc")), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn falls_back_on_empty() {
        assert_eq!(with_env(Some("")), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn falls_back_on_negative() {
        // u64 parse fails on "-1" → default (not a wrap-around).
        assert_eq!(with_env(Some("-1")), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn clamps_zero_to_floor() {
        assert_eq!(with_env(Some("0")), MIN_TIMEOUT_MS);
    }

    #[test]
    fn clamps_small_to_floor() {
        assert_eq!(with_env(Some("500")), MIN_TIMEOUT_MS);
    }
}
