import { describe, it, expect } from 'vitest'
import { isTypingTarget, parseNumber, ruleWithDefaults, RULE_DEFAULT_PARAMS } from './fixes'

describe('isTypingTarget (W1)', () => {
  it('ignores text-entry controls', () => {
    expect(isTypingTarget({ tagName: 'INPUT' } as any)).toBe(true)
    expect(isTypingTarget({ tagName: 'TEXTAREA' } as any)).toBe(true)
    expect(isTypingTarget({ tagName: 'SELECT' } as any)).toBe(true)
  })
  it('ignores contenteditable and data-no-shortcut elements', () => {
    expect(isTypingTarget({ tagName: 'DIV', isContentEditable: true } as any)).toBe(true)
    expect(isTypingTarget({ tagName: 'DIV', hasAttribute: (n: string) => n === 'data-no-shortcut' } as any)).toBe(true)
  })
  it('does not ignore canvas/body', () => {
    expect(isTypingTarget({ tagName: 'CANVAS' } as any)).toBe(false)
    expect(isTypingTarget({ tagName: 'BODY' } as any)).toBe(false)
  })
  it('handles null / non-element targets', () => {
    expect(isTypingTarget(null)).toBe(false)
    expect(isTypingTarget('string' as any)).toBe(false)
  })
})

describe('parseNumber (W3)', () => {
  it('accepts 0 (the previously-dropped value)', () => {
    expect(parseNumber('0')).toBe(0)
  })
  it('parses positive numbers', () => {
    expect(parseNumber('12')).toBe(12)
    expect(parseNumber('5')).toBe(5)
  })
  it('returns undefined for non-numbers (NaN)', () => {
    expect(parseNumber('abc')).toBeUndefined()
    expect(parseNumber('')).toBeUndefined()
  })
})

describe('ruleWithDefaults (W4)', () => {
  it('precise rule carries default area', () => {
    expect(ruleWithDefaults('precise')).toEqual({ type: 'precise', params: { area: 5 } })
  })
  it('range rule carries default min/max', () => {
    expect(ruleWithDefaults('range')).toEqual({ type: 'range', params: { min: 2, max: 256 } })
  })
  it('non-parameterized rule has no params', () => {
    const r = ruleWithDefaults('shape_pool')
    expect(r.type).toBe('shape_pool')
    expect(r.params).toBeUndefined()
  })
  it('default param table matches the UI-implied values', () => {
    expect(RULE_DEFAULT_PARAMS.precise).toEqual({ area: 5 })
    expect(RULE_DEFAULT_PARAMS.range).toEqual({ min: 2, max: 256 })
  })
})
