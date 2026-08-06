//! Solver dispatch and solving algorithms.

pub mod aog;
pub mod backtrack;
pub mod pieces;
pub mod rose;
pub mod validate;

use crate::types::*;
use std::collections::HashMap;
use std::time::Instant;

pub fn solve(puzzle: &Puzzle, timeout_ms: u64) -> Solution {
    let start = Instant::now();

    let fillable_count: usize = (0..puzzle.height)
        .flat_map(|r| (0..puzzle.width).map(move |c| (r, c)))
        .filter(|&(r, c)| puzzle.cells[r][c].fillable())
        .count();

    if fillable_count == 0 {
        return Solution {
            solved: true,
            steps_taken: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            error_message: None,
            regions: Vec::new(),
            rule_results: Default::default(),
        };
    }

    // `timeout_ms` is a UNIT budget: each of aog / pieces / backtrack gets the
    // full timeout as its own deadline (not a share of it).  The Python side
    // gives the subprocess enough wall-clock (3×) for all three to run.

    // 0. AoG DFS solver first: direct port of the C++ reference solver.
    // For pure rose_window puzzles aog solves most in <1s but can hang for the
    // full budget on "no size constraint" ones — give it a short budget, then
    // hand the rest to the rose solver.
    let rose_capable = is_rose_capable(puzzle);
    let aog_budget = if rose_capable {
        AOG_ROSE_BUDGET_MS.min(timeout_ms)
    } else {
        // Non-rose puzzles get the full unit budget.  A flat 1s cap (previous
        // session's AOG_BUDGET_CAP_MS) regressed ~65 puzzles that aog solves in
        // 1-25s but then hands off to pieces/backtrack, which can't solve them.
        // The aog search is bounded by its deadline thanks to the hot-loop
        // checks (search.rs Fix B/C), so it stops at `timeout_ms` instead of
        // burning the whole subprocess budget; pieces/backtrack still get their
        // own full unit budget afterwards.
        timeout_ms
    };
    if !puzzle.rules.is_empty() {
        let deadline = start + std::time::Duration::from_millis(aog_budget);
        if let Some(regions) = aog::solve_aog(puzzle, deadline) {
            return build_solution_trusted(regions, &start, puzzle);
        }

        // NEW: pure rose_window puzzles aog couldn't solve quickly → rose solver.
        if rose_capable {
            let elapsed = start.elapsed().as_millis() as u64;
            let rose_ms = timeout_ms
                .saturating_sub(elapsed)
                .min(ROSE_TIMEOUT_MS);
            if rose_ms > 0 {
                if let Some(regions) = rose::solve_rose(puzzle, &start, rose_ms) {
                    return build_solution(regions, &start, puzzle);
                }
            }
        }

        if std::env::var("AOG_ONLY").is_ok() {
            let elapsed = start.elapsed().as_millis() as u64;
            return Solution {
                solved: false,
                steps_taken: 0,
                elapsed_ms: elapsed,
                error_message: Some("AoG solver only".into()),
                regions: Vec::new(),
                rule_results: Default::default(),
            };
        }
    }

    // Solver dispatch:
    // 1. shape_pool with non-empty pool → pieces (DLX exact cover)
    // 2. Area-number cell clues → try pieces first, fallback to backtrack
    // 3. Otherwise → backtrack
    let has_shape_pool = !puzzle.shape_pool.is_empty();
    let has_area_clues = has_area_number_clues(puzzle);
    let has_compass_clues = has_constrained_compass(puzzle);

    if has_shape_pool || has_area_clues || has_compass_clues {
        // Try piece-based solver first
        if let Some(regions) = pieces::solve_pieces(puzzle, &start, timeout_ms) {
            if std::env::var("AOG_DEBUG").is_ok() {
                eprintln!("solver=pieces regions={}", regions.len());
            }
            return build_solution(regions, &start, puzzle);
        }
    }

    // Fallback: backtracking solver
    if let Some(regions) = backtrack::solve_backtrack(puzzle, &start, timeout_ms) {
        if std::env::var("AOG_DEBUG").is_ok() {
            eprintln!("solver=backtrack regions={}", regions.len());
        }
        return build_solution(regions, &start, puzzle);
    }

    let elapsed = start.elapsed().as_millis() as u64;
    Solution {
        solved: false,
        steps_taken: 0,
        elapsed_ms: elapsed,
        error_message: Some("No solution found".into()),
        regions: Vec::new(),
        rule_results: Default::default(),
    }
}

/// Every pre-drawn boundary (and every constrained edge) must separate two
/// different regions.  The pieces / backtrack solvers are not boundary-aware,
/// so this is the backstop that rejects a "solution" that crosses a drawn edge.
fn regions_respect_boundaries(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    let h = puzzle.height;
    let w = puzzle.width;
    // Flat row-major cell → region id, index `r*w+c`.
    let mut rid: Vec<Option<usize>> = vec![None; h * w];
    for reg in regions {
        for &[r, c] in &reg.cells {
            rid[r * w + c] = Some(reg.region_id);
        }
    }
    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            if puzzle.h_edges[r][c].is_boundary {
                if let (Some(a), Some(b)) = (rid[r * w + c], rid[r * w + (c + 1)]) {
                    if a == b {
                        if std::env::var("AOG_DEBUG").is_ok() {
                            eprintln!(
                                "boundary-violate h ({},{})-({},{}) same region {}",
                                r, c, r, c + 1, a
                            );
                        }
                        return false;
                    }
                }
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            if puzzle.v_edges[r][c].is_boundary {
                if let (Some(a), Some(b)) = (rid[r * w + c], rid[(r + 1) * w + c]) {
                    if a == b {
                        if std::env::var("AOG_DEBUG").is_ok() {
                            eprintln!(
                                "boundary-violate v ({},{})-({},{}) same region {}",
                                r, c, r + 1, c, a
                            );
                        }
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn build_solution(regions: Vec<RegionInfo>, start: &Instant, puzzle: &Puzzle) -> Solution {
    let elapsed = start.elapsed().as_millis() as u64;
    if !regions_respect_boundaries(puzzle, &regions) {
        return Solution::unsolved("Solution found but crosses a pre-drawn boundary");
    }
    // Full independent re-validation via `validate.rs` — the same acceptance
    // gate aog and rose already use.  It covers all 22 rule types (including
    // fence / compass / ring / rose_window, which `constraints.rs` used to
    // stub out with unconditional `true`), so a fallback solver can no longer
    // smuggle a rule-violating answer past the Rust check.  The router's
    // Python IndependentValidator stays as the outer gate.
    if !crate::solver::validate::validate(puzzle, &regions) {
        return Solution {
            solved: false,
            steps_taken: 0,
            elapsed_ms: elapsed,
            error_message: Some("Solution found but fails rule validation".into()),
            regions,
            rule_results: HashMap::new(),
        };
    }
    let rule_results: HashMap<String, bool> = puzzle
        .rules
        .iter()
        .map(|r| (r.ctype.clone(), true))
        .collect();
    Solution {
        solved: true,
        steps_taken: 0,
        elapsed_ms: elapsed,
        error_message: None,
        regions,
        rule_results,
    }
}

/// Solution builder for the AoG solver, whose internal constraint checks are
/// authoritative (the C++ solver enforces every rule during the search).
fn build_solution_trusted(regions: Vec<RegionInfo>, start: &Instant, puzzle: &Puzzle) -> Solution {
    let elapsed = start.elapsed().as_millis() as u64;
    let rule_results: HashMap<String, bool> = puzzle
        .rules
        .iter()
        .map(|r| (r.ctype.clone(), true))
        .collect();
    Solution {
        solved: true,
        steps_taken: 0,
        elapsed_ms: elapsed,
        error_message: None,
        regions,
        rule_results,
    }
}

/// True for puzzles the rose solver can attempt — mirrors Python
/// `RoseSolver.supports` (rose_window present, and neither `same` nor
/// `different`).  region_match itself bails on shape_pool/puzzle_piece.
fn is_rose_capable(puzzle: &Puzzle) -> bool {
    let has_rose = puzzle.rules.iter().any(|r| r.ctype == "rose_window");
    if !has_rose {
        return false;
    }
    !puzzle.rules.iter().any(|r| r.ctype == "same" || r.ctype == "different")
}

const AOG_ROSE_BUDGET_MS: u64 = 3_000;
const ROSE_TIMEOUT_MS: u64 = 30_000;

fn has_area_number_clues(puzzle: &Puzzle) -> bool {
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if puzzle.cells[r][c].number.is_some() {
                return true;
            }
        }
    }
    false
}

fn has_constrained_compass(puzzle: &Puzzle) -> bool {
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                let spec = [comp.up, comp.down, comp.left, comp.right]
                    .iter().filter(|v| v.is_some()).count();
                let is_strip = (comp.right == Some(0) && comp.left == Some(0))
                    || (comp.up == Some(0) && comp.down == Some(0));
                if spec >= 3 || is_strip {
                    return true;
                }
            }
        }
    }
    false
}
