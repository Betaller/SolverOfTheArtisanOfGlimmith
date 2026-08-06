//! Shared shape / region-size helpers.
//!
//! Single home for logic that used to be copy-pasted across solvers and
//! validators: canonical shape keys, rectangle detection, shape-pool
//! collection, region-area bounds, and rose symbol-type collection.  Keeping
//! one implementation per concept prevents the two copies from drifting apart
//! (as they did before the 2026-08-06 `check_mixed` / dihedral-key unification).

use crate::types::*;

/// True if the cells form a solid rectangle (any aspect ratio / size).
pub fn is_rectangle(cells: &[[usize; 2]]) -> bool {
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

/// Canonical dihedral shape key: the lexicographically smallest of the 8
/// rotations/reflections, origin-normalized.  All solvers and validators use
/// this as the identity for "same shape" comparisons.
pub fn dihedral_key(cells: &[[usize; 2]]) -> String {
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

/// Collect pool shapes from both the top-level array and `shape_pool` rule
/// params (some puzzles keep the pool only in the rule params, e.g. A1-5 /
/// C1-3).
pub fn collect_pool_shapes(puzzle: &Puzzle) -> Vec<Vec<[usize; 2]>> {
    let mut pool_shapes: Vec<Vec<[usize; 2]>> = puzzle.shape_pool.clone();
    for rule in &puzzle.rules {
        if rule.ctype != "shape_pool" {
            continue;
        }
        if let Some(shapes) = rule.params.get("shapes").and_then(|v| v.as_array()) {
            for s in shapes {
                let cells: Vec<[usize; 2]> = s
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|xy| {
                                let cc = xy.as_array()?;
                                if cc.len() < 2 {
                                    return None;
                                }
                                let r = cc[0].as_i64()?;
                                let c = cc[1].as_i64()?;
                                Some([r.max(0) as usize, c.max(0) as usize])
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if !cells.is_empty() {
                    pool_shapes.push(cells);
                }
            }
        }
    }
    pool_shapes
}

/// Global region-area bounds `(min, max)` from `precise` / `range` rules and
/// compass clues.  These are valid bounds for ANY region, so every solver can
/// use them to prune candidates (pieces / backtrack / rose region_match).
///
/// `block` (regions are rectangles of any size) and `solitary` (one clue cell
/// per region) deliberately do NOT constrain area — forcing block→4..4 or
/// solitary→1..1 made structurally-unsolvable puzzles in the past.
pub fn area_bounds(puzzle: &Puzzle) -> (usize, usize) {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut min_a: usize = 1;
    let mut max_a: usize = h * w;

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
            _ => {}
        }
    }

    // Compass clues imply a minimum region area: the clue cell itself plus one
    // cell per direction it counts.
    for r in 0..h {
        for c in 0..w {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                let needed = 1 + comp.up.unwrap_or(0) as usize + comp.down.unwrap_or(0) as usize
                    + comp.left.unwrap_or(0) as usize + comp.right.unwrap_or(0) as usize;
                min_a = min_a.max(needed);
            }
        }
    }

    (min_a, max_a)
}

/// Symbol types from the `rose_window` rule params, else the sorted distinct
/// cell symbols.  A non-empty `symbol_types` array wins; an empty array falls
/// back to the grid symbols.  This unified semantics is shared by aog (symbol
/// encoding), rose (solving) and the validator (acceptance).
pub fn rose_symbol_types(puzzle: &Puzzle) -> Vec<String> {
    let rule = puzzle.rules.iter().find(|r| r.ctype == "rose_window");
    let Some(rule) = rule else {
        return Vec::new();
    };
    if let Some(arr) = rule.params.get("symbol_types").and_then(|v| v.as_array()) {
        let t: Vec<String> = arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect();
        if !t.is_empty() {
            return t;
        }
    }
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

