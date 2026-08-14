<script setup lang="ts">
import { usePuzzleStore } from './store/puzzle'
import GridBoard from './components/GridBoard.vue'
import PuzzleBrowser from './components/PuzzleBrowser.vue'

const store = usePuzzleStore()
</script>

<template>
  <div class="app">
    <aside class="sidebar">
      <PuzzleBrowser />
    </aside>
    <main class="stage">
      <header class="toolbar">
        <h1>格里米斯的工匠 · 求解器</h1>
        <div v-if="store.bundled" class="status">
          <span v-if="store.bundled.answer" class="badge official">官方解</span>
          <span v-else-if="store.solving" class="badge">求解中…</span>
          <span v-else-if="store.solution?.solved" class="badge ok">
            {{ store.solution.solver }} · {{ store.solution.elapsed_ms }}ms
          </span>
          <span v-else class="badge err">{{ store.error ?? '未解' }}</span>
        </div>
      </header>
      <section class="board">
        <GridBoard
          v-if="store.model"
          :model="store.model"
          :regions="store.displayRegions"
        />
        <p v-else class="hint">从左侧选择一个题目（官方题直接显示官方解，其余用求解器）。</p>
      </section>
    </main>
  </div>
</template>
