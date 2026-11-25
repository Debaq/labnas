import { useEffect, useRef } from 'react'
import { useAuth } from '../auth/AuthContext'
import { fetchPreferences, savePreferences } from '../api/preferences'
import { useTheme } from './ThemeContext'

/**
 * Guarda la piel en la cuenta: al entrar trae la elegida (en otro equipo o navegador)
 * y cada cambio se guarda en el servidor. La usa tambien la bandeja (labnas-tray).
 */
export default function ThemeSync() {
  const { user } = useAuth()
  const { theme, setTheme } = useTheme()
  // ultima piel conocida en el servidor (null = aun no se consulto)
  const synced = useRef<string | null>(null)
  const username = user?.username

  useEffect(() => {
    synced.current = null
    if (!username) return
    let cancelled = false
    fetchPreferences()
      .then(p => {
        if (cancelled) return
        if (p.theme && p.theme !== theme) {
          synced.current = p.theme
          setTheme(p.theme as Parameters<typeof setTheme>[0])
        } else {
          synced.current = theme
          // primera vez: se guarda la que ya tenia este navegador
          if (!p.theme) savePreferences({ theme }).catch(() => {})
        }
      })
      .catch(() => {})
    return () => { cancelled = true }
    // solo al cambiar de cuenta
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [username])

  useEffect(() => {
    if (!username || synced.current === null || synced.current === theme) return
    synced.current = theme
    savePreferences({ theme }).catch(() => {})
  }, [theme, username])

  return null
}
