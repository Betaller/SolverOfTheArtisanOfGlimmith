//! Rule constraint checkers for puzzle validation.
//! Each function checks one rule against a completed set of regions.

use std::collections::HashSet;
use crate::types::*;

pub fn check_rule(rule: &Rule, puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    match rule.ctype.as_str() {
        "area" => check_area(puzzle, regions),
        "same" => check_same(regions),
        "different" => check_different(regions),
        "mixed" => check_mixed(regions),
        "heterogeneous" => check_heterogeneous(regions),
        "homogeneous" => check_homogeneous(regions),
        "precise" => check_precise(regions, rule),
        "range" => check_range(regions, rule),
        "fence" => true,
        "solitary" => check_solitary(regions),
        "block" => check_block(regions),
        "non_block" => check_nonblock(regions),
        "differentiation" => check_differentiation(regions),
        "brick" => true,
        "ring" => true,
        "inequality" => true,
        "difference" => true,
        "watchtower" => true,
        "compass" => true,
        "puzzle_piece" => true,
        "shape_pool" => check_shape_pool(puzzle, regions),
        "rose_window" => true,
        _ => true,
    }
}

pub fn check_all(puzzle: &Puzzle, rules: &[Rule], regions: &[RegionInfo]) -> HashSet<String> {
    let mut passed = HashSet::new();
    for rule in rules {
        if check_rule(rule, puzzle, regions) {
            passed.insert(rule.ctype.clone());
        }
    }
    passed
}

/// shape_pool: every region must be (dihedrally) congruent to one of the pool shapes.
fn check_shape_pool(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    // Collect from both the top-level array and the `shape_pool` rule params,
    // matching aog::core::collect_pool_shapes (some puzzles keep the pool only
    // in the rule params, e.g. A1-5 / C1-3).
    let mut pool: Vec<Vec<[usize; 2]>> = puzzle.shape_pool.clone();
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
                    pool.push(cells);
                }
            }
        }
    }
    if pool.is_empty() {
        return false;
    }
    let pool_keys: HashSet<String> = pool.iter().map(|s| dihedral_key(s)).collect();
    regions.iter().all(|reg| pool_keys.contains(&dihedral_key(&reg.cells)))
}

/// Canonical dihedral shape key: the lexicographically smallest of the 8
/// rotations/reflections, origin-normalized.  Mirrors the aog validate.rs
/// helper so both solvers agree on shape equivalence.
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

/// area: every cell carrying a number clue must belong to a region of exactly
/// that size (the game's `area` rule), not "regions have strictly increasing
/// areas" as this function originally claimed.
fn check_area(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    for reg in regions {
        for &[r, c] in &reg.cells {
            if let Some(n) = puzzle.cells[r][c].number {
                if n as usize != reg.area {
                    return false;
                }
            }
        }
    }
    true
}

/// same: all regions have the same area.
fn check_same(regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    let a = regions[0].area;
    regions.iter().all(|r| r.area == a)
}

/// different: all regions have different areas.
fn check_different(regions: &[RegionInfo]) -> bool {
    let set: HashSet<usize> = regions.iter().map(|r| r.area).collect();
    set.len() == regions.len()
}

/// mixed: not all regions have the same area.
fn check_mixed(regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    !check_same(regions)
}

/// heterogeneous: not all regions have the same shape.
fn check_heterogeneous(regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    let all_same = regions.windows(2).all(|w| w[0].shape == w[1].shape);
    !all_same
}

/// homogeneous: all regions have the same shape.
fn check_homogeneous(regions: &[RegionInfo]) -> bool {
    regions.windows(2).all(|w| w[0].shape == w[1].shape)
}

/// precise: every region has exactly the specified area.
fn check_precise(regions: &[RegionInfo], rule: &Rule) -> bool {
    let target = rule.params.get("area").and_then(|v| v.as_i64()).unwrap_or(0);
    regions.iter().all(|r| r.area as i64 == target)
}

/// range: every region's area is within [min, max].
fn check_range(regions: &[RegionInfo], rule: &Rule) -> bool {
    let min = rule.params.get("min").and_then(|v| v.as_i64()).unwrap_or(1);
    let max = rule.params.get("max").and_then(|v| v.as_i64()).unwrap_or(i64::MAX);
    regions.iter().all(|r| {
        let a = r.area as i64;
        a >= min && a <= max
    })
}

/// solitary: every region has exactly 1 cell.
fn check_solitary(regions: &[RegionInfo]) -> bool {
    regions.iter().all(|r| r.area == 1)
}

/// block: every region is a 2x2 square.
fn check_block(regions: &[RegionInfo]) -> bool {
    let block: Shape = vec![[0, 0], [0, 1], [1, 0], [1, 1]];
    regions.iter().all(|r| r.shape == block)
}

/// non_block: not all regions are 2x2 squares.
fn check_nonblock(regions: &[RegionInfo]) -> bool {
    !check_block(regions)
}

/// differentiation: all regions have different shapes.
fn check_differentiation(regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    let shapes: HashSet<&Shape> = regions.iter().map(|r| &r.shape).collect();
    shapes.len() == regions.len()
}

// Stubs for edge/vertex/cell-based rules (require board state, not just regions):
// - fence: shape pattern placement check
// - brick: 2x1 or 1x2 domino alternation
// - ring: perimeter check
// - inequality: edge-based comparison of adjacent regions
// - difference: edge-based numeric difference
// - watchtower: vertex-based count of region edges
// - compass: cell-based direction counts
// - puzzle_piece: irregular shapes check
// - shape_pool: all regions match given shape pool
// - rose_window: symbol-based region grouping

#[cfg(test)]
mod tests {
    use super::*;

    fn r(cells: Vec<[usize; 2]>, area: usize) -> RegionInfo {
        RegionInfo {
            region_id: 0,
            area,
            shape: cells,
            cells: Vec::new(),
            normalized_shape_key: String::new(),
            matched_shape_name: None,
        }
    }

    #[test]
    fn test_check_same() {
        let regions = vec![r(vec![[0, 0]], 3), r(vec![[0, 0]], 3), r(vec![[0, 0]], 3)];
        assert!(check_same(&regions));
        let regions = vec![r(vec![[0, 0]], 3), r(vec![[0, 0]], 4)];
        assert!(!check_same(&regions));
    }

    #[test]
    fn test_check_different() {
        let regions = vec![r(vec![[0, 0]], 3), r(vec![[0, 0]], 4), r(vec![[0, 0]], 5)];
        assert!(check_different(&regions));
        let regions = vec![r(vec![[0, 0]], 3), r(vec![[0, 0]], 3)];
        assert!(!check_different(&regions));
    }

    #[test]
    fn test_check_area_number_clues() {
        // area: a numbered cell must be in a region of exactly that size.
        let mut cells = vec![vec![Cell::new(0, 0), Cell::new(0, 1)]];
        cells[0][0].number = Some(2);
        cells[0][1].number = Some(2);
        let puzzle = Puzzle {
            height: 1,
            width: 2,
            cells,
            h_edges: Vec::new(),
            v_edges: Vec::new(),
            vertices: Vec::new(),
            rules: Vec::new(),
            shape_pool: Vec::new(),
            outer_boundaries: Vec::new(),
        };
        let ok = vec![RegionInfo {
            region_id: 1,
            cells: vec![[0, 0], [0, 1]],
            area: 2,
            shape: vec![[0, 0], [0, 1]],
            normalized_shape_key: String::new(),
            matched_shape_name: None,
        }];
        assert!(check_area(&puzzle, &ok));
        let bad = vec![RegionInfo {
            region_id: 1,
            cells: vec![[0, 0], [0, 1]],
            area: 3, // wrong size
            shape: vec![[0, 0], [0, 1]],
            normalized_shape_key: String::new(),
            matched_shape_name: None,
        }];
        assert!(!check_area(&puzzle, &bad));
    }
}
