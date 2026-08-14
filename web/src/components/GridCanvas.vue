<script setup lang="ts">
import { computed, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { colors, P_COLORS, REGION_COLORS, RULE_NAMES } from '../lib/theme'
import { cellKey, edgeKey, edgeBetween, cellAt, vertexAt, makeConstraint } from '../lib/model'
import type { CellJson, EdgeJson, PuzzleJson } from '../lib/types'

const store = usePuzzleStore()
const svgEl = ref<SVGSVGElement | null>(null)
const cellSize = ref(60)
const padding = 24

const p = computed(() => store.puzzle as PuzzleJson)
const h = computed(() => p.value.grid.height)
const w = computed(() => p.value.grid.width)
const W = computed(() => padding * 2 + w.value * cellSize.value)
const H = computed(() => padding * 2 + h.value * cellSize.value)

// ── coordinates ──────────────────────────────────────────────────────────────
const cellX = (c: number) => padding + c * cellSize.value
const cellY = (r: number) => padding + r * cellSize.value
const vx = (vc: number) => padding + vc * cellSize.value
const vy = (vr: number) => padding + vr * cellSize.value

function edgeEndpoints(e: { r1: number; c1: number; r2: number; c2: number }) {
  if (e.r1 === e.r2) {
    const x = padding + (Math.min(e.c1, e.c2) + 1) * cellSize.value
    return { x1: x, y1: padding + e.r1 * cellSize.value, x2: x, y2: padding + (e.r1 + 1) * cellSize.value }
  }
  const y = padding + (Math.min(e.r1, e.r2) + 1) * cellSize.value
  return { x1: padding + e.c1 * cellSize.value, y1: y, x2: padding + (e.c1 + 1) * cellSize.value, y2: y }
}

// ── render models ────────────────────────────────────────────────────────────
const cells = computed(() => {
  const out: any[] = []
  for (let r = 0; r < h.value; r++) for (let c = 0; c < w.value; c++) {
    const cell = cellAt(p.value, r, c)
    const blocked = !!cell?.blocked
    const ri = store.displayRegions?.get(cellKey(r, c))
    out.push({
      r, c, x: cellX(c), y: cellY(r), blocked,
      fill: blocked ? colors.cell_blocked_bg : (ri != null ? REGION_COLORS[ri % REGION_COLORS.length] : colors.cell_bg_null),
      number: cell?.number, symbol: cell?.symbol, compass: cell?.compass,
      shapePattern: cell?.shape_pattern, fencePattern: cell?.fence_pattern,
    })
  }
  return out
})

function isAutoBoundary(e: EdgeJson): boolean {
  const c1 = cellAt(p.value, e.r1, e.c1)
  const c2 = cellAt(p.value, e.r2, e.c2)
  return !!(c1 && c2 && c1.blocked !== c2.blocked)
}

const boundaryLines = computed(() => {
  const lines: any[] = []
  for (const e of p.value.edges) {
    if (e.is_boundary || isAutoBoundary(e)) lines.push({ ...edgeEndpoints(e), key: edgeKey(e.r1, e.c1, e.r2, e.c2) })
  }
  for (const o of p.value.outer_boundaries ?? []) {
    lines.push({ x1: padding + o.c1 * cellSize.value, y1: padding + o.r1 * cellSize.value, x2: padding + o.c2 * cellSize.value, y2: padding + o.r2 * cellSize.value, key: 'o' + edgeKey(o.r1, o.c1, o.r2, o.c2) })
  }
  return lines
})

const gridLines = computed(() => {
  const lines: any[] = []
  for (const e of p.value.edges) {
    const c1 = cellAt(p.value, e.r1, e.c1)
    const c2 = cellAt(p.value, e.r2, e.c2)
    if (c1 && c2 && !c1.blocked && !c2.blocked) {
      const r1 = store.displayRegions?.get(cellKey(e.r1, e.c1))
      const r2 = store.displayRegions?.get(cellKey(e.r2, e.c2))
      if (r1 != null && r1 === r2) continue
    }
    lines.push(edgeEndpoints(e))
  }
  return lines
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

// ── interaction state ────────────────────────────────────────────────────────
const hoverCell = ref<[number, number] | null>(null)
const hoverEdge = ref<[number, number, number, number] | null>(null)
const hoverVertex = ref<[number, number] | null>(null)
const boundaryStart = ref<[number, number] | null>(null)
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
  const c = Math.floor((x - padding) / cellSize.value)
  const r = Math.floor((y - padding) / cellSize.value)
  if (r >= 0 && r < h.value && c >= 0 && c < w.value) return [r, c]
  return null
}
function hitVertex(x: number, y: number): [number, number] | null {
  const th = Math.max(10, cellSize.value / 7)
  for (let r = 0; r <= h.value; r++) for (let c = 0; c <= w.value; c++)
    if (Math.abs(x - vx(c)) < th && Math.abs(y - vy(r)) < th) return [r, c]
  return null
}
function hitEdge(x: number, y: number): [number, number, number, number] | null {
  const th = Math.max(10, cellSize.value / 7)
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
  const edge = edgeBetween(p.value, ...e)
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
  const cell = cellAt(p.value, r, c)
  if (!cell) return
  cell.blocked = !cell.blocked
  if (cell.blocked) clearCellProps(cell)
  store.selectCell(r, c)
  store.markModified()
}
function paintBlocked(r: number, c: number, blocked: boolean) {
  const cell = cellAt(p.value, r, c)
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
      boundaryStart.value = null
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
    const c = cellAt(p.value, cell[0], cell[1])
    if (c && store.currentNumber != null) { c.number = store.currentNumber; store.markModified() }
    inlineNumber.value = ''
    store.selectCell(cell[0], cell[1])
    return
  }
  if (store.mode === 'symbol' && cell) {
    const c = cellAt(p.value, cell[0], cell[1])
    if (c) { c.symbol = store.currentSymbol ?? undefined; store.markModified() }
    store.selectCell(cell[0], cell[1])
    return
  }
  if (store.mode === 'compass' && cell) {
    const c = cellAt(p.value, cell[0], cell[1])
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
  cellSize.value = Math.max(15, Math.min(120, cellSize.value + (e.deltaY > 0 ? -5 : 5)))
}

// ── keyboard ─────────────────────────────────────────────────────────────────
function onKey(e: KeyboardEvent) {
  const k = e.key
  if (k === 'Escape') { store.clearSelection(); inlineNumber.value = ''; return }
  if (k === 'Delete' || k === 'Backspace') {
    if (store.selectedCell) {
      const c = cellAt(p.value, store.selectedCell[0], store.selectedCell[1])
      if (c && !c.blocked) { clearCellProps(c); store.markModified() }
    }
    return
  }
  if (k === 'ArrowUp' || k === 'ArrowDown' || k === 'ArrowLeft' || k === 'ArrowRight') { moveSelection(k); return }
  const modeKeys: Record<string, string> = { v: 'select', b: 'boundary', x: 'block', n: 'number', s: 'symbol', c: 'compass', w: 'watchtower' }
  if (modeKeys[k.toLowerCase()]) { store.mode = modeKeys[k.toLowerCase()]; return }
  if (store.mode === 'number' && store.selectedCell && /^[0-9]$/.test(k)) {
    inlineNumber.value += k
    const c = cellAt(p.value, store.selectedCell[0], store.selectedCell[1])
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
function ctxClearNumber() { if (ctxMenu.value?.cell) { const c = cellAt(p.value, ...ctxMenu.value.cell); if (c) { c.number = undefined; store.markModified() } } ctxMenu.value = null }
function ctxClearSymbol() { if (ctxMenu.value?.cell) { const c = cellAt(p.value, ...ctxMenu.value.cell); if (c) { c.symbol = undefined; store.markModified() } } ctxMenu.value = null }
function ctxClearCellAll() { if (ctxMenu.value?.cell) { const c = cellAt(p.value, ...ctxMenu.value.cell); if (c && !c.blocked) { clearCellProps(c); store.markModified() } } ctxMenu.value = null }
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
  if (ctxMenu.value?.edge) { const e = edgeBetween(p.value, ...ctxMenu.value.edge); if (e) { e.constraint = makeConstraint(type as any, value); store.markModified() } }
  ctxMenu.value = null
}
function ctxClearConstraint() {
  if (ctxMenu.value?.edge) { const e = edgeBetween(p.value, ...ctxMenu.value.edge); if (e) { e.constraint = undefined; store.markModified() } }
  ctxMenu.value = null
}
function ctxClearWatchtower() {
  if (ctxMenu.value?.vertex) { const v = vertexAt(p.value, ...ctxMenu.value.vertex); if (v) { v.watchtower = undefined; store.markModified() } }
  ctxMenu.value = null
}

// ── overlay (rules + shape pool) ─────────────────────────────────────────────
const overlay = computed(() => {
  const rules = [...new Set(p.value.rules.map((r) => ruleLabel(r)))]
  const poolRule = p.value.rules.find((r) => r.type === 'shape_pool')
  const shapes: any[] = (poolRule?.params?.shapes as [number, number][][]) ?? []
  return { rules, shapes }
})
function ruleLabel(r: { type: string; params?: Record<string, any> }): string {
  const name = RULE_NAMES[r.type] ?? r.type
  const pr = r.params ?? {}
  if (r.type === 'range') { const lo = pr.min, hi = pr.max; if (lo != null && hi != null) return `${name} ${lo}~${hi}`; if (lo != null) return `${name} ≥${lo}`; if (hi != null) return `${name} ≤${hi}` }
  if (r.type === 'precise' && pr.area != null) return `${name} ${pr.area}`
  if (r.type === 'rose_window' && pr.symbol_types) return `${name} ${pr.symbol_types.length}种`
  return name
}

// expose for template
const selEdge = computed(() => store.selectedEdge ? edgeEndpoints({ r1: store.selectedEdge[0], c1: store.selectedEdge[1], r2: store.selectedEdge[2], c2: store.selectedEdge[3] }) : null)
</script>

<template>
  <div class="grid-wrap" tabindex="0" @keydown="onKey">
    <svg
      ref="svgEl"
      :width="W" :height="H"
      class="grid"
      @mousedown="onMouseDown"
      @mousemove="onMouseMove"
      @mouseup="onMouseUp"
      @mouseleave="onMouseUp"
      @wheel.prevent="onWheel"
      @contextmenu.prevent
    >
      <g v-for="c in cells" :key="`${c.r}-${c.c}`">
        <rect :x="c.x" :y="c.y" :width="cellSize" :height="cellSize" :fill="c.fill" :stroke="colors.cell_border" stroke-width="1" />
        <g v-if="c.blocked">
          <line :x1="c.x" :y1="c.y" :x2="c.x + cellSize" :y2="c.y + cellSize" :stroke="colors.cell_blocked_x" stroke-width="2" />
          <line :x1="c.x + cellSize" :y1="c.y" :x2="c.x" :y2="c.y + cellSize" :stroke="colors.cell_blocked_x" stroke-width="2" />
        </g>
        <g v-else>
          <circle v-if="c.symbol && /^P[1-9]$/.test(c.symbol)" :cx="c.x + cellSize / 2" :cy="c.y + cellSize / 2" :r="cellSize * 0.24" :fill="P_COLORS[(parseInt(c.symbol[1]) - 1) % P_COLORS.length]" stroke="#000" stroke-width="1.5" />
          <text v-else-if="c.symbol" :x="c.x + cellSize / 2" :y="c.y + cellSize * 0.62" text-anchor="middle" :font-size="cellSize / 2" font-weight="bold" :fill="colors.symbol_text">{{ c.symbol }}</text>

          <text v-if="c.number != null && !c.symbol" :x="c.x + cellSize / 2" :y="c.y + cellSize * 0.62" text-anchor="middle" :font-size="cellSize / 2" font-weight="bold" :fill="colors.number_text">{{ c.number }}</text>
          <text v-else-if="c.number != null" :x="c.x + 4" :y="c.y + cellSize * 0.28" :font-size="cellSize / 3" font-weight="bold" :fill="colors.number_text">{{ c.number }}</text>

          <g v-if="c.compass">
            <line :x1="c.x + cellSize/2" :y1="c.y + cellSize/2" :x2="c.x + cellSize/2" :y2="c.y + cellSize*0.2 + 8" :stroke="colors.compass_line" />
            <line :x1="c.x + cellSize/2" :y1="c.y + cellSize/2" :x2="c.x + cellSize/2" :y2="c.y + cellSize*0.8 - 8" :stroke="colors.compass_line" />
            <line :x1="c.x + cellSize/2" :y1="c.y + cellSize/2" :x2="c.x + cellSize*0.2 + 8" :y2="c.y + cellSize/2" :stroke="colors.compass_line" />
            <line :x1="c.x + cellSize/2" :y1="c.y + cellSize/2" :x2="c.x + cellSize*0.8 - 8" :y2="c.y + cellSize/2" :stroke="colors.compass_line" />
            <text v-if="c.compass.up >= 0" :x="c.x + cellSize/2" :y="c.y + cellSize*0.2" text-anchor="middle" :font-size="cellSize/7" :fill="colors.compass_text">{{ c.compass.up }}</text>
            <text v-if="c.compass.down >= 0" :x="c.x + cellSize/2" :y="c.y + cellSize*0.82" text-anchor="middle" :font-size="cellSize/7" :fill="colors.compass_text">{{ c.compass.down }}</text>
            <text v-if="c.compass.left >= 0" :x="c.x + cellSize*0.16" :y="c.y + cellSize*0.55" text-anchor="middle" :font-size="cellSize/7" :fill="colors.compass_text">{{ c.compass.left }}</text>
            <text v-if="c.compass.right >= 0" :x="c.x + cellSize*0.84" :y="c.y + cellSize*0.55" text-anchor="middle" :font-size="cellSize/7" :fill="colors.compass_text">{{ c.compass.right }}</text>
          </g>
          <g v-if="c.shapePattern">
            <rect v-for="(s, i) in shapeCells(c.shapePattern)" :key="i" :x="c.x + s.c * cellSize * 0.2 + cellSize*0.2" :y="c.y + s.r * cellSize * 0.2 + cellSize*0.2" :width="cellSize*0.2 - 1" :height="cellSize*0.2 - 1" rx="1" :fill="colors.shape_mini_fill" :stroke="colors.shape_mini_pen" />
          </g>
        </g>
      </g>

      <line v-for="(l, i) in boundaryLines" :key="`b${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="colors.boundary_edge" stroke-width="6" stroke-linecap="round" />
      <line v-for="(l, i) in boundaryLines" :key="`bh${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="colors.boundary_highlight" stroke-width="2.5" stroke-linecap="round" />

      <line v-for="(l, i) in gridLines" :key="`g${i}`" :x1="l.x1" :y1="l.y1" :x2="l.x2" :y2="l.y2" :stroke="colors.grid_line" stroke-width="1" />
      <rect :x="padding" :y="padding" :width="w * cellSize" :height="h * cellSize" fill="none" :stroke="colors.grid_line" stroke-width="1" />

      <g v-for="cl in constraintLabels" :key="cl.key">
        <rect :x="cl.mx - 8" :y="cl.my - 8" :width="16" :height="16" rx="3" :fill="cl.kind === 'δ' ? '#111' : colors.edge_constr_bg" :stroke="colors.edge_constr_border" stroke-width="1" />
        <text :x="cl.mx" :y="cl.my + 5" text-anchor="middle" :font-size="cellSize / 6" font-weight="bold" :fill="cl.kind === 'δ' ? '#fff' : colors.edge_constr_text">{{ cl.text }}</text>
      </g>

      <g v-for="(wt, i) in watchtowers" :key="`w${i}`">
        <circle :cx="wt.x" :cy="wt.y" :r="cellSize / 5" :fill="colors.watchtower_bg" :stroke="colors.watchtower_border" stroke-width="2" />
        <circle v-for="(d, j) in diceDots(wt.value)" :key="j" :cx="wt.x + d[0] * cellSize / 5" :cy="wt.y + d[1] * cellSize / 5" :r="cellSize / 20" :fill="colors.watchtower_text" />
        <text v-if="wt.value > 4" :x="wt.x" :y="wt.y + cellSize / 8" text-anchor="middle" :font-size="cellSize / 4" font-weight="bold" :fill="colors.watchtower_text">{{ wt.value }}</text>
      </g>

      <rect v-if="store.selectedCell" :x="cellX(store.selectedCell[1])" :y="cellY(store.selectedCell[0])" :width="cellSize" :height="cellSize" fill="none" :stroke="colors.selection_border" stroke-width="3" />
      <line v-if="selEdge" :x1="selEdge.x1" :y1="selEdge.y1" :x2="selEdge.x2" :y2="selEdge.y2" :stroke="colors.selection_border" stroke-width="5" stroke-linecap="round" />
      <circle v-if="store.selectedVertex" :cx="vx(store.selectedVertex[1])" :cy="vy(store.selectedVertex[0])" :r="cellSize / 6" :fill="colors.selection_vertex_fill" :stroke="colors.selection_border" stroke-width="3" />
      <circle v-if="boundaryStart" :cx="vx(boundaryStart[1])" :cy="vy(boundaryStart[0])" :r="cellSize / 4" fill="none" :stroke="colors.selection_border" stroke-width="2" stroke-dasharray="4" />

      <rect v-if="hoverCell && !(hoverCell[0] === store.selectedCell?.[0] && hoverCell[1] === store.selectedCell?.[1])" :x="cellX(hoverCell[1])" :y="cellY(hoverCell[0])" :width="cellSize" :height="cellSize" fill="none" :stroke="colors.hover_cell" stroke-width="2" />
      <circle v-if="hoverVertex && !(hoverVertex[0] === store.selectedVertex?.[0] && hoverVertex[1] === store.selectedVertex?.[1])" :cx="vx(hoverVertex[1])" :cy="vy(hoverVertex[0])" :r="cellSize / 8" fill="none" :stroke="colors.hover_vertex" stroke-width="2" />
    </svg>

    <div v-if="overlay.rules.length || overlay.shapes.length" class="overlay" :style="{ left: W + 'px', top: padding + 'px' }">
      <div v-for="r in overlay.rules" :key="r" class="overlay-rule">{{ r }}</div>
      <div v-if="overlay.shapes.length" class="overlay-shapes">
        <div class="overlay-header">形状池</div>
        <svg v-for="(s, i) in overlay.shapes" :key="i" :width="40" :height="40" class="mini-shape">
          <rect v-for="(cell, j) in s" :key="j" :x="2 + cell[1] * 8" :y="2 + cell[0] * 8" :width="7" :height="7" :fill="colors.shape_mini_fill" :stroke="colors.shape_mini_pen" />
        </svg>
      </div>
    </div>

    <div v-if="ctxMenu" class="ctx-menu" :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }" @mousedown.stop @contextmenu.prevent>
      <template v-if="ctxMenu.kind === 'cell'">
        <button v-if="cellAt(p, ctxMenu.cell![0], ctxMenu.cell![1])?.blocked" @click="ctxToggleBlocked">取消障碍</button>
        <button v-else @click="ctxToggleBlocked">设为障碍格</button>
        <hr />
        <button @click="ctxClearNumber">清除数字</button>
        <button @click="ctxClearSymbol">清除符号</button>
        <button @click="ctxClearCellAll">清除全部</button>
        <hr />
        <button @click="ctxToggleCellBoundary">切换此格边框</button>
      </template>
      <template v-else-if="ctxMenu.kind === 'edge'">
        <button @click="ctxToggleEdge">切换分割线</button>
        <hr />
        <button @click="ctxSetConstraint('heterogeneous')">设异生 (≠)</button>
        <button @click="ctxSetConstraint('homogeneous')">设双生 (=)</button>
        <button @click="ctxSetConstraint('inequality')">设不等号 (箭头)</button>
        <button @click="ctxSetConstraint('difference', 1)">设差值</button>
        <template v-if="edgeBetween(p, ...ctxMenu.edge!)?.constraint">
          <hr /><button @click="ctxClearConstraint">清除约束</button>
        </template>
      </template>
      <template v-else>
        <button @click="ctxClearWatchtower">清除望塔值</button>
      </template>
    </div>
  </div>
</template>
