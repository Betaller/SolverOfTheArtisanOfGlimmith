<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useTheme } from '../composables/useTheme'
import { useViewport } from '../composables/useViewport'
import { boardPalettes, P_COLORS, REGION_COLORS, FENCE_EDGES, regionFill, regionStroke } from '../lib/theme'
import { cellKey, edgeKey, vertexAt, makeConstraint } from '../lib/model'
import type { CellJson, EdgeJson, PuzzleJson } from '../lib/types'
import AppIcon from './AppIcon.vue'

const store = usePuzzleStore()
const { theme } = useTheme()
const viewport = useViewport()
const { renderSize: cs, zoomBy, fit, publishStage, publishGrid } = viewport

const svgEl = ref<SVGSVGElement | null>(null)

const p = computed(() => store.puzzle as PuzzleJson)
const h = computed(() => p.value.grid.height)
const w = computed(() => p.value.grid.width)

// Board colours follow the app theme; the sheet is its own "paper" surface.
const C = computed(() => boardPalettes[theme.value])
const isDark = computed(() => theme.value === 'dark')

const pad = computed(() => Math.max(10, Math.min(34, Math.round(cs.value * 0.42))))
const W = computed(() => pad.value * 2 + w.value * cs.value)
const H = computed(() => pad.value * 2 + h.value * cs.value)

// ── coordinates ──────────────────────────────────────────────────────────────
const cellX = (c: number) => pad.value + c * cs.value
const cellY = (r: number) => pad.value + r * cs.value
const vx = (vc: number) => pad.value + vc * cs.value
const vy = (vr: number) => pad.value + vr * cs.value

function edgeEndpoints(e: { r1: number; c1: number; r2: number; c2: number }) {
  if (e.r1 === e.r2) {
    const x = pad.value + (Math.min(e.c1, e.c2) + 1) * cs.value
    return { x1: x, y1: pad.value + e.r1 * cs.value, x2: x, y2: pad.value + (e.r1 + 1) * cs.value }
  }
  const y = pad.value + (Math.min(e.r1, e.r2) + 1) * cs.value
  return { x1: pad.value + e.c1 * cs.value, y1: y, x2: pad.value + (e.c1 + 1) * cs.value, y2: y }
}

// O(1) cell/edge indexes — canvas lookups run per-edge/per-cell, so linear
// `cellAt`/`edgeBetween` scans (model.ts) would be O(HW × edges) on large grids.
const cellIndex = computed(() => {
  const m = new Map<string, CellJson>()
  for (const c of p.value.cells) m.set(cellKey(c.row, c.col), c)
  return m
})
const edgeIndex = computed(() => {
  const m = new Map<string, EdgeJson>()
  for (const e of p.value.edges) m.set(edgeKey(e.r1, e.c1, e.r2, e.c2), e)
  return m
})
const getCell = (r: number, c: number): CellJson | undefined => cellIndex.value.get(cellKey(r, c))
const getEdge = (r1: number, c1: number, r2: number, c2: number): EdgeJson | undefined => edgeIndex.value.get(edgeKey(r1, c1, r2, c2))

// ── render models ────────────────────────────────────────────────────────────
const cells = computed(() => {
  const dark = isDark.value
  const out: any[] = []
  for (let r = 0; r < h.value; r++) for (let c = 0; c < w.value; c++) {
    const cell = getCell(r, c)
    const blocked = !!cell?.blocked
    const ri = store.displayRegions?.get(cellKey(r, c))
    const fv = fenceDiamondValue(cell?.fence_pattern)
    // No per-cell outline: same-region neighbours must read as one continuous
    // shape. The silhouette comes from `regionOutlines`, the hairline grid from
    // `gridLines`.
    let fill = C.value.cell_bg_null
    if (blocked) fill = C.value.cell_blocked_bg
    else if (ri != null) fill = regionFill(REGION_COLORS[ri % REGION_COLORS.length], dark)
    out.push({
      r, c, x: cellX(c), y: cellY(r), blocked, region: ri ?? null, fill,
      number: cell?.number, symbol: cell?.symbol, compass: cell?.compass,
      shapePattern: cell?.shape_pattern,
      fence: fv ? fenceSegments(fv, cellX(c) + cs.value / 2, cellY(r) + cs.value / 2, cs.value) : null,
      // Staggered reveal: each region lands a frame after the previous one.
      delay: ri != null ? Math.min(ri * 16, 720) : 0,
    })
  }
  return out
})

function isAutoBoundary(e: EdgeJson): boolean {
  const c1 = getCell(e.r1, e.c1)
  const c2 = getCell(e.r2, e.c2)
  return !!(c1 && c2 && c1.blocked !== c2.blocked)
}

// In the solution display, an edge between two different assigned regions is a
// region boundary (mirrors PyQt _on_solution_ready setting is_boundary).
function separatesRegions(e: EdgeJson): boolean {
  const c1 = getCell(e.r1, e.c1)
  const c2 = getCell(e.r2, e.c2)
  if (!c1 || !c2 || c1.blocked || c2.blocked) return false
  const r1 = store.displayRegions?.get(cellKey(e.r1, e.c1))
  const r2 = store.displayRegions?.get(cellKey(e.r2, e.c2))
  return r1 != null && r2 != null && r1 !== r2
}

const boundaryLines = computed(() => {
  const lines: any[] = []
  for (const e of p.value.edges) {
    if (e.is_boundary || isAutoBoundary(e) || separatesRegions(e)) lines.push({ ...edgeEndpoints(e), key: edgeKey(e.r1, e.c1, e.r2, e.c2) })
  }
  for (const o of p.value.outer_boundaries ?? []) {
    lines.push({ x1: pad.value + o.c1 * cs.value, y1: pad.value + o.r1 * cs.value, x2: pad.value + o.c2 * cs.value, y2: pad.value + o.r2 * cs.value, key: 'o' + edgeKey(o.r1, o.c1, o.r2, o.c2) })
  }
  return lines
})

const gridLines = computed(() => {
  const lines: any[] = []
  for (const e of p.value.edges) {
    const c1 = getCell(e.r1, e.c1)
    const c2 = getCell(e.r2, e.c2)
    if (c1 && c2 && !c1.blocked && !c2.blocked) {
      const r1 = store.displayRegions?.get(cellKey(e.r1, e.c1))
      const r2 = store.displayRegions?.get(cellKey(e.r2, e.c2))
      if (r1 != null && r1 === r2) continue
    }
    lines.push(edgeEndpoints(e))
  }
  return lines
})

/**
 * Region silhouette: the outline of every solved region, drawn only on the
 * sides that face a *different* region (or a blocked cell / the board edge).
 * Sides shared with a same-region neighbour are left open so the region reads
 * as a single continuous shape instead of a patchwork of tiles.
 */
const regionOutlines = computed(() => {
  const regions = store.displayRegions
  const out: any[] = []
  if (!regions) return out
  const dark = isDark.value
  const s = cs.value
  for (let r = 0; r < h.value; r++) for (let c = 0; c < w.value; c++) {
    if (getCell(r, c)?.blocked) continue
    const ri = regions.get(cellKey(r, c))
    if (ri == null) continue
    const sameRegion = (nr: number, nc: number) => {
      if (nr < 0 || nc < 0 || nr >= h.value || nc >= w.value) return false
      if (getCell(nr, nc)?.blocked) return false
      return regions.get(cellKey(nr, nc)) === ri
    }
    const x = cellX(c), y = cellY(r)
    const color = regionStroke(REGION_COLORS[ri % REGION_COLORS.length], dark)
    if (!sameRegion(r - 1, c)) out.push({ x1: x, y1: y, x2: x + s, y2: y, color, k: `t${r},${c}` })
    if (!sameRegion(r + 1, c)) out.push({ x1: x, y1: y + s, x2: x + s, y2: y + s, color, k: `b${r},${c}` })
    if (!sameRegion(r, c - 1)) out.push({ x1: x, y1: y, x2: x, y2: y + s, color, k: `l${r},${c}` })
    if (!sameRegion(r, c + 1)) out.push({ x1: x + s, y1: y, x2: x + s, y2: y + s, color, k: `r${r},${c}` })
  }
  return out
})

const constraintLabels = computed(() => {
  const out: any[] = []
  for (const e of p.value.edges) {
    if (!e.constraint) continue
    const { x1, y1, x2, y2 } = edgeEndpoints(e)
    const mx = (x1 + x2) / 2, my = (y1 + y2) / 2
    const ct = e.constraint.type
    let kind = '', text = ''
    if (ct === 'heterogeneous') { kind = 'δ'; text = 'δ' }
    else if (ct === 'homogeneous') { kind = '♂'; text = '♂' }
    else if (ct === 'inequality') {
      kind = 'ineq'
      const rev = e.constraint.value === 1
      if (e.c1 === e.c2) text = rev ? (e.r1 < e.r2 ? '^' : 'v') : (e.r1 < e.r2 ? 'v' : '^')
      else text = rev ? (e.c1 < e.c2 ? '>' : '<') : (e.c1 < e.c2 ? '<' : '>')
    } else if (ct === 'difference') { kind = 'val'; text = String(e.constraint.value ?? '') }
    out.push({ mx, my, kind, text, key: edgeKey(e.r1, e.c1, e.r2, e.c2) })
  }
  return out
})

const watchtowers = computed(() => {
  const out: any[] = []
  for (const v of p.value.vertices) if (v.watchtower != null) out.push({ x: vx(v.col), y: vy(v.row), value: v.watchtower })
  return out
})

function diceDots(v: number): [number, number][] {
  const map: Record<number, [number, number][]> = {
    1: [[0, 0]],
    2: [[-0.4, 0.4], [0.4, -0.4]],
    3: [[-0.4, 0.4], [0, 0], [0.4, -0.4]],
    4: [[-0.4, -0.4], [0.4, -0.4], [-0.4, 0.4], [0.4, 0.4]],
  }
  return map[v] ?? []
}

function shapeCells(pattern: [number, number][] | null | undefined): { r: number; c: number }[] {
  if (!pattern || !pattern.length) return []
  const minR = Math.min(...pattern.map(([r]) => r))
  const minC = Math.min(...pattern.map(([, c]) => c))
  return pattern.map(([r, c]) => ({ r: r - minR, c: c - minC }))
}

// Fence: recover the F-value from the stored 3x3 directional pattern
// (up/down/left/right bits), matching PyQt _fence_diamond.
function fenceDiamondValue(cells: [number, number][] | null | undefined): string | null {
  if (!cells || !cells.length) return null
  const has = (r: number, c: number) => cells.some(([rr, cc]) => rr === r && cc === c)
  const up = has(0, 1), down = has(2, 1), left = has(1, 0), right = has(1, 2)
  const count = [up, down, left, right].filter(Boolean).length
  if (count === 0) return 'F0'
  if (count === 1) return 'F1'
  if (count === 2) return (up && down) || (left && right) ? 'F2' : 'F7'
  if (count === 3) return 'F3'
  return 'F4'
}

function fenceSegments(fval: string, cx: number, cy: number, size: number) {
  const [nw, ne, sw, se] = FENCE_EDGES[fval]
  const r = size * 0.35
  const t = { x: cx, y: cy - r }, rp = { x: cx + r, y: cy }, b = { x: cx, y: cy + r }, l = { x: cx - r, y: cy }
  return [
    { present: !!nw, x1: t.x, y1: t.y, x2: l.x, y2: l.y },
    { present: !!ne, x1: t.x, y1: t.y, x2: rp.x, y2: rp.y },
    { present: !!sw, x1: l.x, y1: l.y, x2: b.x, y2: b.y },
    { present: !!se, x1: rp.x, y1: rp.y, x2: b.x, y2: b.y },
  ]
}

/** Compass dial geometry: four arms plus a value slot per direction. */
function compassArms(cx: number, cy: number, size: number) {
  const inner = size * 0.14
  const outer = size * 0.42
  return [
    { x1: cx, y1: cy - inner, x2: cx, y2: cy - outer, tx: cx, ty: cy - outer + size * 0.11 },
    { x1: cx, y1: cy + inner, x2: cx, y2: cy + outer, tx: cx, ty: cy + outer + size * 0.02 },
    { x1: cx - inner, y1: cy, x2: cx - outer, y2: cy, tx: cx - outer + size * 0.06, ty: cy + size * 0.04 },
    { x1: cx + inner, y1: cy, x2: cx + outer, y2: cy, tx: cx + outer - size * 0.06, ty: cy + size * 0.04 },
  ]
}

const P_SYMBOL = /^P[1-9]$/

// ── interaction state ────────────────────────────────────────────────────────
const hoverCell = ref<[number, number] | null>(null)
const hoverEdge = ref<[number, number, number, number] | null>(null)
const hoverVertex = ref<[number, number] | null>(null)
const boundaryDragging = ref(false)
const lastBoundaryVertex = ref<[number, number] | null>(null)
const blockDragging = ref(false)
const blockPaint = ref(true)
const inlineNumber = ref('')
const ctxMenu = ref<{ x: number; y: number; kind: 'cell' | 'edge' | 'vertex'; cell?: [number, number]; edge?: [number, number, number, number]; vertex?: [number, number] } | null>(null)

function toLocal(e: MouseEvent) {
  const rect = svgEl.value!.getBoundingClientRect()
  return { x: e.clientX - rect.left, y: e.clientY - rect.top }
}

function hitCell(x: number, y: number): [number, number] | null {
  const c = Math.floor((x - pad.value) / cs.value)
  const r = Math.floor((y - pad.value) / cs.value)
  if (r >= 0 && r < h.value && c >= 0 && c < w.value) return [r, c]
  return null
}
function hitVertex(x: number, y: number): [number, number] | null {
  const th = Math.max(10, cs.value / 7)
  for (let r = 0; r <= h.value; r++) for (let c = 0; c <= w.value; c++)
    if (Math.abs(x - vx(c)) < th && Math.abs(y - vy(r)) < th) return [r, c]
  return null
}
/**
 * Pointer distance at which an edge counts as "aimed at". One radius drives
 * both the hover highlight and the click target, so what lights up is always
 * what a click would select.
 */
function edgeHitRadius(): number {
  return Math.max(5, Math.min(14, cs.value * 0.22))
}

function hitEdge(x: number, y: number): [number, number, number, number] | null {
  const th = edgeHitRadius()
  let best: [number, number, number, number] | null = null
  let bestD = Infinity
  for (const e of p.value.edges) {
    const { x1, y1, x2, y2 } = edgeEndpoints(e)
    const dx = x2 - x1, dy = y2 - y1
    const len2 = dx * dx + dy * dy
    if (!len2) continue
    let t = ((x - x1) * dx + (y - y1) * dy) / len2
    t = Math.max(0, Math.min(1, t))
    const d = Math.hypot(x - (x1 + t * dx), y - (y1 + t * dy))
    if (d < th && d < bestD) { bestD = d; best = [e.r1, e.c1, e.r2, e.c2] }
  }
  return best
}

function verticesAdjacent(a: [number, number], b: [number, number]) {
  return (Math.abs(a[0] - b[0]) === 1 && a[1] === b[1]) || (Math.abs(a[1] - b[1]) === 1 && a[0] === b[0])
}
function outerKey(v1: [number, number], v2: [number, number]): [number, number, number, number] | null {
  const [r1, c1] = v1, [r2, c2] = v2
  if (Math.abs(r1 - r2) + Math.abs(c1 - c2) !== 1) return null
  if (r1 === r2) { const c = Math.min(c1, c2); if (r1 === 0 || r1 === h.value) return [r1, c, r1, c + 1] }
  if (c1 === c2) { const r = Math.min(r1, r2); if (c1 === 0 || c1 === w.value) return [r, c1, r + 1, c1] }
  return null
}
function vertexPairToEdge(v1: [number, number], v2: [number, number]): [number, number, number, number] | null {
  const [r1, c1] = v1, [r2, c2] = v2
  if (r1 === r2 && Math.abs(c1 - c2) === 1) { const c = Math.min(c1, c2); if (r1 > 0 && r1 < h.value) return [r1 - 1, c, r1, c] }
  if (c1 === c2 && Math.abs(r1 - r2) === 1) { const r = Math.min(r1, r2); if (c1 > 0 && c1 < w.value) return [r, c1 - 1, r, c1] }
  return null
}

function toggleEdgeBoundary(e: [number, number, number, number]) {
  const edge = getEdge(...e)
  if (edge) { edge.is_boundary = !edge.is_boundary; store.markModified() }
}
function toggleOuter(o: [number, number, number, number]) {
  const ob = (p.value.outer_boundaries ??= [])
  const i = ob.findIndex((e) => e.r1 === o[0] && e.c1 === o[1] && e.r2 === o[2] && e.c2 === o[3])
  if (i >= 0) ob.splice(i, 1); else ob.push({ r1: o[0], c1: o[1], r2: o[2], c2: o[3] })
  store.markModified()
}
function drawSegment(v1: [number, number], v2: [number, number]) {
  if (!verticesAdjacent(v1, v2)) return
  const e = vertexPairToEdge(v1, v2)
  if (e) toggleEdgeBoundary(e)
  else { const o = outerKey(v1, v2); if (o) toggleOuter(o) }
}

function clearCellProps(cell: CellJson) {
  cell.number = undefined; cell.symbol = undefined; cell.compass = undefined
  cell.shape_pattern = undefined; cell.fence_pattern = undefined
}
function toggleBlocked(r: number, c: number) {
  const cell = getCell(r, c)
  if (!cell) return
  cell.blocked = !cell.blocked
  if (cell.blocked) clearCellProps(cell)
  store.selectCell(r, c)
  store.markModified()
}
function paintBlocked(r: number, c: number, blocked: boolean) {
  const cell = getCell(r, c)
  if (cell && cell.blocked !== blocked) { cell.blocked = blocked; if (blocked) clearCellProps(cell); store.markModified() }
}

// ── mouse handlers ───────────────────────────────────────────────────────────
function onMouseDown(e: MouseEvent) {
  const { x, y } = toLocal(e)
  const vertex = hitVertex(x, y)
  const edge = hitEdge(x, y)
  const cell = hitCell(x, y)

  if (e.button === 2) {
    if (store.mode === 'block' && cell) { blockDragging.value = true; blockPaint.value = false; paintBlocked(cell[0], cell[1], false); store.selectCell(cell[0], cell[1]); return }
    if (vertex) ctxMenu.value = { x: e.clientX, y: e.clientY, kind: 'vertex', vertex }
    else if (edge) ctxMenu.value = { x: e.clientX, y: e.clientY, kind: 'edge', edge }
    else if (cell) ctxMenu.value = { x: e.clientX, y: e.clientY, kind: 'cell', cell }
    else ctxMenu.value = null
    return
  }
  if (e.button !== 0) return

  if (store.mode === 'boundary') {
    if (vertex) {
      if (edge) { toggleEdgeBoundary(edge); store.selectEdge(edge); return }
      boundaryDragging.value = true
      lastBoundaryVertex.value = vertex
      store.selectVertex(vertex[0], vertex[1])
    } else if (edge) { toggleEdgeBoundary(edge); store.selectEdge(edge) }
    return
  }
  if (store.mode === 'watchtower') {
    if (vertex) {
      const v = vertexAt(p.value, vertex[0], vertex[1])
      if (v) { const val = store.currentNumber; if (val != null && val >= 1 && val <= 4) { v.watchtower = val; store.markModified() } }
      store.selectVertex(vertex[0], vertex[1])
    }
    return
  }
  if (store.mode === 'block') {
    if (cell) { blockDragging.value = true; blockPaint.value = true; paintBlocked(cell[0], cell[1], true); store.selectCell(cell[0], cell[1]) }
    return
  }
  if (store.mode === 'number' && cell) {
    const c = getCell(cell[0], cell[1])
    if (c && store.currentNumber != null) { c.number = store.currentNumber; store.markModified() }
    inlineNumber.value = ''
    store.selectCell(cell[0], cell[1])
    return
  }
  if (store.mode === 'symbol' && cell) {
    const c = getCell(cell[0], cell[1])
    if (c) { c.symbol = store.currentSymbol ?? undefined; store.markModified() }
    store.selectCell(cell[0], cell[1])
    return
  }
  if (store.mode === 'compass' && cell) {
    const c = getCell(cell[0], cell[1])
    if (c) { c.compass = store.currentCompass ?? undefined; store.markModified() }
    store.selectCell(cell[0], cell[1])
    return
  }
  if (store.mode === 'select') {
    if (vertex) store.selectVertex(vertex[0], vertex[1])
    else if (edge) store.selectEdge(edge)
    else if (cell) store.selectCell(cell[0], cell[1])
    else store.clearSelection()
  }
}

function onMouseMove(e: MouseEvent) {
  const { x, y } = toLocal(e)
  const vertex = hitVertex(x, y)
  const edge = hitEdge(x, y)
  const cell = hitCell(x, y)

  if (store.mode === 'block' && blockDragging.value) {
    if (cell) paintBlocked(cell[0], cell[1], blockPaint.value)
    hoverCell.value = cell
    return
  }
  if (store.mode === 'boundary' && boundaryDragging.value && lastBoundaryVertex.value) {
    const v = vertex
    if (v && !(v[0] === lastBoundaryVertex.value[0] && v[1] === lastBoundaryVertex.value[1]) && verticesAdjacent(lastBoundaryVertex.value, v)) {
      drawSegment(lastBoundaryVertex.value, v)
      lastBoundaryVertex.value = v
      store.selectVertex(v[0], v[1])
    }
    hoverVertex.value = vertex
    return
  }
  if (store.mode === 'watchtower' || (store.mode === 'boundary' && !boundaryDragging.value)) {
    hoverVertex.value = vertex
    hoverEdge.value = edge
    hoverCell.value = null
    return
  }
  hoverVertex.value = vertex
  hoverEdge.value = edge
  hoverCell.value = cell
}

function onMouseUp() {
  blockDragging.value = false
  boundaryDragging.value = false
  lastBoundaryVertex.value = null
}

function onWheel(e: WheelEvent) {
  zoomBy(e.deltaY > 0 ? -6 : 6)
}

// ── keyboard ─────────────────────────────────────────────────────────────────
function onKey(e: KeyboardEvent) {
  const k = e.key
  if (k === 'Escape') { store.clearSelection(); inlineNumber.value = ''; ctxMenu.value = null; return }
  if (k === 'Delete' || k === 'Backspace') {
    if (store.selectedCell) {
      const c = getCell(store.selectedCell[0], store.selectedCell[1])
      if (c && !c.blocked) { clearCellProps(c); store.markModified() }
    }
    return
  }
  if (k === 'ArrowUp' || k === 'ArrowDown' || k === 'ArrowLeft' || k === 'ArrowRight') { moveSelection(k); return }
  if (store.mode !== 'number') {
    if (k === '+' || k === '=') { zoomBy(6); return }
    if (k === '-' || k === '_') { zoomBy(-6); return }
    if (k === '0') { fit(); return }
    if (k.toLowerCase() === 'f') { fit(); return }
  }
  const modeKeys: Record<string, string> = { v: 'select', b: 'boundary', x: 'block', n: 'number', s: 'symbol', c: 'compass', w: 'watchtower' }
  if (modeKeys[k.toLowerCase()]) { store.mode = modeKeys[k.toLowerCase()]; return }
  if (store.mode === 'number' && store.selectedCell && /^[0-9]$/.test(k)) {
    inlineNumber.value += k
    const c = getCell(store.selectedCell[0], store.selectedCell[1])
    if (c) { c.number = parseInt(inlineNumber.value); store.markModified() }
  }
}

function moveSelection(k: string) {
  const dr = k === 'ArrowUp' ? -1 : k === 'ArrowDown' ? 1 : 0
  const dc = k === 'ArrowLeft' ? -1 : k === 'ArrowRight' ? 1 : 0
  if (store.selectedCell) {
    const [r, c] = store.selectedCell
    store.selectCell(Math.max(0, Math.min(h.value - 1, r + dr)), Math.max(0, Math.min(w.value - 1, c + dc)))
  } else if (store.selectedVertex) {
    const [r, c] = store.selectedVertex
    store.selectVertex(Math.max(0, Math.min(h.value, r + dr)), Math.max(0, Math.min(w.value, c + dc)))
  }
}

// ── context menu actions ─────────────────────────────────────────────────────
function ctxToggleBlocked() { if (ctxMenu.value?.cell) toggleBlocked(ctxMenu.value.cell[0], ctxMenu.value.cell[1]); ctxMenu.value = null }
function ctxClearNumber() { if (ctxMenu.value?.cell) { const c = getCell(...ctxMenu.value.cell); if (c) { c.number = undefined; store.markModified() } } ctxMenu.value = null }
function ctxClearSymbol() { if (ctxMenu.value?.cell) { const c = getCell(...ctxMenu.value.cell); if (c) { c.symbol = undefined; store.markModified() } } ctxMenu.value = null }
function ctxClearCellAll() { if (ctxMenu.value?.cell) { const c = getCell(...ctxMenu.value.cell); if (c && !c.blocked) { clearCellProps(c); store.markModified() } } ctxMenu.value = null }
function ctxToggleCellBoundary() {
  if (ctxMenu.value?.cell) {
    const [r, c] = ctxMenu.value.cell
    for (const e of p.value.edges) if ((e.r1 === r && e.c1 === c) || (e.r2 === r && e.c2 === c)) e.is_boundary = !e.is_boundary
    store.markModified()
  }
  ctxMenu.value = null
}
function ctxToggleEdge() { if (ctxMenu.value?.edge) toggleEdgeBoundary(ctxMenu.value.edge); ctxMenu.value = null }
function ctxSetConstraint(type: string, value?: number) {
  if (ctxMenu.value?.edge) { const e = getEdge(...ctxMenu.value.edge); if (e) { e.constraint = makeConstraint(type as any, value); store.markModified() } }
  ctxMenu.value = null
}
function ctxClearConstraint() {
  if (ctxMenu.value?.edge) { const e = getEdge(...ctxMenu.value.edge); if (e) { e.constraint = undefined; store.markModified() } }
  ctxMenu.value = null
}
function ctxClearWatchtower() {
  if (ctxMenu.value?.vertex) { const v = vertexAt(p.value, ...ctxMenu.value.vertex); if (v) { v.watchtower = undefined; store.markModified() } }
  ctxMenu.value = null
}

// expose for template
const selEdge = computed(() => store.selectedEdge ? edgeEndpoints({ r1: store.selectedEdge[0], c1: store.selectedEdge[1], r2: store.selectedEdge[2], c2: store.selectedEdge[3] }) : null)
const hoverCellRect = computed(() => {
  if (!hoverCell.value) return null
  const [r, c] = hoverCell.value
  if (store.selectedCell && store.selectedCell[0] === r && store.selectedCell[1] === c) return null
  return { x: cellX(c) + 1.5, y: cellY(r) + 1.5, w: cs.value - 3, h: cs.value - 3 }
})
const hoverVertexDot = computed(() => {
  if (!hoverVertex.value) return null
  const [r, c] = hoverVertex.value
  if (store.selectedVertex && store.selectedVertex[0] === r && store.selectedVertex[1] === c) return null
  return { cx: vx(c), cy: vy(r) }
})
const hoverEdgeLine = computed(() => {
  if (!hoverEdge.value) return null
  // Only where a click would act on the edge itself; in the painting tools the
  // cell is the target even right up against its border.
  if (store.mode !== 'select' && store.mode !== 'boundary') return null
  const [r1, c1, r2, c2] = hoverEdge.value
  if (store.selectedEdge && store.selectedEdge.join() === hoverEdge.value.join()) return null
  return edgeEndpoints({ r1, c1, r2, c2 })
})
const guides = computed(() => {
  if (!hoverCell.value || hoverEdgeLine.value || cs.value < 22) return null
  const [r, c] = hoverCell.value
  return {
    x: cellX(c) + cs.value / 2,
    y: cellY(r) + cs.value / 2,
    x1: pad.value,
    x2: pad.value + w.value * cs.value,
    y1: pad.value,
    y2: pad.value + h.value * cs.value,
  }
})

// ── solution reveal ──────────────────────────────────────────────────────────
// Bumping this key remounts the cell layer, replaying the staggered reveal.
const revealKey = ref(0)
watch(
  () => store.showSolution,
  (v) => { if (v) revealKey.value++ },
)

// ── auto-fit ─────────────────────────────────────────────────────────────────
let observer: ResizeObserver | null = null
onMounted(() => {
  publishGrid(h.value, w.value)
  const el = svgEl.value?.closest('.stage') as HTMLElement | null
  if (el) {
    publishStage(el.clientWidth, el.clientHeight)
    observer = new ResizeObserver(() => publishStage(el.clientWidth, el.clientHeight))
    observer.observe(el)
  }
  fit()
})
onUnmounted(() => observer?.disconnect())

// Re-fit whenever the grid dimensions change (new puzzle / loaded puzzle).
watch([h, w], ([nh, nw]) => {
  publishGrid(nh, nw)
  fit()
})

function onDocMouseDown() {
  if (ctxMenu.value) ctxMenu.value = null
}
onMounted(() => window.addEventListener('mousedown', onDocMouseDown))
onUnmounted(() => window.removeEventListener('mousedown', onDocMouseDown))
</script>

<template>
  <div class="board-wrap" tabindex="0" @keydown="onKey">
    <div class="board-sheet">
      <svg
        ref="svgEl"
        :width="W" :height="H"
        class="board-svg"
        :data-mode="store.mode"
        @mousedown="onMouseDown"
        @mousemove="onMouseMove"
        @mouseup="onMouseUp"
        @mouseleave="onMouseUp"
        @wheel.prevent="onWheel"
        @contextmenu.prevent
      >
        <defs>
          <filter id="aogGlow" x="-60%" y="-60%" width="220%" height="220%">
            <feGaussianBlur stdDeviation="2.6" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          <pattern id="aogBlocked" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <line x1="0" y1="0" x2="0" y2="7" :stroke="C.cell_blocked_x" stroke-width="2.4" />
          </pattern>
          <linearGradient id="aogChip" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stop-color="#ffffff" stop-opacity="0.18" />
            <stop offset="100%" stop-color="#000000" stop-opacity="0.12" />
          </linearGradient>
        </defs>

        <!-- sheet margin / inner frame -->
        <rect
          :x="pad - 4" :y="pad - 4" :width="w * cs + 8" :height="h * cs + 8"
          :rx="8" fill="none" :stroke="C.grid_line" stroke-width="1"
        />

        <!-- hover crosshair guides -->
        <g v-if="guides" class="guide">
          <line :x1="guides.x1" :y1="guides.y" :x2="guides.x2" :y2="guides.y" :stroke="C.hover_cell" stroke-width="1" stroke-opacity="0.28" />
          <line :x1="guides.x" :y1="guides.y1" :x2="guides.x" :y2="guides.y2" :stroke="C.hover_cell" stroke-width="1" stroke-opacity="0.28" />
        </g>

        <!-- cells -->
        <g :key="revealKey">
          <g v-for="c in cells" :key="`${c.r}-${c.c}`">
            <rect
              :x="c.x" :y="c.y" :width="cs" :height="cs"
              :fill="c.blocked ? C.cell_blocked_bg : c.fill"
              :class="{ 'cell-region': c.region != null && store.showSolution }"
              :style="{ animationDelay: c.delay + 'ms' }"
            />
            <rect
              v-if="c.blocked"
              :x="c.x" :y="c.y" :width="cs" :height="cs"
              fill="url(#aogBlocked)" opacity="0.4"
            />
            <g v-if="c.blocked">
              <line :x1="c.x + cs * 0.24" :y1="c.y + cs * 0.24" :x2="c.x + cs * 0.76" :y2="c.y + cs * 0.76" :stroke="C.cell_blocked_x" :stroke-width="Math.max(1.4, cs * 0.045)" stroke-linecap="round" />
              <line :x1="c.x + cs * 0.76" :y1="c.y + cs * 0.24" :x2="c.x + cs * 0.24" :y2="c.y + cs * 0.76" :stroke="C.cell_blocked_x" :stroke-width="Math.max(1.4, cs * 0.045)" stroke-linecap="round" />
            </g>
            <g v-else>
              <g v-if="c.symbol && P_SYMBOL.test(c.symbol)">
                <circle :cx="c.x + cs / 2" :cy="c.y + cs / 2" :r="cs * 0.24" :fill="P_COLORS[(parseInt(c.symbol[1]) - 1) % P_COLORS.length]" />
                <circle :cx="c.x + cs / 2" :cy="c.y + cs / 2" :r="cs * 0.24" fill="url(#aogChip)" />
                <circle :cx="c.x + cs / 2" :cy="c.y + cs / 2" :r="cs * 0.24" fill="none" stroke="rgba(0,0,0,.45)" stroke-width="1.2" />
              </g>
              <text v-else-if="c.symbol" :x="c.x + cs / 2" :y="c.y + cs * 0.64" text-anchor="middle" :font-size="cs / 2" font-weight="700" :fill="C.symbol_text">{{ c.symbol }}</text>

              <text v-if="c.number != null && !c.symbol" :x="c.x + cs / 2" :y="c.y + cs * 0.64" text-anchor="middle" :font-size="cs / 2" font-weight="700" :fill="C.number_text">{{ c.number }}</text>
              <text v-else-if="c.number != null" :x="c.x + cs * 0.1" :y="c.y + cs * 0.32" :font-size="cs / 3.2" font-weight="700" :fill="C.number_text">{{ c.number }}</text>

              <g v-if="c.compass">
                <template v-for="(a, i) in compassArms(c.x + cs / 2, c.y + cs / 2, cs)" :key="i">
                  <line :x1="a.x1" :y1="a.y1" :x2="a.x2" :y2="a.y2" :stroke="C.compass_line" :stroke-width="Math.max(1, cs * 0.035)" stroke-linecap="round" />
                </template>
                <circle :cx="c.x + cs / 2" :cy="c.y + cs / 2" :r="cs * 0.055" :fill="C.compass_text" fill-opacity="0.25" />
                <text v-if="c.compass.up >= 0" :x="c.x + cs / 2" :y="c.y + cs * 0.28" text-anchor="middle" :font-size="cs / 6.6" font-weight="600" :fill="C.compass_text">{{ c.compass.up }}</text>
                <text v-if="c.compass.down >= 0" :x="c.x + cs / 2" :y="c.y + cs * 0.82" text-anchor="middle" :font-size="cs / 6.6" font-weight="600" :fill="C.compass_text">{{ c.compass.down }}</text>
                <text v-if="c.compass.left >= 0" :x="c.x + cs * 0.2" :y="c.y + cs * 0.56" text-anchor="middle" :font-size="cs / 6.6" font-weight="600" :fill="C.compass_text">{{ c.compass.left }}</text>
                <text v-if="c.compass.right >= 0" :x="c.x + cs * 0.8" :y="c.y + cs * 0.56" text-anchor="middle" :font-size="cs / 6.6" font-weight="600" :fill="C.compass_text">{{ c.compass.right }}</text>
              </g>

              <g v-if="c.shapePattern">
                <rect v-for="(s, i) in shapeCells(c.shapePattern)" :key="i" :x="c.x + s.c * cs * 0.2 + cs * 0.2" :y="c.y + s.r * cs * 0.2 + cs * 0.2" :width="Math.max(1, cs * 0.2 - 1)" :height="Math.max(1, cs * 0.2 - 1)" :rx="Math.min(2, cs * 0.04)" :fill="C.shape_mini_fill" :stroke="C.shape_mini_pen" stroke-width="0.8" />
              </g>

              <g v-if="c.fence">
                <line v-for="(s, i) in c.fence" :key="i" :x1="s.x1" :y1="s.y1" :x2="s.x2" :y2="s.y2" :stroke="s.present ? C.number_text : C.cell_border" :stroke-width="s.present ? 2 : 1.2" :stroke-dasharray="s.present ? undefined : '3,3'" stroke-linecap="round" />
                <circle :cx="c.x + cs / 2" :cy="c.y + cs / 2" :r="Math.max(1.2, cs * 0.09)" :fill="C.number_text" />
              </g>
            </g>
          </g>
        </g>

        <!-- region silhouette (open on sides shared with the same region) -->
        <line
          v-for="o in regionOutlines" :key="o.k"
          :x1="o.x1" :y1="o.y1" :x2="o.x2" :y2="o.y2"
          :stroke="o.color" :stroke-width="Math.max(1.6, cs * 0.035)" stroke-linecap="round"
        />

        <!-- region boundaries: glow → body → highlight core -->
        <g filter="url(#aogGlow)">
          <line v-for="(l, i) in boundaryLines" :key="`b${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="C.boundary_edge" :stroke-width="Math.max(3, cs * 0.1)" stroke-linecap="round" stroke-opacity="0.55" />
        </g>
        <line v-for="(l, i) in boundaryLines" :key="`bm${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="C.boundary_edge" :stroke-width="Math.max(2, cs * 0.075)" stroke-linecap="round" />
        <line v-for="(l, i) in boundaryLines" :key="`bh${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="C.boundary_highlight" :stroke-width="Math.max(1, cs * 0.028)" stroke-linecap="round" stroke-opacity="0.85" />

        <!-- hairline grid -->
        <line v-for="(l, i) in gridLines" :key="`g${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="C.grid_line" stroke-width="1" />

        <!-- edge constraints -->
        <g v-for="cl in constraintLabels" :key="cl.key" class="constr-chip">
          <rect :x="cl.mx - cs * 0.16" :y="cl.my - cs * 0.16" :width="cs * 0.32" :height="cs * 0.32" :rx="Math.min(4, cs * 0.09)" :fill="cl.kind === 'δ' ? C.number_text : C.edge_constr_bg" :stroke="cl.kind === 'δ' ? C.number_text : C.edge_constr_border" stroke-width="1.2" />
          <text :x="cl.mx" :y="cl.my + cs * 0.09" text-anchor="middle" :font-size="cs / 4.6" font-weight="700" :fill="cl.kind === 'δ' ? C.grid_bg : C.edge_constr_text">{{ cl.text }}</text>
        </g>

        <!-- watchtowers -->
        <g v-for="(wt, i) in watchtowers" :key="`w${i}`" class="wt-chip">
          <circle :cx="wt.x" :cy="wt.y" :r="cs / 4.6" :fill="C.watchtower_bg" :stroke="C.watchtower_border" stroke-width="Math.max(1.4, cs * 0.035)" />
          <circle v-for="(d, j) in diceDots(wt.value)" :key="j" :cx="wt.x + d[0] * cs / 4.6" :cy="wt.y + d[1] * cs / 4.6" :r="Math.max(0.8, cs / 20)" :fill="C.watchtower_text" />
          <text v-if="wt.value > 4" :x="wt.x" :y="wt.y + cs / 9" text-anchor="middle" :font-size="cs / 4.4" font-weight="700" :fill="C.watchtower_text">{{ wt.value }}</text>
        </g>

        <!-- hover affordances: the border zone lights up the border, not the cell -->
        <line v-if="hoverEdgeLine" class="edge-hover" :x1="hoverEdgeLine.x1" :y1="hoverEdgeLine.y1" :x2="hoverEdgeLine.x2" :y2="hoverEdgeLine.y2" :stroke="C.hover_cell" :stroke-width="Math.max(3, cs * 0.085)" stroke-linecap="round" stroke-opacity="0.9" />
        <rect v-if="hoverCellRect && !hoverEdgeLine" :x="hoverCellRect.x" :y="hoverCellRect.y" :width="hoverCellRect.w" :height="hoverCellRect.h" :rx="Math.min(4, cs * 0.1)" fill="none" :stroke="C.hover_cell" stroke-width="1.6" stroke-opacity="0.9" />
        <circle v-if="hoverVertexDot" :cx="hoverVertexDot.cx" :cy="hoverVertexDot.cy" :r="Math.max(2, cs / 9)" fill="none" :stroke="C.hover_vertex" stroke-width="1.6" />

        <!-- selection -->
        <rect
          v-if="store.selectedCell"
          :x="cellX(store.selectedCell[1]) + 1" :y="cellY(store.selectedCell[0]) + 1"
          :width="Math.max(0, cs - 2)" :height="Math.max(0, cs - 2)"
          :rx="Math.min(4, cs * 0.1)" fill="none" :stroke="C.selection_border" stroke-width="2.4" class="sel-ring"
          stroke-dasharray="7 5"
        />
        <line v-if="selEdge" :x1="selEdge.x1" :y1="selEdge.y1" :x2="selEdge.x2" :y2="selEdge.y2" :stroke="C.selection_border" :stroke-width="Math.max(3, cs * 0.09)" stroke-linecap="round" />
        <circle v-if="store.selectedVertex" :cx="vx(store.selectedVertex[1])" :cy="vy(store.selectedVertex[0])" :r="Math.max(3, cs / 6)" :fill="C.selection_vertex_fill" :stroke="C.selection_border" stroke-width="2.4" class="sel-vertex" />
      </svg>
    </div>

    <!-- context menu -->
    <div v-if="ctxMenu" class="ctx-menu" :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }" @mousedown.stop @contextmenu.prevent>
      <template v-if="ctxMenu.kind === 'cell'">
        <div class="ctx-menu__head">单元格 ({{ ctxMenu.cell![0] }}, {{ ctxMenu.cell![1] }})</div>
        <button class="ctx-menu__item" @click="ctxToggleBlocked">
          <AppIcon :name="getCell(ctxMenu.cell![0], ctxMenu.cell![1])?.blocked ? 'eye' : 'block'" :size="14" />
          {{ getCell(ctxMenu.cell![0], ctxMenu.cell![1])?.blocked ? '取消障碍' : '设为障碍格' }}
        </button>
        <div class="ctx-menu__sep" />
        <button class="ctx-menu__item" @click="ctxClearNumber"><AppIcon name="hash" :size="14" />清除数字</button>
        <button class="ctx-menu__item" @click="ctxClearSymbol"><AppIcon name="star" :size="14" />清除符号</button>
        <button class="ctx-menu__item is-danger" @click="ctxClearCellAll"><AppIcon name="trash" :size="14" />清除全部</button>
        <div class="ctx-menu__sep" />
        <button class="ctx-menu__item" @click="ctxToggleCellBoundary"><AppIcon name="pencil" :size="14" />切换此格边框</button>
      </template>
      <template v-else-if="ctxMenu.kind === 'edge'">
        <div class="ctx-menu__head">边框约束</div>
        <button class="ctx-menu__item" @click="ctxToggleEdge"><AppIcon name="pencil" :size="14" />切换分割线</button>
        <div class="ctx-menu__sep" />
        <button class="ctx-menu__item" @click="ctxSetConstraint('heterogeneous')"><AppIcon name="block" :size="14" />设异生 (≠)</button>
        <button class="ctx-menu__item" @click="ctxSetConstraint('homogeneous')"><AppIcon name="check" :size="14" />设双生 (=)</button>
        <button class="ctx-menu__item" @click="ctxSetConstraint('inequality')"><AppIcon name="chevronRight" :size="14" />设不等号 (箭头)</button>
        <button class="ctx-menu__item" @click="ctxSetConstraint('difference', 1)"><AppIcon name="hash" :size="14" />设差值</button>
        <template v-if="getEdge(...ctxMenu.edge!)?.constraint">
          <div class="ctx-menu__sep" />
          <button class="ctx-menu__item is-danger" @click="ctxClearConstraint"><AppIcon name="trash" :size="14" />清除约束</button>
        </template>
      </template>
      <template v-else>
        <div class="ctx-menu__head">顶点 ({{ ctxMenu.vertex![0] }}, {{ ctxMenu.vertex![1] }})</div>
        <button class="ctx-menu__item" @click="ctxClearWatchtower"><AppIcon name="tower" :size="14" />清除望塔值</button>
      </template>
    </div>
  </div>
</template>
