//! Cells-based rule validation, ported from the Python `IndependentValidator`.
//!
//! The complete, solver-agnostic independent validator: it validates the
//! extracted regions of any solver against the rsolver `Puzzle` model so a
//! buggy fill can never be reported as a solution.  The AoG search works on the
//! C++-style padded grid; this module is the common acceptance gate for aog
//! (`solver/aog/mod.rs`) and rose (`solver/rose/mod.rs`).

use std::collections::{HashMap, HashSet};

use crate::shapes::{collect_pool_shapes, dihedral_key, is_rectangle, rose_symbol_types};
use crate::types::*;

pub fn validate(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    let h = puzzle.height;
    let w = puzzle.width;

    // Build region->cell lookup and check all fillable cells assigned.
    let mut by_rid: HashMap<usize, Vec<[usize; 2]>> = HashMap::new();
    // B-V1: flat cell->region-id index for O(1) `region_of` lookups (was O(R.N)
    // linear scan). Built in the same pass as by_rid; blocked cells stay None.
    let mut cell_to_rid: Vec<Option<usize>> = vec![None; h * w];
    for r in 0..h {
        for c in 0..w {
            if puzzle.cells[r][c].blocked {
                continue;
            }
            let mut found = false;
            for reg in regions {
                if reg.cells.iter().any(|&[rr, cc]| rr == r && cc == c) {
                    by_rid.entry(reg.region_id).or_default().push([r, c]);
                    cell_to_rid[r * w + c] = Some(reg.region_id);
                    found = true;
                    break;
                }
            }
            if !found {
                return false; // unassigned fillable cell
            }
        }
    }

    // Connectivity.
    for cells in by_rid.values() {
        if !is_connected(cells, h, w) {
            return false;
        }
    }

    // Pre-drawn boundaries separate regions.
    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            if puzzle.h_edges[r][c].is_boundary {
                let a = region_of(&cell_to_rid, r, c, w);
                let b = region_of(&cell_to_rid, r, c + 1, w);
                if a.is_some() && b.is_some() && a == b {
                    return false;
                }
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            if puzzle.v_edges[r][c].is_boundary {
                let a = region_of(&cell_to_rid, r, c, w);
                let b = region_of(&cell_to_rid, r + 1, c, w);
                if a.is_some() && b.is_some() && a == b {
                    return false;
                }
            }
        }
    }

    // Shape keys (canonical, dihedral-normalized) for each region.
    let shape_key_of: Vec<String> = regions
        .iter()
        .map(|reg| dihedral_key(&reg.cells))
        .collect();

    let ctx = ValidateCtx {
        puzzle,
        regions,
        by_rid: &by_rid,
        cell_to_rid: &cell_to_rid,
        h,
        w,
        shape_key_of: &shape_key_of,
    };
    for rule in &puzzle.rules {
        if !ctx.dispatch(rule) {
            return false;
        }
    }
    true
}

/// Shared, borrow-free view of the data `validate` needs, so each rule check can
/// be a small named function instead of one 22-arm `match`. Mirrors the Python
/// `RULE_CHECKERS` registry in `src/solver/constraints.py`.
struct ValidateCtx<'a> {
    puzzle: &'a Puzzle,
    regions: &'a [RegionInfo],
    by_rid: &'a HashMap<usize, Vec<[usize; 2]>>,
    cell_to_rid: &'a [Option<usize>],
    h: usize,
    w: usize,
    shape_key_of: &'a [String],
}

impl<'a> ValidateCtx<'a> {
    /// Dispatch one rule to its registered checker; unknown rule types pass.
    fn dispatch(&self, rule: &Rule) -> bool {
        let ctype: &str = rule.ctype.as_str();
        for entry in RULE_CHECKERS.iter() {
            if entry.0 == ctype {
                return (entry.1)(self, rule);
            }
        }
        true
    }
}

type RuleChecker = fn(&ValidateCtx, &Rule) -> bool;

/// Registry of per-rule validators (one entry per `ctype`). The four
/// edge-constraint types share `check_edge_constraints_rule`; the original
/// 22-arm `match` in `validate` is now this table lookup.
const RULE_CHECKERS: &[(&str, RuleChecker)] = &[
    ("shape_pool", check_shape_pool),
    ("precise", check_precise),
    ("range", check_range),
    ("area", check_area),
    ("same", check_same),
    ("different", check_different),
    ("mixed", check_mixed),
    ("differentiation", check_differentiation),
    ("solitary", check_solitary),
    ("block", check_block),
    ("non_block", check_non_block),
    ("puzzle_piece", check_puzzle_piece),
    ("fence", check_fence),
    ("compass", check_compass),
    ("rose_window", check_rose_window_rule),
    ("heterogeneous", check_edge_constraints_rule),
    ("homogeneous", check_edge_constraints_rule),
    ("inequality", check_edge_constraints_rule),
    ("difference", check_edge_constraints_rule),
    ("watchtower", check_watchtower),
    ("brick", check_brick),
    ("ring", check_ring),
];

fn check_shape_pool(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    let pool: HashSet<String> = collect_pool_shapes(ctx.puzzle)
        .iter()
        .map(|s| dihedral_key(s))
        .collect();
    for key in ctx.shape_key_of {
        if !pool.contains(key) {
            return false;
        }
    }
    true
}

fn check_precise(ctx: &ValidateCtx, rule: &Rule) -> bool {
    let target = rule.params.get("area").and_then(|v| v.as_i64()).unwrap_or(0);
    for reg in ctx.regions {
        if reg.area as i64 != target {
            return false;
        }
    }
    true
}

fn check_range(ctx: &ValidateCtx, rule: &Rule) -> bool {
    let lo = rule.params.get("min").and_then(|v| v.as_i64()).unwrap_or(1);
    let hi = rule.params.get("max").and_then(|v| v.as_i64()).unwrap_or(i64::MAX);
    for reg in ctx.regions {
        if (reg.area as i64) < lo || (reg.area as i64) > hi {
            return false;
        }
    }
    true
}

fn check_area(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for r in 0..ctx.h {
        for c in 0..ctx.w {
            let cell = &ctx.puzzle.cells[r][c];
            if let Some(n) = cell.number {
                if let Some(rid) = region_of(ctx.cell_to_rid, r, c, ctx.w) {
                    if ctx.by_rid[&rid].len() != n as usize {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn check_same(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    let set: HashSet<&String> = ctx.shape_key_of.iter().collect();
    set.len() <= 1
}

fn check_different(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    let set: HashSet<&String> = ctx.shape_key_of.iter().collect();
    set.len() == ctx.shape_key_of.len()
}

fn check_mixed(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    adjacent_pairs_satisfy(ctx.puzzle, ctx.regions, ctx.by_rid, ctx.cell_to_rid, ctx.w, |a, b| {
        ctx.shape_key_of[a] != ctx.shape_key_of[b]
    })
}

fn check_differentiation(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    adjacent_pairs_satisfy(ctx.puzzle, ctx.regions, ctx.by_rid, ctx.cell_to_rid, ctx.w, |a, b| {
        ctx.regions[a].area != ctx.regions[b].area
    })
}

fn check_solitary(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for reg in ctx.regions {
        let mut clues = 0;
        for &[r, c] in &reg.cells {
            let cell = &ctx.puzzle.cells[r][c];
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
    true
}

fn check_block(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for reg in ctx.regions {
        if !is_rectangle(&reg.cells) {
            return false;
        }
    }
    true
}

fn check_non_block(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for reg in ctx.regions {
        if is_rectangle(&reg.cells) {
            return false;
        }
    }
    true
}

fn check_puzzle_piece(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for reg in ctx.regions {
        for &[r, c] in &reg.cells {
            if let Some(ref pat) = ctx.puzzle.cells[r][c].shape_pattern {
                if dihedral_key(&reg.cells) != dihedral_key(pat) {
                    return false;
                }
            }
        }
    }
    true
}

fn check_fence(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for r in 0..ctx.h {
        for c in 0..ctx.w {
            let cell = &ctx.puzzle.cells[r][c];
            if let Some(ref fp) = cell.fence_pattern {
                if let Some(rid) = region_of(ctx.cell_to_rid, r, c, ctx.w) {
                    let bits = region_boundary_bits(ctx.puzzle, ctx.cell_to_rid, ctx.w, rid, r, c);
                    let pat = fence_pattern_shape(bits);
                    if dihedral_key(&pat) != dihedral_key(fp) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn check_compass(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for r in 0..ctx.h {
        for c in 0..ctx.w {
            let cell = &ctx.puzzle.cells[r][c];
            if let Some(ref comp) = cell.compass {
                if let Some(rid) = region_of(ctx.cell_to_rid, r, c, ctx.w) {
                    let cells = &ctx.by_rid[&rid];
                    for (dr, dc, attr) in [(-1i64, 0i64, 0usize), (1, 0, 1), (0, -1, 2), (0, 1, 3)] {
                        let expected = match attr {
                            0 => comp.up,
                            1 => comp.down,
                            2 => comp.left,
                            _ => comp.right,
                        };
                        let expected = match expected {
                            Some(v) if v >= 0 => v,
                            _ => continue,
                        };
                        let mut count = 0i64;
                        for &[rr, cc] in cells {
                            if rr == r && cc == c {
                                continue;
                            }
                            if dr == -1 && (rr as i64) < (r as i64) {
                                count += 1;
                            } else if dr == 1 && (rr as i64) > (r as i64) {
                                count += 1;
                            } else if dc == -1 && (cc as i64) < (c as i64) {
                                count += 1;
                            } else if dc == 1 && (cc as i64) > (c as i64) {
                                count += 1;
                            }
                        }
                        if count != expected {
                            return false;
                        }
                    }
                }
            }
        }
    }
    true
}

fn check_rose_window_rule(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    check_rose_window(ctx.puzzle, ctx.regions)
}

fn check_edge_constraints_rule(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    check_edge_constraints(ctx.puzzle, ctx.by_rid, ctx.cell_to_rid, ctx.w)
}

fn check_watchtower(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for r in 0..=ctx.h {
        for c in 0..=ctx.w {
            if let Some(val) = ctx.puzzle.vertices[r][c].watchtower {
                let mut distinct = HashSet::new();
                for (dr, dc) in [(-1i64, -1i64), (-1, 0), (0, -1), (0, 0)] {
                    let nr = r as i64 + dr;
                    let nc = c as i64 + dc;
                    if nr < 0 || nc < 0 || nr >= ctx.h as i64 || nc >= ctx.w as i64 {
                        continue;
                    }
                    if let Some(rid) = region_of(ctx.cell_to_rid, nr as usize, nc as usize, ctx.w) {
                        distinct.insert(rid);
                    }
                }
                if distinct.len() != val as usize {
                    return false;
                }
            }
        }
    }
    true
}

fn check_brick(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    for r in 0..ctx.h.saturating_sub(1) {
        for c in 0..ctx.w.saturating_sub(1) {
            if count_boundary_edges_at_vertex(ctx.puzzle, ctx.cell_to_rid, ctx.w, r as i32, c as i32) == 4 {
                return false;
            }
        }
    }
    true
}

fn check_ring(ctx: &ValidateCtx, _rule: &Rule) -> bool {
    let hi = ctx.h as i32;
    let wi = ctx.w as i32;
    for r in -1..hi {
        for c in -1..wi {
            if count_boundary_edges_at_vertex(ctx.puzzle, ctx.cell_to_rid, ctx.w, r, c) == 3 {
                return false;
            }
        }
    }
    true
}


/// O(1) cell→region-id lookup via the pre-built `cell_to_rid` index (B-V1).
/// Was O(R·N) linear scan over `by_rid` — called ~15× per validate, the
/// dominant cost on large grids. (doc 16 §1 V1.)
fn region_of(cell_to_rid: &[Option<usize>], r: usize, c: usize, w: usize) -> Option<usize> {
    cell_to_rid[r * w + c]
}

/// rose_window: each region contains exactly one of each symbol type.
fn check_rose_window(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    // Symbol types come from the shared helper.  A puzzle with no rose_window
    // rule short-circuits (defensive — the dispatcher only reaches this arm
    // when the rule is present).
    if !puzzle.rules.iter().any(|r| r.ctype == "rose_window") {
        return true;
    }
    let types = rose_symbol_types(puzzle);
    if types.is_empty() {
        return false;
    }
    let h = puzzle.height;
    let w = puzzle.width;
    // Count occurrences of each type.
    let mut counts = vec![0usize; types.len()];
    for r in 0..h {
        for c in 0..w {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                match types.iter().position(|t| t == sym) {
                    Some(i) => counts[i] += 1,
                    None => return false,
                }
            }
        }
    }
    let m = counts[0];
    if counts.iter().any(|&c| c != m) {
        return false;
    }
    if regions.len() != m {
        return false;
    }
    // Each region must contain all types.
    for reg in regions {
        let mut seen = vec![false; types.len()];
        let mut cnt = 0usize;
        for &[r, c] in &reg.cells {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                if let Some(i) = types.iter().position(|t| t == sym) {
                    if seen[i] {
                        return false; // duplicate symbol in region
                    }
                    seen[i] = true;
                    cnt += 1;
                }
            }
        }
        if cnt != types.len() {
            return false;
        }
    }
    true
}

fn is_connected(cells: &[[usize; 2]], h: usize, w: usize) -> bool {
    if cells.is_empty() {
        return false;
    }
    let set: HashSet<(usize, usize)> = cells.iter().map(|&[r, c]| (r, c)).collect();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut stack = vec![(cells[0][0], cells[0][1])];
    while let Some((r, c)) = stack.pop() {
        if !seen.insert((r, c)) {
            continue;
        }
        for (dr, dc) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let nr = r as i64 + dr;
            let nc = c as i64 + dc;
            if nr >= 0 && nr < h as i64 && nc >= 0 && nc < w as i64 {
                let p = (nr as usize, nc as usize);
                if set.contains(&p) && !seen.contains(&p) {
                    stack.push(p);
                }
            }
        }
    }
    seen.len() == cells.len()
}

fn adjacent_pairs_satisfy(
    puzzle: &Puzzle,
    regions: &[RegionInfo],
    by_rid: &HashMap<usize, Vec<[usize; 2]>>,
    cell_to_rid: &[Option<usize>],
    w: usize,
    pred: impl Fn(usize, usize) -> bool,
) -> bool {
    let _ = puzzle;
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    for (rid, cells) in by_rid {
        let mut idx = None;
        for (i, reg) in regions.iter().enumerate() {
            if reg.region_id == *rid {
                idx = Some(i);
                break;
            }
        }
        let ai = match idx {
            Some(i) => i,
            None => return false,
        };
        for &[r, c] in cells {
            for (dr, dc) in [(1i64, 0i64), (0, 1i64)] {
                let nr = r as i64 + dr;
                let nc = c as i64 + dc;
                if nr >= 0 && nr < puzzle.height as i64 && nc >= 0 && nc < puzzle.width as i64 {
                    let nr = nr as usize;
                    let nc = nc as usize;
                    if let Some(other) = region_of(cell_to_rid, nr, nc, w) {
                        if other != *rid {
                            let key = (rid.min(&other), rid.max(&other));
                            let key = (*key.0, *key.1);
                            if seen.insert(key) {
                                let mut bi = None;
                                for (j, reg) in regions.iter().enumerate() {
                                    if reg.region_id == other {
                                        bi = Some(j);
                                        break;
                                    }
                                }
                                let bi = match bi {
                                    Some(j) => j,
                                    None => return false,
                                };
                                if !pred(ai, bi) {
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

fn check_edge_constraints(
    puzzle: &Puzzle,
    by_rid: &HashMap<usize, Vec<[usize; 2]>>,
    cell_to_rid: &[Option<usize>],
    w: usize,
) -> bool {
    let area_of = |rid: usize| by_rid.get(&rid).map(|c| c.len());
    // B-V2: precompute each region's dihedral_key once (was recomputed per-edge
    // in the closure below — O(edges × region_size) → O(regions × region_size)).
    // Edge-constraint-dense puzzles (difference/inequality/heterogeneous) see
    // 50-80% validate speedup. (doc 16 §1 V2.)
    let rid_to_key: HashMap<usize, String> = by_rid
        .iter()
        .map(|(&rid, cells)| (rid, dihedral_key(cells)))
        .collect();
    let shape_key_of = |rid: usize| -> Option<String> {
        rid_to_key.get(&rid).cloned()
    };
    // Iterate all edges with constraints.
    for r in 0..puzzle.height {
        for c in 0..puzzle.width.saturating_sub(1) {
            if let Some(ref ec) = puzzle.h_edges[r][c].constraint {
                let a = region_of(cell_to_rid, r, c, w);
                let b = region_of(cell_to_rid, r, c + 1, w);
                if let (Some(ra), Some(rb)) = (a, b) {
                    if ra == rb || !edge_constraint_ok(ec, ra, rb, &area_of, &shape_key_of) {
                        return false;
                    }
                }
            }
        }
    }
    for r in 0..puzzle.height.saturating_sub(1) {
        for c in 0..puzzle.width {
            if let Some(ref ec) = puzzle.v_edges[r][c].constraint {
                let a = region_of(cell_to_rid, r, c, w);
                let b = region_of(cell_to_rid, r + 1, c, w);
                if let (Some(ra), Some(rb)) = (a, b) {
                    if ra == rb || !edge_constraint_ok(ec, ra, rb, &area_of, &shape_key_of) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn edge_constraint_ok(
    ec: &EdgeConstraint,
    ra: usize,
    rb: usize,
    area_of: &dyn Fn(usize) -> Option<usize>,
    shape_key_of: &dyn Fn(usize) -> Option<String>,
) -> bool {
    let aa = area_of(ra).unwrap_or(0);
    let ab = area_of(rb).unwrap_or(0);
    match ec.ctype {
        EdgeConstraintType::Heterogeneous => {
            let sa = shape_key_of(ra);
            let sb = shape_key_of(rb);
            !(sa.is_some() && sa == sb)
        }
        EdgeConstraintType::Homogeneous => {
            let sa = shape_key_of(ra);
            let sb = shape_key_of(rb);
            sa.is_some() && sb.is_some() && sa == sb
        }
        EdgeConstraintType::Inequality => {
            let reversed = ec.value == Some(1);
            if reversed {
                aa > ab
            } else {
                aa < ab
            }
        }
        EdgeConstraintType::Difference => {
            let target = ec.value.unwrap_or(0) as usize;
            aa.abs_diff(ab) == target
        }
    }
}

/// Boundary bits around a region cell (up, down, left, right) in the solution.
fn region_boundary_bits(
    puzzle: &Puzzle,
    cell_to_rid: &[Option<usize>],
    w: usize,
    rid: usize,
    r: usize,
    c: usize,
) -> [bool; 4] {
    let h = puzzle.height;
    let mut bits = [false; 4];
    let neighbor = |nr: i64, nc: i64| -> bool {
        if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
            return true; // outer border
        }
        let nr = nr as usize;
        let nc = nc as usize;
        if puzzle.cells[nr][nc].blocked {
            return true;
        }
        match region_of(cell_to_rid, nr, nc, w) {
            Some(other) => other != rid,
            None => false,
        }
    };
    bits[0] = neighbor(r as i64 - 1, c as i64); // up
    bits[1] = neighbor(r as i64 + 1, c as i64); // down
    bits[2] = neighbor(r as i64, c as i64 - 1); // left
    bits[3] = neighbor(r as i64, c as i64 + 1); // right
    bits
}

/// 3x3 fence pattern from boundary bits (center + up/down/left/right).
///
/// `pub(crate)` so the backtrack solver can build the *same* cross mid-search
/// (in `check_fence_patterns`) and compare `dihedral_key`s byte-for-byte against
/// this leaf validator — keeping the two constructions from drifting.
pub(crate) fn fence_pattern_shape(bits: [bool; 4]) -> Vec<[usize; 2]> {
    let mut cells = vec![[1usize, 1usize]];
    if bits[0] {
        cells.push([0, 1]);
    }
    if bits[1] {
        cells.push([2, 1]);
    }
    if bits[2] {
        cells.push([1, 0]);
    }
    if bits[3] {
        cells.push([1, 2]);
    }
    cells
}

fn count_boundary_edges_at_vertex(
    puzzle: &Puzzle,
    cell_to_rid: &[Option<usize>],
    w_idx: usize,
    r: i32,
    c: i32,
) -> usize {
    let (h, w) = (puzzle.height as i32, puzzle.width as i32);
    let cell_region = |a: (i32, i32)| -> Option<usize> {
        if a.0 < 0 || a.1 < 0 || a.0 >= h || a.1 >= w {
            return None;
        }
        let cell = &puzzle.cells[a.0 as usize][a.1 as usize];
        if cell.blocked {
            return None;
        }
        region_of(cell_to_rid, a.0 as usize, a.1 as usize, w_idx)
    };
    let mut count = 0;
    // Four edges surrounding vertex (r,c): corner of cells
    // (r,c),(r,c+1),(r+1,c),(r+1,c+1) = geometric grid point (r+1,c+1).
    let is_bound = |a: (i32, i32), b: (i32, i32)| -> bool {
        let ra = cell_region(a);
        let rb = cell_region(b);
        match (ra, rb) {
            (Some(x), Some(y)) => x != y,
            // Both endpoints unassigned/outside = blocked cells sharing the
            // same empty space (or the outer border).  They are one entity,
            // not a region boundary — the C++ check_loopy compares the shared
            // AREA_BLOCK value (equal, so NOT a boundary).  Counting this edge
            // as a boundary inflated the junction count near blocked cells and
            // rejected valid ring solutions (e.g. 0678's 12-region tiling).
            (None, None) => false,
            _ => true,
        }
    };
    if is_bound((r, c), (r + 1, c)) {
        count += 1;
    }
    if is_bound((r, c), (r, c + 1)) {
        count += 1;
    }
    if is_bound((r, c + 1), (r + 1, c + 1)) {
        count += 1;
    }
    if is_bound((r + 1, c), (r + 1, c + 1)) {
        count += 1;
    }
    count
}
