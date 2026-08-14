<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { RULE_NAMES } from '../lib/theme'
import type { BundledPuzzle } from '../lib/types'

interface Entry {
  id: string; zone: string; category: string; url: string; has_answer: boolean
  height: number; width: number; rules: string[]; blocked_count: number; has_boundaries: boolean; difficulty: number | null
}

const store = usePuzzleStore()
const entries = ref<Entry[]>([])
const loading = ref(false)
const search = ref('')
const category = ref('全部目录')
const ruleText = ref('')
const ruleMode = ref('包含全部 (与)')
const sizeFilter = ref('全部大小')
const blockedFilter = ref('全部障碍')
const boundaryFilter = ref('全部边界')
const activeId = ref<string | null>(null)
const selected = ref<Entry | null>(null)

const categories = computed(() => ['全部目录', ...new Set(entries.value.map((e) => e.category))])

function ruleMatches(tok: string, r: string): boolean {
  tok = tok.trim().toLowerCase()
  if (!tok) return false
  if (r.toLowerCase().includes(tok)) return true
  return (RULE_NAMES[r] ?? '').toLowerCase().includes(tok)
}
function matchesRules(e: Entry): boolean {
  const tokens = ruleText.value.replace(/，/g, ',').split(',').map((t) => t.trim()).filter(Boolean)
  if (!tokens.length) return true
  const matched = new Set<string>()
  for (const tok of tokens) for (const r of e.rules) if (ruleMatches(tok, r)) matched.add(r)
  if (ruleMode.value.startsWith('包含全部')) return tokens.every((tok) => e.rules.some((r) => ruleMatches(tok, r)))
  if (ruleMode.value.startsWith('包含任一')) return matched.size > 0
  return matched.size === 0
}
function matchesSize(e: Entry): boolean {
  const a = e.height * e.width
  if (sizeFilter.value.startsWith('小')) return a <= 25
  if (sizeFilter.value.startsWith('中')) return a >= 26 && a <= 64
  if (sizeFilter.value.startsWith('大')) return a > 64
  return true
}
function matchesExtra(e: Entry): boolean {
  if (blockedFilter.value === '有障碍格' && e.blocked_count === 0) return false
  if (blockedFilter.value === '无障碍格' && e.blocked_count > 0) return false
  if (boundaryFilter.value === '有预画边界' && !e.has_boundaries) return false
  if (boundaryFilter.value === '无预画边界' && e.has_boundaries) return false
  return true
}

const grouped = computed(() => {
  const map = new Map<string, Entry[]>()
  for (const e of entries.value) {
    if (category.value !== '全部目录' && e.category !== category.value) continue
    if (!matchesRules(e) || !matchesSize(e) || !matchesExtra(e)) continue
    const q = search.value.trim().toLowerCase()
    if (q && !e.id.toLowerCase().includes(q) && !e.rules.some((r) => r.toLowerCase().includes(q) || (RULE_NAMES[r] ?? '').toLowerCase().includes(q))) continue
    ;(map.get(e.category) ?? map.set(e.category, []).get(e.category)!).push(e)
  }
  return map
})

function select(e: Entry) {
  activeId.value = e.id
  selected.value = e
  fetch(`${import.meta.env.BASE_URL}${e.url}`)
    .then((r) => r.json())
    .then((b: BundledPuzzle) => store.loadPuzzle(b.puzzle, e.id, b.answer))
    .catch((err) => console.error('加载题目失败:', e.id, err))
}

onMounted(async () => {
  loading.value = true
  try {
    const r = await fetch(`${import.meta.env.BASE_URL}data/manifest.json`)
    const m = await r.json()
    entries.value = m.puzzles
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="puzzle-browser">
    <input v-model="search" class="text-input" placeholder="搜索名称/规则..." />
    <div class="row">
      <select v-model="category"><option v-for="c in categories" :key="c">{{ c }}</option></select>
    </div>
    <input v-model="ruleText" class="text-input" placeholder="规则，如: shape_pool, 围栏" />
    <div class="row">
      <select v-model="ruleMode"><option>包含全部 (与)</option><option>包含任一 (或)</option><option>排除 (非)</option></select>
    </div>
    <div class="row">
      <select v-model="sizeFilter"><option>全部大小</option><option>小 (≤25格)</option><option>中 (26~64格)</option><option>大 (>64格)</option></select>
      <select v-model="blockedFilter"><option>全部障碍</option><option>有障碍格</option><option>无障碍格</option></select>
      <select v-model="boundaryFilter"><option>全部边界</option><option>有预画边界</option><option>无预画边界</option></select>
    </div>

    <div class="browser-tree">
      <template v-for="[cat, list] in grouped" :key="cat">
        <div class="zone-title">{{ cat }} ({{ list.length }})</div>
        <div v-for="e in list" :key="e.id" class="puzzle-item" :class="{ active: activeId === e.id }" @click="select(e)">
          <span class="dot" :class="{ solver: !e.has_answer }" />
          {{ e.id }} ({{ e.height }}×{{ e.width }})
        </div>
      </template>
    </div>

    <div v-if="selected" class="preview">
      <b>{{ selected.id }}</b> {{ selected.height }}×{{ selected.width }}<span v-if="selected.difficulty"> 难度{{ selected.difficulty }}</span><span v-if="selected.blocked_count"> {{ selected.blocked_count }}障碍</span>
      <div class="muted">{{ selected.rules.map((r) => RULE_NAMES[r] ?? r).join(', ') || '无规则' }}</div>
    </div>
  </div>
</template>
