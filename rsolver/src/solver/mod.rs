//! Solver dispatch and solving algorithms.

pub mod backtrack;
pub mod pieces;

use crate::types::*;
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
            return build_solution(regions, &start, puzzle);
        }
    }

    // Fallback: backtracking solver
    if let Some(regions) = backtrack::solve_backtrack(puzzle, &start, timeout_ms) {
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

fn build_solution(regions: Vec<RegionInfo>, start: &Instant, puzzle: &Puzzle) -> Solution {
    let elapsed = start.elapsed().as_millis() as u64;
    let rule_results = crate::constraints::check_all(&puzzle.rules, &regions);
    let solved = puzzle.rules.iter().all(|r| rule_results.contains(&r.ctype));

    Solution {
        solved,
        steps_taken: 0,
        elapsed_ms: elapsed,
        error_message: if solved { None } else { Some("Solution found but fails rule validation".into()) },
        regions,
        rule_results: rule_results.into_iter().map(|k| (k, true)).collect(),
    }
}

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
