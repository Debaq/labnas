import { useEffect, useRef, useState } from 'react'
import { Copy, ExternalLink, Loader2, LogIn } from 'lucide-react'
import { fetchMsOAuthConfig, fetchMsOAuthStatus, startMsOAuth, type MsFlowStatus } from '../api'
import { useEvent } from '../events/useEvents'
import { useToast } from './ToastContext'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

/** Conecta una cuenta Microsoft (Outlook, Hotmail, Microsoft 365) con el codigo de dispositivo */
export default function MicrosoftConnect({ initialEmail, onConnected }: { initialEmail?: string; onConnected: () => void }) {
  const { addToast } = useToast()
  const [configured, setConfigured] = useState<boolean | null>(null)
  const [email, setEmail] = useState(initialEmail ?? '')
  const [flow, setFlow] = useState<MsFlowStatus | null>(null)
  const [starting, setStarting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const done = useRef(false)

  useEffect(() => {
    fetchMsOAuthConfig().then(c => setConfigured(c.configured)).catch(() => setConfigured(false))
  }, [])

  function handleStatus(s: MsFlowStatus) {
    if (s.status === 'conectado') {
      if (done.current) return
      done.current = true
      setFlow(null)
      addToast(s.message || 'Cuenta de Microsoft conectada', 'success')
      onConnected()
    } else if (s.status === 'error') {
      setFlow(null)
      setError(s.message)
    }
  }

  async function refreshStatus() {
    try { handleStatus(await fetchMsOAuthStatus()) } catch { /* reintenta en el siguiente ciclo */ }
  }

  // Respaldo del bus en vivo: consultar cada 3 s mientras se espera
  const waiting = flow?.status === 'pendiente'
  useEffect(() => {
    if (!waiting) return
    const id = setInterval(() => { void refreshStatus() }, 3000)
    return () => clearInterval(id)
    // refreshStatus solo usa setters y refs estables
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [waiting])

  useEvent('email.oauth', () => { void refreshStatus() })

  async function start() {
    setStarting(true)
    setError(null)
    done.current = false
    try {
      setFlow(await startMsOAuth(email.trim()))
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setStarting(false)
    }
  }

  if (configured === null) return <Loader2 size={18} className="animate-spin" style={{ color: 'var(--text-secondary)' }} />

  if (!configured) {
    return (
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        El administrador debe configurar la conexión con Microsoft (Client ID) más abajo en esta página.
      </p>
    )
  }

  if (flow?.status === 'pendiente' && flow.user_code) {
    return (
      <div className="space-y-4">
        <ol className="text-xs space-y-1 list-decimal list-inside" style={{ color: 'var(--text-secondary)' }}>
          <li>Abre la página de Microsoft (en este equipo o en tu celular).</li>
          <li>Ingresa el código de abajo.</li>
          <li>Inicia sesión con <strong style={{ color: 'var(--text-primary)' }}>{email}</strong> y acepta el acceso.</li>
        </ol>
        <div className="flex items-center gap-3 flex-wrap">
          <span className="text-3xl font-mono font-bold tracking-widest px-4 py-2 rounded-lg select-all"
            style={{ color: 'var(--text-primary)', backgroundColor: 'var(--bg-tertiary)' }}>
            {flow.user_code}
          </span>
          <button
            onClick={() => { navigator.clipboard.writeText(flow.user_code ?? '').then(() => addToast('Código copiado', 'success')).catch(() => {}) }}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium"
            style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
            <Copy size={14} />Copiar
          </button>
          <a href={flow.verification_uri ?? 'https://microsoft.com/devicelogin'} target="_blank" rel="noopener noreferrer"
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <ExternalLink size={14} />Abrir {flow.verification_uri?.replace(/^https?:\/\//, '') ?? 'microsoft.com/devicelogin'}
          </a>
        </div>
        <p className="text-xs flex items-center gap-2" style={{ color: 'var(--text-secondary)' }}>
          <Loader2 size={14} className="animate-spin" />Esperando autorización… (el código vence en unos 15 minutos)
        </p>
        <button onClick={() => setFlow(null)} className="text-xs underline" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
      </div>
    )
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 flex-wrap">
        <input type="email" value={email} onChange={e => setEmail(e.target.value)} placeholder="usuario@uach.cl"
          className="flex-1 min-w-[14rem] px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
        <button onClick={start} disabled={starting || !email.includes('@')}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-90"
          style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {starting ? <Loader2 size={16} className="animate-spin" /> : <LogIn size={16} />}
          Conectar con Microsoft
        </button>
      </div>
      {error && <p className="text-xs" style={{ color: 'var(--danger)' }}>{error}</p>}
      <p className="text-xs" style={{ color: 'var(--text-secondary)', opacity: 0.8 }}>
        LabNAS nunca ve tu contraseña: inicias sesión en Microsoft (con tu doble factor) y LabNAS recibe solo un permiso de lectura por IMAP, que puedes revocar en tu cuenta de Microsoft.
      </p>
    </div>
  )
}
