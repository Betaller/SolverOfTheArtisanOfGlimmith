//! Main search: `place_non_predifined_shape` and the recursive `dfs`
//! (dfs.cpp equivalents).

use std::cell::RefMut;
use std::time::Instant;

use super::types::*;
use super::types::{Node, Pools, PlaceLevel};

use super::core::{
    check_edge, check_edge_shape, check_loopy, check_nearby_shape, check_nearby_size,
    check_palisade_type1, check_palisade_type2, check_radar, check_tatami, sp_at, AoGCore,
};
use super::empty::{dfs_empty_area, empty_area_check, empty_area_size_range, find_special_start_area};

// ── Helpers for mark_size filtering ─────────────────────────────────────────

/// Skip the slash-distance prune when the tuple enumeration would be too large
/// (mirrors dfs.cpp's O(nodes^types) loop, bounded to avoid slowdowns).
const SLASH_DIST_TUPLE_CAP: usize = 16_384;

#[inline]
fn filter_size_range(mk: &mut [bool], lower: i32, upper: i32, neighbor_size: i32, larger_side: bool) {
    if larger_side {
        for i in neighbor_size..=upper {
            if i >= 0 && (i as usize) < mk.len() {
                mk[i as usize] = false;
            }
        }
    } else {
        for i in lower.max(0)..=neighbor_size {
            if (i as usize) < mk.len() {
                mk[i as usize] = false;
            }
        }
    }
}

#[inline]
fn filter_size_diff_value(mk: &mut [bool], lower: i32, upper: i32, neighbor_size: i32, diff: i32) {
    for i in lower.max(0)..=upper {
        if (i as usize) < mk.len() && i != neighbor_size + diff && i != neighbor_size - diff {
            mk[i as usize] = false;
        }
    }
}

// ── place_non_predifined_shape: iterative DFS expansion of a free shape ──────

#[allow(clippy::too_many_arguments)]
fn place_non_predifined_shape(
    index: u32,
    x: i32,
    y: i32,
    size: u32,
    up_left_seq: bool,
    known_shape_index: i32,
    core: &mut AoGCore,
    sp: &mut Vec<Vec<u32>>,
    pools: &Pools,
) -> i32 {
    if (index as usize) >= MAX_DFS_DEPTH {
        return -1;
    }

    let mut L: RefMut<PlaceLevel> = pools.place_level(index as usize);
    let mut rose_syms = [0i32; 16];

    // Slash-distance table geometry (mirrors dfs.cpp sd dimensions).
    let sd_ref_len = if core.rose_type_count > 0 {
        core.slash_nodes.first().map(|v| v.len()).unwrap_or(0)
    } else {
        0
    };
    let sd_dim2 = core.rose_type_count + 1;
    let sd_dim3 = sd_ref_len + 1;
    let sd_stride2 = sd_dim3;
    let sd_stride1 = sd_dim2 * sd_dim3;
    let sd_need = MAX_SHAPE_SIZE * sd_dim2 * sd_dim3;

    L.mark_slash = [false; 16];
    L.current_shape_cnt = 0;
    L.expand_candidates_cnt = 0;
    L.palisade_visited_cnt = 0;
    L.compass_visited_cnt = 0;
    L.symbol_loc = None;
    L.stack_top = 0;

    L.expand_candidates[0] = Node { x: 0, y: 0 };
    L.expand_candidates_distance[0] = 0;
    L.expand_candidates_cnt = 1;

    L.stack_size[0] = 0;
    L.stack_expand_distance_lb[0] = 0;
    L.stack_expand_x_lb[0] = 0;
    L.stack_expand_y_lb[0] = 0;
    L.stack_candidates_i[0] = 0;
    L.stack_candidates_size[0] = 1;
    L.stack_top = 1;

    let mut steps: u64 = 0;

    while L.stack_top > 0 {
        let current_size = L.stack_size[L.stack_top - 1];
        let expand_distance_lb = L.stack_expand_distance_lb[L.stack_top - 1];
        let expand_x_lb = L.stack_expand_x_lb[L.stack_top - 1];
        let expand_y_lb = L.stack_expand_y_lb[L.stack_top - 1];
        let mut candidates_i = L.stack_candidates_i[L.stack_top - 1];
        let candidates_size = L.stack_candidates_size[L.stack_top - 1];
        L.stack_top -= 1;

        steps += 1;
        if steps % 4096 == 0 && Instant::now() >= core.deadline {
            return -1;
        }
        if crate::aog_debug_enabled() && steps % 100_000 == 0 {
            eprintln!("aog place steps={} index={} cnt={} stack={}", steps, index, L.current_shape_cnt, L.stack_top);
        }

        // Roll back shape cells beyond current_size.
        while L.current_shape_cnt > current_size {
            let last = L.current_shape_cnt - 1;
            let temp_x = x + L.current_shape[last].x;
            let temp_y = y + L.current_shape[last].y;
            let tpx = to_puzzle_x(temp_x) as usize;
            let tpy = to_puzzle_y(temp_y) as usize;
            if crate::aog_debug_enabled() {
                let prev = sp[tpx][tpy];
                if prev != AREA_NORMAL && prev & SOLVE_AREA_SHAPE_INDEX_BIT != 0 {
                    eprintln!("aog ROLLBACK cell=({},{}) sp={:08x} cnt={} cur={}", tpx, tpy, prev, L.current_shape_cnt, current_size);
                }
            }
            sp[tpx][tpy] = AREA_NORMAL;
            if core.rose_type_count > 0 {
                let pv = core.puzzle[tpx][tpy];
                if (pv & AREA_SYMBOL_BIT) != 0 {
                    rose_syms[symbol_type_idx(pv)] -= 1;
                }
            }
            let pv = core.puzzle[tpx][tpy];
            if (pv & AREA_PALISADE_INDEX_BIT) != 0 {
                L.palisade_visited_cnt -= 1;
            }
            if (pv & AREA_COMPASS_ENABLE) != 0 {
                L.compass_visited_cnt -= 1;
            }
            if core.config.one_symbol_per_region {
                if let Some(loc) = L.symbol_loc {
                    if loc.x == temp_x && loc.y == temp_y {
                        L.symbol_loc = None;
                    }
                }
            }
            for i in 0..L.compass_visited_cnt {
                let cs = L.current_shape[last];
                if cs.x < L.compass_visited[i].x {
                    L.compass_visited_up_cnt[i] -= 1;
                }
                if cs.x > L.compass_visited[i].x {
                    L.compass_visited_down_cnt[i] -= 1;
                }
                if cs.y < L.compass_visited[i].y {
                    L.compass_visited_left_cnt[i] -= 1;
                }
                if cs.y > L.compass_visited[i].y {
                    L.compass_visited_right_cnt[i] -= 1;
                }
            }
            L.current_shape_cnt -= 1;
        }
        L.expand_candidates_cnt = candidates_size;

        if current_size == size as usize {
            // Full shape reached: check and commit.
            let mut no_rectangle_fail = false;
            if core.config.no_rectangles {
                let c = L.current_shape_cnt - 1;
                let rect_w = L.rectangle_right[c] - L.rectangle_left[c] + 1;
                let rect_h = L.rectangle_down[c] - L.rectangle_up[c] + 1;
                if (rect_w * rect_h) == size as i32 {
                    no_rectangle_fail = true;
                }
            }
            let mut one_symbol_fail = false;
            if core.config.one_symbol_per_region && !L.symbol_loc.is_some() {
                one_symbol_fail = true;
            }
            let mut rose_fail = false;
            if core.rose_type_count > 0 {
                for t in 0..core.rose_type_count {
                    if rose_syms[t] != 1 {
                        rose_fail = true;
                        break;
                    }
                }
            }
            if no_rectangle_fail || one_symbol_fail || rose_fail {
                continue;
            }

            // Build the shape into a minimal bounding square grid.
            let mut shape_buf = vec![vec![0u32; size as usize]; size as usize];
            let mut start_x = 1000;
            let mut start_y = 1000;
            for i in 0..L.current_shape_cnt {
                start_x = start_x.min(L.current_shape[i].x);
                start_y = start_y.min(L.current_shape[i].y);
            }
            let mut shape_size = 0usize;
            for i in 0..L.current_shape_cnt {
                let sx = (L.current_shape[i].x - start_x) as usize;
                let sy = (L.current_shape[i].y - start_y) as usize;
                shape_buf[sx][sy] = 1;
                shape_size = shape_size.max(sx.max(sy) + 1);
            }

            let mut shape_index = core.shapes_search(&shape_buf, shape_size);
            if crate::aog_debug_enabled() && shape_size <= 3 {
                let g: Vec<String> = shape_buf
                    .iter()
                    .take(shape_size)
                    .map(|row| {
                        row.iter()
                            .take(shape_size)
                            .map(|v| if *v == 1 { "#" } else { "." })
                            .collect::<String>()
                    })
                    .collect();
                eprintln!(
                    "aog shapebuf index={} size={} grid=[{}] search={}",
                    index, shape_size, g.join("|"), shape_index
                );
            }
            if shape_index != NO_SHAPE_INDEX {
                if up_left_seq && (shape_index as i32) <= known_shape_index {
                    continue;
                }
                if core.config.all_shapes_different
                    && core
                        .all_shapes_different_check_shape_index_pool
                        .contains(&shape_index)
                {
                    continue;
                }
            } else {
                core.shapes_insert(&mut shape_buf, shape_size);
                shape_index = core.shapes_search(&shape_buf, shape_size);
                if shape_index == NO_SHAPE_INDEX {
                    // Shape cap exhausted (shapes_insert returned 0) AND the
                    // shape is not already in the library. Skip this placement
                    // entirely — never write NO_SHAPE_INDEX (0xffff) into sp,
                    // which would later index `shape_size_by_index` out of
                    // bounds and panic in check_nearby_size / check_edge_shape
                    // / region_size_at. Skipping here is equivalent to "this
                    // shape is not tried", same as the fail-rollback `continue`
                    // below (line 355) but needing no sp cleanup, since the sp
                    // write at line 253 has not yet executed.
                    continue;
                }
            }

            for i in 0..L.current_shape_cnt {
                let nx = to_puzzle_x(x + L.current_shape[i].x) as usize;
                let ny = to_puzzle_y(y + L.current_shape[i].y) as usize;
                sp[nx][ny] |= shape_index << SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT;
            }

            let mut shape_check_fail = false;
            let mut nearby_shape_fail = false;
            let mut nearby_size_fail = false;
            let mut shape_in_puzzle_fail = false;
            let mut tatami_fail = false;
            let mut loopy_fail = false;
            let mut radar_fail = false;
            for i in 0..L.current_shape_cnt {
                let nx = to_puzzle_x(x + L.current_shape[i].x) as usize;
                let ny = to_puzzle_y(y + L.current_shape[i].y) as usize;
                let nxi = nx as i32;
                let nyi = ny as i32;
                if !check_edge_shape(nxi, nyi, core, sp) {
                    shape_check_fail = true;
                    break;
                }
                if core.config.adjacent_shapes_different && !check_nearby_shape(nxi, nyi, core, sp) {
                    nearby_shape_fail = true;
                    break;
                }
                if core.config.adjacent_sizes_different && !check_nearby_size(nxi, nyi, core, sp) {
                    nearby_size_fail = true;
                    break;
                }
                if (core.puzzle[nx][ny] & AREA_SHAPE_INDEX_BIT) != 0 {
                    if (sp[nx][ny] >> SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT)
                        != ((core.puzzle[nx][ny] & AREA_SHAPE_INDEX_BIT) >> AREA_SHAPE_INDEX_BIT_SHIFT)
                    {
                        shape_in_puzzle_fail = true;
                        break;
                    }
                }
                if core.config.no_4_way_intersections && !check_tatami(nxi, nyi, core, sp) {
                    tatami_fail = true;
                    break;
                }
                if core.config.no_3_way_intersections && !check_loopy(nxi, nyi, core, sp) {
                    loopy_fail = true;
                    break;
                }
                if core.config.has_watchtower && !check_radar(nxi, nyi, core, sp) {
                    radar_fail = true;
                    break;
                }
            }
            let mut palisade_fail = false;
            for i in 0..L.palisade_visited_cnt {
                let nx = to_puzzle_x(x + L.palisade_visited[i].x) as usize;
                let ny = to_puzzle_y(y + L.palisade_visited[i].y) as usize;
                if !check_palisade_type2(nx as i32, ny as i32, core, sp) {
                    palisade_fail = true;
                    break;
                }
            }
            let mut compass_fail = false;
            for i in 0..L.compass_visited_cnt {
                let nx = to_puzzle_x(x + L.compass_visited[i].x) as usize;
                let ny = to_puzzle_y(y + L.compass_visited[i].y) as usize;
                if core.puzzle_compass_up[nx][ny] != -1
                    && core.puzzle_compass_up[nx][ny] != L.compass_visited_up_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_down[nx][ny] != -1
                    && core.puzzle_compass_down[nx][ny] != L.compass_visited_down_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_left[nx][ny] != -1
                    && core.puzzle_compass_left[nx][ny] != L.compass_visited_left_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_right[nx][ny] != -1
                    && core.puzzle_compass_right[nx][ny] != L.compass_visited_right_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
            }

            if shape_check_fail
                || nearby_shape_fail
                || shape_in_puzzle_fail
                || nearby_size_fail
                || palisade_fail
                || tatami_fail
                || loopy_fail
                || radar_fail
                || compass_fail
            {
                for i in 0..L.current_shape_cnt {
                    let nx = to_puzzle_x(x + L.current_shape[i].x) as usize;
                    let ny = to_puzzle_y(y + L.current_shape[i].y) as usize;
                    sp[nx][ny] &= !SOLVE_AREA_SHAPE_INDEX_BIT;
                }
                continue;
            }

            let ret: i32;
            if !empty_area_check(core, sp) {
                ret = -1;
            } else {
                if core.config.all_shapes_same {
                    core.all_shapes_same_check_shape_index = shape_index as i32;
                }
                core.all_shapes_different_check_shape_index_pool.insert(shape_index);
                ret = dfs(index + 1, core, sp, pools);
                if core.config.all_shapes_same {
                    core.all_shapes_same_check_shape_index = -1;
                }
            }
            if ret != -1 {
                if crate::aog_debug_enabled() {
                    for i in 0..L.current_shape_cnt {
                        let nx = to_puzzle_x(x + L.current_shape[i].x) as usize;
                        let ny = to_puzzle_y(y + L.current_shape[i].y) as usize;
                        eprintln!(
                            "aog commit idx={} shape_idx={} cell=({},{}) sp={:08x}",
                            index, shape_index, nx, ny, sp[nx][ny]
                        );
                    }
                }
                return ret;
            } else {
                core.all_shapes_different_check_shape_index_pool.remove(&shape_index);
                for i in 0..L.current_shape_cnt {
                    let nx = to_puzzle_x(x + L.current_shape[i].x) as usize;
                    let ny = to_puzzle_y(y + L.current_shape[i].y) as usize;
                    sp[nx][ny] &= !SOLVE_AREA_SHAPE_INDEX_BIT;
                }
            }
            continue;
        }

        // Expand: try each candidate cell.
        while candidates_i < candidates_size {
            let expand_x = x + L.expand_candidates[candidates_i].x;
            let expand_y = y + L.expand_candidates[candidates_i].y;
            let expand_distance = L.expand_candidates_distance[candidates_i];

            // Ordered search: skip candidates before the lower bound.
            let mut jump = false;
            if expand_distance < expand_distance_lb {
                candidates_i += 1;
                jump = true;
            } else if expand_distance == expand_distance_lb {
                if expand_x < expand_x_lb {
                    candidates_i += 1;
                    jump = true;
                } else if expand_x == expand_x_lb && expand_y <= expand_y_lb {
                    candidates_i += 1;
                    jump = true;
                }
            }
            if jump {
                let skipped = L.expand_candidates[candidates_i - 1];
                let mut in_current_shape = false;
                for j in 0..L.current_shape_cnt {
                    if L.current_shape[j].x == skipped.x && L.current_shape[j].y == skipped.y {
                        in_current_shape = true;
                        break;
                    }
                }
                if !in_current_shape
                    && (core.config.shape_size_lower_bound > 1 || core.rose_type_count > 0)
                {
                    dfs_empty_area(expand_x, expand_y, core, sp);
                    let max_area_size =
                        core.dfs_ctx.empty_count - core.dfs_ctx.empty_block_line_count;
                    if max_area_size < core.config.shape_size_lower_bound as usize {
                        break;
                    }
                    if core.rose_type_count > 0 {
                        let first = core.dfs_ctx.slash_count[0];
                        let mut bad = false;
                        for t in 1..core.rose_type_count {
                            if (core.dfs_ctx.slash_count[t] - first).abs() > 1 {
                                bad = true;
                                break;
                            }
                        }
                        if bad {
                            break;
                        }
                    }
                }
                continue;
            }

            // Shape-size target check.
            let epx = to_puzzle_x(expand_x) as usize;
            let epy = to_puzzle_y(expand_y) as usize;
            let epv = core.puzzle[epx][epy];
            if (epv & AREA_SHAPE_SIZE_BIT) != 0 {
                let target_size = (epv & AREA_SHAPE_SIZE_BIT) >> AREA_SHAPE_SIZE_BIT_SHIFT;
                if target_size != size {
                    candidates_i += 1;
                    continue;
                }
            }

            sp[epx][epy] = index;
            if !check_edge(epx as i32, epy as i32, core, sp) {
                sp[epx][epy] = AREA_NORMAL;
                candidates_i += 1;
                continue;
            }

            // rose_window: reject a second symbol of the same type in one region.
            if core.rose_type_count > 0 && (epv & AREA_SYMBOL_BIT) != 0 {
                let t = symbol_type_idx(epv);
                if rose_syms[t] >= 1 {
                    sp[epx][epy] = AREA_NORMAL;
                    candidates_i += 1;
                    continue;
                }
                rose_syms[t] += 1;
            }

            let cn = L.current_shape_cnt;
            let cand = L.expand_candidates[candidates_i];
            L.current_shape[cn] = Node {
                x: cand.x,
                y: cand.y,
            };
            if core.config.only_rectangles || core.config.no_rectangles {
                if cn == 0 {
                    L.rectangle_up[0] = L.current_shape[0].x;
                    L.rectangle_down[0] = L.current_shape[0].x;
                    L.rectangle_left[0] = L.current_shape[0].y;
                    L.rectangle_right[0] = L.current_shape[0].y;
                } else {
                    L.rectangle_up[cn] = L.rectangle_up[cn - 1].min(L.current_shape[cn].x);
                    L.rectangle_down[cn] = L.rectangle_down[cn - 1].max(L.current_shape[cn].x);
                    L.rectangle_left[cn] = L.rectangle_left[cn - 1].min(L.current_shape[cn].y);
                    L.rectangle_right[cn] = L.rectangle_right[cn - 1].max(L.current_shape[cn].y);
                }
            }
            for i in 0..L.compass_visited_cnt {
                let cs = L.current_shape[cn];
                if cs.x < L.compass_visited[i].x {
                    L.compass_visited_up_cnt[i] += 1;
                }
                if cs.x > L.compass_visited[i].x {
                    L.compass_visited_down_cnt[i] += 1;
                }
                if cs.y < L.compass_visited[i].y {
                    L.compass_visited_left_cnt[i] += 1;
                }
                if cs.y > L.compass_visited[i].y {
                    L.compass_visited_right_cnt[i] += 1;
                }
            }

            // Closest-slash-node table (sd) — mirrors dfs.cpp slash sd computation.
            if core.rose_type_count > 0
                && sd_ref_len > 0
                && sd_ref_len.saturating_pow(core.rose_type_count as u32) <= SLASH_DIST_TUPLE_CAP
            {
                if L.slash_dist_buf.len() < sd_need {
                    L.slash_dist_buf.resize(sd_need, 0);
                }
                let pos = L.current_shape_cnt;
                for t in 0..core.rose_type_count {
                    let nodes_t = &core.slash_nodes[t];
                    let node_len = nodes_t.len();
                    if node_len == 0 {
                        continue;
                    }
                    for j in 0..sd_ref_len {
                        let jj = j.min(node_len - 1);
                        let sd_i = pos * sd_stride1 + t * sd_stride2 + j;
                        if pos == 0 {
                            L.slash_dist_buf[sd_i] = 0;
                            continue;
                        }
                        let prev = L.slash_dist_buf[(pos - 1) * sd_stride1 + t * sd_stride2 + j];
                        L.slash_dist_buf[sd_i] = prev;
                        let best = prev as usize;
                        let sn = nodes_t[jj];
                        let new_d =
                            (x + L.current_shape[pos].x - sn.x).abs() + (y + L.current_shape[pos].y - sn.y).abs();
                        let old_d =
                            (x + L.current_shape[best].x - sn.x).abs() + (y + L.current_shape[best].y - sn.y).abs();
                        if new_d < old_d {
                            L.slash_dist_buf[sd_i] = pos as i32;
                        }
                    }
                }
            }

            L.current_shape_cnt += 1;

            if (epv & AREA_PALISADE_INDEX_BIT) != 0 {
                let pvc = L.palisade_visited_cnt;
                L.palisade_visited[pvc] = cand;
                L.palisade_visited_cnt += 1;
            }
            if (epv & AREA_COMPASS_ENABLE) != 0 {
                let cc = L.compass_visited_cnt;
                L.compass_visited[cc] = cand;
                L.compass_visited_up_cnt[cc] = 0;
                L.compass_visited_down_cnt[cc] = 0;
                L.compass_visited_left_cnt[cc] = 0;
                L.compass_visited_right_cnt[cc] = 0;
                for w in 0..L.current_shape_cnt {
                    let cs = L.current_shape[w];
                    if cs.x < L.compass_visited[cc].x {
                        L.compass_visited_up_cnt[cc] += 1;
                    }
                    if cs.x > L.compass_visited[cc].x {
                        L.compass_visited_down_cnt[cc] += 1;
                    }
                    if cs.y < L.compass_visited[cc].y {
                        L.compass_visited_left_cnt[cc] += 1;
                    }
                    if cs.y > L.compass_visited[cc].y {
                        L.compass_visited_right_cnt[cc] += 1;
                    }
                }
                L.compass_visited_cnt += 1;
            }

            // Incremental checks.
            let mut rectangle_fail = false;
            if core.config.only_rectangles {
                let c = L.current_shape_cnt - 1;
                let rect_w = L.rectangle_right[c] - L.rectangle_left[c] + 1;
                let rect_h = L.rectangle_down[c] - L.rectangle_up[c] + 1;
                if (rect_w * rect_h) > size as i32 {
                    rectangle_fail = true;
                }
            }
            let mut palisade_fail = false;
            for i in 0..L.palisade_visited_cnt {
                let nx = to_puzzle_x(x + L.palisade_visited[i].x) as usize;
                let ny = to_puzzle_y(y + L.palisade_visited[i].y) as usize;
                if !check_palisade_type1(nx as i32, ny as i32, core, sp) {
                    palisade_fail = true;
                    break;
                }
            }
            let mut compass_fail = false;
            for i in 0..L.compass_visited_cnt {
                let nx = to_puzzle_x(x + L.compass_visited[i].x) as usize;
                let ny = to_puzzle_y(y + L.compass_visited[i].y) as usize;
                if core.puzzle_compass_up[nx][ny] != -1
                    && core.puzzle_compass_up[nx][ny] < L.compass_visited_up_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_down[nx][ny] != -1
                    && core.puzzle_compass_down[nx][ny] < L.compass_visited_down_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_left[nx][ny] != -1
                    && core.puzzle_compass_left[nx][ny] < L.compass_visited_left_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_right[nx][ny] != -1
                    && core.puzzle_compass_right[nx][ny] < L.compass_visited_right_cnt[i]
                {
                    compass_fail = true;
                    break;
                }
            }

            let mut slash_distance_fail = false;
            if core.rose_type_count > 0 && sd_ref_len > 0 && L.slash_dist_buf.len() >= sd_need
                && sd_ref_len.saturating_pow(core.rose_type_count as u32) <= SLASH_DIST_TUPLE_CAP
            {
                // Geometric lower bound: the shape must still reach one node of
                // every unmarked type; if the minimal L1 diameter of the closest
                // offsets exceeds the remaining cells, prune (dfs.cpp 1260-1306).
                L.slash_node_indexs = [0; 16];
                let mut distance_predict: i32 = 0x0fff_ffff;
                'enum_tuples: loop {
                    let mut minx = 0i32;
                    let mut maxx = 0i32;
                    let mut miny = 0i32;
                    let mut maxy = 0i32;
                    for t in 0..core.rose_type_count {
                        if rose_syms[t] >= 1 {
                            continue;
                        }
                        let nodes_t = &core.slash_nodes[t];
                        if nodes_t.is_empty() {
                            continue;
                        }
                        let jj = L.slash_node_indexs[t].min(nodes_t.len() - 1);
                        let best = L.slash_dist_buf
                            [(L.current_shape_cnt - 1) * sd_stride1 + t * sd_stride2 + jj]
                            as usize;
                        let sn = nodes_t[jj];
                        let _x = x + L.current_shape[best].x - sn.x;
                        let _y = y + L.current_shape[best].y - sn.y;
                        minx = minx.min(_x);
                        maxx = maxx.max(_x);
                        miny = miny.min(_y);
                        maxy = maxy.max(_y);
                    }
                    distance_predict = distance_predict.min(maxx + maxy - minx - miny);

                    L.slash_node_indexs[0] += 1;
                    let mut loc = 0usize;
                    let mut final_flag = false;
                    loop {
                        if L.slash_node_indexs[loc] >= sd_ref_len {
                            L.slash_node_indexs[loc] = 0;
                            if loc + 1 < core.rose_type_count {
                                L.slash_node_indexs[loc + 1] += 1;
                                loc += 1;
                            } else {
                                final_flag = true;
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if final_flag {
                        break 'enum_tuples;
                    }
                }
                let remain_size = size as i32 - L.current_shape_cnt as i32;
                if distance_predict > remain_size {
                    slash_distance_fail = true;
                }
            }

            if palisade_fail || rectangle_fail || compass_fail || slash_distance_fail {
                sp[epx][epy] = AREA_NORMAL;
                L.current_shape_cnt -= 1;
                candidates_i += 1;
                if core.rose_type_count > 0 && (epv & AREA_SYMBOL_BIT) != 0 {
                    rose_syms[symbol_type_idx(epv)] -= 1;
                }
                if (epv & AREA_PALISADE_INDEX_BIT) != 0 {
                    L.palisade_visited_cnt -= 1;
                }
                if (epv & AREA_COMPASS_ENABLE) != 0 {
                    L.compass_visited_cnt -= 1;
                }
                let popped = L.current_shape[L.current_shape_cnt];
                for i in 0..L.compass_visited_cnt {
                    if popped.x < L.compass_visited[i].x {
                        L.compass_visited_up_cnt[i] -= 1;
                    }
                    if popped.x > L.compass_visited[i].x {
                        L.compass_visited_down_cnt[i] -= 1;
                    }
                    if popped.y < L.compass_visited[i].y {
                        L.compass_visited_left_cnt[i] -= 1;
                    }
                    if popped.y > L.compass_visited[i].y {
                        L.compass_visited_right_cnt[i] -= 1;
                    }
                }
                continue;
            }

            if core.config.one_symbol_per_region {
                if core.area_contain_symbol(expand_x, expand_y) {
                    if !L.symbol_loc.is_some() {
                        L.symbol_loc = Some(Node {
                            x: expand_x,
                            y: expand_y,
                        });
                    } else {
                        sp[epx][epy] = AREA_NORMAL;
                        L.current_shape_cnt -= 1;
                        candidates_i += 1;
                        if core.rose_type_count > 0 && (epv & AREA_SYMBOL_BIT) != 0 {
                            rose_syms[symbol_type_idx(epv)] -= 1;
                        }
                        if (epv & AREA_PALISADE_INDEX_BIT) != 0 {
                            L.palisade_visited_cnt -= 1;
                        }
                        if (epv & AREA_COMPASS_ENABLE) != 0 {
                            L.compass_visited_cnt -= 1;
                        }
                        let popped = L.current_shape[L.current_shape_cnt];
                        for i in 0..L.compass_visited_cnt {
                            if popped.x < L.compass_visited[i].x {
                                L.compass_visited_up_cnt[i] -= 1;
                            }
                            if popped.x > L.compass_visited[i].x {
                                L.compass_visited_down_cnt[i] -= 1;
                            }
                            if popped.y < L.compass_visited[i].y {
                                L.compass_visited_left_cnt[i] -= 1;
                            }
                            if popped.y > L.compass_visited[i].y {
                                L.compass_visited_right_cnt[i] -= 1;
                            }
                        }
                        continue;
                    }
                }
            }

            // Add new neighbour cells to the expansion candidates.
            let base = L.expand_candidates[candidates_i];
            let dxs = [0, 0, -1, 1];
            let dys = [-1, 1, 0, 0];
            for d in 0..4 {
                let new_x = base.x + dxs[d];
                let new_y = base.y + dys[d];
                let npx = to_puzzle_x(x + new_x) as usize;
                let npy = to_puzzle_y(y + new_y) as usize;
                if core.puzzle[npx][npy] == AREA_BLOCK {
                    continue;
                }
                if sp[npx][npy] != AREA_NORMAL {
                    continue;
                }
                let mut already = false;
                for j in 0..L.current_shape_cnt {
                    if L.current_shape[j].x == new_x && L.current_shape[j].y == new_y {
                        already = true;
                        break;
                    }
                }
                for j in 0..L.expand_candidates_cnt {
                    if !already
                        && L.expand_candidates[j].x == new_x
                        && L.expand_candidates[j].y == new_y
                    {
                        already = true;
                        break;
                    }
                }
                if already {
                    continue;
                }
                if L.expand_candidates_cnt >= MAX_EXPAND_CANDIDATES {
                    continue;
                }
                let ecnt = L.expand_candidates_cnt;
                L.expand_candidates[ecnt] = Node { x: new_x, y: new_y };
                L.expand_candidates_distance[ecnt] = expand_distance + 1;
                L.expand_candidates_cnt += 1;
            }

            // Push two stack frames: skip this candidate, then expand through it.
            if L.stack_top + 2 >= MAX_STACK_SIZE {
                return -1;
            }
            let st = L.stack_top;
            L.stack_size[st] = current_size;
            L.stack_expand_distance_lb[st] = expand_distance_lb;
            L.stack_expand_x_lb[st] = expand_x_lb;
            L.stack_expand_y_lb[st] = expand_y_lb;
            L.stack_candidates_i[st] = candidates_i + 1;
            L.stack_candidates_size[st] = candidates_size;
            L.stack_top += 1;

            let st = L.stack_top;
            L.stack_size[st] = current_size + 1;
            L.stack_expand_distance_lb[st] = expand_distance;
            L.stack_expand_x_lb[st] = expand_x;
            L.stack_expand_y_lb[st] = expand_y;
            L.stack_candidates_i[st] = 0;
            L.stack_candidates_size[st] = L.expand_candidates_cnt;
            L.stack_top += 1;
            break;
        }
    }

    -1
}

// ── dfs: main recursive search ───────────────────────────────────────────────

pub fn dfs(index: u32, core: &mut AoGCore, sp: &mut Vec<Vec<u32>>, pools: &Pools) -> i32 {
    if (index as usize) >= MAX_DFS_DEPTH {
        return -1;
    }
    if Instant::now() >= core.deadline {
        return -1;
    }

    let (ret, x, y) = find_special_start_area(core, sp);
    if ret == SPECIAL_START_DEFAULT && x == -1 && y == -1 {
        return 0;
    }
    if crate::aog_debug_enabled() {
        eprintln!("dfs index={} ret={} x={} y={}", index, ret, x, y);
    }

    let mut mk_skip = pools.mark_skip[index as usize].borrow_mut();
    if ret == SPECIAL_START_DEFAULT {
        mk_skip.clear();
        mk_skip.resize(core.shapes.len(), false);
    }

    let mut compass_visited = [Node::default(); MAX_SHAPE_SIZE];
    // Initialized at its first (re)assignment in the shape-placement loop;
    // declared here to mirror the C++ `int compass_visited_cnt;`.
    let mut compass_visited_cnt;

    let mut mk_size = pools.mark_size[index as usize].borrow_mut();
    mk_size.clear();
    mk_size.resize(core.n_row * core.n_col + 1, false);

    let (range_check, rlb, rub) = empty_area_size_range(x, y, core, sp);
    let shape_size_lower_bound = rlb.max(core.config.shape_size_lower_bound);
    let shape_size_upper_bound = rub.min(core.config.shape_size_upper_bound);
    let n_row = core.n_row as i32;
    let n_col = core.n_col as i32;
    for i in shape_size_lower_bound..=shape_size_upper_bound {
        if i >= 0 && (i as usize) < mk_size.len() {
            mk_size[i as usize] = true;
        }
    }
    if range_check == -1 {
        return -1;
    }

    if ret == SPECIAL_START_AREA_SIZE {
        let px = to_puzzle_x(x) as usize;
        let py = to_puzzle_y(y) as usize;
        let target =
            (core.puzzle[px][py] & AREA_SHAPE_SIZE_BIT) >> AREA_SHAPE_SIZE_BIT_SHIFT;
        for i in shape_size_lower_bound..=shape_size_upper_bound {
            if i >= 0 && i as u32 != target && (i as usize) < mk_size.len() {
                mk_size[i as usize] = false;
            }
        }
    }

    if ret == SPECIAL_START_LINE_SMALLER_OR_LARGER {
        let px = to_puzzle_x(x) as usize;
        let py = to_puzzle_y(y) as usize;
        // Up
        if (core.puzzle[px - 1][py] & (LINE_LARGER | LINE_SMALLER)) != 0
            && sp[px - 2][py] != AREA_NORMAL
            && core.puzzle[px - 2][py] != AREA_BLOCK
        {
            let nsize = region_size_at(core, sp, px - 2, py) as i32;
            let larger = (core.puzzle[px - 1][py] & LINE_LARGER) != 0;
            filter_size_range(&mut mk_size, shape_size_lower_bound, shape_size_upper_bound, nsize, larger);
        }
        // Down
        if (core.puzzle[px + 1][py] & (LINE_LARGER | LINE_SMALLER)) != 0
            && sp[px + 2][py] != AREA_NORMAL
            && core.puzzle[px + 2][py] != AREA_BLOCK
        {
            let nsize = region_size_at(core, sp, px + 2, py) as i32;
            let larger = (core.puzzle[px + 1][py] & LINE_LARGER) != 0;
            filter_size_range(&mut mk_size, shape_size_lower_bound, shape_size_upper_bound, nsize, !larger);
        }
        // Left
        if (core.puzzle[px][py - 1] & (LINE_LARGER | LINE_SMALLER)) != 0
            && sp[px][py - 2] != AREA_NORMAL
            && core.puzzle[px][py - 2] != AREA_BLOCK
        {
            let nsize = region_size_at(core, sp, px, py - 2) as i32;
            let larger = (core.puzzle[px][py - 1] & LINE_LARGER) != 0;
            filter_size_range(&mut mk_size, shape_size_lower_bound, shape_size_upper_bound, nsize, larger);
        }
        // Right
        if (core.puzzle[px][py + 1] & (LINE_LARGER | LINE_SMALLER)) != 0
            && sp[px][py + 2] != AREA_NORMAL
            && core.puzzle[px][py + 2] != AREA_BLOCK
        {
            let nsize = region_size_at(core, sp, px, py + 2) as i32;
            let larger = (core.puzzle[px][py + 1] & LINE_LARGER) != 0;
            filter_size_range(&mut mk_size, shape_size_lower_bound, shape_size_upper_bound, nsize, !larger);
        }
    }

    if ret == SPECIAL_START_LINE_SIZE_DIFF {
        let px = to_puzzle_x(x) as usize;
        let py = to_puzzle_y(y) as usize;
        for (lx, ly, nx, ny) in [(-1, 0, px - 2, py), (1, 0, px + 2, py), (0, -1, px, py - 2), (0, 1, px, py + 2)] {
            let line_val = core.puzzle[(px as i32 + lx) as usize][(py as i32 + ly) as usize];
            if (line_val & LINE_SIZE_DIFF_BIT) != 0
                && sp[nx][ny] != AREA_NORMAL
                && core.puzzle[nx][ny] != AREA_BLOCK
            {
                // Mirror the C++ special start (dfs.cpp 1524-1545), which uses the
                // stored marker value directly (NOT minus one): the check there is
                // `i == neighbor ± stored`, i.e. one wider than the real difference,
                // and tightening it to the actual difference pruned valid sizes
                // (e.g. puzzle 0404's 2/3 split).  check_edge_shape still enforces
                // the exact difference later, so a looser filter is safe.
                let diff =
                    ((line_val & LINE_SIZE_DIFF_BIT) >> LINE_SIZE_DIFF_BIT_SHIFT) as i32;
                let nsize = region_size_at(core, sp, nx, ny) as i32;
                filter_size_diff_value(
                    &mut mk_size,
                    shape_size_lower_bound,
                    shape_size_upper_bound,
                    nsize,
                    diff,
                );
            }
        }
    }

    // Dynamic loop bound, mirroring the C++ reference (`for i = 0; i < shapes.size()`):
    // a deeper dfs may call shapes_insert while we iterate, and those newly added
    // shapes must ALSO be tried here (e.g. puzzle 0404's vertical domino for the
    // (2,0) corner is only added during the subtree of the size-1 placement).
    // Freezing the bound (as a `for i in 0..n_shapes`) made the search miss
    // regions that the C++ solver explores, so it never found the solution.
    let mut i = 0usize;
    // Deadline check inside the shape loop: each shape can run an expensive
    // placement (Type4 = whole-board empty_area_check, O(n²) + flood fill), so
    // without this a frame burns through every shape after its deadline.
    let mut shapes_seen: u32 = 0;
    while i < core.shapes.len() {
        shapes_seen += 1;
        if shapes_seen % 256 == 0 && Instant::now() >= core.deadline {
            return -1;
        }
        let cur = i;
        i += 1;
        // Shapes added during a deeper dfs grow the library beyond the size
        // mk_skip was resized to at this dfs's entry.  The C++ reference uses a
        // fixed-capacity array (mark_skip_shape[MARK_SKIP_CAP]); grow lazily to
        // match, defaulting new entries to not-skipped.
        if cur >= mk_skip.len() {
            mk_skip.resize(cur + 1, false);
        }
        if ret == SPECIAL_START_DEFAULT && mk_skip[cur] {
            continue;
        }
        if core.config.all_shapes_different
            && core
                .all_shapes_different_check_shape_index_pool
                .contains(&core.shapes[cur].shape_index)
        {
            continue;
        }
        if core.config.all_shapes_same
            && core.all_shapes_same_check_shape_index != -1
            && core.shapes[cur].shape_index != core.all_shapes_same_check_shape_index as u32
        {
            continue;
        }
        let sz = core.shapes[cur].nodes.len() as i32;
        if sz < shape_size_lower_bound || sz > shape_size_upper_bound {
            continue;
        }
        if !mk_size[core.shapes[cur].nodes.len()] {
            continue;
        }

        for p in 0..core.shapes[cur].nodes.len() {
            if ret == SPECIAL_START_DEFAULT && p != 0 {
                break;
            }
            let start_x = core.shapes[cur].nodes[p].x;
            let start_y = core.shapes[cur].nodes[p].y;

            // ── Type 1 check ──
            let mut fail = false;
            let mut one_symbol_check = false;
            let mut rose_marks = [0i32; 16];
            compass_visited_cnt = 0;
            let mut error_node: Option<Node> = None;
            for j in 0..core.shapes[cur].nodes.len() {
                let new_x = x + core.shapes[cur].nodes[j].x - start_x;
                let new_y = y + core.shapes[cur].nodes[j].y - start_y;
                if new_x < 1 || new_x > n_row || new_y < 1 || new_y > n_col {
                    fail = true;
                    break;
                }
                let pxn = to_puzzle_x(new_x) as usize;
                let pyn = to_puzzle_y(new_y) as usize;
                let pval = core.puzzle[pxn][pyn];
                if pval == AREA_BLOCK {
                    fail = true;
                    error_node = Some(core.shapes[cur].nodes[j]);
                    break;
                }
                if sp[pxn][pyn] != AREA_NORMAL {
                    fail = true;
                    error_node = Some(core.shapes[cur].nodes[j]);
                    break;
                }
                if core.rose_type_count > 0 && (pval & AREA_SYMBOL_BIT) != 0 {
                    let t = symbol_type_idx(pval);
                    if rose_marks[t] >= 1 {
                        fail = true;
                        break;
                    }
                    rose_marks[t] += 1;
                }
                if (pval & AREA_SHAPE_INDEX_BIT) != 0 {
                    let target_index = (pval & AREA_SHAPE_INDEX_BIT) >> AREA_SHAPE_INDEX_BIT_SHIFT;
                    if target_index != core.shapes[cur].shape_index {
                        fail = true;
                        break;
                    }
                }
                if (pval & AREA_SHAPE_SIZE_BIT) != 0 {
                    let target_size = (pval & AREA_SHAPE_SIZE_BIT) >> AREA_SHAPE_SIZE_BIT_SHIFT;
                    if target_size != core.shapes[cur].nodes.len() as u32 {
                        fail = true;
                        break;
                    }
                }
                if core.config.one_symbol_per_region {
                    if core.area_contain_symbol(new_x, new_y) {
                        if one_symbol_check {
                            fail = true;
                            break;
                        }
                        one_symbol_check = true;
                    }
                }
                if (pval & AREA_COMPASS_ENABLE) != 0 {
                    compass_visited[compass_visited_cnt] = Node { x: new_x, y: new_y };
                    compass_visited_cnt += 1;
                }
            }
            if let Some(nd) = error_node {
                if let Some(indices) = core.node_to_shape_index.get(&(nd.x, nd.y)) {
                    for &si in indices {
                        if si < mk_skip.len() {
                            mk_skip[si] = true;
                        }
                    }
                }
            }
            if fail {
                continue;
            }
            if core.config.one_symbol_per_region && !one_symbol_check {
                continue;
            }
            if core.rose_type_count > 0 {
                let mut rose_ok = true;
                for t in 0..core.rose_type_count {
                    if rose_marks[t] != 1 {
                        rose_ok = false;
                        break;
                    }
                }
                if !rose_ok {
                    continue;
                }
            }

            let mut compass_fail = false;
            for j in 0..compass_visited_cnt {
                let c = compass_visited[j];
                let cpx = to_puzzle_x(c.x) as usize;
                let cpy = to_puzzle_y(c.y) as usize;
                let mut states = CompassStates::default();
                for k in 0..core.shapes[cur].nodes.len() {
                    let new_x = x + core.shapes[cur].nodes[k].x - start_x;
                    let new_y = y + core.shapes[cur].nodes[k].y - start_y;
                    if new_x < c.x {
                        states.up += 1;
                    }
                    if new_x > c.x {
                        states.down += 1;
                    }
                    if new_y < c.y {
                        states.left += 1;
                    }
                    if new_y > c.y {
                        states.right += 1;
                    }
                }
                if core.puzzle_compass_up[cpx][cpy] != -1
                    && core.puzzle_compass_up[cpx][cpy] != states.up
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_down[cpx][cpy] != -1
                    && core.puzzle_compass_down[cpx][cpy] != states.down
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_left[cpx][cpy] != -1
                    && core.puzzle_compass_left[cpx][cpy] != states.left
                {
                    compass_fail = true;
                    break;
                }
                if core.puzzle_compass_right[cpx][cpy] != -1
                    && core.puzzle_compass_right[cpx][cpy] != states.right
                {
                    compass_fail = true;
                    break;
                }
            }
            if compass_fail {
                continue;
            }

            // ── Type 2 check: place cells ──
            let mut l = 0usize;
            let mut fail2 = false;
            while l < core.shapes[cur].nodes.len() {
                let new_x = x + core.shapes[cur].nodes[l].x - start_x;
                let new_y = y + core.shapes[cur].nodes[l].y - start_y;
                let pxn = to_puzzle_x(new_x) as usize;
                let pyn = to_puzzle_y(new_y) as usize;
                sp[pxn][pyn] = index | (core.shapes[cur].shape_index << SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT);
                if !check_edge(pxn as i32, pyn as i32, core, sp) {
                    fail2 = true;
                    break;
                }
                if !check_edge_shape(pxn as i32, pyn as i32, core, sp) {
                    fail2 = true;
                    break;
                }
                if core.config.adjacent_shapes_different
                    && !check_nearby_shape(pxn as i32, pyn as i32, core, sp)
                {
                    fail2 = true;
                    break;
                }
                if core.config.adjacent_sizes_different
                    && !check_nearby_size(pxn as i32, pyn as i32, core, sp)
                {
                    fail2 = true;
                    break;
                }
                l += 1;
            }
            if fail2 {
                for j in 0..=l {
                    let new_x = x + core.shapes[cur].nodes[j].x - start_x;
                    let new_y = y + core.shapes[cur].nodes[j].y - start_y;
                    sp[to_puzzle_x(new_x) as usize][to_puzzle_y(new_y) as usize] = AREA_NORMAL;
                }
                continue;
            }

            // ── Type 3 check ──
            let mut fail3 = false;
            for j in 0..core.shapes[cur].nodes.len() {
                let new_x = x + core.shapes[cur].nodes[j].x - start_x;
                let new_y = y + core.shapes[cur].nodes[j].y - start_y;
                let pxn = to_puzzle_x(new_x) as usize;
                let pyn = to_puzzle_y(new_y) as usize;
                if (core.puzzle[pxn][pyn] & AREA_PALISADE_INDEX_BIT) != 0
                    && !check_palisade_type2(pxn as i32, pyn as i32, core, sp)
                {
                    fail3 = true;
                    break;
                }
                if core.config.has_watchtower && !check_radar(pxn as i32, pyn as i32, core, sp) {
                    fail3 = true;
                    break;
                }
                if core.config.no_4_way_intersections
                    && !check_tatami(pxn as i32, pyn as i32, core, sp)
                {
                    fail3 = true;
                    break;
                }
                if core.config.no_3_way_intersections
                    && !check_loopy(pxn as i32, pyn as i32, core, sp)
                {
                    fail3 = true;
                    break;
                }
            }
            if fail3 {
                for j in 0..core.shapes[cur].nodes.len() {
                    let new_x = x + core.shapes[cur].nodes[j].x - start_x;
                    let new_y = y + core.shapes[cur].nodes[j].y - start_y;
                    sp[to_puzzle_x(new_x) as usize][to_puzzle_y(new_y) as usize] = AREA_NORMAL;
                }
                continue;
            }

            // ── Type 4 check ──
            if !empty_area_check(core, sp) {
                for j in 0..core.shapes[cur].nodes.len() {
                    let new_x = x + core.shapes[cur].nodes[j].x - start_x;
                    let new_y = y + core.shapes[cur].nodes[j].y - start_y;
                    sp[to_puzzle_x(new_x) as usize][to_puzzle_y(new_y) as usize] = AREA_NORMAL;
                }
                continue;
            }

            // ── DFS ──
            let mut first_shape_flag = false;
            if core.config.all_shapes_same && core.all_shapes_same_check_shape_index == -1 {
                core.all_shapes_same_check_shape_index = core.shapes[cur].shape_index as i32;
                first_shape_flag = true;
            }
            if core.config.all_shapes_different {
                core.all_shapes_different_check_shape_index_pool
                    .insert(core.shapes[cur].shape_index);
            }

            let dfs_ret = dfs(index + 1, core, sp, pools);
            if dfs_ret != -1 {
                return dfs_ret;
            }

            if first_shape_flag {
                core.all_shapes_same_check_shape_index = -1;
            }
            if core.config.all_shapes_different {
                core.all_shapes_different_check_shape_index_pool
                    .remove(&core.shapes[cur].shape_index);
            }
            for j in 0..core.shapes[cur].nodes.len() {
                let new_x = x + core.shapes[cur].nodes[j].x - start_x;
                let new_y = y + core.shapes[cur].nodes[j].y - start_y;
                sp[to_puzzle_x(new_x) as usize][to_puzzle_y(new_y) as usize] = AREA_NORMAL;
            }
        }
    }

    if ret == SPECIAL_START_AREA_INDEX {
        return -1;
    }
    if ret == SPECIAL_START_LINE_SAME {
        return -1;
    }
    if core.config.predefine_shapes_only || core.config.only_rectangles {
        return -1;
    }
    if core.config.all_shapes_same && core.all_shapes_same_check_shape_index != -1 {
        return -1;
    }

    let known_shape_index = if core.shapes.is_empty() {
        -1
    } else {
        core.shapes[core.shapes.len() - 1].shape_index as i32
    };

    for size in shape_size_lower_bound.max(0)..=shape_size_upper_bound {
        // Each size may spend thousands of `place` steps before its own 4096-step
        // deadline check; bound the whole size sweep against the frame deadline.
        if Instant::now() >= core.deadline {
            return -1;
        }
        if !mk_size[size as usize] {
            continue;
        }
        let place_ret = place_non_predifined_shape(
            index,
            x,
            y,
            size as u32,
            true,
            known_shape_index,
            core,
            sp,
            pools,
        );
        if place_ret != -1 {
            return place_ret;
        }
    }
    -1
}

#[inline]
fn region_size_at(core: &AoGCore, sp: &Vec<Vec<u32>>, px: usize, py: usize) -> usize {
    let v = sp_at(sp, px as i32, py as i32);
    core.shape_size_by_index[((v & SOLVE_AREA_SHAPE_INDEX_BIT) >> SOLVE_AREA_SHAPE_INDEX_BIT_SHIFT) as usize]
}
