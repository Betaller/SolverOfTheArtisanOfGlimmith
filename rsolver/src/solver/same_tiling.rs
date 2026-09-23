//! Specialized solver for the global `same` rule (all regions dihedrally
//! congruent, `validate::check_same`) when the region count `m` is derivable
//! from the clues.
//!
//! The `same` clusters of the official corpus (`Zone2/3-all-regions-same`,
//! e.g. 0382/0383/0763/0960) are congruent tilings of the free cells: `m`
//! copies of one polyomino, one rose symbol per piece.  The general solvers
//! all miss them — `is_rose_capable` deliberately refuses `same`/`different`
//! (the matcher has no shape-identity constraint), aog's shape library blows
//! up on the open areas, and edge_csp's shape identity propagator only
//! *checks* shapes at sealing time (it has no shape variable to search over).
//!
//! Two exact methods, both ending in the full `validate::validate` gate:
//!
//! * **m == 2 — isometry orbit method.**  For each of the 8 dihedral maps
//!   `ψ(x) = M·x + t` (all `t` in a bounding range) solve the partition
//!   constraint `S ⊎ ψ(S) = F` as the linear system
//!   `s(x) + s(ψ⁻¹x) = [x ∈ F]`.  On the free cells this is pins + XOR
//!   alternating chains, solvable by BFS coloring in O(|F|); the whole search
//!   is ~10³ isometries × O(|F|), milliseconds.  Complete for m == 2: any two
//!   congruent pieces are related by *some* dihedral `ψ`.
//!
//! * **s ≤ 8 — shared-shape DLX.**  Enumerate the free s-ominoes (108 at
//!   s == 7), and for each shape DLX-tile the free cells with its dihedral
//!   placements (each holding exactly one of every rose symbol).  Catches
//!   0763 (24 pieces of 7), which has m == 24 and so no isometry shortcut.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::clock::Instant;
use crate::dlx::DancingLinks;
use crate::types::*;

/// Largest piece size the shared-shape DLX method will enumerate shapes for.
/// Free s-ominoes grow steeply (108 / 369 / 1285 at s = 7 / 8 / 9); 8 keeps
/// the shape loop well under a second while covering every small piece in the
/// corpus.
const MAX_SMALL_S: usize = 8;

pub fn solve_same_tiling(puzzle: &Puzzle, timeout_ms: u64) -> ModuleOutcome {
    let has_same = puzzle.rules.iter().any(|r| r.ctype == "same");
    let has_patterns = puzzle
        .cells
        .iter()
        .flatten()
        .any(|c| c.shape_pattern.is_some());
    // A homogeneous (Gemini) rule with exactly two regions also forces a
    // congruent split (1248: fence+homogeneous+precise, 2×50) — the edge's
    // two sides are the two regions.  Cheap to try; `validate` decides.
    let has_gemini = puzzle
        .rules
        .iter()
        .any(|r| r.ctype == "homogeneous" || r.ctype == "same");
    // Dense local constraints (fence patterns / watchtowers) make the m==2
    // free growth viable (the rose m=2 cluster).
    let has_local = puzzle
        .cells
        .iter()
        .flatten()
        .any(|c| c.fence_pattern.is_some())
        || puzzle
            .vertices
            .iter()
            .flatten()
            .any(|v| v.watchtower.is_some());
    // Pre-drawn boundaries / constraint edges feed the m=2 parity propagator
    // (0974: 46 precuts, no fence/watchtower at all).
    let has_precuts = puzzle
        .h_edges
        .iter()
        .flatten()
        .any(|e| e.is_boundary || e.constraint.is_some())
        || puzzle
            .v_edges
            .iter()
            .flatten()
            .any(|e| e.is_boundary || e.constraint.is_some());
    if !has_same && !has_patterns && !has_gemini && !has_local && !has_precuts {
        return ModuleOutcome::None;
    }
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);

    let (h, w) = (puzzle.height, puzzle.width);
    let mut free: Vec<(usize, usize)> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if puzzle.cells[r][c].fillable() {
                free.push((r, c));
            }
        }
    }
    let total = free.len();
    if total == 0 {
        return ModuleOutcome::None;
    }
    let Some(m) = derive_region_count(puzzle, total) else {
        return ModuleOutcome::None;
    };
    if m < 2 {
        return ModuleOutcome::None;
    }

    let mut idx_of = vec![vec![usize::MAX; w]; h];
    for (i, &(r, c)) in free.iter().enumerate() {
        idx_of[r][c] = i;
    }

    // Pattern-pinned congruent remainder (the `puzzle_piece + homogeneous`
    // clusters: N shape-pattern regions plus a congruent pair cycled by a
    // Gemini edge).  Each pattern anchor is enumerated as its own region; the
    // leftover then goes through the cyclic method.
    if has_patterns {
        if let Some(regions) = pattern_pinned_cyclic(puzzle, &free, m, deadline) {
            return finish(puzzle, regions);
        }
        if crate::aog_debug_enabled() {
            eprintln!("same-tiling: pattern-pinned cyclic found nothing");
        }
    }

    // m == 2 without any isometry (the rose m=2 cluster: 0987/1249/1137/1149a…)
    // — grow S as a connected half with the complement kept connectable, under
    // the fence-star / must-split / watchtower prunes.  Dense local constraints
    // (75 fence patterns on 0987, 80 watchtowers on 1137) collapse the tree.
    // Runs BEFORE the equal-size gate: the pieces here are often unequal
    // (1137: 60+39).
    let local_density = puzzle
        .cells
        .iter()
        .flatten()
        .filter(|c| c.fence_pattern.is_some())
        .count()
        + puzzle
            .vertices
            .iter()
            .flatten()
            .filter(|v| v.watchtower.is_some())
            .count()
        + puzzle
            .h_edges
            .iter()
            .flatten()
            .filter(|e| e.is_boundary || e.constraint.is_some())
            .count()
        + puzzle
            .v_edges
            .iter()
            .flatten()
            .filter(|e| e.is_boundary || e.constraint.is_some())
            .count();
    if m == 2 && local_density >= 10 {
        if let Some(regions) = m2_region_growth(puzzle, &free, &idx_of, deadline) {
            return finish(puzzle, regions);
        }
        if crate::aog_debug_enabled() {
            eprintln!("same-tiling: m2 region growth found nothing");
        }
    }

    // Precut-only entries used this module solely as the m=2 growth host —
    // the whole-board congruent methods below target same/gemini/pattern
    // puzzles and would only burn the unit budget here.
    if !has_same && !has_patterns && !has_gemini && !has_local {
        return ModuleOutcome::None;
    }

    // The whole-board congruent methods need equal piece sizes.
    if total % m != 0 {
        return ModuleOutcome::None;
    }
    let s = total / m;

    if crate::aog_debug_enabled() {
        eprintln!("same-tiling: total={} m={} s={}", total, m, s);
    }

    let empty_prefix: Vec<Option<usize>> = vec![None; h * w];
    if let Some(regions) = cyclic_isometry(puzzle, &free, &idx_of, m, deadline, &empty_prefix, 0) {
        return finish(puzzle, regions);
    }
    if crate::aog_debug_enabled() {
        eprintln!("same-tiling: cyclic isometry found nothing");
    }
    if s <= MAX_SMALL_S {
        if let Some(regions) = small_shape_dlx(puzzle, &free, &idx_of, m, s, deadline) {
            return finish(puzzle, regions);
        }
    }
    ModuleOutcome::None
}

/// Pattern pre-pin × cyclic remainder: enumerate disjoint placements for the
/// `shape_pattern` anchors (each its own region, see
/// `rose::puzzle_piece_pin`), then split the leftover cells into `m_rem`
/// pieces cycled by one isometry.  The congruence of the leftover pieces is
/// assumed (e.g. forced by a Gemini edge) and only ever *proposed* — the full
/// `validate::validate` accepts or rejects, so a wrong guess can never leak.
fn pattern_pinned_cyclic(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    m_total: usize,
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    use crate::solver::rose::puzzle_piece_pin::{
        enumerate_pin_candidates, for_each_pin_assignment, PinnedPlacement,
    };
    let symbol_types = crate::shapes::rose_symbol_types(puzzle);
    if symbol_types.is_empty() {
        // Without rose bookkeeping `remainder_per_type` has nothing to count.
        return None;
    }
    let anchors = enumerate_pin_candidates(puzzle, &symbol_types)?;
    if anchors.is_empty() {
        return None;
    }
    let mut found: Option<Vec<RegionInfo>> = None;
    for_each_pin_assignment(puzzle, anchors, &symbol_types, deadline, &mut |pinned: &[PinnedPlacement]| {
        if Instant::now() >= deadline {
            return false;
        }
        if let Some(regions) = try_pinned_remainder(puzzle, free, pinned, m_total, &symbol_types, deadline) {
            found = Some(regions);
            return false;
        }
        true
    });
    found
}

/// One pin assignment → try the leftover as `m_rem` congruent pieces (or a
/// single region when `m_rem == 1`).  The congruence of the leftover pieces
/// is only ever *proposed* — `validate` accepts or rejects, so a wrong guess
/// can never leak.
fn try_pinned_remainder(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    pinned: &[crate::solver::rose::puzzle_piece_pin::PinnedPlacement],
    m_total: usize,
    symbol_types: &[String],
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    use crate::solver::rose::puzzle_piece_pin::remainder_per_type;
    let (h, w) = (puzzle.height, puzzle.width);
    let n_pin = pinned.len();
    let Some(m_rem) = remainder_per_type(pinned, puzzle, symbol_types, w) else {
        return None;
    };
    // Region bookkeeping: every region holds exactly one of each rose type.
    if m_rem + n_pin != m_total {
        return None;
    }
    let mut region_of: Vec<Option<usize>> = vec![None; h * w];
    let mut pinned_mask = vec![false; h * w];
    for (ri, p) in pinned.iter().enumerate() {
        for idx in p.cells.iter() {
            if region_of[idx].is_some() {
                return None;
            }
            region_of[idx] = Some(ri);
            pinned_mask[idx] = true;
        }
    }
    let rem: Vec<(usize, usize)> = free
        .iter()
        .copied()
        .filter(|&(r, c)| !pinned_mask[r * w + c])
        .collect();

    if m_rem == 0 {
        if rem.is_empty() {
            let regions = crate::solver::rose::build_regions(&region_of, h, w);
            if crate::solver::validate::validate(puzzle, &regions) {
                return Some(regions);
            }
        }
        return None;
    }
    if m_rem == 1 {
        for &(r, c) in &rem {
            region_of[r * w + c] = Some(n_pin);
        }
        let regions = crate::solver::rose::build_regions(&region_of, h, w);
        if crate::solver::validate::validate(puzzle, &regions) {
            return Some(regions);
        }
        return None;
    }
    let mut rem_idx = vec![vec![usize::MAX; w]; h];
    for (i, &(r, c)) in rem.iter().enumerate() {
        rem_idx[r][c] = i;
    }
    cyclic_isometry(puzzle, &rem, &rem_idx, m_rem, deadline, &region_of, n_pin)
}

fn finish(puzzle: &Puzzle, regions: Vec<RegionInfo>) -> ModuleOutcome {
    // Defensive re-check (callers already validate before accepting).
    if crate::solver::validate::validate(puzzle, &regions) {
        ModuleOutcome::Solved(regions)
    } else {
        ModuleOutcome::ValidationFailed
    }
}

/// Derive the region count `m` from the clues.  Every source is a sound fact
/// (a solvable puzzle makes them agree); conflicts mean the puzzle is
/// unsolvable, and we refuse to guess:
///
/// 1. `rose_window`: every region holds one of each symbol type, so if each
///    type occurs `n` times the partition has exactly `n` pieces.
/// 2. `solitary`: regions ↔ clue cells in bijection.
/// 3. `precise`: every region has area `a` ⟹ `total / a` pieces.
/// 4. area-clue sum: distinct area values summing to `total` force one region
///    per distinct value (see `edge_csp::area_sum_piece_count`).
fn derive_region_count(puzzle: &Puzzle, total: usize) -> Option<usize> {
    let mut found: Option<usize> = None;
    // Accept a derived count; disagreeing derivations make the puzzle
    // unsolvable, so bail instead of guessing.
    let mut accept = |m: usize, found: &mut Option<usize>| -> bool {
        if m < 2 {
            return true;
        }
        match *found {
            None => *found = Some(m),
            Some(prev) if prev == m => {}
            Some(_) => return false,
        }
        true
    };

    // 1. rose symbol counts.
    let types = crate::shapes::rose_symbol_types(puzzle);
    if !types.is_empty() {
        let index: BTreeMap<&str, usize> = types
            .iter()
            .enumerate()
            .map(|(i, t)| (t.as_str(), i))
            .collect();
        let mut counts = vec![0usize; types.len()];
        for row in &puzzle.cells {
            for cell in row {
                if let Some(sym) = &cell.symbol {
                    if let Some(&i) = index.get(sym.as_str()) {
                        counts[i] += 1;
                    }
                }
            }
        }
        if !counts.iter().all(|&c| c == counts[0] && c >= 1) {
            return None;
        }
        if !accept(counts[0], &mut found) {
            return None;
        }
    }

    // 2. solitary clue cells (same predicate as `validate::check_solitary`).
    if puzzle.rules.iter().any(|r| r.ctype == "solitary") {
        let k = puzzle
            .cells
            .iter()
            .flatten()
            .filter(|c| {
                !c.blocked
                    && (c.symbol.is_some()
                        || c.compass.is_some()
                        || c.number.is_some()
                        || c.shape_pattern.is_some()
                        || c.fence_pattern.is_some())
            })
            .count();
        if !accept(k, &mut found) {
            return None;
        }
    }

    // 3. precise.
    if let Some(a) = puzzle
        .rules
        .iter()
        .find(|r| r.ctype == "precise")
        .and_then(|r| r.params.get("area"))
        .and_then(|v| v.as_u64())
    {
        let a = a as usize;
        if a >= 1 && total % a == 0 && !accept(total / a, &mut found) {
            return None;
        }
    }

    // 4. distinct area values summing to total.
    let mut distinct: BTreeSet<i64> = BTreeSet::new();
    for row in &puzzle.cells {
        for cell in row {
            if !cell.blocked {
                if let Some(v) = cell.number {
                    distinct.insert(v);
                }
            }
        }
    }
    if distinct.len() >= 2 && distinct.iter().map(|&v| v as usize).sum::<usize>() == total {
        if !accept(distinct.len(), &mut found) {
            return None;
        }
    }

    found
}

// ── cyclic isometry orbit method ──────────────────────────────────────────────

/// The 8 dihedral maps as (a, b, c, d): (r, c) ↦ (a·r + b·c, c·r + d·c) + t.
const DIHEDRAL: [(i64, i64, i64, i64); 8] = [
    (1, 0, 0, 1),   // id
    (0, -1, 1, 0),  // rot90
    (-1, 0, 0, -1), // rot180
    (0, 1, -1, 0),  // rot270
    (1, 0, 0, -1),  // reflect cols
    (0, 1, 1, 0),   // reflect main-diag (transpose)
    (-1, 0, 0, 1),  // reflect rows
    (0, -1, -1, 0), // reflect anti-diag
];

fn apply(m: (i64, i64, i64, i64), r: i64, c: i64) -> (i64, i64) {
    (m.0 * r + m.1 * c, m.2 * r + m.3 * c)
}

/// Inverse of a dihedral matrix (also dihedral).  Inverse = adjugate / det;
/// with det = ±1 this is det × adjugate — forgetting the sign flips every
/// reflection (det = −1) onto the wrong map.
fn inv(m: (i64, i64, i64, i64)) -> (i64, i64, i64, i64) {
    let det = m.0 * m.3 - m.1 * m.2;
    (det * m.3, det * -m.1, det * -m.2, det * m.0)
}

/// Cap on assignments collected per isometry (a pathological (M, t) can leave
/// many free degrees of freedom; the valid one is among the first few).
const MAX_CSP_SOLUTIONS: usize = 8;

/// Cyclic-isometry method for any `m ≥ 2`: find `ψ` with
/// `F = S ⊎ ψ(S) ⊎ … ⊎ ψ^{m-1}(S)`.  For `m == 2` this is all congruent
/// two-piece splits; for larger `m` it covers the cyclic tilings (0383: three
/// pieces cycled by one glide-rotation).  Non-cyclic tilings fall through to
/// the small-shape DLX.
///
/// The partition is the sliding-window system (s = 1_S, s supported on F):
///   every x ∈ F:  Σ_{j<m} s(ψ⁻ʲx) = 1   (terms outside F are pinned 0)
///   s(y) = 0 whenever some ψⁱ(y) (i < m) leaves F.
/// Summing the equations gives |S| = |F|/m automatically.
fn cyclic_isometry(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    m: usize,
    deadline: Instant,
    region_prefix: &[Option<usize>],
    base: usize,
) -> Option<Vec<RegionInfo>> {
    let (h, w) = (puzzle.height, puzzle.width);
    let reach = (h + w) as i64;

    for &mat in &DIHEDRAL {
        let mat_inv = inv(mat);
        for tx in -reach..=reach {
            for ty in -reach..=reach {
                if Instant::now() >= deadline {
                    return None;
                }
                if m == 2 {
                    if let Some(regions) =
                        m2_transversal_growth(puzzle, free, idx_of, mat, tx, ty, deadline)
                    {
                        // Validate already done inside; prefix support:
                        // re-derive via orbit path is not needed for the
                        // pattern-free case (prefix empty).  For a pinned
                        // prefix the growth method does not model it — fall
                        // through to the CSP path below in that case.
                        if region_prefix.iter().all(|x| x.is_none()) {
                            return Some(regions);
                        }
                    }
                }
                let Some(assigns) =
                    window_csp_solutions(free, idx_of, mat, mat_inv, tx, ty, m, h, w)
                else {
                    continue;
                };
                for assign in assigns {
                    let mut region_of = region_prefix.to_vec();
                    if orbit_fill(
                        puzzle,
                        free,
                        idx_of,
                        mat,
                        tx,
                        ty,
                        m,
                        &assign,
                        h,
                        w,
                        &mut region_of,
                        base,
                    )
                    .is_none()
                    {
                        continue;
                    }
                    // Every fillable cell must be assigned (blocked stay None).
                    let mut covered = true;
                    for r in 0..h {
                        for c in 0..w {
                            if puzzle.cells[r][c].fillable() && region_of[r * w + c].is_none() {
                                covered = false;
                                break;
                            }
                        }
                        if !covered {
                            break;
                        }
                    }
                    if !covered {
                        continue;
                    }
                    let regions = crate::solver::rose::build_regions(&region_of, h, w);
                    // Validate before accepting — keep searching the remaining
                    // (M, t) on rejection.
                    if crate::solver::validate::validate(puzzle, &regions) {
                        return Some(regions);
                    }
                }
            }
        }
    }
    None
}
/// Solve the sliding-window system for ψ(x) = M·x + t (see `cyclic_isometry`).
/// Returns up to `MAX_CSP_SOLUTIONS` assignments; `None` when inconsistent.
#[allow(clippy::too_many_arguments)]
fn window_csp_solutions(
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    mat: (i64, i64, i64, i64),
    mat_inv: (i64, i64, i64, i64),
    tx: i64,
    ty: i64,
    m: usize,
    h: usize,
    w: usize,
) -> Option<Vec<Vec<bool>>> {
    let total = free.len();
    let at = |i: i64, j: i64| -> Option<usize> {
        if i >= 0 && (i as usize) < h && j >= 0 && (j as usize) < w {
            let v = idx_of[i as usize][j as usize];
            if v != usize::MAX {
                Some(v)
            } else {
                None
            }
        } else {
            None
        }
    };
    let psi = |k: usize| -> Option<usize> {
        let (r, c) = free[k];
        let (pr, pc) = apply(mat, r as i64, c as i64);
        at(pr + tx, pc + ty)
    };
    let psi_inv = |k: usize| -> Option<usize> {
        let (r, c) = free[k];
        let (qr, qc) = apply(mat_inv, r as i64 - tx, c as i64 - ty);
        at(qr, qc)
    };

    let mut val: Vec<Option<bool>> = vec![None; total];
    // Pin 0: some forward iterate ψⁱ(y), i < m, leaves F.
    for k in 0..total {
        let mut cur = Some(k);
        for _ in 1..m {
            cur = match cur {
                Some(u) => psi(u),
                None => break,
            };
            if cur.is_none() {
                val[k] = Some(false);
                break;
            }
        }
    }
    // Constraints: for every x ∈ F, exactly one of {ψ⁻ʲx : j < m} is in S.
    // Terms outside F or pinned 0 are dropped.
    let mut constraints: Vec<Vec<usize>> = Vec::with_capacity(total);
    for k in 0..total {
        let mut terms = Vec::with_capacity(m);
        let mut cur = Some(k);
        for _ in 0..m {
            if let Some(u) = cur {
                if val[u] != Some(false) && !terms.contains(&u) {
                    terms.push(u);
                }
            }
            cur = cur.and_then(psi_inv);
        }
        if terms.is_empty() {
            return None;
        }
        constraints.push(terms);
    }

    let mut out: Vec<Vec<bool>> = Vec::new();
    csp_dfs(&constraints, &mut val, &mut out);
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Unit-propagation + DFS over the exactly-one constraints.
fn csp_dfs(constraints: &[Vec<usize>], val: &mut Vec<Option<bool>>, out: &mut Vec<Vec<bool>>) {
    if out.len() >= MAX_CSP_SOLUTIONS {
        return;
    }
    // Unit propagation to fixpoint.
    loop {
        let mut changed = false;
        for c in constraints {
            let mut n_one = 0usize;
            let mut unset: Vec<usize> = Vec::new();
            for &v in c {
                match val[v] {
                    Some(true) => n_one += 1,
                    Some(false) => {}
                    None => unset.push(v),
                }
            }
            if n_one > 1 {
                return;
            }
            if n_one == 1 {
                for v in unset {
                    val[v] = Some(false);
                    changed = true;
                }
            } else if unset.is_empty() {
                return;
            } else if unset.len() == 1 {
                val[unset[0]] = Some(true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // All set?  Record.
    if val.iter().all(|v| v.is_some()) {
        out.push(val.iter().map(|&v| v.unwrap()).collect());
        return;
    }
    // Branch on the constraint with the fewest unset terms.
    let mut best: Option<&Vec<usize>> = None;
    for c in constraints {
        let unset = c.iter().filter(|&&v| val[v].is_none()).count();
        if unset >= 2 && best.map_or(true, |b| unset < b.iter().filter(|&&v| val[v].is_none()).count())
        {
            best = Some(c);
        }
    }
    let Some(best) = best else {
        return;
    };
    let candidates: Vec<usize> = best
        .iter()
        .copied()
        .filter(|&v| val[v].is_none())
        .collect();
    for v in candidates {
        let saved = val.clone();
        val[v] = Some(true);
        for &u in best {
            if u != v && val[u].is_none() {
                val[u] = Some(false);
            }
        }
        csp_dfs(constraints, val, out);
        *val = saved;
        if out.len() >= MAX_CSP_SOLUTIONS {
            return;
        }
    }
}


/// Incremental prune facts for the m=2 growth: fence-pattern star configs and
/// must-split edges (pre-drawn boundaries / Gemini marks).
struct GrowthPruneFacts {
    /// (free index of the fence cell, allowed arm configs [N,S,W,E]).
    fence: Vec<(usize, Vec<[bool; 4]>)>,
    /// (free index a, free index b) — the two cells must end in different
    /// pieces.
    must_split: Vec<(usize, usize)>,
    /// (free index a, free index b) — the two cells must end in the *same*
    /// piece.  Source: the ring frame chain (see `ring_frame_must_same`) —
    /// no wall may touch the outer frame, so every clean perimeter
    /// adjacency is monochrome.
    must_same: Vec<(usize, usize)>,
    /// (free indices of the vertex cells, watchtower value).
    watchtowers: Vec<(Vec<usize>, usize)>,
    /// Must-equal classes (watchtower `val == 1` groups, DSU leader per cell).
    /// Probing/branching one member is identical for every member — run the
    /// leaders only (the probe is per-node O(n) otherwise and 1149a's 196
    /// cells × 2 trials drown the budget).  A must-split pair inside one
    /// class is a build-time contradiction.
    eq_leader: Vec<usize>,
}

/// `ring` frame chain runs: no wall may touch the outer frame.  A frame
/// vertex already carries two boundary edges (the two frame segments meeting
/// there), so a wall between two perimeter cells would make the third — a
/// T-junction the rule forbids.  Every clean perimeter adjacency is
/// therefore same-region, chaining the rim into monochrome **runs**.
/// Runs break at blocked cells and at pre-drawn / constraint edges — those
/// the leaf validator keeps honest.  Empty (no `ring` rule) = no deduction.
pub(crate) fn ring_frame_runs(puzzle: &Puzzle) -> Vec<Vec<(usize, usize)>> {
    let mut runs: Vec<Vec<(usize, usize)>> = Vec::new();
    let (h, w) = (puzzle.height, puzzle.width);
    if !puzzle.rules.iter().any(|r| r.ctype == "ring") || h < 2 || w < 2 {
        return runs;
    }
    // Rim cells in cycle order (top → right → bottom ← left ↑).
    let mut chain: Vec<(usize, usize)> = Vec::new();
    for c in 0..w {
        chain.push((0, c));
    }
    for r in 1..h {
        chain.push((r, w - 1));
    }
    for c in (0..w - 1).rev() {
        chain.push((h - 1, c));
    }
    for r in (1..h - 1).rev() {
        chain.push((r, 0));
    }
    let n = chain.len();
    // Clean link between two adjacent rim cells (shared edge is neither a
    // pre-drawn boundary nor a constraint edge, and neither cell is blocked).
    let clean_link = |a: (usize, usize), b: (usize, usize)| -> bool {
        if puzzle.cells[a.0][a.1].blocked || puzzle.cells[b.0][b.1].blocked {
            return false;
        }
        if a.0 == b.0 {
            let e = &puzzle.h_edges[a.0][a.1.min(b.1)];
            !e.is_boundary && e.constraint.is_none()
        } else {
            let e = &puzzle.v_edges[a.0.min(b.0)][a.1];
            !e.is_boundary && e.constraint.is_none()
        }
    };
    // Split into runs on broken links (wrap-around link included) and
    // blocked cells.
    let mut cur: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        let cell = chain[i];
        if puzzle.cells[cell.0][cell.1].blocked {
            if !cur.is_empty() {
                runs.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let link = clean_link(cell, chain[(i + 1) % n]);
        cur.push(cell);
        if !link {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

/// `ring` frame chain: the must-same pairs (consecutive cells within each run).
fn ring_frame_must_same(
    puzzle: &Puzzle,
    idx_of: &[Vec<usize>],
    h: usize,
    w: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let _ = (h, w);
    for run in ring_frame_runs(puzzle) {
        for pair in run.windows(2) {
            let (i, j) = (idx_of[pair[0].0][pair[0].1], idx_of[pair[1].0][pair[1].1]);
            if i != usize::MAX && j != usize::MAX && i != j {
                out.push((i, j));
            }
        }
    }
    out
}

/// DSU leaders for must-equal groups (`val == 1` watchtowers + must-same
/// pairs).  Leader = the smallest member index (determinism).  `None` on an
/// equal-and-differ clash.
fn eq_leaders(
    total: usize,
    watchtowers: &[(Vec<usize>, usize)],
    must_split: &[(usize, usize)],
    must_same: &[(usize, usize)],
) -> Option<Vec<usize>> {
    let mut parent: Vec<usize> = (0..total).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut u = x;
        while parent[u] != r {
            let next = parent[u];
            parent[u] = r;
            u = next;
        }
        r
    }
    for &(a, b) in must_same {
        let (a, b) = (find(&mut parent, a), find(&mut parent, b));
        if a != b {
            if a < b {
                parent[b] = a;
            } else {
                parent[a] = b;
            }
        }
    }
    for (cells, val) in watchtowers {
        if *val == 1 {
            for &u in &cells[1.min(cells.len())..] {
                let (a, b) = (find(&mut parent, cells[0]), find(&mut parent, u));
                if a != b {
                    // union by smaller index for a stable leader
                    if a < b {
                        parent[b] = a;
                    } else {
                        parent[a] = b;
                    }
                }
            }
        }
    }
    for &(a, b) in must_split {
        if find(&mut parent, a) == find(&mut parent, b) {
            return None;
        }
    }
    Some((0..total).map(|x| find(&mut parent, x)).collect())
}

fn star_configs(pattern: &[[usize; 2]]) -> Vec<[bool; 4]> {
    // A fence pattern is a star: center + up to 4 axis arms.  Every cell is a
    // candidate center; accept when the pattern is exactly its closed
    // neighborhood.  Then expand under the 8 dihedral images of [N,S,W,E].
    let set: BTreeSet<[usize; 2]> = pattern.iter().copied().collect();
    let mut base: Vec<[bool; 4]> = Vec::new();
    for &c in pattern {
        let mut cfg = [false; 4];
        let mut star = vec![c];
        for (i, d) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)].iter().enumerate() {
            let n = [(c[0] as i32 + d.0) as usize, (c[1] as i32 + d.1) as usize];
            if set.contains(&n) {
                cfg[i] = true;
                star.push(n);
            }
        }
        let star_set: BTreeSet<[usize; 2]> = star.iter().copied().collect();
        if star_set == set {
            base.push(cfg);
        }
    }
    if base.is_empty() {
        // Not a star (should not happen for well-formed patterns) — accept
        // every arm config rather than wrongly prune.  (A `[FF, TT]` shortcut
        // here would kill mixed partial bits.)
        return (0..16u8)
            .map(|b| [b & 1 != 0, b & 2 != 0, b & 4 != 0, b & 8 != 0])
            .collect();
    }
    let mut out: Vec<[bool; 4]> = Vec::new();
    for cfg in base {
        // 8 dihedral images of the arm config (rotate/flip the 4 bits).
        for rot in 0..4 {
            for flip in [false, true] {
                let mut img = [false; 4];
                for i in 0..4 {
                    if !cfg[i] {
                        continue;
                    }
                    // bit order [N,S,W,E] = 0,1,2,3.  Map by direction vector.
                    let dirs = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)];
                    let (mut r, mut c) = dirs[i];
                    if flip {
                        c = -c;
                    }
                    for _ in 0..rot {
                        let (nr, nc) = (-c, r);
                        r = nr;
                        c = nc;
                    }
                    let j = if r == -1 {
                        0
                    } else if r == 1 {
                        1
                    } else if c == -1 {
                        2
                    } else {
                        3
                    };
                    img[j] = true;
                }
                if !out.contains(&img) {
                    out.push(img);
                }
            }
        }
    }
    out
}


/// Leaf of the m=2 growth: build the two regions from the labels and run the
/// full validator.
fn growth_leaf(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    state: &GrowthState<'_>,
    h: usize,
    w: usize,
) -> Option<Vec<RegionInfo>> {
    let mut region_of: Vec<Option<usize>> = vec![None; h * w];
    let mut side: [Vec<[usize; 2]>; 2] = [Vec::new(), Vec::new()];
    for (k, &lab) in state.label.iter().enumerate() {
        let cell = free[k];
        side[lab as usize].push([cell.0, cell.1]);
        region_of[cell.0 * w + cell.1] = Some(lab as usize);
    }
    for s in 0..2 {
        if !piece_ok(puzzle, &side[s], h, w) {
            return None;
        }
    }
    let regions = crate::solver::rose::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        return Some(regions);
    }
    None
}

/// Incremental rule prunes: fence-pattern star consistency and must-split
/// edges (Gemini marks / pre-drawn boundaries).
fn growth_prunes_ok(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &GrowthState<'_>,
    facts: &GrowthPruneFacts,
    h: usize,
    w: usize,
) -> bool {
    for &(k, ref configs) in &facts.fence {
        let (r, c) = free[k];
        let mut bits = [false; 4];
        let mut known = [false; 4];
        let lab_v = state.label[k];
        for (i, (nr, nc)) in [
            (r.wrapping_sub(1), c),
            (r + 1, c),
            (r, c.wrapping_sub(1)),
            (r, c + 1),
        ]
        .into_iter()
        .enumerate()
        {
            if nr >= h || nc >= w || puzzle.cells[nr][nc].blocked {
                bits[i] = true;
                known[i] = true;
                continue;
            }
            let j = idx_of[nr][nc];
            if j == usize::MAX {
                continue;
            }
            let lab_u = state.label[j];
            if lab_v != 2 && lab_u != 2 {
                bits[i] = lab_u != lab_v;
                known[i] = true;
            }
        }
        if !configs
            .iter()
            .any(|cfg| (0..4).all(|i| !known[i] || bits[i] == cfg[i]))
        {
            return false;
        }
    }
    for &(a, b) in &facts.must_split {
        let (la, lb) = (state.label[a], state.label[b]);
        if la != 2 && lb != 2 && la == lb {
            return false;
        }
    }
    for &(a, b) in &facts.must_same {
        let (la, lb) = (state.label[a], state.label[b]);
        if la != 2 && lb != 2 && la != lb {
            return false;
        }
    }
    // Watchtower bounds in the TWO-region world: undecided cells can only
    // join the existing S/T labels, so the reachable distinct counts are
    // exactly [d, d + min(u, 2-d)] — the old multi-region bound (lo = d+1)
    // wrongly demanded a fresh region per undecided cell and killed every
    // dense-watchtower puzzle at the root.
    for (cells, val) in &facts.watchtowers {
        let mut seen: Vec<u8> = Vec::new();
        let mut und = 0usize;
        for &idx in cells {
            let lab = state.label[idx];
            if lab == 2 {
                und += 1;
            } else if !seen.contains(&lab) {
                seen.push(lab);
            }
        }
        let d = seen.len();
        let lo = d;
        let hi = d + und.min(2usize.saturating_sub(d));
        if *val < lo || *val > hi {
            return false;
        }
    }
    true
}

/// m == 2 with ψ an involution permuting F (e.g. 1248's point-reflection):
/// every solution is a *connected transversal* S — exactly one cell of each
/// pair {x, ψ(x)} — and the complement is ψ(S), connected iff S is.  The CSP
/// sampler is useless here (2^(|F|/2) free pairs, cap 8 misses the real one),
/// so grow S directly over the pair grid with a visited-set: each growth step
/// takes a frontier cell whose pair is still free and claims its partner for
/// the complement.  Complete: every connected transversal has a frontier
/// growth order.  Both sides get the `piece_ok` check (pre-drawn boundaries
/// are not ψ-invariant).
#[allow(clippy::too_many_arguments)]
fn m2_transversal_growth(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    mat: (i64, i64, i64, i64),
    tx: i64,
    ty: i64,
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    let (h, w) = (puzzle.height, puzzle.width);
    let total = free.len();
    if total % 2 != 0 {
        return None;
    }
    // Pair map: partner[k] = index of ψ(cell_k).  ψ must permute F.
    let mut partner: Vec<usize> = vec![usize::MAX; total];
    for (k, &(r, c)) in free.iter().enumerate() {
        let (pr, pc) = apply(mat, r as i64, c as i64);
        let (pr, pc) = (pr + tx, pc + ty);
        if pr < 0 || pr >= h as i64 || pc < 0 || pc >= w as i64 {
            return None;
        }
        let j = idx_of[pr as usize][pc as usize];
        if j == usize::MAX {
            return None;
        }
        partner[k] = j;
    }
    for k in 0..total {
        if partner[k] == k {
            return None; // fixed point: no split can satisfy s(x)+s(ψx)=1
        }
        if partner[partner[k]] != k {
            return None; // not a perfect matching (ψ² ≠ id on F)
        }
    }

    // Prune facts.
    let mut fence: Vec<(usize, Vec<[bool; 4]>)> = Vec::new();
    let mut must_split: Vec<(usize, usize)> = Vec::new();
    for (k, &(r, c)) in free.iter().enumerate() {
        if let Some(ref fp) = puzzle.cells[r][c].fence_pattern {
            let pat: Vec<[usize; 2]> = fp.clone();
            fence.push((k, star_configs(&pat)));
        }
    }
    for r in 0..h {
        for c in 0..w {
            if c + 1 < w {
                let a = idx_of[r][c];
                let b = idx_of[r][c + 1];
                let split = puzzle.h_edges[r][c].is_boundary
                    || puzzle.h_edges[r][c].constraint.is_some();
                if a != usize::MAX && b != usize::MAX && split {
                    must_split.push((a, b));
                }
            }
            if r + 1 < h {
                let a = idx_of[r][c];
                let b = idx_of[r + 1][c];
                let split = puzzle.v_edges[r][c].is_boundary
                    || puzzle.v_edges[r][c].constraint.is_some();
                if a != usize::MAX && b != usize::MAX && split {
                    must_split.push((a, b));
                }
            }
        }
    }
    let must_same = ring_frame_must_same(puzzle, idx_of, h, w);
    let facts = GrowthPruneFacts {
        eq_leader: eq_leaders(total, &[], &must_split, &must_same)?,
        fence,
        must_split,
        must_same,
        watchtowers: Vec::new(),
    };

    let mut state = GrowthState {
        label: vec![2u8; total], // 0 = S, 1 = T, 2 = undecided
        s_cells: Vec::new(),
        partner: &partner,
    };
    let mut visited: std::collections::HashSet<[u64; 4]> =
        std::collections::HashSet::new();
    // WLOG one fixed root: both orientations of the lex-min pair yield the
    // swapped partition; try the lex-min cell in S first and its partner as
    // the second root for insurance.
    for root in [0usize, partner[0]] {
        if Instant::now() >= deadline {
            return None;
        }
        if crate::aog_debug_enabled() {
            eprintln!("same-tiling: growth root={} t=({},{})", root, tx, ty);
        }
        state.label = vec![2u8; total];
        state.s_cells.clear();
        visited.clear();
        state.label[root] = 0;
        state.label[partner[root]] = 1;
        state.s_cells.push(root);
        visited.insert(bitkey(&state.s_cells));
        if let Some(regions) = grow_transversal(
            puzzle,
            free,
            idx_of,
            total,
            &mut state,
            &mut visited,
            &facts,
            deadline,
        ) {
            return Some(regions);
        }
    }
    None
}


/// Undecided cells reachable from S over undecided cells (assigned T cells are
/// walls), together with the immediate frontier of S.  Every undecided pair
/// must touch the closure — its future S-member has to be reachable — else the
/// branch can never assign all pairs; this prunes the deep dead ends (S walled
/// in at |S| ≈ 48) long before the leaf.
fn reachable_closure(
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &GrowthState<'_>,
    total: usize,
    h: usize,
    w: usize,
) -> (Vec<usize>, Vec<bool>) {
    let mut frontier: Vec<usize> = Vec::new();
    let mut closure: Vec<bool> = vec![false; total];
    let mut q2: Vec<usize> = Vec::new();
    let mut seed = |state: &GrowthState<'_>, closure: &mut Vec<bool>, q2: &mut Vec<usize>, sk: usize| {
        let (r, c) = free[sk];
        for (nr, nc) in [
            (r.wrapping_sub(1), c),
            (r + 1, c),
            (r, c.wrapping_sub(1)),
            (r, c + 1),
        ] {
            if nr >= h || nc >= w {
                continue;
            }
            let j = idx_of[nr][nc];
            if j == usize::MAX || state.label[j] != 2 || closure[j] {
                continue;
            }
            closure[j] = true;
            q2.push(j);
        }
    };
    for &sk in &state.s_cells {
        seed(state, &mut closure, &mut q2, sk);
    }
    while let Some(u) = q2.pop() {
        seed(state, &mut closure, &mut q2, u);
    }
    for &sk in &state.s_cells {
        let (r, c) = free[sk];
        for (nr, nc) in [
            (r.wrapping_sub(1), c),
            (r + 1, c),
            (r, c.wrapping_sub(1)),
            (r, c + 1),
        ] {
            if nr >= h || nc >= w {
                continue;
            }
            let j = idx_of[nr][nc];
            if j == usize::MAX || state.label[j] != 2 {
                continue;
            }
            if !frontier.contains(&j) {
                frontier.push(j);
            }
        }
    }
    frontier.sort_unstable();
    (frontier, closure)
}


/// m == 2 free-size growth (no isometry required).  Grow S from the
/// lex-min rose seed; every other symbol cell is pinned into T (= the
/// complement region, exactly one symbol of each type per side).  Prunes per
/// node: fence-pattern star consistency, must-split edges, watchtower bounds,
/// and the *T closure* — T must stay reachable from its symbol seeds over the
/// undecided cells, else the complement can never form one region.  Leaves are
/// checked for T connectivity and the full validator.
/// Build fence / must-split / watchtower prune facts in free-index form.
fn growth_facts(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    h: usize,
    w: usize,
) -> Option<GrowthPruneFacts> {
    // Prune facts (same shapes as the transversal method).
    let mut fence: Vec<(usize, Vec<[bool; 4]>)> = Vec::new();
    let mut must_split: Vec<(usize, usize)> = Vec::new();
    for (k, &(r, c)) in free.iter().enumerate() {
        if let Some(ref fp) = puzzle.cells[r][c].fence_pattern {
            let pat: Vec<[usize; 2]> = fp.clone();
            fence.push((k, star_configs(&pat)));
        }
    }
    for r in 0..h {
        for c in 0..w {
            if c + 1 < w {
                let (a, b) = (idx_of[r][c], idx_of[r][c + 1]);
                if a != usize::MAX && b != usize::MAX {
                    if puzzle.h_edges[r][c].is_boundary || puzzle.h_edges[r][c].constraint.is_some() {
                        must_split.push((a, b));
                    }
                }
            }
            if r + 1 < h {
                let (a, b) = (idx_of[r][c], idx_of[r + 1][c]);
                if a != usize::MAX && b != usize::MAX {
                    if puzzle.v_edges[r][c].is_boundary || puzzle.v_edges[r][c].constraint.is_some() {
                        must_split.push((a, b));
                    }
                }
            }
        }
    }
    // Watchtower facts (free-index form).
    let mut watchtowers: Vec<(Vec<usize>, usize)> = Vec::new();
    for (cells, val) in crate::solver::rose::puzzle_piece_pin::watchtower_facts(puzzle) {
        let ids: Vec<usize> = cells
            .iter()
            .map(|&flat| idx_of[flat / w][flat % w])
            .filter(|&i| i != usize::MAX)
            .collect();
        if !ids.is_empty() {
            watchtowers.push((ids, val));
        }
    }
    // A 2-cell val==2 vertex is a plain XOR pair — promote it to must_split
    // so xor_round sees it from the root.
    for (cells, val) in &watchtowers {
        if *val == 2 && cells.len() == 2 {
            must_split.push((cells[0], cells[1]));
        }
    }
    // ring ⟹ clean rim adjacencies are must-same (the frame chain).
    let must_same = ring_frame_must_same(puzzle, idx_of, h, w);
    Some(GrowthPruneFacts {
        eq_leader: eq_leaders(free.len(), &watchtowers, &must_split, &must_same)?,
        fence,
        must_split,
        must_same,
        watchtowers,
    })
}

fn m2_region_growth(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    let (h, w) = (puzzle.height, puzzle.width);
    let total = free.len();
    let mut facts = growth_facts(puzzle, free, idx_of, h, w)?;

    // Rose symbol pairs are XOR constraints in the two-piece world (each
    // region holds exactly one cell of each type) — hand them to the parity
    // propagator.  Pin a single root into S: the partition is unordered, so
    // WLOG one representative starts in S.  (The old per-type pinning of
    // first→S/second→T is only sound up to ONE global swap — with ≥2 types
    // the mixed pairings like S={a0,b1} were unreachable.)
    let mut seeds_of_type: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (k, &(r, c)) in free.iter().enumerate() {
        if let Some(sym) = &puzzle.cells[r][c].symbol {
            seeds_of_type.entry(sym.clone()).or_default().push(k);
        }
    }
    for ks in seeds_of_type.values() {
        if ks.len() == 2 {
            if facts.eq_leader[ks[0]] == facts.eq_leader[ks[1]] {
                return None; // same val==1 class but must differ
            }
            facts.must_split.push((ks[0], ks[1]));
        }
    }
    let root = seeds_of_type
        .values()
        .next()
        .and_then(|ks| ks.first().copied())
        .unwrap_or(0);

    let mut state = GrowthState {
        label: vec![2u8; total], // 0 = S, 1 = T, 2 = undecided
        s_cells: vec![root],
        partner: &[],
    };
    state.label[root] = 0;
    let mut visited: std::collections::HashSet<[u64; 8]> = std::collections::HashSet::new();
    grow_free(
        puzzle,
        free,
        idx_of,
        total,
        &mut state,
        &mut visited,
        &facts,
        deadline,
    )
}

/// Unit-propagation outcome: `None` = contradiction, `Some(changed)` = fixpoint
/// pass result.
type Prop = Option<bool>;

impl<'p> GrowthState<'p> {
    /// Assign a previously-undecided cell (0 = S, 1 = T).  S assignments join
    /// `s_cells` (the visited-set key).
    fn set(&mut self, k: usize, v: u8) {
        self.label[k] = v;
        if v == 0 {
            self.s_cells.push(k);
        }
    }
}

/// Force the pairwise label relation (`want_differ` = the two cells must land
/// in different pieces).  In the two-piece world must-split edges, rose symbol
/// pairs and pinned fence arms all reduce to such relations.
fn force_relation(state: &mut GrowthState<'_>, a: usize, b: usize, want_differ: bool) -> Prop {
    let map = |l: u8| if want_differ { 1 - l } else { l };
    let (la, lb) = (state.label[a], state.label[b]);
    if la != 2 && lb != 2 {
        return if lb == map(la) { Some(false) } else { None };
    }
    if la != 2 {
        let t = map(la);
        state.set(b, t);
    } else if lb != 2 {
        let t = map(lb);
        state.set(a, t);
    } else {
        return Some(false);
    }
    Some(true)
}

/// One pass over the XOR pairs (must-split edges + rose symbol pairs).
fn xor_round(state: &mut GrowthState<'_>, facts: &GrowthPruneFacts) -> Prop {
    let mut changed = false;
    for &(a, b) in &facts.must_split {
        changed |= force_relation(state, a, b, true)?;
    }
    Some(changed)
}

/// One pass over the must-same pairs (ring frame chain).
fn same_round(state: &mut GrowthState<'_>, facts: &GrowthPruneFacts) -> Prop {
    let mut changed = false;
    for &(a, b) in &facts.must_same {
        changed |= force_relation(state, a, b, false)?;
    }
    Some(changed)
}

/// Known boundary bits of a fence cell under the current labels (border and
/// blocked neighbours count as boundary; a bit is known once both sides are
/// decided).
fn fence_known_bits(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &GrowthState<'_>,
    k: usize,
    h: usize,
    w: usize,
) -> ([bool; 4], [bool; 4]) {
    let (r, c) = free[k];
    let mut bits = [false; 4];
    let mut known = [false; 4];
    let lab_v = state.label[k];
    for (i, (nr, nc)) in [
        (r.wrapping_sub(1), c),
        (r + 1, c),
        (r, c.wrapping_sub(1)),
        (r, c + 1),
    ]
    .into_iter()
    .enumerate()
    {
        if nr >= h || nc >= w || puzzle.cells[nr][nc].blocked {
            bits[i] = true;
            known[i] = true;
            continue;
        }
        let j = idx_of[nr][nc];
        if j == usize::MAX {
            continue;
        }
        let lab_u = state.label[j];
        if lab_v != 2 && lab_u != 2 {
            bits[i] = lab_u != lab_v;
            known[i] = true;
        }
    }
    (bits, known)
}

/// Which arm values survive in `configs` under the known bits: `None` =
/// contradiction, otherwise per-direction `Some(v)` when every survivor
/// agrees on `v` (unique-config is the all-agree special case).
fn fence_forced_arms(
    configs: &[[bool; 4]],
    bits: &[bool; 4],
    known: &[bool; 4],
) -> Option<[Option<bool>; 4]> {
    let survivors: Vec<&[bool; 4]> = configs
        .iter()
        .filter(|cfg| (0..4).all(|i| !known[i] || bits[i] == cfg[i]))
        .collect();
    let Some(first) = survivors.first() else {
        return None;
    };
    let mut agree = [None; 4];
    for i in 0..4 {
        let v = first[i];
        if survivors.iter().all(|c| c[i] == v) {
            agree[i] = Some(v);
        }
    }
    Some(agree)
}

/// Fence-star unit propagation with arm sharing between adjacent fence cells
/// (the shared edge is one bit: one cell's south arm is its neighbour's north
/// arm).  Each round narrows every star config's domain from decided labels
/// and the neighbours' forced arms; agreed arms then force label relations.
fn fence_round(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &mut GrowthState<'_>,
    facts: &GrowthPruneFacts,
    h: usize,
    w: usize,
) -> Prop {
    let n_f = facts.fence.len();
    let mut f_idx = vec![usize::MAX; free.len()];
    for (fi, &(k, _)) in facts.fence.iter().enumerate() {
        f_idx[k] = fi;
    }
    let mut forced: Vec<[Option<bool>; 4]> = vec![[None; 4]; n_f];
    let mut changed = false;
    loop {
        let mut moved = false;
        for (fi, &(k, ref configs)) in facts.fence.iter().enumerate() {
            let (mut bits, mut known) = fence_known_bits(puzzle, free, idx_of, state, k, h, w);
            // Own previously-agreed arms keep their bit; neighbour fence cells
            // pin the shared edge through their opposite arm.
            let (r, c) = free[k];
            for (i, (nr, nc)) in [
                (r.wrapping_sub(1), c),
                (r + 1, c),
                (r, c.wrapping_sub(1)),
                (r, c + 1),
            ]
            .into_iter()
            .enumerate()
            {
                if known[i] {
                    continue;
                }
                if let Some(v) = forced[fi][i] {
                    bits[i] = v;
                    known[i] = true;
                    continue;
                }
                if nr >= h || nc >= w || puzzle.cells[nr][nc].blocked {
                    continue;
                }
                let j = idx_of[nr][nc];
                if j == usize::MAX {
                    continue;
                }
                let fj = f_idx[j];
                if fj == usize::MAX {
                    continue;
                }
                if let Some(v) = forced[fj][i ^ 1] {
                    bits[i] = v;
                    known[i] = true;
                }
            }
            let agree = fence_forced_arms(configs, &bits, &known)?;
            for (i, (nr, nc)) in [
                (r.wrapping_sub(1), c),
                (r + 1, c),
                (r, c.wrapping_sub(1)),
                (r, c + 1),
            ]
            .into_iter()
            .enumerate()
            {
                let Some(v) = agree[i] else { continue };
                if forced[fi][i] != Some(v) {
                    forced[fi][i] = Some(v);
                    moved = true;
                }
                if nr >= h || nc >= w || puzzle.cells[nr][nc].blocked {
                    continue;
                }
                let j = idx_of[nr][nc];
                if j == usize::MAX {
                    continue;
                }
                changed |= force_relation(state, k, j, v)?;
            }
        }
        if !moved {
            return Some(changed);
        }
    }
}

/// Watchtower unit propagation in the two-piece world (`val` = distinct piece
/// count around the vertex).  `val == 1` forces the quadrant cells equal;
/// `val == 2` turns a two-cell vertex into an XOR pair and flips a lone
/// undecided once the rest already carry one label.  The count bound itself
/// stays in `growth_prunes_ok`.
fn watchtower_round(state: &mut GrowthState<'_>, facts: &GrowthPruneFacts) -> Prop {
    let mut changed = false;
    for (cells, val) in &facts.watchtowers {
        let mut und: Vec<usize> = Vec::new();
        let mut labs: Vec<u8> = Vec::new();
        for &i in cells {
            if state.label[i] == 2 {
                und.push(i);
            } else if !labs.contains(&state.label[i]) {
                labs.push(state.label[i]);
            }
        }
        match *val {
            1 => {
                if labs.len() > 1 {
                    return None;
                }
                if let Some(&l) = labs.first() {
                    for &u in &und {
                        state.set(u, l);
                        changed = true;
                    }
                } else {
                    for &u in &und[1.min(und.len())..] {
                        changed |= force_relation(state, und[0], u, false)?;
                    }
                }
            }
            2 => {
                if und.len() == 1 && labs.len() == 1 {
                    let t = 1 - labs[0];
                    state.set(und[0], t);
                    changed = true;
                } else if und.len() == 2 && labs.is_empty() {
                    changed |= force_relation(state, und[0], und[1], true)?;
                }
            }
            _ => {}
        }
    }
    Some(changed)
}

/// T-reachability round.  Every forced-T cell must sit in one non-S component
/// (the complement has to end connected) — undecided cells that cannot reach
/// that component can never join T and are forced into S.  The old closure
/// prune demanded *every* non-S cell be T-reachable, but undecided cells may
/// still join S: an S' that grew into a wall wrongly killed the official
/// branch (the m=2 cluster's root cause).
fn t_round(
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &mut GrowthState<'_>,
    total: usize,
    h: usize,
    w: usize,
) -> Prop {
    let Some(root) = (0..total).find(|&k| state.label[k] == 1) else {
        return Some(false); // no T seed yet — nothing to enforce
    };
    let mut reach = vec![false; total];
    reach[root] = true;
    let mut q = vec![root];
    while let Some(u) = q.pop() {
        let (r, c) = free[u];
        for (nr, nc) in [
            (r.wrapping_sub(1), c),
            (r + 1, c),
            (r, c.wrapping_sub(1)),
            (r, c + 1),
        ] {
            if nr >= h || nc >= w {
                continue;
            }
            let j = idx_of[nr][nc];
            if j == usize::MAX || reach[j] || state.label[j] == 0 {
                continue;
            }
            reach[j] = true;
            q.push(j);
        }
    }
    let mut changed = false;
    for k in 0..total {
        if reach[k] {
            continue;
        }
        match state.label[k] {
            1 => return None, // T would stay split
            2 => {
                state.set(k, 0);
                changed = true;
            }
            _ => {}
        }
    }
    Some(changed)
}

/// S-reachability round — the exact mirror of `t_round`: S must also end
/// connected, so undecided cells that cannot reach the S mass through
/// (S ∪ undecided) can never join S and are forced to T, and an S cell cut
/// off from the S mass is a contradiction.  One-sided closure leaves the
/// complement-side walls invisible and the tree explodes.
fn s_round(
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &mut GrowthState<'_>,
    total: usize,
    h: usize,
    w: usize,
) -> Prop {
    let Some(root) = (0..total).find(|&k| state.label[k] == 0) else {
        return Some(false); // no S seed yet (root pin normally provides one)
    };
    let mut reach = vec![false; total];
    let mut q: Vec<usize> = Vec::new();
    reach[root] = true;
    q.push(root);
    while let Some(u) = q.pop() {
        let (r, c) = free[u];
        for (nr, nc) in [
            (r.wrapping_sub(1), c),
            (r + 1, c),
            (r, c.wrapping_sub(1)),
            (r, c + 1),
        ] {
            if nr >= h || nc >= w {
                continue;
            }
            let j = idx_of[nr][nc];
            if j == usize::MAX || reach[j] || state.label[j] == 1 {
                continue;
            }
            reach[j] = true;
            q.push(j);
        }
    }
    let mut changed = false;
    for k in 0..total {
        if reach[k] {
            continue;
        }
        match state.label[k] {
            0 => return None, // S would stay split
            2 => {
                state.set(k, 1);
                changed = true;
            }
            _ => {}
        }
    }
    Some(changed)
}

/// Fixpoint of the label propagators.  Every force is implied by the S-set, so
/// the full label state is a function of it — visited-dedup on `s_cells` stays
/// sound.
fn m2_skip(name: &str) -> bool {
    std::env::var("M2_SKIP")
        .map(|v| v.split(',').any(|s| s == name))
        .unwrap_or(false)
}

fn propagate_labels(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &mut GrowthState<'_>,
    facts: &GrowthPruneFacts,
    total: usize,
    h: usize,
    w: usize,
) -> bool {
    loop {
        let mut progress = false;
        if !m2_skip("xor") {
            match xor_round(state, facts) {
                None => return false,
                Some(c) => progress |= c,
            }
        }
        if !m2_skip("same") {
            match same_round(state, facts) {
                None => return false,
                Some(c) => progress |= c,
            }
        }
        if !m2_skip("fence") {
            match fence_round(puzzle, free, idx_of, state, facts, h, w) {
                None => return false,
                Some(c) => progress |= c,
            }
        }
        if !m2_skip("watch") {
            match watchtower_round(state, facts) {
                None => return false,
                Some(c) => progress |= c,
            }
        }
        if !m2_skip("t") {
            match t_round(free, idx_of, state, total, h, w) {
                None => return false,
                Some(c) => progress |= c,
            }
        }
        if !m2_skip("s") {
            match s_round(free, idx_of, state, total, h, w) {
                None => return false,
                Some(c) => progress |= c,
            }
        }
        if !progress {
            return true;
        }
    }
}

/// State cap for the free growth; an abort at the cap is a *budget* outcome,
/// not a proof of unsolvability.
const FREE_GROWTH_STATE_CAP: usize = 2_000_000;

/// Recursion of the free-size growth.  Each node first runs the label
/// propagators to a fixpoint, then tries sealing (rest → T) and expanding S
/// over the frontier.  Propagation writes are rolled back per branch.
#[allow(clippy::too_many_arguments)]
fn grow_free(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    total: usize,
    state: &mut GrowthState<'_>,
    visited: &mut std::collections::HashSet<[u64; 8]>,
    facts: &GrowthPruneFacts,
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    if Instant::now() >= deadline || visited.len() >= FREE_GROWTH_STATE_CAP {
        return None;
    }
    let (h, w) = (puzzle.height, puzzle.width);
    if crate::aog_debug_enabled() && visited.len() % 20000 == 0 && !visited.is_empty() {
        eprintln!(
            "same-tiling: free-growth states={} |S|={}",
            visited.len(),
            state.s_cells.len()
        );
    }
    if !propagate_labels(puzzle, free, idx_of, state, facts, total, h, w) {
        return None;
    }
    // Dedup on the full post-propagation label vector (2 bits/cell): with
    // explicit S/T branching the S-set alone merges states that know
    // different T-decisions and prune differently (false exhaust).
    if !visited.insert(labelkey(&state.label)) {
        return None;
    }
    if !growth_prunes_ok(puzzle, free, idx_of, state, facts, h, w) {
        return None;
    }
    // Failed-label probing (SAC-lite): a label whose fixpoint contradicts can
    // never be the truth — force the opposite.  This is the standard step up
    // from unit propagation for two-colouring-style boards: it collapses the
    // tree long before the 2M-state budget, which plain branch+propagate
    // cannot (the m=2 wall was never a prune-soundness issue but the tree
    // size between forced cells).
    if !m2_skip("probe") {
        let mut k = 0usize;
        while k < total {
            // Must-equal class members probe identically to their leader.
            if state.label[k] == 2 && facts.eq_leader[k] == k {
                let mut ok = [false; 2];
                for v in 0..2u8 {
                    let mut tmp = GrowthState {
                        label: state.label.clone(),
                        s_cells: state.s_cells.clone(),
                        partner: state.partner,
                    };
                    tmp.set(k, v);
                    ok[v as usize] = propagate_labels(
                        puzzle, free, idx_of, &mut tmp, facts, total, h, w,
                    ) && growth_prunes_ok(puzzle, free, idx_of, &tmp, facts, h, w);
                }
                if !ok[0] && !ok[1] {
                    return None;
                }
                if !ok[0] || !ok[1] {
                    let v = if ok[1] { 1 } else { 0 };
                    state.set(k, v);
                    if !propagate_labels(puzzle, free, idx_of, state, facts, total, h, w) {
                        return None;
                    }
                    if !visited.insert(labelkey(&state.label)) {
                        return None;
                    }
                    if !growth_prunes_ok(puzzle, free, idx_of, state, facts, h, w) {
                        return None;
                    }
                    k = 0; // restart: the force may unlock new failed labels
                    continue;
                }
            }
            k += 1;
        }
    }

    // Try sealing S here (the remaining undecided cells go to T).
    if let Some(regions) = growth_leaf_free(puzzle, free, idx_of, state, h, w) {
        return Some(regions);
    }
    // Pick one undecided cell and branch BOTH ways (S/T).  The old loop only
    // ever branched "into S" and left T implicit until sealing — the tree
    // degenerated into S-subset enumeration and hit the state cap long before
    // the fence/XOR prunes could bite.  An explicit T decision fires
    // `propagate_labels` immediately (XOR partners flip to S, fence arms
    // resolve, watchtower bounds tighten), which is what actually collapses
    // the m=2 boards.
    let Some(j) = pick_undecided(free, idx_of, state, facts, h, w) else {
        return None;
    };
    // Value order: the label whose propagation collapses MORE undecided cells
    // first (measured by the failed-label probe's own two trials) — the
    // bigger cascade is the likelier truth and dead-ends faster when wrong.
    let mut order = [0u8, 1u8];
    if !m2_skip("probe") {
        let mut left = [usize::MAX; 2];
        for v in 0..2u8 {
            let mut tmp = GrowthState {
                label: state.label.clone(),
                s_cells: state.s_cells.clone(),
                partner: state.partner,
            };
            tmp.set(j, v);
            if propagate_labels(puzzle, free, idx_of, &mut tmp, facts, total, h, w) {
                left[v as usize] = tmp.label.iter().filter(|&&l| l == 2).count();
            }
        }
        if left[0] != usize::MAX && left[1] != usize::MAX && left[0] < left[1] {
            order.swap(0, 1);
        }
    }
    for v in order {
        if Instant::now() >= deadline {
            return None;
        }
        // Snapshot: child propagation may force many labels (and S cells).
        let snap = state.label.clone();
        let snap_len = state.s_cells.len();
        state.set(j, v);
        if let Some(regions) =
            grow_free(puzzle, free, idx_of, total, state, visited, facts, deadline)
        {
            return Some(regions);
        }
        state.label.copy_from_slice(&snap);
        state.s_cells.truncate(snap_len);
    }
    None
}

/// Most-constrained undecided cell first: fence-star cells (their arms couple
/// 5 labels), then must-split pair members (XOR forces), then watchtower
/// cells, then the S frontier, then the rest.  Ties by index (determinism).
fn pick_undecided(
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &GrowthState<'_>,
    facts: &GrowthPruneFacts,
    h: usize,
    w: usize,
) -> Option<usize> {
    let fence_set: std::collections::BTreeSet<usize> =
        facts.fence.iter().map(|&(k, _)| k).collect();
    let pair_set: std::collections::BTreeSet<usize> = facts
        .must_split
        .iter()
        .flat_map(|&(a, b)| [a, b])
        .collect();
    let watch_set: std::collections::BTreeSet<usize> = facts
        .watchtowers
        .iter()
        .flat_map(|(cs, _)| cs.iter().copied())
        .collect();
    let mut on_frontier = vec![false; free.len()];
    for &sk in &state.s_cells {
        let (r, c) = free[sk];
        for (nr, nc) in [
            (r.wrapping_sub(1), c),
            (r + 1, c),
            (r, c.wrapping_sub(1)),
            (r, c + 1),
        ] {
            if nr >= h || nc >= w {
                continue;
            }
            let j = idx_of[nr][nc];
            if j != usize::MAX && state.label[j] == 2 {
                on_frontier[j] = true;
            }
        }
    }
    let rank = |j: usize| {
        (
            if facts.eq_leader[j] == j { 0u8 } else { 1 },
            if fence_set.contains(&j) { 0u8 } else { 1 },
            if pair_set.contains(&j) { 0u8 } else { 1 },
            if watch_set.contains(&j) { 0u8 } else { 1 },
            if on_frontier[j] { 0u8 } else { 1 },
            j,
        )
    };
    (0..free.len())
        .filter(|&j| state.label[j] == 2)
        .min_by_key(|&j| rank(j))
}

/// Leaf of the free growth: everything outside S becomes T; verify both sides
/// are connected pieces (forced-S cells may sit off the frontier), then run
/// the full validator.
fn growth_leaf_free(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &GrowthState<'_>,
    h: usize,
    w: usize,
) -> Option<Vec<RegionInfo>> {
    let total = free.len();
    let mut region_of: Vec<Option<usize>> = vec![None; h * w];
    let mut s_cells: Vec<[usize; 2]> = Vec::new();
    let mut t_cells: Vec<[usize; 2]> = Vec::new();
    for k in 0..total {
        let cell = free[k];
        let in_s = state.label[k] == 0;
        if in_s {
            s_cells.push([cell.0, cell.1]);
        } else {
            t_cells.push([cell.0, cell.1]);
        }
        region_of[cell.0 * w + cell.1] = Some(if in_s { 0 } else { 1 });
    }
    if s_cells.is_empty() || t_cells.is_empty() {
        return None;
    }
    if !piece_ok(puzzle, &s_cells, h, w) || !piece_ok(puzzle, &t_cells, h, w) {
        return None;
    }
    let _ = idx_of;
    let regions = crate::solver::rose::build_regions(&region_of, h, w);
    if crate::solver::validate::validate(puzzle, &regions) {
        return Some(regions);
    }
    None
}

/// Bitset key of the S cell-index set (supports up to 256 free cells).
fn bitkey(s_cells: &[usize]) -> [u64; 4] {
    let mut k = [0u64; 4];
    for &i in s_cells {
        k[i / 64] |= 1u64 << (i % 64);
    }
    k
}

/// Full 3-way label vector as a dedup key (2 bits per cell).  The S-set alone
/// is only a sound key for one-sided growth: with explicit S/T branching the
/// same S-set can hide different T-decisions (which know more fence bits and
/// propagate differently), and merging them is a false exhaust.
fn labelkey(labels: &[u8]) -> [u64; 8] {
    let mut k = [0u64; 8];
    for (i, &l) in labels.iter().enumerate() {
        k[i / 32] |= (l as u64) << (2 * (i % 32));
    }
    k
}

struct GrowthState<'p> {
    label: Vec<u8>,
    s_cells: Vec<usize>,
    partner: &'p [usize],
}

fn grow_transversal(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    total: usize,
    state: &mut GrowthState<'_>,
    visited: &mut std::collections::HashSet<[u64; 4]>,
    facts: &GrowthPruneFacts,
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    // Per-(M, t) state cap: wrong involutive pairings (grid symmetries) also
    // pass the matching check and would otherwise burn huge trees before the
    // loop reaches the true ψ.
    if Instant::now() >= deadline || visited.len() >= 500_000 {
        return None;
    }
    let (h, w) = (puzzle.height, puzzle.width);
    if crate::aog_debug_enabled() && visited.len() % 10000 == 0 && visited.len() > 0 {
        eprintln!("same-tiling: growth states={} |S|={}", visited.len(), state.s_cells.len());
    }
    if state.s_cells.len() == total / 2 {
        // All pairs assigned (each step assigns one); build and validate.
        return growth_leaf(puzzle, free, state, h, w);
    }
    if !growth_prunes_ok(puzzle, free, idx_of, state, facts, h, w) {
        return None;
    }
    // Frontier = undecided cells adjacent to S, plus the undecided-reachable
    // closure prune (see `reachable_closure`).
    let (mut frontier, closure) = reachable_closure(free, idx_of, state, total, h, w);
    for k in 0..total {
        if state.label[k] != 2 {
            continue;
        }
        // k runs over one member per undecided pair only.
        let p = state.partner[k];
        if !closure[k] && !closure[p] {
            return None;
        }
    }
    frontier.sort_unstable();
    for j in frontier {
        if Instant::now() >= deadline {
            return None;
        }
        let p = state.partner[j];
        if state.label[p] != 2 {
            continue; // pair already split; j cannot join S
        }
        state.s_cells.push(j);
        if !visited.insert(bitkey(&state.s_cells)) {
            state.s_cells.pop();
            continue;
        }
        state.label[j] = 0;
        state.label[p] = 1;
        if let Some(regions) =
            grow_transversal(puzzle, free, idx_of, total, state, visited, facts, deadline)
        {
            return Some(regions);
        }
        state.s_cells.pop();
        state.label[j] = 2;
        state.label[p] = 2;
    }
    None
}

/// Walk each S-seed's ψ-orbit for m steps into `region_of` (ids `base..base+m`)
/// and verify each piece is a legal piece.  `None` on overlap, landing outside
/// `free`, or a piece_ok failure.
#[allow(clippy::too_many_arguments)]
fn orbit_fill(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    mat: (i64, i64, i64, i64),
    tx: i64,
    ty: i64,
    m: usize,
    assign: &[bool],
    h: usize,
    w: usize,
    region_of: &mut [Option<usize>],
    base: usize,
) -> Option<()> {
    let mut side: Vec<Vec<[usize; 2]>> = vec![Vec::new(); m];
    for (k, &in_s) in assign.iter().enumerate() {
        if !in_s {
            continue;
        }
        let (r, c) = free[k];
        // The pins guarantee every ψ-iterate stays in `free`.
        let (mut cr, mut cc) = (r as i64, c as i64);
        for j in 0..m {
            let cell = [cr as usize, cc as usize];
            let i = idx_of[cell[0]][cell[1]];
            if i == usize::MAX || region_of[cell[0] * w + cell[1]].is_some() {
                return None;
            }
            region_of[cell[0] * w + cell[1]] = Some(base + j);
            side[j].push(cell);
            let (nr, nc) = apply(mat, cr, cc);
            cr = nr + tx;
            cc = nc + ty;
        }
    }
    for j in 0..m {
        if side[j].is_empty() || !piece_ok(puzzle, &side[j], h, w) {
            return None;
        }
    }
    Some(())
}
fn piece_ok(puzzle: &Puzzle, cells: &[[usize; 2]], h: usize, w: usize) -> bool {
    let mut in_piece = vec![false; h * w];
    for &[r, c] in cells {
        in_piece[r * w + c] = true;
    }
    let adj = |r: usize, c: usize, nr: usize, nc: usize| -> bool {
        if r == nr {
            let cc = c.min(nc);
            !puzzle.h_edges[r][cc].is_boundary
        } else {
            let rr = r.min(nr);
            !puzzle.v_edges[rr][c].is_boundary
        }
    };
    // No internal is_boundary edge.
    for &[r, c] in cells {
        if c + 1 < w && in_piece[r * w + c + 1] && !adj(r, c, r, c + 1) {
            return false;
        }
        if r + 1 < h && in_piece[(r + 1) * w + c] && !adj(r, c, r + 1, c) {
            return false;
        }
    }
    // Connectivity through non-boundary adjacency.
    let mut seen = vec![false; h * w];
    let mut q = VecDeque::new();
    let start = cells[0];
    seen[start[0] * w + start[1]] = true;
    q.push_back(start);
    let mut count = 0usize;
    while let Some([r, c]) = q.pop_front() {
        count += 1;
        for (nr, nc) in [(r.wrapping_sub(1), c), (r + 1, c), (r, c.wrapping_sub(1)), (r, c + 1)] {
            if nr >= h || nc >= w {
                continue;
            }
            if !in_piece[nr * w + nc] || seen[nr * w + nc] {
                continue;
            }
            if !adj(r, c, nr, nc) {
                continue;
            }
            seen[nr * w + nc] = true;
            q.push_back([nr, nc]);
        }
    }
    count == cells.len()
}

// ── small s: shared-shape DLX ─────────────────────────────────────────────────

fn small_shape_dlx(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    m: usize,
    s: usize,
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    let (h, w) = (puzzle.height, puzzle.width);
    let shapes = free_s_ominoes(s, deadline);
    if crate::aog_debug_enabled() {
        eprintln!("same-tiling: small-shape DLX with {} free {}-ominoes", shapes.len(), s);
    }

    // Rose symbol requirement per piece: exactly one of every symbol type.
    let types = crate::shapes::rose_symbol_types(puzzle);
    let mut need: BTreeMap<String, usize> = BTreeMap::new();
    for t in &types {
        *need.entry(t.clone()).or_default() += 1;
    }

    for shape in &shapes {
        if Instant::now() >= deadline {
            return None;
        }
        let mut placements: Vec<Vec<usize>> =
            shape_placements(puzzle, free, idx_of, shape, s, h, w, &types, &need, deadline);
        if placements.len() < m {
            continue;
        }
        if crate::aog_debug_enabled() {
            eprintln!("same-tiling: shape {:?} -> {} placements", shape.get(0), placements.len());
        }

        // Exact cover of the free cells.
        placements.sort_unstable();
        placements.dedup();
        let mut dlx = DancingLinks::new(free.len());
        for (i, ids) in placements.iter().enumerate() {
            dlx.add_row(ids, i);
        }
        dlx.set_deadline(deadline);
        let mut result: Option<Vec<RegionInfo>> = None;
        let mut partial: Vec<usize> = Vec::new();
        let mut row_check = |_p: &[usize]| true;
        let mut on_solution = |row_ids: &[usize]| {
            let mut region_of: Vec<Option<usize>> = vec![None; h * w];
            for (rid, &row) in row_ids.iter().enumerate() {
                for &i in &placements[row] {
                    let (r, c) = free[i];
                    region_of[r * w + c] = Some(rid);
                }
            }
            // Only the free cells must be covered — blocked cells keep `None`.
            if free.iter().any(|&(r, c)| region_of[r * w + c].is_none()) {
                return false;
            }
            let regions = crate::solver::rose::build_regions(&region_of, h, w);
            if crate::solver::validate::validate(puzzle, &regions) {
                result = Some(regions);
                true
            } else {
                false
            }
        };
        dlx.search_with_check(0, &mut partial, &mut row_check, &mut on_solution);
        if result.is_some() {
            return result;
        }
    }
    None
}


/// All valid placements (free-cell index rows) of one shape's dihedral
/// orientations, filtered by piece legality and the rose symbol signature.
fn shape_placements(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    shape: &Vec<[usize; 2]>,
    s: usize,
    h: usize,
    w: usize,
    types: &[String],
    need: &BTreeMap<String, usize>,
    deadline: Instant,
) -> Vec<Vec<usize>> {
    let mut placements: Vec<Vec<usize>> = Vec::new();
    for orient in crate::polyomino::transforms(shape) {
        // `orient` is normalized to origin; try every anchor.
        let max_r = orient.iter().map(|xy| xy[0]).max().unwrap() as usize;
        let max_c = orient.iter().map(|xy| xy[1]).max().unwrap() as usize;
        if max_r >= h || max_c >= w {
            continue;
        }
        for r in 0..=h - 1 - max_r {
            for c in 0..=w - 1 - max_c {
                if Instant::now() >= deadline {
                    return placements;
                }
                let mut cells: Vec<[usize; 2]> = Vec::with_capacity(s);
                let mut ids: Vec<usize> = Vec::with_capacity(s);
                let mut ok = true;
                for [dr, dc] in &orient {
                    let nr = r + *dr as usize;
                    let nc = c + *dc as usize;
                    let i = idx_of[nr][nc];
                    if i == usize::MAX {
                        ok = false;
                        break;
                    }
                    cells.push([nr, nc]);
                    ids.push(i);
                }
                if !ok || !piece_ok(puzzle, &cells, h, w) {
                    continue;
                }
                // Symbol signature: exactly one of every type.
                if !types.is_empty() {
                    let mut got: BTreeMap<String, usize> = BTreeMap::new();
                    for &[rr, cc] in &cells {
                        if let Some(sym) = &puzzle.cells[rr][cc].symbol {
                            *got.entry(sym.clone()).or_default() += 1;
                        }
                    }
                    if got != *need {
                        continue;
                    }
                }
                ids.sort_unstable();
                placements.push(ids);
            }
        }
    }
    let _ = free;
    placements
}

/// All free s-ominoes (dihedral-canonical), grown from the monomino.  Growth
/// happens in signed coordinates; each grown set is renormalized and deduped
/// by dihedral key (growing from the canonical representative reaches every
/// one-cell growth of the class, since dihedral maps are isometries).
fn free_s_ominoes(s: usize, deadline: Instant) -> Vec<Vec<[usize; 2]>> {
    let mut classes: BTreeMap<String, ()> = BTreeMap::new();
    let mut current: Vec<Vec<[i32; 2]>> = vec![vec![[0i32, 0i32]]];
    classes.insert(key_of_i32(&current[0]), ());
    for _ in 1..s {
        if Instant::now() >= deadline {
            break;
        }
        let mut next: Vec<Vec<[i32; 2]>> = Vec::new();
        for shape in &current {
            let mut frontier: BTreeSet<[i32; 2]> = BTreeSet::new();
            for &[r, c] in shape {
                for p in [[r - 1, c], [r + 1, c], [r, c - 1], [r, c + 1]] {
                    if !shape.contains(&p) {
                        frontier.insert(p);
                    }
                }
            }
            for p in frontier {
                let mut grown = shape.clone();
                grown.push(p);
                normalize_i32(&mut grown);
                let k = key_of_i32(&grown);
                if let std::collections::btree_map::Entry::Vacant(e) = classes.entry(k) {
                    e.insert(());
                    next.push(grown);
                }
            }
        }
        current = next;
    }
    current
        .into_iter()
        .map(|s| {
            s.into_iter()
                .map(|[r, c]| [r as usize, c as usize])
                .collect()
        })
        .collect()
}

fn normalize_i32(shape: &mut [[i32; 2]]) {
    let min_r = shape.iter().map(|xy| xy[0]).min().unwrap_or(0);
    let min_c = shape.iter().map(|xy| xy[1]).min().unwrap_or(0);
    for xy in shape.iter_mut() {
        xy[0] -= min_r;
        xy[1] -= min_c;
    }
    shape.sort_unstable();
}

fn key_of_i32(cells: &[[i32; 2]]) -> String {
    let u: Vec<[usize; 2]> = cells
        .iter()
        .map(|&[r, c]| [r as usize, c as usize])
        .collect();
    crate::shapes::dihedral_key(&u)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The m=2 fence-dense cluster member (8-endgame/1249): rose P1×2 +
    /// 23 fence stars.  Solved by the free-growth rewrite (binary branching
    /// + failed-label probing) in well under a second.
    #[test]
    fn solves_m2_fence_1249() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/8-endgame/1249.json"
        ))
        .expect("parse");
        let out = solve_same_tiling(&p, 30_000);
        assert!(out.is_solved(), "1249 expected solved, got {:?}", out);
    }

    /// The m=2 watchtower-dense cluster member (8-endgame/1149a): ring +
    /// 69 watchtowers + rose P1×2 on 14×14.  The ring frame chain (clean rim
    /// adjacencies are must-same) collapses the 52-cell rim to one label and
    /// the search closes in ~1.5s — previously a 40s timeout.
    #[test]
    fn solves_m2_watchtower_1149a() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/8-endgame/1149a.json"
        ))
        .expect("parse");
        let out = solve_same_tiling(&p, 30_000);
        assert!(out.is_solved(), "1149a expected solved, got {:?}", out);
    }

    /// Pattern-pinned remainder cluster member (11-mixed-rules/0224): 12
    /// shape_pattern anchors + rose P1×13 on 9×13 (12 pin regions of 3 + a
    /// 74-cell remainder).  The streamed pin walk with the exact-one-symbol
    /// filter closes in ~10 ms — the old materialising product OOMed at 14 GB.
    #[test]
    fn solves_pattern_pin_0224() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone1/11-mixed-rules/0224.json"
        ))
        .expect("parse");
        let out = solve_same_tiling(&p, 30_000);
        assert!(out.is_solved(), "0224 expected solved, got {:?}", out);
    }

    /// m == 2 tiling of a 2×2 board into two dominos (identical shapes).
    #[test]
    fn two_dominos_on_2x2() {
        let json = r#"{"grid":{"height":2,"width":2},
            "cells":[{"row":0,"col":0},{"row":0,"col":1},
                     {"row":1,"col":0},{"row":1,"col":1}],
            "edges":[],"vertices":[],"rules":[{"type":"same"}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        // m unknown without clues → None.
        assert!(matches!(
            solve_same_tiling(&puzzle, 2000),
            ModuleOutcome::None
        ));
    }

    /// Two congruent L-tromino halves of a 2×3 board via the `precise` count.
    #[test]
    fn two_trominoes_on_2x3() {
        let json = r#"{"grid":{"height":2,"width":3},
            "cells":[{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2},
                     {"row":1,"col":0},{"row":1,"col":1},{"row":1,"col":2}],
            "edges":[],"vertices":[],"rules":[{"type":"same"},{"type":"precise","params":{"area":3}}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        match solve_same_tiling(&puzzle, 5000) {
            ModuleOutcome::Solved(regions) => {
                assert_eq!(regions.len(), 2);
                let keys: BTreeSet<String> = regions
                    .iter()
                    .map(|r| crate::shapes::dihedral_key(&r.cells))
                    .collect();
                assert_eq!(keys.len(), 1);
            }
            other => panic!("expected Solved, got {:?}", other),
        }
    }

    /// Free-omino generation must contain 0763's heptomino.
    #[test]
    fn free_heptominoes_cover_0763_shape() {
        let shapes = free_s_ominoes(7, Instant::now() + std::time::Duration::from_secs(5));
        assert_eq!(shapes.len(), 108);
        let target = vec![[0usize, 2], [0, 3], [1, 0], [1, 1], [1, 2], [1, 3], [2, 0]];
        let k = crate::shapes::dihedral_key(&target);
        assert!(
            shapes.iter().any(|s| crate::shapes::dihedral_key(s) == k),
            "official 0763 shape missing from free 7-ominoes"
        );
    }

    /// m == 4 tiling of a 2×4 board into four dominos (small-shape DLX).
    #[test]
    fn four_dominos_on_2x4() {
        let json = r#"{"grid":{"height":2,"width":4},
            "cells":[{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2},{"row":0,"col":3},
                     {"row":1,"col":0},{"row":1,"col":1},{"row":1,"col":2},{"row":1,"col":3}],
            "edges":[],"vertices":[],"rules":[{"type":"same"},{"type":"precise","params":{"area":2}}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        match solve_same_tiling(&puzzle, 5000) {
            ModuleOutcome::Solved(regions) => assert_eq!(regions.len(), 4),
            other => panic!("expected Solved, got {:?}", other),
        }
    }

    /// Small-shape DLX with rose symbols: four 2×2 squares, one P1 each
    /// (mirrors 0763's structure: m shapes of size s, one symbol per piece).
    #[test]
    fn four_squares_with_symbols() {
        let json = r#"{"grid":{"height":4,"width":4},
            "cells":[
              {"row":0,"col":0,"symbol":"P1"},{"row":0,"col":1},{"row":0,"col":2,"symbol":"P1"},{"row":0,"col":3},
              {"row":1,"col":0},{"row":1,"col":1},{"row":1,"col":2},{"row":1,"col":3},
              {"row":2,"col":0,"symbol":"P1"},{"row":2,"col":1},{"row":2,"col":2,"symbol":"P1"},{"row":2,"col":3},
              {"row":3,"col":0},{"row":3,"col":1},{"row":3,"col":2},{"row":3,"col":3}],
            "edges":[],"vertices":[],
            "rules":[{"type":"same"},{"type":"rose_window","params":{"symbol_types":["P1"]}}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        match solve_same_tiling(&puzzle, 5000) {
            ModuleOutcome::Solved(regions) => {
                assert_eq!(regions.len(), 4);
                for r in &regions {
                    assert_eq!(r.cells.len(), 4);
                }
            }
            other => panic!("expected Solved, got {:?}", other),
        }
    }

    /// Corpus anchor: 0763 (24 pieces of 7, one P1 each) must go through the
    /// small-shape DLX.
    #[test]
    fn solve_0763_from_corpus() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../puzzles/official/Zone2/3-all-regions-same/0763.json"
        );
        let json = std::fs::read_to_string(path).unwrap();
        let puzzle = crate::io::parse_puzzle(&json).unwrap();
        match solve_same_tiling(&puzzle, 30_000) {
            ModuleOutcome::Solved(regions) => assert_eq!(regions.len(), 24),
            other => panic!("expected Solved, got {:?}", other),
        }
    }

}
