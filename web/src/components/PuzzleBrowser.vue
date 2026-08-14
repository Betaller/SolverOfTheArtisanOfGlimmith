<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import type { BundledPuzzle } from '../lib/types'

defineOptions({ name: 'PuzzleBrowser' })

interface ManifestEntry {
  id: string
  zone: string
  category: string
  url: string
  has_answer: boolean
}

interface Manifest {
  puzzles: ManifestEntry[]
}

const store = usePuzzleStore()
const entries = ref<ManifestEntry[]>([])
const loading = ref(false)
const activeId = ref<string | null>(null)

const grouped = computed(() => {
  const zones = new Map<string, Map<string, ManifestEntry[]>>()
  for (const e of entries.value) {
    if (!zones.has(e.zone)) zones.set(e.zone, new Map())
    const cats = zones.get(e.zone)!
    if (!cats.has(e.category)) cats.set(e.category, [])
    cats.get(e.category)!.push(e)
  }
  return zones
})

function select(e: ManifestEntry) {
  activeId.value = e.id
  fetch(`${import.meta.env.BASE_URL}${e.url}`)
    .then((r) => r.json())
    .then((b: BundledPuzzle) => store.load(b))
}

onMounted(async () => {
  loading.value = true
  try {
    const r = await fetch(`${import.meta.env.BASE_URL}data/manifest.json`)
    const m: Manifest = await r.json()
    entries.value = m.puzzles
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div>
    <p v-if="loading" class="hint">加载题库…</p>
    <template v-else>
      <template v-for="[zone, cats] in grouped" :key="zone">
        <div class="zone-title">{{ zone }}</div>
        <template v-for="[cat, list] in cats" :key="cat">
          <div class="category-title">{{ cat }} ({{ list.length }})</div>
          <div
            v-for="e in list"
            :key="e.id"
            class="puzzle-item"
            :class="{ active: activeId === e.id }"
            @click="select(e)"
          >
            <span class="dot" :class="{ solver: !e.has_answer }" />
            {{ e.id }}
          </div>
        </template>
      </template>
    </template>
  </div>
</template>
