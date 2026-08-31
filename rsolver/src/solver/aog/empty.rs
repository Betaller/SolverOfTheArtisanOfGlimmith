//! Empty-area analysis and start-area selection (dfs.cpp `dfs_empty` + `find_*`).

use std::collections::{HashMap, HashSet, VecDeque};

use super::types::*;
use super::types::{CompassStates, Node};
use crate::clock::Instant;

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
// (Kept for reference; the minimum-vertex-cover computation now uses
// `component_max_matching` below — see H1.)

#[allow(dead_code)]
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
            count += try_place_id(n.x, n.y, 1 - value, visited_value, core);
        }
    }
    count
}

/// Maximum bipartite matching size (Kőnig's theorem: = minimum vertex cover)
/// for the connected component of `block_adj` containing `start_key`.
///
/// `block_adj` is the adjacency list of the block-line graph (bipartite: every
/// edge separates a fillable cell from a forced block-line). The true number of
/// cells that must be excluded from a region is the minimum vertex cover τ =
/// maximum matching size — NOT `min(|X|,|Y|)` (which over-counts, pruning valid
/// regions: bug H1).
///
/// Returns the matching size. DEFENSIVE: if the component is not bipartite
/// (an odd cycle — should never happen for the block-line graph), it falls back
/// to `min(|X|,|Y|)` so we never silently lose more than the old code did.
///
/// `seen` is filled with every node key in the component so the caller can
/// process each component exactly once.
fn component_max_matching(
    start_key: u64,
    block_adj: &HashMap<u64, Vec<Node>>,
    seen: &mut HashSet<u64>,
) -> usize {
    // 1. Collect the component via BFS.
    let mut comp: Vec<u64> = Vec::new();
    let mut q: VecDeque<u64> = VecDeque::new();
    q.push_back(start_key);
    seen.insert(start_key);
    while let Some(u) = q.pop_front() {
        comp.push(u);
        if let Some(neigh) = block_adj.get(&u) {
            for &n in neigh {
                let k = encode_node(n.x, n.y);
                if seen.insert(k) {
                    q.push_back(k);
                }
            }
        }
    }

    // 2. 2-color the component.
    let mut color: HashMap<u64, i32> = HashMap::new();
    let mut bipartite = true;
    {
        let mut q: VecDeque<u64> = VecDeque::new();
        q.push_back(start_key);
        color.insert(start_key, 0);
        while let Some(u) = q.pop_front() {
            let cu = color[&u];
            if let Some(neigh) = block_adj.get(&u) {
                for &n in neigh {
                    let k = encode_node(n.x, n.y);
                    match color.get(&k) {
                        None => {
                            color.insert(k, 1 - cu);
                            q.push_back(k);
                        }
                        Some(&cv) => {
                            if cv == cu {
                                bipartite = false;
                            }
                        }
                    }
                }
            }
        }
    }

    if !bipartite {
        // Fallback: min(|X|,|Y|) of the component (the old, over-counting value).
        let cnt0 = comp.iter().filter(|&&k| color.get(&k).copied() == Some(0)).count();
        let cnt1 = comp.len() - cnt0;
        return cnt0.min(cnt1);
    }

    // 3. Kuhn's augmenting-path maximum matching on the bipartition.
    let left: Vec<u64> = comp.iter().copied().filter(|&k| color[&k] == 0).collect();
    let right: Vec<u64> = comp.iter().copied().filter(|&k| color[&k] == 1).collect();
    let mut right_local: HashMap<u64, usize> = HashMap::new();
    for (i, &k) in right.iter().enumerate() {
        right_local.insert(k, i);
    }
    // right key -> matched left key.
    let mut match_right: HashMap<u64, u64> = HashMap::new();

    // Iterative DFS augmenting path to avoid recursion depth issues.
    fn try_augment(
        u: u64,
        block_adj: &HashMap<u64, Vec<Node>>,
        match_right: &mut HashMap<u64, u64>,
        visited_right: &mut HashSet<u64>,
        right_local: &HashMap<u64, usize>,
    ) -> bool {
        if let Some(neigh) = block_adj.get(&u) {
            for &n in neigh {
                let k = encode_node(n.x, n.y);
                if !right_local.contains_key(&k) {
                    continue; // not on the right side
                }
                if visited_right.contains(&k) {
                    continue;
                }
                visited_right.insert(k);
                let can = match match_right.get(&k) {
                    None => true,
                    Some(&m) => try_augment(m, block_adj, match_right, visited_right, right_local),
                };
                if can {
                    match_right.insert(k, u);
                    return true;
                }
            }
        }
        false
    }

    let mut matching = 0usize;
    for &u in &left {
        let mut visited_right: HashSet<u64> = HashSet::new();
        if try_augment(u, block_adj, &mut match_right, &mut visited_right, &right_local) {
            matching += 1;
        }
    }
    matching
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
    // H1: per connected component of the block-line graph, the cells that must
    // be excluded = minimum vertex cover = maximum bipartite matching size τ
    // (Kőnig).  The old `min(|X|,|Y|)` over-counts excluded cells, pushing
    // `max_area_size` too low and pruning valid regions.  `place_visited` is
    // still cleared (kept for `try_place_id`'s fallback / future use) but the
    // per-component walk below uses its own `seen` set.
    let mut node_keys: Vec<u64> = Vec::new();
    for (a, b) in &pairs {
        node_keys.push(encode_node(a.x, a.y));
        node_keys.push(encode_node(b.x, b.y));
    }
    node_keys.sort_unstable();
    node_keys.dedup();
    let mut comp_seen: HashSet<u64> = HashSet::new();
    for &k in &node_keys {
        if comp_seen.contains(&k) {
            continue;
        }
        let tau = component_max_matching(k, &core.dfs_ctx.block_adj, &mut comp_seen);
        core.dfs_ctx.empty_block_line_count += tau;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// H1: the block-line graph is bipartite.  The minimum vertex cover equals
    /// the maximum bipartite matching size τ (Kőnig), which is ≤ min(|X|,|Y|).
    /// The old code used min(|X|,|Y|), over-counting excluded cells.
    ///
    /// Double-star graph:
    ///   X (color 0) = {x1,x2,x3,x4},  Y (color 1) = {y1,y2,y3}
    ///   y1 hub: y1-x1,x2,x3,x4 ;  y2-x1 ;  y3-x1
    /// min(|X|,|Y|) = 3, but maximum matching = 2 (only two of {y1,y2,y3} can
    /// be matched because x1 is the sole neighbour of y2 and y3).
    #[test]
    fn test_max_bipartite_matching_double_star() {
        let mut adj: HashMap<u64, Vec<Node>> = HashMap::new();
        let x1 = Node { x: 0, y: 0 };
        let x2 = Node { x: 0, y: 1 };
        let x3 = Node { x: 0, y: 2 };
        let x4 = Node { x: 0, y: 3 };
        let y1 = Node { x: 1, y: 0 };
        let y2 = Node { x: 1, y: 1 };
        let y3 = Node { x: 1, y: 2 };
        let add = |adj: &mut HashMap<u64, Vec<Node>>, a: Node, b: Node| {
            adj.entry(encode_node(a.x, a.y)).or_default().push(b);
            adj.entry(encode_node(b.x, b.y)).or_default().push(a);
        };
        add(&mut adj, y1, x1);
        add(&mut adj, y1, x2);
        add(&mut adj, y1, x3);
        add(&mut adj, y1, x4);
        add(&mut adj, y2, x1);
        add(&mut adj, y3, x1);

        let mut seen: HashSet<u64> = HashSet::new();
        let tau = component_max_matching(encode_node(x1.x, x1.y), &adj, &mut seen);
        assert_eq!(tau, 2, "max matching must be 2, not min(|X|,|Y|)=3");
    }

    /// Sanity: a single star K_{1,3} has matching size 1 (== min here).
    #[test]
    fn test_max_bipartite_matching_star() {
        let mut adj: HashMap<u64, Vec<Node>> = HashMap::new();
        let c = Node { x: 5, y: 5 };
        let l1 = Node { x: 6, y: 5 };
        let l2 = Node { x: 7, y: 5 };
        let l3 = Node { x: 8, y: 5 };
        let add = |adj: &mut HashMap<u64, Vec<Node>>, a: Node, b: Node| {
            adj.entry(encode_node(a.x, a.y)).or_default().push(b);
            adj.entry(encode_node(b.x, b.y)).or_default().push(a);
        };
        add(&mut adj, c, l1);
        add(&mut adj, c, l2);
        add(&mut adj, c, l3);
        let mut seen: HashSet<u64> = HashSet::new();
        let tau = component_max_matching(encode_node(c.x, c.y), &adj, &mut seen);
        assert_eq!(tau, 1);
    }
}
