// JSON protocol types — mirrors `rsolver/src/io.rs` (deserialization) and
// `src/io/puzzle_codec.py` (serialization). The single source of truth is the
// Rust solver's input format; these types exist so the UI and the bundler
// produce/consume the exact same shape.

export interface GridJson {
  height: number
  width: number
}

export interface CompassJson {
  up?: number | null
  down?: number | null
  left?: number | null
  right?: number | null
}

export interface CellJson {
  row: number
  col: number
  number?: number | null
  symbol?: string | null
  blocked?: boolean
  compass?: CompassJson | null
  fence_pattern?: [number, number][] | null
  shape_pattern?: [number, number][] | null
}

export interface EdgeConstraintJson {
  type: string
  value?: number | null
}

export interface EdgeJson {
  r1: number
  c1: number
  r2: number
  c2: number
  is_boundary?: boolean
  constraint?: EdgeConstraintJson | null
}

export interface VertexJson {
  row: number
  col: number
  watchtower?: number | null
}

export interface OuterBoundaryJson {
  r1: number
  c1: number
  r2: number
  c2: number
}

export interface RuleJson {
  type: string
  params?: Record<string, unknown>
}

export interface PuzzleJson {
  version?: string
  grid: GridJson
  cells: CellJson[]
  edges: EdgeJson[]
  vertices: VertexJson[]
  outer_boundaries?: OuterBoundaryJson[]
  rules: RuleJson[]
  shape_pool?: [number, number][][]
}

// A region is a list of `[r, c]` cell coordinates. The solver returns richer
// region objects (with area / shape / names); the official answer files carry
// only the bare cell lists. Both reduce to the same "cell → region index" map.
export type RegionCells = [number, number][]

export interface RegionJson {
  region_id: number
  cells: RegionCells
  area: number
  shape: RegionCells
  normalized_shape_key: string
  matched_shape_name?: string | null
}

export interface SolutionJson {
  solved: boolean
  steps_taken: number
  elapsed_ms: number
  error_message?: string | null
  regions: RegionJson[]
  rule_results: Record<string, boolean>
  solver: string
}

// Official answer file (`puzzles/official/<Zone>-answer/…`): `{ regions, … }`.
export interface OfficialAnswerJson {
  version?: string
  grid?: GridJson
  regions: RegionCells[]
  _meta?: unknown
}

// A bundled puzzle+answer entry, produced by `scripts/bundle-puzzles.mjs`.
export interface BundledPuzzle {
  id: string
  zone: string
  category: string
  puzzle: PuzzleJson
  answer: RegionCells[] | null
}
