import { useState } from 'react'
import { Paperclip } from 'lucide-react'
import { finFileUrl, fileToDataUrl } from '../../../api/finance'
import { addPurchaseQuote, type PurchaseDetail } from '../../../api/financePurchases'
import { errorMessage } from '../../../lib/errors'
import { money } from '../../../lib/finance'
import { cardStyle, inputStyle, shortHash } from '../../../lib/financePurchases'
import { useToast } from '../../ToastContext'

/** Cuadro comparativo de cotizaciones; quien aprueba elige una (con razon si no es la mas barata) */
export default function QuotesPanel({ detail, canAdd, selected, onSelect, onChange }: {
  detail: PurchaseDetail
  canAdd: boolean
  /** Cotizacion elegida (solo mientras se puede aprobar) */
  selected: string | null
  onSelect: ((id: string) => void) | null
  onChange: (d: PurchaseDetail) => void
}) {
  const { addToast } = useToast()
  const { purchase: p, quotes } = detail
  const [open, setOpen] = useState(false)
  const [supplier, setSupplier] = useState('')
  const [rutv, setRut] = useState('')
  const [amount, setAmount] = useState('')
  const [until, setUntil] = useState('')
  const [notes, setNotes] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)

  const add = async () => {
    if (!file) return
    setBusy(true)
    try {
      const d = await addPurchaseQuote(p.id, {
        supplier, supplier_rut: rutv, amount: Number(amount.replace(/\./g, '')), valid_until: until || null, notes,
        file: { name: file.name, data: await fileToDataUrl(file) },
      })
      onChange(d)
      setOpen(false); setSupplier(''); setRut(''); setAmount(''); setUntil(''); setNotes(''); setFile(null)
      addToast('Cotizacion agregada', 'success')
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
          Cotizaciones ({quotes.length}{detail.required_quotes > 0 ? ` de ${detail.required_quotes} exigidas` : ''})
        </h3>
        {canAdd && !open && <button onClick={() => setOpen(true)} className="text-xs px-3 py-1 rounded-lg" style={inputStyle}>Agregar cotizacion</button>}
      </div>
      {detail.required_quotes > 0 && (
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Desde {money(detail.settings.quote_threshold)} se exigen al menos {detail.settings.min_quotes} cotizaciones para poder aprobar.
        </p>
      )}
      {quotes.length > 0 && (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-xs" style={{ color: 'var(--text-secondary)' }}>
                {onSelect && <th className="py-1 pr-2">Elegir</th>}
                <th className="py-1 pr-3">Proveedor</th><th className="pr-3">Monto</th><th className="pr-3">Valida hasta</th><th className="pr-3">Archivo</th><th>Notas</th>
              </tr>
            </thead>
            <tbody>
              {quotes.map((q) => (
                <tr key={q.id} style={{ borderTop: '1px solid var(--border)', color: 'var(--text-primary)', fontWeight: p.chosen_quote_id === q.id ? 600 : 400 }}>
                  {onSelect && <td className="py-1.5 pr-2"><input type="radio" name="quote" checked={selected === q.id} onChange={() => onSelect(q.id)} /></td>}
                  <td className="py-1.5 pr-3">{q.supplier}{q.supplier_rut && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}> {q.supplier_rut}</span>}</td>
                  <td className="pr-3 whitespace-nowrap">{money(q.amount)}{q.cheapest && quotes.length > 1 && <span className="ml-1 text-[11px]" style={{ color: '#16a34a' }}>mas barata</span>}{p.chosen_quote_id === q.id && <span className="ml-1 text-[11px]" style={{ color: 'var(--accent)' }}>elegida</span>}</td>
                  <td className="pr-3 whitespace-nowrap">{q.valid_until ?? '—'}</td>
                  <td className="pr-3">{q.file ? <a href={finFileUrl(q.file.id)} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 underline text-xs" title={`SHA-256 ${q.file.sha256}`}><Paperclip size={12} />{shortHash(q.file.sha256)}</a> : '—'}</td>
                  <td className="text-xs" style={{ color: 'var(--text-secondary)' }}>{q.notes}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {p.choice_reason && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Razon de la eleccion: {p.choice_reason}</p>}
      {open && (
        <div className="space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
          <div className="grid gap-2 sm:grid-cols-2">
            <input value={supplier} onChange={(e) => setSupplier(e.target.value)} placeholder="Proveedor" maxLength={120} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            <input value={rutv} onChange={(e) => setRut(e.target.value)} placeholder="RUT (opcional)" maxLength={12} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            <input inputMode="numeric" value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="Monto ($)" className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            <input type="date" value={until} onChange={(e) => setUntil(e.target.value)} title="Valida hasta" className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          </div>
          <input value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Notas (opcional)" maxLength={1000} className="w-full px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          <label className="flex items-center gap-2 text-xs cursor-pointer" style={{ color: 'var(--text-secondary)' }}>
            <Paperclip size={14} /> {file ? file.name : 'Adjuntar la cotizacion (PDF, imagen, XML...)'}
            <input type="file" className="hidden" onChange={(e) => setFile(e.target.files?.[0] ?? null)} />
          </label>
          <div className="flex gap-2 justify-end">
            <button onClick={() => setOpen(false)} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
            <button onClick={add} disabled={busy || !file || !supplier.trim() || !(Number(amount.replace(/\./g, '')) > 0)} className="px-3 py-1.5 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
          </div>
        </div>
      )}
    </div>
  )
}
