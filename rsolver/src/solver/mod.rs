//! Solver dispatch and solving algorithms.

pub mod aog;
pub mod backtrack;
pub mod edge_csp;
pub mod fence;
pub mod pieces;
pub mod prototypes;
pub mod rose;
pub mod same_tiling;
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
            attempts: Vec::new(),
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
            attempts: Vec::new(),
        };
    }

    // `timeout_ms` is a UNIT budget: each of aog / edge_csp / pieces / backtrack
    // gets the full timeout as its own deadline (not a share of it).  The Python
    // side gives the subprocess enough wall-clock (`RUST_PARTS`×) for all to run.

    // Per-module attempt trace (doc 23).  Built up as each module is tried; on
    // success it is attached to the returned `Solution`.
    let mut attempts: Vec<SolverAttempt> = Vec::new();

    // Congruent-tiling pre-pass (`same_tiling`): the global `same` rule, the
    // pattern-pinned congruent-remainder clusters, and homogeneous (Gemini)
    // two-region splits.  Cheap and exact when it applies (cyclic isometry
    // search / small-shape DLX, both bounded).  Runs first because aog burns
    // its full unit budget shape-library-exploding on these puzzles (0382:
    // 45s) where this module finishes in milliseconds.  Gated so the other
    // 1200+ puzzles pay nothing and record nothing.
    let st_local_density = puzzle
        .cells
        .iter()
        .flatten()
        .filter(|c| c.shape_pattern.is_some() || c.fence_pattern.is_some())
        .count()
        + puzzle
            .vertices
            .iter()
            .flatten()
            .filter(|v| v.watchtower.is_some())
            .count()
        // Pre-drawn boundaries / constraint edges feed the m=2 parity
        // propagator (0974: 46 precuts and no fence/watchtower at all).
        + puzzle
            .h_edges
            .iter()
            .flatten()
            .filter(|e| e.is_boundary || e.constraint.is_some())
            .count()
        + puzzle
            .v_edges
            .iter()
            .flatten()
            .filter(|e| e.is_boundary || e.constraint.is_some())
            .count();
    if puzzle.rules.iter().any(|r| r.ctype == "same" || r.ctype == "homogeneous")
        || st_local_density > 0
    {
        let st_start = Instant::now();
        let st_deadline = st_start + std::time::Duration::from_millis(timeout_ms);
        let outcome = same_tiling::solve_same_tiling(puzzle, timeout_ms);
        let elapsed = st_start.elapsed().as_millis() as u64;
        match outcome {
            ModuleOutcome::Solved(regions) => {
                attempts.push(SolverAttempt {
                    solver: "same-tiling".into(),
                    status: SolverStatus::Success,
                    elapsed_ms: elapsed,
                    note: None,
                });
                return build_solution(regions, &start, puzzle, "same-tiling", attempts);
            }
            other => {
                record_module_with_elapsed("same-tiling", other, st_deadline, elapsed, &mut attempts)
            }
        }
    }

    // NOTE (2026-09-03): running edge_csp BEFORE aog here (wiring up the
    // previously dead `is_edge_csp_preempt`) was tried and REJECTED - it cost
    // 61 puzzles (1112 -> 1051, 65 regressions, 4 gains) and *increased* the
    // OOM pool from 57 to 75.  edge_csp burns its unit budget on puzzles aog
    // cracks in seconds, and OOMs on some of its own.  Keep aog first.

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
    // Diagnostic: `SKIP_AOG=1` bypasses the aog/rose phases so a single
    // downstream solver can be exercised in isolation (mirrors `AOG_ONLY`).
    let skip_aog = std::env::var("SKIP_AOG").is_ok();
    if !skip_aog && !puzzle.rules.is_empty() {
        let deadline = start + std::time::Duration::from_millis(aog_budget);
        let aog_start = Instant::now();
        let outcome = aog::solve_aog(puzzle, deadline);
        let elapsed = aog_start.elapsed().as_millis() as u64;
        match outcome {
            ModuleOutcome::Solved(regions) => {
                attempts.push(SolverAttempt {
                    solver: "aog".into(),
                    status: SolverStatus::Success,
                    elapsed_ms: elapsed,
                    note: None,
                });
                return build_solution_trusted(regions, &start, puzzle, attempts);
            }
            _ => record_module_with_elapsed("aog", outcome, deadline, elapsed, &mut attempts),
        }
        // fallthrough: aog timed out or found nothing; try the other solvers.

        // NEW: pure rose_window puzzles aog couldn't solve quickly → rose solver.
        if rose_capable {
            // Unit-budget philosophy — every module gets its own full
            // `timeout_ms` from its own start (see `solve_rose`, which anchors
            // internally).  Computing `timeout_ms - aog_elapsed` instead
            // starved rose to 0ms whenever aog overran its own deadline: aog's
            // hot-loop deadline checks overshoot badly (0382: 45s of work
            // against a 20s `AOG_ROSE_BUDGET_MS`), and the rose+same cluster
            // then got `not_attempted` — the one solver that can crack those
            // puzzles never ran.  (The old `.min(ROSE_TIMEOUT_MS)` clamp was
            // removed earlier for the same reason.)
            let rose_deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
            {
                let rose_start = Instant::now();
                let outcome = rose::solve_rose(puzzle, &start, timeout_ms);
                let r_elapsed = rose_start.elapsed().as_millis() as u64;
                match outcome {
                    ModuleOutcome::Solved(regions) => {
                        attempts.push(SolverAttempt {
                            solver: "rose".into(),
                            status: SolverStatus::Success,
                            elapsed_ms: r_elapsed,
                            note: None,
                        });
                        return build_solution(regions, &start, puzzle, "rose", attempts);
                    }
                    _ => record_module_with_elapsed("rose", outcome, rose_deadline, r_elapsed, &mut attempts),
                }
            }
        } else {
            attempts.push(not_attempted("rose", "puzzle is not rose-capable"));
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
                attempts,
            };
        }
    } else {
        // No rules, or `SKIP_AOG=1`: aog / rose never run (they need
        // rule-driven search).
        let note = if skip_aog { "SKIP_AOG" } else { "no rules" };
        attempts.push(not_attempted("aog", note));
        attempts.push(not_attempted("rose", note));
    }

    // Standalone `shape_pattern` pre-pin for `puzzle_piece` puzzles that have no
    // `rose_window` (so `solve_rose_with_pin` never ran above).  `pieces`' DLX
    // cannot model "N pattern regions + one big unconstrained region" — 0976
    // exhausts in 2ms and nothing else in the chain can attempt it.  Cheap when
    // it does not apply (a few dihedral placements per anchor) and validated
    // end-to-end, so a wrong pin can never leak out.
    let has_pp = puzzle.rules.iter().any(|r| r.ctype == "puzzle_piece");
    if has_pp && !rose_capable {
        let pp_start = Instant::now();
        // Full unit budget, like every other module: the placement search on
        // 1215 needs ~36s of a 40s budget, and the wall clock
        // (`timeout × RUST_PARTS × SLACK` = 40 × 4 × 1.2 = 192s) still covers
        // aog(40) + pp-pin(40) + pieces(40) with room for the OOM retry.
        let outcome = rose::puzzle_piece_pin::solve_puzzle_piece_standalone(puzzle, timeout_ms);
        let elapsed = pp_start.elapsed().as_millis() as u64;
        match outcome {
            ModuleOutcome::Solved(regions) => {
                attempts.push(SolverAttempt {
                    solver: "pp-pin".into(),
                    status: SolverStatus::Success,
                    elapsed_ms: elapsed,
                    note: None,
                });
                return build_solution(regions, &start, puzzle, "pp-pin", attempts);
            }
            _ => attempts.push(not_attempted("pp-pin", "no valid single-remainder pin")),
        }
    }

    // Solver dispatch:
    // 1. edge_csp post-fallback for edge-constraint-dense puzzles (ring / brick /
    //    watchtower / compass / inequality / difference) that aog couldn't solve.
    //    Returns a `build_solution`-validated result (non-trusted: the router's
    //    `validate::validate` gate re-checks it), so a wrong answer can't pass.
    if edge_csp::is_edge_csp_capable(puzzle) {
        let ec_deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let ec_start = Instant::now();
        let outcome = edge_csp::solve_edge_csp(puzzle, &start, timeout_ms);
        let elapsed = ec_start.elapsed().as_millis() as u64;
        match outcome {
            ModuleOutcome::Solved(regions) => {
                attempts.push(SolverAttempt {
                    solver: "edge_csp".into(),
                    status: SolverStatus::Success,
                    elapsed_ms: elapsed,
                    note: None,
                });
                return build_solution(regions, &start, puzzle, "edge_csp", attempts);
            }
            _ => record_module_with_elapsed("edge_csp", outcome, ec_deadline, elapsed, &mut attempts),
        }
    } else {
        attempts.push(not_attempted("edge_csp", "puzzle is not edge_csp-capable"));
    }

    // Solver dispatch:
    // 1. shape_pool with non-empty pool → pieces (DLX exact cover)
    // 2. Area-number cell clues → try pieces first, fallback to backtrack
    // 3. Otherwise → backtrack
    let has_shape_pool = !puzzle.shape_pool.is_empty();
    let has_area_clues = has_area_number_clues(puzzle);
    let has_compass_clues = has_constrained_compass(puzzle);

    let has_block = puzzle.rules.iter().any(|r| r.ctype == "block");
    if has_shape_pool || has_area_clues || has_compass_clues || has_block {
        // Try piece-based solver first
        let p_deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let p_start = Instant::now();
        let outcome = pieces::solve_pieces(puzzle, &start, timeout_ms);
        let elapsed = p_start.elapsed().as_millis() as u64;
        match outcome {
            ModuleOutcome::Solved(regions) => {
                if crate::aog_debug_enabled() {
                    eprintln!("solver=pieces regions={}", regions.len());
                }
                attempts.push(SolverAttempt {
                    solver: "pieces".into(),
                    status: SolverStatus::Success,
                    elapsed_ms: elapsed,
                    note: None,
                });
                return build_solution(regions, &start, puzzle, "pieces", attempts);
            }
            _ => record_module_with_elapsed("pieces", outcome, p_deadline, elapsed, &mut attempts),
        }
    } else {
        attempts.push(not_attempted("pieces", "no shape_pool / area / constrained compass / block clues"));
    }

    // Fallback: backtracking solver.
    //
    // Gated OFF by default (`BACKTRACK_ON=1` re-enables).  Measured on the full
    // official corpus (1258 puzzles, `--timeout 40`): backtrack **solves 0
    // puzzles** while hanging on many — `dfs` gets stuck in a loop inside a
    // single call (steps stop advancing), so its wall-clock deadline never fires.
    // Those hangs blow the harness's `RUST_PARTS × timeout × SLACK` wall budget,
    // so the subprocess is killed and *no* attempt trace is recorded (the ~41
    // "Rust solver timed out after 40s" failures), burning ~190s of CPU each and
    // starving the parallel workers of CPU.  Removing it costs nothing measured
    // and removes the hangs; re-enable for research with BACKTRACK_ON=1.
    // Backtrack is the *only* solver that handles rule-less puzzles (aog/rose are
    // skipped, edge_csp/pieces find nothing to engage), and those are trivial for
    // it — no hang risk.  Keep it for that case regardless of the gate.
    let backtrack_enabled =
        std::env::var("BACKTRACK_ON").is_ok() || puzzle.rules.is_empty();
    if backtrack_enabled {
        let b_deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let b_start = Instant::now();
        let outcome = backtrack::solve_backtrack(puzzle, &start, timeout_ms);
        let elapsed = b_start.elapsed().as_millis() as u64;
        match outcome {
            ModuleOutcome::Solved(regions) => {
                if crate::aog_debug_enabled() {
                    eprintln!("solver=backtrack regions={}", regions.len());
                }
                attempts.push(SolverAttempt {
                    solver: "backtrack".into(),
                    status: SolverStatus::Success,
                    elapsed_ms: elapsed,
                    note: None,
                });
                return build_solution(regions, &start, puzzle, "backtrack", attempts);
            }
            _ => record_module_with_elapsed("backtrack", outcome, b_deadline, elapsed, &mut attempts),
        }
    } else {
        attempts.push(not_attempted(
            "backtrack",
            "disabled (solves 0/1258 and hangs past its deadline); BACKTRACK_ON=1 to enable",
        ));
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
        attempts,
    }
}

// ── per-module attempt recording (doc 23) ─────────────────────────────────────

/// Map a non-success `ModuleOutcome` to a `SolverStatus` and push an attempt
/// carrying the caller-computed `elapsed_ms`.  `timeout` vs `exhausted` is split
/// by comparing the wall clock to `deadline` — an approximation (doc 23 §3.4):
/// a module that naturally exhausted right at the deadline is mislabeled
/// `timeout`.  Precise deadline-hit signaling is deferred to the
/// deadline-blindspot cleanup (doc 17).
fn record_module_with_elapsed(
    name: &str,
    outcome: ModuleOutcome,
    deadline: Instant,
    elapsed_ms: u64,
    attempts: &mut Vec<SolverAttempt>,
) {
    let status = match outcome {
        ModuleOutcome::ValidationFailed => SolverStatus::ValidationFailed,
        ModuleOutcome::None => {
            if Instant::now() >= deadline {
                SolverStatus::Timeout
            } else {
                SolverStatus::Exhausted
            }
        }
        ModuleOutcome::Solved(_) => SolverStatus::Success,
    };
    attempts.push(SolverAttempt {
        solver: name.to_string(),
        status,
        elapsed_ms,
        note: None,
    });
}

/// Build a `not_attempted` entry with a short reason.
fn not_attempted(name: &str, reason: &str) -> SolverAttempt {
    SolverAttempt {
        solver: name.to_string(),
        status: SolverStatus::NotAttempted,
        elapsed_ms: 0,
        note: Some(reason.to_string()),
    }
}

/// Every pre-drawn boundary (and every constrained edge) must separate two
/// different regions.  The pieces / backtrack solvers are not boundary-aware,
/// so this is the backstop that rejects a "solution" that crosses a drawn edge.
fn build_solution(
    regions: Vec<RegionInfo>,
    start: &Instant,
    puzzle: &Puzzle,
    solver: &str,
    mut attempts: Vec<SolverAttempt>,
) -> Solution {
    let elapsed = start.elapsed().as_millis() as u64;
    // V3: the pre-drawn boundary check (`regions_respect_boundaries`) was
    // removed — `validate::validate` below already performs the identical
    // check (validate.rs:48-70: a boundary edge whose two cells share a
    // region → reject). Running it twice was redundant O(H·W) work on every
    // successful pieces/rose/backtrack solution. (doc 16 §1 V3.)
    if !crate::solver::validate::validate(puzzle, &regions) {
        // The module returned a candidate that the global validator rejects.
        // Patch the just-pushed `success` attempt → `validation_failed` so the
        // trace tells the truth (the module found something, but it was wrong).
        if let Some(last) = attempts.last_mut() {
            if last.solver == solver && last.status == SolverStatus::Success {
                last.status = SolverStatus::ValidationFailed;
                last.note = Some("candidate failed validate::validate".into());
            }
        }
        return Solution {
            solved: false,
            steps_taken: 0,
            elapsed_ms: elapsed,
            error_message: Some("Solution found but fails rule validation".into()),
            regions,
            rule_results: HashMap::new(),
            solver: solver.to_string(),
            attempts,
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
        attempts,
    }
}

/// Solution builder for the AoG solver.
///
/// H5 (soundness): this used to mark the result solved WITHOUT running
/// `validate::validate`, on the grounds that aog's internal checks are
/// authoritative.  aog was therefore the only solver path whose output the
/// Rust binary never re-validated (rose / pieces / backtrack / edge_csp all go
/// through `build_solution`'s validate gate) — every soundness hole in aog's
/// search (H1–H3) shipped straight through.  Run the same `validate::validate`
/// gate here: defense in depth, and an invalid aog partition now falls through
/// to the next solver in the router instead of being returned as "solved".
fn build_solution_trusted(
    regions: Vec<RegionInfo>,
    start: &Instant,
    puzzle: &Puzzle,
    attempts: Vec<SolverAttempt>,
) -> Solution {
    build_solution(regions, start, puzzle, "aog", attempts)
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
    // M2: symbol bookkeeping uses u64 bitmasks keyed by symbol-type index.
    // A rose_window with >64 distinct symbol types cannot be represented, and
    // silently masking high bits away could accept invalid regions — refuse
    // the puzzle instead (the router falls through to the other solvers).
    if crate::shapes::rose_symbol_types(puzzle).len() > 64 {
        return false;
    }
    !puzzle.rules.iter().any(|r| r.ctype == "same" || r.ctype == "different")
}

/// Cap on aog's budget when the puzzle is rose-capable, so the rose solver gets
/// the remainder (aog hangs on "rose_window without size constraint" puzzles).
///
/// Raised 3s -> 20s: 31 failing puzzles are rose-capable and had aog time out at
/// this very cap (e.g. 0957, which aog solves in 1.7s unloaded but could not fit
/// in 3s under parallel load).  The tradeoff is nearly free because the rose
/// solver only ever solves ~9 puzzles and its slowest success takes 4.9s (0833) —
/// even after aog takes 20s, rose still has ~20s of the 40s unit, far more than
/// it needs.
///
/// Measured on the full corpus: 1111 -> 1120 PASS, +9 and 0 regressions
/// (0213, 0213nopad, 0856, 0957, 0620, 1386 via aog; 0439, 0491, 0445 via
/// edge_csp).  10s and 30s were also tried: 10s gains 6, 30s gains the same 8 as
/// 20s, so 20s is the knee of the curve.
///
/// This only pays off together with the matching fix in `rose::solve_rose`, which
/// now anchors its deadlines to its own start instead of the global one — without
/// it, aog taking 20s left rose's deadline already expired so rose ran 0ms.
const AOG_ROSE_BUDGET_MS: u64 = 20_000;

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
    // Any compass with at least one constrained direction (count >= 0) carries
    // real half-plane information.  The old `spec >= 3 || strip` gate dropped
    // single-/double-direction clues (e.g. 0418's three `left`-only compasses)
    // and kept the whole pieces compass path dark for them.
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                if [comp.up, comp.down, comp.left, comp.right]
                    .iter()
                    .any(|v| v.is_some_and(|x| x >= 0))
                {
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
