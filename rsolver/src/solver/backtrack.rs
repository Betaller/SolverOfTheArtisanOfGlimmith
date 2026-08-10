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
    let edge_constraints = collect_edge_constraints(puzzle);
    let has_edge_constraints = !edge_constraints.is_empty();

    let has_different = puzzle.rules.iter().any(|r| r.ctype == "different");
    let has_same = puzzle.rules.iter().any(|r| r.ctype == "same");
    let has_block = puzzle.rules.iter().any(|r| r.ctype == "block");
    let has_non_block = puzzle.rules.iter().any(|r| r.ctype == "non_block");
    // B1: rules the backtracker previously only caught at the leaf (doc 16 §3 B1).
    let has_solitary = puzzle.rules.iter().any(|r| r.ctype == "solitary");
    let has_differentiation = puzzle.rules.iter().any(|r| r.ctype == "differentiation");
    // Fence-rule mid-search pruning: pre-compute each fence cell's dihedral
    // invariant once.  Empty (and has_fence=false) when the puzzle has no
    // `fence` rule — zero overhead for the 1046 non-fence official puzzles.
    let fence_cells = crate::solver::fence::build_fence_cells(puzzle);
    let has_fence = !fence_cells.is_empty();
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
        edge_constraints,
        has_edge_constraints,
        has_different,
        has_same,
        has_block,
        has_non_block,
        // B1: mid-search pruning for rules previously only leaf-checked (doc 16 §3 B1).
        has_solitary,
        has_differentiation,
        fence_cells,
        has_fence,
    };

    if crate::aog_debug_enabled() { eprintln!("backtrack: start undecided={}", state.undecided_count); }
    if dfs(puzzle, &mut state) {
        Some(build_regions(&state))
    } else {
        None
    }
}

/// Pre-computed edge constraint between two cell positions.
#[derive(Debug, Clone)]
pub(crate) struct EdgeAreaConstraint {
    pub cell_a: (usize, usize),
    pub cell_b: (usize, usize),
    pub ctype: EdgeConstraintType,
    pub value: Option<i64>,
}

impl BacktrackState {
    pub(crate) fn is_sealed(&self, rid: usize) -> bool {
        self.frontier.get(&rid).map(|f| f.is_empty()).unwrap_or(true)
    }
}

pub(crate) struct BacktrackState {
    /// Flat row-major cell → region id (index `r*w+c`); unassigned / blocked = None.
    pub(crate) cell_to_region: Vec<Option<usize>>,
    /// Region id → cell list; region ids are a contiguous 0..n prefix, so the
    /// region id is the Vec index (new regions are pushed, undone regions popped).
    pub(crate) region_shapes: Vec<Vec<[usize; 2]>>,
    pub(crate) next_region_id: usize,
    /// Grid width (stride for `cell_to_region` row-major indexing).
    pub(crate) width: usize,
    pub(crate) steps: u64,
    deadline: Instant,
    pub(crate) area_bounds: AreaBounds,
    watchtowers: Vec<(Vec<[usize; 2]>, usize)>,
    // Area-clue machinery (zero overhead when the puzzle has no `area` rule).
    fillable: Vec<(usize, usize)>,
    cell_index: Vec<Vec<usize>>,
    pub(crate) undecided_count: usize,
    region_clue: HashMap<usize, usize>, // rid -> required area from a numbered cell inside
    pub(crate) frontier: HashMap<usize, HashMap<(usize, usize), usize>>, // rid -> {undecided cell : adjacency count}
    has_area_rule: bool,
    /// Pre-computed inequality / difference edge constraints for mid-search pruning.
    pub(crate) edge_constraints: Vec<EdgeAreaConstraint>,
    has_edge_constraints: bool,
    /// Mid-search shape-rule pruning flags (lazily set once at construction).
    has_different: bool,
    has_same: bool,
    has_block: bool,
    has_non_block: bool,
    /// B1: mid-search pruning for rules previously only leaf-checked (doc 16 §3 B1).
    has_solitary: bool,
    has_differentiation: bool,
    /// Pre-computed fence-pattern cells for mid-search pruning (empty when no
    /// `fence` rule).  See `crate::solver::fence`.
    fence_cells: Vec<crate::solver::fence::FenceCellData>,
    has_fence: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AreaBounds {
    pub(crate) min_area: usize,
    pub(crate) max_area: usize,
}

/// Collect all area/shape edge constraints for mid-search pruning.
/// B1: extended to include Heterogeneous / Homogeneous (shape-based) in addition
/// to the original Inequality / Difference (area-based). (doc 16 §3 B1.)
fn collect_edge_constraints(puzzle: &Puzzle) -> Vec<EdgeAreaConstraint> {
    let mut out = Vec::new();
    let h = puzzle.height;
    let w = puzzle.width;
    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            if let Some(ref ec) = puzzle.h_edges[r][c].constraint {
                match ec.ctype {
                    EdgeConstraintType::Inequality
                    | EdgeConstraintType::Difference
                    | EdgeConstraintType::Heterogeneous
                    | EdgeConstraintType::Homogeneous => {
                        out.push(EdgeAreaConstraint {
                            cell_a: (r, c),
                            cell_b: (r, c + 1),
                            ctype: ec.ctype.clone(),
                            value: ec.value,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            if let Some(ref ec) = puzzle.v_edges[r][c].constraint {
                match ec.ctype {
                    EdgeConstraintType::Inequality
                    | EdgeConstraintType::Difference
                    | EdgeConstraintType::Heterogeneous
                    | EdgeConstraintType::Homogeneous => {
                        out.push(EdgeAreaConstraint {
                            cell_a: (r, c),
                            cell_b: (r + 1, c),
                            ctype: ec.ctype.clone(),
                            value: ec.value,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    out
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

    // Vertex (r,c) is the ABSOLUTE grid corner (r in 0..=h, c in 0..=w).
    // The cells touching it are the in-bounds, non-blocked members of
    // {(r-1,c-1),(r-1,c),(r,c-1),(r,c)}.  Border corners are touched by 2
    // (edge) or 1 (grid corner) cells.
    for r in 0..=h {
        for c in 0..=w {
            if let Some(val) = puzzle.vertices[r][c].watchtower {
                let v = val as usize;
                // Only include valid watchtowers (1..=4)
                if v >= 1 && v <= 4 {
                    let mut cells = Vec::new();
                    for (dr, dc) in [(-1i64, -1i64), (-1, 0), (0, -1), (0, 0)] {
                        let nr = r as i64 + dr;
                        let nc = c as i64 + dc;
                        if nr < 0 || nc < 0 || nr >= h as i64 || nc >= w as i64 {
                            continue;
                        }
                        let nr = nr as usize;
                        let nc = nc as usize;
                        if !puzzle.cells[nr][nc].blocked {
                            cells.push([nr, nc]);
                        }
                    }
                    result.push((cells, v));
                }
            }
        }
    }
    result
}

fn timed_out(state: &BacktrackState) -> bool {
    Instant::now() >= state.deadline
}

/// Mid-search pruning for inequality / difference edge constraints.
///
/// When one side of a constraint edge is assigned to a sealed region (empty
/// frontier, area fixed), the other side's possible area range is narrowed.
/// If no feasible target remains the partial assignment is dead.
fn check_edge_area_mid_search(state: &BacktrackState) -> bool {
    if !state.has_edge_constraints {
        return true;
    }
    let w = state.width;
    for ec in &state.edge_constraints {
        let ra = state.cell_to_region[ec.cell_a.0 * w + ec.cell_a.1];
        let rb = state.cell_to_region[ec.cell_b.0 * w + ec.cell_b.1];
        let (Some(ra), Some(rb)) = (ra, rb) else { continue; };
        if ra == rb {
            continue; // same region, constraint edge was overridden
        }

        let area_a = state.region_shapes.get(ra).map(|s| s.len()).unwrap_or(0);
        let area_b = state.region_shapes.get(rb).map(|s| s.len()).unwrap_or(0);
        let sealed_a = state
            .frontier
            .get(&ra)
            .map(|f| f.is_empty())
            .unwrap_or(true);
        let sealed_b = state
            .frontier
            .get(&rb)
            .map(|f| f.is_empty())
            .unwrap_or(true);

        match ec.ctype {
            EdgeConstraintType::Difference => {
                let d = ec.value.unwrap_or(0) as usize;
                // If A is sealed, B must eventually be area_a ± d.
                if sealed_a {
                    let lo = area_a.saturating_sub(d);
                    let hi = area_a + d;
                    if area_b > hi {
                        return false;
                    }
                    if sealed_b && area_b != lo && area_b != hi {
                        return false;
                    }
                    if area_b + state.undecided_count < lo {
                        return false;
                    }
                }
                if sealed_b {
                    let lo = area_b.saturating_sub(d);
                    let hi = area_b + d;
                    if area_a > hi {
                        return false;
                    }
                    if sealed_a && area_a != lo && area_a != hi {
                        return false;
                    }
                    if area_a + state.undecided_count < lo {
                        return false;
                    }
                }
            }
            EdgeConstraintType::Inequality => {
                // value==1 ⇒ first endpoint (A) larger: area_a > area_b
                let (larger_area, smaller_area, larger_sealed, _smaller_sealed) =
                    if ec.value == Some(1) {
                        (area_a, area_b, sealed_a, sealed_b)
                    } else {
                        (area_b, area_a, sealed_b, sealed_a)
                    };
                // larger > smaller  ⇒  larger ≥ smaller + 1
                if larger_sealed && larger_area <= smaller_area {
                    return false;
                }
            }
            // B1: shape-based edge constraints (doc 16 §3 B1).
            EdgeConstraintType::Heterogeneous => {
                // delta: adjacent regions must have DIFFERENT shapes.
                if sealed_a && sealed_b && area_a > 0 && area_b > 0 {
                    let key_a = crate::shapes::dihedral_key(&state.region_shapes[ra]);
                    let key_b = crate::shapes::dihedral_key(&state.region_shapes[rb]);
                    if key_a == key_b {
                        return false;
                    }
                }
            }
            EdgeConstraintType::Homogeneous => {
                // gemini: adjacent regions must have the SAME shape.
                if sealed_a && sealed_b && area_a > 0 && area_b > 0 {
                    let key_a = crate::shapes::dihedral_key(&state.region_shapes[ra]);
                    let key_b = crate::shapes::dihedral_key(&state.region_shapes[rb]);
                    if key_a != key_b {
                        return false;
                    }
                }
            }
            _ => {}
        }
    }
    true
}

fn dfs(puzzle: &Puzzle, state: &mut BacktrackState) -> bool {
    if state.undecided_count == 0 {
        let regions = build_regions(state);
        return crate::solver::validate::validate(puzzle, &regions);
    }

    state.steps += 1;
    if state.steps % 1024 == 0 && timed_out(state) {
        return false;
    }

    // Area lower bound: each clue-region must be able to reach its target size.
    if !check_area_lower_bounds(state) {
        return false;
    }

    // Edge area constraint pruning: sealed regions force neighbour areas.
    if !check_edge_area_mid_search(state) {
        return false;
    }

    // Bellman-Ford area-constraint propagation: only for puzzles with
    // inequality / difference edges.  Throttled to every 256 steps.
    // Default on; set BF_PROPAGATE=0 to disable.
    if state.has_edge_constraints && state.steps % 256 == 0 {
        if !crate::solver::prototypes::propagate_area_bounds(
            &state.cell_to_region,
            &state.region_shapes,
            &state.frontier,
            state.next_region_id,
            &state.edge_constraints,
            state.area_bounds.min_area,
            state.area_bounds.max_area,
            state.undecided_count,
            state.width,
        ) {
            return false;
        }
    }

    // SAT-based boundary-graph feasibility (every 64 steps).
    // Proved +1 net improvement at 20s timeout (solves 0573, 1112, 1261).
    if state.steps % 64 == 0 {
        if !crate::solver::prototypes::sat_boundary_feasible(
            puzzle, &state.cell_to_region, state.width,
        ) {
            return false;
        }
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

        if check_watchtowers_ok(state)
            && check_vertex_ring_ok(puzzle, r, c, state)
            && check_sealed_regions(puzzle, state)
            && check_fence_ok(puzzle, state)
        {
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

    if check_watchtowers_ok(state)
        && check_vertex_ring_ok(puzzle, r, c, state)
        && check_sealed_regions(puzzle, state)
        && check_fence_ok(puzzle, state)
    {
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

/// Mid-search shape-rule pruning: when a region seals (empty frontier, its shape
/// is final), check different / same / block / non_block / solitary / differentiation
/// immediately instead of waiting for the leaf.  Returns false if the sealed region
/// violates a rule — the branch is dead.
///
/// Stateless (reads only `region_shapes` and `frontier`) so it needs no undo
/// logic on backtrack — re-evaluated from scratch after every frontier_assign.
fn check_sealed_regions(puzzle: &Puzzle, state: &BacktrackState) -> bool {
    let has_shape_rules = state.has_different
        || state.has_same
        || state.has_block
        || state.has_non_block
        || state.has_solitary
        || state.has_differentiation;
    if !has_shape_rules {
        return true;
    }

    let mut sealed_keys: Vec<String> = Vec::new();
    // B1: track sealed region areas for differentiation check (doc 16 §3 B1).
    let mut sealed_areas: Vec<(usize, usize)> = Vec::new(); // (rid, area)

    for rid in 0..state.next_region_id {
        if !state.is_sealed(rid) {
            continue;
        }
        let shape = match state.region_shapes.get(rid) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        let area = shape.len();

        // block / non_block: check immediately
        if state.has_block || state.has_non_block {
            let rect = crate::shapes::is_rectangle(shape);
            if state.has_block && !rect {
                return false;
            }
            if state.has_non_block && rect {
                return false;
            }
        }

        // different: every sealed shape must be unique
        // same: every sealed shape must match the first one
        // (same and different are mutually exclusive per CONFLICTING_RULES)
        if state.has_different || state.has_same {
            let key = crate::shapes::dihedral_key(shape);
            if state.has_different {
                if sealed_keys.contains(&key) {
                    return false; // duplicate shape
                }
                sealed_keys.push(key);
            } else if state.has_same {
                if let Some(ref first) = sealed_keys.first() {
                    if **first != key {
                        return false; // mismatch
                    }
                } else {
                    sealed_keys.push(key);
                }
            }
        }

        // B1: solitary — each sealed region must contain exactly one clue cell
        // (numbered cell or cell with a symbol). (doc 16 §3 B1.)
        if state.has_solitary {
            let clue_count = shape.iter().filter(|&&[r, c]| {
                puzzle.cells[r][c].number.is_some() || puzzle.cells[r][c].symbol.is_some()
            }).count();
            if clue_count != 1 {
                return false;
            }
        }

        // B1: differentiation — accumulate sealed areas for pairwise check below.
        if state.has_differentiation {
            sealed_areas.push((rid, area));
        }
    }

    // B1: differentiation — adjacent sealed regions must have different areas.
    // Only executed when at least two regions are sealed. (doc 16 §3 B1.)
    if state.has_differentiation && sealed_areas.len() >= 2 {
        let w = state.width;
        let h = state.cell_to_region.len() / w;
        for i in 0..sealed_areas.len() {
            let (rid_a, area_a) = sealed_areas[i];
            for j in (i + 1)..sealed_areas.len() {
                let (rid_b, area_b) = sealed_areas[j];
                if area_a != area_b {
                    continue; // different areas → OK for differentiation
                }
                // Same area — check if the two sealed regions are adjacent.
                if regions_are_adjacent(state, rid_a, rid_b, w, h) {
                    return false;
                }
            }
        }
    }

    true
}

/// B1: return true if two regions share at least one non-boundary edge.
fn regions_are_adjacent(
    state: &BacktrackState,
    rid_a: usize,
    rid_b: usize,
    w: usize,
    h: usize,
) -> bool {
    let shape_a = match state.region_shapes.get(rid_a) {
        Some(s) => s,
        None => return false,
    };
    for &[r, c] in shape_a {
        // Check 4 neighbors
        if c + 1 < w {
            if state.cell_to_region[r * w + c + 1] == Some(rid_b) {
                return true;
            }
        }
        if c > 0 {
            if state.cell_to_region[r * w + c - 1] == Some(rid_b) {
                return true;
            }
        }
        if r + 1 < h {
            if state.cell_to_region[(r + 1) * w + c] == Some(rid_b) {
                return true;
            }
        }
        if r > 0 {
            if state.cell_to_region[(r - 1) * w + c] == Some(rid_b) {
                return true;
            }
        }
    }
    false
}
/// Thin forwarder to `fence::check_fence_patterns`, pulling the search state
/// fields it needs.  Keeps the `fence` module decoupled from `BacktrackState`
/// (no cyclic type dependency) and gives the guard chain a uniform
/// `check_*_ok(puzzle, state)` spelling.  Zero cost when `has_fence` is false
/// (one boolean check, matches `check_sealed_regions`' `has_shape_rules` gate).
#[inline]
fn check_fence_ok(puzzle: &Puzzle, state: &BacktrackState) -> bool {
    if !state.has_fence {
        return true;
    }
    crate::solver::fence::check_fence_patterns(
        puzzle,
        &state.cell_to_region,
        state.width,
        &state.fence_cells,
    )
}

fn is_undecided(state: &BacktrackState, r: usize, c: usize) -> bool {
    state.cell_index[r][c] != usize::MAX && state.cell_to_region[r * state.width + c].is_none()
}

/// Count the number of distinct adjacent regions to an undecided cell, plus one
/// for the "start new region" option.  This is the cell's *domain size* — the
/// MRV (Minimum Remaining Values) heuristic picks the cell with smallest domain.
///
/// Uses `is_adjacent_free` so that pre-drawn boundaries (and blocked neighbours)
/// are respected: a neighbour blocked by a boundary can't be joined.
fn cell_domain_size(puzzle: &Puzzle, state: &BacktrackState, r: usize, c: usize) -> usize {
    let w = state.width;
    let h = state.cell_index.len();
    let mut rids: HashSet<usize> = HashSet::with_capacity(4);
    if c > 0 {
        if let Some(rid) = state.cell_to_region[r * w + (c - 1)] {
            if grid::is_adjacent_free(puzzle, r, c, r, c - 1) {
                rids.insert(rid);
            }
        }
    }
    if c + 1 < w {
        if let Some(rid) = state.cell_to_region[r * w + (c + 1)] {
            if grid::is_adjacent_free(puzzle, r, c, r, c + 1) {
                rids.insert(rid);
            }
        }
    }
    if r > 0 {
        if let Some(rid) = state.cell_to_region[(r - 1) * w + c] {
            if grid::is_adjacent_free(puzzle, r, c, r - 1, c) {
                rids.insert(rid);
            }
        }
    }
    if r + 1 < h {
        if let Some(rid) = state.cell_to_region[(r + 1) * w + c] {
            if grid::is_adjacent_free(puzzle, r, c, r + 1, c) {
                rids.insert(rid);
            }
        }
    }
    // +1 for the "start a new region" option (always available)
    rids.len() + 1
}

/// Pick the next cell: grow an under-target clue-region first (its frontier,
/// smallest domain-size among those, row-major tiebreak), else the globally
/// most-constrained undecided cell (MRV / entropy heuristic).
///
/// The MRV fallback replaces the old row-major scan.  Cells with only 1 option
/// (must join a specific neighbouring region or must start fresh) are selected
/// first — they have no real choice and delaying them only wastes search.
fn pick_next_cell(puzzle: &Puzzle, state: &BacktrackState) -> (usize, usize) {
    // Area-rule path: grow under-target clue-regions from their frontier, using
    // the original row-major ordering (which is well-tuned for this path).
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
    // MRV fallback: when there are no area-number clues to guide growth, pick
    // the globally most-constrained undecided cell (minimum remaining values).
    // Cells with only 1 option (must join a specific region or start fresh) are
    // selected first — they have no real choice and delaying them wastes search.
    let mut best: Option<(usize, usize)> = None;
    let mut best_domain = usize::MAX;
    let mut best_idx = usize::MAX;
    for &(r, c) in &state.fillable {
        if state.cell_to_region[r * state.width + c].is_some() {
            continue;
        }
        let domain = cell_domain_size(puzzle, state, r, c);
        let i = state.cell_index[r][c];
        if domain < best_domain || (domain == best_domain && i < best_idx) {
            best_domain = domain;
            best_idx = i;
            best = Some((r, c));
        }
    }
    best.expect("undecided_count > 0 but no undecided cell")
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
                if crate::aog_debug_enabled() {
                    eprintln!("  LB: sealed rid={} area={} n={}", rid, area, n);
                }
                return false;
            }
        }
        if area + state.undecided_count < n {
            if crate::aog_debug_enabled() {
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


/// Lower / upper bound on boundary-degree at a vertex.
///
/// `lb` = edges where BOTH sides are assigned AND differ (definite boundaries).
/// `ub` = `lb` + edges where at least one side is unassigned (potential boundaries).
///
/// Blocked cells are treated as empty space (blocked-blocked = no boundary,
/// blocked-region = boundary), mirroring `vertex_boundary_count`.
fn vertex_boundary_bounds(
    puzzle: &Puzzle,
    state: &BacktrackState,
    vr: usize,
    vc: usize,
) -> (usize, usize) {
    let cells = [
        (vr as i32, vc as i32),
        (vr as i32 + 1, vc as i32),
        (vr as i32, vc as i32 + 1),
        (vr as i32 + 1, vc as i32 + 1),
    ];
    // Edge pairs: top(0,1), bottom(2,3), left(0,2), right(1,3)
    let edges = [(0usize, 1usize), (2, 3), (0, 2), (1, 3)];

    #[derive(PartialEq)]
    enum CellState {
        Blocked,
        Unassigned,
        Assigned(usize),
    }
    let cell_state = |i: usize| -> Option<CellState> {
        let (a, b) = cells[i];
        if a < 0 || b < 0 {
            return None;
        }
        let au = a as usize;
        let bu = b as usize;
        if au >= puzzle.height || bu >= puzzle.width {
            return None;
        }
        if puzzle.cells[au][bu].blocked {
            Some(CellState::Blocked)
        } else {
            match state.cell_to_region[au * state.width + bu] {
                Some(rid) => Some(CellState::Assigned(rid)),
                None => Some(CellState::Unassigned),
            }
        }
    };

    let mut lb = 0usize;
    let mut unknown = 0usize;
    for &(i, j) in &edges {
        let ra = cell_state(i);
        let rb = cell_state(j);
        match (ra, rb) {
            (Some(CellState::Assigned(a)), Some(CellState::Assigned(b))) => {
                if a != b {
                    lb += 1;
                }
            }
            (Some(CellState::Blocked), Some(CellState::Blocked)) => {
                // blocked-blocked → not a boundary
            }
            (Some(CellState::Assigned(_)), Some(CellState::Blocked))
            | (Some(CellState::Blocked), Some(CellState::Assigned(_)))
            | (Some(CellState::Blocked), Some(CellState::Unassigned))
            | (Some(CellState::Unassigned), Some(CellState::Blocked)) => {
                // region-blocked or future-region-blocked → definite boundary
                lb += 1;
            }
            (Some(CellState::Unassigned), Some(CellState::Unassigned))
            | (Some(CellState::Unassigned), Some(CellState::Assigned(_)))
            | (Some(CellState::Assigned(_)), Some(CellState::Unassigned)) => {
                // at least one side unassigned (not blocked) → unknown
                unknown += 1;
            }
            _ => {
                // out-of-bounds → outer border boundary
                lb += 1;
            }
        }
    }
    (lb, lb + unknown)
}

/// Incremental ring/brick check with arc-consistency propagation.
///
/// After assigning cell (r,c), for each of its four corner vertices compute the
/// lower bound on boundary edges.  If the lower bound already reaches the
/// forbidden degree the partial assignment is dead — no matter how the remaining
/// unassigned cells around that vertex are placed, the vertex will end up with
/// ≥3 (ring) or ≥4 (brick) boundaries.
///
/// This is stronger than the old check, which only looked at vertices whose four
/// cells were *all* assigned and therefore missed early violations (e.g. 3
/// assigned cells in 3 different regions ⇒ 3 definite boundaries ⇒ ring
/// violation even though the 4th cell is still undecided).
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
        // GF(2) parity: boundary-degree must be even (universal topology constraint).
        if !crate::solver::prototypes::check_gf2_parity(
            puzzle, &state.cell_to_region, state.width, vr as usize, vc as usize,
        ) {
            return false;
        }
        let (lb, _ub) = vertex_boundary_bounds(puzzle, state, vr as usize, vc as usize);
        // ring: degree 3 already reached → dead.  (ub check not needed: if lb ≥ 3
        // the violation is certain regardless of future assignments.)
        if has_ring && lb >= 3 {
            return false;
        }
        // brick: degree 4 already reached → dead.
        if has_brick && lb >= 4 {
            return false;
        }
    }
    true
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
