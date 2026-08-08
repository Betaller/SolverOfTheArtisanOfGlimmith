//! Cell-set bitsets and pre-drawn boundary helpers for the rose solver.
//!
//! `CellSet` is a row-major bitset over cell index `idx = r*w + c`.  The MRV
//! matching and candidate BFS do millions of overlap/subset tests, so the
//! bitset keeps them allocation-free (a handful of `u64` AND/popcount ops).

use std::collections::{HashMap, HashSet};

use crate::types::Puzzle;

/// Bit set over `n_bits` cells (word-aligned `Vec<u64>`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CellSet {
    words: Vec<u64>,
}

impl CellSet {
    pub fn new(n_bits: usize) -> Self {
        CellSet {
            words: vec![0u64; n_bits.div_ceil(64)],
        }
    }

    pub fn contains(&self, idx: usize) -> bool {
        self.words[idx / 64] & (1u64 << (idx % 64)) != 0
    }

    pub fn insert(&mut self, idx: usize) {
        self.words[idx / 64] |= 1u64 << (idx % 64);
    }

    pub fn remove(&mut self, idx: usize) {
        self.words[idx / 64] &= !(1u64 << (idx % 64));
    }

    /// True if no bit is set in both sets (HOT in candidate overlap test).
    pub fn is_disjoint(&self, other: &Self) -> bool {
        self.words
            .iter()
            .zip(&other.words)
            .all(|(a, b)| a & b == 0)
    }

    /// True if every bit of self is also set in other.
    pub fn is_subset(&self, other: &Self) -> bool {
        self.words
            .iter()
            .zip(&other.words)
            .all(|(a, b)| a & !b == 0)
    }

    pub fn union_into(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }

    pub fn len(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Number of bits (cells) this set spans (rounded up to the word).
    pub fn len_bits(&self) -> usize {
        self.words.len() * 64
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|w| *w == 0)
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(move |(wi, w)| {
            let mut bits = *w;
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let tz = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                Some(wi * 64 + tz)
            })
        })
    }
}

/// Canonical undirected edge key.  Mirrors the Python `_edge_key`: endpoints
/// sorted so `edge_key(1,2,1,1) == edge_key(1,1,1,2)`.  Grid coords < 256.
pub fn edge_key(r1: usize, c1: usize, r2: usize, c2: usize) -> u32 {
    let (ar, ac, br, bc) = if r1 < r2 || (r1 == r2 && c1 < c2) {
        (r1, c1, r2, c2)
    } else {
        (r2, c2, r1, c1)
    };
    ((ar as u32) << 24) | ((ac as u32) << 16) | ((br as u32) << 8) | (bc as u32)
}

/// Set of pre-drawn boundary edges (forced region separators).
pub struct PreBoundaries {
    set: HashSet<u32>,
}

impl PreBoundaries {
    pub fn from_puzzle(p: &Puzzle) -> Self {
        let mut set = HashSet::new();
        for r in 0..p.height {
            for c in 0..p.width.saturating_sub(1) {
                if p.h_edges[r][c].is_boundary {
                    set.insert(edge_key(r, c, r, c + 1));
                }
            }
        }
        for r in 0..p.height.saturating_sub(1) {
            for c in 0..p.width {
                if p.v_edges[r][c].is_boundary {
                    set.insert(edge_key(r, c, r + 1, c));
                }
            }
        }
        PreBoundaries { set }
    }

    pub fn contains(&self, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
        self.set.contains(&edge_key(r1, c1, r2, c2))
    }

    /// Whether no pre-drawn boundary edges exist. (白捡 W5: when empty, the grid
    /// is 4-connected and `can_partition` can short-circuit.)
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Iterate canonical edges as `[r1, c1, r2, c2]`.
    pub fn iter(&self) -> impl Iterator<Item = [usize; 4]> + '_ {
        self.set.iter().map(|k| {
            [
                (k >> 24) as usize,
                ((k >> 16) & 0xff) as usize,
                ((k >> 8) & 0xff) as usize,
                (k & 0xff) as usize,
            ]
        })
    }
}

/// Build `cell index -> symbol-type index` map and the set of all symbol cells.
pub fn symbol_index_map(
    puzzle: &Puzzle,
    symbol_types: &[String],
) -> HashMap<usize, usize> {
    let w = puzzle.width;
    let mut map = HashMap::new();
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(sym) = puzzle.cells[r][c].symbol.as_ref() {
                if let Some(ti) = symbol_types.iter().position(|t| t == sym) {
                    map.insert(r * w + c, ti);
                }
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cellset_basic() {
        let mut cs = CellSet::new(130); // >64 to cross a word
        assert!(cs.is_empty());
        cs.insert(0);
        cs.insert(100);
        assert!(cs.contains(0));
        assert!(cs.contains(100));
        assert_eq!(cs.len(), 2);
        let mut other = CellSet::new(130);
        other.insert(100);
        assert!(other.is_subset(&cs));
        assert!(!cs.is_subset(&other));
        assert!(cs.is_disjoint(&CellSet::new(130)));
        cs.remove(0);
        assert!(!cs.contains(0));
        let items: Vec<usize> = cs.iter().collect();
        assert_eq!(items, vec![100]);
    }

    #[test]
    fn edge_key_canonical() {
        assert_eq!(edge_key(1, 2, 1, 1), edge_key(1, 1, 1, 2));
        assert_eq!(edge_key(0, 0, 1, 0), edge_key(1, 0, 0, 0));
        assert_ne!(edge_key(0, 0, 1, 0), edge_key(0, 0, 0, 1));
    }
}
