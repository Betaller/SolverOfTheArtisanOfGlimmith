<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { MODE_COLORS } from '../lib/theme'
import type { CompassJson } from '../lib/types'

const store = usePuzzleStore()

const tools = [
  { mode: 'select', label: '📌 选择', tip: '选择/查看单元格属性 (V)' },
  { mode: 'boundary', label: '━ 边框绘制', tip: '点击顶点拖拽绘制分割线 (B)' },
  { mode: 'block', label: '✖ 障碍格', tip: '点击切换障碍格 (X)' },
  { mode: 'number', label: '# 数字标注', tip: '点击输入数字线索 (N)' },
  { mode: 'symbol', label: '★ 符号标注', tip: '点击输入符号 (S)' },
  { mode: 'compass', label: '◎ 罗盘标注', tip: '点击设置四方向计数 (C)' },
  { mode: 'watchtower', label: '◉ 望塔标注', tip: '点击顶点设置望塔值 (W)' },
]
const color = (mode: string) => MODE_COLORS[mode] ?? '#5B9BD5'

// grid size
const width = ref(store.puzzle.grid.width)
const height = ref(store.puzzle.grid.height)
watch(() => store.puzzle.grid, (g) => { width.value = g.width; height.value = g.height }, { deep: false })
function applySize() { if (width.value >= 2 && height.value >= 2) store.newPuzzle(height.value, width.value) }

// number / watchtower (shared "current number")
const numberInput = ref('')
function onNumber(v: string) { const n = parseInt(v); store.currentNumber = isNaN(n) ? null : n }
const watchtower = ref(0)
function setWatchtower(v: number) { watchtower.value = v; store.currentNumber = v }

// symbol
const symbolInput = ref('')
function onSymbol(v: string) { store.currentSymbol = v || null }
const quickSymbols = ['★', '●', '◆', '▲', '♥', '■']
function setSymbol(s: string) { symbolInput.value = s; store.currentSymbol = s }

// compass
const compass = reactive({ up: '', down: '', left: '', right: '' })
function parseVal(v: string): number { return v === '' ? -1 : parseInt(v) }
function applyCompass() {
  const clue: CompassJson = { up: parseVal(compass.up), down: parseVal(compass.down), left: parseVal(compass.left), right: parseVal(compass.right) }
  store.currentCompass = clue
}
</script>

<template>
  <div class="tool-palette">
    <button
      v-for="t in tools" :key="t.mode"
      class="mode-btn"
      :class="{ active: store.mode === t.mode }"
      :style="{ '--c': color(t.mode) }"
      :title="t.tip"
      @click="store.mode = t.mode"
    >{{ t.label }}</button>

    <hr />
    <div class="section-title">盘面大小</div>
    <div class="size-row">
      <input v-model.number="width" type="number" min="2" max="50" class="size-input" @change="applySize" />
      <span>×</span>
      <input v-model.number="height" type="number" min="2" max="50" class="size-input" @change="applySize" />
    </div>

    <hr />
    <div class="section-title">数字</div>
    <input v-model="numberInput" class="text-input" placeholder="数字 (0-999)" @input="onNumber(numberInput)" />

    <hr />
    <div class="section-title">望塔值</div>
    <div class="btn-row">
      <button v-for="v in [1, 2, 3, 4]" :key="v" class="wt-btn" :class="{ active: watchtower === v }" @click="setWatchtower(v)">{{ v }}</button>
    </div>

    <hr />
    <div class="section-title">符号</div>
    <input v-model="symbolInput" class="text-input" maxlength="2" placeholder="符号 (如 ★)" @input="onSymbol(symbolInput)" />
    <div class="btn-row">
      <button v-for="s in quickSymbols" :key="s" class="sym-btn" @click="setSymbol(s)">{{ s }}</button>
    </div>

    <hr />
    <div class="section-title">罗盘</div>
    <div class="compass-grid">
      <span></span><input v-model="compass.up" class="compass-input" placeholder="上" /><span></span>
      <input v-model="compass.left" class="compass-input" placeholder="左" /><span class="compass-center">◎</span><input v-model="compass.right" class="compass-input" placeholder="右" />
      <span></span><input v-model="compass.down" class="compass-input" placeholder="下" /><span></span>
    </div>
    <button class="apply-btn" @click="applyCompass">应用到选中格</button>
  </div>
</template>
