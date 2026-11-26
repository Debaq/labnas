import { useState } from 'react'
import { Lock, Settings } from 'lucide-react'
import { saveProjectConfig, type ProjectConfig } from '../../../api'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { cardStyle, fmtDay, inputStyle } from '../../../lib/financeBudget'

/** Configuración financiera del proyecto: responsable, % de alerta, moneda y cierre */
export default function ConfigPanel({ config, staff, onSaved }: { config: ProjectConfig; staff: boolean; onSaved: () => void }) {
  const { addToast } = useToast()
  const [open, setOpen] = useState(false)
  const [responsible, setResponsible] = useState(config.responsible)
  const [alertPct, setAlertPct] = useState(String(config.alert_pct))
  const [currency, setCurrency] = useState(config.currency)
  const [reason, setReason] = useState('')
  const [busy, setBusy] = useState(false)
  const closed = config.closed_at != null

  const save = async (closeOrOpen?: boolean) => {
    setBusy(true)
    try {
      await saveProjectConfig(config.project_id, {
        responsible: responsible.trim(),
        alert_pct: Number(alertPct),
        currency,
        ...(closeOrOpen === undefined ? {} : { closed: closeOrOpen }),
        ...(reason.trim() ? { reason: reason.trim() } : {}),
      })
      addToast(closeOrOpen === true ? 'Proyecto cerrado: queda de solo lectura' : closeOrOpen === false ? 'Proyecto reabierto' : 'Configuración guardada', 'success')
      setOpen(false)
      setReason('')
      onSaved()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <div className="flex items-center gap-3 flex-wrap text-sm" style={{ color: 'var(--text-primary)' }}>
        <h2 className="font-semibold">{config.title}</h2>
        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Responsable: {config.responsible || 'sin asignar'} · Alerta al {config.alert_pct}% · {config.currency}
        </span>
        {closed && <span className="text-xs flex items-center gap-1" style={{ color: 'var(--danger)' }}><Lock size={12} /> Cerrado el {fmtDay(config.closed_at!)} (solo lectura)</span>}
        {staff && (
          <button onClick={() => setOpen(!open)} className="ml-auto flex items-center gap-1 text-xs px-2 py-1 rounded-lg" style={inputStyle}>
            <Settings size={12} /> Configurar
          </button>
        )}
      </div>
      {open && staff && (
        <div className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-3">
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Responsable (usuario de LabNAS)
              <input value={responsible} onChange={(e) => setResponsible(e.target.value)} disabled={closed} maxLength={60} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>% de alerta (1 a 99)
              <input type="number" min={1} max={99} value={alertPct} onChange={(e) => setAlertPct(e.target.value)} disabled={closed} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Moneda
              <input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} disabled={closed} maxLength={3} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
          </div>
          <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Motivo {closed ? '(obligatorio para reabrir)' : '(opcional; queda en la bitácora)'}
            <input value={reason} onChange={(e) => setReason(e.target.value)} maxLength={300} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          </label>
          <div className="flex gap-2 flex-wrap">
            {!closed && <button disabled={busy} onClick={() => save()} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)' }}>Guardar</button>}
            {!closed && (
              <button disabled={busy} onClick={() => { if (window.confirm('Al cerrar el proyecto no se podrá modificar su presupuesto, caja ni ingresos. ¿Cerrar?')) save(true) }}
                className="px-3 py-1.5 rounded-lg text-sm" style={{ ...inputStyle, color: 'var(--danger)' }}>Cerrar proyecto</button>
            )}
            {closed && <button disabled={busy || !reason.trim()} onClick={() => save(false)} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)', opacity: reason.trim() ? 1 : 0.5 }}>Reabrir</button>}
          </div>
        </div>
      )}
    </div>
  )
}
