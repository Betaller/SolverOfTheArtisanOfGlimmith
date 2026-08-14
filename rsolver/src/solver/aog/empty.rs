//! Empty-area analysis and start-area selection (dfs.cpp `dfs_empty` + `find_*`).

use super::types::*;
use super::types::{CompassStates, Node};
use std::time::Instant;

use super::core::{AoGCore};

// ── dfs_empty: flood fill over empty areas ───────────────────────────────────

fn dfs_empty(x: i32, y: i32, core: &mut AoGCore, sp: &Vec<Vec<u32>>) {
    let puzzle_x = to_puzzle_x(x) as usize;
    let puzzle_y = to_puzzle_y(y) as usize;
    if core.puzzle[puzzle_x][puzzle_y] == AREA_BLOCK {
        return;
    }
    if sp[puzzle_x][puzzle_y] != AREA_NORMAL {
        return;
    }
    if core.dfs_ctx.visited[puzzle_x][puzzle_y] == core.dfs_ctx.visited_index {
        return;
    }
    core.dfs_ctx.visited[puzzle_x][puzzle_y] = core.dfs_ctx.visited_index;
    core.dfs_ctx.empty_count += 1;

    // Detect forced "block lines" inside the connected component.
    let mut block_line_flag = false;
    if (core.puzzle[puzzle_x - 1][puzzle_y] & LINE_BLOCK) != 0
        && core.dfs_ctx.visited[puzzle_x - 2][puzzle_y] == core.dfs_ctx.visited_index
    {
        core.dfs_ctx
            .empty_block_line_node_pairs
            .insert((Node { x: x - 1, y }, Node { x, y }));
        block_line_flag = true;
    }
    if (core.puzzle[puzzle_x + 1][puzzle_y] & LINE_BLOCK) != 0
        && core.dfs_ctx.visited[puzzle_x + 2][puzzle_y] == core.dfs_ctx.visited_index
    {
        core.dfs_ctx
            .empty_block_line_node_pairs
            .insert((Node { x: x + 1, y }, Node { x, y }));
        block_line_flag = true;
    }
    if (core.puzzle[puzzle_x][puzzle_y - 1] & LINE_BLOCK) != 0
        && core.dfs_ctx.visited[puzzle_x][puzzle_y - 2] == core.dfs_ctx.visited_index
    {
        core.dfs_ctx
            .empty_block_line_node_pairs
            .insert((Node { x, y }, Node { x, y: y - 1 }));
        block_line_flag = true;
    }
    if (core.puzzle[puzzle_x][puzzle_y + 1] & LINE_BLOCK) != 0
        && core.dfs_ctx.visited[puzzle_x][puzzle_y + 2] == core.dfs_ctx.visited_index
    {
        core.dfs_ctx
            .empty_block_line_node_pairs
            .insert((Node { x, y }, Node { x, y: y + 1 }));
        block_line_flag = true;
    }
    if block_line_flag {
        core.dfs_ctx.empty_block_line_count += 1;
    }

    if core.area_contain_symbol(x, y) {
        core.dfs_ctx.symbol_count += 1;
    }

    let pv = core.puzzle[puzzle_x][puzzle_y];
    if (pv & AREA_SHAPE_SIZE_BIT) != 0 {
        core.dfs_ctx
            .area_shape_sizes
            .push(((pv & AREA_SHAPE_SIZE_BIT) >> AREA_SHAPE_SIZE_BIT_SHIFT) as usize);
    }
    if core.rose_type_count > 0 && (pv & AREA_SYMBOL_BIT) != 0 {
        core.dfs_ctx.slash_count[symbol_type_idx(pv)] += 1;
    }
    if (pv & AREA_COMPASS_ENABLE) != 0 {
        core.dfs_ctx.compass_nodes.push(Node { x, y });
        core.dfs_ctx.compass_node_states.push(CompassStates::default());
    }

    if (core.puzzle[puzzle_x - 1][puzzle_y] & LINE_BLOCK) == 0 {
        dfs_empty(x - 1, y, core, sp);
    }
    if (core.puzzle[puzzle_x + 1][puzzle_y] & LINE_BLOCK) == 0 {
        dfs_empty(x + 1, y, core, sp);
    }
    if (core.puzzle[puzzle_x][puzzle_y - 1] & LINE_BLOCK) == 0 {
        dfs_empty(x, y - 1, core, sp);
    }
    if (core.puzzle[puzzle_x][puzzle_y + 1] & LINE_BLOCK) == 0 {
        dfs_empty(x, y + 1, core, sp);
    }
}

// ── dfs_empty_compass: flood fill counting compass distances ─────────────────

fn dfs_empty_compass(x: i32, y: i32, core: &mut AoGCore, sp: &Vec<Vec<u32>>) {
    let puzzle_x = to_puzzle_x(x) as usize;
    let puzzle_y = to_puzzle_y(y) as usize;
    if core.puzzle[puzzle_x][puzzle_y] == AREA_BLOCK {
        return;
    }
    if sp[puzzle_x][puzzle_y] != AREA_NORMAL {
        return;
    }
    if core.dfs_ctx.visited[puzzle_x][puzzle_y] == core.dfs_ctx.visited_index {
        return;
    }
    core.dfs_ctx.visited[puzzle_x][puzzle_y] = core.dfs_ctx.visited_index;

    for i in 0..core.dfs_ctx.compass_nodes.len() {
        let c = core.dfs_ctx.compass_nodes[i];
        if x < c.x {
            core.dfs_ctx.compass_node_states[i].up += 1;
        }
        if x > c.x {
            core.dfs_ctx.compass_node_states[i].down += 1;
        }
        if y < c.y {
            core.dfs_ctx.compass_node_states[i].left += 1;
        }
        if y > c.y {
            core.dfs_ctx.compass_node_states[i].right += 1;
        }
    }

    if (core.puzzle[puzzle_x - 1][puzzle_y] & LINE_BLOCK) == 0 {
        dfs_empty_compass(x - 1, y, core, sp);
    }
    if (core.puzzle[puzzle_x + 1][puzzle_y] & LINE_BLOCK) == 0 {
        dfs_empty_compass(x + 1, y, core, sp);
    }
    if (core.puzzle[puzzle_x][puzzle_y - 1] & LINE_BLOCK) == 0 {
        dfs_empty_compass(x, y - 1, core, sp);
    }
    if (core.puzzle[puzzle_x][puzzle_y + 1] & LINE_BLOCK) == 0 {
        dfs_empty_compass(x, y + 1, core, sp);
    }
}

fn dfs_empty_compass_check(x: i32, y: i32, core: &mut AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    core.dfs_ctx.visited_index += 1;
    dfs_empty_compass(x, y, core, sp);

    for i in 0..core.dfs_ctx.compass_nodes.len() {
        let c = core.dfs_ctx.compass_nodes[i];
        let px = to_puzzle_x(c.x) as usize;
        let py = to_puzzle_y(c.y) as usize;
        let s = core.dfs_ctx.compass_node_states[i];
        if core.puzzle_compass_up[px][py] != -1 && core.puzzle_compass_up[px][py] > s.up {
            return false;
        }
        if core.puzzle_compass_down[px][py] != -1 && core.puzzle_compass_down[px][py] > s.down {
            return false;
        }
        if core.puzzle_compass_left[px][py] != -1 && core.puzzle_compass_left[px][py] > s.left {
            return false;
        }
        if core.puzzle_compass_right[px][py] != -1 && core.puzzle_compass_right[px][py] > s.right {
            return false;
        }
    }
    true
}

// ── try_place_id: bipartite partition helper ─────────────────────────────────

fn try_place_id(x: i32, y: i32, value: i32, visited_value: i32, core: &mut AoGCore) -> i32 {
    let key = encode_node(x, y);
    let e = core.dfs_ctx.place_visited.entry(key).or_insert(0);
    if *e == visited_value {
        return 0;
    }
    *e = visited_value;
    let mut count = value;
    if let Some(neighbors) = core.dfs_ctx.block_adj.get(&key).cloned() {
        for n in neighbors {
            count += try_place_id(n.x, n.y, !value, visited_value, core);
        }
    }
    count
}

// ── DFS_empty: high-level empty area analysis ────────────────────────────────

pub fn dfs_empty_area(x: i32, y: i32, core: &mut AoGCore, sp: &Vec<Vec<u32>>) {
    core.dfs_ctx.empty_count = 0;
    core.dfs_ctx.empty_block_line_count = 0;
    core.dfs_ctx.empty_block_line_node_pairs.clear();
    core.dfs_ctx.symbol_count = 0;
    core.dfs_ctx.slash_count = [0; 10];
    core.dfs_ctx.compass_nodes.clear();
    core.dfs_ctx.compass_node_states.clear();
    core.dfs_ctx.area_shape_sizes.clear();

    core.dfs_ctx.visited_index += 1;
    dfs_empty(x, y, core, sp);

    if core.dfs_ctx.empty_block_line_node_pairs.is_empty() {
        core.dfs_ctx.empty_block_line_count = 0;
        core.dfs_ctx.block_adj.clear();
        core.dfs_ctx.place_visited.clear();
        return;
    }

    core.dfs_ctx.block_adj.clear();
    // Collect once, iterate twice (was two `.cloned().collect()` of the same
    // source — 白捡 W4, saves one Vec allocation + clone pass per call).
    let pairs: Vec<(Node, Node)> = core
        .dfs_ctx
        .empty_block_line_node_pairs
        .iter()
        .cloned()
        .collect();
    for (a, b) in &pairs {
        core.dfs_ctx
            .block_adj
            .entry(encode_node(a.x, a.y))
            .or_default()
            .push(*b);
        core.dfs_ctx
            .block_adj
            .entry(encode_node(b.x, b.y))
            .or_default()
            .push(*a);
    }

    core.dfs_ctx.place_visited.clear();
    core.dfs_ctx.empty_block_line_count = 0;
    for (a, b) in &pairs {
        if !core
            .dfs_ctx
            .place_visited
            .contains_key(&encode_node(a.x, a.y))
        {
            let c1 = try_place_id(a.x, a.y, 0, 1, core);
            let c2 = try_place_id(a.x, a.y, 1, 2, core);
            core.dfs_ctx.empty_block_line_count += c1.min(c2) as usize;
        }
        if !core
            .dfs_ctx
            .place_visited
            .contains_key(&encode_node(b.x, b.y))
        {
            let c1 = try_place_id(b.x, b.y, 0, 1, core);
            let c2 = try_place_id(b.x, b.y, 1, 2, core);
            core.dfs_ctx.empty_block_line_count += c1.min(c2) as usize;
        }
    }
}

fn dfs_group_mark(core: &mut AoGCore) {
    core.dfs_ctx.group_mark_index = core.dfs_ctx.visited_index;
}

fn dfs_in_group_mark(x: i32, y: i32, core: &AoGCore) -> bool {
    let px = to_puzzle_x(x) as usize;
    let py = to_puzzle_y(y) as usize;
    core.dfs_ctx.visited[px][py] > core.dfs_ctx.group_mark_index
}

// ── empty_area_check: main constraint check on remaining empty areas ─────────

/// ring (no 3-way intersections): at a vertex whose four surrounding cells have
/// exactly one still-empty cell, the empty cell will become a fresh region.
/// If that forces exactly three solution boundaries at the vertex, the state is
/// dead — the empty can never be filled validly and the solver never merges a
/// cell into an already-placed region. (bilibili 环纹性质; mirrors the Python
/// _check_ring boundary-count definition.)
fn ring_t_junction_check(core: &AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    let n_row = core.n_row as i32;
    let n_col = core.n_col as i32;
    let max_px = 2 * n_row + 1;
    let max_py = 2 * n_col + 1;
    const FRESH: u32 = u32::MAX;
    for x in 1..=n_row {
        for y in 1..=n_col {
            let px = to_puzzle_x(x) as usize;
            let py = to_puzzle_y(y) as usize;
            if core.puzzle[px][py] == AREA_BLOCK || sp[px][py] != AREA_NORMAL {
                continue;
            }
            for (dx, dy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let vx = px as i32 + dx;
                let vy = py as i32 + dy;
                // Cyclic corners: TL, TR, BL, BR. Edges: top (0,1), bottom
                // (2,3), left (0,2), right (1,3).
                let corners = [(vx - 1, vy - 1), (vx + 1, vy - 1), (vx - 1, vy + 1), (vx + 1, vy + 1)];
                let mut rids = [0u32; 4];
                let mut empty_count = 0usize;
                let mut skip = false;
                for k in 0..4 {
                    let (cx, cy) = corners[k];
                    if cx < 3 || cx > max_px || cy < 3 || cy > max_py {
                        skip = true;
                        break;
                    }
                    let cu = cx as usize;
                    let cv = cy as usize;
                    if core.puzzle[cu][cv] == AREA_BLOCK {
                        skip = true;
                        break;
                    }
                    let v = sp[cu][cv];
                    if v == AREA_NORMAL {
                        rids[k] = FRESH;
                        empty_count += 1;
                    } else {
                        rids[k] = v & SOLVE_AREA_BIT;
                    }
                }
                if skip || empty_count != 1 {
                    continue;
                }
                let boundaries = (rids[0] != rids[1]) as u32
                    + (rids[2] != rids[3]) as u32
                    + (rids[0] != rids[2]) as u32
                    + (rids[1] != rids[3]) as u32;
                if boundaries == 3 {
                    return false;
                }
            }
        }
    }
    true
}

pub fn empty_area_check(core: &mut AoGCore, sp: &Vec<Vec<u32>>) -> bool {
    if core.config.no_3_way_intersections && !ring_t_junction_check(core, sp) {
        return false;
    }
    dfs_group_mark(core);
    let mut checked = 0usize;
    for x in 1..=core.n_row as i32 {
        for y in 1..=core.n_col as i32 {
            checked += 1;
            // N2: bail on deadline inside the O(cells²) flood-fill scan so a
            // single shape placement can't overshoot the budget by seconds.
            // Returning false prunes safely — the search is already past its
            // deadline and would fail anyway (doc 17 §4.1 A1).
            if checked % 256 == 0 && Instant::now() >= core.deadline {
                return false;
            }
            let px = to_puzzle_x(x) as usize;
            let py = to_puzzle_y(y) as usize;
            if core.puzzle[px][py] != AREA_BLOCK
                && sp[px][py] == AREA_NORMAL
                && !dfs_in_group_mark(x, y, core)
            {
                dfs_empty_area(x, y, core, sp);

                let max_area_size = core.dfs_ctx.empty_count - core.dfs_ctx.empty_block_line_count;
                if max_area_size < core.config.shape_size_lower_bound as usize {
                    return false;
                }

                let mut seen_size = [false; 256];
                let mut required = 0usize;
                for &val in &core.dfs_ctx.area_shape_sizes {
                    if val < 256 && !seen_size[val] {
                        seen_size[val] = true;
                        required += val;
                    }
                }
                if required > core.dfs_ctx.empty_count {
                    return false;
                }

                if core.config.one_symbol_per_region {
                    if core.dfs_ctx.symbol_count == 0 {
                        return false;
                    }
                    if core.dfs_ctx.symbol_count == 1 && core.dfs_ctx.empty_block_line_count != 0 {
                        return false;
                    }
                }

                // rose_window: every remaining empty area must contain the same
                // number of each symbol type (mirrors C++ slash_count check).
                if core.rose_type_count > 0 {
                    let first = core.dfs_ctx.slash_count[0];
                    let mut rose_ok = true;
                    for t in 1..core.rose_type_count {
                        if core.dfs_ctx.slash_count[t] != first {
                            rose_ok = false;
                            break;
                        }
                    }
                    if !rose_ok || first == 0 {
                        return false;
                    }
                    if first == 1 && core.dfs_ctx.empty_block_line_count != 0 {
                        return false;
                    }
                }

                if core.config.shape_size_lower_bound == core.config.shape_size_upper_bound {
                    let lb = core.config.shape_size_lower_bound as usize;
                    if core.dfs_ctx.empty_count % lb != 0 {
                        return false;
                    }
                    if core.dfs_ctx.empty_count == lb && core.dfs_ctx.empty_block_line_count != 0 {
                        return false;
                    }
                }

                if core.config.all_shapes_same && core.all_shapes_same_check_shape_index != -1 {
                    let shape_size = core.shape_size_by_index
                        [core.all_shapes_same_check_shape_index as usize];
                    if core.dfs_ctx.empty_count % shape_size != 0 {
                        return false;
                    }
                }

                if !core.dfs_ctx.compass_nodes.is_empty() {
                    if !dfs_empty_compass_check(x, y, core, sp) {
                        return false;
                    }
                }
            }
            // Palisade markers on still-empty cells.
            if core.puzzle[px][py] != AREA_BLOCK
                && sp[px][py] == AREA_NORMAL
                && (core.puzzle[px][py] & AREA_PALISADE_INDEX_BIT) != 0
            {
                let palisade_type = (core.puzzle[px][py] & AREA_PALISADE_INDEX_BIT)
                    >> AREA_PALISADE_INDEX_BIT_SHIFT;
                let mut up = false;
                let mut down = false;
                let mut left = false;
                let mut right = false;
                if core.puzzle[px - 2][py] != AREA_BLOCK && sp[px - 2][py] == AREA_NORMAL {
                    left = true;
                }
                if core.puzzle[px + 2][py] != AREA_BLOCK && sp[px + 2][py] == AREA_NORMAL {
                    right = true;
                }
                if core.puzzle[px][py - 2] != AREA_BLOCK && sp[px][py - 2] == AREA_NORMAL {
                    up = true;
                }
                if core.puzzle[px][py + 2] != AREA_BLOCK && sp[px][py + 2] == AREA_NORMAL {
                    down = true;
                }
                let sum = (up as i32) + (down as i32) + (left as i32) + (right as i32);
                match palisade_type {
                    1 => {
                        if !(up && down && left && right) {
                            return false;
                        }
                    }
                    2 => {
                        if sum < 3 {
                            return false;
                        }
                    }
                    3 => {
                        if (!up && !right) || (!right && !down) || (!down && !left) || (!left && !up) {
                            return false;
                        }
                    }
                    4 => {
                        if !up && !down && !left && !right {
                            return false;
                        }
                    }
                    5 => {
                        if (!up && !down) || (!right && !left) {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    true
}

// ── empty_area_size_range ────────────────────────────────────────────────────

fn _empty_area_shape_count(core: &AoGCore) -> i32 {
    if core.dfs_ctx.empty_block_line_count != 0 {
        return 0;
    }
    if core.dfs_ctx.empty_count <= core.config.shape_size_lower_bound as usize {
        return 1;
    }
    if core.dfs_ctx.area_shape_sizes.len() == 1
        && core.dfs_ctx.area_shape_sizes[0] == core.dfs_ctx.empty_count
    {
        return 1;
    }
    if core.config.one_symbol_per_region && core.dfs_ctx.symbol_count == 1 {
        return 1;
    }
    if core.rose_type_count > 0 {
        return core.dfs_ctx.slash_count[0];
    }
    0
}

pub fn empty_area_size_range(x: i32, y: i32, core: &mut AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    dfs_empty_area(x, y, core, sp);

    let mut lb = core.config.shape_size_lower_bound;
    let mut ub = core.config.shape_size_upper_bound;
    let max_area_size = core.dfs_ctx.empty_count as i32 - core.dfs_ctx.empty_block_line_count as i32;
    if max_area_size < lb {
        return (-1, -1, -1);
    }
    ub = ub.min(max_area_size);

    let shape_count = _empty_area_shape_count(core);
    if shape_count == 1 {
        if max_area_size < lb || max_area_size > ub {
            return (-1, -1, -1);
        }
        lb = max_area_size;
    }
    (0, lb, ub)
}

// ── Find functions: locate the best empty starting area ──────────────────────

fn find_empty_compass_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if (core.puzzle[px][py] & AREA_COMPASS_ENABLE) != 0 && sp[px][py] == AREA_NORMAL {
                return (0, i, j);
            }
        }
    }
    (-1, -1, -1)
}

fn find_empty_alone_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if sp[px][py] != AREA_NORMAL {
                continue;
            }
            let mut block_status = 0;
            if (core.puzzle[px - 2][py] & AREA_BLOCK) != 0
                || sp[px - 2][py] != AREA_NORMAL
                || (core.puzzle[px - 1][py] & LINE_BLOCK) != 0
            {
                block_status |= 1 << 3;
            }
            if (core.puzzle[px + 2][py] & AREA_BLOCK) != 0
                || sp[px + 2][py] != AREA_NORMAL
                || (core.puzzle[px + 1][py] & LINE_BLOCK) != 0
            {
                block_status |= 1 << 2;
            }
            if (core.puzzle[px][py - 2] & AREA_BLOCK) != 0
                || sp[px][py - 2] != AREA_NORMAL
                || (core.puzzle[px][py - 1] & LINE_BLOCK) != 0
            {
                block_status |= 1 << 1;
            }
            if (core.puzzle[px][py + 2] & AREA_BLOCK) != 0
                || sp[px][py + 2] != AREA_NORMAL
                || (core.puzzle[px][py + 1] & LINE_BLOCK) != 0
            {
                block_status |= 1 << 0;
            }
            if block_status == 15 {
                return (0, i, j);
            }
            if (core.puzzle[px][py] & AREA_PALISADE_INDEX_BIT) != 0 {
                let palisade_type =
                    (core.puzzle[px][py] & AREA_PALISADE_INDEX_BIT) >> AREA_PALISADE_INDEX_BIT_SHIFT;
                if palisade_type == 6 {
                    return (0, i, j);
                }
            }
        }
    }
    (-1, -1, -1)
}

fn find_size_limit_small_area(core: &mut AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    dfs_group_mark(core);
    for x in 1..=core.n_row as i32 {
        for y in 1..=core.n_col as i32 {
            let px = to_puzzle_x(x) as usize;
            let py = to_puzzle_y(y) as usize;
            if core.puzzle[px][py] != AREA_BLOCK
                && sp[px][py] == AREA_NORMAL
                && !dfs_in_group_mark(x, y, core)
            {
                dfs_empty_area(x, y, core, sp);
                if core.config.shape_size_lower_bound == core.config.shape_size_upper_bound
                    && core.dfs_ctx.empty_count == core.config.shape_size_lower_bound as usize
                {
                    return (0, x, y);
                }
            }
        }
    }
    (-1, -1, -1)
}

fn find_empty_shape_index_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if (core.puzzle[px][py] & AREA_SHAPE_INDEX_BIT) != 0 && sp[px][py] == AREA_NORMAL {
                return (0, i, j);
            }
        }
    }
    (-1, -1, -1)
}

fn find_empty_shape_size_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    for node in &core.shape_size_nodes {
        let px = to_puzzle_x(node.x) as usize;
        let py = to_puzzle_y(node.y) as usize;
        if (core.puzzle[px][py] & AREA_SHAPE_SIZE_BIT) != 0 && sp[px][py] == AREA_NORMAL {
            return (0, node.x, node.y);
        }
    }
    (-1, -1, -1)
}

fn find_empty_corner_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if core.puzzle[px][py] == AREA_BLOCK || sp[px][py] != AREA_NORMAL {
                continue;
            }
            let mut block_line_count = 0;
            if (core.puzzle[px - 1][py] & LINE_BLOCK) != 0 || core.puzzle[px - 2][py] == AREA_BLOCK {
                block_line_count += 1;
            }
            if (core.puzzle[px + 1][py] & LINE_BLOCK) != 0 || core.puzzle[px + 2][py] == AREA_BLOCK {
                block_line_count += 1;
            }
            if (core.puzzle[px][py - 1] & LINE_BLOCK) != 0 || core.puzzle[px][py - 2] == AREA_BLOCK {
                block_line_count += 1;
            }
            if (core.puzzle[px][py + 1] & LINE_BLOCK) != 0 || core.puzzle[px][py + 2] == AREA_BLOCK {
                block_line_count += 1;
            }
            if core.rose_type_count > 0 && (core.puzzle[px][py] & AREA_SYMBOL_BIT) != 0 {
                let me = core.puzzle[px][py] & AREA_SLASH_INDEX_BIT;
                if (core.puzzle[px - 1][py] & LINE_BLOCK) == 0
                    && (core.puzzle[px - 2][py] & AREA_SLASH_INDEX_BIT) == me
                {
                    block_line_count += 1;
                }
                if (core.puzzle[px + 1][py] & LINE_BLOCK) == 0
                    && (core.puzzle[px + 2][py] & AREA_SLASH_INDEX_BIT) == me
                {
                    block_line_count += 1;
                }
                if (core.puzzle[px][py - 1] & LINE_BLOCK) == 0
                    && (core.puzzle[px][py - 2] & AREA_SLASH_INDEX_BIT) == me
                {
                    block_line_count += 1;
                }
                if (core.puzzle[px][py + 1] & LINE_BLOCK) == 0
                    && (core.puzzle[px][py + 2] & AREA_SLASH_INDEX_BIT) == me
                {
                    block_line_count += 1;
                }
            }
            if block_line_count >= 3 {
                return (0, i, j);
            }
            if (core.puzzle[px][py] & AREA_PALISADE_INDEX_BIT) != 0 {
                let palisade_type =
                    (core.puzzle[px][py] & AREA_PALISADE_INDEX_BIT) >> AREA_PALISADE_INDEX_BIT_SHIFT;
                if palisade_type == 4 {
                    return (0, i, j);
                }
            }
        }
    }
    (-1, -1, -1)
}

/// Generic: find a cell whose neighbour (2 steps away) is filled and has a
/// constrained line between them.
fn find_empty_line_constraint_generic(
    core: &AoGCore,
    sp: &Vec<Vec<u32>>,
    matcher: impl Fn(u32) -> bool,
) -> (i32, i32, i32) {
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if core.puzzle[px][py] == AREA_BLOCK || sp[px][py] != AREA_NORMAL {
                continue;
            }
            for (lx, ly, nx, ny) in [(-1, 0, px - 2, py), (1, 0, px + 2, py), (0, -1, px, py - 2), (0, 1, px, py + 2)] {
                let line_val = core.puzzle[(px as i32 + lx) as usize][(py as i32 + ly) as usize];
                let nv = core.puzzle[nx][ny];
                if matcher(line_val) && sp[nx][ny] != AREA_NORMAL && nv != AREA_BLOCK {
                    return (0, i, j);
                }
            }
        }
    }
    (-1, -1, -1)
}

fn find_empty_line_equal_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    find_empty_line_constraint_generic(core, sp, |lv| (lv & LINE_EQUAL) != 0)
}

// Kept (unused) to mirror the C++ reference: dfs.cpp still *calls* this finder in
// find_special_start_area but discards its result (a missing `ret` refresh), so the
// LINE_SIZE_DIFF special start never fires there.  See the note in
// find_special_start_area.  This function documents what C++ computes-and-throws-away.
#[allow(dead_code)]
fn find_empty_line_size_diff_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    find_empty_line_constraint_generic(core, sp, |lv| (lv & LINE_SIZE_DIFF_BIT) != 0)
}

fn find_empty_line_larger_or_smaller_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    find_empty_line_constraint_generic(core, sp, |lv| (lv & (LINE_LARGER | LINE_SMALLER)) != 0)
}

fn find_empty_line_constraint_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    // Mirror dfs.cpp find_empty_line_constraint_area (lines 684-704): unlike the
    // line_equal / line_size_diff / larger_or_smaller finders, this one does NOT
    // require the neighbor across the constraint edge to be placed already.  It
    // fires on the first empty cell with any adjacent constraint edge.  The old
    // implementation reused find_empty_line_constraint_generic (which requires a
    // placed neighbor), so on an empty board it returned -1 and the search fell
    // through to the corner special start — diverging from the C++ search order
    // (e.g. puzzle 0404: C++ starts at LINE_CONSTRAINT (1,0), Rust started at
    // corner (0,6) and never reached a solution).
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if core.puzzle[px][py] == AREA_BLOCK || sp[px][py] != AREA_NORMAL {
                continue;
            }
            for (lx, ly) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let line_val = core.puzzle[(px as i32 + lx) as usize][(py as i32 + ly) as usize];
                if (line_val & (LINE_EQUAL | LINE_LARGER | LINE_SMALLER | LINE_DIFFERENT | LINE_SIZE_DIFF_BIT)) != 0
                {
                    return (0, i, j);
                }
            }
        }
    }
    (-1, -1, -1)
}

fn find_empty_area(core: &AoGCore, sp: &Vec<Vec<u32>>) -> (i32, i32, i32) {
    for i in 1..=core.n_row as i32 {
        for j in 1..=core.n_col as i32 {
            let px = to_puzzle_x(i) as usize;
            let py = to_puzzle_y(j) as usize;
            if core.puzzle[px][py] != AREA_BLOCK && sp[px][py] == AREA_NORMAL {
                return (0, i, j);
            }
        }
    }
    (-1, -1, -1)
}

pub fn find_special_start_area(core: &mut AoGCore, sp: &Vec<Vec<u32>>) -> (u32, i32, i32) {
    let mut special_start_type;
    let mut ret_data = find_empty_alone_area(core, sp);
    special_start_type = SPECIAL_START_SIZE_1_REGION;
    let mut ret = ret_data.0;
    if ret == -1 {
        ret_data = find_size_limit_small_area(core, sp);
        special_start_type = SPECIAL_START_SIZE_MATCH_REGION;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_line_equal_area(core, sp);
        special_start_type = SPECIAL_START_LINE_SAME;
        ret = ret_data.0;
    }
    // NOTE: The C++ reference (dfs.cpp find_special_start_area, lines 748-756)
    // calls find_empty_line_size_diff_area but NEVER refreshes `ret` afterwards,
    // so its result is immediately overwritten by find_empty_line_larger_or_smaller_area.
    // The SPECIAL_START_LINE_SIZE_DIFF special start is therefore dead code in C++
    // and never fires.  The old Rust port faithfully exposed it, which made the
    // search pick LINE_SIZE_DIFF where C++ falls through to LINE_CONSTRAINT —
    // and its (marker-width) size filter pruned valid solutions (puzzle 0404's
    // 2/3 split at cell (1,1)).  Skip it to match the C++ search order exactly.
    if ret == -1 {
        ret_data = find_empty_line_larger_or_smaller_area(core, sp);
        special_start_type = SPECIAL_START_LINE_SMALLER_OR_LARGER;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_shape_index_area(core, sp);
        special_start_type = SPECIAL_START_AREA_INDEX;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_shape_size_area(core, sp);
        special_start_type = SPECIAL_START_AREA_SIZE;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_compass_area(core, sp);
        special_start_type = SPECIAL_START_COMPASS;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_line_constraint_area(core, sp);
        special_start_type = SPECIAL_START_LINE_CONSTRAINT;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_corner_area(core, sp);
        special_start_type = SPECIAL_START_CORNER;
        ret = ret_data.0;
    }
    if ret == -1 {
        ret_data = find_empty_area(core, sp);
        special_start_type = SPECIAL_START_DEFAULT;
    }
    (special_start_type, ret_data.1, ret_data.2)
}
