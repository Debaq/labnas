import { useEffect, useState } from 'react'
import { Lock, Loader2, Download, RefreshCw } from 'lucide-react'
import { fetchTls, saveTls, regenerateTls, type TlsStatus } from '../api'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

/** Admin: HTTPS (autofirmado o certificado propio) y redireccion desde HTTP */
export default function HttpsSection() {
  const [st, setSt] = useState<TlsStatus | null>(null)
  const [certPath, setCertPath] = useState('')
  const [keyPath, setKeyPath] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  function apply(s: TlsStatus) {
    setSt(s)
    setCertPath(s.cert_path)
    setKeyPath(s.key_path)
  }

  useEffect(() => { fetchTls().then(apply).catch((e) => setError(errorMessage(e))) }, [])

  async function run(fn: () => Promise<TlsStatus>, msg: string) {
    setBusy(true)
    setError(null)
    setNotice(null)
    try { apply(await fn()); setNotice(msg) } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  const save = (patch: Partial<TlsStatus>, msg: string) =>
    st && run(() => saveTls({ enabled: st.enabled, redirect: st.redirect, cert_path: certPath, key_path: keyPath, ...patch }), msg)

  const httpsUrl = `https://${window.location.hostname}:${st?.port ?? 3443}`

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <Lock size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>HTTPS</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        {!st ? <Loader2 size={18} className="animate-spin" /> : (
          <>
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              LabNAS atiende por HTTPS en <a href={httpsUrl} className="underline" style={{ color: 'var(--accent)' }}>{httpsUrl}</a> (y
              en 443 si tiene permiso), ademas del HTTP de siempre. Con HTTPS las contraseñas y la sesion no viajan en claro.
            </p>

            <div className="space-y-2 text-sm" style={{ color: 'var(--text-primary)' }}>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={st.enabled} disabled={busy}
                  onChange={(e) => save({ enabled: e.target.checked }, 'Guardado. Activar o desactivar HTTPS se aplica al reiniciar LabNAS.')} />
                HTTPS activo
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={st.redirect} disabled={busy || !st.enabled}
                  onChange={(e) => save({ redirect: e.target.checked }, 'Guardado. La redireccion se aplica al reiniciar LabNAS.')} />
                Redirigir HTTP a HTTPS <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>(excepto sensores, localhost y el visor de escritorio)</span>
              </label>
            </div>

            {st.error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{st.error}</p>}
            {st.cert && (
              <div className="rounded-lg p-3 text-xs space-y-1" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                <p>Certificado <strong style={{ color: 'var(--text-primary)' }}>{st.source}</strong> · vence {new Date(st.cert.not_after).toLocaleDateString()} ({st.cert.days_left} dias)</p>
                <p>Valido para: <span className="font-mono" style={{ color: 'var(--text-primary)' }}>{st.cert.names.join(', ') || '—'}</span></p>
                <p className="break-all">Huella SHA-256: <span className="font-mono" style={{ color: 'var(--text-primary)' }}>{st.cert.fingerprint_sha256}</span></p>
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              <a href="/api/tls/cert.pem" download="labnas.crt" className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs"
                style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}>
                <Download size={13} /> Descargar certificado
              </a>
              {st.source === 'autofirmado' && (
                <button onClick={() => { if (confirm('Generar un certificado nuevo? Los equipos que confiaban en el actual tendran que instalar el nuevo.')) run(regenerateTls, 'Certificado regenerado') }}
                  disabled={busy} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs disabled:opacity-50"
                  style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                  <RefreshCw size={13} /> Regenerar (si cambio la IP o el nombre)
                </button>
              )}
            </div>
            {st.source === 'autofirmado' && (
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                El navegador avisara que el certificado no es de confianza hasta instalarlo en cada equipo (compara la huella).
                Para evitarlo usa un certificado propio.
              </p>
            )}

            <div className="pt-3 space-y-2" style={{ borderTop: '1px solid var(--border)' }}>
              <p className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Certificado propio (opcional)</p>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                <input value={certPath} onChange={(e) => setCertPath(e.target.value)} placeholder="/ruta/cert.pem"
                  className="px-3 py-1.5 rounded-lg text-xs font-mono outline-none" style={inputStyle} />
                <input value={keyPath} onChange={(e) => setKeyPath(e.target.value)} placeholder="/ruta/key.pem"
                  className="px-3 py-1.5 rounded-lg text-xs font-mono outline-none" style={inputStyle} />
              </div>
              <div className="flex items-center gap-2">
                <button onClick={() => save({}, 'Certificado aplicado')} disabled={busy}
                  className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                  {busy && <Loader2 size={12} className="animate-spin" />} Aplicar
                </button>
                {(certPath || keyPath) && (
                  <button onClick={() => { setCertPath(''); setKeyPath(''); if (st) run(() => saveTls({ enabled: st.enabled, redirect: st.redirect, cert_path: '', key_path: '' }), 'Volviste al autofirmado') }}
                    disabled={busy} className="px-3 py-1.5 rounded-lg text-xs" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                    Usar autofirmado
                  </button>
                )}
              </div>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                Con Tailscale se obtiene uno valido sin advertencias: <code>sudo tailscale cert --cert-file ~/.labnas/tls/ts.crt --key-file ~/.labnas/tls/ts.key &lt;equipo&gt;.&lt;tailnet&gt;.ts.net</code> (y darle
                permisos de lectura al usuario de LabNAS). Se recarga sin reiniciar.
              </p>
            </div>

            {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
            {notice && !error && <p className="text-sm" style={{ color: 'var(--success)' }}>{notice}</p>}
          </>
        )}
      </div>
    </section>
  )
}
