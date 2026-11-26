import { useEffect, useState } from 'react'
import { Landmark, Loader2 } from 'lucide-react'
import { fetchPortfolio } from '../api'
import type { PortfolioEntry } from '../types'
import { useAuth } from '../auth/AuthContext'
import { useToast } from '../components/ToastContext'
import { errorMessage } from '../lib/errors'
import BudgetTab from '../components/finance/BudgetTab'
import PurchasesTab from '../components/finance/PurchasesTab'
import RendicionTab from '../components/finance/RendicionTab'
import TravelTab from '../components/finance/TravelTab'

type Tab = 'presupuesto' | 'compras' | 'viajes' | 'rendicion'

const TABS: { id: Tab; label: string }[] = [
  { id: 'presupuesto', label: 'Presupuesto' },
  { id: 'compras', label: 'Compras' },
  { id: 'viajes', label: 'Viajes' },
  { id: 'rendicion', label: 'Rendicion y auditoria' },
]

/** Finanzas: presupuesto por proyecto, compras, viajes y rendicion (docs/FINANZAS.md) */
export default function FinancePage() {
  const { canModule } = useAuth()
  const { addToast } = useToast()
  const staff = canModule('finanzas', 'gestionar')
  const canUse = canModule('finanzas', 'usar')
  const [tab, setTab] = useState<Tab>(() => {
    const t = new URLSearchParams(window.location.search).get('tab') as Tab | null
    return t && TABS.some((x) => x.id === t) ? t : 'presupuesto'
  })
  const [projects, setProjects] = useState<PortfolioEntry[] | null>(null)
  const [projectId, setProjectId] = useState<string | null>(() => new URLSearchParams(window.location.search).get('proyecto'))

  useEffect(() => {
    fetchPortfolio()
      .then((all) => setProjects(all.filter((p) => p.entry_type === 'project' || p.budget != null)))
      .catch((e) => { setProjects([]); addToast(errorMessage(e), 'error') })
  }, [addToast])

  const props = { projectId, projects: projects ?? [], staff, canUse }

  return (
    <div className="p-4 sm:p-6 max-w-6xl mx-auto space-y-4">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <h1 className="text-xl font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
          <Landmark size={20} /> Finanzas
        </h1>
        <select value={projectId ?? ''} onChange={(e) => setProjectId(e.target.value || null)}
          className="rounded-lg px-3 py-1.5 text-sm max-w-xs"
          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
          <option value="">Todos los proyectos</option>
          {(projects ?? []).map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
        </select>
      </div>
      <div className="flex gap-1 overflow-x-auto" style={{ borderBottom: '1px solid var(--border)' }}>
        {TABS.map((t) => (
          <button key={t.id} onClick={() => setTab(t.id)} className="px-4 py-2 text-sm font-medium whitespace-nowrap"
            style={{ color: tab === t.id ? 'var(--accent)' : 'var(--text-secondary)', borderBottom: tab === t.id ? '2px solid var(--accent)' : '2px solid transparent' }}>
            {t.label}
          </button>
        ))}
      </div>
      {projects === null ? (
        <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
      ) : tab === 'presupuesto' ? (
        <BudgetTab {...props} />
      ) : tab === 'compras' ? (
        <PurchasesTab {...props} />
      ) : tab === 'viajes' ? (
        <TravelTab {...props} />
      ) : (
        <RendicionTab {...props} />
      )}
    </div>
  )
}
