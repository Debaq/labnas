import { useEffect, useState, type ReactNode } from 'react'
import { Sparkles, FolderOpen, Wifi, Lock, Send, KeyRound, Loader2, Check, ChevronLeft, ChevronRight } from 'lucide-react'
import StorageRootsSection from './StorageRootsSection'
import HttpsSection from './HttpsSection'
import { finishSetup, getMdnsStatus, setMdns, fetchNotificationConfig, setBotToken, type SetupStatus } from '../api'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const muted = { color: 'var(--text-secondary)' }

function Hint({ children }: { children: ReactNode }) {
  return <p className="text-sm" style={muted}>{children}</p>
}

/** Paso mDNS: nombre en la red local (nombre.local) */
function MdnsStep({ suggested }: { suggested: string }) {
  const [st, setSt] = useState<{ enabled: boolean; hostname: string; url: string } | null>(null)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    getMdnsStatus()
      .then((s) => { setSt(s); setName(s.enabled ? s.hostname : (suggested || s.hostname)) })
      .catch((e) => setError(errorMessage(e)))
  }, [suggested])

  async function toggle(enabled: boolean) {
    setBusy(true)
    setError(null)
    try { setSt(await setMdns(enabled, name)) } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  if (!st) return error ? <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p> : <Loader2 size={18} className="animate-spin" />
  return (
    <div className="space-y-3">
      <Hint>
        Con mDNS los equipos de la red entran por nombre (<strong style={{ color: 'var(--text-primary)' }}>{name || 'labnas'}.local:3001</strong>)
        sin recordar la IP, y el visor de escritorio encuentra el NAS solo.
      </Hint>
      <div className="flex items-center gap-2">
        <input value={name} disabled={st.enabled} onChange={(e) => setName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
          placeholder="labnas" className="px-3 py-1.5 rounded-lg text-sm font-mono outline-none w-48 disabled:opacity-60" style={inputStyle} />
        <span className="text-sm" style={muted}>.local</span>
        <button onClick={() => toggle(!st.enabled)} disabled={busy || !name}
          className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-50"
          style={st.enabled ? { color: 'var(--danger)', border: '1px solid var(--danger)' } : { backgroundColor: 'var(--accent)', color: '#fff' }}>
          {busy && <Loader2 size={13} className="animate-spin" />} {st.enabled ? 'Desactivar' : 'Activar'}
        </button>
      </div>
      {st.enabled && <p className="text-sm" style={{ color: 'var(--success)' }}>Activo: {st.url}</p>}
      <Hint>Algunos clientes (Linux sin Avahi, Windows antiguo) necesitan resolver .local: ver <code>setup-mdns.sh</code> en la carpeta de LabNAS.</Hint>
      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
    </div>
  )
}

/** Paso Telegram: token del bot (opcional) */
function TelegramStep() {
  const [bot, setBot] = useState<string | null | undefined>(undefined)
  const [token, setToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetchNotificationConfig()
      .then((c) => setBot(c.bot_configured ? (c.bot_username ?? '') : null))
      .catch((e) => setError(errorMessage(e)))
  }, [])

  async function save() {
    setBusy(true)
    setError(null)
    try {
      const c = await setBotToken(token.trim())
      setBot(c.bot_configured ? (c.bot_username ?? '') : null)
      setToken('')
    } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  return (
    <div className="space-y-3">
      <Hint>
        Opcional. Un bot de Telegram avisa de impresiones terminadas, respaldos fallidos, discos con problemas y usuarios pendientes,
        y acepta comandos. Crea uno con <strong style={{ color: 'var(--text-primary)' }}>@BotFather</strong> (<code>/newbot</code>) y pega aquí el token.
      </Hint>
      {bot !== null && bot !== undefined ? (
        <p className="text-sm" style={{ color: 'var(--success)' }}>Bot configurado{bot ? `: @${bot}` : ''}. Cada persona debe enviarle <code>/start</code>.</p>
      ) : (
        <div className="flex items-center gap-2">
          <input value={token} onChange={(e) => setToken(e.target.value)} placeholder="123456:ABC-DEF..." type="password" autoComplete="off"
            className="flex-1 px-3 py-1.5 rounded-lg text-sm font-mono outline-none" style={inputStyle} />
          <button onClick={save} disabled={busy || !token.trim()} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy && <Loader2 size={13} className="animate-spin" />} Guardar
          </button>
        </div>
      )}
      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
    </div>
  )
}

/** Paso final: respaldar secret.key */
function SecretKeyStep({ path, confirmed, onConfirm }: { path: string; confirmed: boolean; onConfirm: (v: boolean) => void }) {
  return (
    <div className="space-y-3">
      <Hint>
        Los tokens y contraseñas guardados (bot, correo, API keys) se cifran con esta clave. <strong style={{ color: 'var(--text-primary)' }}>No va en
        los respaldos de LabNAS</strong>: si se pierde, al restaurar en otro equipo habrá que volver a ingresarlos.
      </Hint>
      <div className="rounded-lg p-3 text-sm font-mono break-all" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>{path}</div>
      <Hint>Cópiala a un pendrive o gestor de contraseñas, por ejemplo: <code className="break-all">cp {path} /media/pendrive/labnas-secret.key</code></Hint>
      <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
        <input type="checkbox" checked={confirmed} onChange={(e) => onConfirm(e.target.checked)} />
        Ya la respaldé (o lo haré ahora)
      </label>
    </div>
  )
}

/** Asistente de primer arranque para admins: se cierra al terminar u omitir */
export default function SetupWizard({ status, onClose }: { status: SetupStatus; onClose: () => void }) {
  const [step, setStep] = useState(0)
  const [keySaved, setKeySaved] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const steps: { title: string; icon: typeof Sparkles; body: ReactNode }[] = [
    {
      title: 'Bienvenido a LabNAS', icon: Sparkles, body: (
        <div className="space-y-3">
          <Hint>Tu cuenta es la de administrador. En unos pasos dejas lo básico listo; todo se puede cambiar después en Configuración.</Hint>
          <ul className="text-sm list-disc pl-5 space-y-1" style={muted}>
            <li>Qué carpetas se comparten y quién las ve</li>
            <li>Nombre del NAS en la red</li>
            <li>HTTPS</li>
            <li>Avisos por Telegram (opcional)</li>
            <li>Respaldo de la clave de cifrado</li>
          </ul>
          <Hint>Las cuentas nuevas quedan pendientes hasta que las apruebes en Configuración &gt; Usuarios.</Hint>
        </div>
      ),
    },
    { title: 'Carpetas compartidas', icon: FolderOpen, body: <StorageRootsSection /> },
    { title: 'Nombre en la red', icon: Wifi, body: <MdnsStep suggested={status.hostname} /> },
    { title: 'Conexión segura', icon: Lock, body: <HttpsSection /> },
    { title: 'Avisos por Telegram', icon: Send, body: <TelegramStep /> },
    { title: 'Clave de cifrado', icon: KeyRound, body: <SecretKeyStep path={status.secret_key_path} confirmed={keySaved} onConfirm={setKeySaved} /> },
  ]
  const last = step === steps.length - 1
  const { title, icon: Icon, body } = steps[step]

  async function finish() {
    setBusy(true)
    setError(null)
    try { await finishSetup(); onClose() } catch (e) { setError(errorMessage(e)); setBusy(false) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.6)' }}>
      <div className="w-full max-w-3xl max-h-[90vh] flex flex-col rounded-2xl shadow-2xl"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
        <div className="flex items-center gap-3 px-6 pt-5 pb-3">
          <Icon size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold flex-1" style={{ color: 'var(--text-primary)' }}>{title}</h2>
          <span className="text-xs" style={muted}>Paso {step + 1} de {steps.length}</span>
        </div>
        <div className="flex gap-1 px-6 pb-4">
          {steps.map((_, i) => (
            <div key={i} className="h-1 flex-1 rounded-full" style={{ backgroundColor: i <= step ? 'var(--accent)' : 'var(--border)' }} />
          ))}
        </div>

        <div className="flex-1 overflow-y-auto px-6 pb-4">{body}</div>

        <div className="flex items-center gap-2 px-6 py-4" style={{ borderTop: '1px solid var(--border)' }}>
          <button onClick={finish} disabled={busy} className="text-sm underline disabled:opacity-50" style={muted}>
            Omitir, lo configuro después
          </button>
          <div className="flex-1" />
          {error && <span className="text-sm" style={{ color: 'var(--danger)' }}>{error}</span>}
          {step > 0 && (
            <button onClick={() => setStep(step - 1)} disabled={busy} className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm"
              style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
              <ChevronLeft size={15} /> Atrás
            </button>
          )}
          {last ? (
            <button onClick={finish} disabled={busy || !keySaved} className="flex items-center gap-1 px-4 py-2 rounded-lg text-sm disabled:opacity-50"
              style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              {busy ? <Loader2 size={15} className="animate-spin" /> : <Check size={15} />} Terminar
            </button>
          ) : (
            <button onClick={() => setStep(step + 1)} className="flex items-center gap-1 px-4 py-2 rounded-lg text-sm"
              style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              Siguiente <ChevronRight size={15} />
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
