import { useEffect, useState } from 'react'
import { fetchPurchaseSettings, savePurchaseSettings, type PurchaseSettings } from '../../../api/financePurchases'
import { errorMessage } from '../../../lib/errors'
import { cardStyle, inputStyle } from '../../../lib/financePurchases'
import { useToast } from '../../ToastContext'

/** Ajustes del flujo de compras (quien gestiona Finanzas) */
export default function SettingsPanel({ onClose }: { onClose: () => void }) {
  const { addToast } = useToast()
  const [s, setS] = useState<PurchaseSettings | null>(null)

  useEffect(() => {
    fetchPurchaseSettings().then(setS).catch((e) => addToast(errorMessage(e), 'error'))
  }, [addToast])

  if (!s) return null
  const save = async () => {
    try {
      setS(await savePurchaseSettings(s))
      addToast('Ajustes guardados', 'success')
      onClose()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }
  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Ajustes de compras</h2>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cotizaciones minimas
          <input type="number" min={1} max={10} value={s.min_quotes} onChange={(e) => setS({ ...s, min_quotes: Number(e.target.value) })} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>...desde este monto ($)
          <input type="number" min={0} value={s.quote_threshold} onChange={(e) => setS({ ...s, quote_threshold: Number(e.target.value) })} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
      </div>
      <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
        <input type="checkbox" checked={s.require_passkey} onChange={(e) => setS({ ...s, require_passkey: e.target.checked })} />
        Exigir passkey para aprobar compras
      </label>
      <div className="flex gap-2 justify-end">
        <button onClick={onClose} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cerrar</button>
        <button onClick={save} className="px-3 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
      </div>
    </div>
  )
}
