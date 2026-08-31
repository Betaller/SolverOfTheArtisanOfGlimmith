<script setup lang="ts">
import { computed } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useToast } from '../composables/useToast'
import AppIcon from './AppIcon.vue'

const store = usePuzzleStore()
const toast = useToast()

const ruleCount = computed(() => store.puzzle.rules.length)

const result = computed(() => {
  if (store.showSolution && store.officialAnswer) {
    return { kind: 'ok' as const, title: '官方解', meta: `${store.officialAnswer.length} 个区域` }
  }
  if (store.solution?.solved) {
    const s = store.solution
    return {
      kind: 'ok' as const,
      title: '求解成功',
      meta: `${s.solver} · ${s.elapsed_ms}ms · ${s.regions.length} 个区域`,
    }
  }
  if (store.solution) {
    return { kind: 'error' as const, title: '求解失败', meta: store.solution.error_message || '无解' }
  }
  return null
})

async function run() {
  if (store.solving) {
    store.cancel()
    toast.info('已取消求解')
    return
  }
  if (!ruleCount.value) {
    toast.error('请先启用至少一条规则')
    return
  }
  await store.solve()
  if (store.showSolution && store.officialAnswer) {
    toast.ok(`官方解 · ${store.officialAnswer.length} 个区域`)
  } else if (store.solution?.solved) {
    toast.ok(`求解成功 · ${store.solution.elapsed_ms}ms`)
  } else {
    toast.error(store.solution?.error_message || '无解')
  }
}
</script>

<template>
  <div class="solver-console">
    <button class="solve-btn" :class="{ 'is-cancel': store.solving }" @click="run">
      <span v-if="store.solving" class="solve-btn__shine" />
      <span class="solve-btn__label">
        <AppIcon :name="store.solving ? 'stop' : 'bolt'" :size="16" :stroke="2" />
        {{ store.solving ? '取消求解' : '求解' }}
      </span>
    </button>
    <div v-if="store.solving" class="solve-progress"><i /></div>

    <div class="row" style="margin-top: 10px">
      <div class="field grow">
        <span class="field__label">超时</span>
        <div class="input-wrap">
          <AppIcon name="clock" :size="13" />
          <input
            v-model.number="store.solveTimeoutMs" class="input input--num" type="number"
            min="500" max="60000" step="500" :disabled="store.solving"
          />
        </div>
      </div>
      <div class="field grow">
        <span class="field__label">规则</span>
        <div class="input-wrap">
          <AppIcon name="sliders" :size="13" />
          <span class="input" style="display: flex; align-items: center">{{ ruleCount }} 条</span>
        </div>
      </div>
    </div>

    <div v-if="result" class="result-card" :class="result.kind === 'ok' ? 'result-card--ok' : 'result-card--error'">
      <span class="result-card__icon" :style="{ color: `var(--${result.kind === 'ok' ? 'ok' : 'danger'})` }">
        <AppIcon :name="result.kind === 'ok' ? 'check' : 'alert'" :size="13" :stroke="2.4" />
      </span>
      <div class="result-card__body">
        <div class="result-card__title">{{ result.title }}</div>
        <div class="result-card__meta">{{ result.meta }}</div>
      </div>
    </div>
    <p v-else class="hint" style="margin-top: 10px">
      勾选规则后点击求解。求解在 Web Worker 中运行，不会卡住界面。
    </p>
  </div>
</template>
