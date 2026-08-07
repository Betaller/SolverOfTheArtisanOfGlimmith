use std::collections::HashMap;

// ── Compass clue ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CompassClue {
    pub up: Option<i64>,
    pub down: Option<i64>,
    pub left: Option<i64>,
    pub right: Option<i64>,
}

// ── Shape ─────────────────────────────────────────────────────────────────────

/// Free polyomino shape, normalized to origin (0,0).
pub type Shape = Vec<[usize; 2]>;

pub fn normalize(shape: &mut [[usize; 2]]) {
    let min_r = shape.iter().map(|xy| xy[0]).min().unwrap_or(0);
    let min_c = shape.iter().map(|xy| xy[1]).min().unwrap_or(0);
    for xy in shape.iter_mut() {
        xy[0] -= min_r;
        xy[1] -= min_c;
    }
    shape.sort();
}

pub fn canonical_key(shape: &[[usize; 2]]) -> String {
    let mut s = String::with_capacity(shape.len() * 8);
    for &[r, c] in shape {
        use std::fmt::Write;
        let _ = write!(s, "({},{})", r, c);
    }
    s
}

// ── Edge constraint ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EdgeConstraint {
    pub ctype: EdgeConstraintType,
    pub value: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeConstraintType {
    Heterogeneous,
    Homogeneous,
    Inequality,
    Difference,
}

impl EdgeConstraintType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "heterogeneous" => Some(Self::Heterogeneous),
            "homogeneous" => Some(Self::Homogeneous),
            "inequality" => Some(Self::Inequality),
            "difference" => Some(Self::Difference),
            _ => None,
        }
    }
}

// ── Cell ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub number: Option<i64>,
    pub symbol: Option<String>,
    pub blocked: bool,
    pub compass: Option<CompassClue>,
    pub fence_pattern: Option<Shape>,
    pub shape_pattern: Option<Shape>,
}

impl Cell {
    pub fn new(row: usize, col: usize) -> Self {
        Self {
            row,
            col,
            number: None,
            symbol: None,
            blocked: false,
            compass: None,
            fence_pattern: None,
            shape_pattern: None,
        }
    }

    pub fn fillable(&self) -> bool {
        !self.blocked
    }
}

// ── Edge ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Edge {
    pub is_boundary: bool,
    pub constraint: Option<EdgeConstraint>,
}

// ── Vertex ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Vertex {
    pub watchtower: Option<i64>,
}

// ── Rule ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Rule {
    pub ctype: String,
    pub params: HashMap<String, serde_json::Value>,
}

// ── Puzzle ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Puzzle {
    pub height: usize,
    pub width: usize,
    pub cells: Vec<Vec<Cell>>,
    /// Horizontal edges [r][c] between (r,c) and (r,c+1)
    pub h_edges: Vec<Vec<Edge>>,
    /// Vertical edges [r][c] between (r,c) and (r+1,c)
    pub v_edges: Vec<Vec<Edge>>,
    /// Vertices at absolute grid corners (r,c): r in 0..=height, c in 0..=width
    /// (border corners included).  A corner (r,c) is touched by the in-bounds
    /// cells {(r-1,c-1),(r-1,c),(r,c-1),(r,c)}.
    pub vertices: Vec<Vec<Vertex>>,
    pub rules: Vec<Rule>,
    pub shape_pool: Vec<Shape>,
    /// Outer border segments [r1, c1, r2, c2] that are explicitly recorded in
    /// the puzzle JSON. Kept for round-tripping (parsed from input, never read
    /// by the solvers — the outer border is always a region boundary).
    #[allow(dead_code)]
    pub outer_boundaries: Vec<[usize; 4]>,
}

// ── Solution ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RegionInfo {
    pub region_id: usize,
    pub cells: Vec<[usize; 2]>,
    pub area: usize,
    pub shape: Shape,
    pub normalized_shape_key: String,
    pub matched_shape_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub solved: bool,
    /// Always 0.  Kept for JSON protocol compatibility (Python reads it with a
    /// `0` default and the UI displays it); the Rust solver never counts steps.
    pub steps_taken: u64,
    pub elapsed_ms: u64,
    pub error_message: Option<String>,
    pub regions: Vec<RegionInfo>,
    pub rule_results: HashMap<String, bool>,
    /// Which solver module produced this result (aog / rose / pieces /
    /// backtrack).  Empty for errors, empty-grid, or timeout placeholders.
    /// Used by benchmark / verify tooling to attribute results to a module.
    pub solver: String,
}

impl Solution {
    pub fn unsolved(msg: impl Into<String>) -> Self {
        Self {
            solved: false,
            steps_taken: 0,
            elapsed_ms: 0,
            error_message: Some(msg.into()),
            regions: Vec::new(),
            rule_results: HashMap::new(),
            solver: String::new(),
        }
    }
}

// ── Neighbor helpers ──────────────────────────────────────────────────────────

pub fn neighbor_positions(r: usize, c: usize, h: usize, w: usize) -> Vec<(usize, usize)> {
    let mut v = Vec::with_capacity(4);
    if r > 0 {
        v.push((r - 1, c));
    }
    if r + 1 < h {
        v.push((r + 1, c));
    }
    if c > 0 {
        v.push((r, c - 1));
    }
    if c + 1 < w {
        v.push((r, c + 1));
    }
    v
}
