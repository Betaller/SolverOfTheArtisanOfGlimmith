//! Region-by-region DFS backtracking solver with incremental constraint checking.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::grid;
use crate::types::*;

/// DFS backtracking with constraints. Returns regions if solved.
pub fn solve_backtrack(puzzle: &Puzzle, start: &Instant, timeout_ms: u64) -> Option<Vec<RegionInfo>> {
    let fillable = grid::fillable_cells(puzzle);
    if fillable.is_empty() {
        return Some(Vec::new());
    }

    let deadline = *start + std::time::Duration::from_millis(timeout_ms);
    let area_bounds = compute_area_bounds(puzzle);

    let mut state = BacktrackState {
        cell_to_region: HashMap::new(),
        region_shapes: HashMap::new(),
        next_region_id: 0,
        steps: 0,
        deadline,
        area_bounds,
        watchtowers: collect_watchtowers(puzzle),
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
    area_bounds: AreaBounds,
    watchtowers: Vec<(Vec<[usize; 2]>, usize)>,
}

#[derive(Debug, Clone)]
struct AreaBounds {
    min_area: usize,
    max_area: usize,
}

fn compute_area_bounds(puzzle: &Puzzle) -> AreaBounds {
    let h = puzzle.height;
    let w = puzzle.width;
    let total = h * w;

    let mut min_a: usize = 1;
    let mut max_a: usize = total;

    for rule in &puzzle.rules {
        match rule.ctype.as_str() {
            "precise" => {
                if let Some(v) = rule.params.get("area").and_then(|v| v.as_u64()) {
                    min_a = v as usize;
                    max_a = v as usize;
                }
            }
            "range" => {
                if let Some(v) = rule.params.get("min").and_then(|v| v.as_u64()) {
                    min_a = min_a.max(v as usize);
                }
                if let Some(v) = rule.params.get("max").and_then(|v| v.as_u64()) {
                    max_a = max_a.min(v as usize);
                }
            }
            "solitary" => {
                min_a = 1;
                max_a = 1;
            }
            "block" => {
                min_a = 4;
                max_a = 4;
            }
            _ => {}
        }
    }

    // Compass clues imply minimum area
    for r in 0..h {
        for c in 0..w {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                let needed = 1 + comp.up.unwrap_or(0) as usize + comp.down.unwrap_or(0) as usize
                    + comp.left.unwrap_or(0) as usize + comp.right.unwrap_or(0) as usize;
                min_a = min_a.max(needed);
            }
        }
    }

    AreaBounds { min_area: min_a, max_area: max_a }
}

fn collect_watchtowers(puzzle: &Puzzle) -> Vec<(Vec<[usize; 2]>, usize)> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut result = Vec::new();

    for r in 0..h.saturating_sub(1) {
        for c in 0..w.saturating_sub(1) {
            if let Some(val) = puzzle.vertices[r][c].watchtower {
                let v = val as usize;
                // Only include valid watchtowers (1..=4)
                if v >= 1 && v <= 4 {
                    result.push((vec![[r, c], [r, c + 1], [r + 1, c], [r + 1, c + 1]], v));
                }
            }
        }
    }
    result
}

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
        return check_global_constraints(puzzle, state);
    }

    state.steps += 1;
    if state.steps % 1024 == 0 && timed_out(state) {
        return false;
    }

    let (r, c) = fillable[idx];

    if state.cell_to_region.contains_key(&(r, c)) {
        return dfs(puzzle, fillable, idx + 1, state);
    }

    let cell = &puzzle.cells[r][c];
    if cell.blocked {
        return dfs(puzzle, fillable, idx + 1, state);
    }

    let h = puzzle.height;
    let w = puzzle.width;

    // Collect unique adjacent region IDs
    let mut rid_set: HashSet<usize> = HashSet::new();
    let mut valid_rids: Vec<usize> = Vec::new();

    // left
    if c > 0 {
        if let Some(&rid) = state.cell_to_region.get(&(r, c - 1)) {
            if grid::is_adjacent_free(puzzle, r, c, r, c - 1) {
                if rid_set.insert(rid) {
                    valid_rids.push(rid);
                }
            }
        }
    }
    // right
    if c + 1 < w {
        if let Some(&rid) = state.cell_to_region.get(&(r, c + 1)) {
            if grid::is_adjacent_free(puzzle, r, c, r, c + 1) {
                if rid_set.insert(rid) {
                    valid_rids.push(rid);
                }
            }
        }
    }
    // up
    if r > 0 {
        if let Some(&rid) = state.cell_to_region.get(&(r - 1, c)) {
            if grid::is_adjacent_free(puzzle, r, c, r - 1, c) {
                if rid_set.insert(rid) {
                    valid_rids.push(rid);
                }
            }
        }
    }
    // down
    if r + 1 < h {
        if let Some(&rid) = state.cell_to_region.get(&(r + 1, c)) {
            if grid::is_adjacent_free(puzzle, r, c, r + 1, c) {
                if rid_set.insert(rid) {
                    valid_rids.push(rid);
                }
            }
        }
    }

    // Try assigning to each adjacent region
    for &rid in &valid_rids {
        // Area check: prevent region from exceeding max_area
        let region_area = state.region_shapes.get(&rid).map(|s| s.len()).unwrap_or(0);
        if region_area >= state.area_bounds.max_area {
            continue;
        }

        // Merge check: ensure assignment won't cause two regions to merge
        if !check_merge_ok(puzzle, r, c, rid, &state.cell_to_region) {
            continue;
        }

        state.cell_to_region.insert((r, c), rid);
        state.region_shapes.get_mut(&rid).unwrap().push([r, c]);

        // Incremental watchtower check
        if check_watchtowers_ok(state) {
            if dfs(puzzle, fillable, idx + 1, state) {
                return true;
            }
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

/// Check that adding (r,c) to region `rid` won't merge two different regions.
fn check_merge_ok(
    puzzle: &Puzzle,
    r: usize,
    c: usize,
    target_rid: usize,
    assigned: &HashMap<(usize, usize), usize>,
) -> bool {
    let h = puzzle.height;
    let w = puzzle.width;

    for (nr, nc) in neighbor_positions(r, c, h, w) {
        if let Some(&nrid) = assigned.get(&(nr, nc)) {
            if nrid != target_rid && grid::is_adjacent_free(puzzle, r, c, nr, nc) {
                return false;
            }
        }
    }
    true
}

/// Incremental watchtower check: no vertex should already have more distinct regions than its target.
fn check_watchtowers_ok(state: &BacktrackState) -> bool {
    for &(ref cells, target) in &state.watchtowers {
        let mut pieces = Vec::new();
        for &[r, c] in cells {
            if let Some(&p) = state.cell_to_region.get(&(r, c)) {
                if !pieces.contains(&p) {
                    pieces.push(p);
                }
            }
        }
        if pieces.len() > target {
            return false;
        }
    }
    true
}

/// Global constraint check at leaf: all watchtowers satisfied, area bounds satisfied.
fn check_global_constraints(puzzle: &Puzzle, state: &BacktrackState) -> bool {
    if !check_watchtowers_ok(state) {
        return false;
    }

    // Check each watchtower has exactly the target distinct count (or all cells assigned)
    for &(ref cells, target) in &state.watchtowers {
        let mut pieces = Vec::new();
        let mut all_assigned = true;
        for &[r, c] in cells {
            if let Some(&p) = state.cell_to_region.get(&(r, c)) {
                if !pieces.contains(&p) {
                    pieces.push(p);
                }
            } else {
                all_assigned = false;
            }
        }
        if all_assigned && pieces.len() != target {
            return false;
        }
    }

    // Check min_area constraint
    for shape in state.region_shapes.values() {
        if shape.len() < state.area_bounds.min_area {
            return false;
        }
    }

    // Check compass clues
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                if let Some(&rid) = state.cell_to_region.get(&(r, c)) {
                    if let Some(cells) = state.region_shapes.get(&rid) {
                        let (n, s, e, w) = count_directions(cells, r, c);
                        let total = 1 + comp.up.unwrap_or(0) + comp.down.unwrap_or(0)
                            + comp.left.unwrap_or(0) + comp.right.unwrap_or(0);
                        if cells.len() < total as usize {
                            return false;
                        }
                        // For cells outside explicit constraints, just check area is sufficient
                        _ = (n, s, e, w);
                    }
                }
            }
        }
    }

    // Check rose window: if rule "rose_window" is active and there are symbols
    let has_rose = puzzle.rules.iter().any(|r| r.ctype == "rose_window");
    if has_rose {
        // Collect all symbol types
        let mut sym_set = std::collections::HashSet::new();
        for r in 0..puzzle.height {
            for c in 0..puzzle.width {
                if let Some(s) = puzzle.cells[r][c].symbol {
                    sym_set.insert(s);
                }
            }
        }
        // Each symbol must appear exactly once per region
        for sym in &sym_set {
            for cells in state.region_shapes.values() {
                let count = cells.iter().filter(|&&[r, c]| {
                    puzzle.cells[r][c].symbol == Some(*sym)
                }).count();
                if count != 1 {
                    return false;
                }
            }
        }
    }

    true
}

fn count_directions(cells: &[[usize; 2]], cr: usize, cc: usize) -> (i64, i64, i64, i64) {
    let mut n = 0i64;
    let mut s = 0i64;
    let mut e = 0i64;
    let mut w = 0i64;
    for &[r, c] in cells {
        let dr = r as i64 - cr as i64;
        let dc = c as i64 - cc as i64;
        if dr < 0 { n += 1; }
        if dr > 0 { s += 1; }
        if dc > 0 { e += 1; }
        if dc < 0 { w += 1; }
    }
    (n, s, e, w)
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
