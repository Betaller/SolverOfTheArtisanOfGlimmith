<script setup lang="ts">
import { ref } from 'vue'
import { useToast } from '../composables/useToast'
import { normalize, canonicalKey } from '../lib/shapes'
import AppIcon from './AppIcon.vue'
import AppModal from './AppModal.vue'
import ShapeGridEditor from './ShapeGridEditor.vue'

const props = defineProps<{ shapes: [number, number][][] }>()
const emit = defineEmits<{ (e: 'save', shapes: [number, number][][]): void; (e: 'close'): void }>()
const toast = useToast()

const shapes = ref<[number, number][][]>(props.shapes.map((s) => [...s]))
const drawing = ref<[number, number][]>([])
const selectedIndex = ref<number | null>(null)

function addShape() {
  if (!drawing.value.length) return
  const key = canonicalKey(drawing.value)
  if (shapes.value.some((s) => canonicalKey(s) === key)) {
    toast.info('该形状已在形状池中')
    return
  }
  shapes.value.push(normalize(drawing.value))
  drawing.value = []
}
function removeSelected() {
  if (selectedIndex.value != null && selectedIndex.value < shapes.value.length) {
    shapes.value.splice(selectedIndex.value, 1)
    selectedIndex.value = null
  }
}
function cellsDesc(s: [number, number][]): string {
  return [...s].sort((a, b) => a[0] - b[0] || a[1] - b[1]).map(([r, c]) => `(${r},${c})`).join(' ')
}
function save() {
  emit('save', shapes.value)
  toast.ok(`形状池已保存 · ${shapes.value.length} 个形状`)
}
</script>

<template>
  <AppModal title="形状池编辑器" :subtitle="`${shapes.length} 个形状`" @close="emit('close')">
    <h3 class="section-title">绘制新形状</h3>
    <div style="display: flex; flex-direction: column; align-items: center; gap: var(--sp-3)">
      <ShapeGridEditor :grid-size="6" :model-value="drawing" @update:model-value="drawing = $event" />
      <div class="row">
        <button class="btn btn--sm" :disabled="!drawing.length" @click="drawing = []">清空</button>
        <button class="btn btn--sm btn--primary" :disabled="!drawing.length" @click="addShape">
          <AppIcon name="plus" :size="12" />添加到形状池
        </button>
      </div>
    </div>

    <h3 class="section-title" style="margin-top: var(--sp-5)">形状池列表</h3>
    <ul v-if="shapes.length" class="shape-list">
      <li
        v-for="(s, i) in shapes" :key="i"
        :class="{ 'is-selected': selectedIndex === i }"
        @click="selectedIndex = i"
      >
        <span class="chip chip--brand">{{ i + 1 }}</span>
        <span class="grow truncate">面积 {{ s.length }}</span>
        <span class="shape-list__code">{{ cellsDesc(s) }}</span>
      </li>
    </ul>
    <div v-else class="empty">
      <span class="empty__icon"><AppIcon name="shapes" :size="18" /></span>
      <span class="empty__title">形状池为空</span>
      <span class="empty__text">在上方网格中点选格子画出形状，再点击「添加」。</span>
    </div>

    <div class="row" style="margin-top: var(--sp-3)">
      <button class="btn btn--sm btn--danger" :disabled="selectedIndex == null" @click="removeSelected">
        <AppIcon name="trash" :size="12" />删除选中
      </button>
    </div>

    <template #footer>
      <button class="btn" @click="emit('close')">取消</button>
      <button class="btn btn--primary" @click="save">保存</button>
    </template>
  </AppModal>
</template>
