//! Rose-window solver — port of `src/solver/rose/solver.py` +
//! `src/solver/region_match.py` + `src/solver/rose_growth.py`.
//!
//! The AoG solver times out on "rose_window without size constraint" puzzles
//! (e.g. C4-1, 0277); this module solves them by deducing the region count from
//! symbol multiplicities and matching/growing regions symbol-by-symbol.

pub mod cells;
pub mod region_match;
pub mod rose_growth;

use std::collections::HashMap;
use std::time::Instant;

use crate::types::{Puzzle, RegionInfo};

use cells::{CellSet, PreBoundaries};

/// Symbol types from the rule params, else the sorted distinct cell symbols.
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

/// Number of regions = occurrences of each symbol type; 0 if counts differ.
pub fn rose_m(puzzle: &Puzzle, symbol_types: &[String]) -> usize {
    if symbol_types.is_empty() {
        return 0;
    }
    let mut counts = vec![0usize; symbol_types.len()];
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                    counts[ti] += 1;
                }
            }
        }
    }
    let first = counts[0];
    if counts.iter().any(|&c| c != first) {
        return 0;
    }
    first
}

/// Group a flat `region_id` array into `RegionInfo` (cells sorted, shape
/// normalized).  Region ids are contiguous from 0.
pub fn build_regions(region_of: &[Option<usize>], _h: usize, w: usize) -> Vec<RegionInfo> {
    let mut by_rid: HashMap<usize, Vec<[usize; 2]>> = HashMap::new();
    for (idx, rid) in region_of.iter().enumerate() {
        if let Some(rid) = rid {
            by_rid.entry(*rid).or_default().push([idx / w, idx % w]);
        }
    }
    let mut regions: Vec<RegionInfo> = Vec::new();
    for (rid, mut cells) in by_rid {
        cells.sort();
        let area = cells.len();
        let mut shape = cells.clone();
        crate::types::normalize(&mut shape);
        let key = crate::types::canonical_key(&shape);
        regions.push(RegionInfo {
            region_id: rid,
            cells,
            area,
            shape,
            normalized_shape_key: key,
            matched_shape_name: None,
        });
    }
    regions.sort_by_key(|r| r.region_id);
    regions
}

/// Accept a candidate solution only if the full independent validator passes.
pub fn accept_if_valid(regions: Vec<RegionInfo>, puzzle: &Puzzle) -> Option<Vec<RegionInfo>> {
    if crate::solver::aog::validate::validate(puzzle, &regions) {
        Some(regions)
    } else {
        None
    }
}

/// Solve a rose_window puzzle (region_match first, rose_growth as fallback).
pub fn solve_rose(
    puzzle: &Puzzle,
    start: &Instant,
    timeout_ms: u64,
) -> Option<Vec<RegionInfo>> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut all_positions = CellSet::new(h * w);
    for r in 0..h {
        for c in 0..w {
            if !puzzle.cells[r][c].blocked {
                all_positions.insert(r * w + c);
            }
        }
    }
    let pre = PreBoundaries::from_puzzle(puzzle);
    let symbol_types = rose_symbol_types(puzzle);
    let m = rose_m(puzzle, &symbol_types);
    if symbol_types.is_empty() || m == 0 {
        return None;
    }

    // region_match first (mirrors rose/solver.py:40).
    if let Some(regions) = region_match::solve_by_region_match(
        puzzle,
        &pre,
        &symbol_types,
        m,
        &all_positions,
        start,
        timeout_ms,
    ) {
        if let Some(ok) = accept_if_valid(regions, puzzle) {
            return Some(ok);
        }
    }
    // rose_growth fallback (mirrors rose/solver.py:47).
    if let Some(regions) = super::rose::rose_growth::solve_rose_growth(
        puzzle,
        &pre,
        &symbol_types,
        m,
        &all_positions,
        start,
        timeout_ms,
    ) {
        if let Some(ok) = accept_if_valid(regions, puzzle) {
            return Some(ok);
        }
    }
    None
}
