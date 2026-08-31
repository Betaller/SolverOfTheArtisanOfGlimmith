<script setup lang="ts">
const props = defineProps<{ modelValue: boolean; label?: string; disabled?: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void }>()

function onChange(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).checked)
}
</script>

<template>
  <label class="switch" :class="{ 'is-disabled': props.disabled }">
    <input type="checkbox" :checked="modelValue" :disabled="props.disabled" @change="onChange" />
    <span class="switch__track"><span class="switch__knob" /></span>
    <span v-if="label || $slots.default" class="switch__label">
      <slot>{{ label }}</slot>
    </span>
  </label>
</template>

<style scoped>
.switch.is-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
