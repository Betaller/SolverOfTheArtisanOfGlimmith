<script setup lang="ts">
import { computed } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useTheme } from '../composables/useTheme'
import { RULE_NAMES, boardPalettes } from '../lib/theme'
import type { PuzzleJson } from '../lib/types'

const store = usePuzzleStore()
const { theme } = useTheme()

const p = computed(() => store.puzzle as PuzzleJson)

function ruleLabel(r: { type: string; params?: Record<string, any> }): string {
  const name = RULE_NAMES[r.type] ?? r.type
  const pr = r.params ?? {}
  if (r.type === 'range') {
    const lo = pr.min, hi = pr.max
    if (lo != null && hi != null) return `${name} ${lo}~${hi}`
    if (lo != null) return `${name} ≥${lo}`
    if (hi != null) return `${name} ≤${hi}`
  }
  if (r.type === 'precise' && pr.area != null) return `${name} ${pr.area}`
  if (r.type === 'rose_window' && pr.symbol_types) return `${name} ${pr.symbol_types.length}种`
  return name
}

const rules = computed(() => [...new Set(p.value.rules.map((r) => ruleLabel(r)))])
const shapes = computed<any[]>(() => (p.value.rules.find((r) => r.type === 'shape_pool')?.params?.shapes as any[]) ?? [])
const regionCount = computed(() => store.displayRegions ? new Set(store.displayRegions.values()).size : null)
const C = computed(() => boardPalettes[theme.value])
</script>

<template>
  <div class="rule-summary rule-summary--float">
    <div class="row row--between">
      <span class="rule-summary__title">生效规则</span>
      <span class="chip chip--brand">{{ rules.length }}</span>
    </div>
    <div v-if="rules.length" class="rule-summary__list">
      <span v-for="r in rules" :key="r" class="chip">{{ r }}</span>
    </div>
    <p v-else class="hint">尚未启用任何规则，求解前请先在「规则」面板勾选。</p>

    <template v-if="shapes.length">
      <div class="row row--between">
        <span class="rule-summary__title">形状池</span>
        <span class="chip chip--info">{{ shapes.length }}</span>
      </div>
      <div class="rule-summary__list">
        <svg v-for="(s, i) in shapes" :key="i" width="34" height="34" class="mini-shape">
          <rect
            v-for="(cell, j) in s" :key="j"
            :x="2 + cell[1] * 7" :y="2 + cell[0] * 7" width="6" height="6" rx="1"
            :fill="C.shape_mini_fill" :stroke="C.shape_mini_pen" stroke-width="0.8"
          />
        </svg>
      </div>
    </template>

    <div v-if="regionCount != null" class="row row--between">
      <span class="rule-summary__title">区域数</span>
      <span class="chip chip--ok">{{ regionCount }}</span>
    </div>
  </div>
</template>

<style scoped>
.mini-shape {
  border: 1px solid var(--stroke-subtle);
  border-radius: var(--r-xs);
  background: var(--surface-inset);
}
</style>
