//! JSON I/O boundary: puzzle/solution (de)serialization, `Puzzle` building, and
//! the single-line solve helper.  Kept out of `main.rs` so the entry point stays
//! a thin stdin/stdout dispatcher.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::solver;
use crate::types::*;

// ── JSON models (I/O boundary) ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GridJson {
    height: usize,
    width: usize,
}

#[derive(Debug, Deserialize)]
struct CellJson {
    row: usize,
    col: usize,
    #[serde(default)]
    number: Option<i64>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    blocked: bool,
    #[serde(default)]
    compass: Option<CompassJson>,
    #[serde(default)]
    fence_pattern: Option<Vec<[i64; 2]>>,
    #[serde(default)]
    shape_pattern: Option<Vec<[i64; 2]>>,
}

#[derive(Debug, Deserialize)]
struct CompassJson {
    up: Option<i64>,
    down: Option<i64>,
    left: Option<i64>,
    right: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct EdgeJson {
    r1: usize,
    c1: usize,
    r2: usize,
    c2: usize,
    #[serde(rename = "is_boundary", default)]
    is_boundary: bool,
    #[serde(default)]
    constraint: Option<EdgeConstraintJson>,
}

#[derive(Debug, Deserialize)]
struct EdgeConstraintJson {
    #[serde(rename = "type")]
    ctype: String,
    #[serde(default)]
    value: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct VertexJson {
    row: usize,
    col: usize,
    #[serde(default)]
    watchtower: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OuterBoundaryJson {
    r1: usize, c1: usize,
    r2: usize, c2: usize,
}

#[derive(Debug, Deserialize)]
struct RuleJson {
    #[serde(rename = "type")]
    ctype: String,
    #[serde(default)]
    params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PuzzleJson {
    grid: GridJson,
    cells: Vec<CellJson>,
    edges: Vec<EdgeJson>,
    vertices: Vec<VertexJson>,
    #[serde(rename = "outer_boundaries", default)]
    outer_boundaries: Vec<OuterBoundaryJson>,
    rules: Vec<RuleJson>,
    #[serde(rename = "shape_pool", default)]
    shape_pool: Vec<Vec<[i64; 2]>>,
}

#[derive(Debug, Serialize)]
struct RegionJson {
    region_id: usize,
    cells: Vec<[usize; 2]>,
    area: usize,
    shape: Vec<[usize; 2]>,
    normalized_shape_key: String,
    matched_shape_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct SolutionJson {
    solved: bool,
    steps_taken: u64,
    elapsed_ms: u64,
    error_message: Option<String>,
    regions: Vec<RegionJson>,
    rule_results: HashMap<String, bool>,
    /// Which solver module produced this result (aog / rose / pieces /
    /// backtrack; empty for errors / empty-grid / timeouts).
    solver: String,
}

// ── Board builder ─────────────────────────────────────────────────────────────

fn normalize_compass_value(v: Option<i64>) -> Option<i64> {
    match v {
        Some(x) if x >= 0 => Some(x),
        _ => None,
    }
}

fn build_puzzle(input: &PuzzleJson) -> Result<Puzzle, String> {
    let h = input.grid.height;
    let w = input.grid.width;

    if h == 0 || w == 0 {
        return Err(format!("invalid grid size {}x{}", h, w));
    }

    // Build cells
    let mut cells: Vec<Vec<Cell>> = (0..h)
        .map(|r| (0..w).map(|c| Cell::new(r, c)).collect())
        .collect();
    for cd in &input.cells {
        if cd.row >= h || cd.col >= w {
            return Err(format!(
                "cell out of range: ({},{}) in {}x{} grid",
                cd.row, cd.col, h, w
            ));
        }
        let c = &mut cells[cd.row][cd.col];
        c.row = cd.row;
        c.col = cd.col;
        c.number = cd.number;
        c.symbol = cd.symbol.clone();
        c.blocked = cd.blocked;
        c.compass = cd.compass.as_ref().map(|cp| CompassClue {
            up: normalize_compass_value(cp.up),
            down: normalize_compass_value(cp.down),
            left: normalize_compass_value(cp.left),
            right: normalize_compass_value(cp.right),
        });
        c.fence_pattern = cd
            .fence_pattern
            .as_ref()
            .map(|s| s.iter().map(|xy| [xy[0] as usize, xy[1] as usize]).collect());
        c.shape_pattern = cd
            .shape_pattern
            .as_ref()
            .map(|s| s.iter().map(|xy| [xy[0] as usize, xy[1] as usize]).collect());
    }

    // Build edges
    let mut h_edges = vec![vec![Edge::default(); w.saturating_sub(1)]; h];
    let mut v_edges = vec![vec![Edge::default(); w]; h.saturating_sub(1)];

    for ed in &input.edges {
        let (r1, c1, r2, c2) = (ed.r1, ed.c1, ed.r2, ed.c2);
        // An edge must join two adjacent in-grid cells: same row & adjacent
        // cols (horizontal) or same col & adjacent rows (vertical).
        let horizontal = r1 == r2 && c1.abs_diff(c2) == 1;
        let vertical = c1 == c2 && r1.abs_diff(r2) == 1;
        if !horizontal && !vertical {
            return Err(format!(
                "invalid edge ({},{})-({},{}): endpoints are not adjacent cells",
                r1, c1, r2, c2
            ));
        }
        if r1 >= h || c1 >= w || r2 >= h || c2 >= w {
            return Err(format!(
                "edge out of range ({},{})-({},{}): grid is {}x{}",
                r1, c1, r2, c2, h, w
            ));
        }
        let edge = if horizontal {
            &mut h_edges[r1][c1.min(c2)]
        } else {
            &mut v_edges[r1.min(r2)][c1]
        };
        edge.is_boundary = ed.is_boundary;
        if let Some(ref c) = ed.constraint {
            let ctype = EdgeConstraintType::from_str(&c.ctype)
                .ok_or_else(|| format!("unknown edge constraint type: {}", c.ctype))?;
            edge.constraint = Some(EdgeConstraint {
                ctype,
                value: c.value,
            });
            // Constraint edges (inequality / difference / heterogeneous /
            // homogeneous) explicitly require the two cells to be in different
            // regions, so the edge between them MUST be a boundary.  Setting this
            // at parse time lets ALL solvers (backtrack / pieces / rose) respect
            // the constraint through `is_adjacent_free()` / `is_precut()` without
            // needing solver-specific encoding (aog already does this via
            // LINE_BLOCK in core.rs).
            edge.is_boundary = true;
        }
    }

    // Build vertices.  Vertex (r,c) is the ABSOLUTE grid corner at (r,c):
    // r in 0..=h, c in 0..=w (border corners included).  A border corner is
    // touched by fewer than 4 cells (2 on an edge, 1 on a grid corner).
    let vh = h + 1;
    let vw = w + 1;
    let mut vertices = vec![vec![Vertex::default(); vw]; vh];
    for vd in &input.vertices {
        if vd.row > h || vd.col > w {
            return Err(format!(
                "vertex out of range: ({},{}) in {}x{} grid",
                vd.row, vd.col, h, w
            ));
        }
        vertices[vd.row][vd.col].watchtower = vd.watchtower;
    }

    // Shape pool
    let shape_pool: Vec<Shape> = input.shape_pool.iter().map(|s| {
        s.iter().map(|xy| [xy[0] as usize, xy[1] as usize]).collect()
    }).collect();

    // Rules
    let rules: Vec<Rule> = input.rules.iter().map(|r| Rule {
        ctype: r.ctype.clone(),
        params: r.params.clone(),
    }).collect();

    // Outer border segments, kept for round-tripping.
    let outer_boundaries: Vec<[usize; 4]> = input
        .outer_boundaries
        .iter()
        .map(|o| [o.r1, o.c1, o.r2, o.c2])
        .collect();

    Ok(Puzzle {
        height: h,
        width: w,
        cells,
        h_edges,
        v_edges,
        vertices,
        rules,
        shape_pool,
        outer_boundaries,
    })
}

// ── Output formatter ──────────────────────────────────────────────────────────

fn solution_to_json(sol: &Solution) -> SolutionJson {
    SolutionJson {
        solved: sol.solved,
        steps_taken: sol.steps_taken,
        elapsed_ms: sol.elapsed_ms,
        error_message: sol.error_message.clone(),
        regions: sol.regions.iter().map(|r| RegionJson {
            region_id: r.region_id,
            cells: r.cells.clone(),
            area: r.area,
            shape: r.shape.clone(),
            normalized_shape_key: r.normalized_shape_key.clone(),
            matched_shape_name: r.matched_shape_name.clone(),
        }).collect(),
        rule_results: sol.rule_results.clone(),
        solver: sol.solver.clone(),
    }
}

// ── Helpers used by the entry point ───────────────────────────────────────────

/// Parse one puzzle JSON string into a `Puzzle` (JSON errors and build errors
/// both surface as `Err`).
pub fn parse_puzzle(input: &str) -> Result<Puzzle, String> {
    let puzzle_json: PuzzleJson = serde_json::from_str(input)
        .map_err(|e| format!("Error parsing JSON: {}", e))?;
    build_puzzle(&puzzle_json).map_err(|e| format!("Error building puzzle: {}", e))
}

/// Solve one compact puzzle-JSON line (batch mode).  A malformed line yields an
/// unsolved `Solution` carrying the error, never a panic — the caller keeps the
/// 1 input line → 1 output line invariant.
pub fn solve_json_line(line: &str) -> Solution {
    match parse_puzzle(line) {
        Ok(puzzle) => solver::solve(&puzzle, 30_000), // 30s timeout
        Err(e) => Solution::unsolved(e),
    }
}

/// Serialize a solution to the JSON text emitted on stdout.
pub fn solution_to_json_text(sol: &Solution) -> String {
    serde_json::to_string(&solution_to_json(sol)).unwrap()
}
