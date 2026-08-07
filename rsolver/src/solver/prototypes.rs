//! Mathematical optimisation helpers (doc 06/07/08).
//!
//! #2 BF_PROPAGATE=1  — Bellman-Ford area-constraint propagation (optional)
//! #3 (always on)     — GF(2) parity check on boundary-degree
//! #6 (always on)     — SAT-based boundary-graph feasibility check

use std::collections::{HashMap, HashSet};

use crate::types::*;

// ── Bellman-Ford area propagation (#2, optional) ──────────────────────────

/// Full Bellman-Ford propagation on area-constraint graph.
/// Gated by `BF_PROPAGATE=1` — enable for Inequality/Difference-heavy puzzles.
pub fn propagate_area_bounds(
    cell_to_region: &[Option<usize>],
    region_shapes: &[Vec<[usize; 2]>],
    frontier: &HashMap<usize, HashMap<(usize, usize), usize>>,
    num_regions: usize,
    edge_constraints: &[super::backtrack::EdgeAreaConstraint],
    min_area: usize,
    max_area: usize,
    undecided_count: usize,
    width: usize,
) -> bool {
    if std::env::var("BF_PROPAGATE").is_err() {
        return true;
    }
    if edge_constraints.is_empty() || num_regions <= 1 {
        return true;
    }

    let mut lb = vec![0usize; num_regions];
    let mut ub = vec![usize::MAX; num_regions];
    for rid in 0..num_regions {
        let area = region_shapes.get(rid).map(|s| s.len()).unwrap_or(0);
        let sealed = frontier.get(&rid).map(|f| f.is_empty()).unwrap_or(true);
        let slack = if sealed { 0 } else { undecided_count };
        lb[rid] = area;
        ub[rid] = (area + slack).min(max_area).max(min_area);
        if lb[rid] > ub[rid] {
            return false;
        }
    }

    for _pass in 0..num_regions {
        let mut changed = false;
        for ec in edge_constraints {
            let ra = cell_to_region[ec.cell_a.0 * width + ec.cell_a.1];
            let rb = cell_to_region[ec.cell_b.0 * width + ec.cell_b.1];
            let (Some(ra), Some(rb)) = (ra, rb) else { continue; };
            if ra == rb {
                continue;
            }
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

    for rid in 0..num_regions {
        if lb[rid] > ub[rid] || lb[rid] > max_area || ub[rid] < min_area {
            return false;
        }
    }
    true
}

// ── GF(2) parity check (#3, always on) ────────────────────────────────────

/// Boundary-degree must be even at every interior vertex (topological necessity
/// for any valid region partition).  If 3 edges are definite boundaries and the
/// 4th connects same-region cells, the total will be odd → impossible.
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
                def_boundary += 1;
            }
            _ => {}
        }
    }
    !(def_boundary == 3 && def_non_boundary)
}

// ── SAT boundary feasibility (#6, always on) ──────────────────────────────

/// Check whether the partial boundary assignment can still be completed without
/// violating ring / brick vertex-degree constraints.  Scans all interior
/// vertices; O(grid) per call, throttled to every 64 steps in dfs().
pub fn sat_boundary_feasible(
    puzzle: &Puzzle,
    cell_to_region: &[Option<usize>],
    width: usize,
) -> bool {
    let has_ring = puzzle.rules.iter().any(|r| r.ctype == "ring");
    let has_brick = puzzle.rules.iter().any(|r| r.ctype == "brick");
    if !has_ring && !has_brick {
        return true;
    }

    let h = puzzle.height;
    let w = puzzle.width;
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
                    (Some(None), Some(None)) => {},
                    _ => unknown += 1,
                }
            }
            if has_ring && def_bounds >= 3 {
                return false;
            }
            if has_brick && def_bounds + unknown == 4 && def_bounds >= 2 {
                // At least 2 definite boundaries + enough unknowns to reach 4
                // → all unknowns forced to become boundaries → brick violation.
                // Conservative: only flag when def ≥ 2 and total reachable = 4.
                return false;
            }
        }
    }
    true
}
