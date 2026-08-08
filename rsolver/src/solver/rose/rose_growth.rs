//! Rose-window BFS growth + constraint repair — port of
//! `src/solver/rose_growth.py`.  Fallback used when region_match times out or
//! misses.

use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use crate::types::Puzzle;

use super::cells::{CellSet, PreBoundaries};
use super::build_regions;

const SWAP_REPAIR_ITER: usize = 500;
const MULTI_REPAIR_ITER: usize = 200;

const DIRS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

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
    let result = if symbol_types.len() >= 2 {
        solve_multisymbol(puzzle, pre, symbol_types, m, &seeds, &symbol_of, h, w, all_positions, n_bits, deadline)
    } else {
        solve_singlesymbol(puzzle, pre, m, &seeds, &symbol_of, h, w, all_positions, n_bits, deadline)
    };
    result
}

fn solve_singlesymbol(
    puzzle: &Puzzle,
    pre: &PreBoundaries,
    m: usize,
    seeds: &[usize],
    _symbol_of: &std::collections::HashMap<usize, usize>,
    h: usize,
    w: usize,
    all_positions: &CellSet,
    n_bits: usize,
    deadline: Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
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

    // Wavefront growth.
    let mut steps: u64 = 0;
    while !unassigned.is_empty() {
        steps += 1;
        if steps % 4096 == 0 && Instant::now() >= deadline {
            return None;
        }
        let mut best_cell: Option<usize> = None;
        let mut best_adj: Vec<usize> = Vec::new();
        for idx in unassigned.iter() {
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
            if would_violate(&region_of, cr, cc, rid, pre, w) {
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

    // Swap repair.
    for _ in 0..SWAP_REPAIR_ITER {
        let violations: Vec<[usize; 4]> = pre
            .iter()
            .filter(|[r1, c1, r2, c2]| {
                let a = region_of[r1 * w + c1];
                let b = region_of[r2 * w + c2];
                matches!((a, b), (Some(x), Some(y)) if x == y)
            })
            .collect();
        if violations.is_empty() {
            break;
        }
        let mut fixed = false;
        for [r1, c1, r2, c2] in &violations {
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
                            if nrid != cur && !pre.contains(cell_r, cell_c, nr as usize, nc as usize)
                            {
                                alts.push(nrid);
                            }
                        }
                    }
                }
                alts.sort_by_key(|&rid| region_cells[rid].len());
                for &nrid in &alts {
                    let mut conflict = false;
                    for (dr, dc) in DIRS {
                        let nr = cell_r as i32 + dr;
                        let nc = cell_c as i32 + dc;
                        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                            let nidx = nr as usize * w + nc as usize;
                            if region_of[nidx] == Some(nrid)
                                && pre.contains(cell_r, cell_c, nr as usize, nc as usize)
                            {
                                conflict = true;
                                break;
                            }
                        }
                    }
                    if conflict {
                        continue;
                    }
                    region_of[cell_r * w + cell_c] = Some(nrid);
                    region_cells[cur].remove(cell_r * w + cell_c);
                    region_cells[nrid].insert(cell_r * w + cell_c);
                    fixed = true;
                    break;
                }
                if fixed {
                    break;
                }
            }
            if fixed {
                break;
            }
        }
        if fixed {
            continue;
        }
        // Chain move.
        for [r1, c1, r2, c2] in &violations {
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
                    let mut can_move_neighbor = true;
                    for (ddr, ddc) in DIRS {
                        let nnr = nru as i32 + ddr;
                        let nnc = ncu as i32 + ddc;
                        if nnr >= 0 && nnc >= 0 && (nnr as usize) < h && (nnc as usize) < w {
                            let nnidx = nnr as usize * w + nnc as usize;
                            if nnidx != cell_r * w + cell_c && region_of[nnidx] == Some(cur)
                                && pre.contains(nru, ncu, nnr as usize, nnc as usize)
                            {
                                can_move_neighbor = false;
                                break;
                            }
                        }
                    }
                    if !can_move_neighbor {
                        continue;
                    }
                    let mut can_move_self = true;
                    for (ddr, ddc) in DIRS {
                        let nnr = cell_r as i32 + ddr;
                        let nnc = cell_c as i32 + ddc;
                        if nnr >= 0 && nnc >= 0 && (nnr as usize) < h && (nnc as usize) < w {
                            let nnidx = nnr as usize * w + nnc as usize;
                            if nnidx != nidx && region_of[nnidx] == Some(n_rid)
                                && pre.contains(cell_r, cell_c, nnr as usize, nnc as usize)
                            {
                                can_move_self = false;
                                break;
                            }
                        }
                    }
                    if can_move_self {
                        region_of[nidx] = Some(cur);
                        region_cells[n_rid].remove(nidx);
                        region_cells[cur].insert(nidx);
                        region_of[cell_r * w + cell_c] = Some(n_rid);
                        region_cells[cur].remove(cell_r * w + cell_c);
                        region_cells[n_rid].insert(cell_r * w + cell_c);
                        fixed = true;
                        break;
                    }
                }
                if fixed {
                    break;
                }
            }
            if fixed {
                break;
            }
        }
        if !fixed {
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
    puzzle: &Puzzle,
    pre: &PreBoundaries,
    symbol_types: &[String],
    m: usize,
    seeds: &[usize],
    symbol_of: &std::collections::HashMap<usize, usize>,
    h: usize,
    w: usize,
    all_positions: &CellSet,
    n_bits: usize,
    deadline: Instant,
) -> Option<Vec<crate::types::RegionInfo>> {
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
    let mut region_symbols: Vec<u32> = vec![0u32; m];
    let mut region_sizes: Vec<usize> = vec![1usize; m];
    let mut region_of = vec![None; n_bits];
    for (i, &seed) in seeds.iter().enumerate() {
        region_of[seed] = Some(i);
        region_symbols[i] = 1u32 << symbol_of.get(&seed).copied().unwrap_or(0);
    }

    let mut queue: VecDeque<(usize, usize)> = seeds.iter().enumerate().map(|(i, &s)| (s, i)).collect();
    while let Some((idx, rid)) = queue.pop_front() {
        let (r, c) = (idx / w, idx % w);
        for (dr, dc) in DIRS {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 || (nr as usize) >= h || (nc as usize) >= w {
                continue;
            }
            let (nru, ncu) = (nr as usize, nc as usize);
            let nidx = nru * w + ncu;
            if puzzle.cells[nru][ncu].blocked || region_of[nidx].is_some() {
                continue;
            }
            if pre.contains(r, c, nru, ncu) {
                continue;
            }
            let sym = symbol_of.get(&nidx).copied();
            if let Some(si) = sym {
                if (region_symbols[rid] & (1u32 << si)) != 0 {
                    continue;
                }
            }
            if boundary_endpoints.contains(nidx) {
                let mut in_same = false;
                for (ddr, ddc) in DIRS {
                    let nnr = nru as i32 + ddr;
                    let nnc = ncu as i32 + ddc;
                    if nnr >= 0 && nnc >= 0 && (nnr as usize) < h && (nnc as usize) < w {
                        let nnidx = nnr as usize * w + nnc as usize;
                        if region_of[nnidx] == Some(rid)
                            && pre.contains(nru, ncu, nnr as usize, nnc as usize)
                        {
                            in_same = true;
                            break;
                        }
                    }
                }
                if in_same {
                    continue;
                }
            }
            region_of[nidx] = Some(rid);
            if let Some(si) = sym {
                region_symbols[rid] |= 1u32 << si;
            }
            region_sizes[rid] += 1;
            queue.push_back((nidx, rid));
        }
    }

    let mut unassigned = all_positions.clone();
    for idx in region_of.iter().enumerate().filter_map(|(i, r)| r.map(|_| i)) {
        unassigned.remove(idx);
    }

    // Second pass: assign leftovers to smallest compatible region.
    if !unassigned.is_empty() {
        let mut changed = true;
        let mut pass: u64 = 0;
        while changed {
            changed = false;
            pass += 1;
            if pass % 64 == 0 && Instant::now() >= deadline {
                return None;
            }
            for idx in unassigned.iter().collect::<Vec<_>>() {
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
                    continue;
                }
                let sym = symbol_of.get(&idx).copied();
                let mut valid: Vec<usize> = candidates
                    .into_iter()
                    .filter(|&i| {
                        !(sym.is_some() && (region_symbols[i] & (1u32 << sym.unwrap())) != 0)
                    })
                    .collect();
                if valid.is_empty() {
                    continue;
                }
                valid.sort_by_key(|&i| region_sizes[i]);
                let best = valid[0];
                region_of[idx] = Some(best);
                if let Some(si) = sym {
                    region_symbols[best] |= 1u32 << si;
                }
                region_sizes[best] += 1;
                unassigned.remove(idx);
                changed = true;
            }
        }
    }

    // Repair (multi-symbol).
    for _ in 0..MULTI_REPAIR_ITER {
        let mut repaired = false;
        for [r1, c1, r2, c2] in pre.iter().collect::<Vec<_>>() {
            let rid1 = region_of[r1 * w + c1];
            let rid2 = region_of[r2 * w + c2];
            let Some(rid) = rid1 else { continue };
            if rid2 != Some(rid) {
                continue;
            }
            for (cell_r, cell_c, cur_rid) in [(r1, c1, rid), (r2, c2, rid)] {
                let mut neigh: HashSet<usize> = HashSet::new();
                for (dr, dc) in DIRS {
                    let nr = cell_r as i32 + dr;
                    let nc = cell_c as i32 + dc;
                    if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                        let nidx = nr as usize * w + nc as usize;
                        if let Some(nrid) = region_of[nidx] {
                            if nrid != cur_rid
                                && !pre.contains(cell_r, cell_c, nr as usize, nc as usize)
                            {
                                neigh.insert(nrid);
                            }
                        }
                    }
                }
                let sym = symbol_of.get(&(cell_r * w + cell_c)).copied();
                let mut sorted: Vec<usize> = neigh.into_iter().collect();
                sorted.sort_unstable();
                for nrid in sorted {
                    if sym.is_some() && (region_symbols[nrid] & (1u32 << sym.unwrap())) != 0 {
                        continue;
                    }
                    let mut conflict = false;
                    for (dr, dc) in DIRS {
                        let nr = cell_r as i32 + dr;
                        let nc = cell_c as i32 + dc;
                        if nr >= 0 && nc >= 0 && (nr as usize) < h && (nc as usize) < w {
                            let nidx = nr as usize * w + nc as usize;
                            if region_of[nidx] == Some(nrid)
                                && pre.contains(cell_r, cell_c, nr as usize, nc as usize)
                            {
                                conflict = true;
                                break;
                            }
                        }
                    }
                    if conflict {
                        continue;
                    }
                    if let Some(si) = sym {
                        region_symbols[cur_rid] &= !(1u32 << si);
                        region_symbols[nrid] |= 1u32 << si;
                    }
                    region_of[cell_r * w + cell_c] = Some(nrid);
                    region_sizes[cur_rid] -= 1;
                    region_sizes[nrid] += 1;
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
        if !repaired {
            break;
        }
    }

    let all_mask: u32 = (1u32 << symbol_types.len()) - 1;
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
