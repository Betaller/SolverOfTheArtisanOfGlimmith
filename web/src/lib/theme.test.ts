import { describe, it, expect } from 'vitest'
import { RULE_NAMES, RULE_CATEGORIES } from './theme'

describe('rule tables', () => {
  it('defines all 22 rules', () => {
    expect(Object.keys(RULE_NAMES).length).toBe(22)
  })

  it('categories cover every rule exactly once', () => {
    const flat = RULE_CATEGORIES.flatMap(([, rules]) => rules)
    expect(flat.length).toBe(22)
    expect(new Set(flat).size).toBe(22)
    expect([...flat].sort()).toEqual(Object.keys(RULE_NAMES).sort())
  })
})
