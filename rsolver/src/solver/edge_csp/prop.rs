//! Propagation: fixed-point loop + propagators.
//!
//! Ported from `third_party/aog/src/solver/propagation/`, slimmed to the rules
//! this solver targets.  The core is `build_components` (flood-fill decided-Uncut
//! edges into components, compute per-component target/min/max area and growth
//! edges), plus vertex-degree (`bricky_loopy`), area-target sealing,
//! inequality/difference clue propagation, and failed-literal probing.

use super::types::*;
use super::Solver;

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
            if self.has_compass_clue {
                progress |= self.propagate_compass()?;
            }
            if self.has_palisade_clue {
                progress |= self.propagate_palisade_constraints()?;
            }
            progress |= self.propagate_area_bounds()?;

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
}
