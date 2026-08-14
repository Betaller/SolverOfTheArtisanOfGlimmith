<script setup lang="ts">
import { ref } from 'vue'
import { normalize, canonicalKey } from '../lib/shapes'
import ShapeGridEditor from './ShapeGridEditor.vue'

const props = defineProps<{ shapes: [number, number][][] }>()
const emit = defineEmits<{ (e: 'save', shapes: [number, number][][]): void; (e: 'close'): void }>()

const shapes = ref<[number, number][][]>(props.shapes.map((s) => [...s]))
const drawing = ref<[number, number][]>([])
const selectedIndex = ref<number | null>(null)

function addShape() {
  if (!drawing.value.length) return
  const key = canonicalKey(drawing.value)
  if (shapes.value.some((s) => canonicalKey(s) === key)) return
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
  return [...s].sort((a, b) => a[0] - b[0] || a[1] - b[1]).map(([r, c]) => `(${r},${c})`).join(', ')
}
</script>

<template>
  <div class="modal-backdrop" @mousedown.self="emit('close')">
    <div class="modal">
      <h3>形状池编辑器</h3>
      <div class="shape-editor-grid">
        <ShapeGridEditor :grid-size="6" :model-value="drawing" @update:model-value="drawing = $event" />
        <div class="btn-row">
          <button @click="drawing = []">清空</button>
          <button class="primary" @click="addShape">添加到形状池</button>
        </div>
      </div>
      <h4>形状池列表</h4>
      <ul class="shape-list">
        <li v-for="(s, i) in shapes" :key="i" :class="{ selected: selectedIndex === i }" @click="selectedIndex = i">形状{{ i + 1 }} (面积={{ s.length }}): [{{ cellsDesc(s) }}]</li>
      </ul>
      <div class="btn-row">
        <button :disabled="selectedIndex == null" @click="removeSelected">删除选中</button>
      </div>
      <div class="btn-row right">
        <button class="ok" @click="emit('save', shapes)">确定</button>
        <button @click="emit('close')">取消</button>
      </div>
    </div>
  </div>
</template>
