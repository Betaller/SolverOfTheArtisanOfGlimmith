//! Piece-based solver using DLX for shape_pool / polyomino rule puzzles.

use crate::types::*;

/// Solve a puzzle by placing polyomino pieces (shape pool).
/// Returns regions if solved, None otherwise.
#[allow(unused_variables)]
pub fn solve_pieces(
    _puzzle: &Puzzle,
    _timeout_ms: u64,
) -> Option<Vec<RegionInfo>> {
    // TODO: implement DLX-based piece placement with shape pool
    None
}
