// In-memory editable puzzle model. The JSON protocol IS the model: we keep a
// `PuzzleJson` in the store and provide O(1)-ish helpers over its flat arrays.
// (Grids are ≤50×50, so linear scans are fine; the canvas caches maps for render.)

import type { CellJson, CompassJson, EdgeJson, EdgeConstraintJson, PuzzleJson, RegionCells, RegionJson, RuleJson, VertexJson } from './types'

export const cellKey = (r: number, c: number): string => `${r},${c}`
export const vertexKey = (r: number, c: number): string => `v:${r},${c}`
// Canonical edge key: horizontal → `h:r,minc`, vertical → `v:minr,c`.
export const edgeKey = (r1: number, c1: number, r2: number, c2: number): string =>
  r1 === r2 ? `h:${r1},${Math.min(c1, c2)}` : `v:${Math.min(r1, r2)},${c1}`

export function emptyPuzzle(height: number, width: number): PuzzleJson {
  const cells: CellJson[] = []
  for (let r = 0; r < height; r++) for (let c = 0; c < width; c++) cells.push({ row: r, col: c })
  const edges: EdgeJson[] = []
  for (let r = 0; r < height; r++) for (let c = 0; c + 1 < width; c++) edges.push({ r1: r, c1: c, r2: r, c2: c + 1 })
  for (let r = 0; r + 1 < height; r++) for (let c = 0; c < width; c++) edges.push({ r1: r, c1: c, r2: r + 1, c2: c })
  const vertices: VertexJson[] = []
  for (let r = 0; r <= height; r++) for (let c = 0; c <= width; c++) vertices.push({ row: r, col: c })
  return { grid: { height, width }, cells, edges, vertices, rules: [] }
}

export function cellAt(p: PuzzleJson, r: number, c: number): CellJson | undefined {
  return p.cells.find((x) => x.row === r && x.col === c)
}

export function edgeBetween(p: PuzzleJson, r1: number, c1: number, r2: number, c2: number): EdgeJson | undefined {
  const k = edgeKey(r1, c1, r2, c2)
  return p.edges.find((e) => edgeKey(e.r1, e.c1, e.r2, e.c2) === k)
}

export function vertexAt(p: PuzzleJson, r: number, c: number): VertexJson | undefined {
  return p.vertices.find((v) => v.row === r && v.col === c)
}

export function rule(p: PuzzleJson, type: string): RuleJson | undefined {
  return p.rules.find((r) => r.type === type)
}

/** Map every cell to its region index (0-based, input order). */
export function cellRegionMap(regions: RegionJson[] | RegionCells[]): Map<string, number> {
  const map = new Map<string, number>()
  regions.forEach((region, i) => {
    const cells = 'cells' in region ? (region as RegionJson).cells : (region as RegionCells)
    for (const [r, c] of cells) map.set(cellKey(r, c), i)
  })
  return map
}

export type EdgeConstraintType = 'heterogeneous' | 'homogeneous' | 'inequality' | 'difference'

export function makeConstraint(type: EdgeConstraintType, value?: number): EdgeConstraintJson {
  return { type, value }
}

export function makeCompass(up: number, down: number, left: number, right: number): CompassJson {
  return { up, down, left, right }
}
