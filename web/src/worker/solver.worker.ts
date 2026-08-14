// Solver Web Worker.
//
// Loads the wasm-bindgen `--target web` module and answers `solve` requests
// with the solution JSON string. Running the (synchronous, CPU-bound) DFS here
// keeps the main thread free to render; a runaway puzzle is bounded by the
// wasm-side `WEB_TIMEOUT_MS` deadline (see `rsolver/src/wasm.rs`).

import init, { solve } from '../wasm/rsolver.js'

interface SolveRequest {
  id: number
  puzzleJson: string
}

interface SolveResponse {
  id: number
  solutionJson?: string
  error?: string
}

let readyPromise: Promise<unknown> | null = null

function ensureReady(): Promise<unknown> {
  if (!readyPromise) readyPromise = init()
  return readyPromise
}

async function handle(req: SolveRequest) {
  const post = (msg: SolveResponse) => (self as unknown as Worker).postMessage(msg)
  try {
    await ensureReady()
    // `solve` is synchronous; the deadline inside rsolver guarantees it returns.
    const solutionJson = solve(req.puzzleJson)
    post({ id: req.id, solutionJson })
  } catch (err) {
    post({ id: req.id, error: err instanceof Error ? err.message : String(err) })
  }
}

self.onmessage = (e: MessageEvent<SolveRequest>) => {
  void handle(e.data)
}
