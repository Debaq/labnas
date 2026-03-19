import { NavLink, Outlet, useLocation } from 'react-router-dom'
import { LayoutDashboard, FolderOpen, Network, Settings, Server, TerminalSquare, Printer, Box } from 'lucide-react'
import { useTheme } from '../themes/ThemeContext'
import type { ThemeName } from '../themes/themes'

const navItems = [
  { to: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { to: '/files', label: 'Archivos', icon: FolderOpen },
  { to: '/printing', label: 'Impresion', icon: Printer },
  { to: '/printers3d', label: 'Impresoras 3D', icon: Box },
  { to: '/network', label: 'Red', icon: Network },
  { to: '/terminal', label: 'Terminal', icon: TerminalSquare },
  { to: '/settings', label: 'Configuracion', icon: Settings },
]

const pageTitles: Record<string, string> = {
  '/dashboard': 'Dashboard',
  '/files': 'Explorador de Archivos',
  '/printing': 'Impresion de Documentos',
  '/printers3d': 'Impresoras 3D',
  '/network': 'Red Local',
  '/terminal': 'Terminal',
  '/settings': 'Configuracion',
}

export default function Layout() {
  const { theme, setTheme, themeNames } = useTheme()
  const location = useLocation()
  const pageTitle = pageTitles[location.pathname] || 'LabNAS'

  return (
    <div className="flex h-screen overflow-hidden" style={{ backgroundColor: 'var(--bg-primary)' }}>
      {/* Sidebar */}
      <aside
        className="flex flex-col w-[260px] min-w-[260px] h-screen"
        style={{ backgroundColor: 'var(--sidebar-bg)', borderRight: '1px solid var(--border)' }}
      >
        {/* Logo */}
        <div className="flex items-center gap-3 px-6 py-5" style={{ borderBottom: '1px solid var(--border)' }}>
          <Server size={28} style={{ color: 'var(--accent)' }} />
          <span className="text-xl font-bold tracking-tight" style={{ color: 'var(--text-primary)' }}>
            LabNAS
          </span>
        </div>

        {/* Navigation */}
        <nav className="flex-1 px-3 py-4 space-y-1">
          {navItems.map((item) => (
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
        <div className="px-4 py-4" style={{ borderTop: '1px solid var(--border)' }}>
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
          <div className="flex items-center gap-2">
            <span
              className="inline-block w-2 h-2 rounded-full"
              style={{ backgroundColor: 'var(--success)' }}
            />
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              En linea
            </span>
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
