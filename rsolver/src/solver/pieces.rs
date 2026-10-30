//! Piece-based solver using DLX exact cover.
//!
//! Generates all valid shape placements from cell clues (area numbers, compass,
//! shape pool), builds a DLX matrix (columns = cells), and finds exact cover.

use std::collections::{BTreeMap, BTreeSet};
use crate::clock::Instant;

use crate::dlx::DancingLinks;
use crate::polyomino;
use crate::types::*;

#[derive(Debug, Clone)]
struct Placement {
    cells: Vec<[usize; 2]>,
    area: usize,
    shape: Shape,
    cell_ids_flat: Vec<usize>,
    shape_key: String,
}

struct SolveContext {
    cell_to_idx: Vec<Vec<usize>>,
    num_cells: usize,
    eff_min_area: usize,
    eff_max_area: usize,
    watchtowers: Vec<(Vec<[usize; 2]>, usize)>,
    edge_constraints: Vec<([usize; 2], [usize; 2], &'static str, Option<i64>)>,
    fillable: Vec<[usize; 2]>,
}

const MAX_COMPASS_PLACEMENTS: usize = 10_000;
/// Upper bound on the compass placement-search state space (visited cell-sets).
/// Only loosely-specified clues can reach it; truncation then keeps D8's
/// "first N placements are still useful" semantics instead of blowing memory.
const MAX_COMPASS_ENUM_STATES: usize = 1_000_000;
/// Area-number targets above this are left to the backtracker: enumerating all
/// connected polyominoes of a large size explodes (e.g. area 48 on 1301).  This
/// matches the Python ExactCoverSolver threshold (max(targets) <= 12).
const MAX_AREA_TARGET: usize = 12;

pub fn solve_pieces(puzzle: &Puzzle, _start: &Instant, timeout_ms: u64) -> ModuleOutcome {
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);

    if !has_clues(puzzle) && puzzle.shape_pool.is_empty() {
        return ModuleOutcome::None; // fall back to backtrack
    }

    let ctx = build_context(puzzle);
    let placements = generate_all_placements(puzzle, &ctx, deadline);
    if std::env::var("EDGE_CSP_DEBUG").is_ok() {
        eprintln!("pieces: {} placements, {} cells", placements.len(), ctx.num_cells);
        // Check cell coverage.
        let mut covered = std::collections::HashSet::new();
        for p in &placements {
            for &idx in &p.cell_ids_flat {
                covered.insert(idx);
            }
        }
        let uncovered: Vec<usize> = (0..ctx.num_cells).filter(|i| !covered.contains(i)).collect();
        eprintln!("pieces: {} uncovered cells: {:?}", uncovered.len(), &uncovered[..uncovered.len().min(20)]);
    }

    if placements.is_empty() {
        if ctx.num_cells == 0 {
            return ModuleOutcome::Solved(Vec::new());
        }
        return ModuleOutcome::None;
    }

    // Build DLX
    let mut dlx = DancingLinks::new(ctx.num_cells);
    for (i, p) in placements.iter().enumerate() {
        dlx.add_row(&p.cell_ids_flat, i);
    }
    dlx.set_deadline(deadline);

    // Search, validating every complete tiling against the global rules
    // (edge constraints, watchtowers, and — via `validate.rs` — every rule
    // type, so a tiling that violates fence / compass / ring / rose_window is
    // rejected here instead of slipping to the router).  Keep searching until
    // a valid tiling is found or the deadline expires.  This lets a block
    // puzzle (routed here through a synthesized rectangle pool) land on a
    // *valid* rectangle partition instead of the first (usually trivial
    // all-1×1) one.
    //
    // `reconstruct_and_validate` already validates each candidate inside the
    // callback, so `result` only ever holds a *validated* solution — there is
    // no `ValidationFailed` to surface here (unlike aog / edge_csp, whose
    // internal validate failure is exposed via `ModuleOutcome`).
    let mut result: Option<Vec<RegionInfo>> = None;
    let mut partial: Vec<usize> = Vec::new();
    // Incremental pruning during the DLX search (doc 16 "row_check 未启用" debt):
    //   * watchtower bounds — `d` distinct already-chosen regions among the
    //     vertex cells and `u` unplaced ones force value ∈ [d+(u>0), d+u];
    //   * edge constraints / pre-drawn boundaries must separate regions, so
    //     both endpoints landing in one row is already fatal (the leaf
    //     validator would reject the cover — prune the branch instead).
    let wt = ctx.watchtowers.clone();
    let econs = ctx.edge_constraints.clone();
    // Precomputed dihedral keys for the `different` (Mismatch) rule: any two
    // chosen rows sharing a key is already fatal.
    let need_distinct = puzzle.rules.iter().any(|r| r.ctype == "different");
    let keys: Vec<String> = placements
        .iter()
        .map(|p| p.shape_key.clone())
        .collect();
    // Region-count cap when derivable (solitary clue count): a cover with more
    // rows than regions is already wrong.
    let m_cap: Option<usize> = solitary_clue_count(puzzle);
    // Cell-clue consistency: a row containing a number cell must have exactly
    // that area (`area` rule), and a row containing a `shape_pattern` cell must
    // be dihedrally equal to the pattern (`puzzle_piece` rule).  Both are leaf
    // validator rules — pruning on them early keeps the DLX honest.
    let pat_keys: Vec<(usize, usize, String)> = puzzle
        .cells
        .iter()
        .flatten()
        .filter_map(|c| {
            c.shape_pattern
                .as_ref()
                .map(|p| (c.row, c.col, crate::shapes::dihedral_key(p)))
        })
        .collect();
    // Gemini shape keys: for `homogeneous` / `heterogeneous` rules, compute
    // dihedral shape keys for all placements.  Gemini edges require same shape,
    // delta edges require different shapes.
    let row_check = |partial: &[usize]| -> bool {
        if let Some(mc) = m_cap {
            if partial.len() > mc {
                return false;
            }
        }
        // cell -> row id for the chosen rows.
        let mut cell_row = std::collections::HashMap::new();
        for &rid in partial {
            for &idx in &placements[rid].cell_ids_flat {
                cell_row.insert(idx, rid);
            }
        }
        if std::env::var("PIECES_SKIP_ALL").is_ok() {
            true
        } else {
            let wt_ok = watchtower_rows_ok(&wt, &ctx, &cell_row);
            let econs_ok = econs_rows_ok(&econs, &ctx, &cell_row);
            let distinct_ok = distinct_rows_ok(need_distinct, &keys, partial);
            let clue_ok = cell_clue_rows_ok(puzzle, &placements, &keys, &pat_keys, partial);
            // Gemini/Delta constraints are checked in reconstruct_and_validate (leaf).
            // During DLX search, both are too strict for partial solutions —
            // partial shapes don't yet reflect the final region shapes.
            let gemini_ok = true;
            if std::env::var("EDGE_CSP_DEBUG").is_ok() && !(wt_ok && econs_ok && distinct_ok && clue_ok && gemini_ok) {
                eprintln!("pieces: row_check fail wt={} econs={} distinct={} clue={} gemini={}", wt_ok, econs_ok, distinct_ok, clue_ok, gemini_ok);
            }
            wt_ok && econs_ok && distinct_ok && clue_ok && gemini_ok
        }
    };
    let mut row_check = row_check;
    let mut on_solution = |row_ids: &[usize]| {
        if std::env::var("EDGE_CSP_DEBUG").is_ok() {
            eprintln!("pieces: DLX candidate tiling with {} rows", row_ids.len());
        }
        match reconstruct_and_validate(puzzle, &placements, row_ids, &ctx) {
            Some(regions) => {
                result = Some(regions);
                true // stop the search
            }
            None => false, // keep looking
        }
    };
    dlx.search_with_check(0, &mut partial, &mut row_check, &mut on_solution);

    match result {
        Some(regions) => ModuleOutcome::Solved(regions),
        None => ModuleOutcome::None,
    }
}

fn has_clues(puzzle: &Puzzle) -> bool {
    if puzzle.rules.iter().any(|r| r.ctype == "block") {
        return true; // rectangle placements stand in for the "clues"
    }
    // `precise` gives a uniform area for all regions → shape pool is all
    // polyominoes of that area.
    if puzzle.rules.iter().any(|r| r.ctype == "precise") {
        return true;
    }
    // `range` gives min/max area bounds → shape pool is all polyominoes
    // in that range.
    if puzzle.rules.iter().any(|r| r.ctype == "range") {
        return true;
    }
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            let cell = &puzzle.cells[r][c];
            if cell.number.is_some() || cell.compass.is_some() {
                return true;
            }
        }
    }
    false
}

fn build_context(puzzle: &Puzzle) -> SolveContext {
    let h = puzzle.height;
    let w = puzzle.width;

    let mut cell_to_idx = vec![vec![usize::MAX; w]; h];
    let mut num_cells = 0;
    let fillable: Vec<[usize; 2]> = (0..h)
        .flat_map(|r| (0..w).map(move |c| (r, c)))
        .filter(|&(r, c)| puzzle.cells[r][c].fillable())
        .map(|(r, c)| {
            cell_to_idx[r][c] = num_cells;
            num_cells += 1;
            [r, c]
        })
        .collect();

    let (eff_min, eff_max) = crate::shapes::area_bounds(puzzle);

    let watchtowers = collect_watchtower_data(puzzle);
    let edge_constraints = collect_edge_constraints_data(puzzle);

    SolveContext {
        cell_to_idx,
        num_cells,
        eff_min_area: eff_min,
        eff_max_area: eff_max,
        watchtowers,
        edge_constraints,
        fillable,
    }
}

fn collect_watchtower_data(puzzle: &Puzzle) -> Vec<(Vec<[usize; 2]>, usize)> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut result = Vec::new();
    // Vertex (r,c) = ABSOLUTE grid corner (r in 0..=h, c in 0..=w).  Cells
    // touching it: in-bounds, non-blocked members of {(r-1,c-1),(r-1,c),
    // (r,c-1),(r,c)}.  Border corners are touched by 2 (edge) / 1 (corner).
    for r in 0..=h {
        for c in 0..=w {
            if let Some(val) = puzzle.vertices[r][c].watchtower {
                let v = val as usize;
                if v >= 1 && v <= 4 {
                    let mut cells = Vec::new();
                    for (dr, dc) in [(-1i64, -1i64), (-1, 0), (0, -1), (0, 0)] {
                        let nr = r as i64 + dr;
                        let nc = c as i64 + dc;
                        if nr < 0 || nc < 0 || nr >= h as i64 || nc >= w as i64 {
                            continue;
                        }
                        let nr = nr as usize;
                        let nc = nc as usize;
                        if !puzzle.cells[nr][nc].blocked {
                            cells.push([nr, nc]);
                        }
                    }
                    result.push((cells, v));
                }
            }
        }
    }
    result
}

fn collect_edge_constraints_data(
    puzzle: &Puzzle,
) -> Vec<([usize; 2], [usize; 2], &'static str, Option<i64>)> {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut result = Vec::new();

    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            if let Some(ref ec) = puzzle.h_edges[r][c].constraint {
                let kind = match ec.ctype {
                    EdgeConstraintType::Inequality => "inequality",
                    EdgeConstraintType::Difference => "difference",
                    EdgeConstraintType::Heterogeneous => "delta",
                    EdgeConstraintType::Homogeneous => "gemini",
                };
                result.push(([r, c], [r, c + 1], kind, ec.value));
            }
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            if let Some(ref ec) = puzzle.v_edges[r][c].constraint {
                let kind = match ec.ctype {
                    EdgeConstraintType::Inequality => "inequality",
                    EdgeConstraintType::Difference => "difference",
                    EdgeConstraintType::Heterogeneous => "delta",
                    EdgeConstraintType::Homogeneous => "gemini",
                };
                result.push(([r, c], [r + 1, c], kind, ec.value));
            }
        }
    }
    result
}

/// Rose symbol signature: exactly one cell of every `rose_window` symbol type
/// (the rule demands one of each per region — checked at the leaf by
/// `validate`, but filtering here collapses the DLX row count by orders of
/// magnitude, e.g. 0223's shape_pool tiling).  No-op without `rose_window`.


/// Generate placements for `range` rule: all polyominoes in [min, max].
fn push_range_placements(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    rose_types: &[String],
    placements: &mut Vec<Placement>,
) {
    let Some(range_rule) = puzzle.rules.iter().find(|r| r.ctype == "range") else {
        return;
    };
    let min_area = range_rule
        .params
        .get("min")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as usize;
    let max_area = range_rule
        .params
        .get("max")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as usize;
    let lo = min_area.max(1);
    let hi = max_area.min(MAX_AREA_TARGET);
    if lo > hi {
        return;
    }
    let (h, w) = (puzzle.height, puzzle.width);
    for area in lo..=hi {
        let shapes = crate::polyomino::enumerate_free_polyominoes(area);
        for shape in &shapes {
            let transforms = polyomino::transforms(shape);
            for transform in &transforms {
                let offset_drs: Vec<[isize; 2]> =
                    transform.iter().map(|&[r, c]| [r as isize, c as isize]).collect();
                for r in 0..h {
                    for c in 0..w {
                        if let Some(cells) = try_place(&offset_drs, r, c, h, w, puzzle) {
                            if !rose_signature_ok(puzzle, &cells, rose_types)
                                || !non_block_ok(puzzle, &cells)
                            {
                                continue;
                            }
                            let flat = cells_to_flat(&cells, ctx);
                            placements.push(Placement {
                                cells,
                                area,
                                shape_key: crate::shapes::dihedral_key(shape),
                                shape: shape.clone(),
                                cell_ids_flat: flat,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn push_area_placements(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    rose_types: &[String],
    placements: &mut Vec<Placement>,
) {
    let (h, w) = (puzzle.height, puzzle.width);
    // Collect all area values from number clues to determine the range.
    let mut area_values: Vec<usize> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if let Some(area) = puzzle.cells[r][c].number {
                area_values.push(area as usize);
            }
        }
    }
    if area_values.is_empty() {
        return;
    }
    let min_area = *area_values.iter().min().unwrap();
    let max_area = *area_values.iter().max().unwrap();
    // Generate placements for ALL cells (not just cells with area numbers).
    // Cells with area numbers are constrained to that specific area.
    // Cells without area numbers can have any area in [min_area, max_area].
    for r in 0..h {
        for c in 0..w {
            if !puzzle.cells[r][c].fillable() {
                continue;
            }
            let specific_area = puzzle.cells[r][c].number.map(|n| n as usize);
            let areas_to_try: Vec<usize> = match specific_area {
                Some(a) => {
                    if a < ctx.eff_min_area || a > ctx.eff_max_area || a > MAX_AREA_TARGET {
                        continue;
                    }
                    vec![a]
                }
                None => {
                    // Generate placements for cells without area numbers using
                    // all possible area values.  This ensures the DLX solver
                    // can cover all cells.  Start from 1 (not min_area) because
                    // regions without area numbers can be any size.
                    let lo = 1usize.max(ctx.eff_min_area);
                    let hi = max_area.min(ctx.eff_max_area).min(MAX_AREA_TARGET);
                    if lo > hi {
                        continue;
                    }
                    (lo..=hi).collect()
                }
            };
            for target in areas_to_try {
                let mut results = Vec::new();
                generate_polyominoes(puzzle, r, c, target, &mut results);
                for cells in results {
                    if !rose_signature_ok(puzzle, &cells, rose_types)
                        || !non_block_ok(puzzle, &cells)
                    {
                        continue;
                    }
                    let mut canonical = cells.clone();
                    normalize(&mut canonical);
                    let flat = cells_to_flat(&cells, ctx);
                    placements.push(Placement {
                        cells,
                        area: target,
                        shape_key: crate::shapes::dihedral_key(&canonical),
                        shape: canonical,
                        cell_ids_flat: flat,
                    });
                }
            }
        }
    }
}

fn push_compass_placements(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    rose_types: &[String],
    deadline: Instant,
    placements: &mut Vec<Placement>,
) {
    let (h, w) = (puzzle.height, puzzle.width);
    for r in 0..h {
        for c in 0..w {
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                let spec_count = count_specified(comp);
                let is_strip = comp.east_west_strip() || comp.north_south_strip();
                // D8: relax spec filter — spec<3 was too aggressive (0469:
                // 19/23 cells had spec=1 and were all skipped).  Only skip
                // spec=0 (no direction constraints at all).
                if spec_count == 0 && !is_strip {
                    continue;
                }
                let mut results = generate_compass_polyominoes(puzzle, r, c, comp, deadline);
                // D8: truncate instead of discarding when too many compass placements.
                if results.len() > MAX_COMPASS_PLACEMENTS {
                    results.truncate(MAX_COMPASS_PLACEMENTS);
                }
                for cells in results {
                    if !rose_signature_ok(puzzle, &cells, rose_types) || !non_block_ok(puzzle, &cells)
                    {
                        continue;
                    }
                    let area = cells.len();
                    let mut canonical = cells.clone();
                    normalize(&mut canonical);
                    let flat = cells_to_flat(&cells, ctx);
                    placements.push(Placement {
                        cells,
                        area,
                        shape_key: crate::shapes::dihedral_key(&canonical),
                        shape: canonical,
                        cell_ids_flat: flat,
                    });
                }
            }
        }
    }
}

fn push_block_rect_placements(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    rose_types: &[String],
    placements: &mut Vec<Placement>,
) {
    if !puzzle.rules.iter().any(|r| r.ctype == "block") {
        return;
    }
    let (h, w) = (puzzle.height, puzzle.width);
    for hgt in 1..=h {
        for wdt in 1..=w {
            for r in 0..h {
                if r + hgt > h {
                    continue;
                }
                for c in 0..w {
                    if c + wdt > w {
                        continue;
                    }
                    let mut cells: Vec<[usize; 2]> = Vec::with_capacity(hgt * wdt);
                    let mut ok = true;
                    for rr in r..r + hgt {
                        for cc in c..c + wdt {
                            if !puzzle.cells[rr][cc].fillable() {
                                ok = false;
                                break;
                            }
                            cells.push([rr, cc]);
                        }
                        if !ok {
                            break;
                        }
                    }
                    if !ok || !rose_signature_ok(puzzle, &cells, rose_types) {
                        continue;
                    }
                    let mut canonical = cells.clone();
                    normalize(&mut canonical);
                    let flat = cells_to_flat(&cells, ctx);
                    placements.push(Placement {
                        cells,
                        area: hgt * wdt,
                        shape_key: crate::shapes::dihedral_key(&canonical),
                        shape: canonical,
                        cell_ids_flat: flat,
                    });
                }
            }
        }
    }
}

fn push_pattern_placements(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    w: usize,
    rose_types: &[String],
    placements: &mut Vec<Placement>,
) {
    if !puzzle
        .cells
        .iter()
        .flatten()
        .any(|c| c.shape_pattern.is_some())
    {
        return;
    }
    let Some(anchors) = crate::solver::rose::puzzle_piece_pin::enumerate_pin_candidates(puzzle, &[])
    else {
        return;
    };
    for a in &anchors {
        for p in &a.placements {
            let cells: Vec<[usize; 2]> = p.cells.iter().map(|idx| [idx / w, idx % w]).collect();
            if !rose_signature_ok(puzzle, &cells, rose_types) {
                continue;
            }
            let mut canonical = cells.clone();
            normalize(&mut canonical);
            let flat = cells_to_flat(&cells, ctx);
            placements.push(Placement {
                cells,
                area: flat.len(),
                shape_key: crate::shapes::dihedral_key(&canonical),
                        shape: canonical,
                cell_ids_flat: flat,
            });
        }
    }
}

/// Solitary clue count (mirrors `validate::check_solitary`), or None.
fn solitary_clue_count(puzzle: &Puzzle) -> Option<usize> {
    if !puzzle.rules.iter().any(|r| r.ctype == "solitary") {
        return None;
    }
    Some(
        puzzle
            .cells
            .iter()
            .flatten()
            .filter(|c| {
                !c.blocked
                    && (c.symbol.is_some()
                        || c.compass.is_some()
                        || c.number.is_some()
                        || c.shape_pattern.is_some()
                        || c.fence_pattern.is_some())
            })
            .count(),
    )
}

/// Watchtower bounds over the chosen rows (`d` + `(u>0)` … `d + u`).
fn watchtower_rows_ok(
    wt: &[(Vec<[usize; 2]>, usize)],
    ctx: &SolveContext,
    cell_row: &std::collections::HashMap<usize, usize>,
) -> bool {
    for (cells, val) in wt {
        let mut seen_rows: Vec<usize> = Vec::new();
        let mut unplaced = 0usize;
        for &[r, c] in cells {
            let idx = ctx.cell_to_idx[r][c];
            match cell_row.get(&idx) {
                Some(&rid) => {
                    if !seen_rows.contains(&rid) {
                        seen_rows.push(rid);
                    }
                }
                None => unplaced += 1,
            }
        }
        let d = seen_rows.len();
        let lo = if unplaced > 0 { d + 1 } else { d };
        let hi = d + unplaced;
        if *val < lo || *val > hi {
            return false;
        }
    }
    true
}

/// Edge constraints / pre-drawn boundaries must separate regions.
fn econs_rows_ok(
    econs: &[([usize; 2], [usize; 2], &'static str, Option<i64>)],
    ctx: &SolveContext,
    cell_row: &std::collections::HashMap<usize, usize>,
) -> bool {
    for (c1, c2, _, _) in econs {
        let i1 = ctx.cell_to_idx[c1[0]][c1[1]];
        let i2 = ctx.cell_to_idx[c2[0]][c2[1]];
        if let (Some(&r1), Some(&r2)) = (cell_row.get(&i1), cell_row.get(&i2)) {
            if r1 == r2 {
                return false;
            }
        }
    }
    true
}

/// Gemini/Delta shape compatibility during DLX search.
/// - Gemini edge: both endpoints placed in different rows → same shape.
/// - Delta edge: both endpoints placed in different rows → different shapes.
fn gemini_shape_rows_ok(
    econs: &[([usize; 2], [usize; 2], &'static str, Option<i64>)],
    ctx: &SolveContext,
    keys: &[String],
    cell_row: &std::collections::HashMap<usize, usize>,
) -> bool {
    for (c1, c2, kind, _) in econs {
        // Only check gemini (same shape) during DLX search.
        // Delta (different shape) is checked in reconstruct_and_validate.
        if *kind != "gemini" {
            continue;
        }
        let i1 = ctx.cell_to_idx[c1[0]][c1[1]];
        let i2 = ctx.cell_to_idx[c2[0]][c2[1]];
        if let (Some(&r1), Some(&r2)) = (cell_row.get(&i1), cell_row.get(&i2)) {
            if r1 == r2 {
                continue; // same region — no shape constraint
            }
            if keys[r1] != keys[r2] {
                return false; // gemini requires same shape
            }
        }
    }
    true
}

/// Check edge constraints in a complete tiling.
fn edge_constraints_ok(
    placements: &[Placement],
    row_ids: &[usize],
    piece_of_cell: &BTreeMap<(usize, usize), usize>,
    ctx: &SolveContext,
) -> bool {
    for &(c1, c2, kind, val) in &ctx.edge_constraints {
        let p1 = piece_of_cell.get(&(c1[0], c1[1]));
        let p2 = piece_of_cell.get(&(c2[0], c2[1]));
        let (p1, p2) = match (p1, p2) {
            (Some(&a), Some(&b)) => (a, b),
            _ => continue,
        };
        if p1 == p2 {
            continue;
        }
        let a1 = placements[row_ids[p1]].area;
        let a2 = placements[row_ids[p2]].area;
        match kind {
            "inequality" => {
                if a1 == a2 {
                    if std::env::var("EDGE_CSP_DEBUG").is_ok() {
                        eprintln!("inequality reject: ({},{})-({},{}) a1={} a2={}", c1[0], c1[1], c2[0], c2[1], a1, a2);
                    }
                    return false;
                }
            }
            "difference" => {
                let diff = (a1 as i64 - a2 as i64).unsigned_abs() as usize;
                if let Some(target) = val {
                    if diff != target as usize {
                        if std::env::var("EDGE_CSP_DEBUG").is_ok() {
                            eprintln!("difference reject: ({},{})-({},{}) diff={} target={}", c1[0], c1[1], c2[0], c2[1], diff, target);
                        }
                        return false;
                    }
                }
            }
            "delta" => {
                if placements[row_ids[p1]].shape_key == placements[row_ids[p2]].shape_key {
                    return false;
                }
            }
            "gemini" => {
                if placements[row_ids[p1]].shape_key != placements[row_ids[p2]].shape_key {
                    if std::env::var("EDGE_CSP_DEBUG").is_ok() {
                        eprintln!("gemini reject: ({},{})-({},{}) key1={} key2={}", c1[0], c1[1], c2[0], c2[1], placements[row_ids[p1]].shape_key, placements[row_ids[p2]].shape_key);
                    }
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// Gemini shape constraint propagation: for each gemini edge, if one
/// endpoint is in a placement with shape S, the other endpoint's placements
/// must also have shape S.  Filter out incompatible placements.
fn filter_gemini_incompatible(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    placements: &mut Vec<Placement>,
) {
    if !puzzle
        .rules
        .iter()
        .any(|r| r.ctype == "homogeneous" || r.ctype == "heterogeneous")
    {
        return;
    }
    let gemini_edges: Vec<([usize; 2], [usize; 2])> = ctx
        .edge_constraints
        .iter()
        .filter(|(_, _, kind, _)| *kind == "gemini" || *kind == "delta")
        .map(|&(c1, c2, _, _)| (c1, c2))
        .collect();
    if gemini_edges.is_empty() {
        return;
    }
    // Build cell -> placements index.
    let mut cell_placements: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, p) in placements.iter().enumerate() {
        for &idx in &p.cell_ids_flat {
            cell_placements.entry(idx).or_default().push(i);
        }
    }
    // For each gemini edge, check if placements on both sides have
    // compatible shapes.  If a placement on one side has shape S,
    // filter the other side's placements to only those with shape S.
    let mut to_remove = std::collections::HashSet::new();
    for (c1, c2) in &gemini_edges {
        let i1 = ctx.cell_to_idx[c1[0]][c1[1]];
        let i2 = ctx.cell_to_idx[c2[0]][c2[1]];
        if let (Some(p1s), Some(p2s)) = (cell_placements.get(&i1), cell_placements.get(&i2)) {
            // Collect shapes on each side.
            let shapes1: std::collections::HashSet<&String> =
                p1s.iter().map(|&i| &placements[i].shape_key).collect();
            let shapes2: std::collections::HashSet<&String> =
                p2s.iter().map(|&i| &placements[i].shape_key).collect();
            // Find intersection.
            let common: std::collections::HashSet<&&String> =
                shapes1.intersection(&shapes2).collect();
            if common.is_empty() {
                // No compatible shapes → all placements on both sides are invalid.
                for &i in p1s {
                    to_remove.insert(i);
                }
                for &i in p2s {
                    to_remove.insert(i);
                }
            } else {
                // Keep only placements with compatible shapes.
                for &i in p1s {
                    if !common.contains(&&placements[i].shape_key) {
                        to_remove.insert(i);
                    }
                }
                for &i in p2s {
                    if !common.contains(&&placements[i].shape_key) {
                        to_remove.insert(i);
                    }
                }
            }
        }
    }
    if !to_remove.is_empty() {
        let mut idx = 0;
        placements.retain(|_| {
            let keep = !to_remove.contains(&idx);
            idx += 1;
            keep
        });
    }
}

/// `different` (Mismatch): chosen rows must have pairwise distinct shapes.
fn distinct_rows_ok(need_distinct: bool, keys: &[String], partial: &[usize]) -> bool {
    if !need_distinct {
        return true;
    }
    for i in 0..partial.len() {
        for j in (i + 1)..partial.len() {
            if keys[partial[i]] == keys[partial[j]] {
                return false;
            }
        }
    }
    true
}

/// Gemini/Delta shape compatibility during DLX search.
/// Per-row cell-clue consistency: number ⇒ exact area, `shape_pattern` ⇒
/// dihedrally equal shape, compass ⇒ exact half-plane counts (a row is a whole
/// region, so the counts are final).
fn cell_clue_rows_ok(
    puzzle: &Puzzle,
    placements: &[Placement],
    keys: &[String],
    pat_keys: &[(usize, usize, String)],
    partial: &[usize],
) -> bool {
    for &rid in partial {
        let p = &placements[rid];
        for &[r, c] in &p.cells {
            if let Some(n) = puzzle.cells[r][c].number {
                if p.area != n as usize {
                    return false;
                }
            }
            if let Some(ref comp) = puzzle.cells[r][c].compass {
                if !compass_row_ok(comp, &p.cells, r, c) {
                    return false;
                }
            }
        }
        for (pr, pc, pk) in pat_keys {
            if p.cells.contains(&[*pr, *pc]) && keys[rid] != *pk {
                return false;
            }
        }
    }
    true
}

/// Half-plane counts of the row around (r, c) must match the clue's specified
/// directions exactly (mirrors `validate::check_compass`).
fn compass_row_ok(comp: &CompassClue, cells: &[[usize; 2]], r: usize, c: usize) -> bool {
    let (cr, cc) = (r as i64, c as i64);
    let mut counts = [0i64; 4]; // N, S, W, E
    for &[rr, ccc] in cells {
        if rr == r && ccc == c {
            continue;
        }
        let (dr, dc) = (rr as i64 - cr, ccc as i64 - cc);
        if dr < 0 {
            counts[0] += 1;
        }
        if dr > 0 {
            counts[1] += 1;
        }
        if dc < 0 {
            counts[2] += 1;
        }
        if dc > 0 {
            counts[3] += 1;
        }
    }
    let want = [comp.up, comp.down, comp.left, comp.right];
    (0..4).all(|i| match want[i] {
        Some(v) if v >= 0 => counts[i] == v,
        _ => true,
    })
}

/// `non_block` (Non-Boxy): no region may be a solid rectangle.
fn non_block_ok(puzzle: &Puzzle, cells: &[[usize; 2]]) -> bool {
    if !puzzle.rules.iter().any(|r| r.ctype == "non_block") {
        return true;
    }
    !crate::shapes::is_rectangle(cells)
}

fn rose_signature_ok(puzzle: &Puzzle, cells: &[[usize; 2]], types: &[String]) -> bool {
    if types.is_empty() {
        return true;
    }
    let mut got = std::collections::BTreeMap::new();
    for &[r, c] in cells {
        if let Some(sym) = &puzzle.cells[r][c].symbol {
            *got.entry(sym.clone()).or_insert(0usize) += 1;
        }
    }
    got.len() == types.len() && types.iter().all(|t| got.get(t) == Some(&1))
}

/// Propagate area numbers through gemini edges.  If cell A has area X and cell
/// B is on the other side of a gemini edge, then B's region must also have
/// area X.  Returns a map of cell -> propagated area.
fn propagate_gemini_areas(
    puzzle: &Puzzle,
    ctx: &SolveContext,
) -> std::collections::HashMap<(usize, usize), usize> {
    let mut propagated = std::collections::HashMap::new();
    // Collect area numbers.
    for r in 0..puzzle.height {
        for c in 0..puzzle.width {
            if let Some(n) = puzzle.cells[r][c].number {
                propagated.insert((r, c), n as usize);
            }
        }
    }
    // Propagate through gemini edges (BFS until fixpoint).
    let mut changed = true;
    while changed {
        changed = false;
        for &(c1, c2, kind, _) in &ctx.edge_constraints {
            if kind != "gemini" {
                continue;
            }
            let a1 = propagated.get(&(c1[0], c1[1])).copied();
            let a2 = propagated.get(&(c2[0], c2[1])).copied();
            match (a1, a2) {
                (Some(x), None) => {
                    propagated.insert((c2[0], c2[1]), x);
                    changed = true;
                }
                (None, Some(x)) => {
                    propagated.insert((c1[0], c1[1]), x);
                    changed = true;
                }
                _ => {}
            }
        }
    }
    propagated
}

fn generate_all_placements(
    puzzle: &Puzzle,
    ctx: &SolveContext,
    deadline: Instant,
) -> Vec<Placement> {
    let h = puzzle.height;
    let w = puzzle.width;
    let rose_types = crate::shapes::rose_symbol_types(puzzle);
    let mut placements = Vec::new();

    // Gemini area propagation: gemini edges propagate area numbers to
    // cells that don't have them.  This dramatically reduces the search
    // space for gemini-constrained puzzles.
    let gemini_areas = propagate_gemini_areas(puzzle, ctx);

    // 1. Shape pool placements (if any)
    if !puzzle.shape_pool.is_empty() {
        for shape in &puzzle.shape_pool {
            let transforms = polyomino::transforms(shape);
            let area = shape.len();
            for transform in &transforms {
                let offset_drs: Vec<[isize; 2]> = transform.clone();
                for r in 0..h {
                    for c in 0..w {
                        if let Some(cells) = try_place(&offset_drs, r, c, h, w, puzzle) {
                            if !rose_signature_ok(puzzle, &cells, &rose_types)
                                || !non_block_ok(puzzle, &cells)
                            {
                                continue;
                            }
                            let flat = cells_to_flat(&cells, ctx);
                            placements.push(Placement {
                                cells,
                                area,
                                shape_key: crate::shapes::dihedral_key(&shape),
                                shape: shape.clone(),
                                cell_ids_flat: flat,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. Area number placements
    push_area_placements(puzzle, ctx, &rose_types, &mut placements);

    // 2b. `precise` rule placements: all polyominoes of the precise area.
    if let Some(precise_area) = puzzle
        .rules
        .iter()
        .find(|r| r.ctype == "precise")
        .and_then(|r| r.params.get("area"))
        .and_then(|v| v.as_u64())
    {
        let area = precise_area as usize;
        if area >= 1 && area <= 12 {
            let shapes = crate::polyomino::enumerate_free_polyominoes(area);
            for shape in &shapes {
                let transforms = polyomino::transforms(shape);
                for transform in &transforms {
                    let offset_drs: Vec<[isize; 2]> =
                        transform.iter().map(|&[r, c]| [r as isize, c as isize]).collect();
                    for r in 0..h {
                        for c in 0..w {
                            if let Some(cells) = try_place(&offset_drs, r, c, h, w, puzzle) {
                                if !rose_signature_ok(puzzle, &cells, &rose_types)
                                    || !non_block_ok(puzzle, &cells)
                                {
                                    continue;
                                }
                                let flat = cells_to_flat(&cells, ctx);
                                placements.push(Placement {
                                    cells,
                                    area,
                                    shape_key: crate::shapes::dihedral_key(&shape),
                                shape: shape.clone(),
                                    cell_ids_flat: flat,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 2c. `range` rule placements: all polyominoes in the range [min, max].
    push_range_placements(puzzle, ctx, &rose_types, &mut placements);

    // 3. Compass clue placements
    push_compass_placements(puzzle, ctx, &rose_types, deadline, &mut placements);

    // 4. `block` rectangles
    push_block_rect_placements(puzzle, ctx, &rose_types, &mut placements);

    // 5. `shape_pattern` placements
    push_pattern_placements(puzzle, ctx, w, &rose_types, &mut placements);

    // Pre-filter: remove placements that include cells with area numbers but
    // have the wrong area.  Uses gemini-propagated areas for more effective
    // filtering.
    let before_count = placements.len();
    placements.retain(|p| {
        for &[r, c] in &p.cells {
            if let Some(&n) = gemini_areas.get(&(r, c)) {
                if p.area != n {
                    return false;
                }
            }
        }
        true
    });
    if std::env::var("EDGE_CSP_DEBUG").is_ok() {
        eprintln!("pieces: pre-filter {} -> {} placements", before_count, placements.len());
    }

    // Deduplicate
    let mut seen: BTreeSet<Vec<usize>> = BTreeSet::new();
    placements.retain(|p| {
        let mut ids = p.cell_ids_flat.clone();
        ids.sort();
        seen.insert(ids)
    });

    // Gemini shape constraint propagation
    // Gemini shape filtering is too aggressive for partial solutions —
    // disabled.  Gemini/Delta constraints are checked in reconstruct_and_validate.
    // if std::env::var("PIECES_SKIP_GEMINI").is_err() {
    //     filter_gemini_incompatible(puzzle, ctx, &mut placements);
    // }

    // `solitary`: each region holds exactly one clue cell.  A placement that
    // spans two or more clue cells can never be part of a valid partition, so
    // drop it.  (A placement covering zero clue cells is fine — regions need not
    // all be clue-anchored.)  This also makes Every clue's placements
    // individually valid, which is required for the DLX cover to exist when a
    // clue cell must end up as its own region.
    if puzzle.rules.iter().any(|r| r.ctype == "solitary") {
        placements.retain(|p| {
            let mut clues = 0;
            for &[r, c] in &p.cells {
                if is_clue_cell(puzzle, r, c) {
                    clues += 1;
                    if clues >= 2 {
                        break;
                    }
                }
            }
            clues < 2
        });
    }

    placements
}

/// True iff the cell carries a `solitary`-relevant clue, matching the
/// `validate::validate` "solitary" predicate (symbol / compass / number /
/// shape_pattern / fence_pattern).  Blocked cells are never fillable and so are
/// never present in a placement.
fn is_clue_cell(puzzle: &Puzzle, r: usize, c: usize) -> bool {
    let cell = &puzzle.cells[r][c];
    cell.symbol.is_some()
        || cell.compass.is_some()
        || cell.number.is_some()
        || cell.shape_pattern.is_some()
        || cell.fence_pattern.is_some()
}

impl CompassClue {
    fn east_west_strip(&self) -> bool {
        self.right == Some(0) && self.left == Some(0)
    }
    fn north_south_strip(&self) -> bool {
        self.up == Some(0) && self.down == Some(0)
    }
}

fn count_specified(comp: &CompassClue) -> usize {
    [comp.up, comp.down, comp.right, comp.left]
        .iter()
        .filter(|v| v.is_some())
        .count()
}

fn try_place(
    offsets: &[[isize; 2]],
    r: usize,
    c: usize,
    h: usize,
    w: usize,
    puzzle: &Puzzle,
) -> Option<Vec<[usize; 2]>> {
    let mut cells = Vec::with_capacity(offsets.len());
    for &[dr, dc] in offsets {
        let nr = r as isize + dr;
        let nc = c as isize + dc;
        if nr < 0 || nr >= h as isize || nc < 0 || nc >= w as isize {
            return None;
        }
        let nr = nr as usize;
        let nc = nc as usize;
        if puzzle.cells[nr][nc].blocked {
            return None;
        }
        cells.push([nr, nc]);
    }
    Some(cells)
}

fn cells_to_flat(cells: &[[usize; 2]], ctx: &SolveContext) -> Vec<usize> {
    cells.iter().map(|&[r, c]| ctx.cell_to_idx[r][c]).collect()
}

/// Generate all connected polyominoes of exactly `size` cells containing `(sr, sc)`.
fn generate_polyominoes(
    puzzle: &Puzzle,
    sr: usize,
    sc: usize,
    size: usize,
    results: &mut Vec<Vec<[usize; 2]>>,
) {
    let h = puzzle.height;
    let w = puzzle.width;
    let mut current = vec![[sr, sc]];
    let mut candidates = BTreeSet::new();

    for (nr, nc) in neighbor_positions(sr, sc, h, w) {
        if !puzzle.cells[nr][nc].blocked && !is_precut(puzzle, sr, sc, nr, nc) {
            candidates.insert([nr, nc]);
        }
    }

    poly_rec(puzzle, &mut current, &mut candidates, size, results);
}

fn poly_rec(
    puzzle: &Puzzle,
    current: &mut Vec<[usize; 2]>,
    candidates: &mut BTreeSet<[usize; 2]>,
    size: usize,
    results: &mut Vec<Vec<[usize; 2]>>,
) {
    if current.len() == size {
        results.push(current.clone());
        return;
    }
    if candidates.is_empty() {
        return;
    }

    let h = puzzle.height;
    let w = puzzle.width;
    let mut my_candidates = candidates.clone();

    while let Some(&next) = my_candidates.iter().next() {
        my_candidates.remove(&next);
        candidates.remove(&next);

        let mut added = Vec::new();
        for (nr, nc) in neighbor_positions(next[0], next[1], h, w) {
            let pos = [nr, nc];
            if puzzle.cells[nr][nc].blocked {
                continue;
            }
            if is_precut(puzzle, next[0], next[1], nr, nc) {
                continue;
            }
            if current.contains(&pos) || my_candidates.contains(&pos) {
                continue;
            }
            if candidates.insert(pos) {
                added.push(pos);
            }
        }

        current.push(next);
        poly_rec(puzzle, current, candidates, size, results);
        current.pop();

        for a in added {
            candidates.remove(&a);
        }
    }
}

/// Generate all connected polyominoes containing `(sr, sc)` that satisfy compass constraints.
///
/// Enumerates every connected subset by frontier growth with a visited-set on
/// the grown cell-set (sorted), so each subset is expanded exactly once no
/// matter the addition order.  (The previous shared-`candidates` DFS removed a
/// cell permanently when any branch tried it, so sibling branches lost whole
/// families of sets — e.g. the 4-cell placement of the test case below was
/// never generated.)
///
/// `deadline` bounds the placement DFS: a compass clue with an unspecified
/// direction has no size cap, so without a wall-clock check the recursion can
/// run far past the module budget — observed on 0312 / 0680 where `pieces` was
/// still enumerating 90s+ after aog / edge_csp had finished and the harness
/// killed the whole subprocess (empty attempt trace).
fn generate_compass_polyominoes(
    puzzle: &Puzzle,
    sr: usize,
    sc: usize,
    compass: &CompassClue,
    deadline: Instant,
) -> Vec<Vec<[usize; 2]>> {
    let mut results = Vec::new();
    let current = vec![[sr, sc]];
    let counts = [0usize; 4]; // N=0, S=1, E=2, W=3
    let mut frontier: BTreeSet<[usize; 2]> = BTreeSet::new();
    for (nr, nc) in neighbor_positions(sr, sc, puzzle.height, puzzle.width) {
        if !puzzle.cells[nr][nc].blocked && !is_precut(puzzle, sr, sc, nr, nc) {
            frontier.insert([nr, nc]);
        }
    }
    let mut visited: BTreeSet<Vec<[usize; 2]>> = BTreeSet::new();
    visited.insert(vec![[sr, sc]]);
    compass_rec(
        puzzle,
        &current,
        &counts,
        &frontier,
        sr as isize,
        sc as isize,
        compass,
        &mut results,
        &mut visited,
        deadline,
    );
    results
}

fn compass_rec(
    puzzle: &Puzzle,
    current: &[[usize; 2]],
    counts: &[usize; 4],
    frontier: &BTreeSet<[usize; 2]>,
    cr_i: isize,
    cc_i: isize,
    compass: &CompassClue,
    results: &mut Vec<Vec<[usize; 2]>>,
    visited: &mut BTreeSet<Vec<[usize; 2]>>,
    deadline: Instant,
) {
    if Instant::now() >= deadline || results.len() >= MAX_COMPASS_PLACEMENTS {
        return;
    }
    // Cap the enumerated state space for loosely-specified clues, whose subset
    // lattice is otherwise unbounded (no size cap when a direction is
    // unspecified).  Truncation keeps D8's "first N placements" semantics.
    if visited.len() >= MAX_COMPASS_ENUM_STATES {
        return;
    }

    // A placement is complete when every *specified* direction is exactly
    // satisfied.  Unspecified directions are unbounded, so the region is still
    // complete even if it has cells there.  Single-cell regions are valid when
    // no direction is specified (a clue with spec=0 can be its own region).
    let all_satisfied = compass.up.map_or(true, |v| counts[0] as i64 == v)
        && compass.down.map_or(true, |v| counts[1] as i64 == v)
        && compass.right.map_or(true, |v| counts[2] as i64 == v)
        && compass.left.map_or(true, |v| counts[3] as i64 == v);

    if all_satisfied {
        results.push(current.to_vec());
        // Continue growing: an unspecified direction may still add cells while
        // staying within the specified counts, yielding distinct larger
        // regions.  (When all four directions are specified and saturated the
        // `at_limit` check below forbids every growth.)
    }

    let h = puzzle.height;
    let w = puzzle.width;
    for &next in frontier.iter() {
        let dr = next[0] as isize - cr_i;
        let dc = next[1] as isize - cc_i;

        // Half-plane membership — a quadrant cell belongs to TWO directions (a
        // NW cell is both North and West), matching `validate::check_compass`,
        // which tallies each half-plane independently.  (The old `dir_idx`
        // chain assigned every cell to exactly one bucket with priority
        // N>S>E>W, so e.g. a NW cell never incremented `left` and the true
        // placement of a clue like 0312's (1,1) `left=4` could never reach
        // `all_satisfied` — pieces solved 0/103 official compass puzzles.)
        let mut touches = [false; 4]; // 0=N, 1=S, 2=E, 3=W
        if dr < 0 {
            touches[0] = true;
        }
        if dr > 0 {
            touches[1] = true;
        }
        if dc > 0 {
            touches[2] = true;
        }
        if dc < 0 {
            touches[3] = true;
        }

        // Check direction limits BEFORE recursing — a quadrant cell consumes
        // budget in two directions and must fit both.  Since no count can
        // exceed its target this way and every non-clue cell lies in at least
        // one half-plane, `Σ counts ≥ |R|-1` gives the size bound
        // `|R| ≤ 1 + Σ targets` automatically.
        let limits = [compass.up, compass.down, compass.right, compass.left];
        let at_limit = (0..4).any(|i| touches[i] && limits[i].map_or(false, |v| counts[i] >= v as usize));
        if at_limit {
            continue;
        }

        let mut new_counts = *counts;
        for i in 0..4 {
            if touches[i] {
                new_counts[i] += 1;
            }
        }
        let mut new_current = current.to_vec();
        new_current.push(next);
        new_current.sort_unstable();
        if !visited.insert(new_current.clone()) {
            continue; // already expanded via another addition order
        }

        // New frontier = old frontier minus `next`, plus `next`'s neighbors.
        // Unlike the old shared-`candidates` scheme, the untried siblings stay
        // available to this branch — that is what makes the enumeration
        // complete.
        let mut new_frontier = frontier.clone();
        new_frontier.remove(&next);
        for (nr, nc) in neighbor_positions(next[0], next[1], h, w) {
            let pos = [nr, nc];
            if puzzle.cells[nr][nc].blocked {
                continue;
            }
            if is_precut(puzzle, next[0], next[1], nr, nc) {
                continue;
            }
            if pos == next || new_current.contains(&pos) {
                continue;
            }
            new_frontier.insert(pos);
        }

        compass_rec(
            puzzle,
            &new_current,
            &new_counts,
            &new_frontier,
            cr_i,
            cc_i,
            compass,
            results,
            visited,
            deadline,
        );
        if results.len() >= MAX_COMPASS_PLACEMENTS {
            return;
        }
    }
}

fn is_precut(puzzle: &Puzzle, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
    if r1 == r2 {
        let c = c1.min(c2);
        puzzle.h_edges[r1][c].is_boundary
    } else {
        let r = r1.min(r2);
        puzzle.v_edges[r][c1].is_boundary
    }
}

fn reconstruct_and_validate(
    puzzle: &Puzzle,
    placements: &[Placement],
    row_ids: &[usize],
    ctx: &SolveContext,
) -> Option<Vec<RegionInfo>> {
    let mut piece_of_cell: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (i, &row_id) in row_ids.iter().enumerate() {
        for &[r, c] in &placements[row_id].cells {
            piece_of_cell.insert((r, c), i);
        }
    }

    // Coverage check
    for &[r, c] in &ctx.fillable {
        if !piece_of_cell.contains_key(&(r, c)) {
            return None;
        }
    }

    // Edge constraints
    if !edge_constraints_ok(placements, row_ids, &piece_of_cell, ctx) {
        if std::env::var("EDGE_CSP_DEBUG").is_ok() {
            eprintln!("pieces: reject - edge_constraints");
        }
        return None;
    }

    // Watchtowers
    for (ref cells, target) in &ctx.watchtowers {
        let mut pieces = Vec::new();
        for &[r, c] in cells {
            if let Some(&p) = piece_of_cell.get(&(r, c)) {
                if !pieces.contains(&p) { pieces.push(p); }
            }
        }
        if pieces.len() != *target { return None; }
    }

    let regions: Vec<RegionInfo> = row_ids.iter().enumerate().map(|(rid, &row_id)| {
        let p = &placements[row_id];
        let mut norm = p.shape.clone();
        normalize(&mut norm);
        RegionInfo {
            region_id: rid,
            cells: p.cells.clone(),
            area: p.area,
            shape: norm,
            normalized_shape_key: canonical_key(&p.shape),
            matched_shape_name: None,
        }
    }).collect();

    // Full independent re-validation via `validate.rs` — the same acceptance
    // gate aog and rose use.  Covers every rule type, so a DLX tiling that
    // violates fence / compass / ring / rose_window (previously stubbed in
    // `constraints.rs`) is rejected here instead of slipping to the router.
    if !crate::solver::validate::validate(puzzle, &regions) {
        if std::env::var("EDGE_CSP_DEBUG").is_ok() {
            eprintln!("pieces: reject - validate");
        }
        return None;
    }

    Some(regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dedup_placements(results: Vec<Vec<[usize; 2]>>) -> Vec<Vec<[usize; 2]>> {
        let mut uniq: BTreeSet<Vec<[usize; 2]>> = BTreeSet::new();
        for mut r in results {
            r.sort();
            uniq.insert(r);
        }
        uniq.into_iter().collect()
    }

    /// Compass direction counts must use half-plane membership (a NW cell is
    /// both North and West), matching `validate::check_compass`.  The old
    /// `dir_idx` chain counted each cell once with priority N>S>E>W: it
    /// rejected the placement whose `left=1` is carried by the NW cell (1,1),
    /// and accepted an illegal one that put a second west cell on the W-axis.
    #[test]
    fn compass_placement_counts_half_planes() {
        // 4x4, clue (2,2): up=2, down=0, left=1, right=1.
        // {(1,1),(1,2),(2,2),(2,3)} is valid: NW cell (1,1) fills up AND left.
        // {(1,1),(1,2),(2,1),(2,2),(2,3)} is NOT: left=2 under half-planes,
        // though the old exclusive count saw left=1 (only (2,1)).
        let json = r#"{"grid":{"height":4,"width":4},
            "cells":[{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2},{"row":0,"col":3},
                     {"row":1,"col":0},{"row":1,"col":1},{"row":1,"col":2},{"row":1,"col":3},
                     {"row":2,"col":0},{"row":2,"col":1},
                     {"row":2,"col":2,"compass":{"up":2,"down":0,"left":1,"right":1}},
                     {"row":2,"col":3},
                     {"row":3,"col":0},{"row":3,"col":1},{"row":3,"col":2},{"row":3,"col":3}],
            "edges":[],"vertices":[],"rules":[{"type":"compass"}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        let comp = puzzle.cells[2][2].compass.clone().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let results = dedup_placements(generate_compass_polyominoes(
            &puzzle, 2, 2, &comp, deadline,
        ));
        // The NW-covers-two-directions placement must be present.
        assert!(
            results.contains(&vec![[1, 1], [1, 2], [2, 2], [2, 3]]),
            "half-plane placement missing: {:?}",
            results
        );
        // The W-axis-extra placement (left=2) must be rejected.
        assert!(!results.contains(&vec![[1, 1], [1, 2], [2, 1], [2, 2], [2, 3]]));
        // Every emitted placement must satisfy the half-plane counts exactly.
        for p in &results {
            let mut counts = [0usize; 4];
            for &[r, c] in p {
                if r < 2 {
                    counts[0] += 1;
                }
                if r > 2 {
                    counts[1] += 1;
                }
                if c > 2 {
                    counts[2] += 1;
                }
                if c < 2 {
                    counts[3] += 1;
                }
            }
            assert_eq!(counts, [2, 0, 1, 1], "bad placement {:?}", p);
        }
    }

    /// A quadrant cell consumes budget in BOTH directions: `left=0` forbids
    /// every west-half cell (including NW/SW), not just same-row west ones.
    #[test]
    fn compass_zero_direction_rejects_quadrant_cells() {
        // 3x3, clue (1,1): up=1, down=1, left=0, right=0.
        // Only placement: the N- and S-axis cells {(0,1),(1,1),(2,1)} —
        // (0,0)/(2,0) would touch `left`.
        let json = r#"{"grid":{"height":3,"width":3},
            "cells":[{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2},
                     {"row":1,"col":0},
                     {"row":1,"col":1,"compass":{"up":1,"down":1,"left":0,"right":0}},
                     {"row":1,"col":2},
                     {"row":2,"col":0},{"row":2,"col":1},{"row":2,"col":2}],
            "edges":[],"vertices":[],"rules":[{"type":"compass"}]}"#;
        let puzzle = crate::io::parse_puzzle(json).unwrap();
        let comp = puzzle.cells[1][1].compass.clone().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let results = generate_compass_polyominoes(&puzzle, 1, 1, &comp, deadline);
        assert_eq!(
            dedup_placements(results),
            vec![vec![[0, 1], [1, 1], [2, 1]]]
        );
    }
}
