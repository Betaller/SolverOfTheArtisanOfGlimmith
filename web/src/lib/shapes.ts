// Shape helpers — faithful port of `src/solver/shapes.py`.
// A shape is a list of `[r, c]` cells, normalized to origin. The canonical key
// is the lexicographically smallest of the 8 rotations/reflections (dihedral).

export type ShapeCells = [number, number][]

export function normalize(cells: ShapeCells): ShapeCells {
  if (cells.length === 0) return []
  const minR = Math.min(...cells.map(([r]) => r))
  const minC = Math.min(...cells.map(([, c]) => c))
  return cells.map(([r, c]) => [r - minR, c - minC] as [number, number]).sort((a, b) => a[0] - b[0] || a[1] - b[1])
}

function rotate90(cells: ShapeCells): ShapeCells {
  return cells.map(([r, c]) => [c, -r] as [number, number])
}

function flipHorizontal(cells: ShapeCells): ShapeCells {
  return cells.map(([r, c]) => [r, -c] as [number, number])
}

export function canonicalKey(cells: ShapeCells): string {
  let current: ShapeCells = cells
  const transforms: ShapeCells[] = []
  for (let i = 0; i < 4; i++) {
    transforms.push(normalize(current))
    transforms.push(normalize(flipHorizontal(current)))
    current = rotate90(current)
  }
  const best = transforms
    .map((t) => JSON.stringify([...t].sort((a, b) => a[0] - b[0] || a[1] - b[1])))
    .sort()[0]
  return best
}

/** All distinct free polyominoes of exactly `n` cells. */
export function enumeratePolyominoes(n: number): ShapeCells[] {
  if (n <= 0) return []
  if (n === 1) return [[[0, 0]]]
  const smaller = enumeratePolyominoes(n - 1)
  const seen = new Set<string>()
  const result: ShapeCells[] = []
  for (const shape of smaller) {
    for (const [r, c] of shape) {
      for (const [dr, dc] of [[-1, 0], [1, 0], [0, -1], [0, 1]] as [number, number][]) {
        const nr = r + dr
        const nc = c + dc
        if (shape.some(([rr, cc]) => rr === nr && cc === nc)) continue
        const grown = [...shape, [nr, nc] as [number, number]]
        const key = canonicalKey(grown)
        if (!seen.has(key)) {
          seen.add(key)
          result.push(normalize(grown))
        }
      }
    }
  }
  return result
}
