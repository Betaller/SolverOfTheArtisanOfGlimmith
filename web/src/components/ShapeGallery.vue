<script setup lang="ts">
import { computed, ref } from 'vue'
import { enumeratePolyominoes } from '../lib/shapes'
import { useTheme } from '../composables/useTheme'
import { boardPalettes } from '../lib/theme'

const { theme } = useTheme()
const C = computed(() => boardPalettes[theme.value])

const size = ref(4)
const shapes = computed(() => enumeratePolyominoes(size.value))
function shapeW(s: [number, number][]): number { return Math.max(...s.map(([, c]) => c)) + 1 }
function shapeH(s: [number, number][]): number { return Math.max(...s.map(([r]) => r)) + 1 }
</script>

<template>
  <div class="col" style="gap: var(--sp-4)">
    <section>
      <h3 class="section-title">骨牌大小</h3>
      <div class="segmented" style="width: 100%">
        <button v-for="n in 7" :key="n" :class="{ 'is-active': size === n }" @click="size = n">{{ n }}</button>
      </div>
      <p class="hint" style="margin-top: var(--sp-2)">
        共 {{ shapes.length }} 种不同形状（旋转/翻转视为同一种）。用于「相异」「形状池」等规则。
      </p>
    </section>

    <section>
      <h3 class="section-title">形状一览</h3>
      <div class="gallery-grid">
        <div v-for="(s, i) in shapes" :key="i" class="gallery-item">
          <svg :width="52" :height="52">
            <g :transform="`translate(${26 - shapeW(s) * 6}, ${26 - shapeH(s) * 6})`">
              <rect
                v-for="(cell, j) in s" :key="j"
                :x="cell[1] * 12" :y="cell[0] * 12" width="11" height="11" :rx="1.5"
                :fill="C.shape_mini_fill" :stroke="C.shape_mini_pen" stroke-width="0.9"
              />
            </g>
          </svg>
          <span class="gallery-item__idx">{{ i + 1 }}</span>
        </div>
      </div>
    </section>
  </div>
</template>
