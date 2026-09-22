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

use super::cells::CellSet;

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
/// Filter rule: a pinned region must contain the **same** count of every
/// symbol type (rose regions are balanced).  Equivalently, the placement's
/// per-type counts must all be equal — any other distribution can never
/// partition the remaining symbols into balanced rose regions.
pub fn enumerate_pin_candidates(
    puzzle: &Puzzle,
    symbol_types: &[String],
) -> Option<Vec<AnchorCandidates>> {
    let w = puzzle.width;
    let mut anchors: Vec<AnchorCandidates> = Vec::new();

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
                    // Symbol-balance filter: per-type counts must all be equal.
                    if !symbol_types.is_empty() {
                        let counts = placement_symbol_counts(puzzle, &cells, w, symbol_types);
                        let first = counts[0];
                        if !counts.iter().all(|&x| x == first) {
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

/// One complete pin assignment: one placement per anchor, pairwise disjoint.
/// Produced by combining each anchor's candidates and keeping non-overlapping
/// combinations whose union of pinned cells leaves a symbol-balanced remainder.
pub struct PinAssignment {
    pub pinned: Vec<PinnedPlacement>,
}

/// Enumerate all valid pin assignments (one placement per anchor, pairwise
/// disjoint, remainder symbol-balanced).  Caps the combination count to avoid
/// blow-up on puzzles with many shape_pattern cells.
pub fn enumerate_pin_assignments(
    puzzle: &Puzzle,
    anchors: Vec<AnchorCandidates>,
    symbol_types: &[String],
    rose_m: usize,
) -> Vec<PinAssignment> {
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

    let mut results: Vec<PinAssignment> = Vec::new();
    let mut current: Vec<PinnedPlacement> = Vec::with_capacity(anchors.len());
    combine(&anchors, 0, &mut current, &mut results, puzzle, symbol_types, &total_per_type, rose_m, w);
    results
}

/// Recursive Cartesian product with disjointness + remainder-balance pruning.
fn combine(
    anchors: &[AnchorCandidates],
    i: usize,
    current: &mut Vec<PinnedPlacement>,
    results: &mut Vec<PinAssignment>,
    puzzle: &Puzzle,
    symbol_types: &[String],
    total_per_type: &[usize],
    rose_m: usize,
    w: usize,
) {
    if i == anchors.len() {
        // Complete assignment: check the remainder is symbol-balanced.
        // remainder per type = total - sum(pinned per type); must all be equal
        // so the remaining cells partition into balanced rose regions.
        let mut rem = total_per_type.to_vec();
        for p in current.iter() {
            let counts = placement_symbol_counts(puzzle, &p.cells.iter().collect::<Vec<_>>(), w, symbol_types);
            for (ti, &c) in counts.iter().enumerate() {
                rem[ti] -= c;
            }
        }
        let first = rem[0];
        if rem.iter().all(|&x| x == first) {
            results.push(PinAssignment {
                pinned: current.clone(),
            });
        }
        return;
    }
    for p in &anchors[i].placements {
        // Disjoint with all currently chosen.
        let disjoint = current.iter().all(|c| c.cells.is_disjoint(&p.cells));
        if !disjoint {
            continue;
        }
        current.push(p.clone());
        combine(anchors, i + 1, current, results, puzzle, symbol_types, total_per_type, rose_m, w);
        current.pop();
    }
}

/// For a completed pin assignment, compute the remaining per-type symbol count.
/// Returns `Some(count)` if the remainder is balanced (all types equal), else
/// `None` — in which case the assignment cannot yield a valid rose partition.
pub fn remainder_per_type(
    assignment: &PinAssignment,
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
    for p in &assignment.pinned {
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
/// Exact for the shape the corpus actually has here: pin every `shape_pattern`
/// region, then treat the *entire* remainder as a single region and run it
/// through the full validator.  A wrong remainder is rejected by `validate`;
/// if no assignment yields a valid single-remainder partition this returns
/// `None` and the rest of the chain runs unchanged.  Regions that hold more
/// than one `shape_pattern` cell (0493) or leave several unconstrained
/// remainder regions (0994/1435) are out of scope and fall through.
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
    let Some(a) = covered.iter().position(|&b| !b) else {
        if crate::aog_debug_enabled() {
            eprintln!("pp-pin: leaf with {} placements, checking remainder", current.len());
        }
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
        }
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
