//! Region matching solver for rose_window puzzles — port of
//! `src/solver/region_match.py` + `src/solver/bfs_candidates.py`.
//!
//! Strategy: precompute every legal connected region for each seed (the most
//! constrained symbol type), then exact-cover match one region per seed with an
//! MRV heuristic.  This is the critical path that solves "rose_window without
//! size constraint" puzzles (e.g. C4-1, 0277).

use std::collections::{HashMap, HashSet};
use crate::clock::Instant;

use crate::types::Puzzle;

use super::cells::{CellSet, PreBoundaries};

pub const CANDIDATE_CAP: usize = 20_000;
pub const MAX_CANDIDATE_CELLS: usize = 100;
pub const PER_COMBO_TIMEOUT_MS: u64 = 1_000;
/// Hard cap on the area-combo enumeration (`enum_area_combos_bounded`). For
/// large m (e.g. 0882 m=22, 0223 m=31), the full combo count is astronomical
/// (2.2e20, 1.7e28) — pre-collecting them all OOMs before any combo is tried.
/// This cap stops enumeration after MAX_COMBOS tuples and proceeds with the
/// partial set (sorted, most-balanced first), letting `match_regions_mrv` try
/// them. If the solution needs a later combo it's a false-negative (graceful —
/// `solve_rose` falls through to `rose_growth`). (B-LZ, doc 15 §2 A2.)
pub const MAX_COMBOS: usize = 50_000;
/// Hard cap on the `visited` dedup set (line ~40). Open rose_window grids
/// enumerate unbounded distinct regions → OOM (exit -9) before the caller's
/// deadline can fire (`generate_all_candidates` has no internal time check).
/// When hit, **bail out** (`break`) and return the partial `results`. The
/// caller handles gracefully: `match_regions_mrv` may miss the exact cover →
/// `None` → `solve_rose` falls through to `rose_growth`, which now honors its
/// deadline (previously `_deadline` was unused → hang). `accept_if_valid` →
/// `validate::validate` gates acceptance → false-negative only, never
/// false-positive.
///
/// **Do NOT** switch to "stop inserting but keep `contains()`-checking" — that
/// breaks dedup and enqueues the same region via many paths (exponential
/// re-processing, visited-OOM → queue-OOM). `break` is safe: the queue is
/// bounded by `visited` (every enqueued state is inserted first).
///
/// Value: 2,000,000. Lower values (200k) regressed rose_window PASS puzzles
/// (e.g. 0833) whose true-solution regions are discovered late in the BFS —
/// bailing early dropped them, `match_regions_mrv` failed, and `rose_growth`
/// (pre-fix) hung. 2M × ~88-104B ≈ 176-208MB — under typical RSS limits yet
/// high enough to keep nearly all solvable puzzles' full candidate sets. The
/// 4 historical rose OOM puzzles (0882/0826/0838/0999) had `visited` growing
/// into the multi-million range; 2M stops them well before OOM.
pub const VISITED_CAP: usize = 2_000_000;

/// BFS over boundary-compliant connected subsets containing `seed` (cell idx).
/// Port of `bfs_candidates.generate_all_candidates`.
pub fn generate_all_candidates(
    puzzle: &Puzzle,
    seed: usize,
    all_positions: &CellSet,
    pre: &PreBoundaries,
    symbol_of: &HashMap<usize, usize>,
    symbol_types: &[String],
) -> Vec<CellSet> {
    let h = puzzle.height;
    let w = puzzle.width;
    let n_bits = h * w;
    let is_multi = symbol_types.len() >= 2;

    let mut visited: HashSet<CellSet> = HashSet::new();
    let mut results: Vec<CellSet> = Vec::new();

    let mut initial = CellSet::new(n_bits);
    initial.insert(seed);
    visited.insert(initial.clone());

    let (sr, sc) = (seed / w, seed % w);
    let mut initial_frontier = CellSet::new(n_bits);
    for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
        let nr = sr as i32 + dr;
        let nc = sc as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if all_positions.contains(nidx) && !pre.contains(sr, sc, nr as usize, nc as usize) {
                initial_frontier.insert(nidx);
            }
        }
    }
    let initial_syms: u64 = symbol_of.get(&seed).map(|ti| 1u64 << ti).unwrap_or(0);

    let mut queue: std::collections::VecDeque<(CellSet, CellSet, u64)> =
        std::collections::VecDeque::new();
    queue.push_back((initial, initial_frontier, initial_syms));

    let dirs = [(-1i32, 0), (1, 0), (0, -1), (0, 1)];
    while let Some((current, frontier, syms)) = queue.pop_front() {
        if results.len() >= CANDIDATE_CAP {
            break;
        }
        if visited.len() >= VISITED_CAP {
            // OOM止血: visited 无界增长是 rose OOM 的根因。bail out 返回部分
            // results（caller graceful，rose_growth 已修 deadline 不再挂死）。
            // 必须 break，不能"停插入继续检查"（见 VISITED_CAP 注释）。
            break;
        }
        // M1 (soundness): every BFS state is a candidate.  For multi-symbol
        // puzzles the old B-MB early-stop (`syms == all_required → push,
        // continue`) never generated same-symbol-set SUPERSETS (all required
        // types plus symbol-less filler cells), so the true solution region was
        // missing from the exact-cover rows and multi-symbol rose puzzles were
        // falsely reported unsolvable (bug M1; doc 15 §2 A3 was unsound).
        // Expansion stays cheap: the symbol-skip below refuses further symbol
        // cells of already-present types, so supersets only add filler cells,
        // and CANDIDATE_CAP / MAX_CANDIDATE_CELLS / VISITED_CAP bound the cost.
        results.push(current.clone());
        if current.len() >= MAX_CANDIDATE_CELLS {
            continue;
        }
        for cell in frontier.iter().collect::<Vec<_>>() {
            let (cr, cc) = (cell / w, cell % w);
            let cell_sym = symbol_of.get(&cell).copied();
            let mut skip = false;
            if let Some(ti) = cell_sym {
                if is_multi && (syms & (1u64 << ti)) != 0 {
                    skip = true;
                }
            }
            if !skip {
                for (dr, dc) in dirs {
                    let nr = cr as i32 + dr;
                    let nc = cc as i32 + dc;
                    if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                        let nidx = nr as usize * w + nc as usize;
                        if current.contains(nidx)
                            && pre.contains(cr, cc, nr as usize, nc as usize)
                        {
                            skip = true;
                            break;
                        }
                    }
                }
            }
            if skip {
                continue;
            }
            let mut new_fs = current.clone();
            new_fs.insert(cell);
            if visited.contains(&new_fs) {
                continue;
            }
            visited.insert(new_fs.clone());
            let new_syms = syms | cell_sym.map(|ti| 1u64 << ti).unwrap_or(0);
            let mut new_frontier = frontier.clone();
            new_frontier.remove(cell);
            for (dr, dc) in dirs {
                let nr = cr as i32 + dr;
                let nc = cc as i32 + dc;
                if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                    let nidx = nr as usize * w + nc as usize;
                    if all_positions.contains(nidx)
                        && !current.contains(nidx)
                        && !pre.contains(cr, cc, nr as usize, nc as usize)
                    {
                        new_frontier.insert(nidx);
                    }
                }
            }
            queue.push_back((new_fs, new_frontier, new_syms));
        }
    }
    results
}

/// Port of `region_match._can_partition`: every remaining cell must be reachable
/// from some symbol seed without crossing a pre-boundary, and every connected
/// component must have at least `min_component_cells` cells.
fn can_partition(
    remaining: &CellSet,
    seed_cells: &CellSet,
    pre: &PreBoundaries,
    h: usize,
    w: usize,
    min_component_cells: usize,
) -> bool {
    let active = {
        let mut s = CellSet::new(remaining.len_bits());
        // `remaining & seed_cells` — iterate the smaller set.
        for idx in remaining.iter() {
            if seed_cells.contains(idx) {
                s.insert(idx);
            }
        }
        s
    };
    if active.is_empty() {
        return remaining.is_empty();
    }

    let mut visited = CellSet::new(remaining.len_bits());
    let mut queue: std::collections::VecDeque<usize> = active.iter().collect();
    let dirs = [(-1i32, 0), (1, 0), (0, -1), (0, 1)];
    while let Some(idx) = queue.pop_front() {
        if visited.contains(idx) {
            continue;
        }
        visited.insert(idx);
        let (r, c) = (idx / w, idx % w);
        for (dr, dc) in dirs {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                let nidx = nr as usize * w + nc as usize;
                if remaining.contains(nidx)
                    && !visited.contains(nidx)
                    && !pre.contains(r, c, nr as usize, nc as usize)
                {
                    queue.push_back(nidx);
                }
            }
        }
    }
    if visited.len() != remaining.len() {
        return false;
    }
    if min_component_cells <= 1 {
        return true;
    }

    // Every connected component must be >= min_component_cells.
    let mut comp_visited = CellSet::new(remaining.len_bits());
    for start in remaining.iter() {
        if comp_visited.contains(start) {
            continue;
        }
        let mut comp_size = 0usize;
        let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        q.push_back(start);
        while let Some(idx) = q.pop_front() {
            if comp_visited.contains(idx) {
                continue;
            }
            comp_visited.insert(idx);
            comp_size += 1;
            let (r, c) = (idx / w, idx % w);
            for (dr, dc) in dirs {
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                    let nidx = nr as usize * w + nc as usize;
                    if remaining.contains(nidx)
                        && !comp_visited.contains(nidx)
                        && !pre.contains(r, c, nr as usize, nc as usize)
                    {
                        q.push_back(nidx);
                    }
                }
            }
        }
        if comp_size < min_component_cells {
            return false;
        }
    }
    true
}

/// Port of `region_match._check_boundaries_partial`: no pre-boundary edge may
/// have both endpoints in the same assigned region.  `w` is the grid width so
/// (r,c) → r*w+c.
fn check_boundaries_partial(region_of: &[Option<usize>], pre: &PreBoundaries, w: usize) -> bool {
    for [r1, c1, r2, c2] in pre.iter() {
        let rid1 = region_of[r1 * w + c1];
        let rid2 = region_of[r2 * w + c2];
        if let (Some(a), Some(b)) = (rid1, rid2) {
            if a == b {
                return false;
            }
        }
    }
    true
}

/// Enumerate per-region size tuples (parts[depth] ∈ allowed[depth]) summing to
/// `total`, each ≥ `min_val`.  Port of `_enum_area_combos_bounded`.
fn enum_area_combos_bounded(
    total: usize,
    parts: usize,
    min_val: usize,
    allowed: &[Vec<usize>],
    depth: usize,
    cur: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    // B-LZ: stop once the combo list reaches MAX_COMBOS — prevents the
    // astronomical combo counts (0882 m=22 → 2.2e20) from OOMing before any
    // combo is tried. The partial set (sorted below) is still searched.
    if out.len() >= MAX_COMBOS {
        return;
    }
    if depth == parts - 1 {
        if allowed[depth].contains(&total) && total >= min_val {
            cur.push(total);
            out.push(cur.clone());
            cur.pop();
        }
        return;
    }
    for &sz in &allowed[depth] {
        if out.len() >= MAX_COMBOS {
            return;
        }
        if sz < min_val {
            continue;
        }
        if sz > total - min_val * (parts - depth - 1) {
            continue;
        }
        cur.push(sz);
        enum_area_combos_bounded(total - sz, parts, min_val, allowed, depth + 1, cur, out);
        cur.pop();
    }
}

/// Main entry — port of `region_match.solve_by_region_match` (single symbol).
///
/// NOTE: the previous hard rejection of `puzzle_piece` / `shape_pool` puzzles
/// was lifted — `solve_rose` now pre-resolves `shape_pattern`-pinned regions
/// (see `puzzle_piece_pin`) and calls this with the reduced `all_positions`
/// covering only the remaining (shape-rule-free) cells.  Callers must ensure
/// every `shape_pattern` cell is excluded from `all_positions` before calling,
/// otherwise the produced regions may violate the shape rule (caught by
/// `accept_if_valid` / `validate::validate`).
pub fn solve_by_region_match(
    puzzle: &Puzzle,
    pre: &PreBoundaries,
    symbol_types: &[String],
    m: usize,
    all_positions: &CellSet,
    start: &Instant,
    timeout_ms: u64,
) -> Option<Vec<crate::types::RegionInfo>> {
    let h = puzzle.height;
    let w = puzzle.width;
    let total = all_positions.len();

    // Most constrained symbol type (among cells in `all_positions` — pre-pinned
    // cells are excluded so their symbols don't seed a region).
    let mut best_type_idx = 0usize;
    let mut best_count = usize::MAX;
    for (ti, st) in symbol_types.iter().enumerate() {
        let count = (0..h)
            .flat_map(|r| (0..w).map(move |c| (r, c)))
            .filter(|&(r, c)| {
                !puzzle.cells[r][c].blocked
                    && all_positions.contains(r * w + c)
                    && puzzle.cells[r][c].symbol.as_deref() == Some(st.as_str())
            })
            .count();
        if count < best_count {
            best_count = count;
            best_type_idx = ti;
        }
    }

    let mut seeds: Vec<usize> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            let idx = r * w + c;
            if !puzzle.cells[r][c].blocked
                && all_positions.contains(idx)
                && puzzle.cells[r][c].symbol.as_deref()
                    == Some(symbol_types[best_type_idx].as_str())
            {
                seeds.push(idx);
            }
        }
    }
    seeds.sort_unstable();
    if seeds.len() != m {
        return None;
    }

    // All symbol cells in `all_positions` (for reachability).  Pre-pinned
    // cells are excluded so reachability is computed over the remainder only.
    let mut all_seed_cells = CellSet::new(total_bits(h, w));
    let mut symbol_of: HashMap<usize, usize> = HashMap::new();
    for r in 0..h {
        for c in 0..w {
            let idx = r * w + c;
            if puzzle.cells[r][c].blocked || !all_positions.contains(idx) {
                continue;
            }
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                all_seed_cells.insert(idx);
                if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                    symbol_of.insert(idx, ti);
                }
            }
        }
    }

    // Generate candidates per seed (single-symbol path).
    let mut all_candidates: Vec<Vec<CellSet>> = Vec::new();
    for &seed in &seeds {
        let t0 = Instant::now();
        let cands = generate_all_candidates(puzzle, seed, all_positions, pre, &symbol_of, symbol_types);
        if crate::aog_debug_enabled() {
            eprintln!(
                "rose: seed {} -> {} candidates in {:?}",
                seed,
                cands.len(),
                t0.elapsed()
            );
        }
        if cands.is_empty() {
            return None;
        }
        all_candidates.push(cands);
    }
    if crate::aog_debug_enabled() {
        eprintln!(
            "rose: candidates per seed: {:?}",
            all_candidates.iter().map(|c| c.len()).collect::<Vec<_>>()
        );
    }

    // Pre-filter by area and region-size bounds: each candidate must be within
    // [range.min, range.max] (or precise) and leave >= 1 cell per other seed.
    let (min_sz, max_sz) = crate::shapes::area_bounds(puzzle);
    for cands in all_candidates.iter_mut() {
        let max_for_this = max_sz.min(total - (m - 1));
        cands.retain(|c| c.len() >= min_sz && c.len() <= max_for_this);
        if cands.is_empty() {
            return None;
        }
    }

    // Pre-filter by component reachability.
    let n = symbol_types.len();
    for cands in all_candidates.iter_mut() {
        cands.retain(|c| {
            let mut remaining = all_positions.clone();
            // remaining = all_positions - cand
            for idx in c.iter() {
                remaining.remove(idx);
            }
            can_partition(&remaining, &all_seed_cells, pre, h, w, n)
        });
        if cands.is_empty() {
            return None;
        }
    }

    for cands in all_candidates.iter_mut() {
        cands.sort_by_key(|c| c.len());
    }

    // Sizes and by-size lookup.
    let mut seed_size_sets: Vec<Vec<usize>> = Vec::new();
    let mut candidates_by_size: Vec<HashMap<usize, Vec<CellSet>>> = Vec::new();
    for cands in &all_candidates {
        let mut sizes: Vec<usize> = cands.iter().map(|c| c.len()).collect();
        sizes.sort_unstable();
        sizes.dedup();
        seed_size_sets.push(sizes);
        let mut by_size: HashMap<usize, Vec<CellSet>> = HashMap::new();
        for c in cands {
            by_size.entry(c.len()).or_default().push(c.clone());
        }
        candidates_by_size.push(by_size);
    }

    let min_area_per_region = min_sz.max(n);
    let mut combos: Vec<Vec<usize>> = Vec::new();
    enum_area_combos_bounded(
        total,
        m,
        min_area_per_region,
        &seed_size_sets,
        0,
        &mut Vec::new(),
        &mut combos,
    );
    combos.sort_by_key(|c| c.iter().max().unwrap_or(&0) - c.iter().min().unwrap_or(&0));

    let deadline = *start + std::time::Duration::from_millis(timeout_ms);
    let mut region_of: Vec<Option<usize>> = vec![None; h * w];
    if crate::aog_debug_enabled() {
        eprintln!(
            "rose: {} area combos (min_area={})",
            combos.len(),
            min_area_per_region
        );
    }

    for combo in &combos {
        if Instant::now() >= deadline {
            break;
        }
        let mut sized: Vec<Vec<CellSet>> = Vec::new();
        let mut feasible = true;
        for (i, target_sz) in combo.iter().enumerate() {
            match candidates_by_size[i].get(target_sz) {
                Some(list) => sized.push(list.clone()),
                None => {
                    feasible = false;
                    break;
                }
            }
        }
        if !feasible {
            continue;
        }
        let combo_deadline =
            Instant::now() + std::time::Duration::from_millis(PER_COMBO_TIMEOUT_MS);
        let mut assignment: Vec<Option<CellSet>> = vec![None; m];
        let covered = CellSet::new(total_bits(h, w));
        if match_regions_mrv(
            &sized,
            all_positions,
            total,
            w,
            &mut region_of,
            pre,
            covered,
            &mut assignment,
            combo_deadline,
            deadline,
        ) {
            // Rebuild region info from region_of.
            let regions = super::build_regions(&region_of, h, w);
            return Some(regions);
        }
    }
    None
}

fn total_bits(h: usize, w: usize) -> usize {
    h * w
}

/// Recursive exact cover with MRV seed selection.  Port of
/// `region_match._match_regions_mrv`.
#[allow(clippy::too_many_arguments)]
fn match_regions_mrv(
    sized: &[Vec<CellSet>],
    all_positions: &CellSet,
    total: usize,
    w: usize,
    region_of: &mut [Option<usize>],
    pre: &PreBoundaries,
    covered: CellSet,
    assignment: &mut Vec<Option<CellSet>>,
    combo_deadline: Instant,
    deadline: Instant,
) -> bool {
    if Instant::now() >= combo_deadline || Instant::now() >= deadline {
        return false;
    }
    // Unassigned seed with fewest compatible candidates.
    let mut best_idx = usize::MAX;
    let mut best_count = usize::MAX;
    let mut best_cands: Vec<CellSet> = Vec::new();
    let mut unassigned_count = 0usize;
    for (idx, cands) in sized.iter().enumerate() {
        if assignment[idx].is_some() {
            continue;
        }
        unassigned_count += 1;
        let mut count = 0usize;
        let mut compat: Vec<CellSet> = Vec::new();
        for c in cands {
            if c.is_disjoint(&covered) {
                count += 1;
                compat.push(c.clone());
            }
        }
        if count == 0 {
            return false;
        }
        if count < best_count {
            best_count = count;
            best_idx = idx;
            best_cands = compat;
        }
    }
    if best_idx == usize::MAX {
        return covered.len() == total;
    }

    let remaining_cells = total - covered.len();
    let remaining_seeds = unassigned_count - 1;
    for cand in &best_cands {
        let sz = cand.len();
        if sz > remaining_cells - remaining_seeds {
            continue;
        }
        for idx in cand.iter() {
            region_of[idx] = Some(best_idx);
        }
        if !check_boundaries_partial(region_of, pre, w) {
            for idx in cand.iter() {
                region_of[idx] = None;
            }
            continue;
        }
        let mut new_covered = covered.clone();
        new_covered.union_into(cand);
        assignment[best_idx] = Some(cand.clone());
        if match_regions_mrv(
            sized,
            all_positions,
            total,
            w,
            region_of,
            pre,
            new_covered,
            assignment,
            combo_deadline,
            deadline,
        ) {
            return true;
        }
        assignment[best_idx] = None;
        for idx in cand.iter() {
            region_of[idx] = None;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `VISITED_CAP` must exceed `CANDIDATE_CAP` (so the results cap remains the
    /// normal completeness ceiling) and be bounded (OOM safety).
    #[test]
    fn visited_cap_invariants() {
        assert!(VISITED_CAP > CANDIDATE_CAP);
        assert!(VISITED_CAP > 0);
        assert!(VISITED_CAP <= 10_000_000, "VISITED_CAP too high — OOM risk");
    }

    /// On a tiny grid the BFS exhausts naturally without approaching either cap.
    /// Guards against a regression where the bail-out `break` is placed before
    /// `results.push` (emptying the output) or the loop structure is broken.
    #[test]
    fn generate_all_candidates_terminates_on_small_grid() {
        let mut puzzle = crate::types::Puzzle {
            height: 2,
            width: 2,
            cells: vec![
                vec![crate::types::Cell::new(0, 0), crate::types::Cell::new(0, 1)],
                vec![crate::types::Cell::new(1, 0), crate::types::Cell::new(1, 1)],
            ],
            h_edges: vec![vec![crate::types::Edge::default(); 1]; 2],
            v_edges: vec![vec![crate::types::Edge::default(); 2]; 1],
            vertices: vec![],
            rules: vec![crate::types::Rule {
                ctype: "rose_window".into(),
                params: Default::default(),
            }],
            shape_pool: vec![],
            outer_boundaries: vec![],
        };
        puzzle.cells[0][0].symbol = Some("A".into());
        let h = puzzle.height;
        let w = puzzle.width;
        let n_bits = h * w;
        let mut all_positions = CellSet::new(n_bits);
        for r in 0..h {
            for c in 0..w {
                all_positions.insert(r * w + c);
            }
        }
        let pre = PreBoundaries::from_puzzle(&puzzle);
        let symbol_types = vec!["A".to_string()];
        let mut symbol_of = HashMap::new();
        symbol_of.insert(0usize, 0usize);
        let cands = generate_all_candidates(&puzzle, 0, &all_positions, &pre, &symbol_of, &symbol_types);
        assert!(!cands.is_empty(), "must return at least the singleton region");
        assert!(cands.iter().all(|c| c.contains(0)), "every candidate contains the seed");
        assert!(cands.len() <= CANDIDATE_CAP);
    }
}
