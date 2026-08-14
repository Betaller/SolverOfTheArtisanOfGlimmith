//! Edge-variable CSP solver types.
//!
//! Ported from `third_party/aog/src/types.rs`, slimmed to the rules where the
//! edge-variable paradigm has an advantage (ring / brick / watchtower /
//! compass / inequality / difference).  Shape- and rose-oriented clue types
//! (Polyomino / Palisade / Rose) are deliberately omitted — those are covered
//! by the existing aog / pieces / rose solvers and are enforced on any
//! candidate solution by the router's `validate::validate` gate.

pub type CellId = usize;
pub type EdgeId = usize;
pub type VertexId = usize;

/// Three-state edge variable: undecided, or forced to a boundary (`Cut`) or an
/// internal edge (`Uncut`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeState {
    Unknown,
    Cut,
    Uncut,
}

/// Compass clue: how many piece cells lie in each direction relative to the
/// clued cell.  `n`/`s`/`e`/`w` correspond to north (smaller row), south
/// (larger row), east (larger col), west (smaller col).  A `None` direction
/// carries no information; the clued cell itself is never counted.
#[derive(Clone, Copy, Debug, Default)]
pub struct CompassData {
    pub n: Option<usize>,
    pub s: Option<usize>,
    pub e: Option<usize>,
    pub w: Option<usize>,
}

/// Fence (palisade) clue: how the 4 edges around a cell are cut, up to rotation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PalisadeKind {
    /// p0: no edges cut.
    None,
    /// p1: exactly one edge cut.
    One,
    /// p=: two opposite edges cut.
    Opposite,
    /// p2: two adjacent edges cut.
    Adjacent,
    /// p3: three edges cut.
    Three,
    /// p4: all four edges cut.
    All,
}

impl PalisadeKind {
    pub fn cut_count(self) -> usize {
        match self {
            Self::None => 0,
            Self::One => 1,
            Self::Opposite | Self::Adjacent => 2,
            Self::Three => 3,
            Self::All => 4,
        }
    }

    /// Returns `(expected_cut_count, edge_mask)` for a given rotation.
    /// Mask bits follow `cell_edges` order: north=0, south=1, west=2, east=3.
    /// Rotations go clockwise (north → east → south → west), so the cycle in bit
    /// positions is [0, 3, 1, 2].
    pub fn pattern_at_rotation(self, rot: usize) -> (usize, u8) {
        const CYCLE: [u8; 4] = [0, 3, 1, 2];
        let bit = |i: usize| -> u8 { 1 << CYCLE[i % 4] };
        match self {
            Self::None => (0, 0),
            Self::One => (1, bit(rot)),
            Self::Opposite => (2, bit(rot) | bit(rot + 2)),
            Self::Adjacent => (2, bit(rot) | bit(rot + 1)),
            Self::Three => (3, 0xF & !bit(rot)),
            Self::All => (4, 0xF),
        }
    }
}

/// Cell-level clue (at most one per cell in the source model).
#[derive(Clone, Debug)]
pub enum CellClue {
    Area { cell: CellId, value: usize },
    Compass { cell: CellId, compass: CompassData },
    Palisade { cell: CellId, kind: PalisadeKind },
}

impl CellClue {
    pub fn cell(&self) -> CellId {
        match self {
            CellClue::Area { cell, .. }
            | CellClue::Compass { cell, .. }
            | CellClue::Palisade { cell, .. } => *cell,
        }
    }
}

/// Semantic meaning of a (necessarily cut) edge clue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeClueKind {
    /// `smaller_first` = the cell with the smaller CellId has the smaller area.
    Inequality { smaller_first: bool },
    /// The two adjacent pieces differ in area by exactly `value` (unsigned).
    Diff { value: usize },
}

#[derive(Clone, Debug)]
pub struct EdgeClue {
    pub edge: EdgeId,
    pub kind: EdgeClueKind,
}

/// Vertex-level clue (watchtower: number of distinct pieces meeting there).
#[derive(Clone, Debug)]
pub struct VertexClue {
    pub vertex: VertexId,
    // Read once watchtower propagation lands (iteration 2); collected now for
    // the `watchtower_vertices` edge-selection cache.
    #[allow(dead_code)]
    pub value: usize,
}

/// Global rules relevant to the edge-variable propagators.
///
/// Global area bounds (`precise`/`range`) are carried separately on the solver
/// as `eff_min_area`/`eff_max_area` (per-component compass minima are computed
/// during `build_components`), so they are not repeated here.
#[derive(Clone, Debug, Default)]
pub struct GlobalRules {
    pub bricky: bool,
    pub loopy: bool,
    // Read once size-separation propagation lands (iteration 2).
    #[allow(dead_code)]
    pub size_separation: bool,
}
