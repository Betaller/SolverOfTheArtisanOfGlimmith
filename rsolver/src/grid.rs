use crate::types::*;

/// Check if two adjacent cells are connected (not separated by a boundary).
pub fn is_adjacent_free(puzzle: &Puzzle, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
    if r1 == r2 {
        let c = c1.min(c2);
        !puzzle.h_edges[r1][c].is_boundary
    } else {
        let r = r1.min(r2);
        !puzzle.v_edges[r][c1].is_boundary
    }
}

/// All fillable (non-blocked) cells.
pub fn fillable_cells(puzzle: &Puzzle) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if puzzle.cells[r][c].fillable() {
                v.push((r, c));
            }
        }
    }
    v
}
