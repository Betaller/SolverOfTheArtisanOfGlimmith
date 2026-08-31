<script setup lang="ts">
import { useToast } from '../composables/useToast'
import AppIcon from './AppIcon.vue'

const { toasts, dismiss } = useToast()

const ICONS: Record<string, string> = { ok: 'check', error: 'alert', info: 'info' }
</script>

<template>
  <Teleport to="body">
    <div class="toast-host">
      <TransitionGroup name="toast">
        <div
          v-for="t in toasts"
          :key="t.id"
          class="toast"
          :class="{ 'is-leaving': t.leaving }"
          @click="dismiss(t.id)"
        >
          <span class="toast__icon" :class="`toast__icon--${t.kind}`">
            <AppIcon :name="ICONS[t.kind] ?? 'info'" :size="12" :stroke="2.4" />
          </span>
          <span class="toast__text">{{ t.text }}</span>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-move,
.toast-enter-active,
.toast-leave-active {
  transition: opacity 220ms var(--ease-out), transform 220ms var(--ease-out);
}

.toast-enter-from {
  opacity: 0;
  transform: translate3d(0, 10px, 0) scale(0.96);
}

.toast-leave-to {
  opacity: 0;
  transform: translate3d(0, 6px, 0) scale(0.96);
}

.toast {
  cursor: pointer;
}
</style>
