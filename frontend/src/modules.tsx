import { lazy, type ComponentType } from 'react'
import DashboardPage from './pages/DashboardPage'

// Pagina de cada modulo. Nombre, icono, ruta y si esta encendido vienen del registro del
// servidor (backend/src/modules.rs); aca solo se dice que componente la dibuja.
// Paginas cargadas bajo demanda: el bundle inicial solo trae login, layout y dashboard
export const MODULE_PAGES: Record<string, ComponentType> = {
  dashboard: DashboardPage,
  files: lazy(() => import('./pages/FilesPage')),
  printing: lazy(() => import('./pages/PrintingPage')),
  printers3d: lazy(() => import('./pages/Printers3DPage')),
  network: lazy(() => import('./pages/NetworkPage')),
  tasks: lazy(() => import('./pages/TasksPage')),
  notes: lazy(() => import('./pages/NotesPage')),
  email: lazy(() => import('./pages/EmailPage')),
  inventory: lazy(() => import('./pages/InventoryPage')),
  portfolio: lazy(() => import('./pages/PortfolioPage')),
  sensors: lazy(() => import('./pages/SensorsPage')),
  terminal: lazy(() => import('./pages/TerminalPage')),
  cameras: lazy(() => import('./pages/CamerasPage')),
  machines: lazy(() => import('./pages/MachinesPage')),
  deadlines: lazy(() => import('./pages/DeadlinesPage')),
}

// Ajustes propios de cada modulo: aparecen en Configuracion > Modulos solo si el modulo
// esta encendido (solo admin)
export const MODULE_SETTINGS: Record<string, ComponentType> = {
  email: lazy(() => import('./components/MicrosoftOAuthSection')),
  sensors: lazy(() => import('./components/settings/modules/SensorsSettings')),
  music: lazy(() => import('./components/settings/modules/MusicSettings')),
  cameras: lazy(() => import('./components/settings/modules/CamerasSettings')),
}
