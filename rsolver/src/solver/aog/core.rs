//! Core solver state: grid construction, shape catalog, and constraint checks.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::types::*;
use super::types::*;
use super::types::{Config, DfsContext, Node, Shape};

// ── Core solver state (everything except per-level pools) ───────────────────

pub struct AoGCore {
    pub n_row: usize,
    pub n_col: usize,
    pub config: Config,
    pub puzzle: Vec<Vec<u32>>,
    pub puzzle_compass_up: Vec<Vec<i32>>,
    pub puzzle_compass_down: Vec<Vec<i32>>,
    pub puzzle_compass_left: Vec<Vec<i32>>,
    pub puzzle_compass_right: Vec<Vec<i32>>,
    pub slash_nodes: Vec<Vec<Node>>, // 1-indexed: [0] unused
    pub shape_size_nodes: Vec<Node>,
    pub all_shapes_same_check_shape_index: i32,
    pub all_shapes_different_check_shape_index_pool: HashSet<u32>,
    pub shapes: Vec<Shape>,
    pub shape_size_by_index: Vec<usize>,
    pub shape_digest_index: HashMap<u32, Vec<usize>>,
    pub node_to_shape_index: HashMap<(i32, i32), Vec<usize>>,
    pub next_shape_index: u32,
    /// Hard cap on `shapes.len()`. 0 = unlimited. See `DEFAULT_SHAPE_CAP` /
    /// `AOG_SHAPE_CAP` env. When hit, `shapes_insert` refuses new shapes and
    /// the caller skips the placement, bounding memory to avoid OOM.
    pub shape_cap: usize,
    pub dfs_ctx: DfsContext,
    pub deadline: Instant,
    /// rose_window: number of distinct symbol types (0 = not rose_window).
    pub rose_type_count: usize,
}

// ── Shape grid helpers ───────────────────────────────────────────────────────

pub fn shape_rotate(grid: &mut Vec<Vec<u32>>, shape_size: usize) {
    let n = shape_size;
    if n == 0 {
        return;
    }
    for layer in 0..n / 2 {
        let first = layer;
        let last = n - 1 - layer;
        for i in first..last {
            let offset = i - first;
            let top = grid[first][i];
            grid[first][i] = grid[last - offset][first];
            grid[last - offset][first] = grid[last][last - offset];
            grid[last][last - offset] = grid[i][last];
            grid[i][last] = top;
        }
    }
}

pub fn shape_mirror(grid: &mut Vec<Vec<u32>>, shape_size: usize) {
    let n = shape_size;
    for i in 0..n {
        for j in 0..n / 2 {
            grid[i].swap(j, n - 1 - j);
        }
    }
}

/// Convert a cell-coordinate list into a square 0/1 grid (minimal bounding square).
pub fn shape_grid_from_cells(cells: &[[usize; 2]]) -> (Vec<Vec<u32>>, usize) {
    if cells.is_empty() {
        return (vec![vec![0u32; 1]], 1);
    }
    let mut min_r = usize::MAX;
    let mut min_c = usize::MAX;
    for &[r, c] in cells {
        min_r = min_r.min(r);
        min_c = min_c.min(c);
    }
    let mut max_r = 0usize;
    let mut max_c = 0usize;
    for &[r, c] in cells {
        max_r = max_r.max(r - min_r);
        max_c = max_c.max(c - min_c);
    }
    let size = max_r.max(max_c) + 1;
    let mut grid = vec![vec![0u32; size]; size];
    for &[r, c] in cells {
        grid[r - min_r][c - min_c] = 1;
    }
    (grid, size)
}

// ── Shape catalog operations ─────────────────────────────────────────────────

impl AoGCore {
    fn compute_digest(&self, shape: &[Vec<u32>], shape_size: usize) -> u32 {
        // Replicates shapes.cpp compute_digest exactly.
        let n = shape_size;
        let mut preview = vec![0u32; n];
        let mut most_left_j = n as i32;
        let mut most_up_i = n as i32;
        for i in 0..n {
            let mut line: u32 = 0;
            for j in 0..n {
                line <<= 1;
                if shape[i][j] == 1 {
                    most_left_j = most_left_j.min(j as i32);
                    most_up_i = most_up_i.min(i as i32);
                    line += 1;
                }
            }
            preview[i] = line;
        }
        for i in 0..n {
            preview[i] <<= most_left_j;
        }
        for i in most_up_i..n as i32 {
            preview[(i - most_up_i) as usize] = preview[i as usize];
        }
        for i in 0..most_up_i {
            preview[(i + n as i32 - most_up_i) as usize] = 0;
        }
        let mut d: u32 = 0;
        for i in 0..n {
            d = d.wrapping_mul(131).wrapping_add(preview[i]);
        }
        d
    }

    pub fn shapes_search(&self, shape: &[Vec<u32>], shape_size: usize) -> u32 {
        let d = self.compute_digest(shape, shape_size);
        if let Some(indices) = self.shape_digest_index.get(&d) {
            let cand = Shape::from_grid(shape, shape_size);
            for &idx in indices {
                if self.shapes[idx].eq_shape(&cand) {
                    return self.shapes[idx].shape_index;
                }
            }
        }
        NO_SHAPE_INDEX
    }

    fn add_shape_to_shapes(&mut self, shape_index: u32, shape: &[Vec<u32>], shape_size: usize) -> bool {
        if self.shapes_search(shape, shape_size) == NO_SHAPE_INDEX {
            let mut s = Shape::from_grid(shape, shape_size);
            s.shape_index = shape_index;
            let idx = self.shapes.len();
            let nodes = s.nodes.clone();
            self.shapes.push(s);
            self.shape_digest_index
                .entry(self.shapes[idx].digest)
                .or_default()
                .push(idx);
            for i in 0..nodes.len() {
                let nd = nodes[i];
                self.node_to_shape_index
                    .entry((nd.x, nd.y))
                    .or_default()
                    .push(idx);
            }
            true
        } else {
            false
        }
    }

    pub fn shapes_insert(&mut self, shape: &mut Vec<Vec<u32>>, shape_size: usize) -> u32 {
        if self.shape_cap > 0 && self.shapes.len() >= self.shape_cap {
            // Library full: refuse ALL 8 dihedral variants atomically (not
            // partially — a mid-loop cap in `add_shape_to_shapes` would insert
            // some rotations and reject others, breaking dihedral symmetry and
            // causing `shapes_search` to miss the un-inserted orientations).
            // Caller then `shapes_search`es: if the shape already exists the
            // placement proceeds normally; if not, it gets `NO_SHAPE_INDEX` and
            // skips the placement. We never allocate a new `shape_index` here —
            // doing so would shift indices and break `AREA_SHAPE_INDEX_BIT`.
            return 0;
        }
        let mut insert_success_count = 0u32;
        for _ in 0..4 {
            insert_success_count += self.add_shape_to_shapes(self.next_shape_index, shape, shape_size) as u32;
            shape_rotate(shape, shape_size);
        }
        shape_mirror(shape, shape_size);
        for _ in 0..4 {
            insert_success_count += self.add_shape_to_shapes(self.next_shape_index, shape, shape_size) as u32;
            shape_rotate(shape, shape_size);
        }
        if insert_success_count != 0 {
            let size = self.shapes[self.shapes.len() - 1].nodes.len();
            self.shape_size_by_index.push(size);
            self.next_shape_index += 1;
        }
        if crate::aog_debug_enabled() && insert_success_count != 0 {
            let last = &self.shapes[self.shapes.len() - 1];
            eprintln!(
                "aog shape_insert idx={} nodes={:?} digest={} size={}",
                last.shape_index, last.nodes, last.digest, shape_size
            );
        }
        insert_success_count
    }

    pub fn make_solve_puzzle(&self) -> Vec<Vec<u32>> {
        let mut sp = vec![vec![LINE_BLOCK as u32; 2 * self.n_col + 5]; 2 * self.n_row + 5];
        for i in 1..=(2 * self.n_row as i32 + 1) {
            for j in 1..=(2 * self.n_col as i32 + 1) {
                let v = self.puzzle[i as usize][j as usize];
                sp[i as usize][j as usize] = if v == AREA_BLOCK { AREA_BLOCK } else { AREA_NORMAL };
            }
        }
        sp
    }
}

// ── Board queries (checks.cpp equivalents) ───────────────────────────────────

impl AoGCore {
    pub fn area_in_puzzle_range(&self, x: i32, y: i32) -> bool {
        x > 2 && x < (self.n_row as i32 * 2) + 3 && y > 2 && y < (self.n_col as i32 * 2) + 3
    }

    pub fn area_contain_symbol(&self, x: i32, y: i32) -> bool {
        let pv = self.puzzle[to_puzzle_x(x) as usize][to_puzzle_y(y) as usize];
        (pv & (AREA_PALISADE_INDEX_BIT | AREA_SLASH_INDEX_BIT | AREA_SHAPE_INDEX_BIT
            | AREA_SHAPE_SIZE_BIT | AREA_COMPASS_ENABLE | AREA_SYMBOL_BIT)) != 0
    }
}

pub fn sp_at(sp: &Vec<Vec<u32>>, x: i32, y: i32) -> u32 {
    sp[x as usize][y as usize]
}

/// check_nearby_shape: adjacent regions must have different shapes.
pub fn check_nearby_shape(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    for (dx, dy) in [(-2, 0), (2, 0), (0, -2), (0, 2)] {
        if !core.area_in_puzzle_range(x + dx, y + dy) {
            continue;
        }
        let v = sp_at(sp, x + dx, y + dy);
        if v != AREA_NORMAL
            && v != index
            && (v & SOLVE_AREA_SHAPE_INDEX_BIT) == (index & SOLVE_AREA_SHAPE_INDEX_BIT)
        {
            return false;
        }
    }
    true
}

/// check_nearby_size: adjacent regions must have different sizes.
pub fn check_nearby_size(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    let my_key = (index & SOLVE_AREA_SHAPE_INDEX_BIT) >> SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT;
    let my_size = core.shape_size_by_index[my_key as usize];
    for (dx, dy) in [(-2, 0), (2, 0), (0, -2), (0, 2)] {
        if !core.area_in_puzzle_range(x + dx, y + dy) {
            continue;
        }
        let v = sp_at(sp, x + dx, y + dy);
        if v != AREA_NORMAL && v != AREA_BLOCK && v != index {
            let nearby_key = (v & SOLVE_AREA_SHAPE_INDEX_BIT) >> SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT;
            let nearby_size = core.shape_size_by_index[nearby_key as usize];
            if my_size == nearby_size {
                return false;
            }
        }
    }
    true
}

/// check_edge_shape: line constraints between adjacent regions.
pub fn check_edge_shape(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    let my_key = (index & SOLVE_AREA_SHAPE_INDEX_BIT) >> SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT;
    // (ax, ay) = neighbour step, (lx, ly) = line-cell offset, up_left = is neighbour on the upper/left side
    let dirs = [
        (-2, 0, -1, 0, true),
        (2, 0, 1, 0, false),
        (0, -2, 0, -1, true),
        (0, 2, 0, 1, false),
    ];
    for (ax, ay, lx, ly, up_left) in dirs {
        if !core.area_in_puzzle_range(x + ax, y + ay) {
            continue;
        }
        let nv = sp_at(sp, x + ax, y + ay);
        if nv == AREA_NORMAL || nv == AREA_BLOCK {
            continue;
        }
        let lv = core.puzzle[(x + lx) as usize][(y + ly) as usize];
        let nkey = (nv & SOLVE_AREA_SHAPE_INDEX_BIT) >> SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT;
        let my_size = core.shape_size_by_index[my_key as usize];
        let n_size = core.shape_size_by_index[nkey as usize];
        if (lv & LINE_EQUAL) != 0 && nkey != my_key {
            return false;
        }
        if (lv & LINE_DIFFERENT) != 0 && nkey == my_key {
            return false;
        }
        if (lv & (LINE_LARGER | LINE_SMALLER | LINE_SIZE_DIFF_BIT)) != 0 {
            let (left_size, right_size) = if up_left {
                (n_size, my_size)
            } else {
                (my_size, n_size)
            };
            if (lv & LINE_LARGER) != 0 && left_size <= right_size {
                return false;
            }
            if (lv & LINE_SMALLER) != 0 && left_size >= right_size {
                return false;
            }
            if (lv & LINE_SIZE_DIFF_BIT) != 0 {
                let diff_val = ((lv & LINE_SIZE_DIFF_BIT) >> LINE_SIZE_DIFF_BIT_SHIFT) as i32 - 1;
                if left_size.abs_diff(right_size) != diff_val as usize {
                    return false;
                }
            }
        }
    }
    true
}

/// check_edge: same-region cells must not cross a LINE_BLOCK.
pub fn check_edge(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    let dirs = [(-2, 0, -1, 0), (2, 0, 1, 0), (0, -2, 0, -1), (0, 2, 0, 1)];
    for (ax, ay, lx, ly) in dirs {
        if !core.area_in_puzzle_range(x + ax, y + ay) {
            continue;
        }
        if sp_at(sp, x + ax, y + ay) == index
            && (core.puzzle[(x + lx) as usize][(y + ly) as usize] & LINE_BLOCK) != 0
        {
            return false;
        }
    }
    true
}

pub fn check_palisade_type2(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    let palisade_type =
        (core.puzzle[x as usize][y as usize] & AREA_PALISADE_INDEX_BIT) >> AREA_PALISADE_INDEX_BIT_SHIFT;
    let mut up = false;
    let mut down = false;
    let mut left = false;
    let mut right = false;
    for (ax, ay, flag) in [(-2, 0, 0), (2, 0, 1), (0, -2, 2), (0, 2, 3)] {
        if core.area_in_puzzle_range(x + ax, y + ay) && sp_at(sp, x + ax, y + ay) == index {
            match flag {
                0 => up = true,
                1 => down = true,
                2 => left = true,
                _ => right = true,
            }
        }
    }
    let sum = (up as i32) + (down as i32) + (left as i32) + (right as i32);
    match palisade_type {
        1 => up && down && left && right,
        2 => sum == 3,
        3 => sum == 2 && ((up && down) || (left && right)),
        4 => sum == 1,
        5 => sum == 2 && ((up && right) || (right && down) || (down && left) || (left && up)),
        6 => sum == 0,
        _ => true,
    }
}

pub fn check_palisade_type1(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    let palisade_type =
        (core.puzzle[x as usize][y as usize] & AREA_PALISADE_INDEX_BIT) >> AREA_PALISADE_INDEX_BIT_SHIFT;
    let mut up = false;
    let mut down = false;
    let mut left = false;
    let mut right = false;
    for (ax, ay, flag) in [(-2, 0, 0), (2, 0, 1), (0, -2, 2), (0, 2, 3)] {
        if core.area_in_puzzle_range(x + ax, y + ay) && sp_at(sp, x + ax, y + ay) == index {
            match flag {
                0 => up = true,
                1 => down = true,
                2 => left = true,
                _ => right = true,
            }
        }
    }
    let sum = (up as i32) + (down as i32) + (left as i32) + (right as i32);
    match palisade_type {
        2 => sum <= 3,
        3 => sum <= 2 && (sum != 2 || ((up && down) || (left && right))),
        4 => sum <= 1,
        5 => sum <= 2 && (sum != 2 || ((up && right) || (right && down) || (down && left) || (left && up))),
        6 => sum == 0,
        _ => true,
    }
}

/// check_tatami: no 4-way intersections (brick).
pub fn check_tatami(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    for (cx, cy) in [(-2, -2), (-2, 2), (2, -2), (2, 2)] {
        if !core.area_in_puzzle_range(x + cx, y + cy) {
            continue;
        }
        let a = sp_at(sp, x + cx, y);
        let b = sp_at(sp, x, y + cy);
        let c = sp_at(sp, x + cx, y + cy);
        if a != index && b != index && c != b && c != a {
            return false;
        }
    }
    true
}

/// check_loopy: no 3-way intersections (ring).
pub fn check_loopy(x: i32, y: i32, _core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    for (cx, cy) in [(-2, -2), (-2, 2), (2, -2), (2, 2)] {
        let v0 = sp_at(sp, x + cx, y);
        let v1 = sp_at(sp, x, y + cy);
        let v2 = sp_at(sp, x + cx, y + cy);
        let mut count = 0;
        count += (v0 != index) as i32;
        count += (v1 != index) as i32;
        count += (v2 != v1) as i32;
        count += (v2 != v0) as i32;
        let mut zero_count = 0;
        if v0 == 0 {
            zero_count += 1;
        }
        if v1 == 0 {
            zero_count += 1;
        }
        if v2 == 0 {
            zero_count += 1;
        }
        if count == 3 && zero_count != 2 {
            return false;
        }
    }
    true
}

/// check_radar: watchtower vertices.
pub fn check_radar(x: i32, y: i32, core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let index = sp_at(sp, x, y);
    for (dx, dy) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
        let vx = (x + dx) as usize;
        let vy = (y + dy) as usize;
        if (core.puzzle[vx][vy] & VERTEX_RADAR_BIT) == 0 {
            continue;
        }
        let radar_value = (core.puzzle[vx][vy] & VERTEX_RADAR_BIT) >> VERTEX_RADAR_BIT_SHIFT;
        let mut filled_count = 1;
        let mut regions = [index; 4];
        let mut region_cnt = 1usize;
        let offsets = [[dx * 2, 0], [0, dy * 2], [dx * 2, dy * 2]];
        for [ox, oy] in offsets {
            let nx = x + ox;
            let ny = y + oy;
            if core.puzzle[nx as usize][ny as usize] != AREA_BLOCK {
                if sp_at(sp, nx, ny) != AREA_NORMAL {
                    let r = sp_at(sp, nx, ny);
                    let mut dup = false;
                    for m in 0..region_cnt {
                        if regions[m] == r {
                            dup = true;
                            break;
                        }
                    }
                    if !dup {
                        regions[region_cnt] = r;
                        region_cnt += 1;
                    }
                    filled_count += 1;
                }
            } else {
                filled_count += 1;
            }
        }
        let ok = if filled_count == 4 {
            region_cnt == radar_value as usize
        } else {
            region_cnt + (4 - filled_count) >= radar_value as usize
        };
        if !ok {
            return false;
        }
    }
    true
}

// ── Puzzle grid construction from the JSON model ─────────────────────────────

fn pattern_key(cells: &[[usize; 2]]) -> String {
    let mut v: Vec<String> = cells.iter().map(|xy| format!("{},{}", xy[0], xy[1])).collect();
    v.sort();
    v.join(";")
}

/// Map a JSON fence pattern (3x3 boundary shape) onto a C++ palisade marker.
pub fn palisade_type_from_fence(fp: &[[usize; 2]]) -> u32 {
    let mut set: [bool; 4] = [false; 4]; // up, down, left, right
    for &[r, c] in fp {
        match (r, c) {
            (0, 1) => set[0] = true,
            (2, 1) => set[1] = true,
            (1, 0) => set[2] = true,
            (1, 2) => set[3] = true,
            _ => {}
        }
    }
    let count = set.iter().filter(|&&b| b).count();
    let opposite_pair = (set[0] && set[1]) || (set[2] && set[3]);
    let t = match count {
        0 => 1u32,
        1 => 2u32,
        2 if opposite_pair => 3u32,
        2 => 5u32,
        3 => 4u32,
        _ => 6u32,
    };
    (t << AREA_PALISADE_INDEX_BIT_SHIFT) & AREA_PALISADE_INDEX_BIT
}

impl AoGCore {
    /// Build the solver state from a rsolver `Puzzle`. Returns None when the
    /// puzzle cannot be handled by this solver (e.g. rose_window).
    pub fn build(puzzle: &Puzzle, deadline: Instant) -> Option<AoGCore> {
        let h = puzzle.height;
        let w = puzzle.width;
        if h == 0 || w == 0 || h > 50 || w > 50 {
            return None;
        }

        if puzzle.rules.iter().any(|r| r.ctype == "rose_window") {
            // rose_window is supported natively; continue.
        }
        // Symbol type list (shared helper: rule params, else distinct cell
        // symbols — unified with rose / validate).
        let rose_types: Vec<String> = crate::shapes::rose_symbol_types(puzzle);

        let mut config = Config::default();
        for rule in &puzzle.rules {
            match rule.ctype.as_str() {
                "precise" => {
                    if let Some(v) = rule.params.get("area").and_then(|v| v.as_i64()) {
                        config.shape_size_lower_bound = v as i32;
                        config.shape_size_upper_bound = v as i32;
                    }
                }
                "range" => {
                    if let Some(v) = rule.params.get("min").and_then(|v| v.as_i64()) {
                        config.shape_size_lower_bound = config.shape_size_lower_bound.max(v as i32);
                    }
                    if let Some(v) = rule.params.get("max").and_then(|v| v.as_i64()) {
                        config.shape_size_upper_bound = if config.shape_size_upper_bound < 0 {
                            v as i32
                        } else {
                            config.shape_size_upper_bound.min(v as i32)
                        };
                    }
                }
                "different" => config.all_shapes_different = true,
                "same" => config.all_shapes_same = true,
                "mixed" => config.adjacent_shapes_different = true,
                "differentiation" => config.adjacent_sizes_different = true,
                "block" => config.only_rectangles = true,
                "non_block" => {
                    config.no_rectangles = true;
                    // A3: non_block regions have area >= 3 (13号 O5 verified
                    // 70/70 official solutions). Tighten the lower bound so aog
                    // skips enumerating size-1/2 regions. (doc 16 §2 A3.)
                    if config.shape_size_lower_bound < 3 {
                        config.shape_size_lower_bound = 3;
                    }
                }
                "brick" => config.no_4_way_intersections = true,
                "ring" => config.no_3_way_intersections = true,
                "solitary" => config.one_symbol_per_region = true,
                _ => {}
            }
        }

        // Only enforce clues whose rule is present.
        let active: HashSet<&str> = puzzle.rules.iter().map(|r| r.ctype.as_str()).collect();
        // predefine_shapes_only only when a `shape_pool` RULE requires every
        // region to come from the pool.  puzzle_piece puzzles carry a pool
        // array for its markers but allow arbitrary shapes elsewhere.
        config.predefine_shapes_only = active.contains("shape_pool");
        // W3: gate `check_radar` on whether the puzzle actually has watchtower
        // vertices (rule present). 1100+ puzzles have none and would otherwise
        // pay 4 useless puzzle[vx][vy] reads per cell expansion.
        config.has_watchtower = active.contains("watchtower");

        // Collect pool shapes from both the top-level array and the rule params.
        let pool_shapes = crate::shapes::collect_pool_shapes(puzzle);
        let has_shape_pool = !pool_shapes.is_empty();
        let use_area_numbers = active.contains("area");
        let use_compass = active.contains("compass");
        let use_watchtower = active.contains("watchtower");
        let use_puzzle_piece = active.contains("puzzle_piece");
        let use_fence = active.contains("fence");
        let use_edge_rules = active.contains("heterogeneous")
            || active.contains("homogeneous")
            || active.contains("inequality")
            || active.contains("difference");

        // Padded grid: rows/cols 0..=2n+4 (5 border rows/cols).
        let gh = 2 * h + 5;
        let gw = 2 * w + 5;
        let mut puzzle_grid = vec![vec![LINE_BLOCK; gw]; gh];
        for i in 2..=2 * h + 2 {
            for j in 2..=2 * w + 2 {
                puzzle_grid[i][j] = LINE_NORMAL;
            }
        }

        let mut compass_up = vec![vec![-1i32; gw]; gh];
        let mut compass_down = vec![vec![-1i32; gw]; gh];
        let mut compass_left = vec![vec![-1i32; gw]; gh];
        let mut compass_right = vec![vec![-1i32; gw]; gh];

        // Cells.
        for row in &puzzle.cells {
            for cell in row {
                let r = cell.row;
                let c = cell.col;
                if r >= h || c >= w {
                    continue;
                }
                let px = to_puzzle_x((r + 1) as i32) as usize;
                let py = to_puzzle_y((c + 1) as i32) as usize;
                if cell.blocked {
                    puzzle_grid[px][py] = AREA_BLOCK;
                    continue;
                }
                let mut v: u32 = AREA_NORMAL;
                if use_area_numbers {
                    if let Some(n) = cell.number {
                        if n >= 1 && n <= 255 {
                            v |= ((n as u32) << AREA_SHAPE_SIZE_BIT_SHIFT) & AREA_SHAPE_SIZE_BIT;
                        }
                    }
                }
                if use_compass {
                    if let Some(ref comp) = cell.compass {
                        v |= AREA_COMPASS_ENABLE;
                        compass_up[px][py] = comp.up.map(|x| x as i32).unwrap_or(-1);
                        compass_down[px][py] = comp.down.map(|x| x as i32).unwrap_or(-1);
                        compass_left[px][py] = comp.left.map(|x| x as i32).unwrap_or(-1);
                        compass_right[px][py] = comp.right.map(|x| x as i32).unwrap_or(-1);
                    }
                }
                if use_puzzle_piece {
                    if cell.shape_pattern.is_some() {
                        v |= AREA_SHAPE_INDEX_BIT; // index fixed after catalog build
                    }
                }
                if use_fence {
                    if let Some(ref fp) = cell.fence_pattern {
                        v |= palisade_type_from_fence(fp);
                    }
                }
                if let Some(sym) = cell.symbol.as_ref() {
                    v |= AREA_SYMBOL_BIT;
                    if !rose_types.is_empty() {
                        let idx = rose_types.iter().position(|t| t == sym).unwrap_or(0);
                        v |= ((idx as u32) << AREA_SLASH_INDEX_BIT_SHIFT) & AREA_SLASH_INDEX_BIT;
                    }
                }
                puzzle_grid[px][py] = v;
            }
        }

        // Edges.
        for r in 0..h {
            for c in 0..w.saturating_sub(1) {
                let e = &puzzle.h_edges[r][c];
                let forced = e.is_boundary || (e.constraint.is_some() && use_edge_rules);
                let px = to_puzzle_x((r + 1) as i32) as usize;
                let py = to_puzzle_x((c + 1) as i32) as usize + 1;
                let mut lv = puzzle_grid[px][py];
                if forced {
                    lv |= LINE_BLOCK;
                }
                if let Some(ref ec) = e.constraint {
                    if use_edge_rules {
                        lv = apply_line_constraint(lv, ec, true);
                    }
                }
                puzzle_grid[px][py] = lv;
            }
        }
        for r in 0..h.saturating_sub(1) {
            for c in 0..w {
                let e = &puzzle.v_edges[r][c];
                let forced = e.is_boundary || (e.constraint.is_some() && use_edge_rules);
                let px = to_puzzle_x((r + 1) as i32) as usize + 1;
                let py = to_puzzle_y((c + 1) as i32) as usize;
                let mut lv = puzzle_grid[px][py];
                if forced {
                    lv |= LINE_BLOCK;
                }
                if let Some(ref ec) = e.constraint {
                    if use_edge_rules {
                        lv = apply_line_constraint(lv, ec, true);
                    }
                }
                puzzle_grid[px][py] = lv;
            }
        }

        // Vertices (watchtowers / radar).  Vertex (r,c) is the ABSOLUTE grid
        // corner at (r,c) — r in 0..=h, c in 0..=w, border corners included.
        // In the padded grid a grid corner (r,c) sits at (2r+2, 2c+2): a cell
        // (r,c) sits at (2r+3, 2c+3), so its top-left corner is (2r+2, 2c+2).
        // Border corners (r=0/h, c=0/w) are valid padded positions (>= 2).
        if use_watchtower {
            for r in 0..=h {
                for c in 0..=w {
                    if let Some(val) = puzzle.vertices[r][c].watchtower {
                        if val >= 1 && val <= 4 {
                            let px = 2 * r + 2;
                            let py = 2 * c + 2;
                            puzzle_grid[px][py] |= (val as u32) << VERTEX_RADAR_BIT_SHIFT;
                        }
                    }
                }
            }
        }

        let mut core = AoGCore {
            n_row: h,
            n_col: w,
            config,
            puzzle: puzzle_grid,
            puzzle_compass_up: compass_up,
            puzzle_compass_down: compass_down,
            puzzle_compass_left: compass_left,
            puzzle_compass_right: compass_right,
            slash_nodes: Vec::new(),
            shape_size_nodes: Vec::new(),
            all_shapes_same_check_shape_index: -1,
            all_shapes_different_check_shape_index_pool: HashSet::new(),
            shapes: Vec::new(),
            shape_size_by_index: vec![0],
            shape_digest_index: HashMap::new(),
            node_to_shape_index: HashMap::new(),
            next_shape_index: 1,
            shape_cap: if config.predefine_shapes_only {
                // shape_pool puzzles: bounded predefined pool, DFS never calls
                // shapes_insert (search.rs short-circuits free enumeration), and
                // build-time pool registration is tiny — never cap.
                0
            } else {
                std::env::var("AOG_SHAPE_CAP")
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(DEFAULT_SHAPE_CAP)
            },
            dfs_ctx: DfsContext::new(h, w),
            deadline,
            rose_type_count: rose_types.len(),
        };

        // Register shape pool.
        if has_shape_pool {
            for cells in &pool_shapes {
                let (mut grid, size) = shape_grid_from_cells(cells);
                core.shapes_insert(&mut grid, size);
            }
        }
        if crate::aog_debug_enabled() {
            eprintln!(
                "aog build: h={} w={} has_pool={} pool_shapes={} n_rules={} predef={}",
                h,
                w,
                has_shape_pool,
                core.shapes.len(),
                puzzle.rules.len(),
                core.config.predefine_shapes_only
            );
        }

        // Register puzzle-piece patterns and fix up their shape index markers.
        if use_puzzle_piece {
            let mut index_map: HashMap<String, u32> = HashMap::new();
            for row in &puzzle.cells {
                for cell in row {
                    if cell.row >= h || cell.col >= w || cell.blocked {
                        continue;
                    }
                    if let Some(ref pattern) = cell.shape_pattern {
                        let (mut grid, size) = shape_grid_from_cells(pattern);
                        let key = pattern_key(pattern);
                        let idx = if let Some(&idx) = index_map.get(&key) {
                            idx
                        } else {
                            core.shapes_insert(&mut grid, size);
                            let found = core.shapes_search(&grid, size);
                            let idx = if found == NO_SHAPE_INDEX {
                                continue;
                            } else {
                                found
                            };
                            index_map.insert(key, idx);
                            idx
                        };
                        let px = to_puzzle_x((cell.row + 1) as i32) as usize;
                        let py = to_puzzle_y((cell.col + 1) as i32) as usize;
                        let cur = core.puzzle[px][py];
                        core.puzzle[px][py] = (cur & !AREA_SHAPE_INDEX_BIT)
                            | ((idx & 0x0f) << AREA_SHAPE_INDEX_BIT_SHIFT);
                    }
                }
            }
        }

        // Shape size nodes (cells with an area number), sorted ascending by size.
        let mut ssn: Vec<(i32, Node)> = Vec::new();
        for row in &puzzle.cells {
            for cell in row {
                if cell.row >= h || cell.col >= w || cell.blocked {
                    continue;
                }
                if use_area_numbers {
                    if let Some(n) = cell.number {
                        let node = Node {
                            x: (cell.row + 1) as i32,
                            y: (cell.col + 1) as i32,
                        };
                        ssn.push((n as i32, node));
                    }
                }
            }
        }
        ssn.sort_by_key(|&(s, _)| s);
        core.shape_size_nodes = ssn.into_iter().map(|(_, n)| n).collect();

        // Default size bounds (mirrors main.cpp).
        let mut empty_area_cnt = 0usize;
        for row in &puzzle.cells {
            for cell in row {
                if cell.row < h && cell.col < w && !cell.blocked {
                    empty_area_cnt += 1;
                }
            }
        }
        // rose_window: every region holds one of each symbol type, so its size
        // is at least the number of types (mirrors main.cpp slash lower bound).
        if core.rose_type_count > 0 {
            core.config.shape_size_lower_bound =
                core.config.shape_size_lower_bound.max(core.rose_type_count as i32);
        }
        if core.config.predefine_shapes_only {
            let mut lo = usize::MAX;
            let mut hi = 0usize;
            for s in &core.shapes {
                lo = lo.min(s.nodes.len());
                hi = hi.max(s.nodes.len());
            }
            core.config.shape_size_lower_bound = lo as i32;
            core.config.shape_size_upper_bound = hi as i32;
        } else if core.config.shape_size_lower_bound < 1 {
            core.config.shape_size_lower_bound = 1;
        }
        if core.config.shape_size_upper_bound < 1 {
            core.config.shape_size_upper_bound = empty_area_cnt as i32;
        }

        // only_rectangles (block rule): generate all rectangle shapes into the
        // catalog, mirroring main.cpp's ONLY_RECTANGLES handling.
        if core.config.only_rectangles {
            let lo = core.config.shape_size_lower_bound.max(1) as usize;
            let hi = core.config.shape_size_upper_bound.max(1) as usize;
            for size in lo..=hi {
                let mut l = 1usize;
                while l * l <= size {
                    if size % l == 0 {
                        let h = size / l;
                        if l <= core.n_row && h <= core.n_col {
                            let dim = h.max(l);
                            let mut grid = vec![vec![0u32; dim]; dim];
                            for i in 0..l {
                                for j in 0..h {
                                    grid[i][j] = 1;
                                }
                            }
                            core.shapes_insert(&mut grid, dim);
                        }
                    }
                    l += 1;
                }
            }
        }

        // rose_window: per-type node lists (mirrors main.cpp slash_nodes).
        if core.rose_type_count > 0 {
            let mut sn: Vec<Vec<Node>> = vec![Vec::new(); core.rose_type_count];
            for row in &puzzle.cells {
                for cell in row {
                    if cell.row >= h || cell.col >= w || cell.blocked {
                        continue;
                    }
                    if let Some(sym) = cell.symbol.as_ref() {
                        if let Some(t) = rose_types.iter().position(|x| x == sym) {
                            sn[t].push(Node {
                                x: (cell.row + 1) as i32,
                                y: (cell.col + 1) as i32,
                            });
                        }
                    }
                }
            }
            core.slash_nodes = sn;
        }

        Some(core)
    }
}

fn apply_line_constraint(mut lv: u32, ec: &EdgeConstraint, cell1_first: bool) -> u32 {
    match ec.ctype {
        EdgeConstraintType::Heterogeneous => lv |= LINE_DIFFERENT,
        EdgeConstraintType::Homogeneous => lv |= LINE_EQUAL,
        EdgeConstraintType::Inequality => {
            // JSON `value` convention, shared with the Python solver, exact-cover
            // solver and the independent validator (src/validation/validator.py):
            //   value == 1 → the FIRST endpoint (r1,c1) region is LARGER
            //   value != 1 → the SECOND endpoint (r2,c2) region is LARGER
            //
            // C++ LINE_LARGER means the upper/left neighbour is larger, i.e.
            // the first endpoint (cell1) region; LINE_SMALLER the reverse.
            // Both call sites pass cell1_first=true (cell1 is always the first
            // endpoint of the edge), so this reduces to value==1 → LINE_LARGER.
            let reversed = ec.value == Some(1);
            if (reversed && cell1_first) || (!reversed && !cell1_first) {
                lv |= LINE_LARGER;
            } else {
                lv |= LINE_SMALLER;
            }
        }
        EdgeConstraintType::Difference => {
            let diff = ec.value.unwrap_or(0);
            let dv = (diff + 1).min(15);
            lv |= ((dv as u32) << LINE_SIZE_DIFF_BIT_SHIFT) & LINE_SIZE_DIFF_BIT;
        }
    }
    lv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_core() -> AoGCore {
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        AoGCore {
            n_row: 4,
            n_col: 4,
            config: Config::default(),
            puzzle: vec![vec![LINE_NORMAL; 13]; 13],
            puzzle_compass_up: vec![vec![-1; 13]; 13],
            puzzle_compass_down: vec![vec![-1; 13]; 13],
            puzzle_compass_left: vec![vec![-1; 13]; 13],
            puzzle_compass_right: vec![vec![-1; 13]; 13],
            slash_nodes: Vec::new(),
            shape_size_nodes: Vec::new(),
            all_shapes_same_check_shape_index: -1,
            all_shapes_different_check_shape_index_pool: HashSet::new(),
            shapes: Vec::new(),
            shape_size_by_index: vec![0],
            shape_digest_index: HashMap::new(),
            node_to_shape_index: HashMap::new(),
            next_shape_index: 1,
            shape_cap: 0,
            dfs_ctx: crate::solver::aog::types::DfsContext::new(4, 4),
            deadline,
            rose_type_count: 0,
        }
    }

    #[test]
    fn test_shape_dedup_rotation() {
        let mut core = make_core();
        // Horizontal domino.
        let mut h = vec![vec![1u32, 1], vec![0, 0]];
        core.shapes_insert(&mut h, 2);
        // Vertical domino (a 90° rotation).
        let v = vec![vec![1u32, 0], vec![1, 0]];
        let idx = core.shapes_search(&v, 2);
        assert_ne!(idx, NO_SHAPE_INDEX, "vertical domino should be found");
        // Both must be the same class index.
        let h2 = vec![vec![1u32, 1], vec![0, 0]];
        let hidx = core.shapes_search(&h2, 2);
        assert_eq!(idx, hidx, "horizontal and vertical domino must dedup");
    }

    #[test]
    fn test_shape_dedup_l_tromino() {
        let mut core = make_core();
        let mut l1 = vec![vec![1u32, 1], vec![0, 1]];
        core.shapes_insert(&mut l1, 2);
        // Rotated L-tromino.
        let l2 = vec![vec![1u32, 0], vec![1, 1]];
        let a = core.shapes_search(&l2, 2);
        assert_ne!(a, NO_SHAPE_INDEX, "rotated L must be found");
        let l3 = vec![vec![1u32, 1], vec![1, 0]];
        let b = core.shapes_search(&l3, 2);
        assert_eq!(a, b, "L-tromino orientations must dedup");
    }

    #[test]
    fn test_shape_dedup_monomino() {
        let mut core = make_core();
        let mut m = vec![vec![1u32]];
        core.shapes_insert(&mut m, 1);
        let a = core.shapes_search(&vec![vec![1u32]], 1);
        let b = core.shapes_search(&vec![vec![1u32]], 1);
        assert_eq!(a, b);
        assert_ne!(a, NO_SHAPE_INDEX);
    }

    fn make_core_with_cap(shape_cap: usize) -> AoGCore {
        let mut core = make_core();
        core.shape_cap = shape_cap;
        core
    }

    #[test]
    fn test_shape_cap_refuses_new_shapes() {
        // cap = 1: after the first shapes_insert populates the library (with up
        // to 8 dihedral entries for one shape), further shapes_insert calls must
        // return 0 (refused) and shapes_search must report NO_SHAPE_INDEX for a
        // genuinely new shape.
        let mut core = make_core_with_cap(1);
        let mut domino = vec![vec![1u32, 1], vec![0, 0]];
        let first = core.shapes_insert(&mut domino, 2);
        assert_ne!(first, 0, "first insert must succeed under cap=1");
        // A distinct shape (L-tromino) the library has not seen.
        let mut l = vec![vec![1u32, 1], vec![0, 1]];
        let refused = core.shapes_insert(&mut l, 2);
        assert_eq!(refused, 0, "insert beyond cap must return 0");
        let miss = core.shapes_search(&l, 2);
        assert_eq!(
            miss, NO_SHAPE_INDEX,
            "refused shape must not be found in the capped library"
        );
        // The originally inserted shape is still searchable (cap does not evict).
        let still_found = core.shapes_search(&vec![vec![1u32, 1], vec![0, 0]], 2);
        assert_ne!(
            still_found, NO_SHAPE_INDEX,
            "already-catalogued shape must remain searchable after cap is hit"
        );
    }

    #[test]
    fn test_shape_cap_zero_unlimited() {
        // cap = 0 means disabled (legacy behavior): several distinct shapes
        // insert without refusal. All grids are square n×n matching shape_size.
        let mut core = make_core_with_cap(0);
        // (grid, shape_size) pairs, each a distinct polyomino class.
        for (mut s, sz) in [
            (vec![vec![1u32]], 1),
            (vec![vec![1u32, 1], vec![0, 0]], 2),
            (vec![vec![1u32, 1], vec![0, 1]], 2),
            (vec![vec![1u32, 1, 1], vec![0, 0, 0], vec![0, 0, 0]], 3),
        ] {
            let n = core.shapes_insert(&mut s, sz);
            // At least one orientation is new for each distinct shape.
            assert_ne!(n, 0, "cap=0 must not refuse any new shape");
        }
    }
}
