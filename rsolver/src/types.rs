use std::collections::HashMap;

// ── Directions ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

// ── Compass clue ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CompassClue {
    pub up: Option<i64>,
    pub down: Option<i64>,
    pub left: Option<i64>,
    pub right: Option<i64>,
}

impl CompassClue {
    pub fn get(&self, d: Direction) -> Option<i64> {
        match d {
            Direction::Up => self.up,
            Direction::Down => self.down,
            Direction::Left => self.left,
            Direction::Right => self.right,
        }
    }
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
    pub region_id: Option<usize>,
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
            region_id: None,
        }
    }

    pub fn assigned(&self) -> bool {
        self.region_id.is_some()
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
    /// Internal vertices at (r,c) for cell corners
    pub vertices: Vec<Vec<Vertex>>,
    pub rules: Vec<Rule>,
    pub shape_pool: Vec<Shape>,
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
    pub steps_taken: u64,
    pub elapsed_ms: u64,
    pub error_message: Option<String>,
    pub regions: Vec<RegionInfo>,
    pub rule_results: HashMap<String, bool>,
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
