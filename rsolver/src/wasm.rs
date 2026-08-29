//! WASM entry point — the single JS-facing function exposed by the browser build.
//!
//! The whole solver is reachable through [`solve`]: pass a puzzle JSON string,
//! get a solution JSON string back. The function never panics across the WASM
//! boundary — parse errors and timeouts surface as a `solved:false` solution
//! object (see [`crate::io::solve_json_line`]).
//!
//! Compiled only for `target_arch = "wasm32"`.

use wasm_bindgen::prelude::*;

/// Default per-puzzle timeout (ms) for the browser. Much shorter than the
/// offline 30s benchmark budget: the browser tab must stay responsive, and the
/// UI runs this in a Web Worker so a slow puzzle never freezes the page.
const WEB_TIMEOUT_MS: u64 = 5_000;

/// Solve one puzzle from its JSON text, returning the solution JSON text.
///
/// Always returns a well-formed solution object (`solved:false` + `error_message`
/// on parse failure / timeout), so the JS caller can `JSON.parse` unconditionally.
///
/// `timeout_ms` overrides the default browser deadline (see [`WEB_TIMEOUT_MS`]);
/// pass `None` to use it.  Cancelling a running solve is done from JS by
/// terminating the Worker that called this (the synchronous DFS can't be
/// interrupted mid-search), so the deadline here is only a backstop.
#[wasm_bindgen]
pub fn solve(puzzle_json: &str, timeout_ms: Option<u64>) -> String {
    let timeout = timeout_ms.unwrap_or(WEB_TIMEOUT_MS);
    crate::solution_to_json_text(&crate::solve_json_line(puzzle_json, timeout))
}
