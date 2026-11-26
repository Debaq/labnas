import { useState } from 'react'
import { Fingerprint } from 'lucide-react'
import { approvePurchase, rejectPurchase, signApproval, type PurchaseDetail } from '../../../api/financePurchases'
import { passkeysSupported } from '../../../api/passkeys'
import { errorMessage } from '../../../lib/errors'
import { money } from '../../../lib/finance'
import { cardStyle, inputStyle } from '../../../lib/financePurchases'
import { useToast } from '../../ToastContext'

/** Aprobar (con declaracion de conflicto de interes y, si se exige, firma con passkey) o rechazar */
export default function ApprovePanel({ detail, quoteId, onChange, onRefresh }: {
  detail: PurchaseDetail
  quoteId: string | null
  onChange: (d: PurchaseDetail) => void
  /** Recarga la ficha (un conflicto declarado queda registrado aunque no apruebe) */
  onRefresh: () => void
}) {
  const { addToast } = useToast()
  const { purchase: p, quotes, settings } = detail
  const [conflict, setConflict] = useState<'no' | 'si' | ''>('')
  const [cdetail, setCdetail] = useState('')
  const [reason, setReason] = useState('')
  const [rejectWhy, setRejectWhy] = useState('')
  const [sign, setSign] = useState(settings.require_passkey)
  const [busy, setBusy] = useState(false)

  const chosen = quotes.find((q) => q.id === quoteId)
  const cheapest = Math.min(...quotes.map((q) => q.amount))
  const needsReason = !!chosen && quotes.length > 1 && chosen.amount > cheapest
  const amount = chosen?.amount ?? p.estimated_amount
  const supplier = chosen?.supplier ?? 'sin cotizacion (monto estimado)'

  const run = async (fn: () => Promise<PurchaseDetail>, ok: string) => {
    setBusy(true)
    try {
      onChange(await fn())
      addToast(ok, 'success')
    } catch (e) {
      addToast(errorMessage(e), 'error')
      onRefresh()
    } finally {
      setBusy(false)
    }
  }

  const approve = () => run(async () => {
    const passkey = sign || settings.require_passkey ? await signApproval(p.id, quoteId) : undefined
    return approvePurchase(p.id, { quote_id: quoteId, reason, conflict: { has_conflict: conflict === 'si', detail: cdetail }, passkey })
  }, 'Compra aprobada')

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Decision de gasto</h3>
      <p className="text-sm" style={{ color: 'var(--text-primary)' }}>Se aprobaria {money(amount)} a {supplier}.</p>
      {needsReason && (
        <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>No es la cotizacion mas barata: razon de la eleccion
          <textarea value={reason} onChange={(e) => setReason(e.target.value)} rows={2} maxLength={1000} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
      )}
      <fieldset className="space-y-1">
        <legend className="text-xs" style={{ color: 'var(--text-secondary)' }}>Declaracion de conflicto de interes con este proveedor</legend>
        <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}><input type="radio" name="conf" checked={conflict === 'no'} onChange={() => setConflict('no')} /> No tengo conflicto de interes</label>
        <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}><input type="radio" name="conf" checked={conflict === 'si'} onChange={() => setConflict('si')} /> Si tengo (no podre aprobar esta compra)</label>
        {conflict === 'si' && <input value={cdetail} onChange={(e) => setCdetail(e.target.value)} placeholder="Describe el conflicto" maxLength={1000} className="w-full px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />}
      </fieldset>
      {passkeysSupported() && !settings.require_passkey && (
        <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
          <input type="checkbox" checked={sign} onChange={(e) => setSign(e.target.checked)} /> <Fingerprint size={14} /> Firmar la aprobacion con mi passkey
        </label>
      )}
      {settings.require_passkey && <p className="text-xs flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}><Fingerprint size={14} /> Esta aprobacion exige firma con passkey.</p>}
      <div className="flex gap-2 justify-end flex-wrap">
        <button onClick={approve} disabled={busy || !conflict || (conflict === 'si' && cdetail.trim().length < 3) || (needsReason && reason.trim().length < 5)}
          className="px-3 py-1.5 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {conflict === 'si' ? 'Registrar conflicto' : 'Aprobar'}
        </button>
      </div>
      <div className="flex gap-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
        <input value={rejectWhy} onChange={(e) => setRejectWhy(e.target.value)} placeholder="Motivo del rechazo" maxLength={1000} className="flex-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        <button onClick={() => run(() => rejectPurchase(p.id, rejectWhy), 'Compra rechazada')} disabled={busy || rejectWhy.trim().length < 5} className="px-3 py-1.5 rounded-lg text-sm disabled:opacity-50" style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>Rechazar</button>
      </div>
    </div>
  )
}
