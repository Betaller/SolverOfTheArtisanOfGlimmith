//! Per-node fence-pattern check, called from backtrack's `dfs` guard chain.
//!
//! Stateless: reads only `cell_to_region` (and the immutable `puzzle`), so it
//! needs no undo logic on backtrack — re-evaluated from scratch after every
//! assignment, mirroring `backtrack::check_sealed_regions`.
//!
//! # Soundness
//!
//! `dihedral_key` (shapes.rs) normalizes the *whole* point set under the 8
//! rotations/reflections.  A cross with an undetermined arm has no comparable
//! key, so the dihedral comparison fires **only when all four boundary bits are
//! determined**.  When they are, the computed cross is identical to what the
//! leaf validator (`validate::region_boundary_bits` + `fence_pattern_shape`)
//! would produce — every membership input is fixed — so a mismatch implies the
//! leaf validator will reject this branch: pruning is sound (no false
//! positives).
//!
//! A cheaper **partial** check runs even before all bits are known: the pattern's
//! arm count `k = fp.len() - 1` is dihedral-invariant, so if the determined-true
//! count already exceeds `k` (or the determined-false count exceeds `4 - k`),
//! no completion can match — prune.

use crate::shapes::dihedral_key;
use crate::solver::validate::fence_pattern_shape;
use crate::types::Puzzle;

use super::FenceCellData;

/// Direction index → (row delta, col delta).
///
/// Order matches `validate::region_boundary_bits` (validate.rs:571-574):
/// `[0] up, [1] down, [2] left, [3] right`, which in turn matches the arm
/// mapping in `fence_pattern_shape` (validate.rs:580-593).  Keeping this order
/// byte-identical to the leaf validator is what makes the full dihedral
/// comparison agree.
const DIRS: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// One boundary bit: `Some(value)` if determined, `None` if still unknown.
///
/// Determination table (verified against `validate::region_boundary_bits`,
/// validate.rs:557-570):
/// - neighbour off-grid / blocked → `Some(true)` (outer border / hole edge)
/// - neighbour assigned to `n_rid` → `Some(n_rid != my_rid)`
/// - neighbour unassigned, edge `is_boundary` → `Some(true)` (dfs can never
///   join them: `is_adjacent_free` forbids crossing an `is_boundary` edge, so
///   in any completion they are different regions)
/// - neighbour unassigned, edge not `is_boundary` → `None` (could become same
///   or different region)
#[derive(Clone, Copy)]
enum Bit {
    Determined(bool),
    Unknown,
}

/// Look up the edge between `(r,c)` and an in-grid, non-blocked neighbour
/// `(nr,nc)`, mirroring `grid::is_adjacent_free` (grid.rs:4-12) indexing
/// exactly: same row → `h_edges[r][c.min(nc)]`; same col → `v_edges[r.min(nr)][c]`.
///
/// Returns `true` iff a pre-drawn boundary separates the two cells (so the bit
/// is determined-true regardless of future assignment).  The caller has already
/// established the neighbour is in-grid and shares exactly one axis with `(r,c)`.
#[inline]
fn edge_is_boundary(puzzle: &Puzzle, r: usize, c: usize, nr: usize, nc: usize) -> bool {
    if r == nr {
        // Horizontal edge between (r,c) and (r,c±1): stored at h_edges[r][min].
        let min_c = c.min(nc);
        // c != nc guarantees min_c < width-1 (the larger is min_c+1 < width).
        puzzle.h_edges[r][min_c].is_boundary
    } else {
        // Vertical edge between (r,c) and (r±1,c): stored at v_edges[min][c].
        let min_r = r.min(nr);
        puzzle.v_edges[min_r][c].is_boundary
    }
}

/// Evaluate the boundary bit for `(r,c)` (in region `my_rid`) toward direction
/// `(dr,dc)`, reading region membership straight from `cell_to_region`.
#[inline]
fn eval_bit(
    puzzle: &Puzzle,
    cell_to_region: &[Option<usize>],
    width: usize,
    r: usize,
    c: usize,
    my_rid: usize,
    dr: i64,
    dc: i64,
) -> Bit {
    let nr = r as i64 + dr;
    let nc = c as i64 + dc;
    // Off-grid → outer border, always a boundary.
    if nr < 0 || nc < 0 || nr >= puzzle.height as i64 || nc >= puzzle.width as i64 {
        return Bit::Determined(true);
    }
    let nr = nr as usize;
    let nc = nc as usize;
    // Blocked neighbour → the region outline runs along the hole: boundary.
    if puzzle.cells[nr][nc].blocked {
        return Bit::Determined(true);
    }
    match cell_to_region[nr * width + nc] {
        // Neighbour already assigned: boundary iff different region.
        Some(n_rid) => Bit::Determined(n_rid != my_rid),
        // Neighbour unassigned: the bit is determined only if a pre-drawn
        // boundary edge forbids them ever joining (→ true).  Otherwise unknown.
        None => {
            if edge_is_boundary(puzzle, r, c, nr, nc) {
                Bit::Determined(true)
            } else {
                Bit::Unknown
            }
        }
    }
}

/// Mid-search fence pruning.  Returns `false` if any fence-pattern cell's
/// determined boundary bits already contradict its pattern.
///
/// Called from backtrack's guard chain after every `frontier_assign`.  The
/// `fence_cells` slice is empty (and this is never called) when the puzzle has
/// no `fence` rule — the `has_fence` gate in `BacktrackState` makes the 1046
/// non-fence official puzzles pay only one boolean check.
pub fn check_fence_patterns(
    puzzle: &Puzzle,
    cell_to_region: &[Option<usize>],
    width: usize,
    fence_cells: &[FenceCellData],
) -> bool {
    for fc in fence_cells {
        // Skip fence cells not yet assigned to a region (can't evaluate
        // `n_rid != my_rid`).  Mirrors validate.rs:197's `if let Some(rid)`.
        let Some(my_rid) = cell_to_region[fc.r * width + fc.c] else {
            continue;
        };

        let mut bits = [false; 4];
        let mut determined = 0usize;
        let mut true_count = 0usize; // T
        // F = determined - true_count; computed inline below.

        for (i, &(dr, dc)) in DIRS.iter().enumerate() {
            match eval_bit(
                puzzle,
                cell_to_region,
                width,
                fc.r,
                fc.c,
                my_rid,
                dr,
                dc,
            ) {
                Bit::Determined(v) => {
                    bits[i] = v;
                    determined += 1;
                    if v {
                        true_count += 1;
                    }
                }
                Bit::Unknown => {}
            }
        }

        // Partial check (always sound): arm count k is dihedral-invariant, so
        // the final true-count must equal k.  Final true-count ∈ [T, T+U]
        // where U = 4 - determined, F = determined - T.
        //   T > k  → too many boundaries already → dead.
        //   F > 4-k (i.e. T + U < k) → too few boundaries possible → dead.
        let false_count = determined - true_count;
        let k = fc.arm_count;
        if true_count > k || false_count > 4 - k {
            return false;
        }

        // Full check: only when all four bits are determined can we compare
        // dihedral keys (a partial cross's normalized key is meaningless).
        if determined == 4 {
            let cross = fence_pattern_shape(bits);
            if dihedral_key(&cross) != fc.pattern_dihedral_key {
                return false;
            }
        }
    }
    true
}
