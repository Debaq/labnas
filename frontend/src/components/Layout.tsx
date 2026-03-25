import { useState, useEffect } from 'react'
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { LayoutDashboard, FolderOpen, Network, Settings, Server, TerminalSquare, Printer, Box, Power, LogOut, User, ClipboardList, FileText, ChevronLeft, ChevronRight, Mail, Download } from 'lucide-react'
import { useTheme } from '../themes/ThemeContext'
import { useAuth } from '../auth/AuthContext'
import { shutdownServer, getBranding, fetchHealth, checkUpdate } from '../api'
import MusicPanel from './MusicPanel'

declare const __APP_VERSION__: string
const FRONTEND_VERSION = __APP_VERSION__

const pageTitles: Record<string, string> = {
  '/dashboard': 'Dashboard',
  '/files': 'Explorador de Archivos',
  '/printing': 'Impresion de Documentos',
  '/printers3d': 'Impresoras 3D',
  '/network': 'Red Local',
  '/tasks': 'Tareas / Horario',
  '/notes': 'Notas',
  '/terminal': 'Terminal',
  '/email': 'Correo',
  '/settings': 'Configuracion',
}

export default function Layout() {
  const { theme, setTheme, themeNames } = useTheme()
  const { user, logout, can, isAdmin } = useAuth()
  const location = useLocation()
  const navigate = useNavigate()
  const pageTitle = pageTitles[location.pathname] || 'LabNAS'
  const [shuttingDown, setShuttingDown] = useState(false)
  const [newVersion, setNewVersion] = useState<string | null>(null)
  const [labName, setLabName] = useState('LabNAS')
  const [logoUrl, setLogoUrl] = useState('')

  // Sidebar colapsable
  const [collapsed, setCollapsed] = useState(() => {
    return localStorage.getItem('labnas-sidebar-collapsed') === 'true'
  })

  useEffect(() => {
    localStorage.setItem('labnas-sidebar-collapsed', String(collapsed))
  }, [collapsed])

  useEffect(() => {
    getBranding().then(b => {
      if (b.lab_name) { setLabName(b.lab_name); document.title = b.lab_name }
      if (b.logo_url) {
        setLogoUrl(b.logo_url)
        localStorage.setItem('labnas_logo_url', b.logo_url)
        const link = document.querySelector("link[rel~='icon']") as HTMLLinkElement
        if (link) link.href = b.logo_url
      }
      // Aplicar accent color personalizado del branding
      if (b.accent_color) {
        localStorage.setItem('labnas-accent-color', b.accent_color)
        document.documentElement.style.setProperty('--accent', b.accent_color)
        document.documentElement.style.setProperty('--accent-alpha', b.accent_color + '1f')
        document.documentElement.style.setProperty('--sidebar-active', b.accent_color)
      }
    }).catch(() => {})
  }, [])

  // Auto-reload cuando el backend se actualiza
  useEffect(() => {
    const checkVersion = async () => {
      try {
        const health = await fetchHealth()
        if (health.version && health.version !== FRONTEND_VERSION) {
          console.log(`[LabNAS] Backend ${health.version} != Frontend ${FRONTEND_VERSION}, recargando...`)
          window.location.reload()
        }
      } catch {}
    }
    const interval = setInterval(checkVersion, 30000)
    return () => clearInterval(interval)
  }, [])

  // Chequear actualizaciones (solo admin)
  useEffect(() => {
    if (!isAdmin) return
    const check = () => {
      checkUpdate().then(info => {
        if (info.update_available && info.latest_version) {
          setNewVersion(info.latest_version)
        } else {
          setNewVersion(null)
        }
      }).catch(() => {})
    }
    check()
    const interval = setInterval(check, 30 * 60 * 1000) // cada 30 min
    return () => clearInterval(interval)
  }, [isAdmin])

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
    { to: '/tasks', label: 'Tareas / Horario', icon: ClipboardList, show: true },
    { to: '/notes', label: 'Notas', icon: FileText, show: true },
    { to: '/email', label: 'Correo', icon: Mail, show: true },
    { to: '/terminal', label: 'Terminal', icon: TerminalSquare, show: can('terminal') },
    { to: '/settings', label: 'Configuracion', icon: Settings, show: true },
  ]

  const roleLabel = user?.role === 'admin' ? 'Admin' : user?.role === 'operador' ? 'Operador' : user?.role === 'observador' ? 'Observador' : 'Pendiente'

  return (
    <div className="flex h-screen overflow-hidden" style={{ backgroundColor: 'var(--bg-primary)' }}>
      {/* Sidebar */}
      <aside
        className="flex flex-col h-screen transition-all duration-300"
        style={{
          backgroundColor: 'var(--sidebar-bg)',
          borderRight: '1px solid var(--border)',
          width: collapsed ? '70px' : '260px',
          minWidth: collapsed ? '70px' : '260px',
        }}
      >
        {/* Toggle + Logo */}
        <div className="flex items-center gap-3 px-4 py-5 relative" style={{ borderBottom: '1px solid var(--border)' }}>
          {collapsed ? (
            <div className="flex items-center justify-center w-full">
              {logoUrl ? (
                <img src={logoUrl} alt={labName} className="w-7 h-7 rounded object-contain" />
              ) : (
                <Server size={28} style={{ color: 'var(--accent)' }} />
              )}
            </div>
          ) : (
            <>
              {logoUrl ? (
                <img src={logoUrl} alt={labName} className="w-7 h-7 rounded object-contain" />
              ) : (
                <Server size={28} style={{ color: 'var(--accent)' }} />
              )}
              <span className="text-xl font-bold tracking-tight truncate" style={{ color: 'var(--text-primary)' }}>
                {labName}
              </span>
            </>
          )}
          <button
            onClick={() => setCollapsed(!collapsed)}
            className="absolute -right-3 top-1/2 -translate-y-1/2 w-6 h-6 rounded-full flex items-center justify-center transition-all duration-200 hover:opacity-80 z-10"
            style={{
              backgroundColor: 'var(--sidebar-bg)',
              border: '1px solid var(--border)',
              color: 'var(--text-secondary)',
            }}
          >
            {collapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
          </button>
        </div>

        {/* Navigation */}
        <nav className="flex-1 px-2 py-4 space-y-1">
          {navItems.filter(i => i.show).map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              title={collapsed ? item.label : undefined}
              className="flex items-center gap-3 px-3 py-2.5 rounded-lg transition-all duration-200 group"
              style={({ isActive }) => ({
                backgroundColor: isActive ? 'var(--accent-alpha)' : 'transparent',
                color: isActive ? 'var(--sidebar-active)' : 'var(--sidebar-text)',
                justifyContent: collapsed ? 'center' : 'flex-start',
              })}
            >
              {({ isActive }) => (
                <>
                  <item.icon
                    size={20}
                    style={{ color: isActive ? 'var(--sidebar-active)' : 'var(--sidebar-text)' }}
                  />
                  {!collapsed && <span className="text-sm font-medium">{item.label}</span>}
                </>
              )}
            </NavLink>
          ))}
        </nav>

        {/* Theme selector - oculto cuando esta colapsado */}
        {!collapsed && (
          <div className="px-4 py-3" style={{ borderTop: '1px solid var(--border)' }}>
            <label className="block text-xs font-medium mb-2" style={{ color: 'var(--text-secondary)' }}>
              Tema
            </label>
            <select
              value={theme}
              onChange={(e) => setTheme(e.target.value as any)}
              className="w-full px-3 py-1.5 rounded-lg text-sm cursor-pointer outline-none transition-all duration-200"
              style={{
                backgroundColor: 'var(--input-bg)',
                color: 'var(--text-primary)',
                border: '1px solid var(--input-border)',
              }}
            >
              {themeNames.map((t) => (
                <option key={t} value={t}>
                  {t === 'auto' ? 'Automatico' : t.charAt(0).toUpperCase() + t.slice(1)}
                </option>
              ))}
            </select>
          </div>
        )}

        {/* User info */}
        <div
          className="px-3 py-3 flex items-center"
          style={{
            borderTop: '1px solid var(--border)',
            justifyContent: collapsed ? 'center' : 'space-between',
          }}
        >
          {collapsed ? (
            <button
              onClick={logout}
              className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
              style={{ color: 'var(--text-secondary)' }}
              title={`${user?.username} - Cerrar sesion`}
            >
              <User size={18} style={{ color: 'var(--accent)' }} />
            </button>
          ) : (
            <>
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
            </>
          )}
        </div>

        {/* Powered by footer */}
        <div className="px-3 py-2 text-center" style={{ borderTop: '1px solid var(--border)' }}>
          <a
            href="https://github.com/Debaq/labnas"
            target="_blank"
            rel="noopener noreferrer"
            className="text-[10px] transition-opacity hover:opacity-80"
            style={{ color: 'var(--text-secondary)' }}
          >
            {collapsed ? 'LabNAS' : 'Powered by TecMedHub'}
          </a>
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

        {/* Update banner */}
        {newVersion && (
          <div
            className="flex items-center justify-between px-6 py-2 cursor-pointer hover:opacity-90 transition-opacity"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
            onClick={() => navigate('/settings')}
          >
            <div className="flex items-center gap-2 text-sm font-medium">
              <Download size={16} />
              Nueva version disponible: {newVersion}
            </div>
            <span className="text-xs opacity-80">Click para actualizar</span>
          </div>
        )}

        {/* Content */}
        <main className="flex-1 overflow-auto p-8" style={{ backgroundColor: 'var(--bg-primary)' }}>
          <Outlet />
        </main>
      </div>

      {/* Music Panel */}
      <MusicPanel />
    </div>
  )
}
