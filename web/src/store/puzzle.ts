import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { BundledPuzzle, SolutionJson } from '../lib/types'
import { cellRegionMap, modelFromPuzzle, type PuzzleModel } from '../lib/codec'
import { solvePuzzle } from '../worker/solverClient'

export const usePuzzleStore = defineStore('puzzle', () => {
  const bundled = ref<BundledPuzzle | null>(null)
  const solution = ref<SolutionJson | null>(null)
  const solving = ref(false)
  const error = ref<string | null>(null)

  const model = computed<PuzzleModel | null>(() =>
    bundled.value ? modelFromPuzzle(bundled.value.puzzle) : null,
  )

  // Official answer takes priority (the corpus is unique-solution); the solver
  // only runs when there is no canonical answer (user-built / non-official).
  const displayRegions = computed<Map<string, number> | null>(() => {
    if (!bundled.value) return null
    if (bundled.value.answer) return cellRegionMap(bundled.value.answer)
    if (solution.value?.solved) return cellRegionMap(solution.value.regions)
    return null
  })

  async function load(b: BundledPuzzle): Promise<void> {
    bundled.value = b
    solution.value = null
    error.value = null
    if (b.answer) return // official answer → no solving needed
    await solve()
  }

  async function solve(): Promise<void> {
    if (!bundled.value || bundled.value.answer) return
    solving.value = true
    error.value = null
    try {
      const s = await solvePuzzle(JSON.stringify(bundled.value.puzzle))
      solution.value = s
      if (!s.solved) error.value = s.error_message ?? '未解出'
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
    } finally {
      solving.value = false
    }
  }

  return { bundled, solution, solving, error, model, displayRegions, load, solve }
})
