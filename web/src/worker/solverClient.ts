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
export function solvePuzzle(puzzleJson: string, timeoutMs?: number): Promise<SolutionJson> {
  return new Promise((resolve) => {
    const id = nextId++
    pending.set(id, (res) => resolve(res instanceof Error ? errorToSolution(res) : res))
    // wasm-bindgen maps `Option<u64>` to `bigint | null | undefined`.
    getWorker().postMessage({
      id,
      puzzleJson,
      timeoutMs: timeoutMs === undefined ? undefined : BigInt(timeoutMs),
    })
  })
}

/**
 * Abort the in-flight solve by terminating the Worker — the synchronous wasm
 * DFS can't be interrupted in place. Every pending call is settled with a
 * "cancelled" solution so its awaiter doesn't hang forever, then the worker is
 * killed (and lazily rebuilt by the next `solvePuzzle`).
 */
export function cancelSolve(): void {
  if (!worker) return
  const cancelled: SolutionJson = {
    solved: false,
    steps_taken: 0,
    elapsed_ms: 0,
    error_message: '已取消',
    regions: [],
    rule_results: {},
    solver: '',
  }
  for (const [, resolve] of pending) resolve(cancelled)
  pending.clear()
  worker.terminate()
  worker = null
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
