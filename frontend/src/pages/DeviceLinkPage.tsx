import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Loader2, MonitorSmartphone, Music } from 'lucide-react'
import { createDeviceToken } from '../api'
import { useAuth } from '../auth/AuthContext'
import { errorMessage } from '../lib/errors'

/**
 * /vincular?port=&state=&name= — la app de bandeja (labnas-tray) abre esta pagina en el
 * navegador. Al autorizar se crea un token de dispositivo y se le entrega por la
 * direccion local (127.0.0.1) donde la app espera, como el login de las apps de terminal.
 */
export default function DeviceLinkPage() {
  const navigate = useNavigate()
  const { user } = useAuth()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const req = useMemo(() => {
    const q = new URLSearchParams(window.location.search)
    const port = Number(q.get('port'))
    const state = q.get('state') ?? ''
    const name = (q.get('name') ?? '').replace(/[\p{C}]/gu, '').trim().slice(0, 60) || 'Dispositivo'
    const valid = Number.isInteger(port) && port >= 1024 && port <= 65535 && /^[0-9a-f]{32}$/.test(state)
    return valid ? { port, state, name } : null
  }, [])

  function back(params: Record<string, string>) {
    if (!req) return
    const qs = new URLSearchParams({ state: req.state, ...params })
    window.location.href = `http://127.0.0.1:${req.port}/callback?${qs}`
  }

  async function authorize() {
    if (!req) return
    setBusy(true)
    setError(null)
    try {
      const created = await createDeviceToken(req.name)
      back({ token: created.token, user: user?.username ?? '' })
    } catch (e) {
      setError(errorMessage(e))
      setBusy(false)
    }
  }

  const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

  return (
    <div className="max-w-lg mx-auto mt-10 p-4">
      <div className="rounded-xl p-6 space-y-4" style={card}>
        <div className="flex items-center gap-3">
          <MonitorSmartphone size={24} style={{ color: 'var(--accent)' }} />
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Vincular dispositivo</h1>
        </div>
        {!req ? (
          <p className="text-sm" style={{ color: 'var(--danger)' }}>
            El enlace de vinculación no es válido. Vuelve a elegir «Vincular» en el icono de LabNAS de la bandeja.
          </p>
        ) : (
          <>
            <p className="text-sm" style={{ color: 'var(--text-primary)' }}>
              El control de LabNAS en <strong>{req.name}</strong> pide permiso para usar tu cuenta
              (<strong>{user?.username}</strong>).
            </p>
            <div className="flex items-start gap-2 text-xs p-3 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
              <Music size={14} className="shrink-0 mt-0.5" />
              <span>
                Solo podrá ver qué suena y pausar, pasar o retroceder canciones y cambiar el volumen. No tendrá acceso a
                archivos ni a nada más. Puedes quitarlo cuando quieras en Configuración › General › Dispositivos vinculados.
              </span>
            </div>
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              Autoriza solo si acabas de pedirlo desde el icono de la bandeja en este computador.
            </p>
            {error && <p className="text-xs" style={{ color: 'var(--danger)' }}>{error}</p>}
            <div className="flex gap-2 justify-end">
              <button onClick={() => { back({ error: 'cancelado' }) }} disabled={busy}
                className="px-4 py-2 rounded-lg text-sm" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                Cancelar
              </button>
              <button onClick={authorize} disabled={busy}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {busy && <Loader2 size={16} className="animate-spin" />}Autorizar
              </button>
            </div>
          </>
        )}
        {!req && (
          <button onClick={() => navigate('/dashboard')} className="text-xs underline" style={{ color: 'var(--accent)' }}>Ir al inicio</button>
        )}
      </div>
    </div>
  )
}
