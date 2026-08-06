//! Region-by-region DFS backtracking solver with incremental constraint checking.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::grid;
use crate::types::*;

/// DFS backtracking with constraints. Returns regions if solved.
pub fn solve_backtrack(puzzle: &Puzzle, _start: &Instant, timeout_ms: u64) -> Option<Vec<RegionInfo>> {
    let fillable = grid::fillable_cells(puzzle);
    if fillable.is_empty() {
        return Some(Vec::new());
    }

    // Budget relative to this solver's own start (the caller passes an equal
    // share), not the original solve() start, so backtrack gets its slice even
    // when AoG / pieces consumed theirs.
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let area_bounds = compute_area_bounds(puzzle);

    // Row-major index of every fillable cell (blocked = usize::MAX).
    let h = puzzle.height;
    let w = puzzle.width;
    let mut cell_index = vec![vec![usize::MAX; w]; h];
    for (i, &(r, c)) in fillable.iter().enumerate() {
        cell_index[r][c] = i;
    }
    let has_area_rule = puzzle.rules.iter().any(|r| r.ctype == "area");

    let mut state = BacktrackState {
        cell_to_region: vec![None; h * w],
        region_shapes: Vec::new(),
        next_region_id: 0,
        width: w,
        steps: 0,
        deadline,
        area_bounds,
        watchtowers: collect_watchtowers(puzzle),
        fillable: fillable.clone(),
        cell_index,
        undecided_count: fillable.len(),
        region_clue: HashMap::new(),
        frontier: HashMap::new(),
        has_area_rule,
    };

    if std::env::var("AOG_DEBUG").is_ok() { eprintln!("backtrack: start undecided={}", state.undecided_count); }
    if dfs(puzzle, &mut state) {
        Some(build_regions(&state))
    } else {
        None
    }
}

struct BacktrackState {
    /// Flat row-major cell → region id (index `r*w+c`); unassigned / blocked = None.
    cell_to_region: Vec<Option<usize>>,
    /// Region id → cell list; region ids are a contiguous 0..n prefix, so the
    /// region id is the Vec index (new regions are pushed, undone regions popped).
    region_shapes: Vec<Vec<[usize; 2]>>,
    next_region_id: usize,
    /// Grid width (stride for `cell_to_region` row-major indexing).
    width: usize,
    steps: u64,
    deadline: Instant,
    area_bounds: AreaBounds,
    watchtowers: Vec<(Vec<[usize; 2]>, usize)>,
    // Area-clue machinery (zero overhead when the puzzle has no `area` rule).
    fillable: Vec<(usize, usize)>,
    cell_index: Vec<Vec<usize>>,
    undecided_count: usize,
    region_clue: HashMap<usize, usize>, // rid -> required area from a numbered cell inside
    frontier: HashMap<usize, HashMap<(usize, usize), usize>>, // rid -> {undecided cell : adjacency count}
    has_area_rule: bool,
}

#[derive(Debug, Clone)]
struct AreaBounds {
    min_area: usize,
    max_area: usize,
}

fn compute_area_bounds(puzzle: &Puzzle) -> AreaBounds {
    // Shared helper: precise/range rules + compass-derived minimum.
    let (min_area, max_area) = crate::shapes::area_bounds(puzzle);
    AreaBounds { min_area, max_area }
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

fn dfs(puzzle: &Puzzle, state: &mut BacktrackState) -> bool {
    if state.undecided_count == 0 {
        return check_global_constraints(puzzle, state);
    }

    state.steps += 1;
    if state.steps % 1024 == 0 && timed_out(state) {
        return false;
    }

    // Area lower bound: each clue-region must be able to reach its target size.
    if !check_area_lower_bounds(state) {
        return false;
    }

    let (r, c) = pick_next_cell(puzzle, state);
    let cell = &puzzle.cells[r][c];
    let h = puzzle.height;
    let w = puzzle.width;

    // Collect unique adjacent region IDs
    let mut rid_set: HashSet<usize> = HashSet::new();
    let mut valid_rids: Vec<usize> = Vec::new();

    if c > 0 {
        if let Some(rid) = state.cell_to_region[r * w + (c - 1)] {
            if grid::is_adjacent_free(puzzle, r, c, r, c - 1) && rid_set.insert(rid) {
                valid_rids.push(rid);
            }
        }
    }
    if c + 1 < w {
        if let Some(rid) = state.cell_to_region[r * w + (c + 1)] {
            if grid::is_adjacent_free(puzzle, r, c, r, c + 1) && rid_set.insert(rid) {
                valid_rids.push(rid);
            }
        }
    }
    if r > 0 {
        if let Some(rid) = state.cell_to_region[(r - 1) * w + c] {
            if grid::is_adjacent_free(puzzle, r, c, r - 1, c) && rid_set.insert(rid) {
                valid_rids.push(rid);
            }
        }
    }
    if r + 1 < h {
        if let Some(rid) = state.cell_to_region[(r + 1) * w + c] {
            if grid::is_adjacent_free(puzzle, r, c, r + 1, c) && rid_set.insert(rid) {
                valid_rids.push(rid);
            }
        }
    }

    // Try assigning to each adjacent region
    for &rid in &valid_rids {
        let region_area = state.region_shapes.get(rid).map(|s| s.len()).unwrap_or(0);
        if region_area >= state.area_bounds.max_area {
            continue;
        }

        let boundary_conflict = neighbor_positions(r, c, h, w).iter().any(|&(nr, nc)| {
            state.cell_to_region[nr * w + nc] == Some(rid)
                && !grid::is_adjacent_free(puzzle, r, c, nr, nc)
        });
        if boundary_conflict {
            continue;
        }

        // area-number upper bound + clue conflict (a region can't hold two
        // numbered cells with different targets).
        let new_area = region_area + 1;
        let mut area_clue_ok = true;
        let prev_clue = state.region_clue.get(&rid).copied();
        if let Some(n) = cell.number {
            if new_area > n as usize {
                area_clue_ok = false;
            } else if let Some(m) = prev_clue {
                if m != n as usize {
                    area_clue_ok = false;
                }
            }
        }
        if area_clue_ok {
            if let Some(shape) = state.region_shapes.get(rid) {
                for &[rr, cc] in shape {
                    if let Some(n) = puzzle.cells[rr][cc].number {
                        if new_area > n as usize {
                            area_clue_ok = false;
                            break;
                        }
                    }
                }
            }
        }
        if !area_clue_ok {
            continue;
        }

        // No check_merge_ok here: the old guard rejected a cell joining a region
        // whenever it touched any other region with a free edge — but adjacent
        // regions legitimately share boundaries, so that blocked valid solutions
        // (e.g. 1301's official singleton-(6,7) tiling).  The final validators
        // (check_global_constraints / check_all / router IndependentValidator)
        // still reject any wrong answer.

        state.cell_to_region[r * w + c] = Some(rid);
        state.region_shapes[rid].push([r, c]);
        state.undecided_count -= 1;
        frontier_assign(state, r, c, rid);
        if let Some(n) = cell.number {
            state.region_clue.insert(rid, n as usize);
        }

        if check_watchtowers_ok(state) && check_vertex_ring_ok(puzzle, r, c, state) {
            if dfs(puzzle, state) {
                return true;
            }
        }

        state.cell_to_region[r * w + c] = None;
        state.region_shapes[rid].pop();
        state.undecided_count += 1;
        frontier_unassign(state, r, c, rid);
        match prev_clue {
            Some(m) => {
                state.region_clue.insert(rid, m);
            }
            None => {
                state.region_clue.remove(&rid);
            }
        }
    }

    // Start a new region (its id is the next contiguous index, so a push keeps
    // `region_shapes` indexed by region id; unwinding pops it back off).
    let new_rid = state.next_region_id;
    state.next_region_id += 1;
    state.cell_to_region[r * w + c] = Some(new_rid);
    state.region_shapes.push(vec![[r, c]]);
    state.undecided_count -= 1;
    frontier_assign(state, r, c, new_rid);
    if let Some(n) = cell.number {
        state.region_clue.insert(new_rid, n as usize);
    }

    if check_watchtowers_ok(state) && check_vertex_ring_ok(puzzle, r, c, state) {
        if dfs(puzzle, state) {
            return true;
        }
    }

    state.cell_to_region[r * w + c] = None;
    state.region_shapes.pop();
    state.undecided_count += 1;
    frontier_unassign(state, r, c, new_rid);
    state.region_clue.remove(&new_rid);
    state.next_region_id -= 1;

    false
}

fn is_undecided(state: &BacktrackState, r: usize, c: usize) -> bool {
    state.cell_index[r][c] != usize::MAX && state.cell_to_region[r * state.width + c].is_none()
}

/// Pick the next cell: grow an under-target clue-region first (its frontier,
/// smallest row-major index), else the smallest row-major undecided cell.
fn pick_next_cell(puzzle: &Puzzle, state: &BacktrackState) -> (usize, usize) {
    if state.has_area_rule {
        let mut best: Option<(usize, usize)> = None;
        let mut best_idx = usize::MAX;
        for (&rid, &n) in &state.region_clue {
            let area = state.region_shapes.get(rid).map(|s| s.len()).unwrap_or(0);
            if area < n {
                if let Some(fr) = state.frontier.get(&rid) {
                    for &cell in fr.keys() {
                        if is_undecided(state, cell.0, cell.1) {
                            let i = state.cell_index[cell.0][cell.1];
                            if i < best_idx {
                                best_idx = i;
                                best = Some(cell);
                            }
                        }
                    }
                }
            }
        }
        if let Some(cell) = best {
            return cell;
        }
    }
    for &cell in &state.fillable {
        if state.cell_to_region[cell.0 * state.width + cell.1].is_none() {
            return cell;
        }
    }
    unreachable!("undecided_count > 0 but no undecided cell")
}

/// Each clue-region must reach its target area: sealed regions (empty frontier)
/// are final, and total capacity must suffice.
fn check_area_lower_bounds(state: &BacktrackState) -> bool {
    if !state.has_area_rule {
        return true;
    }
    for (&rid, &n) in &state.region_clue {
        let area = state.region_shapes.get(rid).map(|s| s.len()).unwrap_or(0);
        if let Some(fr) = state.frontier.get(&rid) {
            if fr.is_empty() && area != n {
                if std::env::var("AOG_DEBUG").is_ok() {
                    eprintln!("  LB: sealed rid={} area={} n={}", rid, area, n);
                }
                return false;
            }
        }
        if area + state.undecided_count < n {
            if std::env::var("AOG_DEBUG").is_ok() {
                eprintln!("  LB: capacity rid={} area={} undecided={} n={}", rid, area, state.undecided_count, n);
            }
            return false;
        }
    }
    true
}

/// Mark (r,c) as assigned to `rid`, updating frontiers (no-op without area rule).
fn frontier_assign(state: &mut BacktrackState, r: usize, c: usize, rid: usize) {
    if !state.has_area_rule {
        return;
    }
    if let Some(fr) = state.frontier.get_mut(&rid) {
        fr.remove(&(r, c));
    }
    for (dr, dc) in [(1i64, 0i64), (-1, 0), (0, 1i64), (0, -1i64)] {
        let nr = r as i64 + dr;
        let nc = c as i64 + dc;
        if nr < 0 || nc < 0 {
            continue;
        }
        let (nru, ncu) = (nr as usize, nc as usize);
        if nru >= state.cell_index.len() || ncu >= state.cell_index[0].len() {
            continue;
        }
        if is_undecided(state, nru, ncu) {
            *state.frontier.entry(rid).or_default().entry((nru, ncu)).or_insert(0) += 1;
        } else if let Some(nrid) = state.cell_to_region[nru * state.width + ncu] {
            if nrid != rid {
                if let Some(fr) = state.frontier.get_mut(&nrid) {
                    if let Some(cnt) = fr.get_mut(&(r, c)) {
                        *cnt -= 1;
                        if *cnt == 0 {
                            fr.remove(&(r, c));
                        }
                    }
                }
            }
        }
    }
}

/// Undo frontier_assign for (r,c) leaving region `rid`.
fn frontier_unassign(state: &mut BacktrackState, r: usize, c: usize, rid: usize) {
    if !state.has_area_rule {
        return;
    }
    for (dr, dc) in [(1i64, 0i64), (-1, 0), (0, 1i64), (0, -1i64)] {
        let nr = r as i64 + dr;
        let nc = c as i64 + dc;
        if nr < 0 || nc < 0 {
            continue;
        }
        let (nru, ncu) = (nr as usize, nc as usize);
        if nru >= state.cell_index.len() || ncu >= state.cell_index[0].len() {
            continue;
        }
        if is_undecided(state, nru, ncu) {
            if let Some(fr) = state.frontier.get_mut(&rid) {
                if let Some(cnt) = fr.get_mut(&(nru, ncu)) {
                    *cnt -= 1;
                    if *cnt == 0 {
                        fr.remove(&(nru, ncu));
                    }
                }
            }
        } else if let Some(nrid) = state.cell_to_region[nru * state.width + ncu] {
            if nrid != rid {
                *state.frontier.entry(nrid).or_default().entry((r, c)).or_insert(0) += 1;
            }
        }
    }
    // Re-insert (r,c) into rid's frontier if still adjacent to rid.
    let count = neighbor_positions(r, c, state.cell_index.len(), state.cell_index[0].len())
        .iter()
        .filter(|&&(nr, nc)| state.cell_to_region[nr * state.width + nc] == Some(rid))
        .count();
    if count > 0 {
        state.frontier.entry(rid).or_default().insert((r, c), count);
    }
}

/// Incremental watchtower check: no vertex should already have more distinct regions than its target.
fn check_watchtowers_ok(state: &BacktrackState) -> bool {
    for &(ref cells, target) in &state.watchtowers {
        let mut pieces = Vec::new();
        for &[r, c] in cells {
            if let Some(p) = state.cell_to_region[r * state.width + c] {
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

/// Number of solution boundaries around the interior vertex (r,c) (the corner
/// shared by cells (r,c), (r+1,c), (r,c+1), (r+1,c+1)).
///
/// Blocked cells are empty space, not regions: a blocked-blocked edge is one
/// shared AREA_BLOCK value (NOT a boundary), mirroring the authoritative
/// `aog::validate::count_boundary_edges_at_vertex` and the C++ check_tatami.
fn vertex_boundary_count(puzzle: &Puzzle, state: &BacktrackState, r: usize, c: usize) -> usize {
    let is_bound = |a: (usize, usize), b: (usize, usize)| -> bool {
        let ra = if puzzle.cells[a.0][a.1].blocked {
            None
        } else {
            state.cell_to_region[a.0 * state.width + a.1]
        };
        let rb = if puzzle.cells[b.0][b.1].blocked {
            None
        } else {
            state.cell_to_region[b.0 * state.width + b.1]
        };
        match (ra, rb) {
            (Some(x), Some(y)) => x != y,
            (None, None) => false, // blocked-blocked = same empty space / outer border
            _ => true, // one side a region, the other empty space / outer border
        }
    };
    is_bound((r, c), (r + 1, c)) as usize
        + is_bound((r, c), (r, c + 1)) as usize
        + is_bound((r, c + 1), (r + 1, c + 1)) as usize
        + is_bound((r + 1, c), (r + 1, c + 1)) as usize
}

/// Incremental ring/brick check: after assigning cell (r,c), verify its corner
/// vertices don't already form a forbidden intersection (ring: 3 boundaries,
/// brick: 4 boundaries).
fn check_vertex_ring_ok(puzzle: &Puzzle, r: usize, c: usize, state: &BacktrackState) -> bool {
    let has_ring = puzzle.rules.iter().any(|rule| rule.ctype == "ring");
    let has_brick = puzzle.rules.iter().any(|rule| rule.ctype == "brick");
    if !has_ring && !has_brick {
        return true;
    }
    let h = puzzle.height as i32;
    let w = puzzle.width as i32;
    for (vr, vc) in [
        (r as i32 - 1, c as i32 - 1),
        (r as i32, c as i32 - 1),
        (r as i32 - 1, c as i32),
        (r as i32, c as i32),
    ] {
        if vr < 0 || vc < 0 || vr + 1 >= h || vc + 1 >= w {
            continue;
        }
        let cells = [(vr, vc), (vr + 1, vc), (vr, vc + 1), (vr + 1, vc + 1)];
        if !cells.iter().all(|&(a, b)| {
            state.cell_to_region[a as usize * state.width + b as usize].is_some()
                || puzzle.cells[a as usize][b as usize].blocked
        }) {
            continue;
        }
        let bc = vertex_boundary_count(puzzle, state, vr as usize, vc as usize);
        // brick: 4-way junction — blocked cells count as a distinct value
        // (their edges are border segments, mirroring C++ check_tatami and the
        // game's glimmith-solver).  A vertex with one blocked + 3 regions IS a
        // 4-way.  vertex_boundary_count already treats blocked-blocked as no
        // boundary, so no skip is needed here.
        if (has_ring && bc == 3) || (has_brick && bc == 4) {
            return false;
        }
    }
    true
}

/// Leaf ring/brick check: no interior vertex may form a forbidden intersection.
fn check_ring_ok(puzzle: &Puzzle, state: &BacktrackState) -> bool {
    let has_ring = puzzle.rules.iter().any(|rule| rule.ctype == "ring");
    let has_brick = puzzle.rules.iter().any(|rule| rule.ctype == "brick");
    if !has_ring && !has_brick {
        return true;
    }
    for r in 0..puzzle.height.saturating_sub(1) {
        for c in 0..puzzle.width.saturating_sub(1) {
            let bc = vertex_boundary_count(puzzle, state, r, c);
            if (has_ring && bc == 3) || (has_brick && bc == 4) {
                return false;
            }
        }
    }
    true
}

/// Global constraint check at leaf: all watchtowers satisfied, area bounds satisfied.
fn check_global_constraints(puzzle: &Puzzle, state: &BacktrackState) -> bool {
    if !check_watchtowers_ok(state) {
        return false;
    }
    if !check_ring_ok(puzzle, state) {
        return false;
    }

    // Check each watchtower has exactly the target distinct count (or all cells assigned)
    for &(ref cells, target) in &state.watchtowers {
        let mut pieces = Vec::new();
        let mut all_assigned = true;
        for &[r, c] in cells {
            if let Some(p) = state.cell_to_region[r * state.width + c] {
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
    for shape in &state.region_shapes {
        if shape.len() < state.area_bounds.min_area {
            return false;
        }
    }

    // Check compass clues
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                if let Some(rid) = state.cell_to_region[r * state.width + c] {
                    if let Some(cells) = state.region_shapes.get(rid) {
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

    // area rule: every numbered cell must lie in a region of exactly that size.
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(n) = puzzle.cells[r][c].number {
                if let Some(rid) = state.cell_to_region[r * state.width + c] {
                    if let Some(shape) = state.region_shapes.get(rid) {
                        if shape.len() != n as usize {
                            return false;
                        }
                    }
                }
            }
        }
    }

    // difference edges: |area(a) - area(b)| == value for each drawn difference edge.
    for r in 0..puzzle.height {
        for c in 0..puzzle.width.saturating_sub(1) {
            if let Some(ec) = &puzzle.h_edges[r][c].constraint {
                if ec.ctype == EdgeConstraintType::Difference {
                    if let (Some(ra), Some(rb)) = (
                        state.cell_to_region[r * state.width + c],
                        state.cell_to_region[r * state.width + (c + 1)],
                    ) {
                        let sa = state.region_shapes.get(ra).map(|s| s.len()).unwrap_or(0);
                        let sb = state.region_shapes.get(rb).map(|s| s.len()).unwrap_or(0);
                        if sa.abs_diff(sb) != ec.value.unwrap_or(0) as usize {
                            return false;
                        }
                    }
                }
            }
        }
    }
    for r in 0..puzzle.height.saturating_sub(1) {
        for c in 0..puzzle.width {
            if let Some(ec) = &puzzle.v_edges[r][c].constraint {
                if ec.ctype == EdgeConstraintType::Difference {
                    if let (Some(ra), Some(rb)) = (
                        state.cell_to_region[r * state.width + c],
                        state.cell_to_region[(r + 1) * state.width + c],
                    ) {
                        let sa = state.region_shapes.get(ra).map(|s| s.len()).unwrap_or(0);
                        let sb = state.region_shapes.get(rb).map(|s| s.len()).unwrap_or(0);
                        if sa.abs_diff(sb) != ec.value.unwrap_or(0) as usize {
                            return false;
                        }
                    }
                }
            }
        }
    }

    // inequality edges: value==1 → first endpoint region larger; else second.
    let check_ineq = |sa: usize, sb: usize, rev: bool| -> bool {
        if rev {
            sa > sb
        } else {
            sa < sb
        }
    };
    for r in 0..puzzle.height {
        for c in 0..puzzle.width.saturating_sub(1) {
            if let Some(ec) = &puzzle.h_edges[r][c].constraint {
                if ec.ctype == EdgeConstraintType::Inequality {
                    if let (Some(ra), Some(rb)) = (
                        state.cell_to_region[r * state.width + c],
                        state.cell_to_region[r * state.width + (c + 1)],
                    ) {
                        let sa = state.region_shapes.get(ra).map(|s| s.len()).unwrap_or(0);
                        let sb = state.region_shapes.get(rb).map(|s| s.len()).unwrap_or(0);
                        if !check_ineq(sa, sb, ec.value == Some(1)) {
                            return false;
                        }
                    }
                }
            }
        }
    }
    for r in 0..puzzle.height.saturating_sub(1) {
        for c in 0..puzzle.width {
            if let Some(ec) = &puzzle.v_edges[r][c].constraint {
                if ec.ctype == EdgeConstraintType::Inequality {
                    if let (Some(ra), Some(rb)) = (
                        state.cell_to_region[r * state.width + c],
                        state.cell_to_region[(r + 1) * state.width + c],
                    ) {
                        let sa = state.region_shapes.get(ra).map(|s| s.len()).unwrap_or(0);
                        let sb = state.region_shapes.get(rb).map(|s| s.len()).unwrap_or(0);
                        if !check_ineq(sa, sb, ec.value == Some(1)) {
                            return false;
                        }
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
                if let Some(s) = puzzle.cells[r][c].symbol.clone() {
                    sym_set.insert(s);
                }
            }
        }
        // Each symbol must appear exactly once per region
        for sym in &sym_set {
            for cells in &state.region_shapes {
                let count = cells.iter().filter(|&&[r, c]| {
                    puzzle.cells[r][c].symbol.as_deref() == Some(sym.as_str())
                }).count();
                if count != 1 {
                    return false;
                }
            }
        }
    }

    // block / non_block: every region must (not) be a solid rectangle.
    let has_block = puzzle.rules.iter().any(|r| r.ctype == "block");
    let has_non_block = puzzle.rules.iter().any(|r| r.ctype == "non_block");
    if has_block || has_non_block {
        for shape in &state.region_shapes {
            let rect = crate::shapes::is_rectangle(shape);
            if (has_block && !rect) || (has_non_block && rect) {
                return false;
            }
        }
    }

    // different: all region shapes dihedrally distinct (raw normalize+canonical
    // key missed rotation/reflection duplicates; use the dihedral key).
    if puzzle.rules.iter().any(|r| r.ctype == "different") {
        let mut keys: HashSet<String> = HashSet::new();
        for shape in &state.region_shapes {
            if !keys.insert(crate::shapes::dihedral_key(shape)) {
                return false;
            }
        }
    }

    // solitary: each region has exactly one clue-bearing cell.
    if puzzle.rules.iter().any(|r| r.ctype == "solitary") {
        for shape in &state.region_shapes {
            let mut clues = 0usize;
            for &[r, c] in shape {
                let cell = &puzzle.cells[r][c];
                if cell.symbol.is_some()
                    || cell.compass.is_some()
                    || cell.number.is_some()
                    || cell.shape_pattern.is_some()
                    || cell.fence_pattern.is_some()
                {
                    clues += 1;
                }
            }
            if clues != 1 {
                return false;
            }
        }
    }

    // differentiation: adjacent regions (sharing an edge) have different areas.
    if puzzle.rules.iter().any(|r| r.ctype == "differentiation") {
        let mut cell_to_rid: HashMap<(usize, usize), usize> = HashMap::new();
        for (rid, shape) in state.region_shapes.iter().enumerate() {
            for &[r, c] in shape {
                cell_to_rid.insert((r, c), rid);
            }
        }
        let area_of = |rid: usize| state.region_shapes.get(rid).map(|s| s.len()).unwrap_or(0);
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        for (rid, shape) in state.region_shapes.iter().enumerate() {
            for &[r, c] in shape {
                for (dr, dc) in [(1i64, 0i64), (0, 1i64)] {
                    let nr = r as i64 + dr;
                    let nc = c as i64 + dc;
                    if nr >= 0 && nr < puzzle.height as i64 && nc >= 0 && nc < puzzle.width as i64 {
                        if let Some(&other) = cell_to_rid.get(&(nr as usize, nc as usize)) {
                            if other != rid {
                                let key = if rid < other { (rid, other) } else { (other, rid) };
                                if seen.insert(key) && area_of(rid) == area_of(other) {
                                    return false;
                                }
                            }
                        }
                    }
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
        .enumerate()
        .map(|(rid, shape)| {
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
    // Region ids are a contiguous 0..n prefix, so this is already id-ordered;
    // keep the sort as a defensive no-op.
    regions.sort_by_key(|r| r.region_id);
    regions
}
