<script setup lang="ts">
import { ref } from 'vue'
import { usePuzzleStore } from '../store/puzzle'
import { RULE_NAMES, RULE_CATEGORIES, RULE_DESCRIPTIONS } from '../lib/theme'
import { RULE_DEFAULT_PARAMS, ruleWithDefaults } from '../lib/fixes'
import type { RuleJson } from '../lib/types'
import ShapeEditor from './ShapeEditor.vue'

const store = usePuzzleStore()

const showShapeEditor = ref(false)

function hasRule(type: string) { return store.puzzle.rules.some((r) => r.type === type) }
function getRule(type: string): RuleJson | undefined { return store.puzzle.rules.find((r) => r.type === type) }

function toggleRule(type: string, checked: boolean) {
  if (checked) {
    if (!hasRule(type)) store.puzzle.rules.push(ruleWithDefaults(type) as any)
  } else { store.puzzle.rules = store.puzzle.rules.filter((r) => r.type !== type) as any }
  store.markModified()
}
function setParam(type: string, key: string, value: any) {
  const r = getRule(type)
  if (r) { r.params = r.params ?? {}; r.params[key] = value; store.markModified() }
}
function clearAll() {
  store.puzzle.rules = [] as any
  store.markModified()
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
</script>

<template>
  <div class="constraint-panel">
    <div class="panel-title">规则配置</div>
    <div class="rules-scroll">
      <fieldset v-for="[cat, types] in RULE_CATEGORIES" :key="cat" class="rule-group">
        <legend>{{ cat }}</legend>
        <div v-for="t in types" :key="t" class="rule-row">
          <label class="rule-check">
            <input type="checkbox" :checked="hasRule(t)" @change="toggleRule(t, ($event.target as HTMLInputElement).checked)" />
            <span>{{ RULE_NAMES[t] }}</span>
          </label>
          <div class="rule-desc">{{ RULE_DESCRIPTIONS[t] ?? '' }}</div>
          <div v-if="hasRule(t) && t === 'precise'" class="rule-params">
            <label>面积: <input type="number" min="1" max="256" :value="getRule('precise')?.params?.area ?? 5" @change="setParam('precise', 'area', parseInt(($event.target as HTMLInputElement).value))" /></label>
          </div>
          <div v-if="hasRule(t) && t === 'range'" class="rule-params">
            <label>最小: <input type="number" min="1" max="256" :value="getRule('range')?.params?.min ?? 2" @change="setParam('range', 'min', parseInt(($event.target as HTMLInputElement).value))" /></label>
            <label>最大: <input type="number" min="1" max="256" :value="getRule('range')?.params?.max ?? 256" @change="setParam('range', 'max', parseInt(($event.target as HTMLInputElement).value))" /></label>
          </div>
          <div v-if="hasRule(t) && t === 'shape_pool'" class="rule-params">
            <button @click="showShapeEditor = true">编辑形状池...</button>
            <span>{{ shapeCount() }} 个形状</span>
          </div>
        </div>
      </fieldset>
    </div>
    <div class="panel-footer"><button @click="clearAll">全部清除</button></div>

    <ShapeEditor v-if="showShapeEditor" :shapes="(getRule('shape_pool')?.params?.shapes as any[]) ?? []" @save="saveShapes($event); showShapeEditor = false" @close="showShapeEditor = false" />
  </div>
</template>
