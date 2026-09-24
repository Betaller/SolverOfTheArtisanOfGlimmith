//! `compass_part` — joint region-partition searcher for pure `compass` +
//! `solitary` puzzles (the `6-compass-main` family).
//!
//! Why not `pieces`: its compass placements are pre-enumerated into DLX rows
//! behind hard truncation (`MAX_COMPASS_PLACEMENTS=2000` /
//! `MAX_COMPASS_ENUM_STATES=200k`).  A clue with an unspecified direction
//! (`-1`) has no size cap, so its frontier-growth lattice explodes and the
//! true placement is truncated away — DLX then falsely reports "no cover"
//! (`pieces:exhausted` on 0312 / 0680–0683 / 1246 / 1258–1260).  The
//! reference C++ AoG_Solver times out on the same puzzles (1246: 60s,
//! 0312: 30s, zero output), so this is a reference blind spot, not a port
//! regression — the fix is a different paradigm, not a port.
//!
//! Paradigm: `solitary` locks the region count to `k = #compass clues`
//! (markers ↔ regions).  Search the k-region partition **jointly**: tight
//! clues (many specified directions) are placed first, the loosest clue
//! absorbs the leftover as its forced region.  Placements are constructed in
//! place over the shrinking free pool — no pre-enumerated lists to truncate —
//! with:
//!
//! * exact half-plane counts (quadrant cells touch two directions, matching
//!   `validate::check_compass`);
//! * per-clue area windows `[lo, hi]` (`1 + max(N+S, E+W)` lower bound,
//!   availability upper bound) tightened by the global sum `Σ|R| = fillable`;
//! * arc-consistent forced-cell deduction: a direction whose available cells
//!   exactly equal its target forces those cells into the clue's region
//!   (fixpoint over clues/dirs, shrinking pools);
//! * residual window checks: every free connected component must be able to
//!   host exactly the unplaced clues it contains.
//!
//! Residual cluster (0682 / 0683 / 1258 / 1260) defeats set enumeration:
//! loose `-1` directions leave huge windows and the first region's lattice
//! alone explodes.  `solve_compass_part` therefore runs a **short-leash phase**
//! of this search, then falls back to the cell-labeling CSP in
//! [`super::compass_label`] for the remainder of the budget.

use std::collections::BTreeSet;
use std::time::Duration;

use crate::clock::Instant;
use crate::types::*;

/// Direction indices: 0=N, 1=S, 2=E, 3=W.
pub(crate) const DIRS: usize = 4;

/// Per-region-growth cap on distinct cell-sets expanded.  A safety valve
/// against RAM blowup (the 1260 OOM family); hitting it abandons the search
/// honestly (`Walk::Aborted` → no solution claimed), never truncates a
/// placement list.
const MAX_GROW_STATES: usize = 3_000_000;

/// True for puzzles this module may attempt: exactly the rules
/// `{compass, solitary}`, every marker cell is a compass cell, at least two
/// clues.
pub fn is_applicable(puzzle: &Puzzle) -> bool {
    let mut ctypes: BTreeSet<&str> = BTreeSet::new();
    for r in &puzzle.rules {
        ctypes.insert(r.ctype.as_str());
    }
    if ctypes.len() != 2 || !ctypes.contains("compass") || !ctypes.contains("solitary") {
        return false;
    }
    if !puzzle.shape_pool.is_empty() {
        return false;
    }
    let mut k = 0usize;
    for row in &puzzle.cells {
        for c in row {
            if c.blocked {
                continue;
            }
            let marker = c.compass.is_some()
                || c.symbol.is_some()
                || c.number.is_some()
                || c.shape_pattern.is_some()
                || c.fence_pattern.is_some();
            if marker {
                if c.compass.is_none() {
                    return false;
                }
                k += 1;
            }
        }
    }
    k >= 2
}

pub fn solve_compass_part(puzzle: &Puzzle, timeout_ms: u64) -> ModuleOutcome {
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms);
    let Some(model) = Model::build(puzzle) else {
        return ModuleOutcome::None;
    };
    // Phase 1 — frontier-growth set enumeration (fast when the area windows
    // are tight: the currently-solved cluster finishes in ≤4s).  Short leash:
    // its failure mode is burning the whole budget grinding a loose
    // first-region lattice, which starves the labeling fallback below.
    let leash = (timeout_ms / 3).clamp(8_000, 12_000).min(timeout_ms);
    let v1_deadline = start + Duration::from_millis(leash);
    let mut st = St::new(model.h * model.w);
    if matches!(model.dfs(&mut st, 0, v1_deadline), Walk::Found) {
        return accept(puzzle, &st.region_of, &model);
    }
    // Phase 2 — cell-labeling CSP (compass_label) on the shared Model.
    if let Some(region_of) = crate::solver::compass_label::solve_labeling(&model, deadline) {
        return accept(puzzle, &region_of, &model);
    }
    ModuleOutcome::None
}

fn accept(puzzle: &Puzzle, region_of: &[Option<usize>], model: &Model) -> ModuleOutcome {
    let regions = crate::solver::rose::build_regions(region_of, model.h, model.w);
    crate::solver::rose::accept_if_valid(regions, puzzle)
        .map(ModuleOutcome::Solved)
        .unwrap_or(ModuleOutcome::ValidationFailed)
}

enum Walk {
    Found,
    Exhausted,
    /// Deadline or state cap hit — the search abandoned its branch space.
    Aborted,
}

pub(crate) struct Model {
    pub(crate) h: usize,
    pub(crate) w: usize,
    /// Cell idx (r*w+c) → clue id.
    clue_of: Vec<Option<usize>>,
    /// Clue id → cell idx.
    pub(crate) clue_pos: Vec<usize>,
    /// Per clue [N, S, E, W] targets (`None` = free direction).
    pub(crate) targets: Vec<[Option<usize>; DIRS]>,
    pub(crate) lo: Vec<usize>,
    pub(crate) hi: Vec<usize>,
    /// Placement order (tightest first), as clue ids.
    order: Vec<usize>,
    /// Linkable neighbours per cell (in-grid, both fillable, no pre-cut edge).
    pub(crate) nbrs: Vec<Vec<usize>>,
    pub(crate) free: Vec<bool>,
    pub(crate) total_fillable: usize,
    /// Per clue/dir: free cells in the half-plane minus other clues' homes
    /// (the pool `R_j ∩ hp_d` can draw from at the root).
    hp_all: Vec<[usize; DIRS]>,
    /// Free-free pre-cut pairs: a pre-drawn wall forces **different regions**
    /// (not merely "unlinked for connectivity") — two cells across one may
    /// still be connected around it, so the labeling search needs the pair as
    /// a must-differ constraint (1093's `(3,1)-(4,1)`).
    pub(crate) wall_pairs: Vec<(usize, usize)>,
}

impl Model {
    pub(crate) fn build(puzzle: &Puzzle) -> Option<Model> {
        Self::build_excluding(puzzle, None)
    }

    /// Like `build`, but `excluded` cells (already-owned by pre-pinned pattern
    /// regions in the pp-pin remainder hand-off) are treated as non-fillable:
    /// they belong to other regions, so they can neither join a compass label
    /// nor count toward any half-plane total.
    pub(crate) fn build_excluding(
        puzzle: &Puzzle,
        excluded: Option<&std::collections::HashSet<usize>>,
    ) -> Option<Model> {
        let (h, w) = (puzzle.height, puzzle.width);
        let n = h * w;
        let mut free = vec![false; n];
        let mut clue_of = vec![None; n];
        let mut clue_pos = Vec::new();
        for row in &puzzle.cells {
            for c in row {
                let idx = c.row * w + c.col;
                if c.blocked || excluded.map_or(false, |e| e.contains(&idx)) {
                    continue;
                }
                free[idx] = true;
                if c.compass.is_some() {
                    clue_of[idx] = Some(clue_pos.len());
                    clue_pos.push(idx);
                }
            }
        }
        let k = clue_pos.len();
        if k < 2 {
            return None;
        }
        let mut targets = Vec::with_capacity(k);
        for &pos in &clue_pos {
            let (r, c) = (pos / w, pos % w);
            let comp = puzzle.cells[r][c].compass.as_ref()?;
            let raw = [
                comp.up.and_then(|v| usize::try_from(v).ok()),
                comp.down.and_then(|v| usize::try_from(v).ok()),
                comp.right.and_then(|v| usize::try_from(v).ok()),
                comp.left.and_then(|v| usize::try_from(v).ok()),
            ];
            targets.push(raw);
        }
        let nbrs = build_nbrs(puzzle, &free);
        let total_fillable = free.iter().filter(|&&f| f).count();
        let mut model = Model {
            h,
            w,
            clue_of,
            clue_pos,
            targets,
            lo: Vec::new(),
            hi: Vec::new(),
            order: Vec::new(),
            nbrs,
            free,
            total_fillable,
            hp_all: Vec::new(),
            wall_pairs: Vec::new(),
        };
        // An empty half-plane (blocked/board edge) is effectively 0 even when
        // the clue leaves it unspecified — pin it so the completion arithmetic
        // and `at_limit` see the real capacity (edge_csp does the same).
        for i in 0..k {
            for d in 0..DIRS {
                if model.targets[i][d].is_none() && model.avail_in_halfplane(i, d) == 0 {
                    model.targets[i][d] = Some(0);
                }
            }
        }
        model.wall_pairs = model.precompute_wall_pairs();
        model.hp_all = model.precompute_hp_all();
        let (lo, hi) = model.area_windows();
        model.lo = lo;
        model.hi = hi;
        model.order = model.placement_order();
        Some(model)
    }

    /// Free-free adjacent pairs separated by a pre-drawn wall.
    fn precompute_wall_pairs(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for x in 0..self.h * self.w {
            if !self.free[x] {
                continue;
            }
            let (r, c) = (x / self.w, x % self.w);
            for (nr, nc) in [
                (r as i64 + 1, c as i64),
                (r as i64, c as i64 + 1),
            ] {
                if nr < 0 || nc < 0 || nr >= self.h as i64 || nc >= self.w as i64 {
                    continue;
                }
                let (nr, nc) = (nr as usize, nc as usize);
                let y = nr * self.w + nc;
                if !self.free[y] {
                    continue;
                }
                if !self.nbrs[x].contains(&y) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// Area window per clue.  Lower: `1 + max(N+S, E+W)` (axis pairs are
    /// disjoint; quadrant cells are double-counted in the four-way sum but at
    /// least one axis pair holds them).  Upper: `1 + Σ target-or-avail` per
    /// direction (availability = fillable cells in the half-plane).  One axis
    /// fully pinned and the other zero-only gives an exact size.  Finally
    /// tightened by the global sum over all clues (`Σ|R| = fillable`).
    fn area_windows(&self) -> (Vec<usize>, Vec<usize>) {
        let k = self.clue_pos.len();
        let mut lo = vec![0usize; k];
        let mut hi = vec![0usize; k];
        for i in 0..k {
            let t = &self.targets[i];
            let nv = t[0].unwrap_or(0);
            let sv = t[1].unwrap_or(0);
            let ev = t[2].unwrap_or(0);
            let wv = t[3].unwrap_or(0);
            lo[i] = 1 + (nv + sv).max(ev + wv);
            if (t[2] == Some(0) && t[3] == Some(0) && t[0].is_some() && t[1].is_some())
                || (t[0] == Some(0) && t[1] == Some(0) && t[2].is_some() && t[3].is_some())
            {
                hi[i] = lo[i];
                continue;
            }
            let mut cap = 1;
            for d in 0..DIRS {
                cap += t[d].unwrap_or_else(|| self.avail_in_halfplane(i, d));
            }
            hi[i] = cap.max(lo[i]);
        }
        self.tighten_global(&mut lo, &mut hi);
        (lo, hi)
    }

    /// Global sum tightening: Σ|R| = total_fillable, so each `hi[i]` shrinks
    /// by the other clues' lower bounds.  Unsatisfiable windows collapse to
    /// `lo > hi` and the search fails at the root.
    fn tighten_global(&self, lo: &mut [usize], hi: &mut [usize]) {
        let k = self.clue_pos.len();
        let sum_lo: usize = lo.iter().sum();
        if sum_lo > self.total_fillable {
            for h in hi.iter_mut() {
                *h = 0;
            }
            return;
        }
        for i in 0..k {
            let others_lo = sum_lo - lo[i];
            let joint = self.total_fillable - others_lo;
            hi[i] = hi[i].min(joint).max(lo[i]);
        }
    }

    /// `hp_all[i][d]`: free cells in `hp_d(i)` excluding other clues' homes.
    fn precompute_hp_all(&self) -> Vec<[usize; DIRS]> {
        let k = self.clue_pos.len();
        let mut out = vec![[0; DIRS]; k];
        for i in 0..k {
            let (cr, cc) = (self.clue_pos[i] / self.w, self.clue_pos[i] % self.w);
            for (idx, &f) in self.free.iter().enumerate() {
                if !f {
                    continue;
                }
                if let Some(o) = self.clue_of[idx] {
                    if o != i {
                        continue;
                    }
                }
                let (r, c) = (idx / self.w, idx % self.w);
                for d in 0..DIRS {
                    if in_halfplane(cr, cc, r, c, d) {
                        out[i][d] += 1;
                    }
                }
            }
        }
        out
    }

    /// Fillable cells strictly in half-plane `d` of clue `i`.
    fn avail_in_halfplane(&self, i: usize, d: usize) -> usize {
        let (cr, cc) = (self.clue_pos[i] / self.w, self.clue_pos[i] % self.w);
        self.free
            .iter()
            .enumerate()
            .filter(|(idx, &f)| {
                f && in_halfplane(cr, cc, idx / self.w, idx % self.w, d)
            })
            .count()
    }

    /// Tightest first: more specified directions, then smaller window, then
    /// larger lower bound.  The loosest clue lands last and takes the leftover.
    fn placement_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.clue_pos.len()).collect();
        order.sort_by_key(|&i| {
            let spec = self.targets[i].iter().filter(|t| t.is_some()).count();
            (
                std::cmp::Reverse(spec),
                self.hi[i] - self.lo[i],
                std::cmp::Reverse(self.lo[i]),
            )
        });
        order
    }

    fn dfs(&self, st: &mut St, pos: usize, deadline: Instant) -> Walk {
        if Instant::now() >= deadline {
            return Walk::Aborted;
        }
        let unplaced: Vec<usize> = self.order[pos..].to_vec();
        if unplaced.len() == 1 {
            return self.last_region(st, unplaced[0]);
        }
        let forced = match propagate_forced(self, st, &unplaced) {
            Some(f) => f,
            None => return Walk::Exhausted,
        };
        if !residual_ok(self, st, &unplaced) {
            return Walk::Exhausted;
        }
        let i = self.order[pos];
        self.place_region(st, &unplaced, &forced, i, pos, deadline)
    }

    /// Grow region `i` over the free pool (minus cells reserved for other
    /// unplaced clues), covering its forced cells, with exact half-plane
    /// counts.  Every valid placement and every strict frontier superset is
    /// tried; `commit_and_next` seals each and recurses.
    fn place_region(
        &self,
        st: &mut St,
        unplaced: &[usize],
        forced: &[Vec<usize>],
        i: usize,
        pos: usize,
        deadline: Instant,
    ) -> Walk {
        let others_lo: usize = unplaced.iter().filter(|&&j| j != i).map(|&j| self.lo[j]).sum();
        let remain_total = self.total_fillable - st.taken_count;
        let cap = self.hi[i].min(remain_total.saturating_sub(others_lo));
        if cap < self.lo[i] {
            return Walk::Exhausted;
        }
        let reserved = reserved_mask(self, st, unplaced, forced, i);
        let mut frontier = BTreeSet::new();
        for &nb in &self.nbrs[self.clue_pos[i]] {
            if !reserved[nb] {
                frontier.insert(nb);
            }
        }
        let blocked = self.init_blocked(st, unplaced, forced, i);
        let mut g = GrowCtx {
            cur: vec![self.clue_pos[i]],
            counts: [0; DIRS],
            frontier,
            visited: BTreeSet::new(),
            blocked,
        };
        g.visited.insert(set_key(&g.cur));
        self.grow_rec(st, &mut g, &forced[i], &reserved, i, cap, pos, deadline)
            .unwrap_or(Walk::Aborted)
    }

    /// Frontier growth with snapshot rollback (`cur` / `counts` / `frontier`)
    /// and one shared visited set over cell-sets, so each placement is
    /// expanded exactly once regardless of addition order.  `Err(())` = the
    /// deadline or the state cap fired.
    #[allow(clippy::too_many_arguments)]
    fn grow_rec(
        &self,
        st: &mut St,
        g: &mut GrowCtx,
        need: &[usize],
        reserved: &[bool],
        i: usize,
        cap: usize,
        pos: usize,
        deadline: Instant,
    ) -> Result<Walk, ()> {
        if Instant::now() >= deadline {
            return Err(());
        }
        st.states += 1;
        if st.states > MAX_GROW_STATES {
            return Err(());
        }
        if !self.completion_ok(g, i, cap) {
            return Ok(Walk::Exhausted);
        }
        if !self.others_capacity_ok(g, pos, i) {
            return Ok(Walk::Exhausted);
        }
        if self.emit_ok(g, need, i, cap) {
            match self.commit_and_next(st, g, i, pos, deadline) {
                Walk::Found => return Ok(Walk::Found),
                Walk::Aborted => return Err(()),
                Walk::Exhausted => {}
            }
        }
        let candidates: Vec<usize> = g.frontier.iter().copied().collect();
        for next in candidates {
            let (mut ncounts, mut ok) = (g.counts, true);
            let (cr, cc) = (self.clue_pos[i] / self.w, self.clue_pos[i] % self.w);
            let (r, c) = (next / self.w, next % self.w);
            for d in 0..DIRS {
                if in_halfplane(cr, cc, r, c, d) {
                    if let Some(t) = self.targets[i][d] {
                        if ncounts[d] + 1 > t {
                            ok = false;
                            break;
                        }
                    }
                    ncounts[d] += 1;
                }
            }
            if !ok || g.cur.len() + 1 > cap {
                continue;
            }
            let mut ncur = g.cur.clone();
            ncur.push(next);
            ncur.sort_unstable();
            if !g.visited.insert(set_key(&ncur)) {
                continue;
            }
            let mut nfrontier = g.frontier.clone();
            nfrontier.remove(&next);
            for &nb in &self.nbrs[next] {
                if !reserved[nb] && !ncur.contains(&nb) {
                    nfrontier.insert(nb);
                }
            }
            let saved = (
                std::mem::replace(&mut g.cur, ncur),
                std::mem::replace(&mut g.counts, ncounts),
                std::mem::replace(&mut g.frontier, nfrontier),
            );
            self.bump_blocked(g, next, pos, 1);
            let res = self.grow_rec(st, g, need, reserved, i, cap, pos, deadline);
            self.bump_blocked(g, next, pos, -1);
            g.cur = saved.0;
            g.counts = saved.1;
            g.frontier = saved.2;
            match res {
                Ok(Walk::Found) => return Ok(Walk::Found),
                Err(()) => return Err(()),
                Ok(Walk::Exhausted) | Ok(Walk::Aborted) => {}
            }
        }
        Ok(Walk::Exhausted)
    }

    /// Per-clue/dir counter of cells already lost from the pool `R_j ∩ hp_d`:
    /// `taken` cells plus cells forced to *another* unplaced clue.  `cur` is
    /// folded in incrementally via `bump_blocked`.  Clue `own`'s forced cells
    /// stay available — they will land in its own region.
    fn init_blocked(
        &self,
        st: &St,
        unplaced: &[usize],
        forced: &[Vec<usize>],
        own: usize,
    ) -> Vec<[usize; DIRS]> {
        let mut blocked = vec![[0; DIRS]; self.clue_pos.len()];
        for &j in unplaced {
            let (cr, cc) = (self.clue_pos[j] / self.w, self.clue_pos[j] % self.w);
            let mut bump = |idx: usize, blocked: &mut Vec<[usize; DIRS]>| {
                let (r, c) = (idx / self.w, idx % self.w);
                for d in 0..DIRS {
                    if in_halfplane(cr, cc, r, c, d) {
                        blocked[j][d] += 1;
                    }
                }
            };
            for (idx, &t) in st.taken.iter().enumerate() {
                // Clue homes are excluded from `hp_all` already; counting them
                // here too would double-subtract and false-prune (the 1246/
                // 1259 regression this guard was added for).
                if t && self.clue_of[idx].is_none() {
                    bump(idx, &mut blocked);
                }
            }
            for &m in unplaced {
                if m == j || m == own {
                    continue;
                }
                for &u in &forced[m] {
                    if !st.taken[u] {
                        bump(u, &mut blocked);
                    }
                }
            }
        }
        blocked
    }

    /// Add (`delta=1`) or remove (`delta=-1`) one cell from the blocked
    /// counters of every unplaced clue whose half-plane contains it.
    fn bump_blocked(&self, g: &mut GrowCtx, cell: usize, pos: usize, delta: i64) {
        if self.clue_of[cell].is_some() {
            return;
        }
        let (r, c) = (cell / self.w, cell % self.w);
        for &j in &self.order[pos..] {
            let (cr, cc) = (self.clue_pos[j] / self.w, self.clue_pos[j] % self.w);
            for d in 0..DIRS {
                if in_halfplane(cr, cc, r, c, d) {
                    if delta >= 0 {
                        g.blocked[j][d] += 1;
                    } else {
                        g.blocked[j][d] -= 1;
                    }
                }
            }
        }
    }

    /// Cross-clue half-plane capacity: every *other* unplaced clue `j` must
    /// still find `t` cells for each specified direction among the pool
    /// `hp_all[j][d] − blocked[j][d]` (cells eaten by `taken ∪ cur` are gone).
    /// Monotone in `cur`, so a violation prunes the whole subtree — this is
    /// what collapses the loose-clue lattices (0682/0683/1258) that pure
    /// per-region completion arithmetic cannot touch.
    fn others_capacity_ok(&self, g: &GrowCtx, pos: usize, i: usize) -> bool {
        for &j in &self.order[pos..] {
            if j == i {
                continue;
            }
            for d in 0..DIRS {
                if let Some(t) = self.targets[j][d] {
                    if self.hp_all[j][d].saturating_sub(g.blocked[j][d]) < t {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Completion arithmetic: with `slots` cells still to add, each add
    /// touches at most two half-planes, so the remaining direction shortfalls
    /// must fit `Σ short ≤ 2·slots` and `short_d ≤ slots`.  Without this the
    /// lattice of partial sets explodes (1259's first region alone exceeded
    /// `MAX_GROW_STATES`); with it a tight clue like D=13,L=12 collapses to a
    /// near-linear chain (25 touches needed in 13 slots ⟹ at most one add may
    /// contribute a single touch).
    fn completion_ok(&self, g: &GrowCtx, i: usize, cap: usize) -> bool {
        let slots = cap.saturating_sub(g.cur.len());
        let mut need = 0usize;
        for d in 0..DIRS {
            if let Some(t) = self.targets[i][d] {
                let short = t.saturating_sub(g.counts[d]);
                if short > slots {
                    return false;
                }
                need += short;
            }
        }
        need <= 2 * slots
    }

    /// A placement is complete when every specified direction is exactly
    /// satisfied, the area window holds, all forced cells are covered, and the
    /// set is connected (a multi-component forced seed may need growth
    /// bridging; a final check is cheaper than tracking components).
    fn emit_ok(&self, g: &GrowCtx, need: &[usize], i: usize, cap: usize) -> bool {
        if g.cur.len() < self.lo[i] || g.cur.len() > cap {
            return false;
        }
        for d in 0..DIRS {
            if let Some(t) = self.targets[i][d] {
                if g.counts[d] != t {
                    return false;
                }
            }
        }
        need.iter().all(|&u| g.cur.contains(&u)) && connected(&self.nbrs, &g.cur)
    }

    /// Seal the placement into `st` and recurse to the next clue; roll back
    /// after the child returns without a solution.
    fn commit_and_next(
        &self,
        st: &mut St,
        g: &GrowCtx,
        i: usize,
        pos: usize,
        deadline: Instant,
    ) -> Walk {
        for &u in &g.cur {
            st.taken[u] = true;
            st.region_of[u] = Some(i);
        }
        st.taken_count += g.cur.len();
        let res = self.dfs(st, pos + 1, deadline);
        if !matches!(res, Walk::Found) {
            for &u in &g.cur {
                st.taken[u] = false;
                st.region_of[u] = None;
            }
            st.taken_count -= g.cur.len();
        }
        res
    }

    /// The last unplaced clue's region is forced: every remaining free cell.
    fn last_region(&self, st: &mut St, i: usize) -> Walk {
        let mut cur = vec![self.clue_pos[i]];
        for (idx, &f) in self.free.iter().enumerate() {
            if f && !st.taken[idx] && idx != self.clue_pos[i] {
                cur.push(idx);
            }
        }
        cur.sort_unstable();
        if cur.len() < self.lo[i] || cur.len() > self.hi[i] {
            return Walk::Exhausted;
        }
        if !connected(&self.nbrs, &cur) {
            return Walk::Exhausted;
        }
        let counts = halfplane_counts(self.clue_pos[i], self.w, &cur);
        for d in 0..DIRS {
            if let Some(t) = self.targets[i][d] {
                if counts[d] != t {
                    return Walk::Exhausted;
                }
            }
        }
        for &u in &cur {
            st.taken[u] = true;
            st.region_of[u] = Some(i);
        }
        Walk::Found
    }
}

type SetKey = [u64; 4];

fn set_key(cells: &[usize]) -> SetKey {
    let mut key = [0u64; 4];
    for &u in cells {
        key[u / 64] |= 1u64 << (u % 64);
    }
    key
}

struct GrowCtx {
    cur: Vec<usize>,
    counts: [usize; DIRS],
    frontier: BTreeSet<usize>,
    visited: BTreeSet<SetKey>,
    /// Per unplaced clue/dir: cells of `taken ∪ cur` inside that half-plane
    /// (incrementally maintained; feeds `others_capacity_ok`).
    blocked: Vec<[usize; DIRS]>,
}

struct St {
    taken: Vec<bool>,
    region_of: Vec<Option<usize>>,
    taken_count: usize,
    /// Global expansion counter across all region growths (one budget for the
    /// whole search — a per-call cap let aborted subtrees' parents keep
    /// spawning fresh lattices, which is what OOM'd 1260).
    states: usize,
}

impl St {
    fn new(n: usize) -> St {
        St {
            taken: vec![false; n],
            region_of: vec![None; n],
            taken_count: 0,
            states: 0,
        }
    }
}

/// Cells region `i` may never grow into: taken cells, other unplaced clues'
/// home cells, and cells forced to another unplaced clue.
fn reserved_mask(
    model: &Model,
    st: &St,
    unplaced: &[usize],
    forced: &[Vec<usize>],
    i: usize,
) -> Vec<bool> {
    let n = model.h * model.w;
    let mut reserved = st.taken.clone();
    for &j in unplaced {
        if j != i {
            reserved[model.clue_pos[j]] = true;
            for &u in &forced[j] {
                reserved[u] = true;
            }
        }
    }
    reserved
}

/// Arc-consistent forced-cell deduction (fixpoint).  For each unplaced clue
/// `j` and specified direction `d`: the pool of cells that can still land in
/// `R_j ∩ halfplane(d)` is `avail`; `avail < target` kills the node;
/// `avail == target` forces every pool cell into `R_j`.  Pools exclude taken
/// cells and cells owned by another unplaced clue (home or forced).  Returns
/// `None` on contradiction.
fn propagate_forced(model: &Model, st: &St, unplaced: &[usize]) -> Option<Vec<Vec<usize>>> {
    let k = model.clue_pos.len();
    let n = model.h * model.w;
    let mut forced: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut owner = vec![None; n];
    for &j in unplaced {
        owner[model.clue_pos[j]] = Some(j);
    }
    loop {
        let mut changed = false;
        for &j in unplaced {
            for d in 0..DIRS {
                if !force_dir(model, st, unplaced, j, d, &mut owner, &mut forced, &mut changed) {
                    return None;
                }
            }
        }
        if !changed {
            return Some(forced);
        }
    }
}

/// One (clue, direction) forced-deduction step.  `false` = contradiction.
#[allow(clippy::too_many_arguments)]
fn force_dir(
    model: &Model,
    st: &St,
    _unplaced: &[usize],
    j: usize,
    d: usize,
    owner: &mut [Option<usize>],
    forced: &mut [Vec<usize>],
    changed: &mut bool,
) -> bool {
    let Some(t) = model.targets[j][d] else {
        return true;
    };
    let (cr, cc) = (model.clue_pos[j] / model.w, model.clue_pos[j] % model.w);
    let mut cand = Vec::new();
    for (idx, &f) in model.free.iter().enumerate() {
        if !f || st.taken[idx] {
            continue;
        }
        match owner[idx] {
            Some(o) if o != j => continue,
            None if model.clue_of[idx].is_some() => continue,
            _ => {}
        }
        if in_halfplane(cr, cc, idx / model.w, idx % model.w, d) {
            cand.push(idx);
        }
    }
    if cand.len() < t {
        return false;
    }
    if cand.len() == t {
        for &u in &cand {
            if owner[u] == Some(j) {
                continue;
            }
            if owner[u].is_some() {
                return false;
            }
            owner[u] = Some(j);
            forced[j].push(u);
            *changed = true;
        }
    }
    true
}

/// Every free connected component must host exactly the unplaced clues inside
/// it, within their combined area window.  A component with no unplaced clue
/// is dead (regions are sealed once placed).
fn residual_ok(model: &Model, st: &St, unplaced: &[usize]) -> bool {
    let n = model.h * model.w;
    let mut seen = vec![false; n];
    let mut q = Vec::new();
    for start in 0..n {
        if !model.free[start] || st.taken[start] || seen[start] {
            continue;
        }
        q.clear();
        q.push(start);
        seen[start] = true;
        let mut size = 0usize;
        let mut lo_sum = 0usize;
        let mut hi_sum = 0usize;
        let mut clues_in = 0usize;
        while let Some(u) = q.pop() {
            size += 1;
            if let Some(j) = model.clue_of[u] {
                if unplaced.contains(&j) {
                    clues_in += 1;
                    lo_sum += model.lo[j];
                    hi_sum += model.hi[j];
                }
            }
            for &nb in &model.nbrs[u] {
                if model.free[nb] && !st.taken[nb] && !seen[nb] {
                    seen[nb] = true;
                    q.push(nb);
                }
            }
        }
        if clues_in == 0 || size < lo_sum || size > hi_sum {
            return false;
        }
    }
    true
}

/// Half-plane membership: a quadrant cell belongs to TWO directions (a NW cell
/// is both North and West), matching `validate::check_compass`.
pub(crate) fn in_halfplane(cr: usize, cc: usize, r: usize, c: usize, d: usize) -> bool {
    match d {
        0 => r < cr,
        1 => r > cr,
        2 => c > cc,
        _ => c < cc,
    }
}

fn halfplane_counts(pos: usize, w: usize, cells: &[usize]) -> [usize; DIRS] {
    let (cr, cc) = (pos / w, pos % w);
    let mut counts = [0; DIRS];
    for &u in cells {
        if u == pos {
            continue;
        }
        let (r, c) = (u / w, u % w);
        for d in 0..DIRS {
            if in_halfplane(cr, cc, r, c, d) {
                counts[d] += 1;
            }
        }
    }
    counts
}

fn connected(nbrs: &[Vec<usize>], cells: &[usize]) -> bool {
    if cells.len() <= 1 {
        return true;
    }
    let set: BTreeSet<usize> = cells.iter().copied().collect();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut q = vec![cells[0]];
    seen.insert(cells[0]);
    while let Some(u) = q.pop() {
        for &nb in &nbrs[u] {
            if set.contains(&nb) && seen.insert(nb) {
                q.push(nb);
            }
        }
    }
    seen.len() == cells.len()
}

/// Precompute linkable neighbours: adjacent (4-neighbourhood), both fillable,
/// no pre-cut boundary edge between them.
fn build_nbrs(puzzle: &Puzzle, free: &[bool]) -> Vec<Vec<usize>> {
    let (h, w) = (puzzle.height, puzzle.width);
    let mut nbrs = vec![Vec::new(); h * w];
    for r in 0..h {
        for c in 0..w {
            let a = r * w + c;
            if !free[a] {
                continue;
            }
            if c + 1 < w {
                let b = r * w + (c + 1);
                if free[b] && !puzzle.h_edges[r][c].is_boundary {
                    nbrs[a].push(b);
                    nbrs[b].push(a);
                }
            }
            if r + 1 < h {
                let b = (r + 1) * w + c;
                if free[b] && !puzzle.v_edges[r][c].is_boundary {
                    nbrs[a].push(b);
                    nbrs[b].push(a);
                }
            }
        }
    }
    nbrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_grid(h: usize, w: usize) -> Puzzle {
        Puzzle {
            height: h,
            width: w,
            cells: (0..h)
                .map(|r| (0..w).map(|c| Cell::new(r, c)).collect())
                .collect(),
            h_edges: vec![vec![Edge::default(); w.saturating_sub(1)]; h],
            v_edges: vec![vec![Edge::default(); w]; h.saturating_sub(1)],
            vertices: vec![vec![Vertex::default(); w + 1]; h + 1],
            rules: vec![
                Rule {
                    ctype: "compass".into(),
                    params: Default::default(),
                },
                Rule {
                    ctype: "solitary".into(),
                    params: Default::default(),
                },
            ],
            shape_pool: Vec::new(),
            outer_boundaries: Vec::new(),
        }
    }

    fn set_compass(p: &mut Puzzle, r: usize, c: usize, u: i64, d: i64, ri: i64, le: i64) {
        p.cells[r][c].compass = Some(CompassClue {
            up: Some(u).filter(|&x| x >= 0),
            down: Some(d).filter(|&x| x >= 0),
            right: Some(ri).filter(|&x| x >= 0),
            left: Some(le).filter(|&x| x >= 0),
        });
    }

    /// 3×3, two clues: (0,0) with S=2,E=2 and (2,2) with N=2,W=2 — the
    /// anti-diagonal split {NW half} / {SE half} of sizes 5/4.
    #[test]
    fn solves_tiny_two_clue() {
        let mut p = empty_grid(3, 3);
        set_compass(&mut p, 0, 0, -1, 2, 2, -1);
        set_compass(&mut p, 2, 2, 2, -1, -1, 2);
        let out = solve_compass_part(&p, 5_000);
        assert!(out.is_solved(), "expected solved, got {:?}", out);
    }

    /// Unsatisfiable: both clues demand the same single shared cell.
    #[test]
    fn rejects_unsatisfiable() {
        let mut p = empty_grid(2, 2);
        // (0,0): E=1 means its only region cell besides itself is east.
        set_compass(&mut p, 0, 0, -1, -1, 1, -1);
        // (0,1): W=1 means its only region cell besides itself is west —
        // both want (0,0)/(0,1)'s counterpart exclusively → conflict.
        set_compass(&mut p, 0, 1, -1, -1, -1, 1);
        // 2×2 board: regions {a,b} and {c,d}... make targets impossible
        // instead: N count of 5 on a 2-row board.
        let mut q = empty_grid(2, 2);
        set_compass(&mut q, 1, 0, 5, -1, -1, -1);
        set_compass(&mut q, 0, 1, -1, 5, -1, -1);
        assert!(matches!(
            solve_compass_part(&q, 5_000),
            ModuleOutcome::None
        ));
        let _ = p;
    }

    /// The real 7×7 member of the cluster (Zone3/6-compass-main/1246).
    #[test]
    fn solves_official_1246() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/6-compass-main/1246.json"
        ))
        .expect("parse 1246");
        assert!(is_applicable(&p));
        let out = solve_compass_part(&p, 30_000);
        assert!(out.is_solved(), "1246 expected solved, got {:?}", out);
    }

    /// The real 7×7 member 1259 (same folder family).
    #[test]
    fn solves_official_1259() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/6-compass-main/1259.json"
        ))
        .expect("parse 1259");
        let out = solve_compass_part(&p, 30_000);
        assert!(out.is_solved(), "1259 expected solved, got {:?}", out);
    }
}

#[cfg(test)]
mod tests_extra {
    use super::*;

    #[test]
    fn solves_official_0680() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/8-endgame/0680.json"
        ))
        .expect("parse 0680");
        assert!(is_applicable(&p));
        let out = solve_compass_part(&p, 30_000);
        assert!(out.is_solved(), "0680 expected solved, got {:?}", out);
    }
}
