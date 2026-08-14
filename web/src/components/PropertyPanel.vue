<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { cellAt, edgeBetween, vertexAt, makeConstraint } from '../lib/model'
import type { CellJson, EdgeJson } from '../lib/types'
import ShapeGridEditor from './ShapeGridEditor.vue'

const store = usePuzzleStore()
const patternModal = ref<'shape' | 'fence' | null>(null)
const patternCells = ref<[number, number][]>([])

const selCell = computed<CellJson | null>(() => store.selectedCell ? cellAt(store.puzzle, ...store.selectedCell) ?? null : null)
const selEdge = computed<EdgeJson | null>(() => store.selectedEdge ? edgeBetween(store.puzzle, ...store.selectedEdge) ?? null : null)
const selVertex = computed(() => store.selectedVertex ? vertexAt(store.puzzle, ...store.selectedVertex) ?? null : null)

function setCell(props: Partial<CellJson>) {
  if (store.selectedCell) { Object.assign(cellAt(store.puzzle, ...store.selectedCell)!, props); store.markModified() }
}
function setBlocked(b: boolean) {
  if (store.selectedCell) {
    const c = cellAt(store.puzzle, ...store.selectedCell)!
    c.blocked = b
    if (b) { c.number = undefined; c.symbol = undefined; c.compass = undefined; c.shape_pattern = undefined; c.fence_pattern = undefined }
    store.markModified()
  }
}
function toggleBoundary() {
  if (selEdge.value) { selEdge.value.is_boundary = !selEdge.value.is_boundary; store.markModified() }
}
function setConstraint(type: string, value?: number) {
  if (store.selectedEdge) { const e = edgeBetween(store.puzzle, ...store.selectedEdge)!; e.constraint = makeConstraint(type as any, value); store.markModified() }
}
function clearConstraint() { if (store.selectedEdge) { edgeBetween(store.puzzle, ...store.selectedEdge)!.constraint = undefined; store.markModified() } }
function setWatchtower(val: number | null) {
  if (store.selectedVertex) { const v = vertexAt(store.puzzle, ...store.selectedVertex)!; v.watchtower = val ?? undefined; store.markModified() }
}

function openPattern(kind: 'shape' | 'fence') {
  patternModal.value = kind
  const c = selCell.value
  patternCells.value = kind === 'shape' ? [...(c?.shape_pattern ?? [])] : [...(c?.fence_pattern ?? [])]
}
function savePattern() {
  if (patternModal.value === 'shape') setCell({ shape_pattern: patternCells.value.length ? patternCells.value : undefined })
  else setCell({ fence_pattern: patternCells.value.length ? patternCells.value : undefined })
  patternModal.value = null
}

const compassVals = reactive({ up: 0, down: 0, left: 0, right: 0 })
function openCompass() {
  const cp = selCell.value?.compass
  compassVals.up = cp?.up ?? -1; compassVals.down = cp?.down ?? -1; compassVals.left = cp?.left ?? -1; compassVals.right = cp?.right ?? -1
}
function applyCompass() {
  setCell({ compass: { up: compassVals.up, down: compassVals.down, left: compassVals.left, right: compassVals.right } })
}
</script>

<template>
  <div class="property-panel">
    <div class="panel-title">属性面板</div>
    <div class="prop-info">
      <template v-if="store.selectedCell">单元格 ({{ store.selectedCell[0] }}, {{ store.selectedCell[1] }})</template>
      <template v-else-if="store.selectedEdge">边框 ({{ store.selectedEdge[0] }},{{ store.selectedEdge[1] }})-{{ store.selectedEdge[2] }},{{ store.selectedEdge[3] }})</template>
      <template v-else-if="store.selectedVertex">顶点 ({{ store.selectedVertex[0] }}, {{ store.selectedVertex[1] }})</template>
      <template v-else>未选中任何对象</template>
    </div>

    <!-- cell -->
    <template v-if="selCell">
      <label class="check"><input type="checkbox" :checked="selCell.blocked" @change="setBlocked(($event.target as HTMLInputElement).checked)" /> 障碍格</label>
      <hr />
      <div class="row"><label>数字</label><input type="number" min="0" max="999" :value="selCell.number ?? 0" @change="setCell({ number: parseInt(($event.target as HTMLInputElement).value) || undefined })" /><button @click="setCell({ number: undefined })">清除</button></div>
      <div class="row"><label>符号</label><input :value="selCell.symbol ?? ''" maxlength="2" @change="setCell({ symbol: ($event.target as HTMLInputElement).value || undefined })" /><button v-for="s in ['★','●','◆','▲','♥','■']" :key="s" @click="setCell({ symbol: s })">{{ s }}</button></div>
      <hr />
      <div class="row"><label>罗盘</label><button @click="openCompass">编辑</button></div>
      <div v-if="compassVals.up !== -1 || compassVals.down !== -1 || compassVals.left !== -1 || compassVals.right !== -1" class="compass-grid">
        <span></span><input v-model.number="compassVals.up" type="number" min="-1" max="99" /><span></span>
        <input v-model.number="compassVals.left" type="number" min="-1" max="99" /><span>●</span><input v-model.number="compassVals.right" type="number" min="-1" max="99" />
        <span></span><input v-model.number="compassVals.down" type="number" min="-1" max="99" /><span></span>
      </div>
      <button @click="applyCompass">应用罗盘</button>
      <hr />
      <div class="row"><label>图案</label></div>
      <button @click="openPattern('shape')">拼块图案 {{ selCell.shape_pattern ? '有' : '无' }}</button>
      <button @click="openPattern('fence')">围栏标记 {{ selCell.fence_pattern ? '有' : '无' }}</button>
    </template>

    <!-- edge -->
    <template v-else-if="selEdge">
      <div class="prop-info">分割: {{ selEdge.is_boundary ? '是' : '否' }}，约束: {{ selEdge.constraint ? selEdge.constraint.type + (selEdge.constraint.value != null ? ', 值=' + selEdge.constraint.value : '') : '无' }}</div>
      <button :class="selEdge.is_boundary ? 'danger' : 'blue'" @click="toggleBoundary">{{ selEdge.is_boundary ? '取消分割线' : '设为分割线' }}</button>
      <hr />
      <div class="row">
        <button :class="{ active: selEdge.constraint?.type === 'heterogeneous' }" @click="setConstraint('heterogeneous')">≠异生</button>
        <button :class="{ active: selEdge.constraint?.type === 'homogeneous' }" @click="setConstraint('homogeneous')">=双生</button>
      </div>
      <div class="row"><label>不等</label><button @click="setConstraint('inequality', 0)">v下大</button><button @click="setConstraint('inequality', 1)">^上大</button></div>
      <div class="row"><label>差值</label><input type="number" min="1" max="999" :value="selEdge.constraint?.type === 'difference' ? selEdge.constraint.value : 1" @change="setConstraint('difference', parseInt(($event.target as HTMLInputElement).value))" /><button @click="setConstraint('difference', 1)">设差值</button></div>
      <hr />
      <button class="danger" @click="clearConstraint">清除约束</button>
    </template>

    <!-- vertex -->
    <template v-else-if="selVertex">
      <div class="prop-info">望塔: {{ selVertex.watchtower ?? '无' }}</div>
      <div class="row"><label>望塔值</label><input type="number" min="0" max="4" :value="selVertex.watchtower ?? 0" @change="setWatchtower(parseInt(($event.target as HTMLInputElement).value) || null)" /><button @click="setWatchtower(null)">清除</button></div>
    </template>

    <!-- pattern modal -->
    <div v-if="patternModal" class="modal-backdrop" @mousedown.self="patternModal = null">
      <div class="modal">
        <h3>{{ patternModal === 'shape' ? '拼块图案编辑' : '围栏标记编辑' }}</h3>
        <ShapeGridEditor :grid-size="5" :model-value="patternCells" @update:model-value="patternCells = $event" />
        <div class="btn-row right">
          <button class="ok" @click="savePattern">确定</button>
          <button @click="patternModal = null">取消</button>
        </div>
      </div>
    </div>
  </div>
</template>
