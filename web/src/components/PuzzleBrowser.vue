<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useToast } from '../composables/useToast'
import { RULE_NAMES } from '../lib/theme'
import type { BundledPuzzle } from '../lib/types'
import AppIcon from './AppIcon.vue'

interface Entry {
  id: string; zone: string; category: string; url: string; has_answer: boolean
  height: number; width: number; rules: string[]; blocked_count: number; has_boundaries: boolean; difficulty: number | null
}

const store = usePuzzleStore()
const toast = useToast()

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
const loadingId = ref<string | null>(null)

const SIZE_OPTIONS = ['全部大小', '小 (≤25格)', '中 (26~64格)', '大 (>64格)']
const BLOCKED_OPTIONS = ['全部障碍', '有障碍格', '无障碍格']
const BOUNDARY_OPTIONS = ['全部边界', '有预画边界', '无预画边界']

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

const total = computed(() => [...grouped.value.values()].reduce((n, l) => n + l.length, 0))

async function select(e: Entry) {
  activeId.value = e.id
  selected.value = e
  loadingId.value = e.id
  try {
    const res = await fetch(`${import.meta.env.BASE_URL}${e.url}`)
    const b: BundledPuzzle = await res.json()
    store.loadPuzzle(b.puzzle, e.id, b.answer)
    toast.ok(`已加载 ${e.id}`)
  } catch (err) {
    console.error('加载题目失败:', e.id, err)
    toast.error(`加载失败：${e.id}`)
  } finally {
    loadingId.value = null
  }
}

function resetFilters() {
  search.value = ''
  ruleText.value = ''
  category.value = '全部目录'
  sizeFilter.value = '全部大小'
  blockedFilter.value = '全部障碍'
  boundaryFilter.value = '全部边界'
}

onMounted(async () => {
  loading.value = true
  try {
    const r = await fetch(`${import.meta.env.BASE_URL}data/manifest.json`)
    const m = await r.json()
    entries.value = m.puzzles
  } catch (err) {
    console.error('加载题库清单失败:', err)
    toast.error('题库清单加载失败')
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="puzzle-browser">
    <div class="browser-filters">
      <div class="input-wrap">
        <AppIcon name="search" :size="13" />
        <input v-model="search" class="input" placeholder="搜索名称或规则…" />
        <button
          v-if="search" class="btn btn--icon btn--sm btn--ghost"
          style="position: absolute; right: 2px" @click="search = ''"
        ><AppIcon name="close" :size="12" /></button>
      </div>
      <div class="input-wrap" style="margin-top: var(--sp-2)">
        <AppIcon name="filter" :size="13" />
        <input v-model="ruleText" class="input" placeholder="规则，如: shape_pool, 围栏" />
      </div>
      <div class="row" style="margin-top: var(--sp-2)">
        <select v-model="ruleMode" class="select" style="flex: 2">
          <option>包含全部 (与)</option><option>包含任一 (或)</option><option>排除 (非)</option>
        </select>
        <select v-model="sizeFilter" class="select" style="flex: 2">
          <option v-for="o in SIZE_OPTIONS" :key="o">{{ o }}</option>
        </select>
      </div>
      <div class="row">
        <select v-model="blockedFilter" class="select" style="flex: 1">
          <option v-for="o in BLOCKED_OPTIONS" :key="o">{{ o }}</option>
        </select>
        <select v-model="boundaryFilter" class="select" style="flex: 1">
          <option v-for="o in BOUNDARY_OPTIONS" :key="o">{{ o }}</option>
        </select>
      </div>
      <div class="filter-chips">
        <button
          v-for="c in categories" :key="c"
          class="filter-chip" :class="{ 'is-active': category === c }"
          @click="category = c"
        >{{ c }}</button>
      </div>
      <div class="row row--between">
        <span class="muted">{{ loading ? '载入中…' : `${total} 个题目` }}</span>
        <button class="btn btn--sm btn--ghost" @click="resetFilters">重置筛选</button>
      </div>
    </div>

    <div class="scroll-y">
      <div v-if="loading" style="padding: var(--sp-3) var(--sp-4)">
        <div v-for="n in 8" :key="n" class="skeleton" style="height: 22px; margin-bottom: 6px" />
      </div>

      <template v-else-if="total">
        <template v-for="[cat, list] in grouped" :key="cat">
          <div class="zone-title">
            <AppIcon name="layers" :size="11" />{{ cat }}
            <span class="chip">{{ list.length }}</span>
          </div>
          <div
            v-for="e in list" :key="e.id"
            class="puzzle-item" :class="{ 'is-active': activeId === e.id }"
            @click="select(e)"
          >
            <span v-if="loadingId === e.id" class="chip chip--info" style="height: 16px">…</span>
            <span v-else class="dot" :class="{ 'dot--solver': !e.has_answer }" />
            <span class="puzzle-item__name">{{ e.id }}</span>
            <span class="puzzle-item__meta">{{ e.height }}×{{ e.width }}</span>
          </div>
        </template>
      </template>

      <div v-else class="empty">
        <span class="empty__icon"><AppIcon name="search" :size="18" /></span>
        <span class="empty__title">没有匹配的题目</span>
        <span class="empty__text">试着放宽筛选条件，或点击「重置筛选」。</span>
      </div>
    </div>

    <div v-if="selected && !loading" class="browser-preview">
      <div class="browser-preview__title">
        <AppIcon name="book" :size="13" />
        <span class="truncate">{{ selected.id }}</span>
        <span class="chip">{{ selected.height }}×{{ selected.width }}</span>
      </div>
      <div class="row row--wrap" style="margin-top: var(--sp-2)">
        <span v-if="selected.difficulty" class="chip chip--warn">难度 {{ selected.difficulty }}</span>
        <span v-if="selected.blocked_count" class="chip">{{ selected.blocked_count }} 障碍</span>
        <span v-if="selected.has_boundaries" class="chip">预画边界</span>
        <span class="chip" :class="selected.has_answer ? 'chip--ok' : 'chip--info'">
          {{ selected.has_answer ? '有官方解' : '待求解' }}
        </span>
      </div>
      <div class="muted" style="margin-top: var(--sp-2)">
        {{ selected.rules.map((r) => RULE_NAMES[r] ?? r).join(' · ') || '无规则' }}
      </div>
    </div>
  </div>
</template>
