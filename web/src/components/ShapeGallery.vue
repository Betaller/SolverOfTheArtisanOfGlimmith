<script setup lang="ts">
import { computed, ref } from 'vue'
import { enumeratePolyominoes } from '../lib/shapes'
import { colors } from '../lib/theme'

const size = ref(1)
const shapes = computed(() => enumeratePolyominoes(size.value))
function shapeW(s: [number, number][]): number { return Math.max(...s.map(([, c]) => c)) + 1 }
function shapeH(s: [number, number][]): number { return Math.max(...s.map(([r]) => r)) + 1 }
</script>

<template>
  <div class="shape-gallery">
    <div class="row">
      <label>大小</label>
      <select v-model.number="size">
        <option v-for="n in 7" :key="n" :value="n">{{ n }}</option>
      </select>
      <span class="muted">共 {{ shapes.length }} 种</span>
    </div>
    <p class="hint">按大小列出所有不同的形状（忽略旋转/翻转），用于“相异”等形状规则。</p>
    <div class="gallery-grid">
      <div v-for="(s, i) in shapes" :key="i" class="gallery-item">
        <svg :width="60" :height="60">
          <g :transform="`translate(${30 - shapeW(s) * 7}, ${30 - shapeH(s) * 7})`">
            <rect v-for="(cell, j) in s" :key="j" :x="cell[1] * 14" :y="cell[0] * 14" :width="13" :height="13" rx="1" :fill="colors.shape_mini_fill" :stroke="colors.shape_mini_pen" />
          </g>
        </svg>
        <span class="muted">{{ i + 1 }}</span>
      </div>
    </div>
  </div>
</template>
