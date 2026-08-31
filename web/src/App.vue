<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { usePuzzleStore } from './store/puzzle'
import { isTypingTarget } from './lib/fixes'
import GridCanvas from './components/GridCanvas.vue'
import ToolPalette from './components/ToolPalette.vue'
import ConstraintPanel from './components/ConstraintPanel.vue'
import PropertyPanel from './components/PropertyPanel.vue'
import PuzzleBrowser from './components/PuzzleBrowser.vue'
import ShapeGallery from './components/ShapeGallery.vue'

const store = usePuzzleStore()
const tab = ref('工具')
const showNew = ref(false)
const newH = ref(6)
const newW = ref(6)

function onKey(e: KeyboardEvent) {
  // Ignore shortcuts while typing in a text field so single-letter keys,
  // Ctrl+Z (native undo), Ctrl+R / F5 (native reload) keep working.
  if (isTypingTarget(e.target)) return
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') { e.preventDefault(); e.shiftKey ? store.redo() : store.undo(); return }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') { e.preventDefault(); showNew.value = true; return }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r') { e.preventDefault(); store.reset(); return }
  if (e.key === 'F5') { e.preventDefault(); store.solve(); return }
  const modeKeys: Record<string, string> = { v: 'select', b: 'boundary', x: 'block', n: 'number', s: 'symbol', c: 'compass', w: 'watchtower' }
  if (modeKeys[e.key.toLowerCase()]) { store.mode = modeKeys[e.key.toLowerCase()] }
}
onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))

function createNew() { store.newPuzzle(newH.value, newW.value); showNew.value = false }
</script>

<template>
  <div class="app">
    <header class="toolbar">
      <button @click="showNew = true">新建</button>
      <button @click="store.undo">撤销</button>
      <button @click="store.redo">重做</button>
      <button @click="store.reset">重置</button>
      <h1>格里米斯的工匠 - 求解器</h1>
      <div class="status" v-if="store.showSolution && store.officialAnswer">
        <span class="badge official">官方解</span>
      </div>
      <div class="status" v-else-if="store.showSolution && store.solution?.solved">
        <span class="badge ok">{{ store.solution.solver }} · {{ store.solution.elapsed_ms }}ms</span>
      </div>
    </header>

    <div class="content">
      <aside class="left-panel">
        <div class="tabs">
          <button v-for="t in ['工具', '规则配置', '谜题列表', '形状工具']" :key="t" :class="{ active: tab === t }" @click="tab = t">{{ t }}</button>
        </div>
        <div class="tab-body">
          <ToolPalette v-if="tab === '工具'" />
          <ConstraintPanel v-else-if="tab === '规则配置'" />
          <PuzzleBrowser v-else-if="tab === '谜题列表'" />
          <ShapeGallery v-else />
        </div>
      </aside>

      <main class="grid-stage">
        <GridCanvas />
      </main>

      <aside class="right-panel">
        <PropertyPanel />
        <div class="control">
          <button class="solve-btn" @click="store.solving ? store.cancel() : store.solve()">{{ store.solving ? '取消求解' : '求解' }}</button>
          <button class="reset-btn" @click="store.reset">重置</button>
          <label class="timeout-label">超时(ms):
            <input class="timeout-input" type="number" min="500" max="60000" step="500"
                   v-model.number="store.solveTimeoutMs" :disabled="store.solving" />
          </label>
          <div class="result" v-html="store.resultHtml" />
        </div>
      </aside>
    </div>

    <footer class="status-bar">{{ store.solveMessage }}</footer>

    <div v-if="showNew" class="modal-backdrop" @mousedown.self="showNew = false">
      <div class="modal">
        <h3>新建谜题</h3>
        <div class="row"><label>高度:</label><input v-model.number="newH" type="number" min="2" max="50" /></div>
        <div class="row"><label>宽度:</label><input v-model.number="newW" type="number" min="2" max="50" /></div>
        <div class="btn-row right"><button class="ok" @click="createNew">确定</button><button @click="showNew = false">取消</button></div>
      </div>
    </div>
  </div>
</template>
