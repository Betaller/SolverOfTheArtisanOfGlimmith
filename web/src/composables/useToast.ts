import { ref } from 'vue'

export type ToastKind = 'ok' | 'error' | 'info'

export interface Toast {
  id: number
  kind: ToastKind
  text: string
  leaving: boolean
}

const toasts = ref<Toast[]>([])
let nextId = 1
const timers = new Map<number, ReturnType<typeof setTimeout>>()

function dismiss(id: number) {
  const t = toasts.value.find((x) => x.id === id)
  if (!t || t.leaving) return
  t.leaving = true
  const timer = timers.get(id)
  if (timer) clearTimeout(timer)
  setTimeout(() => {
    toasts.value = toasts.value.filter((x) => x.id !== id)
    timers.delete(id)
  }, 220)
}

function push(kind: ToastKind, text: string, ttl = 2800) {
  const id = nextId++
  toasts.value.push({ id, kind, text, leaving: false })
  if (toasts.value.length > 4) dismiss(toasts.value[0].id)
  timers.set(
    id,
    setTimeout(() => dismiss(id), ttl),
  )
  return id
}

/** Singleton toast queue — mounted once by `ToastHost`. */
export function useToast() {
  return {
    toasts,
    dismiss,
    info: (text: string, ttl?: number) => push('info', text, ttl),
    ok: (text: string, ttl?: number) => push('ok', text, ttl),
    error: (text: string, ttl?: number) => push('error', text, ttl),
  }
}
