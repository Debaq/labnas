import { useState } from 'react'
import { FileText } from 'lucide-react'
import { finFileUrl, fileToDataUrl } from '../../../api/finance'
import { confirmInvoice, uploadInvoice, type PurchaseDetail, type PurchaseDoc } from '../../../api/financePurchases'
import { errorMessage } from '../../../lib/errors'
import { money } from '../../../lib/finance'
import { READING, cardStyle, inputStyle } from '../../../lib/financePurchases'
import { useToast } from '../../ToastContext'

const num = (s: string) => (s.trim() ? Number(s.replace(/\./g, '')) : null)

/** Factura de un borrador: datos extraidos para que una persona los revise y confirme */
function Draft({ purchaseId, doc, canConfirm, onChange }: { purchaseId: string; doc: PurchaseDoc; canConfirm: boolean; onChange: (d: PurchaseDetail) => void }) {
  const { addToast } = useToast()
  const exact = doc.extracted_by === 'sii_xml'
  const [folio, setFolio] = useState(doc.folio)
  const [rut, setRut] = useState(doc.issuer_rut)
  const [name, setName] = useState(doc.issuer_name)
  const [date, setDate] = useState(doc.issued_on ?? '')
  const [net, setNet] = useState(doc.net_amount?.toString() ?? '')
  const [tax, setTax] = useState(doc.tax_amount?.toString() ?? '')
  const [total, setTotal] = useState(doc.total_amount?.toString() ?? '')
  const [busy, setBusy] = useState(false)
  const field = 'w-full px-2 py-1.5 rounded-lg text-sm outline-none'

  const confirm = async () => {
    setBusy(true)
    try {
      onChange(await confirmInvoice(purchaseId, doc.id, { kind: doc.kind, folio, issuer_rut: rut, issuer_name: name, issued_on: date, net: num(net), tax: num(tax), total: num(total) }))
      addToast('Factura confirmada', 'success')
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-2">
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Datos: {READING[doc.extracted_by]}. {exact ? 'Son los del documento y no se editan.' : 'Revisalos contra el documento antes de confirmar.'}
        {doc.file && <> <a href={finFileUrl(doc.file.id)} target="_blank" rel="noreferrer" className="underline">Ver archivo</a></>}
      </p>
      <div className="grid gap-2 sm:grid-cols-3">
        <input value={folio} onChange={(e) => setFolio(e.target.value)} disabled={exact} placeholder="Folio" className={field} style={inputStyle} />
        <input value={rut} onChange={(e) => setRut(e.target.value)} disabled={exact} placeholder="RUT emisor" className={field} style={inputStyle} />
        <input type="date" value={date} onChange={(e) => setDate(e.target.value)} disabled={exact} className={field} style={inputStyle} />
        <input value={name} onChange={(e) => setName(e.target.value)} disabled={exact} placeholder="Razon social" className={`${field} sm:col-span-3`} style={inputStyle} />
        <input value={net} onChange={(e) => setNet(e.target.value)} disabled={exact} placeholder="Neto" inputMode="numeric" className={field} style={inputStyle} />
        <input value={tax} onChange={(e) => setTax(e.target.value)} disabled={exact} placeholder="IVA" inputMode="numeric" className={field} style={inputStyle} />
        <input value={total} onChange={(e) => setTotal(e.target.value)} disabled={exact} placeholder="Total" inputMode="numeric" className={field} style={inputStyle} />
      </div>
      {canConfirm
        ? <div className="flex justify-end"><button onClick={confirm} disabled={busy} className="px-3 py-1.5 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Confirmar datos de la factura</button></div>
        : <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Quien gestiona Finanzas debe confirmar estos datos.</p>}
    </div>
  )
}

/** Subida de la factura (XML del SII, PDF o imagen) y confirmacion de sus datos */
export default function InvoicePanel({ detail, canUpload, canConfirm, onChange }: {
  detail: PurchaseDetail
  canUpload: boolean
  canConfirm: boolean
  onChange: (d: PurchaseDetail) => void
}) {
  const { addToast } = useToast()
  const [busy, setBusy] = useState(false)
  const draft = detail.documents.find((d) => !d.confirmed_by)

  const upload = async (f: File) => {
    setBusy(true)
    try {
      const d = await uploadInvoice(detail.purchase.id, { name: f.name, data: await fileToDataUrl(f) })
      onChange(d)
      addToast('Factura subida: revisa los datos y confirma', 'success')
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Factura</h3>
      {detail.documents.filter((d) => d.confirmed_by).map((d) => (
        <p key={d.id} className="text-sm" style={{ color: 'var(--text-primary)' }}>
          {d.kind} folio {d.folio} · {d.issuer_name || d.issuer_rut} · {money(d.total_amount)} · confirmada por {d.confirmed_by}
        </p>
      ))}
      {draft && <Draft key={draft.id} purchaseId={detail.purchase.id} doc={draft} canConfirm={canConfirm} onChange={onChange} />}
      {canUpload && detail.purchase.status === 'recibida' && (
        <label className="flex items-center gap-2 text-xs cursor-pointer w-fit px-3 py-1.5 rounded-lg" style={inputStyle}>
          <FileText size={14} /> {busy ? 'Subiendo...' : draft ? 'Subir otro archivo' : 'Subir factura (XML del SII, PDF o imagen)'}
          <input type="file" accept=".xml,.pdf,image/jpeg,image/png" className="hidden" disabled={busy}
            onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ''; if (f) void upload(f) }} />
        </label>
      )}
    </div>
  )
}
