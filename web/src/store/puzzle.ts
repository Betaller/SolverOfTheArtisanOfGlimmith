import { computed, reactive, ref } from 'vue'
import { defineStore } from 'pinia'
import type { CompassJson, EdgeJson, PuzzleJson, RegionCells, RegionJson, SolutionJson } from '../lib/types'
import { cellRegionMap, emptyPuzzle, normalizePuzzle } from '../lib/model'
import { solvePuzzle, cancelSolve } from '../worker/solverClient'

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
  const showSolution = ref(false)
  const solving = ref(false)
  const solveMessage = ref('就绪')
  // Per-puzzle deadline handed to the wasm solver (overrides its 5s default).
  const solveTimeoutMs = ref(5000)

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

  // Answer is only shown after the user clicks 求解. Official answer takes
  // priority; the WASM solver fills the gap for puzzles without one.
  const displayRegions = computed<Map<string, number> | null>(() => {
    if (!showSolution.value) return null
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
  // Guards against stale results / concurrent solves (see `solve`/`cancel`).
  let solveToken = 0
  function markModified() {
    // Editing invalidates both the displayed solution and the loaded official
    // answer (the board no longer matches the canonical partition).
    solution.value = null
    showSolution.value = false
    officialAnswer.value = null
    if (undoTimer) clearTimeout(undoTimer)
    undoTimer = setTimeout(snapshot, 300)
  }

  function applySnapshot(data: PuzzleJson) {
    Object.assign(puzzle, data)
    solution.value = null
    showSolution.value = false
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
    showSolution.value = false
    clearSelection()
    undoStack.value = []
    redoStack.value = []
  }

  function loadPuzzle(p: PuzzleJson, puzzleName: string, answer: RegionCells[] | null = null) {
    Object.assign(puzzle, clone(normalizePuzzle(p)))
    name.value = puzzleName
    initialData.value = clone(puzzle)
    solution.value = null
    officialAnswer.value = answer
    showSolution.value = false
    clearSelection()
    undoStack.value = []
    redoStack.value = []
  }

  function reset() {
    if (initialData.value) applySnapshot(initialData.value)
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
    // Official answer: just reveal it (no solver run).
    if (officialAnswer.value) {
      showSolution.value = true
      solveMessage.value = `官方解 · ${officialAnswer.value.length} 个区域`
      return
    }
    if (!puzzle.rules.length) {
      solveMessage.value = '请至少启用一条规则'
      return
    }
    if (solving.value) return  // ignore re-entrant clicks; a worker is single-threaded
    solving.value = true
    // A monotonically increasing token lets a slow, superseded solve's result
    // be dropped instead of overwriting the current one.
    const token = ++solveToken
    try {
      const s = await solvePuzzle(JSON.stringify(puzzle), solveTimeoutMs.value)
      if (token !== solveToken) return  // superseded by a newer solve/cancel
      solution.value = s
      if (s.solved) {
        showSolution.value = true
        solveMessage.value = `求解成功 · ${s.elapsed_ms}ms · ${s.regions.length} 个区域`
      } else {
        solveMessage.value = `求解失败 · ${s.error_message || '无解'}`
      }
    } catch (e) {
      if (token !== solveToken) return
      solveMessage.value = `求解出错 · ${e instanceof Error ? e.message : String(e)}`
    } finally {
      if (token === solveToken) solving.value = false
    }
  }

  /** Stop the in-flight solve: terminate the Worker and rebuild it next time. */
  function cancel() {
    cancelSolve()
    solveToken++  // invalidate any in-flight result
    solving.value = false
    solveMessage.value = '求解已取消'
  }

  return {
    puzzle, name, solution, officialAnswer, showSolution, solving, solveMessage,
    solveTimeoutMs, undoStack, redoStack,
    mode, currentNumber, currentSymbol, currentCompass,
    selectedCell, selectedEdge, selectedVertex, displayRegions,
    snapshot, markModified, undo, redo, newPuzzle, loadPuzzle, reset, solve, cancel,
    clearSelection, selectCell, selectEdge, selectVertex,
  }
})
