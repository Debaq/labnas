import { useState } from 'react'
import { Camera } from 'lucide-react'
import { fileToDataUrl } from '../../../api/finance'
import { receivePurchase, type PurchaseDetail } from '../../../api/financePurchases'
import { errorMessage } from '../../../lib/errors'
import { cardStyle, inputStyle } from '../../../lib/financePurchases'
import { useToast } from '../../ToastContext'

/** Acta de recepcion: conforme o no, observaciones, foto de respaldo y alta en Inventario */
export default function ReceivePanel({ detail, canInventory, onChange }: {
  detail: PurchaseDetail
  canInventory: boolean
  onChange: (d: PurchaseDetail) => void
}) {
  const { addToast } = useToast()
  const p = detail.purchase
  const [conforming, setConforming] = useState(true)
  const [notes, setNotes] = useState('')
  const [photo, setPhoto] = useState<File | null>(null)
  const [inv, setInv] = useState(false)
  const [invName, setInvName] = useState(p.title)
  const [qty, setQty] = useState('1')
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    setBusy(true)
    try {
      const d = await receivePurchase(p.id, {
        conforming, notes,
        photo: photo ? { name: photo.name, data: await fileToDataUrl(photo) } : null,
        inventory: conforming && inv ? { name: invName.trim() || null, quantity: Number(qty) } : null,
      })
      onChange(d)
      addToast(conforming ? 'Recepcion registrada' : 'Recepcion con observaciones registrada', 'success')
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Acta de recepcion</h3>
      <div className="flex gap-4 text-sm" style={{ color: 'var(--text-primary)' }}>
        <label className="flex items-center gap-1.5"><input type="radio" checked={conforming} onChange={() => setConforming(true)} /> Conforme</label>
        <label className="flex items-center gap-1.5"><input type="radio" checked={!conforming} onChange={() => setConforming(false)} /> No conforme</label>
      </div>
      <textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={2} maxLength={2000} placeholder={conforming ? 'Observaciones (opcional)' : 'Que esta mal o falta'} className="w-full px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
      <label className="flex items-center gap-2 text-xs cursor-pointer w-fit px-3 py-1.5 rounded-lg" style={inputStyle}>
        <Camera size={14} /> {photo ? photo.name : 'Foto del bien recibido'}
        <input type="file" accept="image/jpeg,image/png" className="hidden" onChange={(e) => setPhoto(e.target.files?.[0] ?? null)} />
      </label>
      {conforming && canInventory && (
        <div className="space-y-2">
          <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}><input type="checkbox" checked={inv} onChange={(e) => setInv(e.target.checked)} /> Crear el bien en Inventario</label>
          {inv && (
            <div className="grid gap-2 sm:grid-cols-3">
              <input value={invName} onChange={(e) => setInvName(e.target.value)} maxLength={120} className="sm:col-span-2 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
              <input type="number" min={1} value={qty} onChange={(e) => setQty(e.target.value)} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </div>
          )}
        </div>
      )}
      <div className="flex justify-end">
        <button onClick={submit} disabled={busy || (!conforming && !notes.trim())} className="px-3 py-1.5 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {conforming ? 'Registrar recepcion' : 'Registrar observaciones'}
        </button>
      </div>
    </div>
  )
}
