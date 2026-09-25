import { useEffect, useState } from 'react'
import { ShieldCheck, Loader2, Copy, Download } from 'lucide-react'
import {
  fetchTwoFactor, startTwoFactor, enableTwoFactor, disableTwoFactor, regenerateRecoveryCodes, createAppPassword,
  type TwoFactorStatus, type TwoFactorSetup,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const muted = { color: 'var(--text-secondary)' }
const primary = { backgroundColor: 'var(--accent)', color: '#fff' }
const secondary = { color: 'var(--text-secondary)', border: '1px solid var(--border)' }

type Action = 'recovery' | 'app' | 'disable'

/** Muestra un secreto una sola vez (codigos de recuperacion o contrasena de aplicacion) */
function OneTime({ title, lines, file, onDone }: { title: string; lines: string[]; file: string; onDone: () => void }) {
  const text = lines.join('\n')
  const download = () => {
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([text + '\n'], { type: 'text/plain' }))
    a.download = file
    a.click()
    URL.revokeObjectURL(a.href)
  }
  return (
    <div className="space-y-3">
      <p className="text-sm" style={{ color: 'var(--warning)' }}>{title} Solo se muestra esta vez.</p>
      <div className="rounded-lg p-3 grid grid-cols-2 gap-x-6 gap-y-1 font-mono text-sm w-fit"
        style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
        {lines.map((l) => <span key={l}>{l}</span>)}
      </div>
      <div className="flex gap-2">
        <button onClick={() => navigator.clipboard?.writeText(text)} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs" style={secondary}>
          <Copy size={13} /> Copiar
        </button>
        <button onClick={download} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs" style={secondary}>
          <Download size={13} /> Descargar
        </button>
        <button onClick={onDone} className="px-3 py-1.5 rounded-lg text-xs" style={primary}>Listo, los guardé</button>
      </div>
    </div>
  )
}

/** Doble factor (TOTP) de la cuenta propia */
export default function TwoFactorSection() {
  const { refreshUser, isAdmin } = useAuth()
  const [st, setSt] = useState<TwoFactorStatus | null>(null)
  const [password, setPassword] = useState('')
  const [code, setCode] = useState('')
  const [setup, setSetup] = useState<TwoFactorSetup | null>(null)
  const [action, setAction] = useState<Action | null>(null)
  const [secret, setSecret] = useState<{ title: string; lines: string[]; file: string } | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const reload = () => fetchTwoFactor().then(setSt).catch((e) => setError(errorMessage(e)))
  useEffect(() => { reload() }, [])

  function reset() {
    setPassword('')
    setCode('')
    setSetup(null)
    setAction(null)
    setError(null)
  }

  async function run(fn: () => Promise<void>) {
    setBusy(true)
    setError(null)
    try { await fn() } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  const start = () => run(async () => { setSetup(await startTwoFactor(password)); setPassword('') })
  const confirm = () => run(async () => {
    const codes = await enableTwoFactor(code)
    reset()
    setSecret({ title: 'Códigos de recuperación: cada uno sirve una vez si pierdes el teléfono.', lines: codes, file: 'labnas-recuperacion.txt' })
    await reload()
    await refreshUser()
  })
  const act = () => run(async () => {
    if (action === 'recovery') {
      const codes = await regenerateRecoveryCodes(code)
      setSecret({ title: 'Nuevos códigos de recuperación (los anteriores ya no sirven).', lines: codes, file: 'labnas-recuperacion.txt' })
    } else if (action === 'app') {
      const pw = await createAppPassword(code)
      setSecret({ title: 'Contraseña para WebDAV (reemplaza a la anterior).', lines: [pw], file: 'labnas-webdav.txt' })
    } else if (action === 'disable') {
      await disableTwoFactor(password, code)
    }
    reset()
    await reload()
    await refreshUser()
  })

  const codeInput = (
    <input value={code} onChange={(e) => setCode(e.target.value)} placeholder="Código de la app" inputMode="numeric" autoComplete="one-time-code"
      className="px-3 py-1.5 rounded-lg text-sm font-mono outline-none w-44" style={inputStyle} />
  )
  const passwordInput = (
    <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Contraseña"
      className="px-3 py-1.5 rounded-lg text-sm outline-none w-44" style={inputStyle} />
  )

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <ShieldCheck size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Doble factor</h2>
        {st?.enabled && (
          <span className="text-xs px-2 py-0.5 rounded-full" style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)' }}>Activo</span>
        )}
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        {!st ? <Loader2 size={18} className="animate-spin" /> : secret ? (
          <OneTime {...secret} onDone={() => setSecret(null)} />
        ) : !st.enabled ? (
          setup ? (
            <div className="space-y-3">
              <p className="text-sm" style={muted}>
                Escanea el código con Google Authenticator, Aegis, 2FAS u otra app TOTP y escribe el código de 6 dígitos que muestra.
              </p>
              <div className="flex flex-wrap items-start gap-4">
                <img src={`data:image/svg+xml;utf8,${encodeURIComponent(setup.qr_svg)}`} alt="Código QR" width={180} height={180}
                  className="rounded-lg bg-white" />
                <div className="space-y-2 text-xs" style={muted}>
                  <p>¿No puedes escanear? Ingresa esta clave a mano:</p>
                  <p className="font-mono break-all text-sm" style={{ color: 'var(--text-primary)' }}>{setup.secret.match(/.{1,4}/g)?.join(' ')}</p>
                </div>
              </div>
              <div className="flex items-center gap-2">
                {codeInput}
                <button onClick={confirm} disabled={busy || code.trim().length < 6} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-50" style={primary}>
                  {busy && <Loader2 size={13} className="animate-spin" />} Activar
                </button>
                <button onClick={reset} className="px-3 py-1.5 rounded-lg text-sm" style={secondary}>Cancelar</button>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <p className="text-sm" style={muted}>
                Además de la contraseña, pide un código de una app del teléfono al iniciar sesión.
                {isAdmin && <strong style={{ color: 'var(--text-primary)' }}> Muy recomendado para administradores.</strong>}
              </p>
              <div className="flex items-center gap-2">
                {passwordInput}
                <button onClick={start} disabled={busy || !password} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-50" style={primary}>
                  {busy && <Loader2 size={13} className="animate-spin" />} Activar doble factor
                </button>
              </div>
            </div>
          )
        ) : (
          <div className="space-y-3">
            <p className="text-sm" style={muted}>
              Al iniciar sesión se pide el código de la app. Quedan <strong style={{ color: st.recovery_left > 2 ? 'var(--text-primary)' : 'var(--danger)' }}>{st.recovery_left}</strong> códigos
              de recuperación. WebDAV no admite un segundo factor: usa una contraseña de aplicación
              {st.app_password ? ' (ya tienes una)' : ''}.
            </p>
            {!action ? (
              <div className="flex flex-wrap gap-2">
                <button onClick={() => setAction('recovery')} className="px-3 py-1.5 rounded-lg text-xs" style={secondary}>Nuevos códigos de recuperación</button>
                <button onClick={() => setAction('app')} className="px-3 py-1.5 rounded-lg text-xs" style={secondary}>
                  {st.app_password ? 'Nueva contraseña para WebDAV' : 'Contraseña para WebDAV'}
                </button>
                <button onClick={() => setAction('disable')} className="px-3 py-1.5 rounded-lg text-xs" style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>
                  Desactivar
                </button>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-2">
                {action === 'disable' && passwordInput}
                {codeInput}
                <button onClick={act} disabled={busy || !code.trim() || (action === 'disable' && !password)}
                  className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-50"
                  style={action === 'disable' ? { backgroundColor: 'var(--danger)', color: '#fff' } : primary}>
                  {busy && <Loader2 size={13} className="animate-spin" />}
                  {action === 'disable' ? 'Desactivar' : action === 'app' ? 'Generar' : 'Regenerar'}
                </button>
                <button onClick={reset} className="px-3 py-1.5 rounded-lg text-sm" style={secondary}>Cancelar</button>
              </div>
            )}
          </div>
        )}
        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      </div>
    </section>
  )
}
