<script setup lang="ts">
import type { PanelKey } from '../lib/panels'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ modelValue: PanelKey }>()
const emit = defineEmits<{
  (e: 'update:modelValue', v: PanelKey): void
  (e: 'help'): void
}>()

const items: { key: PanelKey; icon: string; label: string }[] = [
  { key: 'tools', icon: 'cursor', label: '工具' },
  { key: 'rules', icon: 'sliders', label: '规则' },
  { key: 'library', icon: 'book', label: '题库' },
  { key: 'shapes', icon: 'shapes', label: '形状' },
]
</script>

<template>
  <nav class="nav-rail">
    <button
      v-for="it in items" :key="it.key"
      class="nav-item" :class="{ 'is-active': props.modelValue === it.key }"
      :data-tip="it.label" data-tip-pos="right"
      @click="emit('update:modelValue', it.key)"
    >
      <AppIcon :name="it.icon" :size="18" class="nav-item__icon" />
      <span>{{ it.label }}</span>
    </button>

    <div class="nav-rail__foot">
      <button class="nav-item" data-tip="快捷键" data-tip-pos="right" @click="emit('help')">
        <AppIcon name="keyboard" :size="18" />
        <span>按键</span>
      </button>
    </div>
  </nav>
</template>
