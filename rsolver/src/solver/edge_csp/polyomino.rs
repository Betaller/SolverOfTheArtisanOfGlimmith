//! Polyomino shape canonicalization — ported from `third_party/aog/src/polyomino.rs`.
//!
//! Only the subset needed by `propagate_shape_constraints` (gemini/delta shape
//! identity) is included: `make_shape`, `canonical`, `normalize`.  A canonical
//! shape is the lexicographically-smallest of the 8 dihedral (rotation +
//! reflection) transforms, so two regions have the "same normalized shape" iff
//! their canonical forms are equal.

use super::types::Shape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn all() -> [Self; 4] {
        [Self::R0, Self::R90, Self::R180, Self::R270]
    }

    pub fn transform(self, r: i32, c: i32) -> (i32, i32) {
        match self {
            Self::R0 => (r, c),
            Self::R90 => (c, -r),
            Self::R180 => (-r, -c),
            Self::R270 => (-c, r),
        }
    }
}

/// Shift cells so the minimum row/column is 0, then sort.
pub fn normalize(cells: &[(i32, i32)]) -> Vec<(i32, i32)> {
    if cells.is_empty() {
        return vec![];
    }
    let min_r = cells.iter().map(|&(r, _)| r).min().unwrap();
    let min_c = cells.iter().map(|&(_, c)| c).min().unwrap();
    let mut out: Vec<_> = cells.iter().map(|&(r, c)| (r - min_r, c - min_c)).collect();
    out.sort();
    out
}

pub fn make_shape(cells: &[(i32, i32)]) -> Shape {
    let cells = normalize(cells);
    if cells.is_empty() {
        return Shape::default();
    }
    let max_r = cells.iter().map(|&(r, _)| r).max().unwrap();
    let max_c = cells.iter().map(|&(_, c)| c).max().unwrap();
    Shape {
        height: max_r + 1,
        width: max_c + 1,
        cells,
    }
}

fn rotate(cells: &[(i32, i32)], rot: Rotation, flip: bool) -> Vec<(i32, i32)> {
    cells
        .iter()
        .map(|&(r, c)| {
            let (nr, nc) = rot.transform(r, c);
            if flip {
                (nr, -nc)
            } else {
                (nr, nc)
            }
        })
        .collect()
}

/// Lexicographically smallest of all 8 dihedral transforms (the canonical form).
pub fn canonical(s: &Shape) -> Shape {
    let mut best = s.clone();
    for rot in Rotation::all() {
        for flip in [false, true] {
            let cand = make_shape(&rotate(&s.cells, rot, flip));
            if cand < best {
                best = cand;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_idempotent() {
        // L-tromino and its rotations all map to the same canonical form.
        let l = make_shape(&[(0, 0), (1, 0), (2, 0)]);
        assert_eq!(canonical(&canonical(&l)), canonical(&l));
        let rot = make_shape(&rotate(&l.cells, Rotation::R90, false));
        assert_eq!(canonical(&rot), canonical(&l));
    }

    #[test]
    fn canonical_rotation_invariant() {
        let s = make_shape(&[(0, 0), (0, 1), (1, 1)]); // S/Z tetromino-ish
        for rot in Rotation::all() {
            let r = make_shape(&rotate(&s.cells, rot, false));
            assert_eq!(canonical(&r), canonical(&s));
        }
    }
}
