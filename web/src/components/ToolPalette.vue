<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useToast } from '../composables/useToast'
import { MODE_COLORS } from '../lib/theme'
import type { CompassJson } from '../lib/types'
import AppIcon from './AppIcon.vue'

const store = usePuzzleStore()
const toast = useToast()

const tools = [
  { mode: 'select', icon: 'cursor', name: '选择', desc: '选中格/边/顶点查看属性', key: 'V' },
  { mode: 'boundary', icon: 'pencil', name: '边框绘制', desc: '沿顶点拖拽绘制分割线', key: 'B' },
  { mode: 'block', icon: 'block', name: '障碍格', desc: '拖拽批量涂改，右键擦除', key: 'X' },
  { mode: 'number', icon: 'hash', name: '数字标注', desc: '点击落数，可直接键入数字', key: 'N' },
  { mode: 'symbol', icon: 'star', name: '符号标注', desc: '点击落下当前符号', key: 'S' },
  { mode: 'compass', icon: 'compass', name: '罗盘标注', desc: '四方向同区域格数', key: 'C' },
  { mode: 'watchtower', icon: 'tower', name: '望塔标注', desc: '顶点相邻区域数', key: 'W' },
]
const color = (mode: string) => MODE_COLORS[mode] ?? '#5B9BD5'

// grid size
const width = ref(store.puzzle.grid.width)
const height = ref(store.puzzle.grid.height)
watch(() => store.puzzle.grid, (g) => { width.value = g.width; height.value = g.height }, { deep: false })
function applySize() {
  const h = Math.max(2, Math.min(50, height.value))
  const w = Math.max(2, Math.min(50, width.value))
  if (h === store.puzzle.grid.height && w === store.puzzle.grid.width) return
  store.newPuzzle(h, w)
  toast.ok(`盘面已改为 ${h}×${w}`)
}

// number / watchtower (shared "current number")
const numberInput = ref('')
function onNumber(v: string) { const n = parseInt(v); store.currentNumber = isNaN(n) ? null : n }
const watchtower = ref(0)
const DICE: Record<number, number[]> = {
  1: [4],
  2: [0, 8],
  3: [0, 4, 8],
  4: [0, 2, 6, 8],
}
function setWatchtower(v: number) {
  watchtower.value = v
  store.currentNumber = v
  if (store.mode !== 'watchtower') store.mode = 'watchtower'
}

// symbol
const symbolInput = ref('')
function onSymbol(v: string) { store.currentSymbol = v || null }
const quickSymbols = ['★', '●', '◆', '▲', '♥', '■', '✚', '☾']
function setSymbol(s: string) {
  symbolInput.value = s
  store.currentSymbol = s
  if (store.mode !== 'symbol') store.mode = 'symbol'
}

// compass
const compass = reactive({ up: '', down: '', left: '', right: '' })
function parseVal(v: string): number { return v === '' ? -1 : parseInt(v) || 0 }
function applyCompass() {
  const clue: CompassJson = {
    up: parseVal(compass.up), down: parseVal(compass.down),
    left: parseVal(compass.left), right: parseVal(compass.right),
  }
  store.currentCompass = clue
  store.mode = 'compass'
  toast.info('罗盘已就绪，点击格子应用', 1600)
}
</script>

<template>
  <div class="col" style="gap: var(--sp-5)">
    <section>
      <h3 class="section-title">绘制工具</h3>
      <div class="tool-grid">
        <button
          v-for="t in tools" :key="t.mode"
          class="tool-card" :class="{ 'is-active': store.mode === t.mode }"
          :style="{ '--c': color(t.mode) }"
          @click="store.mode = t.mode"
        >
          <span class="tool-card__icon"><AppIcon :name="t.icon" :size="15" /></span>
          <span class="tool-card__text">
            <span class="tool-card__name">{{ t.name }}</span>
            <span class="tool-card__desc">{{ t.desc }}</span>
          </span>
          <kbd class="kbd">{{ t.key }}</kbd>
        </button>
      </div>
    </section>

    <section>
      <h3 class="section-title">盘面尺寸</h3>
      <div class="row">
        <div class="field grow">
          <span class="field__label">行</span>
          <input v-model.number="height" class="input input--num" type="number" min="2" max="50" @keyup.enter="applySize" />
        </div>
        <span class="muted" style="padding-top: 16px">×</span>
        <div class="field grow">
          <span class="field__label">列</span>
          <input v-model.number="width" class="input input--num" type="number" min="2" max="50" @keyup.enter="applySize" />
        </div>
      </div>
      <button class="btn btn--block btn--sm" style="margin-top: var(--sp-2)" @click="applySize">
        <AppIcon name="grid" :size="13" />重建盘面
      </button>
      <p class="hint" style="margin-top: var(--sp-2)">重建会清空当前盘面内容与求解结果。</p>
    </section>

    <section>
      <h3 class="section-title">数字线索</h3>
      <input v-model="numberInput" class="input input--num" placeholder="0 – 999" @input="onNumber(numberInput)" />
      <p class="hint" style="margin-top: var(--sp-1)">选择「数字标注」后点击格子落下。</p>
    </section>

    <section>
      <h3 class="section-title">望塔值</h3>
      <div class="dice-row">
        <button
          v-for="v in [1, 2, 3, 4]" :key="v"
          class="dice-btn" :class="{ 'is-active': watchtower === v }"
          :data-tip="`望塔 ${v}`" @click="setWatchtower(v)"
        >
          <i v-for="n in 9" :key="n" :class="{ on: DICE[v].includes(n - 1) }" />
        </button>
      </div>
    </section>

    <section>
      <h3 class="section-title">符号</h3>
      <input v-model="symbolInput" class="input" maxlength="2" placeholder="输入任意符号" @input="onSymbol(symbolInput)" />
      <div class="swatch-row" style="margin-top: var(--sp-2)">
        <button
          v-for="s in quickSymbols" :key="s"
          class="swatch" :class="{ 'is-active': store.currentSymbol === s }"
          @click="setSymbol(s)"
        >{{ s }}</button>
      </div>
    </section>

    <section>
      <h3 class="section-title">罗盘</h3>
      <div class="compass-pad">
        <span />
        <input v-model="compass.up" class="input" placeholder="上" />
        <span />
        <input v-model="compass.left" class="input" placeholder="左" />
        <span class="compass-pad__center">◎</span>
        <input v-model="compass.right" class="input" placeholder="右" />
        <span />
        <input v-model="compass.down" class="input" placeholder="下" />
        <span />
      </div>
      <button class="btn btn--block btn--sm" style="margin-top: var(--sp-2)" @click="applyCompass">
        <AppIcon name="compass" :size="13" />应用罗盘
      </button>
      <p class="hint" style="margin-top: var(--sp-1)">留空表示不约束该方向（内部记为 −1）。</p>
    </section>
  </div>
</template>
