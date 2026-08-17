//! Rose-window solver — port of `src/solver/rose/solver.py` +
//! `src/solver/region_match.py` + `src/solver/rose_growth.py`.
//!
//! The AoG solver times out on "rose_window without size constraint" puzzles
//! (e.g. C4-1, 0277); this module solves them by deducing the region count from
//! symbol multiplicities and matching/growing regions symbol-by-symbol.

pub mod cells;
pub mod puzzle_piece_pin;
pub mod region_match;
pub mod rose_growth;

use std::collections::HashMap;
use crate::clock::Instant;

use crate::shapes::rose_symbol_types;
use crate::types::{ModuleOutcome, Puzzle, RegionInfo};

use cells::{CellSet, PreBoundaries};

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
    if crate::solver::validate::validate(puzzle, &regions) {
        Some(regions)
    } else {
        None
    }
}

/// Solve a rose_window puzzle (region_match first, rose_growth as fallback).
///
/// For `puzzle_piece + rose_window` puzzles, pre-resolves the
/// `shape_pattern`-pinned regions first (see `puzzle_piece_pin`), then runs
/// region_match on the remaining cells with a reduced `m`.
///
/// Returns `ModuleOutcome`: a candidate that `accept_if_valid` rejected is
/// surfaced as `ValidationFailed` (previously folded into `None` and lost —
/// doc 23 §3.3).  `None` means no candidate was produced at all.
pub fn solve_rose(
    puzzle: &Puzzle,
    start: &Instant,
    timeout_ms: u64,
) -> ModuleOutcome {
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
        return ModuleOutcome::None;
    }

    let has_puzzle_piece = puzzle.rules.iter().any(|r| r.ctype == "puzzle_piece");

    // puzzle_piece + rose_window path: pre-pin shape_pattern regions, then
    // run region_match on the remainder.  Gated by ROSE_PP_PIN (default on).
    let pp_pin_enabled = !std::env::var("ROSE_PP_PIN")
        .map(|v| v == "0")
        .unwrap_or(false);
    if has_puzzle_piece && pp_pin_enabled {
        // `solve_rose_with_pin` returns its own `ModuleOutcome`; on Solved we
        // return, otherwise we record whether it was a validation failure for
        // the final outcome decision, then fall through.
        let mut validation_failed = false;
        match solve_rose_with_pin(puzzle, &pre, &symbol_types, m, &all_positions, start, timeout_ms) {
            ModuleOutcome::Solved(regions) => return ModuleOutcome::Solved(regions),
            ModuleOutcome::ValidationFailed => validation_failed = true,
            ModuleOutcome::None => {}
        }
        // Pin path failed → fall through to the plain path (which will likely
        // also fail for puzzle_piece, but rose_growth is a last resort).
        return rose_plain_and_growth(
            puzzle, &pre, &symbol_types, m, &all_positions,
            start, timeout_ms, validation_failed,
        );
    }

    rose_plain_and_growth(
        puzzle, &pre, &symbol_types, m, &all_positions,
        start, timeout_ms, false,
    )
}

/// region_match then rose_growth fallback, with `accept_if_valid` gating each
/// candidate.  `validation_failed_in` carries forward any earlier pin-path
/// validation failure so the final outcome can be `ValidationFailed` when no
/// path produced an accepted solution.
fn rose_plain_and_growth(
    puzzle: &Puzzle,
    pre: &PreBoundaries,
    symbol_types: &[String],
    m: usize,
    all_positions: &CellSet,
    start: &Instant,
    timeout_ms: u64,
    mut validation_failed_in: bool,
) -> ModuleOutcome {
    // region_match first (mirrors rose/solver.py:40).
    if crate::aog_debug_enabled() {
        eprintln!("rose: region_match start (types={} m={})", symbol_types.len(), m);
    }
    if let Some(regions) = region_match::solve_by_region_match(
        puzzle,
        pre,
        symbol_types,
        m,
        all_positions,
        start,
        timeout_ms,
    ) {
        match accept_if_valid(regions, puzzle) {
            Some(ok) => return ModuleOutcome::Solved(ok),
            None => validation_failed_in = true,
        }
    }
    // rose_growth fallback (mirrors rose/solver.py:47).
    if let Some(regions) = super::rose::rose_growth::solve_rose_growth(
        puzzle,
        pre,
        symbol_types,
        m,
        all_positions,
        start,
        timeout_ms,
    ) {
        match accept_if_valid(regions, puzzle) {
            Some(ok) => return ModuleOutcome::Solved(ok),
            None => validation_failed_in = true,
        }
    }
    if validation_failed_in {
        ModuleOutcome::ValidationFailed
    } else {
        ModuleOutcome::None
    }
}

/// Pre-pin `shape_pattern` regions, then run region_match on the remainder.
///
/// Returns `ModuleOutcome`; `accept_if_valid` rejections are tracked and, if
/// no assignment yielded an accepted solution, surfaced as `ValidationFailed`.
fn solve_rose_with_pin(
    puzzle: &Puzzle,
    pre: &PreBoundaries,
    symbol_types: &[String],
    m: usize,
    all_positions: &CellSet,
    start: &Instant,
    timeout_ms: u64,
) -> ModuleOutcome {
    let h = puzzle.height;
    let w = puzzle.width;
    let n_bits = h * w;
    let dbg = crate::aog_debug_enabled();

    let anchors = match puzzle_piece_pin::enumerate_pin_candidates(puzzle, symbol_types) {
        Some(a) => {
            if dbg {
                eprintln!("rose-pp-pin: {} anchors, candidates {:?}", a.len(), a.iter().map(|x| x.placements.len()).collect::<Vec<_>>());
            }
            a
        }
        None => {
            if dbg { eprintln!("rose-pp-pin: enumerate_pin_candidates returned None"); }
            return ModuleOutcome::None;
        }
    };
    let assignments =
        puzzle_piece_pin::enumerate_pin_assignments(puzzle, anchors, symbol_types, m);

    if crate::aog_debug_enabled() {
        eprintln!(
            "rose-pp-pin: {} assignments to try (m={})",
            assignments.len(),
            m
        );
    }

    let mut validation_failed = false;
    let deadline = *start + std::time::Duration::from_millis(timeout_ms);
    for assignment in &assignments {
        // Budget guard: stop trying assignments if we're out of time.
        if crate::clock::Instant::now() >= deadline {
            break;
        }

        // Remainder per-type count (must be balanced → that's the new m').
        let m_remainder = match puzzle_piece_pin::remainder_per_type(assignment, puzzle, symbol_types, w) {
            Some(c) => c,
            None => continue,
        };
        if m_remainder == 0 && !all_positions_empty_after_pin(all_positions, assignment, n_bits) {
            // No symbol cells remain but cells do — region_match needs ≥1 seed.
            continue;
        }

        // Reduced all_positions = original minus all pinned cells.
        let mut reduced = all_positions.clone();
        for p in &assignment.pinned {
            for idx in p.cells.iter() {
                reduced.remove(idx);
            }
        }

        if crate::aog_debug_enabled() {
            eprintln!(
                "rose-pp-pin: assignment {} pinned regions, remainder={} cells m'={}",
                assignment.pinned.len(),
                reduced.len(),
                m_remainder
            );
        }

        // Fast path: m' == 1 means the remainder is a single rose region.  If
        // the remaining cells form one connected component containing every
        // remaining symbol cell, that IS the region — no MRV search needed.
        // This sidesteps region_match's candidate-cap (20000) which can drop
        // the large single-region candidate on big remainders.
        if m_remainder == 1 {
            if let Some(single) = try_single_region(puzzle, pre, &reduced, symbol_types, w) {
                let merged = merge_pinned(single, assignment, w);
                match accept_if_valid(merged, puzzle) {
                    Some(ok) => return ModuleOutcome::Solved(ok),
                    None => validation_failed = true,
                }
            }
            // else fall through to region_match (may still find it via MRV).
        }

        // region_match on the remainder.  m' = m_remainder (each remaining rose
        // region contains one of each remaining symbol per type... actually m'
        // is the per-type count in the remainder, which equals the region
        // count only if each region has one per type — the standard rose model).
        let remaining_ms = timeout_ms.saturating_sub(start.elapsed().as_millis() as u64);
        if remaining_ms == 0 {
            break;
        }
        let rose_regions = region_match::solve_by_region_match(
            puzzle,
            pre,
            symbol_types,
            m_remainder,
            &reduced,
            start,
            remaining_ms,
        );
        let Some(rose_regions) = rose_regions else { continue };

        // Merge: rose_regions + pinned regions (each pinned region gets a
        // fresh region_id above the rose ids).
        let merged = merge_pinned(rose_regions, assignment, w);
        match accept_if_valid(merged, puzzle) {
            Some(ok) => return ModuleOutcome::Solved(ok),
            None => validation_failed = true,
        }
    }
    if validation_failed {
        ModuleOutcome::ValidationFailed
    } else {
        ModuleOutcome::None
    }
}

/// Fast path for m' == 1: if the remaining cells form a single 4-connected
/// component (respecting pre-drawn boundaries) that contains every remaining
/// symbol cell, return it as a one-element region list.  Returns None if the
/// remainder is disconnected or a symbol cell is isolated by a boundary.
fn try_single_region(
    puzzle: &Puzzle,
    pre: &PreBoundaries,
    reduced: &CellSet,
    symbol_types: &[String],
    w: usize,
) -> Option<Vec<RegionInfo>> {
    let h = puzzle.height;
    if reduced.is_empty() {
        return None;
    }
    // BFS from any remaining cell; ensure it reaches every remaining cell
    // (single component) without crossing a pre-boundary.
    let start_cell = reduced.iter().next()?;
    let mut visited = CellSet::new(h * w);
    visited.insert(start_cell);
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start_cell);
    while let Some(idx) = queue.pop_front() {
        let r = idx / w;
        let c = idx % w;
        for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 || nr >= h as i32 || nc >= w as i32 {
                continue;
            }
            let nidx = nr as usize * w + nc as usize;
            if !reduced.contains(nidx) || visited.contains(nidx) {
                continue;
            }
            // Don't cross a pre-drawn boundary.
            if pre.contains(r, c, nr as usize, nc as usize) {
                continue;
            }
            visited.insert(nidx);
            queue.push_back(nidx);
        }
    }
    if visited.len() != reduced.len() {
        return None; // disconnected
    }
    // Verify every remaining symbol cell is in the component (it must be, since
    // visited == reduced, but guard against symbol cells outside reduced that
    // the caller failed to pin).  Already implied by len check.
    let _ = symbol_types;

    // Build a single RegionInfo from all remaining cells.
    let mut cells: Vec<[usize; 2]> = reduced.iter().map(|idx| [idx / w, idx % w]).collect();
    cells.sort();
    let area = cells.len();
    let mut shape = cells.clone();
    crate::types::normalize(&mut shape);
    let key = crate::types::canonical_key(&shape);
    Some(vec![RegionInfo {
        region_id: 0,
        cells,
        area,
        shape,
        normalized_shape_key: key,
        matched_shape_name: None,
    }])
}

/// True if `all_positions` minus the pinned cells is non-empty.
fn all_positions_empty_after_pin(
    all_positions: &CellSet,
    assignment: &puzzle_piece_pin::PinAssignment,
    _n_bits: usize,
) -> bool {
    let mut count = all_positions.len();
    for p in &assignment.pinned {
        // Only count cells actually in all_positions (they always are, since
        // placements avoid blocked cells).
        for idx in p.cells.iter() {
            if all_positions.contains(idx) {
                count -= 1;
            }
        }
    }
    count == 0
}

/// Merge rose regions with pinned regions, assigning fresh region_ids.
fn merge_pinned(
    mut rose_regions: Vec<RegionInfo>,
    assignment: &puzzle_piece_pin::PinAssignment,
    w: usize,
) -> Vec<RegionInfo> {
    let next_id = rose_regions.iter().map(|r| r.region_id).max().map_or(0, |m| m + 1);
    let _ = w;
    for (i, p) in assignment.pinned.iter().enumerate() {
        let mut cells: Vec<[usize; 2]> = p.cells.iter().map(|idx| [idx / w, idx % w]).collect();
        cells.sort();
        let area = cells.len();
        let mut shape = cells.clone();
        crate::types::normalize(&mut shape);
        let key = crate::types::canonical_key(&shape);
        rose_regions.push(RegionInfo {
            region_id: next_id + i,
            cells,
            area,
            shape,
            normalized_shape_key: key,
            matched_shape_name: None,
        });
    }
    rose_regions
}
