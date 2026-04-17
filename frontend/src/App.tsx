import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { ThemeProvider } from './themes/ThemeContext'
import { AuthProvider, useAuth } from './auth/AuthContext'
import { ToastProvider } from './components/ToastContext'
import Layout from './components/Layout'
import LoginPage from './pages/LoginPage'
import DashboardPage from './pages/DashboardPage'
import FilesPage from './pages/FilesPage'
import NetworkPage from './pages/NetworkPage'
import SettingsPage from './pages/SettingsPage'
import TerminalPage from './pages/TerminalPage'
import Printers3DPage from './pages/Printers3DPage'
import PrintingPage from './pages/PrintingPage'
import TasksPage from './pages/TasksPage'
import NotesPage from './pages/NotesPage'
import EmailPage from './pages/EmailPage'
import InventoryPage from './pages/InventoryPage'
import PortfolioPage from './pages/PortfolioPage'
import PlaylistEditorPage from './pages/PlaylistEditorPage'
import SensorsPage from './pages/SensorsPage'
import { Loader2 } from 'lucide-react'

// Registro de modulos: mapea module_id -> ruta + componente
const MODULE_ROUTES: Record<string, { path: string; component: React.ComponentType }> = {
  dashboard:  { path: '/dashboard',  component: DashboardPage },
  files:      { path: '/files',      component: FilesPage },
  printing:   { path: '/printing',   component: PrintingPage },
  printers3d: { path: '/printers3d', component: Printers3DPage },
  network:    { path: '/network',    component: NetworkPage },
  tasks:      { path: '/tasks',      component: TasksPage },
  notes:      { path: '/notes',      component: NotesPage },
  email:      { path: '/email',      component: EmailPage },
  inventory:  { path: '/inventory',  component: InventoryPage },
  portfolio:  { path: '/portfolio',  component: PortfolioPage },
  sensors:    { path: '/sensors',    component: SensorsPage },
  terminal:   { path: '/terminal',   component: TerminalPage },
}

function LoadingScreen() {
  const logoUrl = localStorage.getItem('labnas_logo_url')

  return (
    <div className="min-h-screen flex items-center justify-center" style={{ backgroundColor: 'var(--bg-primary)' }}>
      {logoUrl ? (
        <img
          src={logoUrl}
          alt="Cargando..."
          className="w-16 h-16 rounded-xl object-contain animate-pulse"
        />
      ) : (
        <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
      )}
    </div>
  )
}

function AppRoutes() {
  const { user, loading, isModuleEnabled } = useAuth()

  if (loading) {
    return <LoadingScreen />
  }

  if (!user) {
    return <LoginPage />
  }

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Navigate to="/dashboard" replace />} />
        {Object.entries(MODULE_ROUTES)
          .filter(([id]) => isModuleEnabled(id))
          .map(([id, { path, component: Comp }]) => (
            <Route key={id} path={path} element={<Comp />} />
          ))}
        {/* Rutas no-modulo: siempre disponibles */}
        <Route path="/playlists/:id" element={<PlaylistEditorPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/admin" element={<Navigate to="/settings" replace />} />
        {/* Catch-all: redirige a dashboard */}
        <Route path="*" element={<Navigate to="/dashboard" replace />} />
      </Route>
    </Routes>
  )
}

export default function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <ToastProvider>
          <BrowserRouter>
            <AppRoutes />
          </BrowserRouter>
        </ToastProvider>
      </AuthProvider>
    </ThemeProvider>
  )
}
