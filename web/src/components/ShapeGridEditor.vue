<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'

const props = defineProps<{ gridSize?: number; modelValue: [number, number][] }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: [number, number][]): void }>()

const has = (r: number, c: number) => props.modelValue.some(([rr, cc]) => rr === r && cc === c)

// Drag-paint: the first cell decides whether the stroke draws or erases, mirroring
// how the board's block tool behaves.
const painting = ref(false)
const paintOn = ref(true)

function set(r: number, c: number, on: boolean) {
  if (has(r, c) === on) return
  const next = on
    ? [...props.modelValue, [r, c] as [number, number]]
    : props.modelValue.filter(([rr, cc]) => !(rr === r && cc === c))
  emit('update:modelValue', next)
}
function start(r: number, c: number) {
  painting.value = true
  paintOn.value = !has(r, c)
  set(r, c, paintOn.value)
}
function over(r: number, c: number) {
  if (painting.value) set(r, c, paintOn.value)
}
function stop() { painting.value = false }
function clear() { emit('update:modelValue', []) }

onMounted(() => window.addEventListener('mouseup', stop))
onUnmounted(() => window.removeEventListener('mouseup', stop))
</script>

<template>
  <div class="shape-grid-editor">
    <div
      v-for="r in (gridSize ?? 5)" :key="r" class="sg-row"
      @mousedown.prevent
    >
      <div
        v-for="c in (gridSize ?? 5)" :key="c"
        class="sg-cell" :class="{ on: has(r - 1, c - 1) }"
        @mousedown="start(r - 1, c - 1)"
        @mouseenter="over(r - 1, c - 1)"
      />
    </div>
    <div class="row row--between" style="margin-top: var(--sp-2)">
      <span class="sg-area" style="margin: 0">格数 {{ modelValue.length }}</span>
      <button class="btn btn--sm btn--ghost" :disabled="!modelValue.length" @click="clear">清空</button>
    </div>
  </div>
</template>
