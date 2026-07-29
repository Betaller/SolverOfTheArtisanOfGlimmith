//! rsolver — puzzle solver for "The Artisan of Glimmith".
//!
//! Reads puzzle JSON from stdin, writes solution JSON to stdout.
//! Usage: rsolver [--parse] {file}

mod constraints;
mod dlx;
mod grid;
mod polyomino;
mod solver;
mod types;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use types::*;

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
}

// ── Board builder ─────────────────────────────────────────────────────────────

fn normalize_compass_value(v: Option<i64>) -> Option<i64> {
    match v {
        Some(x) if x >= 0 => Some(x),
        _ => None,
    }
}

fn build_puzzle(input: &PuzzleJson) -> Puzzle {
    let h = input.grid.height;
    let w = input.grid.width;

    // Build cells
    let mut cells: Vec<Vec<Cell>> = (0..h)
        .map(|r| (0..w).map(|c| Cell::new(r, c)).collect())
        .collect();
    for cd in &input.cells {
        let c = &mut cells[cd.row][cd.col];
        c.row = cd.row;
        c.col = cd.col;
        c.number = cd.number;
        c.symbol = cd.symbol.as_ref().and_then(|s| s.chars().next());
        c.blocked = cd.blocked;
        c.compass = cd.compass.as_ref().map(|cp| CompassClue {
            up: normalize_compass_value(cp.up),
            down: normalize_compass_value(cp.down),
            left: normalize_compass_value(cp.left),
            right: normalize_compass_value(cp.right),
        });
    }

    // Build edges
    let mut h_edges = vec![vec![Edge::default(); w.saturating_sub(1)]; h];
    let mut v_edges = vec![vec![Edge::default(); w]; h.saturating_sub(1)];

    for ed in &input.edges {
        let (r1, c1, r2, c2) = (ed.r1, ed.c1, ed.r2, ed.c2);
        let edge = if r1 == r2 {
            &mut h_edges[r1][c1.min(c2)]
        } else {
            &mut v_edges[r1.min(r2)][c1]
        };
        edge.is_boundary = ed.is_boundary;
        edge.constraint = ed.constraint.as_ref().and_then(|c| {
            EdgeConstraintType::from_str(&c.ctype).map(|t| EdgeConstraint {
                ctype: t,
                value: c.value,
            })
        });
    }

    // Build vertices
    let vh = h.saturating_sub(1);
    let vw = w.saturating_sub(1);
    let mut vertices = vec![vec![Vertex::default(); vw]; vh];
    for vd in &input.vertices {
        if vd.row < vh && vd.col < vw {
            vertices[vd.row][vd.col].watchtower = vd.watchtower;
        }
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

    Puzzle {
        height: h,
        width: w,
        cells,
        h_edges,
        v_edges,
        vertices,
        rules,
        shape_pool,
    }
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
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut input = String::new();

    if args.len() >= 2 {
        // Read from file argument
        let path = PathBuf::from(&args[1]);
        std::fs::File::open(&path)
            .unwrap_or_else(|e| {
                eprintln!("Error opening {}: {}", path.display(), e);
                std::process::exit(1);
            })
            .read_to_string(&mut input)
            .unwrap_or_else(|e| {
                eprintln!("Error reading {}: {}", path.display(), e);
                std::process::exit(1);
            });
    } else {
        // Read from stdin
        io::stdin()
            .read_to_string(&mut input)
            .unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(1);
            });
    }

    let puzzle_json: PuzzleJson = serde_json::from_str(&input).unwrap_or_else(|e| {
        eprintln!("Error parsing JSON: {}", e);
        std::process::exit(1);
    });

    let puzzle = build_puzzle(&puzzle_json);
    let solution = solver::solve(&puzzle, 30_000); // 30s timeout

    let output = serde_json::to_string(&solution_to_json(&solution)).unwrap();
    let _ = io::stdout().write_all(output.as_bytes());
    let _ = io::stdout().write_all(b"\n");
}
