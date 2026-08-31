// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import GridCanvas from './GridCanvas.vue'
import { usePuzzleStore } from '../store/puzzle'

describe('GridCanvas inlineNumber leak (W2)', () => {
  let pinia: ReturnType<typeof createPinia>
  beforeEach(() => {
    pinia = createPinia()
    setActivePinia(pinia)
  })

  it('resets the partially-typed number when the selection moves', async () => {
    const store = usePuzzleStore()
    store.mode = 'number'
    store.selectCell(0, 0)

    const wrapper = mount(GridCanvas, { global: { plugins: [pinia] } })
    const grid = wrapper.find('.grid-wrap')

    // Type "1" into cell (0,0)
    await grid.trigger('keydown', { key: '1' })
    expect(store.puzzle.cells[0].number).toBe(1)

    // Move the selection to (0,1) — must clear the pending multi-digit buffer
    await grid.trigger('keydown', { key: 'ArrowRight' })
    expect(store.selectedCell).toEqual([0, 1])

    // Type "2" into the newly selected cell
    await grid.trigger('keydown', { key: '2' })
    // The new cell must be 2, NOT 12 (the leak), and (0,0) must be untouched.
    expect(store.puzzle.cells[1].number).toBe(2)
    expect(store.puzzle.cells[0].number).toBe(1)
  })

  it('accumulates digits within the same cell', async () => {
    const store = usePuzzleStore()
    store.mode = 'number'
    store.selectCell(2, 2)
    const wrapper = mount(GridCanvas, { global: { plugins: [pinia] } })
    const grid = wrapper.find('.grid-wrap')
    await grid.trigger('keydown', { key: '1' })
    await grid.trigger('keydown', { key: '2' })
    expect(store.puzzle.cells[2 * 6 + 2].number).toBe(12)
  })
})
