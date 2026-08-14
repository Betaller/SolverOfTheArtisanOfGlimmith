<script setup lang="ts">
import { computed } from 'vue'
import type { PuzzleModel } from '../lib/codec'
import { cellKey, regionColor } from '../lib/codec'

const props = defineProps<{
  model: PuzzleModel
  regions: Map<string, number> | null
}>()

// Cell size scales to the grid so a 16×16 board still fits comfortably.
const CELL = computed(() => {
  const n = Math.max(props.model.height, props.model.width)
  return Math.max(24, Math.min(56, Math.floor(560 / n)))
})

const W = computed(() => props.model.width * CELL.value)
const H = computed(() => props.model.height * CELL.value)

interface RenderedCell {
  r: number
  c: number
  x: number
  y: number
  blocked: boolean
  fill: string
  number?: number
  symbol?: string
}

const cells = computed<RenderedCell[]>(() => {
  const S = CELL.value
  const out: RenderedCell[] = []
  for (let r = 0; r < props.model.height; r++) {
    for (let c = 0; c < props.model.width; c++) {
      const k = cellKey(r, c)
      const blocked = props.model.blocked.has(k)
      const ri = props.regions?.get(k)
      out.push({
        r,
        c,
        x: c * S,
        y: r * S,
        blocked,
        fill: blocked ? '#3a3f4b' : ri != null ? regionColor(ri) : '#fafafa',
        number: props.model.numbers.get(k),
        symbol: props.model.symbols.get(k),
      })
    }
  }
  return out
})

interface BoundaryLine {
  x1: number
  y1: number
  x2: number
  y2: number
}

// h-edge (between (r,c) and (r,c+1)) → vertical line; v-edge → horizontal line.
const boundaryLines = computed<BoundaryLine[]>(() => {
  const S = CELL.value
  const lines: BoundaryLine[] = []
  for (const k of props.model.boundaries) {
    const kind = k[0]
    const [r, c] = k.slice(2).split(',').map(Number)
    if (kind === 'h') {
      lines.push({ x1: (c + 1) * S, y1: r * S, x2: (c + 1) * S, y2: (r + 1) * S })
    } else {
      lines.push({ x1: c * S, y1: (r + 1) * S, x2: (c + 1) * S, y2: (r + 1) * S })
    }
  }
  return lines
})
</script>

<template>
  <svg :width="W" :height="H" class="grid" :viewBox="`0 0 ${W} ${H}`">
    <g v-for="cell in cells" :key="`${cell.r}-${cell.c}`">
      <rect
        :x="cell.x"
        :y="cell.y"
        :width="CELL"
        :height="CELL"
        :fill="cell.fill"
        stroke="#cfd4dc"
        stroke-width="1"
      />
      <text
        v-if="cell.number != null"
        :x="cell.x + CELL / 2"
        :y="cell.y + CELL * 0.62"
        text-anchor="middle"
        :font-size="CELL * 0.5"
        fill="#1f2430"
        font-weight="600"
      >
        {{ cell.number }}
      </text>
      <text
        v-if="cell.symbol"
        :x="cell.x + CELL / 2"
        :y="cell.y + CELL * 0.85"
        text-anchor="middle"
        :font-size="CELL * 0.4"
        fill="#1f2430"
      >
        {{ cell.symbol }}
      </text>
    </g>
    <line
      v-for="(l, i) in boundaryLines"
      :key="`b${i}`"
      :x1="l.x1"
      :y1="l.y1"
      :x2="l.x2"
      :y2="l.y2"
      stroke="#1f2430"
      stroke-width="3"
      stroke-linecap="round"
    />
  </svg>
</template>
