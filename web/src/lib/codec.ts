// Puzzle JSON query/normalization helpers. The solver speaks the JSON protocol
// directly, so the UI only needs a few maps to render the grid and overlay the
// solution. No separate in-app model is required.

import type { CompassJson, PuzzleJson, RegionCells, RegionJson } from './types'

export function cellKey(r: number, c: number): string {
  return `${r},${c}`
}

// Horizontal edge between (r,c) and (r,c+1).
export function hEdgeKey(r: number, c: number): string {
  return `h:${r},${c}`
}

// Vertical edge between (r,c) and (r+1,c).
export function vEdgeKey(r: number, c: number): string {
  return `v:${r},${c}`
}

export interface PuzzleModel {
  height: number
  width: number
  blocked: Set<string>
  numbers: Map<string, number>
  symbols: Map<string, string>
  compass: Map<string, CompassJson>
  boundaries: Set<string> // hEdgeKey / vEdgeKey of pre-drawn or constraint edges
  rules: PuzzleJson['rules']
}

/** Flatten a puzzle JSON into render-friendly maps. */
export function modelFromPuzzle(p: PuzzleJson): PuzzleModel {
  const m: PuzzleModel = {
    height: p.grid.height,
    width: p.grid.width,
    blocked: new Set(),
    numbers: new Map(),
    symbols: new Map(),
    compass: new Map(),
    boundaries: new Set(),
    rules: p.rules,
  }
  for (const c of p.cells) {
    const k = cellKey(c.row, c.col)
    if (c.blocked) m.blocked.add(k)
    if (c.number != null) m.numbers.set(k, c.number)
    if (c.symbol != null) m.symbols.set(k, c.symbol)
    if (c.compass) m.compass.set(k, c.compass)
  }
  for (const e of p.edges) {
    if (!e.is_boundary) continue
    const { r1, c1, r2, c2 } = e
    if (r1 === r2) m.boundaries.add(hEdgeKey(r1, Math.min(c1, c2)))
    else m.boundaries.add(vEdgeKey(Math.min(r1, r2), c1))
  }
  return m
}

/**
 * Map every cell to its region index (0-based, in input order). Accepts either
 * the solver's `RegionJson[]` (which has `.cells`) or the official answer's
 * bare `RegionCells[]`.
 */
export function cellRegionMap(
  regions: RegionJson[] | RegionCells[],
): Map<string, number> {
  const map = new Map<string, number>()
  regions.forEach((region, i) => {
    const cells: RegionCells = 'cells' in region ? (region as RegionJson).cells : (region as RegionCells)
    for (const [r, c] of cells) map.set(cellKey(r, c), i)
  })
  return map
}

/** Stable, distinct pastel hue for a region index (golden-angle spacing). */
export function regionColor(i: number): string {
  const hue = (i * 137.508) % 360
  return `hsl(${hue.toFixed(1)}, 62%, 74%)`
}

/** Serialize an edited puzzle back to the JSON protocol (for user-built puzzles). */
export function buildPuzzleJson(
  height: number,
  width: number,
  model: Omit<PuzzleModel, 'height' | 'width' | 'rules'>,
  rules: PuzzleJson['rules'] = [],
): PuzzleJson {
  const cells: PuzzleJson['cells'] = []
  for (let r = 0; r < height; r++) {
    for (let c = 0; c < width; c++) {
      const k = cellKey(r, c)
      const cell: PuzzleJson['cells'][number] = { row: r, col: c }
      if (model.blocked.has(k)) cell.blocked = true
      const n = model.numbers.get(k)
      if (n != null) cell.number = n
      const s = model.symbols.get(k)
      if (s != null) cell.symbol = s
      const comp = model.compass.get(k)
      if (comp) cell.compass = comp
      cells.push(cell)
    }
  }

  const edges: PuzzleJson['edges'] = []
  for (let r = 0; r < height; r++) {
    for (let c = 0; c + 1 < width; c++) {
      edges.push({
        r1: r, c1: c, r2: r, c2: c + 1,
        is_boundary: model.boundaries.has(hEdgeKey(r, c)),
      })
    }
  }
  for (let r = 0; r + 1 < height; r++) {
    for (let c = 0; c < width; c++) {
      edges.push({
        r1: r, c1: c, r2: r + 1, c2: c,
        is_boundary: model.boundaries.has(vEdgeKey(r, c)),
      })
    }
  }

  return { grid: { height, width }, cells, edges, vertices: [], rules }
}
