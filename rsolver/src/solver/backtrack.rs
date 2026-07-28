//! Region-by-region DFS backtracking solver.

use std::collections::HashMap;
use std::time::Instant;

use crate::grid;
use crate::types::*;

/// DFS backtracking: assign each fillable cell to a region.
/// Returns the list of regions if a solution is found.
pub fn solve_backtrack(puzzle: &Puzzle, start: &Instant, timeout_ms: u64) -> Option<Vec<RegionInfo>> {
    let fillable = grid::fillable_cells(puzzle);
    if fillable.is_empty() {
        return Some(Vec::new());
    }

    let deadline = *start + std::time::Duration::from_millis(timeout_ms);

    let mut state = BacktrackState {
        cell_to_region: HashMap::new(),
        region_shapes: HashMap::new(),
        next_region_id: 0,
        steps: 0,
        deadline,
        timeout_ms,
        start: *start,
        solved: None,
    };

    if dfs(puzzle, &fillable, 0, &mut state) {
        Some(build_regions(&state))
    } else {
        None
    }
}

struct BacktrackState {
    cell_to_region: HashMap<(usize, usize), usize>,
    region_shapes: HashMap<usize, Vec<[usize; 2]>>,
    next_region_id: usize,
    steps: u64,
    deadline: Instant,
    timeout_ms: u64,
    start: Instant,
    solved: Option<Vec<RegionInfo>>,
}

/// Check timeout.
fn timed_out(state: &BacktrackState) -> bool {
    Instant::now() >= state.deadline
}

fn dfs(
    puzzle: &Puzzle,
    fillable: &[(usize, usize)],
    idx: usize,
    state: &mut BacktrackState,
) -> bool {
    if idx >= fillable.len() {
        return true;
    }

    if state.steps % 1024 == 0 && timed_out(state) {
        return false;
    }
    state.steps += 1;

    let (r, c) = fillable[idx];

    // Already assigned (should not happen with this ordering)
    if state.cell_to_region.contains_key(&(r, c)) {
        return dfs(puzzle, fillable, idx + 1, state);
    }

    let cell = &puzzle.cells[r][c];
    if cell.blocked {
        return dfs(puzzle, fillable, idx + 1, state);
    }

    let h = puzzle.height;
    let w = puzzle.width;

    // Collect adjacent assigned regions (respecting boundaries)
    let mut neighbor_regions: Vec<(usize, usize, usize)> = Vec::new(); // (nr, nc, rid)

    // left
    if c > 0 {
        if let Some(&rid) = state.cell_to_region.get(&(r, c - 1)) {
            if grid::is_adjacent_free(puzzle, r, c, r, c - 1) {
                neighbor_regions.push((r, c - 1, rid));
            }
        }
    }
    // right
    if c + 1 < w {
        if let Some(&rid) = state.cell_to_region.get(&(r, c + 1)) {
            if grid::is_adjacent_free(puzzle, r, c, r, c + 1) {
                neighbor_regions.push((r, c + 1, rid));
            }
        }
    }
    // up
    if r > 0 {
        if let Some(&rid) = state.cell_to_region.get(&(r - 1, c)) {
            if grid::is_adjacent_free(puzzle, r, c, r - 1, c) {
                neighbor_regions.push((r - 1, c, rid));
            }
        }
    }
    // down
    if r + 1 < h {
        if let Some(&rid) = state.cell_to_region.get(&(r + 1, c)) {
            if grid::is_adjacent_free(puzzle, r, c, r + 1, c) {
                neighbor_regions.push((r + 1, c, rid));
            }
        }
    }

    // Deduplicate region IDs (cell might border same region from multiple sides)
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let unique_rids: Vec<usize> = neighbor_regions.iter()
        .map(|&(_, _, rid)| rid)
        .filter(|rid| seen.insert(*rid))
        .collect();

    // Try assigning to each adjacent region
    for rid in &unique_rids {
        let rid = *rid;
        // Check if adding this cell to the region would merge two regions
        if !check_merge_ok(puzzle, r, c, rid, &state.cell_to_region) {
            continue;
        }

        state.cell_to_region.insert((r, c), rid);
        state.region_shapes.get_mut(&rid).unwrap().push([r, c]);

        if dfs(puzzle, fillable, idx + 1, state) {
            return true;
        }

        state.cell_to_region.remove(&(r, c));
        state.region_shapes.get_mut(&rid).unwrap().pop();
    }

    // Start a new region
    let new_rid = state.next_region_id;
    state.next_region_id += 1;
    state.cell_to_region.insert((r, c), new_rid);
    state.region_shapes.insert(new_rid, vec![[r, c]]);

    if dfs(puzzle, fillable, idx + 1, state) {
        return true;
    }

    state.cell_to_region.remove(&(r, c));
    state.region_shapes.remove(&new_rid);
    state.next_region_id -= 1;

    false
}

/// Check that adding cell (r,c) to region `rid` won't merge two separate regions.
fn check_merge_ok(
    puzzle: &Puzzle,
    r: usize,
    c: usize,
    target_rid: usize,
    assigned: &HashMap<(usize, usize), usize>,
) -> bool {
    let h = puzzle.height;
    let w = puzzle.width;

    // For each neighbor of (r,c), if it's assigned to a different region
    // and there's no boundary between them, that's a merge — prohibited.
    for (nr, nc) in neighbor_positions(r, c, h, w) {
        if let Some(&nrid) = assigned.get(&(nr, nc)) {
            if nrid != target_rid && grid::is_adjacent_free(puzzle, r, c, nr, nc) {
                return false;
            }
        }
    }
    true
}

fn build_regions(state: &BacktrackState) -> Vec<RegionInfo> {
    let mut regions: Vec<_> = state
        .region_shapes
        .iter()
        .map(|(&rid, shape)| {
            let mut norm = shape.clone();
            normalize(&mut norm);
            RegionInfo {
                region_id: rid,
                cells: shape.clone(),
                area: shape.len(),
                shape: norm,
                normalized_shape_key: canonical_key(shape),
                matched_shape_name: None,
            }
        })
        .collect();
    regions.sort_by_key(|r| r.region_id);
    regions
}
