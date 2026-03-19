import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import type { UserRole, UserPermissions } from '../types'

interface AuthUser {
  token: string
  username: string
  role: UserRole
  permissions: UserPermissions
}

interface AuthContextType {
  user: AuthUser | null
  loading: boolean
  login: (username: string, password: string) => Promise<void>
  register: (username: string, password: string) => Promise<void>
  logout: () => void
  can: (perm: 'terminal' | 'impresion' | 'archivos_escritura' | 'settings') => boolean
  isAdmin: boolean
}

const AuthContext = createContext<AuthContextType | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [loading, setLoading] = useState(true)

  function refreshFromServer(token: string, parsed: AuthUser) {
    return fetch('/api/auth/me', {
      headers: { Authorization: `Bearer ${token}` },
    }).then(res => {
      if (res.ok) {
        return res.json().then(data => {
          const updated = { ...parsed, role: data.role, permissions: data.permissions }
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
    const saved = localStorage.getItem('labnas_auth')
    if (saved) {
      try {
        const parsed = JSON.parse(saved) as AuthUser
        refreshFromServer(parsed.token, parsed)
          .catch(() => localStorage.removeItem('labnas_auth'))
          .finally(() => setLoading(false))
      } catch {
        localStorage.removeItem('labnas_auth')
        setLoading(false)
      }
    } else {
      setLoading(false)
    }
  }, [])

  // Poll for role/permission changes every 30s
  useEffect(() => {
    const interval = setInterval(() => {
      const saved = localStorage.getItem('labnas_auth')
      if (saved) {
        try {
          const parsed = JSON.parse(saved) as AuthUser
          refreshFromServer(parsed.token, parsed).catch(() => {})
        } catch {}
      }
    }, 30000)
    return () => clearInterval(interval)
  }, [])

  async function login(username: string, password: string) {
    const res = await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    })
    if (!res.ok) {
      const text = await res.text()
      throw new Error(text || 'Error al iniciar sesion')
    }
    const data = await res.json()
    const authUser: AuthUser = {
      token: data.token,
      username: data.username,
      role: data.role,
      permissions: data.permissions,
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
  }

  async function register(username: string, password: string) {
    const res = await fetch('/api/auth/register', {
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
      token: data.token,
      username: data.username,
      role: data.role,
      permissions: data.permissions,
    }
    setUser(authUser)
    localStorage.setItem('labnas_auth', JSON.stringify(authUser))
  }

  function logout() {
    if (user) {
      fetch('/api/auth/logout', {
        method: 'POST',
        headers: { Authorization: `Bearer ${user.token}` },
      }).catch(() => {})
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

  return (
    <AuthContext.Provider value={{ user, loading, login, register, logout, can, isAdmin }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be inside AuthProvider')
  return ctx
}
