import { useCallback, useEffect, useState } from 'react'
import { Loader2, Plus, Settings } from 'lucide-react'
import { fetchPurchase, fetchPurchases, type Purchase, type PurchaseDetail } from '../../api/financePurchases'
import { useEvent } from '../../events/useEvents'
import { errorMessage } from '../../lib/errors'
import { money, type FinanceTabProps } from '../../lib/finance'
import { STATUS, STATUS_ORDER, cardStyle, inputStyle } from '../../lib/financePurchases'
import { useToast } from '../ToastContext'
import NewPurchaseForm from './purchases/NewPurchaseForm'
import PurchaseView from './purchases/PurchaseView'
import SettingsPanel from './purchases/SettingsPanel'

/** Compras: solicitud, cotizacion, aprobacion, orden, recepcion, factura y pago (docs/FINANZAS.md) */
export default function PurchasesTab({ projectId, projects, staff, canUse }: FinanceTabProps) {
  const { addToast } = useToast()
  const [list, setList] = useState<Purchase[] | null>(null)
  const [status, setStatus] = useState('')
  const [open, setOpen] = useState<PurchaseDetail | null>(null)
  const [creating, setCreating] = useState(false)
  const [settings, setSettings] = useState(false)

  const load = useCallback(() => {
    fetchPurchases({ project: projectId, status: status || undefined })
      .then(setList)
      .catch((e) => addToast(errorMessage(e), 'error'))
  }, [projectId, status, addToast])
  useEffect(load, [load])
  useEvent('finance', () => {
    load()
    if (open) fetchPurchase(open.purchase.id).then(setOpen).catch(() => undefined)
  }, load)

  const show = (id: string) => fetchPurchase(id).then(setOpen).catch((e) => addToast(errorMessage(e), 'error'))

  // Enlace directo a una compra (?compra=<id>), p. ej. desde Anomalias
  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get('compra')
    if (id) fetchPurchase(id).then(setOpen).catch((e) => addToast(errorMessage(e), 'error'))
  }, [addToast])

  if (open) return <PurchaseView detail={open} staff={staff} canUse={canUse} onChange={setOpen} onBack={() => { setOpen(null); load() }} />

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 flex-wrap">
        <select value={status} onChange={(e) => setStatus(e.target.value)} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
          <option value="">Todos los estados</option>
          {STATUS_ORDER.map((s) => <option key={s} value={s}>{STATUS[s].label}</option>)}
        </select>
        <div className="flex-1" />
        {staff && <button onClick={() => setSettings((v) => !v)} className="flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg" style={inputStyle}><Settings size={14} /> Ajustes</button>}
        {canUse && <button onClick={() => setCreating(true)} className="flex items-center gap-1 text-sm px-3 py-1.5 rounded-lg font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}><Plus size={14} /> Pedir compra</button>}
      </div>
      {settings && <SettingsPanel onClose={() => setSettings(false)} />}
      {creating && <NewPurchaseForm projects={projects} projectId={projectId} onCancel={() => setCreating(false)} onCreated={(d) => { setCreating(false); setOpen(d) }} />}
      {list === null ? (
        <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
      ) : list.length === 0 ? (
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>No hay compras{status ? ' en ese estado' : ''}.</p>
      ) : (
        <div className="rounded-xl overflow-x-auto" style={cardStyle}>
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-xs" style={{ color: 'var(--text-secondary)' }}>
                <th className="p-2">N°</th><th className="p-2">Compra</th><th className="p-2">Proyecto</th><th className="p-2">Monto</th><th className="p-2">Estado</th><th className="p-2">Pidio</th>
              </tr>
            </thead>
            <tbody>
              {list.map((p) => (
                <tr key={p.id} onClick={() => show(p.id)} className="cursor-pointer" style={{ borderTop: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                  <td className="p-2">{p.folio}</td>
                  <td className="p-2">{p.title}{p.chosen_supplier && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}> · {p.chosen_supplier}</span>}</td>
                  <td className="p-2 text-xs" style={{ color: 'var(--text-secondary)' }}>{p.project_title}</td>
                  <td className="p-2 whitespace-nowrap">{money(p.final_amount ?? p.estimated_amount)}</td>
                  <td className="p-2"><span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: STATUS[p.status].color, border: '1px solid currentColor' }}>{STATUS[p.status].label}</span></td>
                  <td className="p-2 text-xs" style={{ color: 'var(--text-secondary)' }}>{p.requested_by}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
