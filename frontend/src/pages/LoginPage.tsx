import { useState, useEffect } from 'react'
import { Loader2, LogIn, UserPlus } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { getBranding } from '../api'

export default function LoginPage() {
  const { login, register } = useAuth()
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  // Branding
  const [labName, setLabName] = useState('LabNAS')
  const [institution, setInstitution] = useState('')
  const [logoUrl, setLogoUrl] = useState('')
  const [hasUsers, setHasUsers] = useState(true)

  useEffect(() => {
    getBranding().then(b => {
      if (b.lab_name) setLabName(b.lab_name)
      if (b.institution) setInstitution(b.institution)
      if (b.logo_url) {
        setLogoUrl(b.logo_url)
        // Guardar en localStorage para el loading spinner
        localStorage.setItem('labnas_logo_url', b.logo_url)
      }
    }).catch(() => {})

    // Intentar login con credenciales inexistentes para ver si hay usuarios
    // Si la respuesta es un error especifico, podemos deducir si hay usuarios
    // Forma mas simple: intentar registrar y ver el error, pero mejor no.
    // Usamos un endpoint publico para verificar si hay usuarios
    fetch('/api/auth/has-users').then(res => {
      if (res.ok) return res.json()
      // Si el endpoint no existe, asumimos que hay usuarios (no mostramos el hint)
      return { has_users: true }
    }).then(data => {
      setHasUsers(data.has_users !== false)
    }).catch(() => {
      setHasUsers(true)
    })
  }, [])

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!username.trim() || !password) return
    setLoading(true)
    setError(null)
    try {
      if (mode === 'login') {
        await login(username, password)
      } else {
        await register(username, password)
      }
    } catch (err: any) {
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-4" style={{ backgroundColor: 'var(--bg-primary)' }}>
      <div
        className="rounded-2xl p-8 w-full max-w-sm shadow-xl"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        {/* Logo */}
        {logoUrl && (
          <div className="flex justify-center mb-4">
            <img src={logoUrl} alt={labName} className="w-16 h-16 rounded-xl object-contain" />
          </div>
        )}

        <h1 className="text-2xl font-bold text-center mb-1" style={{ color: 'var(--accent)' }}>{labName}</h1>
        {institution && (
          <p className="text-xs text-center mb-1" style={{ color: 'var(--text-secondary)' }}>
            {institution}
          </p>
        )}
        <p className="text-xs text-center mb-6" style={{ color: 'var(--text-secondary)' }}>
          {mode === 'login' ? 'Inicia sesion para acceder' : 'Crea una cuenta'}
        </p>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Usuario</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="usuario"
              autoFocus
              className="w-full px-3 py-2.5 rounded-lg text-sm outline-none"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
            />
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Contrasena</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="****"
              className="w-full px-3 py-2.5 rounded-lg text-sm outline-none"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
            />
          </div>

          {error && (
            <div className="text-xs rounded-lg p-3" style={{ backgroundColor: 'var(--danger)' + '15', color: 'var(--danger)' }}>
              {error}
            </div>
          )}

          <button
            type="submit"
            disabled={loading || !username.trim() || !password}
            className="w-full flex items-center justify-center gap-2 py-2.5 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
            style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
          >
            {loading ? <Loader2 size={16} className="animate-spin" /> : mode === 'login' ? <LogIn size={16} /> : <UserPlus size={16} />}
            {mode === 'login' ? 'Entrar' : 'Crear cuenta'}
          </button>
        </form>

        <div className="mt-4 text-center">
          <button
            onClick={() => { setMode(mode === 'login' ? 'register' : 'login'); setError(null) }}
            className="text-xs transition-opacity hover:opacity-80"
            style={{ color: 'var(--accent)' }}
          >
            {mode === 'login' ? 'No tienes cuenta? Crear una' : 'Ya tienes cuenta? Inicia sesion'}
          </button>
        </div>

        {!hasUsers && (
          <p className="text-[10px] text-center mt-4" style={{ color: 'var(--text-secondary)' }}>
            La primera cuenta creada sera administrador
          </p>
        )}
      </div>
    </div>
  )
}
