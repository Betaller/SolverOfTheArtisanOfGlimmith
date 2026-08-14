/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}

// wasm-pack (`--target web`) emits `rsolver.d.ts` alongside `rsolver.js`, which
// takes precedence when present. This fallback keeps `vue-tsc` happy before the
// wasm build has run (`src/wasm/` is gitignored / generated).
declare module '*/wasm/rsolver.js' {
  export function solve(puzzle_json: string): string
  export default function init(input?: string | URL | Request): Promise<unknown>
}
