/**
 * Global shortcuts must not fire while the user is typing into a field, so
 * both the window handler (App) and the board handler (GridCanvas) bail out
 * on editable targets.
 */
export function isEditableTarget(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null
  if (!t || !t.tagName) return false
  const tag = t.tagName.toUpperCase()
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t.isContentEditable === true
}
