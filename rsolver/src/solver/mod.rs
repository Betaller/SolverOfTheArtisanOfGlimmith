//! Solver dispatch and solving algorithms.

pub mod backtrack;
pub mod pieces;

use crate::types::*;
use std::time::Instant;

pub fn solve(puzzle: &Puzzle, timeout_ms: u64) -> Solution {
    let start = Instant::now();

    // Quick validation
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

    // Decide solver mode:
    // - shape_pool / puzzle_piece with non-empty shape pool → DLX pieces
    // - Otherwise → backtrack
    let use_pieces = !puzzle.shape_pool.is_empty()
        && puzzle.rules.iter().any(|r| r.ctype == "shape_pool" || r.ctype == "puzzle_piece");

    let result = if use_pieces {
        pieces::solve_pieces(puzzle, timeout_ms)
    } else {
        backtrack::solve_backtrack(puzzle, &start, timeout_ms)
    };

    let elapsed = start.elapsed().as_millis() as u64;

    if let Some(regions) = result {
        // Post-validation: check all rules
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
