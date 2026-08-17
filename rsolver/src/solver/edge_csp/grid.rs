//! Grid geometry: cell / edge / vertex indexing.
//!
//! Ported from `third_party/aog/src/grid.rs`.  The edge-indexing convention is
//! the reference solver's own (and is *not* the same as the `h_edges`/`v_edges`
//! naming in `crate::types::Puzzle` — the adapter maps between them via
//! `edge_between`): `h_edge(r,c)` connects `(r,c)-(r+1,c)` and `v_edge(r,c)`
//! connects `(r,c)-(r,c+1)`.

use super::types::{CellId, EdgeId, VertexId};

#[derive(Clone, Debug)]
pub struct Grid {
    pub rows: usize,
    pub cols: usize,
    /// `cell_exists[c]` is false for blocked cells (and for any cell outside a
    /// non-rectangular grid).
    pub cell_exists: Vec<bool>,
}

impl Grid {
    pub fn new(rows: usize, cols: usize, cell_exists: Vec<bool>) -> Self {
        debug_assert_eq!(cell_exists.len(), rows * cols);
        Self {
            rows,
            cols,
            cell_exists,
        }
    }

    #[inline]
    pub fn cell_pos(&self, c: CellId) -> (usize, usize) {
        (c / self.cols, c % self.cols)
    }

    #[inline]
    pub fn cell_id(&self, r: usize, c: usize) -> CellId {
        r * self.cols + c
    }

    pub fn num_h_edges(&self) -> usize {
        if self.rows > 1 {
            (self.rows - 1) * self.cols
        } else {
            0
        }
    }

    pub fn num_v_edges(&self) -> usize {
        if self.cols > 1 {
            self.rows * (self.cols - 1)
        } else {
            0
        }
    }

    pub fn num_edges(&self) -> usize {
        self.num_h_edges() + self.num_v_edges()
    }

    pub fn num_cells(&self) -> usize {
        self.rows * self.cols
    }

    /// Horizontal edge below cell `(r,c)`, between rows `r` and `r+1`.
    #[inline]
    pub fn h_edge(&self, r: usize, c: usize) -> EdgeId {
        c * (self.rows - 1) + r
    }

    /// Vertical edge right of cell `(r,c)`, between cols `c` and `c+1`.
    #[inline]
    pub fn v_edge(&self, r: usize, c: usize) -> EdgeId {
        self.num_h_edges() + r * (self.cols - 1) + c
    }

    /// Decode edge to `(is_horizontal, r, c)`.
    pub fn decode_edge(&self, e: EdgeId) -> (bool, usize, usize) {
        let nh = self.num_h_edges();
        if e < nh {
            let r = e % (self.rows - 1);
            let c = e / (self.rows - 1);
            (true, r, c)
        } else {
            let idx = e - nh;
            let r = idx / (self.cols - 1);
            let c = idx % (self.cols - 1);
            (false, r, c)
        }
    }

    /// Edge between two adjacent cells, or `None` if not adjacent.
    pub fn edge_between(&self, a: CellId, b: CellId) -> Option<EdgeId> {
        let (ra, ca) = self.cell_pos(a);
        let (rb, cb) = self.cell_pos(b);
        if ra == rb && ca + 1 == cb {
            return Some(self.v_edge(ra, ca));
        }
        if ra == rb && ca == cb + 1 {
            return Some(self.v_edge(ra, cb));
        }
        if ca == cb && ra + 1 == rb {
            return Some(self.h_edge(ra, ca));
        }
        if ca == cb && ra == rb + 1 {
            return Some(self.h_edge(rb, cb));
        }
        None
    }

    /// The two cells adjacent to an edge, sorted: `.0 < .1`.
    pub fn edge_cells(&self, e: EdgeId) -> (CellId, CellId) {
        let (is_h, r, c) = self.decode_edge(e);
        if is_h {
            (self.cell_id(r, c), self.cell_id(r + 1, c))
        } else {
            (self.cell_id(r, c), self.cell_id(r, c + 1))
        }
    }

    /// Vertex at grid point `(i,j)`, `0<=i<=rows`, `0<=j<=cols`.
    #[inline]
    pub fn vertex(&self, i: usize, j: usize) -> VertexId {
        i * (self.cols + 1) + j
    }

    /// Inverse of `vertex`: recover the grid point `(i,j)` from a `VertexId`.
    #[inline]
    pub fn vertex_pos(&self, v: VertexId) -> (usize, usize) {
        (v / (self.cols + 1), v % (self.cols + 1))
    }

    /// The two endpoint vertices of an edge, sorted: `.0 < .1`.
    pub fn edge_vertices(&self, e: EdgeId) -> (VertexId, VertexId) {
        let (is_h, r, c) = self.decode_edge(e);
        if is_h {
            (self.vertex(r + 1, c), self.vertex(r + 1, c + 1))
        } else {
            (self.vertex(r, c + 1), self.vertex(r + 1, c + 1))
        }
    }

    /// Cells sharing a vertex: top-left, top-right, bottom-left, bottom-right.
    pub fn vertex_cells(&self, i: usize, j: usize) -> [Option<CellId>; 4] {
        [
            if i > 0 && j > 0 {
                Some(self.cell_id(i - 1, j - 1))
            } else {
                None
            },
            if i > 0 && j < self.cols {
                Some(self.cell_id(i - 1, j))
            } else {
                None
            },
            if i < self.rows && j > 0 {
                Some(self.cell_id(i, j - 1))
            } else {
                None
            },
            if i < self.rows && j < self.cols {
                Some(self.cell_id(i, j))
            } else {
                None
            },
        ]
    }

    /// The up-to-4 edges around a cell, in order `[north, south, west, east]`.
    pub fn cell_edges(&self, c: CellId) -> [Option<EdgeId>; 4] {
        let (r, col) = self.cell_pos(c);
        [
            if r > 0 {
                Some(self.h_edge(r - 1, col))
            } else {
                None
            },
            if r < self.rows - 1 {
                Some(self.h_edge(r, col))
            } else {
                None
            },
            if col > 0 {
                Some(self.v_edge(r, col - 1))
            } else {
                None
            },
            if col < self.cols - 1 {
                Some(self.v_edge(r, col))
            } else {
                None
            },
        ]
    }

    pub fn total_existing_cells(&self) -> usize {
        self.cell_exists.iter().filter(|&&x| x).count()
    }
}
