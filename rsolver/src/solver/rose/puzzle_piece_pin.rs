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
        let mut d = 0usize;
        let mut u = 0usize;
        for &idx in cells {
            let mut placed = false;
            for (ri, p) in current.iter().enumerate() {
                if p.cells.contains(idx) {
                    // Distinctness: two cells of one region share the id.
                    // Count unique ids lazily via a tiny scan.
                    let _ = ri;
                    placed = true;
                    break;
                }
            }
            if placed {
                d += 1; // overcount fixed below
            } else {
                u += 1;
            }
        }
        // Recount `d` as distinct region ids properly (cells ≤ 4).
        d = 0;
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
        d = seen.len();
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
    sz_lo: &'a Vec<usize>,
    sz_hi: &'a Vec<usize>,
    /// Rose-cardinality mode: per-unit symbol-type bitmask (empty = off).
    unit_types: &'a Vec<u64>,
    n_types: usize,
    /// Region (= label) count: symbols-per-type, NOT the type count.
    max_labels: usize,
    lab_of: Vec<Option<u32>>,
    lab_units: Vec<Vec<u32>>,
    deadline: crate::clock::Instant,
    nodes: usize,
}

const MAX_FREE_NODES: usize = 2_000_000;

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
        {
            let a = self.lab_of.iter().filter(|l| l.is_some()).count();
            if a > self.nodes % 1000 {
            }
        }
        let u = self.pick_mrv()?;
        let values = self.value_order(u);
        let snap_lab = self.lab_of.clone();
        let snap_units = self.lab_units.clone();
        for val in values {
            self.lab_of = snap_lab.clone();
            self.lab_units = snap_units.clone();
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

    /// Propagate to a fixpoint.  `None` = contradiction (dead node).
    ///
    /// Forces apply **one unit at a time with a full recompute**: with
    /// emergent labels every unit's domain starts as `{SPAWN}`, so a batch
    /// force would mint one label per unit in a single round and kill the node
    /// (0994 root: 37 units → 37 labels → WT bounds die).  Sequential forcing
    /// lets each spawn become joinable by the next unit.
    fn fixpoint(&mut self) -> Option<()> {
        'outer: loop {
            if !self.joinable_all() || !self.wt_all_ok() || !self.sizes_ok() {
                return None;
            }
            match self.rose_step() {
                RoseStep::Dead => return None,
                RoseStep::Force(u, l) => {
                    self.lab_units[l as usize].push(u);
                    self.lab_of[u as usize] = Some(l);
                    continue;
                }
                RoseStep::Ok => {}
            }
            for u in 0..self.lab_of.len() as u32 {
                if self.lab_of[u as usize].is_some() {
                    continue;
                }
                let dom = self.cheap_domain(u);
                match dom.len() {
                    0 => return None,
                    1 => {
                        let val = dom[0];
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
            out.push(Some(l as u32));
        }
        if self.n_types == 0 {
            out.push(None);
        } else if self.lab_units.len() < self.max_labels {
            out.push(None);
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
            if self.label_potential_ok(&merged) && self.static_cap_ok(&merged) {
                out.push(Some(l as u32));
            }
        }
        // SPAWN: the unit's own label is trivially connectable when it is a
        // single cell; a non-contiguous must-same class needs its cells
        // connectable through bridges.  In rose mode the label count is
        // exactly `n_types` (the region count = symbols-per-type), so spawn
        // stops at that ceiling, and a fresh label must still be able to
        // collect one unit of every type it lacks.
        if self.n_types > 0 {
            if self.lab_units.len() < self.max_labels
                && self.label_potential_ok(std::slice::from_ref(&u))
                && self.spawn_completable(u)
            {
                out.push(None);
            }
        } else if self.label_potential_ok(std::slice::from_ref(&u)) {
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
    fn pick_mrv(&self) -> Option<u32> {
        let mut best_key: Option<(usize, std::cmp::Reverse<usize>, usize)> = None;
        let mut best_u = None;
        for u in 0..self.lab_of.len() as u32 {
            if self.lab_of[u as usize].is_some() {
                continue;
            }
            let key = (
                self.cheap_domain(u).len(),
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

    /// Inequality size windows: every label must fit `size < cap` from its
    /// pinned-side orders, and orders between two placed labels must still
    /// admit `size(a) < size(b)`.  The cap is further limited by the label's
    /// reachability extent (the enclosure a pinned geometry leaves open —
    /// the wrong-pin root-die for the 0899 class).
    fn sizes_ok(&self) -> bool {
        let mut windows: Vec<(usize, usize)> = Vec::with_capacity(self.lab_units.len());
        for units in &self.lab_units {
            if units.is_empty() {
                windows.push((0, usize::MAX));
                continue;
            }
            let cur: usize = units.iter().map(|&v| self.unit_cells[v as usize].len()).sum();
            let mut lo = cur;
            let mut hi = usize::MAX;
            for &v in units {
                let i = v as usize;
                lo = lo.max(self.sz_lo[i]);
                hi = hi.min(self.sz_hi[i]);
            }
            let reach = self.label_reach(units).1;
            if reach < lo || cur > hi || reach < cur {
                return false;
            }
            hi = hi.min(reach);
            if lo > hi {
                return false;
            }
            windows.push((lo, hi));
        }
        for &(ua, ub) in self.ord {
            let la = self.lab_of[ua as usize];
            let lb = self.lab_of[ub as usize];
            if la == lb && la.is_some() {
                return false;
            }
            if let (Some(a), Some(b)) = (la, lb) {
                let (lo_a, hi_a) = windows[a as usize];
                let (lo_b, hi_b) = windows[b as usize];
                let _ = hi_a;
                // need some size_a < size_b inside the windows
                if std::cmp::max(lo_b, lo_a + 1) > hi_b {
                    return false;
                }
            }
        }
        true
    }

    /// Static part of the size cap (inequality vs pinned sides).
    fn static_cap_ok(&self, units: &[u32]) -> bool {
        let cur: usize = units.iter().map(|&v| self.unit_cells[v as usize].len()).sum();
        let mut hi = usize::MAX;
        for &v in units {
            hi = hi.min(self.sz_hi[v as usize]);
        }
        cur <= hi
    }

    /// Every multi-unit label must remain connectable through undecided cells
    /// that are not must-split from the label (they may still join it).
    fn joinable_all(&self) -> bool {
        for units in &self.lab_units {
            if units.len() >= 2 && !self.label_potential_ok(units) {
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
        (if reached == need { usize::MAX } else { reached }, extent)
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
) -> Option<(Vec<(u32, u32)>, Vec<usize>, Vec<usize>)> {
    let n_units = unit_of_cell
        .iter()
        .copied()
        .max()
        .map_or(0, |m| m as usize + 1);
    let mut ord: Vec<(u32, u32)> = Vec::new();
    let mut lo = vec![1usize; n_units];
    let mut hi = vec![usize::MAX; n_units];
    let mut edge = |a: usize,
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
    for r in 0..puzzle.height {
        for c in 0..puzzle.width.saturating_sub(1) {
            if let Some(ec) = &puzzle.h_edges[r][c].constraint {
                if matches!(ec.ctype, crate::types::EdgeConstraintType::Inequality) {
                    edge(r * w + c, r * w + c + 1, ec.value == Some(1), &mut ord, &mut lo, &mut hi)?;
                }
            }
        }
    }
    for r in 0..puzzle.height.saturating_sub(1) {
        for c in 0..puzzle.width {
            if let Some(ec) = &puzzle.v_edges[r][c].constraint {
                if matches!(ec.ctype, crate::types::EdgeConstraintType::Inequality) {
                    edge(r * w + c, (r + 1) * w + c, ec.value == Some(1), &mut ord, &mut lo, &mut hi)?;
                }
            }
        }
    }
    Some((ord, lo, hi))
}

/// Must-same closure (UF) + pre-drawn walls as must-split + unit compression
/// + unit adjacency.  Returns `(unit_cells, unit_of_cell, unit_adj, split)`;
/// `None` on a must-same class straddling a must-split pair.
#[allow(clippy::type_complexity)]
fn build_free_units(
    free_cells: &[usize],
    free_index: &std::collections::HashMap<usize, u32>,
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

/// Rose-cardinality partition (0975a class): every region holds exactly one
/// unit of each symbol type, so the free labeling has emergent labels pinned
/// apart by the same-type must-split pairs and completed one type at a time
/// (`rose_step`).  The ring frame chain collapses the rim into a single unit
/// (`ring_frame_runs`).  No pins — the whole board is the residue.
pub(crate) fn solve_cardinal_partition(
    puzzle: &Puzzle,
    symbol_types: &[String],
    n: usize,
    deadline: crate::clock::Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
    let w = puzzle.width;
    if symbol_types.len() < 2 {
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
    let (mut unit_cells, unit_of_cell, unit_adj, mut split) =
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
    let sz_lo: Vec<usize> = vec![1usize; unit_cells.len()];
    let sz_hi: Vec<usize> = vec![usize::MAX; unit_cells.len()];
    let facts: Vec<WtUnitFact> = Vec::new();
    let facts_of_unit: Vec<Vec<usize>> = vec![Vec::new(); unit_cells.len()];
    let pinned: Vec<PinnedPlacement> = Vec::new();
    let _ = &mut unit_cells;
    let _ = &mut split;
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
        sz_lo: &sz_lo,
        sz_hi: &sz_hi,
        unit_types: &unit_types,
        n_types,
        max_labels,
        lab_of: vec![None; unit_cells.len()],
        lab_units: Vec::new(),
        deadline: leaf_deadline,
        nodes: 0,
    };
    let out = search.search();
    let deepest = 0usize;
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
    let (ord, sz_lo, sz_hi) =
        collect_size_orders(puzzle, w, pinned, &pin_of, &free_index, &unit_of_cell)?;
    let unit_types: Vec<u64> = vec![0u64; unit_cells.len()];
    let n_types = 0usize;
    let max_labels = 0usize;
    // Order pairs are also must-differ pairs.
    let mut split = split;
    for &(ua, ub) in &ord {
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
        sz_lo: &sz_lo,
        sz_hi: &sz_hi,
        unit_types: &unit_types,
        n_types,
        max_labels,
        lab_of: vec![None; unit_cells.len()],
        lab_units: Vec::new(),
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
}
