import { Printer, X } from 'lucide-react'
import { finFileUrl } from '../../../api/finance'
import type { PurchaseDetail } from '../../../api/financePurchases'
import { money } from '../../../lib/finance'
import { READING, STATUS, STEP_LABEL, fmtWhen, shortHash } from '../../../lib/financePurchases'

const PRINT_CSS = `@media print {
  body * { visibility: hidden !important; }
  #acta-print, #acta-print * { visibility: visible !important; }
  #acta-print { position: absolute; left: 0; top: 0; width: 100%; box-shadow: none !important; }
  .no-print { display: none !important; }
}`

/** Acta de la decision de gasto: todo lo que pasó con la compra, lista para imprimir o guardar en PDF */
export default function ActaView({ detail, onClose }: { detail: PurchaseDetail; onClose: () => void }) {
  const { purchase: p, steps, quotes, documents, receptions, declarations } = detail
  const th = 'text-left py-1 pr-3 font-semibold'
  return (
    <div className="fixed inset-0 z-50 overflow-auto p-4" style={{ backgroundColor: 'rgba(0,0,0,0.5)' }}>
      <style>{PRINT_CSS}</style>
      <div id="acta-print" className="max-w-3xl mx-auto rounded-xl p-6 space-y-4 text-sm" style={{ backgroundColor: '#fff', color: '#111' }}>
        <div className="no-print flex justify-end gap-2">
          <button onClick={() => window.print()} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: '#2563eb' }}><Printer size={14} /> Imprimir</button>
          <button onClick={onClose} className="px-2 py-1.5 rounded-lg border border-gray-300" aria-label="Cerrar"><X size={14} /></button>
        </div>
        <header>
          <h1 className="text-lg font-bold">Acta de decision de gasto · Compra #{p.folio}</h1>
          <p>{p.title}</p>
          <p>Proyecto: {p.project_title ?? p.project_id}{p.budget_item_name && ` · Item: ${p.budget_item_name}`}</p>
          <p>Estado: {STATUS[p.status].label} · Monto estimado {money(p.estimated_amount)}{p.final_amount != null && ` · Monto facturado ${money(p.final_amount)}`}</p>
          <p>Pidio: {p.requested_by} · Aprobo: {p.approved_by ?? '—'} · Recibio: {p.received_by ?? '—'}</p>
          <p className="mt-1"><strong>Justificacion:</strong> {p.justification}</p>
        </header>

        <section>
          <h2 className="font-semibold">Cotizaciones</h2>
          {quotes.length === 0 ? <p>Sin cotizaciones (se uso el monto estimado).</p> : (
            <table className="w-full"><thead><tr><th className={th}>Proveedor</th><th className={th}>Monto</th><th className={th}>Archivo (SHA-256)</th><th className={th}>Elegida</th></tr></thead>
              <tbody>{quotes.map((q) => (
                <tr key={q.id} className="border-t border-gray-300">
                  <td className="py-1 pr-3">{q.supplier} {q.supplier_rut}</td><td className="pr-3">{money(q.amount)}</td>
                  <td className="pr-3 font-mono text-xs">{q.file ? <a href={finFileUrl(q.file.id)} target="_blank" rel="noreferrer">{q.file.name} {shortHash(q.file.sha256)}</a> : '—'}</td>
                  <td>{p.chosen_quote_id === q.id ? 'Si' : ''}</td>
                </tr>))}</tbody></table>
          )}
          {p.choice_reason && <p>Razon de la eleccion: {p.choice_reason}</p>}
        </section>

        <section>
          <h2 className="font-semibold">Declaracion de conflicto de interes</h2>
          {declarations.length === 0 ? <p>Sin declaraciones.</p> : declarations.map((d) => (
            <p key={d.id}>{d.username} ({fmtWhen(d.at)}): {d.has_conflict ? `TIENE conflicto con ${d.supplier} — ${d.detail}` : `declara no tener conflicto con ${d.supplier}`}{d.ledger_seq != null && ` · bitacora #${d.ledger_seq}`}</p>
          ))}
        </section>

        <section>
          <h2 className="font-semibold">Pasos y bitacora inalterable</h2>
          <table className="w-full"><thead><tr><th className={th}>Fecha</th><th className={th}>Paso</th><th className={th}>Quien</th><th className={th}>Bitacora</th></tr></thead>
            <tbody>{steps.map((s) => (
              <tr key={s.id} className="border-t border-gray-300 align-top">
                <td className="py-1 pr-3 whitespace-nowrap">{fmtWhen(s.at)}</td>
                <td className="pr-3">{STEP_LABEL[s.step] ?? s.step}{s.passkey && ' (firmada con passkey)'}{s.note && <div className="text-xs text-gray-600">{s.note}</div>}</td>
                <td className="pr-3">{s.username}</td>
                <td className="font-mono text-xs">{s.ledger_seq != null ? `#${s.ledger_seq} ${shortHash(s.ledger_hash)}` : '—'}</td>
              </tr>))}</tbody></table>
        </section>

        <section>
          <h2 className="font-semibold">Respaldos</h2>
          <ul className="list-disc pl-5">
            {documents.map((d) => <li key={d.id}>{d.kind} folio {d.folio} de {d.issuer_name || d.issuer_rut} ({d.issuer_rut}) por {money(d.total_amount)} · {READING[d.extracted_by]} · {d.confirmed_by ? `confirmada por ${d.confirmed_by}` : 'SIN CONFIRMAR'}{d.file && <span className="font-mono text-xs"> · {shortHash(d.file.sha256)}</span>}</li>)}
            {receptions.map((r) => <li key={r.id}>Recepcion {r.conforming ? 'conforme' : 'NO conforme'} por {r.received_by} ({fmtWhen(r.received_at)}){r.notes && `: ${r.notes}`}{r.photo && <span className="font-mono text-xs"> · foto {shortHash(r.photo.sha256)}</span>}</li>)}
            {documents.length === 0 && receptions.length === 0 && <li>Sin respaldos todavia.</li>}
          </ul>
        </section>
        <p className="text-xs text-gray-500">Las huellas SHA-256 y los numeros de bitacora permiten comprobar que nada fue modificado despues.</p>
      </div>
    </div>
  )
}
