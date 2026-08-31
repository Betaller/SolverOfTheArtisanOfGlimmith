<script setup lang="ts">
import { computed, ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { useToast } from '../composables/useToast'
import { RULE_NAMES, RULE_CATEGORIES, RULE_DESCRIPTIONS } from '../lib/theme'
import type { RuleJson } from '../lib/types'
import AppIcon from './AppIcon.vue'
import AppSwitch from './AppSwitch.vue'
import ShapeEditor from './ShapeEditor.vue'

const store = usePuzzleStore()
const toast = useToast()

const query = ref('')
const collapsed = ref<Record<string, boolean>>({})
const showShapeEditor = ref(false)

function hasRule(type: string) { return store.puzzle.rules.some((r) => r.type === type) }
function getRule(type: string): RuleJson | undefined { return store.puzzle.rules.find((r) => r.type === type) }

function toggleRule(type: string, checked: boolean) {
  if (checked) { if (!hasRule(type)) store.puzzle.rules.push({ type }) }
  else { store.puzzle.rules = store.puzzle.rules.filter((r) => r.type !== type) as any }
  store.markModified()
  toast.info(`${checked ? '启用' : '停用'}规则「${RULE_NAMES[type] ?? type}」`, 1400)
}
function setParam(type: string, key: string, value: any) {
  const r = getRule(type)
  if (r) { r.params = r.params ?? {}; r.params[key] = value; store.markModified() }
}
function clearAll() {
  if (!store.puzzle.rules.length) return
  store.puzzle.rules = [] as any
  store.markModified()
  toast.info('已清除全部规则')
}
function shapeCount(): number {
  return (getRule('shape_pool')?.params?.shapes as any[])?.length ?? 0
}
function saveShapes(shapes: any[]) {
  const r = getRule('shape_pool')
  if (r) r.params = { ...(r.params ?? {}), shapes }
  else store.puzzle.rules.push({ type: 'shape_pool', params: { shapes } })
  store.markModified()
}

function matches(type: string): boolean {
  const q = query.value.trim().toLowerCase()
  if (!q) return true
  return (
    type.toLowerCase().includes(q) ||
    (RULE_NAMES[type] ?? '').toLowerCase().includes(q) ||
    (RULE_DESCRIPTIONS[type] ?? '').toLowerCase().includes(q)
  )
}

const groups = computed(() =>
  RULE_CATEGORIES.map(([cat, types]) => {
    const visible = types.filter(matches)
    return { cat, types: visible, active: visible.filter(hasRule).length }
  }).filter((g) => g.types.length),
)

const totalActive = computed(() => store.puzzle.rules.length)
</script>

<template>
  <div class="col" style="gap: var(--sp-4)">
    <div class="row row--between">
      <h3 class="section-title" style="margin: 0">规则 {{ totalActive }} / 22</h3>
      <button class="btn btn--sm btn--danger" :disabled="!totalActive" @click="clearAll">
        <AppIcon name="trash" :size="12" />清除
      </button>
    </div>

    <div class="input-wrap">
      <AppIcon name="search" :size="13" />
      <input v-model="query" class="input" placeholder="搜索规则名称或说明…" />
      <button
        v-if="query" class="btn btn--icon btn--sm btn--ghost"
        style="position: absolute; right: 2px" @click="query = ''"
      ><AppIcon name="close" :size="12" /></button>
    </div>

    <section v-for="g in groups" :key="g.cat" class="rule-group">
      <button class="rule-group__head" @click="collapsed[g.cat] = !collapsed[g.cat]">
        <AppIcon name="chevronRight" :size="13" class="rule-group__chevron" :class="{ 'is-open': !collapsed[g.cat] }" />
        <span class="rule-group__title">{{ g.cat }}</span>
        <span v-if="g.active" class="chip chip--brand">{{ g.active }}</span>
      </button>

      <div v-if="!collapsed[g.cat]" class="rule-group__body">
        <div v-for="t in g.types" :key="t" class="rule-row">
          <div class="rule-row__top">
            <span class="rule-row__name">{{ RULE_NAMES[t] ?? t }}</span>
            <span class="chip" style="font-family: var(--font-mono); font-size: 10px">{{ t }}</span>
            <AppSwitch :model-value="hasRule(t)" @update:model-value="(v) => toggleRule(t, v)" />
          </div>
          <div class="rule-row__desc">{{ RULE_DESCRIPTIONS[t] ?? '' }}</div>

          <div v-if="hasRule(t) && t === 'precise'" class="rule-row__params">
            <div class="field">
              <span class="field__label">面积</span>
              <input
                class="input input--num" type="number" min="1" max="256"
                :value="getRule('precise')?.params?.area ?? 5"
                @change="setParam('precise', 'area', parseInt(($event.target as HTMLInputElement).value))"
              />
            </div>
          </div>
          <div v-if="hasRule(t) && t === 'range'" class="rule-row__params">
            <div class="field">
              <span class="field__label">最小</span>
              <input
                class="input input--num" type="number" min="1" max="256"
                :value="getRule('range')?.params?.min ?? 2"
                @change="setParam('range', 'min', parseInt(($event.target as HTMLInputElement).value))"
              />
            </div>
            <div class="field">
              <span class="field__label">最大</span>
              <input
                class="input input--num" type="number" min="1" max="256"
                :value="getRule('range')?.params?.max ?? 256"
                @change="setParam('range', 'max', parseInt(($event.target as HTMLInputElement).value))"
              />
            </div>
          </div>
          <div v-if="hasRule(t) && t === 'shape_pool'" class="rule-row__params">
            <button class="btn btn--sm" @click="showShapeEditor = true">
              <AppIcon name="shapes" :size="12" />编辑形状池
            </button>
            <span class="chip" :class="shapeCount() ? 'chip--info' : ''">{{ shapeCount() }} 个形状</span>
          </div>
        </div>
      </div>
    </section>

    <p v-if="!groups.length" class="hint" style="text-align: center; padding: var(--sp-5) 0">
      没有匹配「{{ query }}」的规则
    </p>

    <ShapeEditor
      v-if="showShapeEditor"
      :shapes="(getRule('shape_pool')?.params?.shapes as any[]) ?? []"
      @save="saveShapes($event); showShapeEditor = false"
      @close="showShapeEditor = false"
    />
  </div>
</template>
