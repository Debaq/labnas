import { createContext, useContext, useState, useEffect, type ReactNode } from 'react'
import { type ThemeName, applyTheme, getThemeNames } from './themes'

interface ThemeContextValue {
  theme: ThemeName
  setTheme: (name: ThemeName) => void
  themeNames: ThemeName[]
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined)

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeName>(() => {
    const stored = localStorage.getItem('labnas-theme')
    if (stored && getThemeNames().includes(stored as ThemeName)) {
      return stored as ThemeName
    }
    return 'dracula'
  })

  useEffect(() => {
    applyTheme(theme)
  }, [theme])

  const setTheme = (name: ThemeName) => {
    setThemeState(name)
    localStorage.setItem('labnas-theme', name)
    applyTheme(name)
  }

  return (
    <ThemeContext.Provider value={{ theme, setTheme, themeNames: getThemeNames() }}>
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
