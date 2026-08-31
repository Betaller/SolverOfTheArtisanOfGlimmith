// Pure helpers extracted from the Vue components so the bug fixes are
// independently unit-testable. Kept DOM-agnostic (no reliance on `HTMLElement`)
// so the same code runs under jsdom and under a plain Node test environment.

// Returns true when the keydown event originated from a text-entry control, so
// global single-letter / Ctrl+Z / Ctrl+R / F5 shortcuts should be suppressed
// and the browser's native behavior preserved. (Bug W1)
export function isTypingTarget(el: EventTarget | null): boolean {
  if (!el || typeof el !== 'object') return false
  const node = el as {
    tagName?: string
    isContentEditable?: boolean
    hasAttribute?: (n: string) => boolean
  }
  if (typeof node.tagName !== 'string') return false
  const tag = node.tagName.toLowerCase()
  if (tag === 'input' || tag === 'textarea' || tag === 'select') return true
  if (node.isContentEditable) return true
  if (typeof node.hasAttribute === 'function' && node.hasAttribute('data-no-shortcut')) return true
  return false
}

// Parse a cell/vertex numeric input. Unlike `parseInt(x) || undefined`, this
// accepts 0 and only maps genuine non-numbers to undefined. (Bug W3)
export function parseNumber(v: string): number | undefined {
  const n = parseInt(v)
  return isNaN(n) ? undefined : n
}

// Default params for parameterized rules, mirroring the input defaults shown in
// ConstraintPanel (precise area=5, range min=2/max=256). When a rule is enabled
// without editing these inputs, we still persist the implied defaults so the
// rule is not stored with an empty params object. (Bug W4)
export const RULE_DEFAULT_PARAMS: Record<string, Record<string, unknown>> = {
  precise: { area: 5 },
  range: { min: 2, max: 256 },
}

// Build a rule object with its default params applied, for toggling a
// parameterized rule ON. Returns just `{ type }` for non-parameterized rules.
export function ruleWithDefaults(type: string): { type: string; params?: Record<string, unknown> } {
  const def = RULE_DEFAULT_PARAMS[type]
  return def ? { type, params: { ...def } } : { type }
}
