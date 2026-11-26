import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import type { UserRole, ModuleInfo, ModuleLevel } from '../types'
import { api, UNAUTHORIZED_EVENT } from '../api/client'
import { loginWithPasskey } from '../api/passkeys'

/** Lo que se guarda en localStorage: sin token (la sesion va en una cookie HttpOnly) */
interface AuthUser {
  username: string
  role: UserRole
  /** Nombre visible del rol */
  roleName?: string
  /** Nivel en cada modulo (rol + excepciones de la persona) */
  levels?: Record<string, ModuleLevel>
  /** Fecha de termino de la cuenta (unix) */
  expiresAt?: number | null
  enabledModules: ModuleInfo[]
}

const LEVEL_ORDER: Record<ModuleLevel, number> = { ninguno: 0, ver: 1, usar: 2, gestionar: 3 }

interface AuthContextType {
  user: AuthUser | null
  loading: boolean
  /** false: la cuenta tiene doble factor y falta el codigo */
  login: (username: string, password: string, code?: string) => Promise<boolean>
  /** Entrar con una passkey (huella, Face ID, llave) */
  passkeyLogin: (username?: string) => Promise<void>
  register: (username: string, password: string) => Promise<void>
  logout: () => void
  can: (perm: 'terminal' | 'impresion' | 'archivos_escritura' | 'settings') => boolean
  /** Tiene al menos `level` en el modulo `module` (el admin, siempre) */
  canModule: (module: string, level: ModuleLevel) => boolean
  isAdmin: boolean
  enabledModules: ModuleInfo[]
  isModuleEnabled: (id: string) => boolean
  refreshModules: () => Promise<void>
  /** Recarga rol, permisos y modulos desde el servidor */
  refreshUser: () => Promise<void>
}

const AuthContext = createContext<AuthContextType | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(null)
  // Sesion guardada: si hay una, se valida contra el servidor antes de mostrar la app
  const [saved] = useState<AuthUser | null>(() => {
    try {
      const raw = localStorage.getItem('labnas_auth')
      if (!raw) return null
      const parsed = JSON.parse(raw) as AuthUser & { token?: string }
      // Formato viejo (con token en localStorage): se descarta; hay que volver a entrar
      if (parsed.token) {
        localStorage.removeItem('labnas_auth')
        return null
      }
      if (!parsed.enabledModules) parsed.enabledModules = []
      return parsed
    } catch {
      localStorage.removeItem('labnas_auth')
      return null
    }
  })
  const [loading, setLoading] = useState(saved !== null)

  function parseModules(data: { enabled_modules?: unknown } | null | undefined): ModuleInfo[] {
    if (Array.isArray(data?.enabled_modules)) return data.enabled_modules
    return []
  }

  function refreshFromServer(parsed: AuthUser) {
    return api('/api/auth/me').then(res => {
      if (res.ok) {
        return res.json().then(data => {
          const updated: AuthUser = {
            ...parsed,
            role: data.role,
            roleName: data.role_name,
            levels: data.levels,
            expiresAt: data.expires_at ?? null,
            enabledModules: parseModules(data),
          }
          setUser(updated)
          localStorage.setItem('labnas_auth', JSON.stringify(updated))
        })
      } else {
        setUser(null)
        localStorage.removeItem('labnas_auth')
      }
    })
  }

  // Initial load
  useEffect(() => {
    if (!saved) return
    refreshFromServer(saved)
      .catch(() => localStorage.removeItem('labnas_auth'))
      .finally(() => setLoading(false))
    // Solo al montar
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Poll for role/permission/module changes every 30s
  useEffect(() => {
    const interval = setInterval(() => {
      const saved = localStorage.getItem('labnas_auth')
      if (saved) {
        try {
          const parsed = JSON.parse(saved) as AuthUser
          if (!parsed.enabledModules) parsed.enabledModules = []
          refreshFromServer(parsed).catch(() => {})
        } catch {}
      }
    }, 30000)
    return () => clearInterval(interval)
  }, [])

  // Cualquier 401 de la API (sesion vencida o revocada) vuelve al login
  useEffect(() => {
    const onUnauthorized = () => {
      setUser(null)
      localStorage.removeItem('labnas_auth')
    }
    window.addEventListener(UNAUTHORIZED_EVENT, onUnauthorized)
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, onUnauthorized)
  }, [])

  async function login(username: string, password: string, code?: string): Promise<boolean> {
    const res = await api('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password, code }),
    })
    if (!res.ok) {
      const text = await res.text()
      throw new Error(text || 'Error al iniciar sesion')
    }
    const data = await res.json()
    if (data.totp_required) return false
    const authUser: AuthUser = {
      username: data.username,
      role: data.role,
      enabledModules: parseModules(data),
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
    // Niveles por modulo y nombre del rol
    await refreshFromServer(authUser).catch(() => {})
    return true
  }

  async function passkeyLogin(username?: string) {
    const data = await loginWithPasskey(username)
    const authUser: AuthUser = {
      username: data.username,
      role: data.role as UserRole,
      enabledModules: parseModules(data as { enabled_modules?: unknown }),
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
    await refreshFromServer(authUser).catch(() => {})
  }

  async function register(username: string, password: string) {
    const res = await api('/api/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    })
    if (!res.ok) {
      const text = await res.text()
      throw new Error(text || 'Error al registrar')
    }
    const data = await res.json()
    const authUser: AuthUser = {
      username: data.username,
      role: data.role,
      enabledModules: parseModules(data),
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
    await refreshFromServer(authUser).catch(() => {})
  }

  function logout() {
    if (user) {
      api('/api/auth/logout', { method: 'POST' }).catch(() => {})
    }
    setUser(null)
    localStorage.removeItem('labnas_auth')
  }

  function canModule(module: string, level: ModuleLevel): boolean {
    if (!user) return false
    if (user.role === 'admin') return true
    if (user.role === 'pendiente') return false
    if (module === 'dashboard') return true
    const have = user.levels?.[module] ?? 'ninguno'
    return LEVEL_ORDER[have] >= LEVEL_ORDER[level]
  }

  /** Atajos de los permisos de antes, hoy niveles en su modulo */
  function can(perm: 'terminal' | 'impresion' | 'archivos_escritura' | 'settings'): boolean {
    if (!user) return false
    if (perm === 'settings') return user.role === 'admin'
    if (perm === 'terminal') return canModule('terminal', 'usar')
    if (perm === 'impresion') return canModule('printing', 'usar')
    return canModule('files', 'usar')
  }

  const isAdmin = user?.role === 'admin'
  const enabledModules = user?.enabledModules ?? []

  function isModuleEnabled(id: string): boolean {
    if (id === 'dashboard') return true
    const mod = enabledModules.find(m => m.id === id)
    return mod?.enabled ?? false
  }

  async function refreshUser() {
    if (!user) return
    await refreshFromServer(user).catch(() => {})
  }

  /** /api/auth/me ya trae los modulos filtrados por lo que el rol puede ver */
  async function refreshModules() {
    if (!user) return
    await refreshFromServer(user).catch(() => {})
  }

  return (
    <AuthContext.Provider value={{ user, loading, login, passkeyLogin, register, logout, can, canModule, isAdmin, enabledModules, isModuleEnabled, refreshModules, refreshUser }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be inside AuthProvider')
  return ctx
}
