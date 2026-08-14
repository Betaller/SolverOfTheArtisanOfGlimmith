<script setup lang="ts">
const props = defineProps<{ gridSize?: number; modelValue: [number, number][] }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: [number, number][]): void }>()

const has = (r: number, c: number) => props.modelValue.some(([rr, cc]) => rr === r && cc === c)
function toggle(r: number, c: number) {
  const next = has(r, c)
    ? props.modelValue.filter(([rr, cc]) => !(rr === r && cc === c))
    : [...props.modelValue, [r, c] as [number, number]]
  emit('update:modelValue', next)
}
function clear() { emit('update:modelValue', []) }
</script>

<template>
  <div class="shape-grid-editor">
    <div v-for="r in (gridSize ?? 5)" :key="r" class="sg-row">
      <div
        v-for="c in (gridSize ?? 5)" :key="c"
        class="sg-cell" :class="{ on: has(r - 1, c - 1) }"
        @mousedown.prevent @click="toggle(r - 1, c - 1)"
      />
    </div>
    <div class="sg-area">格数: {{ modelValue.length }}</div>
  </div>
</template>
