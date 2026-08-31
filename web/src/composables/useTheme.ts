import { ref, watch } from 'vue'

export type ThemeName = 'dark' | 'light'

const STORAGE_KEY = 'aog.theme'

function initial(): ThemeName {
  const saved = localStorage.getItem(STORAGE_KEY)
  if (saved === 'dark' || saved === 'light') return saved
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

const theme = ref<ThemeName>(initial())

watch(
  theme,
  (t) => {
    document.documentElement.dataset.theme = t
    localStorage.setItem(STORAGE_KEY, t)
  },
  { immediate: true },
)

/** Singleton: the whole app shares one theme state. */
export function useTheme() {
  function toggle() {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
  }
  return { theme, toggle, isDark: () => theme.value === 'dark' }
}
