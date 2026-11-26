import { useCallback, useState } from 'react'
import { ArrowLeft, FileText } from 'lucide-react'
import {
  cancelPurchase, fetchPurchase, orderPurchase, payPurchase, settlePurchase, startQuoting,
  type PurchaseDetail,
} from '../../../api/financePurchases'
import { useAuth } from '../../../auth/AuthContext'
import { errorMessage } from '../../../lib/errors'
import { money } from '../../../lib/finance'
import { STATUS, STEP_LABEL, cardStyle, fmtWhen, inputStyle, shortHash } from '../../../lib/financePurchases'
import { finFileUrl } from '../../../api/finance'
import { useToast } from '../../ToastContext'
import ActaView from './ActaView'
import ApprovePanel from './ApprovePanel'
import InvoicePanel from './InvoicePanel'
import QuotesPanel from './QuotesPanel'
import ReceivePanel from './ReceivePanel'

/** Ficha de una compra: linea de tiempo, cotizaciones y los botones que corresponden al estado y al nivel */
export default function PurchaseView({ detail, staff, canUse, onChange, onBack }: {
  detail: PurchaseDetail
  staff: boolean
  canUse: boolean
  onChange: (d: PurchaseDetail) => void
  onBack: () => void
}) {
  const { user, canModule } = useAuth()
  const { addToast } = useToast()
  const me = user?.username ?? ''
  const { purchase: p } = detail
  const [quote, setQuote] = useState<string | null>(p.chosen_quote_id)
  const [acta, setActa] = useState(false)
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)

  const refresh = useCallback(() => { fetchPurchase(p.id).then(onChange).catch((e) => addToast(errorMessage(e), 'error')) }, [p.id, onChange, addToast])
  const run = async (fn: () => Promise<PurchaseDetail>, ok: string) => {
    setBusy(true)
    try { onChange(await fn()); setText(''); addToast(ok, 'success') } catch (e) { addToast(errorMessage(e), 'error') } finally { setBusy(false) }
  }

  const isRequester = p.requested_by === me
  const open = !['rechazada', 'anulada', 'rendida'].includes(p.status)
  const canApprove = staff && p.status === 'cotizacion' && !isRequester
  const canReceive = canUse && p.status === 'orden' && !isRequester && p.approved_by !== me
  const quoteOpen = ['solicitud', 'cotizacion'].includes(p.status)
  const early = quoteOpen
  const canCancel = open && p.status !== 'pagada' && (staff || (early && isRequester))
  const btn = 'px-3 py-1.5 rounded-lg text-sm font-medium disabled:opacity-50'
  const accent = { backgroundColor: 'var(--accent)', color: '#fff' }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <button onClick={onBack} className="flex items-center gap-1 text-sm" style={{ color: 'var(--text-secondary)' }}><ArrowLeft size={14} /> Compras</button>
        <button onClick={() => setActa(true)} className="flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg" style={inputStyle}><FileText size={14} /> Acta de la decision de gasto</button>
      </div>

      <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
        <div className="flex items-center gap-2 flex-wrap">
          <h2 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>#{p.folio} · {p.title}</h2>
          <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: STATUS[p.status].color, border: '1px solid currentColor' }}>{STATUS[p.status].label}</span>
        </div>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          {p.project_title ?? p.project_id}{p.budget_item_name && ` · ${p.budget_item_name}`} · pidio {p.requested_by}
          {p.approved_by && ` · aprobo ${p.approved_by}`}{p.received_by && ` · recibio ${p.received_by}`}
        </p>
        <p className="text-sm" style={{ color: 'var(--text-primary)' }}>Estimado {money(p.estimated_amount)}{p.final_amount != null && ` · Facturado ${money(p.final_amount)}`}</p>
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>{p.justification}</p>
        {detail.budget && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Item «{detail.budget.item_name}»: aprobado {money(detail.budget.approved)}, disponible {money(detail.budget.available)}</p>}
        {detail.warnings.map((w) => <p key={w} className="text-xs" style={{ color: 'var(--danger)' }}>{w}</p>)}
      </div>

      {(detail.quotes.length > 0 || quoteOpen) && (
        <QuotesPanel detail={detail} canAdd={quoteOpen && (isRequester || staff)} selected={quote} onSelect={canApprove ? setQuote : null} onChange={onChange} />
      )}

      {p.status === 'solicitud' && (isRequester || staff) && (
        <button onClick={() => run(() => startQuoting(p.id), 'Pasa a cotizacion')} disabled={busy} className={btn} style={accent}>Pasar a cotizacion</button>
      )}
      {canApprove && <ApprovePanel detail={detail} quoteId={quote} onChange={onChange} onRefresh={refresh} />}
      {p.status === 'cotizacion' && staff && isRequester && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Quien pidio la compra no puede aprobarla: la aprueba otra persona.</p>}
      {p.status === 'aprobada' && staff && (
        <div className="flex gap-2"><input value={text} onChange={(e) => setText(e.target.value)} placeholder="N° de orden o nota (opcional)" className="flex-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          <button onClick={() => run(() => orderPurchase(p.id, text), 'Orden emitida')} disabled={busy} className={btn} style={accent}>Emitir orden de compra</button></div>
      )}
      {p.status === 'orden' && !canReceive && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>La recepcion la hace alguien que no pidio ni aprobo la compra.</p>}
      {canReceive && <ReceivePanel detail={detail} canInventory={canModule('inventory', 'usar')} onChange={onChange} />}
      {['recibida', 'facturada', 'pagada', 'rendida'].includes(p.status) && (
        <InvoicePanel detail={detail} canUpload={isRequester || p.received_by === me || staff} canConfirm={staff} onChange={onChange} />
      )}
      {p.status === 'facturada' && staff && (
        <div className="flex gap-2"><input value={text} onChange={(e) => setText(e.target.value)} placeholder="Referencia del pago (transferencia, cheque...)" className="flex-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          <button onClick={() => run(() => payPurchase(p.id, null, text), 'Pago registrado')} disabled={busy} className={btn} style={accent}>Registrar pago</button></div>
      )}
      {p.status === 'pagada' && staff && <button onClick={() => run(() => settlePurchase(p.id, ''), 'Compra rendida')} disabled={busy} className={btn} style={accent}>Marcar como rendida</button>}

      {detail.receptions.length > 0 && (
        <div className="rounded-xl p-4 space-y-1" style={cardStyle}>
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Recepciones</h3>
          {detail.receptions.map((r) => (
            <p key={r.id} className="text-sm" style={{ color: 'var(--text-primary)' }}>
              {r.conforming ? 'Conforme' : 'NO conforme'} · {r.received_by} · {fmtWhen(r.received_at)}{r.notes && ` · ${r.notes}`}
              {r.photo && <> · <a href={finFileUrl(r.photo.id)} target="_blank" rel="noreferrer" className="underline">foto</a></>}
            </p>
          ))}
        </div>
      )}

      <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
        <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Linea de tiempo</h3>
        <ol className="space-y-2">
          {detail.steps.map((s) => (
            <li key={s.id} className="text-sm" style={{ color: 'var(--text-primary)' }}>
              <span className="font-medium">{STEP_LABEL[s.step] ?? s.step}</span> · {s.username} · <span style={{ color: 'var(--text-secondary)' }}>{fmtWhen(s.at)}</span>
              {s.passkey && <span className="ml-1 text-[11px]" style={{ color: '#16a34a' }}>firmada con passkey</span>}
              {s.note && <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{s.note}</div>}
              {s.ledger_seq != null && <div className="text-[11px] font-mono" style={{ color: 'var(--text-secondary)' }}>bitacora #{s.ledger_seq} {shortHash(s.ledger_hash)}</div>}
            </li>
          ))}
        </ol>
      </div>

      {canCancel && (
        <div className="flex gap-2"><input value={text} onChange={(e) => setText(e.target.value)} placeholder="Motivo de la anulacion" className="flex-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          <button onClick={() => run(() => cancelPurchase(p.id, text), 'Compra anulada')} disabled={busy || text.trim().length < 5} className={btn} style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>Anular</button></div>
      )}
      {acta && <ActaView detail={detail} onClose={() => setActa(false)} />}
    </div>
  )
}
