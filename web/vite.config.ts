import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vitejs.dev/config/
export default defineConfig({
  // Relative base so the built site works on a GitHub Pages subpath
  // (https://<user>.github.io/<repo>/) without a hardcoded origin.
  base: './',
  plugins: [vue()],
  build: {
    // The solver worker loads the wasm-bindgen `--target web` module which uses
    // `import.meta.url`; es2022 keeps that + `WebAssembly.instantiateStreaming`
    // intact with no down-level rewrites.
    target: 'es2022',
  },
})
