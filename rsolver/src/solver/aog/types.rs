//! Constants and shared data types for the AoG-style DFS solver.

use std::cell::RefCell;

// ── Constants (from defines.h) ───────────────────────────────────────────────

pub const LINE_NORMAL: u32 = 0x0000_0000;
pub const LINE_BLOCK: u32 = 0x8000_0000;
pub const LINE_DIFFERENT: u32 = 0x4000_0000;
pub const LINE_EQUAL: u32 = 0x2000_0000;
pub const LINE_SMALLER: u32 = 0x1000_0000;
pub const LINE_LARGER: u32 = 0x0800_0000;
pub const LINE_SIZE_DIFF_BIT: u32 = 0x000f_0000;
pub const LINE_SIZE_DIFF_BIT_SHIFT: u32 = 16;

pub const AREA_NORMAL: u32 = 0x0000_0000;
pub const AREA_BLOCK: u32 = 0x8000_0000;
pub const AREA_PALISADE_INDEX_BIT: u32 = 0x7000_0000;
pub const AREA_PALISADE_INDEX_BIT_SHIFT: u32 = 28;
pub const AREA_SHAPE_INDEX_BIT: u32 = 0x0f00_0000;
pub const AREA_SHAPE_INDEX_BIT_SHIFT: u32 = 24;
pub const AREA_SHAPE_SIZE_BIT: u32 = 0x00ff_0000;
pub const AREA_SHAPE_SIZE_BIT_SHIFT: u32 = 16;
pub const AREA_SLASH_INDEX_BIT: u32 = 0x0000_f000;
pub const AREA_SLASH_INDEX_BIT_SHIFT: u32 = 12;
pub const AREA_COMPASS_ENABLE: u32 = 0x0000_0800;
/// Custom bit: cell carries a symbol string (JSON-only marker for symbols).
pub const AREA_SYMBOL_BIT: u32 = 0x0000_0004;

pub const SOLVE_AREA_SHAPE_INDEX_BIT: u32 = 0xffff_0000;
pub const SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT: u32 = 16;
pub const SOLVE_AREA_BIT: u32 = 0x0000_ffff;

pub const VERTEX_RADAR_BIT: u32 = 0x0000_000f;
pub const VERTEX_RADAR_BIT_SHIFT: u32 = 0;

pub const SPECIAL_START_DEFAULT: u32 = 0;
pub const SPECIAL_START_SIZE_1_REGION: u32 = 1;
pub const SPECIAL_START_SIZE_MATCH_REGION: u32 = 2;
pub const SPECIAL_START_LINE_SAME: u32 = 3;
pub const SPECIAL_START_LINE_SMALLER_OR_LARGER: u32 = 4;
pub const SPECIAL_START_AREA_INDEX: u32 = 5;
pub const SPECIAL_START_AREA_SIZE: u32 = 6;
pub const SPECIAL_START_CORNER: u32 = 7;
pub const SPECIAL_START_COMPASS: u32 = 8;
pub const SPECIAL_START_LINE_CONSTRAINT: u32 = 9;
pub const SPECIAL_START_LINE_SIZE_DIFF: u32 = 10;

pub const MAX_SHAPE_SIZE: usize = 256;
pub const MAX_DFS_DEPTH: usize = 100;
pub const MAX_EXPAND_CANDIDATES: usize = (MAX_SHAPE_SIZE + 2) * 3;
/// DFS work-stack depth: one frame per shape cell plus slack (dfs.cpp sizes
/// this array with MAX_SHAPE_SIZE but silently overflows for large regions).
pub const MAX_STACK_SIZE: usize = MAX_SHAPE_SIZE + 2;

pub const NO_SHAPE_INDEX: u32 = 0xffff_ffff;

// ── Small types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Node {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Default)]
pub struct CompassStates {
    pub up: i32,
    pub down: i32,
    pub left: i32,
    pub right: i32,
}

#[derive(Clone)]
pub struct Shape {
    pub shape_index: u32,
    pub nodes: Vec<Node>,
    pub digest: u32,
    pub preview: Vec<u32>,
}

impl Shape {
    /// Build a shape from a `size x size` 0/1 grid (matching the C++ constructor).
    pub fn from_grid(grid: &[Vec<u32>], shape_size: usize) -> Shape {
        let mut shape = Shape {
            shape_index: 0,
            nodes: Vec::new(),
            digest: 0,
            preview: Vec::new(),
        };
        let mut start_x: i32 = -1;
        let mut start_y: i32 = -1;
        let mut most_left_j: i32 = shape_size as i32;
        let mut most_up_i: i32 = shape_size as i32;
        for i in 0..shape_size {
            let mut line_preview: u32 = 0;
            for j in 0..shape_size {
                line_preview <<= 1;
                if grid[i][j] == 1 {
                    most_left_j = most_left_j.min(j as i32);
                    most_up_i = most_up_i.min(i as i32);
                    if start_x == -1 {
                        start_x = i as i32;
                        start_y = j as i32;
                    }
                    shape.nodes.push(Node {
                        x: i as i32 - start_x,
                        y: j as i32 - start_y,
                    });
                    line_preview += 1;
                }
            }
            shape.preview.push(line_preview);
        }
        for i in 0..shape.preview.len() {
            shape.preview[i] <<= most_left_j;
        }
        let len = shape.preview.len() as i32;
        for i in most_up_i..len {
            shape.preview[(i - most_up_i) as usize] = shape.preview[i as usize];
        }
        for i in 0..most_up_i {
            shape.preview[(i + len - most_up_i) as usize] = 0;
        }
        for i in 0..shape.preview.len() {
            shape.digest = shape.digest.wrapping_mul(131).wrapping_add(shape.preview[i]);
        }
        shape
    }

    pub fn eq_shape(&self, other: &Shape) -> bool {
        if self.digest != other.digest {
            return false;
        }
        if self.preview.len() != other.preview.len() {
            return false;
        }
        for i in 0..self.preview.len() {
            if self.preview[i] != other.preview[i] {
                return false;
            }
        }
        true
    }
}

// ── Config ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct Config {
    pub only_rectangles: bool,
    pub no_rectangles: bool,
    pub adjacent_shapes_different: bool,
    pub adjacent_sizes_different: bool,
    pub all_shapes_different: bool,
    pub all_shapes_same: bool,
    pub one_symbol_per_region: bool,
    pub predefine_shapes_only: bool,
    pub no_4_way_intersections: bool,
    pub no_3_way_intersections: bool,
    pub shape_size_lower_bound: i32,
    pub shape_size_upper_bound: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            only_rectangles: false,
            no_rectangles: false,
            adjacent_shapes_different: false,
            adjacent_sizes_different: false,
            all_shapes_different: false,
            all_shapes_same: false,
            one_symbol_per_region: false,
            predefine_shapes_only: false,
            no_4_way_intersections: false,
            no_3_way_intersections: false,
            shape_size_lower_bound: -1,
            shape_size_upper_bound: -1,
        }
    }
}

// ── DFS context ─────────────────────────────────────────────────────────────

pub struct DfsContext {
    pub visited: Vec<Vec<i32>>,
    pub visited_index: i32,
    pub empty_count: usize,
    pub empty_block_line_node_pairs: std::collections::BTreeSet<(Node, Node)>,
    pub empty_block_line_count: usize,
    pub symbol_count: usize,
    pub slash_count: [i32; 10],
    pub compass_nodes: Vec<Node>,
    pub compass_node_states: Vec<CompassStates>,
    pub area_shape_sizes: Vec<usize>,
    pub group_mark_index: i32,
    pub block_adj: std::collections::HashMap<u64, Vec<Node>>,
    pub place_visited: std::collections::HashMap<u64, i32>,
}

impl DfsContext {
    pub fn new(n_row: usize, n_col: usize) -> Self {
        Self {
            visited: vec![vec![0; 2 * n_col + 5]; 2 * n_row + 5],
            visited_index: 0,
            empty_count: 0,
            empty_block_line_node_pairs: std::collections::BTreeSet::new(),
            empty_block_line_count: 0,
            symbol_count: 0,
            slash_count: [0; 10],
            compass_nodes: Vec::new(),
            compass_node_states: Vec::new(),
            area_shape_sizes: Vec::new(),
            group_mark_index: 0,
            block_adj: std::collections::HashMap::new(),
            place_visited: std::collections::HashMap::new(),
        }
    }
}

// ── Per-level pools (RefCell so disjoint levels can be borrowed independently) ──

#[derive(Clone)]
pub struct PlaceLevel {
    pub current_shape: [Node; MAX_SHAPE_SIZE],
    pub current_shape_cnt: usize,
    pub expand_candidates: [Node; MAX_EXPAND_CANDIDATES],
    pub expand_candidates_distance: [i32; MAX_EXPAND_CANDIDATES],
    pub expand_candidates_cnt: usize,
    pub rectangle_up: [i32; MAX_SHAPE_SIZE],
    pub rectangle_down: [i32; MAX_SHAPE_SIZE],
    pub rectangle_left: [i32; MAX_SHAPE_SIZE],
    pub rectangle_right: [i32; MAX_SHAPE_SIZE],
    pub palisade_visited: [Node; MAX_SHAPE_SIZE],
    pub palisade_visited_cnt: usize,
    pub compass_visited: [Node; MAX_SHAPE_SIZE],
    pub compass_visited_up_cnt: [i32; MAX_SHAPE_SIZE],
    pub compass_visited_down_cnt: [i32; MAX_SHAPE_SIZE],
    pub compass_visited_left_cnt: [i32; MAX_SHAPE_SIZE],
    pub compass_visited_right_cnt: [i32; MAX_SHAPE_SIZE],
    pub compass_visited_cnt: usize,
    pub stack_size: [usize; MAX_STACK_SIZE],
    pub stack_expand_distance_lb: [i32; MAX_STACK_SIZE],
    pub stack_expand_x_lb: [i32; MAX_STACK_SIZE],
    pub stack_expand_y_lb: [i32; MAX_STACK_SIZE],
    pub stack_candidates_i: [usize; MAX_STACK_SIZE],
    pub stack_candidates_size: [usize; MAX_STACK_SIZE],
    pub stack_top: usize,
    pub symbol_loc: Option<Node>,
    pub mark_slash: [bool; 16],
    pub slash_node_indexs: [usize; 16],
    pub slash_dist_buf: Vec<i32>,
}

impl PlaceLevel {
    pub fn new() -> Self {
        Self {
            current_shape: [Node::default(); MAX_SHAPE_SIZE],
            current_shape_cnt: 0,
            expand_candidates: [Node::default(); MAX_EXPAND_CANDIDATES],
            expand_candidates_distance: [0; MAX_EXPAND_CANDIDATES],
            expand_candidates_cnt: 0,
            rectangle_up: [0; MAX_SHAPE_SIZE],
            rectangle_down: [0; MAX_SHAPE_SIZE],
            rectangle_left: [0; MAX_SHAPE_SIZE],
            rectangle_right: [0; MAX_SHAPE_SIZE],
            palisade_visited: [Node::default(); MAX_SHAPE_SIZE],
            palisade_visited_cnt: 0,
            compass_visited: [Node::default(); MAX_SHAPE_SIZE],
            compass_visited_up_cnt: [0; MAX_SHAPE_SIZE],
            compass_visited_down_cnt: [0; MAX_SHAPE_SIZE],
            compass_visited_left_cnt: [0; MAX_SHAPE_SIZE],
            compass_visited_right_cnt: [0; MAX_SHAPE_SIZE],
            compass_visited_cnt: 0,
            stack_size: [0; MAX_STACK_SIZE],
            stack_expand_distance_lb: [0; MAX_STACK_SIZE],
            stack_expand_x_lb: [0; MAX_STACK_SIZE],
            stack_expand_y_lb: [0; MAX_STACK_SIZE],
            stack_candidates_i: [0; MAX_STACK_SIZE],
            stack_candidates_size: [0; MAX_STACK_SIZE],
            stack_top: 0,
            symbol_loc: None,
            mark_slash: [false; 16],
            slash_node_indexs: [0; 16],
            slash_dist_buf: Vec::new(),
        }
    }
}

pub struct Pools {
    pub mark_skip: Vec<RefCell<Vec<bool>>>,
    pub mark_size: Vec<RefCell<Vec<bool>>>,
    pub place: Vec<RefCell<PlaceLevel>>,
}

impl Pools {
    pub fn new(depth: usize) -> Self {
        let mut mark_skip = Vec::new();
        let mut mark_size = Vec::new();
        let mut place = Vec::new();
        for _ in 0..depth {
            mark_skip.push(RefCell::new(Vec::new()));
            mark_size.push(RefCell::new(Vec::new()));
            place.push(RefCell::new(PlaceLevel::new()));
        }
        Self {
            mark_skip,
            mark_size,
            place,
        }
    }
}

// ── Coordinate helpers ───────────────────────────────────────────────────────

#[inline]
pub fn to_puzzle_x(x: i32) -> i32 {
    (x << 1) + 1
}
#[inline]
pub fn to_puzzle_y(y: i32) -> i32 {
    (y << 1) + 1
}

/// Symbol type index encoded in AREA_SLASH_INDEX_BIT (rose_window).
#[inline]
pub fn symbol_type_idx(pv: u32) -> usize {
    ((pv & AREA_SLASH_INDEX_BIT) >> AREA_SLASH_INDEX_BIT_SHIFT) as usize
}

#[inline]
pub fn encode_node(x: i32, y: i32) -> u64 {
    ((x as u32 as u64) << 32) | (y as u32 as u64)
}
