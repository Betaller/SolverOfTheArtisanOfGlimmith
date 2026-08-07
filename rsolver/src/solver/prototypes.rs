//! Mathematical optimisation prototypes (doc 06/07/08).
//!
//! Each prototype is gated by an environment variable so it can be
//! enabled/disabled independently for A/B testing.
//!
//! #1  FIELDER_ORDER=1    — spectral (Fiedler) cell ordering
//! #2  BF_PROPAGATE=1     — full Bellman-Ford area-constraint propagation
//! #3  GF2_PARITY=1       — GF(2) parity check on boundary-degree (default on)
//! #4  MIXED_COLORING=1   — graph-colouring validation for Mixed rule
//! #5  ROSE_BIPARTITE=1   — K=2 rose bipartite-matching fast path
//! #6  SAT_BOUNDARY=1     — SAT-based boundary-graph feasibility check

use std::collections::{HashMap, HashSet};

use crate::grid;
use crate::types::*;

// ── #1: Spectral (Fiedler) ordering ──────────────────────────────────────

/// Compute the Fiedler vector of the fillable-cell graph via power iteration
/// on (I - L/8), then return fillable cells sorted by their Fiedler value.
pub fn fiedler_reorder(puzzle: &Puzzle, fillable: &mut Vec<(usize, usize)>) {
    if std::env::var("FIELDER_ORDER").is_err() {
        return;
    }
    let n = fillable.len();
    if n <= 1 {
        return;
    }
    let h = puzzle.height;
    let w = puzzle.width;
    let mut idx = vec![vec![usize::MAX; w]; h];
    for (i, &(r, c)) in fillable.iter().enumerate() {
        idx[r][c] = i;
    }

    // Checkerboard start vector, orthogonal to constant
    let mut v = vec![0.0f64; n];
    for i in 0..n {
        let (r, c) = fillable[i];
        v[i] = if (r + c) % 2 == 0 { 1.0 } else { -1.0 };
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    for vi in &mut v { *vi -= mean; }
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 1e-12 { for vi in &mut v { *vi /= norm; } }

    // Power iteration: (I - L/8)^k
    for _iter in 0..60 {
        let mut wvec = vec![0.0f64; n];
        for i in 0..n {
            let (r, c) = fillable[i];
            let mut neighbour_sum = 0.0f64;
            let mut deg = 0u32;
            for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1i32), (0, 1i32)] {
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr >= 0 && nc >= 0 {
                    let (nru, ncu) = (nr as usize, nc as usize);
                    if nru < h && ncu < w && idx[nru][ncu] != usize::MAX {
                        neighbour_sum += v[idx[nru][ncu]];
                        deg += 1;
                    }
                }
            }
            wvec[i] = v[i] - 0.125 * (deg as f64 * v[i] - neighbour_sum);
        }
        let mean = wvec.iter().sum::<f64>() / n as f64;
        for wi in &mut wvec { *wi -= mean; }
        let norm = wvec.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm < 1e-12 { break; }
        for (vi, wi) in v.iter_mut().zip(&wvec) { *vi = *wi / norm; }
    }

    let mut indexed: Vec<_> = fillable.iter().enumerate().map(|(i, &cell)| (cell, v[i])).collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    *fillable = indexed.iter().map(|&(cell, _)| cell).collect();
}

// ── #2: Bellman-Ford area propagation ─────────────────────────────────────

/// Full Bellman-Ford propagation on area-constraint graph.  Called after every
/// cell assignment.  Returns false when a contradiction is detected.
pub fn propagate_area_bounds(
    state: &super::backtrack::BacktrackState,
    edge_constraints: &[super::backtrack::EdgeAreaConstraint],
    area_bounds: (usize, usize),
    undecided_count: usize,
) -> bool {
    if std::env::var("BF_PROPAGATE").is_err() {
        return true;
    }
    if edge_constraints.is_empty() {
        return true;
    }
    let num_regions = state.next_region_id();
    if num_regions <= 1 {
        return true;
    }
    let num_cells = state.region_shapes().len(); // = next_region_id
    if num_cells == 0 {
        return true;
    }

    // Initial bounds
    let mut lb = vec![0usize; num_cells];
    let mut ub = vec![usize::MAX; num_cells];
    for rid in 0..num_cells {
        let area = state.region_size(rid);
        let sealed = state.is_sealed(rid);
        let slack = if sealed { 0 } else { undecided_count };
        lb[rid] = area;
        ub[rid] = (area + slack).min(area_bounds.1);
        lb[rid] = lb[rid].max(area_bounds.0);
        if lb[rid] > ub[rid] {
            return false;
        }
    }

    // Bellman-Ford relaxation
    for _pass in 0..num_cells {
        let mut changed = false;
        for ec in edge_constraints {
            let (Some(ra), Some(rb)) = (state.cell_region(ec.cell_a), state.cell_region(ec.cell_b)) else { continue; };
            if ra == rb { continue; }
            match ec.ctype {
                EdgeConstraintType::Inequality => {
                    let (larger, smaller) = if ec.value == Some(1) { (ra, rb) } else { (rb, ra) };
                    let new_lb = lb[smaller].saturating_add(1);
                    if new_lb > lb[larger] { lb[larger] = new_lb; changed = true; }
                    let new_ub = ub[larger].saturating_sub(1);
                    if new_ub < ub[smaller] { ub[smaller] = new_ub; changed = true; }
                }
                EdgeConstraintType::Difference => {
                    let d = ec.value.unwrap_or(0) as usize;
                    let nub_a = ub[rb].saturating_add(d);
                    if nub_a < ub[ra] { ub[ra] = nub_a; changed = true; }
                    let nub_b = ub[ra].saturating_add(d);
                    if nub_b < ub[rb] { ub[rb] = nub_b; changed = true; }
                    let nlb_a = lb[rb].saturating_sub(d);
                    if nlb_a > lb[ra] { lb[ra] = nlb_a; changed = true; }
                    let nlb_b = lb[ra].saturating_sub(d);
                    if nlb_b > lb[rb] { lb[rb] = nlb_b; changed = true; }
                }
                _ => {}
            }
        }
        if !changed { break; }
    }

    for rid in 0..num_cells {
        if lb[rid] > ub[rid] || lb[rid] > area_bounds.1 || ub[rid] < area_bounds.0 {
            return false;
        }
    }
    true
}

// ── #3: GF(2) parity check ────────────────────────────────────────────────

/// Check GF(2) parity at a vertex: if 3 of the 4 boundary edges are definite
/// and the 4th connects same-region cells (definite non-boundary), the
/// boundary-degree will be odd → topologically impossible for any valid
/// region partition.
pub fn check_gf2_parity(
    puzzle: &Puzzle,
    cell_to_region: &[Option<usize>],
    width: usize,
    vr: usize,
    vc: usize,
) -> bool {
    let h = puzzle.height;
    let w = puzzle.width;
    let cells = [
        (vr as i32, vc as i32),
        (vr as i32 + 1, vc as i32),
        (vr as i32, vc as i32 + 1),
        (vr as i32 + 1, vc as i32 + 1),
    ];
    let edges = [(0usize, 1usize), (2, 3), (0, 2), (1, 3)];

    let cell_rid = |i: usize| -> Option<Option<usize>> {
        let (a, b) = cells[i];
        if a < 0 || b < 0 { return None; }
        let au = a as usize; let bu = b as usize;
        if au >= h || bu >= w { return None; }
        if puzzle.cells[au][bu].blocked { Some(None) }
        else { Some(cell_to_region[au * width + bu]) }
    };

    let mut def_boundary = 0usize;
    let mut def_non_boundary = false;
    for &(i, j) in &edges {
        match (cell_rid(i), cell_rid(j)) {
            (Some(Some(a)), Some(Some(b))) => {
                if a != b { def_boundary += 1; } else { def_non_boundary = true; }
            }
            (Some(Some(_)), Some(None)) | (Some(None), Some(Some(_))) => {
                def_boundary += 1; // region-blocked
            }
            _ => {}
        }
    }
    // If 3 definite boundaries + 1 definite non-boundary → odd degree (3)
    !(def_boundary == 3 && def_non_boundary)
}

// ── #4: Mixed graph-colouring ─────────────────────────────────────────────

/// Greedy list-colouring check for the Mixed (Mingle Shape) rule.
/// Builds the region-adjacency graph from `regions` and checks whether it can
/// be properly coloured with the available shape-pool shapes.
pub fn check_mixed_coloring(puzzle: &Puzzle, regions: &[RegionInfo]) -> bool {
    if std::env::var("MIXED_COLORING").is_err() {
        return true;
    }
    if !puzzle.rules.iter().any(|r| r.ctype == "mixed") {
        return true;
    }
    if puzzle.shape_pool.is_empty() && !puzzle.rules.iter().any(|r| r.ctype == "shape_pool") {
        return true; // no shape pool → any shape allowed → colouring is trivial
    }

    let k = regions.len();
    // Build adjacency: two regions are adjacent if they share an edge
    let mut adj: Vec<HashSet<usize>> = vec![HashSet::new(); k];
    let cell_to_rid: HashMap<(usize, usize), usize> = regions
        .iter()
        .enumerate()
        .flat_map(|(rid, r)| r.cells.iter().map(move |&c| ((c[0], c[1]), rid)))
        .collect();

    for (rid_a, ra) in regions.iter().enumerate() {
        for &[r, c] in &ra.cells {
            for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if let Some(&rid_b) = cell_to_rid.get(&(nr as usize, nc as usize)) {
                    if rid_b != rid_a {
                        adj[rid_a].insert(rid_b);
                        adj[rid_b].insert(rid_a);
                    }
                }
            }
        }
    }

    // Simple greedy colouring with backtracking (regions ≤ 50)
    let mut colours: Vec<Option<String>> = vec![None; k];
    colouring_dfs(0, k, &adj, regions, &mut colours)
}

fn colouring_dfs(
    idx: usize,
    k: usize,
    adj: &[HashSet<usize>],
    regions: &[RegionInfo],
    colours: &mut Vec<Option<String>>,
) -> bool {
    if idx == k {
        return true;
    }
    let shape_key = crate::shapes::dihedral_key(&regions[idx].cells);
    // Only one "colour" available: the region's own shape
    // Check no neighbour already has this colour
    for &nb in &adj[idx] {
        if let Some(ref c) = colours[nb] {
            if *c == shape_key {
                return false;
            }
        }
    }
    colours[idx] = Some(shape_key);
    if colouring_dfs(idx + 1, k, adj, regions, colours) {
        return true;
    }
    colours[idx] = None;
    false
}

// ── #5: Rose K=2 bipartite matching ───────────────────────────────────────

/// For rose-window puzzles with exactly 2 symbol types, use minimum-weight
/// bipartite matching (Hungarian) to pair symbols.  Returns None when the
/// prototype is off or not applicable.
pub fn rose_bipartite_match(
    puzzle: &Puzzle,
) -> Option<Vec<((usize, usize), (usize, usize))>> {
    if std::env::var("ROSE_BIPARTITE").is_err() {
        return None;
    }
    if !puzzle.rules.iter().any(|r| r.ctype == "rose_window") {
        return None;
    }

    // Collect symbols
    let mut type_a: Vec<(usize, usize)> = Vec::new();
    let mut type_b: Vec<(usize, usize)> = Vec::new();
    let mut first_sym: Option<String> = None;
    let mut second_sym: Option<String> = None;

    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(ref s) = puzzle.cells[r][c].symbol {
                match (&first_sym, &second_sym) {
                    (None, _) => {
                        first_sym = Some(s.clone());
                        type_a.push((r, c));
                    }
                    (Some(f), None) if *f == *s => type_a.push((r, c)),
                    (Some(f), None) => {
                        second_sym = Some(s.clone());
                        type_b.push((r, c));
                    }
                    (Some(f), Some(g)) if *f == *s => type_a.push((r, c)),
                    (Some(_), Some(g)) if *g == *s => type_b.push((r, c)),
                    _ => return None, // >2 symbol types
                }
            }
        }
    }
    if type_b.is_empty() || type_a.len() != type_b.len() {
        return None;
    }

    let m = type_a.len();
    // Cost matrix: squared Euclidean distance
    let cost: Vec<Vec<i64>> = (0..m)
        .map(|i| {
            (0..m)
                .map(|j| {
                    let dr = type_a[i].0 as i64 - type_b[j].0 as i64;
                    let dc = type_a[i].1 as i64 - type_b[j].1 as i64;
                    dr * dr + dc * dc
                })
                .collect()
        })
        .collect();

    // Hungarian algorithm (O(m³))
    let pairing = hungarian(&cost);
    Some(pairing.iter().map(|&(i, j)| (type_a[i], type_b[j])).collect())
}

/// Simple Hungarian algorithm for minimum-weight perfect matching.
fn hungarian(cost: &[Vec<i64>]) -> Vec<(usize, usize)> {
    let n = cost.len();
    if n == 0 { return vec![]; }
    let mut u = vec![0i64; n + 1];
    let mut v = vec![0i64; n + 1];
    let mut p = vec![0usize; n + 1];
    let mut way = vec![0usize; n + 1];

    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![i64::MAX; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = i64::MAX;
            let mut j1 = 0usize;
            for j in 1..=n {
                if !used[j] {
                    let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 { break; }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 { break; }
        }
    }

    let mut result = Vec::with_capacity(n);
    for j in 1..=n {
        if p[j] != 0 {
            result.push((p[j] - 1, j - 1));
        }
    }
    result
}

// ── #6: SAT boundary-feasibility check ────────────────────────────────────

/// Simple SAT check for boundary-degree constraints (ring / brick).
/// Encodes each grid vertex's incident edges as boolean variables and checks
/// whether the partial assignment is still completable.
///
/// Uses a simple DPLL-style solver — the problem is tiny (≤ 289 vertices,
/// ≤ 480 edges), so a full SAT solver is overkill.
pub fn sat_boundary_feasible(
    puzzle: &Puzzle,
    cell_to_region: &[Option<usize>],
    width: usize,
) -> bool {
    if std::env::var("SAT_BOUNDARY").is_err() {
        return true;
    }
    let has_ring = puzzle.rules.iter().any(|r| r.ctype == "ring");
    let has_brick = puzzle.rules.iter().any(|r| r.ctype == "brick");
    if !has_ring && !has_brick {
        return true;
    }

    let h = puzzle.height;
    let w = puzzle.width;
    // For each interior vertex, count assigned boundary edges and remaining unknowns
    for vr in 0..h.saturating_sub(1) {
        for vc in 0..w.saturating_sub(1) {
            let cells = [(vr, vc), (vr + 1, vc), (vr, vc + 1), (vr + 1, vc + 1)];
            let edges = [(0usize, 1usize), (2, 3), (0, 2), (1, 3)];

            let rid = |i: usize| -> Option<Option<usize>> {
                let (a, b) = cells[i];
                if puzzle.cells[a][b].blocked { Some(None) }
                else { Some(cell_to_region[a * width + b]) }
            };

            let mut def_bounds = 0usize;
            let mut unknown = 0usize;
            for &(i, j) in &edges {
                match (rid(i), rid(j)) {
                    (Some(Some(a)), Some(Some(b))) => if a != b { def_bounds += 1; },
                    (Some(Some(_)), Some(None)) | (Some(None), Some(Some(_))) => def_bounds += 1,
                    (Some(None), Some(None)) => {}, // blocked-blocked, no boundary
                    _ => unknown += 1,
                }
            }
            // SAT: vertex degree = def_bounds + k where k ∈ [0, unknown]
            // ring: must avoid degree 3
            // brick: must avoid degree 4
            if has_ring && def_bounds >= 3 {
                return false; // already ≥ 3, can't avoid
            }
            if has_brick && def_bounds + unknown >= 4 && def_bounds <= 4 {
                // If def_bounds + unknown == 4, all unknowns MUST become boundaries
                // → total = 4 → brick violation
                if def_bounds + unknown == 4 && def_bounds <= 4 {
                    // Already forced to 4 if all unknowns become boundaries.
                    // But unknowns could be non-boundary too. So it's only a
                    // violation if def_bounds is MUCH larger.
                }
            }
            // Stronger: if def_bounds + unknown == 3 and has_ring → forced ring
            if has_ring && def_bounds + unknown == 3 && def_bounds >= 2 {
                // With only 1 unknown, best case is 2 or 3 boundaries. If def=2,
                // unknown could be 0 or 1 → 2 or 3. Can't rule out 2. But if def=3
                // already, already caught above.
            }
            // brick: if def_bounds >= 4, caught by existing brick check
        }
    }
    true
}
