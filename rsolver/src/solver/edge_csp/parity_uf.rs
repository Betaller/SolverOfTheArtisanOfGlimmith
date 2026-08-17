//! Union-Find with XOR parity — port of `third_party/aog/src/uf.rs`.
//!
//! Tracks equivalence classes with a parity bit between elements. A parity of
//! 0 means "same group" and 1 means "different group" (e.g. Cut/Uncut for
//! edge-level UF in `propagate_vertex_edge_parity`).
//!
//! Used by the watchtower parity propagator to build global XOR constraints
//! between unknown edges around watchtower vertices (e.g. value=2 on a 4-cell
//! cycle requires exactly 2 cuts, so e1⊕e2⊕e3⊕e4=0 mod 2; when 2 edges are
//! known and 2 unknown, the unknowns are unioned with their XOR relationship).

/// Find the root of `x` and the accumulated XOR parity from `x` to its root.
pub fn uf_find(parent: &[usize], par: &[u8], x: usize) -> (usize, u8) {
    let mut cur = x;
    let mut p = 0u8;
    while parent[cur] != cur {
        p ^= par[cur];
        cur = parent[cur];
    }
    (cur, p)
}

/// Union `c1` and `c2` with parity `rel` (0=same, 1=different).
/// Returns `Ok(true)` if newly merged, `Ok(false)` if already consistent,
/// `Err(())` on contradiction.
pub fn uf_union(
    parent: &mut Vec<usize>,
    rank: &mut Vec<u8>,
    par: &mut Vec<u8>,
    c1: usize,
    c2: usize,
    rel: u8,
) -> Result<bool, ()> {
    let (r1, p1) = uf_find(parent, par, c1);
    let (r2, p2) = uf_find(parent, par, c2);
    if r1 == r2 {
        return if (p1 ^ p2) == rel { Ok(false) } else { Err(()) };
    }
    if rank[r1] < rank[r2] {
        parent[r1] = r2;
        par[r1] = p1 ^ p2 ^ rel;
    } else if rank[r1] > rank[r2] {
        parent[r2] = r1;
        par[r2] = p1 ^ p2 ^ rel;
    } else {
        parent[r2] = r1;
        par[r2] = p1 ^ p2 ^ rel;
        rank[r1] += 1;
    }
    Ok(true)
}

/// Owning wrapper around the parity union-find arrays.
pub struct ParityUF {
    parent: Vec<usize>,
    rank: Vec<u8>,
    par: Vec<u8>,
}

impl ParityUF {
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
            par: vec![0; n],
        }
    }

    pub fn find(&self, x: usize) -> (usize, u8) {
        uf_find(&self.parent, &self.par, x)
    }

    /// Union `c1` and `c2` with parity `rel` (0=same, 1=different).
    /// Returns `Ok(true)` if newly merged, `Ok(false)` if already consistent,
    /// `Err(())` on contradiction.
    pub fn union(&mut self, c1: usize, c2: usize, rel: u8) -> Result<bool, ()> {
        uf_union(&mut self.parent, &mut self.rank, &mut self.par, c1, c2, rel)
    }
}
