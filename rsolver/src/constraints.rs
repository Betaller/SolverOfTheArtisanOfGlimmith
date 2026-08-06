//! Rule constraint checkers for puzzle validation.
//! Each function checks one rule against a completed set of regions.

use std::collections::{HashMap, HashSet};

use crate::shapes::{collect_pool_shapes, dihedral_key, is_rectangle};
use crate::types::*;

pub fn check_rule(rule: &Rule, puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    match rule.ctype.as_str() {
        "area" => check_area(puzzle, regions),
        "same" => check_same(regions),
        "different" => check_different(regions),
        "mixed" => check_mixed(puzzle, regions),
        "heterogeneous" => check_heterogeneous(regions),
        "homogeneous" => check_homogeneous(regions),
        "precise" => check_precise(regions, rule),
        "range" => check_range(regions, rule),
        "fence" => true,
        "solitary" => check_solitary(puzzle, regions),
        "block" => check_block(regions),
        "non_block" => check_nonblock(regions),
        "differentiation" => check_differentiation(puzzle, regions),
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
    // Collect from both the top-level array and the `shape_pool` rule params
    // (shared helper; some puzzles keep the pool only in the rule params,
    // e.g. A1-5 / C1-3).
    let pool = collect_pool_shapes(puzzle);
    if pool.is_empty() {
        return false;
    }
    let pool_keys: HashSet<String> = pool.iter().map(|s| dihedral_key(s)).collect();
    regions.iter().all(|reg| pool_keys.contains(&dihedral_key(&reg.cells)))
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

/// same: all regions are dihedrally congruent (mirrors Python
/// `check_rule_same` / aog::validate "same").
fn check_same(regions: &[RegionInfo]) -> bool {
    let mut keys: HashSet<String> = HashSet::new();
    for reg in regions {
        keys.insert(dihedral_key(&reg.shape));
    }
    keys.len() <= 1
}

/// different: all region shapes are dihedrally distinct (mirrors Python
/// `check_rule_different` / aog::validate "different").  A raw-shape
/// comparison missed rotation/reflection duplicates and accepted wrong
/// solutions (e.g. 1114's P-pentomino groups).
fn check_different(regions: &[RegionInfo]) -> bool {
    let mut keys: HashSet<String> = HashSet::new();
    for reg in regions {
        if !keys.insert(dihedral_key(&reg.shape)) {
            return false;
        }
    }
    true
}

/// mixed: every pair of ADJACENT regions has dihedrally different shapes
/// (mirrors Python `check_rule_mixed` / aog::validate "mixed" — NOT the
/// global "not all same shape" that this function previously implemented).
fn check_mixed(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    let key_of = |rid: usize| -> Option<String> {
        regions
            .iter()
            .find(|r| r.region_id == rid)
            .map(|r| dihedral_key(&r.shape))
    };
    let mut cell_to_rid: HashMap<(usize, usize), usize> = HashMap::new();
    for reg in regions {
        for &[r, c] in &reg.cells {
            cell_to_rid.insert((r, c), reg.region_id);
        }
    }
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    for reg in regions {
        for &[r, c] in &reg.cells {
            for (dr, dc) in [(1i64, 0i64), (0, 1i64)] {
                let nr = r as i64 + dr;
                let nc = c as i64 + dc;
                if nr >= 0 && nr < puzzle.height as i64 && nc >= 0 && nc < puzzle.width as i64 {
                    if let Some(&other) = cell_to_rid.get(&(nr as usize, nc as usize)) {
                        if other != reg.region_id {
                            let key = if reg.region_id < other {
                                (reg.region_id, other)
                            } else {
                                (other, reg.region_id)
                            };
                            if seen.insert(key) && key_of(reg.region_id) == key_of(other) {
                                return false;
                            }
                        }
                    }
                }
            }
        }
    }
    true
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

/// solitary: every region has exactly one clue-bearing cell.
fn check_solitary(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    for reg in regions {
        let mut clues = 0usize;
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
    true
}

/// block: every region is a solid rectangle.
fn check_block(regions: &[RegionInfo]) -> bool {
    regions.iter().all(|r| is_rectangle(&r.cells))
}

/// non_block: no region is a rectangle.
fn check_nonblock(regions: &[RegionInfo]) -> bool {
    regions.iter().all(|r| !is_rectangle(&r.cells))
}

/// differentiation: adjacent regions (sharing an edge) have different areas.
fn check_differentiation(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    let mut cell_to_rid: HashMap<(usize, usize), usize> = HashMap::new();
    for reg in regions {
        for &[r, c] in &reg.cells {
            cell_to_rid.insert((r, c), reg.region_id);
        }
    }
    let area_of = |rid: usize| -> usize {
        regions.iter().find(|r| r.region_id == rid).map(|r| r.area).unwrap_or(0)
    };
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    for reg in regions {
        for &[r, c] in &reg.cells {
            for (dr, dc) in [(1i64, 0i64), (0, 1i64)] {
                let nr = r as i64 + dr;
                let nc = c as i64 + dc;
                if nr >= 0 && nr < puzzle.height as i64 && nc >= 0 && nc < puzzle.width as i64 {
                    if let Some(&other) = cell_to_rid.get(&(nr as usize, nc as usize)) {
                        if other != reg.region_id {
                            let key = if reg.region_id < other {
                                (reg.region_id, other)
                            } else {
                                (other, reg.region_id)
                            };
                            if seen.insert(key) && area_of(reg.region_id) == area_of(other) {
                                return false;
                            }
                        }
                    }
                }
            }
        }
    }
    true
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
        // same shape → ok; different shape → fail (shape semantics, not area).
        let regions = vec![r(vec![[0, 0], [0, 1]], 2), r(vec![[0, 0], [0, 1]], 2)];
        assert!(check_same(&regions));
        let regions = vec![r(vec![[0, 0]], 1), r(vec![[0, 0], [0, 1]], 2)];
        assert!(!check_same(&regions));
    }

    #[test]
    fn test_check_different() {
        // distinct shapes → ok; duplicated shape → fail (shape semantics, not area).
        let regions = vec![r(vec![[0, 0]], 1), r(vec![[0, 0], [0, 1]], 2)];
        assert!(check_different(&regions));
        let regions = vec![r(vec![[0, 0]], 1), r(vec![[0, 0]], 1)];
        assert!(!check_different(&regions));
    }

    #[test]
    fn test_is_rectangle() {
        assert!(is_rectangle(&[[0, 0], [0, 1], [1, 0], [1, 1]]));
        assert!(is_rectangle(&[[0, 0], [0, 1], [0, 2], [1, 0], [1, 1], [1, 2]])); // 2x3
        assert!(!is_rectangle(&[[0, 0], [0, 1], [1, 0]])); // L triomino
        assert!(!is_rectangle(&[]));
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
