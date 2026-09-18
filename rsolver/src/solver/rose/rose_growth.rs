//! Rose-window BFS growth + constraint repair — port of
//! `src/solver/rose_growth.py`.  Fallback used when region_match times out or
//! misses.

use std::collections::{HashSet, VecDeque};
use crate::clock::Instant;

use crate::types::Puzzle;

use super::cells::{CellSet, PreBoundaries};
use super::build_regions;

const SWAP_REPAIR_ITER: usize = 500;
const MULTI_REPAIR_ITER: usize = 200;

const DIRS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// Region ids touching `idx` across non-pre-boundary edges.
#[inline]
fn adjacent_regions(
    region_of: &[Option<usize>],
    idx: usize,
    pre: &PreBoundaries,
    h: usize,
    w: usize,
) -> HashSet<usize> {
    let (r, c) = (idx / w, idx % w);
    let mut adj: HashSet<usize> = HashSet::new();
    for (dr, dc) in DIRS {
        let nr = r as i32 + dr;
        let nc = c as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if let Some(nrid) = region_of[nidx] {
                if !pre.contains(r, c, nr as usize, nc as usize) {
                    adj.insert(nrid);
                }
            }
        }
    }
    adj
}

/// Pre-boundary edge whose two endpoints currently share a region id.
#[inline]
fn collect_violations(region_of: &[Option<usize>], pre: &PreBoundaries, w: usize) -> Vec<[usize; 4]> {
    pre.iter()
        .filter(|[r1, c1, r2, c2]| {
            let a = region_of[r1 * w + c1];
            let b = region_of[r2 * w + c2];
            matches!((a, b), (Some(x), Some(y)) if x == y)
        })
        .collect()
}

/// True when some neighbour of `(r, c)` sits in region `rid` behind a
/// pre-boundary edge — i.e. adding `(r, c)` to `rid` would violate.
#[inline]
fn has_pre_neighbor_in(
    region_of: &[Option<usize>],
    pre: &PreBoundaries,
    rid: usize,
    r: usize,
    c: usize,
    h: usize,
    w: usize,
) -> bool {
    for (dr, dc) in DIRS {
        let nr = r as i32 + dr;
        let nc = c as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if region_of[nidx] == Some(rid) && pre.contains(r, c, nr as usize, nc as usize) {
                return true;
            }
        }
    }
    false
}

/// `can_move_neighbor` half of the chain move: no neighbour of `(nru, ncu)`
/// (other than the mover itself) lies in region `cur` behind a pre-boundary.
#[inline]
fn can_move_neighbor(
    region_of: &[Option<usize>],
    pre: &PreBoundaries,
    cur: usize,
    cell_r: usize,
    cell_c: usize,
    nru: usize,
    ncu: usize,
    h: usize,
    w: usize,
) -> bool {
    for (ddr, ddc) in DIRS {
        let nnr = nru as i32 + ddr;
        let nnc = ncu as i32 + ddc;
        if nnr >= 0 && nnc >= 0 && (nnr as usize) < h && (nnc as usize) < w {
            let nnidx = nnr as usize * w + nnc as usize;
            if nnidx != cell_r * w + cell_c
                && region_of[nnidx] == Some(cur)
                && pre.contains(nru, ncu, nnr as usize, nnc as usize)
            {
                return false;
            }
        }
    }
    true
}

/// `can_move_self` half of the chain move: no neighbour of `(cell_r, cell_c)`
/// (other than the swap target `nidx`) lies in region `n_rid` behind a
/// pre-boundary.
#[inline]
fn can_move_self(
    region_of: &[Option<usize>],
    pre: &PreBoundaries,
    n_rid: usize,
    nidx: usize,
    cell_r: usize,
    cell_c: usize,
    h: usize,
    w: usize,
) -> bool {
    for (ddr, ddc) in DIRS {
        let nnr = cell_r as i32 + ddr;
        let nnc = cell_c as i32 + ddc;
        if nnr >= 0 && nnc >= 0 && (nnr as usize) < h && (nnc as usize) < w {
            let nnidx = nnr as usize * w + nnc as usize;
            if nnidx != nidx
                && region_of[nnidx] == Some(n_rid)
                && pre.contains(cell_r, cell_c, nnr as usize, nnc as usize)
            {
                return false;
            }
        }
    }
    true
}

/// Wavefront growth: repeatedly attach the unassigned cell with the most
/// adjacent regions to the smallest such region.  Returns `false` when the
/// deadline expired (caller must bail out).
fn wavefront_growth(
    region_of: &mut [Option<usize>],
    region_cells: &mut [CellSet],
    unassigned: &mut CellSet,
    pre: &PreBoundaries,
    h: usize,
    w: usize,
    deadline: Instant,
) -> bool {
    let mut steps: u64 = 0;
    while !unassigned.is_empty() {
        steps += 1;
        if steps % 4096 == 0 && Instant::now() >= deadline {
            return false;
        }
        let mut best_cell: Option<usize> = None;
        let mut best_adj: Vec<usize> = Vec::new();
        for idx in unassigned.iter() {
            let adj = adjacent_regions(region_of, idx, pre, h, w);
            if adj.len() > best_adj.len() {
                best_cell = Some(idx);
                best_adj = adj.into_iter().collect();
            }
        }
        let Some(cell) = best_cell else { break };
        if best_adj.is_empty() {
            break;
        }
        best_adj.sort_by_key(|&rid| region_cells[rid].len());
        let (cr, cc) = (cell / w, cell % w);
        let mut assigned = false;
        for &rid in &best_adj {
            if would_violate(region_of, cr, cc, rid, pre, w) {
                continue;
            }
            region_of[cell] = Some(rid);
            region_cells[rid].insert(cell);
            unassigned.remove(cell);
            assigned = true;
            break;
        }
        if !assigned {
            let rid = best_adj[0];
            region_of[cell] = Some(rid);
            region_cells[rid].insert(cell);
            unassigned.remove(cell);
        }
    }
    true
}

/// Is the cell set 4-connected?  Guards `rose_growth`'s repair moves so they
/// never slice a region in two (doc 28).  Regions are small, so a plain BFS
/// over the set is cheap.
fn is_connected_set(cells: &CellSet, w: usize) -> bool {
    let n = cells.len();
    if n <= 1 {
        return true;
    }
    let mut max_idx = 0usize;
    for i in cells.iter() {
        if i > max_idx {
            max_idx = i;
        }
    }
    let mut seen = vec![false; max_idx + 1];
    let start = cells.iter().next().unwrap();
    let mut q = std::collections::VecDeque::new();
    q.push_back(start);
    seen[start] = true;
    let mut reached = 0usize;
    while let Some(cur) = q.pop_front() {
        reached += 1;
        let (r, c) = (cur / w, cur % w);
        for (dr, dc) in DIRS {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 {
                continue;
            }
            let ni = nr as usize * w + nc as usize;
            if ni <= max_idx && !seen[ni] && cells.contains(ni) {
                seen[ni] = true;
                q.push_back(ni);
            }
        }
    }
    reached == n
}

/// First repair strategy: move one endpoint of a violating pre-boundary edge
/// into an adjacent region that has no pre-boundary conflict with it.
///
/// Every move is guarded by `is_connected_set` on the region the cell leaves:
/// without that guard the repair can slice a region in two, and the resulting
/// candidate is rejected by `validate` (doc 28 — 25 FAILs showed
/// `rose:validation_failed`, traced to this path).
fn try_swap_fix(
    region_of: &mut [Option<usize>],
    region_cells: &mut [CellSet],
    violations: &[[usize; 4]],
    pre: &PreBoundaries,
    h: usize,
    w: usize,
) -> bool {
    for [r1, c1, r2, c2] in violations {
        for (cell_r, cell_c) in [(*r1, *c1), (*r2, *c2)] {
            let Some(cur) = region_of[cell_r * w + cell_c] else {
                continue;
            };
            let mut alts: Vec<usize> = Vec::new();
            for (dr, dc) in DIRS {
                let nr = cell_r as i32 + dr;
                let nc = cell_c as i32 + dc;
                if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                    let nidx = nr as usize * w + nc as usize;
                    if let Some(nrid) = region_of[nidx] {
                        if nrid != cur && !pre.contains(cell_r, cell_c, nr as usize, nc as usize) {
                            alts.push(nrid);
                        }
                    }
                }
            }
            alts.sort_by_key(|&rid| region_cells[rid].len());
            for &nrid in &alts {
                if has_pre_neighbor_in(region_of, pre, nrid, cell_r, cell_c, h, w) {
                    continue;
                }
                let idx = cell_r * w + cell_c;
                region_of[idx] = Some(nrid);
                region_cells[cur].remove(idx);
                region_cells[nrid].insert(idx);
                // The cell left `cur`; if that split it, undo and try the next.
                if !is_connected_set(&region_cells[cur], w) {
                    region_of[idx] = Some(cur);
                    region_cells[cur].insert(idx);
                    region_cells[nrid].remove(idx);
                    continue;
                }
                return true;
            }
        }
    }
    false
}

/// Second repair strategy: swap a violating cell with an adjacent cell of
/// another region (a "chain move") when a plain reassignment is impossible.
fn try_chain_move(
    region_of: &mut [Option<usize>],
    region_cells: &mut [CellSet],
    violations: &[[usize; 4]],
    pre: &PreBoundaries,
    h: usize,
    w: usize,
) -> bool {
    for [r1, c1, r2, c2] in violations {
        for (cell_r, cell_c) in [(*r1, *c1), (*r2, *c2)] {
            let Some(cur) = region_of[cell_r * w + cell_c] else {
                continue;
            };
            for (dr, dc) in DIRS {
                let nr = cell_r as i32 + dr;
                let nc = cell_c as i32 + dc;
                if nr < 0 || nc < 0 || (nr as usize) >= h || (nc as usize) >= w {
                    continue;
                }
                let (nru, ncu) = (nr as usize, nc as usize);
                let nidx = nru * w + ncu;
                let Some(n_rid) = region_of[nidx] else { continue };
                if n_rid == cur || pre.contains(cell_r, cell_c, nru, ncu) {
                    continue;
                }
                if !can_move_neighbor(region_of, pre, cur, cell_r, cell_c, nru, ncu, h, w) {
                    continue;
                }
                if can_move_self(region_of, pre, n_rid, nidx, cell_r, cell_c, h, w) {
                    let a = nidx;
                    let b = cell_r * w + cell_c;
                    region_of[a] = Some(cur);
                    region_cells[n_rid].remove(a);
                    region_cells[cur].insert(a);
                    region_of[b] = Some(n_rid);
                    region_cells[cur].remove(b);
                    region_cells[n_rid].insert(b);
                    // Both regions changed hands; a swap that disconnects either
                    // one is not a repair (doc 28).
                    if !is_connected_set(&region_cells[cur], w)
                        || !is_connected_set(&region_cells[n_rid], w)
                    {
                        region_of[a] = Some(n_rid);
                        region_cells[cur].remove(a);
                        region_cells[n_rid].insert(a);
                        region_of[b] = Some(cur);
                        region_cells[n_rid].remove(b);
                        region_cells[cur].insert(b);
                        continue;
                    }
                    return true;
                }
            }
        }
    }
    false
}

/// One iteration of the swap-repair loop; `false` means no further progress is
/// possible (either clean or stuck).
fn swap_repair_iteration(
    region_of: &mut [Option<usize>],
    region_cells: &mut [CellSet],
    pre: &PreBoundaries,
    h: usize,
    w: usize,
) -> bool {
    let violations = collect_violations(region_of, pre, w);
    if violations.is_empty() {
        return false;
    }
    if try_swap_fix(region_of, region_cells, &violations, pre, h, w) {
        return true;
    }
    try_chain_move(region_of, region_cells, &violations, pre, h, w)
}

/// Bundles the per-solve run state shared by `solve_singlesymbol` /
/// `solve_multisymbol`, replacing the 9-11 flat parameters they used to take.
/// Pure signature-level change — no solving logic is altered.
struct RoseGrowthCtx<'a> {
    puzzle: &'a Puzzle,
    pre: &'a PreBoundaries,
    m: usize,
    seeds: &'a [usize],
    h: usize,
    w: usize,
    n_bits: usize,
    all_positions: &'a CellSet,
    deadline: Instant,
}

/// Port of `rose_growth.solve_rose_growth`.
pub fn solve_rose_growth(
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
    let n_bits = h * w;
    let deadline = *start + std::time::Duration::from_millis(timeout_ms);

    // Seeds = cells of the first symbol type.
    let mut seeds: Vec<usize> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if !puzzle.cells[r][c].blocked
                && puzzle.cells[r][c].symbol.as_deref() == Some(symbol_types[0].as_str())
            {
                seeds.push(r * w + c);
            }
        }
    }
    if seeds.len() != m {
        return None;
    }

    let symbol_of = super::cells::symbol_index_map(puzzle, symbol_types);
    let ctx = RoseGrowthCtx {
        puzzle,
        pre,
        m,
        seeds: &seeds,
        h,
        w,
        n_bits,
        all_positions,
        deadline,
    };
    let result = if symbol_types.len() >= 2 {
        solve_multisymbol(&ctx, symbol_types, &symbol_of)
    } else {
        solve_singlesymbol(&ctx)
    };
    result
}

fn solve_singlesymbol(ctx: &RoseGrowthCtx) -> Option<Vec<crate::types::RegionInfo>> {
    let puzzle = ctx.puzzle;
    let pre = ctx.pre;
    let m = ctx.m;
    let seeds = ctx.seeds;
    let h = ctx.h;
    let w = ctx.w;
    let all_positions = ctx.all_positions;
    let n_bits = ctx.n_bits;
    let deadline = ctx.deadline;
    let mut region_of = vec![None; n_bits];
    let mut region_cells: Vec<CellSet> = vec![CellSet::new(n_bits); m];
    for (i, &seed) in seeds.iter().enumerate() {
        region_of[seed] = Some(i);
        region_cells[i].insert(seed);
    }
    let mut unassigned = all_positions.clone();
    for &s in seeds {
        unassigned.remove(s);
    }

    // Wavefront growth (delegates to the shared, byte-identical helper).
    if !wavefront_growth(
        &mut region_of,
        &mut region_cells,
        &mut unassigned,
        pre,
        h,
        w,
        deadline,
    ) {
        return None;
    }

    // Swap repair (delegates to the shared, byte-identical helper).  Runs up to
    // SWAP_REPAIR_ITER passes, stopping as soon as a pass makes no progress
    // (clean or stuck) — exactly the original early-exit semantics.
    for _ in 0..SWAP_REPAIR_ITER {
        if !swap_repair_iteration(&mut region_of, &mut region_cells, pre, h, w) {
            break;
        }
    }

    let symbol_present: Vec<bool> = (0..n_bits)
        .map(|idx| puzzle.cells[idx / w][idx % w].symbol.is_some())
        .collect();
    repair_symbol_distribution(&mut region_of, &mut region_cells, &symbol_present, m, h, w);

    // Final: each region must have exactly one symbol.
    for i in 0..m {
        let sym_count = region_cells[i]
            .iter()
            .filter(|&idx| puzzle.cells[idx / w][idx % w].symbol.is_some())
            .count();
        if sym_count != 1 {
            return None;
        }
    }
    Some(build_regions(&region_of, h, w))
}

fn solve_multisymbol(
    ctx: &RoseGrowthCtx,
    symbol_types: &[String],
    symbol_of: &std::collections::HashMap<usize, usize>,
) -> Option<Vec<crate::types::RegionInfo>> {
    let puzzle = ctx.puzzle;
    let pre = ctx.pre;
    let m = ctx.m;
    let seeds = ctx.seeds;
    let h = ctx.h;
    let w = ctx.w;
    let all_positions = ctx.all_positions;
    let n_bits = ctx.n_bits;
    let deadline = ctx.deadline;
    // Honor the caller's deadline in every potentially-long loop below. The
    // previous signature took `_deadline` (unused) — a latent hang that was
    // masked while region_match always found the solution, but surfaces as a
    // full-budget spin (RSS flat, no output, deadline never fires) when
    // region_match returns partial candidates (e.g. after a visited cap bail-out)
    // and this fallback can't repair them.
    if Instant::now() >= deadline {
        return None;
    }
    let mut boundary_endpoints = CellSet::new(n_bits);
    for [r1, c1, r2, c2] in pre.iter() {
        boundary_endpoints.insert(r1 * w + c1);
        boundary_endpoints.insert(r2 * w + c2);
    }
    let mut region_symbols: Vec<u64> = vec![0u64; m];
    let mut region_sizes: Vec<usize> = vec![1usize; m];
    let mut region_of = vec![None; n_bits];
    for (i, &seed) in seeds.iter().enumerate() {
        region_of[seed] = Some(i);
        region_symbols[i] = 1u64 << symbol_of.get(&seed).copied().unwrap_or(0);
    }

    grow_initial_regions(
        &mut region_of,
        &mut region_symbols,
        &mut region_sizes,
        pre,
        puzzle,
        seeds,
        symbol_of,
        &boundary_endpoints,
        h,
        w,
    );

    let mut unassigned = all_positions.clone();
    for idx in region_of.iter().enumerate().filter_map(|(i, r)| r.map(|_| i)) {
        unassigned.remove(idx);
    }

    // Second pass: assign leftovers to smallest compatible region.
    if !assign_leftovers(
        &mut region_of,
        &mut region_symbols,
        &mut region_sizes,
        &mut unassigned,
        symbol_of,
        pre,
        deadline,
        h,
        w,
    ) {
        return None;
    }

    // Repair (multi-symbol).
    for _ in 0..MULTI_REPAIR_ITER {
        if !repair_multisymbol(
            &mut region_of,
            &mut region_symbols,
            &mut region_sizes,
            pre,
            symbol_of,
            h,
            w,
        ) {
            break;
        }
    }

    let all_mask: u64 = (1u64 << symbol_types.len()) - 1;
    if region_symbols.iter().any(|&s| s != all_mask) {
        return None;
    }
    if !unassigned.is_empty() {
        return None;
    }
    Some(build_regions(&region_of, h, w))
}

/// Port of `rose_growth._would_violate`.
fn would_violate(
    region_of: &[Option<usize>],
    r: usize,
    c: usize,
    rid: usize,
    pre: &PreBoundaries,
    w: usize,
) -> bool {
    let h = region_of.len() / w;
    for (dr, dc) in DIRS {
        let nr = r as i32 + dr;
        let nc = c as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if region_of[nidx] == Some(rid) && pre.contains(r, c, nr as usize, nc as usize) {
                return true;
            }
        }
    }
    false
}

/// Port of `rose_growth._repair_symbol_distribution`.  `symbol_present[cell]`
/// is true when that cell carries a rose symbol.
fn repair_symbol_distribution(
    region_of: &mut [Option<usize>],
    region_cells: &mut [CellSet],
    symbol_present: &[bool],
    m: usize,
    h: usize,
    w: usize,
) -> bool {
    let sym_count = |set: &CellSet| set.iter().filter(|&idx| symbol_present[idx]).count();
    for _ in 0..200 {
        let excess: Vec<usize> = (0..m).filter(|&i| sym_count(&region_cells[i]) > 1).collect();
        let deficit: Vec<usize> = (0..m).filter(|&i| sym_count(&region_cells[i]) == 0).collect();
        if excess.is_empty() && deficit.is_empty() {
            return true;
        }
        if excess.is_empty() || deficit.is_empty() {
            break;
        }
        let mut moved = false;
        for &ei in &excess {
            for idx in region_cells[ei].iter().collect::<Vec<_>>() {
                if symbol_present[idx] {
                    continue;
                }
                let (r, c) = (idx / w, idx % w);
                for &di in &deficit {
                    let mut adj = false;
                    for (dr, dc) in DIRS {
                        let nr = r as i32 + dr;
                        let nc = c as i32 + dc;
                        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                            let nidx = nr as usize * w + nc as usize;
                            if region_of[nidx] == Some(di) {
                                adj = true;
                                break;
                            }
                        }
                    }
                    if adj {
                        region_of[idx] = Some(di);
                        region_cells[ei].remove(idx);
                        region_cells[di].insert(idx);
                        // Moving a non-symbol cell out of an over-full region
                        // must not split it (doc 28).
                        if !is_connected_set(&region_cells[ei], w) {
                            region_of[idx] = Some(ei);
                            region_cells[ei].insert(idx);
                            region_cells[di].remove(idx);
                            continue;
                        }
                        moved = true;
                        break;
                    }
                }
                if moved {
                    break;
                }
            }
            if moved {
                break;
            }
        }
        if !moved {
            break;
        }
    }
    (0..m).all(|i| sym_count(&region_cells[i]) > 0)
}

/// True when some neighbour of `(nru, ncu)` already lies in region `rid` behind
/// a pre-boundary edge — i.e. attaching `(nru, ncu)` to `rid` would fuse two
/// regions a pre-boundary is meant to keep apart.  Pure helper for
/// `process_growth_neighbor`.
#[inline]
fn boundary_in_same_region(
    region_of: &[Option<usize>],
    pre: &PreBoundaries,
    nru: usize,
    ncu: usize,
    rid: usize,
    h: usize,
    w: usize,
) -> bool {
    for (ddr, ddc) in DIRS {
        let nnr = nru as i32 + ddr;
        let nnc = ncu as i32 + ddc;
        if nnr >= 0 && nnc >= 0 && (nnr as usize) < h && (nnc as usize) < w {
            let nnidx = nnr as usize * w + nnc as usize;
            if region_of[nnidx] == Some(rid) && pre.contains(nru, ncu, nnr as usize, nnc as usize) {
                return true;
            }
        }
    }
    false
}

/// Grow `region_of` one BFS step from `(rid)` in direction `(dr, dc)`.  Returns
/// `true` if the neighbour was attached.  Byte-identical to the inner body of
/// `solve_multisymbol`'s initial BFS.
#[inline]
fn process_growth_neighbor(
    region_of: &mut [Option<usize>],
    region_symbols: &mut [u64],
    region_sizes: &mut [usize],
    queue: &mut VecDeque<(usize, usize)>,
    r: usize,
    c: usize,
    rid: usize,
    dr: i32,
    dc: i32,
    pre: &PreBoundaries,
    puzzle: &Puzzle,
    symbol_of: &std::collections::HashMap<usize, usize>,
    boundary_endpoints: &CellSet,
    h: usize,
    w: usize,
) -> bool {
    let nr = r as i32 + dr;
    let nc = c as i32 + dc;
    if nr < 0 || nc < 0 || (nr as usize) >= h || (nc as usize) >= w {
        return false;
    }
    let (nru, ncu) = (nr as usize, nc as usize);
    let nidx = nru * w + ncu;
    if puzzle.cells[nru][ncu].blocked || region_of[nidx].is_some() {
        return false;
    }
    if pre.contains(r, c, nru, ncu) {
        return false;
    }
    let sym = symbol_of.get(&nidx).copied();
    if let Some(si) = sym {
        if (region_symbols[rid] & (1u64 << si)) != 0 {
            return false;
        }
    }
    if boundary_endpoints.contains(nidx)
        && boundary_in_same_region(region_of, pre, nru, ncu, rid, h, w)
    {
        return false;
    }
    region_of[nidx] = Some(rid);
    if let Some(si) = sym {
        region_symbols[rid] |= 1u64 << si;
    }
    region_sizes[rid] += 1;
    queue.push_back((nidx, rid));
    true
}

/// Initial rose-region BFS from every seed (multi-symbol path).  Same traversal
/// and pruning as the inlined loop it replaces.
fn grow_initial_regions(
    region_of: &mut [Option<usize>],
    region_symbols: &mut [u64],
    region_sizes: &mut [usize],
    pre: &PreBoundaries,
    puzzle: &Puzzle,
    seeds: &[usize],
    symbol_of: &std::collections::HashMap<usize, usize>,
    boundary_endpoints: &CellSet,
    h: usize,
    w: usize,
) {
    let mut queue: VecDeque<(usize, usize)> = seeds.iter().enumerate().map(|(i, &s)| (s, i)).collect();
    while let Some((idx, rid)) = queue.pop_front() {
        let (r, c) = (idx / w, idx % w);
        for (dr, dc) in DIRS {
            process_growth_neighbor(
                region_of,
                region_symbols,
                region_sizes,
                &mut queue,
                r,
                c,
                rid,
                dr,
                dc,
                pre,
                puzzle,
                symbol_of,
                boundary_endpoints,
                h,
                w,
            );
        }
    }
}

/// Try to assign one unassigned `idx` to its smallest compatible region.
/// Returns `true` if it assigned (caller then removes `idx` from `unassigned`).
#[inline]
fn assign_one_leftover(
    region_of: &mut [Option<usize>],
    region_symbols: &mut [u64],
    region_sizes: &mut [usize],
    idx: usize,
    symbol_of: &std::collections::HashMap<usize, usize>,
    pre: &PreBoundaries,
    h: usize,
    w: usize,
) -> bool {
    let (r, c) = (idx / w, idx % w);
    let mut candidates: HashSet<usize> = HashSet::new();
    for (dr, dc) in DIRS {
        let nr = r as i32 + dr;
        let nc = c as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if let Some(nrid) = region_of[nidx] {
                if !pre.contains(r, c, nr as usize, nc as usize) {
                    candidates.insert(nrid);
                }
            }
        }
    }
    if candidates.is_empty() {
        return false;
    }
    let sym = symbol_of.get(&idx).copied();
    let mut valid: Vec<usize> = candidates
        .into_iter()
        .filter(|&i| !(sym.is_some() && (region_symbols[i] & (1u64 << sym.unwrap())) != 0))
        .collect();
    if valid.is_empty() {
        return false;
    }
    valid.sort_by_key(|&i| region_sizes[i]);
    // The candidate filter above only checked the edge this cell would enter
    // through; joining `best` must not cross a pre-drawn boundary on any of the
    // cell's other three sides either (doc 28 — that gap let rose_growth emit
    // candidates whose region straddles a boundary, rejected by `validate`).
    let Some(&best) = valid.iter().find(|&&i| !would_violate(region_of, r, c, i, pre, w))
    else {
        return false;
    };
    region_of[idx] = Some(best);
    if let Some(si) = sym {
        region_symbols[best] |= 1u64 << si;
    }
    region_sizes[best] += 1;
    true
}

/// Second-pass assignment of leftover cells to the smallest compatible region.
/// Returns `false` only if the caller's deadline expired (signalling bail-out).
fn assign_leftovers(
    region_of: &mut [Option<usize>],
    region_symbols: &mut [u64],
    region_sizes: &mut [usize],
    unassigned: &mut CellSet,
    symbol_of: &std::collections::HashMap<usize, usize>,
    pre: &PreBoundaries,
    deadline: Instant,
    h: usize,
    w: usize,
) -> bool {
    if unassigned.is_empty() {
        return true;
    }
    let mut changed = true;
    let mut pass: u64 = 0;
    while changed {
        changed = false;
        pass += 1;
        if pass % 64 == 0 && Instant::now() >= deadline {
            return false;
        }
        for idx in unassigned.iter().collect::<Vec<_>>() {
            if assign_one_leftover(region_of, region_symbols, region_sizes, idx, symbol_of, pre, h, w) {
                unassigned.remove(idx);
                changed = true;
            }
        }
    }
    true
}

/// Region ids of `cur_rid`-free neighbours of `(cell_r, cell_c)` across
/// non-pre-boundary edges — candidates to receive this cell during repair.
#[inline]
fn collect_repair_neighbors(
    region_of: &[Option<usize>],
    pre: &PreBoundaries,
    cell_r: usize,
    cell_c: usize,
    cur_rid: usize,
    h: usize,
    w: usize,
) -> Vec<usize> {
    let mut neigh: HashSet<usize> = HashSet::new();
    for (dr, dc) in DIRS {
        let nr = cell_r as i32 + dr;
        let nc = cell_c as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if let Some(nrid) = region_of[nidx] {
                if nrid != cur_rid && !pre.contains(cell_r, cell_c, nr as usize, nc as usize) {
                    neigh.insert(nrid);
                }
            }
        }
    }
    neigh.into_iter().collect()
}

/// True when moving `(cell_r, cell_c)` into `nrid` would violate a pre-boundary
/// (some neighbour of the cell already sits in `nrid` behind a pre-boundary).
#[inline]
fn repair_move_conflicts(
    region_of: &[Option<usize>],
    pre: &PreBoundaries,
    cell_r: usize,
    cell_c: usize,
    nrid: usize,
    h: usize,
    w: usize,
) -> bool {
    for (dr, dc) in DIRS {
        let nr = cell_r as i32 + dr;
        let nc = cell_c as i32 + dc;
        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
            let nidx = nr as usize * w + nc as usize;
            if region_of[nidx] == Some(nrid) && pre.contains(cell_r, cell_c, nr as usize, nc as usize) {
                return true;
            }
        }
    }
    false
}

/// One repair pass for the multi-symbol solver: try to move each violating cell
/// into a compatible adjacent region.  Returns `true` if any cell was moved.
/// Is region `rid` 4-connected, judging membership from `region_of`?  Same
/// guard as `is_connected_set`, for the callers that track membership in a
/// `region_of` map rather than a `CellSet` (doc 28).
fn is_region_connected(region_of: &[Option<usize>], rid: usize, w: usize) -> bool {
    let mut start: Option<usize> = None;
    let mut count = 0usize;
    for (i, &r) in region_of.iter().enumerate() {
        if r == Some(rid) {
            count += 1;
            if start.is_none() {
                start = Some(i);
            }
        }
    }
    if count <= 1 {
        return true;
    }
    let start = start.unwrap();
    let n = region_of.len();
    let mut seen = vec![false; n];
    let mut q = std::collections::VecDeque::new();
    q.push_back(start);
    seen[start] = true;
    let mut reached = 0usize;
    while let Some(cur) = q.pop_front() {
        reached += 1;
        let (r, c) = (cur / w, cur % w);
        for (dr, dc) in DIRS {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 || (nr as usize) * w + (nc as usize) >= n {
                continue;
            }
            let ni = nr as usize * w + nc as usize;
            if !seen[ni] && region_of[ni] == Some(rid) {
                seen[ni] = true;
                q.push_back(ni);
            }
        }
    }
    reached == count
}

fn repair_multisymbol(
    region_of: &mut [Option<usize>],
    region_symbols: &mut [u64],
    region_sizes: &mut [usize],
    pre: &PreBoundaries,
    symbol_of: &std::collections::HashMap<usize, usize>,
    h: usize,
    w: usize,
) -> bool {
    let mut repaired = false;
    for [r1, c1, r2, c2] in pre.iter().collect::<Vec<_>>() {
        let rid1 = region_of[r1 * w + c1];
        let rid2 = region_of[r2 * w + c2];
        let Some(rid) = rid1 else { continue };
        if rid2 != Some(rid) {
            continue;
        }
        for (cell_r, cell_c, cur_rid) in [(r1, c1, rid), (r2, c2, rid)] {
            let neigh = collect_repair_neighbors(region_of, pre, cell_r, cell_c, cur_rid, h, w);
            let sym = symbol_of.get(&(cell_r * w + cell_c)).copied();
            let mut sorted: Vec<usize> = neigh.into_iter().collect();
            sorted.sort_unstable();
            for nrid in sorted {
                if sym.is_some() && (region_symbols[nrid] & (1u64 << sym.unwrap())) != 0 {
                    continue;
                }
                if repair_move_conflicts(region_of, pre, cell_r, cell_c, nrid, h, w) {
                    continue;
                }
                if let Some(si) = sym {
                    region_symbols[cur_rid] &= !(1u64 << si);
                    region_symbols[nrid] |= 1u64 << si;
                }
                let idx = cell_r * w + cell_c;
                region_of[idx] = Some(nrid);
                region_sizes[cur_rid] -= 1;
                region_sizes[nrid] += 1;
                // Guard: the move must not slice the region the cell left
                // (doc 28).  Undo everything on failure.
                if !is_region_connected(region_of, cur_rid, w) {
                    region_of[idx] = Some(cur_rid);
                    region_sizes[cur_rid] += 1;
                    region_sizes[nrid] -= 1;
                    if let Some(si) = sym {
                        region_symbols[cur_rid] |= 1u64 << si;
                        region_symbols[nrid] &= !(1u64 << si);
                    }
                    continue;
                }
                repaired = true;
                break;
            }
            if repaired {
                break;
            }
        }
        if repaired {
            break;
        }
    }
    repaired
}
