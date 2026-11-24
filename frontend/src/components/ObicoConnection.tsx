import { useEffect, useState } from 'react'
import { Loader2, CheckCircle2, AlertTriangle } from 'lucide-react'
import { fetchObico, saveObico, type ObicoStatus } from '../api'
import { useToast } from './ToastContext'
import { errorMessage } from '../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
const btn = { color: 'var(--accent)', border: '1px solid var(--border)' }

/** Conexion con el ml_api de Obico, que corre aparte (setup-obico.sh). Solo admin. */
export default function ObicoConnection() {
  const { addToast } = useToast()
  const [status, setStatus] = useState<ObicoStatus | null>(null)
  const [url, setUrl] = useState('')
  const [labnasUrl, setLabnasUrl] = useState('')
  const [token, setToken] = useState('')
  const [busy, setBusy] = useState(false)

  const apply = (s: ObicoStatus) => {
    setStatus(s)
    setUrl(s.url)
    setLabnasUrl(s.labnas_url)
    setToken('')
  }

  useEffect(() => {
    fetchObico().then(apply).catch((e) => addToast(errorMessage(e), 'error'))
  }, [addToast])

  const save = async (removeToken = false) => {
    setBusy(true)
    try {
      apply(await saveObico({ url, labnas_url: labnasUrl, token: removeToken ? '' : token || undefined }))
      addToast('Conexión guardada', 'success')
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  if (!status) return null

  return (
    <div className="rounded-xl p-3 space-y-2" style={card}>
      <div className="flex items-center gap-2 text-xs flex-wrap">
        <span className="font-medium" style={{ color: 'var(--text-primary)' }}>Conexión con Obico</span>
        {status.ready ? (
          <span className="flex items-center gap-1" style={{ color: 'var(--success)' }}><CheckCircle2 size={12} /> Listo</span>
        ) : (
          <span className="flex items-center gap-1" style={{ color: 'var(--warning)' }}><AlertTriangle size={12} /> {status.error}</span>
        )}
      </div>
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Obico analiza la imagen en el propio servidor (se instala aparte con <code>setup-obico.sh</code>; ver docs/DETECCION.md).
        Las imágenes no salen del laboratorio.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
        <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
          <span>Dirección de Obico</span>
          <input value={url} onChange={(e) => setUrl(e.target.value)} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
        </label>
        <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }} title="Obico baja cada cuadro desde aquí con un enlace de un solo uso">
          <span>Dirección de LabNAS para Obico</span>
          <input value={labnasUrl} onChange={(e) => setLabnasUrl(e.target.value)} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
        </label>
        <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
          <span>Token {status.has_token ? '(guardado; vacío = no cambiar)' : '(opcional)'}</span>
          <input type="password" value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
        </label>
      </div>
      <div className="flex gap-2 justify-end">
        {status.has_token && (
          <button onClick={() => save(true)} disabled={busy} className="text-xs px-3 py-1.5 rounded-lg disabled:opacity-50" style={btn}>Quitar token</button>
        )}
        <button
          onClick={() => save()}
          disabled={busy}
          className="flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg disabled:opacity-50"
          style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}
        >
          {busy && <Loader2 size={13} className="animate-spin" />} Guardar y probar
        </button>
      </div>
    </div>
  )
}
