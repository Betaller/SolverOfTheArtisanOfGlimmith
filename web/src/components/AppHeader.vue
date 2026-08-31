<script setup lang="ts">
import { computed } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useTheme } from '../composables/useTheme'
import AppIcon from './AppIcon.vue'

const emit = defineEmits<{ (e: 'new'): void; (e: 'help'): void }>()

const store = usePuzzleStore()
const { theme, toggle } = useTheme()

const canUndo = computed(() => store.undoStack.length > 0)
const canRedo = computed(() => store.redoStack.length > 0)
const statusKind = computed(() => {
  if (store.solving) return 'busy'
  if (store.showSolution) return 'ok'
  return 'idle'
})
</script>

<template>
  <header class="app-header">
    <div class="brand">
      <span class="brand__mark">
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <rect x="3" y="3" width="8" height="8" rx="2" fill="currentColor" />
          <rect x="13" y="3" width="8" height="8" rx="2" fill="currentColor" opacity="0.4" />
          <rect x="3" y="13" width="8" height="8" rx="2" fill="currentColor" opacity="0.4" />
          <rect x="13" y="13" width="8" height="8" rx="2" fill="currentColor" opacity="0.75" />
        </svg>
      </span>
      <span class="brand__text">
        <span class="brand__title">格里米斯的工匠</span>
        <span class="brand__sub">Puzzle Solver</span>
      </span>
    </div>

    <div class="header__divider" />

    <button class="btn btn--sm" data-tip="新建空白盘面 (Ctrl+N)" data-tip-pos="bottom" @click="emit('new')">
      <AppIcon name="plus" :size="14" />新建
    </button>
    <div class="btn-group">
      <button
        class="btn btn--icon btn--sm" :disabled="!canUndo"
        data-tip="撤销 (Ctrl+Z)" data-tip-pos="bottom" @click="store.undo()"
      ><AppIcon name="undo" :size="14" /></button>
      <button
        class="btn btn--icon btn--sm" :disabled="!canRedo"
        data-tip="重做 (Ctrl+Shift+Z)" data-tip-pos="bottom" @click="store.redo()"
      ><AppIcon name="redo" :size="14" /></button>
      <button
        class="btn btn--icon btn--sm"
        data-tip="重置到初始盘面 (Ctrl+R)" data-tip-pos="bottom" @click="store.reset()"
      ><AppIcon name="reset" :size="14" /></button>
    </div>

    <div class="header__spacer" />

    <div class="header__status">
      <span class="label truncate" style="max-width: 180px">{{ store.name }}</span>
      <span v-if="statusKind === 'busy'" class="chip chip--info">
        <AppIcon name="clock" :size="11" />求解中
      </span>
      <span v-else-if="statusKind === 'ok'" class="chip chip--ok">
        <AppIcon name="check" :size="11" />已解出
      </span>
      <span v-else class="chip">未求解</span>
    </div>

    <button
      class="btn btn--icon btn--ghost"
      :data-tip="theme === 'dark' ? '切换到浅色主题' : '切换到深色主题'"
      data-tip-pos="bottom"
      @click="toggle()"
    >
      <AppIcon :name="theme === 'dark' ? 'sun' : 'moon'" :size="15" />
    </button>
    <button class="btn btn--icon btn--ghost" data-tip="快捷键 (?)" data-tip-pos="bottom" @click="emit('help')">
      <AppIcon name="keyboard" :size="15" />
    </button>
  </header>
</template>
