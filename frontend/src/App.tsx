import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { ThemeProvider } from './themes/ThemeContext'
import { AuthProvider, useAuth } from './auth/AuthContext'
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
import { Loader2 } from 'lucide-react'

function AppRoutes() {
  const { user, loading } = useAuth()

  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center" style={{ backgroundColor: 'var(--bg-primary)' }}>
        <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
      </div>
    )
  }

  if (!user) {
    return <LoginPage />
  }

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Navigate to="/dashboard" replace />} />
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/files" element={<FilesPage />} />
        <Route path="/printing" element={<PrintingPage />} />
        <Route path="/printers3d" element={<Printers3DPage />} />
        <Route path="/network" element={<NetworkPage />} />
        <Route path="/tasks" element={<TasksPage />} />
        <Route path="/terminal" element={<TerminalPage />} />
        <Route path="/settings" element={<SettingsPage />} />
      </Route>
    </Routes>
  )
}

export default function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <BrowserRouter>
          <AppRoutes />
        </BrowserRouter>
      </AuthProvider>
    </ThemeProvider>
  )
}
