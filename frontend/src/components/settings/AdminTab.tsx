import { useEffect, useState } from 'react'
import { Power, Loader2, Shield, ChevronUp, ChevronDown, AlertTriangle } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import SetupWizard from '../SetupWizard'
import LedgerSection from '../LedgerSection'
import AuditLogSection from '../AuditLogSection'
import { fetchModules, toggleModule, reorderModules, fetchSetup, ModuleInUseError, type SetupStatus } from '../../api'
import type { ModuleInfo } from '../../types'
import { moduleIcon } from '../../lib/moduleIcons'

export default function AdminTab() {
  const { refreshModules } = useAuth()
  const [wizard, setWizard] = useState<SetupStatus | null>(null)
  const [modules, setModules] = useState<ModuleInfo[]>([])
  const label = (id: string) => modules.find(m => m.id === id)?.name ?? id
  const [modulesLoading, setModulesLoading] = useState(true)
  const [toggling, setToggling] = useState<string | null>(null)
  const [needsRestart, setNeedsRestart] = useState(false)

  useEffect(() => {
    fetchModules()
      .then(m => { setModules(m); setModulesLoading(false) })
      .catch(() => setModulesLoading(false))
  }, [])

  async function handleModuleToggle(mod: ModuleInfo) {
    if (mod.locked) return
    setToggling(mod.id)
    try {
      let missing: string[]
      try {
        missing = await toggleModule(mod.id, !mod.enabled)
      } catch (e) {
        // Otro modulo encendido lo usa: confirmar antes de apagar
        if (!(e instanceof ModuleInUseError) || !confirm(`${e.message}.\n\n¿Apagar ${label(mod.id)} igual?`)) throw e
        missing = await toggleModule(mod.id, false, true)
      }
      setModules(prev => prev.map(m => m.id === mod.id ? { ...m, enabled: !m.enabled } : m))
      await refreshModules()
      setNeedsRestart(true)
      if (missing.length > 0) {
        alert(`${label(mod.id)} quedo encendido. Usa ${missing.map(label).join(', ')}, que esta apagado: enciendelo para tener todo.`)
      }
    } catch {}
    setToggling(null)
  }

  async function handleModuleMove(id: string, direction: 'up' | 'down') {
    const sorted = [...modules].sort((a, b) => a.display_order - b.display_order)
    const idx = sorted.findIndex(m => m.id === id)
    if (idx < 0) return
    const swapIdx = direction === 'up' ? idx - 1 : idx + 1
    if (swapIdx < 0 || swapIdx >= sorted.length) return
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

  const sortedModules = [...modules].sort((a, b) => a.display_order - b.display_order)
  const enabledCount = modules.filter(m => m.enabled).length

  return (
    <>
      <LedgerSection />
      <AuditLogSection />

      <div className="flex items-center gap-3 text-sm" style={{ color: 'var(--text-secondary)' }}>
        <span>Carpetas, nombre en la red, HTTPS, Telegram y clave de cifrado en pasos guiados.</span>
        <button onClick={() => fetchSetup().then(setWizard).catch(() => {})} className="px-3 py-1.5 rounded-lg text-xs"
          style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}>
          Abrir asistente inicial
        </button>
      </div>
      {wizard && <SetupWizard status={wizard} onClose={() => setWizard(null)} />}

      {/* Administracion: modulos (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Shield size={22} style={{ color: 'var(--accent)' }} />
          <div>
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Modulos de LabNAS
            </h2>
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              Activa, desactiva y reordena los modulos disponibles
            </p>
          </div>
        </div>

        {needsRestart && (
          <div
            className="flex items-center gap-3 rounded-xl px-5 py-3 mb-4"
            style={{ backgroundColor: 'var(--warning-alpha, rgba(255,180,0,0.1))', border: '1px solid var(--warning, #f5a623)' }}
          >
            <AlertTriangle size={18} style={{ color: 'var(--warning, #f5a623)' }} />
            <span className="text-sm" style={{ color: 'var(--text-primary)' }}>
              Los cambios en modulos con procesos en segundo plano (sensores, red, musica, email, impresoras 3D) requieren reiniciar LabNAS para tomar efecto completo.
            </span>
          </div>
        )}

        <div
          className="flex items-center gap-6 rounded-xl px-5 py-3 mb-4"
          style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
        >
          <div className="flex items-center gap-2">
            <Power size={16} style={{ color: 'var(--success)' }} />
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
              {enabledCount} de {modules.length} modulos activos
            </span>
          </div>
        </div>

        {modulesLoading ? (
          <div className="flex justify-center py-12">
            <Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} />
          </div>
        ) : (
          <div className="space-y-2">
            {sortedModules.map((mod, idx) => {
              const Icon = moduleIcon(mod.icon)
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
                  <div
                    className="w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0"
                    style={{ backgroundColor: mod.enabled ? 'var(--accent-alpha)' : 'var(--bg-tertiary)' }}
                  >
                    <Icon size={20} style={{ color: mod.enabled ? 'var(--accent)' : 'var(--text-secondary)' }} />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-medium" style={{ color: 'var(--text-primary)' }}>
                        {mod.name ?? mod.id}
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
                      {mod.description}
                    </p>
                    {mod.uses && mod.uses.length > 0 && (
                      <p className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                        Usa: {mod.uses.map(u => {
                          const off = !modules.find(m => m.id === u)?.enabled
                          return (
                            <span key={u} style={{ color: off ? 'var(--warning)' : undefined }} title={off ? 'Apagado: esta parte no funciona' : undefined}>
                              {label(u)}{off ? ' (apagado)' : ''}
                            </span>
                          )
                        }).reduce<React.ReactNode[]>((acc, el, i) => (i ? [...acc, ', ', el] : [el]), [])}
                      </p>
                    )}
                  </div>
                  <div className="flex flex-col gap-0.5">
                    <button
                      onClick={() => handleModuleMove(mod.id, 'up')}
                      disabled={idx === 0}
                      className="p-0.5 rounded transition-all hover:opacity-80 disabled:opacity-20"
                      style={{ color: 'var(--text-secondary)' }}
                    >
                      <ChevronUp size={14} />
                    </button>
                    <button
                      onClick={() => handleModuleMove(mod.id, 'down')}
                      disabled={idx === sortedModules.length - 1}
                      className="p-0.5 rounded transition-all hover:opacity-80 disabled:opacity-20"
                      style={{ color: 'var(--text-secondary)' }}
                    >
                      <ChevronDown size={14} />
                    </button>
                  </div>
                  <button
                    onClick={() => handleModuleToggle(mod)}
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
      </section>
    </>
  )
}
