import { useEffect, useState } from 'react'
import { ExternalLink, Eye } from 'lucide-react'
import { fetchTransparencyConfig } from '../api'
import { useAuth } from '../auth/AuthContext'
import ReportsPanel from '../components/transparency/ReportsPanel'
import RequestsPanel from '../components/transparency/RequestsPanel'
import SectionsPanel from '../components/transparency/SectionsPanel'

type Tab = 'portal' | 'informes' | 'solicitudes'

/** Transparencia: que se publica en el portal, informes y solicitudes de informacion (docs/TRANSPARENCIA.md) */
export default function TransparencyPage() {
  const { canModule } = useAuth()
  const canManage = canModule('transparencia', 'gestionar')
  const [tab, setTab] = useState<Tab>('portal')
  const [managers, setManagers] = useState<string[]>([])
  // Al cambiar secciones o informes, los otros paneles se vuelven a leer
  const [version, setVersion] = useState(0)

  useEffect(() => {
    if (canManage) fetchTransparencyConfig().then((c) => setManagers(c.managers)).catch(() => setManagers([]))
  }, [canManage])

  const tabs: { id: Tab; label: string }[] = [
    { id: 'portal', label: 'Portal' },
    { id: 'informes', label: 'Informes' },
    ...(canManage ? [{ id: 'solicitudes' as Tab, label: 'Solicitudes de informacion' }] : []),
  ]

  return (
    <div className="p-4 sm:p-6 max-w-5xl mx-auto space-y-4">
      <div className="flex items-center gap-2">
        <h1 className="text-xl font-semibold flex items-center gap-2 flex-1" style={{ color: 'var(--text-primary)' }}>
          <Eye size={20} /> Transparencia
        </h1>
        <a href="/portal-transparencia" target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-sm" style={{ color: 'var(--accent)' }}>
          Abrir el portal publico <ExternalLink size={14} />
        </a>
      </div>
      <div className="flex gap-1 overflow-x-auto" style={{ borderBottom: '1px solid var(--border)' }}>
        {tabs.map((t) => (
          <button key={t.id} onClick={() => setTab(t.id)} className="px-4 py-2 text-sm font-medium whitespace-nowrap"
            style={{ color: tab === t.id ? 'var(--accent)' : 'var(--text-secondary)', borderBottom: tab === t.id ? '2px solid var(--accent)' : '2px solid transparent' }}>{t.label}</button>
        ))}
      </div>
      {tab === 'portal' && <SectionsPanel key={version} canManage={canManage} onSaved={() => setVersion((v) => v + 1)} />}
      {tab === 'informes' && <ReportsPanel canManage={canManage} refreshKey={version} onChanged={() => setVersion((v) => v + 1)} />}
      {tab === 'solicitudes' && canManage && <RequestsPanel managers={managers} />}
    </div>
  )
}
