import { useState, useEffect } from 'react'
import { NavLink, Outlet, useLocation } from 'react-router-dom'
import { LayoutDashboard, FolderOpen, Network, Settings, Server, TerminalSquare, Printer, Box, Power, LogOut, User, ClipboardList, FileText } from 'lucide-react'
import { useTheme } from '../themes/ThemeContext'
import { useAuth } from '../auth/AuthContext'
import { shutdownServer, getBranding } from '../api'
import type { ThemeName } from '../themes/themes'

const pageTitles: Record<string, string> = {
  '/dashboard': 'Dashboard',
  '/files': 'Explorador de Archivos',
  '/printing': 'Impresion de Documentos',
  '/printers3d': 'Impresoras 3D',
  '/network': 'Red Local',
  '/tasks': 'Tareas y Proyectos',
  '/notes': 'Notas',
  '/terminal': 'Terminal',
  '/settings': 'Configuracion',
}

export default function Layout() {
  const { theme, setTheme, themeNames } = useTheme()
  const { user, logout, can, isAdmin } = useAuth()
  const location = useLocation()
  const pageTitle = pageTitles[location.pathname] || 'LabNAS'
  const [shuttingDown, setShuttingDown] = useState(false)
  const [labName, setLabName] = useState('LabNAS')
  const [logoUrl, setLogoUrl] = useState('')

  useEffect(() => {
    getBranding().then(b => {
      if (b.lab_name) { setLabName(b.lab_name); document.title = b.lab_name }
      if (b.logo_url) setLogoUrl(b.logo_url)
    }).catch(() => {})
  }, [])

  async function handleShutdown() {
    if (!confirm('Apagar LabNAS? El servidor se detendrá.')) return
    setShuttingDown(true)
    try {
      await shutdownServer()
    } catch { /* conexion se cortara */ }
  }

  // Build nav items based on permissions
  const navItems = [
    { to: '/dashboard', label: 'Dashboard', icon: LayoutDashboard, show: true },
    { to: '/files', label: 'Archivos', icon: FolderOpen, show: true },
    { to: '/printing', label: 'Impresion', icon: Printer, show: can('impresion') },
    { to: '/printers3d', label: 'Impresoras 3D', icon: Box, show: true },
    { to: '/network', label: 'Red', icon: Network, show: true },
    { to: '/tasks', label: 'Tareas', icon: ClipboardList, show: true },
    { to: '/notes', label: 'Notas', icon: FileText, show: true },
    { to: '/terminal', label: 'Terminal', icon: TerminalSquare, show: can('terminal') },
    { to: '/settings', label: 'Configuracion', icon: Settings, show: true },
  ]

  const roleLabel = user?.role === 'admin' ? 'Admin' : user?.role === 'operador' ? 'Operador' : user?.role === 'observador' ? 'Observador' : 'Pendiente'

  return (
    <div className="flex h-screen overflow-hidden" style={{ backgroundColor: 'var(--bg-primary)' }}>
      {/* Sidebar */}
      <aside
        className="flex flex-col w-[260px] min-w-[260px] h-screen"
        style={{ backgroundColor: 'var(--sidebar-bg)', borderRight: '1px solid var(--border)' }}
      >
        {/* Logo */}
        <div className="flex items-center gap-3 px-6 py-5" style={{ borderBottom: '1px solid var(--border)' }}>
          {logoUrl ? (
            <img src={logoUrl} alt={labName} className="w-7 h-7 rounded object-contain" />
          ) : (
            <Server size={28} style={{ color: 'var(--accent)' }} />
          )}
          <span className="text-xl font-bold tracking-tight truncate" style={{ color: 'var(--text-primary)' }}>
            {labName}
          </span>
        </div>

        {/* Navigation */}
        <nav className="flex-1 px-3 py-4 space-y-1">
          {navItems.filter(i => i.show).map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className="flex items-center gap-3 px-4 py-2.5 rounded-lg transition-all duration-200 group"
              style={({ isActive }) => ({
                backgroundColor: isActive ? 'var(--accent-alpha)' : 'transparent',
                color: isActive ? 'var(--sidebar-active)' : 'var(--sidebar-text)',
              })}
            >
              {({ isActive }) => (
                <>
                  <item.icon
                    size={20}
                    style={{ color: isActive ? 'var(--sidebar-active)' : 'var(--sidebar-text)' }}
                  />
                  <span className="text-sm font-medium">{item.label}</span>
                </>
              )}
            </NavLink>
          ))}
        </nav>

        {/* Theme selector */}
        <div className="px-4 py-3" style={{ borderTop: '1px solid var(--border)' }}>
          <label className="block text-xs font-medium mb-2" style={{ color: 'var(--text-secondary)' }}>
            Tema
          </label>
          <select
            value={theme}
            onChange={(e) => setTheme(e.target.value as ThemeName)}
            className="w-full px-3 py-1.5 rounded-lg text-sm cursor-pointer outline-none transition-all duration-200"
            style={{
              backgroundColor: 'var(--input-bg)',
              color: 'var(--text-primary)',
              border: '1px solid var(--input-border)',
            }}
          >
            {themeNames.map((t) => (
              <option key={t} value={t}>
                {t.charAt(0).toUpperCase() + t.slice(1)}
              </option>
            ))}
          </select>
        </div>

        {/* User info */}
        <div className="px-4 py-3 flex items-center justify-between" style={{ borderTop: '1px solid var(--border)' }}>
          <div className="flex items-center gap-2 min-w-0">
            <User size={16} style={{ color: 'var(--accent)' }} />
            <div className="min-w-0">
              <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{user?.username}</p>
              <p className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>{roleLabel}</p>
            </div>
          </div>
          <button
            onClick={logout}
            className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
            style={{ color: 'var(--text-secondary)' }}
            title="Cerrar sesion"
          >
            <LogOut size={16} />
          </button>
        </div>
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Header */}
        <header
          className="flex items-center justify-between px-8 py-4 min-h-[65px]"
          style={{ backgroundColor: 'var(--bg-secondary)', borderBottom: '1px solid var(--border)' }}
        >
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            {pageTitle}
          </h1>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2">
              <span
                className="inline-block w-2 h-2 rounded-full"
                style={{ backgroundColor: shuttingDown ? 'var(--danger)' : 'var(--success)' }}
              />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                {shuttingDown ? 'Apagando...' : 'En linea'}
              </span>
            </div>
            {isAdmin && (
              <button
                onClick={handleShutdown}
                disabled={shuttingDown}
                className="p-2 rounded-lg transition-all duration-200 hover:opacity-80"
                style={{
                  backgroundColor: 'var(--danger-alpha)',
                  color: 'var(--danger)',
                }}
                title="Apagar LabNAS"
              >
                <Power size={18} />
              </button>
            )}
          </div>
        </header>

        {/* Content */}
        <main className="flex-1 overflow-auto p-8" style={{ backgroundColor: 'var(--bg-primary)' }}>
          <Outlet />
        </main>
      </div>
    </div>
  )
}
