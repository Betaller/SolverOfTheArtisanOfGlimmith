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
    for r in 0..h {
        for c in 0..w {
            if puzzle.cells[r][c].blocked {
                continue;
            }
            let mut found = false;
            for reg in regions {
                if reg.cells.iter().any(|&[rr, cc]| rr == r && cc == c) {
                    by_rid.entry(reg.region_id).or_default().push([r, c]);
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

    let active: HashSet<&str> = puzzle.rules.iter().map(|r| r.ctype.as_str()).collect();

    // Pre-drawn boundaries separate regions.
    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            if puzzle.h_edges[r][c].is_boundary {
                let a = region_of(&by_rid, r, c);
                let b = region_of(&by_rid, r, c + 1);
                if a.is_some() && b.is_some() && a == b {
                    return false;
                }
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            if puzzle.v_edges[r][c].is_boundary {
                let a = region_of(&by_rid, r, c);
                let b = region_of(&by_rid, r + 1, c);
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

    for rule in &puzzle.rules {
        match rule.ctype.as_str() {
            "shape_pool" => {
                let pool: HashSet<String> = collect_pool_shapes(puzzle)
                    .iter()
                    .map(|s| dihedral_key(s))
                    .collect();
                for key in &shape_key_of {
                    if !pool.contains(key) {
                        return false;
                    }
                }
            }
            "precise" => {
                let target = rule.params.get("area").and_then(|v| v.as_i64()).unwrap_or(0);
                for reg in regions {
                    if reg.area as i64 != target {
                        return false;
                    }
                }
            }
            "range" => {
                let lo = rule.params.get("min").and_then(|v| v.as_i64()).unwrap_or(1);
                let hi = rule.params.get("max").and_then(|v| v.as_i64()).unwrap_or(i64::MAX);
                for reg in regions {
                    if (reg.area as i64) < lo || (reg.area as i64) > hi {
                        return false;
                    }
                }
            }
            "area" => {
                for r in 0..h {
                    for c in 0..w {
                        let cell = &puzzle.cells[r][c];
                        if let Some(n) = cell.number {
                            if let Some(rid) = region_of(&by_rid, r, c) {
                                if by_rid[&rid].len() != n as usize {
                                    return false;
                                }
                            }
                        }
                    }
                }
            }
            "same" => {
                let set: HashSet<&String> = shape_key_of.iter().collect();
                if set.len() > 1 {
                    return false;
                }
            }
            "different" => {
                let set: HashSet<&String> = shape_key_of.iter().collect();
                if set.len() != shape_key_of.len() {
                    return false;
                }
            }
            "mixed" => {
                if !adjacent_pairs_satisfy(puzzle, regions, &by_rid, |a, b| {
                    shape_key_of[a] != shape_key_of[b]
                }) {
                    return false;
                }
            }
            "differentiation" => {
                if !adjacent_pairs_satisfy(puzzle, regions, &by_rid, |a, b| {
                    regions[a].area != regions[b].area
                }) {
                    return false;
                }
            }
            "solitary" => {
                for reg in regions {
                    let mut clues = 0;
                    for &[r, c] in &reg.cells {
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
            "block" => {
                for reg in regions {
                    if !is_rectangle(&reg.cells) {
                        return false;
                    }
                }
            }
            "non_block" => {
                for reg in regions {
                    if is_rectangle(&reg.cells) {
                        return false;
                    }
                }
            }
            "puzzle_piece" => {
                for reg in regions {
                    for &[r, c] in &reg.cells {
                        if let Some(ref pat) = puzzle.cells[r][c].shape_pattern {
                            if dihedral_key(&reg.cells) != dihedral_key(pat) {
                                return false;
                            }
                        }
                    }
                }
            }
            "fence" => {
                for r in 0..h {
                    for c in 0..w {
                        let cell = &puzzle.cells[r][c];
                        if let Some(ref fp) = cell.fence_pattern {
                            if let Some(rid) = region_of(&by_rid, r, c) {
                                let bits = region_boundary_bits(puzzle, &by_rid, rid, r, c);
                                let pat = fence_pattern_shape(bits);
                                if dihedral_key(&pat) != dihedral_key(fp) {
                                    return false;
                                }
                            }
                        }
                    }
                }
            }
            "compass" => {
                for r in 0..h {
                    for c in 0..w {
                        let cell = &puzzle.cells[r][c];
                        if let Some(ref comp) = cell.compass {
                            if let Some(rid) = region_of(&by_rid, r, c) {
                                let cells = &by_rid[&rid];
                                for (dr, dc, attr) in
                                    [(-1i64, 0i64, 0usize), (1, 0, 1), (0, -1, 2), (0, 1, 3)]
                                {
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
            }
            "rose_window" => {
                if !check_rose_window(puzzle, regions) {
                    return false;
                }
            }
            "heterogeneous" | "homogeneous" | "inequality" | "difference" => {
                if !check_edge_constraints(puzzle, &by_rid) {
                    return false;
                }
            }
            "watchtower" => {
                // Vertex (r,c) is the ABSOLUTE grid corner (r in 0..=h,
                // c in 0..=w, border corners included).  The cells touching it
                // are the in-bounds ones of {(r-1,c-1),(r-1,c),(r,c-1),(r,c)}.
                // A border corner is touched by 2 (edge) or 1 (grid corner)
                // cells; blocked cells add no region (not in by_rid).
                for r in 0..=h {
                    for c in 0..=w {
                        if let Some(val) = puzzle.vertices[r][c].watchtower {
                            let mut distinct = HashSet::new();
                            for (dr, dc) in [(-1i64, -1i64), (-1, 0), (0, -1), (0, 0)] {
                                let nr = r as i64 + dr;
                                let nc = c as i64 + dc;
                                if nr < 0 || nc < 0 || nr >= h as i64 || nc >= w as i64 {
                                    continue;
                                }
                                if let Some(rid) = region_of(&by_rid, nr as usize, nc as usize) {
                                    distinct.insert(rid);
                                }
                            }
                            if distinct.len() != val as usize {
                                return false;
                            }
                        }
                    }
                }
            }
            "brick" => {
                // A 4-way junction can involve a blocked cell: its edges count
                // as border segments (blocked is a distinct value from every
                // region, though blocked-blocked shares one AREA_BLOCK and is
                // NOT a boundary — count_boundary_edges_at_vertex handles that).
                // So do NOT skip blocked vertices (a vertex with one blocked +
                // three distinct regions IS a 4-way, e.g. 1301's twin singleton
                // at (7,6)).  Mirrors the C++ check_tatami and the game.
                for r in 0..h.saturating_sub(1) {
                    for c in 0..w.saturating_sub(1) {
                        if count_boundary_edges_at_vertex(puzzle, &by_rid, r as i32, c as i32) == 4 {
                            return false;
                        }
                    }
                }
            }
            "ring" => {
                // A 3-way junction can form where an internal boundary meets
                // the OUTER border, so check every geometric grid point
                // (0..=h x 0..=w), not just interior vertices.  Vertex (r,c)
                // is geometric point (r+1,c+1), so r/c go from -1 to h-1/w-1.
                let hi = h as i32;
                let wi = w as i32;
                for r in -1..hi {
                    for c in -1..wi {
                        if count_boundary_edges_at_vertex(puzzle, &by_rid, r, c) == 3 {
                            return false;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let _ = active;
    true
}

fn region_of(by_rid: &HashMap<usize, Vec<[usize; 2]>>, r: usize, c: usize) -> Option<usize> {
    for (&rid, cells) in by_rid {
        if cells.iter().any(|&[rr, cc]| rr == r && cc == c) {
            return Some(rid);
        }
    }
    None
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
                    if let Some(other) = region_of(by_rid, nr, nc) {
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
) -> bool {
    let area_of = |rid: usize| by_rid.get(&rid).map(|c| c.len());
    let shape_key_of = |rid: usize| -> Option<String> {
        let cells = by_rid.get(&rid)?;
        Some(dihedral_key(cells))
    };
    // Iterate all edges with constraints.
    for r in 0..puzzle.height {
        for c in 0..puzzle.width.saturating_sub(1) {
            if let Some(ref ec) = puzzle.h_edges[r][c].constraint {
                let a = region_of(by_rid, r, c);
                let b = region_of(by_rid, r, c + 1);
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
                let a = region_of(by_rid, r, c);
                let b = region_of(by_rid, r + 1, c);
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
    by_rid: &HashMap<usize, Vec<[usize; 2]>>,
    rid: usize,
    r: usize,
    c: usize,
) -> [bool; 4] {
    let h = puzzle.height;
    let w = puzzle.width;
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
        match region_of(by_rid, nr, nc) {
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
fn fence_pattern_shape(bits: [bool; 4]) -> Vec<[usize; 2]> {
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
    by_rid: &HashMap<usize, Vec<[usize; 2]>>,
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
        region_of(by_rid, a.0 as usize, a.1 as usize)
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
