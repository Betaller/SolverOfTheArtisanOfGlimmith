//! Fence-rule dedicated solver support.
//!
//! The `fence` rule (`validate.rs` `"fence"` arm) is a *per-cell* constraint:
//! each cell carrying a `fence_pattern` specifies, up to rotation/reflection,
//! which of its four edges are region boundaries.  The generic backtrack solver
//! used to check this only at the leaf via `validate::validate`, so it freely
//! grew regions that violated fence and learned of the violation only after a
//! full cover was built — causing timeouts and (worse) wrong answers that the
//! validator then rejected (8 official puzzles failed with "validation fail").
//!
//! This module turns fence into a **mid-search** constraint so backtrack prunes
//! a violating branch the moment its boundary bits are determined, instead of
//! at the leaf.  It is deliberately *not* a standalone solver: fence puzzles
//! almost always carry companion rules (area / ring / brick / homogeneous …)
//! that backtrack already enforces, so reusing backtrack's search is both less
//! code and more capable than reimplementing a dedicated region-grower.
//!
//! Architecture mirrors `rose/`: a thin `mod.rs` exposing the public API
//! (`FenceCellData`, `build_fence_cells`, `check_fence_patterns`) with the
//! per-node check logic in `check.rs`.

pub mod check;

use crate::shapes::dihedral_key;
use crate::types::Puzzle;

/// A cell carrying a `fence_pattern` clue, with everything pre-computed once at
/// `solve_backtrack` construction so the per-node check only does cheap lookups
/// + (when all four bits are determined) a single `dihedral_key` comparison.
#[derive(Debug, Clone)]
pub struct FenceCellData {
    pub r: usize,
    pub c: usize,
    /// Number of arms in the pattern = `fp.len() - 1` (the center `[1,1]` is
    /// always present).  Dihedral-invariant: rotations/reflections are
    /// bijections on the arm set, so this bounds the partial check.
    pub arm_count: usize,
    /// `dihedral_key(&fp)` computed once — compared against the key of the
    /// cross built from the cell's current boundary bits.
    pub pattern_dihedral_key: String,
}

/// Collect every cell with a `fence_pattern` and pre-compute its invariant data.
///
/// Returns an empty `Vec` when the puzzle has no `fence` rule, so the
/// `has_fence` gate in `solve_backtrack` makes this zero-overhead for the 1046
/// non-fence official puzzles.  Call once at `BacktrackState` construction.
pub fn build_fence_cells(puzzle: &Puzzle) -> Vec<FenceCellData> {
    if !puzzle.rules.iter().any(|r| r.ctype == "fence") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(ref fp) = puzzle.cells[r][c].fence_pattern {
                debug_assert!(
                    fp.iter().any(|&p| p == [1usize, 1usize]),
                    "fence_pattern at ({},{}) missing center [1,1]: {:?}",
                    r, c, fp
                );
                out.push(FenceCellData {
                    r,
                    c,
                    arm_count: fp.len().saturating_sub(1),
                    pattern_dihedral_key: dihedral_key(fp),
                });
            }
        }
    }
    out
}

/// Re-export the per-node check so callers can write `fence::check_fence_patterns`.
pub use check::check_fence_patterns;
