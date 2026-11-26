import { Suspense } from 'react'
import { Loader2 } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { moduleIcon } from '../../lib/moduleIcons'
import { MODULE_SETTINGS } from '../../modules'

/** Ajustes de cada modulo encendido, en el orden del menu */
export default function ModulesTab() {
  const { enabledModules } = useAuth()
  const mods = enabledModules
    .filter(m => m.enabled && MODULE_SETTINGS[m.id])
    .sort((a, b) => a.display_order - b.display_order)

  if (mods.length === 0) {
    return (
      <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
        Ningun modulo encendido tiene ajustes propios. Se encienden en Administracion.
      </p>
    )
  }

  return (
    <>
      {mods.map(m => {
        const Settings = MODULE_SETTINGS[m.id]
        const Icon = moduleIcon(m.icon)
        return (
          <section key={m.id} id={`modulo-${m.id}`} className="scroll-mt-4 space-y-4">
            <div className="flex items-center gap-3 pb-2" style={{ borderBottom: '1px solid var(--border)' }}>
              <Icon size={22} style={{ color: 'var(--accent)' }} />
              <h2 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>{m.name ?? m.id}</h2>
            </div>
            <Suspense fallback={<Loader2 size={18} className="animate-spin" style={{ color: 'var(--accent)' }} />}>
              <div className="space-y-6"><Settings /></div>
            </Suspense>
          </section>
        )
      })}
    </>
  )
}
