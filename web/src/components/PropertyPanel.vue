<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { cellAt, edgeBetween, vertexAt, makeConstraint } from '../lib/model'
import type { CellJson, EdgeJson } from '../lib/types'
import AppIcon from './AppIcon.vue'
import AppModal from './AppModal.vue'
import AppSwitch from './AppSwitch.vue'
import ShapeGridEditor from './ShapeGridEditor.vue'

const store = usePuzzleStore()
const patternModal = ref<'shape' | 'fence' | null>(null)
const patternCells = ref<[number, number][]>([])
const showCompass = ref(false)

const selCell = computed<CellJson | null>(() => store.selectedCell ? cellAt(store.puzzle, ...store.selectedCell) ?? null : null)
const selEdge = computed<EdgeJson | null>(() => store.selectedEdge ? edgeBetween(store.puzzle, ...store.selectedEdge) ?? null : null)
const selVertex = computed(() => store.selectedVertex ? vertexAt(store.puzzle, ...store.selectedVertex) ?? null : null)

const QUICK_SYMBOLS = ['★', '●', '◆', '▲', '♥', '■']

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

const compassVals = reactive({ up: -1, down: -1, left: -1, right: -1 })
function openCompass() {
  const cp = selCell.value?.compass
  compassVals.up = cp?.up ?? -1; compassVals.down = cp?.down ?? -1; compassVals.left = cp?.left ?? -1; compassVals.right = cp?.right ?? -1
  showCompass.value = true
}
function applyCompass() {
  setCell({ compass: { up: compassVals.up, down: compassVals.down, left: compassVals.left, right: compassVals.right } })
  showCompass.value = false
}
const compassSet = computed(() => selCell.value?.compass != null)
</script>

<template>
  <div class="col" style="gap: var(--sp-4)">
    <!-- selection hero -->
    <div v-if="selCell || selEdge || selVertex" class="prop-hero">
      <span class="prop-hero__icon">
        <AppIcon :name="selCell ? 'grid' : selEdge ? 'pencil' : 'tower'" :size="15" />
      </span>
      <span class="grow">
        <span class="prop-hero__title">
          <template v-if="selCell">单元格 ({{ store.selectedCell![0] }}, {{ store.selectedCell![1] }})</template>
          <template v-else-if="selEdge">边框 ({{ store.selectedEdge![0] }},{{ store.selectedEdge![1] }})–({{ store.selectedEdge![2] }},{{ store.selectedEdge![3] }})</template>
          <template v-else>顶点 ({{ store.selectedVertex![0] }}, {{ store.selectedVertex![1] }})</template>
        </span>
        <span class="prop-hero__sub">
          <template v-if="selCell">{{ selCell.blocked ? '障碍格' : '可填格' }}</template>
          <template v-else-if="selEdge">{{ selEdge.is_boundary ? '已设为分割线' : '未分割' }}</template>
          <template v-else>望塔 {{ selVertex?.watchtower ?? '无' }}</template>
        </span>
      </span>
      <button class="btn btn--icon btn--sm btn--ghost" data-tip="取消选中 (Esc)" @click="store.clearSelection()">
        <AppIcon name="close" :size="13" />
      </button>
    </div>
    <div v-else class="empty">
      <span class="empty__icon"><AppIcon name="cursor" :size="18" /></span>
      <span class="empty__title">未选中任何对象</span>
      <span class="empty__text">在盘面上点击一个格子、边框或顶点，即可在此编辑它的属性。</span>
    </div>

    <!-- cell -->
    <template v-if="selCell">
      <section>
        <h3 class="section-title">格属性</h3>
        <AppSwitch :model-value="!!selCell.blocked" @update:model-value="setBlocked">障碍格</AppSwitch>
      </section>

      <template v-if="!selCell.blocked">
        <section>
          <h3 class="section-title">数字</h3>
          <div class="row">
            <input
              class="input input--num grow" type="number" min="0" max="999"
              :value="selCell.number ?? ''" placeholder="—"
              @change="setCell({ number: parseInt(($event.target as HTMLInputElement).value) || undefined })"
            />
            <button class="btn btn--sm" @click="setCell({ number: undefined })">清除</button>
          </div>
        </section>

        <section>
          <h3 class="section-title">符号</h3>
          <div class="row">
            <input
              class="input grow" maxlength="2" :value="selCell.symbol ?? ''" placeholder="—"
              @change="setCell({ symbol: ($event.target as HTMLInputElement).value || undefined })"
            />
            <button class="btn btn--sm" @click="setCell({ symbol: undefined })">清除</button>
          </div>
          <div class="swatch-row" style="margin-top: var(--sp-2)">
            <button
              v-for="s in QUICK_SYMBOLS" :key="s"
              class="swatch" :class="{ 'is-active': selCell.symbol === s }"
              @click="setCell({ symbol: s })"
            >{{ s }}</button>
          </div>
        </section>

        <section>
          <h3 class="section-title">罗盘</h3>
          <button class="btn btn--block btn--sm" @click="openCompass">
            <AppIcon name="compass" :size="13" />
            {{ compassSet ? '编辑四方向计数' : '设置四方向计数' }}
          </button>
          <button v-if="compassSet" class="btn btn--block btn--sm btn--ghost" style="margin-top: var(--sp-1)" @click="setCell({ compass: undefined })">
            清除罗盘
          </button>
        </section>

        <section>
          <h3 class="section-title">图案标记</h3>
          <div class="row">
            <button class="btn btn--sm grow" @click="openPattern('shape')">
              <AppIcon name="shapes" :size="12" />拼块 {{ selCell.shape_pattern ? '已设置' : '—' }}
            </button>
            <button class="btn btn--sm grow" @click="openPattern('fence')">
              <AppIcon name="target" :size="12" />围栏 {{ selCell.fence_pattern ? '已设置' : '—' }}
            </button>
          </div>
        </section>
      </template>
    </template>

    <!-- edge -->
    <template v-else-if="selEdge">
      <section>
        <h3 class="section-title">分割</h3>
        <button class="btn btn--block btn--sm" :class="selEdge.is_boundary ? 'btn--danger' : ''" @click="toggleBoundary">
          <AppIcon :name="selEdge.is_boundary ? 'close' : 'pencil'" :size="13" />
          {{ selEdge.is_boundary ? '取消分割线' : '设为分割线' }}
        </button>
      </section>

      <section>
        <h3 class="section-title">形状约束</h3>
        <div class="segmented" style="width: 100%">
          <button :class="{ 'is-active': selEdge.constraint?.type === 'heterogeneous' }" @click="setConstraint('heterogeneous')">≠ 异生</button>
          <button :class="{ 'is-active': selEdge.constraint?.type === 'homogeneous' }" @click="setConstraint('homogeneous')">= 双生</button>
          <button :class="{ 'is-active': selEdge.constraint?.type == null }" @click="clearConstraint()">无</button>
        </div>
      </section>

      <section>
        <h3 class="section-title">不等号</h3>
        <div class="segmented" style="width: 100%">
          <button :class="{ 'is-active': selEdge.constraint?.type === 'inequality' && selEdge.constraint?.value === 0 }" @click="setConstraint('inequality', 0)">v 下大</button>
          <button :class="{ 'is-active': selEdge.constraint?.type === 'inequality' && selEdge.constraint?.value === 1 }" @click="setConstraint('inequality', 1)">^ 上大</button>
        </div>
      </section>

      <section>
        <h3 class="section-title">差值</h3>
        <div class="row">
          <input
            class="input input--num grow" type="number" min="1" max="999"
            :value="selEdge.constraint?.type === 'difference' ? selEdge.constraint.value : 1"
            @change="setConstraint('difference', parseInt(($event.target as HTMLInputElement).value))"
          />
          <button class="btn btn--sm" @click="setConstraint('difference', 1)">设为差值</button>
        </div>
      </section>
    </template>

    <!-- vertex -->
    <template v-else-if="selVertex">
      <section>
        <h3 class="section-title">望塔值</h3>
        <div class="row">
          <input
            class="input input--num grow" type="number" min="0" max="4"
            :value="selVertex.watchtower ?? ''" placeholder="—"
            @change="setWatchtower(parseInt(($event.target as HTMLInputElement).value) || null)"
          />
          <button class="btn btn--sm" @click="setWatchtower(null)">清除</button>
        </div>
        <p class="hint" style="margin-top: var(--sp-2)">望塔值 = 该顶点相邻的区域数量（0–4）。</p>
      </section>
    </template>

    <!-- compass editor -->
    <AppModal
      v-if="showCompass" title="罗盘计数" subtitle="留空 / −1 表示该方向不约束" narrow
      @close="showCompass = false"
    >
      <div class="compass-pad" style="width: 190px">
        <span />
        <input v-model.number="compassVals.up" class="input" type="number" min="-1" max="99" />
        <span />
        <input v-model.number="compassVals.left" class="input" type="number" min="-1" max="99" />
        <span class="compass-pad__center">◎</span>
        <input v-model.number="compassVals.right" class="input" type="number" min="-1" max="99" />
        <span />
        <input v-model.number="compassVals.down" class="input" type="number" min="-1" max="99" />
        <span />
      </div>
      <template #footer>
        <button class="btn" @click="showCompass = false">取消</button>
        <button class="btn btn--primary" @click="applyCompass">应用</button>
      </template>
    </AppModal>

    <!-- pattern editor -->
    <AppModal
      v-if="patternModal" :title="patternModal === 'shape' ? '拼块图案' : '围栏标记'" narrow
      @close="patternModal = null"
    >
      <div style="display: flex; justify-content: center">
        <ShapeGridEditor :grid-size="5" :model-value="patternCells" @update:model-value="patternCells = $event" />
      </div>
      <template #footer>
        <button class="btn" @click="patternModal = null">取消</button>
        <button class="btn btn--primary" @click="savePattern">确定</button>
      </template>
    </AppModal>
  </div>
</template>
