//! Solver dispatch and solving algorithms.

pub mod aog;
pub mod backtrack;
pub mod edge_csp;
pub mod fence;
pub mod pieces;
pub mod prototypes;
pub mod rose;
pub mod validate;

use crate::types::*;
use std::collections::HashMap;
use crate::clock::Instant;

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
            solver: String::new(),
        };
    }

    // Pre-search: ring / brick constraints may already be violated by pre-drawn
    // and constraint edges alone, regardless of region assignment.
    if !pre_search_topology_check(puzzle) {
        let elapsed = start.elapsed().as_millis() as u64;
        return Solution {
            solved: false,
            steps_taken: 0,
            elapsed_ms: elapsed,
            error_message: Some("Pre-drawn boundaries already violate ring/brick".into()),
            regions: Vec::new(),
            rule_results: Default::default(),
            solver: String::new(),
        };
    }

    // `timeout_ms` is a UNIT budget: each of aog / edge_csp / pieces / backtrack
    // gets the full timeout as its own deadline (not a share of it).  The Python
    // side gives the subprocess enough wall-clock (`RUST_PARTS`×) for all to run.

    // 0. AoG DFS solver first: direct port of the C++ reference solver.
    // For pure rose_window puzzles aog solves most in <1s but can hang for the
    // full budget on "no size constraint" ones — give it a short budget, then
    // hand the rest to the rose solver.
    let rose_capable = is_rose_capable(puzzle);

    // ROSE_ONLY: bypass aog entirely and hand the full unit budget to the rose
    // solver. Used to isolate R1 companion-rule pruning (rose region_match)
    // from the aog 40s wall that normally blocks rose from ever running on
    // rose+companion puzzles. Debug/diagnostic only — not a production path.
    if std::env::var("ROSE_ONLY").is_ok() && rose_capable {
        if let Some(regions) = rose::solve_rose(puzzle, &start, timeout_ms) {
            return build_solution(regions, &start, puzzle, "rose");
        }
        let elapsed = start.elapsed().as_millis() as u64;
        return Solution {
            solved: false,
            steps_taken: 0,
            elapsed_ms: elapsed,
            error_message: Some("rose solver only".into()),
            regions: Vec::new(),
            rule_results: Default::default(),
            solver: "rose".to_string(),
        };
    }
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
        // fallthrough: aog timed out or found nothing; try the other solvers.

        // NEW: pure rose_window puzzles aog couldn't solve quickly → rose solver.
        if rose_capable {
            let elapsed = start.elapsed().as_millis() as u64;
            // `timeout_ms` is the sole ceiling — the old `.min(ROSE_TIMEOUT_MS)`
            // clamp (30s) silently capped rose even when the caller asked for
            // more, so `--timeout 40` never reached a rose-capable puzzle's
            // rose phase.  Dropping it makes the unit-budget philosophy apply
            // uniformly to aog/pieces/backtrack/rose.
            let rose_ms = timeout_ms.saturating_sub(elapsed);
            if rose_ms > 0 {
                if let Some(regions) = rose::solve_rose(puzzle, &start, rose_ms) {
                    return build_solution(regions, &start, puzzle, "rose");
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
                solver: "aog".to_string(),
            };
        }
    }

    // Solver dispatch:
    // 1. edge_csp post-fallback for edge-constraint-dense puzzles (ring / brick /
    //    watchtower / compass / inequality / difference) that aog couldn't solve.
    //    Returns a `build_solution`-validated result (non-trusted: the router's
    //    `validate::validate` gate re-checks it), so a wrong answer can't pass.
    if edge_csp::is_edge_csp_capable(puzzle) {
        if let Some(regions) = edge_csp::solve_edge_csp(puzzle, &start, timeout_ms) {
            return build_solution(regions, &start, puzzle, "edge_csp");
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
            if crate::aog_debug_enabled() {
                eprintln!("solver=pieces regions={}", regions.len());
            }
            return build_solution(regions, &start, puzzle, "pieces");
        }
    }

    // Fallback: backtracking solver
    if let Some(regions) = backtrack::solve_backtrack(puzzle, &start, timeout_ms) {
        if crate::aog_debug_enabled() {
            eprintln!("solver=backtrack regions={}", regions.len());
        }
        return build_solution(regions, &start, puzzle, "backtrack");
    }

    let elapsed = start.elapsed().as_millis() as u64;
    Solution {
        solved: false,
        steps_taken: 0,
        elapsed_ms: elapsed,
        error_message: Some("No solution found".into()),
        regions: Vec::new(),
        rule_results: Default::default(),
        solver: String::new(),
    }
}

/// Every pre-drawn boundary (and every constrained edge) must separate two
/// different regions.  The pieces / backtrack solvers are not boundary-aware,
/// so this is the backstop that rejects a "solution" that crosses a drawn edge.
fn build_solution(regions: Vec<RegionInfo>, start: &Instant, puzzle: &Puzzle, solver: &str) -> Solution {
    let elapsed = start.elapsed().as_millis() as u64;
    // V3: the pre-drawn boundary check (`regions_respect_boundaries`) was
    // removed — `validate::validate` below already performs the identical
    // check (validate.rs:48-70: a boundary edge whose two cells share a
    // region → reject). Running it twice was redundant O(H·W) work on every
    // successful pieces/rose/backtrack solution. (doc 16 §1 V3.)
    if !crate::solver::validate::validate(puzzle, &regions) {
        return Solution {
            solved: false,
            steps_taken: 0,
            elapsed_ms: elapsed,
            error_message: Some("Solution found but fails rule validation".into()),
            regions,
            rule_results: HashMap::new(),
            solver: solver.to_string(),
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
        solver: solver.to_string(),
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
        solver: "aog".to_string(),
    }
}

/// True for puzzles the rose solver can attempt — mirrors Python
/// `RoseSolver.supports` (rose_window present, and neither `same` nor
/// `different`).  `puzzle_piece` is allowed: `solve_rose` pre-resolves
/// `shape_pattern`-pinned regions (see `rose::puzzle_piece_pin`) before
/// running region_match on the remainder.
fn is_rose_capable(puzzle: &Puzzle) -> bool {
    let has_rose = puzzle.rules.iter().any(|r| r.ctype == "rose_window");
    if !has_rose {
        return false;
    }
    !puzzle.rules.iter().any(|r| r.ctype == "same" || r.ctype == "different")
}

const AOG_ROSE_BUDGET_MS: u64 = 3_000;

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

/// Pre-search topology check: ring / brick constraints may already be violated
/// by pre-drawn and constraint edges alone.  O(V) scan catches impossible
/// puzzles before any solver runs.
///
/// For each vertex (vr,vc), the four incident edges are examined.  An edge is a
/// **definite boundary** when:
/// - Exactly one of its two adjacent cells exists in the grid (outer border), or
/// - Both cells exist and the edge is marked `is_boundary` (pre-drawn or
///   constraint-forced by `io.rs`).
///
/// An edge where **neither** adjacent cell exists is outside the grid entirely
/// (e.g. the "top" edge at a grid corner) — it is NOT a region boundary.
fn pre_search_topology_check(puzzle: &Puzzle) -> bool {
    let has_ring = puzzle.rules.iter().any(|r| r.ctype == "ring");
    let has_brick = puzzle.rules.iter().any(|r| r.ctype == "brick");
    if !has_ring && !has_brick {
        return true;
    }
    let h = puzzle.height;
    let w = puzzle.width;

    // Vertex (vr,vc) sits at the corner of four cells:
    //   tl = (vr-1, vc-1),  tr = (vr-1, vc)
    //   bl = (vr  , vc-1),  br = (vr  , vc)
    // Four edges: top(tl-tr), bottom(bl-br), left(tl-bl), right(tr-br).
    for vr in 0..=h {
        for vc in 0..=w {
            let cells = [
                (vr as isize - 1, vc as isize - 1), // tl
                (vr as isize - 1, vc as isize),     // tr
                (vr as isize,     vc as isize - 1), // bl
                (vr as isize,     vc as isize),     // br
            ];
            let edges = [(0usize, 1usize), (2, 3), (0, 2), (1, 3)];

            let hi = h as isize;
            let wi = w as isize;

            let in_bounds = |i: usize| -> bool {
                let (r, c) = cells[i];
                r >= 0 && r < hi && c >= 0 && c < wi
            };

            // h_edges[r][c] has dims [h][w-1]; edge between (r,c) and (r,c+1).
            // v_edges[r][c] has dims [h-1][w]; edge between (r,c) and (r+1,c).
            let is_boundary_edge = |a: usize, b: usize| -> bool {
                let (r1, c1) = cells[a];
                let (r2, c2) = cells[b];
                if r1 == r2 {
                    let minc = c1.min(c2) as usize;
                    puzzle.h_edges[r1 as usize][minc].is_boundary
                } else {
                    let minr = r1.min(r2) as usize;
                    puzzle.v_edges[minr][c1 as usize].is_boundary
                }
            };

            let mut def_boundary = 0usize;
            let mut unknown = 0usize;

            for &(ai, bi) in &edges {
                let a_ok = in_bounds(ai);
                let b_ok = in_bounds(bi);
                match (a_ok, b_ok) {
                    (true, true) => {
                        if is_boundary_edge(ai, bi) {
                            def_boundary += 1;
                        } else {
                            unknown += 1; // internal edge, not pre-drawn → may or may not be boundary
                        }
                    }
                    (false, false) => {
                        // outside grid, not a region boundary
                    }
                    _ => {
                        // one cell exists → outer border boundary (definite)
                        def_boundary += 1;
                    }
                }
            }

            // Only reject when the ring/brick violation is CERTAIN regardless
            // of how the unknown edges resolve:
            // - Ring prohibits degree==3 → reject when def_boundary==3 && unknown==0
            //   (all edges determined, exactly 3).  def_boundary==4 is OK for ring.
            // - Brick prohibits degree==4 → reject when def_boundary>=4
            //   (guaranteed ≥4 in final solution).
            // def_boundary==3 with unknown>0 could become 4 if the unknown edges
            //   become boundaries → OK for ring, so can't reject.
            if has_ring && def_boundary == 3 && unknown == 0 {
                return false;
            }
            if has_brick && def_boundary >= 4 {
                return false;
            }
        }
    }
    true
}
