// Main-thread wrapper over the solver Worker — a small promise API so the Vue
// store can `await solve(puzzleJson)` without touching `postMessage` directly.

import type { SolutionJson } from '../lib/types'

let worker: Worker | null = null
let nextId = 1
const pending = new Map<number, (res: SolutionJson | Error) => void>()

function getWorker(): Worker {
  if (!worker) {
    worker = new Worker(new URL('./solver.worker.ts', import.meta.url), {
      type: 'module',
    })
    worker.onmessage = (e: MessageEvent) => {
      const { id, solutionJson, error } = e.data as {
        id: number
        solutionJson?: string
        error?: string
      }
      const resolve = pending.get(id)
      if (!resolve) return
      pending.delete(id)
      if (error) resolve(new Error(error))
      else resolve(JSON.parse(solutionJson!) as SolutionJson)
    }
    worker.onerror = (e) => {
      // A worker-level error (e.g. wasm load failure) rejects all outstanding calls.
      const err = new Error(e.message || 'solver worker failed')
      for (const [, resolve] of pending) resolve(err)
      pending.clear()
    }
  }
  return worker
}

/** Solve one puzzle (JSON text in, parsed solution out). Never throws. */
export function solvePuzzle(puzzleJson: string): Promise<SolutionJson> {
  return new Promise((resolve) => {
    const id = nextId++
    pending.set(id, (res) => resolve(res instanceof Error ? errorToSolution(res) : res))
    getWorker().postMessage({ id, puzzleJson })
  })
}

function errorToSolution(err: Error): SolutionJson {
  return {
    solved: false,
    steps_taken: 0,
    elapsed_ms: 0,
    error_message: err.message,
    regions: [],
    rule_results: {},
    solver: '',
  }
}

/** Tear the worker down (optional; mainly for HMR cleanliness). */
export function disposeWorker(): void {
  worker?.terminate()
  worker = null
  pending.clear()
}
