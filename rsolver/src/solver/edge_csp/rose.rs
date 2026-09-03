//! Rose-window support for the edge-variable CSP solver.
//!
//! Ported from `third_party/aog/src/solver/propagation/rose.rs` (and the
//! rose-pair branching from `pair.rs`).  The edge_csp engine already maintains
//! the three-state `EdgeState` array and the component model (`curr_comp_id`,
//! `comp_cells`, `prop.growth_edges`, `prop.growing_list`, `can_grow_buf`) that
//! the reference rose propagators rely on, so this module adapts those
//! propagators to our structures.
//!
//! All propagation here is *additive*: it only forces edges that are logically
//! implied by the rose_window constraint (a rose cell of each symbol type must
//! appear in every piece).  The router re-validates every solution via
//! `validate::validate`, so a wrong forced edge can never surface as a false
//! solve; at worst a buggy propagator makes edge_csp bail and the router
//! falls through to the other solvers.

use super::parity_uf::ParityUF;
use super::types::*;
use super::Solver;
use std::collections::HashSet;

/// Branching constraint state managed by the rose-pair subsystem.
#[derive(Default)]
pub(crate) struct PairBranchState {
    /// Manual DIFF constraints accumulated during branching.
    pub diffs: Vec<(CellId, CellId)>,
    /// Fast-lookup set for manual DIFF pairs.
    pub diff_set: HashSet<(CellId, CellId)>,
    /// Manual SAME constraints accumulated during branching.
    pub sames: Vec<(CellId, CellId)>,
    /// Fast-lookup set for manual SAME pairs.
    pub same_set: HashSet<(CellId, CellId)>,
    /// Reusable BFS parent buffer for path-finding.
    pub bfs_prev: Vec<Option<(CellId, EdgeId)>>,
}

impl<'a> Solver<'a> {
    /// BFS from component `ci` through Uncut+Unknown edges, collect reachable
    /// rose types.  If `exclude_rose_mask` is nonzero, cells containing those
    /// rose symbols are treated as blocked.  If `exclude_e` is Some, that edge
    /// is treated as blocked.  Returns the bitmask of reachable rose types.
    pub(crate) fn bfs_reachable_rose_types(
        &mut self,
        ci: usize,
        exclude_rose_mask: u8,
        exclude_e: Option<EdgeId>,
    ) -> u8 {
        let n = self.grid.num_cells();
        self.rose_visited[..n].fill(false);
        let mut types: u8 = 0;
        let sym = &self.cell_rose_sym;

        self.q_buf.clear();
        for &c in &self.comp_cells[ci] {
            self.rose_visited[c] = true;
            self.q_buf.push(c);
            if sym[c] != u8::MAX {
                types |= 1 << sym[c];
            }
        }

        while let Some(cur) = self.q_buf.pop() {
            for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                if let Some(ex) = exclude_e {
                    if eid == ex {
                        continue;
                    }
                }
                if self.edges[eid] == EdgeState::Cut {
                    continue;
                }
                let (a, b) = self.grid.edge_cells(eid);
                let other = if a == cur { b } else { a };
                if !self.grid.cell_exists[other] || self.rose_visited[other] {
                    continue;
                }
                if exclude_rose_mask != 0
                    && sym[other] != u8::MAX
                    && (exclude_rose_mask & (1 << sym[other])) != 0
                {
                    continue;
                }
                self.rose_visited[other] = true;
                self.q_buf.push(other);
                if sym[other] != u8::MAX {
                    types |= 1 << sym[other];
                }
            }
        }

        types
    }

    /// Rose window advanced propagation (Phase 1 + 2 of the reference).
    pub(crate) fn propagate_rose_separation(&mut self) -> Result<bool, ()> {
        if self.rose_bits_all == 0 || self.curr_comp_id.is_empty() {
            return Ok(false);
        }

        // Quick check: if no growing components have rose symbols, skip.
        let num_comp = self.curr_comp_sz.len();
        let mut any_rose_comp = false;
        for &ci in &self.prop.growing_list {
            for &c in &self.comp_cells[ci] {
                if self.cell_rose_sym[c] != u8::MAX {
                    any_rose_comp = true;
                    break;
                }
            }
            if any_rose_comp {
                break;
            }
        }
        if !any_rose_comp {
            return Ok(false);
        }

        // Precompute comp_rose bitmask for each growing component.
        let mut comp_rose_arr: Vec<u8> = vec![0; num_comp];
        for &ci in &self.prop.growing_list {
            for &c in &self.comp_cells[ci] {
                let sym = self.cell_rose_sym[c];
                if sym != u8::MAX {
                    comp_rose_arr[ci] |= 1 << sym;
                }
            }
        }

        // --- Phase 1: Cross-type chokepoint Uncut forcing ---
        {
            let growing_list = std::mem::take(&mut self.prop.growing_list);
            for &ci in &growing_list {
                let comp_rose = comp_rose_arr[ci];
                let missing = self.rose_bits_all & !comp_rose;
                if missing == 0 {
                    continue;
                }

                let mut unknown_edges: Vec<EdgeId> = Vec::new();
                for &e in &self.prop.growth_edges[ci] {
                    if self.edges[e] == EdgeState::Unknown {
                        unknown_edges.push(e);
                    }
                }
                if unknown_edges.len() > 2 {
                    continue;
                }

                for e in unknown_edges {
                    let reachable_without = self.bfs_reachable_rose_types(ci, comp_rose, Some(e));
                    if (reachable_without & missing) != missing {
                        if !self.set_edge(e, EdgeState::Uncut) {
                            self.prop.growing_list = growing_list;
                            return Err(());
                        }
                        self.prop.growing_list = growing_list;
                        return Ok(true);
                    }
                }
            }
            self.prop.growing_list = growing_list;
        }

        // --- Phase 2: Two-level restricted reachability + single-growth-edge forcing ---
        {
            let growing_list = std::mem::take(&mut self.prop.growing_list);
            for &ci in &growing_list {
                let comp_rose = comp_rose_arr[ci];
                let missing = self.rose_bits_all & !comp_rose;
                if missing == 0 {
                    continue;
                }

                if missing.count_ones() == 1 {
                    let reachable = self.bfs_reachable_rose_types(ci, comp_rose, None);
                    if (reachable & missing) != missing {
                        self.prop.growing_list = growing_list;
                        return Err(());
                    }
                }

                let mut unknown_growth: Option<EdgeId> = None;
                let mut unknown_count = 0usize;
                for &e in &self.prop.growth_edges[ci] {
                    if self.edges[e] == EdgeState::Unknown {
                        unknown_count += 1;
                        unknown_growth = Some(e);
                    }
                }

                if unknown_count == 1 {
                    let e = unknown_growth.unwrap();
                    if !self.set_edge(e, EdgeState::Uncut) {
                        self.prop.growing_list = growing_list;
                        return Err(());
                    }
                    self.prop.growing_list = growing_list;
                    return Ok(true);
                }
            }
            self.prop.growing_list = growing_list;
        }

        Ok(false)
    }

    /// Phase 3: all-types-complete component rose blocking.
    pub(crate) fn propagate_rose_phase3(&mut self) -> Result<bool, ()> {
        if self.rose_bits_all == 0 || self.curr_comp_id.is_empty() {
            return Ok(false);
        }
        let growing_list = std::mem::take(&mut self.prop.growing_list);
        for &ci in &growing_list {
            let mut comp_rose: u8 = 0;
            for &c in &self.comp_cells[ci] {
                let sym = self.cell_rose_sym[c];
                if sym != u8::MAX {
                    comp_rose |= 1 << sym;
                }
            }

            if comp_rose != self.rose_bits_all {
                continue;
            }

            let growth: Vec<EdgeId> = self.prop.growth_edges[ci]
                .iter()
                .copied()
                .filter(|&e| self.edges[e] == EdgeState::Unknown)
                .collect();

            for e in growth {
                let (c1, c2) = self.grid.edge_cells(e);
                let other = if self.curr_comp_id[c1] == ci { c2 } else { c1 };
                if !self.grid.cell_exists[other] {
                    continue;
                }
                if self.cell_rose_sym[other] != u8::MAX {
                    if !self.set_edge(e, EdgeState::Cut) {
                        self.prop.growing_list = growing_list;
                        return Err(());
                    }
                    self.prop.growing_list = growing_list;
                    return Ok(true);
                }
            }
        }
        self.prop.growing_list = growing_list;

        Ok(false)
    }

    /// Parity propagation using a Union-Find with parity.
    ///
    /// Seeds the UF with:
    ///   - Rose cells of the same type (exactly 2 of them) → parity=1 (must be
    ///     in different pieces).  The reference only unions the 2-of-a-type
    ///     case; 3+ of a type cannot be represented as a single bipartition.
    ///   - Uncut edges → parity=0 (same piece).
    ///   - sames → parity=0, diffs → parity=1.
    ///
    /// Detects contradictions and forces Cut on unknown edges where both
    /// endpoints are already determined to be in different pieces.
    pub(crate) fn propagate_parity(&mut self) -> Result<bool, ()> {
        if self.rose_bits_all == 0
            && self.pair_branch.diffs.is_empty()
            && self.pair_branch.sames.is_empty()
        {
            return Ok(false);
        }

        let n = self.grid.num_cells();
        let ne = self.grid.num_edges();
        let two_piece = self.exact_piece_count == Some(2);

        // The forcing loop below can only fire if some parity-1 relation gets
        // seeded: Uncut edges and SAME pairs only seed parity 0, so without a
        // 2-of-a-kind rose pair (or a known 2-piece count, which also seeds
        // Cut edges as parity 1) every root ends up with uniform parity and no
        // edge can be forced Cut - and no union can contradict either.  That
        // makes the whole O(V + E) pass provably a no-op, so skip it.  This
        // matters on puzzles whose rose types all occur >2 times, where the
        // pass was pure per-propagation overhead.
        let has_parity1_source = two_piece || self.rose_by_type.iter().any(|t| t.len() == 2);
        if !has_parity1_source {
            return Ok(false);
        }

        let mut uf = ParityUF::new(n);

        // Seed: rose cells of same type (exactly 2) → different pieces (parity=1).
        for type_cells in &self.rose_by_type {
            if type_cells.len() == 2 {
                if uf.union(type_cells[0], type_cells[1], 1).is_err() {
                    return Err(());
                }
            }
        }

        // sames → parity=0.
        for &(c1, c2) in &self.pair_branch.sames {
            if uf.union(c1, c2, 0).is_err() {
                return Err(());
            }
        }
        // diffs → parity=1 (only safe for 2-piece puzzles).
        if two_piece {
            for &(c1, c2) in &self.pair_branch.diffs {
                if uf.union(c1, c2, 1).is_err() {
                    return Err(());
                }
            }
        }

        // Seed: Uncut edges → parity=0; Cut edges → parity=1 only for 2-piece.
        for e in 0..ne {
            let rel = match self.edges[e] {
                EdgeState::Uncut => 0u8,
                EdgeState::Cut => {
                    if two_piece {
                        1u8
                    } else {
                        continue;
                    }
                }
                EdgeState::Unknown => continue,
            };
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            if uf.union(c1, c2, rel).is_err() {
                return Err(());
            }
        }

        // Force: unknown edges where both endpoints have parity=1 → Cut.
        for e in 0..ne {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let (r1, p1) = uf.find(c1);
            let (r2, p2) = uf.find(c2);
            if r1 == r2 && (p1 ^ p2) == 1 {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Inline DIFF check: are `c1` and `c2` forced to be in different pieces?
    /// Same-type rose cells are always DIFF; manual DIFF pairs and a direct Cut
    /// edge between them also force DIFF.
    pub(crate) fn is_diff_inline(&self, c1: CellId, c2: CellId) -> bool {
        let s1 = self.cell_rose_sym[c1];
        let s2 = self.cell_rose_sym[c2];
        if s1 != u8::MAX && s1 == s2 {
            return true;
        }
        if self.pair_branch.diff_set.contains(&(c1, c2))
            || self.pair_branch.diff_set.contains(&(c2, c1))
        {
            return true;
        }
        for eid in self.grid.cell_edges(c1).into_iter().flatten() {
            let (a, b) = self.grid.edge_cells(eid);
            let other = if a == c1 { b } else { a };
            if other == c2 && self.edges[eid] == EdgeState::Cut {
                return true;
            }
        }
        false
    }

    /// BFS from `c1` to `c2` through Uncut+Unknown edges; records path parents in
    /// `pair_branch.bfs_prev`. Returns true if a path exists.
    pub(crate) fn bfs_path(&mut self, c1: CellId, c2: CellId) -> bool {
        let n = self.grid.num_cells();
        self.rose_visited[..n].fill(false);
        self.pair_branch.bfs_prev.resize(n, None);
        self.rose_visited[c1] = true;
        self.q_buf.clear();
        self.q_buf.push(c1);
        while let Some(cur) = self.q_buf.pop() {
            if cur == c2 {
                return true;
            }
            for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                if self.edges[eid] == EdgeState::Cut {
                    continue;
                }
                let (a, b) = self.grid.edge_cells(eid);
                let other = if a == cur { b } else { a };
                if !self.grid.cell_exists[other] || self.rose_visited[other] {
                    continue;
                }
                self.rose_visited[other] = true;
                self.pair_branch.bfs_prev[other] = Some((cur, eid));
                self.q_buf.push(other);
            }
        }
        false
    }

    /// Force `c1` and `c2` into the same piece: BFS through Uncut+Unknown, set
    /// every Unknown edge on the path to Uncut. Returns `Err` if no path exists
    /// or forcing fails.
    pub(crate) fn branch_pair_same(&mut self, c1: CellId, c2: CellId) -> Result<(), ()> {
        if self.curr_comp_id[c1] == self.curr_comp_id[c2] {
            return Ok(());
        }
        if !self.bfs_path(c1, c2) {
            return Err(());
        }
        let mut cur = c2;
        while cur != c1 {
            if let Some((prev, eid)) = self.pair_branch.bfs_prev[cur] {
                if self.edges[eid] == EdgeState::Unknown {
                    if !self.set_edge(eid, EdgeState::Uncut) {
                        return Err(());
                    }
                }
                cur = prev;
            } else {
                return Err(());
            }
        }
        Ok(())
    }

    /// Select the best rose cell pair to branch on (SAME/DIFF), if any pair
    /// scores higher than `edge_score`.  Only BFS from type-0 rose cells; each
    /// (type-0, other-type) pair is considered once.
    pub(crate) fn select_rose_pair(&mut self, edge_score: i32) -> Option<(CellId, CellId)> {
        if self.rose_by_type.is_empty() || self.rose_by_type.len() < 2 {
            return None;
        }
        if self.curr_comp_id.is_empty() {
            return None;
        }

        let rose_by_type: &[Vec<CellId>] = &self.rose_by_type;
        let sym = &self.cell_rose_sym;
        let n = self.grid.num_cells();
        let num_types = rose_by_type.len();
        let num_comp = self.curr_comp_sz.len();
        let mut comp_rose_count: Vec<u8> = vec![0; num_comp];
        for ci in 0..num_comp {
            if ci >= self.comp_cells.len() {
                break;
            }
            let mut mask: u8 = 0;
            for &c in &self.comp_cells[ci] {
                if sym[c] != u8::MAX {
                    mask |= 1 << sym[c];
                }
            }
            comp_rose_count[ci] = mask.count_ones() as u8;
        }

        let mut best_pair: Option<(CellId, CellId)> = None;
        let mut best_pair_score: i32 = edge_score;

        for &c1 in &rose_by_type[0] {
            let ci1 = self.curr_comp_id[c1];

            self.rose_visited[..n].fill(false);
            self.pair_branch.bfs_prev.resize(n, None);
            self.rose_visited[c1] = true;
            self.q_buf.clear();
            self.q_buf.push(c1);

            while let Some(cur) = self.q_buf.pop() {
                let cur_sym = sym[cur];

                if cur_sym != u8::MAX && cur_sym == 0 && cur != c1 {
                    // Skip type-0 cells (must be DIFF by rose rule) except start.
                } else if cur_sym != u8::MAX {
                    let ci2 = self.curr_comp_id[cur];
                    if ci1 != ci2 && !self.is_diff_inline(c1, cur) {
                        let mut dist = 0usize;
                        let mut tmp = cur;
                        while tmp != c1 {
                            if let Some((p, _)) = self.pair_branch.bfs_prev[tmp] {
                                dist += 1;
                                tmp = p;
                            } else {
                                break;
                            }
                        }

                        let mut score: i32 = 150;
                        if dist <= 2 {
                            score += 50;
                        }
                        if comp_rose_count[ci1] >= (num_types as u8) - 1 {
                            score += 40;
                        }
                        if comp_rose_count[ci2] >= (num_types as u8) - 1 {
                            score += 40;
                        }

                        if score > best_pair_score {
                            best_pair_score = score;
                            best_pair = Some((c1, cur));
                        }

                        if best_pair_score >= 280 {
                            return best_pair;
                        }
                    }
                }

                for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                    if self.edges[eid] == EdgeState::Cut {
                        continue;
                    }
                    let (a, b) = self.grid.edge_cells(eid);
                    let other = if a == cur { b } else { a };
                    if !self.grid.cell_exists[other] || self.rose_visited[other] {
                        continue;
                    }
                    self.rose_visited[other] = true;
                    self.pair_branch.bfs_prev[other] = Some((cur, eid));
                    self.q_buf.push(other);
                }
            }
        }

        best_pair
    }

    /// Branch on a rose cell pair: try SAME (force same piece), then DIFF (force
    /// different pieces).  Each branch recurses into `backtrack_edges`.
    pub(crate) fn branch_on_pair(&mut self, c1: CellId, c2: CellId) {
        let in_same_comp = self.curr_comp_id[c1] == self.curr_comp_id[c2];

        if !in_same_comp {
            let snap = self.snapshot();
            if self.branch_pair_same(c1, c2).is_ok() && self.propagate().is_ok() {
                self.backtrack_edges();
            }
            self.restore(snap);
        }

        if self.solution_regions.is_some() || self.timed_out {
            return;
        }

        // DIFF branch: record a manual DIFF and propagate.
        let snap = self.snapshot();
        self.pair_branch.diffs.push((c1, c2));
        self.pair_branch.diff_set.insert((c1, c2));
        if self.propagate().is_ok() {
            self.backtrack_edges();
        }
        self.restore(snap);
    }
}
