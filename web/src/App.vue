<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef, type Component } from 'vue'
import { usePuzzleStore } from './store/puzzle'
import { useToast } from './composables/useToast'
import { isEditableTarget } from './lib/dom'
import type { PanelKey } from './lib/panels'
import GridCanvas from './components/GridCanvas.vue'
import ToolPalette from './components/ToolPalette.vue'
import ConstraintPanel from './components/ConstraintPanel.vue'
import PropertyPanel from './components/PropertyPanel.vue'
import PuzzleBrowser from './components/PuzzleBrowser.vue'
import ShapeGallery from './components/ShapeGallery.vue'
import AppHeader from './components/AppHeader.vue'
import AppModal from './components/AppModal.vue'
import AppIcon from './components/AppIcon.vue'
import NavRail from './components/NavRail.vue'
import RuleSummary from './components/RuleSummary.vue'
import StageHud from './components/StageHud.vue'
import SolverConsole from './components/SolverConsole.vue'
import ShortcutHelp from './components/ShortcutHelp.vue'
import ToastHost from './components/ToastHost.vue'

const store = usePuzzleStore()
const toast = useToast()

const tab = ref<PanelKey>('tools')
const PANELS = { tools: ToolPalette, rules: ConstraintPanel, library: PuzzleBrowser, shapes: ShapeGallery }
const PANEL_TITLES: Record<PanelKey, string> = {
  tools: '工具箱',
  rules: '规则配置',
  library: '谜题列表',
  shapes: '形状工具',
}
const currentPanel = shallowRef<Component>(ToolPalette)
function setTab(k: PanelKey) {
  tab.value = k
  currentPanel.value = PANELS[k]
}

const showNew = ref(false)
const showHelp = ref(false)
const newH = ref(store.puzzle.grid.height)
const newW = ref(store.puzzle.grid.width)

function openNew() {
  newH.value = store.puzzle.grid.height
  newW.value = store.puzzle.grid.width
  showNew.value = true
}
function createNew() {
  const h = Math.max(2, Math.min(50, newH.value))
  const w = Math.max(2, Math.min(50, newW.value))
  store.newPuzzle(h, w)
  showNew.value = false
  toast.ok(`已新建 ${h}×${w} 盘面`)
}
function doReset() {
  store.reset()
  toast.info('已重置到初始盘面')
}

// ── panel resizing ───────────────────────────────────────────────────────────
const leftW = ref(288)
const rightW = ref(348)
const draggingSide = ref<'left' | 'right' | null>(null)
let drag: { side: 'left' | 'right'; startX: number; startW: number } | null = null

function startResize(side: 'left' | 'right', e: MouseEvent) {
  drag = { side, startX: e.clientX, startW: side === 'left' ? leftW.value : rightW.value }
  draggingSide.value = side
  window.addEventListener('mousemove', onResizeMove)
  window.addEventListener('mouseup', stopResize)
  document.body.style.cursor = 'col-resize'
  document.body.style.userSelect = 'none'
}
function onResizeMove(e: MouseEvent) {
  if (!drag) return
  const dx = e.clientX - drag.startX
  const next = Math.max(220, Math.min(520, drag.side === 'left' ? drag.startW + dx : drag.startW - dx))
  if (drag.side === 'left') leftW.value = next
  else rightW.value = next
}
function stopResize() {
  drag = null
  draggingSide.value = null
  window.removeEventListener('mousemove', onResizeMove)
  window.removeEventListener('mouseup', stopResize)
  document.body.style.cursor = ''
  document.body.style.userSelect = ''
}
onUnmounted(stopResize)

// ── global shortcuts ─────────────────────────────────────────────────────────
const MODE_KEYS: Record<string, string> = {
  v: 'select', b: 'boundary', x: 'block', n: 'number', s: 'symbol', c: 'compass', w: 'watchtower',
}
const MODE_LABELS: Record<string, string> = {
  select: '选择', boundary: '边框', block: '障碍', number: '数字', symbol: '符号', compass: '罗盘', watchtower: '望塔',
}

function onKey(e: KeyboardEvent) {
  if (isEditableTarget(e)) return
  const k = e.key
  if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === 'z') {
    e.preventDefault()
    e.shiftKey ? store.redo() : store.undo()
    return
  }
  if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === 'y') { e.preventDefault(); store.redo(); return }
  if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === 'n') { e.preventDefault(); openNew(); return }
  if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === 'r') { e.preventDefault(); doReset(); return }
  if (k === 'F5') { e.preventDefault(); store.solve(); return }
  if (k === '?') { e.preventDefault(); showHelp.value = !showHelp.value; return }
  if (k === 'Escape' && showHelp.value) { showHelp.value = false; return }
  const mode = MODE_KEYS[k.toLowerCase()]
  if (mode && !e.ctrlKey && !e.metaKey && !e.altKey) {
    store.mode = mode
    toast.info(`工具：${MODE_LABELS[mode]}`, 1200)
  }
}
onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))

// ── status bar ───────────────────────────────────────────────────────────────
const selectionText = computed(() => {
  if (store.selectedCell) return `格 (${store.selectedCell[0]}, ${store.selectedCell[1]})`
  if (store.selectedVertex) return `顶点 (${store.selectedVertex[0]}, ${store.selectedVertex[1]})`
  if (store.selectedEdge) return `边框 (${store.selectedEdge[0]},${store.selectedEdge[1]})–(${store.selectedEdge[2]},${store.selectedEdge[3]})`
  return '未选中'
})
</script>

<template>
  <div class="app">
    <AppHeader @new="openNew" @help="showHelp = true" />

    <div class="app-body">
      <NavRail :model-value="tab" @update:model-value="setTab" @help="showHelp = true" />

      <aside class="side-panel side-panel--left" :style="{ width: leftW + 'px' }">
        <div class="panel-head">
          <span class="panel-head__title">{{ PANEL_TITLES[tab] }}</span>
          <span class="grow" />
          <span class="chip chip--brand">{{ MODE_LABELS[store.mode] }}</span>
        </div>
        <div class="panel-body" :class="{ 'panel-body--flush': tab === 'library' }">
          <Transition name="fade-slide" mode="out-in">
            <component :is="currentPanel" :key="tab" />
          </Transition>
        </div>
        <div
          class="resizer resizer--right" :class="{ 'is-active': draggingSide === 'left' }"
          @mousedown.prevent="startResize('left', $event)"
        />
      </aside>

      <main class="stage">
        <div class="stage__scroll">
          <div class="stage__inner">
            <GridCanvas />
          </div>
        </div>
        <RuleSummary />
        <StageHud />
      </main>

      <aside class="side-panel side-panel--right" :style="{ width: rightW + 'px' }">
        <div class="panel-head">
          <span class="panel-head__title">属性</span>
          <span class="grow" />
          <span class="chip">{{ selectionText }}</span>
        </div>
        <div class="panel-body">
          <PropertyPanel />
        </div>
        <SolverConsole />
        <div
          class="resizer resizer--left" :class="{ 'is-active': draggingSide === 'right' }"
          @mousedown.prevent="startResize('right', $event)"
        />
      </aside>
    </div>

    <footer class="status-bar">
      <span class="status-bar__item">
        <i class="status-bar__dot" :class="store.solving ? 'status-bar__dot--busy' : 'status-bar__dot--live'" />
        {{ store.solving ? '求解中' : '就绪' }}
      </span>
      <span class="status-bar__item">{{ store.name }}</span>
      <span class="status-bar__item tabular">
        {{ store.puzzle.grid.height }}×{{ store.puzzle.grid.width }}
      </span>
      <span class="status-bar__item">{{ store.puzzle.rules.length }} 条规则</span>
      <span class="grow" />
      <span class="status-bar__item truncate">{{ store.solveMessage }}</span>
    </footer>

    <ToastHost />

    <AppModal v-if="showNew" title="新建谜题" subtitle="2 × 2 ~ 50 × 50" narrow @close="showNew = false">
      <template #icon>
        <span class="prop-hero__icon"><AppIcon name="plus" :size="15" /></span>
      </template>
      <div class="row">
        <div class="field grow">
          <span class="field__label">高度（行）</span>
          <input v-model.number="newH" class="input input--num" type="number" min="2" max="50" />
        </div>
        <div class="field grow">
          <span class="field__label">宽度（列）</span>
          <input v-model.number="newW" class="input input--num" type="number" min="2" max="50" />
        </div>
      </div>
      <p class="hint" style="margin-top: 10px">新建会清空当前盘面与求解结果。</p>
      <template #footer>
        <button class="btn" @click="showNew = false">取消</button>
        <button class="btn btn--primary" @click="createNew">创建</button>
      </template>
    </AppModal>

    <ShortcutHelp v-if="showHelp" @close="showHelp = false" />
  </div>
</template>
