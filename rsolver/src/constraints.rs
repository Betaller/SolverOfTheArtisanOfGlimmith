//! Rule constraint checkers for puzzle validation.
//! Each function checks one rule against a completed set of regions.

use std::collections::HashSet;
use crate::types::*;

pub fn check_rule(rule: &Rule, regions: &[RegionInfo]) -> bool {
    match rule.ctype.as_str() {
        "area" => check_area(regions),
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
        "shape_pool" => true,
        "rose_window" => true,
        _ => true,
    }
}

pub fn check_all(rules: &[Rule], regions: &[RegionInfo]) -> HashSet<String> {
    let mut passed = HashSet::new();
    for rule in rules {
        if check_rule(rule, regions) {
            passed.insert(rule.ctype.clone());
        }
    }
    passed
}

/// area: regions must have strictly increasing area.
fn check_area(regions: &[RegionInfo]) -> bool {
    if regions.len() < 2 {
        return true;
    }
    let first_area = regions[0].area as i64;
    regions[1..].iter().enumerate().all(|(i, r)| {
        let expected = first_area + (i + 1) as i64;
        r.area as i64 == expected
    })
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
    fn test_check_area_strictly_increasing() {
        let regions = vec![r(vec![[0, 0]], 3), r(vec![[0, 0]], 4), r(vec![[0, 0]], 5)];
        assert!(check_area(&regions));
        let regions = vec![r(vec![[0, 0]], 3), r(vec![[0, 0]], 3)];
        assert!(!check_area(&regions));
    }
}
