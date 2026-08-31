<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import AppIcon from './AppIcon.vue'

const props = withDefaults(
  defineProps<{ title: string; subtitle?: string; narrow?: boolean; width?: number }>(),
  { narrow: false },
)
const emit = defineEmits<{ (e: 'close'): void }>()

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    emit('close')
  }
}

onMounted(() => window.addEventListener('keydown', onKey, true))
onUnmounted(() => window.removeEventListener('keydown', onKey, true))
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @mousedown.self="emit('close')">
      <div
        class="modal"
        :class="{ 'modal--narrow': narrow }"
        :style="width ? { width: `min(${width}px, 100%)` } : undefined"
        role="dialog"
        aria-modal="true"
        @contextmenu.prevent
      >
        <div class="modal__head">
          <slot name="icon" />
          <div class="grow">
            <div class="modal__title">{{ title }}</div>
            <div v-if="subtitle" class="hint">{{ subtitle }}</div>
          </div>
          <button class="btn btn--icon btn--ghost" data-tip="关闭 (Esc)" data-tip-pos="bottom" @click="emit('close')">
            <AppIcon name="close" :size="15" />
          </button>
        </div>
        <div class="modal__body">
          <slot />
        </div>
        <div v-if="$slots.footer" class="modal__foot">
          <slot name="footer" />
        </div>
      </div>
    </div>
  </Teleport>
</template>
