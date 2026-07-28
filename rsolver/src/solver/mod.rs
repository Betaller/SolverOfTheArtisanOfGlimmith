//! Solver dispatch and solving algorithms.

pub mod backtrack;
pub mod pieces;

use crate::types::*;
use std::time::Instant;

pub fn solve(puzzle: &Puzzle, timeout_ms: u64) -> Solution {
    let start = Instant::now();

    // Quick validation: check that fillable cells count is manageable
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

    // For now, always use backtracking solver.
    // Future: dispatch to piece-based DLX for shape_pool / polyomino puzzles.
    let result = backtrack::solve_backtrack(puzzle, &start, timeout_ms);

    let elapsed = start.elapsed().as_millis() as u64;

    if let Some(regions) = result {
        Solution {
            solved: true,
            steps_taken: 0, // FIXME: track steps
            elapsed_ms: elapsed,
            error_message: None,
            regions,
            rule_results: Default::default(),
        }
    } else {
        Solution {
            solved: false,
            steps_taken: 0,
            elapsed_ms: elapsed,
            error_message: Some("No solution found".into()),
            regions: Vec::new(),
            rule_results: Default::default(),
        }
    }
}
