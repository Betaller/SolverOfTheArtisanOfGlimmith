<script setup lang="ts">
import { computed } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useViewport } from '../composables/useViewport'
import AppIcon from './AppIcon.vue'

const store = usePuzzleStore()
const { cellSize, zoomBy, fit } = useViewport()

const size = computed(() => `${store.puzzle.grid.height}×${store.puzzle.grid.width}`)
const cells = computed(() => store.puzzle.grid.height * store.puzzle.grid.width)
const blocked = computed(() => store.puzzle.cells.filter((c) => c.blocked).length)
const regions = computed(() => (store.displayRegions ? new Set(store.displayRegions.values()).size : null))
</script>

<template>
  <div class="stage-hud stage-hud--bottom">
    <span class="chip">{{ size }}</span>
    <span class="chip">{{ cells }} 格</span>
    <span v-if="blocked" class="chip">{{ blocked }} 障碍</span>
    <span v-if="regions != null" class="chip chip--ok">{{ regions }} 区域</span>
    <span class="stage-hud__sep" />
    <button class="btn btn--icon btn--sm btn--glass" data-tip="缩小 (-)" @click="zoomBy(-6)">
      <AppIcon name="zoomOut" :size="14" />
    </button>
    <span class="stage-hud__zoom">{{ Math.round(cellSize) }}px</span>
    <button class="btn btn--icon btn--sm btn--glass" data-tip="放大 (+)" @click="zoomBy(6)">
      <AppIcon name="zoomIn" :size="14" />
    </button>
    <button class="btn btn--icon btn--sm btn--glass" data-tip="适应窗口 (F)" @click="fit()">
      <AppIcon name="fit" :size="14" />
    </button>
  </div>
</template>
