import { useEffect, useState } from 'react'
import { Navigate } from 'react-router-dom'
import {
  Shield, LayoutDashboard, FolderOpen, Network, Printer, Box, ClipboardList,
  FileText, Mail, Package, GraduationCap, Thermometer, TerminalSquare, Music,
  ChevronUp, ChevronDown, Loader2, AlertTriangle, Power as PowerIcon
} from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { fetchModules, toggleModule, reorderModules } from '../api'
import type { ModuleInfo } from '../types'

const MODULE_META: Record<string, { label: string; icon: any; description: string }> = {
  dashboard:  { label: 'Dashboard',       icon: LayoutDashboard, description: 'Panel principal con estadisticas y resumen' },
  files:      { label: 'Archivos',        icon: FolderOpen,      description: 'Explorador y gestor de archivos' },
  network:    { label: 'Red',             icon: Network,         description: 'Escaneo y monitoreo de red local' },
  printing:   { label: 'Impresion',       icon: Printer,         description: 'Impresion de documentos via CUPS' },
  printers3d: { label: 'Impresoras 3D',   icon: Box,             description: 'Control de impresoras 3D (OctoPrint/Moonraker)' },
  tasks:      { label: 'Tareas / Horario', icon: ClipboardList,  description: 'Tareas, proyectos y calendario' },
  notes:      { label: 'Notas',           icon: FileText,        description: 'Notas compartidas del laboratorio' },
  email:      { label: 'Correo',          icon: Mail,            description: 'Correo electronico IMAP' },
  inventory:  { label: 'Inventario',      icon: Package,         description: 'Inventario de equipos y materiales' },
  portfolio:  { label: 'Portafolio',      icon: GraduationCap,   description: 'Portafolio academico e investigacion' },
  sensors:    { label: 'Sensores',        icon: Thermometer,     description: 'Sensores IoT (ESP-NOW / WiFi)' },
  terminal:   { label: 'Terminal',        icon: TerminalSquare,  description: 'Terminal web remoto' },
  music:      { label: 'Musica',          icon: Music,           description: 'Reproductor de musica (yt-dlp + mpv)' },
}

export default function AdminPage() {
  const { isAdmin, refreshModules } = useAuth()
  const [modules, setModules] = useState<ModuleInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [toggling, setToggling] = useState<string | null>(null)
  const [needsRestart, setNeedsRestart] = useState(false)

  useEffect(() => {
    fetchModules()
      .then(m => { setModules(m); setLoading(false) })
      .catch(() => setLoading(false))
  }, [])

  if (!isAdmin) return <Navigate to="/dashboard" replace />

  async function handleToggle(mod: ModuleInfo) {
    if (mod.locked) return
    setToggling(mod.id)
    try {
      await toggleModule(mod.id, !mod.enabled)
      setModules(prev =>
        prev.map(m => m.id === mod.id ? { ...m, enabled: !m.enabled } : m)
      )
      await refreshModules()
      setNeedsRestart(true)
    } catch {}
    setToggling(null)
  }

  async function handleMove(id: string, direction: 'up' | 'down') {
    const sorted = [...modules].sort((a, b) => a.display_order - b.display_order)
    const idx = sorted.findIndex(m => m.id === id)
    if (idx < 0) return
    const swapIdx = direction === 'up' ? idx - 1 : idx + 1
    if (swapIdx < 0 || swapIdx >= sorted.length) return

    // Swap display_order
    const newOrder = sorted.map((m, i) => {
      if (i === idx) return { ...m, display_order: sorted[swapIdx].display_order }
      if (i === swapIdx) return { ...m, display_order: sorted[idx].display_order }
      return m
    })

    setModules(newOrder)
    try {
      await reorderModules(newOrder.map(m => ({ id: m.id, order: m.display_order })))
      await refreshModules()
    } catch {}
  }

  const sorted = [...modules].sort((a, b) => a.display_order - b.display_order)
  const enabledCount = modules.filter(m => m.enabled).length
  const totalCount = modules.length

  return (
    <div className="space-y-6 max-w-4xl">
      {/* Header */}
      <div className="flex items-center gap-3">
        <Shield size={24} style={{ color: 'var(--accent)' }} />
        <div>
          <h2 className="text-xl font-bold" style={{ color: 'var(--text-primary)' }}>
            Administracion
          </h2>
          <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
            Gestiona los modulos activos de LabNAS
          </p>
        </div>
      </div>

      {/* Restart warning */}
      {needsRestart && (
        <div
          className="flex items-center gap-3 rounded-xl px-5 py-3"
          style={{ backgroundColor: 'var(--warning-alpha, rgba(255,180,0,0.1))', border: '1px solid var(--warning, #f5a623)' }}
        >
          <AlertTriangle size={18} style={{ color: 'var(--warning, #f5a623)' }} />
          <span className="text-sm" style={{ color: 'var(--text-primary)' }}>
            Los cambios en modulos con procesos en segundo plano (sensores, red, musica, email, impresoras 3D) requieren reiniciar LabNAS para tomar efecto completo.
          </span>
        </div>
      )}

      {/* Stats */}
      <div
        className="flex items-center gap-6 rounded-xl px-5 py-3"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
      >
        <div className="flex items-center gap-2">
          <PowerIcon size={16} style={{ color: 'var(--success)' }} />
          <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
            {enabledCount} de {totalCount} modulos activos
          </span>
        </div>
      </div>

      {/* Modules grid */}
      {loading ? (
        <div className="flex justify-center py-12">
          <Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} />
        </div>
      ) : (
        <div className="space-y-2">
          {sorted.map((mod, idx) => {
            const meta = MODULE_META[mod.id]
            if (!meta) return null
            const Icon = meta.icon

            return (
              <div
                key={mod.id}
                className="flex items-center gap-4 rounded-xl px-5 py-4 transition-all duration-200"
                style={{
                  backgroundColor: 'var(--bg-secondary)',
                  border: '1px solid var(--border)',
                  opacity: mod.enabled ? 1 : 0.5,
                }}
              >
                {/* Icon */}
                <div
                  className="w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0"
                  style={{ backgroundColor: mod.enabled ? 'var(--accent-alpha)' : 'var(--bg-tertiary)' }}
                >
                  <Icon size={20} style={{ color: mod.enabled ? 'var(--accent)' : 'var(--text-secondary)' }} />
                </div>

                {/* Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium" style={{ color: 'var(--text-primary)' }}>
                      {meta.label}
                    </span>
                    {mod.locked && (
                      <span
                        className="text-[10px] px-1.5 py-0.5 rounded font-medium"
                        style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}
                      >
                        Siempre activo
                      </span>
                    )}
                  </div>
                  <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>
                    {meta.description}
                  </p>
                </div>

                {/* Reorder */}
                <div className="flex flex-col gap-0.5">
                  <button
                    onClick={() => handleMove(mod.id, 'up')}
                    disabled={idx === 0}
                    className="p-0.5 rounded transition-all hover:opacity-80 disabled:opacity-20"
                    style={{ color: 'var(--text-secondary)' }}
                  >
                    <ChevronUp size={14} />
                  </button>
                  <button
                    onClick={() => handleMove(mod.id, 'down')}
                    disabled={idx === sorted.length - 1}
                    className="p-0.5 rounded transition-all hover:opacity-80 disabled:opacity-20"
                    style={{ color: 'var(--text-secondary)' }}
                  >
                    <ChevronDown size={14} />
                  </button>
                </div>

                {/* Toggle */}
                <button
                  onClick={() => handleToggle(mod)}
                  disabled={mod.locked || toggling === mod.id}
                  className="relative w-12 h-6 rounded-full transition-all duration-300 flex-shrink-0"
                  style={{
                    backgroundColor: mod.enabled ? 'var(--accent)' : 'var(--bg-tertiary)',
                    cursor: mod.locked ? 'not-allowed' : 'pointer',
                  }}
                >
                  {toggling === mod.id ? (
                    <Loader2 size={12} className="animate-spin absolute top-1.5 left-1/2 -translate-x-1/2" style={{ color: '#fff' }} />
                  ) : (
                    <span
                      className="absolute top-0.5 w-5 h-5 rounded-full transition-all duration-300"
                      style={{
                        backgroundColor: '#fff',
                        left: mod.enabled ? '26px' : '2px',
                      }}
                    />
                  )}
                </button>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
