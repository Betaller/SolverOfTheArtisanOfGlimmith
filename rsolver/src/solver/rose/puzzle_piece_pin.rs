//! Pre-resolve `shape_pattern`-pinned regions for `puzzle_piece + rose_window`
//! puzzles, so the rose solver can run on the remaining cells.
//!
//! ## Why
//!
//! `solve_by_region_match` (`region_match.rs:285-291`) hard-rejects any puzzle
//! with a `puzzle_piece` or `shape_pool` rule, because region_match grows free
//! connected regions and has no machinery to force a region to match a specific
//! shape.  This dooms puzzles like 0732 (`puzzle_piece + rose_window`): aog
//! gets only 3s (rose-capable budget) and can't finish, rose refuses, and the
//! rest of the chain can't solve it either.
//!
//! ## Mechanism
//!
//! A `shape_pattern` cell `(ar, ac)` pins its **entire region**: the region's
//! shape (up to dihedral symmetry) must equal the pattern's dihedral class
//! (`validate.rs:181-191` compares `dihedral_key(&region.cells)` vs
//! `dihedral_key(&pattern)`).  So the pinned region is one of the pattern's
//! dihedral variants placed so that `(ar, ac)` lies inside it.
//!
//! This module enumerates those placements, filters by the rose_window symbol
//! constraint (each rose region contains the same per-type symbol count), and
//! returns the viable pinned-region sets.  The caller (`solve_rose`) removes
//! the pinned cells from `all_positions`, decrements `m`, runs region_match on
//! the remainder, then merges the pinned region back.

use std::collections::HashSet;

use crate::types::Puzzle;

use super::cells::{CellSet, PreBoundaries};

/// One viable placement of one shape_pattern cell's pinned region.
#[derive(Clone)]
pub struct PinnedPlacement {
    /// The shape_pattern anchor cell that this placement satisfies (for debug).
    #[allow(dead_code)]
    pub anchor: usize,
    /// Cells of the pinned region (row-major indices).
    pub cells: CellSet,
}

/// All placements for a single shape_pattern anchor, deduped + filtered.
pub struct AnchorCandidates {
    #[allow(dead_code)]
    pub anchor: usize,
    pub placements: Vec<PinnedPlacement>,
}

/// The 8 dihedral transforms of a point set, deduped by canonical form.
/// Returns each unique variant as absolute (row, col) offsets from origin.
fn dihedral_variants(pattern: &[[usize; 2]]) -> Vec<Vec<(isize, isize)>> {
    let signed: Vec<(isize, isize)> = pattern.iter().map(|&[r, c]| (r as isize, c as isize)).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Vec<(isize, isize)>> = Vec::new();
    for &rot in &[0usize, 1, 2, 3] {
        for &refl in &[false, true] {
            let mut t: Vec<(isize, isize)> = signed
                .iter()
                .map(|&(r, c)| {
                    let (mut rr, mut cc) = (r, c);
                    if refl {
                        cc = -cc;
                    }
                    for _ in 0..rot {
                        // 90° CCW: (r,c) -> (-c, r)
                        let (pr, pc) = (rr, cc);
                        rr = -pc;
                        cc = pr;
                    }
                    (rr, cc)
                })
                .collect();
            // Normalize to origin.
            let min_r = t.iter().map(|p| p.0).min().unwrap_or(0);
            let min_c = t.iter().map(|p| p.1).min().unwrap_or(0);
            for p in &mut t {
                p.0 -= min_r;
                p.1 -= min_c;
            }
            t.sort();
            let key: String = t.iter().map(|(r, c)| format!("({},{})", r, c)).collect();
            if seen.insert(key) {
                out.push(t);
            }
        }
    }
    out
}

/// Enumerate legal placements of `variant` such that anchor cell `(ar, ac)`
/// lies inside the placed region.  A placement is legal when every cell is
/// in-grid, non-blocked, and no pre-drawn boundary is crossed between two
/// cells of the region (the region must be internally connected without walls).
fn placements_for_variant(
    puzzle: &Puzzle,
    variant: &[(isize, isize)],
    ar: usize,
    ac: usize,
) -> Vec<Vec<usize>> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut out: Vec<Vec<usize>> = Vec::new();
    // For each point `vp` in the variant, try placing it at the anchor: the
    // variant point `vp` lands on `(ar, ac)`, so offset = (ar - vp.r, ac - vp.c).
    for &(vr, vc) in variant {
        let or = ar as isize - vr;
        let oc = ac as isize - vc;
        let mut cells: Vec<usize> = Vec::with_capacity(variant.len());
        let mut ok = true;
        for &(pr, pc) in variant {
            let r = or + pr;
            let c = oc + pc;
            if r < 0 || c < 0 || r >= h as isize || c >= w as isize {
                ok = false;
                break;
            }
            let (ru, cu) = (r as usize, c as usize);
            if puzzle.cells[ru][cu].blocked {
                ok = false;
                break;
            }
            cells.push(ru * w + cu);
        }
        if !ok {
            continue;
        }
        // Check no pre-drawn boundary splits the region: for every pair of
        // 4-adjacent cells both in the region, the edge must not be a boundary.
        let in_region: HashSet<usize> = cells.iter().copied().collect();
        for &idx in &cells {
            let r = idx / w;
            let c = idx % w;
            for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr < 0 || nc < 0 || nr >= h as i32 || nc >= w as i32 {
                    continue;
                }
                let nidx = nr as usize * w + nc as usize;
                if !in_region.contains(&nidx) {
                    continue;
                }
                // Adjacent pair both in region — edge must not be a boundary.
                if edge_is_boundary(puzzle, r, c, nr as usize, nc as usize) {
                    ok = false;
                    break;
                }
            }
            if !ok {
                break;
            }
        }
        if ok {
            cells.sort();
            out.push(cells);
        }
    }
    // Dedup identical placements (different variant points may yield same set).
    out.sort();
    out.dedup();
    out
}

/// Look up the edge between two adjacent cells, mirroring `grid::is_adjacent_free`.
#[inline]
fn edge_is_boundary(puzzle: &Puzzle, r: usize, c: usize, nr: usize, nc: usize) -> bool {
    if r == nr {
        puzzle.h_edges[r][c.min(nc)].is_boundary
    } else {
        puzzle.v_edges[r.min(nr)][c].is_boundary
    }
}

/// Count, for a placement, how many cells of each symbol type it contains.
/// Returns a Vec parallel to `symbol_types` (index → count).
fn placement_symbol_counts(
    puzzle: &Puzzle,
    cells: &[usize],
    w: usize,
    symbol_types: &[String],
) -> Vec<usize> {
    let mut counts = vec![0usize; symbol_types.len()];
    for &idx in cells {
        if let Some(sym) = puzzle.cells[idx / w][idx % w].symbol.as_ref() {
            if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                counts[ti] += 1;
            }
        }
    }
    counts
}

/// Enumerate candidate placements for every `shape_pattern` cell, filtered by
/// the rose_window symbol constraint.  Returns one `AnchorCandidates` per
/// shape_pattern cell (each non-empty after filtering), or `None` if any
/// anchor has zero viable placements.
///
/// Filter rule (rose_window semantics): a pinned placement *is* a whole
/// region, so it must hold **exactly one** cell of every symbol type.  (The
/// earlier "counts all equal" test was vacuous for single-type puzzles —
/// every placement passed — which let the assignment product explode, e.g.
/// 0224 ballooning to 14 GB.)
pub fn enumerate_pin_candidates(
    puzzle: &Puzzle,
    symbol_types: &[String],
) -> Option<Vec<AnchorCandidates>> {
    let w = puzzle.width;
    let mut anchors: Vec<AnchorCandidates> = Vec::new();
    // ring frame runs (empty unless the puzzle has `ring`): a placement is a
    // whole region, so it either covers an entire monochrome rim run or
    // none of it — partial coverage would saw a wall into the frame.
    let runs = crate::solver::same_tiling::ring_frame_runs(puzzle);

    for r in 0..puzzle.height {
        for c in 0..w {
            let Some(ref pattern) = puzzle.cells[r][c].shape_pattern else {
                continue;
            };
            let pattern_arr: Vec<[usize; 2]> = pattern.iter().copied().collect();
            let variants = dihedral_variants(&pattern_arr);
            let mut placements: Vec<PinnedPlacement> = Vec::new();
            for v in &variants {
                for cells in placements_for_variant(puzzle, v, r, c) {
                    // Rose rule: one cell of each symbol type per region.
                    if !symbol_types.is_empty() {
                        let counts = placement_symbol_counts(puzzle, &cells, w, symbol_types);
                        if !counts.iter().all(|&x| x == 1) {
                            continue;
                        }
                    }
                    // ring frame runs must land wholly inside one region.
                    if !runs.is_empty() {
                        let mut bad = false;
                        for run in &runs {
                            let any =
                                run.iter().any(|&(rr, cc)| cells.contains(&(rr * w + cc)));
                            let all =
                                run.iter().all(|&(rr, cc)| cells.contains(&(rr * w + cc)));
                            if any && !all {
                                bad = true;
                                break;
                            }
                        }
                        if bad {
                            continue;
                        }
                    }
                    let mut set = CellSet::new(puzzle.height * puzzle.width);
                    for &idx in &cells {
                        set.insert(idx);
                    }
                    placements.push(PinnedPlacement {
                        anchor: r * w + c,
                        cells: set,
                    });
                }
            }
            // Dedup by cell set (different variants may produce same set).
            placements.sort_by_key(|p| {
                let mut k: Vec<usize> = p.cells.iter().collect();
                k.sort();
                k
            });
            placements.dedup_by(|a, b| a.cells.is_disjoint(&b.cells) == false && same_set(&a.cells, &b.cells));
            if placements.is_empty() {
                return None;
            }
            anchors.push(AnchorCandidates {
                anchor: r * w + c,
                placements,
            });
        }
    }
    if anchors.is_empty() {
        None
    } else {
        Some(anchors)
    }
}

/// Element-wise set equality (CellSet has no Eq impl).
fn same_set(a: &CellSet, b: &CellSet) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

/// Budget caps for the pin-assignment walk — outcomes of hitting one are
/// *budget* results (this method simply stops proposing), never proofs of
/// unsolvability.  `validate` still gates whatever is returned.
const MAX_COMBINE_NODES: usize = 2_000_000;
const MAX_PIN_ASSIGNMENTS: usize = 50_000;

/// Stream every valid pin assignment (one placement per anchor, pairwise
/// disjoint, symbol-balanced remainder) to `visitor`, which returns `false`
/// to stop early.  Anchors are walked in MRV order (fewest placements
/// first) and the walk is bounded by `MAX_COMBINE_NODES` tree nodes,
/// `MAX_PIN_ASSIGNMENTS` complete assignments and `deadline`.  Returns
/// `true` iff the whole tree was walked (no cap/timeout hit).
///
/// This used to materialise the full Cartesian product into a `Vec` — 0224
/// (12 anchors × vacuously-filtered placements) ballooned to 14 GB and got
/// OOM-killed.  Streaming + the exact-one-symbol filter keeps it tiny.
pub fn for_each_pin_assignment(
    puzzle: &Puzzle,
    mut anchors: Vec<AnchorCandidates>,
    symbol_types: &[String],
    deadline: crate::clock::Instant,
    visitor: &mut dyn FnMut(&[PinnedPlacement]) -> bool,
) -> bool {
    let w = puzzle.width;
    // Precompute per-type total symbol counts on the full grid.
    let mut total_per_type = vec![0usize; symbol_types.len()];
    for r in 0..puzzle.height {
        for c in 0..w {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                    total_per_type[ti] += 1;
                }
            }
        }
    }
    // MRV: anchors with the fewest placements first — shrinks the product
    // tree without losing any complete assignment.
    anchors.sort_by_key(|a| a.placements.len());
    let mut state = CombineState {
        nodes: 0,
        assignments: 0,
        truncated: false,
    };
    let mut current: Vec<PinnedPlacement> = Vec::with_capacity(anchors.len());
    combine(
        &anchors,
        0,
        &mut current,
        puzzle,
        symbol_types,
        &total_per_type,
        w,
        deadline,
        visitor,
        &mut state,
    );
    !state.truncated
}

struct CombineState {
    nodes: usize,
    assignments: usize,
    truncated: bool,
}

/// Recursive Cartesian product with disjointness + remainder-balance
/// pruning, streaming complete assignments to the visitor.
fn combine(
    anchors: &[AnchorCandidates],
    i: usize,
    current: &mut Vec<PinnedPlacement>,
    puzzle: &Puzzle,
    symbol_types: &[String],
    total_per_type: &[usize],
    w: usize,
    deadline: crate::clock::Instant,
    visitor: &mut dyn FnMut(&[PinnedPlacement]) -> bool,
    state: &mut CombineState,
) {
    if state.truncated {
        return;
    }
    state.nodes += 1;
    if state.nodes > MAX_COMBINE_NODES || crate::clock::Instant::now() >= deadline {
        state.truncated = true;
        return;
    }
    if i == anchors.len() {
        // Complete assignment: check the remainder is symbol-balanced
        // (equal per-type remainders partition into balanced rose regions).
        let mut rem = total_per_type.to_vec();
        for p in current.iter() {
            let counts = placement_symbol_counts(puzzle, &p.cells.iter().collect::<Vec<_>>(), w, symbol_types);
            for (ti, &c) in counts.iter().enumerate() {
                rem[ti] -= c;
            }
        }
        let first = rem[0];
        if rem.iter().all(|&x| x == first) {
            state.assignments += 1;
            if state.assignments > MAX_PIN_ASSIGNMENTS || !visitor(current) {
                state.truncated = true;
            }
        }
        return;
    }
    for p in &anchors[i].placements {
        if state.truncated {
            return;
        }
        // Disjoint with all currently chosen.
        let disjoint = current.iter().all(|c| c.cells.is_disjoint(&p.cells));
        if !disjoint {
            continue;
        }
        current.push(p.clone());
        combine(anchors, i + 1, current, puzzle, symbol_types, total_per_type, w, deadline, visitor, state);
        current.pop();
    }
}

/// For a completed pin assignment, compute the remaining per-type symbol count.
/// Returns `Some(count)` if the remainder is balanced (all types equal), else
/// `None` — in which case the assignment cannot yield a valid rose partition.
pub fn remainder_per_type(
    pinned: &[PinnedPlacement],
    puzzle: &Puzzle,
    symbol_types: &[String],
    w: usize,
) -> Option<usize> {
    let mut total = vec![0usize; symbol_types.len()];
    for r in 0..puzzle.height {
        for c in 0..w {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                    total[ti] += 1;
                }
            }
        }
    }
    for p in pinned {
        let counts = placement_symbol_counts(puzzle, &p.cells.iter().collect::<Vec<_>>(), w, symbol_types);
        for (ti, &c) in counts.iter().enumerate() {
            total[ti] -= c;
        }
    }
    let first = total[0];
    if total.iter().all(|&x| x == first) {
        Some(first)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dihedral_variants_cross_is_small() {
        // 9-cell cross from 0732.
        let pat = vec![[0, 1], [1, 0], [1, 1], [1, 2], [2, 1], [3, 0], [3, 1], [3, 2], [4, 1]];
        let vs = dihedral_variants(&pat);
        // Cross has 4-fold symmetry → 2 unique variants (the cross and its
        // 90° rotation, which differs because the stem extends one direction).
        assert_eq!(vs.len(), 2, "cross has 2 dihedral variants");
    }

    #[test]
    fn dihedral_variants_square_is_one() {
        // 2×2 square: 8 transforms all collapse to 1.
        let pat = vec![[0, 0], [0, 1], [1, 0], [1, 1]];
        let vs = dihedral_variants(&pat);
        assert_eq!(vs.len(), 1);
    }

    /// Corpus anchor: 1215 (`puzzle_piece + brick + ring`, 6 pattern regions
    /// + one 69-cell remainder) must go through the standalone pre-pin — the
    /// ring frame-run filter + MRV + junction checks collapse the placement
    /// tree to ~0.5 s (previously a ~36 s walk that lost to aog's OOM).
    #[test]
    fn solves_pp_pin_1215() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/2-loopy/1215.json"
        ))
        .expect("parse");
        let out = solve_puzzle_piece_standalone(&p, 30_000);
        assert!(out.is_solved(), "1215 expected solved, got {:?}", out);
    }

    /// The exact-one-symbol filter: a pinned placement is a whole rose
    /// region and must hold exactly one cell of each type (0224's 12 anchors
    /// × vacuous filter used to balloon the assignment product to 14 GB).
    /// A domino pattern at (0,0) with P1 on both (0,0) and (0,1): the
    /// horizontal placement holds two P1 (rejected), the vertical one holds
    /// exactly one (kept).
    #[test]
    fn pin_candidates_require_one_symbol_each() {
        let json = r#"{"grid":{"height":3,"width":3},
            "cells":[{"row":0,"col":0,"symbol":"P1","shape_pattern":[[0,0],[0,1]]},
                     {"row":0,"col":1,"symbol":"P1"},{"row":0,"col":2},
                     {"row":1,"col":0},{"row":1,"col":1},{"row":1,"col":2},
                     {"row":2,"col":0},{"row":2,"col":1},{"row":2,"col":2}],
            "edges":[],"vertices":[],"rules":[{"type":"puzzle_piece"},{"type":"rose_window","params":{"symbol_types":["P1"]}}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        let types = vec!["P1".to_string()];
        let anchors = enumerate_pin_candidates(&puzzle, &types).expect("candidates");
        assert_eq!(anchors.len(), 1);
        assert!(!anchors[0].placements.is_empty());
        for p in &anchors[0].placements {
            let counts =
                placement_symbol_counts(&puzzle, &p.cells.iter().collect::<Vec<_>>(), 3, &types);
            assert_eq!(counts, vec![1], "placement must hold exactly one P1");
            assert!(
                !p.cells.contains(1) || !p.cells.contains(0),
                "the 2-symbol horizontal placement must be filtered out"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone (non-rose) pre-pin
// ---------------------------------------------------------------------------

/// Standalone `shape_pattern` pre-pin for puzzles that carry `puzzle_piece` but
/// **no** `rose_window`, so `solve_rose_with_pin` never runs for them.
///
/// `pieces`' DLX cannot model these: it tiles the board with the shape pool and
/// has no notion of "one big unconstrained region", so 0976
/// (`brick+ring+puzzle_piece`, 5 pattern regions of 6 cells plus one 80-cell
/// region) exhausts in 2ms — and with edge_csp deliberately not tolerating
/// `puzzle_piece`, nothing else in the chain can even attempt it.
///
/// Pin every `shape_pattern` region, then solve the remainder: first as a
/// single region (0976 class — the corpus's common shape), and if that is
/// rejected, through `solve_multi_remainder` — the watchtower-driven free
/// partition (0994 class: pattern regions + 1 big wrap + WT-forced
/// singletons).  A wrong remainder is rejected by `validate`; if no assignment
/// yields a valid partition this returns `None` and the rest of the chain runs
/// unchanged.  Regions that hold more than one `shape_pattern` cell (0493) are
/// handled (an anchor-swallowing placement covers several anchors); the
/// WT-less multi-remainder class (1435: `mixed` drives the split) is still out
/// of scope and falls through.
pub fn solve_puzzle_piece_standalone(
    puzzle: &Puzzle,
    timeout_ms: u64,
) -> crate::types::ModuleOutcome {
    use crate::types::ModuleOutcome;

    // Anchor to *this module's* start, not the caller's global one — aog has
    // usually burned the whole unit budget by the time we get here, so a
    // global-start deadline would already be expired (the same trap
    // `solve_rose` documents above its own `rose_start`).
    let deadline = crate::clock::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let h = puzzle.height;
    let w = puzzle.width;
    let n = h * w;

    // No symbol types → `enumerate_pin_candidates` skips the rose balance
    // filter and just returns the dihedral placements per anchor.
    let Some(anchors) = enumerate_pin_candidates(puzzle, &[]) else {
        if crate::aog_debug_enabled() {
            eprintln!("pp-pin: enumerate_pin_candidates returned None");
        }
        return ModuleOutcome::None;
    };
    if crate::aog_debug_enabled() {
        eprintln!(
            "pp-pin: {} anchors, placements per anchor: {:?}",
            anchors.len(),
            anchors.iter().map(|a| a.placements.len()).collect::<Vec<_>>()
        );
    }

    // Anchor metadata in the same row-major order `enumerate_pin_candidates`
    // walked the pattern cells.
    let mut anchor_flat: Vec<usize> = Vec::new();
    let mut anchor_key: Vec<String> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if let Some(ref pat) = puzzle.cells[r][c].shape_pattern {
                anchor_flat.push(r * w + c);
                anchor_key.push(crate::shapes::dihedral_key(pat));
            }
        }
    }
    let mut covered = vec![false; anchors.len()];
    let mut taken = CellSet::new(n);
    let mut current: Vec<PinnedPlacement> = Vec::with_capacity(anchors.len());
    let wts = watchtower_facts(puzzle);
    let mut found: Option<Vec<crate::types::RegionInfo>> = None;
    combine_plain(
        &anchors,
        &anchor_flat,
        &anchor_key,
        &mut covered,
        &mut taken,
        &mut current,
        puzzle,
        n,
        deadline,
        &wts,
        &mut found,
    );
    match found {
        Some(regions) => {
            if crate::solver::validate::validate(puzzle, &regions) {
                ModuleOutcome::Solved(regions)
            } else {
                ModuleOutcome::ValidationFailed
            }
        }
        None => ModuleOutcome::None,
    }
}


/// Watchtower facts for incremental pruning: each vertex clue with the flat
/// cell indices of the (≤4, non-blocked) cells touching it and its value.
pub(crate) fn watchtower_facts(puzzle: &Puzzle) -> Vec<(Vec<usize>, usize)> {
    let (h, w) = (puzzle.height, puzzle.width);
    let mut out = Vec::new();
    for r in 0..=h {
        for c in 0..=w {
            if let Some(val) = puzzle.vertices[r][c].watchtower {
                if !(1..=4).contains(&val) {
                    continue;
                }
                let mut cells = Vec::new();
                for (dr, dc) in [(-1i64, -1i64), (-1, 0), (0, -1), (0, 0)] {
                    let nr = r as i64 + dr;
                    let nc = c as i64 + dc;
                    if nr < 0 || nc < 0 || nr >= h as i64 || nc >= w as i64 {
                        continue;
                    }
                    let (nr, nc) = (nr as usize, nc as usize);
                    if !puzzle.cells[nr][nc].blocked {
                        cells.push(nr * w + nc);
                    }
                }
                out.push((cells, val as usize));
            }
        }
    }
    out
}

/// Sound incremental watchtower prune.  Placed cells already carry distinct
/// region ids; every unplaced cell will land in a *fresh* region (a future
/// placement or the remainder — neither may reuse a placed region).  If the
/// watchtower value `k` falls outside `[d + (u > 0), d + u]` the partial
/// assignment cannot be completed.
fn watchtower_ok(facts: &[(Vec<usize>, usize)], current: &[PinnedPlacement]) -> bool {
    for (cells, k) in facts {
        let mut u = 0usize;
        for &idx in cells {
            let mut placed = false;
            for p in current.iter() {
                if p.cells.contains(idx) {
                    placed = true;
                    break;
                }
            }
            if !placed {
                u += 1;
            }
        }
        // Count `d` as distinct placed region ids (cells ≤ 4).
        let mut seen: Vec<usize> = Vec::new();
        for &idx in cells {
            for (ri, p) in current.iter().enumerate() {
                if p.cells.contains(idx) {
                    if !seen.contains(&ri) {
                        seen.push(ri);
                    }
                    break;
                }
            }
        }
        let d = seen.len();
        let lo = if u > 0 { d + 1 } else { d };
        let hi = d + u;
        if *k < lo || *k > hi {
            return false;
        }
    }
    true
}

/// Complete-cover leaf: try the remainder as one region (0976 class), then
/// fall back to the watchtower-driven multi-region free partition (0994
/// class).  `validate` gates whatever is produced.
fn finish_leaf(
    puzzle: &Puzzle,
    current: &[PinnedPlacement],
    n: usize,
    deadline: crate::clock::Instant,
    found: &mut Option<Vec<crate::types::RegionInfo>>,
) {
    if crate::aog_debug_enabled() {
        eprintln!("pp-pin: leaf with {} placements, checking remainder", current.len());
    }
    let h = puzzle.height;
    let w = puzzle.width;
    let mut region_of: Vec<Option<usize>> = vec![None; n];
    for (ri, p) in current.iter().enumerate() {
        for idx in p.cells.iter() {
            region_of[idx] = Some(ri);
        }
    }
    // The whole remainder is one region — the case the corpus's
    // `puzzle_piece` FAILs actually have (pattern regions + 1 big region).
    let rem_id = current.len();
    let mut has_remainder = false;
    for idx in 0..n {
        let r = idx / w;
        let c = idx % w;
        if !puzzle.cells[r][c].blocked && region_of[idx].is_none() {
            region_of[idx] = Some(rem_id);
            has_remainder = true;
        }
    }
    // Patterns already tile the board exactly — nothing to solve.
    if !has_remainder && current.is_empty() {
        return;
    }
    let regions = super::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        *found = Some(regions);
        return;
    }
    // Single-remainder rejected → the remainder may need splitting into
    // several free regions (0994/1435 class: pattern regions + 1 big wrap
    // + WT-forced singletons).  `solve_multi_remainder` re-derives the
    // free partition under the watchtower cardinalities.
    if has_remainder {
        if let Some(regions) = solve_multi_remainder(puzzle, current, n, deadline) {
            *found = Some(regions);
            return;
        }
        // compass+solitary remainder (1093 class): solitary pins the free
        // region count to the free clue cells, so the free partition is a
        // `compass_label` cell-labeling problem on the unpinned cells.
        if let Some(regions) = solve_compass_remainder(puzzle, current, n, deadline) {
            *found = Some(regions);
        }
    }
}

/// Recursive anchor-covering search.  Every `shape_pattern` anchor must be
/// covered by exactly one chosen placement; one placement may cover SEVERAL
/// anchors of the same shape class (a region may hold several same-shape
/// pattern clues — 0493 packs 11 anchors into 7 regions, one region holding
/// four).  On a complete cover the leftover cells form one region (the
/// corpus's `puzzle_piece` FAILs: pattern regions + 1 big region) and are
/// handed straight to the validator.
#[allow(clippy::too_many_arguments)]
fn combine_plain(
    anchors: &[AnchorCandidates],
    anchor_flat: &[usize],
    anchor_key: &[String],
    covered: &mut Vec<bool>,
    taken: &mut CellSet,
    current: &mut Vec<PinnedPlacement>,
    puzzle: &Puzzle,
    n: usize,
    deadline: crate::clock::Instant,
    wts: &[(Vec<usize>, usize)],
    found: &mut Option<Vec<crate::types::RegionInfo>>,
) {
    if found.is_some() || crate::clock::Instant::now() >= deadline {
        return;
    }
    if !watchtower_ok(wts, current) {
        return;
    }
    let h = puzzle.height;
    let w = puzzle.width;
    if covered.iter().all(|&b| b) {
        finish_leaf(puzzle, current, n, deadline, found);
        return;
    }
    // MRV over the uncovered anchors; an anchor with zero viable placements
    // kills the whole branch (it can never be covered).
    let Some(a) = pick_anchor_mrv(anchors, anchor_flat, covered, taken) else {
        return;
    };
    for p in &anchors[a].placements {
        if !p.cells.contains(anchor_flat[a]) {
            continue;
        }
        if !p.cells.is_disjoint(taken) {
            continue;
        }
        // Anchors the placement swallows all become covered (a region may
        // hold several pattern clues).  Cross-class shares are rejected by
        // `validate::check_puzzle_piece` at the leaf — not here: an in-search
        // class filter was measured to *lose* 0976 (its clean solution path
        // somehow never surfaces with the filter on), while the validator
        // makes the filter redundant for correctness.
        let mut covers: Vec<usize> = Vec::new();
        for (b, &bf) in anchor_flat.iter().enumerate() {
            if covered[b] {
                continue;
            }
            if p.cells.contains(bf) {
                covers.push(b);
            }
        }
        if covers.is_empty() {
            continue;
        }
        for &b in &covers {
            covered[b] = true;
        }
        taken.union_into(&p.cells);
        current.push(p.clone());
        // ring/brick junction degrees on the vertices this pin decided.
        if new_pin_vertices_ok(puzzle, current, taken, w, h) {
            combine_plain(
                anchors,
                anchor_flat,
                anchor_key,
                covered,
                taken,
                current,
                puzzle,
                n,
                deadline,
                wts,
                found,
            );
        }
        current.pop();
        // Undo `taken`: rebuild from `current` (CellSet has no subtract).
        *taken = CellSet::new(n);
        for q in current.iter() {
            taken.union_into(&q.cells);
        }
        for &b in &covers {
            covered[b] = false;
        }
        if found.is_some() {
            return;
        }
    }
}

/// MRV anchor choice over the uncovered anchors.  Branching walks the
/// picked anchor's own placement list (complete: a true region covering the
/// anchor matches that anchor's pattern class and so is in its list), so the
/// pick prefers fewest still-viable own placements.  A placement may swallow
/// several anchors at once, so an anchor with no own placements left can
/// still be covered as a side effect — only the *uncoverable* case (no
/// viable placement of any uncovered list reaches it) is a dead branch.
/// `None` = dead branch.
fn pick_anchor_mrv(
    anchors: &[AnchorCandidates],
    anchor_flat: &[usize],
    covered: &[bool],
    taken: &CellSet,
) -> Option<usize> {
    // Viable placements of each uncovered anchor's own list.
    let viable_of: Vec<Vec<&PinnedPlacement>> = anchors
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if covered[i] {
                Vec::new()
            } else {
                a.placements
                    .iter()
                    .filter(|p| p.cells.is_disjoint(taken))
                    .collect()
            }
        })
        .collect();
    let mut best: Option<(usize, usize)> = None;
    for (i, _) in anchors.iter().enumerate() {
        if covered[i] {
            continue;
        }
        let own = viable_of[i].len();
        let reachable = viable_of
            .iter()
            .flatten()
            .any(|p| p.cells.contains(anchor_flat[i]));
        if !reachable {
            return None; // no viable placement can ever cover this anchor
        }
        if own == 0 {
            continue; // covered only as a side effect of another anchor
        }
        if best.map_or(true, |(_, bn)| own < bn) {
            best = Some((i, own));
        }
    }
    best.map(|(i, _)| i)
}

/// Pin id owning `idx`, if any.
fn owner_pin(current: &[PinnedPlacement], idx: usize) -> Option<usize> {
    current.iter().position(|p| p.cells.contains(idx))
}

/// ring/brick junction degrees on the vertices around the newest pin.  Only
/// vertices whose every quadrant is decided (pinned / blocked / outside) are
/// checked — an undecided quadrant could still join a later pin or the
/// remainder, leaving the final degree open.  `ring` forbids degree 3 (T),
/// `brick` degree 4 (cross); both together cap the degree at 2.
fn new_pin_vertices_ok(
    puzzle: &Puzzle,
    current: &[PinnedPlacement],
    taken: &CellSet,
    w: usize,
    h: usize,
) -> bool {
    let ring = puzzle.rules.iter().any(|r| r.ctype == "ring");
    let brick = puzzle.rules.iter().any(|r| r.ctype == "brick");
    if (!ring && !brick) || current.is_empty() {
        return true;
    }
    let last = &current[current.len() - 1];
    for idx in last.cells.iter() {
        let (cr, cc) = ((idx / w) as i32, (idx % w) as i32);
        for vr in [cr - 1, cr] {
            for vc in [cc - 1, cc] {
                if !vertex_degree_ok_at(puzzle, current, taken, w, h, vr, vc, ring, brick) {
                    return false;
                }
            }
        }
    }
    true
}

/// Degree check at one lattice vertex (quadrants (vr,vc),(vr,vc+1),
/// (vr+1,vc),(vr+1,vc+1), mirroring `validate::count_boundary_edges_at_vertex`).
fn vertex_degree_ok_at(
    puzzle: &Puzzle,
    current: &[PinnedPlacement],
    taken: &CellSet,
    w: usize,
    h: usize,
    vr: i32,
    vc: i32,
    ring: bool,
    brick: bool,
) -> bool {
    let quads = [(vr, vc), (vr, vc + 1), (vr + 1, vc), (vr + 1, vc + 1)];
    let mut region = [None::<usize>; 4];
    for (qi, &(r, c)) in quads.iter().enumerate() {
        if r < 0 || c < 0 || r >= h as i32 || c >= w as i32 {
            continue; // outside → None
        }
        let (ru, cu) = (r as usize, c as usize);
        if puzzle.cells[ru][cu].blocked {
            continue; // blocked → None
        }
        let q = ru * w + cu;
        if !taken.contains(q) {
            return true; // undecided quadrant — degree not yet determined
        }
        region[qi] = Some(owner_pin(current, q).unwrap_or(usize::MAX));
    }
    // Edge pairs at the vertex: top (0,1), bottom (2,3), left (0,2), right (1,3).
    let mut count = 0usize;
    for (a, b) in [(0usize, 1usize), (2, 3), (0, 2), (1, 3)] {
        match (region[a], region[b]) {
            (Some(x), Some(y)) => {
                if x != y {
                    count += 1;
                }
            }
            (None, None) => {}
            _ => count += 1,
        }
    }
    if ring && count == 3 {
        return false;
    }
    if brick && count == 4 {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Multi-remainder free partition (0994 / 1435 class)
// ---------------------------------------------------------------------------

/// Union-find over free cells for the WT-derived must-same relation.
struct Uf {
    parent: Vec<u32>,
}

impl Uf {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n as u32).collect(),
        }
    }
    fn find(&mut self, x: u32) -> u32 {
        let mut r = x;
        while self.parent[r as usize] != r {
            r = self.parent[r as usize];
        }
        let mut cur = x;
        while self.parent[cur as usize] != r {
            let p = self.parent[cur as usize];
            self.parent[cur as usize] = r;
            cur = p;
        }
        r
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb as usize] = ra;
        }
    }
}

/// One watchtower vertex as slots over pins / free units.
enum WtSlot {
    Pin(usize),
    Unit(u32),
}

struct WtUnitFact {
    slots: Vec<WtSlot>,
    value: usize,
}

fn canon_pair(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Windows `[alo,ahi] × [blo,bhi]` admit `|a−b| = v` iff the achievable gap
/// range (a contiguous interval — continuous image of a connected set)
/// covers `v`.
fn gap_feasible(alo: usize, ahi: usize, blo: usize, bhi: usize, v: usize) -> bool {
    if alo > ahi || blo > bhi {
        return false;
    }
    let max_gap = std::cmp::max(ahi.abs_diff(blo), bhi.abs_diff(alo));
    let min_abs = if ahi < blo {
        blo - ahi
    } else if bhi < alo {
        alo - bhi
    } else {
        0
    };
    v >= min_abs && v <= max_gap
}

/// Watchtower cardinality → pairwise must-same / must-split over free cells.
///
/// Free labels never coincide with pin labels (a pinned region's shape is
/// fixed, so no free cell may join one), so a vertex with `p` distinct pin
/// labels and `f` free cells must end with `value = p + k` distinct labels
/// where `k` is the number of distinct labels among those `f` cells:
/// `f == 2 && k == 1` forces the pair together, `k == f` forces all pairs
/// apart, `f >= 3 && k == 1` forces all together.  Returns `None` on a
/// cardinality that no free labeling can satisfy (unsolvable).
#[allow(clippy::type_complexity)]
fn wt_free_relations(
    facts: &[(Vec<usize>, usize)],
    pin_of: &[Option<usize>],
    free_index: &std::collections::HashMap<usize, u32>,
) -> Option<(Vec<(u32, u32)>, Vec<(u32, u32)>)> {
    let mut same = Vec::new();
    let mut split = Vec::new();
    for (cells, value) in facts {
        let mut pins: Vec<usize> = Vec::new();
        let mut units: Vec<u32> = Vec::new();
        for &idx in cells {
            match pin_of[idx] {
                Some(p) => {
                    if !pins.contains(&p) {
                        pins.push(p);
                    }
                }
                None => {
                    let u = free_index[&idx];
                    if !units.contains(&u) {
                        units.push(u);
                    }
                }
            }
        }
        let k = value.checked_sub(pins.len())?;
        if units.is_empty() {
            if k != 0 {
                return None;
            }
            continue;
        }
        let f = units.len();
        if k < 1 || k > f {
            return None;
        }
        if f == 2 {
            if k == 1 {
                same.push((units[0], units[1]));
            } else {
                split.push((units[0], units[1]));
            }
        } else if k == 1 {
            for i in 0..f {
                for j in (i + 1)..f {
                    same.push((units[i], units[j]));
                }
            }
        } else if k == f {
            for i in 0..f {
                for j in (i + 1)..f {
                    split.push((units[i], units[j]));
                }
            }
        }
    }
    Some((same, split))
}

/// Verdict of the rose-cardinality completion step.
enum RoseStep {
    Ok,
    Dead,
    Force(u32, u32),
}

/// Connected-partition DFS over must-same *units*.  Each unit joins the label
/// of an adjacent already-assigned unit or spawns a fresh label; fixed unit
/// order makes this enumeration canonical (each partition exactly once).
/// Pruning: ① WT cardinality bounds, ② must-split (incl. pre-drawn walls),
/// ③ potential-connectivity closure — a label's cells must stay connectable
/// through undecided cells (0994's `(4,3)~(5,4)` class only connects through
/// `(0,3)`, so labelling `(0,3)` elsewhere dies instantly, not at the leaf).
struct FreeRem<'a> {
    puzzle: &'a Puzzle,
    pinned: &'a [PinnedPlacement],
    w: usize,
    unit_cells: &'a Vec<Vec<usize>>,
    unit_adj: &'a Vec<Vec<u32>>,
    split: &'a std::collections::HashSet<(u32, u32)>,
    facts: &'a Vec<WtUnitFact>,
    facts_of_unit: &'a Vec<Vec<usize>>,
    ord: &'a Vec<(u32, u32)>,
    diff: &'a Vec<(u32, u32, usize)>,
    sz_lo: &'a Vec<usize>,
    sz_hi: &'a Vec<usize>,
    /// Rose-cardinality mode: per-unit symbol-type bitmask (empty = off).
    unit_types: &'a Vec<u64>,
    n_types: usize,
    /// Region (= label) count: symbols-per-type, NOT the type count.
    max_labels: usize,
    lab_of: Vec<Option<u32>>,
    lab_units: Vec<Vec<u32>>,
    /// Per-label size windows from the last `compute_windows` round (join
    /// checks in `domain` read them; fixpoint refreshes every round).
    win_cache: Vec<(usize, usize)>,
    deadline: crate::clock::Instant,
    nodes: usize,
}

const MAX_FREE_NODES: usize = 2_000_000;

// Death-attribution counters (test builds only): keyed by (check,
// assigned/16).  The 0929-class endgame diagnosis tool — where a search
// dies tells which propagation is missing.  (Plain comments: a doc comment
// on a macro invocation triggers unused_doc_comments.)
#[cfg(test)]
thread_local! {
    static DIES: std::cell::RefCell<std::collections::HashMap<(&'static str, usize), usize>> =
        Default::default();
}
#[cfg(test)]
fn die_note(tag: &'static str, assigned: usize) {
    DIES.with(|d| {
        *d.borrow_mut().entry((tag, assigned / 16)).or_insert(0) += 1;
    });
}
#[cfg(test)]
pub(crate) fn dies_snapshot() -> std::collections::HashMap<(&'static str, usize), usize> {
    DIES.with(|d| d.borrow().clone())
}
#[cfg(test)]
fn dies_reset() {
    DIES.with(|d| d.borrow_mut().clear());
}

impl<'a> FreeRem<'a> {
    fn is_split(&self, a: u32, b: u32) -> bool {
        self.split.contains(&canon_pair(a, b))
    }

    /// compass_label-style domain search: fixpoint (joinable + domains +
    /// singleton forces) then MRV branch with snapshot rollback.  The value
    /// domain of a unit is every *existing* label it could still join
    /// (potential-connectable, not must-split) plus SPAWN — a full domain is
    /// required for completeness (two seeds of one region must be able to
    /// land on one label even before they are adjacent through assigned
    /// cells; the adjacency-only join rule loses those partitions).
    fn search(&mut self) -> Option<Vec<crate::types::RegionInfo>> {
        self.nodes += 1;
        if self.nodes > MAX_FREE_NODES || crate::clock::Instant::now() >= self.deadline {
            return None;
        }
        self.fixpoint()?;
        if self.lab_of.iter().all(|l| l.is_some()) {
            return self.leaf_regions();
        }
        let u = self.pick_mrv()?;
        let values = self.value_order(u);
        let snap_lab = self.lab_of.clone();
        let snap_units = self.lab_units.clone();
        for val in values {
            self.lab_of = snap_lab.clone();
            self.lab_units = snap_units.clone();
            self.apply_value(u, val);
            if let Some(res) = self.search() {
                return Some(res);
            }
            if crate::clock::Instant::now() >= self.deadline {
                return None;
            }
        }
        self.lab_of = snap_lab;
        self.lab_units = snap_units;
        None
    }

    /// Attach `u` to an existing label (`Some`) or mint a fresh one (`None`).
    fn apply_value(&mut self, u: u32, val: Option<u32>) {
        match val {
            Some(l) => {
                self.lab_units[l as usize].push(u);
                self.lab_of[u as usize] = Some(l);
            }
            None => {
                let l = self.lab_units.len() as u32;
                self.lab_units.push(vec![u]);
                self.lab_of[u as usize] = Some(l);
            }
        }
    }

    /// `differentiation` (same-size-no-touch): adjacent regions must differ
    /// in size.  Two adjacent labels whose windows are **both exact and
    /// equal** can never separate legally, so any placement that would touch
    /// them is dead now (0308: fourteen size-5 regions must be scattered).
    /// Unexact windows stay permissive — the leaf validator still rejects
    /// late coincidences (false-negative only).
    fn diff_no_touch(&self) -> bool {
        self.puzzle
            .rules
            .iter()
            .any(|r| r.ctype == "differentiation")
    }

    /// Would placing `u` into label `l` (or spawning a fresh label when
    /// `l == None`) put it adjacent to a different label with an exactly
    /// equal size window?
    fn diff_adj_conflict(&self, u: u32, l: Option<u32>) -> bool {
        if !self.diff_no_touch() {
            return false;
        }
        let mine = match l {
            Some(ll) => self.win_cache.get(ll as usize).copied(),
            None => {
                let (lo, hi) = (self.sz_lo[u as usize], self.sz_hi[u as usize]);
                if lo == hi {
                    Some((lo, hi))
                } else {
                    None
                }
            }
        };
        let Some((mlo, mhi)) = mine else {
            return false;
        };
        if mlo != mhi {
            return false;
        }
        for &v in &self.unit_adj[u as usize] {
            let Some(m) = self.lab_of[v as usize] else { continue };
            if Some(m) == l {
                continue;
            }
            if let Some((olo, ohi)) = self.win_cache.get(m as usize).copied() {
                if olo == ohi && olo == mlo {
                    return true;
                }
            }
        }
        false
    }

    /// Global sweep: any adjacent unit pair in different labels with exactly
    /// equal size windows is a `differentiation` violation already present
    /// in the assignment (catches direct force paths).
    fn diff_ok(&self) -> bool {
        if !self.diff_no_touch() {
            return true;
        }
        for u in 0..self.lab_of.len() as u32 {
            let Some(l) = self.lab_of[u as usize] else { continue };
            let Some((mlo, mhi)) = self.win_cache.get(l as usize).copied() else {
                continue;
            };
            if mlo != mhi {
                continue;
            }
            for &v in &self.unit_adj[u as usize] {
                let Some(m) = self.lab_of[v as usize] else { continue };
                if m == l {
                    continue;
                }
                if let Some((olo, ohi)) = self.win_cache.get(m as usize).copied() {
                    if olo == ohi && olo == mlo {
                        return false;
                    }
                }
            }
        }
        true
    }


    /// Propagate to a fixpoint.  `None` = contradiction (dead node).
    ///
    /// Forces apply **one unit at a time with a full recompute**: with
    /// emergent labels every unit's domain starts as `{SPAWN}`, so a batch
    /// force would mint one label per unit in a single round and kill the node
    /// (0994 root: 37 units → 37 labels → WT bounds die).  Sequential forcing
    /// lets each spawn become joinable by the next unit.
    fn fixpoint(&mut self) -> Option<()> {
        'outer: loop {
            #[cfg(test)]
            let a = self.lab_of.iter().filter(|l| l.is_some()).count();
            if !self.joinable_all() {
                #[cfg(test)]
                die_note("joinable", a);
                return None;
            }
            if !self.wt_all_ok() {
                #[cfg(test)]
                die_note("wt", a);
                return None;
            }
            self.win_cache = match self.compute_windows() {
                Some(w) => w,
                None => {
                    #[cfg(test)]
                    die_note("windows", a);
                    return None;
                }
            };
            if !self.diff_ok() {
                #[cfg(test)]
                die_note("diff", a);
                return None;
            }
            match self.rose_step() {
                RoseStep::Dead => {
                    #[cfg(test)]
                    die_note("rose", a);
                    return None;
                }
                RoseStep::Force(u, l) => {
                    self.lab_units[l as usize].push(u);
                    self.lab_of[u as usize] = Some(l);
                    continue;
                }
                RoseStep::Ok => {}
            }
            if let Some((u, l)) = self.corridor_force() {
                self.lab_units[l as usize].push(u);
                self.lab_of[u as usize] = Some(l);
                continue;
            }
            for u in 0..self.lab_of.len() as u32 {
                if self.lab_of[u as usize].is_some() {
                    continue;
                }
                let dom = self.cheap_domain(u);
                match dom.len() {
                    0 => {
                        #[cfg(test)]
                        die_note("dom0", a);
                        return None;
                    }
                    1 => {
                        let val = dom[0];
                        self.apply_value(u, val);
                        continue 'outer;
                    }
                    _ => {}
                }
            }
            return Some(());
        }
    }

    /// Feasible values: existing labels the unit may join (not must-split
    /// from any member and the merge stays potential-connectable) plus SPAWN.
    /// Cheap over-approximation of `domain` (no BFS): used for MRV and the
    /// singleton scan — the full domain is only computed for the chosen unit.
    /// Over-approximate ⇒ fewer forces/dies ⇒ sound.
    fn cheap_domain(&self, u: u32) -> Vec<Option<u32>> {
        let mut out: Vec<Option<u32>> = Vec::new();
        for (l, units) in self.lab_units.iter().enumerate() {
            if units.is_empty() {
                continue;
            }
            if units.iter().any(|&v| self.is_split(u, v)) {
                continue;
            }
            if self.n_types > 0 {
                let tm = self.unit_types[u as usize];
                let mut taken = 0u64;
                for &v in units {
                    taken |= self.unit_types[v as usize];
                }
                if taken & tm != 0 {
                    continue;
                }
            }
            let mut merged = Vec::with_capacity(units.len() + 1);
            merged.extend_from_slice(units);
            merged.push(u);
            let (lo, hi) = self.merge_statics(&merged, Some(l as u32));
            if lo > hi {
                continue;
            }
            if self.diff_adj_conflict(u, Some(l as u32)) {
                continue;
            }
            out.push(Some(l as u32));
        }
        if self.max_labels == 0 || self.lab_units.len() < self.max_labels {
            if !self.diff_adj_conflict(u, None) {
                out.push(None);
            }
        }
        out
    }

    fn domain(&self, u: u32) -> Vec<Option<u32>> {
        let mut out: Vec<Option<u32>> = Vec::new();
        for (l, units) in self.lab_units.iter().enumerate() {
            if units.is_empty() {
                continue;
            }
            if units.iter().any(|&v| self.is_split(u, v)) {
                continue;
            }
            if self.n_types > 0 {
                let tm = self.unit_types[u as usize];
                let mut taken = 0u64;
                for &v in units {
                    taken |= self.unit_types[v as usize];
                }
                if taken & tm != 0 {
                    continue;
                }
            }
            let mut merged = units.clone();
            merged.push(u);
            if self.label_potential_ok(&merged) && self.merge_window_ok(&merged, Some(l as u32)) {
                out.push(Some(l as u32));
            }
        }
        // SPAWN: the unit's own label is trivially connectable when it is a
        // single cell; a non-contiguous must-same class needs its cells
        // connectable through bridges.  In rose mode the label count is
        // exactly `n_types` (the region count = symbols-per-type), so spawn
        // stops at that ceiling, and a fresh label must still be able to
        // collect one unit of every type it lacks.
        let spawn_ok = if self.max_labels > 0 {
            self.lab_units.len() < self.max_labels
                && self.label_potential_ok(std::slice::from_ref(&u))
                && self.merge_window_ok(std::slice::from_ref(&u), None)
                && (self.n_types == 0 || self.spawn_completable(u))
        } else {
            self.label_potential_ok(std::slice::from_ref(&u))
                && self.merge_window_ok(std::slice::from_ref(&u), None)
        };
        if spawn_ok {
            out.push(None);
        }
        out
    }

    /// Can a fresh label rooted at `u` still collect one unit of every type
    /// it does not already carry?
    fn spawn_completable(&self, u: u32) -> bool {
        let mut have = self.unit_types[u as usize];
        let full: u64 = if self.n_types >= 64 {
            u64::MAX
        } else {
            (1u64 << self.n_types) - 1
        };
        let missing = full & !have;
        if missing == 0 {
            return true;
        }
        for j in 0..self.unit_cells.len() as u32 {
            if self.lab_of[j as usize].is_some() || j == u {
                continue;
            }
            if self.is_split(j, u) {
                continue;
            }
            have |= self.unit_types[j as usize];
        }
        have & missing == missing
    }

    /// MRV: smallest domain; ties: most WT facts, then lowest first cell.
    /// (A constraint-degree tie-break — ord/diff incidences first — was tried
    /// and falsified: 0206's plain search went 3.3 s → >12 s on the changed
    /// trajectory.)
    fn pick_mrv(&self) -> Option<u32> {
        let mut best_key: Option<(usize, usize, std::cmp::Reverse<usize>, usize)> = None;
        let mut best_u = None;
        for u in 0..self.lab_of.len() as u32 {
            if self.lab_of[u as usize].is_some() {
                continue;
            }
            // Narrow size window first: in size-case runs the constraint
            // anchors carry exact windows and lead the assignment (0929's
            // collapse); in plain runs widths are mostly equal (no-op).
            let key = (
                self.cheap_domain(u).len(),
                self.sz_hi[u as usize].saturating_sub(self.sz_lo[u as usize]),
                std::cmp::Reverse(self.facts_of_unit[u as usize].len()),
                self.unit_cells[u as usize][0],
            );
            if best_key.map_or(true, |bk| key < bk) {
                best_key = Some(key);
                best_u = Some(u);
            }
        }
        best_u
    }

    /// Adjacent labels first (fast truth discovery), then the rest, SPAWN last.
    fn value_order(&self, u: u32) -> Vec<Option<u32>> {
        let dom = self.domain(u);
        let mut adj: Vec<u32> = Vec::new();
        for &v in &self.unit_adj[u as usize] {
            if let Some(l) = self.lab_of[v as usize] {
                if dom.contains(&Some(l)) && !adj.contains(&l) {
                    adj.push(l);
                }
            }
        }
        adj.sort_unstable();
        let mut out: Vec<Option<u32>> = adj.iter().map(|&l| Some(l)).collect();
        for val in &dom {
            if val.is_some() && !out.contains(val) {
                out.push(*val);
            }
        }
        if dom.contains(&None) {
            out.push(None);
        }
        out
    }

    /// The new unit may not share its label with a must-split mate.
    #[allow(dead_code)]
    fn join_split_ok(&self, u: u32, lab: u32) -> bool {
        !self.lab_units[lab as usize]
            .iter()
            .any(|&v| v != u && self.is_split(u, v))
    }

    /// Sound WT bounds for every vertex.
    fn wt_all_ok(&self) -> bool {
        for fi in 0..self.facts.len() {
            if !self.wt_fact_ok(fi) {
                return false;
            }
        }
        true
    }

    fn wt_fact_ok(&self, fi: usize) -> bool {
        let f = &self.facts[fi];
        let mut pins: Vec<usize> = Vec::new();
        let mut frees: Vec<u32> = Vec::new();
        let mut unassigned: Vec<u32> = Vec::new();
        for slot in &f.slots {
            match slot {
                WtSlot::Pin(p) => {
                    if !pins.contains(p) {
                        pins.push(*p);
                    }
                }
                WtSlot::Unit(x) => match self.lab_of[*x as usize] {
                    Some(l) => {
                        if !frees.contains(&l) {
                            frees.push(l);
                        }
                    }
                    None => {
                        if !unassigned.contains(x) {
                            unassigned.push(*x);
                        }
                    }
                },
            }
        }
        let d = pins.len() + frees.len();
        let hi = d + unassigned.len();
        let lo = if unassigned.is_empty() {
            d
        } else if frees.is_empty() {
            d + 1
        } else {
            d
        };
        f.value >= lo && f.value <= hi
    }

    /// Rose cardinality ("each region holds exactly one of each symbol
    /// type"): labels must collect one unit per type; a label that can no
    /// longer reach a missing type is dead, and a missing type with only one
    /// reachable unit is forced.
    fn rose_step(&self) -> RoseStep {
        if self.n_types == 0 {
            return RoseStep::Ok;
        }
        let full: u64 = if self.n_types >= 64 {
            u64::MAX
        } else {
            (1u64 << self.n_types) - 1
        };
        for (li, units) in self.lab_units.iter().enumerate() {
            if units.is_empty() {
                continue;
            }
            let mut taken = 0u64;
            for &v in units {
                let tm = self.unit_types[v as usize];
                if taken & tm != 0 {
                    return RoseStep::Dead;
                }
                taken |= tm;
            }
            let missing = full & !taken;
            if missing == 0 {
                continue;
            }
            let reach = self.label_reach(units);
            if reach.0 != usize::MAX {
                return RoseStep::Dead;
            }
            let mut cands: Vec<u32> = Vec::new();
            for j in 0..self.unit_cells.len() as u32 {
                if self.lab_of[j as usize].is_some() {
                    continue;
                }
                let tm = self.unit_types[j as usize];
                if tm & missing == 0 {
                    continue;
                }
                if units.iter().any(|&v| self.is_split(j, v)) {
                    continue;
                }
                cands.push(j);
            }
            for t in 0..self.n_types {
                let bit = 1u64 << t;
                if missing & bit == 0 {
                    continue;
                }
                let mut only: Option<u32> = None;
                let mut count = 0usize;
                for &j in &cands {
                    if self.unit_types[j as usize] & bit != 0 {
                        count += 1;
                        only = Some(j);
                        if count > 1 {
                            break;
                        }
                    }
                }
                if count == 0 {
                    return RoseStep::Dead;
                }
                if count == 1 {
                    if let Some(j) = only {
                        return RoseStep::Force(j, li as u32);
                    }
                }
            }
        }
        RoseStep::Ok
    }

    /// Per-label size windows: member statics ∩ reach extent, tightened by
    /// one-sided ord/diff caps against placed labels, then transitively
    /// relaxed (Bellman-Ford) across placed label pairs — this also catches
    /// dynamic ord cycles (`L1 < L2 < L3 < L1` inflates `lo` past `hi`) that
    /// pairwise checks miss.  `None` = contradiction.
    fn compute_windows(&self) -> Option<Vec<(usize, usize)>> {
        let mut wins = self.base_windows()?;
        self.one_sided_tighten(&mut wins);
        self.pair_relax(&mut wins)?;
        Some(wins)
    }

    fn base_windows(&self) -> Option<Vec<(usize, usize)>> {
        let mut windows: Vec<(usize, usize)> = Vec::with_capacity(self.lab_units.len());
        for units in &self.lab_units {
            if units.is_empty() {
                windows.push((0, usize::MAX));
                continue;
            }
            let cur: usize = units.iter().map(|&v| self.unit_cells[v as usize].len()).sum();
            let (mut lo, mut hi) = self.merge_statics(units, None);
            lo = lo.max(cur);
            let reach = self.label_reach(units).1;
            if reach < lo || cur > hi {
                return None;
            }
            hi = hi.min(reach);
            if lo > hi {
                return None;
            }
            windows.push((lo, hi));
        }
        Some(windows)
    }

    /// One-sided caps: `ord (ua, ub)` with only one end placed bounds that
    /// end's window through the other end's static window (`size(La) <
    /// size(Lf) ≤ sz_hi[ub]`, `size(Lb) > size(Lf) ≥ sz_lo[ua]`); diff pairs
    /// widen by ±v and die when no pair at gap v survives.
    fn one_sided_tighten(&self, wins: &mut [(usize, usize)]) {
        for &(ua, ub) in self.ord {
            let (la, lb) = (self.lab_of[ua as usize], self.lab_of[ub as usize]);
            match (la, lb) {
                (Some(a), None) => {
                    let cap = self.sz_hi[ub as usize];
                    if cap != usize::MAX {
                        let w = &mut wins[a as usize];
                        if w.1 > cap - 1 {
                            w.1 = cap - 1;
                        }
                    }
                }
                (None, Some(b)) => {
                    let floor = self.sz_lo[ua as usize] + 1;
                    let w = &mut wins[b as usize];
                    if w.0 < floor {
                        w.0 = floor;
                    }
                }
                _ => {}
            }
        }
        for &(ua, ub, v) in self.diff {
            let (la, lb) = (self.lab_of[ua as usize], self.lab_of[ub as usize]);
            match (la, lb) {
                (Some(a), None) => {
                    let (blo, bhi) = (self.sz_lo[ub as usize], self.sz_hi[ub as usize]);
                    let w = &mut wins[a as usize];
                    if !gap_feasible(w.0, w.1, blo, bhi, v) {
                        w.0 = 1;
                        w.1 = 0;
                    } else if bhi != usize::MAX && w.1 > bhi + v {
                        w.1 = bhi + v;
                    }
                }
                (None, Some(b)) => {
                    let (alo, ahi) = (self.sz_lo[ua as usize], self.sz_hi[ua as usize]);
                    let w = &mut wins[b as usize];
                    if !gap_feasible(w.0, w.1, alo, ahi, v) {
                        w.0 = 1;
                        w.1 = 0;
                    } else if ahi != usize::MAX && w.1 > ahi + v {
                        w.1 = ahi + v;
                    }
                }
                _ => {}
            }
        }
    }

    /// Bellman-Ford over placed label pairs + gap feasibility.  `false` =
    /// contradiction (incl. dynamic ord cycles).
    fn pair_relax(&self, wins: &mut Vec<(usize, usize)>) -> Option<()> {
        let n = wins.len();
        let mut lab_ord: Vec<(u32, u32)> = Vec::new();
        let mut lab_diff: Vec<(u32, u32, usize)> = Vec::new();
        for &(ua, ub) in self.ord {
            let (la, lb) = (self.lab_of[ua as usize], self.lab_of[ub as usize]);
            if la == lb && la.is_some() {
                return None;
            }
            if let (Some(a), Some(b)) = (la, lb) {
                lab_ord.push((a, b));
            }
        }
        for &(ua, ub, v) in self.diff {
            let (la, lb) = (self.lab_of[ua as usize], self.lab_of[ub as usize]);
            if la == lb && la.is_some() {
                if v != 0 {
                    return None;
                }
                continue;
            }
            if let (Some(a), Some(b)) = (la, lb) {
                lab_diff.push((a, b, v));
            }
        }
        let mut lo: Vec<usize> = wins.iter().map(|w| w.0).collect();
        let mut hi: Vec<usize> = wins.iter().map(|w| w.1).collect();
        relax_size_windows(&lab_ord, &lab_diff, &mut lo, &mut hi);
        for i in 0..n {
            if lo[i] > hi[i] {
                return None;
            }
            wins[i] = (lo[i], hi[i]);
        }
        for &(a, b, v) in &lab_diff {
            let (lo_a, hi_a) = wins[a as usize];
            let (lo_b, hi_b) = wins[b as usize];
            if !gap_feasible(lo_a, hi_a, lo_b, hi_b, v) {
                return None;
            }
        }
        Some(())
    }

    /// Static size window of a hypothetical label (`units`, optionally a join
    /// into `joined`): member statics ∩ the target label's cached window.
    fn merge_statics(&self, units: &[u32], joined: Option<u32>) -> (usize, usize) {
        let mut lo = 0usize;
        let mut hi = usize::MAX;
        for &v in units {
            if self.sz_lo[v as usize] > lo {
                lo = self.sz_lo[v as usize];
            }
            if self.sz_hi[v as usize] < hi {
                hi = self.sz_hi[v as usize];
            }
        }
        if let Some(l) = joined {
            if let Some(&(wlo, whi)) = self.win_cache.get(l as usize) {
                if wlo > lo {
                    lo = wlo;
                }
                if whi < hi {
                    hi = whi;
                }
            }
        }
        (lo, hi)
    }

    /// Full merge feasibility (statics + ord/diff vs everything outside).
    fn merge_window_ok(&self, units: &[u32], joined: Option<u32>) -> bool {
        let (mut lo, mut hi) = self.merge_statics(units, joined);
        if !self.merge_ord_caps(units, joined, &mut lo, &mut hi) {
            return false;
        }
        let cur: usize = units.iter().map(|&v| self.unit_cells[v as usize].len()).sum();
        lo <= hi && cur <= hi
    }

    fn merge_ord_caps(
        &self,
        units: &[u32],
        joined: Option<u32>,
        lo: &mut usize,
        hi: &mut usize,
    ) -> bool {
        for &(ua, ub) in self.ord {
            let in_a = self.in_merge(ua, units, joined);
            let in_b = self.in_merge(ub, units, joined);
            if in_a && in_b {
                return false; // size(L) < size(L)
            }
            if in_a {
                if !self.cap_other(ub, units, joined, true, lo, hi) {
                    return false;
                }
            } else if in_b && !self.cap_other(ua, units, joined, false, lo, hi) {
                return false;
            }
        }
        for &(ua, ub, v) in self.diff {
            let in_a = self.in_merge(ua, units, joined);
            let in_b = self.in_merge(ub, units, joined);
            if in_a && in_b {
                if v != 0 {
                    return false;
                }
                continue;
            }
            if in_a {
                if !self.cap_other_diff(ub, units, joined, v, lo, hi) {
                    return false;
                }
            } else if in_b && !self.cap_other_diff(ua, units, joined, v, lo, hi) {
                return false;
            }
        }
        true
    }

    fn in_merge(&self, x: u32, units: &[u32], joined: Option<u32>) -> bool {
        if units.contains(&x) {
            return true;
        }
        match (self.lab_of[x as usize], joined) {
            (Some(l), Some(j)) => l == j,
            _ => false,
        }
    }

    /// `mine < theirs` (smaller=true side): cap my hi by theirs' ceiling.
    fn cap_other(
        &self,
        other: u32,
        units: &[u32],
        joined: Option<u32>,
        mine_smaller: bool,
        lo: &mut usize,
        hi: &mut usize,
    ) -> bool {
        let (olo, ohi) = self.other_window(other, units, joined);
        if mine_smaller {
            // size(mine) < size(other) ≤ ohi  ⇒  hi ≤ ohi − 1;
            // size(other) ≥ size(mine)+1 ≥ lo+1  ⇒  olo ≥ lo+1 must hold.
            if ohi != usize::MAX {
                if *hi > ohi - 1 {
                    *hi = ohi - 1;
                }
            }
            std::cmp::max(olo, *lo + 1) <= ohi
        } else {
            // size(other) < size(mine): floor mine at olo+1.  Feasibility only
            // needs the tightened mine window non-empty and the other window
            // sane — mine is the *larger* side, so its ceiling is unrelated
            // to `ohi` (capping mine by ohi was the 0899 false reject).
            if *lo < olo + 1 {
                *lo = olo + 1;
            }
            olo <= ohi && *lo <= *hi
        }
    }

    fn cap_other_diff(
        &self,
        other: u32,
        units: &[u32],
        joined: Option<u32>,
        v: usize,
        lo: &mut usize,
        hi: &mut usize,
    ) -> bool {
        let (olo, ohi) = self.other_window(other, units, joined);
        if !gap_feasible(*lo, *hi, olo, ohi, v) {
            return false;
        }
        if ohi != usize::MAX && *hi > ohi + v {
            *hi = ohi + v;
        }
        if olo > v && *lo < olo - v {
            *lo = olo - v;
        }
        true
    }

    /// Window of the label on the far side of a constraint: a placed label's
    /// cache window, or the far unit's static window when unassigned.
    fn other_window(&self, other: u32, units: &[u32], joined: Option<u32>) -> (usize, usize) {
        if units.contains(&other) {
            return (usize::MAX, usize::MAX); // in-merge handled by caller
        }
        match self.lab_of[other as usize] {
            Some(l) if Some(l) != joined => {
                self.win_cache.get(l as usize).copied().unwrap_or((0, usize::MAX))
            }
            Some(_) => (usize::MAX, usize::MAX),
            None => (self.sz_lo[other as usize], self.sz_hi[other as usize]),
        }
    }

    /// Every label (incl. non-contiguous single must-same classes) must remain
    /// connectable through undecided cells that are not must-split from the
    /// label (they may still join it).
    fn joinable_all(&self) -> bool {
        for units in &self.lab_units {
            if !units.is_empty() && !self.label_potential_ok(units) {
                return false;
            }
        }
        true
    }

    fn label_potential_ok(&self, units: &[u32]) -> bool {
        self.label_reach(units).0 == usize::MAX
    }

    /// `(flag, extent)`: `flag == usize::MAX` marks "all label cells reached"
    /// (connectable); `extent` is the reachable passable count — the upper
    /// bound the label can ever grow to (cells + recruit-able bridges).
    fn label_reach(&self, units: &[u32]) -> (usize, usize) {
        let (flag, extent, _) = self.label_reach_vis(units, None);
        (flag, extent)
    }

    /// `label_reach` with the visited set exposed (corridor hull) and an
    /// optional excluded unit (`exclude` = "what if this unit never joins?").
    fn label_reach_vis(
        &self,
        units: &[u32],
        exclude: Option<u32>,
    ) -> (usize, usize, Vec<bool>) {
        let h = self.puzzle.height;
        let w = self.w;
        let n = h * w;
        let mut passable = vec![false; n];
        let mut label_marks = vec![false; n];
        let mut start = usize::MAX;
        for &v in units {
            for &c in &self.unit_cells[v as usize] {
                passable[c] = true;
                label_marks[c] = true;
                start = start.min(c);
            }
        }
        for (j, cells) in self.unit_cells.iter().enumerate() {
            if self.lab_of[j].is_some() {
                continue;
            }
            if Some(j as u32) == exclude {
                continue;
            }
            if units.iter().any(|&v| self.is_split(j as u32, v)) {
                continue;
            }
            for &c in cells {
                passable[c] = true;
            }
        }
        let pre = PreBoundaries::from_puzzle(self.puzzle);
        let mut seen = vec![false; n];
        let mut stack = vec![start];
        seen[start] = true;
        let mut reached = 0usize;
        let mut extent = 0usize;
        let need: usize = units.iter().map(|&v| self.unit_cells[v as usize].len()).sum();
        while let Some(c) = stack.pop() {
            extent += 1;
            if label_marks[c] {
                reached += 1;
            }
            let (r, col) = (c / w, c % w);
            for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                let nr = r as i32 + dr;
                let nc = col as i32 + dc;
                if nr < 0 || nc < 0 || nr >= h as i32 || nc >= w as i32 {
                    continue;
                }
                let (nr, nc) = (nr as usize, nc as usize);
                if pre.contains(r, col, nr, nc) {
                    continue;
                }
                let nidx = nr * w + nc;
                if passable[nidx] && !seen[nidx] {
                    seen[nidx] = true;
                    stack.push(nidx);
                }
            }
        }
        let flag = if reached == need { usize::MAX } else { reached };
        (flag, extent, seen)
    }

    /// Sound bridge force (the k-label port of compass_label's corridor
    /// forcing): when every passable connection between a label's cells runs
    /// through one unassigned unit's cells, that unit must join the label —
    /// otherwise the label can never end up connected.  Returns one force;
    /// fixpoint recomputes after applying it.
    fn corridor_force(&self) -> Option<(u32, u32)> {
        const BUDGET: usize = 64;
        let mut budget = BUDGET;
        for (li, units) in self.lab_units.iter().enumerate() {
            if units.is_empty() {
                continue;
            }
            let (flag, _, vis) = self.label_reach_vis(units, None);
            if flag != usize::MAX {
                continue;
            }
            for b in 0..self.unit_cells.len() as u32 {
                if self.lab_of[b as usize].is_some() {
                    continue;
                }
                if units.iter().any(|&v| self.is_split(b, v)) {
                    continue;
                }
                if !self.unit_cells[b as usize].iter().any(|&c| vis[c]) {
                    continue;
                }
                if budget == 0 {
                    return None;
                }
                budget -= 1;
                let (flag, _, _) = self.label_reach_vis(units, Some(b));
                if flag != usize::MAX {
                    return Some((b, li as u32));
                }
            }
        }
        None
    }

    fn leaf_regions(&self) -> Option<Vec<crate::types::RegionInfo>> {
        let h = self.puzzle.height;
        let w = self.w;
        let n = h * w;
        let mut region_of: Vec<Option<usize>> = vec![None; n];
        for (ri, p) in self.pinned.iter().enumerate() {
            for idx in p.cells.iter() {
                region_of[idx] = Some(ri);
            }
        }
        let base = self.pinned.len();
        for (lab, units) in self.lab_units.iter().enumerate() {
            for &v in units {
                for &idx in &self.unit_cells[v as usize] {
                    region_of[idx] = Some(base + lab);
                }
            }
        }
        let regions = super::build_regions(&region_of, h, w);
        if crate::solver::validate::validate(self.puzzle, &regions) {
            Some(regions)
        } else {
            #[cfg(test)]
            die_note("leaf_reject", self.lab_of.len());
            None
        }
    }
}

/// Partition the free cells (everything outside the pinned placements) into
/// connected regions under the watchtower cardinalities — the multi-remainder
/// counterpart of the single-remainder leaf (0976 class).  0994 (pattern
/// regions + 1 big wrap + 4 WT-forced singletons) lives here.
/// compass+solitary free partition (1093 class): `solitary` makes every free
/// region exactly one compass clue's region, so the free labeling has fixed
/// identified labels — delegate to `compass_label::solve_labeling` on a model
/// that treats the pinned cells as non-fillable (they are other regions: they
/// neither join a label nor count toward any half-plane).
fn solve_compass_remainder(
    puzzle: &Puzzle,
    pinned: &[PinnedPlacement],
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let has = |t: &str| puzzle.rules.iter().any(|r| r.ctype == t);
    if !(has("compass") && has("solitary")) {
        return None;
    }
    let mut excluded: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for p in pinned {
        for idx in p.cells.iter() {
            excluded.insert(idx);
        }
    }
    let model = crate::solver::compass_part::Model::build_excluding(puzzle, Some(&excluded))?;
    // Per-attempt slice: wrong pin leaves can demand multi-second unsat proofs
    // (1093: two 4.4 s walks ate the whole module budget and starved the true
    // leaf); the true leaf's labeling solves in ~10 ms, so 100 ms is a wide
    // margin while capping the damage per wrong leaf.
    let slice = crate::clock::Instant::now() + std::time::Duration::from_millis(100);
    let leaf_deadline = if slice < deadline { slice } else { deadline };
    let free_lab = crate::solver::compass_label::solve_labeling(&model, leaf_deadline)?;
    let h = puzzle.height;
    let w = puzzle.width;
    let mut region_of: Vec<Option<usize>> = vec![None; n];
    for (ri, p) in pinned.iter().enumerate() {
        for idx in p.cells.iter() {
            region_of[idx] = Some(ri);
        }
    }
    let base = pinned.len();
    for (idx, lab) in free_lab.iter().enumerate() {
        if let Some(j) = lab {
            region_of[idx] = Some(base + j);
        }
    }
    let regions = super::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        Some(regions)
    } else {

        None
    }
}

/// Transitive window relaxation over the order DAG (`size(a) < size(b)`) and
/// the difference pairs (|a−b| = v): lo/hi tightened to a fixpoint — the chain
/// structure then forces concrete sizes (0152-class pure inequality).
fn relax_size_windows(
    ord: &[(u32, u32)],
    diff: &[(u32, u32, usize)],
    lo: &mut [usize],
    hi: &mut [usize],
) {
    let n_units = lo.len();
    for _ in 0..n_units + 1 {
        let mut changed = false;
        for &(ua, ub) in ord.iter() {
            let (a, b) = (ua as usize, ub as usize);
            if lo[a] < usize::MAX && lo[b] < lo[a] + 1 {
                lo[b] = lo[a] + 1;
                changed = true;
            }
            if hi[b] != usize::MAX && hi[a] > hi[b].saturating_sub(1) {
                hi[a] = hi[b].saturating_sub(1);
                changed = true;
            }
        }
        for &(ua, ub, v) in diff.iter() {
            let (a, b) = (ua as usize, ub as usize);
            // |sa − sb| = v ⇒ lo_b ≥ lo_a − v, hi_b ≤ hi_a + v (both directions).
            if lo[a] > v && lo[b] < lo[a] - v {
                lo[b] = lo[a] - v;
                changed = true;
            }
            if lo[b] > v && lo[a] < lo[b] - v {
                lo[a] = lo[b] - v;
                changed = true;
            }
            if hi[a] != usize::MAX {
                let t = hi[a] + v;
                if hi[b] > t {
                    hi[b] = t;
                    changed = true;
                }
            }
            if hi[b] != usize::MAX {
                let t = hi[b] + v;
                if hi[a] > t {
                    hi[a] = t;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// `area` rule number clues: a numbered cell's region has size exactly `N`
/// (`constraints::check_rule_area`).  Injects exact per-unit windows.  When
/// the numbered units carry **pairwise distinct** values (⇒ distinct regions)
/// and the values sum to the whole board, the sum identity leaves no room for
/// further regions — returns the label-count cap (`0262` class: 4 corner
/// numbers 14/15/18/17 sum to 64 = board).  Contradictory windows surface
/// through the regular window checks.
fn inject_number_clues(
    puzzle: &Puzzle,
    free_cells: &[usize],
    unit_of_cell: &[u32],
    lo: &mut [usize],
    hi: &mut [usize],
    total: usize,
    w: usize,
) -> Option<(usize, Vec<(u32, usize)>)> {
    let mut vals: Vec<(u32, usize)> = Vec::new();
    let mut all_sum = 0usize;
    let mut per_value: std::collections::HashMap<usize, usize> = Default::default();
    for (i, &idx) in free_cells.iter().enumerate() {
        let (r, c) = (idx / w, idx % w);
        if let Some(n) = puzzle.cells[r][c].number {
            let u = unit_of_cell[i] as usize;
            let n = n.max(1) as usize;
            if lo[u] < n {
                lo[u] = n;
            }
            if hi[u] > n {
                hi[u] = n;
            }
            *per_value.entry(n).or_insert(0) += 1;
            all_sum += n;
            if !vals.iter().any(|(x, _)| *x == u as u32) {
                vals.push((u as u32, n));
            } else {
                // Two number cells collapsed into one must-same unit: they
                // would share a region, so a per-clue lock is unsatisfiable.
                let prev = vals.iter().find(|(x, _)| *x == u as u32).unwrap().1;
                if prev != n {
                    return None;
                }
            }
        }
    }
    if vals.is_empty() {
        return None;
    }
    // Sum-lock v2 (two forms; `check_area` = number == region size):
    //  * **all-sum**: Σ over number cells == total  ⇒ every clue owns its
    //    own region (Σ v·r_v = total ≤ Σ v·c_v = all-sum, equality iff
    //    r_v = c_v ∀v) — region count = clue-cell count (0308: 14×5+84).
    //  * **distinct-sum**: Σ over distinct values == total ⇒ r_v = 1 ∀v
    //    (any r_v > 1 overshoots) — region count = distinct value count
    //    (0262: 14+15+17+18; 1138: 60+61 with 26 clue cells).
    let distinct_sum: usize = per_value.keys().sum();
    if all_sum == total {
        if per_value.values().sum::<usize>() != vals.len() {
            return None; // clue cells merged into shared units — contradicts
        }
        Some((vals.len(), vals))
    } else if distinct_sum == total {
        Some((per_value.len(), vals))
    } else {
        None
    }
}

/// Hall-interval pinning for strict ord chains over a **known distinct region
/// size set** V (the number-clue lock of `inject_number_clues`: distinct
/// `area` numbers summing to the board ⇒ every unit joins one of those
/// regions, so every label size ∈ V).
///
/// A chain `u1 → … → uc` (sizes strictly increasing along the edges) must
/// embed as an order-preserving injection into sorted V.  The i-th member
/// (0-based, source = smallest) can therefore only take
/// `sorted_v[i ..= i + k − c]` — exact windows when `c == k` (0262: two
/// 4-chains over {14,15,17,18} each pin uniquely to 14<15<17<18).  Sound
/// even when several chains alias into the same regions (values may repeat
/// across chains; the interval bound is per-chain).
///
/// Chains touching a fixed number unit are skipped (their pinning is already
/// reflected in the windows by `inject_number_clues` + `relax_size_windows`).
fn chain_value_pins(
    ord: &[(u32, u32)],
    lo: &mut [usize],
    hi: &mut [usize],
    values: &[(u32, usize)],
) {
    let mut v: Vec<usize> = values.iter().map(|&(_, s)| s).collect();
    v.sort_unstable();
    v.dedup();
    let k = v.len();
    if k == 0 {
        return;
    }
    let fixed: std::collections::HashSet<u32> = values.iter().map(|&(u, _)| u).collect();
    let n = lo.len();
    // Undirected adjacency restricted to non-fixed ord units.
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut touched = vec![false; n];
    for &(a, b) in ord {
        if fixed.contains(&a) || fixed.contains(&b) {
            continue;
        }
        adj[a as usize].push(b);
        adj[b as usize].push(a);
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    let mut seen = vec![false; n];
    for s in 0..n {
        if !touched[s] || seen[s] {
            continue;
        }
        // Weakly-connected comp via undirected DFS.
        let mut comp: Vec<usize> = Vec::new();
        let mut stack = vec![s];
        seen[s] = true;
        while let Some(u) = stack.pop() {
            comp.push(u);
            for &wv in &adj[u] {
                if !seen[wv as usize] {
                    seen[wv as usize] = true;
                    stack.push(wv as usize);
                }
            }
        }
        // A path comp: |E| = |V|−1 and undirected degree ≤ 2.
        let edges: usize = comp.iter().map(|&u| adj[u].len()).sum::<usize>() / 2;
        let path = edges + 1 == comp.len() && comp.iter().all(|&u| adj[u].len() <= 2);
        if !path {
            continue;
        }
        let c = comp.len();
        if c > k {
            continue; // needs more distinct sizes than V offers — infeasible, leave to search
        }
        orient_and_pin_comp(&comp, ord, &fixed, &v, k, &touched, lo, hi);
    }
}

/// Orient one undirected path comp through the ord digraph (consistent
/// chains only: unique indegree-0 endpoint = smallest member, no branching
/// out-edges) and narrow each member's window to its Hall interval in the
/// sorted value set `v` (member i gets `v[i ..= i + k − c]`).
fn orient_and_pin_comp(
    comp: &[usize],
    ord: &[(u32, u32)],
    fixed: &std::collections::HashSet<u32>,
    v: &[usize],
    k: usize,
    touched: &[bool],
    lo: &mut [usize],
    hi: &mut [usize],
) {
    let n = lo.len();
    let c = comp.len();
    let mut indeg = vec![0usize; n];
    let mut follow: Vec<Option<u32>> = vec![None; n];
    let mut multi_out = false;
    for &(a, b) in ord {
        if fixed.contains(&a) || fixed.contains(&b) || !touched[a as usize] {
            continue;
        }
        indeg[b as usize] += 1;
        if follow[a as usize].is_some() {
            multi_out = true;
        }
        follow[a as usize] = Some(b);
    }
    if multi_out {
        return;
    }
    let sources: Vec<usize> = comp.iter().copied().filter(|&u| indeg[u] == 0).collect();
    if sources.len() != 1 {
        return;
    }
    let mut cur = sources[0] as u32;
    let mut walked = 0usize;
    for i in 0..c {
        let lo_v = v[i];
        let hi_v = v[i + k - c];
        let u = cur as usize;
        if lo[u] < lo_v {
            lo[u] = lo_v;
        }
        if hi[u] > hi_v {
            hi[u] = hi_v;
        }
        walked += 1;
        match follow[cur as usize] {
            Some(nx) => cur = nx,
            None => break,
        }
    }
    if walked != c {
        return; // orientation did not span the comp — leave untouched
    }
}

/// Enumerate exact per-unit size cases for a **connected** ord/diff
/// constraint graph (0929 class).  The sufficiency probe (tmp_diag_0929b)
/// showed the search collapses to ~20 nodes once every unit's label size is
/// pinned; the gap system's disjunctions (signed-sum offsets) are invisible
/// to interval windows, so enumerate the surviving size tuples here instead.
/// Assumes each constraint unit holds its own label — merging with equal
/// sizes stays expressible and anything missed falls back to the plain
/// un-hinted search.  Returns `(per-unit exact windows, label count)` cases.
fn enum_size_cases(
    ord: &[(u32, u32)],
    diff: &[(u32, u32, usize)],
    sz_lo: &[usize],
    sz_hi: &[usize],
    total: usize,
    g: usize,
    max_labels: usize,
) -> Vec<(Vec<(usize, usize)>, usize)> {
    const CASE_CAP: usize = 256;
    let n = sz_lo.len();
    // Adjacency over the constraint graph; disconnected graphs are skipped
    // (the cartesian product across components explodes — the plain search
    // already covers those).  kind 0 = ord ("my size < their size"), kind 1
    // = diff gap `|a−b| = val`.
    let mut adj: Vec<Vec<(u32, u8, usize)>> = vec![Vec::new(); n];
    let mut touched = vec![false; n];
    for &(a, b) in ord {
        adj[a as usize].push((b, 0, 0));
        adj[b as usize].push((a, 0, 1));
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    for &(a, b, v) in diff {
        adj[a as usize].push((b, 1, v));
        adj[b as usize].push((a, 1, v));
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    let members: Vec<u32> = (0..n as u32).filter(|&u| touched[u as usize]).collect();
    if members.is_empty() {
        return Vec::new();
    }
    // Component labelling over the constraint graph.
    let mut comp_of = vec![usize::MAX; n];
    let mut comps: Vec<Vec<u32>> = Vec::new();
    for &m0 in &members {
        if comp_of[m0 as usize] != usize::MAX {
            continue;
        }
        let cid = comps.len();
        let mut comp = Vec::new();
        let mut stack = vec![m0 as usize];
        comp_of[m0 as usize] = cid;
        while let Some(u) = stack.pop() {
            comp.push(u as u32);
            for &(v, _, _) in &adj[u] {
                if comp_of[v as usize] == usize::MAX {
                    comp_of[v as usize] = cid;
                    stack.push(v as usize);
                }
            }
        }
        comps.push(comp);
    }
    if comps.len() > 1 {
        // Multi-component product (0912/0928/0930d/1407a class): each
        // component's units are pairwise unconstrained across components, so
        // enumerate per component and take the cartesian product, applying the
        // residual account (`rest = total − Σ sizes`, `rest == 0 || rest >= g`)
        // once over the merged assignment.  Previously this class was skipped
        // outright ("cartesian explosion") and the plain FreeRem run wandered;
        // the cases are window hints (the fallback run always follows), so a
        // wrong hint costs ordering only.
        let mut merged: Vec<(Vec<(usize, usize)>, usize, usize)> =
            vec![(vec![(0usize, 0usize); n], 0usize, 0usize)];
        for comp in &comps {
            let mut order_c = comp.clone();
            order_c.sort_by_key(|&u| std::cmp::Reverse(adj[u as usize].len()));
            let mut comp_cases: Vec<(Vec<(usize, usize)>, usize, usize)> = Vec::new();
            enum_comp_raw(
                &order_c,
                0,
                &adj,
                sz_lo,
                sz_hi,
                total,
                &mut Default::default(),
                &mut comp_cases,
                CASE_CAP,
            );
            if comp_cases.is_empty() {
                return Vec::new();
            }
            let mut next: Vec<(Vec<(usize, usize)>, usize, usize)> = Vec::new();
            'outer: for (mwin, msum, mna) in &merged {
                for (cwin, csum, cna) in &comp_cases {
                    let mut win = mwin.clone();
                    for (u, &ab) in cwin.iter().enumerate() {
                        if ab != (0usize, 0usize) {
                            win[u] = ab;
                        }
                    }
                    next.push((win, msum + csum, mna + cna));
                    if next.len() >= CASE_CAP * 4 {
                        break 'outer;
                    }
                }
            }
            merged = next;
        }
        let mut cases: Vec<(Vec<(usize, usize)>, usize)> = Vec::new();
        for (win, sum, na) in merged {
            let rest = total.saturating_sub(sum);
            let n_lbl = na + if rest > 0 { 1 } else { 0 };
            if sum <= total && n_lbl <= max_labels && (rest == 0 || rest >= g) {
                cases.push((win, n_lbl));
            }
        }
        cases.sort_by_key(|(win, _)| {
            std::cmp::Reverse(win.iter().map(|&(a, _)| a).sum::<usize>())
        });
        cases.truncate(CASE_CAP);
        return cases;
    }
    // Single component: highest-degree first keeps the DFS pruned early.
    let mut order = members.clone();
    order.sort_by_key(|&u| std::cmp::Reverse(adj[u as usize].len()));
    let mut cases: Vec<(Vec<(usize, usize)>, usize)> = Vec::new();
    let mut assign: std::collections::HashMap<u32, usize> = Default::default();
    enum_comp_dfs(
        &order,
        0,
        &adj,
        sz_lo,
        sz_hi,
        total,
        g,
        max_labels,
        &mut assign,
        &mut cases,
        CASE_CAP,
    );
    // Most-constrained first: the true case packs the bulk of the board into
    // the constraint labels (0929: rest = 3 of 30), so smallest remainder
    // (= largest anchor sum) leads.
    cases.sort_by_key(|(win, _)| {
        std::cmp::Reverse(win.iter().map(|&(a, _)| a).sum::<usize>())
    });
    cases
}

fn enum_comp_dfs(
    order: &[u32],
    k: usize,
    adj: &[Vec<(u32, u8, usize)>],
    sz_lo: &[usize],
    sz_hi: &[usize],
    total: usize,
    g: usize,
    max_labels: usize,
    assign: &mut std::collections::HashMap<u32, usize>,
    cases: &mut Vec<(Vec<(usize, usize)>, usize)>,
    cap: usize,
) {
    if cases.len() >= cap {
        return;
    }
    if k == order.len() {
        let sum: usize = assign.values().sum();
        let rest = total.saturating_sub(sum);
        let n_lbl = assign.len() + if rest > 0 { 1 } else { 0 };
        if sum <= total && n_lbl <= max_labels && (rest == 0 || rest >= g) {
            let mut win = vec![(0usize, 0usize); sz_lo.len()];
            for (u, s) in assign.iter() {
                win[*u as usize] = (*s, *s);
            }
            cases.push((win, n_lbl));
        }
        return;
    }
    let u = order[k];
    for s in sz_lo[u as usize]..=sz_hi[u as usize].min(total) {
        if size_edge_ok(u, s, adj, assign) {
            assign.insert(u, s);
            enum_comp_dfs(
                order, k + 1, adj, sz_lo, sz_hi, total, g, max_labels, assign, cases, cap,
            );
            assign.remove(&u);
        }
    }
}

/// One component's raw size assignments for the multi-component product in
/// `enum_size_cases` (0912 class).  The residual account (`rest = total − Σ
/// sizes`, `rest == 0 || rest >= g`) and the label count are global facts, so
/// per-component leaves only report `(window, sum, n_units)` and the product
/// applies the account once over the merged assignment.
fn enum_comp_raw(
    order: &[u32],
    k: usize,
    adj: &[Vec<(u32, u8, usize)>],
    sz_lo: &[usize],
    sz_hi: &[usize],
    total: usize,
    assign: &mut std::collections::HashMap<u32, usize>,
    cases: &mut Vec<(Vec<(usize, usize)>, usize, usize)>,
    cap: usize,
) {
    if cases.len() >= cap {
        return;
    }
    if k == order.len() {
        let sum: usize = assign.values().sum();
        if sum > total {
            return;
        }
        let mut win = vec![(0usize, 0usize); sz_lo.len()];
        for (u, s) in assign.iter() {
            win[*u as usize] = (*s, *s);
        }
        cases.push((win, sum, assign.len()));
        return;
    }
    let u = order[k];
    for s in sz_lo[u as usize]..=sz_hi[u as usize].min(total) {
        if size_edge_ok(u, s, adj, assign) {
            assign.insert(u, s);
            enum_comp_raw(order, k + 1, adj, sz_lo, sz_hi, total, assign, cases, cap);
            assign.remove(&u);
        }
    }
}

/// Shared feasibility test of one unit's size against its already-assigned
/// constraint neighbours (`ord` kind 0: strict order; `diff` kind 1: |a−b|).
fn size_edge_ok(
    u: u32,
    s: usize,
    adj: &[Vec<(u32, u8, usize)>],
    assign: &std::collections::HashMap<u32, usize>,
) -> bool {
    for &(v, kind, val) in &adj[u as usize] {
        if let Some(&t) = assign.get(&v) {
            // kind 0 + val 0 = my < their; kind 0 + val 1 = their < my.
            let fits = if kind == 0 {
                if val == 0 {
                    s < t
                } else {
                    t < s
                }
            } else {
                s.abs_diff(t) == val
            };
            if !fits {
                return false;
            }
        }
    }
    true
}

/// Distinct-size chain deduction over the unit-ord digraph (0152 class).
///
/// A directed path `u1 → u2 → … → uc` forces pairwise-distinct label sizes
/// `s1 < s2 < … < sc` (a shared label would be `size(L) < size(L)`), so with
/// every region at least `g` cells the chain sum obeys
/// `Σ s_k ≥ c·g + c·(c−1)/2 =: floor`.  Labels outside the chain add `m·g`
/// more, hence `m ≤ (total − floor) / g` and the total label count is capped
/// at `c + m`.  When `floor == total` everything is pinned: `s_k = g+k−1`
/// exactly (the 0152 case: 8-chain over a 36-cell board = sizes 1..8), no
/// label outside the chain may exist, and every free unit must join one of
/// the chain labels.
///
/// Returns the label-count bound.  `None` = the ord digraph has a cycle
/// (strict orders around a cycle are unsatisfiable — Bellman-Ford on
/// `usize::MAX`-hi windows cannot see this).
fn chain_size_deduction(
    ord: &[(u32, u32)],
    lo: &mut [usize],
    hi: &mut [usize],
    total: usize,
    g: usize,
) -> Option<usize> {
    let n = lo.len();
    let g = g.max(1);
    // Kahn topo order; cycle ⇒ unsat.
    let mut indeg = vec![0usize; n];
    let mut outs: Vec<Vec<u32>> = vec![Vec::new(); n];
    for &(ua, ub) in ord {
        outs[ua as usize].push(ub);
        indeg[ub as usize] += 1;
    }
    let mut queue: Vec<u32> = (0..n as u32).filter(|&u| indeg[u as usize] == 0).collect();
    let mut topo: Vec<u32> = Vec::with_capacity(n);
    while let Some(u) = queue.pop() {
        topo.push(u);
        for &v in &outs[u as usize] {
            indeg[v as usize] -= 1;
            if indeg[v as usize] == 0 {
                queue.push(v);
            }
        }
    }
    if topo.len() != n {
        return None;
    }
    // Longest directed path (chain) with parent pointers.
    let mut dist = vec![1usize; n];
    let mut parent = vec![u32::MAX; n];
    for &u in &topo {
        for &v in &outs[u as usize] {
            if dist[u as usize] + 1 > dist[v as usize] {
                dist[v as usize] = dist[u as usize] + 1;
                parent[v as usize] = u;
            }
        }
    }
    let (mut c, mut end) = (0usize, 0u32);
    for u in 0..n {
        if dist[u] > c {
            c = dist[u];
            end = u as u32;
        }
    }
    if c == 0 {
        return Some(usize::MAX);
    }
    let mut chain: Vec<u32> = Vec::with_capacity(c);
    let mut cur = end;
    while cur != u32::MAX {
        chain.push(cur);
        cur = parent[cur as usize];
    }
    chain.reverse(); // u1 (smallest) .. uc (largest)
    let floor = c * g + c * (c - 1) / 2;
    if floor > total {
        return None;
    }
    for (k, &u) in chain.iter().enumerate() {
        let s_min = g + k;
        let s_max = total - floor + s_min;
        let i = u as usize;
        if lo[i] < s_min {
            lo[i] = s_min;
        }
        if hi[i] > s_max {
            hi[i] = s_max;
        }
        if lo[i] > hi[i] {
            return None;
        }
    }
    Some(c + (total - floor) / g)
}

/// Directed size orders from `inequality` edges (`size(a) < size(b)`; `value
/// == 1` flips — matching `validate::edge_constraint_ok`).  Returns the
/// unit-unit orders plus per-unit size floors/caps (from pinned sides), or
/// `None` when a fixed pair already violates its order.  Order pairs double
/// as must-differ (no label satisfies `size(L) < size(L)`).
fn collect_size_orders(
    puzzle: &Puzzle,
    w: usize,
    pinned: &[PinnedPlacement],
    pin_of: &[Option<usize>],
    free_index: &std::collections::HashMap<usize, u32>,
    unit_of_cell: &[u32],
) -> Option<(Vec<(u32, u32)>, Vec<(u32, u32, usize)>, Vec<usize>, Vec<usize>)> {
    let n_units = unit_of_cell
        .iter()
        .copied()
        .max()
        .map_or(0, |m| m as usize + 1);
    let mut ord: Vec<(u32, u32)> = Vec::new();
    let mut diff: Vec<(u32, u32, usize)> = Vec::new();
    let mut lo = vec![1usize; n_units];
    let mut hi = vec![usize::MAX; n_units];
    let edge = |a: usize,
                    b: usize,
                    reversed: bool,
                    ord: &mut Vec<(u32, u32)>,
                    lo: &mut Vec<usize>,
                    hi: &mut Vec<usize>|
     -> Option<()> {
        let (li, ui) = if reversed { (b, a) } else { (a, b) };
        let lsz = pin_of[li].map(|ri| pinned[ri].cells.len());
        let usz = pin_of[ui].map(|ri| pinned[ri].cells.len());
        match (lsz, usz) {
            (Some(sl), Some(su)) => {
                if sl >= su {
                    return None;
                }
            }
            (None, Some(su)) => {
                let u = unit_of_cell[free_index[&li] as usize] as usize;
                hi[u] = hi[u].min(su.saturating_sub(1));
            }
            (Some(sl), None) => {
                let u = unit_of_cell[free_index[&ui] as usize] as usize;
                lo[u] = lo[u].max(sl + 1);
            }
            (None, None) => {
                let ua = unit_of_cell[free_index[&li] as usize];
                let ub = unit_of_cell[free_index[&ui] as usize];
                if ua == ub {
                    return None;
                }
                ord.push((ua, ub));
            }
        }
        Some(())
    };
    let diff_edge = |a: usize,
                         b: usize,
                         v: usize,
                         diff: &mut Vec<(u32, u32)>,
                         diffv: &mut Vec<(u32, u32, usize)>|
     -> Option<()> {
        let _ = diff;
        match (pin_of[a], pin_of[b]) {
            (Some(_), Some(_)) => Some(()),
            (None, None) => {
                let ua = unit_of_cell[free_index[&a] as usize];
                let ub = unit_of_cell[free_index[&b] as usize];
                if ua == ub {
                    return if v == 0 { Some(()) } else { None };
                }
                diffv.push((ua, ub, v));
                Some(())
            }
            _ => Some(()),
        }
    };
    for r in 0..puzzle.height {
        for c in 0..puzzle.width.saturating_sub(1) {
            if let Some(ec) = &puzzle.h_edges[r][c].constraint {
                if matches!(ec.ctype, crate::types::EdgeConstraintType::Inequality) {
                    edge(r * w + c, r * w + c + 1, ec.value == Some(1), &mut ord, &mut lo, &mut hi)?;
                } else if matches!(ec.ctype, crate::types::EdgeConstraintType::Difference) {
                    diff_edge(
                        r * w + c,
                        r * w + c + 1,
                        ec.value.unwrap_or(0).max(0) as usize,
                        &mut Vec::new(),
                        &mut diff,
                    )?;
                }
            }
        }
    }
    for r in 0..puzzle.height.saturating_sub(1) {
        for c in 0..puzzle.width {
            if let Some(ec) = &puzzle.v_edges[r][c].constraint {
                if matches!(ec.ctype, crate::types::EdgeConstraintType::Inequality) {
                    edge(r * w + c, (r + 1) * w + c, ec.value == Some(1), &mut ord, &mut lo, &mut hi)?;
                } else if matches!(ec.ctype, crate::types::EdgeConstraintType::Difference) {
                    diff_edge(
                        r * w + c,
                        (r + 1) * w + c,
                        ec.value.unwrap_or(0).max(0) as usize,
                        &mut Vec::new(),
                        &mut diff,
                    )?;
                }
            }
        }
    }
    relax_size_windows(&ord, &diff, &mut lo, &mut hi);
    for u in 0..n_units {
        if lo[u] > hi[u] {
            return None;
        }
    }
    Some((ord, diff, lo, hi))
}

/// Must-same closure (UF) + pre-drawn walls as must-split + unit compression
/// + unit adjacency.  Returns `(unit_cells, unit_of_cell, unit_adj, split)`;
/// `None` on a must-same class straddling a must-split pair.
#[allow(clippy::type_complexity)]
fn build_free_units(
    free_cells: &[usize],
    _free_index: &std::collections::HashMap<usize, u32>,
    same: &[(u32, u32)],
    split_pairs: &[(u32, u32)],
    puzzle: &Puzzle,
    w: usize,
) -> Option<(Vec<Vec<usize>>, Vec<u32>, Vec<Vec<u32>>, std::collections::HashSet<(u32, u32)>)> {
    // Must-same closure + pre-drawn walls as must-split.
    let mut uf = Uf::new(free_cells.len());
    for &(a, b) in same {
        uf.union(a, b);
    }
    let mut split_cell: Vec<(u32, u32)> = split_pairs.to_vec();
    let pre = PreBoundaries::from_puzzle(puzzle);
    for i in 0..free_cells.len() {
        for j in (i + 1)..free_cells.len() {
            let (a, b) = (free_cells[i], free_cells[j]);
            let ((r1, c1), (r2, c2)) = ((a / w, a % w), (b / w, b % w));
            let adj = (r1 == r2 && c1.abs_diff(c2) == 1) || (c1 == c2 && r1.abs_diff(r2) == 1);
            if adj && pre.contains(r1, c1, r2, c2) {
                split_cell.push((i as u32, j as u32));
            }
        }
    }

    // Compress must-same classes into units.
    let mut root_to_unit: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();
    let mut unit_cells: Vec<Vec<usize>> = Vec::new();
    let mut unit_of_cell: Vec<u32> = vec![u32::MAX; free_cells.len()];
    for (i, &idx) in free_cells.iter().enumerate() {
        let r = uf.find(i as u32);
        let u = *root_to_unit.entry(r).or_insert_with(|| {
            unit_cells.push(Vec::new());
            (unit_cells.len() - 1) as u32
        });
        unit_cells[u as usize].push(idx);
        unit_of_cell[i] = u;
    }
    // Conflict: a must-same class straddling a must-split pair is unsolvable.
    let mut split: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for &(a, b) in &split_cell {
        let (ua, ub) = (unit_of_cell[a as usize], unit_of_cell[b as usize]);
        if ua == ub {
            return None;
        }
        split.insert(canon_pair(ua, ub));
    }

    // Unit adjacency (join rule) over non-wall free-free edges.
    let mut unit_adj: Vec<std::collections::BTreeSet<u32>> =
        vec![Default::default(); unit_cells.len()];
    for i in 0..free_cells.len() {
        for j in (i + 1)..free_cells.len() {
            let (a, b) = (free_cells[i], free_cells[j]);
            let ((r1, c1), (r2, c2)) = ((a / w, a % w), (b / w, b % w));
            let adj = (r1 == r2 && c1.abs_diff(c2) == 1) || (c1 == c2 && r1.abs_diff(r2) == 1);
            if adj && !pre.contains(r1, c1, r2, c2) {
                let (ua, ub) = (unit_of_cell[i], unit_of_cell[j]);
                if ua != ub {
                    unit_adj[ua as usize].insert(ub);
                    unit_adj[ub as usize].insert(ua);
                }
            }
        }
    }
    let unit_adj: Vec<Vec<u32>> = unit_adj.into_iter().map(|s| s.into_iter().collect()).collect();

    Some((unit_cells, unit_of_cell, unit_adj, split))
}

/// Area-bounded partition (1351 class: `range`/`precise` with an emergent
/// region count): every region's size lives in `area_bounds`, so the FreeRem
/// size windows + the spawn ceiling `total / min_sz` carry the whole search.
/// Pre-drawn walls arrive as must-split via `build_free_units`.
/// Merge a case's exact-window overrides (`0` = sentinel, keep base) into
/// the concrete `sz_lo`/`sz_hi` pair for one run.
fn merge_case_windows(
    case_lo: &[usize],
    case_hi: &[usize],
    base_lo: &[usize],
    base_hi: &[usize],
) -> (Vec<usize>, Vec<usize>) {
    let mut lo: Vec<usize> = Vec::with_capacity(base_lo.len());
    let mut hi: Vec<usize> = Vec::with_capacity(base_hi.len());
    for u in 0..base_lo.len() {
        if case_lo[u] != 0 || case_hi[u] != 0 {
            lo.push(case_lo[u]);
            hi.push(case_hi[u]);
        } else {
            lo.push(base_lo[u]);
            hi.push(base_hi[u]);
        }
    }
    (lo, hi)
}

/// Per-run deadline: 800 ms per size case (the true case converges in
/// ~0.5 s; wrong cases die shallow), full residual for the plain run.
fn run_deadline(is_case: bool, module_deadline: crate::clock::Instant) -> crate::clock::Instant {
    // `RANGE_CASE_MS` diagnostic override (default 800).
    let case_ms: u64 = std::env::var("RANGE_CASE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(800);
    let budget = if is_case {
        std::time::Duration::from_millis(case_ms)
    } else {
        std::time::Duration::from_secs(12)
    };
    let t = crate::clock::Instant::now() + budget;
    if t < module_deadline {
        t
    } else {
        module_deadline
    }
}

/// Size-window runs to try: one exact-window run per enumerated size case
/// (untouched units keep `(0, 0)` = base windows) with the case's own label
/// count as the spawn ceiling, then the plain un-hinted run last (the
/// residual budget fallback).
fn windows_runs_from_cases(
    cases: &[(Vec<(usize, usize)>, usize)],
    n_units: usize,
    max_labels: usize,
) -> Vec<(Vec<usize>, Vec<usize>, usize)> {
    let mut runs: Vec<(Vec<usize>, Vec<usize>, usize)> = Vec::with_capacity(cases.len() + 1);
    for (win, n_lbl) in cases {
        let mut lo = vec![0usize; n_units];
        let mut hi = vec![0usize; n_units];
        for (u, &(a, b)) in win.iter().enumerate() {
            if a != 0 || b != 0 {
                lo[u] = a;
                hi[u] = b;
            }
        }
        // Sentinel 0 = keep base window at run time.
        runs.push((lo, hi, *n_lbl));
    }
    runs.push((vec![0usize; n_units], vec![0usize; n_units], max_labels));
    runs
}

/// Value-assignment cases under the number-clue lock (0262 class): the
/// region-size multiset V is fully known (distinct `area` numbers summing to
/// the board) and every ord/diff-constraint unit joins one of the number
/// regions, so each unit's size ∈ V.  Strict chains over V collapse (a
/// c=|V| chain has a unique order-preserving assignment — already pinned by
/// `chain_value_pins`); the residue is small (0262: two 2-chains ⇒ 6×6=36
/// cases).  Pre-drawn-wall / must-split pairs force distinct values (values
/// are distinct per region).  Each case pins the constraint units' windows
/// exactly; number units keep their injected windows.
///
/// Sorted by anchor distance (unit's first cell → its value's number-cell,
/// Manhattan): a region's cells tend to sit near its number clue, so the
/// true assignment leads.  Ordering only — never a filter.
fn enum_number_cases(
    ord: &[(u32, u32)],
    diff: &[(u32, u32, usize)],
    n_units: usize,
    num_vals: &[(u32, usize)],
    split: &std::collections::HashSet<(u32, u32)>,
    sz_lo: &[usize],
    sz_hi: &[usize],
    unit_cells: &[Vec<usize>],
    unit_adj: &[Vec<u32>],
    w: usize,
    num_anchors: &[(usize, usize, usize)], // (value, r, c)
) -> Vec<(Vec<(usize, usize)>, usize)> {
    const CASE_CAP: usize = 256;
    let mut v: Vec<usize> = num_vals.iter().map(|&(_, s)| s).collect();
    v.sort_unstable();
    v.dedup();
    if v.is_empty() {
        return Vec::new();
    }
    let fixed: std::collections::HashMap<u32, usize> = num_vals.iter().copied().collect();
    let mut adj: Vec<Vec<(u32, u8, usize)>> = vec![Vec::new(); n_units];
    let mut touched = vec![false; n_units];
    for &(a, b) in ord {
        // kind 0: my < their.
        adj[a as usize].push((b, 0, 0));
        adj[b as usize].push((a, 0, 1));
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    for &(a, b, gap) in diff {
        adj[a as usize].push((b, 1, gap));
        adj[b as usize].push((a, 1, gap));
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    for &(a, b) in split {
        adj[a as usize].push((b, 2, 0));
        adj[b as usize].push((a, 2, 0));
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    // DFS over the non-fixed constraint units; each tries every value of V
    // that survives its unit window and the pairwise checks.  Components are
    // explored jointly (cross-component aliasing is legal: equal values may
    // repeat across comps) — the cartesian product stays tiny because V is
    // small and chains are pre-pinned.
    let members: Vec<u32> = (0..n_units as u32)
        .filter(|&u| touched[u as usize] && !fixed.contains_key(&u))
        .collect();
    if members.is_empty() {
        return Vec::new();
    }
    // Most-constrained first: units with the tightest window lead.
    let mut order = members.clone();
    order.sort_by_key(|&u| sz_hi[u as usize] - sz_lo[u as usize]);
    let mut cases: Vec<(Vec<(usize, usize)>, usize)> = Vec::new();
    let mut assign: std::collections::HashMap<u32, usize> = Default::default();
    enum_value_dfs(
        &order,
        0,
        &adj,
        &fixed,
        &v,
        sz_lo,
        sz_hi,
        &mut assign,
        &mut cases,
        CASE_CAP,
    );
    let dist_of = |val: usize, u: u32| -> usize {
        let (tr, tc) = match num_anchors.iter().find(|(x, _, _)| *x == val) {
            Some(&(_, r, c)) => (r, c),
            None => return 0,
        };
        let cell = unit_cells[u as usize][0];
        let (r, c) = (cell / w, cell % w);
        r.abs_diff(tr) + c.abs_diff(tc)
    };
    // Primary key: adjacency cohesion DESC — adjacent non-wall units sharing
    // a value lie in the same region, and the true assignment maximises the
    // merges it forces (0262: true case scores 2, all others ≤ 1).  Secondary:
    // anchor distance ASC (cells tend to sit near their region's number).
    let val_of = |win: &[(usize, usize)], u: u32| -> Option<usize> {
        let (a, b) = win[u as usize];
        if a != 0 || b != 0 {
            Some(a)
        } else {
            fixed.get(&u).copied()
        }
    };
    let score = |win: &[(usize, usize)]| -> usize {
        let mut s = 0usize;
        for u in 0..n_units as u32 {
            let Some(vu) = val_of(win, u) else { continue };
            for &nb in &unit_adj[u as usize] {
                if nb <= u {
                    continue;
                }
                if split.contains(&canon_pair(u, nb)) {
                    continue;
                }
                if val_of(win, nb) == Some(vu) {
                    s += 1;
                }
            }
        }
        s
    };
    cases.sort_by_cached_key(|(win, _)| {
        let dist: usize = win
            .iter()
            .enumerate()
            .filter(|(_, &(a, b))| a != 0 || b != 0)
            .map(|(u, &(a, _))| dist_of(a, u as u32))
            .sum();
        (std::cmp::Reverse(score(win)), dist)
    });
    cases
}

/// DFS for `enum_number_cases`.  kind 0 = ord (val 0: my < their, val 1:
/// their < my), kind 1 = gap |a−b| = val, kind 2 = must-split (values differ).
fn enum_value_dfs(
    order: &[u32],
    k: usize,
    adj: &[Vec<(u32, u8, usize)>],
    fixed: &std::collections::HashMap<u32, usize>,
    values: &[usize],
    sz_lo: &[usize],
    sz_hi: &[usize],
    assign: &mut std::collections::HashMap<u32, usize>,
    cases: &mut Vec<(Vec<(usize, usize)>, usize)>,
    cap: usize,
) {
    if cases.len() >= cap {
        return;
    }
    if k == order.len() {
        let mut win = vec![(0usize, 0usize); sz_lo.len()];
        for (u, s) in assign.iter() {
            win[*u as usize] = (*s, *s);
        }
        cases.push((win, fixed.len() + assign.len()));
        return;
    }
    let u = order[k];
    for &s in values {
        if s < sz_lo[u as usize] || s > sz_hi[u as usize] {
            continue;
        }
        let mut ok = true;
        for &(v, kind, val) in &adj[u as usize] {
            let t = match assign.get(&v) {
                Some(&t) => t,
                None => match fixed.get(&v) {
                    Some(&t) => t,
                    None => continue,
                },
            };
            let fits = match kind {
                0 => {
                    if val == 0 {
                        s < t
                    } else {
                        t < s
                    }
                }
                1 => s.abs_diff(t) == val,
                _ => s != t,
            };
            if !fits {
                ok = false;
                break;
            }
        }
        if ok {
            assign.insert(u, s);
            enum_value_dfs(order, k + 1, adj, fixed, values, sz_lo, sz_hi, assign, cases, cap);
            assign.remove(&u);
        }
    }
}

/// Region count from rose cardinality: each symbol type appears `m` times
/// and every region holds exactly one of each type ⇒ exactly `m` regions.
/// The rarest type's cells are the region anchors (one per region).  `None`
/// when the puzzle has no rose symbols or the per-type counts disagree.
fn rose_region_count(puzzle: &Puzzle, w: usize) -> Option<(usize, Vec<usize>)> {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut cells_by_type: HashMap<&str, Vec<usize>> = HashMap::new();
    for r in 0..puzzle.height {
        for c in 0..w {
            if puzzle.cells[r][c].blocked {
                continue;
            }
            if let Some(sym) = puzzle.cells[r][c].symbol.as_deref() {
                *counts.entry(sym).or_insert(0) += 1;
                cells_by_type.entry(sym).or_default().push(r * w + c);
            }
        }
    }
    if counts.is_empty() {
        return None;
    }
    let m = *counts.values().next()?;
    if m == 0 || m > 8 || !counts.values().all(|&c| c == m) {
        return None;
    }
    let mut cells: Vec<usize> = cells_by_type.values().min_by_key(|v| v.len())?.clone();
    cells.sort_unstable();
    Some((m, cells))
}

/// Alias-aware multi-component cases (0839 class): the gap/ord constraint
/// graph is **disconnected at unit level** (wall ends alias into a shared
/// region set), so `enum_size_cases` skips it.  With the region count `m`
/// known (rose cardinality) enumerate (unit → region) aliasings — wall /
/// must-split endpoints must land in different regions, ord endpoints
/// always differ (no size satisfies `s < s`) — then a joint size-tuple DFS
/// satisfying the region-level gap/ord constraints and the sum identity.
/// Each case pins every constraint unit and every region anchor to its
/// region's exact size; `0839` (3 gap walls over 3 seeded regions) is the
/// prototype.  Returns `(per-unit exact windows, m)` cases.
fn enum_alias_gap_cases(
    ord: &[(u32, u32)],
    diff: &[(u32, u32, usize)],
    n_units: usize,
    anchor_units: &[u32],
    split: &std::collections::HashSet<(u32, u32)>,
    m: usize,
    total: usize,
    unit_cells: &[Vec<usize>],
    unit_adj: &[Vec<u32>],
    w: usize,
) -> Vec<(Vec<(usize, usize)>, usize)> {
    const CASE_CAP: usize = 256;
    if m == 0 || anchor_units.len() != m {
        return Vec::new();
    }
    let mut touched = vec![false; n_units];
    for &(a, b) in ord {
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    for &(a, b, _) in diff {
        touched[a as usize] = true;
        touched[b as usize] = true;
    }
    let fixed: std::collections::HashMap<u32, usize> = anchor_units
        .iter()
        .enumerate()
        .map(|(k, &u)| (u, k))
        .collect();
    let members: Vec<u32> = (0..n_units as u32)
        .filter(|&u| touched[u as usize] && !fixed.contains_key(&u))
        .collect();
    if members.is_empty() {
        return Vec::new();
    }
    // Precomputed constraint adjacency: every kind 0 (split) / kind 1 (ord)
    // / kind 2 with gap > 0 pair means the two units lie in different
    // regions (no size satisfies `s < s` or `|a−b| = v > 0` within one
    // region); kind 2 with gap 0 allows a merge.
    let mut adj: Vec<Vec<(u32, u8, usize)>> = vec![Vec::new(); n_units];
    for &(a, b) in split {
        adj[a as usize].push((b, 0, 0));
        adj[b as usize].push((a, 0, 0));
    }
    for &(a, b) in ord {
        // kind 1 val 0 = size(a) < size(b); the reverse edge carries val 1.
        adj[a as usize].push((b, 1, 0));
        adj[b as usize].push((a, 1, 1));
    }
    for &(a, b, v) in diff {
        adj[a as usize].push((b, 2, v));
        adj[b as usize].push((a, 2, v));
    }
    let mut cases: Vec<(Vec<(usize, usize)>, usize)> = Vec::new();
    let mut region_of: std::collections::HashMap<u32, usize> = Default::default();
    alias_dfs(
        &members,
        0,
        m,
        &adj,
        &fixed,
        total,
        &mut region_of,
        &mut cases,
        n_units,
        CASE_CAP,
    );
    // Ordering: adjacency cohesion DESC (adjacent non-wall units sharing a
    // region), then anchor distance ASC (unit near its region's seed).
    let anchor_cell = |reg: usize| -> (usize, usize) {
        let cell = unit_cells[anchor_units[reg] as usize][0];
        (cell / w, cell % w)
    };
    let val_of_reg = |win: &[(usize, usize)], u: u32| -> Option<usize> {
        let (a, b) = win[u as usize];
        if a != 0 || b != 0 {
            Some(a)
        } else {
            None
        }
    };
    cases.sort_by_cached_key(|(win, _)| {
        let mut cohesion = 0usize;
        for u in 0..n_units as u32 {
            let Some(vu) = val_of_reg(win, u) else { continue };
            for &nb in &unit_adj[u as usize] {
                if nb <= u || split.contains(&canon_pair(u, nb)) {
                    continue;
                }
                if val_of_reg(win, nb) == Some(vu) {
                    cohesion += 1;
                }
            }
        }
        let dist: usize = win
            .iter()
            .enumerate()
            .filter(|(_, &(a, b))| a != 0 || b != 0)
            .map(|(u, &(a, _))| {
                // distance from unit u's first cell to a's region anchor:
                // recover the region by matching the window value to a
                // region size — values may repeat across regions, so use
                // the region-indexed anchor via the pinned window only
                // when unique; else 0.
                let cell = unit_cells[u][0];
                let (r, c) = (cell / w, cell % w);
                let mut best = usize::MAX;
                for k in 0..anchor_units.len() {
                    if win[anchor_units[k] as usize].0 == a {
                        let (ar, ac) = anchor_cell(k);
                        best = best.min(r.abs_diff(ar) + c.abs_diff(ac));
                    }
                }
                if best == usize::MAX {
                    0
                } else {
                    best
                }
            })
            .sum();
        (std::cmp::Reverse(cohesion), dist)
    });
    cases
}

/// DFS over (unit → region) aliasings; each complete aliasing runs the
/// region-size tuple DFS and emits exact-window cases.  Constraint pairs
/// (split / ord / gap>0) forbid a shared region; `kind 2, val 0` gap pairs
/// are the only merge-friendly edges.
fn alias_dfs(
    members: &[u32],
    k: usize,
    m: usize,
    adj: &[Vec<(u32, u8, usize)>],
    fixed: &std::collections::HashMap<u32, usize>,
    total: usize,
    region_of: &mut std::collections::HashMap<u32, usize>,
    cases: &mut Vec<(Vec<(usize, usize)>, usize)>,
    n_units: usize,
    cap: usize,
) {
    if cases.len() >= cap {
        return;
    }
    if k == members.len() {
        emit_size_cases(adj, fixed, region_of, m, total, cases, n_units, cap);
        return;
    }
    let u = members[k];
    'vals: for reg in 0..m {
        for &(v, kind, val) in &adj[u as usize] {
            if kind == 2 && val == 0 {
                continue;
            }
            let other = match region_of.get(&v) {
                Some(&r) => r,
                None => match fixed.get(&v) {
                    Some(&r) => r,
                    None => continue,
                },
            };
            if other == reg {
                continue 'vals;
            }
        }
        region_of.insert(u, reg);
        alias_dfs(members, k + 1, m, adj, fixed, total, region_of, cases, n_units, cap);
        region_of.remove(&u);
    }
}

/// Size-tuple DFS for one complete aliasing: region sizes ≥ 1 summing to
/// `total`, satisfying the region-level ord/gap constraints.  Emits one
/// exact-window case per surviving tuple.
fn emit_size_cases(
    adj: &[Vec<(u32, u8, usize)>],
    fixed: &std::collections::HashMap<u32, usize>,
    region_of: &std::collections::HashMap<u32, usize>,
    m: usize,
    total: usize,
    cases: &mut Vec<(Vec<(usize, usize)>, usize)>,
    n_units: usize,
    cap: usize,
) {
    let reg_of = |u: u32| -> usize {
        *region_of
            .get(&u)
            .or_else(|| fixed.get(&u))
            .expect("aliasing complete")
    };
    let mut r_ord: Vec<(usize, usize)> = Vec::new();
    let mut r_diff: Vec<(usize, usize, usize)> = Vec::new();
    // Dedup key is (pair, kind): one wall can carry BOTH a split entry and a
    // gap/ord entry, and dropping the second would lose its constraint.
    let mut seen_pairs: std::collections::HashSet<((u32, u32), u8)> = Default::default();
    for u in 0..adj.len() as u32 {
        for &(v, kind, val) in &adj[u as usize] {
            if !seen_pairs.insert((canon_pair(u, v), kind)) {
                continue;
            }
            let (ra, rb) = (reg_of(u), reg_of(v));
            match kind {
                0 => {
                    if ra == rb {
                        return;
                    }
                }
                1 => {
                    if ra == rb {
                        return;
                    }
                    // val 0 = size(u) < size(v); val 1 = size(v) < size(u).
                    let p = if val == 0 { (ra, rb) } else { (rb, ra) };
                    if !r_ord.contains(&p) {
                        r_ord.push(p);
                    }
                }
                _ => {
                    if ra == rb {
                        if val == 0 {
                            continue;
                        }
                        return;
                    }
                    let q = if ra < rb { (ra, rb) } else { (rb, ra) };
                    if !r_diff.iter().any(|&(x, y, t)| (x, y) == q && t == val) {
                        r_diff.push((q.0, q.1, val));
                    }
                }
            }
        }
    }
    // Size DFS over regions 0..m−1; the last region closes the sum.
    let mut sizes = vec![0usize; m];
    size_dfs(
        0,
        m,
        total,
        &r_ord,
        &r_diff,
        &mut sizes,
        cases,
        n_units,
        fixed,
        region_of,
        cap,
    );
}

fn size_dfs(
    k: usize,
    m: usize,
    total: usize,
    r_ord: &[(usize, usize)],
    r_diff: &[(usize, usize, usize)],
    sizes: &mut [usize],
    cases: &mut Vec<(Vec<(usize, usize)>, usize)>,
    n_units: usize,
    fixed: &std::collections::HashMap<u32, usize>,
    region_of: &std::collections::HashMap<u32, usize>,
    cap: usize,
) {
    if cases.len() >= cap {
        return;
    }
    if k == m {
        if sizes.iter().sum::<usize>() != total {
            return;
        }
        let mut win = vec![(0usize, 0usize); n_units];
        for (&u, &reg) in region_of.iter().chain(fixed.iter()) {
            win[u as usize] = (sizes[reg], sizes[reg]);
        }
        cases.push((win, m));
        return;
    }
    let remaining_regions = m - k;
    let assigned: usize = sizes[..k].iter().sum();
    let max_here = total - assigned - (remaining_regions - 1);
    for s in 1..=max_here {
        let mut ok = true;
        for &(a, b) in r_ord {
            if a == k && b < k && s <= sizes[b] {
                ok = false;
            }
            if b == k && a < k && sizes[a] >= s {
                ok = false;
            }
        }
        for &(a, b, v) in r_diff {
            let other = if a == k {
                sizes[b]
            } else if b == k {
                sizes[a]
            } else {
                continue;
            };
            if (if a == k { b } else { a }) < k && s.abs_diff(other) != v {
                ok = false;
            }
        }
        if ok {
            sizes[k] = s;
            size_dfs(
                k + 1,
                m,
                total,
                r_ord,
                r_diff,
                sizes,
                cases,
                n_units,
                fixed,
                region_of,
                cap,
            );
            sizes[k] = 0;
        }
    }
}

/// Choose the case-window source: number-locked puzzles enumerate value
/// assignments (0262 class); otherwise the connected-graph size tuples
/// (0929 class); `RANGE_CASES=0` skips straight to the plain run.
fn select_case_runs(
    puzzle: &Puzzle,
    ord: &[(u32, u32)],
    diff: &[(u32, u32, usize)],
    sz_lo: &[usize],
    sz_hi: &[usize],
    total: usize,
    g: usize,
    max_labels: usize,
    num_lock: Option<(&[(u32, usize)], &std::collections::HashSet<(u32, u32)>)>,
    rose_lock: Option<(usize, &Vec<u32>)>,
    split: &std::collections::HashSet<(u32, u32)>,
    unit_cells: &[Vec<usize>],
    unit_adj: &[Vec<u32>],
    w: usize,
) -> Vec<(Vec<usize>, Vec<usize>, usize)> {
    if std::env::var("RANGE_CASES").map(|v| v == "0").unwrap_or(false) {
        return windows_runs_from_cases(&[], sz_lo.len(), max_labels);
    }
    if let Some((num_vals, split)) = num_lock {
        let num_anchors: Vec<(usize, usize, usize)> = num_vals
            .iter()
            .map(|&(u, val)| {
                let cell = unit_cells[u as usize][0];
                (val, cell / w, cell % w)
            })
            .collect();
        let _ = puzzle;
        let cases = enum_number_cases(
            ord,
            diff,
            sz_lo.len(),
            num_vals,
            split,
            sz_lo,
            sz_hi,
            unit_cells,
            unit_adj,
            w,
            &num_anchors,
        );
        windows_runs_from_cases(&cases, sz_lo.len(), max_labels)
    } else {
        let mut cases = enum_size_cases(ord, diff, sz_lo, sz_hi, total, g, max_labels);
        // Disconnected gap/ord graphs (0839 class): wall ends alias into a
        // shared seeded region set — enumerate aliasings jointly with the
        // region size tuples instead.
        if cases.is_empty() {
            if let Some((m, anchors)) = rose_lock {
                cases = enum_alias_gap_cases(
                    ord,
                    diff,
                    sz_lo.len(),
                    anchors,
                    split,
                    m,
                    total,
                    unit_cells,
                    unit_adj,
                    w,
                );
            }
        }
        windows_runs_from_cases(&cases, sz_lo.len(), max_labels)
    }
}

/// Build the per-unit watchtower fact records (slots resolved through the
/// pin/free index map) plus the per-unit fact-id lists.
fn build_wt_unit_facts(
    facts_raw: &[(Vec<usize>, usize)],
    pin_of: &[Option<usize>],
    free_index: &std::collections::HashMap<usize, u32>,
    unit_of_cell: &[u32],
    n_units: usize,
) -> (Vec<WtUnitFact>, Vec<Vec<usize>>) {
    let mut facts: Vec<WtUnitFact> = Vec::new();
    let mut facts_of_unit: Vec<Vec<usize>> = vec![Vec::new(); n_units];
    for (cells, value) in facts_raw {
        let mut slots = Vec::new();
        for &idx in cells {
            match pin_of[idx] {
                Some(p) => slots.push(WtSlot::Pin(p)),
                None => {
                    let u = unit_of_cell[free_index[&idx] as usize];
                    slots.push(WtSlot::Unit(u));
                    if !facts_of_unit[u as usize].contains(&(facts.len())) {
                        facts_of_unit[u as usize].push(facts.len());
                    }
                }
            }
        }
        facts.push(WtUnitFact {
            slots,
            value: *value,
        });
    }
    (facts, facts_of_unit)
}

pub(crate) fn solve_range_partition(
    puzzle: &Puzzle,
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let has = |t: &str| puzzle.rules.iter().any(|r| r.ctype == t);
    // `RANGE_FORCE=1` diagnostic override: probe the number-clue machinery
    // on puzzles outside the rule gate (0171/0308/0742: area + differentiation
    // / hom-hetero without range/precise/inequality/difference).
    let force = std::env::var("RANGE_FORCE").map(|v| v == "1").unwrap_or(false);
    if !force && !(has("range") || has("precise") || has("inequality") || has("difference")) {
        return None;
    }
    // `RANGE_SLICE_MS` diagnostic override (default 12_000).
    let slice_ms: u64 = std::env::var("RANGE_SLICE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12_000);
    solve_range_core(puzzle, n, deadline, slice_ms)
}

/// Sum-locked number pre-pass (0308/1138 class): `check_area` pins each
/// number cell's region size, and either sum identity (all-sum or
/// distinct-sum — see `inject_number_clues`) locks the region count.  Runs
/// the range-part FreeRem pipeline with a short slice (the true case
/// collapses in ms when the lock is the key, as 0262/0929 probes showed).
pub(crate) fn solve_number_pin(
    puzzle: &Puzzle,
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    if !number_sum_lock(puzzle, puzzle.width, n) {
        return None;
    }
    // All-sum form (every clue owns its own region): seeded exact-size cover
    // first — FreeRem's emergent-label DFS wanders on the many-small-region
    // shape (0308: fourteen size-5 islands + one residue).  The cover gets a
    // bounded slice so the FreeRem fallback keeps its budget (0262 solves in
    // ~160 ms there — a runaway cover must not starve it).
    let cover_deadline = {
        // Cover slice: 30 s by default (the FreeRem fallback keeps its own
        // budget; a runaway cover must not starve it).  `COVER_TIME_CAP_MS`
        // raises/lowers the slice for research runs.
        let cap: u64 = std::env::var("COVER_TIME_CAP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30_000);
        let t = crate::clock::Instant::now() + std::time::Duration::from_millis(cap);
        if t < deadline {
            t
        } else {
            deadline
        }
    };
    if let Some(regions) = solve_number_cover(puzzle, puzzle.width, n, cover_deadline) {
        return Some(regions);
    }
    solve_range_core(puzzle, n, deadline, 3_000)
}

/// Wall between two 4-adjacent cells (pre-drawn boundary ⇒ must-differ).
fn wall_between(puzzle: &Puzzle, w: usize, a: usize, b: usize) -> bool {
    let (r1, c1) = (a / w, a % w);
    let (r2, c2) = (b / w, b % w);
    if r1 == r2 {
        let cmin = if c1 < c2 { c1 } else { c2 };
        puzzle.h_edges[r1][cmin].is_boundary
    } else {
        let rmin = if r1 < r2 { r1 } else { r2 };
        puzzle.v_edges[rmin][c1].is_boundary
    }
}

/// Number-seed collection: (cell, size) for every clue cell; the caller
/// checks the all-sum identity.
fn collect_number_seeds(puzzle: &Puzzle, w: usize, n: usize) -> (Vec<(usize, usize)>, usize, usize) {
    let mut seeds: Vec<(usize, usize)> = Vec::new();
    let mut all_sum = 0usize;
    let mut free = 0usize;
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if puzzle.cells[r][c].blocked {
            continue;
        }
        free += 1;
        if let Some(v) = puzzle.cells[r][c].number {
            let v = v.max(1) as usize;
            seeds.push((idx, v));
            all_sum += v;
        }
    }
    (seeds, all_sum, free)
}

/// Bitset cell mask (3×u64 covers up to 192 cells) for cover candidates.
type Mask = [u64; 3];

fn mask_of(cells: &[usize], n: usize) -> Mask {
    let mut m = [0u64; 3];
    for &x in cells {
        let _ = n;
        m[x / 64] |= 1u64 << (x % 64);
    }
    m
}

/// Stamp placed (region, mask) pairs into a region_of vector.
fn mask_assign(placed: &[(usize, Mask)], region_of: &mut [Option<usize>], n: usize) {
    for (ri, pm) in placed.iter() {
        for x in 0..n {
            if pm[x / 64] & (1u64 << (x % 64)) != 0 {
                region_of[x] = Some(*ri);
            }
        }
    }
}

fn mask_and(a: &Mask, b: &Mask) -> bool {
    (a[0] & b[0]) != 0 || (a[1] & b[1]) != 0 || (a[2] & b[2]) != 0
}

/// Cover-search depth histogram (COVER_DEBUG only, zeroed per solve call via
/// `cover_depth_hist_reset`).  Where the tree grows tells value-order work
/// which levels to target.
thread_local! {
    static COVER_DEPTH_HIST: std::cell::RefCell<[usize; 32]> =
        std::cell::RefCell::new([0; 32]);
    /// COVER_TRUTH diagnostic: official assignment's region masks, so the
    /// value-order can report where the truth candidate ranks.
    static COVER_TRUTH_MASKS: std::cell::RefCell<Vec<Mask>> =
        std::cell::RefCell::new(Vec::new());
}

fn cover_depth_hist_reset() {
    COVER_DEPTH_HIST.with(|h| *h.borrow_mut() = [0; 32]);
}

fn cover_depth_hist_snapshot() -> [usize; 32] {
    COVER_DEPTH_HIST.with(|h| *h.borrow())
}

/// One-cell dilation (4-neighbourhood) of a cell mask.
fn dilate(m: &Mask, w: usize, h: usize, n: usize) -> Mask {
    let mut out = [0u64; 3];
    for x in 0..n {
        if m[x / 64] & (1u64 << (x % 64)) == 0 {
            continue;
        }
        let (r, c) = (x / w, x % w);
        for (nr, nc) in [
            (r as i64 - 1, c as i64),
            (r as i64 + 1, c as i64),
            (r as i64, c as i64 - 1),
            (r as i64, c as i64 + 1),
        ] {
            if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                continue;
            }
            let y = (nr as usize) * w + nc as usize;
            out[y / 64] |= 1u64 << (y % 64);
        }
    }
    out
}

/// Candidate = cells + placement mask + dilated touch mask.
#[derive(Clone)]
struct CoverCand {
    cells: Vec<usize>,
    mask: Mask,
    touch: Mask,
}

/// Reorder `list` in place so position `i` gets the item formerly at `perm[i]`.
fn reorder(list: &mut Vec<CoverCand>, perm: &[usize]) {
    let mut out: Vec<CoverCand> = Vec::with_capacity(list.len());
    for &ci in perm {
        out.push(CoverCand {
            cells: std::mem::take(&mut list[ci].cells),
            mask: list[ci].mask,
            touch: list[ci].touch,
        });
    }
    *list = out;
}

/// Candidate compatibility: disjoint from placed, and (under
/// `differentiation`) non-touching equal-size placed regions.  Mask-based
/// (the per-cell `contains` version capped node rate at ~3k/s).
fn cand_fits(
    seeds: &[(usize, usize)],
    order: &[usize],
    placed: &[(usize, Mask)],
    diff_mode: bool,
    k: usize,
    cand: &CoverCand,
) -> bool {
    let my = seeds[order[k]].1;
    for (ri, pm) in placed.iter() {
        if mask_and(&cand.mask, pm) {
            return false;
        }
        if diff_mode && seeds[*ri].1 == my && mask_and(&cand.touch, pm) {
            return false;
        }
    }
    true
}

/// Residue potential-connectivity after adding `cand` (BFS — called only
/// for candidates that survive the mask checks).
/// Unreachable-pocket exact-fill check (`res_size > 0` only).
///
/// The residue must be one connected region containing `res_cell`, so a free
/// cell that cannot reach `res_cell` (through free cells, never crossing
/// pre-drawn walls) can never belong to the residue: every such pocket must be
/// filled *exactly* by the remaining islands seeded inside it.  A pocket whose
/// cell count differs from the sum of its remaining seeds' sizes is a dead
/// branch — one BFS per node, sound, and the structural prune the 0308 cover
/// search was missing (2.5M nodes without converging: the tree only died at
/// the leaf residue check, long after every mis-sized pocket was sealed).

/// COVER_TRUTH + COVER_DEBUG diagnostic (extracted from solve_number_cover
/// for complexity): walk the official assignment one placement at a time,
/// mirroring the search MRV + probe scoring, and report the truth
/// candidate rank at each level.
#[allow(clippy::too_many_arguments)]

/// COVER_ORDER static value ordering (extracted from solve_number_cover for
/// complexity): `truth` (diagnostic, COVER_TRUTH first), `rare`/`mcv`/`wall`
/// heuristic orders.  Generation order for anything else.
#[allow(clippy::too_many_arguments)]

/// COVER_ORDER=truth (diagnostic): move the COVER_TRUTH official candidates
/// to the front of each region list so a sound search solves in ~15 nodes.
fn cover_order_truth_first(w: usize, h: usize, n: usize, order: &[usize], seeds: &[(usize, usize)], cands: &mut Vec<Vec<CoverCand>>) {
    let _ = h;

            // Diagnostic: force the COVER_TRUTH official assignment to the
            // front of each region's candidate list.  A sound search + complete
            // candidates must then solve in milliseconds; a still-failing
            // search means a prune is unsound (kills the truth path).
            if let Ok(truth_path) = std::env::var("COVER_TRUTH") {
                if let Ok(text) = std::fs::read_to_string(&truth_path) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(arr) = v.get("regions").and_then(|r| r.as_array()) {
                            let mut truth_masks: Vec<Mask> = Vec::new();
                            for reg in arr {
                                let mut cells: Vec<usize> = Vec::new();
                                if let Some(list) = reg.as_array() {
                                    for cell in list {
                                        if let Some(pair) = cell.as_array() {
                                            let r = pair.first().and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                                            let c = pair.get(1).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                                            cells.push(r * w + c);
                                        }
                                    }
                                }
                                truth_masks.push(mask_of(&cells, n));
                            }
                            for (j, &k) in order.iter().enumerate() {
                                let (cell, _size) = seeds[k];
                                if let Some(ti) = truth_masks
                                    .iter()
                                    .position(|m| m[cell / 64] & (1u64 << (cell % 64)) != 0)
                                {
                                    let pos = cands[j].iter().position(|c| c.mask == truth_masks[ti]);
                                    if let Some(pos) = pos {
                                        let cand = cands[j].remove(pos);
                                        cands[j].insert(0, cand);
                                    } else {
                                        eprintln!("cover-order-truth: seed {k} truth cand MISSING");
                                    }
                                }
                            }
                        }
                    }
                }
            }
}

fn apply_cover_static_order(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    order: &[usize],
    cands: &mut Vec<Vec<CoverCand>>,
    seeds: &[(usize, usize)],
) {
    let _ = (h, seeds);
    if let Ok(mode) = std::env::var("COVER_ORDER") {
        if mode == "truth" {
            cover_order_truth_first(w, h, n, order, seeds, cands);
        } else if mode == "rare" || mode == "mcv" || mode == "wall" {
            cover_order_heuristic(&mode, puzzle, w, h, n, cands);
            if std::env::var("COVER_DEBUG").is_ok() {
                eprintln!("cover: ordered by {mode}");
            }
        }
    }
    cover_truth_membership_report(puzzle, w, h, n, order, cands, seeds);
}


/// COVER_ORDER=probe / probe-lcv dynamic value ordering (extracted from
/// `cover_dfs` for complexity): score every viable candidate by the surviving
/// candidate mass of the child state (post-MAC + pocket), then order by it
/// (`probe` = MCV, `probe-lcv` = LCV).  Dead children never recurse at all.
#[allow(clippy::too_many_arguments)]
fn probe_order_cover_candidates(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    k: usize,
    remaining: &[usize],
    order: &[usize],
    cands: &[Vec<CoverCand>],
    seeds: &[(usize, usize)],
    viable: &[usize],
    active: &[Vec<bool>],
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
    placed: &[(usize, Mask)],
    deadline: crate::clock::Instant,
) -> Vec<usize> {
    if !std::env::var("COVER_ORDER")
        .map(|v| v == "probe" || v == "probe-lcv")
        .unwrap_or(false)
    {
        return viable.to_vec();
    }
    // `probe` = dynamic MCV (lowest surviving mass first);
    // `probe-lcv` = dynamic LCV (highest mass first).
    let lcv = std::env::var("COVER_ORDER")
        .map(|v| v == "probe-lcv")
        .unwrap_or(false);
    let mut scored: Vec<(u64, usize)> = Vec::with_capacity(viable.len());
    for &ci in viable {
        if crate::clock::Instant::now() >= deadline {
            break;
        }
        let mut rem2: Vec<usize> = remaining.to_vec();
        rem2.retain(|&x| x != k);
        let mut placed2 = placed.to_vec();
        placed2.push((order[k], cands[k][ci].mask));
        let mut active2 = active.to_vec();
        active2[k][ci] = true;
        if mac_propagate(
            w, h, n, puzzle, &mut rem2, order, cands, seeds, &mut active2, &mut placed2,
            res_cell, res_size, diff_mode,
        )
        .is_none()
            || !pocket_ok(puzzle, w, h, n, &placed2, &rem2, order, seeds, res_cell, res_size)
        {
            continue; // dead child — skip entirely
        }
        let mass: u64 = rem2
            .iter()
            .map(|&x| (0..cands[x].len()).filter(|&c| active2[x][c]).count() as u64)
            .sum();
        scored.push((mass, ci));
    }
    if lcv {
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    } else {
        scored.sort_unstable();
    }
    scored.into_iter().map(|(_, ci)| ci).collect()
}

/// Count candidates of *other* region lists whose mask overlaps `mask`.
fn count_mask_conflicts(k: usize, mask: &Mask, cands: &[Vec<CoverCand>]) -> i64 {
    let mut conflicts = 0i64;
    for (k2, list2) in cands.iter().enumerate() {
        if k2 == k {
            continue;
        }
        for c2 in list2 {
            if mask_and(mask, &c2.mask) {
                conflicts += 1;
            }
        }
    }
    conflicts
}

/// COVER_ORDER=rare/mcv/wall static heuristic orderings.
fn cover_order_heuristic(
    mode: &str,
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    cands: &mut Vec<Vec<CoverCand>>,
) {
    if mode == "rare" {
        let mut owners = vec![0u32; n];
        for list in cands.iter() {
            for c in list {
                for &x in &c.cells {
                    owners[x] += 1;
                }
            }
        }
        let mut keyed: Vec<(u64, usize)> = Vec::new();
        for list in cands.iter_mut() {
            keyed.clear();
            for (ci, c) in list.iter().enumerate() {
                let s: u64 = c.cells.iter().map(|&x| owners[x] as u64).sum();
                keyed.push((s, ci));
            }
            keyed.sort_by_key(|&(s, ci)| (s, ci));
            let perm: Vec<usize> = keyed.iter().map(|&(_, ci)| ci).collect();
            reorder(list, &perm);
        }
    } else if mode == "mcv" {
        let mut keyed: Vec<(i64, usize)> = Vec::new();
        for k in 0..cands.len() {
            keyed.clear();
            for (ci, c) in cands[k].iter().enumerate() {
                keyed.push((-count_mask_conflicts(k, &c.mask, cands), ci));
            }
            keyed.sort_by_key(|&t| t);
            let perm: Vec<usize> = keyed.iter().map(|&(_, ci)| ci).collect();
            reorder(&mut cands[k], &perm);
        }
    } else if mode == "wall" {
        let mut keyed: Vec<(i64, usize)> = Vec::new();
        for list in cands.iter_mut() {
            keyed.clear();
            for (ci, c) in list.iter().enumerate() {
                let mut walls = 0i64;
                for &x in &c.cells {
                    let (r, col) = (x / w, x % w);
                    if r > 0 && puzzle.v_edges[r - 1][col].is_boundary {
                        walls += 1;
                    }
                    if r + 1 < h && puzzle.v_edges[r][col].is_boundary {
                        walls += 1;
                    }
                    if col > 0 && puzzle.h_edges[r][col - 1].is_boundary {
                        walls += 1;
                    }
                    if col + 1 < w && puzzle.h_edges[r][col].is_boundary {
                        walls += 1;
                    }
                }
                keyed.push((-walls, ci));
            }
            keyed.sort_by_key(|&t| t);
            let perm: Vec<usize> = keyed.iter().map(|&(_, ci)| ci).collect();
            reorder(list, &perm);
        }
    }
}

/// COVER_TRUTH membership + heuristic-rank report for the number cover.
/// Missing members mean the candidate generator is incomplete (fix
/// generation); complete membership with a failing search is an ordering
/// problem (fix value order).  Also stashes the truth masks for
/// `cover_truth_rank_walk`.
fn cover_truth_membership_report(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    order: &[usize],
    cands: &[Vec<CoverCand>],
    seeds: &[(usize, usize)],
) {
    let Ok(truth_path) = std::env::var("COVER_TRUTH") else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&truth_path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(arr) = v.get("regions").and_then(|r| r.as_array()) else {
        return;
    };
    let mut truth_masks: Vec<Mask> = Vec::new();
    for reg in arr {
        let mut cells: Vec<usize> = Vec::new();
        if let Some(list) = reg.as_array() {
            for cell in list {
                if let Some(pair) = cell.as_array() {
                    let r = pair.first().and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                    let c = pair.get(1).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                    cells.push(r * w + c);
                }
            }
        }
        truth_masks.push(mask_of(&cells, n));
    }
    COVER_TRUTH_MASKS.with(|m| *m.borrow_mut() = truth_masks.clone());
    for (j, &k) in order.iter().enumerate() {
        let (cell, _size) = seeds[k];
        let hit = truth_masks
            .iter()
            .position(|m| m[cell / 64] & (1u64 << (cell % 64)) != 0);
        match hit {
            None => eprintln!("cover-truth: seed {k} cell {cell}: no truth region"),
            Some(ti) => {
                let found = cands[j].iter().position(|c| c.mask == truth_masks[ti]);
                let tpos = found.unwrap_or(usize::MAX);
                let wall_of = |c: &CoverCand| -> i64 {
                    let mut walls = 0i64;
                    for &x in &c.cells {
                        let (r, col) = (x / w, x % w);
                        if r > 0 && puzzle.v_edges[r - 1][col].is_boundary {
                            walls += 1;
                        }
                        if r + 1 < h && puzzle.v_edges[r][col].is_boundary {
                            walls += 1;
                        }
                        if col > 0 && puzzle.h_edges[r][col - 1].is_boundary {
                            walls += 1;
                        }
                        if col + 1 < w && puzzle.h_edges[r][col].is_boundary {
                            walls += 1;
                        }
                    }
                    walls
                };
                let mut owners = vec![0u32; n];
                for list in cands.iter() {
                    for c in list {
                        for &x in &c.cells {
                            owners[x] += 1;
                        }
                    }
                }
                let rare_of = |c: &CoverCand| -> u64 {
                    c.cells.iter().map(|&x| owners[x] as u64).sum()
                };
                if found.is_some() {
                    let tc = &cands[j][tpos];
                    let wv = wall_of(tc);
                    let rv = rare_of(tc);
                    let w_rank = cands[j].iter().filter(|c| wall_of(c) > wv).count();
                    let r_rank = cands[j].iter().filter(|c| rare_of(c) < rv).count();
                    eprintln!(
                        "cover-truth: seed {k} cell {cell}: idx={tpos}/{} wall={wv} (rank {w_rank}) rare={rv} (rank {r_rank})",
                        cands[j].len()
                    );
                } else {
                    eprintln!("cover-truth: seed {k} cell {cell}: MISSING from cands");
                }
            }
        }
    }
}

fn cover_truth_rank_walk(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    order: &[usize],
    cands: &[Vec<CoverCand>],
    seeds: &[(usize, usize)],
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
) {
    // COVER_TRUTH + COVER_DEBUG: walk the official assignment one placement at
    // a time, mirroring the search's MRV + probe scoring, and report the truth
    // candidate's rank at each level.  Answers "is the mass heuristic ranking
    // the truth first?" without drowning in per-node prints.
    if std::env::var("COVER_TRUTH").is_ok() && std::env::var("COVER_DEBUG").is_ok() {
        let tms = COVER_TRUTH_MASKS.with(|m| m.borrow().clone());
        if !tms.is_empty() {
            let mut placed_t: Vec<(usize, Mask)> = Vec::new();
            let mut active_t: Vec<Vec<bool>> = cands.iter().map(|v| vec![true; v.len()]).collect();
            let mut rem_t: Vec<usize> = (0..order.len()).collect();
            while !rem_t.is_empty() {
                // Same MRV as the search.
                let mut best: Option<(usize, usize)> = None;
                for &j in rem_t.iter() {
                    let cnt = (0..cands[j].len()).filter(|&ci| active_t[j][ci]).count();
                    if best.map(|(_, b)| cnt < b).unwrap_or(true) {
                        best = Some((j, cnt));
                    }
                }
                let (k, _) = best.unwrap();
                let (kcell, _) = seeds[order[k]];
                let ti = tms
                    .iter()
                    .position(|m| m[kcell / 64] & (1u64 << (kcell % 64)) != 0);
                // Score siblings exactly like the probe block.
                let mut scored: Vec<(u64, usize)> = Vec::new();
                for ci in 0..cands[k].len() {
                    if !active_t[k][ci] {
                        continue;
                    }
                    if !compat(&seeds, &order, &placed_t, diff_mode, k, &cands[k][ci]) {
                        continue;
                    }
                    if !res_roomy(puzzle, w, h, n, &placed_t, res_cell, res_size, &cands[k][ci]) {
                        continue;
                    }
                    let mut rem2 = rem_t.clone();
                    rem2.retain(|&x| x != k);
                    let mut placed2 = placed_t.clone();
                    placed2.push((order[k], cands[k][ci].mask));
                    let mut active2 = active_t.clone();
                    active2[k][ci] = true;
                    if mac_propagate(
                        w, h, n, puzzle, &mut rem2, &order, &cands, &seeds, &mut active2,
                        &mut placed2, res_cell, res_size, diff_mode,
                    )
                    .is_none()
                        || !pocket_ok(puzzle, w, h, n, &placed2, &rem2, &order, &seeds, res_cell, res_size)
                    {
                        continue;
                    }
                    let mass: u64 = rem2
                        .iter()
                        .map(|&x| (0..cands[x].len()).filter(|&c| active2[x][c]).count() as u64)
                        .sum();
                    scored.push((mass, ci));
                }
                scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1))); // LCV
                let tmask = ti.map(|t| tms[t]);
                let rank = tmask.and_then(|tm| {
                    scored.iter().position(|&(_, ci)| cands[k][ci].mask == tm)
                });
                eprintln!(
                    "cover-rank: depth={} region={} truth-rank={:?}/{} best-mass={:?} truth-mass={:?}",
                    order.len() - rem_t.len(),
                    k,
                    rank,
                    scored.len(),
                    scored.first().map(|&(m, _)| m),
                    rank.map(|r| scored[r].0),
                );
                if rem_t.len() >= 12 {
                    for (ri, &(_, ci)) in scored.iter().take(3).enumerate() {
                        let cells: Vec<(usize, usize)> = cands[k][ci]
                            .cells
                            .iter()
                            .map(|&x| (x / w, x % w))
                            .collect();
                        eprintln!("cover-rank:   sibling{ri} cells={cells:?}");
                    }
                    if let Some(ti) = ti {
                        let cells: Vec<(usize, usize)> = (0..n)
                            .filter(|&x| tms[ti][x / 64] & (1u64 << (x % 64)) != 0)
                            .map(|x| (x / w, x % w))
                            .collect();
                        eprintln!("cover-rank:   TRUTH cells={cells:?}");
                    }
                }
                // Advance along the truth path.
                let Some(ti) = ti else { break };
                let Some(pos) = scored.iter().position(|&(_, ci)| cands[k][ci].mask == tms[ti])
                else {
                    break;
                };
                let ci = scored[pos].1;
                placed_t.push((order[k], cands[k][ci].mask));
                rem_t.retain(|&x| x != k);
                for c in 0..cands[k].len() {
                    if c != ci && !compat(&seeds, &order, &placed_t, diff_mode, k, &cands[k][c]) {
                        active_t[k][c] = false;
                    }
                }
                let mut active2 = active_t.clone();
                if mac_propagate(
                    w, h, n, puzzle, &mut rem_t, &order, &cands, &seeds, &mut active2,
                    &mut placed_t, res_cell, res_size, diff_mode,
                )
                .is_none()
                {
                    eprintln!("cover-rank: truth path DIED in mac at depth {}", placed_t.len());
                    break;
                }
                active_t = active2;
            }
        }
    }
}

fn pocket_ok(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    placed: &[(usize, Mask)],
    remaining: &[usize],
    order: &[usize],
    seeds: &[(usize, usize)],
    res_cell: usize,
    res_size: usize,
) -> bool {
    if res_size == 0 {
        return true;
    }
    let mut used = [0u64; 3];
    for (_, pm) in placed.iter() {
        used[0] |= pm[0];
        used[1] |= pm[1];
        used[2] |= pm[2];
    }
    if used[res_cell / 64] & (1u64 << (res_cell % 64)) != 0 {
        return false;
    }
    let free = |x: usize| -> bool {
        used[x / 64] & (1u64 << (x % 64)) == 0 && !puzzle.cells[x / w][x % w].blocked
    };
    let neigh = |x: usize, out: &mut Vec<usize>| {
        out.clear();
        let (r, c) = (x / w, x % w);
        for (nr, nc) in [
            (r as i64 - 1, c as i64),
            (r as i64 + 1, c as i64),
            (r as i64, c as i64 - 1),
            (r as i64, c as i64 + 1),
        ] {
            if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                continue;
            }
            let y = (nr as usize) * w + nc as usize;
            if free(y) && !wall_between(puzzle, w, x, y) {
                out.push(y);
            }
        }
    };
    // label: 0 unvisited-free, 1 reachable-from-res_cell, 2 pocket
    let mut lab = vec![0u8; n];
    let mut nb = Vec::with_capacity(4);
    let mut stack = vec![res_cell];
    lab[res_cell] = 1;
    let mut r_cells = 0usize;
    let mut r_need = 0usize;
    while let Some(x) = stack.pop() {
        r_cells += 1;
        for &j in remaining {
            let (sc, sz) = seeds[order[j]];
            if sc == x {
                r_need += sz;
            }
        }
        neigh(x, &mut nb);
        for &y in &nb {
            if lab[y] == 0 {
                lab[y] = 1;
                stack.push(y);
            }
        }
    }
    // Exact account over R (the `res_cell` component): every R cell ends up
    // either in the residue (`res_size` cells) or in a still-unplaced island
    // seeded inside R (`r_need` cells), and island cells are chosen from free
    // cells only — so `|R| == res_size + r_need` must hold at *every* node of
    // a solution prefix, not just at the leaf.  This is the sharp mid-search
    // prune: the 0308 tree dies at depth 7-8 on `|R|` overshoot long before
    // the leaf residue check.
    if r_cells != res_size + r_need {
        return false;
    }
    for x in 0..n {
        if lab[x] != 0 || !free(x) {
            continue;
        }
        // New pocket: count cells and the island area its own seeds demand.
        let mut cells = 0usize;
        let mut need = 0usize;
        lab[x] = 2;
        let mut stack2 = vec![x];
        while let Some(y) = stack2.pop() {
            cells += 1;
            for &j in remaining {
                let (sc, sz) = seeds[order[j]];
                if sc == y {
                    need += sz;
                }
            }
            neigh(y, &mut nb);
            for &z in &nb {
                if lab[z] == 0 {
                    lab[z] = 2;
                    stack2.push(z);
                }
            }
        }
        if cells != need {
            return false;
        }
    }
    true
}

fn res_roomy(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    placed: &[(usize, Mask)],
    res_cell: usize,
    res_size: usize,
    cand: &CoverCand,
) -> bool {
    let _ = n;
    if res_size == 0 {
        // full-cover mode: no residue region to protect
        return true;
    }
    let mut used = [0u64; 3];
    for (_, pm) in placed.iter() {
        used[0] |= pm[0];
        used[1] |= pm[1];
        used[2] |= pm[2];
    }
    used[0] |= cand.mask[0];
    used[1] |= cand.mask[1];
    used[2] |= cand.mask[2];
    if used[res_cell / 64] & (1u64 << (res_cell % 64)) != 0 {
        return false;
    }
    let mut seen = [0u64; 3];
    let mut stack = vec![res_cell];
    seen[res_cell / 64] |= 1u64 << (res_cell % 64);
    let mut cnt = 0usize;
    while let Some(x) = stack.pop() {
        cnt += 1;
        if cnt >= res_size {
            return true;
        }
        let (r, c) = (x / w, x % w);
        for (nr, nc) in [
            (r as i64 - 1, c as i64),
            (r as i64 + 1, c as i64),
            (r as i64, c as i64 - 1),
            (r as i64, c as i64 + 1),
        ] {
            if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                continue;
            }
            let y = (nr as usize) * w + nc as usize;
            if seen[y / 64] & (1u64 << (y % 64)) != 0 {
                continue;
            }
            if used[y / 64] & (1u64 << (y % 64)) != 0 {
                continue;
            }
            if puzzle.cells[nr as usize][nc as usize].blocked {
                continue;
            }
            if wall_between(puzzle, w, x, y) {
                continue;
            }
            seen[y / 64] |= 1u64 << (y % 64);
            stack.push(y);
        }
    }
    cnt >= res_size
}

/// Connected exact-size candidates for one seed (no other seeds, no wall
/// crossings); mask + dilated touch mask stored up front.

/// Rose regions hold exactly one cell of each symbol type.
fn rose_exactly_one_each(
    cells: &[usize],
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
) -> bool {
    let mut counts = vec![0usize; n_types];
    for &x in cells {
        if let Some(&t) = sym_of.get(&x) {
            counts[t] += 1;
            if counts[t] > 1 {
                return false;
            }
        }
    }
    counts.iter().all(|&c| c == 1)
}

/// Early prune: can this partial set still reach a complete rose region?
fn rose_can_complete(
    cur: &[usize],
    size: usize,
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
) -> bool {
    let remaining = size - cur.len();
    let mut type_seen = vec![false; n_types];
    let mut types_present = 0u32;
    for &x in cur {
        if let Some(&t) = sym_of.get(&x) {
            if !type_seen[t] {
                type_seen[t] = true;
                types_present += 1;
            }
        }
    }
    let missing = n_types as u32 - types_present;
    remaining >= missing as usize
}

/// Sort frontier cells so missing-type symbol cells are pushed last (LIFO).
fn prioritize_missing_symbols(
    mut frontier: Vec<usize>,
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
    cur: &[usize],
) -> Vec<usize> {
    // Deterministic hash-based interleaving for DFS diversity: different
    // paths from the seed explore different candidate shapes.
    let hash = |y: usize| y.wrapping_mul(2654435761) & 0xffff;
    if let Some((sym_of, n_types)) = sym {
        let mut type_seen = vec![false; n_types];
        for &x in cur {
            if let Some(&t) = sym_of.get(&x) {
                type_seen[t] = true;
            }
        }
        frontier.sort_by_key(|&y| {
            let pri = match sym_of.get(&y) {
                Some(&t) if !type_seen[t] => 0u8,
                _ => 1u8,
            };
            (pri, hash(y))
        });
    } else {
        frontier.sort_by_key(|&y| hash(y));
    }
    frontier
}
/// Accept a complete cover candidate: symbol check + non_block rectangle filter.
fn cover_cand_acceptable(
    cur: &[usize],
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
    non_block: bool,
    w: usize,
) -> bool {
    if let Some((sym_of, n_types)) = sym {
        if !rose_exactly_one_each(cur, sym_of, n_types) {
            return false;
        }
    }
    if non_block {
        let cells: Vec<[usize; 2]> = cur.iter().map(|&x| [x / w, x % w]).collect();
        if crate::shapes::is_rectangle(&cells) {
            return false;
        }
    }
    true
}

fn gen_cover_cands(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    cell: usize,
    size: usize,
    seed_cell: &std::collections::HashSet<usize>,
    deadline: crate::clock::Instant,
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
) -> Vec<CoverCand> {
    // Large regions (size > 10): fewer candidates per list — the per-attempt
    // cover DFS is O(cap^m) and 4000^4 is intractable.  Small regions keep
    // the full cap (0289 class needs the breadth).
    let cand_cap: usize = if size > 10 { 500 } else { 4_000 };
    // non_block: no region may be a rectangle — filter at generation time.
    let non_block = puzzle.rules.iter().any(|r| r.ctype == "non_block");
    let mut out: Vec<CoverCand> = Vec::new();
    let mut seen: std::collections::HashSet<Vec<usize>> = Default::default();
    let mut stack: Vec<Vec<usize>> = vec![vec![cell]];
    while let Some(cur) = stack.pop() {
        if crate::clock::Instant::now() >= deadline || out.len() >= cand_cap {
            break;
        }
        if cur.len() == size {
            if !cover_cand_acceptable(&cur, sym, non_block, w) {
                continue;
            }
            let mask = mask_of(&cur, n);
            let touch = dilate(&mask, w, h, n);
            out.push(CoverCand {
                cells: cur,
                mask,
                touch,
            });
            continue;
        }
        // Early prune: if remaining cells < missing symbol types, this branch
        // can never produce a complete rose region (exactly one of each type).
        if let Some((sym_of, n_types)) = sym {
            if !rose_can_complete(&cur, size, sym_of, n_types) {
                continue;
            }
        }
        let mut frontier: std::collections::BTreeSet<usize> = Default::default();
        for &x in &cur {
            let (r, c) = (x / w, x % w);
            for (nr, nc) in [
                (r as i64 - 1, c as i64),
                (r as i64 + 1, c as i64),
                (r as i64, c as i64 - 1),
                (r as i64, c as i64 + 1),
            ] {
                if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                    continue;
                }
                let y = (nr as usize) * w + nc as usize;
                if puzzle.cells[nr as usize][nc as usize].blocked || cur.contains(&y) {
                    continue;
                }
                if y != cell && seed_cell.contains(&y) {
                    continue;
                }
                if let Some((sym_of, _)) = sym {
                    // Rose: at most one cell per symbol type in a region.
                    if let Some(&t) = sym_of.get(&y) {
                        if cur.iter().any(|&x| sym_of.get(&x) == Some(&t)) {
                            continue;
                        }
                    }
                }
                if wall_between(puzzle, w, x, y) {
                    continue;
                }
                frontier.insert(y);
            }
        }
        // Push missing-type symbol cells last (stack is LIFO → explored first).
        let frontier_vec = prioritize_missing_symbols(frontier.into_iter().collect(), sym, &cur);
        for y in frontier_vec {
            let mut nxt = cur.clone();
            nxt.push(y);
            nxt.sort_unstable();
            if seen.insert(nxt.clone()) {
                stack.push(nxt);
            }
        }
    }
    out
}

/// All-sum number cover (0308 class): every number cell owns its own region
/// of exact size (see `inject_number_clues`), sizes sum to the board.  Pick
/// the largest region as residue, enumerate connected placements for the
/// rest (never eating other seeds, never crossing pre-drawn walls), and
/// check the leftover is the residue region.  `differentiation` forbids
/// equal-size regions from touching.  False-negative only — every accepted
/// answer passes the full validator.  The placement search is MAC (arc-
/// consistency cascade) over candidate sets; the guided-walk diagnostic
/// (`tmp_diag_0308_guided`) proves the pruning never rejects the official
/// placement.
fn solve_number_cover(
    puzzle: &Puzzle,
    w: usize,
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let h = puzzle.height;
    let (seeds, all_sum, free) = collect_number_seeds(puzzle, w, n);
    if seeds.is_empty() || all_sum != free {
        return None;
    }
    let diff_mode = puzzle
        .rules
        .iter()
        .any(|r| r.ctype == "differentiation");
    // Only the small-island shape: large non-residue regions generate
    // candidate explosions (0262/1183) where FreeRem's fallback is better.
    let max_size = seeds.iter().map(|&(_, v)| v).max().unwrap_or(0);
    if seeds
        .iter()
        .any(|&(_, v)| v != max_size && v > 8)
    {
        return None;
    }
    // Residue = largest region (its placement is the leftover by construction).
    let res_i = seeds
        .iter()
        .enumerate()
        .max_by_key(|(_, &(_, v))| v)
        .map(|(i, _)| i)
        .unwrap();
    let (res_cell, res_size) = seeds[res_i];
    let seed_cell: std::collections::HashSet<usize> = seeds.iter().map(|&(c, _)| c).collect();
    let mut cands: Vec<Vec<CoverCand>> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for (i, &(cell, size)) in seeds.iter().enumerate() {
        if i == res_i {
            continue;
        }
        order.push(i);
        let out = gen_cover_cands(puzzle, w, h, n, cell, size, &seed_cell, deadline, None);
        if std::env::var("COVER_DEBUG").is_ok() {
            eprintln!("cover: region {i} size {size} cands {}", out.len());
        }
        if out.is_empty() {
            return None;
        }
        cands.push(out);
    }
    let mut remaining: Vec<usize> = (0..order.len()).collect();
    let mut placed: Vec<(usize, Mask)> = Vec::new();
    let mut active: Vec<Vec<bool>> = cands.iter().map(|v| vec![true; v.len()]).collect();
    // Value ordering (doc 07 修订11 / 0308 开项: "值序待优化").  The candidate
    // generator is complete for the official assignment (COVER_TRUTH confirms
    // every truth region is in its seed's list) — the search just walks the
    // 200-candidate lists in generation order and the truth sits deep.
    // `COVER_ORDER` picks a static value order:
    //   * `rare` — ascending Σ owners(cell): prefer placements that consume
    //     "prize" cells few other candidates can cover (fail-first on cells);
    //   * `mcv`  — descending count of other regions' candidates the
    //     placement overlaps (most-constraining value first);
    //   * `wall` — descending number of sides against pre-drawn walls (tight
    //     placements first: they cannot shift, loose ones can).
    // Generation order is kept for any other value (and as the safe default).
    apply_cover_static_order(puzzle, w, h, n, &order, &mut cands, &seeds);
    cover_truth_rank_walk(puzzle, w, h, n, &order, &cands, &seeds, res_cell, res_size, diff_mode);
    let mut nodes = 0usize;
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("cover: total cands {:?}", cands.iter().map(|v| v.len()).collect::<Vec<_>>());
    }
    cover_depth_hist_reset();
    if !cover_dfs(
        puzzle,
        w,
        h,
        n,
        &mut remaining,
        &order,
        &cands,
        &seeds,
        res_cell,
        res_size,
        diff_mode,
        &mut placed,
        &mut active,
        &mut nodes,
        deadline,
        None,
        None,
        false,
        std::env::var("COVER_CHILD_CAP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024),
    ) {
        if std::env::var("COVER_DEBUG").is_ok() {
            eprintln!("cover: FAIL nodes={nodes}");
            let hist = cover_depth_hist_snapshot();
            let prof: Vec<(usize, usize)> = hist
                .iter()
                .enumerate()
                .filter(|(_, &c)| c > 0)
                .map(|(d, &c)| (d, c))
                .collect();
            eprintln!("cover: depth-hist {prof:?}");
        }
        return None;
    }
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("cover: SOLVED nodes={nodes}");
    }
    // Build regions from placed masks + residue.
    let mut region_of: Vec<Option<usize>> = vec![None; n];
    for (ri, pm) in placed.iter() {
        for x in 0..n {
            if pm[x / 64] & (1u64 << (x % 64)) != 0 {
                region_of[x] = Some(*ri);
            }
        }
    }
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if puzzle.cells[r][c].blocked {
            continue;
        }
        if region_of[idx].is_none() {
            region_of[idx] = Some(res_i);
        }
    }
    let regions = super::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        Some(regions)
    } else {
        if std::env::var("COVER_DEBUG").is_ok() {
            let sizes: Vec<usize> = regions.iter().map(|r| r.cells.len()).collect();
            eprintln!("cover: leaf REJECTED sizes={sizes:?}");
            for reg in &regions {
                let nums: Vec<(usize, usize, i64)> = reg
                    .cells
                    .iter()
                    .filter_map(|&[r, c]| {
                        puzzle.cells[r][c].number.map(|v| (r, c, v))
                    })
                    .collect();
                eprintln!(
                    "  region id={} len={} numbers={nums:?}",
                    reg.region_id,
                    reg.cells.len()
                );
            }
        }
        None
    }
}

/// Pairwise compatibility under the current placement (disjoint; under
/// `differentiation`, non-touching equal-size placed regions).
fn compat(
    seeds: &[(usize, usize)],
    order: &[usize],
    placed: &[(usize, Mask)],
    diff_mode: bool,
    k: usize,
    cand: &CoverCand,
) -> bool {
    cand_fits(seeds, order, placed, diff_mode, k, cand)
}

/// MAC propagation: deactivate every candidate conflicting with the current
/// placement; singleton domains cascade as implicit placements.  Returns
/// the implied placements (including the seed choice) or None on dead end.
#[allow(clippy::too_many_arguments)]
fn mac_propagate(
    w: usize,
    h: usize,
    n: usize,
    puzzle: &Puzzle,
    remaining: &mut Vec<usize>,
    order: &[usize],
    cands: &[Vec<CoverCand>],
    seeds: &[(usize, usize)],
    active: &mut Vec<Vec<bool>>,
    placed: &mut Vec<(usize, Mask)>,
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
) -> Option<()> {
    let mut implied: Vec<usize> = Vec::new();
    loop {
        let mut again = false;
        for &j in remaining.iter() {
            // A region forced earlier in this pass is already in `placed` —
            // its own candidates now conflict with its placed mask; skipping
            // it is required for completeness (self-conflict node death).
            if placed.iter().any(|&(ri, _)| ri == order[j]) {
                continue;
            }
            let mut cnt = 0usize;
            let mut only: Option<usize> = None;
            for (ci, c) in cands[j].iter().enumerate() {
                if !active[j][ci] {
                    continue;
                }
                if !compat(seeds, order, placed, diff_mode, j, c) {
                    active[j][ci] = false;
                    continue;
                }
                cnt += 1;
                only = Some(ci);
            }
            if cnt == 0 {
                return None;
            }
            if cnt == 1 {
                let ci = only.unwrap();
                if !res_roomy(puzzle, w, h, n, placed, res_cell, res_size, &cands[j][ci]) {
                    active[j][ci] = false;
                    return None;
                }
                placed.push((order[j], cands[j][ci].mask));
                implied.push(j);
                again = true;
            }
        }
        for &j in implied.iter() {
            remaining.retain(|&x| x != j);
        }
        implied.clear();
        if !again {
            break;
        }
    }
    Some(())
}

/// Compact dihedral-normalized shape key for small regions (the `different`
/// distinct-shape constraint pushed into the cover search).
fn shape_key_small(cells: &[usize], w: usize) -> u64 {
    let pts: Vec<(i64, i64)> = cells
        .iter()
        .map(|&x| ((x / w) as i64, (x % w) as i64))
        .collect();
    let mut best: Option<Vec<(i64, i64)>> = None;
    for flip in 0..2 {
        for rot in 0..4 {
            let mut t: Vec<(i64, i64)> = pts
                .iter()
                .map(|&(r, c)| {
                    let (mut r, mut c) = (r, c);
                    if flip == 1 {
                        c = -c;
                    }
                    for _ in 0..rot {
                        let (nr, nc) = (-c, r);
                        r = nr;
                        c = nc;
                    }
                    (r, c)
                })
                .collect();
            let min_r = t.iter().map(|p| p.0).min().unwrap_or(0);
            let min_c = t.iter().map(|p| p.1).min().unwrap_or(0);
            for p in t.iter_mut() {
                p.0 -= min_r;
                p.1 -= min_c;
            }
            t.sort_unstable();
            if best.as_ref().map(|b| t < *b).unwrap_or(true) {
                best = Some(t);
            }
        }
    }
    let mut hsh: u64 = 0xcbf2_9ce4_8422_2325;
    for (r, c) in best.unwrap_or_default() {
        hsh ^= (r as u64 & 0xffff) | ((c as u64 & 0xffff) << 16);
        hsh = hsh.wrapping_mul(0x100_0000_01b3);
    }
    hsh
}

/// Algorithm X (cell-first exact cover) for the all-small full-cover class
/// (0317): each region picks exactly one candidate; the union covers every
/// free cell exactly once.  Branching on the most-constrained *cell* (the
/// classic Knuth ordering) collapses the 252-candidate space in
/// milliseconds where region-first MRV wanders through millions of nodes.
#[allow(clippy::too_many_arguments)]
fn exact_cover_x(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    cands: &[Vec<CoverCand>],
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
    distinct_shapes: bool,
    deadline: crate::clock::Instant,
) -> Option<Vec<(usize, Mask)>> {
    let m = cands.len();
    // Precompute per-cell option lists: cell -> Vec<(region, cand index)>.
    let mut by_cell: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (ri, list) in cands.iter().enumerate() {
        for (ci, cand) in list.iter().enumerate() {
            for x in 0..n {
                if cand.mask[x / 64] & (1u64 << (x % 64)) != 0 {
                    by_cell[x].push((ri, ci));
                }
            }
        }
    }
    let mut free_cells: Vec<usize> = Vec::new();
    for x in 0..n {
        let (r, c) = (x / w, x % w);
        if !puzzle.cells[r][c].blocked {
            free_cells.push(x);
        }
    }
    let mut nodes = 0usize;
    let mut placed: Vec<Option<(usize, Mask)>> = vec![None; m];
    let mut used = [0u64; 3];
    fn search(
        puzzle: &Puzzle,
        w: usize,
        h: usize,
        n: usize,
        cands: &[Vec<CoverCand>],
        by_cell: &[Vec<(usize, usize)>],
        free_cells: &[usize],
        sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
        placed: &mut Vec<Option<(usize, Mask)>>,
        used: &mut [u64; 3],
        nodes: &mut usize,
        distinct_shapes: bool,
        seen_keys: &mut std::collections::HashSet<u64>,
        deadline: crate::clock::Instant,
    ) -> bool {
        if *nodes > 2_000_000 || crate::clock::Instant::now() >= deadline {
            return false;
        }
        *nodes += 1;
        // Most-constrained uncovered free cell.
        let mut best: Option<(usize, Vec<(usize, usize)>)> = None;
        for &x in free_cells {
            if used[x / 64] & (1u64 << (x % 64)) != 0 {
                continue;
            }
            let mut options: Vec<(usize, usize)> = Vec::new();
            for &(ri, ci) in &by_cell[x] {
                if placed[ri].is_some() {
                    continue;
                }
                let cand = &cands[ri][ci];
                if (cand.mask[0] & used[0]) == 0
                    && (cand.mask[1] & used[1]) == 0
                    && (cand.mask[2] & used[2]) == 0
                {
                    options.push((ri, ci));
                }
            }
            if options.is_empty() {
                return false; // uncovered cell with no option — dead
            }
            match &best {
                Some((_, b)) if b.len() <= options.len() => {}
                _ => best = Some((x, options)),
            }
        }
        let Some((_, options)) = best else {
            // every free cell covered; all regions assigned — leaf-validate
            // (companion rules like `different` reject structurally fine
            // covers) and keep backtracking on failure.
            if !placed.iter().all(|p| p.is_some()) {
                return false;
            }
            let mut region_of: Vec<Option<usize>> = vec![None; n];
            for entry in placed.iter() {
                if let Some((ri, mask)) = entry {
                    for x in 0..n {
                        if mask[x / 64] & (1u64 << (x % 64)) != 0 {
                            region_of[x] = Some(*ri);
                        }
                    }
                }
            }
            let regions = super::build_regions(&region_of, h, w);
            return crate::solver::validate::validate(puzzle, &regions);
        };
        for (ri, ci) in options {
            let cand = &cands[ri][ci];
            let key = if distinct_shapes {
                Some(shape_key_small(&cand.cells, w))
            } else {
                None
            };
            if let Some(k) = key {
                if seen_keys.contains(&k) {
                    continue; // `different`: shapes must all be distinct
                }
            }
            placed[ri] = Some((ri, cand.mask));
            if let Some(k) = key {
                seen_keys.insert(k);
            }
            used[0] |= cand.mask[0];
            used[1] |= cand.mask[1];
            used[2] |= cand.mask[2];
            if search(
                puzzle, w, h, n, cands, by_cell, free_cells, sym, placed, used, nodes,
                distinct_shapes, seen_keys, deadline,
            ) {
                return true;
            }
            used[0] &= !cand.mask[0];
            used[1] &= !cand.mask[1];
            used[2] &= !cand.mask[2];
            if let Some(k) = key {
                seen_keys.remove(&k);
            }
            placed[ri] = None;
        }
        false
    }
    let mut seen_keys: std::collections::HashSet<u64> = Default::default();
    if search(
        puzzle, w, h, n, cands, &by_cell, &free_cells, sym, &mut placed, &mut used, &mut nodes,
        distinct_shapes, &mut seen_keys, deadline,
    ) {
        let out: Vec<(usize, Mask)> = placed
            .into_iter()
            .map(|p| p.expect("assigned"))
            .collect();
        Some(out)
    } else {
        None
    }
}

/// Tail with a deferred second-large region (0289 class): enumerate the
/// second-large region's exact-size sets within the leftover (a small
/// leftover BFS beats the 136k full-board pre-generation) and return the
/// first whose rest passes the residue checks.
/// Growth frontier for the deferred-tail BFS: unused, unblocked, non-seed
/// cells that keep the type budget (at most one cell per symbol type).
#[allow(clippy::too_many_arguments)]
fn tail_frontier(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    cur: &[usize],
    used: &[u64; 3],
    tail_seed: usize,
    res_cell: usize,
    seed_cell: &std::collections::HashSet<usize>,
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
) -> std::collections::BTreeSet<usize> {
    let mut frontier: std::collections::BTreeSet<usize> = Default::default();
    for &x in cur {
        let (r, c) = (x / w, x % w);
        for (nr, nc) in [
            (r as i64 - 1, c as i64),
            (r as i64 + 1, c as i64),
            (r as i64, c as i64 - 1),
            (r as i64, c as i64 + 1),
        ] {
            if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                continue;
            }
            let y = (nr as usize) * w + nc as usize;
            if puzzle.cells[nr as usize][nc as usize].blocked || cur.contains(&y) {
                continue;
            }
            if used[y / 64] & (1u64 << (y % 64)) != 0 {
                continue;
            }
            if y == res_cell || (y != tail_seed && seed_cell.contains(&y)) {
                continue;
            }
            if wall_between(puzzle, w, x, y) {
                continue;
            }
            if let Some((sym_of, _)) = sym {
                if let Some(&t) = sym_of.get(&y) {
                    if cur.iter().any(|&x2| sym_of.get(&x2) == Some(&t)) {
                        continue;
                    }
                }
            }
            frontier.insert(y);
        }
    }
    frontier
}

#[allow(clippy::too_many_arguments)]
fn find_tail_set(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    placed: &[(usize, Mask)],
    seeds: &[(usize, usize)],
    tail_ri: usize,
    tail_sz: usize,
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
    deadline: crate::clock::Instant,
) -> Option<Mask> {
    let tail_seed = seeds[tail_ri].0;
    let mut used = [0u64; 3];
    for (_, pm) in placed.iter() {
        used[0] |= pm[0];
        used[1] |= pm[1];
        used[2] |= pm[2];
    }
    if used[tail_seed / 64] & (1u64 << (tail_seed % 64)) != 0 {
        return None;
    }
    let mut seen: std::collections::HashSet<Vec<usize>> = Default::default();
    let mut stack: Vec<Vec<usize>> = vec![vec![tail_seed]];
    seen.insert(vec![tail_seed]);
    let seed_cell: std::collections::HashSet<usize> = seeds.iter().map(|&(c, _)| c).collect();
    let mut tried = 0usize;
    while let Some(cur) = stack.pop() {
        if crate::clock::Instant::now() >= deadline || tried > 80_000 {
            return None;
        }
        if cur.len() == tail_sz {
            tried += 1;
            if let Some((sym_of, n_types)) = sym {
                let mut counts = vec![0usize; n_types];
                let mut ok = true;
                for &x in &cur {
                    if let Some(&t) = sym_of.get(&x) {
                        counts[t] += 1;
                        if counts[t] > 1 {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok || counts.iter().any(|&c| c != 1) {
                    continue;
                }
            }
            let mask = mask_of(&cur, n);
            let mut placed2 = placed.to_vec();
            placed2.push((tail_ri, mask));
            if residue_ok(puzzle, w, h, n, &placed2, seeds, res_cell, res_size, diff_mode, sym) {
                return Some(mask);
            }
            continue;
        }
        let frontier = tail_frontier(
            puzzle, w, h, &cur, &used, tail_seed, res_cell, &seed_cell, sym,
        );
        for y in frontier {
            let mut nxt = cur.clone();
            nxt.push(y);
            nxt.sort_unstable();
            if seen.insert(nxt.clone()) {
                stack.push(nxt);
            }
        }
    }
    None
}

/// Depth-first placement with dynamic MRV + MAC (arc-consistency cascade).
/// `tail = Some((region idx, size))` defers a second-large region to
/// `tail_two_large` at the leaf; `None` means the residue alone closes.
#[allow(clippy::too_many_arguments)]
fn cover_dfs(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    remaining: &mut Vec<usize>,
    order: &[usize],
    cands: &[Vec<CoverCand>],
    seeds: &[(usize, usize)],
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
    placed: &mut Vec<(usize, Mask)>,
    active: &mut Vec<Vec<bool>>,
    nodes: &mut usize,
    deadline: crate::clock::Instant,
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
    tail: Option<(usize, usize)>,
    full_cover: bool,
    child_cap: usize,
) -> bool {
    const NODE_CAP: usize = 20_000_000;
    let my_start = *nodes;
    if *nodes >= NODE_CAP || crate::clock::Instant::now() >= deadline {
        return false;
    }
    *nodes += 1;
    // Iterative broadening (doc 07 修订11): `child_cap` bounds how many nodes
    // this invocation's *subtree* may visit.  A giant dead sibling subtree can
    // no longer starve the shallow solution sitting behind it (0308: the first
    // root candidate's 800k-node dead subtree burned the whole budget while the
    // truth sat at rank 2).  Parents retry truncated children at 8× the cap, so
    // the union of passes is complete.  `0` disables the bound.
    if child_cap > 0 && *nodes - my_start >= child_cap {
        return false;
    }
    // Depth profile (COVER_DEBUG): where the tree actually grows.  Bounded
    // histogram, printed once when the cover search ends.
    if std::env::var("COVER_DEBUG").is_ok() {
        let depth = (order.len() - remaining.len()).min(31);
        COVER_DEPTH_HIST.with(|h| h.borrow_mut()[depth] += 1);
    }
    // Base propagation at every node (root included).
    let rem_snapshot = remaining.clone();
    let placed_snapshot = placed.len();
    let active_snapshot = active.clone();
    if mac_propagate(
        w, h, n, puzzle, remaining, order, cands, seeds, active, placed,
        res_cell, res_size, diff_mode,
    )
    .is_none()
    {
        return false;
    }
    // Pocket exact-fill (see `pocket_ok`): dies at the first mis-sized
    // unreachable pocket instead of at the leaf residue check.  Gated on
    // `tail.is_none()`: a deferred tail region is carved from the leftover at
    // the very end (`find_tail_set`), so its cells are not in `remaining` and
    // a pocket that the tail would legitimately fill must not be declared
    // dead (the gate that regressed 0289's tail-slot cover).
    if tail.is_none()
        && !pocket_ok(puzzle, w, h, n, placed, remaining, order, seeds, res_cell, res_size)
    {
        return false;
    }
    let result = (|| {
        if remaining.is_empty() {
            if full_cover {
                // Every region is placed; the partition must cover all free
                // cells exactly.
                let mut used = [0u64; 3];
                for (_, pm) in placed.iter() {
                    used[0] |= pm[0];
                    used[1] |= pm[1];
                    used[2] |= pm[2];
                }
                for idx in 0..n {
                    let (r, c) = (idx / w, idx % w);
                    if puzzle.cells[r][c].blocked {
                        continue;
                    }
                    if used[idx / 64] & (1u64 << (idx % 64)) == 0 {
                        return false;
                    }
                }
                return true;
            }
            return match tail {
                None => residue_ok(puzzle, w, h, n, placed, seeds, res_cell, res_size, diff_mode, sym),
                Some((ri2, sz2)) => find_tail_set(
                    puzzle, w, h, n, placed, seeds, ri2, sz2, res_cell, res_size,
                    diff_mode, sym, deadline,
                )
                .is_some(),
            };
        }
        // Dynamic MRV over active candidates.
        let mut best: Option<(usize, usize)> = None;
        for &k in remaining.iter() {
            let cnt = (0..cands[k].len()).filter(|&ci| active[k][ci]).count();
            if cnt == 0 {
                return false;
            }
            if best.map(|(_, b)| cnt < b).unwrap_or(true) {
                best = Some((k, cnt));
            }
        }
        let (k, _) = best.unwrap();
        // Collect the viable candidates first, then decide their visit order.
        let mut viable: Vec<usize> = Vec::new();
        for ci in 0..cands[k].len() {
            if !active[k][ci] {
                continue;
            }
            if !compat(seeds, order, placed, diff_mode, k, &cands[k][ci]) {
                continue;
            }
            if !res_roomy(puzzle, w, h, n, placed, res_cell, res_size, &cands[k][ci]) {
                continue;
            }
            viable.push(ci);
        }
        let mut ordered = probe_order_cover_candidates(
            puzzle, w, h, n, k, remaining, order, cands, seeds, &viable, active,
            res_cell, res_size, diff_mode, placed, deadline,
        );
        // Visit in order with iterative broadening (see `child_cap` on the
        // entry): each child's subtree is bounded by `cap_now` nodes; children
        // truncated at the cap are retried at 8× (final round unlimited), so
        // no giant dead sibling subtree can starve a shallow solution.
        let mut pending = ordered;
        let mut cap_now = child_cap;
        loop {
            let mut truncated: Vec<usize> = Vec::new();
            for &ci in &pending {
                if crate::clock::Instant::now() >= deadline {
                    break;
                }
                let before = *nodes;
                let mut rem2 = remaining.clone();
                rem2.retain(|&x| x != k);
                let mut placed2 = placed.clone();
                placed2.push((order[k], cands[k][ci].mask));
                let mut active2 = active.clone();
                active2[k][ci] = true; // keep chosen visible for compat of others
                if cover_dfs(
                    puzzle, w, h, n, &mut rem2, order, cands, seeds, res_cell, res_size,
                    diff_mode, &mut placed2, &mut active2, nodes, deadline, sym, tail, full_cover,
                    cap_now,
                ) {
                    *placed = placed2;
                    *remaining = rem2;
                    return true;
                }
                if cap_now > 0 && *nodes - before >= cap_now {
                    truncated.push(ci);
                }
            }
            if truncated.is_empty() {
                break;
            }
            pending = truncated;
            cap_now = if cap_now == 0 || cap_now > usize::MAX / 8 {
                0
            } else {
                cap_now * 8
            };
        }
        false
    })();
    if !result {
        *remaining = rem_snapshot;
        placed.truncate(placed_snapshot);
        *active = active_snapshot;
    }
    result
}



/// Count cells reachable from `start` through unused, unblocked cells
/// without wall crossings (capped at `cap`).
fn reachable_count(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    used: &[u64; 3],
    start: usize,
    cap: usize,
) -> usize {
    let mut seen = [0u64; 3];
    let mut stack = vec![start];
    seen[start / 64] |= 1u64 << (start % 64);
    let mut cnt = 0usize;
    while let Some(x) = stack.pop() {
        cnt += 1;
        if cnt >= cap {
            return cnt;
        }
        let (r, c) = (x / w, x % w);
        for (nr, nc) in [
            (r as i64 - 1, c as i64),
            (r as i64 + 1, c as i64),
            (r as i64, c as i64 - 1),
            (r as i64, c as i64 + 1),
        ] {
            if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                continue;
            }
            let y = (nr as usize) * w + nc as usize;
            if seen[y / 64] & (1u64 << (y % 64)) != 0 {
                continue;
            }
            if used[y / 64] & (1u64 << (y % 64)) != 0 {
                continue;
            }
            if puzzle.cells[nr as usize][nc as usize].blocked {
                continue;
            }
            if wall_between(puzzle, w, x, y) {
                continue;
            }
            seen[y / 64] |= 1u64 << (y % 64);
            stack.push(y);
        }
    }
    cnt
}

/// Residue checks at the leaf: exact size (implied by the sum identity),
/// seed in, 4-connected without wall crossings, and (under
/// `differentiation`) no adjacency to equal-size placed regions.
fn residue_ok(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    placed: &[(usize, Mask)],
    seeds: &[(usize, usize)],
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
    sym: Option<(&std::collections::HashMap<usize, usize>, usize)>,
) -> bool {
    let mut used = [0u64; 3];
    for (_, pm) in placed.iter() {
        used[0] |= pm[0];
        used[1] |= pm[1];
        used[2] |= pm[2];
    }
    if used[res_cell / 64] & (1u64 << (res_cell % 64)) != 0 {
        return false;
    }
    let mut rest: Vec<usize> = Vec::new();
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if puzzle.cells[r][c].blocked {
            continue;
        }
        if used[idx / 64] & (1u64 << (idx % 64)) == 0 {
            rest.push(idx);
        }
    }
    if rest.len() != res_size {
        return false;
    }
    if reachable_count(puzzle, w, h, &used, res_cell, res_size + 1) != res_size {
        return false;
    }
    if let Some((sym_of, n_types)) = sym {
        let mut counts = vec![0usize; n_types];
        for &x in &rest {
            if let Some(&t) = sym_of.get(&x) {
                counts[t] += 1;
            }
        }
        if counts.iter().any(|&c| c != 1) {
            return false;
        }
    }
    if diff_mode {
        let res_mask = mask_of(&rest, n);
        for (ri, pm) in placed.iter() {
            if seeds[*ri].1 != res_size {
                continue;
            }
            let touch = dilate(pm, w, h, n);
            if mask_and(&touch, &res_mask) {
                return false;
            }
        }
    }
    true
}

/// Non-decreasing partitions of `total` into exactly `m` parts within
/// `[lo, hi]` (size multisets), capped and lexicographic.  With
/// `shape_limited` (the `different` rule — every region shape distinct),
/// the count of size-k regions is bounded by the number of free polyominoes
/// of size k (1, 1, 2, 5, 12, 35, 108, 369, …), which collapses the
/// partition space (0317: [1,2,3,3,4x5,5,6] is one of a handful).
fn size_multisets(
    total: usize,
    m: usize,
    lo: usize,
    hi: usize,
    cap: usize,
    shape_limited: bool,
) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    const FREE_POLY: [usize; 9] = [0, 1, 1, 2, 5, 12, 35, 108, 369];
    fn rec(
        rem: usize,
        parts: usize,
        lo: usize,
        hi: usize,
        cur: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
        cap: usize,
        shape_limited: bool,
    ) {
        if out.len() >= cap {
            return;
        }
        if parts == 0 {
            if rem == 0 {
                out.push(cur.clone());
            }
            return;
        }
        let min_here = lo.max(rem.saturating_sub(hi * (parts - 1)));
        let max_here = hi.min(rem.saturating_sub(lo * (parts - 1)));
        let start = cur.last().copied().unwrap_or(lo).max(min_here);
        for v in start..=max_here {
            if shape_limited && v < FREE_POLY.len() {
                let used = cur.iter().filter(|&&x| x == v).count();
                if used >= FREE_POLY[v] {
                    continue;
                }
            }
            cur.push(v);
            rec(rem - v, parts - 1, lo, hi, cur, out, cap, shape_limited);
            cur.pop();
        }
    }
    rec(total, m, lo, hi, &mut Vec::new(), &mut out, cap, shape_limited);
    out
}

/// Symbol map (cell -> type index) over non-blocked cells; None when the
/// puzzle carries no symbols.
fn rose_symbol_map(
    puzzle: &Puzzle,
    h: usize,
    w: usize,
) -> Option<(std::collections::HashMap<usize, usize>, usize)> {
    let mut sym_of: std::collections::HashMap<usize, usize> = Default::default();
    let mut type_names: Vec<&str> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if puzzle.cells[r][c].blocked {
                continue;
            }
            if let Some(sym) = puzzle.cells[r][c].symbol.as_deref() {
                let t = match type_names.iter().position(|&x| x == sym) {
                    Some(i) => i,
                    None => {
                        type_names.push(sym);
                        type_names.len() - 1
                    }
                };
                sym_of.insert(r * w + c, t);
            }
        }
    }
    if type_names.is_empty() {
        None
    } else {
        let n_types = type_names.len();
        Some((sym_of, n_types))
    }
}

/// Full-cover pass (all-small class, 0317): every region's candidates are
/// generated symbol-filtered up to `full_bound`; Algorithm X (cell-first)
/// with the `different` distinct-shape constraint inside the search enforces
/// the size assignment itself — no multiset enumeration needed.
#[allow(clippy::too_many_arguments)]
fn full_cover_pass(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    anchor_cells: &[usize],
    min_sz: usize,
    full_bound: usize,
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
    seed_cell: &std::collections::HashSet<usize>,
    cache: &mut std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let m = anchor_cells.len();
    let mut cands: Vec<Vec<CoverCand>> = Vec::new();
    for i in 0..m {
        let mut list: Vec<CoverCand> = Vec::new();
        for sz in min_sz..=full_bound {
            let key = (i, sz);
            if !cache.contains_key(&key) {
                let cs = gen_cover_cands(
                    puzzle,
                    w,
                    h,
                    n,
                    anchor_cells[i],
                    sz,
                    seed_cell,
                    deadline,
                    Some((sym_of, n_types)),
                );
                cache.insert(key, cs);
            }
            list.extend(cache[&key].iter().cloned());
        }
        if std::env::var("COVER_DEBUG").is_ok() {
            eprintln!("rose_cover: full list[{i}] len={}", list.len());
        }
        if list.is_empty() {
            return None;
        }
        cands.push(list);
    }
    // Algorithm X is hopeless over giant lists (0289: 35k-row size-10
    // spaces) — leave those boards to the assignment attempts; the cache
    // keeps whatever was generated.
    let rows: usize = cands.iter().map(|v| v.len()).sum();
    if rows > 30_000 || cands.iter().any(|v| v.len() > 8_000) {
        return None;
    }
    let pass_deadline = {
        let t = crate::clock::Instant::now() + std::time::Duration::from_millis(15_000);
        if t < deadline {
            t
        } else {
            deadline
        }
    };
    let placed = exact_cover_x(
        puzzle,
        w,
        h,
        n,
        &cands,
        Some((sym_of, n_types)),
        puzzle.rules.iter().any(|r| r.ctype == "different"),
        pass_deadline,
    )?;
    let mut region_of: Vec<Option<usize>> = vec![None; n];
    mask_assign(&placed, &mut region_of, n);
    let regions = super::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        Some(regions)
    } else {
        None
    }
}

/// Per-attempt candidate lists: the placed regions' cached lists filtered
/// to candidates whose rose partner survives the size matching's allowed
/// edges.  `None` when any placed list is empty after filtering.
#[allow(clippy::too_many_arguments)]
fn assemble_attempt_lists(
    seeds: &[(usize, usize)],
    res_seed: usize,
    tail: Option<(usize, usize)>,
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    allowed: &Option<Vec<Vec<usize>>>,
    ptype: Option<usize>,
    partner_col: &std::collections::HashMap<usize, usize>,
    sym_of: &std::collections::HashMap<usize, usize>,
) -> Option<(Vec<usize>, Vec<Vec<CoverCand>>)> {
    let mut order: Vec<usize> = Vec::new();
    let mut cands: Vec<Vec<CoverCand>> = Vec::new();
    for (i, &(_, sz)) in seeds.iter().enumerate() {
        if i == res_seed || Some((i, sz)) == tail {
            continue;
        }
        let list = cache.get(&(i, sz)).filter(|l| !l.is_empty())?;
        let v: Vec<CoverCand> = match (allowed, ptype) {
            (Some(allw), Some(pt)) => list
                .iter()
                .filter(|c| {
                    c.cells.iter().any(|&x| {
                        if sym_of.get(&x).copied() != Some(pt) {
                            return false;
                        }
                        match partner_col.get(&x) {
                            Some(&col) => allw[i].contains(&col),
                            None => true,
                        }
                    })
                })
                .cloned()
                .collect(),
            _ => list.to_vec(),
        };
        if v.is_empty() {
            return None;
        }
        order.push(i);
        cands.push(v);
    }
    Some((order, cands))
}

/// Phase A (matching-led slack families) + Phase B/B2/B3 (size-DFS families
/// in at_min / cost / reversed order) — extracted from solve_rose_cover for
/// complexity.  Three orderings, one queue (doc 07 「分相排序」).
#[allow(clippy::too_many_arguments)]
fn run_rose_cover_phases(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    total: usize,
    min_sz: usize,
    attempts: &[((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)],
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
    diff_mode: bool,
    deadline: crate::clock::Instant,
    pair: Option<&PairLb>,
    anchor_cells: &[usize],
) -> Option<Vec<crate::types::RegionInfo>> {
    const DEFER_CAP: usize = 10;
    let h_ = h;
    let _ = h_;
    // Phase A: matching-led tight families (0804 class — the truth sits at
    // pairing-lb+slack<=2 which the size DFS never reaches in budget).
    let mut slack_attempts: Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)> =
        Vec::new();
    if let Some((lb, _)) = pair.as_ref() {
        let _t_slack = crate::clock::Instant::now();
        // Bound the slack enumeration — with the pair_lb fix, the smaller
        // lower bounds explode the slack space and eat the runner's budget.
        let slack_deadline = {
            let now = crate::clock::Instant::now();
            let cap = now + std::time::Duration::from_millis(500);
            if cap < deadline { cap } else { deadline }
        };
        enum_pair_slacks(
            lb,
            anchor_cells.len(),
            total,
            min_sz,
            anchor_cells,
            &cache,
            DEFER_CAP,
            &mut slack_attempts,
            slack_deadline,
        );
        if std::env::var("COVER_DEBUG").is_ok() {
            eprintln!("rose_cover: enum_pair_slacks took {:?}", _t_slack.elapsed());
        }
    }
    slack_attempts.sort_by_key(|(c, _, _, _)| *c);
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: slack_attempts={}", slack_attempts.len());
    }
    let mid = {
        let now = crate::clock::Instant::now();
        let t = now + deadline.saturating_duration_since(now) / 2;
        if t < deadline {
            t
        } else {
            deadline
        }
    };
    if let Some(regions) = run_cover_attempts(
        puzzle,
        w,
        h,
        n,
        &slack_attempts,
        &cache,
        &sym_of,
        n_types,
        diff_mode,
        mid,
        pair,
        anchor_cells,
    ) {
        return Some(regions);
    }
    // Phase B: the enumeration's own order (at_min DESC — 0289 class:
    // floor islands with big residue/tail lead).
    let mut out = run_cover_attempts(
        puzzle,
        w,
        h,
        n,
        &attempts,
        &cache,
        &sym_of,
        n_types,
        diff_mode,
        deadline,
        pair,
        anchor_cells,
    );
    if out.is_none() {
        // Phase B2: cost-ascending (product of candidate-list lengths).  The
        // 0838 balanced truth ranks 9140 here vs 16581 in the at_min order
        // (COVER_TRUTH measured) — still deep for one-attempt enumeration;
        // the joint size-multiset search is the real fix (open item).
        let mut by_cost = attempts.to_vec();
        by_cost.sort_by_key(|((_, _, cost), _, _, _)| *cost);
        out = run_cover_attempts(
            puzzle,
            w,
            h,
            n,
            &by_cost,
            &cache,
            &sym_of,
            n_types,
            diff_mode,
            deadline,
            pair,
            anchor_cells,
        );
    }
    if out.is_none() {
        // Phase B3: reversed queue (lean-floor families first).  Three
        // orderings, one queue — doc 07 「分相排序」.
        let mut rev = attempts.to_vec();
        rev.reverse();
        out = run_cover_attempts(
            puzzle,
            w,
            h,
            n,
            &rev,
            &cache,
            &sym_of,
            n_types,
            diff_mode,
            deadline,
            pair,
            anchor_cells,
        );
    }
    out
}

/// COVER_TRUTH membership check for the rose cover assignment enumeration
/// (extracted from solve_rose_cover for complexity).  A missing member means
/// the enumeration is incomplete (cap/order) — sorting can never recover it.
fn rose_cover_truth_report(
    puzzle: &Puzzle,
    w: usize,
    n: usize,
    anchor_cells: &[usize],
    seed_cell: &std::collections::HashSet<usize>,
    attempts: &[((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)],
) {
    let _ = n;
    let Ok(truth_path) = std::env::var("COVER_TRUTH") else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&truth_path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(arr) = v.get("regions").and_then(|r| r.as_array()) else {
        return;
    };
    let mut truth_sz: std::collections::HashMap<usize, usize> = Default::default();
    for reg in arr {
        let cells: Vec<usize> = reg
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|cell| {
                        let pair = cell.as_array()?;
                        let r = pair.first().and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                        let c = pair.get(1).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                        Some(r * w + c)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for &x in &cells {
            if seed_cell.contains(&x) {
                truth_sz.insert(x, cells.len());
            }
        }
    }
    let want: Vec<(usize, usize)> = anchor_cells
        .iter()
        .map(|&a| (a, *truth_sz.get(&a).unwrap_or(&0)))
        .collect();
    let hit = attempts.iter().position(|(_, _, seeds, tail)| {
        tail.is_none()
            && seeds.iter().copied().collect::<std::collections::HashSet<_>>()
                == want.iter().copied().collect::<std::collections::HashSet<_>>()
    });
    let mut cost_rank = None;
    if let Some(h) = hit {
        let tc = attempts[h].0 .2;
        cost_rank = Some(attempts.iter().filter(|(k, _, _, _)| k.2 < tc).count());
    }
    eprintln!(
        "rose-cover-truth: anchors={:?} in-attempts={:?} cost-rank={cost_rank:?} (puzzle cells {}) ",
        want.iter().map(|&(_, s)| s).collect::<Vec<_>>(),
        hit,
        puzzle.height * puzzle.width
    );
}

/// Close one successful cover attempt: stamp placed regions, capture the
/// deferred tail via leftover BFS, give the rest to the residue, validate.
#[allow(clippy::too_many_arguments)]
fn close_attempt(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    placed: &[(usize, Mask)],
    seeds: &[(usize, usize)],
    res_seed: usize,
    tail: Option<(usize, usize)>,
    res_cell: usize,
    res_size: usize,
    diff_mode: bool,
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let mut region_of: Vec<Option<usize>> = vec![None; n];
    mask_assign(placed, &mut region_of, n);
    if let Some((ti, tsz)) = tail {
        let captured = find_tail_set(
            puzzle,
            w,
            h,
            n,
            placed,
            seeds,
            ti,
            tsz,
            res_cell,
            res_size,
            diff_mode,
            Some((sym_of, n_types)),
            deadline,
        );
        if let Some(tmask) = captured {
            for x in 0..n {
                if tmask[x / 64] & (1u64 << (x % 64)) != 0 {
                    region_of[x] = Some(ti);
                }
            }
        }
    }
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if puzzle.cells[r][c].blocked {
            continue;
        }
        if region_of[idx].is_none() {
            region_of[idx] = Some(res_seed);
        }
    }
    let regions = super::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        Some(regions)
    } else {
        None
    }
}

/// Multiset attempt runner: small regions pre-generated (exact-size,
/// symbol-filtered), the second-largest deferred to the leftover BFS tail,
/// the largest as residue.
#[allow(clippy::too_many_arguments)]
fn run_cover_attempts(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    attempts: &[((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)],
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
    diff_mode: bool,
    deadline: crate::clock::Instant,
    pair: Option<&PairLb>,
    anchor_cells: &[usize],
) -> Option<Vec<crate::types::RegionInfo>> {
    const RUN_CAP: usize = 100_000;
    // Partner type (rose pairing filter) — every kept candidate must carry
    // a partner the size matching can still assign to this anchor.
    let ptype = pair.and_then(|(_, pcells)| pcells.first().and_then(|&c| sym_of.get(&c).copied()));
    let partner_col: std::collections::HashMap<usize, usize> = match pair {
        Some((_, pcells)) => pcells.iter().enumerate().map(|(j, &c)| (c, j)).collect(),
        None => Default::default(),
    };
    // COVER_TRUTH: run the official assignment's attempt first with a full
    // deadline slice — the "is it ordering or is the attempt itself unable to
    // crack it in its slice" judge.
    let mut ordered_idx: Vec<usize> = (0..attempts.len()).collect();
    if std::env::var("COVER_TRUTH").is_ok() {
        if let Ok(tp) = std::env::var("COVER_TRUTH") {
            if let Ok(text) = std::fs::read_to_string(&tp) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(arr) = v.get("regions").and_then(|r| r.as_array()) {
                        let mut truth_sz = std::collections::HashSet::new();
                        let anchor_set: std::collections::HashSet<usize> = anchor_cells.iter().copied().collect();
                        for reg in arr {
                            let cells: Vec<usize> = reg.as_array().map(|list| list.iter().filter_map(|cell| {
                                let pair = cell.as_array()?;
                                let r = pair.first().and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                                let c = pair.get(1).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                                Some(r * w + c)
                            }).collect()).unwrap_or_default();
                            for &x in &cells {
                                if anchor_set.contains(&x) {
                                    truth_sz.insert((x, cells.len()));
                                }
                            }
                        }
                        if let Some(pos) = attempts.iter().position(|(_, _, seeds, tail)| {
                            tail.is_none() && seeds.iter().copied().collect::<std::collections::HashSet<_>>() == truth_sz
                        }) {
                            let total_att = attempts.len();
                            eprintln!("rose-runner-truth: forcing truth attempt at {pos}/{total_att}");
                            ordered_idx.remove(pos);
                            ordered_idx.insert(0, pos);
                        } else {
                            eprintln!("rose-runner-truth: truth attempt NOT FOUND");
                        }
                    }
                }
            }
        }
    }
    for (run_ord, &ri) in ordered_idx.iter().enumerate() {
        let (_cost, res_seed, seeds, tail) = &attempts[ri];
        let (res_seed, seeds, tail) = (*res_seed, seeds, *tail);
        let run_i = run_ord;
        if crate::clock::Instant::now() >= deadline || run_i >= RUN_CAP {
            if std::env::var("COVER_DEBUG").is_ok() {
                eprintln!("rose_cover: runner stopped at {run_i} (now={:?} deadline={:?})", crate::clock::Instant::now(), deadline);
            }
            break;
        }
        let res_cell = seeds[res_seed].0;
        let res_size = seeds[res_seed].1;
        let sizes_full: Vec<usize> = seeds.iter().map(|&(_, sz)| sz).collect();
        let allowed = pair.map(|(lb, _)| pair_allowed(lb, &sizes_full));
        let (order, cands) = match assemble_attempt_lists(
            seeds,
            res_seed,
            tail,
            cache,
            &allowed,
            ptype,
            &partner_col,
            sym_of,
        ) {
            Some(v) => v,
            None => continue,
        };
        let mut remaining: Vec<usize> = (0..order.len()).collect();
        let mut placed: Vec<(usize, Mask)> = Vec::new();
        let mut active: Vec<Vec<bool>> = cands.iter().map(|v| vec![true; v.len()]).collect();
        let mut nodes = 0usize;
        let attempt_deadline = {
            // Short slice + volume: a solvable attempt lands in ~12ms
            // (0289) / ~0.4ms (0804); wandering unsat proofs must not eat
            // the queue.
            // 30ms slice: the 0838 cost-order walk sat ~800 attempts short of
            // the truth at 50ms; 0289 truth attempts solve in ~12ms so 30ms
            // still covers the known truths.
            let t = crate::clock::Instant::now() + std::time::Duration::from_millis(50);
            if t < deadline {
                t
            } else {
                deadline
            }
        };
        if std::env::var("COVER_DEBUG").is_ok() && run_i < 20 {
            eprintln!(
                "rose_cover: run[{run_i}] sizes={:?} lists={:?} tail={tail:?}",
                seeds.iter().map(|&(_, s)| s).collect::<Vec<_>>(),
                cands.iter().map(|v| v.len()).collect::<Vec<_>>()
            );
        }
        if cover_dfs(
            puzzle,
            w,
            h,
            n,
            &mut remaining,
            &order,
            &cands,
            seeds,
            res_cell,
            res_size,
            diff_mode,
            &mut placed,
            &mut active,
            &mut nodes,
            attempt_deadline,
            Some((sym_of, n_types)),
            tail,
            false,
            0,
        ) {
            if let Some(regions) = close_attempt(
                puzzle,
                w,
                h,
                n,
                &placed,
                seeds,
                res_seed,
                tail,
                res_cell,
                res_size,
                diff_mode,
                sym_of,
                n_types,
                deadline,
            ) {
                return Some(regions);
            }
        }
    }
    None
}

/// Pairwise feasibility among the small regions of one assignment: for
/// every pair, some candidate pair must be disjoint (and non-touching under
/// `differentiation`).  Kills most wrong assignments in microseconds before
/// the cover DFS (0289: 105k attempts of which almost all conflict early).
#[allow(clippy::too_many_arguments)]
fn attempt_pairwise_ok(
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    _seeds: &[(usize, usize)],
    small: &[(usize, usize)], // (region idx, size)
    diff_mode: bool,
) -> bool {
    for ai in 0..small.len() {
        for bj in (ai + 1)..small.len() {
            let (ia, sa) = small[ai];
            let (ib, sb) = small[bj];
            let la = cache.get(&(ia, sa)).map(|v| v.as_slice()).unwrap_or(&[]);
            let lb = cache.get(&(ib, sb)).map(|v| v.as_slice()).unwrap_or(&[]);
            if la.is_empty() || lb.is_empty() {
                return false;
            }
            let mut ok = false;
            'outer: for ca in la {
                for cb in lb {
                    if mask_and(&ca.mask, &cb.mask) {
                        continue;
                    }
                    if diff_mode && sa == sb && (mask_and(&ca.touch, &cb.mask) || mask_and(&cb.touch, &ca.mask)) {
                        continue;
                    }
                    ok = true;
                    break 'outer;
                }
            }
            if !ok {
                return false;
            }
        }
    }
    true
}

/// (anchors × partners size lower-bound matrix, partner cells in column
/// order) for the rose one-of-each-type pairing.
type PairLb = (Vec<Vec<usize>>, Vec<usize>);

/// Shortest path (in cells) of any rose region containing both `a` and `b`:
/// every other symbol cell is excluded (one of each type per region), as
/// are walls and blocked cells.  `usize::MAX` when the pair can never share
/// a region.  This is a size lower bound for the anchor↔partner pairing.
fn pair_lb(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    a: usize,
    b: usize,
    sym_of: &std::collections::HashMap<usize, usize>,
) -> usize {
    use std::collections::{HashMap, VecDeque};
    let mut dist: HashMap<usize, usize> = Default::default();
    let mut q: VecDeque<usize> = Default::default();
    dist.insert(a, 0usize);
    q.push_back(a);
    while let Some(x) = q.pop_front() {
        let d = dist[&x];
        let (r, c) = (x / w, x % w);
        for (nr, nc) in [
            (r as i64 - 1, c as i64),
            (r as i64 + 1, c as i64),
            (r as i64, c as i64 - 1),
            (r as i64, c as i64 + 1),
        ] {
            if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                continue;
            }
            let y = (nr as usize) * w + nc as usize;
            if dist.contains_key(&y)
                || puzzle.cells[nr as usize][nc as usize].blocked
                || wall_between(puzzle, w, x, y)
            {
                continue;
            }
            // Only exclude symbol cells of the SAME type as a or b (the region
            // can hold one of each type — other-type symbol cells are allowed).
            if y != a && y != b {
                if let Some(&yt) = sym_of.get(&y) {
                    let ta = sym_of.get(&a).copied().unwrap_or(usize::MAX);
                    let tb = sym_of.get(&b).copied().unwrap_or(usize::MAX);
                    if yt == ta || yt == tb {
                        continue;
                    }
                }
            }
            if y == b {
                return d + 2;
            }
            dist.insert(y, d + 1);
            q.push_back(y);
        }
    }
    usize::MAX
}

/// Anchors × partner-type lower bounds plus partner cells in column order.
/// The partner type is the smallest symbol type other than the anchors'
/// type; the counts must match (rose: one of each type per region).
fn pair_lb_matrix(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    anchor_cells: &[usize],
    sym_of: &std::collections::HashMap<usize, usize>,
) -> Option<PairLb> {
    let m = anchor_cells.len();
    let anchor_type = *sym_of.get(&anchor_cells[0])?;
    let partner_type = sym_of
        .values()
        .copied()
        .filter(|&t| t != anchor_type)
        .min()?;
    let mut partner_cells: Vec<usize> = sym_of
        .iter()
        .filter(|(_, &t)| t == partner_type)
        .map(|(&c, _)| c)
        .collect();
    partner_cells.sort_unstable();
    if partner_cells.len() != m {
        return None;
    }
    let mut lb = Vec::with_capacity(m);
    for &a in anchor_cells {
        let row: Vec<usize> = partner_cells
            .iter()
            .map(|&b| pair_lb(puzzle, w, h, a, b, sym_of))
            .collect();
        lb.push(row);
    }
    Some((lb, partner_cells))
}

/// Kuhn augmenting path on the lb graph under `bounds` (edge (i, j) exists
/// iff `lb[i][j] <= bounds[i]`).
fn pair_augment(
    lb: &[Vec<usize>],
    bounds: &[usize],
    i: usize,
    seen: &mut [bool],
    match_r: &mut [usize],
) -> bool {
    for j in 0..lb[i].len() {
        if lb[i][j] > bounds[i] || seen[j] {
            continue;
        }
        seen[j] = true;
        if match_r[j] == usize::MAX || pair_augment(lb, bounds, match_r[j], seen, match_r) {
            match_r[j] = i;
            return true;
        }
    }
    false
}

/// Perfect matching under per-anchor size bounds — a rose partition must
/// give every anchor a distinct partner its region can reach within its
/// size.  Kills whole size families in microseconds (0289: all-5s dies on
/// the (6,5) partner's lower bound 7).
fn pair_match_ok(lb: &[Vec<usize>], bounds: &[usize]) -> bool {
    let m = lb.len();
    if m == 0 {
        return true;
    }
    let p = lb[0].len();
    if p < m {
        return false;
    }
    let mut match_r = vec![usize::MAX; p];
    let mut seen = vec![false; p];
    for i in 0..m {
        for s in seen.iter_mut() {
            *s = false;
        }
        if !pair_augment(lb, bounds, i, &mut seen, &mut match_r) {
            return false;
        }
    }
    true
}

/// Allowed partner columns per anchor: `j` is allowed for `i` iff some
/// perfect matching uses (i, j) under `bounds`.
fn pair_allowed(lb: &[Vec<usize>], bounds: &[usize]) -> Vec<Vec<usize>> {
    let m = lb.len();
    let p = lb.first().map(|r| r.len()).unwrap_or(0);
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); m];
    for i in 0..m {
        for j in 0..p {
            if lb[i][j] > bounds[i] {
                continue;
            }
            let mut match_r = vec![usize::MAX; p];
            match_r[j] = i;
            let mut ok = true;
            for i2 in 0..m {
                if i2 == i {
                    continue;
                }
                let mut seen = vec![false; p];
                seen[j] = true;
                if !pair_augment(lb, bounds, i2, &mut seen, &mut match_r) {
                    ok = false;
                    break;
                }
            }
            if ok {
                out[i].push(j);
            }
        }
    }
    out
}

/// Bottleneck pairing excess: min over perfect matchings of
/// max_i(size_i - lb[i][pi(i)]).  A tight value (0-2) means some pairing
/// fits the sizes almost exactly — the truth's shape; wild tail/residue
/// sizes price high here and sort last.
fn pair_min_max_excess(lb: &[Vec<usize>], sizes: &[usize]) -> (usize, Vec<usize>) {
    let m = lb.len();
    if m == 0 {
        return (0, Vec::new());
    }
    let p = lb[0].len();
    // Edge set at threshold thr: hosts within size and within excess thr.
    let mut lb2: Vec<Vec<usize>> = vec![vec![usize::MAX; p]; m];
    let zero = vec![0usize; m];
    let mut match_r = vec![usize::MAX; p];
    let mut seen = vec![false; p];
    let matching_at = |thr: usize,
                       lb2: &mut Vec<Vec<usize>>,
                       match_r: &mut Vec<usize>,
                       seen: &mut [bool]|
     -> Option<Vec<usize>> {
        for i in 0..m {
            for j in 0..p {
                lb2[i][j] = if lb[i][j] <= sizes[i] && sizes[i] - lb[i][j] <= thr {
                    0
                } else {
                    usize::MAX
                };
            }
        }
        for mm in match_r.iter_mut() {
            *mm = usize::MAX;
        }
        for i in 0..m {
            for s in seen.iter_mut() {
                *s = false;
            }
            if !pair_augment(lb2, &zero, i, seen, match_r) {
                return None;
            }
        }
        let mut out = vec![usize::MAX; m];
        for j in 0..p {
            if match_r[j] != usize::MAX {
                out[match_r[j]] = j;
            }
        }
        Some(out)
    };
    let mut hi = 0usize;
    for i in 0..m {
        for j in 0..p {
            if lb[i][j] <= sizes[i] {
                hi = hi.max(sizes[i] - lb[i][j]);
            }
        }
    }
    if matching_at(hi, &mut lb2, &mut match_r, &mut seen).is_none() {
        return (usize::MAX, vec![usize::MAX; m]); // no matching at all
    }
    let mut lo = 0usize;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if matching_at(mid, &mut lb2, &mut match_r, &mut seen).is_some() {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let pi = matching_at(lo, &mut lb2, &mut match_r, &mut seen).unwrap();
    let mut excess = vec![usize::MAX; m];
    for i in 0..m {
        if pi[i] != usize::MAX {
            excess[i] = sizes[i] - lb[i][pi[i]];
        }
    }
    (lo, excess)
}

/// Per-anchor region-size ceiling: the connected component of the board
/// minus the other anchors (walls and blocked cells excluded) — the region
/// may grow freely otherwise, so this bounds residue/tail sizes.
fn reach_upper(puzzle: &Puzzle, w: usize, h: usize, anchor_cells: &[usize]) -> Vec<usize> {
    use std::collections::{HashSet, VecDeque};
    let anchors: HashSet<usize> = anchor_cells.iter().copied().collect();
    let mut out = Vec::with_capacity(anchor_cells.len());
    for &a in anchor_cells {
        let mut seen: HashSet<usize> = Default::default();
        let mut q: VecDeque<usize> = Default::default();
        seen.insert(a);
        q.push_back(a);
        while let Some(x) = q.pop_front() {
            let (r, c) = (x / w, x % w);
            for (nr, nc) in [
                (r as i64 - 1, c as i64),
                (r as i64 + 1, c as i64),
                (r as i64, c as i64 - 1),
                (r as i64, c as i64 + 1),
            ] {
                if nr < 0 || nr >= h as i64 || nc < 0 || nc >= w as i64 {
                    continue;
                }
                let y = (nr as usize) * w + nc as usize;
                if seen.contains(&y)
                    || puzzle.cells[nr as usize][nc as usize].blocked
                    || (y != a && anchors.contains(&y))
                    || wall_between(puzzle, w, x, y)
                {
                    continue;
                }
                seen.insert(y);
                q.push_back(y);
            }
        }
        out.push(seen.len());
    }
    out
}

/// Matching-led size enumeration: every perfect matching of the lb graph
/// fixes each region's floor `max(lb, min_sz)`; distribute the remaining
/// slack (per-region cap) to reach the board total.  Both cover truths live
/// at slack <= 2 per region (0289: +2/+2/+0/+0/+2/+0, 0804: one +2), so this
/// pass emits the tight families in milliseconds where the size DFS reaches
/// them only deep in its order.
#[allow(clippy::too_many_arguments)]
fn enum_pair_slacks(
    lb: &[Vec<usize>],
    m: usize,
    total: usize,
    min_sz: usize,
    anchor_cells: &[usize],
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    defer_cap: usize,
    out: &mut Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)>,
    deadline: crate::clock::Instant,
) {
    const SMAX: usize = 2;
    const SLACK_CAP: usize = 50_000;
    let p = lb[0].len();
    let mut pi: Vec<usize> = vec![usize::MAX; m];
    let mut used = 0u64;
    let mut slack_left = 0usize;
    let mut base: Vec<usize> = vec![0; m];
    // recursive matching enumeration with inline slack distribution
    fn rec(
        i: usize,
        m: usize,
        p: usize,
        lb: &[Vec<usize>],
        pi: &mut Vec<usize>,
        used: &mut u64,
        base: &mut Vec<usize>,
        slack_left: &mut usize,
        total: usize,
        min_sz: usize,
        anchor_cells: &[usize],
        cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
        defer_cap: usize,
        out: &mut Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)>,
        deadline: crate::clock::Instant,
    ) {
        if out.len() >= SLACK_CAP || crate::clock::Instant::now() >= deadline {
            return;
        }
        if i == m {
            if std::env::var("COVER_DEBUG").is_ok() {
                eprintln!("rose_cover: slack-matching pi={pi:?}");
            }
            // base floors and remaining slack for this matching
            let mut bsum = 0usize;
            for k in 0..m {
                base[k] = lb[k][pi[k]].max(min_sz);
                bsum += base[k];
            }
            if bsum > total {
                return;
            }
            *slack_left = total - bsum;
            emit_slacks(
                0,
                m,
                base,
                *slack_left,
                min_sz,
                anchor_cells,
                cache,
                defer_cap,
                lb,
                pi,
                out,
                deadline,
            );
            return;
        }
        for j in 0..p {
            if (*used >> j) & 1 == 1 || lb[i][j] == usize::MAX {
                continue;
            }
            *used |= 1 << j;
            pi[i] = j;
            rec(
                i + 1,
                m,
                p,
                lb,
                pi,
                used,
                base,
                slack_left,
                total,
                min_sz,
                anchor_cells,
                cache,
                defer_cap,
                out,
                deadline,
            );
            *used &= !(1 << j);
        }
    }
    fn emit_slacks(
        k: usize,
        m: usize,
        base: &[usize],
        slack: usize,
        min_sz: usize,
        anchor_cells: &[usize],
        cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
        defer_cap: usize,
        lb: &[Vec<usize>],
        pi: &[usize],
        out: &mut Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)>,
        deadline: crate::clock::Instant,
    ) {
        if out.len() >= SLACK_CAP || crate::clock::Instant::now() >= deadline {
            return;
        }
        if k == m {
            if slack != 0 {
                return;
            }
            // sizes = base[..]; residue = a max region (normalization)
            let mut sizes: Vec<usize> = base.to_vec();
            let res_seed = sizes
                .iter()
                .enumerate()
                .max_by_key(|(_, &s)| s)
                .map(|(i, _)| i)
                .unwrap();
            let mut tail: Option<(usize, usize)> = None;
            let mut ok = true;
            let mut cost = 1usize;
            let mut seeds: Vec<(usize, usize)> = vec![(0, 0); m];
            for i in 0..m {
                seeds[i] = (anchor_cells[i], sizes[i]);
                if i == res_seed {
                    continue;
                }
                if sizes[i] > defer_cap {
                    if tail.is_some() {
                        ok = false; // machinery places one deferred tail
                        break;
                    }
                    tail = Some((i, sizes[i]));
                    cost = cost.saturating_mul(8);
                } else {
                    let cl = cache.get(&(i, sizes[i])).map(|v| v.len()).unwrap_or(0);
                    if cl == 0 {
                        ok = false;
                        break;
                    }
                    cost = cost.saturating_mul(cl.max(1));
                }
            }
            if !ok {
                return;
            }
            let small: Vec<(usize, usize)> = (0..m)
                .filter(|&i| i != res_seed && Some((i, sizes[i])) != tail)
                .map(|i| (i, sizes[i]))
                .collect();
            if !attempt_pairwise_ok(cache, &seeds, &small, false) {
                return;
            }
            // key: floor/lb-tight count, bottleneck excess, cost
            let (max_ex, ex_vec) = pair_min_max_excess(lb, &sizes);
            let at_min = sizes.iter().filter(|&&s| s == min_sz).count();
            let at_lb = ex_vec.iter().filter(|&&e| e == 0).count();
            let tight = at_min.max(at_lb);
            out.push(((usize::MAX - tight, max_ex, cost), res_seed, seeds, tail));
            return;
        }
        for add in 0..=SMAX.min(slack) {
            let mut b2 = base.to_vec();
            b2[k] += add;
            emit_slacks(
                k + 1,
                m,
                &b2,
                slack - add,
                min_sz,
                anchor_cells,
                cache,
                defer_cap,
                lb,
                pi,
                out,
                deadline,
            );
        }
    }
    rec(
        0, m, p, lb, &mut pi, &mut used, &mut base, &mut slack_left, total, min_sz, anchor_cells,
        cache, defer_cap, out, deadline,
    );
}

/// Availability-guided size DFS: assign each non-residue region a size from
/// its **hostable** set (exact-size candidates exist), pruning by partial
/// sum; the residue absorbs the rest.  One deferred tail slot per attempt
/// covers the second-large region (0289: truth `[5,5,5,7,12,15]` needs the
/// size-12 region past the candidate cap).  Rose pairings add a matching
/// feasibility prune: every anchor must host a distinct partner within its
/// size (all-5s dies on the (6,5) partner's lower bound).  Collects
/// (cost, res_seed, seeds, tail) attempts sorted by filtered candidate cost.
#[allow(clippy::too_many_arguments)]
fn enumerate_assignments(
    puzzle: &Puzzle,
    w: usize,
    h: usize,
    n: usize,
    anchor_cells: &[usize],
    sym_of: &std::collections::HashMap<usize, usize>,
    n_types: usize,
    seed_cell: &std::collections::HashSet<usize>,
    cache: &mut std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    min_sz: usize,
    defer_cap: usize,
    shape_limited: bool,
    deadline: crate::clock::Instant,
    pair: Option<&PairLb>,
    reach: &[usize],
) -> Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)> {
    const ATTEMPT_CAP: usize = 50_000;
    let m = anchor_cells.len();
    let mut total = 0usize;
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if !puzzle.cells[r][c].blocked {
            total += 1;
        }
    }
    let mut out: Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)> = Vec::new();
    // Hostable sizes per region.  Round-robin over sizes so a deadline hit
    // leaves every seed with candidates for the smaller sizes (sequential
    // per-seed order starves seeds 1..m when seed 0's large-size DFS is slow).
    //
    // For the large class (defer_cap > 10), the non-residue sizes are bounded
    // by total/m (the residue must be the largest).  The truth can be very
    // unbalanced (0975a: {61,20,15,13,12}), so the window is [min_sz, total/m]
    // — not just ±m around the average.
    let (sz_lo, sz_hi) = if defer_cap > 10 {
        let avg = total / m.max(1);
        (min_sz, avg.min(defer_cap))
    } else {
        (min_sz, defer_cap)
    };
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: enum sizes [{sz_lo}, {sz_hi}] (min={min_sz} cap={defer_cap} total={total} m={m})");
    }
    let mut hostable: Vec<Vec<usize>> = vec![Vec::new(); m];
    for sz in sz_lo..=sz_hi {
        if crate::clock::Instant::now() >= deadline {
            break;
        }
        for si in 0..m {
            let key = (si, sz);
            if !cache.contains_key(&key) {
                let cands = gen_cover_cands(
                    puzzle,
                    w,
                    h,
                    n,
                    anchor_cells[si],
                    sz,
                    seed_cell,
                    deadline,
                    Some((sym_of, n_types)),
                );
                let empty = cands.is_empty();
                cache.insert(key, cands);
                if empty {
                    continue;
                }
            } else if cache[&key].is_empty() {
                continue;
            }
            hostable[si].push(sz);
        }
    }
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: hostable lists: {:?}", hostable);
    }
    // DFS over residues: for each residue seed, assign sizes to the others.
    for res_seed in 0..m {
        if crate::clock::Instant::now() >= deadline || out.len() >= ATTEMPT_CAP {
            break;
        }
        let mut order: Vec<usize> = (0..m).filter(|&i| i != res_seed).collect();
        // most-constrained seeds first (fewest hostable sizes)
        order.sort_by_key(|&i| hostable[i].len());
        let mut cur: Vec<usize> = vec![0; m];
        let mut used_count: Vec<usize> = vec![0; 9]; // per-size region counts (shape cap)
        let mut sum = 0usize;
        let mut evals = 0usize;
        let mut tail_used = false;
        assign_dfs(
            &order,
            0,
            &hostable,
            &mut cur,
            &mut used_count,
            &mut sum,
            total,
            m,
            anchor_cells,
            res_seed,
            defer_cap,
            shape_limited,
            &mut out,
            cache,
            deadline,
            &mut evals,
            min_sz,
            reach,
            pair,
            &mut tail_used,
        );
    }
    out.sort_by_key(|(c, _, _, _)| *c);
    if std::env::var("COVER_DEBUG").is_ok() {
        let with_tail = out.iter().filter(|(_, _, _, t)| t.is_some()).count();
        let mut hist = std::collections::BTreeMap::new();
        let mut hist_min = std::collections::BTreeMap::new();
        for ((nm, ex, _), _, _, _) in out.iter() {
            *hist.entry(*ex).or_insert(0usize) += 1;
            *hist_min.entry(usize::MAX - nm).or_insert(0usize) += 1;
        }
        eprintln!(
            "rose_cover: assign hostable={:?} attempts={} with_tail={} evals_sum={} excess_hist={:?} at_min_hist={:?}",
            hostable.iter().map(|v| v.len()).collect::<Vec<_>>(),
            out.len(),
            with_tail,
            EVALS_DBG.with(|e| e.get()),
            hist,
            hist_min
        );
        for (ci, (cost, rs, seeds, tail)) in out.iter().take(8).enumerate() {
            let sizes: Vec<usize> = seeds.iter().map(|&(_, s)| s).collect();
            eprintln!(
                "rose_cover: top[{ci}] key={cost:?} res_seed={rs} sizes={sizes:?} tail={tail:?}"
            );
        }
    }
    out
}

#[cfg(test)]
thread_local! {
    static EVALS_DBG: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(not(test))]
thread_local! {
    static EVALS_DBG: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Leaf assembly for the size DFS: residue check, residue-is-max
/// normalization, pairing match, cost, pairwise screen, push.
#[allow(clippy::too_many_arguments)]
fn emit_attempt_leaf(
    cur: &[usize],
    m: usize,
    anchor_cells: &[usize],
    res_seed: usize,
    total: usize,
    min_sz: usize,
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    pair: Option<&PairLb>,
    out: &mut Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)>,
    order: &[usize],
    defer_cap: usize,
) {
    let res_sz = total.saturating_sub(cur.iter().sum::<usize>());
    if res_sz < min_sz {
        if std::env::var("COVER_DEBUG").is_ok() {
            eprintln!("emit: reject res_sz={res_sz} < min={min_sz} cur={cur:?}");
        }
        return;
    }
    let mut bounds: Vec<usize> = vec![0; m];
    for i in 0..m {
        bounds[i] = if i == res_seed { res_sz } else { cur[i] };
    }
    // Normalization: the residue is a largest region.  Every size multiset
    // has a decomposition with its maximum as the leftover, so dropping
    // residue-smaller-than-tail/placed decompositions (the cheap noisy
    // family) keeps solutions reachable.
    if (0..m).any(|i| i != res_seed && cur[i] > res_sz) {
        if std::env::var("COVER_DEBUG").is_ok() {
            eprintln!("emit: reject residue-not-max res={res_sz} cur={cur:?}");
        }
        return;
    }
    // Pair matching: skip for the large class — the bounds are computed for
    // small regions and reject everything when regions are large.
    if defer_cap <= 10 {
        if let Some((lb, _)) = pair {
            if !pair_match_ok(lb, &bounds) {
                if std::env::var("COVER_DEBUG").is_ok() {
                    eprintln!("emit: reject pair_match bounds={bounds:?}");
                }
                return;
            }
        }
    }
    // assemble the attempt: seeds + tail + raw list cost
    let mut cost = 1usize;
    let mut seeds: Vec<(usize, usize)> = vec![(0, 0); m];
    seeds[res_seed] = (anchor_cells[res_seed], res_sz);
    let mut tail: Option<(usize, usize)> = None;
    for &i in order {
        let sz = cur[i];
        seeds[i] = (anchor_cells[i], sz);
        if sz > defer_cap {
            tail = Some((i, sz));
            cost = cost.saturating_mul(8);
        } else {
            let cl = cache.get(&(i, sz)).map(|v| v.len()).unwrap_or(0);
            if cl == 0 {
                if std::env::var("COVER_DEBUG").is_ok() {
                    eprintln!("emit: reject empty-cache seed={i} sz={sz}");
                }
                return;
            }
            cost = cost.saturating_mul(cl.max(1));
        }
    }
    let small: Vec<(usize, usize)> = (0..m)
        .filter(|&i| i != res_seed && Some((i, seeds[i].1)) != tail)
        .map(|i| (i, seeds[i].1))
        .collect();
    // Pairwise disjointness screen: for dense covers (large-class, regions
    // filling the board) independently-generated candidates always overlap,
    // so the screen rejects everything.  Skip it there and let the runner
    // find the actual disjoint cover.
    let pairwise_ok = if defer_cap > 10 {
        true
    } else {
        attempt_pairwise_ok(cache, &seeds, &small, false)
    };
    if pairwise_ok {
        let (max_ex, _) = pair
            .map(|(lb, _)| pair_min_max_excess(lb, &bounds))
            .unwrap_or((0, Vec::new()));
        // Floor islands (0289's three size-5s) lead the DFS list order.
        // For the large class, prefer "one big residue" patterns (the truth
        // is often unbalanced — 0975a: {61,20,15,13,12}).
        let at_min = bounds.iter().filter(|&&s| s == min_sz).count();
        let sort_key = if defer_cap > 10 {
            // Tighter pairing first (lower max_ex), then one-big residue.
            let max_other = bounds
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != res_seed)
                .map(|(_, &s)| s)
                .max()
                .unwrap_or(0);
            let residue_excess = res_sz.saturating_sub(max_other);
            (max_ex, usize::MAX - residue_excess, cost)
        } else {
            (usize::MAX - at_min, max_ex, cost)
        };
        out.push((sort_key, res_seed, seeds, tail));
    } else if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("emit: reject pairwise seeds={seeds:?}");
    }
}

#[allow(clippy::too_many_arguments)]
fn assign_dfs(
    order: &[usize],
    k: usize,
    hostable: &[Vec<usize>],
    cur: &mut [usize],
    used_count: &mut [usize],
    sum: &mut usize,
    total: usize,
    m: usize,
    anchor_cells: &[usize],
    res_seed: usize,
    defer_cap: usize,
    shape_limited: bool,
    out: &mut Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)>,
    cache: &std::collections::HashMap<(usize, usize), Vec<CoverCand>>,
    deadline: crate::clock::Instant,
    evals: &mut usize,
    min_sz: usize,
    reach: &[usize],
    pair: Option<&PairLb>,
    tail_used: &mut bool,
) {
    if out.len() >= 50_000 || crate::clock::Instant::now() >= deadline {
        return;
    }
    if k == order.len() {
        emit_attempt_leaf(
            cur,
            m,
            anchor_cells,
            res_seed,
            total,
            min_sz,
            cache,
            pair,
            out,
            order,
            defer_cap,
        );
        *evals += 1;
        EVALS_DBG.with(|e| e.set(e.get() + 1));
        return;
    }
    // Partial pairing feasibility: assigned regions carry their exact
    // size; unassigned ones are capped by the placement machinery (hostable
    // list sizes, or one deferred tail within the residue>=tail half-bound);
    // the residue keeps its reach ceiling.  If no injective partner
    // assignment survives, the whole subtree is dead.
    if let Some((lb, _)) = pair {
        let rest_min = (order.len() - k) * min_sz;
        let half = total.saturating_sub(*sum + rest_min) / 2;
        let ub = if *tail_used {
            defer_cap
        } else {
            defer_cap.max(half)
        };
        let mut bounds: Vec<usize> = Vec::with_capacity(m);
        for i in 0..m {
            if i != res_seed && order[..k].contains(&i) {
                bounds.push(cur[i]);
            } else if i == res_seed {
                bounds.push(reach[i]);
            } else {
                bounds.push(ub.min(reach[i]));
            }
        }
        if defer_cap <= 10 && !pair_match_ok(lb, &bounds) {
            if std::env::var("COVER_DEBUG").is_ok() {
                eprintln!("assign_dfs: pair_match prune k={k} bounds={bounds:?}");
            }
            return;
        }
    }
    let i = order[k];
    for &sz in &hostable[i] {
        if *sum + sz + (order.len() - k) * min_sz > total {
            continue; // not enough room for the rest + residue at min_sz
        }
        if shape_limited && sz < used_count.len() {
            if used_count[sz] >= [0, 1, 1, 2, 5, 12, 35, 108, 369][sz] {
                continue;
            }
            used_count[sz] += 1;
        }
        cur[i] = sz;
        *sum += sz;
        assign_dfs(
            order,
            k + 1,
            hostable,
            cur,
            used_count,
            sum,
            total,
            m,
            anchor_cells,
            res_seed,
            defer_cap,
            shape_limited,
            out,
            cache,
            deadline,
            evals,
            min_sz,
            reach,
            pair,
            tail_used,
        );
        *sum -= sz;
        if shape_limited && sz < used_count.len() {
            used_count[sz] -= 1;
        }
    }
    // One deferred tail size per attempt: no candidates are generated (the
    // runner's leftover BFS closes it), so only the reach ceiling bounds it.
    if !*tail_used {
        // Normalization keeps residue >= every other region, so the tail
        // cannot exceed half of what remains after the placed prefix.
        let rest_min = (order.len() - k - 1) * min_sz;
        let half = total.saturating_sub(*sum + rest_min) / 2;
        let tail_hi = reach[i]
            .min(total.saturating_sub((m - 1) * min_sz))
            .min(half);
        for sz in (defer_cap + 1)..=tail_hi {
            if *sum + sz + (order.len() - k) * min_sz > total {
                continue;
            }
            *tail_used = true;
            cur[i] = sz;
            *sum += sz;
            assign_dfs(
                order,
                k + 1,
                hostable,
                cur,
                used_count,
                sum,
                total,
                m,
                anchor_cells,
                res_seed,
                defer_cap,
                shape_limited,
                out,
                cache,
                deadline,
                evals,
                min_sz,
                reach,
                pair,
                tail_used,
            );
            *sum -= sz;
            *tail_used = false;
        }
    }
}


/// Rose multiset cover (0289 class): range min + rose cardinality bound the
/// size family (m parts >= g summing to total) and the symbol cells anchor
/// each region.  Tries the full-cover Algorithm X pass first (all-small
/// class), then the multiset/assignment attempts with the second-largest
/// deferred to the leftover BFS tail.  False-negative only - leaves pass the
/// full validator.
#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_rose_cover(
    puzzle: &Puzzle,
    w: usize,
    n: usize,
    anchor_cells: &[usize],
    min_sz: usize,
    max_sz: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    const DEFER_CAP: usize = 10;
    let h = puzzle.height;
    let m = anchor_cells.len();
    // Region-count cap.  12 covered the 0289/0804 class (m ≤ 9); 0838
    // (non_block + rose, m=14, forced multiset 7×4+7×3) sat just above it and
    // fell to region_match's single-candidate pipeline.  24 keeps the
    // all-small enumeration (DEFER_CAP parts) meaningful while bounding the
    // size-multiset fan-out.
    if m < 2 || m > 24 {
        return None;
    }
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: enter m={m} min={min_sz} max={max_sz}");
    }
    let (sym_of, n_types) = match rose_symbol_map(puzzle, h, w) {
        Some(v) => v,
        None => {
            if std::env::var("COVER_DEBUG").is_ok() {
                eprintln!("rose_cover: symbol map empty");
            }
            return None;
        }
    };
    let mut total = 0usize;
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if !puzzle.cells[r][c].blocked {
            total += 1;
        }
    }
    let shape_limited = puzzle.rules.iter().any(|r| r.ctype == "different");
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: total={total} n_types={n_types} shape_limited={shape_limited}");
    }
    let multisets = size_multisets(total, m, min_sz, max_sz, 2000, shape_limited);
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: {} multisets", multisets.len());
    }
    if multisets.is_empty() {
        return None;
    }
    let seed_cell: std::collections::HashSet<usize> = anchor_cells.iter().copied().collect();
    let t_enter = crate::clock::Instant::now();
    let pair = pair_lb_matrix(puzzle, w, h, anchor_cells, &sym_of);
    let reach = reach_upper(puzzle, w, h, anchor_cells);
    let mut cache: std::collections::HashMap<(usize, usize), Vec<CoverCand>> = Default::default();
    // The all-small space gets its own bounded enumeration (the wide
    // lexicographic cap 2000 cuts off before balanced parts): when empty the
    // board is not an all-small or mixed cover instance (0833) and running
    // the attempts burn would starve region_match.
    let small_multisets = size_multisets(total, m, min_sz, DEFER_CAP, 2000, shape_limited);
    let all_small_max = small_multisets
        .iter()
        .map(|p| p.last().copied().unwrap_or(0))
        .max();
    // Enum cap: the small class uses DEFER_CAP (all regions ≤ 10 cells); the
    // large class (all regions > DEFER_CAP, e.g. 0975a avg=24) needs the wide
    // multisets' max part so `gen_cover_cands` produces exact-size candidates
    // the BFS misses.
    let enum_cap: usize = match all_small_max {
        Some(_) => DEFER_CAP,
        None => {
            let wide_max = multisets
                .iter()
                .map(|p| p.last().copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            if wide_max < min_sz {
                return None;
            }
            wide_max.min(max_sz)
        }
    };
    let diff_mode = puzzle.rules.iter().any(|r| r.ctype == "differentiation");
    // Full-cover whenever an all-small space exists (0317/0325 — the truth
    // lives there even when the wide space is mixed).  0289/0804 mixed-class
    // boards pay the pass then fall to the assignment attempts; 0833-class
    // boards are gated out upstream by the avg check in `try_rose_cover`.
    if !small_multisets.is_empty() {
        if let Some(full_bound) = all_small_max {
            // Split the cover budget: full-cover gets the first half, the
            // assignment DFS the second (a full-cover burn must not zero the
            // attempts channel's deadline).
            let mid = crate::clock::Instant::now()
                + deadline
                    .saturating_duration_since(crate::clock::Instant::now())
                    / 2;
            if let Some(regions) = full_cover_pass(
                puzzle,
                w,
                h,
                n,
                anchor_cells,
                min_sz,
                full_bound,
                &sym_of,
                n_types,
                &seed_cell,
                &mut cache,
                mid,
            ) {
                return Some(regions);
            }
        }
    }
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: phase full-cover+gen {:?}", t_enter.elapsed());
    }
    let t_enum = crate::clock::Instant::now();
    // The assignment enumeration must never starve the runner: give it at
    // most a third of the remaining cover budget (0804's m=9 tree walked
    // straight into the deadline and the runner started at zero).  The large
    // class (enum_cap > DEFER_CAP) needs more time — `gen_cover_cands` for
    // size 20+ is much slower than for size ≤ 10.
    let enum_deadline = {
        let now = crate::clock::Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        let (frac, cap_ms) = if enum_cap > DEFER_CAP {
            (2u32, 3000u64) // large class: up to half the budget, cap 3s
        } else {
            (3u32, 500u64) // small class: one third, cap 500ms
        };
        let share = now + remaining / frac;
        let cap = now + std::time::Duration::from_millis(cap_ms);
        let t = if share < cap { share } else { cap };
        if t < deadline {
            t
        } else {
            deadline
        }
    };
    let attempts = enumerate_assignments(
        puzzle,
        w,
        h,
        n,
        anchor_cells,
        &sym_of,
        n_types,
        &seed_cell,
        &mut cache,
        min_sz,
        enum_cap,
        shape_limited,
        enum_deadline,
        pair.as_ref(),
        &reach,
    );
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!(
            "rose_cover: {} multisets {} attempts",
            multisets.len(),
            attempts.len()
        );
    }
    rose_cover_truth_report(puzzle, w, n, &anchor_cells, &seed_cell, &attempts);
    if std::env::var("COVER_DEBUG").is_ok() {
        eprintln!("rose_cover: phase enum {:?}", t_enum.elapsed());
        eprintln!("rose_cover: before runner, deadline={:?} now={:?}", deadline, crate::clock::Instant::now());
    }
    run_rose_cover_phases(
        puzzle, w, h, n, total, min_sz, &attempts, &cache, &sym_of, n_types,
        diff_mode, deadline, pair.as_ref(), &anchor_cells,
    )
}

/// Cheap sum-lock detector (no side effects): true when the number cells
/// satisfy either `inject_number_clues` lock form.
fn number_sum_lock(puzzle: &Puzzle, w: usize, n: usize) -> bool {
    let mut all_sum = 0usize;
    let mut per_value: std::collections::HashSet<usize> = Default::default();
    let mut distinct_sum = 0usize;
    let mut free = 0usize;
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if puzzle.cells[r][c].blocked {
            continue;
        }
        free += 1;
        if let Some(v) = puzzle.cells[r][c].number {
            let v = v.max(1) as usize;
            all_sum += v;
            if per_value.insert(v) {
                distinct_sum += v;
            }
        }
    }
    free > 0 && (all_sum == free || distinct_sum == free)
}

fn solve_range_core(
    puzzle: &Puzzle,
    n: usize,
    deadline: crate::clock::Instant,
    slice_ms: u64,
) -> Option<Vec<crate::types::RegionInfo>> {
    let w = puzzle.width;
    let pin_of: Vec<Option<usize>> = vec![None; n];
    let mut free_index: std::collections::HashMap<usize, u32> =
        std::collections::HashMap::new();
    let mut free_cells: Vec<usize> = Vec::new();
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if !puzzle.cells[r][c].blocked {
            free_index.insert(idx, free_cells.len() as u32);
            free_cells.push(idx);
        }
    }
    let (lo, hi) = crate::shapes::area_bounds(puzzle);
    // Watchtower cardinalities ride along (0985 class: range + watchtower):
    // pairwise must-same merges go through the UF in `build_free_units`, the
    // pairwise must-split pairs land in the split set.
    let facts_raw = watchtower_facts(puzzle);
    let (wt_same, wt_split) = match wt_free_relations(&facts_raw, &pin_of, &free_index) {
        Some(v) => v,
        None => return None,
    };
    let (unit_cells, unit_of_cell, unit_adj, split) = build_free_units(
        &free_cells,
        &free_index,
        &wt_same,
        &wt_split,
        puzzle,
        w,
    )?;
    let pinned: Vec<PinnedPlacement> = Vec::new();
    let (ord, diff, mut ext_lo, mut ext_hi) =
        collect_size_orders(puzzle, w, &pinned, &pin_of, &free_index, &unit_of_cell)?;
    let total = free_cells.len();
    let mut max_labels = if lo >= 1 { total / lo } else { 0 };
    // `area` number clues pin exact region sizes (0262 class: distinct-valued
    // numbers summing to the board also lock the region count).  Under that
    // lock every label size is known to lie in the number value set, so strict
    // ord chains admit Hall-interval pinning (`chain_value_pins`) and the
    // residual constraint units enumerate value assignments
    // (`enum_number_cases`).
    let num_lock =
        inject_number_clues(puzzle, &free_cells, &unit_of_cell, &mut ext_lo, &mut ext_hi, total, w);
    if let Some((cap, ref num_vals)) = num_lock {
        if cap < max_labels {
            max_labels = cap;
        }
        chain_value_pins(&ord, &mut ext_lo, &mut ext_hi, num_vals);
    }
    // Rose cardinality (0839 class): m regions, one per symbol occurrence —
    // the symbol cells anchor region identities for the aliasing cases.
    let rose_lock: Option<(usize, Vec<u32>)> =
        rose_region_count(puzzle, w).and_then(|(m, cells)| {
            let mut anchors: Vec<u32> = cells
                .iter()
                .map(|&idx| unit_of_cell[free_index[&idx] as usize])
                .collect();
            anchors.sort_unstable();
            anchors.dedup();
            if anchors.len() == m {
                if m < max_labels {
                    max_labels = m;
                }
                Some((m, anchors))
            } else {
                None
            }
        });
    relax_size_windows(&ord, &diff, &mut ext_lo, &mut ext_hi);
    // Distinct-size chain deduction (0152 class): a directed ord chain of c
    // units forces c distinct sizes summing to ≥ c·g+c(c−1)/2 — when that
    // floor saturates `total` every size is pinned exactly and the label
    // count drops to the chain length.
    if let Some(cap) = chain_size_deduction(&ord, &mut ext_lo, &mut ext_hi, total, lo) {
        if cap < max_labels {
            max_labels = cap;
        }
    } else {
        return None;
    }
    if max_labels == 0 {
        return None;
    }
    let unit_types: Vec<u64> = vec![0u64; unit_cells.len()];
    let sz_lo: Vec<usize> = (0..unit_cells.len()).map(|i| lo.max(ext_lo[i])).collect();
    let sz_hi: Vec<usize> = (0..unit_cells.len()).map(|i| hi.min(ext_hi[i])).collect();
    let (facts, facts_of_unit) =
        build_wt_unit_facts(&facts_raw, &pin_of, &free_index, &unit_of_cell, unit_cells.len());
    // Bounded slice: the range/inequality/difference gate fires on many FAILs
    // whose searches never converge — without the cap range-part burns the
    // full unit budget per puzzle and starves the later modules (1146 at
    // -j 6).  1351 solves in <5 s, so 12 s is a wide margin.
    let slice = crate::clock::Instant::now() + std::time::Duration::from_millis(slice_ms);
    let module_deadline = if slice < deadline { slice } else { deadline };
    // Size/value cases first (0929 size-tuple class / 0262 number-value
    // class): the true case converges in ~20 nodes and wrong cases die at
    // the root, so a tiny per-case slice bounds the tail; the un-hinted
    // plain run keeps the residual budget (0270 class).
    let runs = select_case_runs(
        puzzle,
        &ord,
        &diff,
        &sz_lo,
        &sz_hi,
        total,
        lo,
        max_labels,
        num_lock.as_ref().map(|(_, vals)| (vals.as_slice(), &split)),
        rose_lock.as_ref().map(|(m, anchors)| (*m, anchors)),
        &split,
        &unit_cells,
        &unit_adj,
        w,
    );
    run_range_searches(
        puzzle,
        &pinned,
        w,
        &unit_cells,
        &unit_adj,
        &split,
        &facts,
        &facts_of_unit,
        &ord,
        &diff,
        &sz_lo,
        &sz_hi,
        &unit_types,
        runs,
        module_deadline,
        num_lock.is_some(),
    )
}

/// Drive the FreeRem windowed runs: exact-window case runs first (sliced),
/// the plain un-hinted run last with the residual deadline.
fn run_range_searches(
    puzzle: &Puzzle,
    pinned: &[PinnedPlacement],
    w: usize,
    unit_cells: &Vec<Vec<usize>>,
    unit_adj: &Vec<Vec<u32>>,
    split: &std::collections::HashSet<(u32, u32)>,
    facts: &Vec<WtUnitFact>,
    facts_of_unit: &Vec<Vec<usize>>,
    ord: &Vec<(u32, u32)>,
    diff: &Vec<(u32, u32, usize)>,
    sz_lo: &Vec<usize>,
    sz_hi: &Vec<usize>,
    unit_types: &Vec<u64>,
    mut runs: Vec<(Vec<usize>, Vec<usize>, usize)>,
    module_deadline: crate::clock::Instant,
    number_locked: bool,
) -> Option<Vec<crate::types::RegionInfo>> {
    let n_cases = runs.len().saturating_sub(1);
    // Under the number lock the case set is complete (every solution lies in
    // some case), so cases get the whole module budget — no plain reserve.
    // Size-tuple cases (0929 class) may miss aliasings, so they keep the 6 s
    // cap and leave the plain fallback its reserve (0206 class).
    let case_deadline = if number_locked {
        module_deadline
    } else {
        let t = crate::clock::Instant::now() + std::time::Duration::from_millis(6_000);
        if t < module_deadline {
            t
        } else {
            module_deadline
        }
    };
    for (i, (case_lo, case_hi, case_max)) in runs.drain(..).enumerate() {
        let is_case = i < n_cases;
        if is_case && crate::clock::Instant::now() >= case_deadline {
            continue; // case budget spent — fall through to the plain run
        }
        if crate::clock::Instant::now() >= module_deadline {
            break;
        }
        let (slo, shi) = merge_case_windows(&case_lo, &case_hi, sz_lo, sz_hi);
        let mut search = FreeRem {
            puzzle,
            pinned,
            w,
            unit_cells,
            unit_adj,
            split,
            facts,
            facts_of_unit,
            ord,
            diff,
            sz_lo: &slo,
            sz_hi: &shi,
            unit_types,
            n_types: 0,
            max_labels: case_max,
            lab_of: vec![None; unit_cells.len()],
            lab_units: Vec::new(),
            win_cache: Vec::new(),
            deadline: run_deadline(is_case, module_deadline),
            nodes: 0,
        };
        if let Some(res) = search.search() {
            return Some(res);
        }
    }
    None
}

/// Rose-cardinality partition (0975a class): every region holds exactly one
/// unit of each symbol type, so the free labeling has emergent labels pinned
/// apart by the same-type must-split pairs and completed one type at a time
/// (`rose_step`).  The ring frame chain collapses the rim into a single unit
/// (`ring_frame_runs`).  No pins — the whole board is the residue.
/// WIP: routing not wired yet (see `rose/mod.rs`) — exercised by tests only.
#[allow(dead_code)]
pub(crate) fn solve_cardinal_partition(
    puzzle: &Puzzle,
    symbol_types: &[String],
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let w = puzzle.width;
    if symbol_types.is_empty() {
        return None;
    }
    // Per-cell symbol type index.
    let mut type_of_cell: Vec<Option<usize>> = vec![None; n];
    for r in 0..puzzle.height {
        for c in 0..w {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                    type_of_cell[r * w + c] = Some(ti);
                }
            }
        }
    }
    let mut free_index: std::collections::HashMap<usize, u32> =
        std::collections::HashMap::new();
    let mut free_cells: Vec<usize> = Vec::new();
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if !puzzle.cells[r][c].blocked {
            free_index.insert(idx, free_cells.len() as u32);
            free_cells.push(idx);
        }
    }
    // must-split: same-type symbol pairs (two equal symbols never share a
    // region — each region holds exactly one of each type).
    let mut split_pairs: Vec<(u32, u32)> = Vec::new();
    for i in 0..free_cells.len() {
        for j in (i + 1)..free_cells.len() {
            let (a, b) = (free_cells[i], free_cells[j]);
            match (type_of_cell[a], type_of_cell[b]) {
                (Some(ta), Some(tb)) if ta == tb => {
                    split_pairs.push((i as u32, j as u32));
                }
                _ => {}
            }
        }
    }
    // must-same: the ring frame chain (rim vertices forbid the wall between
    // two rim cells — the whole clean run is one region).
    let mut same: Vec<(u32, u32)> = Vec::new();
    for run in crate::solver::same_tiling::ring_frame_runs(puzzle) {
        for pair in run.windows(2) {
            let (a, b) = (pair[0].0 * w + pair[0].1, pair[1].0 * w + pair[1].1);
            if let (Some(&ia), Some(&ib)) = (free_index.get(&a), free_index.get(&b)) {
                same.push((ia, ib));
            }
        }
    }
    // Watchtower cardinalities ride along (1406 class: rose + watchtower) —
    // pairwise must-same/split from the vertex counts feed the UF like every
    // other FreeRem host.
    let pin_of: Vec<Option<usize>> = vec![None; n];
    let facts_raw = watchtower_facts(puzzle);
    let (wt_same, wt_split) = match wt_free_relations(&facts_raw, &pin_of, &free_index) {
        Some(v) => v,
        None => return None,
    };
    let mut same = same;
    same.extend(wt_same);
    let mut split_pairs = split_pairs;
    split_pairs.extend(wt_split);
    let (unit_cells, unit_of_cell, unit_adj, split) =
        match build_free_units(&free_cells, &free_index, &same, &split_pairs, puzzle, w) {
            Some(v) => v,
            None => {
                return None;
            }
        };
    // Per-unit symbol-type bitmask; a unit carrying two of one type is dead.
    let n_types = symbol_types.len();
    // The region (= label) count is the symbols-per-type count (each region
    // holds exactly one of each type) — NOT the type count.
    let max_labels = symbol_types
        .iter()
        .map(|s| {
            (0..puzzle.height)
                .flat_map(|r| (0..w).map(move |c| (r, c)))
                .filter(|&(r, c)| puzzle.cells[r][c].symbol.as_deref() == Some(s.as_str()))
                .count()
        })
        .min()
        .unwrap_or(0)
        .max(1);
    let mut unit_types: Vec<u64> = vec![0u64; unit_cells.len()];
    for (i, &idx) in free_cells.iter().enumerate() {
        if let Some(t) = type_of_cell[idx] {
            let u = unit_of_cell[i] as usize;
            let bit = 1u64 << t;
            if unit_types[u] & bit != 0 {
                return None;
            }
            unit_types[u] |= bit;
        }
    }
    let ord: Vec<(u32, u32)> = Vec::new();
    let diff: Vec<(u32, u32, usize)> = Vec::new();
    let (alo, ahi) = crate::shapes::area_bounds(puzzle);
    let sz_lo: Vec<usize> = vec![alo; unit_cells.len()];
    let sz_hi: Vec<usize> = vec![ahi; unit_cells.len()];
    let (facts, facts_of_unit) =
        build_wt_unit_facts(&facts_raw, &pin_of, &free_index, &unit_of_cell, unit_cells.len());
    let pinned: Vec<PinnedPlacement> = Vec::new();
    let slice = crate::clock::Instant::now() + std::time::Duration::from_millis(8_000);
    let leaf_deadline = if slice < deadline { slice } else { deadline };
    let mut search = FreeRem {
        puzzle,
        pinned: &pinned,
        w,
        unit_cells: &unit_cells,
        unit_adj: &unit_adj,
        split: &split,
        facts: &facts,
        facts_of_unit: &facts_of_unit,
        ord: &ord,
        diff: &diff,
        sz_lo: &sz_lo,
        sz_hi: &sz_hi,
        unit_types: &unit_types,
        n_types,
        max_labels,
        lab_of: vec![None; unit_cells.len()],
        lab_units: Vec::new(),
        win_cache: Vec::new(),
        deadline: leaf_deadline,
        nodes: 0,
    };
    let out = search.search();
    out
}

fn solve_multi_remainder(
    puzzle: &Puzzle,
    pinned: &[PinnedPlacement],
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let w = puzzle.width;
    let mut pin_of: Vec<Option<usize>> = vec![None; n];
    for (ri, p) in pinned.iter().enumerate() {
        for idx in p.cells.iter() {
            pin_of[idx] = Some(ri);
        }
    }
    let mut free_cells: Vec<usize> = Vec::new();
    for idx in 0..n {
        let (r, c) = (idx / w, idx % w);
        if !puzzle.cells[r][c].blocked && pin_of[idx].is_none() {
            free_cells.push(idx);
        }
    }
    if free_cells.is_empty() {
        return None;
    }
    // Cell → free-cell ordinal for the union-find.
    let mut free_index: std::collections::HashMap<usize, u32> =
        std::collections::HashMap::new();
    for (i, &idx) in free_cells.iter().enumerate() {
        free_index.insert(idx, i as u32);
    }
    let facts_raw = watchtower_facts(puzzle);
    // v1 scope: the free-partition search needs a *driving* constraint —
    // watchtower cardinalities (0994 class: they force the singletons /
    // must-same corridors) or inequality edges (0899 class: directed size
    // order across pre-drawn walls; the potential-connect closure then
    // derives the enclosed singletons).  Truly constraint-free residues
    // (1435: 59 units of `mixed`) degenerate into a Bell-number walk and
    // every wrong pin leaf of 1215 would burn its slice here instead of
    // falling through to the next leaf.
    let has_inequality = puzzle.rules.iter().any(|r| r.ctype == "inequality");
    if facts_raw.is_empty() && !has_inequality {
        return None;
    }
    let (same, split_pairs) = wt_free_relations(&facts_raw, &pin_of, &free_index)?;


    let (unit_cells, unit_of_cell, unit_adj, split) =
        build_free_units(&free_cells, &free_index, &same, &split_pairs, puzzle, w)?;
    let (ord, diff, sz_lo, sz_hi) =
        collect_size_orders(puzzle, w, pinned, &pin_of, &free_index, &unit_of_cell)?;
    let unit_types: Vec<u64> = vec![0u64; unit_cells.len()];
    let n_types = 0usize;
    let max_labels = 0usize;
    // Order pairs are also must-differ pairs.
    let mut split = split;
    for &(ua, ub) in ord.iter() {
        split.insert(canon_pair(ua, ub));
    }

    // Recast the WT facts over (pin, unit) slots.
    let mut facts: Vec<WtUnitFact> = Vec::new();
    let mut facts_of_unit: Vec<Vec<usize>> = vec![Vec::new(); unit_cells.len()];
    for (cells, value) in &facts_raw {
        let mut slots = Vec::new();
        for &idx in cells {
            match pin_of[idx] {
                Some(p) => slots.push(WtSlot::Pin(p)),
                None => {
                    let u = unit_of_cell[free_index[&idx] as usize];
                    slots.push(WtSlot::Unit(u));
                    if !facts_of_unit[u as usize].contains(&(facts.len())) {
                        facts_of_unit[u as usize].push(facts.len());
                    }
                }
            }
        }
        facts.push(WtUnitFact { slots, value: *value });
    }

    if crate::aog_debug_enabled() {
        eprintln!(
            "pp-pin: multi-remainder: {} free cells, {} units, {} wt facts, {} split pairs",
            free_cells.len(),
            unit_cells.len(),
            facts.len(),
            split.len()
        );
    }
    // Per-attempt slice: a wrong pin leaf may demand a long unsat proof while
    // the true leaf's labeling solves in tens of ms (same lesson as the
    // compass remainder — 1093's 4.4 s walks starved the true leaf).
    let slice = crate::clock::Instant::now() + std::time::Duration::from_millis(250);
    let leaf_deadline = if slice < deadline { slice } else { deadline };
    let mut search = FreeRem {
        puzzle,
        pinned,
        w,
        unit_cells: &unit_cells,
        unit_adj: &unit_adj,
        split: &split,
        facts: &facts,
        facts_of_unit: &facts_of_unit,
        ord: &ord,
        diff: &diff,
        sz_lo: &sz_lo,
        sz_hi: &sz_hi,
        unit_types: &unit_types,
        n_types,
        max_labels,
        lab_of: vec![None; unit_cells.len()],
        lab_units: Vec::new(),
        win_cache: Vec::new(),
        deadline: leaf_deadline,
        nodes: 0,
    };
    search.search()
}

#[cfg(test)]
mod multi_rem_tests {
    use super::*;

    /// Isolation anchor: with 0994's **official** pin assignment the free
    /// partition search must reconstruct the wrap + 4 WT-forced singletons.
    #[test]
    fn multi_remainder_official_pins_0994() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/3-vertex-radar/0994.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone3-answer/3-vertex-radar/0994.json"
        ))
        .expect("parse answer");
        let w = p.width;
        let mut pinned: Vec<PinnedPlacement> = Vec::new();
        for region in ans["regions"].as_array().unwrap() {
            let cells: Vec<(usize, usize)> = region
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    (
                        c[0].as_u64().unwrap() as usize,
                        c[1].as_u64().unwrap() as usize,
                    )
                })
                .collect();
            let anchor = cells
                .iter()
                .copied()
                .find(|&(r, c)| p.cells[r][c].shape_pattern.is_some());
            if let Some((ar, ac)) = anchor {
                let mut set = CellSet::new(p.height * w);
                for &(r, c) in &cells {
                    set.insert(r * w + c);
                }
                pinned.push(PinnedPlacement {
                    anchor: ar * w + ac,
                    cells: set,
                });
            }
        }
        assert_eq!(pinned.len(), 10, "0994 has 10 pattern regions");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(20);
        let out = solve_multi_remainder(&p, &pinned, p.height * w, deadline);
        assert!(out.is_some(), "free partition with official pins must solve");
    }

    /// End-to-end: the standalone pre-pin must solve 0994 (multi-remainder).
    #[test]
    fn solves_multi_remainder_0994() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/3-vertex-radar/0994.json"
        ))
        .expect("parse");
        let out = solve_puzzle_piece_standalone(&p, 30_000);
        assert!(out.is_solved(), "0994 expected solved, got {:?}", out);
    }

    /// Diagnostic: the official pattern placements must survive enumeration.
    #[test]
    fn official_placements_enumerated_1093() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/6-compass-main/1093.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone3-answer/6-compass-main/1093.json"
        ))
        .expect("parse answer");
        let w = p.width;
        let anchors = enumerate_pin_candidates(&p, &[]).expect("anchors");
        for region in ans["regions"].as_array().unwrap() {
            let cells: Vec<(usize, usize)> = region
                .as_array()
                .unwrap()
                .iter()
                .map(|c| (c[0].as_u64().unwrap() as usize, c[1].as_u64().unwrap() as usize))
                .collect();
            let Some(&(ar, ac)) = cells
                .iter()
                .find(|&&(r, c)| p.cells[r][c].shape_pattern.is_some())
            else {
                continue;
            };
            let mut set = CellSet::new(p.height * w);
            for &(r, c) in &cells {
                set.insert(r * w + c);
            }
            let found = anchors.iter().any(|a| {
                a.anchor == ar * w + ac
                    && a.placements.iter().any(|pl| same_set(&pl.cells, &set))
            });
            assert!(found, "official region at ({},{}) must be enumerated", ar, ac);
        }
    }

    /// Isolation anchor for the compass remainder path.
    #[test]
    fn compass_remainder_official_pins_1093() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/6-compass-main/1093.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone3-answer/6-compass-main/1093.json"
        ))
        .expect("parse answer");
        let w = p.width;
        let mut pinned: Vec<PinnedPlacement> = Vec::new();
        for region in ans["regions"].as_array().unwrap() {
            let cells: Vec<(usize, usize)> = region
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    (
                        c[0].as_u64().unwrap() as usize,
                        c[1].as_u64().unwrap() as usize,
                    )
                })
                .collect();
            let anchor = cells
                .iter()
                .copied()
                .find(|&(r, c)| p.cells[r][c].shape_pattern.is_some());
            if let Some((ar, ac)) = anchor {
                let mut set = CellSet::new(p.height * w);
                for &(r, c) in &cells {
                    set.insert(r * w + c);
                }
                pinned.push(PinnedPlacement {
                    anchor: ar * w + ac,
                    cells: set,
                });
            }
        }
        assert_eq!(pinned.len(), 6, "1093 has 6 pattern regions");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(25);
        let out = solve_compass_remainder(&p, &pinned, p.height * w, deadline);
        assert!(out.is_some(), "compass remainder with official pins must solve");
    }

    /// TEMP diagnostic for the compass residual cluster (run manually).
    #[test]
    #[ignore]
    fn tmp_diag_0683() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/6-compass-main/0683.json"
        ))
        .expect("parse");
        let out = crate::solver::compass_part::solve_compass_part(&p, 60_000);
        eprintln!("0683 SOLVED={}", out.is_solved());
    }

    /// TEMP diagnostic for the compass residual cluster (run manually).
    #[test]
    #[ignore]
    fn tmp_diag_1258() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/6-compass-main/1258.json"
        ))
        .expect("parse");
        let out = crate::solver::compass_part::solve_compass_part(&p, 60_000);
        eprintln!("1258 SOLVED={}", out.is_solved());
    }

    /// Isolation anchor for the inequality remainder path.
    #[test]
    fn ineq_remainder_official_pins_0899() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/5-inequality/0899.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone3-answer/5-inequality/0899.json"
        ))
        .expect("parse answer");
        let w = p.width;
        let mut pinned: Vec<PinnedPlacement> = Vec::new();
        for region in ans["regions"].as_array().unwrap() {
            let cells: Vec<(usize, usize)> = region
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    (
                        c[0].as_u64().unwrap() as usize,
                        c[1].as_u64().unwrap() as usize,
                    )
                })
                .collect();
            let anchor = cells
                .iter()
                .copied()
                .find(|&(r, c)| p.cells[r][c].shape_pattern.is_some());
            if let Some((ar, ac)) = anchor {
                let mut set = CellSet::new(p.height * w);
                for &(r, c) in &cells {
                    set.insert(r * w + c);
                }
                pinned.push(PinnedPlacement {
                    anchor: ar * w + ac,
                    cells: set,
                });
            }
        }
        assert_eq!(pinned.len(), 8, "0899 has 8 pattern regions");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(25);
        let out = solve_multi_remainder(&p, &pinned, p.height * w, deadline);
        assert!(out.is_some(), "free partition with official pins must solve");
    }

    /// Guided check: the official 0975a labeling must survive `leaf_regions`.
    #[test]
    fn cardinal_leaf_accepts_official_0975a() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/8-endgame/0975a.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone3-answer/8-endgame/0975a.json"
        ))
        .expect("parse answer");
        let w = p.width;
        let n = p.height * w;
        let regions: Vec<Vec<(usize, usize)>> = ans["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                r.as_array()
                    .unwrap()
                    .iter()
                    .map(|c| {
                        (
                            c[0].as_u64().unwrap() as usize,
                            c[1].as_u64().unwrap() as usize,
                        )
                    })
                    .collect()
            })
            .collect();
        let mut region_of: Vec<Option<usize>> = vec![None; n];
        for (i, r) in regions.iter().enumerate() {
            for &(rr, cc) in r {
                region_of[rr * w + cc] = Some(i);
            }
        }
        let built = crate::solver::rose::build_regions(&region_of, p.height, w);
        let ok = crate::solver::validate::validate(&p, &built);
        assert!(ok, "official 0975a labeling must pass validate");
    }

    /// 1351 (pure range, min=9): the area-bounded FreeRem must crack the
    /// emergent-count partition (35 cells, 3 regions [17,9,9]).
    #[test]
    fn solves_range_partition_1351() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone2/5-minmax/1351.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "1351 range partition must solve");
    }

    /// 0289 (range min=5 + rose 2 types) — WIP tracker (same endgame gap).
    #[test]
    #[ignore]
    fn solves_cardinal_0289() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone2/5-minmax/0289.json"
        ))
        .expect("parse");
        let types = crate::shapes::rose_symbol_types(&p);
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_cardinal_partition(&p, &types, p.height * p.width, deadline);
        assert!(out.is_some(), "0289 cardinal must solve");
    }

    /// 0975a (ring + rose_window) — WIP tracker: the leaf accepts the truth
    /// and the machinery is sound, but the search's endgame does not converge
    /// in budget (1.9M nodes / 120 s, no full assignment).  Run manually:
    /// `cargo test -- --ignored solves_cardinal`.
    #[test]
    #[ignore]
    fn solves_cardinal_partition_0975a() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/8-endgame/0975a.json"
        ))
        .expect("parse");
        let types = crate::shapes::rose_symbol_types(&p);
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(120);
        let out = solve_cardinal_partition(&p, &types, p.height * p.width, deadline);
        assert!(out.is_some(), "0975a cardinal partition must solve");
    }

    /// End-to-end: 0899 (`puzzle_piece + inequality`) — the free partition is
    /// driven by the directed size orders across the 8 pre-drawn walls plus
    /// the potential-connect closure (enclosed free cells force singletons).
    #[test]
    fn solves_ineq_remainder_0899() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/5-inequality/0899.json"
        ))
        .expect("parse");
        let out = solve_puzzle_piece_standalone(&p, 30_000);
        assert!(out.is_solved(), "0899 expected solved, got {:?}", out);
    }

    /// End-to-end: 1093 (`puzzle_piece + compass + solitary`) — solitary
    /// pins the free region count to the 7 compass clue cells, so the free
    /// partition delegates to `compass_label`.
    #[test]
    fn solves_compass_remainder_1093() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/6-compass-main/1093.json"
        ))
        .expect("parse");
        let out = solve_puzzle_piece_standalone(&p, 30_000);
        assert!(out.is_solved(), "1093 expected solved, got {:?}", out);
    }

    /// 0152 (pure inequality, 6×6): the 7 clue walls form one directed ord
    /// chain of 8 units whose distinct-size sum floor (1+…+8 = 36) saturates
    /// the grid — `chain_size_deduction` pins every region size exactly and
    /// caps the label count at 8.
    #[test]
    fn solves_range_partition_0152() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/5-inequality/0152.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "0152 chain deduction must solve");
    }

    /// 0270 (pure difference, 7×7, 13 regions): window-intersection joins +
    /// corridor forcing drive the endgame home (0.4 s).
    #[test]
    fn solves_range_partition_0270() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/4-difference/0270.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "0270 range partition must solve");
    }

    /// 0929 (non_block + difference, 5×6, 6 regions): the size-case
    /// enumeration pins the gap-chain's disjunctions (3.6 s).
    #[test]
    fn solves_range_partition_0929() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/4-difference/0929.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "0929 range partition must solve");
    }

    /// 0770 (pure range min=12, 9×10, 7 regions): endgame tracker.
    #[test]
    #[ignore]
    fn solves_range_partition_0770() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone2/5-minmax/0770.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "0770 range partition must solve");
    }

    /// Hall-interval pinning sanity (0262 comps A/D shape): a 4-chain over
    /// |V|=4 pins exactly; a 2-chain narrows to its interval; V-shapes pin
    /// nothing (not a total order).
    #[test]
    fn chain_value_pins_exact_on_full_chains() {
        // ord pairs are (smaller, larger); number units 10..13 hold V.
        let ord = vec![(0u32, 1), (1, 2), (2, 3), (4, 5), (5, 6), (6, 7), (8, 9)];
        let mut lo = vec![1usize; 10];
        let mut hi = vec![64usize; 10];
        let values = vec![(10u32, 14usize), (11, 15), (12, 17), (13, 18)];
        chain_value_pins(&ord, &mut lo, &mut hi, &values);
        assert_eq!((lo[0], hi[0]), (14, 14), "chain A member 0");
        assert_eq!((lo[1], hi[1]), (15, 15), "chain A member 1");
        assert_eq!((lo[2], hi[2]), (17, 17), "chain A member 2");
        assert_eq!((lo[3], hi[3]), (18, 18), "chain A member 3");
        assert_eq!((lo[8], hi[8]), (14, 17), "2-chain smaller end");
        assert_eq!((lo[9], hi[9]), (15, 18), "2-chain larger end");
        // V-shape (0<2, 1<2): two sources, not a chain — untouched.
        let ord_v = vec![(0u32, 2), (1, 2)];
        let mut lo2 = vec![1usize; 10];
        let mut hi2 = vec![64usize; 10];
        chain_value_pins(&ord_v, &mut lo2, &mut hi2, &values);
        assert_eq!((lo2[0], hi2[0]), (1, 64), "V-shape must not pin");
        assert_eq!((lo2[2], hi2[2]), (1, 64), "V-shape must not pin");
    }

    /// 0262 death-attribution dump.
    #[test]
    #[ignore]
    fn tmp_diag_0262() {
        dies_reset();
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/5-inequality/0262.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(300);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        eprintln!("tmp_diag_0262 solved={}", out.is_some());
        let mut rows: Vec<_> = dies_snapshot().into_iter().collect();
        rows.sort_by_key(|(k, _)| *k);
        for ((tag, bucket), n) in rows {
            eprintln!("die {tag:12} assigned~{:<3} x{}", bucket * 16, n);
        }
    }

    /// Gate-gap probe: 0171/0308/0742 carry `area` numbers but no
    /// range/precise/inequality/difference rule, so range-part never runs.
    /// `RANGE_FORCE=1` bypasses the gate and measures the number machinery.
    /// `cargo test -- --ignored tmp_diag_area_gate --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_area_gate() {
        std::env::set_var("RANGE_FORCE", "1");
        for (tag, src) in [
            ("0171", include_str!("../../../../puzzles/official/Zone2/6-same-size-no-touch/0171.json")),
            ("0308", include_str!("../../../../puzzles/official/Zone2/10-zone2-mixed/0308.json")),
            ("0742", include_str!("../../../../puzzles/official/Zone2/1-area-numbers/0742.json")),
        ] {
            let p = crate::io::parse_puzzle(src).expect("parse");
            let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
            let t0 = crate::clock::Instant::now();
            let out = solve_range_partition(&p, p.height * p.width, deadline);
            eprintln!(
                "tmp_diag_area_gate {tag}: solved={} in {:?}",
                out.is_some(),
                t0.elapsed()
            );
        }
        std::env::remove_var("RANGE_FORCE");
    }

    /// Completeness bisect for the full-cover search: candidate lists hold
    /// ONLY the official regions — if this fails the search control flow is
    /// broken; if it succeeds the issue is value ordering.
    /// `cargo test -- --ignored tmp_diag_0317_truth --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_0317_truth() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone2/4-all-regions-different/0317.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone2-answer/4-all-regions-different/0317.json"
        ))
        .expect("ans");
        let w = p.width;
        let h = p.height;
        let n = h * w;
        let mut sym_of: std::collections::HashMap<usize, usize> = Default::default();
        let mut anchors: Vec<usize> = Vec::new();
        for r in 0..h {
            for c in 0..w {
                if p.cells[r][c].symbol.is_some() {
                    sym_of.insert(r * w + c, 0);
                    anchors.push(r * w + c);
                }
            }
        }
        anchors.sort_unstable();
        let m = anchors.len();
        let mut regs: Vec<(usize, Mask)> = Vec::new();
        for region in ans["regions"].as_array().unwrap() {
            let mut cells: Vec<usize> = Vec::new();
            let mut seed = 0usize;
            for cell in region.as_array().unwrap() {
                let (r, c) = (
                    cell[0].as_u64().unwrap() as usize,
                    cell[1].as_u64().unwrap() as usize,
                );
                cells.push(r * w + c);
                if p.cells[r][c].symbol.is_some() {
                    seed = r * w + c;
                }
            }
            regs.push((seed, mask_of(&cells, n)));
        }
        // order regions by anchor index
        regs.sort_by_key(|(sd, _)| anchors.iter().position(|&a| a == *sd).unwrap());
        let seeds: Vec<(usize, usize)> = regs.iter().map(|&(sd, _)| (sd, 0)).collect();
        let mut cands: Vec<Vec<CoverCand>> = Vec::new();
        for (i, (sd, mask)) in regs.iter().enumerate() {
            cands.push(vec![CoverCand {
                cells: Vec::new(),
                mask: *mask,
                touch: dilate(mask, w, h, n),
            }]);
            let _ = (sd, i);
        }
        let mut remaining: Vec<usize> = (0..m).collect();
        let mut placed: Vec<(usize, Mask)> = Vec::new();
        let mut active: Vec<Vec<bool>> = cands.iter().map(|v| vec![true; v.len()]).collect();
        let mut nodes = 0usize;
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(5);
        let out = cover_dfs(
            &p, w, h, n, &mut remaining, &(0..m).collect::<Vec<_>>(), &cands, &seeds,
            0, 0, false, &mut placed, &mut active, &mut nodes, deadline,
            Some((&sym_of, 1)), None, true, 0,
        );
        eprintln!("tmp_diag_0317_truth: solved={out} nodes={nodes}");
    }

    /// Feed the 0289 official size assignment straight into the attempt
    /// runner: if this solves quickly the gap is ordering; if not, the
    /// cover value order is the gap.
    /// `cargo test -- --ignored tmp_diag_0289_truth --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_0289_truth() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone2/5-minmax/0289.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone2-answer/5-minmax/0289.json"
        ))
        .expect("ans");
        let w = p.width;
        let h = p.height;
        let n = h * w;
        let mut sym_of: std::collections::HashMap<usize, usize> = Default::default();
        for r in 0..h {
            for c in 0..w {
                if let Some(s) = p.cells[r][c].symbol.as_deref() {
                    sym_of.insert(r * w + c, if s == "P1" { 0 } else { 1 });
                }
            }
        }
        let mut anchors: Vec<usize> = sym_of
            .iter()
            .filter(|(_, &t)| t == 0)
            .map(|(&x, _)| x)
            .collect();
        anchors.sort_unstable();
        let m = anchors.len();
        // truth sizes per anchor
        let mut size_of: std::collections::HashMap<usize, usize> = Default::default();
        for region in ans["regions"].as_array().unwrap() {
            let cells = region.as_array().unwrap();
            let mut anchor_hit = None;
            for cell in cells {
                let (r, c) = (
                    cell[0].as_u64().unwrap() as usize,
                    cell[1].as_u64().unwrap() as usize,
                );
                let x = r * w + c;
                if anchors.contains(&x) {
                    anchor_hit = Some(x);
                }
            }
            size_of.insert(anchor_hit.unwrap(), cells.len());
        }
        let sizes: Vec<usize> = anchors.iter().map(|&a| size_of[&a]).collect();
        eprintln!("tmp_diag_0289_truth: anchors={anchors:?} sizes={sizes:?}");
        // residue = largest, tail = second-largest when above the cap
        let res_pos = sizes
            .iter()
            .enumerate()
            .max_by_key(|(_, &s)| s)
            .map(|(i, _)| i)
            .unwrap();
        let mut tail: Option<(usize, usize)> = None;
        for (i, &s) in sizes.iter().enumerate() {
            if i != res_pos && s > 10 {
                tail = Some((i, s));
            }
        }
        let seeds: Vec<(usize, usize)> = anchors
            .iter()
            .zip(sizes.iter())
            .map(|(&a, &s)| (a, s))
            .collect();
        let seed_cell: std::collections::HashSet<usize> = anchors.iter().copied().collect();
        let pair = pair_lb_matrix(&p, w, h, &anchors, &sym_of);
        eprintln!("tmp_diag_0289_truth: pair_some={} tail={tail:?}", pair.is_some());
        let mut cache: std::collections::HashMap<(usize, usize), Vec<CoverCand>> = Default::default();
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        for (i, &a) in anchors.iter().enumerate() {
            if Some((i, sizes[i])) == tail || i == res_pos {
                continue;
            }
            let list = gen_cover_cands(
                &p,
                w,
                h,
                n,
                a,
                sizes[i],
                &seed_cell,
                deadline,
                Some((&sym_of, 2)),
            );
            eprintln!("tmp_diag_0289_truth: list[{i}] size={} len={}", sizes[i], list.len());
            cache.insert((i, sizes[i]), list);
        }
        let attempts: Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)> =
            vec![((0, 0, 0), res_pos, seeds, tail)];
        let t0 = crate::clock::Instant::now();
        let out = run_cover_attempts(
            &p,
            w,
            h,
            n,
            &attempts,
            &cache,
            &sym_of,
            2,
            false,
            deadline,
            pair.as_ref(),
            &[],
        );
        eprintln!(
            "tmp_diag_0289_truth: solved={} in {:?}",
            out.is_some(),
            t0.elapsed()
        );
    }

    /// Feed the 0804 official size assignment into the attempt runner.
    /// `cargo test -- --ignored tmp_diag_0804_truth --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_0804_truth() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone1/7-slash-pack/0804.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone1-answer/7-slash-pack/0804.json"
        ))
        .expect("ans");
        let w = p.width;
        let h = p.height;
        let n = h * w;
        let mut sym_of: std::collections::HashMap<usize, usize> = Default::default();
        for r in 0..h {
            for c in 0..w {
                if let Some(s) = p.cells[r][c].symbol.as_deref() {
                    sym_of.insert(r * w + c, if s == "P1" { 0 } else { 1 });
                }
            }
        }
        let mut anchors: Vec<usize> = sym_of
            .iter()
            .filter(|(_, &t)| t == 0)
            .map(|(&x, _)| x)
            .collect();
        anchors.sort_unstable();
        let mut size_of: std::collections::HashMap<usize, usize> = Default::default();
        for region in ans["regions"].as_array().unwrap() {
            let cells = region.as_array().unwrap();
            let mut anchor_hit = None;
            for cell in cells {
                let (r, c) = (
                    cell[0].as_u64().unwrap() as usize,
                    cell[1].as_u64().unwrap() as usize,
                );
                let x = r * w + c;
                if anchors.contains(&x) {
                    anchor_hit = Some(x);
                }
            }
            size_of.insert(anchor_hit.unwrap(), cells.len());
        }
        let sizes: Vec<usize> = anchors.iter().map(|&a| size_of[&a]).collect();
        eprintln!("tmp_diag_0804_truth: anchors={anchors:?} sizes={sizes:?}");
        let res_pos = sizes
            .iter()
            .enumerate()
            .max_by_key(|(_, &s)| s)
            .map(|(i, _)| i)
            .unwrap();
        let seeds: Vec<(usize, usize)> = anchors
            .iter()
            .zip(sizes.iter())
            .map(|(&a, &s)| (a, s))
            .collect();
        let seed_cell: std::collections::HashSet<usize> = anchors.iter().copied().collect();
        let pair = pair_lb_matrix(&p, w, h, &anchors, &sym_of);
        eprintln!("tmp_diag_0804_truth: pair_some={} res_pos={res_pos}", pair.is_some());
        let mut cache: std::collections::HashMap<(usize, usize), Vec<CoverCand>> = Default::default();
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(60);
        for (i, &a) in anchors.iter().enumerate() {
            if i == res_pos {
                continue;
            }
            let t0 = crate::clock::Instant::now();
            let list = gen_cover_cands(
                &p,
                w,
                h,
                n,
                a,
                sizes[i],
                &seed_cell,
                deadline,
                Some((&sym_of, 2)),
            );
            eprintln!(
                "tmp_diag_0804_truth: list[{i}] size={} len={} gen={:?}",
                sizes[i],
                list.len(),
                t0.elapsed()
            );
            cache.insert((i, sizes[i]), list);
        }
        let attempts: Vec<((usize, usize, usize), usize, Vec<(usize, usize)>, Option<(usize, usize)>)> =
            vec![((0, 0, 0), res_pos, seeds, None)];
        let t0 = crate::clock::Instant::now();
        let out = run_cover_attempts(
            &p,
            w,
            h,
            n,
            &attempts,
            &cache,
            &sym_of,
            2,
            false,
            deadline,
            pair.as_ref(),
            &[],
        );
        eprintln!(
            "tmp_diag_0804_truth: solved={} in {:?}",
            out.is_some(),
            t0.elapsed()
        );
    }

    /// Cardinal host with wt facts on 1406 (rose P1/P2/P3 ×3 + watchtower).
    /// `cargo test -- --ignored tmp_diag_cardinal_1406 --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_cardinal_1406() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/7-zone3-mixed/1406.json"
        ))
        .expect("parse");
        let mut types: Vec<String> = Vec::new();
        for row in &p.cells {
            for c in row {
                if let Some(sym) = &c.symbol {
                    if !types.contains(sym) {
                        types.push(sym.clone());
                    }
                }
            }
        }
        types.sort();
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(60);
        let t0 = crate::clock::Instant::now();
        let out = solve_cardinal_partition(&p, &types, p.height * p.width, deadline);
        eprintln!(
            "tmp_diag_cardinal_1406: types={types:?} solved={} in {:?}",
            out.is_some(),
            t0.elapsed()
        );
    }

    /// Walk the 0308 official placement through the cover-search checks to
    /// prove the pruning never rejects the truth.
    /// `cargo test -- --ignored tmp_diag_0308_guided --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_0308_guided() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone2/10-zone2-mixed/0308.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone2-answer/10-zone2-mixed/0308.json"
        ))
        .expect("answer");
        let h = p.height;
        let w = p.width;
        let n = h * w;
        // answer regions as masks (skip the 84-residue = largest)
        let mut regs: Vec<(Mask, usize, (usize, usize))> = Vec::new(); // (mask, size, seed)
        let mut seed_of: std::collections::HashMap<(usize, usize), i64> = Default::default();
        for row in &p.cells {
            for c in row {
                if let Some(v) = c.number {
                    seed_of.insert((c.row, c.col), v);
                }
            }
        }
        for region in ans["regions"].as_array().unwrap() {
            let mut cells: Vec<usize> = Vec::new();
            let mut seed = (0usize, 0usize);
            for cell in region.as_array().unwrap() {
                let (r, c) = (
                    cell[0].as_u64().unwrap() as usize,
                    cell[1].as_u64().unwrap() as usize,
                );
                cells.push(r * w + c);
                if let Some(&v) = seed_of.get(&(r, c)) {
                    seed = (r * w + c, v as usize);
                }
            }
            let mask = mask_of(&cells, n);
            regs.push((mask, cells.len(), seed));
        }
        let res_i = regs
            .iter()
            .enumerate()
            .max_by_key(|(_, &(_, sz, _))| sz)
            .map(|(i, _)| i)
            .unwrap();
        let mut placed: Vec<(usize, Mask)> = Vec::new();
        for (i, (mask, sz, seed)) in regs.iter().enumerate() {
            if i == res_i {
                continue;
            }
            let cand = CoverCand {
                cells: Vec::new(),
                mask: *mask,
                touch: dilate(mask, w, h, n),
            };
            let mut seeds_vec = vec![(0usize, 0usize); regs.len()];
            seeds_vec[i] = *seed;
            let order = [i];
            let fits = cand_fits(&seeds_vec, &order, &placed, true, 0, &cand);
            let roomy = res_roomy(&p, w, h, n, &placed, regs[res_i].2 .0, regs[res_i].1, &cand);
            eprintln!(
                "guided region {i} size={sz} fits={fits} roomy={roomy}"
            );
            if !fits || !roomy {
                panic!("truth rejected at region {i}");
            }
            placed.push((i, *mask));
        }
        eprintln!("guided walk OK");
    }

    /// Cardinal-host probe on the rose-family FAILs (single-type rose now
    /// allowed: 0317 class).
    /// `cargo test -- --ignored tmp_diag_cardinal --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_cardinal() {
        for (tag, src) in [
            ("1406", include_str!("../../../../puzzles/official/Zone3/7-zone3-mixed/1406.json")),
            ("1067", include_str!("../../../../puzzles/official/Zone3/6-compass-main/1067.json")),
            ("1402", include_str!("../../../../puzzles/official/Zone3/6-compass-main/1402.json")),
            ("0804", include_str!("../../../../puzzles/official/Zone1/7-slash-pack/0804.json")),
            ("0317", include_str!("../../../../puzzles/official/Zone2/4-all-regions-different/0317.json")),
        ] {
            let p = crate::io::parse_puzzle(src).expect("parse");
            let mut types: Vec<String> = Vec::new();
            for row in &p.cells {
                for c in row {
                    if let Some(sym) = &c.symbol {
                        if !types.contains(sym) {
                            types.push(sym.clone());
                        }
                    }
                }
            }
            types.sort();
            let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(15);
            let t0 = crate::clock::Instant::now();
            let out = solve_cardinal_partition(&p, &types, p.height * p.width, deadline);
            eprintln!(
                "tmp_diag_cardinal {tag}: types={:?} solved={} in {:?}",
                types,
                out.is_some(),
                t0.elapsed()
            );
        }
    }

    /// Sum-lock v2 probe: `solve_number_pin` on the all-sum / distinct-sum
    /// classes outside the range-part rule gate.
    /// `cargo test -- --ignored tmp_diag_number_pin --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_number_pin() {
        for (tag, src) in [
            ("0308-30s", include_str!("../../../../puzzles/official/Zone2/10-zone2-mixed/0308.json")),
            ("1138", include_str!("../../../../puzzles/official/Zone3/8-endgame/1138.json")),
            ("1183", include_str!("../../../../puzzles/official/Zone2/1-area-numbers/1183.json")),
            ("0262", include_str!("../../../../puzzles/official/Zone3/5-inequality/0262.json")),
        ] {
            let p = crate::io::parse_puzzle(src).expect("parse");
            let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(60);
            let t0 = crate::clock::Instant::now();
            dies_reset();
            let out = solve_number_pin(&p, p.height * p.width, deadline);
            eprintln!(
                "tmp_diag_number_pin {tag}: solved={} in {:?}",
                out.is_some(),
                t0.elapsed()
            );
            let mut rows: Vec<_> = dies_snapshot().into_iter().collect();
            rows.sort_by_key(|(k, _)| *k);
            for ((tag2, bucket), n) in rows {
                eprintln!("die {tag2:12} assigned~{:<3} x{}", bucket * 16, n);
            }
        }
    }

    /// 0839 (difference + rose P1×3, 5×7): three gap walls alias into the
    /// three seeded regions — the disconnected constraint graph needs the
    /// alias-aware case enumeration.
    #[test]
    fn solves_range_partition_0839() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/4-difference/0839.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "0839 alias-gap cases must solve");
    }

    /// 0262 (area+inequality, 8×8): number clues 14/15/18/17 pin the four
    /// region sizes and the sum identity locks the region count to 4.
    #[test]
    fn solves_range_partition_0262() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/5-inequality/0262.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(30);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        assert!(out.is_some(), "0262 number-clue lock must solve");
    }

    /// Death-attribution dump for the 0929-class endgame gap.
    /// `cargo test -- --ignored tmp_diag_0929 --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_0929() {
        dies_reset();
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/4-difference/0929.json"
        ))
        .expect("parse");
        let deadline = crate::clock::Instant::now() + std::time::Duration::from_secs(12);
        let out = solve_range_partition(&p, p.height * p.width, deadline);
        eprintln!("tmp_diag_0929 solved={}", out.is_some());
        let mut rows: Vec<_> = dies_snapshot().into_iter().collect();
        rows.sort_by_key(|(k, _)| *k);
        for ((tag, bucket), n) in rows {
            eprintln!("die {tag:12} assigned~{:<3} x{}", bucket * 16, n);
        }
    }

    /// 0206 economics probe: plain-only (RANGE_CASES=0) vs cases.  Run both:
    /// `cargo test -- --ignored tmp_diag_0206 --nocapture`
    #[test]
    #[ignore]
    fn tmp_diag_0206() {
        for mode in ["1", "0"] {
            std::env::set_var("RANGE_CASES", mode);
            dies_reset();
            let p = crate::io::parse_puzzle(include_str!(
                "../../../../puzzles/official/Zone3/4-difference/0206.json"
            ))
            .expect("parse");
            let t0 = std::time::Instant::now();
            let out = solve_range_partition(&p, p.height * p.width, crate::clock::Instant::now() + std::time::Duration::from_secs(12));
            eprintln!(
                "tmp_diag_0206 cases={mode} solved={} wall={:?}",
                out.is_some(),
                t0.elapsed()
            );
        }
        std::env::remove_var("RANGE_CASES");
    }

    /// Sufficiency probe: feed the official per-region sizes as exact static
    /// windows.  If the search then converges instantly, size domains are the
    /// missing key for the 0929 class (⇒ disjunctive gap/bitset windows).
    #[test]
    #[ignore]
    fn tmp_diag_0929b_exact_sizes() {
        dies_reset();
        let p = crate::io::parse_puzzle(include_str!(
            "../../../../puzzles/official/Zone3/4-difference/0929.json"
        ))
        .expect("parse");
        let ans: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../puzzles/official/Zone3-answer/4-difference/0929.json"
        ))
        .expect("parse answer");
        let n = p.height * p.width;
        let w = p.width;
        let mut region_size = vec![0usize; n];
        for region in ans["regions"].as_array().unwrap() {
            let cells: Vec<(usize, usize)> = region
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    (
                        c[0].as_u64().unwrap() as usize,
                        c[1].as_u64().unwrap() as usize,
                    )
                })
                .collect();
            for &(r, c) in &cells {
                region_size[r * w + c] = cells.len();
            }
        }
        let pin_of: Vec<Option<usize>> = vec![None; n];
        let mut free_index: std::collections::HashMap<usize, u32> =
            std::collections::HashMap::new();
        let mut free_cells: Vec<usize> = Vec::new();
        for idx in 0..n {
            let (r, c) = (idx / w, idx % w);
            if !p.cells[r][c].blocked {
                free_index.insert(idx, free_cells.len() as u32);
                free_cells.push(idx);
            }
        }
        let facts_raw = watchtower_facts(&p);
        let (wt_same, wt_split) =
            wt_free_relations(&facts_raw, &pin_of, &free_index).expect("wt relations");
        let (unit_cells, unit_of_cell, unit_adj, split) =
            build_free_units(&free_cells, &free_index, &wt_same, &wt_split, &p, w).expect("units");
        let pinned: Vec<PinnedPlacement> = Vec::new();
        let (ord, diff, _ext_lo, _ext_hi) =
            collect_size_orders(&p, w, &pinned, &pin_of, &free_index, &unit_of_cell).expect("ord");
        // Official size hints on the constraint-graph units only (the true
        // case's production shape) — measure how heavy the loose tail is.
        let mut in_diff = vec![false; unit_cells.len()];
        for &(a, b, _) in &diff {
            in_diff[a as usize] = true;
            in_diff[b as usize] = true;
        }
        let sz_lo: Vec<usize> = (0..unit_cells.len())
            .map(|i| {
                if in_diff[i] {
                    region_size[unit_cells[i][0]]
                } else {
                    1
                }
            })
            .collect();
        let sz_hi: Vec<usize> = (0..unit_cells.len())
            .map(|i| {
                if in_diff[i] {
                    region_size[unit_cells[i][0]]
                } else {
                    30
                }
            })
            .collect();
        let unit_types: Vec<u64> = vec![0u64; unit_cells.len()];
        let facts: Vec<WtUnitFact> = Vec::new();
        let facts_of_unit: Vec<Vec<usize>> = vec![Vec::new(); unit_cells.len()];
        let mut search = FreeRem {
            puzzle: &p,
            pinned: &pinned,
            w,
            unit_cells: &unit_cells,
            unit_adj: &unit_adj,
            split: &split,
            facts: &facts,
            facts_of_unit: &facts_of_unit,
            ord: &ord,
            diff: &diff,
            sz_lo: &sz_lo,
            sz_hi: &sz_hi,
            unit_types: &unit_types,
            n_types: 0,
            max_labels: 6,
            lab_of: vec![None; unit_cells.len()],
            lab_units: Vec::new(),
            win_cache: Vec::new(),
            deadline: crate::clock::Instant::now() + std::time::Duration::from_secs(12),
            nodes: 0,
        };
        let out = search.search();
        eprintln!("tmp_diag_0929b solved={} nodes={}", out.is_some(), search.nodes);
        let mut rows: Vec<_> = dies_snapshot().into_iter().collect();
        rows.sort_by_key(|(k, _)| *k);
        for ((tag, bucket), n) in rows {
            eprintln!("die {tag:12} assigned~{:<3} x{}", bucket * 16, n);
        }
    }
}
