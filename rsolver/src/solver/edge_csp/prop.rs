//! Propagation: fixed-point loop + propagators.
//!
//! Ported from `third_party/aog/src/solver/propagation/`, slimmed to the rules
//! this solver targets.  The core is `build_components` (flood-fill decided-Uncut
//! edges into components, compute per-component target/min/max area and growth
//! edges), plus vertex-degree (`bricky_loopy`), area-target sealing,
//! inequality/difference clue propagation, and failed-literal probing.

use super::types::*;
use super::parity_uf::ParityUF;
use super::polyomino::{canonical, make_shape};
use super::Solver;
use std::collections::{BTreeSet, VecDeque};

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
    /// Scratch for `build_components`' representative → contiguous-id map.
    /// Hoisted out of the function because it used to `vec![usize::MAX; n]` on
    /// every call, and `build_components` now runs once per progress-making
    /// sub-propagator per fixed-point round — at ~1M search nodes that
    /// allocation churn showed up on the compass cluster.
    pub id_map_buf: Vec<usize>,
    /// Scratch for `solitary_potential_connectivity`'s non-Cut flood-fill.
    pub pot_buf: Vec<usize>,
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
            id_map_buf: vec![usize::MAX; nc],
            pot_buf: vec![usize::MAX; nc],
            growing_list: Vec::new(),
            sealed_list: Vec::new(),
        }
    }
}

/// Per-bridge, per-direction reachability info for the H4 "sole isolating
/// bridge" guard in `force_compass_via_bridges_and_gateways`.
///
/// For each Unknown bridge edge we record, per unsatisfied compass direction:
/// - `other`: number of direction-cells that would become unreachable from the
///   compass if this bridge were cut, and
/// - `ciside_lt`: whether the still-reachable side holds fewer than the
///   direction's required count `v`.
///
/// A bridge is forced Uncut only when it is the SOLE isolating bridge for some
/// direction (`isolating_count == 1`) — when ≥2 bridges are each individually
/// isolating, forcing all of them Uncut over-merges the component and can push
/// the direction count past `v`, contradicting valid "cut only A" solutions.
struct BridgeInfo {
    eid: usize,
    other: Vec<usize>,
    ciside_lt: Vec<bool>,
}

/// Pairwise zero-value direction conflict: a direction pinned to `0` forbids a
/// neighbour in that direction from being in the same piece.
#[inline]
fn compass_zero_conflict(
    ra: usize,
    rb: usize,
    cola: usize,
    colb: usize,
    pa: &CompassData,
    pb: &CompassData,
) -> bool {
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
    false
}

/// Diagnostic: `EDGE_CSP_SKIP=compass,solitary,probe` disables the named
/// propagators so a soundness bug can be bisected without a rebuild.
fn csp_skip(name: &str) -> bool {
    match std::env::var("EDGE_CSP_SKIP") {
        Ok(v) => v.split(',').any(|s| s == name),
        Err(_) => false,
    }
}

/// Pairwise value-ordering conflict along one compass axis (North/South/East/
/// West).  `before` is true when `a` is strictly before `b` along this axis,
/// `after` when strictly after.
#[inline]
fn compass_dir_conflict(before: bool, after: bool, a_val: Option<usize>, b_val: Option<usize>) -> bool {
    if before {
        if let (Some(vb), Some(va)) = (b_val, a_val) {
            if vb >= va {
                return true;
            }
        }
    } else if after {
        if let (Some(va), Some(vb)) = (a_val, b_val) {
            if va >= vb {
                return true;
            }
        }
    } else if let (Some(va), Some(vb)) = (a_val, b_val) {
        if va != vb {
            return true;
        }
    }
    false
}

impl<'a> Solver<'a> {
    /// Fixed-point propagation.  Returns `Ok(true)` when stable (no further
    /// progress), `Err(())` on contradiction or timeout.
    pub(crate) fn propagate(&mut self) -> Result<bool, ()> {
        // Diagnostic: `EDGE_CSP_SKIP=compass,solitary,probe` disables named
        // propagators so a soundness bug can be bisected without a rebuild.
        loop {
            if self.check_deadline() {
                return Err(());
            }
            let mut progress = false;

            if (self.rules.bricky || self.rules.loopy) && !csp_skip("bricky") {
                progress |= self.propagate_bricky_loopy()?;
            }
            if self.exact_piece_count == Some(2) && !csp_skip("vparity2") {
                progress |= self.propagate_two_piece_vertex_parity()?;
            }
            if self.exact_piece_count.is_some() || self.structural_pieces.is_some() {
                progress |= self.propagate_loop_closure()?;
            }
            if !self.vertex_clues.is_empty() {
                progress |= self.propagate_vertex_edge_parity()?;
            }
            if self.has_compass_clue && !csp_skip("compass") {
                progress |= self.propagate_compass()?;
            }
            if self.has_palisade_clue {
                progress |= self.propagate_palisade_constraints()?;
            }
            if self.has_gemini_and_delta {
                progress |= self.propagate_delta_gemini_interaction()?;
            }
            let mut area_progress = false;
            if !csp_skip("area") {
                area_progress = self.propagate_area_bounds()?;
                progress |= area_progress;
            }
            // `propagate_area_bounds` may have set edges after `build_components`
            // ran (growth-edge cuts, solitary S4 Uncut, …), which leaves
            // `curr_comp_id` / `curr_comp_sz` stale.  D0/D2 read those, so a
            // stale `cc` used to be turned into a false `cc > pieces`
            // contradiction — defer dual connectivity to the next iteration,
            // where components are rebuilt.
            if self.structural_pieces.is_some() && !csp_skip("dual") && !area_progress {
                let num_comp = self.curr_comp_sz.len();
                progress |= self.propagate_dual_connectivity(num_comp)?;
            }
            if !self.vertex_clues.is_empty() {
                progress |= self.propagate_watchtower()?;
                progress |= self.propagate_watchtower_degree()?;
            }
            if self.rose_bits_all != 0 {
                progress |= self.propagate_rose_separation()?;
                progress |= self.propagate_rose_phase3()?;
                progress |= self.propagate_parity()?;
            }

            if !progress {
                // Failed-literal detection: probe unknown edges / edge pairs.
                if !self.in_probing && self.curr_unknown > 0 && self.curr_unknown <= 256 && !csp_skip("probe") {
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

    /// Number of closed loops in the Cut-edge vertex graph (all-even-degree
    /// components).  Odd-degree components are open paths and not counted.
    fn cut_loop_count(&self) -> usize {
        let ne = self.grid.num_edges();
        let nv = (self.grid.rows + 1) * (self.grid.cols + 1);
        let mut parent: Vec<usize> = (0..nv).collect();
        let mut rank: Vec<u8> = vec![0; nv];
        let mut cut_deg: Vec<u8> = vec![0; nv];
        let mut any_cut = false;
        for e in 0..ne {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            any_cut = true;
            let (v1, v2) = self.grid.edge_vertices(e);
            cut_deg[v1] += 1;
            cut_deg[v2] += 1;
            let (mut r1, mut r2) = (v1, v2);
            while parent[r1] != r1 {
                r1 = parent[r1];
            }
            while parent[r2] != r2 {
                r2 = parent[r2];
            }
            if r1 != r2 {
                if rank[r1] < rank[r2] {
                    parent[r1] = r2;
                } else if rank[r1] > rank[r2] {
                    parent[r2] = r1;
                } else {
                    parent[r2] = r1;
                    rank[r1] += 1;
                }
            }
        }
        if !any_cut {
            return 0;
        }
        let mut odd: Vec<u8> = vec![0; nv];
        for v in 0..nv {
            if cut_deg[v] % 2 == 1 {
                let mut r = v;
                while parent[r] != r {
                    r = parent[r];
                }
                odd[r] += 1;
            }
        }
        let mut seen = vec![false; nv];
        let mut num_loops = 0usize;
        for v in 0..nv {
            if cut_deg[v] == 0 {
                continue;
            }
            let mut r = v;
            while parent[r] != r {
                r = parent[r];
            }
            if !seen[r] {
                seen[r] = true;
                if odd[r] == 0 {
                    num_loops += 1;
                }
            }
        }
        num_loops
    }

    fn is_border_vertex(&self, v: usize) -> bool {
        let (i, j) = self.grid.vertex_pos(v);
        i == 0 || i == self.grid.rows || j == 0 || j == self.grid.cols
    }

    /// Loop-closure detection on the Cut-edge vertex graph (rules 1 + 4 of
    /// `third_party/aog/src/solver/propagation/loop_closure.rs`).  The
    /// reference's `@`-vertex degree rules are deliberately NOT ported: they
    /// assume `watchtower` value == required cut-degree, while our (and the
    /// game's) semantics is the distinct-region count — 241 of 471 official
    /// answers have cut-degree ≠ 2 at value-2 vertices.  Rules 1 and 4 are
    /// pure cut-graph theory, verified against 544 / 19 official answers with
    /// 0 violations:
    ///   1. the number of closed cut-loops cannot exceed `pieces - 1`;
    ///   4. with `max_loops == 1` under ring (no T-junctions) the single
    ///      interface cannot end at the grid border (that would be a degree-3
    ///      junction with the outer border), so every edge touching a border
    ///      vertex is internal to the outer piece and must be Uncut.
    pub(crate) fn propagate_loop_closure(&mut self) -> Result<bool, ()> {
        let Some(pieces) = self.exact_piece_count.or(self.structural_pieces) else {
            return Ok(false);
        };
        if pieces < 2 {
            return Ok(false);
        }
        let max_loops = pieces - 1;
        // Rule 1 only holds under ring (no T-junctions): a closed loop's
        // vertices are degree-saturated, so loop components only accrete.  On
        // brick-only puzzles degree-3 T-links are legal and separate loops can
        // later *merge* — an intermediate `loops > max_loops` state stays
        // completable (1294: root-level brick cuts looked like 5 loops against
        // max=2, the official 3-piece answer merges them via T-junctions).
        if self.rules.loopy {
            let loops = self.cut_loop_count();
            if loops > max_loops {
                return Err(());
            }
        }
        if max_loops == 1 && self.rules.loopy {
            let mut progress = false;
            for e in 0..self.grid.num_edges() {
                let (v1, v2) = self.grid.edge_vertices(e);
                if !self.is_border_vertex(v1) && !self.is_border_vertex(v2) {
                    continue;
                }
                match self.edges[e] {
                    EdgeState::Cut => return Err(()),
                    EdgeState::Unknown => {
                        if !self.set_edge(e, EdgeState::Uncut) {
                            return Err(());
                        }
                        progress = true;
                    }
                    EdgeState::Uncut => {}
                }
            }
            if progress {
                return Ok(true);
            }
        }
        Ok(false)
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
                    // Force Uncut only when EVERY unknown must go Uncut
                    // (`cut_count == 2`): with `cut_count < 2` and too many
                    // unknowns we only know that *some* of them end Uncut, and
                    // picking the first `n` was an unsound gamble — on 1378 it
                    // forced the W/E edges of cell (2,2) Uncut at the root and
                    // the palisade propagator then rejected the official
                    // solution (all 14 ring+brick FAILs share this path).
                    if cut_count == 2 {
                        for &eid in &unk_edges {
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
                    // Same soundness rule as the ring+brick branch: force
                    // Uncut only when every unknown must go Uncut.
                    if cut_count == 3 {
                        for &eid in &unk_edges {
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
    ///
    /// Compass axes are **half-planes** (`_compass_halfplane_count` in
    /// `src/solver/constraints.py`): a NE cell counts in both N and E, so the
    /// four direction counts overlap and `size ≠ 1 + Σ`.  With `Q` = cells in
    /// the four quadrants (counted twice by the sum):
    ///   `size = 1 + (n+s) + (e+w) - Q`, hence
    ///   `size ≥ 1 + max(n+s, e+w)`   (Q = min(n+s, e+w)) — the tightest lower
    ///   bound expressible without knowing Q, and
    ///   `size ≤ 1 + Σ dir_max`       (Q = 0) — a valid upper bound once every
    ///   direction is capped.
    ///
    /// An unspecified direction (`-1`) is capped by the board: at most
    /// `compass_halfplane_avail[cell][d]` existing cells lie in that
    /// half-plane.  Previously an unspecified direction left `max_area = None`
    /// (unbounded), which disabled the `size == max → seal` inference,
    /// `growth_potential` capping and the compass placement-enumeration
    /// threshold for every clue carrying a `-1` — the common case on the
    /// compass+solitary FAIL cluster (only 58/971 corpus clues qualified).
    /// A direction with no existing cell at all is inferred 0, as before.
    fn get_compass_area_bounds(
        &self,
        cell: CellId,
        compass: &CompassData,
    ) -> (usize, Option<usize>, Option<usize>) {
        let avail = self.compass_halfplane_avail[cell];

        let n = compass.n.or_else(|| if avail[0] == 0 { Some(0) } else { None });
        let s = compass.s.or_else(|| if avail[1] == 0 { Some(0) } else { None });
        let e = compass.e.or_else(|| if avail[2] == 0 { Some(0) } else { None });
        let w = compass.w.or_else(|| if avail[3] == 0 { Some(0) } else { None });

        let nv = n.unwrap_or(0);
        let sv = s.unwrap_or(0);
        let ev = e.unwrap_or(0);
        let wv = w.unwrap_or(0);

        let min_area = 1 + (nv + sv).max(ev + wv);

        // `exact_area` is only exact when *all four* counts are known: with E
        // and W pinned to 0 every region cell sits in the clue's column, but
        // the column's extent is `n + s`, which is unknown as soon as one of
        // them is unspecified (-1).  Treating the unspecified side as 0 (the
        // old `n.unwrap_or(0)`) under-counted the region and turned a valid
        // board into a false `size > target` contradiction.
        let mut exact_area = None;
        if e == Some(0) && w == Some(0) && n.is_some() && s.is_some() {
            exact_area = Some(1 + nv + sv);
        } else if n == Some(0) && s == Some(0) && e.is_some() && w.is_some() {
            exact_area = Some(1 + ev + wv);
        }

        // Upper bound: known directions contribute their exact value; unknown
        // ones contribute the number of cells that half-plane can hold.
        let cap = |known: Option<usize>, idx: usize| known.unwrap_or(avail[idx]);
        let max_area = Some(1 + cap(n, 0) + cap(s, 1) + cap(e, 2) + cap(w, 3));

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
    pub(crate) fn build_components(&mut self) -> Result<usize, ()> {
        let n = self.grid.num_cells();
        self.prop.comp_buf.fill(usize::MAX);
        for c in 0..n {
            if !self.grid.cell_exists[c] || self.prop.comp_buf[c] != usize::MAX {
                continue;
            }
            self.flood_fill_decided(c);
        }

        // Map component representatives to contiguous ids (reusable scratch —
        // see `PropagationState::id_map_buf`).  Reset *before* use: this
        // function has early `return Err(())` paths, and a post-loop reset
        // would be skipped there, leaving the scratch dirty for the next call
        // (which then produced a wrong `num_comp` — 1017 went from a 5s solve
        // to a 7ms false exhaust).
        let mut num_comp = 0usize;
        self.prop.id_map_buf[..n].fill(usize::MAX);
        for c in 0..n {
            if !self.grid.cell_exists[c] {
                continue;
            }
            let rep = self.prop.comp_buf[c];
            if self.prop.id_map_buf[rep] == usize::MAX {
                self.prop.id_map_buf[rep] = num_comp;
                num_comp += 1;
            }
        }

        self.curr_comp_id.resize(n, usize::MAX);
        for c in 0..n {
            if self.grid.cell_exists[c] {
                self.curr_comp_id[c] = self.prop.id_map_buf[self.prop.comp_buf[c]];
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
            let (target_area, local_min, local_max) = self.compute_component_area_bounds(ci)?;

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
        self.build_progress = false;
        self.build_components_growth_edges(num_comp)?;

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

    /// Per-component area bounds: fold `cell_clues`/`compass` constraints into
    /// `(target_area, min, max)`.  Returns `Err(())` on an internal area clash.
    fn compute_component_area_bounds(
        &mut self,
        ci: usize,
    ) -> Result<(Option<usize>, usize, usize), ()> {
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

        Ok((target_area, local_min, local_max))
    }

    /// Growth-edge pass: classify Unknown edges straddling two different
    /// components, force `Cut` where the merged region would violate area
    /// bounds or a distinct target area, and record remaining growth edges.
    fn build_components_growth_edges(&mut self, num_comp: usize) -> Result<(), ()> {
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
                    self.build_progress = true;
                    continue;
                }

                // Area-feasibility cut (sound): an Uncut edge merges `ci1` and
                // `ci2` into one region whose size is *at least* `sz1 + sz2`,
                // and that region must satisfy each component's area upper
                // bound.  If that minimum already exceeds either component's
                // `max_area`, no valid solution can uncut this edge -> force Cut.
                // Strictly reduces branching on area / precise / range puzzles
                // (e.g. 0289's 5M-node explosion) and prunes no valid solution.
                let sum_sz = self.curr_comp_sz[ci1] + self.curr_comp_sz[ci2];
                if sum_sz > self.prop.curr_max_area[ci1]
                    || sum_sz > self.prop.curr_max_area[ci2]
                {
                    if !self.set_edge(e, EdgeState::Cut) {
                        return Err(());
                    }
                    self.build_progress = true;
                    continue;
                }

                self.can_grow_buf[ci1] = true;
                self.can_grow_buf[ci2] = true;
                self.prop.growth_edges[ci1].push(e);
                self.prop.growth_edges[ci2].push(e);

                let limit1 = self.prop.curr_max_area[ci1];
                let limit2 = self.prop.curr_max_area[ci2];
                if self.curr_comp_sz[ci1] >= limit1 || self.curr_comp_sz[ci2] >= limit2 {
                    if !self.set_edge(e, EdgeState::Cut) {
                        return Err(());
                    }
                    self.build_progress = true;
                }
            }
        }
        Ok(())
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

        let mut progress = self.build_progress;
        progress |= self.propagate_area_constraints(num_comp)?;
        Ok(progress)
    }

    /// Area-target sealing, inequality and difference clue propagation.
    ///
    /// Each sub-propagator below reads `curr_comp_id` / `curr_comp_sz` /
    /// `growth_edges` and may write edges, which invalidates those arrays.  A
    /// later sub-propagator that trusts the stale snapshot can therefore see a
    /// "sealed clue-less component" that has in fact already merged
    /// (`propagate_solitary` S3 on 1017), or a `cc` computed over a
    /// half-updated graph.  So whenever a step reports progress the components
    /// are rebuilt before the next step runs.
    fn propagate_area_constraints(&mut self, mut num_comp: usize) -> Result<bool, ()> {
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

        let sealing = !csp_skip("areatgt");
        for ci in 0..num_comp {
            if !sealing {
                break;
            }
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
        if progress {
            num_comp = self.build_components()?;
        }
        progress |= self.propagate_component_clues(num_comp)?;
        Ok(progress)
    }

    /// Per-rule propagation over the current component snapshot.  Any step that
    /// reports progress rebuilds the components first, so the next step never
    /// reads a stale `curr_comp_id` / `growth_edges` (see the method doc on
    /// `propagate_area_constraints`).
    fn propagate_component_clues(&mut self, mut num_comp: usize) -> Result<bool, ()> {
        let mut progress = false;
        // `step!` runs one propagator and, when it changed any edge, rebuilds
        // the component snapshot for whatever comes next.  `$num` is refreshed
        // in place; `$prog` accumulates across steps.
        macro_rules! step {
            ($prog:ident, $num:ident, $call:expr) => {{
                if $call? {
                    $prog = true;
                    $num = self.build_components()?;
                }
            }};
        }
        step!(progress, num_comp, self.propagate_inequality_clues(num_comp));
        step!(progress, num_comp, self.propagate_diff_clues(num_comp));
        if self.has_compass_clue && !csp_skip("compass_in_comp") {
            step!(
                progress,
                num_comp,
                self.propagate_compass_in_components(num_comp)
            );
        }
        if self.has_compass_clue && !csp_skip("compass_enum") {
            step!(
                progress,
                num_comp,
                self.propagate_compass_placement_enumeration()
            );
        }
        if self.rules.size_separation {
            step!(
                progress,
                num_comp,
                self.propagate_size_separation(num_comp)
            );
        }
        if self.rules.boxy || self.rules.non_boxy {
            step!(progress, num_comp, self.propagate_boxy_nonboxy(num_comp));
        }
        // solitary S4 writes Uncut, which merges components; the rebuild inside
        // `step!` makes the sealed-pair / shape checks below see the
        // post-merge partition.
        if self.rules.solitary && !csp_skip("solitary") {
            step!(progress, num_comp, self.propagate_solitary(num_comp));
        }
        if !csp_skip("areachk") {
            self.check_size_separation_sealed_pairs(num_comp)?;
            // Gemini: the two regions across a gemini edge must match in area.
            self.check_gemini_pairs(num_comp)?;
        }
        // Gemini/Delta shape-identity: sealed regions across a gemini edge must
        // share a canonical shape; across a delta edge they must differ.
        // Also `same`/`different`/`mixed` global shape-identity rules.
        progress |= self.propagate_shape_constraints(num_comp)?;
        Ok(progress)
    }

    /// `solitary` propagation: every finished piece holds exactly one clue cell,
    /// so clue cells stand in bijection with pieces.  edge_csp previously only
    /// leaf-checked this rule via `validate::validate`, which meant a
    /// compass+solitary puzzle burned its whole budget rediscovering that two
    /// clues had been merged.
    ///
    /// Every inference below only discards assignments that appear in no valid
    /// solution, so the search stays complete:
    ///
    /// * **S2** a component holding ≥2 clue cells already breaks the rule —
    ///   components only ever grow, so the clue count can never come back down
    ///   → contradiction.
    /// * **S7** an Unknown edge between two components that each already hold a
    ///   clue must be Cut: merging them would trigger S2.  (This subsumes the
    ///   adjacent-clue-cell special case, since a lone clue cell is its own
    ///   single-cell component.)
    /// * **S3** a *sealed* component holding no clue cell is a finished piece
    ///   with zero clues → contradiction.
    /// * **S4** a clue-less component with exactly one Unknown growth edge must
    ///   use it — cutting it would seal the component clue-less (S3) → Uncut.
    ///
    /// Ordering matters for soundness: S7 only ever writes Cut, which cannot
    /// change connectivity over Uncut edges and so leaves `comp_cells` /
    /// `curr_comp_id` valid.  S4 writes Uncut, which *merges* components and
    /// invalidates them, so all S4 edges are decided against one consistent
    /// snapshot and applied last; the fixed-point loop then rebuilds components.
    ///
    /// S5 (compass-bbox feasibility) is factored out into
    /// `propagate_solitary_feasibility`.
    ///
    /// S5 — compass-bbox feasibility, gated on `solitary_feasible_active`
    /// (see `solve()`): every piece holds exactly one clue, so a cell may only
    /// join the piece of a clue whose bbox covers it.
    /// * **S5a** a cell outside every clue's bbox can belong to no piece.
    /// * **S5b** adjacent cells with no shared candidate clue can never be the
    ///   same piece → the edge between them must be Cut.
    /// * **S5c** a component already holding compass clue `i` is that clue's
    ///   piece, so every cell in it must lie in clue `i`'s bbox.
    /// S5d — potential connectivity for `solitary` + compass.
    ///
    /// Flood-fills over *non-Cut* edges (Uncut or still Unknown) to get the
    /// "may still end up connected" components.  A cell whose only candidate
    /// clue is `i` must land in region `i`, and a region is Uncut-connected,
    /// so it has to share a potential component with clue `i`.  A Cut that
    /// severs every such path is a contradiction that S5a/S5b/S5c cannot see
    /// (they never look at reachability).  One flood-fill for the whole board,
    /// so this is O(cells + edges).
    fn solitary_potential_connectivity(&mut self) -> Result<(), ()> {
        let n = self.grid.num_cells();
        // Reset *before* use — see the `id_map_buf` note above (the singleton
        // check below can `return Err(())`, and a post-loop reset would be
        // skipped).
        self.prop.pot_buf[..n].fill(usize::MAX);
        let mut pot_id = 0usize;
        for c in 0..n {
            if !self.grid.cell_exists[c] || self.prop.pot_buf[c] != usize::MAX {
                continue;
            }
            self.prop.pot_buf[c] = pot_id;
            self.q_buf.clear();
            self.q_buf.push(c);
            while let Some(cur) = self.q_buf.pop() {
                for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                    if self.edges[eid] == EdgeState::Cut {
                        continue;
                    }
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == cur { c2 } else { c1 };
                    if !self.grid.cell_exists[other] || self.prop.pot_buf[other] != usize::MAX {
                        continue;
                    }
                    self.prop.pot_buf[other] = pot_id;
                    self.q_buf.push(other);
                }
            }
            pot_id += 1;
        }
        let mut clue_pot = vec![usize::MAX; 64];
        for (bit, &cl_idx) in self.prop.compass_clue_indices.iter().enumerate() {
            if bit >= clue_pot.len() {
                break;
            }
            if let CellClue::Compass { cell, .. } = &self.cell_clues[cl_idx] {
                if self.grid.cell_exists[*cell] {
                    clue_pot[bit] = self.prop.pot_buf[*cell];
                }
            }
        }
        for c in 0..n {
            if !self.grid.cell_exists[c] {
                continue;
            }
            let m = self.solitary_feasible[c];
            // Singleton mask: exactly one candidate clue.
            if m == 0 || (m & (m - 1)) != 0 {
                continue;
            }
            let bit = m.trailing_zeros() as usize;
            if bit < clue_pot.len() && clue_pot[bit] != usize::MAX && self.prop.pot_buf[c] != clue_pot[bit] {
                return Err(());
            }
        }
        Ok(())
    }

    fn propagate_solitary_feasibility(&mut self, num_comp: usize) -> Result<bool, ()> {
        let mut progress = false;
        let n = self.grid.num_cells();
        // S5a.
        for c in 0..n {
            if self.grid.cell_exists[c] && self.solitary_feasible[c] == 0 {
                return Err(());
            }
        }
        // S5b.
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            if self.solitary_feasible[c1] & self.solitary_feasible[c2] == 0 {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        // S5c.
        let mut clue_bit = vec![u8::MAX; n];
        for (bit, &cl_idx) in self.prop.compass_clue_indices.iter().enumerate() {
            if let CellClue::Compass { cell, .. } = &self.cell_clues[cl_idx] {
                clue_bit[*cell] = bit as u8;
            }
        }
        for ci in 0..num_comp {
            let mut bit: Option<u8> = None;
            for &c in &self.comp_cells[ci] {
                if clue_bit[c] != u8::MAX {
                    bit = Some(clue_bit[c]);
                    break;
                }
            }
            let Some(b) = bit else { continue };
            let mask = 1u64 << b;
            for &c in &self.comp_cells[ci] {
                if self.solitary_feasible[c] & mask == 0 {
                    return Err(());
                }
            }
        }
        self.solitary_potential_connectivity()?;
        Ok(progress)
    }

    fn propagate_solitary(&mut self, num_comp: usize) -> Result<bool, ()> {
        let mut progress = false;

        // S5 — compass-bbox feasibility (only when `solitary_feasible_active`).
        if self.solitary_feasible_active {
            progress |= self.propagate_solitary_feasibility(num_comp)?;
        }

        // Per-component clue counts + S2.
        let mut clues_in = vec![0usize; num_comp];
        for ci in 0..num_comp {
            let mut n = 0usize;
            for &c in &self.comp_cells[ci] {
                if self.clue_cell[c] {
                    n += 1;
                }
            }
            if n >= 2 {
                return Err(());
            }
            clues_in[ci] = n;
        }

        // S7: separate two clue-bearing components.
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let (ci1, ci2) = (self.curr_comp_id[c1], self.curr_comp_id[c2]);
            if ci1 == ci2 {
                continue;
            }
            if clues_in[ci1] >= 1 && clues_in[ci2] >= 1 {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }

        // S3 / S4 on clue-less components, decided against the post-S7 snapshot.
        let mut to_uncut: Vec<EdgeId> = Vec::new();
        for ci in 0..num_comp {
            if clues_in[ci] != 0 {
                continue;
            }
            let mut unknown: Option<EdgeId> = None;
            let mut count = 0usize;
            for &e in &self.prop.growth_edges[ci] {
                if self.edges[e] == EdgeState::Unknown {
                    count += 1;
                    if count > 1 {
                        break;
                    }
                    unknown = Some(e);
                }
            }
            match count {
                0 => return Err(()), // S3
                1 => to_uncut.push(unknown.unwrap()),
                _ => {}
            }
        }
        for e in to_uncut {
            // A previously-decided edge here is not a contradiction: two adjacent
            // clue-less components can share their single growth edge, so the
            // first Uncut already merged them (and satisfies both).
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Uncut) {
                    return Err(());
                }
                progress = true;
            }
        }

        Ok(progress)
    }

    /// D0 — exact piece count bounds the component count.
    ///
    /// Components only ever *merge* (setting an Unknown edge to Cut never
    /// splits an Uncut-connected set), so the final region count is at most
    /// `num_comp`.  With an exact piece count `K`:
    ///
    /// * `num_comp < K` → contradiction (cannot create more regions);
    /// * `num_comp == K` → the partition is frozen: no component may still
    ///   need to grow, and every Unknown edge between two distinct components
    ///   must be Cut.
    ///
    /// Returns `Ok(None)` when neither rule fires, `Ok(Some(progress))` when
    /// the partition was frozen (the caller must return immediately — D1
    /// would merge, which is now illegal, and D2's graph has no edges left).
    fn propagate_piece_count_bound(
        &mut self,
        num_comp: usize,
        exact: Option<usize>,
    ) -> Result<Option<bool>, ()> {
        let Some(k) = exact else { return Ok(None) };
        if num_comp < k {
            return Err(());
        }
        if num_comp != k {
            return Ok(None);
        }
        // Partition is frozen: every component *is* a final region.
        for ci in 0..num_comp {
            let sz = self.curr_comp_sz[ci];
            let must_grow = match self.curr_target_area[ci] {
                Some(t) => sz < t,
                None => sz < self.prop.curr_min_area[ci],
            };
            if must_grow {
                return Err(());
            }
        }
        let mut progress = false;
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            if self.curr_comp_id[c1] == self.curr_comp_id[c2] {
                continue;
            }
            if !self.set_edge(e, EdgeState::Cut) {
                return Err(());
            }
            progress = true;
        }
        Ok(Some(progress))
    }

    /// Dual connectivity (port of `third_party/aog/src/solver/propagation/
    /// dual.rs`, checks D1/D2, plus the D0 piece-count bound we add here).
    /// Gated on `structural_pieces` — an exact piece count derived from a
    /// *structural* rule (`precise` / `rose_window` / `solitary`).
    ///
    /// * **D0** components only ever *merge* (setting an Unknown edge to Cut
    ///   never splits an Uncut-connected set), so the final region count is at
    ///   most `num_comp`.  With an exact piece count `K` that means
    ///   `num_comp < K` is a contradiction, and `num_comp == K` freezes the
    ///   partition: no component may still need to grow, and every Unknown
    ///   edge between two distinct components must be Cut.
    /// * **D1** a component that must still grow (size below its target or
    ///   `curr_min_area`) and has exactly one Unknown growth edge has no other
    ///   way to reach its size → that edge must be Uncut.
    /// * **D2** view components as nodes and Unknown edges between distinct
    ///   components as edges.  Every connected component of this graph must
    ///   become at least one piece, so `cc > pieces` is a contradiction; when
    ///   `cc == pieces` each graph component is exactly one piece and all its
    ///   internal Unknown edges must be Uncut.
    ///
    /// D3 (bridge analysis) is deliberately not ported: it needs a finer
    /// argument about the two sides of each bridge and is the riskiest of the
    /// three.
    pub(crate) fn propagate_dual_connectivity(&mut self, num_comp: usize) -> Result<bool, ()> {
        let exact = self.structural_pieces;
        let bound = self.structural_pieces_max;
        if exact.is_none() && bound.is_none() {
            return Ok(false);
        }
        if exact.map_or(false, |p| p < 2) {
            return Ok(false);
        }
        let mut progress = false;

        // D0: `num_comp` is an upper bound on the final region count.
        if let Some(frozen) = self.propagate_piece_count_bound(num_comp, exact)? {
            return Ok(frozen);
        }

        // D1: single growth edge on a component that still needs to grow.
        let mut to_uncut: Vec<EdgeId> = Vec::new();
        for ci in 0..num_comp {
            let sz = self.curr_comp_sz[ci];
            let must_grow = match self.curr_target_area[ci] {
                Some(t) => sz < t,
                None => sz < self.prop.curr_min_area[ci],
            };
            if !must_grow {
                continue;
            }
            let mut unknown: Option<EdgeId> = None;
            let mut count = 0usize;
            for &e in &self.prop.growth_edges[ci] {
                if self.edges[e] == EdgeState::Unknown {
                    count += 1;
                    if count > 1 {
                        break;
                    }
                    unknown = Some(e);
                }
            }
            if count == 1 {
                to_uncut.push(unknown.unwrap());
            }
        }
        for e in to_uncut {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Uncut) {
                    return Err(());
                }
                progress = true;
            }
        }
        if progress {
            // Uncut merges components; the fixed-point loop rebuilds them.
            return Ok(true);
        }

        // D2: connected-component count of the component graph.
        // Union-Find over components, joined by Unknown edges.
        let mut parent: Vec<usize> = (0..num_comp).collect();
        fn find(parent: &mut Vec<usize>, x: usize) -> usize {
            let mut r = x;
            while parent[r] != r {
                r = parent[r];
            }
            let mut cur = x;
            while parent[cur] != r {
                let next = parent[cur];
                parent[cur] = r;
                cur = next;
            }
            r
        }
        let mut cross_edges: Vec<EdgeId> = Vec::new();
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let (ci1, ci2) = (self.curr_comp_id[c1], self.curr_comp_id[c2]);
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            cross_edges.push(e);
            let (r1, r2) = (find(&mut parent, ci1), find(&mut parent, ci2));
            if r1 != r2 {
                parent[r1] = r2;
            }
        }
        let mut cc = 0usize;
        for ci in 0..num_comp {
            if find(&mut parent, ci) == ci {
                cc += 1;
            }
        }
        // `cc > pieces` is a contradiction whether `pieces` is exact or only
        // an upper bound — every graph component needs at least one piece.
        let cap = exact.or(bound).unwrap();
        if cc > cap {
            return Err(());
        }
        // Forcing Uncut needs the count to be exact: with only an upper bound
        // a component could still split further.
        if exact == Some(cc) {
            for e in cross_edges {
                if self.edges[e] == EdgeState::Unknown {
                    if !self.set_edge(e, EdgeState::Uncut) {
                        return Err(());
                    }
                    progress = true;
                }
            }
        }
        Ok(progress)
    }

    /// Arc-consistency narrowing of the components' `[min, max]` area bounds
    /// through the inequality chain (helper of `propagate_inequality_clues`).
    ///
    /// Skipped during failed-literal probing: the fixpoint loop is O(pairs)
    /// per iteration and probing calls propagate once per Unknown edge — on
    /// 1131 (9×9 area+inequality+difference) that overhead alone pushed
    /// edge_csp from a 2.5s solve past the 40s deadline.  Probe results are
    /// only used for contradiction detection, which the caller's check loop
    /// already covers.
    fn narrow_inequality_bounds(&mut self, pairs: &[(usize, usize)]) -> Result<bool, ()> {
        if self.prop.curr_min_area.is_empty() || self.in_probing || pairs.is_empty() {
            return Ok(false);
        }
        let mut progress = false;
        let mut changed = true;
        while changed {
            changed = false;
            for &(smaller_ci, larger_ci) in pairs {
                let new_max_s = self.prop.curr_max_area[larger_ci].saturating_sub(1);
                if self.prop.curr_max_area[smaller_ci] > new_max_s {
                    self.prop.curr_max_area[smaller_ci] = new_max_s;
                    changed = true;
                }
                let new_min_l = self.prop.curr_min_area[smaller_ci].saturating_add(1);
                if self.prop.curr_min_area[larger_ci] < new_min_l {
                    self.prop.curr_min_area[larger_ci] = new_min_l;
                    changed = true;
                }
            }
            for &(smaller_ci, larger_ci) in pairs {
                for ci in [smaller_ci, larger_ci] {
                    if self.prop.curr_min_area[ci] > self.prop.curr_max_area[ci]
                        || self.curr_comp_sz[ci] > self.prop.curr_max_area[ci]
                    {
                        return Err(());
                    }
                    // Narrowed to a point → pin as target (enables sealing).
                    if self.prop.curr_min_area[ci] == self.prop.curr_max_area[ci]
                        && self.curr_target_area[ci].is_none()
                    {
                        self.curr_target_area[ci] = Some(self.prop.curr_min_area[ci]);
                        progress = true;
                    }
                }
            }
        }
        Ok(progress)
    }

    /// Inequality edge clues: verify / prune area ordering, plus arc-consistency
    /// narrowing of the components' `[min, max]` area bounds through the
    /// inequality chain (port of `third_party/aog/src/solver/pieces.rs:337-377`,
    /// which the pieces/DXL path uses but edge_csp previously lacked).
    ///
    /// For an adjacent pair with `area(small) < area(large)`: the final regions
    /// are distinct (the clue edge is always Cut), so
    /// `max[small] <= max[large] - 1` and `min[large] >= min[small] + 1`.
    /// Iterating to a fixpoint propagates along whole chains (e.g. the size
    /// ordering on 0152's 8 regions).  When narrowing makes `min == max` the
    /// bound becomes a target, unlocking the sealing paths downstream.
    /// The narrowing is skipped during failed-literal probing (one fixpoint
    /// per probe × up to 256 probes is pure overhead — it pushed 1131 from a
    /// 2.5s solve past its deadline); the contradiction checks below still run.
    fn propagate_inequality_clues(&mut self, num_comp: usize) -> Result<bool, ()> {
        let ineq_clues: Vec<(EdgeId, bool)> = self
            .edge_clues
            .iter()
            .filter_map(|cl| match cl.kind {
                EdgeClueKind::Inequality { smaller_first } => Some((cl.edge, smaller_first)),
                _ => None,
            })
            .collect();

        // Component-level ordered pairs across Cut clue edges.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for &(e, smaller_first) in &ineq_clues {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            pairs.push(if smaller_first { (ci1, ci2) } else { (ci2, ci1) });
        }

        let progress = self.narrow_inequality_bounds(&pairs)?;

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
        Ok(progress)
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

        if compass_zero_conflict(ra, rb, cola, colb, pa, pb) {
            return true;
        }
        if compass_dir_conflict(rb < ra, ra < rb, pa.n, pb.n) {
            return true;
        }
        if compass_dir_conflict(rb > ra, ra > rb, pa.s, pb.s) {
            return true;
        }
        if compass_dir_conflict(colb > cola, cola > colb, pa.e, pb.e) {
            return true;
        }
        if compass_dir_conflict(colb < cola, cola < colb, pa.w, pb.w) {
            return true;
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

        let clue_indices = self.prop.compass_clue_indices.clone();
        for &cl_idx in &clue_indices {
            let CellClue::Compass { cell, compass } = &self.cell_clues[cl_idx] else {
                continue;
            };
            let ci = self.curr_comp_id[*cell];
            if ci == usize::MAX || ci >= num_comp {
                continue;
            }
            let compass = *compass;
            self.propagate_compass_clue(*cell, ci, compass, &mut cut_ef, &mut uncut_ef)?;
        }

        // Pair-wise compass compatibility + bounding-box pruning.
        self.propagate_compass_pairwise()?;
        progress |= self.propagate_compass_bbox(num_comp)?;

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

    /// Per-compass-clue directional count + limit propagation.  Pushes `Cut`
    /// (direction at limit) and `Uncut` (single remaining growth edge) candidates
    /// into `cut_ef` / `uncut_ef`.  Returns `Err(())` on an unsatisfied or
    /// contradictory count.
    fn propagate_compass_clue(
        &mut self,
        cell: CellId,
        ci: usize,
        compass: CompassData,
        cut_ef: &mut Vec<EdgeId>,
        uncut_ef: &mut Vec<EdgeId>,
    ) -> Result<(), ()> {
        let (cr, cc) = self.grid.cell_pos(cell);

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
            let Some(v) = compass_vals[idx] else {
                continue;
            };
            if counts[idx] > v {
                return Err(());
            }
            if counts[idx] == v {
                self.compass_collect_dir_cut_edges(ci, cr, cc, idx, cut_ef);
            }
            if counts[idx] < v {
                self.compass_maybe_force_uncut(
                    ci,
                    idx,
                    &counts,
                    &dir_count,
                    &dir_last,
                    &compass_vals,
                    uncut_ef,
                );
            }
            // Sealed component with unsatisfied compass constraint.
            if self.is_sealed(ci) && counts[idx] < v {
                return Err(());
            }
        }
        Ok(())
    }

    /// Push every growth edge of `ci` lying in compass direction `idx` into
    /// `cut_ef` (the direction is already at its required count).
    fn compass_collect_dir_cut_edges(
        &mut self,
        ci: usize,
        cr: usize,
        cc: usize,
        idx: usize,
        cut_ef: &mut Vec<EdgeId>,
    ) {
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

    /// If `ci` is growing, direction `idx` has exactly one growth edge, and every
    /// other direction is already blocked, force that edge `Uncut`.
    fn compass_maybe_force_uncut(
        &mut self,
        ci: usize,
        idx: usize,
        counts: &[usize; 4],
        dir_count: &[usize; 4],
        dir_last: &[usize; 4],
        compass_vals: &[Option<usize>; 4],
        uncut_ef: &mut Vec<EdgeId>,
    ) {
        if !self.is_growing(ci) || dir_count[idx] != 1 {
            return;
        }
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

    /// Pair-wise compass compatibility: two cells in the same component whose
    /// compass readings forbid coexistence are a contradiction.
    fn propagate_compass_pairwise(&mut self) -> Result<(), ()> {
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
                let CellClue::Compass { cell: cb, compass: pb } = &self.cell_clues[cci[jj]] else {
                    continue;
                };
                if self.curr_comp_id[*cb] == ci_a && self.compass_cells_incompatible(*ca, pa, *cb, pb) {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    /// Bounding-box pruning: each compass clue constrains the reachable region of
    /// its component; growth edges reaching outside the box are forced `Cut`.
    fn propagate_compass_bbox(&mut self, num_comp: usize) -> Result<bool, ()> {
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

        let mut progress = false;
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
            let unsatisfied = self.collect_unsatisfied_dirs(ci, compass_per_comp);
            if unsatisfied.is_empty() {
                continue;
            }

            // Build reachable subgraph from CI via non-Cut edges (BFS).
            let (local_id, local_cells) = self.build_local_subgraph(ci);
            let n_local = local_cells.len();
            if n_local <= 1 {
                continue;
            }
            self.check_dir_reachability(&unsatisfied, &local_cells)?;
            let adj = self.build_subgraph_adj(&local_cells, &local_id, n_local);

            // Tarjan bridge detection.
            let bridges = Self::find_bridges_in_subgraph(&adj, n_local);

            // Force Uncut on Unknown bridges that separate CI cells from cells
            // needed for an unsatisfied direction.
            //
            // H4 (soundness): the old code forced Uncut on EVERY bridge that,
            // individually, leaves some direction's required cells unreachable
            // (`ci_side_count < v && other_side_count > 0`).  When ≥2 bridges
            // share a direction — each individually insufficient, but their
            // union overflows v — forcing ALL of them Uncut keeps every dir-cell
            // connected, pushing the direction's count past v and triggering a
            // contradiction (Err), which loses valid solutions ("cut only A" /
            // "cut only B").  Mirror the single-gateway guard (~line 1448
            // `len()==1`) and the quota guard (~line 1017 `dir_count==1 &&
            // all_others_blocked`): only force Uncut when the bridge is the SOLE
            // reachable bridge for that direction.  We compute, per direction,
            // how many bridges would isolate dir-cells; a bridge is forced only
            // if that count is exactly 1.
            let infos = self.collect_bridge_infos(&bridges, ci, &adj, &local_cells, &unsatisfied);
            self.apply_bridge_forcing(&infos, &unsatisfied, compass_uncut_ef);

            // Single-gateway-edge forcing (skip if pending forced cuts — the
            // reachable subgraph would be stale).
            if !compass_cut_ef.is_empty() {
                continue;
            }

            // Fresh CI membership via current Uncut edges + single-gateway forcing.
            self.force_compass_gateways(ci, &local_cells, &adj, &unsatisfied, n_local, compass_uncut_ef);
        }
        Ok(())
    }

    /// Collect unsatisfied compass directions for component `ci`: each entry is
    /// `(dir_idx, required_count, compass_row, compass_col)`.
    fn collect_unsatisfied_dirs(
        &self,
        ci: usize,
        compass_per_comp: &[Vec<(CellId, CompassData)>],
    ) -> Vec<(usize, usize, isize, isize)> {
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
                let Some(v) = val else {
                    continue;
                };
                if counts[idx] < v {
                    unsatisfied.push((idx, v, cri, cci));
                }
            }
        }
        unsatisfied
    }

    /// BFS the reachable subgraph of `ci` over non-Cut edges, returning a local
    /// cell id map and the ordered local cells.
    fn build_local_subgraph(&self, ci: usize) -> (Vec<usize>, Vec<CellId>) {
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
        (local_id, local_cells)
    }

    /// Contradiction check: every unsatisfied direction must still have at least
    /// `v` reachable cells in its direction within the subgraph.
    fn check_dir_reachability(
        &self,
        unsatisfied: &[(usize, usize, isize, isize)],
        local_cells: &[CellId],
    ) -> Result<(), ()> {
        for &(dir_idx, v, cri, cci) in unsatisfied {
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
        Ok(())
    }

    /// Build local-subgraph adjacency (each non-Cut internal edge → `(lj, eid)`).
    fn build_subgraph_adj(
        &self,
        local_cells: &[CellId],
        local_id: &[usize],
        n_local: usize,
    ) -> Vec<Vec<(usize, EdgeId)>> {
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
        adj
    }

    /// For each Unknown bridge, compute per-unsatisfied-direction reachability
    /// info (`other` side count + whether the CI side is below `v`).
    fn collect_bridge_infos(
        &self,
        bridges: &[EdgeId],
        ci: usize,
        adj: &[Vec<(usize, EdgeId)>],
        local_cells: &[CellId],
        unsatisfied: &[(usize, usize, isize, isize)],
    ) -> Vec<BridgeInfo> {
        let mut infos: Vec<BridgeInfo> = Vec::new();
        for &bridge_eid in bridges {
            if self.edges[bridge_eid] != EdgeState::Unknown {
                continue;
            }
            let mut ci_side = vec![false; local_cells.len()];
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

            let mut other = vec![0usize; unsatisfied.len()];
            let mut ciside_lt = vec![false; unsatisfied.len()];
            for (di, &(dir_idx, v, cri, cci)) in unsatisfied.iter().enumerate() {
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
                other[di] = other_side_count;
                ciside_lt[di] = ci_side_count < v;
            }
            infos.push(BridgeInfo {
                eid: bridge_eid,
                other,
                ciside_lt,
            });
        }
        infos
    }

    /// H4 (soundness): force Uncut only on bridges that are the SOLE isolating
    /// bridge for some unsatisfied direction (`isolating_count == 1`).
    fn apply_bridge_forcing(
        &self,
        infos: &[BridgeInfo],
        unsatisfied: &[(usize, usize, isize, isize)],
        compass_uncut_ef: &mut Vec<EdgeId>,
    ) {
        let mut isolating_count = vec![0usize; unsatisfied.len()];
        for info in infos {
            for di in 0..unsatisfied.len() {
                if info.other[di] > 0 && info.ciside_lt[di] {
                    isolating_count[di] += 1;
                }
            }
        }
        for info in infos {
            let mut force_uncut = false;
            for di in 0..unsatisfied.len() {
                if info.other[di] > 0 && info.ciside_lt[di] && isolating_count[di] == 1 {
                    force_uncut = true;
                    break;
                }
            }
            if force_uncut {
                compass_uncut_ef.push(info.eid);
            }
        }
    }

    /// Single-gateway-edge forcing: a growing component with one open gateway
    /// edge to a needed direction is forced Uncut.
    fn force_compass_gateways(
        &self,
        ci: usize,
        local_cells: &[CellId],
        adj: &[Vec<(usize, EdgeId)>],
        unsatisfied: &[(usize, usize, isize, isize)],
        n_local: usize,
        compass_uncut_ef: &mut Vec<EdgeId>,
    ) {
        let is_fresh_ci = self.compute_fresh_ci(ci, local_cells, adj, n_local);
        for &(dir_idx, _v, cri, cci) in unsatisfied {
            let gateway_edges = self.gateway_edges_for_dir(dir_idx, cri, cci, local_cells, adj, &is_fresh_ci);
            if gateway_edges.len() == 1 {
                compass_uncut_ef.push(gateway_edges[0]);
            }
        }
    }

    /// Fresh CI membership from the current Uncut edges (BFS over `adj`).
    fn compute_fresh_ci(
        &self,
        ci: usize,
        local_cells: &[CellId],
        adj: &[Vec<(usize, EdgeId)>],
        n_local: usize,
    ) -> Vec<bool> {
        let mut is_fresh_ci = vec![false; n_local];
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
        is_fresh_ci
    }

    /// Backward BFS from non-CI dir-cells; the Unknown edges from CI into the
    /// reachable set are this direction's gateway edges.
    fn gateway_edges_for_dir(
        &self,
        dir_idx: usize,
        cri: isize,
        cci: isize,
        local_cells: &[CellId],
        adj: &[Vec<(usize, EdgeId)>],
        is_fresh_ci: &[bool],
    ) -> Vec<EdgeId> {
        let n_local = local_cells.len();
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
            return Vec::new();
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
        gateway_edges
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

    /// Fence (palisade) **star-domain AC**.  Each fence cell's placements are
    /// the rotations of its `PalisadeKind` (mask bits N,S,W,E == `cell_edges`
    /// order), filtered by the currently-decided incident edges.  Two adjacent
    /// fence stars constrain the edge they share, so their placement domains
    /// narrow each other (binary AC on shared edges), and any edge on which
    /// all remaining placements of a star agree is forced.  The single-star
    /// intersection alone can never force anything for 2/3-arm kinds (no mask
    /// bit is universal across their rotations) — the shared-edge consensus is
    /// what collapses dense fence boards (1249).
    pub(crate) fn propagate_palisade_constraints(&mut self) -> Result<bool, ()> {
        let mut stars: Vec<(PalisadeKind, [Option<EdgeId>; 4])> = Vec::new();
        let mut star_of: Vec<Option<usize>> = vec![None; self.grid.num_cells()];
        for cl in &self.cell_clues {
            if let CellClue::Palisade { cell, kind } = cl {
                let cell = *cell;
                if !self.grid.cell_exists[cell] {
                    continue;
                }
                star_of[cell] = Some(stars.len());
                stars.push((*kind, self.grid.cell_edges(cell)));
            }
        }
        if stars.is_empty() {
            return Ok(false);
        }
        // Partner lookup: shared edge → (other star, other side).
        let mut partner: Vec<[Option<(usize, usize)>; 4]> = vec![[None; 4]; stars.len()];
        for si in 0..stars.len() {
            for k in 0..4 {
                let Some(eid) = stars[si].1[k] else { continue };
                let (a, b) = self.grid.edge_cells(eid);
                let nb = if star_of[a] == Some(si) { b } else { a };
                let Some(ti) = star_of[nb] else { continue };
                for tk in 0..4 {
                    if stars[ti].1[tk] == Some(eid) {
                        partner[si][k] = Some((ti, tk));
                    }
                }
            }
        }
        let mut doms: Vec<Vec<u8>> = stars
            .iter()
            .map(|&(kind, ref edges)| {
                let states = self.star_states(edges);
                star_placements(kind, states)
            })
            .collect();
        let mut changed = true;
        let mut progress = false;
        while changed {
            changed = false;
            for si in 0..stars.len() {
                let states = self.star_states(&stars[si].1);
                let before = doms[si].len();
                doms[si].retain(|&m| star_mask_ok(m, states));
                if doms[si].is_empty() {
                    return Err(());
                }
                if doms[si].len() != before {
                    changed = true;
                }
            }
            changed |= self.star_consensus(&partner, &mut doms)?;
            let p = self.star_force(&stars, &mut doms)?;
            progress |= p;
            changed |= p;
        }
        Ok(progress)
    }

    /// Binary AC on shared edges: two stars constrain the edge they share, so
    /// each star's placement domain narrows to the values its partner can
    /// mirror.  Returns whether any domain shrank.
    fn star_consensus(
        &self,
        partner: &[[Option<(usize, usize)>; 4]],
        doms: &mut [Vec<u8>],
    ) -> Result<bool, ()> {
        let mut changed = false;
        for si in 0..doms.len() {
            for k in 0..4 {
                let Some((ti, tk)) = partner[si][k] else {
                    continue;
                };
                let mut p0 = 0u8;
                let mut p1 = 0u8;
                for &m in &doms[si] {
                    p0 |= 1 << ((m >> k) & 1);
                }
                for &m in &doms[ti] {
                    p1 |= 1 << ((m >> tk) & 1);
                }
                let inter = p0 & p1;
                if inter == 0 {
                    return Err(());
                }
                let (b0, b1) = (doms[si].len(), doms[ti].len());
                doms[si].retain(|&m| inter & (1 << ((m >> k) & 1)) != 0);
                doms[ti].retain(|&m| inter & (1 << ((m >> tk) & 1)) != 0);
                if doms[si].is_empty() || doms[ti].is_empty() {
                    return Err(());
                }
                if doms[si].len() != b0 || doms[ti].len() != b1 {
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    /// Force every edge on which a star's remaining placements all agree.
    /// Returns whether any edge was set.
    fn star_force(
        &mut self,
        stars: &[(PalisadeKind, [Option<EdgeId>; 4])],
        doms: &mut [Vec<u8>],
    ) -> Result<bool, ()> {
        let mut progress = false;
        for si in 0..stars.len() {
            for k in 0..4 {
                let Some(eid) = stars[si].1[k] else { continue };
                if self.edges[eid] != EdgeState::Unknown {
                    continue;
                }
                let mut bits = 0u8;
                for &m in &doms[si] {
                    bits |= 1 << ((m >> k) & 1);
                }
                let target = match bits {
                    0b01 => Some(EdgeState::Uncut),
                    0b10 => Some(EdgeState::Cut),
                    _ => None,
                };
                if let Some(t) = target {
                    if !self.set_edge(eid, t) {
                        return Err(());
                    }
                    progress = true;
                }
            }
        }
        Ok(progress)
    }

    fn star_states(&self, edges: &[Option<EdgeId>; 4]) -> [EdgeState; 4] {
        edges.map(|e| e.map(|eid| self.edges[eid]).unwrap_or(EdgeState::Cut))
    }

    /// m=2 vertex-parity unit propagation.  With exactly two regions the four
    /// cells around any interior vertex are 2-coloured, and a cyclic binary
    /// sequence has an even number of transitions — so the Cut-spoke count at
    /// every interior vertex (4 fillable cells) is **even**.  3 decided spokes
    /// therefore force the 4th; a fully decided odd vertex is a contradiction.
    /// Fence-star domains never couple across a vertex; this curve-pairing
    /// half is what closes the net on fence-dense boards (1249).  Sound only
    /// for `exact_piece_count == Some(2)` (≥3 colours allow odd transitions).
    pub(crate) fn propagate_two_piece_vertex_parity(&mut self) -> Result<bool, ()> {
        let mut progress = false;
        for vr in 1..self.grid.rows {
            for vc in 1..self.grid.cols {
                let cells = [
                    self.grid.cell_id(vr - 1, vc - 1),
                    self.grid.cell_id(vr - 1, vc),
                    self.grid.cell_id(vr, vc - 1),
                    self.grid.cell_id(vr, vc),
                ];
                if cells.iter().any(|&c| !self.grid.cell_exists[c]) {
                    continue;
                }
                // Cycle c0→c1→c3→c2→c0: top, right, bottom, left spokes.
                let spokes = [
                    self.grid.edge_between(cells[0], cells[1]),
                    self.grid.edge_between(cells[1], cells[3]),
                    self.grid.edge_between(cells[2], cells[3]),
                    self.grid.edge_between(cells[0], cells[2]),
                ];
                let mut cut = 0usize;
                let mut unk: Vec<EdgeId> = Vec::new();
                let mut any_missing = false;
                for e in spokes {
                    let Some(eid) = e else {
                        any_missing = true;
                        break;
                    };
                    match self.edges[eid] {
                        EdgeState::Cut => cut += 1,
                        EdgeState::Uncut => {}
                        EdgeState::Unknown => unk.push(eid),
                    }
                }
                if any_missing {
                    continue;
                }
                match unk.len() {
                    0 => {
                        if cut % 2 == 1 {
                            return Err(());
                        }
                    }
                    1 => {
                        let want = if cut % 2 == 1 {
                            EdgeState::Cut
                        } else {
                            EdgeState::Uncut
                        };
                        if !self.set_edge(unk[0], want) {
                            return Err(());
                        }
                        progress = true;
                    }
                    _ => {}
                }
            }
        }
        Ok(progress)
    }

    /// Failed-literal detection: probe each unknown edge; if one value
    /// contradicts, force the other.  Returns early on first force.
    fn probe_one_round(&mut self) -> Result<bool, ()> {
        let num_edges = self.grid.num_edges();
        // Diagnostic: `EDGE_CSP_PROBE_ORDER=rev|inter` reshuffles the scan so
        // the early-return lands on informative edges sooner (each round
        // restarts from scratch, so index order is the de-facto priority).
        let order: Vec<usize> = match std::env::var("EDGE_CSP_PROBE_ORDER")
            .unwrap_or_default()
            .as_str()
        {
            "rev" => (0..num_edges).rev().collect(),
            "inter" => {
                let mut v = Vec::with_capacity(num_edges);
                let (mut lo, mut hi) = (0usize, num_edges);
                while lo < hi {
                    v.push(lo);
                    lo += 1;
                    if lo < hi {
                        hi -= 1;
                        v.push(hi);
                    }
                }
                v
            }
            _ => (0..num_edges).collect(),
        };
        for e in order {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let cut_ok = self.probe(|s| s.set_edge(e, EdgeState::Cut));
            // A probe that died on the deadline is not a failed literal —
            // forcing the opposite value from it would prune the real branch.
            if self.timed_out {
                return Ok(false);
            }
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
            if self.timed_out {
                return Ok(false);
            }
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
                            if self.timed_out {
                                // Deadline kills must not be read as "combination
                                // invalid" — that would force a wrong pair.
                                return Ok(false);
                            }
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
    /// Exact cut-degree propagation at interior watchtower vertices with all
    /// four quadrants fillable.  The four cyclic quadrant pairs make the
    /// cut-degree the number of label transitions in a 4-cycle, so the
    /// distinct-region count `val` bounds it tightly:
    ///   val=1 → degree 0;  val=2 → degree ∈ {2, 4};  val=3 → {3, 4};  val=4 → 4.
    /// Under a single-interface world (`max_loops == 1`, 2 pieces) a degree-4
    /// crossing is a self-touching figure-8 (two cycles) — across every m=2
    /// official answer all such vertices have degree exactly 2 (634/634), so
    /// val=2 collapses to a Slitherlink exact-2 clue.  Blocked quadrants break
    /// the cycle argument (odd degrees appear) and are skipped.
    pub(crate) fn propagate_watchtower_degree(&mut self) -> Result<bool, ()> {
        let pieces = self.exact_piece_count.or(self.structural_pieces);
        let max_loops = pieces.map(|p| p.saturating_sub(1));
        let clues: Vec<(VertexId, usize)> = self
            .vertex_clues
            .iter()
            .map(|cl| (cl.vertex, cl.value as usize))
            .collect();
        if let Some(p) = pieces {
            for &(_, v) in &clues {
                if v > p {
                    return Err(());
                }
            }
        }
        let (h, w) = (self.grid.rows, self.grid.cols);
        let mut progress = false;
        for &(vtx, val) in &clues {
            if !(1..=4).contains(&val) {
                continue;
            }
            let (i, j) = self.grid.vertex_pos(vtx);
            if i == 0 || i == h || j == 0 || j == w {
                continue;
            }
            let quads = [
                (i - 1, j - 1),
                (i - 1, j),
                (i, j),
                (i, j - 1),
            ];
            if quads.iter().any(|&(r, c)| !self.grid.cell_exists[r * w + c]) {
                continue;
            }
            let pairs = [
                (quads[0], quads[1]),
                (quads[1], quads[2]),
                (quads[2], quads[3]),
                (quads[3], quads[0]),
            ];
            let mut sides = [usize::MAX; 4];
            for (k, (a, b)) in pairs.iter().enumerate() {
                let ea = self.grid.cell_id(a.0, a.1);
                let eb = self.grid.cell_id(b.0, b.1);
                let Some(e) = self.grid.edge_between(ea, eb) else {
                    continue;
                };
                sides[k] = e;
            }
            let mut known_cut = 0usize;
            let mut unknown: Vec<EdgeId> = Vec::new();
            for &e in &sides {
                match self.edges[e] {
                    EdgeState::Cut => known_cut += 1,
                    EdgeState::Unknown => unknown.push(e),
                    EdgeState::Uncut => {}
                }
            }
            let candidates: Vec<usize> = match val {
                1 => vec![0],
                2 => {
                    if max_loops == Some(1) {
                        vec![2]
                    } else {
                        vec![2, 4]
                    }
                }
                3 => {
                    if self.rules.loopy {
                        vec![4]
                    } else {
                        vec![3, 4]
                    }
                }
                _ => vec![4],
            };
            let x = unknown.len();
            let fits: Vec<usize> = candidates
                .iter()
                .copied()
                .filter(|&d| d >= known_cut && d <= known_cut + x)
                .collect();
            if fits.is_empty() {
                return Err(());
            }
            let lo = *fits.iter().min().unwrap();
            let hi = *fits.iter().max().unwrap();
            // Singleton-fit pinning.  The rule is sound (doc 27 追记: 1568
            // official vertices with 0 counterexamples; 24/24 root forces
            // correct) but used to kill 1135/1392/1137 — the killer was the
            // stale component cache feeding `rose_separation`'s chokepoint BFS
            // (and Pass A), not this rule.  With the consumer-entry rebuilds
            // the forces are safe and sharpen the search (1137: 29s → 24s).
            if hi == known_cut {
                for &e in &unknown {
                    if self.set_edge(e, EdgeState::Uncut) {
                        progress = true;
                    }
                }
            } else if lo == known_cut + x {
                for &e in &unknown {
                    if self.set_edge(e, EdgeState::Cut) {
                        progress = true;
                    }
                }
            }
        }
        Ok(progress)
    }

    pub(crate) fn propagate_watchtower(&mut self) -> Result<bool, ()> {
        if self.vertex_clues.is_empty() {
            return Ok(false);
        }
        let mut progress = false;

        // Stale-component guard: `build_components` writes growth-edge cuts
        // after the flood and `propagate_area_constraints` writes mid-round —
        // Pass A's `max_distinct == comp_count` force is unsound on the stale
        // (under-split) grouping.  Rebuild here so Pass A sees the current
        // edge state (same treatment dual connectivity got via deferral).
        self.build_components()?;
        let pa = self.watchtower_pass_a()?;
        progress |= pa;
        let pb = self.watchtower_pass_b()?;
        progress |= pb;

        Ok(progress)
    }
    /// Pass A of `propagate_watchtower`: component-ID-based distinct-region
    /// counting. Extracted verbatim from `propagate_watchtower` — no logic change.
    fn watchtower_pass_a(&mut self) -> Result<bool, ()> {
        let mut progress = false;
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
        Ok(progress)
    }

    /// Pass B of `propagate_watchtower`: edge-count-based propagation.
    /// Extracted verbatim from `propagate_watchtower` — no logic change.
    fn watchtower_pass_b(&mut self) -> Result<bool, ()> {
        let mut progress = false;
        let cell_pair_indices: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];
        // === Pass B: edge-count-based ===
        let constraints: Vec<(usize, usize, usize, Vec<EdgeId>, bool, usize)> = self
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
                    return Some((vi, vj, value, vec![], false, n));
                }
                if n == 1 {
                    return Some((vi, vj, value, vec![], false, n));
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
                Some((vi, vj, value, edge_ids, is_cycle, n))
            })
            .collect();

        for (_vi, _vj, value, edge_ids, is_cycle, n) in constraints {
            if edge_ids.is_empty() {
                // The cells around this vertex share no edge *here*.  They can
                // still end up in the same region via a path outside the
                // vertex, so nothing follows from the edge count — unless only
                // one cell touches the vertex (a corner), where `value > 1` is
                // impossible no matter what.  (The old code returned `Err` for
                // any `value > 1`, which wrongly killed 0496: blocked cells
                // left only diagonally-adjacent pairs around vertex (5,2).)
                if n == 1 && value > 1 {
                    return Err(());
                }
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

        // Collect vertex constraints: (edge_ids, required_parity).
        let constraints: Vec<(Vec<EdgeId>, u8)> = self.collect_vertex_parity_constraints();

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

        Self::vertex_parity_phase1(&constraints, &mut uf, &mut ev, &mut forced)?;
        Self::vertex_parity_phase2(&constraints, &mut uf, &mut ev, &mut forced)?;
        Self::vertex_parity_phase3(ne, &mut uf, &mut ev, &mut forced)?;

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

    /// Build the per-vertex edge-parity constraints: each entry is
    /// `(edge_ids, required_parity)` where `required_parity` is the parity of the
    /// number of Cut edges around the vertex.  Vertices whose parity is not yet
    /// fixed are skipped.
    fn collect_vertex_parity_constraints(&self) -> Vec<(Vec<EdgeId>, u8)> {
        let pair_idx: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];
        self.vertex_clues
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
            .collect()
    }

    /// Phase 1: build the parity UF from pairwise constraints (0, 1, or 2
    /// unknowns), fixing single unknowns and detecting contradictions.
    fn vertex_parity_phase1(
        constraints: &[(Vec<EdgeId>, u8)],
        uf: &mut ParityUF,
        ev: &mut [u8],
        forced: &mut Vec<(EdgeId, EdgeState)>,
    ) -> Result<(), ()> {
        for (edge_ids, parity) in constraints {
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
                    forced.push((unks[0], if v == 1 { EdgeState::Cut } else { EdgeState::Uncut }));
                }
                2 => {
                    uf.union(unks[0], unks[1], kx ^ parity)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Phase 2: resolve 3+-unknown constraints using UF pairs already merged.
    fn vertex_parity_phase2(
        constraints: &[(Vec<EdgeId>, u8)],
        uf: &mut ParityUF,
        ev: &mut [u8],
        forced: &mut Vec<(EdgeId, EdgeState)>,
    ) -> Result<(), ()> {
        for (edge_ids, parity) in constraints {
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
                            forced.push((rem[0], if v == 1 { EdgeState::Cut } else { EdgeState::Uncut }));
                        } else if rem.len() == 2 {
                            uf.union(rem[0], rem[1], target ^ xij)?;
                        }
                        break 'outer;
                    }
                }
            }
        }
        Ok(())
    }

    /// Phase 3: cascade known edge values through the UF to resolve unknowns.
    fn vertex_parity_phase3(
        ne: usize,
        uf: &mut ParityUF,
        ev: &mut [u8],
        forced: &mut Vec<(EdgeId, EdgeState)>,
    ) -> Result<(), ()> {
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
        Ok(())
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
                let (edge_cut_count, total_surviving) = self.enumerate_watchtower_configs(
                    possible_ks,
                    n_cut,
                    n_unk,
                    &unk_indices,
                    edge_ids,
                );

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

    /// Enumerate all valid Cut/Uncut configs of a vertex's unknown edges that
    /// satisfy `possible_ks`; returns per-unknown Cut-count tallies and the
    /// number of surviving configs.
    fn enumerate_watchtower_configs(
        &mut self,
        possible_ks: &[usize],
        n_cut: usize,
        n_unk: usize,
        unk_indices: &[usize],
        edge_ids: &[EdgeId],
    ) -> (Vec<usize>, usize) {
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
        (edge_cut_count, total_surviving)
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
    /// Tightest bounding box implied by a compass clue's [N, S, E, W] limits.
    fn compass_bbox(&self, cell: CellId, compass: CompassData) -> (isize, isize, isize, isize) {
        let (cr, cc) = self.grid.cell_pos(cell);
        let (cri, cci) = (cr as isize, cc as isize);
        let limits = [compass.n, compass.s, compass.e, compass.w];
        let bbox_min_r = limits[0].map_or(0isize, |v| cri - v as isize).max(0);
        let bbox_max_r = limits[1]
            .map_or(self.grid.rows as isize - 1, |v| cri + v as isize)
            .min(self.grid.rows as isize - 1);
        let bbox_min_c = limits[3].map_or(0isize, |v| cci - v as isize).max(0);
        let bbox_max_c = limits[2]
            .map_or(self.grid.cols as isize - 1, |v| cci + v as isize)
            .min(self.grid.cols as isize - 1);
        (bbox_min_r, bbox_max_r, bbox_min_c, bbox_max_c)
    }

    /// Number of fillable cells inside `bbox`.
    fn count_bbox_cells(&self, bbox: (isize, isize, isize, isize)) -> usize {
        let (r0, r1, c0, c1) = bbox;
        let mut n = 0usize;
        for r in r0.max(0)..=r1.min(self.grid.rows as isize - 1) {
            for c in c0.max(0)..=c1.min(self.grid.cols as isize - 1) {
                if self.grid.cell_exists[self.grid.cell_id(r as usize, c as usize)] {
                    n += 1;
                }
            }
        }
        n
    }

    /// Step 1 of compass placement enumeration: BFS from the compass cell over
    /// non-Cut edges, restricted to `bbox`.
    fn compass_reachable_bfs(
        &self,
        cell: CellId,
        bbox: (isize, isize, isize, isize),
    ) -> (Vec<bool>, Vec<CellId>) {
        let (bbox_min_r, bbox_max_r, bbox_min_c, bbox_max_c) = bbox;
        let n = self.grid.num_cells();
        let mut cell_in_reachable = vec![false; n];
        let mut reachable_cells: Vec<CellId> = Vec::new();
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
        (cell_in_reachable, reachable_cells)
    }

    /// Step 2: group the reachable cells into local components joined by current
    /// Uncut edges (followed GLOBALLY, beyond the bbox, so a cell committed to
    /// an outside piece drags that piece in).  `None` when the component budget
    /// is exhausted.
    fn compass_local_comps(
        &self,
        reachable_cells: &[CellId],
        max_comps: usize,
    ) -> Option<(Vec<usize>, Vec<Vec<CellId>>)> {
        let n = self.grid.num_cells();
        let mut local_comp_of = vec![usize::MAX; n];
        let mut local_comps: Vec<Vec<CellId>> = Vec::new();
        for &start in reachable_cells {
            if local_comp_of[start] != usize::MAX {
                continue;
            }
            if local_comps.len() >= max_comps {
                return None;
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
        Some((local_comp_of, local_comps))
    }

    /// Can local component 0 still grow — into another reachable local
    /// component, or out of the bbox entirely?
    fn compass_can_grow(
        &self,
        lcomp0: &[CellId],
        cell_in_reachable: &[bool],
        local_comp_of: &[usize],
    ) -> bool {
        lcomp0.iter().any(|&c| {
            self.grid.cell_edges(c).into_iter().flatten().any(|eid| {
                if self.edges[eid] != EdgeState::Unknown {
                    return false;
                }
                let (c1, c2) = self.grid.edge_cells(eid);
                let other = if c1 == c { c2 } else { c1 };
                if cell_in_reachable[other] {
                    local_comp_of[other] != 0 && local_comp_of[other] != usize::MAX
                } else {
                    self.grid.cell_exists[other]
                }
            })
        })
    }

    /// Directional cell counts `[N, S, E, W]` and sizes per local component.
    fn compass_dir_counts(
        &self,
        local_comps: &[Vec<CellId>],
        cri: isize,
        cci: isize,
    ) -> (Vec<[usize; 4]>, Vec<usize>) {
        let mut comp_dir_counts = vec![[0usize; 4]; local_comps.len()];
        let mut comp_sizes = vec![0usize; local_comps.len()];
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
        (comp_dir_counts, comp_sizes)
    }

    /// Bitmask of local components reachable from each local component through
    /// Unknown edges.
    fn compass_adj_mask(&self, local_comps: &[Vec<CellId>], local_comp_of: &[usize]) -> Vec<u32> {
        let mut adj_mask = vec![0u32; local_comps.len()];
        for lc in 0..local_comps.len() {
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
        adj_mask
    }

    /// Force Cut/Uncut on the Unknown growth edges out of local component 0:
    /// merges present in every valid placement are forced Uncut, merges present
    /// in none are forced Cut, and anything outside the bbox is forced Cut.
    fn compass_collect_forced(
        &self,
        lcomp0: &[CellId],
        cell_in_reachable: &[bool],
        local_comp_of: &[usize],
        in_all: u32,
        in_any: u32,
        forced_cuts: &mut Vec<EdgeId>,
        forced_uncuts: &mut Vec<EdgeId>,
    ) {
        for &c in lcomp0 {
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

    /// `(min_area, max_area)` of the growing, non-sealed component containing
    /// `cell`, or `None` when the cell has no usable area bounds.
    fn compass_area_bounds(&self, cell: CellId) -> Option<(usize, usize)> {
        let ci = if cell < self.curr_comp_id.len() {
            self.curr_comp_id[cell]
        } else {
            usize::MAX
        };
        if ci == usize::MAX || self.is_sealed(ci) {
            return None;
        }
        let max_a = if ci < self.prop.curr_max_area.len() {
            self.prop.curr_max_area[ci]
        } else {
            return None;
        };
        let min_a = if ci < self.prop.curr_min_area.len() {
            self.prop.curr_min_area[ci]
        } else {
            return None;
        };
        Some((min_a, max_a))
    }

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
            let Some((min_a, max_a)) = self.compass_area_bounds(cell) else {
                continue;
            };

            let (cr, cc) = self.grid.cell_pos(cell);
            let (cri, cci) = (cr as isize, cc as isize);
            // Compass limits [N, S, E, W].
            let limits = [compass.n, compass.s, compass.e, compass.w];

            // Bounding box from compass constraints (tightest possible).
            let bbox = self.compass_bbox(cell, compass);

            // Region connectivity confines the piece to the clue's bounding
            // box, so the box's fillable-cell count is a sound upper bound —
            // usually far tighter than `curr_max_area`, which sums the four
            // half-planes and therefore double-counts the quadrants.  Gate the
            // enumeration on the tighter of the two so a `-1`-heavy clue still
            // qualifies (1017's 6x6 board: `curr_max_area` is 36, the box is
            // 8-16 cells).
            let bbox_area = self.count_bbox_cells(bbox);
            let max_a = max_a.min(bbox_area);
            if max_a > MAX_AREA_THRESHOLD {
                continue;
            }

            // Step 1: BFS reachable cells from compass cell via non-Cut edges, in bbox.
            let (cell_in_reachable, reachable_cells) = self.compass_reachable_bfs(cell, bbox);

            // Step 2: group reachable cells into fresh local components via CURRENT
            // Uncut edges. IMPORTANT: follow Uncut edges GLOBALLY (beyond bbox) so a
            // cell committed to an outside piece drags that piece in — prevents false
            // forced-uncuts. The compass cell ends up in local component 0.
            let Some((mut local_comp_of, mut local_comps)) =
                self.compass_local_comps(&reachable_cells, MAX_REACHABLE_COMPS)
            else {
                continue 'outer;
            };

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
            if !self.compass_can_grow(&local_comps[0], &cell_in_reachable, &local_comp_of) {
                continue;
            }

            // Per-component directional counts and sizes.
            let (comp_dir_counts, comp_sizes) = self.compass_dir_counts(&local_comps, cri, cci);

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
            let adj_mask = self.compass_adj_mask(&local_comps, &local_comp_of);

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
            self.compass_collect_forced(
                &local_comps[0],
                &cell_in_reachable,
                &local_comp_of,
                in_all,
                in_any,
                &mut forced_cuts,
                &mut forced_uncuts,
            );
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

    /// Size-separation (`differentiation`) propagation — port of
    /// `third_party/aog area.rs::propagate_size_separation` (370-485).
    ///
    /// Adjacent pieces must have different cell counts. Propagates by:
    /// 1. Building `sealed_neighbor_sizes[ci]` — sizes of adjacent components
    ///    whose final size is known (sealed, or growing with a fixed target).
    /// 2. For each Unknown growth edge, if merging the two components would
    ///    produce a size equal to a sealed neighbor's size → force Cut.
    /// 3. For a growing component whose current/target size is forbidden (equals
    ///    a sealed neighbor) → if exactly 1 Unknown growth edge remains, force
    ///    it Uncut (must grow away from the forbidden size); if 0 → Err.
    /// Step 1 of size separation: for every component, the set of sizes of the
    /// neighbours it may not end up equal to (a sealed neighbour's current size,
    /// or a still-growing neighbour's already-fixed target area).
    fn collect_sealed_neighbor_sizes(&self, num_comp: usize) -> Vec<BTreeSet<usize>> {
        let mut sizes: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); num_comp];
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            if self.is_sealed(ci1) {
                sizes[ci2].insert(self.curr_comp_sz[ci1]);
            } else if ci1 < self.curr_target_area.len() {
                if let Some(t) = self.curr_target_area[ci1] {
                    sizes[ci2].insert(t);
                }
            }
            if self.is_sealed(ci2) {
                sizes[ci1].insert(self.curr_comp_sz[ci2]);
            } else if ci2 < self.curr_target_area.len() {
                if let Some(t) = self.curr_target_area[ci2] {
                    sizes[ci1].insert(t);
                }
            }
        }
        sizes
    }

    /// Step 2: an Unknown edge whose merged component size is forbidden must be
    /// cut open.
    fn size_separation_merge_cuts(
        &mut self,
        num_comp: usize,
        sizes: &[BTreeSet<usize>],
    ) -> Result<bool, ()> {
        let mut cuts: Vec<EdgeId> = Vec::new();
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
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            let merged_sz = self.curr_comp_sz[ci1] + self.curr_comp_sz[ci2];
            // Sound only when the merged component cannot grow any further
            // (`merged_sz == cap`): the forbidden-size set only constrains the
            // FINAL size, and a merged component below its cap can keep
            // growing past `merged_sz` to a legal size.
            //
            // The reference aog (area.rs:429) Cuts on `contains(&merged_sz)`
            // unconditionally — unsound.  On 0926 (9×9 differentiation+area+
            // difference) at the root, component (3,2) (sz=1, target 4) and
            // (4,2) (sz=1) share e=19; merged_sz=2 equals a sealed neighbour's
            // size 2, so the reference logic Cuts e=19 — but the official
            // region 14 = {(3,2),(4,2),(4,3),(5,2)} merges them and grows to
            // its target 4.  Requiring `merged_sz == min(max1, max2)` keeps
            // the sound case (merge lands exactly on the cap → final size is
            // `merged_sz` → genuinely forbidden) and drops the unsound one.
            if ci1 >= self.prop.curr_max_area.len() || ci2 >= self.prop.curr_max_area.len() {
                continue;
            }
            let cap = self.prop.curr_max_area[ci1].min(self.prop.curr_max_area[ci2]);
            let hits_cap = merged_sz == cap
                && (sizes[ci1].contains(&merged_sz) || sizes[ci2].contains(&merged_sz));
            // A target on either side pins the merged region's final size, so
            // a forbidden target is a genuine merge conflict even below the
            // cap (this is the case the reference's `merged_sz` check was
            // approximating).
            let target_conflict = match (
                self.curr_target_area.get(ci1).copied().flatten(),
                self.curr_target_area.get(ci2).copied().flatten(),
            ) {
                (Some(t), Some(u)) if t == u => {
                    sizes[ci1].contains(&t) || sizes[ci2].contains(&t)
                }
                (Some(t), None) => sizes[ci1].contains(&t) || sizes[ci2].contains(&t),
                (None, Some(u)) => sizes[ci1].contains(&u) || sizes[ci2].contains(&u),
                _ => false,
            };
            if hits_cap || target_conflict {
                cuts.push(e);
            }
        }
        let mut progress = false;
        for e in cuts {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }

    /// `(count, last)` Unknown growth edges of component `ci`.
    fn growth_unknown_edges(&self, ci: usize) -> (usize, Option<EdgeId>) {
        let mut unk_count = 0usize;
        let mut last_unk: Option<EdgeId> = None;
        if ci < self.prop.growth_edges.len() {
            for &e in &self.prop.growth_edges[ci] {
                if self.edges[e] == EdgeState::Unknown {
                    unk_count += 1;
                    last_unk = Some(e);
                }
            }
        }
        (unk_count, last_unk)
    }

    /// Step 3: a component whose current size is forbidden must grow; with
    /// exactly one Unknown growth edge left that edge is forced Uncut.
    fn size_separation_forced_uncuts(
        &mut self,
        num_comp: usize,
        sizes: &[BTreeSet<usize>],
    ) -> Result<bool, ()> {
        let mut uncuts: Vec<EdgeId> = Vec::new();
        for ci in 0..num_comp {
            let forbidden = &sizes[ci];
            if forbidden.is_empty() {
                continue;
            }
            if self.is_sealed(ci) {
                if forbidden.contains(&self.curr_comp_sz[ci]) {
                    return Err(());
                }
                continue;
            }
            if ci < self.curr_target_area.len() {
                if let Some(t) = self.curr_target_area[ci] {
                    if forbidden.contains(&t) {
                        return Err(());
                    }
                }
            }
            if forbidden.contains(&self.curr_comp_sz[ci]) {
                // Current size forbidden → must grow.
                let (unk_count, last_unk) = self.growth_unknown_edges(ci);
                if unk_count == 0 {
                    return Err(());
                }
                if unk_count == 1 {
                    uncuts.push(last_unk.unwrap());
                }
            }
        }
        let mut progress = false;
        for e in uncuts {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Uncut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }

    fn propagate_size_separation(&mut self, num_comp: usize) -> Result<bool, ()> {
        if !self.rules.size_separation {
            return Ok(false);
        }
        let sizes = self.collect_sealed_neighbor_sizes(num_comp);
        let mut progress = self.size_separation_merge_cuts(num_comp, &sizes)?;
        progress |= self.size_separation_forced_uncuts(num_comp, &sizes)?;
        Ok(progress)
    }

    /// Final sealed-pair size-separation contradiction check — port of
    /// `third_party/aog area.rs:1244-1265`. For each Cut edge between two
    /// SEALED components of equal size → contradiction (adjacent equal-size
    /// pieces violate differentiation).
    fn check_size_separation_sealed_pairs(&self, num_comp: usize) -> Result<(), ()> {
        if !self.rules.size_separation {
            return Ok(());
        }
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            if self.is_growing(ci1) || self.is_growing(ci2) {
                continue;
            }
            if self.curr_comp_sz[ci1] == self.curr_comp_sz[ci2] {
                return Err(());
            }
        }
        Ok(())
    }

    /// Gemini (`homogeneous`) area-equality propagation.
    ///
    /// A gemini edge says the two regions it separates have the SAME shape,
    /// which implies equal area.  Enforcing the area half is the cheap, sound
    /// part of doc-21's "gemini=>尺寸相等" item; the shape-identity half (and
    /// the delta_gemini vertex interaction) is not modelled.
    ///
    /// Three checks per gemini edge:
    /// - both components finished → areas must be equal;
    /// - one finished at size `a`, the other still growing → the other has
    ///   already overshot (`> a`), or can no longer reach `a`
    ///   (`growth_potential < a`) → contradiction.
    fn check_gemini_pairs(&mut self, num_comp: usize) -> Result<(), ()> {
        if self.gemini_edges.is_empty() {
            return Ok(());
        }
        for i in 0..self.gemini_edges.len() {
            let e = self.gemini_edges[i];
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            let s1 = self.curr_comp_sz[ci1];
            let s2 = self.curr_comp_sz[ci2];
            let g1 = self.is_growing(ci1);
            let g2 = self.is_growing(ci2);

            if !g1 && !g2 {
                if s1 != s2 {
                    return Err(());
                }
                continue;
            }
            // Exactly one side still growing: it must finish at the other's size.
            if !g1 && g2 {
                if s2 > s1 || self.growth_potential(ci2) < s1 {
                    return Err(());
                }
            } else if g1 && !g2 {
                if s1 > s2 || self.growth_potential(ci1) < s2 {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    /// Shape-identity propagation for Gemini (same shape) and Delta (different
    /// shape) edge clues.  Ported from `third_party/aog/src/solver/propagation/
    /// shape.rs::propagate_shape_constraints` (the gemini/delta portions).
    ///
    /// For each sealed component we compute its canonical polyomino shape, then:
    /// - Gemini edge with both sides sealed: the two shapes must compare equal.
    /// - Delta edge with both sides sealed: the two shapes must differ.
    ///
    /// Pure contradiction check (`Err` on violation) — never forces an edge, so it
    /// is regression-safe.  It prunes search branches where two sealed regions
    /// violate their shape relation, which is what the `homogeneous` /
    /// `heterogeneous` official puzzles actually require (area equality alone is
    /// insufficient: equal area does not imply equal shape).
    /// Components on both sides of `e` when the edge is Cut and both endpoints
    /// are live, distinct, in-range components — the shared guard of every
    /// "compare the two sides of a clue edge" propagation.
    fn cut_edge_comp_pair(&self, e: usize, num_comp: usize) -> Option<(usize, usize)> {
        if self.edges[e] != EdgeState::Cut {
            return None;
        }
        let (c1, c2) = self.grid.edge_cells(e);
        if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
            return None;
        }
        let ci1 = self.curr_comp_id[c1];
        let ci2 = self.curr_comp_id[c2];
        if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
            return None;
        }
        Some((ci1, ci2))
    }

    /// Canonical shape of every sealed component that already reached its
    /// target area; components still growing (or not sealed) stay `None`.
    fn sealed_comp_shapes(&self, num_comp: usize) -> Vec<Option<Shape>> {
        let mut comp_shape: Vec<Option<Shape>> = vec![None; num_comp];
        for &ci in &self.prop.sealed_list {
            let at_limit = match self.curr_target_area[ci] {
                Some(t) => self.curr_comp_sz[ci] == t,
                None => true,
            };
            if !at_limit {
                continue;
            }
            let cells: Vec<(i32, i32)> = self.comp_cells[ci]
                .iter()
                .map(|&c| {
                    let (r, col) = self.grid.cell_pos(c);
                    (r as i32, col as i32)
                })
                .collect();
            comp_shape[ci] = Some(canonical(&make_shape(&cells)));
        }
        comp_shape
    }

    /// Gemini: the two sealed sides of a Gemini edge must have EQUAL shapes.
    fn check_gemini_shape_pairs(
        &self,
        num_comp: usize,
        comp_shape: &[Option<Shape>],
    ) -> Result<(), ()> {
        for clue in &self.edge_clues {
            if !matches!(clue.kind, EdgeClueKind::Gemini) {
                continue;
            }
            let Some((ci1, ci2)) = self.cut_edge_comp_pair(clue.edge, num_comp) else {
                continue;
            };
            if self.is_sealed(ci1) && self.is_sealed(ci2) {
                if let (Some(s1), Some(s2)) = (&comp_shape[ci1], &comp_shape[ci2]) {
                    if s1 != s2 {
                        return Err(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Delta: the two sealed sides of a Delta edge must have DIFFERENT shapes.
    fn check_delta_shape_pairs(
        &self,
        num_comp: usize,
        comp_shape: &[Option<Shape>],
    ) -> Result<(), ()> {
        for clue in &self.edge_clues {
            if !matches!(clue.kind, EdgeClueKind::Delta) {
                continue;
            }
            let Some((ci1, ci2)) = self.cut_edge_comp_pair(clue.edge, num_comp) else {
                continue;
            };
            if let (Some(s1), Some(s2)) = (&comp_shape[ci1], &comp_shape[ci2]) {
                if s1 == s2 {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    fn propagate_shape_constraints(&mut self, num_comp: usize) -> Result<bool, ()> {
        let has_gemini = self
            .edge_clues
            .iter()
            .any(|cl| matches!(cl.kind, EdgeClueKind::Gemini));
        let has_delta = self
            .edge_clues
            .iter()
            .any(|cl| matches!(cl.kind, EdgeClueKind::Delta));
        let has_mingle = self.rules.mingle;
        let has_mismatch = self.rules.mismatch;
        let has_mixed = self.rules.mixed;
        if !has_gemini && !has_delta && !has_mingle && !has_mismatch && !has_mixed {
            return Ok(false);
        }

        // Canonical shape per sealed component.
        let comp_shape = self.sealed_comp_shapes(num_comp);
        let mut progress = false;
        if has_gemini {
            self.check_gemini_shape_pairs(num_comp, &comp_shape)?;
        }
        if has_delta {
            self.check_delta_shape_pairs(num_comp, &comp_shape)?;
        }
        if has_mingle {
            progress |= self.check_mingle(num_comp, &comp_shape)?;
        }
        if has_mismatch {
            self.check_mismatch(num_comp, &comp_shape)?;
        }
        if has_mixed {
            self.check_mixed(num_comp, &comp_shape)?;
        }

        Ok(progress)
    }

    /// `same` (mingle): EVERY piece shares one canonical shape — global, not
    /// merely adjacent (`check_rule_same` requires `len(shape_keys) <= 1`).
    ///
    /// Once a piece is sealed at size `a`, every other piece must also finish
    /// at `a`:
    /// - any two sealed shapes that differ → contradiction;
    /// - a growing component whose size already exceeds `a`, whose target area
    ///   (from clues) is not `a`, or whose growth potential falls short of `a`
    ///   → contradiction;
    /// - a growing component that has already reached size `a` is sealed: force
    ///   its remaining growth edges Cut (the same inference
    ///   `propagate_area_constraints` performs at `size == max_a`).
    ///
    /// Returns whether any edge was forced.
    fn check_mingle(
        &mut self,
        num_comp: usize,
        comp_shape: &[Option<Shape>],
    ) -> Result<bool, ()> {
        // Reference size: the first sealed shape's component size.
        let mut ref_sz: Option<usize> = None;
        let mut ref_shape: Option<&Shape> = None;
        for ci in 0..num_comp {
            let Some(shape) = &comp_shape[ci] else {
                continue;
            };
            match ref_shape {
                None => {
                    ref_shape = Some(shape);
                    ref_sz = Some(self.curr_comp_sz[ci]);
                }
                Some(rs) => {
                    if rs != shape {
                        return Err(());
                    }
                }
            }
        }
        let Some(a) = ref_sz else {
            return Ok(false); // nothing sealed yet — size unknown, no inference
        };
        let mut progress = false;
        for ci in 0..num_comp {
            if comp_shape[ci].is_some() {
                continue; // sealed at `a` already (shapes verified equal above)
            }
            if let Some(target) = self.curr_target_area[ci] {
                if target != a {
                    return Err(());
                }
            }
            let sz = self.curr_comp_sz[ci];
            if sz > a {
                return Err(());
            }
            if !self.is_growing(ci) {
                continue;
            }
            if sz == a {
                // Reached the shared shape size → seal, mirroring the
                // `size == max_a` branch of `propagate_area_constraints`.
                for i in 0..self.prop.growth_edges[ci].len() {
                    let e = self.prop.growth_edges[ci][i];
                    if self.edges[e] == EdgeState::Unknown {
                        if !self.set_edge(e, EdgeState::Cut) {
                            return Err(());
                        }
                        progress = true;
                    }
                }
            } else if self.growth_potential(ci) < a {
                return Err(());
            }
        }
        Ok(progress)
    }

    /// `different` (mismatch): every piece has a distinct canonical shape
    /// (`check_rule_different` requires all shape keys pairwise unique), so two
    /// sealed components carrying the same canonical shape is a contradiction.
    fn check_mismatch(
        &self,
        num_comp: usize,
        comp_shape: &[Option<Shape>],
    ) -> Result<(), ()> {
        // BTreeSet (not HashSet): membership-only here, but the project keeps
        // search-path containers ordered so results stay reproducible.
        let mut taken: std::collections::BTreeSet<&Shape> = std::collections::BTreeSet::new();
        for ci in 0..num_comp {
            if let Some(shape) = &comp_shape[ci] {
                if !taken.insert(shape) {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    /// `mixed`: pieces sharing an edge have different canonical shapes.  Every
    /// Cut edge between two distinct sealed components is such an adjacency, so
    /// equal shapes on the two sides → contradiction.
    fn check_mixed(&self, num_comp: usize, comp_shape: &[Option<Shape>]) -> Result<(), ()> {
        for e in 0..self.grid.num_edges() {
            if self.edges[e] != EdgeState::Cut {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            if ci1 == ci2 || ci1 >= num_comp || ci2 >= num_comp {
                continue;
            }
            if let (Some(s1), Some(s2)) = (&comp_shape[ci1], &comp_shape[ci2]) {
                if s1 == s2 {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    /// Geometric interaction between Gemini and Delta edge clues at a vertex.
    ///
    /// Ported from `third_party/aog/src/solver/propagation/delta_gemini.rs`.
    /// If a Gemini edge and a Delta edge (both same orientation) meet at a
    /// vertex, the two orthogonal (transverse) edges at that vertex cannot BOTH
    /// be Uncut — that would merge the pieces on both sides, requiring
    /// Shape(L) == Shape(R) (Gemini) and Shape(L) != Shape(R) (Delta)
    /// simultaneously.  When the `bricky` rule is on they also cannot BOTH be
    /// Cut (a clue edge is already Cut, so a second Cut would isolate a cell).
    fn propagate_delta_gemini_interaction(&mut self) -> Result<bool, ()> {
        let mut progress = false;
        let mut edge_kinds = vec![None; self.grid.num_edges()];
        for clue in &self.edge_clues {
            edge_kinds[clue.edge] = Some(clue.kind);
        }

        for i in 0..=self.grid.rows {
            for j in 0..=self.grid.cols {
                let ((h_west, h_east), (v_north, v_south)) = self.grid.vertex_edges(i, j);

                // Case 1: Gemini/Delta on a collinear horizontal pair.
                // Transverse edges are the vertical pair.
                if let (Some(e1), Some(e2)) = (h_west, h_east) {
                    if matches!(
                        (edge_kinds[e1], edge_kinds[e2]),
                        (Some(EdgeClueKind::Gemini), Some(EdgeClueKind::Delta))
                            | (Some(EdgeClueKind::Delta), Some(EdgeClueKind::Gemini))
                    ) {
                        if let (Some(t1), Some(t2)) = (v_north, v_south) {
                            progress |= self.propagate_transverse_pair(t1, t2)?;
                        }
                    }
                }

                // Case 2: Gemini/Delta on a collinear vertical pair.
                // Transverse edges are the horizontal pair.
                if let (Some(e1), Some(e2)) = (v_north, v_south) {
                    if matches!(
                        (edge_kinds[e1], edge_kinds[e2]),
                        (Some(EdgeClueKind::Gemini), Some(EdgeClueKind::Delta))
                            | (Some(EdgeClueKind::Delta), Some(EdgeClueKind::Gemini))
                    ) {
                        if let (Some(t1), Some(t2)) = (h_west, h_east) {
                            progress |= self.propagate_transverse_pair(t1, t2)?;
                        }
                    }
                }
            }
        }

        Ok(progress)
    }

    fn propagate_transverse_pair(&mut self, e1: EdgeId, e2: EdgeId) -> Result<bool, ()> {
        let mut progress = false;
        let s1 = self.edges[e1];
        let s2 = self.edges[e2];

        // 1. Cannot both be Uncut.
        if s1 == EdgeState::Uncut && s2 == EdgeState::Uncut {
            return Err(());
        }
        if s1 == EdgeState::Uncut && s2 == EdgeState::Unknown && self.set_edge(e2, EdgeState::Cut) {
            progress = true;
        }
        if s2 == EdgeState::Uncut && s1 == EdgeState::Unknown && self.set_edge(e1, EdgeState::Cut) {
            progress = true;
        }

        // 2. If Bricky, cannot both be Cut.
        if self.rules.bricky {
            if s1 == EdgeState::Cut && s2 == EdgeState::Cut {
                return Err(());
            }
            if s1 == EdgeState::Cut
                && s2 == EdgeState::Unknown
                && self.set_edge(e2, EdgeState::Uncut)
            {
                progress = true;
            }
            if s2 == EdgeState::Cut
                && s1 == EdgeState::Unknown
                && self.set_edge(e1, EdgeState::Uncut)
            {
                progress = true;
            }
        }

        Ok(progress)
    }

    /// Boxy (`block`) / non-boxy (`non_block`) propagation — port of
    /// `third_party/aog area.rs::propagate_boxy_nonboxy` (904-1001).
    ///
    /// A piece is rectangular iff `cell_count == bbox_size`
    /// (`bbox_w * bbox_h` over its cells' row/col extents).
    /// - **boxy**: sealed non-rectangle → Err; growing with holes it can't fill → Err.
    /// - **non_boxy**: sealed rectangle → Err; growing with exactly 1 hole it could
    ///   fill → force Cut on edges to that hole (prevent rectangle formation).
    fn propagate_boxy_nonboxy(&mut self, num_comp: usize) -> Result<bool, ()> {
        if !self.rules.boxy && !self.rules.non_boxy {
            return Ok(false);
        }
        let mut progress = false;
        let (mut min_r, mut max_r, mut min_c, mut max_c) = (
            vec![self.grid.rows; num_comp],
            vec![0; num_comp],
            vec![self.grid.cols; num_comp],
            vec![0; num_comp],
        );
        for ci in 0..num_comp {
            for &c in &self.comp_cells[ci] {
                let (r, col) = self.grid.cell_pos(c);
                min_r[ci] = min_r[ci].min(r);
                max_r[ci] = max_r[ci].max(r);
                min_c[ci] = min_c[ci].min(col);
                max_c[ci] = max_c[ci].max(col);
            }
        }

        let mut non_boxy_cuts: Vec<EdgeId> = Vec::new();
        for ci in 0..num_comp {
            let cell_count = self.curr_comp_sz[ci];
            if cell_count == 0 {
                continue;
            }
            let bbox_w = max_r[ci] - min_r[ci] + 1;
            let bbox_h = max_c[ci] - min_c[ci] + 1;
            let bbox_size = bbox_w * bbox_h;
            let is_rect = cell_count == bbox_size;

            if self.is_sealed(ci) {
                if self.rules.non_boxy && is_rect {
                    return Err(());
                }
                if self.rules.boxy && !is_rect {
                    return Err(());
                }
            } else {
                let max_possible = if ci < self.prop.curr_max_area.len() {
                    self.prop.curr_max_area[ci]
                } else {
                    continue;
                };
                if self.rules.boxy && cell_count < bbox_size && bbox_size > max_possible {
                    return Err(());
                }
                if self.rules.non_boxy && cell_count < bbox_size {
                    let holes = bbox_size - cell_count;
                    // Sound only when the component is capped AT the bbox
                    // (`max_possible == bbox_size`): taking the hole would
                    // reach the cap, seal, and be a rectangle → contradiction,
                    // so the hole edges must be Cut.
                    //
                    // The reference aog (area.rs:958) uses `>=` — unsound:
                    // with `max_possible > bbox_size` the component can take
                    // the hole AND keep growing past the bbox, ending
                    // non-rectangular.  On 0497 (7×7 non_block+fence) the
                    // root component {(0,0),(1,0),(1,1)} has a 2×2 bbox with
                    // one hole (0,1) and unbounded max; the official region 0
                    // takes that hole and extends right to (0,3) — a legal
                    // 6-cell non-rectangle.  The `>=` version Cut e=6
                    // ((0,1)-(1,1)) at the root and exhausted the search
                    // instantly (0497/0171/0688/0824/0921/0926/0932/0993/
                    // 1003/1091 all FAILed this way).
                    if holes == 1 && max_possible == bbox_size {
                        for &c in &self.comp_cells[ci] {
                            let (r, col) = self.grid.cell_pos(c);
                            for (dr, dc) in [(-1isize, 0), (1, 0), (0, -1), (0, 1)] {
                                let nr = r as isize + dr;
                                let nc = col as isize + dc;
                                if nr < 0
                                    || nr >= self.grid.rows as isize
                                    || nc < 0
                                    || nc >= self.grid.cols as isize
                                {
                                    continue;
                                }
                                let nid = self.grid.cell_id(nr as usize, nc as usize);
                                if !self.grid.cell_exists[nid] || self.curr_comp_id[nid] == ci {
                                    continue;
                                }
                                let (hr, hc) = self.grid.cell_pos(nid);
                                if hr >= min_r[ci]
                                    && hr <= max_r[ci]
                                    && hc >= min_c[ci]
                                    && hc <= max_c[ci]
                                {
                                    if let Some(e) = self.grid.edge_between(c, nid) {
                                        if self.edges[e] == EdgeState::Unknown {
                                            non_boxy_cuts.push(e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for e in non_boxy_cuts {
            if self.edges[e] == EdgeState::Unknown {
                if !self.set_edge(e, EdgeState::Cut) {
                    return Err(());
                }
                progress = true;
            }
        }
        Ok(progress)
    }
}

/// Rotation masks of a `PalisadeKind` compatible with the given incident edge
/// states (mask bits N,S,W,E).  Deduped (symmetric kinds repeat under rotation).
fn star_placements(kind: PalisadeKind, states: [EdgeState; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut seen = [false; 16];
    for rot in 0..4 {
        let (_ec, em) = kind.pattern_at_rotation(rot);
        if seen[em as usize] {
            continue;
        }
        seen[em as usize] = true;
        if star_mask_ok(em, states) {
            out.push(em);
        }
    }
    out
}

fn star_mask_ok(m: u8, states: [EdgeState; 4]) -> bool {
    (0..4).all(|k| match states[k] {
        EdgeState::Cut => (m >> k) & 1 == 1,
        EdgeState::Uncut => (m >> k) & 1 == 0,
        EdgeState::Unknown => true,
    })
}
