//! AoG-style DFS solver — a 1:1 Rust port of `third_party/AoG_Solver`.
//!
//! Entry point: [`solve_aog`]. Reads a rsolver [`Puzzle`] (JSON model), runs the
//! C++-translated shape-placement DFS, and returns the regions as `RegionInfo`.

pub mod core;
pub mod empty;
pub mod search;
pub mod types;
pub mod validate;

use std::time::Instant;

use crate::types::*;
use types::*;
use types::{Pools, SOLVE_AREA_BIT};

use self::core::AoGCore;

/// Solve a puzzle with the AoG DFS. Returns regions on success.
pub fn solve_aog(puzzle: &Puzzle, deadline: Instant) -> Option<Vec<RegionInfo>> {
    let mut core = match AoGCore::build(puzzle, deadline) {
        Some(c) => c,
        None => {
            eprintln!("aog: build returned None");
            return None;
        }
    };
    let mut sp = core.make_solve_puzzle();
    let pools = Pools::new(MAX_DFS_DEPTH);
    let ret = search::dfs(1, &mut core, &mut sp, &pools);
    if ret != -1 {
        let regions = extract_regions(&core, &sp, puzzle);
        if !validate::validate(puzzle, &regions) {
            eprintln!("aog: internal validation rejected solution ({} regions)", regions.len());
            return None;
        }
        Some(regions)
    } else {
        None
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
