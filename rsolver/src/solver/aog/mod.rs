//! AoG-style DFS solver — a 1:1 Rust port of `third_party/AoG_Solver`.
//!
//! Entry point: [`solve_aog`]. Reads a rsolver [`Puzzle`] (JSON model), runs the
//! C++-translated shape-placement DFS, and returns the regions as `RegionInfo`.

pub mod core;
pub mod empty;
pub mod search;
pub mod types;

use crate::clock::Instant;

use crate::types::*;
use types::*;
use types::{Pools, SOLVE_AREA_BIT};

use self::core::AoGCore;

/// Solve a puzzle with the AoG DFS.
///
/// Returns `ModuleOutcome` so the dispatcher can distinguish a genuine
/// "searched everything, no solution" (`None`) from "found a candidate but
/// `validate` rejected it" (`ValidationFailed`) — the latter was previously
/// folded into `None` and lost (doc 23 §3.3).
pub fn solve_aog(puzzle: &Puzzle, deadline: Instant) -> ModuleOutcome {
    let mut core = match AoGCore::build(puzzle, deadline) {
        Some(c) => c,
        None => {
            eprintln!("aog: build returned None");
            return ModuleOutcome::None;
        }
    };
    let mut sp = core.make_solve_puzzle();
    let pools = Pools::new(MAX_DFS_DEPTH);
    let ret = search::dfs(1, &mut core, &mut sp, &pools);
    if ret != -1 {
        let regions = extract_regions(&core, &sp, puzzle);
        if !crate::solver::validate::validate(puzzle, &regions) {
            eprintln!("aog: internal validation rejected solution ({} regions)", regions.len());
            return ModuleOutcome::ValidationFailed;
        }
        ModuleOutcome::Solved(regions)
    } else {
        ModuleOutcome::None
    }
}

/// Extract per-region cell lists from the solved padded grid.
fn extract_regions(core: &AoGCore, sp: &Vec<Vec<u32>>, puzzle: &Puzzle) -> Vec<RegionInfo> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut by_rid: HashMap<usize, Vec<[usize; 2]>> = HashMap::new();
    for r in 0..h {
        for c in 0..w {
            let px = types::to_puzzle_x((r + 1) as i32) as usize;
            let py = types::to_puzzle_y((c + 1) as i32) as usize;
            if core.puzzle[px][py] == AREA_BLOCK {
                continue;
            }
            let v = sp[px][py];
            if v == AREA_NORMAL {
                continue; // not filled
            }
            let rid = (v & SOLVE_AREA_BIT) as usize;
            by_rid.entry(rid).or_default().push([r, c]);
        }
    }

    let mut regions: Vec<RegionInfo> = by_rid
        .into_iter()
        .map(|(rid, cells)| {
            let mut norm = cells.clone();
            normalize(&mut norm);
            let area = cells.len();
            let key = canonical_key_region(&cells);
            RegionInfo {
                region_id: rid,
                cells,
                area,
                shape: norm,
                normalized_shape_key: key,
                matched_shape_name: None,
            }
        })
        .collect();
    regions.sort_by_key(|r| r.region_id);
    regions
}

fn canonical_key_region(cells: &[[usize; 2]]) -> String {
    let mut norm = cells.to_vec();
    normalize(&mut norm);
    canonical_key(&norm)
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use crate::clock::Instant;
    use crate::types::*;

    /// H3: with `block` (only_rectangles) + a `shape_pool` that contains a
    /// non-rectangular shape (L-tromino), the AoG solver must NOT emit a
    /// non-rectangular region.  The rectangle catalog (built for `block`) still
    /// supplies the 2×2 square, so the 2×2 board is solved by that rectangle;
    /// the L-tromino must be rejected.  If a non-rectangle ever slips through,
    /// the assertion fails.
    #[test]
    fn test_block_rejects_nonrect_pool_shape() {
        let h = 2usize;
        let w = 2usize;
        let cells = (0..h)
            .map(|r| (0..w).map(|c| Cell::new(r, c)).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let puzzle = Puzzle {
            height: h,
            width: w,
            cells,
            h_edges: vec![vec![Edge::default(); w.saturating_sub(1)]; h],
            v_edges: vec![vec![Edge::default(); w]; h.saturating_sub(1)],
            vertices: vec![vec![Vertex::default(); w + 1]; h + 1],
            rules: vec![Rule {
                ctype: "block".into(),
                params: Default::default(),
            }],
            // A non-rectangular pool shape (L-tromino) plus the 2×2 square.
            shape_pool: vec![
                vec![[0usize, 0], [0, 1], [1, 1]],
                vec![[0usize, 0], [0, 1], [1, 0], [1, 1]],
            ],
            outer_boundaries: vec![],
        };
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        let outcome = super::solve_aog(&puzzle, deadline);
        let regions = match outcome {
            ModuleOutcome::Solved(r) => r,
            other => panic!("expected a solved 2x2 block partition, got {:?}", other),
        };
        // Exactly one region covering all 4 cells, and it must be a rectangle.
        assert_eq!(regions.len(), 1, "2x2 block board is a single region");
        let cells_set: std::collections::HashSet<[usize; 2]> =
            regions[0].cells.iter().copied().collect();
        assert_eq!(cells_set.len(), 4, "region must cover all 4 cells");
        // Rectangle check: bounding box area == cell count.
        let mut min_r = usize::MAX;
        let mut max_r = 0;
        let mut min_c = usize::MAX;
        let mut max_c = 0;
        for &[r, c] in &regions[0].cells {
            min_r = min_r.min(r);
            max_r = max_r.max(r);
            min_c = min_c.min(c);
            max_c = max_c.max(c);
        }
        let area = (max_r - min_r + 1) * (max_c - min_c + 1);
        assert_eq!(
            area,
            regions[0].cells.len(),
            "emitted region must be a rectangle (block rule)"
        );
    }
}
