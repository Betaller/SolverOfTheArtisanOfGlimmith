import { describe, it, expect } from 'vitest'
import {
  cellKey, edgeKey, vertexKey, emptyPuzzle, normalizePuzzle, cellRegionMap, cellAt, edgeBetween,
} from './model'
import type { PuzzleJson } from './types'

describe('key helpers', () => {
  it('formats cell/edge/vertex keys', () => {
    expect(cellKey(2, 3)).toBe('2,3')
    expect(edgeKey(0, 1, 0, 2)).toBe('h:0,1')
    expect(edgeKey(1, 0, 2, 0)).toBe('v:1,0')
    expect(vertexKey(1, 2)).toBe('v:1,2')
  })
})

describe('emptyPuzzle', () => {
  it('synthesizes the full grid', () => {
    const p = emptyPuzzle(3, 4)
    expect(p.cells.length).toBe(12)
    expect(p.edges.length).toBe(3 * 3 + 2 * 4) // h*(w-1) + (h-1)*w
    expect(p.vertices.length).toBe(4 * 5) // (h+1)*(w+1)
  })
})

describe('normalizePuzzle', () => {
  it('synthesizes full edges/vertices from sparse corpus data (regression)', () => {
    // Official corpus stores edges/vertices sparsely (often empty). normalizePuzzle
    // must build the full grid and overlay the sparse boundary/watchtower data.
    const sparse: PuzzleJson = {
      grid: { height: 2, width: 3 },
      cells: [
        { row: 0, col: 0 }, { row: 0, col: 1, number: 5 }, { row: 0, col: 2 },
        { row: 1, col: 0 }, { row: 1, col: 1 }, { row: 1, col: 2 },
      ],
      edges: [{ r1: 0, c1: 0, r2: 0, c2: 1, is_boundary: true }],
      vertices: [{ row: 1, col: 1, watchtower: 3 }],
      rules: [],
    }
    const n = normalizePuzzle(sparse)

    expect(n.edges.length).toBe(2 * 2 + 1 * 3) // 7
    expect(n.vertices.length).toBe(3 * 4) // 12

    // sparse boundary is preserved
    expect(edgeBetween(n, 0, 0, 0, 1)?.is_boundary).toBe(true)
    // synthesized edge defaults to non-boundary
    expect(edgeBetween(n, 0, 1, 0, 2)?.is_boundary).toBeFalsy()
    // sparse vertex is preserved
    expect(n.vertices.find((v) => v.row === 1 && v.col === 1)?.watchtower).toBe(3)
    // cell props are preserved
    expect(cellAt(n, 0, 1)?.number).toBe(5)
  })
})

describe('cellRegionMap', () => {
  it('maps each cell to its region index', () => {
    const m = cellRegionMap([[[0, 0], [0, 1]], [[1, 0], [1, 1]]])
    expect(m.get('0,0')).toBe(0)
    expect(m.get('0,1')).toBe(0)
    expect(m.get('1,0')).toBe(1)
    expect(m.get('1,1')).toBe(1)
  })
})
