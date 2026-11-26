import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { ThemeProvider } from './themes/ThemeContext'
import { AuthProvider, useAuth } from './auth/AuthContext'
import { ToastProvider } from './components/ToastContext'
import EventsProvider from './events/EventsProvider'
import ThemeSync from './themes/ThemeSync'
import Layout from './components/Layout'
import LoginPage from './pages/LoginPage'
import { MODULE_PAGES } from './modules'
import DashboardPage from './pages/DashboardPage'
import { Loader2 } from 'lucide-react'
import { lazy, Suspense, useEffect, useState } from 'react'
import { fetchSetup, type SetupStatus } from './api'

// Paginas cargadas bajo demanda: el bundle inicial solo trae login, layout y dashboard
const SettingsPage = lazy(() => import('./pages/SettingsPage'))
const PlaylistEditorPage = lazy(() => import('./pages/PlaylistEditorPage'))
const MachineDetailPage = lazy(() => import('./pages/MachineDetailPage'))
const DeviceLinkPage = lazy(() => import('./pages/DeviceLinkPage'))
const SharedPage = lazy(() => import('./pages/SharedPage'))
const PublicRecordPage = lazy(() => import('./pages/PublicRecordPage'))
const PublicWorkshopPage = lazy(() => import('./pages/PublicWorkshopPage'))
const PublicCertificatePage = lazy(() => import('./pages/PublicCertificatePage'))
const DisplayPage = lazy(() => import('./pages/DisplayPage'))
const SetupWizard = lazy(() => import('./components/SetupWizard'))

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

/** Admin: muestra el asistente de primer arranque hasta que se termine u omita */
function SetupGate() {
  const [status, setStatus] = useState<SetupStatus | null>(null)
  useEffect(() => { fetchSetup().then(setStatus).catch(() => {}) }, [])
  if (!status || status.done) return null
  return (
    <Suspense fallback={null}>
      <SetupWizard status={status} onClose={() => setStatus({ ...status, done: true })} />
    </Suspense>
  )
}

function AppRoutes() {
  const { user, loading, isModuleEnabled, isAdmin, enabledModules } = useAuth()

  // Ficha publica: se ve sin cuenta (ver handlers/access.rs)
  const publicToken = window.location.pathname.match(/^\/p\/([0-9a-f]{48})$/)?.[1]
  if (publicToken) {
    return <Suspense fallback={<LoadingScreen />}><PublicRecordPage token={publicToken} /></Suspense>
  }
  // Inscripcion a un taller y verificacion de certificados: sin cuenta (ver handlers/workshops.rs)
  const workshopToken = window.location.pathname.match(/^\/taller\/([0-9a-f]{32})$/)?.[1]
  if (workshopToken) {
    return <Suspense fallback={<LoadingScreen />}><PublicWorkshopPage token={workshopToken} /></Suspense>
  }
  const certCode = window.location.pathname.match(/^\/certificado\/([0-9A-Fa-f]{4}(?:-[0-9A-Fa-f]{4}){3})$/)?.[1]
  if (certCode) {
    return <Suspense fallback={<LoadingScreen />}><PublicCertificatePage code={certCode} /></Suspense>
  }
  // Pantalla del laboratorio (TV o tablet): sin sesion (ver handlers/display.rs)
  const screenToken = window.location.pathname.match(/^\/pantalla\/([0-9a-f]{40})$/)?.[1]
  if (screenToken) {
    return <Suspense fallback={<LoadingScreen />}><DisplayPage token={screenToken} /></Suspense>
  }

  if (loading) {
    return <LoadingScreen />
  }

  if (!user) {
    return <LoginPage />
  }

  return (
    <>
      {isAdmin && <SetupGate />}
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Navigate to="/dashboard" replace />} />
          {/* La ruta de cada modulo la dice el servidor; el dashboard existe siempre */}
          <Route path="/dashboard" element={<DashboardPage />} />
          {enabledModules
            .filter(m => m.id !== 'dashboard' && m.enabled && m.path && MODULE_PAGES[m.id])
            .map(m => {
              const Page = MODULE_PAGES[m.id]
              return <Route key={m.id} path={m.path} element={<Page />} />
            })}
          {/* Ficha de una maquina: a ella lleva el QR pegado en la maquina */}
          {isModuleEnabled('machines') && <Route path="/machines/:id" element={<MachineDetailPage />} />}
          {/* Rutas no-modulo: siempre disponibles */}
          <Route path="/playlists/:id" element={<PlaylistEditorPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          {/* Lo que otros me autorizaron a ver (niveles de acceso) */}
          <Route path="/compartido" element={<SharedPage />} />
          {/* labnas-tray: autorizar un dispositivo (se abre desde la bandeja) */}
          <Route path="/vincular" element={<DeviceLinkPage />} />
          <Route path="/admin" element={<Navigate to="/settings" replace />} />
          {/* Catch-all: redirige a dashboard */}
          <Route path="*" element={<Navigate to="/dashboard" replace />} />
        </Route>
      </Routes>
    </>
  )
}

export default function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <ThemeSync />
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
