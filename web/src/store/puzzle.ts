import { computed, reactive, ref } from 'vue'
import { defineStore } from 'pinia'
import type { CompassJson, EdgeJson, PuzzleJson, RegionCells, RegionJson, SolutionJson } from '../lib/types'
import { cellRegionMap, emptyPuzzle } from '../lib/model'
import { solvePuzzle } from '../worker/solverClient'

export interface Selection {
  cell: [number, number] | null
  edge: [number, number, number, number] | null
  vertex: [number, number] | null
}

const clone = (p: PuzzleJson): PuzzleJson => JSON.parse(JSON.stringify(p))

export const usePuzzleStore = defineStore('puzzle', () => {
  const puzzle = reactive<PuzzleJson>(emptyPuzzle(6, 6))
  const name = ref('未命名')
  const initialData = ref<PuzzleJson>(clone(puzzle))

  const solution = ref<SolutionJson | null>(null)
  const officialAnswer = ref<RegionCells[] | null>(null)
  const solving = ref(false)
  const solveMessage = ref('就绪')
  const resultHtml = ref('就绪')

  const undoStack = ref<PuzzleJson[]>([])
  const redoStack = ref<PuzzleJson[]>([])

  // Tool state (set by ToolPalette, consumed by GridCanvas).
  const mode = ref('select')
  const currentNumber = ref<number | null>(null)
  const currentSymbol = ref<string | null>(null)
  const currentCompass = ref<CompassJson | null>(null)

  // Selection (set by GridCanvas, consumed by PropertyPanel).
  const selectedCell = ref<[number, number] | null>(null)
  const selectedEdge = ref<[number, number, number, number] | null>(null)
  const selectedVertex = ref<[number, number] | null>(null)

  // Official answer takes priority; solver fills the gap.
  const displayRegions = computed<Map<string, number> | null>(() => {
    if (officialAnswer.value) return cellRegionMap(officialAnswer.value)
    if (solution.value?.solved) return cellRegionMap(solution.value.regions)
    return null
  })

  function snapshot() {
    const snap = clone(puzzle)
    if (undoStack.value.length && JSON.stringify(undoStack.value[undoStack.value.length - 1]) === JSON.stringify(snap)) return
    undoStack.value.push(snap)
    if (undoStack.value.length > 100) undoStack.value.shift()
    redoStack.value = []
  }

  let undoTimer: ReturnType<typeof setTimeout> | null = null
  function markModified() {
    if (undoTimer) clearTimeout(undoTimer)
    undoTimer = setTimeout(snapshot, 300)
  }

  function applySnapshot(data: PuzzleJson) {
    Object.assign(puzzle, data)
    solution.value = null
    officialAnswer.value = null
    clearSelection()
  }

  function undo() {
    if (!undoStack.value.length) return
    const prev = undoStack.value.pop()!
    redoStack.value.push(clone(puzzle))
    applySnapshot(prev)
    solveMessage.value = '已撤销'
  }

  function redo() {
    if (!redoStack.value.length) return
    const next = redoStack.value.pop()!
    undoStack.value.push(clone(puzzle))
    applySnapshot(next)
    solveMessage.value = '已重做'
  }

  function newPuzzle(height: number, width: number) {
    Object.assign(puzzle, emptyPuzzle(height, width))
    name.value = '未命名'
    initialData.value = clone(puzzle)
    solution.value = null
    officialAnswer.value = null
    clearSelection()
    undoStack.value = []
    redoStack.value = []
    resultHtml.value = '就绪'
  }

  function loadPuzzle(p: PuzzleJson, puzzleName: string, answer: RegionCells[] | null = null) {
    Object.assign(puzzle, clone(p))
    name.value = puzzleName
    initialData.value = clone(puzzle)
    solution.value = null
    officialAnswer.value = answer
    clearSelection()
    undoStack.value = []
    redoStack.value = []
    resultHtml.value = '已加载'
  }

  function reset() {
    if (initialData.value) applySnapshot(initialData.value)
    resultHtml.value = '已重置'
  }

  function clearSelection() {
    selectedCell.value = null
    selectedEdge.value = null
    selectedVertex.value = null
  }

  function selectCell(r: number, c: number) {
    selectedCell.value = [r, c]
    selectedEdge.value = null
    selectedVertex.value = null
  }
  function selectEdge(e: [number, number, number, number]) {
    selectedEdge.value = e
    selectedCell.value = null
    selectedVertex.value = null
  }
  function selectVertex(r: number, c: number) {
    selectedVertex.value = [r, c]
    selectedCell.value = null
    selectedEdge.value = null
  }

  async function solve() {
    if (officialAnswer.value) return // official answer already shown; never re-solve
    if (!puzzle.rules.length) {
      resultHtml.value = '请至少启用一条规则'
      return
    }
    solving.value = true
    resultHtml.value = '求解中...'
    try {
      const s = await solvePuzzle(JSON.stringify(puzzle))
      solution.value = s
      if (s.solved) {
        solveMessage.value = `求解成功! ${s.elapsed_ms}ms, ${s.regions.length}个区域`
        resultHtml.value = `求解成功!<br>耗时: ${s.elapsed_ms}ms<br>区域数: ${s.regions.length}`
      } else {
        solveMessage.value = `求解失败: ${s.error_message || '无解'}`
        resultHtml.value = `求解失败<br>原因: ${s.error_message || '无解'}`
      }
    } catch (e) {
      solveMessage.value = '求解出错'
      resultHtml.value = `出错: ${e instanceof Error ? e.message : String(e)}`
    } finally {
      solving.value = false
    }
  }

  return {
    puzzle, name, solution, officialAnswer, solving, solveMessage, resultHtml,
    mode, currentNumber, currentSymbol, currentCompass,
    selectedCell, selectedEdge, selectedVertex, displayRegions,
    snapshot, markModified, undo, redo, newPuzzle, loadPuzzle, reset, solve,
    clearSelection, selectCell, selectEdge, selectVertex,
  }
})
