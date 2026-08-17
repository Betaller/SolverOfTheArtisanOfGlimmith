//! Propagation: fixed-point loop + propagators.
//!
//! Ported from `third_party/aog/src/solver/propagation/`, slimmed to the rules
//! this solver targets.  The core is `build_components` (flood-fill decided-Uncut
//! edges into components, compute per-component target/min/max area and growth
//! edges), plus vertex-degree (`bricky_loopy`), area-target sealing,
//! inequality/difference clue propagation, and failed-literal probing.

use super::types::*;
use super::parity_uf::ParityUF;
use super::Solver;
use std::collections::VecDeque;

/// Reusable buffers and caches owned by the propagation subsystem.
pub(crate) struct PropagationState {
    /// Pre-extracted diff clues: `(edge_id, value)`.
    pub diff_clues: Vec<(EdgeId, usize)>,
    /// Per-component minimum area (updated each propagation round).
    pub curr_min_area: Vec<usize>,
    /// Per-component maximum area.
    pub curr_max_area: Vec<usize>,
    /// Growth edges per component (populated by `build_components`).
    pub growth_edges: Vec<Vec<EdgeId>>,
    /// Pre-computed indices into `cell_clues` for compass clues.
    pub compass_clue_indices: Vec<usize>,
    /// Reusable BFS buffer (`usize::MAX` = unvisited).
    pub comp_buf: Vec<usize>,
    /// Precomputed growing / sealed component index lists.
    pub growing_list: Vec<usize>,
    pub sealed_list: Vec<usize>,
}

impl PropagationState {
    pub(crate) fn new(diff_clues: Vec<(EdgeId, usize)>, nc: usize) -> Self {
        Self {
            diff_clues,
            curr_min_area: Vec::new(),
            curr_max_area: Vec::new(),
            growth_edges: Vec::new(),
            compass_clue_indices: Vec::new(),
            comp_buf: vec![usize::MAX; nc],
            growing_list: Vec::new(),
            sealed_list: Vec::new(),
        }
    }
}

impl<'a> Solver<'a> {
    /// Fixed-point propagation.  Returns `Ok(true)` when stable (no further
    /// progress), `Err(())` on contradiction or timeout.
    pub(crate) fn propagate(&mut self) -> Result<bool, ()> {
        loop {
            if self.check_deadline() {
                return Err(());
            }
            let mut progress = false;

            if self.rules.bricky || self.rules.loopy {
                progress |= self.propagate_bricky_loopy()?;
            }
            if !self.vertex_clues.is_empty() {
                progress |= self.propagate_vertex_edge_parity()?;
            }
            if self.has_compass_clue {
                progress |= self.propagate_compass()?;
            }
            if self.has_palisade_clue {
                progress |= self.propagate_palisade_constraints()?;
            }
            progress |= self.propagate_area_bounds()?;
            if !self.vertex_clues.is_empty() {
                progress |= self.propagate_watchtower()?;
            }

            if !progress {
                // Failed-literal detection: probe unknown edges / edge pairs.
                if !self.in_probing && self.curr_unknown > 0 && self.curr_unknown <= 256 {
                    let saved = self.in_probing;
                    self.in_probing = true;
                    progress |= self.probe_one_round()?;
                    let pair_threshold: usize = if self.rules.loopy && !self.vertex_clues.is_empty()
                    {
                        20
                    } else {
                        10
                    };
                    if !progress && self.curr_unknown <= pair_threshold {
                        progress |= self.probe_pair_round()?;
                    }
                    self.in_probing = saved;
                }

                if !progress {
                    return Ok(true);
                }
            }
        }
    }

    /// Ring (`loopy`) / brick (`bricky`) vertex-degree propagation.
    ///
    /// The vertex degree counts *all* boundary edges, including the outer grid
    /// border and the boundary of blocked cells (mirroring
    /// `validate::count_boundary_edges_at_vertex`): an edge between a fillable
    /// cell and a non-fillable/outside cell is a definite boundary, an edge
    /// between two non-fillable cells is not.  The reference aog solver's
    /// `bricky_loopy` counts only internal edges (it never validates ring/brick
    /// at the leaf), so omitting the outer border here would let border
    /// T-junctions through and produce answers the router's validator rejects.
    pub(crate) fn propagate_bricky_loopy(&mut self) -> Result<bool, ()> {
        let mut progress = false;
        let loopy = self.rules.loopy;
        let bricky = self.rules.bricky;
        if !loopy && !bricky {
            return Ok(false);
        }
        // Pairs of vertex-cells forming the 4 incident edges: top(tl-tr),
        // bottom(bl-br), left(tl-bl), right(tr-br).
        const PAIRS: [(usize, usize); 4] = [(0, 1), (2, 3), (0, 2), (1, 3)];
        for i in 0..=self.grid.rows {
            for j in 0..=self.grid.cols {
                let cells = self.grid.vertex_cells(i, j);
                let mut cut_count = 0usize;
                let mut unk_edges = Vec::new();
                for &(ai, bi) in &PAIRS {
                    let fa = cells[ai].map_or(false, |c| self.grid.cell_exists[c]);
                    let fb = cells[bi].map_or(false, |c| self.grid.cell_exists[c]);
                    match (fa, fb) {
                        (true, true) => {
                            let eid = self
                                .grid
                                .edge_between(cells[ai].unwrap(), cells[bi].unwrap())
                                .unwrap();
                            match self.edges[eid] {
                                EdgeState::Cut => cut_count += 1,
                                EdgeState::Unknown => unk_edges.push(eid),
                                EdgeState::Uncut => {}
                            }
                        }
                        // Exactly one fillable endpoint → outer border (definite boundary).
                        (true, false) | (false, true) => cut_count += 1,
                        // Both non-fillable → shared empty space, not a boundary.
                        (false, false) => {}
                    }
                }

                if loopy && bricky {
                    // ring+brick ⇒ every boundary vertex touches ≤ 2 boundary edges.
                    if cut_count >= 3 {
                        return Err(());
                    }
                    let to_uncut = cut_count + unk_edges.len();
                    if to_uncut > 2 {
                        let n = to_uncut - 2;
                        for &eid in &unk_edges[..n] {
                            if !self.set_edge(eid, EdgeState::Uncut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                } else if loopy {
                    // Exactly 3 boundary edges is a T-junction → forbidden.
                    if cut_count == 3 && unk_edges.is_empty() {
                        return Err(());
                    }
                    // 3 cut + 1 unknown → force Cut (cross = 4, allowed by loopy).
                    if cut_count == 3 && unk_edges.len() == 1 {
                        if !self.set_edge(unk_edges[0], EdgeState::Cut) {
                            return Err(());
                        }
                        progress = true;
                    }
                    // 2 cut + 1 unknown → force Uncut (else T-junction).
                    if cut_count == 2 && unk_edges.len() == 1 {
                        if !self.set_edge(unk_edges[0], EdgeState::Uncut) {
                            return Err(());
                        }
                        progress = true;
                    }
                } else {
                    // bricky: at most 3 boundary edges (no cross junction).
                    if cut_count > 3 {
                        return Err(());
                    }
                    if cut_count + unk_edges.len() > 3 {
                        let n = cut_count + unk_edges.len() - 3;
                        for &eid in &unk_edges[..n] {
                            if !self.set_edge(eid, EdgeState::Uncut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                }
            }
        }
        Ok(progress)
    }

    /// Flood-fill one decided-Uncut connected component, tagging cells with the
    /// component's representative cell id.
    fn flood_fill_decided(&mut self, start: CellId) {
        self.prop.comp_buf[start] = start;
        self.q_buf.clear();
        self.q_buf.push(start);
        while let Some(cur) = self.q_buf.pop() {
            for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                let (c1, c2) = self.grid.edge_cells(eid);
                let other = if c1 == cur { c2 } else { c1 };
                if !self.grid.cell_exists[other] || self.prop.comp_buf[other] != usize::MAX {
                    continue;
                }
                if self.edges[eid] == EdgeState::Uncut {
                    self.prop.comp_buf[other] = start;
                    self.q_buf.push(other);
                }
            }
        }
    }

    /// Compass area bounds for a component: `(min, max, exact)`.
    /// Inferred 0 for directions where no cells exist in the grid.
    fn get_compass_area_bounds(
        &self,
        cell: CellId,
        compass: &CompassData,
    ) -> (usize, Option<usize>, Option<usize>) {
        let (r, c) = self.grid.cell_pos(cell);

        let has_dir = |dr: isize, dc: isize| -> bool {
            let mut d = 1isize;
            loop {
                let nr = r as isize + dr * d;
                let nc = c as isize + dc * d;
                if nr < 0
                    || nc < 0
                    || nr >= self.grid.rows as isize
                    || nc >= self.grid.cols as isize
                {
                    return false;
                }
                let nid = self.grid.cell_id(nr as usize, nc as usize);
                if self.grid.cell_exists[nid] {
                    return true;
                }
                d += 1;
            }
        };
        let has_north = has_dir(-1, 0);
        let has_south = has_dir(1, 0);
        let has_east = has_dir(0, 1);
        let has_west = has_dir(0, -1);

        let n = compass
            .n
            .or_else(|| if !has_north { Some(0) } else { None });
        let s = compass
            .s
            .or_else(|| if !has_south { Some(0) } else { None });
        let e = compass.e.or_else(|| if !has_east { Some(0) } else { None });
        let w = compass.w.or_else(|| if !has_west { Some(0) } else { None });

        let nv = n.unwrap_or(0);
        let sv = s.unwrap_or(0);
        let ev = e.unwrap_or(0);
        let wv = w.unwrap_or(0);

        let min_area = 1 + (nv + sv).max(ev + wv);

        let mut exact_area = None;
        if e == Some(0) && w == Some(0) {
            exact_area = Some(1 + nv + sv);
        } else if n == Some(0) && s == Some(0) {
            exact_area = Some(1 + ev + wv);
        }

        let mut max_area = None;
        if n.is_some() && s.is_some() && e.is_some() && w.is_some() {
            max_area = Some(1 + nv + sv + ev + wv);
        }

        (min_area, max_area, exact_area)
    }

    /// Upper bound on a component's final size: its target, or the flood-fill
    /// reach through non-Cut edges, capped at the component's local max area.
    fn growth_potential(&mut self, ci: usize) -> usize {
        if let Some(target) = self.curr_target_area[ci] {
            return target;
        }
        let n = self.grid.num_cells();
        self.visited_buf[..n].fill(false);
        self.q_buf.clear();
        let mut reachable = 0usize;
        for &c in &self.comp_cells[ci] {
            self.visited_buf[c] = true;
            self.q_buf.push(c);
            reachable += 1;
        }
        while let Some(cur) = self.q_buf.pop() {
            for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                if self.edges[eid] == EdgeState::Cut {
                    continue;
                }
                let (c1, c2) = self.grid.edge_cells(eid);
                let other = if c1 == cur { c2 } else { c1 };
                if !self.grid.cell_exists[other] || self.visited_buf[other] {
                    continue;
                }
                self.visited_buf[other] = true;
                self.q_buf.push(other);
                reachable += 1;
            }
        }
        reachable.min(self.prop.curr_max_area[ci])
    }

    /// Flood fill components, compute target/min/max areas and growth edges.
    fn build_components(&mut self) -> Result<usize, ()> {
        let n = self.grid.num_cells();
        self.prop.comp_buf.fill(usize::MAX);
        for c in 0..n {
            if !self.grid.cell_exists[c] || self.prop.comp_buf[c] != usize::MAX {
                continue;
            }
            self.flood_fill_decided(c);
        }

        // Map component representatives to contiguous ids.
        let mut num_comp = 0usize;
        let mut id_map = vec![usize::MAX; n];
        for c in 0..n {
            if !self.grid.cell_exists[c] {
                continue;
            }
            let rep = self.prop.comp_buf[c];
            if id_map[rep] == usize::MAX {
                id_map[rep] = num_comp;
                num_comp += 1;
            }
        }

        self.curr_comp_id.resize(n, usize::MAX);
        for c in 0..n {
            if self.grid.cell_exists[c] {
                self.curr_comp_id[c] = id_map[self.prop.comp_buf[c]];
            }
        }

        self.curr_comp_sz.clear();
        self.curr_comp_sz.resize(num_comp, 0);
        self.comp_cells.truncate(num_comp);
        for v in &mut self.comp_cells {
            v.clear();
        }
        while self.comp_cells.len() < num_comp {
            self.comp_cells.push(Vec::new());
        }
        for c in 0..n {
            if self.grid.cell_exists[c] {
                let ci = self.curr_comp_id[c];
                self.curr_comp_sz[ci] += 1;
                self.comp_cells[ci].push(c);
            }
        }

        self.curr_target_area.clear();
        self.curr_target_area.resize(num_comp, None);
        self.prop.curr_min_area.clear();
        self.prop
            .curr_min_area
            .resize(num_comp, self.eff_min_area.max(1));
        self.prop.curr_max_area.clear();
        self.prop.curr_max_area.resize(num_comp, self.eff_max_area);

        for ci in 0..num_comp {
            let mut target_area: Option<usize> = None;
            let mut local_min = self.prop.curr_min_area[ci];
            let mut local_max = self.prop.curr_max_area[ci];

            for &c in &self.comp_cells[ci] {
                for &clue_idx in &self.cell_clues_indexed[c] {
                    let clue = &self.cell_clues[clue_idx];
                    match clue {
                        CellClue::Area { value, .. } => {
                            if let Some(prev) = target_area {
                                if prev != *value {
                                    return Err(());
                                }
                            }
                            target_area = Some(*value);
                        }
                        CellClue::Compass { cell, compass } => {
                            let (cmin, cmax, cexact) = self.get_compass_area_bounds(*cell, compass);
                            if let Some(exact) = cexact {
                                if let Some(prev) = target_area {
                                    if prev != exact {
                                        return Err(());
                                    }
                                }
                                target_area = Some(exact);
                            }
                            local_min = local_min.max(cmin);
                            if let Some(maxv) = cmax {
                                local_max = local_max.min(maxv);
                            }
                        }
                        // Palisade (fence) carries no area constraint.
                        CellClue::Palisade { .. } => {}
                    }
                }
            }

            if let Some(a0) = target_area {
                if a0 < local_min || a0 > local_max {
                    return Err(());
                }
                self.curr_target_area[ci] = Some(a0);
                self.prop.curr_min_area[ci] = a0;
                self.prop.curr_max_area[ci] = a0;
                if self.curr_comp_sz[ci] > a0 {
                    return Err(());
                }
            } else {
                if local_min > local_max || self.curr_comp_sz[ci] > local_max {
                    return Err(());
                }
                self.prop.curr_min_area[ci] = local_min;
                self.prop.curr_max_area[ci] = local_max;
                if local_min == local_max {
                    self.curr_target_area[ci] = Some(local_min);
                } else if self.curr_comp_sz[ci] > self.eff_max_area {
                    return Err(());
                }
            }
        }

        // Growth-edge pass: classify Unknown edges between different components.
        self.can_grow_buf.clear();
        self.can_grow_buf.resize(num_comp, false);
        self.prop.growth_edges.truncate(num_comp);
        for v in &mut self.prop.growth_edges {
            v.clear();
        }
        while self.prop.growth_edges.len() < num_comp {
            self.prop.growth_edges.push(Vec::new());
        }

        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 != ci2 {
                let cannot_merge = match (self.curr_target_area[ci1], self.curr_target_area[ci2]) {
                    (Some(a1), Some(a2)) => a1 != a2,
                    _ => false,
                };
                if cannot_merge {
                    if !self.set_edge(e, EdgeState::Cut) {
                        return Err(());
                    }
                    continue;
                }

                self.can_grow_buf[ci1] = true;
                self.can_grow_buf[ci2] = true;
                self.prop.growth_edges[ci1].push(e);
                self.prop.growth_edges[ci2].push(e);

                let limit1 = self.prop.curr_max_area[ci1];
                let limit2 = self.prop.curr_max_area[ci2];
                if (self.curr_comp_sz[ci1] >= limit1 || self.curr_comp_sz[ci2] >= limit2)
                    && !self.set_edge(e, EdgeState::Cut)
                {
                    return Err(());
                }
            }
        }

        self.prop.growing_list.clear();
        self.prop.sealed_list.clear();
        for ci in 0..num_comp {
            if self.can_grow_buf[ci] {
                self.prop.growing_list.push(ci);
            } else {
                self.prop.sealed_list.push(ci);
            }
        }

        Ok(num_comp)
    }

    /// Hub: rebuild components, verify no Cut edge straddles a component, then
    /// run area / clue propagation.
    pub(crate) fn propagate_area_bounds(&mut self) -> Result<bool, ()> {
        let num_comp = self.build_components()?;

        // Cut-edge straddle check: a Cut edge with both cells in the same
        // component is a boundary drawn *inside* a piece → invalid.
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            if self.curr_comp_id[c1] == self.curr_comp_id[c2] {
                return Err(());
            }
        }

        self.propagate_area_constraints(num_comp)
    }

    /// Area-target sealing, inequality and difference clue propagation.
    fn propagate_area_constraints(&mut self, num_comp: usize) -> Result<bool, ()> {
        let mut progress = false;

        // Refresh per-component growth-edge counts (heuristic cache).
        self.growth_edge_count.clear();
        self.growth_edge_count.resize(num_comp, 0);
        for ci in 0..num_comp {
            let mut cnt = 0usize;
            for &e in &self.prop.growth_edges[ci] {
                if self.edges[e] == EdgeState::Unknown {
                    cnt += 1;
                }
            }
            self.growth_edge_count[ci] = cnt;
        }

        for ci in 0..num_comp {
            let target = self.curr_target_area[ci];
            let min_a = self.prop.curr_min_area[ci];
            let max_a = self.prop.curr_max_area[ci];

            if let Some(t) = target {
                if self.curr_comp_sz[ci] < t && self.is_sealed(ci) {
                    return Err(());
                }
                // Growth-potential check (skip during probing to limit overhead).
                if !self.in_probing && self.is_growing(ci) && self.curr_comp_sz[ci] < t {
                    let unk_growth = self.prop.growth_edges[ci]
                        .iter()
                        .filter(|&&e| self.edges[e] == EdgeState::Unknown)
                        .count();
                    if unk_growth <= 4 {
                        let potential = self.growth_potential(ci);
                        if potential < t {
                            return Err(());
                        }
                    }
                }
                if self.curr_comp_sz[ci] == t && self.is_growing(ci) {
                    for i in 0..self.prop.growth_edges[ci].len() {
                        let e = self.prop.growth_edges[ci][i];
                        if self.edges[e] == EdgeState::Unknown {
                            if !self.set_edge(e, EdgeState::Cut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                }
            } else {
                if self.curr_comp_sz[ci] < min_a && self.is_sealed(ci) {
                    return Err(());
                }
                if self.curr_comp_sz[ci] == max_a && self.is_growing(ci) {
                    for i in 0..self.prop.growth_edges[ci].len() {
                        let e = self.prop.growth_edges[ci][i];
                        if self.edges[e] == EdgeState::Unknown {
                            if !self.set_edge(e, EdgeState::Cut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                }
            }
        }

        progress |= self.propagate_inequality_clues(num_comp)?;
        progress |= self.propagate_diff_clues(num_comp)?;
        if self.has_compass_clue {
            progress |= self.propagate_compass_in_components(num_comp)?;
            progress |= self.propagate_compass_placement_enumeration()?;
        }

        Ok(progress)
    }

    /// Inequality edge clues: verify / prune area ordering.
    fn propagate_inequality_clues(&mut self, _num_comp: usize) -> Result<bool, ()> {
        let ineq_clues: Vec<(EdgeId, bool)> = self
            .edge_clues
            .iter()
            .filter_map(|cl| match cl.kind {
                EdgeClueKind::Inequality { smaller_first } => Some((cl.edge, smaller_first)),
                _ => None,
            })
            .collect();

        for (e, smaller_first) in ineq_clues {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 {
                continue;
            }
            let (smaller_ci, larger_ci) = if smaller_first {
                (ci1, ci2)
            } else {
                (ci2, ci1)
            };
            let smaller_done = self.is_sealed(smaller_ci);
            let larger_done = self.is_sealed(larger_ci);

            if smaller_done && larger_done {
                if self.curr_comp_sz[smaller_ci] >= self.curr_comp_sz[larger_ci] {
                    return Err(());
                }
            } else if larger_done && self.curr_comp_sz[larger_ci] <= self.curr_comp_sz[smaller_ci] {
                return Err(());
            } else if smaller_done {
                if self.curr_comp_sz[smaller_ci] >= self.growth_potential(larger_ci) {
                    return Err(());
                }
            } else if larger_done {
                if let Some(t) = self.curr_target_area[smaller_ci] {
                    if t >= self.curr_comp_sz[larger_ci] {
                        return Err(());
                    }
                }
            } else {
                let max_larger = self.growth_potential(larger_ci);
                if self.curr_comp_sz[smaller_ci] >= max_larger {
                    return Err(());
                }
                if let Some(t) = self.curr_target_area[smaller_ci] {
                    if t >= max_larger {
                        return Err(());
                    }
                }
                if let Some(t) = self.curr_target_area[larger_ci] {
                    if t <= self.curr_comp_sz[smaller_ci] {
                        return Err(());
                    }
                }
            }
        }
        Ok(false)
    }

    /// Difference edge clues: propagate target area when one side is sealed.
    fn propagate_diff_clues(&mut self, _num_comp: usize) -> Result<bool, ()> {
        let mut progress = false;
        let mut forced_cut: Vec<EdgeId> = Vec::new();

        for &(e, value) in &self.prop.diff_clues {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 {
                continue;
            }
            let sealed1 = self.is_sealed(ci1);
            let sealed2 = self.is_sealed(ci2);
            if sealed1 && sealed2 {
                if self.curr_comp_sz[ci1].abs_diff(self.curr_comp_sz[ci2]) != value {
                    return Err(());
                }
                continue;
            }
            let (sealed_ci, other_ci) = if sealed1 {
                (ci1, ci2)
            } else if sealed2 {
                (ci2, ci1)
            } else {
                continue;
            };
            let sealed_sz = self.curr_comp_sz[sealed_ci];
            let min_area = self.prop.curr_min_area[other_ci];
            let max_area = self.prop.curr_max_area[other_ci];
            let mut candidates: Vec<usize> = Vec::new();
            candidates.push(sealed_sz + value);
            if sealed_sz > value {
                candidates.push(sealed_sz - value);
            }
            candidates.retain(|&a| a >= min_area && a <= max_area);
            if candidates.is_empty() {
                return Err(());
            }
            if let Some(existing) = self.curr_target_area[other_ci] {
                if !candidates.contains(&existing) {
                    return Err(());
                }
                continue;
            }
            if candidates.len() == 1 {
                let new_target = candidates[0];
                if self.curr_comp_sz[other_ci] > new_target {
                    return Err(());
                }
                self.curr_target_area[other_ci] = Some(new_target);
                if self.curr_comp_sz[other_ci] == new_target {
                    for &ge in &self.prop.growth_edges[other_ci] {
                        if self.edges[ge] == EdgeState::Unknown {
                            forced_cut.push(ge);
                        }
                    }
                }
            }
        }

        for e in forced_cut {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }

    /// Check if two compass cells cannot coexist in the same piece (zero-value
    /// direction conflicts + value-ordering contradictions).
    fn compass_cells_incompatible(
        &self,
        ca: CellId,
        pa: &CompassData,
        cb: CellId,
        pb: &CompassData,
    ) -> bool {
        let (ra, cola) = self.grid.cell_pos(ca);
        let (rb, colb) = self.grid.cell_pos(cb);

        // Zero-value direction conflicts.
        if pa.n == Some(0) && rb < ra {
            return true;
        }
        if pb.n == Some(0) && ra < rb {
            return true;
        }
        if pa.s == Some(0) && rb > ra {
            return true;
        }
        if pb.s == Some(0) && ra > rb {
            return true;
        }
        if pa.e == Some(0) && colb > cola {
            return true;
        }
        if pb.e == Some(0) && cola > colb {
            return true;
        }
        if pa.w == Some(0) && colb < cola {
            return true;
        }
        if pb.w == Some(0) && cola < colb {
            return true;
        }

        // Value ordering: North.
        if rb < ra {
            if let (Some(vb), Some(va)) = (pb.n, pa.n) {
                if vb >= va {
                    return true;
                }
            }
        } else if ra < rb {
            if let (Some(va), Some(vb)) = (pa.n, pb.n) {
                if va >= vb {
                    return true;
                }
            }
        } else if let (Some(va), Some(vb)) = (pa.n, pb.n) {
            if va != vb {
                return true;
            }
        }
        // Value ordering: South.
        if rb > ra {
            if let (Some(vb), Some(va)) = (pb.s, pa.s) {
                if vb >= va {
                    return true;
                }
            }
        } else if ra > rb {
            if let (Some(va), Some(vb)) = (pa.s, pb.s) {
                if va >= vb {
                    return true;
                }
            }
        } else if let (Some(va), Some(vb)) = (pa.s, pb.s) {
            if va != vb {
                return true;
            }
        }
        // Value ordering: East.
        if colb > cola {
            if let (Some(vb), Some(va)) = (pb.e, pa.e) {
                if vb >= va {
                    return true;
                }
            }
        } else if cola > colb {
            if let (Some(va), Some(vb)) = (pa.e, pb.e) {
                if va >= vb {
                    return true;
                }
            }
        } else if let (Some(va), Some(vb)) = (pa.e, pb.e) {
            if va != vb {
                return true;
            }
        }
        // Value ordering: West.
        if colb < cola {
            if let (Some(vb), Some(va)) = (pb.w, pa.w) {
                if vb >= va {
                    return true;
                }
            }
        } else if cola < colb {
            if let (Some(va), Some(vb)) = (pa.w, pb.w) {
                if va >= vb {
                    return true;
                }
            }
        } else if let (Some(va), Some(vb)) = (pa.w, pb.w) {
            if va != vb {
                return true;
            }
        }

        false
    }

    /// Basic compass propagation: a direction pinned to 0 means the neighbour
    /// cell in that direction cannot be in the same piece → force Cut.
    pub(crate) fn propagate_compass(&mut self) -> Result<bool, ()> {
        let mut forced: Vec<EdgeId> = Vec::new();
        for cl in &self.cell_clues {
            let CellClue::Compass { cell, compass } = cl else {
                continue;
            };
            let cell = *cell;
            if !self.grid.cell_exists[cell] {
                continue;
            }
            let (r, c) = self.grid.cell_pos(cell);
            for &(dr, dc, val) in &[
                (-1isize, 0, compass.n),
                (0, 1, compass.e),
                (1, 0, compass.s),
                (0, -1, compass.w),
            ] {
                let Some(v) = val else { continue };
                if v != 0 {
                    continue;
                }
                let nr = r as isize + dr;
                let nc = c as isize + dc;
                if nr < 0
                    || nr >= self.grid.rows as isize
                    || nc < 0
                    || nc >= self.grid.cols as isize
                {
                    continue;
                }
                let nid = self.grid.cell_id(nr as usize, nc as usize);
                if !self.grid.cell_exists[nid] {
                    continue;
                }
                let Some(edge) = self.grid.edge_between(cell, nid) else {
                    continue;
                };
                if self.edges[edge] == EdgeState::Unknown {
                    forced.push(edge);
                } else if self.edges[edge] != EdgeState::Cut {
                    return Err(());
                }
            }
        }
        let mut progress = false;
        for e in forced {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }

    /// Compass directional-count propagation: for each compass clue in each
    /// component, count cells per direction and force Cut/Uncut to satisfy the
    /// exact direction counts, plus pair-wise compatibility and bounding-box
    /// pruning of growth edges.  (Bridge/gateway forcing is left to iteration 3.)
    fn propagate_compass_in_components(&mut self, num_comp: usize) -> Result<bool, ()> {
        let mut progress = false;
        let mut cut_ef: Vec<EdgeId> = Vec::new();
        let mut uncut_ef: Vec<EdgeId> = Vec::new();

        for &cl_idx in &self.prop.compass_clue_indices {
            let CellClue::Compass { cell, compass } = &self.cell_clues[cl_idx] else {
                continue;
            };
            let ci = self.curr_comp_id[*cell];
            if ci == usize::MAX || ci >= num_comp {
                continue;
            }
            let (cr, cc) = self.grid.cell_pos(*cell);

            // Count cells in each compass direction (single pass).
            let mut counts = [0usize; 4]; // N, S, E, W
            for &c in &self.comp_cells[ci] {
                let (pr, pc) = self.grid.cell_pos(c);
                if pr < cr {
                    counts[0] += 1;
                }
                if pr > cr {
                    counts[1] += 1;
                }
                if pc > cc {
                    counts[2] += 1;
                }
                if pc < cc {
                    counts[3] += 1;
                }
            }

            // Classify growth edges by direction.
            let mut dir_count = [0usize; 4];
            let mut dir_last = [0usize; 4];
            for &e in &self.prop.growth_edges[ci] {
                let (c1, c2) = self.grid.edge_cells(e);
                let other = if self.curr_comp_id[c1] == ci { c2 } else { c1 };
                let (pr, pc) = self.grid.cell_pos(other);
                if pr < cr {
                    dir_count[0] += 1;
                    dir_last[0] = e;
                }
                if pr > cr {
                    dir_count[1] += 1;
                    dir_last[1] = e;
                }
                if pc > cc {
                    dir_count[2] += 1;
                    dir_last[2] = e;
                }
                if pc < cc {
                    dir_count[3] += 1;
                    dir_last[3] = e;
                }
            }

            let compass_vals: [Option<usize>; 4] = [compass.n, compass.s, compass.e, compass.w];

            for idx in 0..4 {
                let Some(v) = compass_vals[idx] else { continue };
                if counts[idx] > v {
                    return Err(());
                }
                if counts[idx] == v {
                    // At limit: cut growth edges in this direction.
                    for &e in &self.prop.growth_edges[ci] {
                        let (c1, c2) = self.grid.edge_cells(e);
                        let other = if self.curr_comp_id[c1] == ci { c2 } else { c1 };
                        let (pr, pc) = self.grid.cell_pos(other);
                        let matches = match idx {
                            0 => pr < cr,
                            1 => pr > cr,
                            2 => pc > cc,
                            3 => pc < cc,
                            _ => false,
                        };
                        if matches {
                            cut_ef.push(e);
                        }
                    }
                }
                if counts[idx] < v {
                    // Below limit: if only 1 growth edge in this direction and all
                    // other directions are blocked, force Uncut.
                    if self.is_growing(ci) && dir_count[idx] == 1 {
                        let mut all_others_blocked = true;
                        for pidx in 0..4 {
                            if pidx == idx {
                                continue;
                            }
                            if let Some(pv) = compass_vals[pidx] {
                                if counts[pidx] < pv {
                                    all_others_blocked = false;
                                    break;
                                }
                            } else if dir_count[pidx] > 0 {
                                all_others_blocked = false;
                                break;
                            }
                        }
                        if all_others_blocked {
                            uncut_ef.push(dir_last[idx]);
                        }
                    }
                }
                // Sealed component with unsatisfied compass constraint.
                if self.is_sealed(ci) && counts[idx] < v {
                    return Err(());
                }
            }
        }

        // Pair-wise compass compatibility + bounding-box pruning.
        {
            let cci = &self.prop.compass_clue_indices;
            for ii in 0..cci.len() {
                let CellClue::Compass { cell: ca, compass: pa } = &self.cell_clues[cci[ii]] else {
                    continue;
                };
                let ci_a = self.curr_comp_id[*ca];
                if ci_a == usize::MAX {
                    continue;
                }
                for jj in (ii + 1)..cci.len() {
                    let CellClue::Compass { cell: cb, compass: pb } = &self.cell_clues[cci[jj]]
                    else {
                        continue;
                    };
                    if self.curr_comp_id[*cb] == ci_a && self.compass_cells_incompatible(*ca, pa, *cb, pb) {
                        return Err(());
                    }
                }
            }

            // Bounding box per component.
            let mut bbox_inited = vec![false; num_comp];
            let mut bbox_min_r = vec![0isize; num_comp];
            let mut bbox_max_r = vec![0isize; num_comp];
            let mut bbox_min_c = vec![0isize; num_comp];
            let mut bbox_max_c = vec![0isize; num_comp];

            for &cl_idx in &self.prop.compass_clue_indices {
                let CellClue::Compass { cell, compass } = &self.cell_clues[cl_idx] else {
                    continue;
                };
                let ci = self.curr_comp_id[*cell];
                if ci == usize::MAX || ci >= num_comp {
                    continue;
                }
                let (r, c) = self.grid.cell_pos(*cell);
                let (ri, ci_col) = (r as isize, c as isize);
                if !bbox_inited[ci] {
                    bbox_inited[ci] = true;
                    bbox_max_r[ci] = self.grid.rows as isize - 1;
                    bbox_max_c[ci] = self.grid.cols as isize - 1;
                }
                if let Some(v) = compass.n {
                    bbox_min_r[ci] = bbox_min_r[ci].max(ri - v as isize);
                }
                if let Some(v) = compass.s {
                    bbox_max_r[ci] = bbox_max_r[ci].min(ri + v as isize);
                }
                if let Some(v) = compass.e {
                    bbox_max_c[ci] = bbox_max_c[ci].min(ci_col + v as isize);
                }
                if let Some(v) = compass.w {
                    bbox_min_c[ci] = bbox_min_c[ci].max(ci_col - v as isize);
                }
            }

            for ci in 0..num_comp {
                if !bbox_inited[ci] {
                    continue;
                }
                if bbox_min_r[ci] > bbox_max_r[ci] || bbox_min_c[ci] > bbox_max_c[ci] {
                    return Err(());
                }
                for i in 0..self.prop.growth_edges[ci].len() {
                    let e = self.prop.growth_edges[ci][i];
                    if self.edges[e] != EdgeState::Unknown {
                        continue;
                    }
                    let (c1, c2) = self.grid.edge_cells(e);
                    let other = if self.curr_comp_id[c1] == ci { c2 } else { c1 };
                    let (pr, pc) = self.grid.cell_pos(other);
                    let (pri, pci) = (pr as isize, pc as isize);
                    if pri < bbox_min_r[ci]
                        || pri > bbox_max_r[ci]
                        || pci < bbox_min_c[ci]
                        || pci > bbox_max_c[ci]
                    {
                        if !self.set_edge(e, EdgeState::Cut) {
                            return Err(());
                        }
                        progress = true;
                    }
                }
            }
        }

        // Bridge/gateway forcing (skip during probing to avoid per-probe overhead).
        if !self.in_probing {
            let mut compass_per_comp: Vec<Vec<(CellId, CompassData)>> = vec![Vec::new(); num_comp];
            for &cl_idx in &self.prop.compass_clue_indices {
                if let CellClue::Compass { cell, compass } = &self.cell_clues[cl_idx] {
                    let ci = self.curr_comp_id[*cell];
                    if ci != usize::MAX && ci < num_comp {
                        compass_per_comp[ci].push((*cell, *compass));
                    }
                }
            }
            self.force_compass_via_bridges_and_gateways(
                &compass_per_comp,
                &mut cut_ef,
                &mut uncut_ef,
            )?;
        }

        for e in cut_ef {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        for e in uncut_ef {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Uncut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }

    /// Bridge/articulation-point based path forcing + single-gateway-edge forcing
    /// for growing components with unsatisfied compass directions.  Ported from
    /// `third_party/aog/src/solver/propagation/compass.rs:71`.
    fn force_compass_via_bridges_and_gateways(
        &mut self,
        compass_per_comp: &[Vec<(CellId, CompassData)>],
        compass_cut_ef: &mut Vec<EdgeId>,
        compass_uncut_ef: &mut Vec<EdgeId>,
    ) -> Result<(), ()> {
        for &ci in &self.prop.growing_list {
            if compass_per_comp[ci].is_empty() {
                continue;
            }

            // Collect unsatisfied directions: (dir_idx, target, compass_row, compass_col).
            let mut unsatisfied: Vec<(usize, usize, isize, isize)> = Vec::new();
            for &(cell, compass) in &compass_per_comp[ci] {
                let (cr, cc) = self.grid.cell_pos(cell);
                let (cri, cci) = (cr as isize, cc as isize);
                let mut counts = [0usize; 4];
                for &c in &self.comp_cells[ci] {
                    let (pr, pc) = self.grid.cell_pos(c);
                    let dr = pr as isize - cri;
                    let dc = pc as isize - cci;
                    if dr < 0 {
                        counts[0] += 1;
                    }
                    if dr > 0 {
                        counts[1] += 1;
                    }
                    if dc > 0 {
                        counts[2] += 1;
                    }
                    if dc < 0 {
                        counts[3] += 1;
                    }
                }
                for &(val, idx) in &[
                    (compass.n, 0usize),
                    (compass.s, 1),
                    (compass.e, 2),
                    (compass.w, 3),
                ] {
                    let Some(v) = val else { continue };
                    if counts[idx] < v {
                        unsatisfied.push((idx, v, cri, cci));
                    }
                }
            }
            if unsatisfied.is_empty() {
                continue;
            }

            // Build reachable subgraph from CI via non-Cut edges (BFS).
            let nc = self.grid.num_cells();
            let mut local_id = vec![usize::MAX; nc];
            let mut local_cells: Vec<CellId> = Vec::new();
            let mut queue: VecDeque<CellId> = VecDeque::new();
            for &c in &self.comp_cells[ci] {
                if local_id[c] == usize::MAX {
                    local_id[c] = local_cells.len();
                    local_cells.push(c);
                    queue.push_back(c);
                }
            }
            while let Some(cur) = queue.pop_front() {
                for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                    if self.edges[eid] == EdgeState::Cut {
                        continue;
                    }
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == cur { c2 } else { c1 };
                    if !self.grid.cell_exists[other] {
                        continue;
                    }
                    if local_id[other] == usize::MAX {
                        local_id[other] = local_cells.len();
                        local_cells.push(other);
                        queue.push_back(other);
                    }
                }
            }

            let n_local = local_cells.len();
            if n_local <= 1 {
                continue;
            }

            // Direction-reachability contradiction check.
            for &(dir_idx, v, cri, cci) in &unsatisfied {
                let reachable_dir = local_cells
                    .iter()
                    .filter(|&&c| {
                        let (pr, pc) = self.grid.cell_pos(c);
                        match dir_idx {
                            0 => (pr as isize) < cri,
                            1 => (pr as isize) > cri,
                            2 => (pc as isize) > cci,
                            3 => (pc as isize) < cci,
                            _ => false,
                        }
                    })
                    .count();
                if reachable_dir < v {
                    return Err(());
                }
            }

            // Build adjacency for the reachable subgraph.
            let mut adj: Vec<Vec<(usize, EdgeId)>> = vec![Vec::new(); n_local];
            for (li, &c) in local_cells.iter().enumerate() {
                for eid in self.grid.cell_edges(c).into_iter().flatten() {
                    if self.edges[eid] == EdgeState::Cut {
                        continue;
                    }
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == c { c2 } else { c1 };
                    let lj = local_id[other];
                    if lj == usize::MAX {
                        continue;
                    }
                    adj[li].push((lj, eid));
                }
            }

            // Tarjan bridge detection.
            let bridges = Self::find_bridges_in_subgraph(&adj, n_local);

            // Force Uncut on Unknown bridges that separate CI cells from cells
            // needed for an unsatisfied direction.
            for bridge_eid in bridges {
                if self.edges[bridge_eid] != EdgeState::Unknown {
                    continue;
                }
                let mut ci_side = vec![false; n_local];
                let mut bfs: VecDeque<usize> = VecDeque::new();
                for (i, &c) in local_cells.iter().enumerate() {
                    if self.curr_comp_id[c] == ci {
                        ci_side[i] = true;
                        bfs.push_back(i);
                    }
                }
                while let Some(u) = bfs.pop_front() {
                    for &(v, eid) in &adj[u] {
                        if eid == bridge_eid {
                            continue;
                        }
                        if !ci_side[v] {
                            ci_side[v] = true;
                            bfs.push_back(v);
                        }
                    }
                }

                let mut force_uncut = false;
                'dir_check: for &(dir_idx, v, cri, cci) in &unsatisfied {
                    let mut ci_side_count = 0usize;
                    let mut other_side_count = 0usize;
                    for (i, &cell) in local_cells.iter().enumerate() {
                        let cell_comp = self.curr_comp_id[cell];
                        if cell_comp != ci && cell_comp != usize::MAX {
                            continue;
                        }
                        let (pr, pc) = self.grid.cell_pos(cell);
                        let in_dir = match dir_idx {
                            0 => (pr as isize) < cri,
                            1 => (pr as isize) > cri,
                            2 => (pc as isize) > cci,
                            3 => (pc as isize) < cci,
                            _ => false,
                        };
                        if in_dir {
                            if ci_side[i] {
                                ci_side_count += 1;
                            } else {
                                other_side_count += 1;
                            }
                        }
                    }
                    if ci_side_count < v && other_side_count > 0 {
                        force_uncut = true;
                        break 'dir_check;
                    }
                }
                if force_uncut {
                    compass_uncut_ef.push(bridge_eid);
                }
            }

            // Single-gateway-edge forcing (skip if pending forced cuts — the
            // reachable subgraph would be stale).
            if !compass_cut_ef.is_empty() {
                continue;
            }

            // Fresh CI membership via current Uncut edges.
            let mut is_fresh_ci = vec![false; n_local];
            {
                let mut fc_bfs: VecDeque<usize> = VecDeque::new();
                for li in 0..n_local {
                    if self.curr_comp_id[local_cells[li]] == ci {
                        is_fresh_ci[li] = true;
                        fc_bfs.push_back(li);
                    }
                }
                while let Some(u) = fc_bfs.pop_front() {
                    for &(vj, eid) in &adj[u] {
                        if is_fresh_ci[vj] {
                            continue;
                        }
                        if self.edges[eid] == EdgeState::Uncut {
                            is_fresh_ci[vj] = true;
                            fc_bfs.push_back(vj);
                        }
                    }
                }
            }

            // For each unsatisfied direction, backward BFS from non-CI dir-cells;
            // any Unknown edge from CI to a reachable cell is a gateway edge.
            for &(dir_idx, _v, cri, cci) in &unsatisfied {
                let mut visited_local = vec![false; n_local];
                let mut bfs: VecDeque<usize> = VecDeque::new();
                for li in 0..n_local {
                    if is_fresh_ci[li] {
                        continue;
                    }
                    let c = local_cells[li];
                    if self.curr_comp_id[c] != usize::MAX {
                        continue;
                    }
                    let (pr, pc) = self.grid.cell_pos(c);
                    let in_dir = match dir_idx {
                        0 => (pr as isize) < cri,
                        1 => (pr as isize) > cri,
                        2 => (pc as isize) > cci,
                        3 => (pc as isize) < cci,
                        _ => false,
                    };
                    if in_dir {
                        visited_local[li] = true;
                        bfs.push_back(li);
                    }
                }
                if bfs.is_empty() {
                    continue;
                }
                while let Some(u) = bfs.pop_front() {
                    for &(vj, _eid) in &adj[u] {
                        if visited_local[vj] {
                            continue;
                        }
                        if is_fresh_ci[vj] {
                            continue;
                        }
                        if self.curr_comp_id[local_cells[vj]] != usize::MAX {
                            continue;
                        }
                        visited_local[vj] = true;
                        bfs.push_back(vj);
                    }
                }

                let mut gateway_edges: Vec<EdgeId> = Vec::new();
                for li in 0..n_local {
                    if !is_fresh_ci[li] {
                        continue;
                    }
                    for &(vj, eid) in &adj[li] {
                        if !visited_local[vj] {
                            continue;
                        }
                        if self.edges[eid] != EdgeState::Unknown {
                            continue;
                        }
                        gateway_edges.push(eid);
                    }
                }
                if gateway_edges.len() == 1 {
                    compass_uncut_ef.push(gateway_edges[0]);
                }
            }
        }
        Ok(())
    }

    /// Iterative Tarjan bridge detection on a local subgraph.
    /// `adj[u]` is a list of `(neighbor_local_id, EdgeId)`.
    fn find_bridges_in_subgraph(adj: &[Vec<(usize, EdgeId)>], n_local: usize) -> Vec<EdgeId> {
        let mut disc = vec![usize::MAX; n_local];
        let mut low = vec![0usize; n_local];
        let mut parent_edge: Vec<Option<EdgeId>> = vec![None; n_local];
        let mut timer = 0usize;
        let mut bridges: Vec<EdgeId> = Vec::new();
        let mut dfs_stack: Vec<(usize, usize)> = Vec::new();

        for root in 0..n_local {
            if disc[root] != usize::MAX {
                continue;
            }
            disc[root] = timer;
            low[root] = timer;
            timer += 1;
            dfs_stack.push((root, 0));

            while !dfs_stack.is_empty() {
                let (u, adj_idx) = *dfs_stack.last().unwrap();
                if adj_idx < adj[u].len() {
                    let (v, eid) = adj[u][adj_idx];
                    dfs_stack.last_mut().unwrap().1 += 1;
                    if disc[v] == usize::MAX {
                        parent_edge[v] = Some(eid);
                        disc[v] = timer;
                        low[v] = timer;
                        timer += 1;
                        dfs_stack.push((v, 0));
                    } else if Some(eid) != parent_edge[u] {
                        low[u] = low[u].min(disc[v]);
                    }
                } else {
                    dfs_stack.pop();
                    if let Some(&(p, _)) = dfs_stack.last() {
                        low[p] = low[p].min(low[u]);
                        if let Some(eid) = parent_edge[u] {
                            if low[u] > disc[p] {
                                bridges.push(eid);
                            }
                        }
                    }
                }
            }
        }
        bridges
    }

    /// Fence (palisade) propagation: enumerate the rotations compatible with the
    /// currently-decided edges around each fence cell, then force edges where
    /// every compatible rotation agrees (Cut/Uncut).
    pub(crate) fn propagate_palisade_constraints(&mut self) -> Result<bool, ()> {
        let mut forced_cut: Vec<EdgeId> = Vec::new();
        let mut forced_uncut: Vec<EdgeId> = Vec::new();

        for cl in &self.cell_clues {
            let CellClue::Palisade { cell, kind } = cl else {
                continue;
            };
            let cell = *cell;
            if !self.grid.cell_exists[cell] {
                continue;
            }

            // cell_edges order [north, south, west, east] == pattern mask bit order.
            let edges = self.grid.cell_edges(cell);
            let states: [EdgeState; 4] =
                edges.map(|e| e.map(|eid| self.edges[eid]).unwrap_or(EdgeState::Cut));

            let mut known_cuts = 0u8;
            let mut known_uncuts = 0u8;
            let mut known_cut_mask = 0u8;
            for k in 0..4 {
                match states[k] {
                    EdgeState::Cut => {
                        known_cuts += 1;
                        known_cut_mask |= 1 << k;
                    }
                    EdgeState::Uncut => {
                        known_uncuts += 1;
                    }
                    EdgeState::Unknown => {}
                }
            }

            let mut can_be_cut = [false; 4];
            let mut can_be_uncut = [false; 4];
            let mut any_compatible = false;

            for rot in 0..4 {
                let (ec, em) = kind.pattern_at_rotation(rot);
                let unknown_count = 4 - known_cuts - known_uncuts;
                if (known_cuts as usize) > ec {
                    continue;
                }
                if (known_cuts as usize) + (unknown_count as usize) < ec {
                    continue;
                }
                if (known_cut_mask & em) != known_cut_mask {
                    continue;
                }
                let known_uncut_mask: u8 = (0..4u8)
                    .filter(|&k| states[k as usize] == EdgeState::Uncut)
                    .fold(0, |m, k| m | (1 << k));
                if (known_uncut_mask & em) != 0 {
                    continue;
                }
                any_compatible = true;
                for k in 0..4 {
                    if (em >> k) & 1 == 1 {
                        can_be_cut[k] = true;
                    } else {
                        can_be_uncut[k] = true;
                    }
                }
            }

            if !any_compatible {
                return Err(());
            }

            for k in 0..4 {
                if states[k] != EdgeState::Unknown {
                    continue;
                }
                let Some(eid) = edges[k] else { continue };
                if can_be_cut[k] && !can_be_uncut[k] {
                    forced_cut.push(eid);
                } else if !can_be_cut[k] && can_be_uncut[k] {
                    forced_uncut.push(eid);
                }
            }
        }

        let mut progress = false;
        for e in forced_cut {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        for e in forced_uncut {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Uncut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }

    /// Failed-literal detection: probe each unknown edge; if one value
    /// contradicts, force the other.  Returns early on first force.
    fn probe_one_round(&mut self) -> Result<bool, ()> {
        let num_edges = self.grid.num_edges();
        for e in 0..num_edges {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let cut_ok = self.probe(|s| s.set_edge(e, EdgeState::Cut));
            if !cut_ok {
                if self.edges[e] == EdgeState::Unknown && self.set_edge(e, EdgeState::Uncut) {
                    return Ok(true);
                }
                continue;
            }
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let uncut_ok = self.probe(|s| s.set_edge(e, EdgeState::Uncut));
            if !uncut_ok {
                if self.edges[e] == EdgeState::Unknown && self.set_edge(e, EdgeState::Cut) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Probe pairs of edges sharing a vertex; if exactly one combination
    /// survives, force it.
    fn probe_pair_round(&mut self) -> Result<bool, ()> {
        let unknowns: Vec<EdgeId> = (0..self.grid.num_edges())
            .filter(|&e| self.edges[e] == EdgeState::Unknown)
            .collect();
        if unknowns.len() < 2 || unknowns.len() > 30 {
            return Ok(false);
        }

        let mut vert_edges: Vec<Vec<EdgeId>> = Vec::new();
        for &e in &unknowns {
            let (v1, v2) = self.grid.edge_vertices(e);
            while vert_edges.len() <= v1.max(v2) {
                vert_edges.push(Vec::new());
            }
            vert_edges[v1].push(e);
            vert_edges[v2].push(e);
        }

        let vals = [EdgeState::Cut, EdgeState::Uncut];
        for v_edges in &vert_edges {
            if v_edges.len() < 2 {
                continue;
            }
            for i in 0..v_edges.len() {
                let e1 = v_edges[i];
                if self.edges[e1] != EdgeState::Unknown {
                    continue;
                }
                for j in (i + 1)..v_edges.len() {
                    let e2 = v_edges[j];
                    if self.edges[e2] != EdgeState::Unknown {
                        continue;
                    }
                    let mut ok_count = 0usize;
                    let mut last_ok = (EdgeState::Cut, EdgeState::Cut);
                    for &v1 in &vals {
                        for &v2 in &vals {
                            let ok = self.probe(|s| s.set_edge(e1, v1) && s.set_edge(e2, v2));
                            if ok {
                                ok_count += 1;
                                last_ok = (v1, v2);
                            }
                        }
                    }
                    if ok_count == 1 {
                        let (v1, v2) = last_ok;
                        if self.edges[e1] == EdgeState::Unknown {
                            let _ = self.set_edge(e1, v1);
                        }
                        if self.edges[e2] == EdgeState::Unknown {
                            let _ = self.set_edge(e2, v2);
                        }
                        return Ok(true);
                    }
                    if ok_count == 0 {
                        return Err(());
                    }
                    if self.edges[e1] != EdgeState::Unknown {
                        break;
                    }
                }
            }
        }
        Ok(false)
    }

    /// Propagate watchtower (vertex) clues — port of
    /// `third_party/aog/src/solver/propagation/watchtower.rs::propagate_watchtower`.
    ///
    /// For a vertex surrounded by N existing cells with E internal edges:
    ///   - N=4, E=4 (2×2 block, one cycle): pieces = max(1, k) where k = cut edges
    ///   - N=2..3 (tree): pieces = 1 + k
    ///   - N=1: always 1 piece (no edges to propagate)
    ///
    /// value=2 and value=3 on cycles allow more than the minimum cuts because a
    /// piece reaching the vertex via two different paths (double-touching, e.g.
    /// around a hole) is counted once. Only value=1 enforces exact cut counts on
    /// cycles. value=v constrains the required number of cut edges accordingly.
    ///
    /// Two passes:
    ///   **Pass A (component-ID)** — when `curr_comp_id` is populated (after
    ///   `build_components`), counts distinct sealed/growing components touching
    ///   the vertex to get a `[min_distinct, max_distinct]` range; contradiction
    ///   if `value` is outside it, and force `Cut` on Unknown edges between
    ///   different components when `max_distinct == value`.
    ///   **Pass B (edge-count)** — counts Cut/Unknown among the 4 internal edges
    ///   and forces the remaining Unknowns to reach the required cut count.
    pub(crate) fn propagate_watchtower(&mut self) -> Result<bool, ()> {
        if self.vertex_clues.is_empty() {
            return Ok(false);
        }
        let mut progress = false;
        // Adjacent cell pairs in the 2×2 layout: (TL,TR), (TL,BL), (TR,BR), (BL,BR).
        let cell_pair_indices: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];

        // === Pass A: component-ID-based (distinct region counting) ===
        if !self.curr_comp_id.is_empty() {
            // Collect (is_err, forced_cuts) per clue to avoid borrow conflicts.
            let comp_id_results: Vec<(bool, Vec<EdgeId>)> = self
                .vertex_clues
                .iter()
                .map(|clue| {
                    let (vi, vj) = self.grid.vertex_pos(clue.vertex);
                    let cell_opts = self.grid.vertex_cells(vi, vj);
                    let value = clue.value;

                    let cells: Vec<CellId> = cell_opts
                        .iter()
                        .copied()
                        .flatten()
                        .filter(|&cid| self.grid.cell_exists[cid])
                        .collect();
                    let n = cells.len();
                    if n == 0 || value > n || (n == 1 && value > 1) {
                        return (false, vec![]); // caught by edge-based pass
                    }

                    // Deduplicate component IDs (at most 4 cells).
                    let mut comp_arr = [usize::MAX; 4];
                    let mut comp_count = 0usize;
                    for &c in &cells {
                        let ci = self.curr_comp_id[c];
                        if !comp_arr[..comp_count].contains(&ci) {
                            comp_arr[comp_count] = ci;
                            comp_count += 1;
                        }
                    }
                    let mut num_sealed = 0usize;
                    for &ci in &comp_arr[..comp_count] {
                        if self.is_sealed(ci) {
                            num_sealed += 1;
                        }
                    }
                    let num_growing = comp_count - num_sealed;

                    let min_distinct = num_sealed + if num_growing > 0 { 1 } else { 0 };
                    let max_distinct = comp_count;

                    let is_err = value < min_distinct || value > max_distinct;

                    let mut forced_cuts = Vec::new();
                    if max_distinct == value && comp_count > 1 {
                        for &(a_idx, b_idx) in &cell_pair_indices {
                            if let (Some(a), Some(b)) = (cell_opts[a_idx], cell_opts[b_idx]) {
                                if !self.grid.cell_exists[a] || !self.grid.cell_exists[b] {
                                    continue;
                                }
                                if self.curr_comp_id[a] != self.curr_comp_id[b] {
                                    if let Some(eid) = self.grid.edge_between(a, b) {
                                        if self.edges[eid] == EdgeState::Unknown {
                                            forced_cuts.push(eid);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    (is_err, forced_cuts)
                })
                .collect();

            for (is_err, _) in &comp_id_results {
                if *is_err {
                    return Err(());
                }
            }
            for (_, forced_cuts) in &comp_id_results {
                for &eid in forced_cuts {
                    if !self.set_edge(eid, EdgeState::Cut) {
                        return Err(());
                    }
                    progress = true;
                }
            }
        }

        // === Pass B: edge-count-based ===
        let constraints: Vec<(usize, usize, usize, Vec<EdgeId>, bool)> = self
            .vertex_clues
            .iter()
            .filter_map(|clue| {
                let (vi, vj) = self.grid.vertex_pos(clue.vertex);
                let cell_opts = self.grid.vertex_cells(vi, vj);
                let value = clue.value;

                let n = cell_opts
                    .iter()
                    .copied()
                    .flatten()
                    .filter(|&cid| self.grid.cell_exists[cid])
                    .count();
                if n == 0 || (n == 1 && value == 1) {
                    return None; // nothing to propagate
                }
                if value > n {
                    return Some((vi, vj, value, vec![], false));
                }
                if n == 1 {
                    return Some((vi, vj, value, vec![], false));
                }

                let mut edge_ids = Vec::new();
                for &(a_idx, b_idx) in &cell_pair_indices {
                    if let (Some(a_cid), Some(b_cid)) = (cell_opts[a_idx], cell_opts[b_idx]) {
                        if self.grid.cell_exists[a_cid] && self.grid.cell_exists[b_cid] {
                            if let Some(eid) = self.grid.edge_between(a_cid, b_cid) {
                                edge_ids.push(eid);
                            }
                        }
                    }
                }
                let is_cycle = n == 4 && edge_ids.len() == 4;
                Some((vi, vj, value, edge_ids, is_cycle))
            })
            .collect();

        for (_vi, _vj, value, edge_ids, is_cycle) in constraints {
            if edge_ids.is_empty() && value > 1 {
                return Err(());
            }
            if edge_ids.is_empty() {
                continue;
            }
            let mut n_cut = 0usize;
            let mut unk = Vec::new();
            for &eid in &edge_ids {
                match self.edges[eid] {
                    EdgeState::Cut => n_cut += 1,
                    EdgeState::Unknown => unk.push(eid),
                    EdgeState::Uncut => {}
                }
            }

            if is_cycle {
                // 4 cells, 4 edges, one cycle: pieces = max(1, k).
                if value == 1 {
                    // exact: k ≥ 2 always gives ≥ 2 pieces.
                    if n_cut >= 2 {
                        return Err(());
                    }
                    if n_cut == 1 && !unk.is_empty() {
                        for eid in unk {
                            if !self.set_edge(eid, EdgeState::Uncut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                } else {
                    // value >= 2: lower bound only (double-touching allows k > value).
                    if n_cut + unk.len() < value {
                        return Err(());
                    }
                    if n_cut + unk.len() == value && !unk.is_empty() {
                        for eid in unk {
                            if !self.set_edge(eid, EdgeState::Cut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                }
            } else {
                // Tree (2 or 3 cells): pieces = 1 + k.
                let needed_k = value.saturating_sub(1);
                if value == 2 {
                    // lower bound only
                    if n_cut + unk.len() < needed_k {
                        return Err(());
                    }
                    if n_cut + unk.len() == needed_k && !unk.is_empty() {
                        for eid in unk {
                            if !self.set_edge(eid, EdgeState::Cut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                } else {
                    // value == 1 or value >= 3: exact cuts
                    if n_cut > needed_k {
                        return Err(());
                    }
                    if n_cut == needed_k && !unk.is_empty() {
                        for eid in unk {
                            if !self.set_edge(eid, EdgeState::Uncut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    } else if n_cut + unk.len() < needed_k {
                        return Err(());
                    } else if n_cut + unk.len() == needed_k && !unk.is_empty() {
                        for eid in unk {
                            if !self.set_edge(eid, EdgeState::Cut) {
                                return Err(());
                            }
                            progress = true;
                        }
                    }
                }
            }
        }
        Ok(progress)
    }

    /// Edge-level parity propagation for watchtower vertices — port of
    /// `third_party/aog watchtower.rs::propagate_vertex_edge_parity`.
    ///
    /// For each watchtower vertex with a **deterministic** cut-count parity,
    /// builds pairwise XOR constraints between its unknown edges and propagates
    /// them globally through a Union-Find. Parity is only fixed for:
    ///   - cycle (4 cells): value==4 (k=4, parity 0); value≤3 skipped (double-
    ///     touching makes k ∈ {value..4}, parity unfixed).
    ///   - tree (2-3 cells): value∈{1,3,4} (k = value-1 fixed); value==2 skipped
    ///     (k ∈ {1,2}, parity unfixed).
    ///
    /// When a constraint has 0 unknowns → check parity; 1 unknown → force it;
    /// 2 unknowns → `uf.union` with XOR. Phase 2 reduces 3+-unknown constraints
    /// using pairs already in the same UF component. Phase 3 cascades known edge
    /// values through the UF to resolve remaining unknowns.
    pub(crate) fn propagate_vertex_edge_parity(&mut self) -> Result<bool, ()> {
        if self.vertex_clues.is_empty() {
            return Ok(false);
        }
        let ne = self.grid.num_edges();
        let pair_idx: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];

        // Collect vertex constraints: (edge_ids, required_parity).
        let constraints: Vec<(Vec<EdgeId>, u8)> = self
            .vertex_clues
            .iter()
            .filter_map(|clue| {
                let (vi, vj) = self.grid.vertex_pos(clue.vertex);
                let cell_opts = self.grid.vertex_cells(vi, vj);
                let n = cell_opts
                    .iter()
                    .copied()
                    .flatten()
                    .filter(|&cid| self.grid.cell_exists[cid])
                    .count();
                if n < 2 {
                    return None;
                }
                let is_cycle = n == 4;
                let required_k = if is_cycle {
                    if clue.value <= 1 {
                        return None; // k ∈ {0,1}, parity not fixed
                    }
                    if clue.value <= 3 {
                        return None; // k ∈ {value,..4}, parity not fixed (double-touching)
                    }
                    clue.value
                } else {
                    if clue.value == 2 {
                        return None; // k ∈ {1,2}, parity not fixed
                    }
                    clue.value.saturating_sub(1)
                };
                let mut edge_ids: Vec<EdgeId> = Vec::new();
                for &(a, b) in &pair_idx {
                    if let (Some(ca), Some(cb)) = (cell_opts[a], cell_opts[b]) {
                        if self.grid.cell_exists[ca] && self.grid.cell_exists[cb] {
                            if let Some(eid) = self.grid.edge_between(ca, cb) {
                                edge_ids.push(eid);
                            }
                        }
                    }
                }
                if edge_ids.len() < 2 {
                    return None;
                }
                Some((edge_ids, (required_k & 1) as u8))
            })
            .collect();

        if constraints.is_empty() {
            return Ok(false);
        }

        let mut uf = ParityUF::new(ne);
        // ev: 0=Uncut, 1=Cut, 2=Unknown
        let mut ev: Vec<u8> = self
            .edges
            .iter()
            .map(|&e| match e {
                EdgeState::Cut => 1,
                EdgeState::Uncut => 0,
                EdgeState::Unknown => 2,
            })
            .collect();

        // Forced edges to apply after the UF analysis (collected to avoid borrow
        // conflicts with `uf` / `ev`).
        let mut forced: Vec<(EdgeId, EdgeState)> = Vec::new();

        // Phase 1: build UF from pairwise constraints (0, 1, or 2 unknowns).
        for (edge_ids, parity) in &constraints {
            let mut kx = 0u8;
            let mut unks: Vec<EdgeId> = Vec::new();
            for &e in edge_ids {
                if ev[e] <= 1 {
                    kx ^= ev[e];
                } else {
                    unks.push(e);
                }
            }
            match unks.len() {
                0 => {
                    if kx != *parity {
                        return Err(());
                    }
                }
                1 => {
                    let v = kx ^ parity;
                    ev[unks[0]] = v;
                    forced.push((
                        unks[0],
                        if v == 1 { EdgeState::Cut } else { EdgeState::Uncut },
                    ));
                }
                2 => {
                    uf.union(unks[0], unks[1], kx ^ parity)?;
                }
                _ => {}
            }
        }

        // Phase 2: resolve 3+-unknown constraints using UF pairs already merged.
        for (edge_ids, parity) in &constraints {
            let mut kx = 0u8;
            let mut unks: Vec<EdgeId> = Vec::new();
            for &e in edge_ids {
                if ev[e] <= 1 {
                    kx ^= ev[e];
                } else {
                    unks.push(e);
                }
            }
            if unks.len() < 3 {
                continue;
            }
            let target = kx ^ parity;
            'outer: for i in 0..unks.len() {
                for j in (i + 1)..unks.len() {
                    let (r1, p1) = uf.find(unks[i]);
                    let (r2, p2) = uf.find(unks[j]);
                    if r1 == r2 {
                        let xij = p1 ^ p2;
                        let rem: Vec<EdgeId> = unks
                            .iter()
                            .enumerate()
                            .filter(|(idx, _)| *idx != i && *idx != j)
                            .map(|(_, &e)| e)
                            .collect();
                        if rem.len() == 1 {
                            let v = target ^ xij;
                            ev[rem[0]] = v;
                            forced.push((
                                rem[0],
                                if v == 1 { EdgeState::Cut } else { EdgeState::Uncut },
                            ));
                        } else if rem.len() == 2 {
                            uf.union(rem[0], rem[1], target ^ xij)?;
                        }
                        break 'outer;
                    }
                }
            }
        }

        // Phase 3: cascade known edge values through the UF to resolve unknowns.
        let mut rv: Vec<Option<u8>> = vec![None; ne];
        for e in 0..ne {
            if ev[e] > 1 {
                continue;
            }
            let (root, p) = uf.find(e);
            let r = p ^ ev[e];
            if let Some(ex) = rv[root] {
                if ex != r {
                    return Err(());
                }
            } else {
                rv[root] = Some(r);
            }
        }
        for e in 0..ne {
            if ev[e] <= 1 {
                continue;
            }
            let (root, p) = uf.find(e);
            if let Some(r) = rv[root] {
                let v = p ^ r;
                ev[e] = v;
                forced.push((e, if v == 1 { EdgeState::Cut } else { EdgeState::Uncut }));
            }
        }

        if forced.is_empty() {
            return Ok(false);
        }
        let mut progress = false;
        for (eid, st) in forced {
            if !self.set_edge(eid, st) {
                return Err(());
            }
            progress = true;
        }
        Ok(progress)
    }

    /// Iterative vertex-level watchtower config probing — port of
    /// `third_party/aog watchtower.rs::probe_watchtower_vertex_configs`.
    ///
    /// **Disabled (dead code):** benchmarked on the 85-puzzle watchtower set,
    /// it added **0 new solves** beyond the parity propagator
    /// (`propagate_vertex_edge_parity`) — the parity UF already captures the
    /// forcible edges. Kept as dead code (`#[allow(dead_code)]`) for future
    /// compass+watchtower puzzles where config enumeration may add value; not
    /// wired into `solve()` to avoid the startup enumeration cost.
    ///
    /// For each watchtower vertex, enumerate all valid Cut/Uncut configurations
    /// of its unknown internal edges (bitmask over ≤4 unknowns). If an edge is
    /// Cut in *all* surviving configs → force Cut; Uncut in all → force Uncut.
    /// Contradiction (0 surviving) → restore this iteration. Loops until no
    /// progress. Called once at solver startup (before the main `propagate`),
    /// not in the fixed-point loop.
    ///
    /// `possible_ks` per vertex (valid cut counts):
    ///   - loopy cycle: value==2 → k=2 only (k=3 forbidden by loopy, k=4
    ///     uncertain); else none.
    ///   - non-loopy cycle: value==2 → {2,3,4}; value==3 → {3,4}; else {value}
    ///     (double-touching allows k > value).
    ///   - tree: k = value-1.
    #[allow(dead_code)]
    pub(crate) fn probe_watchtower_vertex_configs(&mut self) -> usize {
        if self.in_probing {
            return 0;
        }
        let cell_pair_indices: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];
        let is_loopy = self.rules.loopy;
        let mut total_forced = 0usize;
        let saved = self.in_probing;
        self.in_probing = true;

        loop {
            let snap_iteration = self.snapshot();

            let vertex_info: Vec<(Vec<usize>, Vec<EdgeId>)> = self
                .vertex_clues
                .iter()
                .filter_map(|clue| {
                    let (vi, vj) = self.grid.vertex_pos(clue.vertex);
                    let value = clue.value;
                    let cell_opts = self.grid.vertex_cells(vi, vj);
                    let n = cell_opts
                        .iter()
                        .copied()
                        .flatten()
                        .filter(|&cid| self.grid.cell_exists[cid])
                        .count();
                    if n == 0 || n == 1 {
                        return None;
                    }
                    let is_cycle = n == 4;
                    let possible_ks: Vec<usize> = if is_cycle {
                        if is_loopy {
                            match value {
                                2 => vec![2],
                                _ => vec![],
                            }
                        } else {
                            match value {
                                2 => vec![2, 3, 4],
                                3 => vec![3, 4],
                                _ => vec![value],
                            }
                        }
                    } else {
                        vec![value.saturating_sub(1)]
                    };
                    if possible_ks.is_empty() {
                        return None;
                    }
                    if possible_ks.iter().all(|&k| k == 0) {
                        return None;
                    }
                    let mut edge_ids: Vec<EdgeId> = Vec::new();
                    for &(a_idx, b_idx) in &cell_pair_indices {
                        if let (Some(a), Some(b)) = (cell_opts[a_idx], cell_opts[b_idx]) {
                            if self.grid.cell_exists[a] && self.grid.cell_exists[b] {
                                if let Some(eid) = self.grid.edge_between(a, b) {
                                    edge_ids.push(eid);
                                }
                            }
                        }
                    }
                    if edge_ids.len() < 2 {
                        return None;
                    }
                    Some((possible_ks, edge_ids))
                })
                .collect();

            if vertex_info.is_empty() {
                break;
            }

            let mut made_progress = false;

            for (possible_ks, edge_ids) in &vertex_info {
                let states: Vec<EdgeState> = edge_ids.iter().map(|&e| self.edges[e]).collect();
                let n_cut = states.iter().filter(|&&s| s == EdgeState::Cut).count();
                let n_unk = states.iter().filter(|&&s| s == EdgeState::Unknown).count();

                let any_achievable = possible_ks
                    .iter()
                    .any(|&k| k >= n_cut && k.saturating_sub(n_cut) <= n_unk);
                if !any_achievable {
                    break; // contradiction this iteration
                }
                let all_satisfied = possible_ks.iter().all(|&k| n_cut == k);
                if all_satisfied {
                    continue;
                }
                if n_unk > 4 || n_unk == 0 {
                    continue;
                }

                let unk_indices: Vec<usize> = states
                    .iter()
                    .enumerate()
                    .filter(|(_, &s)| s == EdgeState::Unknown)
                    .map(|(i, _)| i)
                    .collect();
                let nm = unk_indices.len();
                let mut edge_cut_count: Vec<usize> = vec![0; nm];
                let mut total_surviving = 0usize;

                for &k in possible_ks {
                    let remaining = k.saturating_sub(n_cut);
                    if remaining > n_unk {
                        continue;
                    }
                    if remaining == 0 {
                        total_surviving += 1;
                    } else {
                        for mask in 0u32..(1u32 << nm) {
                            if mask.count_ones() as usize != remaining {
                                continue;
                            }
                            let ok = self.probe(|s| {
                                for (bit, &idx) in unk_indices.iter().enumerate() {
                                    let val = if (mask >> bit) & 1 == 1 {
                                        EdgeState::Cut
                                    } else {
                                        EdgeState::Uncut
                                    };
                                    if !s.set_edge(edge_ids[idx], val) {
                                        return false;
                                    }
                                }
                                true
                            });
                            if ok {
                                total_surviving += 1;
                                for (bit, _) in unk_indices.iter().enumerate() {
                                    if (mask >> bit) & 1 == 1 {
                                        edge_cut_count[bit] += 1;
                                    }
                                }
                            }
                        }
                    }
                }

                if total_surviving == 0 {
                    self.restore(snap_iteration);
                    total_forced = 0;
                    made_progress = false;
                    break;
                }

                let snap_before_force = self.snapshot();
                let mut forced_here = 0usize;
                for (bit, &idx) in unk_indices.iter().enumerate() {
                    if self.edges[edge_ids[idx]] != EdgeState::Unknown {
                        continue;
                    }
                    if edge_cut_count[bit] == total_surviving {
                        let _ = self.set_edge(edge_ids[idx], EdgeState::Cut);
                        total_forced += 1;
                        forced_here += 1;
                    } else if edge_cut_count[bit] == 0 {
                        let _ = self.set_edge(edge_ids[idx], EdgeState::Uncut);
                        total_forced += 1;
                        forced_here += 1;
                    }
                }
                if forced_here > 0 {
                    if self.propagate().is_err() {
                        self.restore(snap_before_force);
                        total_forced -= forced_here;
                    } else {
                        made_progress = true;
                    }
                }
            }

            if !made_progress {
                break;
            }
        }

        self.in_probing = saved;
        total_forced
    }

    /// DFS for compass placement enumeration — port of
    /// `third_party/aog area.rs::compass_placement_dfs` (2129-2213).
    ///
    /// Enumerates all valid connected component merges via include/exclude
    /// branching on the smallest-index frontier component. Returns `true` if
    /// the result set overflowed (too many placements → caller skips forcing).
    fn compass_placement_dfs(
        current_mask: u32,
        frontier_mask: u32,
        excluded_mask: u32,
        counts: [usize; 4],
        size: usize,
        comp_dir_counts: &[[usize; 4]],
        comp_sizes: &[usize],
        adj_mask: &[u32],
        limits: &[Option<usize>; 4],
        min_a: usize,
        max_a: usize,
        max_placements: usize,
        results: &mut Vec<u32>,
    ) -> bool {
        // Record if the current merged set is a valid placement (size ≥ min_a
        // AND every specified direction hit exactly).
        if size >= min_a {
            let satisfied = (0..4).all(|d| limits[d].map_or(true, |v| counts[d] == v));
            if satisfied {
                results.push(current_mask);
                if results.len() >= max_placements {
                    return true; // overflow
                }
            }
        }

        if size >= max_a || frontier_mask == 0 {
            return false;
        }

        // Pick the smallest-index frontier component.
        let v = frontier_mask.trailing_zeros() as usize;
        let v_bit = 1u32 << v;
        let rest = frontier_mask & !v_bit;

        // Branch 1: include component v.
        let new_counts = [
            counts[0] + comp_dir_counts[v][0],
            counts[1] + comp_dir_counts[v][1],
            counts[2] + comp_dir_counts[v][2],
            counts[3] + comp_dir_counts[v][3],
        ];
        let new_size = size + comp_sizes[v];
        let exceeds = (0..4).any(|d| limits[d].map_or(false, |lim| new_counts[d] > lim));
        if !exceeds && new_size <= max_a {
            let new_current = current_mask | v_bit;
            let new_frontier = rest | (adj_mask[v] & !new_current & !excluded_mask);
            if Self::compass_placement_dfs(
                new_current,
                new_frontier,
                excluded_mask,
                new_counts,
                new_size,
                comp_dir_counts,
                comp_sizes,
                adj_mask,
                limits,
                min_a,
                max_a,
                max_placements,
                results,
            ) {
                return true;
            }
        }

        // Branch 2: exclude component v.
        Self::compass_placement_dfs(
            current_mask,
            rest,
            excluded_mask | v_bit,
            counts,
            size,
            comp_dir_counts,
            comp_sizes,
            adj_mask,
            limits,
            min_a,
            max_a,
            max_placements,
            results,
        )
    }

    /// Compass placement enumeration — port of
    /// `third_party/aog area.rs::propagate_compass_placement_enumeration`
    /// (1742-2124). For each compass clue whose component max_area ≤ 12, build
    /// fresh local components (global Uncut flood-fill), enumerate all valid
    /// connected merges satisfying the direction limits via DFS, then force
    /// Cut/Uncut on Unknown growth edges based on the intersection (in_all) and
    /// union (in_any) of valid placements. Self-gates on `in_probing`.
    fn propagate_compass_placement_enumeration(&mut self) -> Result<bool, ()> {
        const MAX_AREA_THRESHOLD: usize = 12;
        const MAX_REACHABLE_COMPS: usize = 16;
        const MAX_PLACEMENTS: usize = 500;

        if self.in_probing {
            return Ok(false);
        }
        if !self.has_compass_clue {
            return Ok(false);
        }

        let mut forced_cuts: Vec<EdgeId> = Vec::new();
        let mut forced_uncuts: Vec<EdgeId> = Vec::new();

        // Compass clues (cell, compass) — use the precomputed index for speed.
        let compass_entries: Vec<(CellId, CompassData)> = self
            .prop
            .compass_clue_indices
            .iter()
            .filter_map(|&i| match &self.cell_clues[i] {
                CellClue::Compass { cell, compass } if self.grid.cell_exists[*cell] => {
                    Some((*cell, *compass))
                }
                _ => None,
            })
            .collect();

        'outer: for &(cell, compass) in &compass_entries {
            let ci = if cell < self.curr_comp_id.len() {
                self.curr_comp_id[cell]
            } else {
                usize::MAX
            };
            if ci == usize::MAX {
                continue;
            }
            if self.is_sealed(ci) {
                continue;
            }
            let max_a = if ci < self.prop.curr_max_area.len() {
                self.prop.curr_max_area[ci]
            } else {
                continue;
            };
            let min_a = if ci < self.prop.curr_min_area.len() {
                self.prop.curr_min_area[ci]
            } else {
                continue;
            };
            if max_a > MAX_AREA_THRESHOLD {
                continue;
            }

            let (cr, cc) = self.grid.cell_pos(cell);
            let (cri, cci) = (cr as isize, cc as isize);
            // Compass limits [N, S, E, W].
            let limits = [compass.n, compass.s, compass.e, compass.w];

            // Bounding box from compass constraints (tightest possible).
            let bbox_min_r = limits[0].map_or(0isize, |v| cri - v as isize).max(0);
            let bbox_max_r = limits[1]
                .map_or(self.grid.rows as isize - 1, |v| cri + v as isize)
                .min(self.grid.rows as isize - 1);
            let bbox_min_c = limits[3].map_or(0isize, |v| cci - v as isize).max(0);
            let bbox_max_c = limits[2]
                .map_or(self.grid.cols as isize - 1, |v| cci + v as isize)
                .min(self.grid.cols as isize - 1);

            let n = self.grid.num_cells();
            // Step 1: BFS reachable cells from compass cell via non-Cut edges, in bbox.
            let mut cell_in_reachable = vec![false; n];
            let mut reachable_cells: Vec<CellId> = Vec::new();
            {
                cell_in_reachable[cell] = true;
                reachable_cells.push(cell);
                let mut bfs_q: VecDeque<CellId> = VecDeque::new();
                bfs_q.push_back(cell);
                while let Some(cur) = bfs_q.pop_front() {
                    for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                        if self.edges[eid] == EdgeState::Cut {
                            continue;
                        }
                        let (c1, c2) = self.grid.edge_cells(eid);
                        let other = if c1 == cur { c2 } else { c1 };
                        if !self.grid.cell_exists[other] || cell_in_reachable[other] {
                            continue;
                        }
                        let (pr, pc) = self.grid.cell_pos(other);
                        if (pr as isize) < bbox_min_r
                            || (pr as isize) > bbox_max_r
                            || (pc as isize) < bbox_min_c
                            || (pc as isize) > bbox_max_c
                        {
                            continue;
                        }
                        cell_in_reachable[other] = true;
                        reachable_cells.push(other);
                        bfs_q.push_back(other);
                    }
                }
            }

            // Step 2: group reachable cells into fresh local components via CURRENT
            // Uncut edges. IMPORTANT: follow Uncut edges GLOBALLY (beyond bbox) so a
            // cell committed to an outside piece drags that piece in — prevents false
            // forced-uncuts. The compass cell ends up in local component 0.
            let mut local_comp_of = vec![usize::MAX; n];
            let mut local_comps: Vec<Vec<CellId>> = Vec::new();
            for &start in &reachable_cells {
                if local_comp_of[start] != usize::MAX {
                    continue;
                }
                if local_comps.len() >= MAX_REACHABLE_COMPS {
                    continue 'outer;
                }
                let lc = local_comps.len();
                let mut lcomp_cells = vec![start];
                local_comp_of[start] = lc;
                let mut q: VecDeque<CellId> = VecDeque::new();
                q.push_back(start);
                while let Some(cur) = q.pop_front() {
                    for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                        if self.edges[eid] != EdgeState::Uncut {
                            continue;
                        }
                        let (c1, c2) = self.grid.edge_cells(eid);
                        let other = if c1 == cur { c2 } else { c1 };
                        if !self.grid.cell_exists[other] || local_comp_of[other] != usize::MAX {
                            continue;
                        }
                        local_comp_of[other] = lc;
                        lcomp_cells.push(other);
                        q.push_back(other);
                    }
                }
                local_comps.push(lcomp_cells);
            }

            // Ensure compass cell is in local component 0 (swap if needed).
            let compass_lc = local_comp_of[cell];
            if compass_lc != 0 {
                local_comps.swap(0, compass_lc);
                for &c in &local_comps[0] {
                    local_comp_of[c] = 0;
                }
                for &c in &local_comps[compass_lc] {
                    local_comp_of[c] = compass_lc;
                }
            }

            let num_rc = local_comps.len();
            if num_rc > MAX_REACHABLE_COMPS {
                continue 'outer;
            }

            // Check if local comp 0 can still grow (Unknown edges to other local comps).
            let can_grow = local_comps[0].iter().any(|&c| {
                self.grid.cell_edges(c).into_iter().flatten().any(|eid| {
                    if self.edges[eid] != EdgeState::Unknown {
                        return false;
                    }
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == c { c2 } else { c1 };
                    cell_in_reachable[other]
                        && local_comp_of[other] != 0
                        && local_comp_of[other] != usize::MAX
                })
            });
            let has_outside_growth = local_comps[0].iter().any(|&c| {
                self.grid.cell_edges(c).into_iter().flatten().any(|eid| {
                    if self.edges[eid] != EdgeState::Unknown {
                        return false;
                    }
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == c { c2 } else { c1 };
                    self.grid.cell_exists[other] && !cell_in_reachable[other]
                })
            });
            if !can_grow && !has_outside_growth {
                continue;
            }

            // Per-component directional counts and sizes.
            let mut comp_dir_counts = vec![[0usize; 4]; num_rc];
            let mut comp_sizes = vec![0usize; num_rc];
            for (lc, lcomp) in local_comps.iter().enumerate() {
                for &c in lcomp {
                    let (pr, pc) = self.grid.cell_pos(c);
                    let dr = pr as isize - cri;
                    let dc = pc as isize - cci;
                    if dr < 0 {
                        comp_dir_counts[lc][0] += 1; // N
                    }
                    if dr > 0 {
                        comp_dir_counts[lc][1] += 1; // S
                    }
                    if dc > 0 {
                        comp_dir_counts[lc][2] += 1; // E
                    }
                    if dc < 0 {
                        comp_dir_counts[lc][3] += 1; // W
                    }
                    comp_sizes[lc] += 1;
                }
            }

            // Base-count feasibility check on comp 0.
            for d in 0..4 {
                if let Some(v) = limits[d] {
                    if comp_dir_counts[0][d] > v {
                        return Err(());
                    }
                }
            }

            // Component adjacency bitmask via Unknown edges (iterate ALL cells in
            // each local comp, including outside-bbox ones from global flood-fill).
            let mut adj_mask = vec![0u32; num_rc];
            for lc in 0..num_rc {
                for ci in 0..local_comps[lc].len() {
                    let c = local_comps[lc][ci];
                    for eid in self.grid.cell_edges(c).into_iter().flatten() {
                        if self.edges[eid] != EdgeState::Unknown {
                            continue;
                        }
                        let (c1, c2) = self.grid.edge_cells(eid);
                        let other = if c1 == c { c2 } else { c1 };
                        if !self.grid.cell_exists[other] {
                            continue;
                        }
                        let l2 = local_comp_of[other];
                        if l2 == usize::MAX || l2 == lc {
                            continue;
                        }
                        adj_mask[lc] |= 1u32 << l2;
                    }
                }
            }

            // Enumerate valid connected merges via DFS (local comp 0 = mandatory start).
            let mut valid_placements: Vec<u32> = Vec::new();
            let overflow = Self::compass_placement_dfs(
                1u32,
                adj_mask[0],
                0u32,
                comp_dir_counts[0],
                comp_sizes[0],
                &comp_dir_counts,
                &comp_sizes,
                &adj_mask,
                &limits,
                min_a,
                max_a,
                MAX_PLACEMENTS,
                &mut valid_placements,
            );
            if overflow {
                continue 'outer;
            }
            if valid_placements.is_empty() {
                return Err(());
            }

            // Intersection (in_all) + union (in_any) over valid placements.
            let mut in_all: u32 = u32::MAX;
            let mut in_any: u32 = 0;
            for &m in &valid_placements {
                in_all &= m;
                in_any |= m;
            }
            in_all &= !1u32; // local comp 0 always merged

            // Force Cut/Uncut on Unknown growth edges from local comp 0.
            for &c in &local_comps[0] {
                for eid in self.grid.cell_edges(c).into_iter().flatten() {
                    if self.edges[eid] != EdgeState::Unknown {
                        continue;
                    }
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == c { c2 } else { c1 };
                    if !self.grid.cell_exists[other] {
                        continue;
                    }
                    if !cell_in_reachable[other] {
                        forced_cuts.push(eid); // outside bbox → Cut
                        continue;
                    }
                    let lj = local_comp_of[other];
                    if lj == usize::MAX || lj == 0 {
                        continue;
                    }
                    let bit = 1u32 << lj;
                    if in_all & bit != 0 {
                        forced_uncuts.push(eid);
                    } else if in_any & bit == 0 {
                        forced_cuts.push(eid);
                    }
                }
            }
        }

        let mut progress = false;
        for &e in &forced_cuts {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        for &e in &forced_uncuts {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Uncut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }
}
