import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { ThemeProvider } from './themes/ThemeContext'
import Layout from './components/Layout'
import DashboardPage from './pages/DashboardPage'
import FilesPage from './pages/FilesPage'
import NetworkPage from './pages/NetworkPage'
import SettingsPage from './pages/SettingsPage'
import TerminalPage from './pages/TerminalPage'
import Printers3DPage from './pages/Printers3DPage'
import PrintingPage from './pages/PrintingPage'

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<Navigate to="/dashboard" replace />} />
            <Route path="/dashboard" element={<DashboardPage />} />
            <Route path="/files" element={<FilesPage />} />
            <Route path="/printing" element={<PrintingPage />} />
            <Route path="/printers3d" element={<Printers3DPage />} />
            <Route path="/network" element={<NetworkPage />} />
            <Route path="/terminal" element={<TerminalPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ThemeProvider>
  )
}
