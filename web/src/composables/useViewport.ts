import { ref } from 'vue'

export const ZOOM_MIN = 16
export const ZOOM_MAX = 120

/**
 * Board zoom state, shared by `GridCanvas` (which paints + hit-tests) and the
 * floating stage HUD (zoom buttons / fit). `cellSize` is the *target*;
 * `renderSize` is the eased value actually used for painting, so wheel/button
 * zooms glide instead of snapping.
 */
const cellSize = ref(60)
const renderSize = ref(60)
// Container + grid dimensions, published by GridCanvas so `fit()` can run from
// anywhere (toolbar button, keyboard shortcut, window resize).
const stage = ref({ w: 0, h: 0 })
const grid = ref({ h: 0, w: 0 })

let raf = 0
function animate() {
  raf = 0
  const cur = renderSize.value
  const tgt = cellSize.value
  const diff = tgt - cur
  if (Math.abs(diff) < 0.15) {
    renderSize.value = tgt
    return
  }
  renderSize.value = cur + diff * 0.28
  raf = requestAnimationFrame(animate)
}

function schedule() {
  if (!raf) raf = requestAnimationFrame(animate)
}

function clamp(v: number) {
  return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, Math.round(v)))
}

function setZoom(v: number) {
  cellSize.value = clamp(v)
  schedule()
}

function zoomBy(step: number) {
  setZoom(cellSize.value + step)
}

function publishStage(w: number, h: number) {
  stage.value = { w, h }
}

function publishGrid(h: number, w: number) {
  grid.value = { h, w }
}

/** Largest cell size that fits the whole board inside the stage, minus margin. */
function fit() {
  const { w: sw, h: sh } = stage.value
  const { h: gh, w: gw } = grid.value
  if (!sw || !sh || !gh || !gw) return
  const margin = 96 // stage padding (24×2) + sheet padding + breathing room
  setZoom(Math.min((sw - margin) / gw, (sh - margin) / gh))
}

export function useViewport() {
  return {
    cellSize,
    renderSize,
    setZoom,
    zoomBy,
    fit,
    publishStage,
    publishGrid,
    canZoomIn: () => cellSize.value < ZOOM_MAX,
    canZoomOut: () => cellSize.value > ZOOM_MIN,
  }
}
