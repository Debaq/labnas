import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { ThemeProvider } from './themes/ThemeContext'
import { AuthProvider, useAuth } from './auth/AuthContext'
import { ToastProvider } from './components/ToastContext'
import EventsProvider from './events/EventsProvider'
import Layout from './components/Layout'
import LoginPage from './pages/LoginPage'
import DashboardPage from './pages/DashboardPage'
import { Loader2 } from 'lucide-react'
import { lazy } from 'react'

// Paginas cargadas bajo demanda: el bundle inicial solo trae login, layout y dashboard
const FilesPage = lazy(() => import('./pages/FilesPage'))
const NetworkPage = lazy(() => import('./pages/NetworkPage'))
const SettingsPage = lazy(() => import('./pages/SettingsPage'))
const TerminalPage = lazy(() => import('./pages/TerminalPage'))
const Printers3DPage = lazy(() => import('./pages/Printers3DPage'))
const PrintingPage = lazy(() => import('./pages/PrintingPage'))
const TasksPage = lazy(() => import('./pages/TasksPage'))
const NotesPage = lazy(() => import('./pages/NotesPage'))
const EmailPage = lazy(() => import('./pages/EmailPage'))
const InventoryPage = lazy(() => import('./pages/InventoryPage'))
const PortfolioPage = lazy(() => import('./pages/PortfolioPage'))
const PlaylistEditorPage = lazy(() => import('./pages/PlaylistEditorPage'))
const SensorsPage = lazy(() => import('./pages/SensorsPage'))

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
          <EventsProvider>
            <BrowserRouter>
              <AppRoutes />
            </BrowserRouter>
          </EventsProvider>
        </ToastProvider>
      </AuthProvider>
    </ThemeProvider>
  )
}
