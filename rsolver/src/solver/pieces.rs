//! Piece-based solver using DLX exact cover.
//!
//! Generates all valid shape placements from cell clues (area numbers, compass,
//! shape pool), builds a DLX matrix (columns = cells), and finds exact cover.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use crate::dlx::DancingLinks;
use crate::polyomino;
use crate::types::*;

#[derive(Debug, Clone)]
struct Placement {
    cells: Vec<[usize; 2]>,
    area: usize,
    shape: Shape,
    cell_ids_flat: Vec<usize>,
}

struct SolveContext {
    cell_to_idx: Vec<Vec<usize>>,
    num_cells: usize,
    eff_min_area: usize,
    eff_max_area: usize,
    watchtowers: Vec<(Vec<[usize; 2]>, usize)>,
    edge_constraints: Vec<([usize; 2], [usize; 2], &'static str, Option<i64>)>,
    fillable: Vec<[usize; 2]>,
}

const MAX_COMPASS_PLACEMENTS: usize = 2000;
/// Area-number targets above this are left to the backtracker: enumerating all
/// connected polyominoes of a large size explodes (e.g. area 48 on 1301).  This
/// matches the Python ExactCoverSolver threshold (max(targets) <= 12).
const MAX_AREA_TARGET: usize = 12;

pub fn solve_pieces(puzzle: &Puzzle, _start: &Instant, timeout_ms: u64) -> Option<Vec<RegionInfo>> {
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);

    if !has_clues(puzzle) && puzzle.shape_pool.is_empty() {
        return None; // fall back to backtrack
    }

    let ctx = build_context(puzzle);
    let placements = generate_all_placements(puzzle, &ctx);

    if placements.is_empty() {
        if ctx.num_cells == 0 {
            return Some(Vec::new());
        }
        return None;
    }

    // Build DLX
    let mut dlx = DancingLinks::new(ctx.num_cells);
    for (i, p) in placements.iter().enumerate() {
        dlx.add_row(&p.cell_ids_flat, i);
    }
    dlx.set_deadline(deadline);

    // Search, validating every complete tiling against the global rules
    // (edge constraints, watchtowers, and — via `validate.rs` — every rule
    // type, so a tiling that violates fence / compass / ring / rose_window is
    // rejected here instead of slipping to the router).  Keep searching until
    // a valid tiling is found or the deadline expires.  This lets a block
    // puzzle (routed here through a synthesized rectangle pool) land on a
    // *valid* rectangle partition instead of the first (usually trivial
    // all-1×1) one.
    let mut result: Option<Vec<RegionInfo>> = None;
    let mut partial: Vec<usize> = Vec::new();
    let mut row_check = |_partial: &[usize]| true;
    let mut on_solution = |row_ids: &[usize]| {
        match reconstruct_and_validate(puzzle, &placements, row_ids, &ctx) {
            Some(regions) => {
                result = Some(regions);
                true // stop the search
            }
            None => false, // keep looking
        }
    };
    dlx.search_with_check(0, &mut partial, &mut row_check, &mut on_solution);

    result
}

fn has_clues(puzzle: &Puzzle) -> bool {
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            let cell = &puzzle.cells[r][c];
            if cell.number.is_some() || cell.compass.is_some() {
                return true;
            }
        }
    }
    false
}

fn build_context(puzzle: &Puzzle) -> SolveContext {
    let h = puzzle.height;
    let w = puzzle.width;

    let mut cell_to_idx = vec![vec![usize::MAX; w]; h];
    let mut num_cells = 0;
    let fillable: Vec<[usize; 2]> = (0..h)
        .flat_map(|r| (0..w).map(move |c| (r, c)))
        .filter(|&(r, c)| puzzle.cells[r][c].fillable())
        .map(|(r, c)| {
            cell_to_idx[r][c] = num_cells;
            num_cells += 1;
            [r, c]
        })
        .collect();

    let (eff_min, eff_max) = crate::shapes::area_bounds(puzzle);

    let watchtowers = collect_watchtower_data(puzzle);
    let edge_constraints = collect_edge_constraints_data(puzzle);

    SolveContext {
        cell_to_idx,
        num_cells,
        eff_min_area: eff_min,
        eff_max_area: eff_max,
        watchtowers,
        edge_constraints,
        fillable,
    }
}

fn collect_watchtower_data(puzzle: &Puzzle) -> Vec<(Vec<[usize; 2]>, usize)> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut result = Vec::new();
    // Vertex (r,c) = ABSOLUTE grid corner (r in 0..=h, c in 0..=w).  Cells
    // touching it: in-bounds, non-blocked members of {(r-1,c-1),(r-1,c),
    // (r,c-1),(r,c)}.  Border corners are touched by 2 (edge) / 1 (corner).
    for r in 0..=h {
        for c in 0..=w {
            if let Some(val) = puzzle.vertices[r][c].watchtower {
                let v = val as usize;
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

fn collect_edge_constraints_data(
    puzzle: &Puzzle,
) -> Vec<([usize; 2], [usize; 2], &'static str, Option<i64>)> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut result = Vec::new();

    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            if let Some(ref ec) = puzzle.h_edges[r][c].constraint {
                let kind = match ec.ctype {
                    EdgeConstraintType::Inequality => "inequality",
                    EdgeConstraintType::Difference => "difference",
                    EdgeConstraintType::Heterogeneous => "delta",
                    EdgeConstraintType::Homogeneous => "gemini",
                };
                result.push(([r, c], [r, c + 1], kind, ec.value));
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            if let Some(ref ec) = puzzle.v_edges[r][c].constraint {
                let kind = match ec.ctype {
                    EdgeConstraintType::Inequality => "inequality",
                    EdgeConstraintType::Difference => "difference",
                    EdgeConstraintType::Heterogeneous => "delta",
                    EdgeConstraintType::Homogeneous => "gemini",
                };
                result.push(([r, c], [r + 1, c], kind, ec.value));
            }
        }
    }
    result
}

fn generate_all_placements(puzzle: &Puzzle, ctx: &SolveContext) -> Vec<Placement> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut placements = Vec::new();

    // 1. Shape pool placements (if any)
    if !puzzle.shape_pool.is_empty() {
        for shape in &puzzle.shape_pool {
            let transforms = polyomino::transforms(shape);
            let area = shape.len();
            for transform in &transforms {
                let offset_drs: Vec<[isize; 2]> = transform.clone();
                for r in 0..h {
                    for c in 0..w {
                        if let Some(cells) = try_place(&offset_drs, r, c, h, w, puzzle) {
                            let flat = cells_to_flat(&cells, ctx);
                            placements.push(Placement {
                                cells,
                                area,
                                shape: shape.clone(),
                                cell_ids_flat: flat,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. Area number placements
    for r in 0..h {
        for c in 0..w {
            if let Some(area) = puzzle.cells[r][c].number {
                let target = area as usize;
                if target < ctx.eff_min_area || target > ctx.eff_max_area {
                    continue;
                }
                if target > MAX_AREA_TARGET {
                    // Too large for DLX candidate generation — leave to backtrack.
                    continue;
                }
                let mut results = Vec::new();
                generate_polyominoes(puzzle, r, c, target, &mut results);
                for cells in results {
                    let mut canonical = cells.clone();
                    normalize(&mut canonical);
                    let flat = cells_to_flat(&cells, ctx);
                    placements.push(Placement {
                        cells,
                        area: target,
                        shape: canonical,
                        cell_ids_flat: flat,
                    });
                }
            }
        }
    }

    // 3. Compass clue placements (only for highly constrained clues)
    for r in 0..h {
        for c in 0..w {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                let spec_count = count_specified(comp);
                let is_strip = (comp.east_west_strip()) || (comp.north_south_strip());
                if spec_count < 3 && !is_strip {
                    continue; // too loosely constrained
                }

                let mut results = generate_compass_polyominoes(puzzle, r, c, comp);
                // D8: truncate instead of discarding when too many compass placements
                // are generated.  The first N placements are still useful; discarding
                // all of them (old `continue`) skipped DLX entirely for these clues.
                // (doc 16 §2 D8.)
                if results.len() > MAX_COMPASS_PLACEMENTS {
                    results.truncate(MAX_COMPASS_PLACEMENTS);
                }
                for cells in results {
                    let area = cells.len();
                    let mut canonical = cells.clone();
                    normalize(&mut canonical);
                    let flat = cells_to_flat(&cells, ctx);
                    placements.push(Placement {
                        cells,
                        area,
                        shape: canonical,
                        cell_ids_flat: flat,
                    });
                }
            }
        }
    }

    // Deduplicate
    let mut seen: HashSet<Vec<usize>> = HashSet::new();
    placements.retain(|p| {
        let mut ids = p.cell_ids_flat.clone();
        ids.sort();
        seen.insert(ids)
    });

    placements
}

impl CompassClue {
    fn east_west_strip(&self) -> bool {
        self.right == Some(0) && self.left == Some(0)
    }
    fn north_south_strip(&self) -> bool {
        self.up == Some(0) && self.down == Some(0)
    }
}

fn count_specified(comp: &CompassClue) -> usize {
    [comp.up, comp.down, comp.right, comp.left]
        .iter()
        .filter(|v| v.is_some())
        .count()
}

fn try_place(
    offsets: &[[isize; 2]],
    r: usize,
    c: usize,
    h: usize,
    w: usize,
    puzzle: &Puzzle,
) -> Option<Vec<[usize; 2]>> {
    let mut cells = Vec::with_capacity(offsets.len());
    for &[dr, dc] in offsets {
        let nr = r as isize + dr;
        let nc = c as isize + dc;
        if nr < 0 || nr >= h as isize || nc < 0 || nc >= w as isize {
            return None;
        }
        let nr = nr as usize;
        let nc = nc as usize;
        if puzzle.cells[nr][nc].blocked {
            return None;
        }
        cells.push([nr, nc]);
    }
    Some(cells)
}

fn cells_to_flat(cells: &[[usize; 2]], ctx: &SolveContext) -> Vec<usize> {
    cells.iter().map(|&[r, c]| ctx.cell_to_idx[r][c]).collect()
}

/// Generate all connected polyominoes of exactly `size` cells containing `(sr, sc)`.
fn generate_polyominoes(
    puzzle: &Puzzle,
    sr: usize,
    sc: usize,
    size: usize,
    results: &mut Vec<Vec<[usize; 2]>>,
) {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut current = vec![[sr, sc]];
    let mut candidates = BTreeSet::new();

    for (nr, nc) in neighbor_positions(sr, sc, h, w) {
        if !puzzle.cells[nr][nc].blocked && !is_precut(puzzle, sr, sc, nr, nc) {
            candidates.insert([nr, nc]);
        }
    }

    poly_rec(puzzle, &mut current, &mut candidates, size, results);
}

fn poly_rec(
    puzzle: &Puzzle,
    current: &mut Vec<[usize; 2]>,
    candidates: &mut BTreeSet<[usize; 2]>,
    size: usize,
    results: &mut Vec<Vec<[usize; 2]>>,
) {
    if current.len() == size {
        results.push(current.clone());
        return;
    }
    if candidates.is_empty() {
        return;
    }

    let h = puzzle.height;
    let w = puzzle.width;
    let mut my_candidates = candidates.clone();

    while let Some(&next) = my_candidates.iter().next() {
        my_candidates.remove(&next);
        candidates.remove(&next);

        let mut added = Vec::new();
        for (nr, nc) in neighbor_positions(next[0], next[1], h, w) {
            let pos = [nr, nc];
            if puzzle.cells[nr][nc].blocked {
                continue;
            }
            if is_precut(puzzle, next[0], next[1], nr, nc) {
                continue;
            }
            if current.contains(&pos) || my_candidates.contains(&pos) {
                continue;
            }
            if candidates.insert(pos) {
                added.push(pos);
            }
        }

        current.push(next);
        poly_rec(puzzle, current, candidates, size, results);
        current.pop();

        for a in added {
            candidates.remove(&a);
        }
    }
}

/// Generate all connected polyominoes containing `(sr, sc)` that satisfy compass constraints.
fn generate_compass_polyominoes(
    puzzle: &Puzzle,
    sr: usize,
    sc: usize,
    compass: &CompassClue,
) -> Vec<Vec<[usize; 2]>> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut results = Vec::new();
    let mut current = vec![[sr, sc]];
    let mut counts = [0usize; 4]; // N=0, S=1, E=2, W=3
    let mut candidates = BTreeSet::new();

    let sri = sr as isize;
    let sci = sc as isize;

    for (nr, nc) in neighbor_positions(sr, sc, h, w) {
        if !puzzle.cells[nr][nc].blocked && !is_precut(puzzle, sr, sc, nr, nc) {
            candidates.insert([nr, nc]);
        }
    }

    compass_rec(
        puzzle, &mut current, &mut counts, &mut candidates,
        sri, sci, compass, &mut results,
    );
    results
}

fn compass_rec(
    puzzle: &Puzzle,
    current: &mut Vec<[usize; 2]>,
    counts: &mut [usize; 4],
    candidates: &mut BTreeSet<[usize; 2]>,
    cr_i: isize,
    cc_i: isize,
    compass: &CompassClue,
    results: &mut Vec<Vec<[usize; 2]>>,
) {
    // Check if any direction exceeds its compass value
    if counts[0] > compass.up.unwrap_or(0) as usize
        || counts[1] > compass.down.unwrap_or(0) as usize
        || counts[2] > compass.right.unwrap_or(0) as usize
        || counts[3] > compass.left.unwrap_or(0) as usize
    {
        return;
    }

    // Check if all specified directions are exactly satisfied
    // (Note: in our format, 0 means "not specified" BUT ONLY if the compass is from the I/O layer
    //  For compass clues, all values default to 0, meaning "no constraint in that direction")
    let all_satisfied = compass.up.unwrap_or(0) == counts[0] as i64
        && compass.down.unwrap_or(0) == counts[1] as i64
        && compass.right.unwrap_or(0) == counts[2] as i64
        && compass.left.unwrap_or(0) == counts[3] as i64;

    if all_satisfied && current.len() > 1 {
        results.push(current.clone());
        // Continue: can grow in unspecified... but in our format all values are specified
        // as 0 (meaning "exactly 0") or non-zero. So if all_satisfied, we're done.
        return;
    }

    if candidates.is_empty() {
        return;
    }

    // D7: precise compass region size upper bound = 1 (compass cell) + cells in
    // each direction (up+down+left+right). None directions contribute 0 (matching
    // the count check above, which treats None as "0 cells allowed"). This is far
    // tighter than the old `current.len() + 20` heuristic and terminates the
    // placement DFS as soon as the region can't grow further. (doc 16 §2 D7.)
    let max_sz = 1usize
        + compass.up.unwrap_or(0) as usize
        + compass.down.unwrap_or(0) as usize
        + compass.left.unwrap_or(0) as usize
        + compass.right.unwrap_or(0) as usize;
    if current.len() >= max_sz {
        return;
    }

    if results.len() >= MAX_COMPASS_PLACEMENTS {
        return;
    }

    let h = puzzle.height;
    let w = puzzle.width;
    let mut my_candidates = candidates.clone();

    while let Some(&next) = my_candidates.iter().next() {
        my_candidates.remove(&next);
        candidates.remove(&next);

        let dr = next[0] as isize - cr_i;
        let dc = next[1] as isize - cc_i;

        let dir_idx = if dr < 0 {
            0
        } else if dr > 0 {
            1
        } else if dc > 0 {
            2
        } else {
            3
        };

        // Check direction limit BEFORE recursing
        let at_limit = match dir_idx {
            0 => compass.up.map_or(false, |v| counts[0] >= v as usize),
            1 => compass.down.map_or(false, |v| counts[1] >= v as usize),
            2 => compass.right.map_or(false, |v| counts[2] >= v as usize),
            3 => compass.left.map_or(false, |v| counts[3] >= v as usize),
            _ => false,
        };

        // Find neighbors reachable only through `next`
        let mut added = Vec::new();
        for (nr, nc) in neighbor_positions(next[0], next[1], h, w) {
            let pos = [nr, nc];
            if puzzle.cells[nr][nc].blocked {
                continue;
            }
            if is_precut(puzzle, next[0], next[1], nr, nc) {
                continue;
            }
            if current.contains(&pos) || my_candidates.contains(&pos) {
                continue;
            }
            if candidates.insert(pos) {
                added.push(pos);
            }
        }

        if at_limit {
            for a in added {
                candidates.remove(&a);
            }
            continue;
        }

        counts[dir_idx] += 1;
        current.push(next);
        compass_rec(puzzle, current, counts, candidates, cr_i, cc_i, compass, results);
        current.pop();
        counts[dir_idx] -= 1;

        for a in added {
            candidates.remove(&a);
        }
    }
}

fn is_precut(puzzle: &Puzzle, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
    if r1 == r2 {
        let c = c1.min(c2);
        puzzle.h_edges[r1][c].is_boundary
    } else {
        let r = r1.min(r2);
        puzzle.v_edges[r][c1].is_boundary
    }
}

fn reconstruct_and_validate(
    puzzle: &Puzzle,
    placements: &[Placement],
    row_ids: &[usize],
    ctx: &SolveContext,
) -> Option<Vec<RegionInfo>> {
    let mut piece_of_cell: HashMap<(usize, usize), usize> = HashMap::new();
    for (i, &row_id) in row_ids.iter().enumerate() {
        for &[r, c] in &placements[row_id].cells {
            piece_of_cell.insert((r, c), i);
        }
    }

    // Coverage check
    for &[r, c] in &ctx.fillable {
        if !piece_of_cell.contains_key(&(r, c)) {
            return None;
        }
    }

    // Edge constraints
    for &(c1, c2, kind, val) in &ctx.edge_constraints {
        let p1 = piece_of_cell.get(&(c1[0], c1[1]));
        let p2 = piece_of_cell.get(&(c2[0], c2[1]));
        let p1 = match p1 { Some(&v) => v, None => continue };
        let p2 = match p2 { Some(&v) => v, None => continue };
        if p1 == p2 { continue; }

        let a1 = placements[row_ids[p1]].area;
        let a2 = placements[row_ids[p2]].area;
        match kind {
            "inequality" => { if a1 == a2 { return None; } }
            "difference" => {
                let diff = (a1 as i64 - a2 as i64).unsigned_abs() as usize;
                if let Some(target) = val {
                    if diff != target as usize { return None; }
                }
            }
            "delta" => {
                if placements[row_ids[p1]].shape == placements[row_ids[p2]].shape { return None; }
            }
            "gemini" => {
                if placements[row_ids[p1]].shape != placements[row_ids[p2]].shape { return None; }
            }
            _ => {}
        }
    }

    // Watchtowers
    for (ref cells, target) in &ctx.watchtowers {
        let mut pieces = Vec::new();
        for &[r, c] in cells {
            if let Some(&p) = piece_of_cell.get(&(r, c)) {
                if !pieces.contains(&p) { pieces.push(p); }
            }
        }
        if pieces.len() != *target { return None; }
    }

    let regions: Vec<RegionInfo> = row_ids.iter().enumerate().map(|(rid, &row_id)| {
        let p = &placements[row_id];
        let mut norm = p.shape.clone();
        normalize(&mut norm);
        RegionInfo {
            region_id: rid,
            cells: p.cells.clone(),
            area: p.area,
            shape: norm,
            normalized_shape_key: canonical_key(&p.shape),
            matched_shape_name: None,
        }
    }).collect();

    // Full independent re-validation via `validate.rs` — the same acceptance
    // gate aog and rose use.  Covers every rule type, so a DLX tiling that
    // violates fence / compass / ring / rose_window (previously stubbed in
    // `constraints.rs`) is rejected here instead of slipping to the router.
    if !crate::solver::validate::validate(puzzle, &regions) {
        return None;
    }

    Some(regions)
}
