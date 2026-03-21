import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from 'react'
import { type ThemeName, applyTheme, getThemeNames } from './themes'

// Tipo extendido que incluye "auto"
type ThemeSelection = ThemeName | 'auto'

interface ThemeContextValue {
  theme: ThemeSelection
  setTheme: (name: ThemeSelection) => void
  themeNames: ThemeSelection[]
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined)

// Detectar el tema del sistema operativo
function getSystemTheme(): ThemeName {
  if (typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: light)').matches) {
    return 'light'
  }
  return 'dracula'
}

// Aplicar el accent color personalizado si existe
function applyCustomAccent() {
  const customAccent = localStorage.getItem('labnas-accent-color')
  if (customAccent) {
    document.documentElement.style.setProperty('--accent', customAccent)
    // Recalcular accent-alpha y accent-hover basado en el color personalizado
    document.documentElement.style.setProperty('--accent-alpha', customAccent + '1f')
    document.documentElement.style.setProperty('--sidebar-active', customAccent)
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeSelection>(() => {
    const stored = localStorage.getItem('labnas-theme')
    if (stored === 'auto') return 'auto'
    if (stored && getThemeNames().includes(stored as ThemeName)) {
      return stored as ThemeName
    }
    return 'dracula'
  })

  const applyResolvedTheme = useCallback((selection: ThemeSelection) => {
    const resolved: ThemeName = selection === 'auto' ? getSystemTheme() : selection
    applyTheme(resolved)
    applyCustomAccent()
  }, [])

  // Aplicar tema al montar y cuando cambie
  useEffect(() => {
    applyResolvedTheme(theme)
  }, [theme, applyResolvedTheme])

  // Listener para cambios en la preferencia del sistema (solo relevante si tema es "auto")
  useEffect(() => {
    if (theme !== 'auto') return

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = () => {
      applyResolvedTheme('auto')
    }
    mediaQuery.addEventListener('change', handler)
    return () => mediaQuery.removeEventListener('change', handler)
  }, [theme, applyResolvedTheme])

  const setTheme = (name: ThemeSelection) => {
    setThemeState(name)
    localStorage.setItem('labnas-theme', name)
    applyResolvedTheme(name)
  }

  // Construir lista de temas: "auto" primero, luego los normales
  const allThemeNames: ThemeSelection[] = ['auto', ...getThemeNames()]

  return (
    <ThemeContext.Provider value={{ theme, setTheme, themeNames: allThemeNames }}>
      {children}
    </ThemeContext.Provider>
  )
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext)
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider')
  }
  return context
}
