import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import type { UserRole, UserPermissions, ModuleInfo } from '../types'
import { api, UNAUTHORIZED_EVENT } from '../api/client'
import { loginWithPasskey } from '../api/passkeys'

/** Lo que se guarda en localStorage: sin token (la sesion va en una cookie HttpOnly) */
interface AuthUser {
  username: string
  role: UserRole
  permissions: UserPermissions
  enabledModules: ModuleInfo[]
}

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
          const updated = {
            ...parsed,
            role: data.role,
            permissions: data.permissions,
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
      permissions: data.permissions,
      enabledModules: parseModules(data),
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
    return true
  }

  async function passkeyLogin(username?: string) {
    const data = await loginWithPasskey(username)
    const authUser: AuthUser = {
      username: data.username,
      role: data.role as UserRole,
      permissions: data.permissions as UserPermissions,
      enabledModules: parseModules(data as { enabled_modules?: unknown }),
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
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
      permissions: data.permissions,
      enabledModules: parseModules(data),
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
  }

  function logout() {
    if (user) {
      api('/api/auth/logout', { method: 'POST' }).catch(() => {})
    }
    setUser(null)
    localStorage.removeItem('labnas_auth')
  }

  function can(perm: 'terminal' | 'impresion' | 'archivos_escritura' | 'settings'): boolean {
    if (!user) return false
    if (user.role === 'admin') return true
    if (perm === 'settings') return false
    if (user.role === 'pendiente') return false
    if (user.role === 'observador') return perm === 'impresion' && user.permissions.impresion
    return user.permissions[perm] ?? false
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

  async function refreshModules() {
    if (!user) return
    try {
      const res = await api('/api/modules')
      if (res.ok) {
        const modules: ModuleInfo[] = await res.json()
        const updated = { ...user, enabledModules: modules }
        setUser(updated)
        localStorage.setItem('labnas_auth', JSON.stringify(updated))
      }
    } catch {}
  }

  return (
    <AuthContext.Provider value={{ user, loading, login, passkeyLogin, register, logout, can, isAdmin, enabledModules, isModuleEnabled, refreshModules, refreshUser }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be inside AuthProvider')
  return ctx
}
