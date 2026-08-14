//! `rsolver` library crate — the puzzle solver core for "The Artisan of Glimmith".
//!
//! The solver was originally a single stdin/stdout CLI (`main.rs`). It is split
//! into a library crate so the *same* solving core can be linked two ways:
//!
//! - **native CLI** (`main.rs`) — reads puzzle JSON from stdin/argv, writes
//!   solution JSON to stdout (unchanged behavior);
//! - **WebAssembly** ([`wasm`]) — a single `solve(puzzle_json) -> solution_json`
//!   function bound to JS via `wasm-bindgen`, for the browser UI.
//!
//! Both share the exact same solver dispatch ([`solver::solve`]) and JSON I/O
//! boundary ([`io`]), so a solution produced in the browser is identical to one
//! produced on the command line (modulo the per-puzzle timeout).

mod dlx;
mod grid;
pub mod io;
mod polyomino;
mod shapes;
pub mod solver;
mod types;

/// Monotonic clock abstraction: `std::time::Instant` on native, `performance.now()`
/// on `wasm32` (which has no OS clock).  See [`clock`].
pub mod clock;

/// WASM entry point (`solve(puzzle_json) -> solution_json`), compiled only for
/// `target_arch = "wasm32"`.  See [`wasm`].
#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// Public entry points shared by the CLI and the WASM binding.
pub use io::{parse_puzzle, solution_to_json_text, solve_json_line};

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
