//! Edge-variable CSP solver.
//!
//! An independent solver for the edge-constraint-dense rules where the
//! cell-variable aog DFS is a poor fit: `ring`, `brick`, `watchtower`,
//! `compass`, `inequality`, `difference`.  It maintains an explicit three-state
//! edge array (`Unknown`/`Cut`/`Uncut`) and runs a fixed-point propagation loop
//! (vertex-degree, area bounds, clue propagation) plus failed-literal probing,
//! then a DFS over the remaining unknown edges.
//!
//! The internal `EdgeState` array is local to this solver — the global
//! `Edge.is_boundary` model (52 read sites across the codebase) is untouched.
//! A completed edge assignment is flood-filled into regions and handed to the
//! router's `validate::validate` gate via `build_solution`, so a wrong answer
//! can never pass (the router independently re-verifies every solver's output).

pub mod adapter;
pub mod grid;
pub mod parity_uf;
pub mod polyomino;
pub mod prop;
pub mod rose;
pub mod types;

use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;

use crate::clock::Instant;
use crate::types::{ModuleOutcome, Puzzle, RegionInfo};

use adapter::Input;
use grid::Grid;
use prop::PropagationState;
use types::*;

/// Snapshot of undo-trail length for backtracking (trail-based restore).
#[derive(Clone, Copy, Debug)]
struct Snapshot {
    edges: usize,
}

/// The edge-variable search state.  All fields are `pub(crate)` so the
/// propagation submodule (`prop.rs`) can reach them directly.
pub(crate) struct Solver<'a> {
    /// Borrowed for leaf validation (`validate::validate`) — a completed edge
    /// assignment is only accepted when it passes the full 22-rule check.
    pub puzzle: &'a Puzzle,
    pub grid: Grid,
    pub cell_clues: Vec<CellClue>,
    pub edge_clues: Vec<EdgeClue>,
    /// Cached gemini (`homogeneous`) edge ids — the two regions they separate
    /// must end up the same shape, hence the same area.
    pub gemini_edges: Vec<EdgeId>,
    /// True iff the puzzle has at least one Gemini edge AND one Delta edge (the
    /// `propagate_delta_gemini_interaction` precondition).
    pub has_gemini_and_delta: bool,
    pub vertex_clues: Vec<VertexClue>,
    pub rules: GlobalRules,
    pub edges: Vec<EdgeState>,
    /// Undo trail: (edge, previous_state), appended by every `set_edge`.
    pub changed: Vec<(EdgeId, EdgeState)>,
    pub is_pre_cut: Vec<bool>,
    pub eff_min_area: usize,
    pub eff_max_area: usize,
    pub total_cells: usize,
    pub curr_unknown: usize,
    pub node_count: u64,
    // Cached component info from the last `build_components`.
    pub curr_comp_id: Vec<usize>,
    pub curr_comp_sz: Vec<usize>,
    pub curr_target_area: Vec<Option<usize>>,
    // Reusable buffers.
    pub q_buf: Vec<usize>,
    pub visited_buf: Vec<bool>,
    pub can_grow_buf: Vec<bool>,
    pub comp_cells: Vec<Vec<CellId>>,
    pub in_probing: bool,
    pub has_compass_clue: bool,
    pub has_palisade_clue: bool,
    pub prop: PropagationState,
    // Edge-selection cache.
    pub growth_edge_count: Vec<usize>,
    pub watchtower_vertices: BTreeSet<VertexId>,
    // Pre-computed cell clue index.
    pub cell_clues_indexed: Vec<Vec<usize>>,
    // Deadline / timeout bookkeeping.
    pub deadline: Instant,
    pub timed_out: bool,
    // First solution found.
    pub solution_regions: Option<Vec<RegionInfo>>,
    // --- Rose-window state (populated only when rose_window is present) ---
    /// Per-cell rose symbol type index (u8::MAX = no symbol).
    pub cell_rose_sym: Vec<u8>,
    /// Bitmask of all present rose symbol types.
    pub rose_bits_all: u8,
    /// Number of distinct rose symbol types.
    pub num_rose_types: usize,
    /// Rose cells grouped by symbol type index.
    pub rose_by_type: Vec<Vec<CellId>>,
    /// Reusable BFS-visited buffer for rose reachability.
    pub rose_visited: Vec<bool>,
    /// Branching state for rose-pair SAME/DIFF (used by propagation + search).
    pub pair_branch: rose::PairBranchState,
    /// Exact piece count, if known (used by parity propagation for 2-piece).
    pub exact_piece_count: Option<usize>,
    /// `clue_cell[cell]` — cell carries a `solitary`-relevant clue.  Mirrors the
    /// `validate::validate` "solitary" predicate exactly (symbol / compass /
    /// number / shape_pattern / fence_pattern), so `propagate_solitary` prunes
    /// on the same notion of "clue" the leaf check enforces.
    pub clue_cell: Vec<bool>,
    /// Per-cell count of existing cells in each compass half-plane
    /// (N, S, E, W), excluding the cell itself.  Static board geometry, so it
    /// is computed once in `solve()`; `get_compass_area_bounds` uses it to turn
    /// an unspecified direction into a board-sized upper bound.
    pub compass_halfplane_avail: Vec<[usize; 4]>,
    /// `solitary_feasible[cell]` — bitmask of compass-clue indices whose
    /// bounding box contains `cell`.  Under `solitary` every piece holds
    /// exactly one clue, so a cell can only join the piece of a clue whose
    /// bbox covers it; bit `i` clear means "cannot belong to clue i's piece".
    /// Only populated when the propagation is active (see
    /// `solitary_feasible_active`); otherwise empty.
    pub solitary_feasible: Vec<u64>,
    /// Whether `solitary_feasible` carries real information.  Requires the
    /// `solitary` rule, at least one compass clue, ≤64 of them (u64 bitmask),
    /// and — crucially — that *every* clue cell is a compass clue.  If some
    /// clue carries no bbox (a bare symbol / area number / shape pattern), a
    /// cell outside every compass bbox could still join that clue's piece, so
    /// an empty bitmask would be a false contradiction.
    pub solitary_feasible_active: bool,
    /// Exact number of pieces when it follows from a *structural* rule, as
    /// opposed to the (unsound to enable) rose-window deduction that flips on
    /// the two-piece parity seeding — see
    /// `docs/优化/27-exact-piece-count与two-piece-parity证伪.md`.
    ///
    /// Currently only the `precise` rule supplies it: every region has area
    /// `A`, so with `F` fillable cells the partition has exactly `F / A`
    /// pieces (set only when `A` divides `F`).  Consumed by
    /// `propagate_dual_connectivity`.
    pub structural_pieces: Option<usize>,
    /// Upper bound on the piece count when only a lower area bound is known
    /// (`range`/`precise` min): every region has at least `min_area` cells, so
    /// there are at most `total_cells / min_area` of them.  Used by the D2
    /// contradiction check (`cc > pieces_max`); the "equal → force Uncut"
    /// half needs an exact count and is skipped here.
    pub structural_pieces_max: Option<usize>,
}

/// `avail[cell] = [N, S, E, W]` — count of *existing* cells strictly in each
/// half-plane relative to `cell`.  Static board geometry, computed once in
/// `Solver::new`; `get_compass_area_bounds` uses it to turn an unspecified
/// compass direction into a board-sized upper bound.  Doing that inline would
/// be O(H·W) per component per propagate call, i.e. on every search node.
fn compute_halfplane_avail(grid: &Grid) -> Vec<[usize; 4]> {
    let nc = grid.num_cells();
    let mut avail = vec![[0usize; 4]; nc];
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            let id = grid.cell_id(r, c);
            if !grid.cell_exists[id] {
                continue;
            }
            let mut a = [0usize; 4];
            for rr in 0..grid.rows {
                for cc in 0..grid.cols {
                    let nid = grid.cell_id(rr, cc);
                    if !grid.cell_exists[nid] || (rr == r && cc == c) {
                        continue;
                    }
                    if rr < r {
                        a[0] += 1;
                    }
                    if rr > r {
                        a[1] += 1;
                    }
                    if cc > c {
                        a[2] += 1;
                    }
                    if cc < c {
                        a[3] += 1;
                    }
                }
            }
            avail[id] = a;
        }
    }
    avail
}

/// Exact piece count from the `rose_window` rule, when derivable.
///
/// Every piece holds exactly one cell of each symbol type, so if each type
/// occurs `N` times the partition has exactly `N` pieces.  The type set comes
/// from `rose_symbol_types` (rule params first, matching `validate.rs`'s rose
/// semantics); counts that differ mean the puzzle is unsolvable, in which case
/// we return `None` and let the rose propagation reject later.
///
/// This is a *different* consumer from the disabled `exact_piece_count`
/// deduction: that one feeds `rose.rs::propagate_parity`'s two-piece branch,
/// which seeds Cut edges as parity-1 and forces wrong Cuts at the root
/// (`docs/优化/27-exact-piece-count与two-piece-parity证伪.md`).
/// `structural_pieces` only feeds `propagate_dual_connectivity`.
fn rose_structural_pieces(puzzle: &Puzzle) -> Option<usize> {
    let types = crate::shapes::rose_symbol_types(puzzle);
    if types.is_empty() {
        return None;
    }
    let index: std::collections::BTreeMap<&str, usize> = types
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), i))
        .collect();
    let mut counts = vec![0usize; types.len()];
    for row in &puzzle.cells {
        for cell in row {
            if let Some(sym) = &cell.symbol {
                if let Some(&i) = index.get(sym.as_str()) {
                    counts[i] += 1;
                }
            }
        }
    }
    if counts.iter().all(|&c| c == counts[0] && c >= 1) {
        Some(counts[0])
    } else {
        None
    }
}

/// Can two compass clues never sit in the same region?
///
/// Ported from `third_party/aog/src/solver/propagation/area.rs::
/// compass_cells_incompatible`.  Two independent families of conflicts:
///
/// * **Zero-value**: `A.n == 0` forbids any region cell north of A, so a clue
///   B strictly north of A cannot share A's region (and symmetrically for the
///   other five axis/side combinations).
/// * **Value ordering**: if B is north of A then, inside a shared region,
///   everything north of B is also north of A *plus B itself*, so
///   `A.n ≥ B.n + 1`.  Both values known and `B.n ≥ A.n` → incompatible.
///   When the two sit on the same row neither is north of the other, so a
///   shared region forces `A.n == B.n` (and likewise for S / E / W).
///
/// All tests only use known direction values, so they are one-sided: a
/// `false` answer just means "not proven incompatible".
fn compass_cells_incompatible(
    grid: &Grid,
    ca: CellId,
    pa: &CompassData,
    cb: CellId,
    pb: &CompassData,
) -> bool {
    let (ra, cola) = grid.cell_pos(ca);
    let (rb, colb) = grid.cell_pos(cb);
    let (ra, cola, rb, colb) = (ra as isize, cola as isize, rb as isize, colb as isize);

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

    // Value ordering along each axis: the "further" clue needs a strictly
    // smaller count; on the same line the counts must match.
    fn axis_conflict(ahead: isize, va: Option<usize>, vb: Option<usize>) -> bool {
        match (va, vb) {
            (Some(a), Some(b)) => {
                if ahead < 0 {
                    a <= b
                } else if ahead > 0 {
                    b <= a
                } else {
                    a != b
                }
            }
            _ => false,
        }
    }
    // North / South compare along the row axis; East / West along the column.
    axis_conflict(rb - ra, pa.n, pb.n)
        || axis_conflict(ra - rb, pa.s, pb.s)
        || axis_conflict(colb - cola, pa.e, pb.e)
        || axis_conflict(cola - colb, pa.w, pb.w)
}

impl<'a> Solver<'a> {
    fn new(input: Input, deadline: Instant, puzzle: &'a Puzzle) -> Self {
        let n = input.grid.num_edges();
        let nc = input.grid.num_cells();

        let mut cell_clues_indexed = vec![vec![]; nc];
        for (i, clue) in input.cell_clues.iter().enumerate() {
            cell_clues_indexed[clue.cell()].push(i);
        }

        let diff_clues: Vec<(EdgeId, usize)> = input
            .edge_clues
            .iter()
            .filter_map(|cl| match cl.kind {
                EdgeClueKind::Diff { value } => Some((cl.edge, value)),
                _ => None,
            })
            .collect();

        let watchtower_vertices: BTreeSet<VertexId> =
            input.vertex_clues.iter().map(|cl| cl.vertex).collect();

        // Gemini edges cached up front: `check_gemini_pairs` runs inside the
        // propagation loop and needs `&mut self` for `growth_potential`, so it
        // cannot borrow `self.edge_clues` while iterating.
        let gemini_edges: Vec<EdgeId> = input
            .edge_clues
            .iter()
            .filter(|cl| matches!(cl.kind, EdgeClueKind::Gemini))
            .map(|cl| cl.edge)
            .collect();
        let has_gemini = !gemini_edges.is_empty();
        let has_delta = input
            .edge_clues
            .iter()
            .any(|cl| matches!(cl.kind, EdgeClueKind::Delta));
        let has_gemini_and_delta = has_gemini && has_delta;

        let has_compass_clue = input
            .cell_clues
            .iter()
            .any(|c| matches!(c, CellClue::Compass { .. }));
        let has_palisade_clue = input
            .cell_clues
            .iter()
            .any(|c| matches!(c, CellClue::Palisade { .. }));

        // Solitary clue-cell index.  Built straight from `puzzle` (not from
        // `cell_clues`, which only carries the area / compass / palisade clues
        // the propagators consume) so that `symbol` / `shape_pattern` cells also
        // count — `validate`'s solitary check counts them too.
        let mut clue_cell = vec![false; nc];
        for r in 0..puzzle.height {
            for c in 0..puzzle.width {
                let cell = &puzzle.cells[r][c];
                if cell.symbol.is_some()
                    || cell.compass.is_some()
                    || cell.number.is_some()
                    || cell.shape_pattern.is_some()
                    || cell.fence_pattern.is_some()
                {
                    clue_cell[r * puzzle.width + c] = true;
                }
            }
        }

        // Static half-plane cell counts per cell (compass upper bounds).
        let compass_halfplane_avail = compute_halfplane_avail(&input.grid);

        let mut solver = Self {
            puzzle,
            grid: input.grid,
            cell_clues: input.cell_clues,
            edge_clues: input.edge_clues,
            gemini_edges,
            has_gemini_and_delta,
            vertex_clues: input.vertex_clues,
            rules: input.rules,
            edges: vec![EdgeState::Unknown; n],
            changed: Vec::new(),
            is_pre_cut: vec![false; n],
            eff_min_area: input.min_area,
            eff_max_area: input.max_area,
            total_cells: 0,
            curr_unknown: n,
            node_count: 0,
            curr_comp_id: Vec::new(),
            curr_comp_sz: Vec::new(),
            curr_target_area: Vec::new(),
            q_buf: Vec::with_capacity(nc),
            visited_buf: vec![false; nc],
            can_grow_buf: Vec::new(),
            comp_cells: Vec::new(),
            in_probing: false,
            has_compass_clue,
            has_palisade_clue,
            prop: PropagationState::new(diff_clues, nc),
            growth_edge_count: Vec::new(),
            watchtower_vertices,
            cell_clues_indexed,
            deadline,
            timed_out: false,
            solution_regions: None,
            cell_rose_sym: vec![u8::MAX; nc],
            rose_bits_all: 0,
            num_rose_types: 0,
            rose_by_type: Vec::new(),
            rose_visited: vec![false; nc],
            pair_branch: rose::PairBranchState::default(),
            exact_piece_count: None,
            clue_cell,
            compass_halfplane_avail,
            solitary_feasible: Vec::new(),
            solitary_feasible_active: false,
            structural_pieces: None,
            structural_pieces_max: None,
        };

        // Rose-window state: map each distinct symbol string to a type index and
        // record per-cell type.  Only populated when rose_window is present.
        if puzzle.rules.iter().any(|r| r.ctype == "rose_window") {
            let mut sym_to_idx: std::collections::BTreeMap<String, u8> =
                std::collections::BTreeMap::new();
            for r in 0..puzzle.height {
                for c in 0..puzzle.width {
                    if let Some(sym) = &puzzle.cells[r][c].symbol {
                        if !sym_to_idx.contains_key(sym) {
                            sym_to_idx.insert(sym.clone(), sym_to_idx.len() as u8);
                        }
                    }
                }
            }
            let num = sym_to_idx.len();
            if num > 0 && num <= 8 {
                solver.num_rose_types = num;
                solver.rose_bits_all = (1u8 << num) - 1;
                solver.cell_rose_sym = vec![u8::MAX; nc];
                solver.rose_by_type = vec![Vec::new(); num];
                for r in 0..puzzle.height {
                    for c in 0..puzzle.width {
                        let idx = r * puzzle.width + c;
                        if let Some(sym) = &puzzle.cells[r][c].symbol {
                            if let Some(&ti) = sym_to_idx.get(sym) {
                                solver.cell_rose_sym[idx] = ti;
                                solver.rose_by_type[ti as usize].push(idx);
                            }
                        }
                    }
                }

                // NOTE: AOG deduces `exact_piece_count` from the rose window
                // here ("all types occur N times => exactly N pieces",
                // `third_party/aog/src/solver/mod.rs:139`).  We deliberately
                // leave it `None`: writing `Some(n)` flips on the two-piece
                // branch of `rose.rs::propagate_parity`, which seeds every Cut
                // edge as parity=1 and — on 1135 / 1392 — forces edges Cut at
                // the ROOT (nodes=0) that the official solution has Uncut.
                // Count-only (no extra seeding) was re-tried 2026-09-18 with
                // the same result: the seeding lives behind
                // `two_piece == exact_piece_count == Some(2)`, so the count
                // cannot be enabled without it.  Blocks the loop_closure /
                // dual_connectivity ports (doc 26 §2.1-2.2).
                // Full analysis: `docs/优化/27-exact-piece-count与two-piece-parity证伪.md`.
                solver.exact_piece_count = None;
            }
        }

        for &e in &input.pre_cut {
            solver.mark_pre_cut(e);
        }
        solver
    }

    fn mark_pre_cut(&mut self, e: EdgeId) {
        self.is_pre_cut[e] = true;
        if self.edges[e] == EdgeState::Unknown {
            self.edges[e] = EdgeState::Cut;
            self.changed.push((e, EdgeState::Unknown));
        }
    }

    pub(crate) fn set_edge(&mut self, e: EdgeId, s: EdgeState) -> bool {
        if self.edges[e] == s {
            return true;
        }
        if self.edges[e] != EdgeState::Unknown {
            return false;
        }
        self.edges[e] = s;
        self.curr_unknown -= 1;
        self.changed.push((e, EdgeState::Unknown));
        true
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            edges: self.changed.len(),
        }
    }

    fn restore(&mut self, snap: Snapshot) {
        while self.changed.len() > snap.edges {
            let (e, old_state) = self.changed.pop().unwrap();
            if self.edges[e] != EdgeState::Unknown && old_state == EdgeState::Unknown {
                self.curr_unknown += 1;
            }
            self.edges[e] = old_state;
        }
    }

    /// Run `setup` then propagate; always restores.  Returns true iff both
    /// succeeded (used by failed-literal probing).
    fn probe(&mut self, setup: impl FnOnce(&mut Self) -> bool) -> bool {
        let snap = self.snapshot();
        let ok = setup(self) && self.propagate().is_ok();
        self.restore(snap);
        ok
    }

    #[inline]
    pub(crate) fn is_sealed(&self, ci: usize) -> bool {
        ci < self.can_grow_buf.len() && !self.can_grow_buf[ci]
    }

    #[inline]
    pub(crate) fn is_growing(&self, ci: usize) -> bool {
        ci < self.can_grow_buf.len() && self.can_grow_buf[ci]
    }

    fn check_deadline(&mut self) -> bool {
        if Instant::now() >= self.deadline {
            self.timed_out = true;
            true
        } else {
            false
        }
    }

    /// Entry: returns the first valid region assignment, or `None` (no solution
    /// or timed out).  The router re-validates via `validate::validate`.
    pub fn solve(&mut self) -> Option<Vec<RegionInfo>> {
        self.total_cells = self.grid.total_existing_cells();

        // Structural piece count (see the field doc).
        //
        // Source 1 — `precise`: every region has area `A`, so with `F` fillable
        // cells the partition has exactly `F / A` pieces.
        if let Some(a) = self
            .puzzle
            .rules
            .iter()
            .find(|r| r.ctype == "precise")
            .and_then(|r| r.params.get("area"))
            .and_then(|v| v.as_u64())
        {
            let a = a as usize;
            if a >= 1 && self.total_cells % a == 0 {
                self.structural_pieces = Some(self.total_cells / a);
            }
        }
        // Source 2 — `rose_window`: every piece holds exactly one cell of each
        // symbol type, so if each type occurs `N` times the partition has
        // exactly `N` pieces.  See `rose_structural_pieces`.
        if self.structural_pieces.is_none() {
            self.structural_pieces = rose_structural_pieces(self.puzzle);
        }
        // Piece-count upper bound from the global area lower bound: every
        // region has at least `min_area` cells, so there are at most
        // `total_cells / min_area` regions.  Only meaningful when the bound
        // actually bites (`min_area > 1`).
        if self.eff_min_area > 1 {
            self.structural_pieces_max = Some(self.total_cells / self.eff_min_area);
        }

        // Edges adjacent to a blocked/outside cell are outer borders → Cut.
        for e in 0..self.grid.num_edges() {
            let (c1, c2) = self.grid.edge_cells(e);
            if (!self.grid.cell_exists[c1] || !self.grid.cell_exists[c2])
                && self.edges[e] == EdgeState::Unknown
            {
                self.set_edge(e, EdgeState::Cut);
            }
        }

        self.curr_unknown = self
            .edges
            .iter()
            .filter(|&&e| e == EdgeState::Unknown)
            .count();

        // Pre-compute compass clue indices (fast iteration during propagation).
        if self.has_compass_clue {
            self.prop.compass_clue_indices = self
                .cell_clues
                .iter()
                .enumerate()
                .filter_map(|(i, cl)| match cl {
                    CellClue::Compass { cell, .. } if self.grid.cell_exists[*cell] => Some(i),
                    _ => None,
                })
                .collect();
        }

        // Solitary feasibility bitset (see `setup_solitary_feasibility`).
        self.setup_solitary_feasibility();

        // Pre-search compass incompatibility (see
        // `init_compass_incompatibility`): adjacent compass clues that can
        // never share a region get their edge forced Cut before the search
        // starts.
        self.init_compass_incompatibility();

        // Watchtower value==1 startup optimization (port of reference
        // `apply_watchtower_value_one_optimization`): an interior vertex (all 4
        // cells exist) with value==1 means all 4 cells are the same region → all
        // 4 internal edges are Uncut. Force them now so the propagator's Pass B
        // (which only acts when a cut already exists) doesn't leave them Unknown.
        // Border value==1 vertices are left for Pass B (their tree topology means
        // pieces = 1 + k, and value==1 ⇒ k==0 is handled there).
        if !self.vertex_clues.is_empty() {
            let cell_pair_indices: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];
            for clue in self.vertex_clues.clone() {
                if clue.value != 1 {
                    continue;
                }
                let (vi, vj) = self.grid.vertex_pos(clue.vertex);
                let cell_opts = self.grid.vertex_cells(vi, vj);
                // Interior = all 4 cells exist.
                let all_exist = cell_opts.iter().all(|c| c.map_or(false, |cid| self.grid.cell_exists[cid]));
                if !all_exist {
                    continue;
                }
                for &(a_idx, b_idx) in &cell_pair_indices {
                    if let (Some(a), Some(b)) = (cell_opts[a_idx], cell_opts[b_idx]) {
                        if let Some(eid) = self.grid.edge_between(a, b) {
                            if self.edges[eid] == EdgeState::Unknown {
                                self.set_edge(eid, EdgeState::Uncut);
                            }
                        }
                    }
                }
            }
        }

        if self.propagate().is_err() {
            return None;
        }

        self.backtrack_edges();

        if std::env::var("EDGE_CSP_DEBUG").is_ok() {
            eprintln!(
                "edge_csp: nodes={} unknown={} solved={} timed_out={}",
                self.node_count,
                self.curr_unknown,
                self.solution_regions.is_some(),
                self.timed_out
            );
        }

        self.solution_regions.take()
    }

    /// Build the `solitary` feasibility bitset (consumed by
    /// `propagate_solitary_feasibility`).
    ///
    /// Under `solitary` every finished piece holds exactly one clue cell, so a
    /// cell may only join the piece of a clue whose compass bounding box
    /// covers it.  The bbox is sound thanks to region connectivity: to reach a
    /// cell `n` rows north of the clue the region must cross `n` north
    /// half-plane cells, so with `up == n` no region cell can sit further
    /// north than `n` rows (same for the other axes).  An unspecified
    /// direction (-1) leaves that axis unbounded.
    ///
    /// Activated only when every clue cell is a compass clue — otherwise a
    /// cell outside every compass bbox could still belong to a symbol /
    /// area-number piece, and an empty bitmask would be a false contradiction.
    /// K ≤ 64 because the mask is a u64.
    fn setup_solitary_feasibility(&mut self) {
        if !self.rules.solitary || !self.has_compass_clue {
            return;
        }
        let k = self.prop.compass_clue_indices.len();
        if k == 0 || k > 64 {
            return;
        }
        let mut compass_cells = std::collections::BTreeSet::new();
        for &cl_idx in &self.prop.compass_clue_indices {
            if let CellClue::Compass { cell, .. } = &self.cell_clues[cl_idx] {
                compass_cells.insert(*cell);
            }
        }
        let all_clues_are_compass = (0..self.grid.num_cells())
            .all(|c| !self.clue_cell[c] || compass_cells.contains(&c));
        if !all_clues_are_compass {
            return;
        }
        let mut feasible = vec![0u64; self.grid.num_cells()];
        for (bit, &cl_idx) in self.prop.compass_clue_indices.iter().enumerate() {
            let CellClue::Compass { cell, compass } = &self.cell_clues[cl_idx] else {
                continue;
            };
            let (r, c) = self.grid.cell_pos(*cell);
            let (ri, ci) = (r as isize, c as isize);
            let min_r = compass.n.map_or(0, |v| ri - v as isize).max(0);
            let max_r = compass
                .s
                .map_or(self.grid.rows as isize - 1, |v| ri + v as isize)
                .min(self.grid.rows as isize - 1);
            let min_c = compass.w.map_or(0, |v| ci - v as isize).max(0);
            let max_c = compass
                .e
                .map_or(self.grid.cols as isize - 1, |v| ci + v as isize)
                .min(self.grid.cols as isize - 1);
            for rr in min_r..=max_r {
                for cc in min_c..=max_c {
                    let id = self.grid.cell_id(rr as usize, cc as usize);
                    if self.grid.cell_exists[id] {
                        feasible[id] |= 1u64 << bit;
                    }
                }
            }
        }
        self.solitary_feasible = feasible;
        self.solitary_feasible_active = true;
    }

    /// Pre-search compass incompatibility (port of
    /// `third_party/aog/src/solver/propagation/area.rs::init_compass_incompatibility`).
    ///
    /// Two compass clues that can never sit in the same region, and that are
    /// adjacent, get their shared edge forced Cut now — before the search
    /// burns budget rediscovering it.  (Non-adjacent incompatible pairs are
    /// left alone: the reference records them as rose `diffs`, which we do not
    /// want to couple to, and the compass direction propagation already
    /// rejects a merged pair at the leaf.)
    fn init_compass_incompatibility(&mut self) {
        if !self.has_compass_clue {
            return;
        }
        let clues: Vec<(CellId, CompassData)> = self
            .prop
            .compass_clue_indices
            .iter()
            .filter_map(|&cl_idx| match &self.cell_clues[cl_idx] {
                CellClue::Compass { cell, compass } if self.grid.cell_exists[*cell] => {
                    Some((*cell, *compass))
                }
                _ => None,
            })
            .collect();
        for i in 0..clues.len() {
            for j in (i + 1)..clues.len() {
                let (ca, pa) = clues[i];
                let (cb, pb) = clues[j];
                if !compass_cells_incompatible(&self.grid, ca, &pa, cb, &pb) {
                    continue;
                }
                if let Some(eid) = self.grid.edge_between(ca, cb) {
                    if self.edges[eid] == EdgeState::Unknown {
                        // A failed set_edge here means a pre-drawn Uncut
                        // contradicts the incompatibility — the puzzle is
                        // unsolvable, but that is the search's job to prove;
                        // ignore rather than abort setup.
                        let _ = self.set_edge(eid, EdgeState::Cut);
                    }
                }
            }
        }
    }

    /// Flood-fill decided-Uncut edges into connected components, then build
    /// `region_of` and reuse `rose::build_regions` to produce `RegionInfo`.
    /// Only called when `curr_unknown == 0` (every edge decided).
    fn extract_regions(&self) -> Vec<RegionInfo> {
        let n = self.grid.num_cells();
        let mut comp = vec![usize::MAX; n];
        let mut num_pieces = 0usize;
        for c in 0..n {
            if !self.grid.cell_exists[c] || comp[c] != usize::MAX {
                continue;
            }
            comp[c] = num_pieces;
            let mut q = VecDeque::new();
            q.push_back(c);
            while let Some(cur) = q.pop_front() {
                for eid in self.grid.cell_edges(cur).into_iter().flatten() {
                    let (c1, c2) = self.grid.edge_cells(eid);
                    let other = if c1 == cur { c2 } else { c1 };
                    if !self.grid.cell_exists[other] || comp[other] != usize::MAX {
                        continue;
                    }
                    if self.edges[eid] != EdgeState::Cut {
                        comp[other] = num_pieces;
                        q.push_back(other);
                    }
                }
            }
            num_pieces += 1;
        }
        let mut region_of = vec![None; n];
        for c in 0..n {
            if self.grid.cell_exists[c] {
                region_of[c] = Some(comp[c]);
            }
        }
        crate::solver::rose::build_regions(&region_of, self.grid.rows, self.grid.cols)
    }

    /// Multi-factor edge selection (target area, sealed/growing, watchtower).
    fn select_edge(&mut self) -> Option<(EdgeId, i32)> {
        let num_edges = self.grid.num_edges();
        if self.curr_comp_id.is_empty() {
            for e in 0..num_edges {
                if self.edges[e] == EdgeState::Unknown {
                    return Some((e, 0));
                }
            }
            return None;
        }

        let mut best_e = None;
        let mut best_score = i32::MIN;
        for e in 0..num_edges {
            if self.edges[e] != EdgeState::Unknown {
                continue;
            }
            let (c1, c2) = self.grid.edge_cells(e);
            if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
                continue;
            }
            let ci1 = self.curr_comp_id[c1];
            let ci2 = self.curr_comp_id[c2];
            let sz1 = self.curr_comp_sz[ci1];
            let sz2 = self.curr_comp_sz[ci2];

            let mut score = 0i32;
            if let Some(target) = self.curr_target_area[ci1] {
                score += if sz1 < target { 100 } else { 1 };
            } else {
                score += 10;
            }
            if let Some(target) = self.curr_target_area[ci2] {
                score += if sz2 < target { 100 } else { 1 };
            } else {
                score += 10;
            }

            // Cutting would seal a component that still has a target to reach.
            if ci1 < self.growth_edge_count.len()
                && self.growth_edge_count[ci1] == 1
                && self.curr_target_area[ci1].is_some()
            {
                score += 75;
            }
            if ci2 < self.growth_edge_count.len()
                && self.growth_edge_count[ci2] == 1
                && self.curr_target_area[ci2].is_some()
            {
                score += 75;
            }

            let sealed1 = self.is_sealed(ci1);
            let sealed2 = self.is_sealed(ci2);
            if sealed1 ^ sealed2 {
                let other_ci = if sealed1 { ci2 } else { ci1 };
                if self.curr_target_area[other_ci].is_some() {
                    score += 50;
                }
            }

            // Edge incident to a watchtower vertex.
            if !self.watchtower_vertices.is_empty() {
                let (v1, v2) = self.grid.edge_vertices(e);
                if self.watchtower_vertices.contains(&v1) || self.watchtower_vertices.contains(&v2)
                {
                    score += 25;
                }
            }

            if score > best_score {
                best_score = score;
                best_e = Some((e, score));
                if score >= 200 {
                    break;
                }
            }
        }
        best_e
    }

    /// Whether to try `Cut` before `Uncut` for the selected edge.
    fn prefer_cut_first(&self, e: EdgeId) -> bool {
        if self.curr_comp_id.is_empty() {
            return true;
        }
        let (c1, c2) = self.grid.edge_cells(e);
        if !self.grid.cell_exists[c1] || !self.grid.cell_exists[c2] {
            return true;
        }
        let ci1 = self.curr_comp_id[c1];
        let ci2 = self.curr_comp_id[c2];
        let sz1 = self.curr_comp_sz[ci1];
        let sz2 = self.curr_comp_sz[ci2];
        // A component below its target wants to grow → prefer Uncut.
        if let Some(target) = self.curr_target_area[ci1] {
            if sz1 < target {
                return false;
            }
        }
        if let Some(target) = self.curr_target_area[ci2] {
            if sz2 < target {
                return false;
            }
        }
        true
    }

    fn backtrack_edges(&mut self) {
        if self.solution_regions.is_some() || self.timed_out {
            return;
        }
        self.node_count += 1;
        if self.node_count % 1024 == 0 && self.check_deadline() {
            return;
        }

        if self.curr_unknown == 0 {
            // Leaf: validate the completed edge assignment against the full
            // 22-rule checker.  Accept only if valid — otherwise backtrack and
            // keep searching (essential for compass, whose direction counts are
            // only partially propagated and so the first complete assignment is
            // often invalid).
            let regions = self.extract_regions();
            if crate::solver::validate::validate(self.puzzle, &regions) {
                self.solution_regions = Some(regions);
            }
            return;
        }

        let Some((e, score)) = self.select_edge() else {
            return;
        };

        // Rose-pair branching: resolve the final rose ambiguity near the leaves
        // (few unknown edges left).  Gated on `curr_unknown <= thr` (AOG uses a
        // fixed 80; we scale it to `num_edges/4` so small puzzles only branch at
        // their tail too).  Branching on every node explodes the search and
        // regresses borderline puzzles — the bulk of the work is done by edge
        // propagation; pairing only disambiguates the tail.  Safe: a wrong branch
        // is caught by `validate` at the leaf and the router falls through.
        let rose_branch_thr = (self.grid.num_edges() / 4).min(80);
        if self.rose_bits_all != 0 && self.curr_unknown <= rose_branch_thr {
            if let Some((c1, c2)) = self.select_rose_pair(score) {
                self.branch_on_pair(c1, c2);
                return;
            }
        }

        let cut_first = self.prefer_cut_first(e);
        let order: &[EdgeState; 2] = if cut_first {
            &[EdgeState::Cut, EdgeState::Uncut]
        } else {
            &[EdgeState::Uncut, EdgeState::Cut]
        };

        for &val in order {
            if self.solution_regions.is_some() || self.timed_out {
                return;
            }
            let snap = self.snapshot();
            if !self.set_edge(e, val) {
                continue;
            }
            match self.propagate() {
                Ok(_) => self.backtrack_edges(),
                Err(_) => {}
            }
            self.restore(snap);
        }
    }
}

/// Entry point: solve via the edge-variable CSP solver.
///
/// Solutions are accepted at the leaf only after passing the full 22-rule
/// `validate::validate` (see `backtrack_edges`), and the returned regions are
/// re-validated once more as a defensive backstop — so a wrong answer can never
/// pass; the router falls through to the other solvers instead.
pub fn solve_edge_csp(
    puzzle: &Puzzle,
    _start: &Instant,
    timeout_ms: u64,
) -> ModuleOutcome {
    let input = adapter::build_input(puzzle);
    // Nothing to solve on (no fillable cells) → not capable, never a panic.
    if input.grid.total_existing_cells() == 0 {
        return ModuleOutcome::None;
    }
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut solver = Solver::new(input, deadline, puzzle);
    let Some(regions) = solver.solve() else {
        return ModuleOutcome::None;
    };
    if crate::solver::validate::validate(puzzle, &regions) {
        ModuleOutcome::Solved(regions)
    } else {
        ModuleOutcome::ValidationFailed
    }
}

/// Post-fallback trigger: puzzles whose rules are all in the set this solver
/// propagates (ring / brick / watchtower / compass / inequality / difference,
/// plus area numbers and global area bounds).
///
/// Rules edge_csp does NOT propagate (rose / shape / solitary /
/// shape-delta-gemini) are excluded — for those, the search would only be
/// filtered at the leaf by `validate::validate`, wasting the whole budget; the
/// dedicated rose / pieces / aog solvers handle them instead.
///
/// The area family (`area` / `precise` / `range`) qualifies on its own, without
/// an edge rule — doc 22 §4: area/precise/range are the integer-linear
/// constraints of §1's table, and `propagate_area_bounds` already models them
/// (per-component target-area sealing, growth-potential pruning), so an
/// area-only puzzle has real propagation content.  They are additionally gated
/// by `has_area_signal` so a puzzle carrying no usable area information is
/// still rejected instead of routed into the edge DFS.
pub fn is_edge_csp_capable(puzzle: &Puzzle) -> bool {
    const EDGE_RULES: [&str; 7] = [
        "ring",
        "brick",
        "watchtower",
        "compass",
        "inequality",
        "difference",
        "fence",
    ];
    const AREA_RULES: [&str; 3] = ["area", "precise", "range"];
    // `rose_window` / `same` / `different` / `homogeneous` / `mixed` /
    // `heterogeneous` are NOT propagated by edge_csp (it can only leaf-check
    // them via `validate::validate`), but they frequently co-occur with a
    // propagatable edge rule (ring/fence/compass/…). Tolerating them lets
    // edge_csp engage on those edge rules instead of the puzzle being entirely
    // excluded (which starves the search of edge_csp's strong propagation).
    // Pure non-edge combos are still rejected by the EDGE_RULES/AREA_RULES
    // checks below, so the blast radius is limited to edge+window/shape puzzles.
    //
    // `puzzle_piece` is deliberately NOT tolerated: a `puzzle_piece` puzzle is
    // solved by the `pieces` (DLX) solver, and gating it into edge_csp makes
    // edge_csp run first (full budget) and blow the Python subprocess wall
    // budget before `pieces` gets to run (regressed Zone2/0745). `pieces` runs
    // independently of the gate, so excluding `puzzle_piece` here costs nothing.
    const SUPPORTED: [&str; 20] = [
        "ring",
        "brick",
        "watchtower",
        "compass",
        "inequality",
        "difference",
        "area",
        "precise",
        "range",
        "fence",
        "differentiation",
        "block",
        "non_block",
        "solitary",
        "rose_window",
        "same",
        "different",
        "homogeneous",
        "mixed",
        "heterogeneous",
    ];
    if !puzzle
        .rules
        .iter()
        .all(|r| SUPPORTED.contains(&r.ctype.as_str()))
    {
        return false;
    }
    if puzzle
        .rules
        .iter()
        .any(|r| EDGE_RULES.contains(&r.ctype.as_str()))
    {
        return true;
    }
    // Shape-identity rules now carry real propagation (`check_mingle` /
    // `check_mismatch` / `check_mixed` in `propagate_shape_constraints`), so a
    // puzzle whose only usable signal is `same` / `different` / `mixed` is no
    // longer a leaf-check-only search.  This matters most for
    // rose_window+same / rose_window+different combos: `is_rose_capable`
    // rejects them (the rose solver's symbol bookkeeping assumes no global
    // shape identity), so without this gate they were attempted by aog alone.
    if puzzle
        .rules
        .iter()
        .any(|r| matches!(r.ctype.as_str(), "same" | "different" | "mixed"))
    {
        return true;
    }
    // `rose_window` alone also qualifies: edge_csp propagates rose separation
    // (`propagate_rose_separation` / `propagate_rose_phase3` /
    // `propagate_parity`), so a pure rose puzzle is not a leaf-check-only
    // search here.  This gives a second chance to the handful of rose puzzles
    // aog and the dedicated rose solver both miss (the rose solver's greedy
    // `rose_growth` fallback exhausts on them — doc 28).
    if puzzle.rules.iter().any(|r| r.ctype == "rose_window") {
        return true;
    }
    if !puzzle
        .rules
        .iter()
        .any(|r| AREA_RULES.contains(&r.ctype.as_str()))
    {
        return false;
    }
    has_area_signal(puzzle)
}

/// Whether an area-family puzzle (no edge rule) carries something the
/// propagator can actually act on:
/// - per-cell area numbers (`CellClue::Area`), or
/// - a `precise`/`range` bound that really restricts region sizes.
///
/// A vacuous bound (`range` with `min=1`/`max=#fillable`, e.g. a sparse
/// range-only puzzle) gives `build_components` nothing to prune with, so the
/// search would degenerate to an unguided 2^|E| edge DFS — reject it here
/// (doc 22 §4: sparse range puzzles are "constraint-poor, essentially hard").
fn has_area_signal(puzzle: &Puzzle) -> bool {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut fillable = 0usize;
    for r in 0..h {
        for c in 0..w {
            let cell = &puzzle.cells[r][c];
            if cell.blocked {
                continue;
            }
            fillable += 1;
            if cell.number.is_some() {
                return true;
            }
        }
    }
    if fillable == 0 {
        return false;
    }
    for rule in &puzzle.rules {
        match rule.ctype.as_str() {
            // `precise` fixes every region to one size — always a real bound.
            "precise" => {
                if rule.params.get("area").and_then(|v| v.as_u64()).is_some() {
                    return true;
                }
            }
            "range" => {
                if let Some(v) = rule.params.get("min").and_then(|v| v.as_u64()) {
                    if v as usize > 1 {
                        return true;
                    }
                }
                if let Some(v) = rule.params.get("max").and_then(|v| v.as_u64()) {
                    if (v as usize) < fillable {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Preempt trigger: edge-constraint puzzles with no size constraint (precise /
/// range / shape_pool / puzzle_piece) let aog's free shape library grow
/// unboundedly → OOM (exit -9).
///
/// Reserved, not wired: `DEFAULT_SHAPE_CAP = 50k` already turns those OOMs into
/// graceful aog timeouts, letting the *post*-fallback edge_csp run — so a pre-aog
/// preempt is redundant (and would just reattribute small edge puzzles to
/// edge_csp with no solve gain).  Revisit only if the shape cap proves
/// insufficient.
#[allow(dead_code)]
pub fn is_edge_csp_preempt(puzzle: &Puzzle) -> bool {
    let has_edge = is_edge_csp_capable(puzzle);
    let has_size = puzzle.rules.iter().any(|r| {
        matches!(
            r.ctype.as_str(),
            "precise" | "range" | "shape_pool" | "puzzle_piece"
        )
    });
    has_edge && !has_size
}

#[cfg(test)]
mod tests {
    use super::is_edge_csp_capable;

    /// Minimal 2x2 puzzle JSON with the given rules (no cell numbers, no
    /// pre-drawn edges).
    fn puzzle_with_rules(rules_json: &str) -> crate::types::Puzzle {
        let json = format!(
            r#"{{"grid":{{"height":2,"width":2}},
                "cells":[{{"row":0,"col":0}},{{"row":0,"col":1}},
                         {{"row":1,"col":0}},{{"row":1,"col":1}}],
                "edges":[],"vertices":[],"rules":{}}}"#,
            rules_json
        );
        crate::io::parse_puzzle(&json).expect("test puzzle must parse")
    }

    #[test]
    fn edge_rule_puzzles_stay_capable() {
        let p = puzzle_with_rules(r#"[{"type":"ring"}]"#);
        assert!(is_edge_csp_capable(&p));
    }

    #[test]
    fn area_only_with_numbers_is_capable() {
        let json = r#"{"grid":{"height":2,"width":2},
            "cells":[{"row":0,"col":0,"number":2},{"row":0,"col":1},
                     {"row":1,"col":0},{"row":1,"col":1}],
            "edges":[],"vertices":[],"rules":[{"type":"area"}]}"#;
        let p = crate::io::parse_puzzle(json).unwrap();
        assert!(is_edge_csp_capable(&p));
    }

    #[test]
    fn restrictive_range_only_is_capable() {
        let p = puzzle_with_rules(r#"[{"type":"range","params":{"min":2,"max":3}}]"#);
        assert!(is_edge_csp_capable(&p));
    }

    #[test]
    fn vacuous_range_only_stays_out() {
        // min=1 / max=4 == #fillable on a 2x2 board: nothing to propagate.
        let p = puzzle_with_rules(r#"[{"type":"range","params":{"min":1,"max":4}}]"#);
        assert!(!is_edge_csp_capable(&p));
    }

    #[test]
    fn unsupported_rule_stays_out() {
        let p = puzzle_with_rules(r#"[{"type":"rose_window"},{"type":"area"}]"#);
        assert!(!is_edge_csp_capable(&p));
    }

    #[test]
    fn no_area_information_stays_out() {
        // `differentiation` alone is supported but carries no area signal.
        let p = puzzle_with_rules(r#"[{"type":"differentiation"}]"#);
        assert!(!is_edge_csp_capable(&p));
    }

    #[test]
    fn shape_identity_rule_alone_is_capable() {
        // `same`/`different`/`mixed` now propagate, so they qualify even with
        // no edge or area rule (e.g. rose_window+same, which the rose solver
        // refuses and aog alone used to attempt).
        let p = puzzle_with_rules(r#"[{"type":"rose_window"},{"type":"same"}]"#);
        assert!(is_edge_csp_capable(&p));
        let p = puzzle_with_rules(r#"[{"type":"different"}]"#);
        assert!(is_edge_csp_capable(&p));
    }
}
