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
    if !has_same && !has_patterns && !has_gemini && !has_local {
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
            .count();
    if m == 2 && local_density >= 10 {
        if let Some(regions) = m2_region_growth(puzzle, &free, &idx_of, deadline) {
            return finish(puzzle, regions);
        }
        if crate::aog_debug_enabled() {
            eprintln!("same-tiling: m2 region growth found nothing");
        }
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
        enumerate_pin_assignments, enumerate_pin_candidates, remainder_per_type,
    };
    let (h, w) = (puzzle.height, puzzle.width);
    let symbol_types = crate::shapes::rose_symbol_types(puzzle);
    if symbol_types.is_empty() {
        // Without rose bookkeeping `remainder_per_type` has nothing to count.
        return None;
    }
    let anchors = enumerate_pin_candidates(puzzle, &symbol_types)?;
    if anchors.is_empty() {
        return None;
    }
    let assignments = enumerate_pin_assignments(puzzle, anchors, &symbol_types, m_total);

    for assignment in &assignments {
        if Instant::now() >= deadline {
            return None;
        }
        let n_pin = assignment.pinned.len();
        let Some(m_rem) = remainder_per_type(assignment, puzzle, &symbol_types, w) else {
            continue;
        };
        // Region bookkeeping: every region holds exactly one of each rose type.
        if m_rem + n_pin != m_total {
            continue;
        }
        let mut region_of: Vec<Option<usize>> = vec![None; h * w];
        let mut pinned_mask = vec![false; h * w];
        let mut ok = true;
        for (ri, p) in assignment.pinned.iter().enumerate() {
            for idx in p.cells.iter() {
                if region_of[idx].is_some() {
                    ok = false;
                    break;
                }
                region_of[idx] = Some(ri);
                pinned_mask[idx] = true;
            }
            if !ok {
                break;
            }
        }
        if !ok {
            continue;
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
            continue;
        }
        if m_rem == 1 {
            for &(r, c) in &rem {
                region_of[r * w + c] = Some(n_pin);
            }
            let regions = crate::solver::rose::build_regions(&region_of, h, w);
            if crate::solver::validate::validate(puzzle, &regions) {
                return Some(regions);
            }
            continue;
        }
        let mut rem_idx = vec![vec![usize::MAX; w]; h];
        for (i, &(r, c)) in rem.iter().enumerate() {
            rem_idx[r][c] = i;
        }
        if let Some(regions) =
            cyclic_isometry(puzzle, &rem, &rem_idx, m_rem, deadline, &region_of, n_pin)
        {
            return Some(regions);
        }
    }
    None
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
    /// (free indices of the vertex cells, watchtower value).
    watchtowers: Vec<(Vec<usize>, usize)>,
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
        // everything rather than wrongly prune.
        return vec![[false; 4], [true; 4]];
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
    let facts = GrowthPruneFacts {
        fence,
        must_split,
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
) -> GrowthPruneFacts {
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
    GrowthPruneFacts {
        fence,
        must_split,
        watchtowers,
    }
}

fn m2_region_growth(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    let (h, w) = (puzzle.height, puzzle.width);
    let total = free.len();
    let facts = growth_facts(puzzle, free, idx_of, h, w);

    // Symbol bookkeeping: S gets exactly one cell of each rose type; the other
    // same-type cells are pinned into T.
    let types = crate::shapes::rose_symbol_types(puzzle);
    let mut seeds_of_type: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (k, &(r, c)) in free.iter().enumerate() {
        if let Some(sym) = &puzzle.cells[r][c].symbol {
            seeds_of_type.entry(sym.clone()).or_default().push(k);
        }
    }
    // WLOG the first cell of each type goes to S (the partition is unordered —
    // swapping S ↔ T swaps every choice; fix one representative per type) and
    // the second is pinned into T.
    let mut pinned_t: Vec<usize> = Vec::new();
    let mut pinned_s: Vec<usize> = Vec::new();
    for (_t, ks) in &seeds_of_type {
        if ks.len() == 2 {
            pinned_s.push(ks[0]);
            pinned_t.push(ks[1]);
        } else if ks.len() == 1 {
            pinned_s.push(ks[0]);
        }
    }

    let mut state = GrowthState {
        label: vec![2u8; total], // 0 = S, 1 = T, 2 = undecided
        s_cells: Vec::new(),
        partner: &[],
    };
    for &t in &pinned_t {
        state.label[t] = 1;
    }
    for &s in &pinned_s {
        state.label[s] = 0;
        state.s_cells.push(s);
    }
    if state.s_cells.is_empty() {
        return None;
    }
    let mut visited: std::collections::HashSet<[u64; 4]> = std::collections::HashSet::new();
    visited.insert(bitkey(&state.s_cells));
    grow_free(
        puzzle,
        free,
        idx_of,
        total,
        &mut state,
        &mut visited,
        &facts,
        &pinned_t,
        deadline,
    )
}

/// Recursion of the free-size growth.  `pinned_t` are the T-side symbol seeds.
fn grow_free(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    total: usize,
    state: &mut GrowthState<'_>,
    visited: &mut std::collections::HashSet<[u64; 4]>,
    facts: &GrowthPruneFacts,
    pinned_t: &[usize],
    deadline: Instant,
) -> Option<Vec<RegionInfo>> {
    if Instant::now() >= deadline || visited.len() >= 800_000 {
        return None;
    }
    let (h, w) = (puzzle.height, puzzle.width);
    if crate::aog_debug_enabled() && visited.len() % 20000 == 0 && visited.len() > 0 {
        eprintln!(
            "same-tiling: free-growth states={} |S|={}",
            visited.len(),
            state.s_cells.len()
        );
    }
    if !growth_prunes_ok(puzzle, free, idx_of, state, facts, h, w) {
        return None;
    }
    // T closure: T must stay reachable from the pinned T seeds over the
    // undecided cells (S cells are walls), covering every non-S cell — else
    // the complement can never become one region.
    let mut t_reach: Vec<bool> = vec![false; total];
    let mut q: Vec<usize> = Vec::new();
    for &t in pinned_t {
        if state.label[t] == 1 {
            t_reach[t] = true;
            q.push(t);
        }
    }
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
            if j == usize::MAX || t_reach[j] || state.label[j] == 0 {
                continue;
            }
            t_reach[j] = true;
            q.push(j);
        }
    }
    for k in 0..total {
        if state.label[k] != 0 && !t_reach[k] {
            return None;
        }
    }

    // Leaf: everything decided (S plus pinned-T; free non-seed cells decided
    // implicitly by growth order is NOT enough — undecided cells may still
    // join either side).  Instead the leaf condition is "no undecided cell is
    // adjacent to S": S is sealed; the rest is T.
    let mut frontier: Vec<usize> = Vec::new();
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
    // Fence-adjacent cells first so the star prunes bite early.
    let fence_set: std::collections::BTreeSet<usize> =
        facts.fence.iter().map(|&(k, _)| k).collect();
    frontier.sort_by_key(|&j| (if fence_set.contains(&j) { 0u8 } else { 1 }, j));

    // Also try sealing S here (the remaining undecided cells go to T).
    if let Some(regions) = growth_leaf_free(puzzle, free, idx_of, state, pinned_t, h, w) {
        return Some(regions);
    }

    for j in frontier {
        if Instant::now() >= deadline {
            return None;
        }
        // Symbol rule: the other same-type seed is pinned T; S must not swallow it.
        // (Already labelled 1; the check is implicit.)
        state.s_cells.push(j);
        if !visited.insert(bitkey(&state.s_cells)) {
            state.s_cells.pop();
            continue;
        }
        state.label[j] = 0;
        if let Some(regions) =
            grow_free(puzzle, free, idx_of, total, state, visited, facts, pinned_t, deadline)
        {
            return Some(regions);
        }
        state.s_cells.pop();
        state.label[j] = 2;
    }
    None
}

/// Leaf of the free growth: everything outside S becomes T; verify T is
/// connected (S is by construction), then the full validator.
fn growth_leaf_free(
    puzzle: &Puzzle,
    free: &[(usize, usize)],
    idx_of: &[Vec<usize>],
    state: &GrowthState<'_>,
    pinned_t: &[usize],
    h: usize,
    w: usize,
) -> Option<Vec<RegionInfo>> {
    let total = free.len();
    // Seal all undecided into T and check T connectivity + piece rules.
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
    for &t in pinned_t {
        if state.label[t] != 1 {
            return None; // symbol seed must sit in T
        }
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
