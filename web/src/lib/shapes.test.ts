import { describe, it, expect } from 'vitest'
import { normalize, canonicalKey, enumeratePolyominoes } from './shapes'

describe('normalize', () => {
  it('shifts cells to the origin', () => {
    expect(normalize([[2, 3], [3, 3]])).toEqual([[0, 0], [1, 0]])
  })
})

describe('canonicalKey', () => {
  it('is invariant under rotation (dihedral canonicalization)', () => {
    const l = [[0, 0], [1, 0], [2, 0], [2, 1]] as [number, number][]
    const rotated = [[0, 0], [0, 1], [0, 2], [1, 0]] as [number, number][]
    expect(canonicalKey(l)).toBe(canonicalKey(rotated))
  })

  it('distinguishes different shapes', () => {
    const domino = [[0, 0], [0, 1]] as [number, number][]
    const tromino = [[0, 0], [0, 1], [0, 2]] as [number, number][]
    expect(canonicalKey(domino)).not.toBe(canonicalKey(tromino))
  })
})

describe('enumeratePolyominoes', () => {
  it('produces the known free-polyomino counts', () => {
    const counts = [0, 1, 1, 2, 5, 12, 35, 108]
    for (let n = 1; n <= 7; n++) {
      expect(enumeratePolyominoes(n).length).toBe(counts[n])
    }
  })
})
