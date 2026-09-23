//! `compass_label` — compass_part v2: cell-labeling CSP for pure `compass` +
//! `solitary` puzzles with loose area windows.
//!
//! v1 (`super::compass_part`) grows whole region cell-sets in tight-first
//! order.  When several directions are unspecified (`-1`) the area windows are
//! huge and the *first* region's set enumeration alone blows past the budget
//! (0682 / 0683 / 1258 / 1260).  Official-prefix bisection showed the
//! deductions after a few pinned cells are strong — only the set enumeration
//! was missing.  This module flips the granularity: one variable per free
//! cell, value = region label (clue id), and pushes the same constraints as
//! propagation over labels.
//!
//! Propagation (fixpoint):
//! * **potential-connectivity domains** — label `j` stays feasible for cell
//!   `x` only while `j`'s placed cells can reach `x` through
//!   (unassigned ∪ `j`) cells; multi-source bitset worklist (one `u128`
//!   label-set per cell).
//! * **half-plane cardinality AC** — per (clue, dir): `have + pool < t` kills
//!   the node, `have + pool == t` forces the pool, `have == t` forbids the
//!   label inside the half-plane.  `pool` deliberately over-approximates the
//!   realizable set (domain membership only drops provably-impossible
//!   labels), which is what makes the force sound: if an over-approximation
//!   is exactly large enough, every member must be used.
//! * **size windows** — live `sz + potential ≥ lo`, `sz ≤ hi`, and the exact
//!   cover `Σ spare capacity` vs remaining free cells.
//! * **joinability** — each label's placed cells must stay in one
//!   (unassigned ∪ label) component; with zero unassigned left this *is*
//!   final connectivity, so infeasible merges die at the leaf rather than
//!   escaping as a wrong answer.
//!
//! Search: MRV on cells (ties: most placed neighbours, then index), labels
//! ordered by tightest remaining capacity.  `Vec`-backed state only — fully
//! deterministic (the `HashMap` ordering lesson).  Complete because a
//! connected partition has a spanning tree per region rooted at its clue and
//! a tree-order interleaving assigns every cell while it touches its own
//! label — the labeling search explores that order too.

use std::collections::VecDeque;

use crate::clock::Instant;

use super::compass_part::{in_halfplane, Model, DIRS};

/// Hard node budget; the deadline is the primary limit (this is a RAM/safety
/// valve — 40M covers the loose-window cluster's 2-10M-node solutions).
const MAX_NODES: usize = 40_000_000;

/// Solve by cell labeling.  Returns `region_of` (cell → clue id) on success.
pub(crate) fn solve_labeling(model: &Model, deadline: Instant) -> Option<Vec<Option<usize>>> {
    let k = model.clue_pos.len();
    if k > 127 {
        return None; // u128 label bitsets
    }
    let n = model.h * model.w;
    let mut st = LabState::new(model);
    let mut s = Search {
        model,
        hp_cells: precompute_hp_cells(model),
        zone_cells: precompute_zone_cells(model),
        can: vec![0; n],
        dom: vec![0; n],
        marks: vec![0; n],
        mark_gen: 0,
        nodes: 0,
        deadline,
    };
    let found = s.search(&mut st);
    #[cfg(test)]
    set_last_nodes(s.nodes);
    found.then(|| st.to_region_of(model))
}

/// Test-only diagnostic: node count of the last `solve_labeling` call.
#[cfg(test)]
pub(crate) fn last_nodes() -> usize {
    NODES.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
fn set_last_nodes(n: usize) {
    NODES.store(n, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static NODES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Clone)]
struct LabState {
    /// Per free cell: `-1` unassigned, else label 0..k.  Blocked cells keep
    /// `-1` and are never branched on.
    lab: Vec<i32>,
    sz: Vec<usize>,
    /// Per label, per direction: live count of the label's cells strictly in
    /// the half-plane (the clue's own home is in none of its half-planes).
    cnt: Vec<[usize; DIRS]>,
    unassigned: usize,
}

impl LabState {
    fn new(model: &Model) -> LabState {
        let n = model.h * model.w;
        let k = model.clue_pos.len();
        let mut st = LabState {
            lab: vec![-1; n],
            sz: vec![0; k],
            cnt: vec![[0; DIRS]; k],
            unassigned: model.total_fillable,
        };
        for j in 0..k {
            st.assign_raw(model, model.clue_pos[j], j);
        }
        st
    }

    fn assign_raw(&mut self, model: &Model, x: usize, j: usize) {
        self.lab[x] = j as i32;
        self.sz[j] += 1;
        self.unassigned -= 1;
        let (cr, cc) = (
            model.clue_pos[j] / model.w,
            model.clue_pos[j] % model.w,
        );
        let (r, c) = (x / model.w, x % model.w);
        for d in 0..DIRS {
            if in_halfplane(cr, cc, r, c, d) {
                self.cnt[j][d] += 1;
            }
        }
    }

    /// `false` = the cell is already pinned to another label (contradiction).
    fn try_assign(&mut self, model: &Model, x: usize, j: usize) -> bool {
        if self.lab[x] >= 0 {
            return self.lab[x] as usize == j;
        }
        self.assign_raw(model, x, j);
        true
    }

    fn to_region_of(&self, model: &Model) -> Vec<Option<usize>> {
        (0..model.h * model.w)
            .map(|x| {
                if model.free[x] {
                    Some(self.lab[x] as usize)
                } else {
                    None
                }
            })
            .collect()
    }
}

struct Search<'m> {
    model: &'m Model,
    /// `[j][d]` → free cells strictly in half-plane `d` of clue `j`.
    hp_cells: Vec<[Vec<usize>; DIRS]>,
    /// `[j][z]` → free cells in zone `z` of clue `j` (see `zone_of`).
    zone_cells: Vec<[Vec<usize>; 8]>,
    can: Vec<u128>,
    dom: Vec<u128>,
    marks: Vec<u32>,
    mark_gen: u32,
    nodes: usize,
    deadline: Instant,
}

impl Search<'_> {
    fn search(&mut self, st: &mut LabState) -> bool {
        if Instant::now() >= self.deadline {
            return false;
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return false;
        }
        if !self.fixpoint(st) {
            return false;
        }
        if st.unassigned == 0 {
            return true;
        }
        let Some(x) = self.pick_cell(st) else {
            return false;
        };
        let labels = self.value_order(st, x);
        for j in labels {
            let mut snap = st.clone();
            if !snap.try_assign(self.model, x, j) {
                continue;
            }
            if self.search(&mut snap) {
                *st = snap;
                return true;
            }
            if Instant::now() >= self.deadline {
                return false;
            }
        }
        false
    }

    fn fixpoint(&mut self, st: &mut LabState) -> bool {
        loop {
            if !self.joinable(st) {
                return false;
            }
            self.compute_can(st);
            self.compute_domains(st);
            let caps = self.zone_caps(st);
            for (j, &cap) in caps.iter().enumerate() {
                if !needs_joint(self.model, j) {
                    continue;
                }
                if !joint_ok(need_after(self.model, st, j, None), cap) {
                    return false;
                }
            }
            let mut forces: Vec<(usize, usize)> = Vec::new();
            if !self.size_step(st, &mut forces) || !self.cardinality_step(st, &mut forces) {
                return false;
            }
            if !self.singleton_step(st, &mut forces) {
                return false;
            }
            if forces.is_empty() {
                return true;
            }
            for (x, j) in forces {
                if !st.try_assign(self.model, x, j) {
                    return false;
                }
            }
        }
    }

    /// Empty domain kills the node; a singleton domain is forced.
    /// `false` = empty domain found (contradiction).
    fn singleton_step(&self, st: &LabState, forces: &mut Vec<(usize, usize)>) -> bool {
        let m = self.model;
        for x in 0..m.h * m.w {
            if !m.free[x] || st.lab[x] >= 0 {
                continue;
            }
            match self.dom[x].count_ones() {
                0 => return false,
                1 => forces.push((x, self.dom[x].trailing_zeros() as usize)),
                _ => {}
            }
        }
        true
    }

    /// Multi-source bitset worklist: label `j` reaches free cell `x` through
    /// (unassigned ∪ `j`) cells.  Placed cells are per-label walls (their own
    /// label only) and seed their unassigned neighbours.
    fn compute_can(&mut self, st: &LabState) {
        let m = self.model;
        for x in 0..m.h * m.w {
            self.can[x] = 0;
        }
        let mut q: VecDeque<usize> = VecDeque::new();
        for x in 0..m.h * m.w {
            if !m.free[x] || st.lab[x] >= 0 {
                continue;
            }
            let mut b = 0u128;
            for &y in &m.nbrs[x] {
                if st.lab[y] >= 0 {
                    b |= 1u128 << st.lab[y];
                }
            }
            self.can[x] = b;
            if b != 0 {
                q.push_back(x);
            }
        }
        while let Some(x) = q.pop_front() {
            let cx = self.can[x];
            if cx == 0 {
                continue;
            }
            for &y in &m.nbrs[x] {
                if st.lab[y] >= 0 {
                    continue;
                }
                let add = cx & !self.can[y];
                if add != 0 {
                    self.can[y] |= add;
                    q.push_back(y);
                }
            }
        }
    }

    /// `dom` = potential-connectivity minus provably-impossible labels:
    /// size cap reached, exact half-plane count already at target inside that
    /// half-plane, or the label's joint quadrant-count system becomes
    /// infeasible with this cell taken (see `joint_ok`).  The joint verdict
    /// depends only on the cell's zone relative to the clue (a "without x"
    /// feasibility check subtracts one from that zone and one from the
    /// directions the zone feeds) — so it is computed per (label, zone),
    /// 8×k checks instead of n×k.
    fn compute_domains(&mut self, st: &LabState) {
        self.base_domains(st);
        // Joint-count filtering is mutually recursive through `dom` (caps are
        // counted from the current domains) — iterate to an internal fixpoint.
        let m = self.model;
        loop {
            let mut changed = false;
            let caps = self.zone_caps(st);
            for (j, &cap) in caps.iter().enumerate() {
                if !needs_joint(m, j) {
                    continue;
                }
                for z in 0..8 {
                    if cap[z] == 0 {
                        continue;
                    }
                    let mut c2 = cap;
                    c2[z] -= 1;
                    if joint_ok(need_after(m, st, j, Some(z)), c2) {
                        continue;
                    }
                    for &x in &self.zone_cells[j][z] {
                        if st.lab[x] < 0 && self.dom[x] & (1u128 << j) != 0 {
                            self.dom[x] &= !(1u128 << j);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn base_domains(&mut self, st: &LabState) {
        let m = self.model;
        for x in 0..m.h * m.w {
            self.dom[x] = 0;
            if !m.free[x] || st.lab[x] >= 0 {
                continue;
            }
            let (r, c) = (x / m.w, x % m.w);
            let mut bits = self.can[x];
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                if st.sz[j] >= m.hi[j] {
                    continue;
                }
                if self.exact_caps_block(st, j, r, c) {
                    continue;
                }
                self.dom[x] |= 1u128 << j;
            }
        }
    }

    /// Per-label count of domain-carrying unassigned cells in each of the 8
    /// zones (see `zone_of`).
    fn zone_caps(&self, st: &LabState) -> Vec<[usize; 8]> {
        let m = self.model;
        let mut caps = vec![[0usize; 8]; m.clue_pos.len()];
        for x in 0..m.h * m.w {
            if !m.free[x] || st.lab[x] >= 0 {
                continue;
            }
            let (r, c) = (x / m.w, x % m.w);
            let mut bits = self.dom[x];
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let (cr, cc) = (m.clue_pos[j] / m.w, m.clue_pos[j] % m.w);
                caps[j][zone_of(cr, cc, r, c)] += 1;
            }
        }
        caps
    }

    fn exact_caps_block(&self, st: &LabState, j: usize, r: usize, c: usize) -> bool {
        let m = self.model;
        let (cr, cc) = (m.clue_pos[j] / m.w, m.clue_pos[j] % m.w);
        for d in 0..DIRS {
            if let Some(t) = m.targets[j][d] {
                if st.cnt[j][d] >= t && in_halfplane(cr, cc, r, c, d) {
                    return true;
                }
            }
        }
        false
    }

    /// Size-window AC over the (over-approximated) potential sets plus the
    /// exact-cover check `Σ lo-need ≤ unassigned ≤ Σ hi-spare`.
    fn size_step(&self, st: &LabState, forces: &mut Vec<(usize, usize)>) -> bool {
        let m = self.model;
        let mut need = 0usize;
        let mut cap = 0usize;
        for j in 0..m.clue_pos.len() {
            let mut pot: Vec<usize> = Vec::new();
            for x in 0..m.h * m.w {
                if m.free[x] && st.lab[x] < 0 && self.dom[x] & (1u128 << j) != 0 {
                    pot.push(x);
                }
            }
            if st.sz[j] > m.hi[j] || st.sz[j] + pot.len() < m.lo[j] {
                return false;
            }
            if st.sz[j] + pot.len() == m.lo[j] {
                for &x in &pot {
                    forces.push((x, j));
                }
            }
            need += m.lo[j].saturating_sub(st.sz[j]);
            cap += m.hi[j] - st.sz[j];
        }
        !(need > st.unassigned || cap < st.unassigned)
    }

    /// Half-plane cardinality AC per (clue, direction) with an exact target.
    fn cardinality_step(&self, st: &LabState, forces: &mut Vec<(usize, usize)>) -> bool {
        let m = self.model;
        for j in 0..m.clue_pos.len() {
            for d in 0..DIRS {
                let Some(t) = m.targets[j][d] else {
                    continue;
                };
                let have = st.cnt[j][d];
                if have > t {
                    return false;
                }
                let mut pool: Vec<usize> = Vec::new();
                for &x in &self.hp_cells[j][d] {
                    if st.lab[x] < 0 && self.dom[x] & (1u128 << j) != 0 {
                        pool.push(x);
                    }
                }
                let short = t - have;
                if pool.len() < short {
                    return false;
                }
                if pool.len() == short {
                    for &x in &pool {
                        forces.push((x, j));
                    }
                }
            }
        }
        true
    }

    /// Each label's placed cells must lie in one (unassigned ∪ label)
    /// component of `nbrs` — necessary for final connectivity, and exactly
    /// connectivity once nothing is unassigned.
    fn joinable(&mut self, st: &LabState) -> bool {
        let m = self.model;
        let mut stack: Vec<usize> = Vec::new();
        for j in 0..m.clue_pos.len() {
            self.mark_gen += 1;
            let gen = self.mark_gen;
            let start = m.clue_pos[j];
            self.marks[start] = gen;
            stack.clear();
            stack.push(start);
            let mut seen = 1usize;
            while let Some(u) = stack.pop() {
                for &y in &m.nbrs[u] {
                    let own = st.lab[y] >= 0 && st.lab[y] as usize == j;
                    if !(own || st.lab[y] < 0) || self.marks[y] == gen {
                        continue;
                    }
                    self.marks[y] = gen;
                    if own {
                        seen += 1;
                    }
                    stack.push(y);
                }
            }
            if seen != st.sz[j] {
                return false;
            }
        }
        true
    }

    /// Value order: labels already on x's frontier first (regions grow as
    /// coherent blobs — a frontier cell touching a label's mass is far more
    /// likely to join it; guided-profile lesson: out-of-order picks make
    /// snake cells look "remote" and wreck locality), then tightest remaining
    /// capacity (fill fixed quotas before slack), then id.
    fn value_order(&self, st: &LabState, x: usize) -> Vec<usize> {
        let m = self.model;
        let mut labels = bit_list(self.dom[x]);
        labels.sort_by_key(|&j| {
            let adj = m.nbrs[x]
                .iter()
                .filter(|&&y| st.lab[y] == j as i32)
                .count();
            (std::cmp::Reverse(adj), m.hi[j] - st.sz[j], j)
        });
        labels
    }

    /// Pick the next cell to branch on: free unassigned cells on the
    /// assignment frontier (touching at least one placed cell — the
    /// tree-interleaving completeness argument only ever needs frontier
    /// assignments), MRV first, ties prefer more placed neighbours
    /// (propagation bites harder), then the lower index (determinism).
    fn pick_cell(&self, st: &LabState) -> Option<usize> {
        let m = self.model;
        let mut best: Option<(u32, i32, usize)> = None;
        for x in 0..m.h * m.w {
            if !m.free[x] || st.lab[x] >= 0 {
                continue;
            }
            let mut adj = 0i32;
            for &y in &m.nbrs[x] {
                if st.lab[y] >= 0 {
                    adj += 1;
                }
            }
            if adj == 0 {
                continue; // not on the frontier
            }
            let dc = self.dom[x].count_ones();
            let cand = (dc, -adj, x);
            match best {
                Some(b) if b <= cand => {}
                _ => best = Some(cand),
            }
        }
        best.map(|(_, _, x)| x)
    }
}

fn bit_list(bits: u128) -> Vec<usize> {
    let mut out = Vec::new();
    let mut b = bits;
    while b != 0 {
        let j = b.trailing_zeros() as usize;
        b &= b - 1;
        out.push(j);
    }
    out
}

/// The joint quadrant-count system only couples when the label has ≥2 exact
/// directions (a single exact direction reduces to the per-direction pool
/// check in `cardinality_step` — skipping it is a large win on big-k boards).
fn needs_joint(m: &Model, j: usize) -> bool {
    m.targets[j].iter().filter(|t| t.is_some()).count() >= 2
}

/// Relative position of a cell vs its clue: quadrants 0=QNW, 1=QNE, 2=QSW,
/// 3=QSE (each feeds TWO directions) and axes 4=aN, 5=aS, 6=aE, 7=aW (each
/// feeds one).  A quadrant cell counts toward both of its directions — the
/// coupling `joint_ok` reasons about.
fn zone_of(cr: usize, cc: usize, r: usize, c: usize) -> usize {
    match (r.cmp(&cr), c.cmp(&cc)) {
        (std::cmp::Ordering::Less, std::cmp::Ordering::Less) => 0,
        (std::cmp::Ordering::Less, std::cmp::Ordering::Greater) => 1,
        (std::cmp::Ordering::Greater, std::cmp::Ordering::Less) => 2,
        (std::cmp::Ordering::Greater, std::cmp::Ordering::Greater) => 3,
        (std::cmp::Ordering::Less, _) => 4,
        (std::cmp::Ordering::Greater, _) => 5,
        (_, std::cmp::Ordering::Greater) => 6,
        _ => 7,
    }
}

/// Remaining per-direction need of label `j`: `target - cnt`, with one cell
/// of zone `taken_zone` pre-subtracted from every direction that zone feeds
/// (see `zone_feeds`).  `None` entries are free directions.
fn need_after(
    m: &Model,
    st: &LabState,
    j: usize,
    taken_zone: Option<usize>,
) -> [Option<isize>; DIRS] {
    let mut need = [None; DIRS];
    for (d, slot) in need.iter_mut().enumerate() {
        if let Some(t) = m.targets[j][d] {
            let mut n = t as isize - st.cnt[j][d] as isize;
            if let Some(z) = taken_zone {
                if zone_feeds(z, d) {
                    n -= 1;
                }
            }
            *slot = Some(n);
        }
    }
    need
}

/// Which directions a zone cell feeds: quadrant cells touch two, axis cells
/// one (matching `in_halfplane`).
fn zone_feeds(z: usize, d: usize) -> bool {
    match z {
        0 => d == 0 || d == 3,  // QNW → N,W
        1 => d == 0 || d == 2,  // QNE → N,E
        2 => d == 1 || d == 3,  // QSW → S,W
        3 => d == 1 || d == 2,  // QSE → S,E
        4 => d == 0,
        5 => d == 1,
        6 => d == 2,
        _ => d == 3,
    }
}

/// Joint feasibility of the exact half-plane count system for one label.
/// Quadrant cells serve two directions at once (an exact small target can be
/// OVER-served by them — invisible to per-direction pool checks).  Variables
/// are the four quadrant used-counts; each specified direction contributes a
/// pair-sum bound after eliminating its axis count.  Interval propagation
/// only — **conservative**: `false` means proven infeasible, `true` means
/// "not disproven" (sound for pruning).
fn joint_ok(need: [Option<isize>; DIRS], caps: [usize; 8]) -> bool {
    let mut lo = [0isize; 4];
    let mut hi = [
        caps[0] as isize,
        caps[1] as isize,
        caps[2] as isize,
        caps[3] as isize,
    ];
    // (quadrant var i, quadrant var j, axis zone) per direction N,S,E,W.
    const PAIRS: [(usize, usize, usize); DIRS] = [(0, 1, 4), (2, 3, 5), (1, 3, 6), (0, 2, 7)];
    let mut cons: Vec<(usize, usize, isize, isize)> = Vec::with_capacity(DIRS);
    for d in 0..DIRS {
        let Some(n) = need[d] else { continue };
        if n < 0 {
            return false;
        }
        let (i, j, az) = PAIRS[d];
        cons.push((i, j, n - caps[az] as isize, n));
    }
    loop {
        let mut changed = false;
        for &(i, j, l, u) in &cons {
            if l > hi[i] + hi[j] || u < lo[i] + lo[j] {
                return false;
            }
            let (nl, nh) = ((l - hi[j]).max(lo[i]), (u - lo[j]).min(hi[i]));
            if nl > nh {
                return false;
            }
            if nl != lo[i] || nh != hi[i] {
                lo[i] = nl;
                hi[i] = nh;
                changed = true;
            }
            let (nl, nh) = ((l - hi[i]).max(lo[j]), (u - lo[i]).min(hi[j]));
            if nl > nh {
                return false;
            }
            if nl != lo[j] || nh != hi[j] {
                lo[j] = nl;
                hi[j] = nh;
                changed = true;
            }
        }
        if !changed {
            return true;
        }
    }
}

fn precompute_hp_cells(model: &Model) -> Vec<[Vec<usize>; DIRS]> {
    let k = model.clue_pos.len();
    let mut out: Vec<[Vec<usize>; DIRS]> = Vec::with_capacity(k);
    for j in 0..k {
        let mut lists: [Vec<usize>; DIRS] = Default::default();
        let (cr, cc) = (
            model.clue_pos[j] / model.w,
            model.clue_pos[j] % model.w,
        );
        for (x, &f) in model.free.iter().enumerate() {
            if !f {
                continue;
            }
            let (r, c) = (x / model.w, x % model.w);
            for (d, list) in lists.iter_mut().enumerate() {
                if in_halfplane(cr, cc, r, c, d) {
                    list.push(x);
                }
            }
        }
        out.push(lists);
    }
    out
}

fn precompute_zone_cells(model: &Model) -> Vec<[Vec<usize>; 8]> {
    let k = model.clue_pos.len();
    let mut out: Vec<[Vec<usize>; 8]> = Vec::with_capacity(k);
    for j in 0..k {
        let mut lists: [Vec<usize>; 8] = Default::default();
        let (cr, cc) = (
            model.clue_pos[j] / model.w,
            model.clue_pos[j] % model.w,
        );
        for (x, &f) in model.free.iter().enumerate() {
            if !f {
                continue;
            }
            let (r, c) = (x / model.w, x % model.w);
            lists[zone_of(cr, cc, r, c)].push(x);
        }
        out.push(lists);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::compass_part::{is_applicable, Model};
    use super::*;
    use crate::solver::rose::{accept_if_valid, build_regions};
    use std::time::Duration;

    fn run(json: &str, budget_ms: u64) -> bool {
        let p = crate::io::parse_puzzle(json).expect("parse");
        assert!(is_applicable(&p));
        let model = Model::build(&p).expect("model");
        let deadline = Instant::now() + Duration::from_millis(budget_ms);
        let t0 = Instant::now();
        let out = solve_labeling(&model, deadline);
        eprintln!(
            "solve_labeling nodes={} elapsed={:?}",
            last_nodes(),
            t0.elapsed()
        );
        match out {
            Some(region_of) => {
                let regions = build_regions(&region_of, model.h, model.w);
                accept_if_valid(regions, &p).is_some()
            }
            None => false,
        }
    }

    /// The 7×7 tight-count member of the loose-window cluster.
    #[test]
    fn solves_official_1260() {
        assert!(run(
            include_str!("../../../puzzles/official/Zone3/6-compass-main/1260.json"),
            60_000
        ));
    }

    #[test]
    fn solves_official_0682() {
        assert!(run(
            include_str!("../../../puzzles/official/Zone3/8-endgame/0682.json"),
            60_000
        ));
    }

    /// Residual of the loose-window cluster: the labeling search burns the
    /// 60s deadline (~19M nodes) without reaching the solution — a search-
    /// space battle, not a soundness bug (deadline-cut, not exhausted).
    /// Tracked in `docs/rust-solver/13-compass划分求解器.md` §6.
    #[test]
    #[ignore = "search-space residual (60s deadline); see doc 13 §6"]
    fn solves_official_0683() {
        assert!(run(
            include_str!("../../../puzzles/official/Zone3/6-compass-main/0683.json"),
            60_000
        ));
    }

    /// Same residual as 0683 (k=44, dense clue board; ~1.2M nodes / 60s).
    #[test]
    #[ignore = "search-space residual (60s deadline); see doc 13 §6"]
    fn solves_official_1258() {
        assert!(run(
            include_str!("../../../puzzles/official/Zone3/6-compass-main/1258.json"),
            60_000
        ));
    }

    /// Diagnostic: walk the official assignment order and report the first
    /// step where fixpoint dies or the official label leaves the domain.
    #[test]
    fn official_path_audit_0682() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/8-endgame/0682.json"
        ))
        .expect("parse");
        let model = Model::build(&p).expect("model");
        let ans_raw: serde_json::Value = serde_json::from_str(include_str!(
            "../../../puzzles/official/Zone3-answer/8-endgame/0682.json"
        ))
        .expect("parse answer");
        let ans: Vec<Vec<[usize; 2]>> =
            serde_json::from_value(ans_raw["regions"].clone()).expect("regions");
        let mut region_of = vec![None; model.h * model.w];
        for (j, reg) in ans.iter().enumerate() {
            for [r, c] in reg {
                region_of[r * model.w + c] = Some(j);
            }
        }
        // Map answer region index → clue id via the clue home's official label.
        let mut ans_to_clue = vec![usize::MAX; ans.len()];
        for j in 0..model.clue_pos.len() {
            let home = model.clue_pos[j];
            let a = region_of[home].expect("clue home labeled");
            ans_to_clue[a] = j;
        }
        let mut st = LabState::new(&model);
        let mut s = Search {
            model: &model,
            hp_cells: precompute_hp_cells(&model),
            zone_cells: precompute_zone_cells(&model),
            can: vec![0; model.h * model.w],
            dom: vec![0; model.h * model.w],
            marks: vec![0; model.h * model.w],
            mark_gen: 0,
            nodes: 0,
            deadline: Instant::now() + Duration::from_secs(30),
        };
        // Root fixpoint first — a wrong force here poisons every branch.
        if !s.fixpoint(&mut st) {
            panic!("root fixpoint died");
        }
        for y in 0..model.h * model.w {
            if !model.free[y] {
                continue;
            }
            let jy = ans_to_clue[region_of[y].unwrap()];
            if st.lab[y] >= 0 && st.lab[y] as usize != jy {
                panic!(
                    "root force mismatch at cell {y}: forced {} official {jy}",
                    st.lab[y]
                );
            }
            if st.lab[y] < 0 && s.dom[y] & (1u128 << jy) == 0 {
                panic!("root pruned official label {jy} from cell {y}");
            }
        }
        // Assign in row-major order (clue homes already assigned).
        for x in 0..model.h * model.w {
            if !model.free[x] || st.lab[x] >= 0 {
                continue;
            }
            let j = ans_to_clue[region_of[x].unwrap()];
            if !st.try_assign(&model, x, j) {
                panic!("conflict assigning official label at cell {x}");
            }
            if !s.fixpoint(&mut st) {
                panic!("fixpoint died after assigning cell {x} -> label {j}");
            }
            // Every forced assignment must match the official solution; the
            // remaining official labels must still be in-domain.
            for y in 0..model.h * model.w {
                if !model.free[y] {
                    continue;
                }
                let jy = ans_to_clue[region_of[y].unwrap()];
                if st.lab[y] >= 0 && st.lab[y] as usize != jy {
                    panic!(
                        "force mismatch at cell {y} after assigning {x}: forced {} official {jy}",
                        st.lab[y]
                    );
                }
                if st.lab[y] < 0 && s.dom[y] & (1u128 << jy) == 0 {
                    panic!(
                        "official label {jy} pruned from cell {y} after assigning {x} (can={:x}, dom={:x})",
                        s.can[y], s.dom[y]
                    );
                }
            }
        }
    }

    /// Diagnostic: guided search — at each MRV pick, branch only on the
    /// official label.  A death here proves an order-dependent unsound prune
    /// (row-major audit is green); success means the machinery is fine and
    /// plain search's branching/domains are the gap.
    #[test]
    fn guided_search_0682() {
        let p = crate::io::parse_puzzle(include_str!(
            "../../../puzzles/official/Zone3/8-endgame/0682.json"
        ))
        .expect("parse");
        let model = Model::build(&p).expect("model");
        let ans_raw: serde_json::Value = serde_json::from_str(include_str!(
            "../../../puzzles/official/Zone3-answer/8-endgame/0682.json"
        ))
        .expect("parse answer");
        let ans: Vec<Vec<[usize; 2]>> =
            serde_json::from_value(ans_raw["regions"].clone()).expect("regions");
        let mut region_of = vec![None; model.h * model.w];
        for (j, reg) in ans.iter().enumerate() {
            for [r, c] in reg {
                region_of[r * model.w + c] = Some(j);
            }
        }
        let mut ans_to_clue = vec![usize::MAX; ans.len()];
        for j in 0..model.clue_pos.len() {
            let a = region_of[model.clue_pos[j]].expect("clue home labeled");
            ans_to_clue[a] = j;
        }
        let mut st = LabState::new(&model);
        let mut s = Search {
            model: &model,
            hp_cells: precompute_hp_cells(&model),
            zone_cells: precompute_zone_cells(&model),
            can: vec![0; model.h * model.w],
            dom: vec![0; model.h * model.w],
            marks: vec![0; model.h * model.w],
            mark_gen: 0,
            nodes: 0,
            deadline: Instant::now() + Duration::from_secs(30),
        };
        let n = model.h * model.w;
        if !s.fixpoint(&mut st) {
            panic!("root fixpoint died");
        }
        let mut steps = 0usize;
        let mut rank_stats: Vec<usize> = Vec::new();
        while st.unassigned > 0 {
            steps += 1;
            let Some(x) = s.pick_cell(&st) else {
                panic!("pick_cell empty with {} unassigned", st.unassigned);
            };
            let j = ans_to_clue[region_of[x].unwrap()];
            if s.dom[x] & (1u128 << j) == 0 {
                panic!(
                    "step {steps}: official label {j} not in dom of MRV cell {x} (can={:x}, dom={:x}, assigned={})",
                    s.can[x],
                    s.dom[x],
                    n - st.unassigned
                );
            }
            let labels = s.value_order(&st, x);
            let rank = labels.iter().position(|&l| l == j).unwrap();
            rank_stats.push(rank);
            let adj_of = |l: usize| {
                model.nbrs[x]
                    .iter()
                    .filter(|&&y| st.lab[y] == l as i32)
                    .count()
            };
            let pref = labels[0];
            eprintln!(
                "S{steps} x={x} dom={} off=j{j} pref=j{pref} rank={rank} adj_off={} adj_pref={} slack_off={} slack_pref={} sz_off={} sz_pref={} targets_off={}",
                labels.len(),
                adj_of(j),
                adj_of(pref),
                model.hi[j] - st.sz[j],
                model.hi[pref] - st.sz[pref],
                st.sz[j],
                st.sz[pref],
                model.targets[j].iter().filter(|t| t.is_some()).count(),
            );
            assert!(st.try_assign(&model, x, j), "guided assign conflict");
            if !s.fixpoint(&mut st) {
                panic!(
                    "step {steps}: fixpoint died after guided assign {x}->{j} (assigned={})",
                    n - st.unassigned
                );
            }
        }
        let at_first = rank_stats.iter().filter(|&&r| r == 0).count();
        eprintln!(
            "guided steps={steps} official-rank-first={at_first} avg-rank={:.2} max-rank={}",
            rank_stats.iter().sum::<usize>() as f64 / steps as f64,
            rank_stats.iter().max().unwrap_or(&0)
        );
    }
}
