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

/// All fillable unassigned cells in row-major order.
pub fn unassigned_cells(puzzle: &Puzzle) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            let cell = &puzzle.cells[r][c];
            if cell.fillable() && !cell.assigned() {
                v.push((r, c));
            }
        }
    }
    v
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

/// Connected components of fillable cells (respecting boundaries).
/// Each component is a vector of (row, col) coordinates.
pub fn connected_components(puzzle: &Puzzle) -> Vec<Vec<(usize, usize)>> {
    let fillable: Vec<Vec<bool>> = (0..puzzle.height)
        .map(|r| (0..puzzle.width).map(|c| puzzle.cells[r][c].fillable()).collect())
        .collect();

    let mut visited = vec![vec![false; puzzle.width]; puzzle.height];
    let mut components = Vec::new();

    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if !fillable[r][c] || visited[r][c] {
                continue;
            }

            let mut comp = Vec::new();
            let mut stack = vec![(r, c)];
            visited[r][c] = true;

            while let Some((cr, cc)) = stack.pop() {
                comp.push((cr, cc));
                for (nr, nc) in neighbor_positions(cr, cc, puzzle.height, puzzle.width) {
                    if fillable[nr][nc] && !visited[nr][nc] && is_adjacent_free(puzzle, cr, cc, nr, nc) {
                        visited[nr][nc] = true;
                        stack.push((nr, nc));
                    }
                }
            }
            components.push(comp);
        }
    }
    components
}
