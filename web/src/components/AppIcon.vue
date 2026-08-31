<script setup lang="ts">
import { computed } from 'vue'

/**
 * Inline 24×24 stroke icon set. Everything is a path so a single `<path>`
 * loop renders any glyph and the whole set stays tree-shake-free and off-line.
 */
const PATHS: Record<string, string[]> = {
  // tools
  cursor: ['M5 3l6.4 16 2-6.4 6.4-2L5 3z'],
  pencil: ['M17 3a2.8 2.8 0 014 4L7.5 20.5 2 22l1.5-5.5L17 3z'],
  block: ['M18 6L6 18', 'M6 6l12 12'],
  hash: ['M4 9h16', 'M4 15h16', 'M10 3L8 21', 'M16 3l-2 18'],
  star: ['M12 3l2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 17.8 6.2 21l1.1-6.5L2.6 9.8l6.5-.9L12 3z'],
  compass: ['M21 12a9 9 0 11-18 0 9 9 0 0118 0z', 'm16 8-2 6-6 2 2-6 6-2z'],
  tower: ['M12 2v4', 'M5 21h14', 'M8 21l1.2-9h5.6L16 21', 'M6.8 12h10.4'],
  // chrome
  undo: ['M3 8h11a5 5 0 010 10H8', 'M3 8l4-4', 'M3 8l4 4'],
  redo: ['M21 8H10a5 5 0 000 10h6', 'M21 8l-4-4', 'M21 8l-4 4'],
  reset: ['M3 12a9 9 0 109-9 9 9 0 00-6.4 2.7L3 8', 'M3 3v5h5'],
  plus: ['M12 5v14', 'M5 12h14'],
  minus: ['M5 12h14'],
  fit: ['M8 3H5a2 2 0 00-2 2v3', 'M21 8V5a2 2 0 00-2-2h-3', 'M16 21h3a2 2 0 002-2v-3', 'M3 16v3a2 2 0 002 2h3'],
  zoomIn: ['M21 21l-4.3-4.3', 'M10.5 18a7.5 7.5 0 100-15 7.5 7.5 0 000 15z', 'M7.5 10.5h6', 'M10.5 7.5v6'],
  zoomOut: ['M21 21l-4.3-4.3', 'M10.5 18a7.5 7.5 0 100-15 7.5 7.5 0 000 15z', 'M7.5 10.5h6'],
  sun: [
    'M12 17a5 5 0 100-10 5 5 0 000 10z',
    'M12 1v2',
    'M12 21v2',
    'M4.2 4.2l1.4 1.4',
    'M18.4 18.4l1.4 1.4',
    'M1 12h2',
    'M21 12h2',
    'M4.2 19.8l1.4-1.4',
    'M18.4 5.6l1.4-1.4',
  ],
  moon: ['M21 12.8A9 9 0 1111.2 3a7 7 0 009.8 9.8z'],
  keyboard: ['M3 7h18v10H3z', 'M7 11h.01', 'M11 11h.01', 'M15 11h.01', 'M17 11h.01', 'M8 14h8'],
  search: ['M21 21l-4.3-4.3', 'M10.5 18a7.5 7.5 0 100-15 7.5 7.5 0 000 15z'],
  close: ['M18 6L6 18', 'M6 6l12 12'],
  check: ['M20 6L9 17l-5-5'],
  chevronRight: ['M9 18l6-6-6-6'],
  chevronDown: ['M6 9l6 6 6-6'],
  chevronLeft: ['M15 18l-6-6 6-6'],
  layers: ['M12 2l9 5-9 5-9-5 9-5z', 'M3 12l9 5 9-5', 'M3 17l9 5 9-5'],
  grid: ['M3 3h18v18H3z', 'M9 3v18', 'M15 3v18', 'M3 9h18', 'M3 15h18'],
  shapes: ['M3 3h8v8H3z', 'M13 13h8v8h-8z'],
  list: ['M8 6h13', 'M8 12h13', 'M8 18h13', 'M3.5 6h.01', 'M3.5 12h.01', 'M3.5 18h.01'],
  sliders: ['M4 21v-7', 'M4 10V3', 'M12 21v-9', 'M12 8V3', 'M20 21v-5', 'M20 12V3', 'M1 14h6', 'M9 8h6', 'M17 16h6'],
  play: ['M7 4l12 8-12 8V4z'],
  stop: ['M6.5 6.5h11v11h-11z'],
  info: ['M21 12a9 9 0 11-18 0 9 9 0 0118 0z', 'M12 16v-4.5', 'M12 8h.01'],
  alert: ['M10.3 3.9L2 18a2 2 0 001.7 3h16.6a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z', 'M12 9v4', 'M12 17h.01'],
  filter: ['M22 3H2l8 9.5V19l4 2v-8.5L22 3z'],
  sparkles: [
    'M12 3l1.9 4.6L18.5 9l-4.6 1.9L12 15.5l-1.9-4.6L5.5 9l4.6-1.4L12 3z',
    'M19 15l.9 2.1 2.1.9-2.1.9L19 21l-.9-2.1-2.1-.9 2.1-.9L19 15z',
  ],
  target: ['M21 12a9 9 0 11-18 0 9 9 0 0118 0z', 'M16 12a4 4 0 11-8 0 4 4 0 018 0z', 'M12 12h.01'],
  help: ['M21 12a9 9 0 11-18 0 9 9 0 0118 0z', 'M9.2 9.2a3 3 0 015.6 1.2c0 2-2.8 2.6-2.8 4', 'M12 17.5h.01'],
  trash: ['M3 6h18', 'M8 6V4h8v2', 'M6 6l1 14h10l1-14'],
  eye: ['M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z', 'M15 12a3 3 0 11-6 0 3 3 0 016 0z'],
  eyeOff: ['M10.6 6.2A9.9 9.9 0 0112 5c6.4 0 10 7 10 7a18 18 0 01-2.4 3.3', 'M6.2 6.6A18 18 0 002 12s3.6 7 10 7a9.7 9.7 0 004.2-.9', 'M3 3l18 18', 'M9.9 9.9a3 3 0 004.2 4.2'],
  bolt: ['M13 2L4 14h7l-1 8 9-12h-7l1-8z'],
  clock: ['M21 12a9 9 0 11-18 0 9 9 0 0118 0z', 'M12 7.5V12l3.2 2'],
  panelLeft: ['M3 3h18v18H3z', 'M9 3v18'],
  cursorText: ['M9 4h6', 'M12 4v16', 'M8 20h8'],
  dice: ['M4 4h16v16H4z', 'M9 9h.01', 'M15 15h.01', 'M12 12h.01'],
  book: ['M4 4h7v16H4z', 'M13 4h7v16h-7z', 'M11 4v16'],
}

const props = withDefaults(defineProps<{ name: string; size?: number | string; stroke?: number }>(), {
  size: 16,
  stroke: 1.7,
})

const paths = computed(() => PATHS[props.name] ?? [])
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="stroke"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <path v-for="(d, i) in paths" :key="i" :d="d" />
  </svg>
</template>
