//! Cells-based rule validation, ported from the Python `IndependentValidator`.
//!
//! The AoG search works on the C++-style padded grid; this validates the
//! extracted regions against the rsolver `Puzzle` model so a buggy fill can
//! never be reported as a solution.

use std::collections::{HashMap, HashSet};

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
                let pool: HashSet<String> = super::core::collect_pool_shapes(puzzle)
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
                if !check_edge_constraints(puzzle, regions, &by_rid) {
                    return false;
                }
            }
            "watchtower" => {
                for r in 0..h.saturating_sub(1) {
                    for c in 0..w.saturating_sub(1) {
                        if let Some(val) = puzzle.vertices[r][c].watchtower {
                            let mut distinct = HashSet::new();
                            for (dr, dc) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                                if let Some(rid) = region_of(&by_rid, r + dr, c + dc) {
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
                for r in 0..h.saturating_sub(1) {
                    for c in 0..w.saturating_sub(1) {
                        if count_boundary_edges_at_vertex(puzzle, &by_rid, r, c) == 4 {
                            return false;
                        }
                    }
                }
            }
            "ring" => {
                for r in 0..h.saturating_sub(1) {
                    for c in 0..w.saturating_sub(1) {
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
    // Determine symbol types.
    let types: Vec<String> = if let Some(rule) = puzzle.rules.iter().find(|r| r.ctype == "rose_window") {
        if let Some(arr) = rule.params.get("symbol_types").and_then(|v| v.as_array()) {
            arr.iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect()
        } else {
            let mut s: Vec<String> = puzzle
                .cells
                .iter()
                .flatten()
                .filter_map(|c| c.symbol.clone())
                .collect();
            s.sort();
            s.dedup();
            s
        }
    } else {
        return true;
    };
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

/// Dihedral-normalized shape key (all rotations + reflections).
fn dihedral_key(cells: &[[usize; 2]]) -> String {
    let signed: Vec<(isize, isize)> = cells
        .iter()
        .map(|&[r, c]| (r as isize, c as isize))
        .collect();
    let mut best: Option<String> = None;
    for &rot in &[0, 1, 2, 3] {
        for &refl in &[false, true] {
            let mut t: Vec<(isize, isize)> = signed
                .iter()
                .map(|&(r, c)| {
                    let (mut rr, mut cc) = (r, c);
                    if refl {
                        cc = -cc;
                    }
                    for _ in 0..rot {
                        (rr, cc) = (-cc, rr);
                    }
                    (rr, cc)
                })
                .collect();
            let min_r = t.iter().map(|x| x.0).min().unwrap_or(0);
            let min_c = t.iter().map(|x| x.1).min().unwrap_or(0);
            for p in &mut t {
                p.0 -= min_r;
                p.1 -= min_c;
            }
            t.sort();
            let key = t
                .iter()
                .map(|&(r, c)| format!("({},{})", r, c))
                .collect::<String>();
            if best.as_ref().map_or(true, |b| &key < b) {
                best = Some(key);
            }
        }
    }
    best.unwrap_or_default()
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

fn is_rectangle(cells: &[[usize; 2]]) -> bool {
    if cells.is_empty() {
        return false;
    }
    let mut min_r = usize::MAX;
    let mut max_r = 0usize;
    let mut min_c = usize::MAX;
    let mut max_c = 0usize;
    for &[r, c] in cells {
        min_r = min_r.min(r);
        max_r = max_r.max(r);
        min_c = min_c.min(c);
        max_c = max_c.max(c);
    }
    cells.len() == (max_r - min_r + 1) * (max_c - min_c + 1)
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
    regions: &[RegionInfo],
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
    r: usize,
    c: usize,
) -> usize {
    let mut count = 0;
    // Four edges surrounding vertex (r,c).
    let mut is_bound = |a: (usize, usize), b: (usize, usize)| -> bool {
        let ra = region_of(by_rid, a.0, a.1);
        let rb = region_of(by_rid, b.0, b.1);
        match (ra, rb) {
            (Some(x), Some(y)) => x != y,
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
