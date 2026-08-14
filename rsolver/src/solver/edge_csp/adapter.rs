//! Adapter: `crate::types::Puzzle` → edge_csp solver input.
//!
//! Maps the project's rule/clue model onto the edge-variable representation:
//! - `ring`/`brick`/`differentiation` → `GlobalRules` flags
//! - `area` numbers / `compass` clues → `CellClue`
//! - `inequality`/`difference` edge constraints → `EdgeClue` (the edge is
//!   already `is_boundary` from `io.rs`, so it starts as `Cut` automatically)
//! - `watchtower` vertex clues → `VertexClue`
//! - `is_boundary` edges → pre-cut (`Cut`) edges
//!
//! `heterogeneous`/`homogeneous` (shape-delta/gemini) edge constraints are
//! deliberately not ported as clues: the edge is already cut and the shape
//! relation is enforced by the router's `validate::validate`, not by a
//! propagator here.

use super::grid::Grid;
use super::types::*;
use crate::types::{EdgeConstraintType, Puzzle};

/// Everything the edge_csp solver needs, pre-digested from a `Puzzle`.
pub struct Input {
    pub grid: Grid,
    pub cell_clues: Vec<CellClue>,
    pub edge_clues: Vec<EdgeClue>,
    pub vertex_clues: Vec<VertexClue>,
    pub rules: GlobalRules,
    pub pre_cut: Vec<EdgeId>,
    /// Global area lower bound from `precise`/`range` only (compass minima are
    /// per-component and handled in `build_components`).
    pub min_area: usize,
    /// Global area upper bound (`precise`/`range` capped by the largest
    /// pre-cut component — B2).
    pub max_area: usize,
}

/// Map a `fence_pattern` (a 3x3 cross: centre `[1,1]` + cut-edge markers
/// `[0,1]`=up / `[2,1]`=down / `[1,0]`=left / `[1,2]`=right) to a `PalisadeKind`.
/// Returns `None` for a malformed pattern (safe: just loses pruning).
fn palisade_kind(fp: &[[usize; 2]]) -> Option<PalisadeKind> {
    let has = |cell: [usize; 2]| fp.contains(&cell);
    let up = has([0, 1]);
    let down = has([2, 1]);
    let left = has([1, 0]);
    let right = has([1, 2]);
    let cuts = [up, down, left, right].iter().filter(|&&b| b).count();
    match cuts {
        0 => Some(PalisadeKind::None),
        1 => Some(PalisadeKind::One),
        4 => Some(PalisadeKind::All),
        3 => Some(PalisadeKind::Three),
        2 => {
            if (up && down) || (left && right) {
                Some(PalisadeKind::Opposite)
            } else {
                Some(PalisadeKind::Adjacent)
            }
        }
        _ => None,
    }
}

pub fn build_input(puzzle: &Puzzle) -> Input {
    let h = puzzle.height;
    let w = puzzle.width;
    let n = h * w;

    let cell_exists: Vec<bool> = (0..n)
        .map(|idx| !puzzle.cells[idx / w][idx % w].blocked)
        .collect();
    let grid = Grid::new(h, w, cell_exists);

    // Cell clues.
    let mut cell_clues = Vec::new();
    for r in 0..h {
        for c in 0..w {
            let cell = &puzzle.cells[r][c];
            if cell.blocked {
                continue;
            }
            let cid = grid.cell_id(r, c);
            if let Some(number) = cell.number {
                cell_clues.push(CellClue::Area {
                    cell: cid,
                    value: number as usize,
                });
            }
            if let Some(comp) = &cell.compass {
                cell_clues.push(CellClue::Compass {
                    cell: cid,
                    compass: CompassData {
                        n: comp.up.map(|v| v as usize),
                        s: comp.down.map(|v| v as usize),
                        e: comp.right.map(|v| v as usize),
                        w: comp.left.map(|v| v as usize),
                    },
                });
            }
            if let Some(fp) = &cell.fence_pattern {
                if let Some(kind) = palisade_kind(fp) {
                    cell_clues.push(CellClue::Palisade { cell: cid, kind });
                }
            }
        }
    }

    // Edge clues + pre-cut edges.  `edge_between` sidesteps the h/v naming
    // mismatch between the two grid models.
    let mut edge_clues = Vec::new();
    let mut pre_cut = Vec::new();

    let mut push_edge =
        |eid: EdgeId, is_boundary: bool, ctype: Option<&EdgeConstraintType>, value: Option<i64>| {
            if is_boundary {
                pre_cut.push(eid);
            }
            match ctype {
                Some(EdgeConstraintType::Inequality) => edge_clues.push(EdgeClue {
                    edge: eid,
                    kind: EdgeClueKind::Inequality {
                        // value==1 ⇒ the first endpoint (cell_a, the smaller CellId)
                        // is the *larger* region, so smaller_first is false.
                        smaller_first: value != Some(1),
                    },
                }),
                Some(EdgeConstraintType::Difference) => edge_clues.push(EdgeClue {
                    edge: eid,
                    kind: EdgeClueKind::Diff {
                        value: value.unwrap_or(0) as usize,
                    },
                }),
                // Heterogeneous/Homogeneous: shape relation, router-validated.
                _ => {}
            }
        };

    for r in 0..h {
        for c in 0..w.saturating_sub(1) {
            let e = &puzzle.h_edges[r][c];
            let eid = grid
                .edge_between(grid.cell_id(r, c), grid.cell_id(r, c + 1))
                .unwrap();
            let ctype = e.constraint.as_ref().map(|ec| &ec.ctype);
            push_edge(
                eid,
                e.is_boundary,
                ctype,
                e.constraint.as_ref().and_then(|ec| ec.value),
            );
        }
    }
    for r in 0..h.saturating_sub(1) {
        for c in 0..w {
            let e = &puzzle.v_edges[r][c];
            let eid = grid
                .edge_between(grid.cell_id(r, c), grid.cell_id(r + 1, c))
                .unwrap();
            let ctype = e.constraint.as_ref().map(|ec| &ec.ctype);
            push_edge(
                eid,
                e.is_boundary,
                ctype,
                e.constraint.as_ref().and_then(|ec| ec.value),
            );
        }
    }

    // Vertex clues (watchtower).
    let mut vertex_clues = Vec::new();
    for r in 0..=h {
        for c in 0..=w {
            if let Some(v) = puzzle.vertices[r][c].watchtower {
                vertex_clues.push(VertexClue {
                    vertex: grid.vertex(r, c),
                    value: v as usize,
                });
            }
        }
    }

    // Global rules + area bounds.
    let has = |name: &str| puzzle.rules.iter().any(|r| r.ctype == name);
    let rules = GlobalRules {
        bricky: has("brick"),
        loopy: has("ring"),
        size_separation: has("differentiation"),
    };

    // `area_bounds` returns (min incl. compass, max incl. B2 cap).  We want the
    // precise/range-only min (compass minima are per-component) but the B2-capped
    // max, so compute the former directly and reuse the latter.
    let mut min_area = 1usize;
    for rule in &puzzle.rules {
        match rule.ctype.as_str() {
            "precise" => {
                if let Some(v) = rule.params.get("area").and_then(|v| v.as_u64()) {
                    min_area = min_area.max(v as usize);
                }
            }
            "range" => {
                if let Some(v) = rule.params.get("min").and_then(|v| v.as_u64()) {
                    min_area = min_area.max(v as usize);
                }
            }
            _ => {}
        }
    }
    let (_, max_area) = crate::shapes::area_bounds(puzzle);

    Input {
        grid,
        cell_clues,
        edge_clues,
        vertex_clues,
        rules,
        pre_cut,
        min_area,
        max_area,
    }
}
