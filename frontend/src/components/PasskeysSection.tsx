import { useEffect, useState } from 'react'
import { Fingerprint, Loader2, Plus, Trash2 } from 'lucide-react'
import { fetchPasskeys, registerPasskey, deletePasskey, passkeysSupported, type PasskeyInfo } from '../api'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

/** Passkeys del usuario: entrar con huella, Face ID, Windows Hello o una llave, sin contraseña */
export default function PasskeysSection() {
  const [list, setList] = useState<PasskeyInfo[]>([])
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const supported = passkeysSupported()

  async function load() {
    try { setList(await fetchPasskeys()) } catch { /* sin sesion */ }
  }
  useEffect(() => { void load() }, [])

  async function add() {
    setBusy(true)
    setMsg(null)
    try {
      await registerPasskey(name.trim() || 'Passkey')
      setName('')
      setMsg({ ok: true, text: 'Passkey guardada: ya puedes entrar con ella' })
      await load()
    } catch (e) {
      setMsg({ ok: false, text: e instanceof DOMException && e.name === 'NotAllowedError' ? 'Operación cancelada' : errorMessage(e) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="mt-8">
      <div className="flex items-center gap-3 mb-4">
        <Fingerprint size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Passkeys</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Entra sin contraseña con la huella, Face ID, Windows Hello o una llave de seguridad. La passkey queda ligada a la dirección
          con que entras ahora{supported ? <> (<strong>{window.location.hostname}</strong>)</> : ''}.
        </p>
        {!supported && (
          <p className="text-xs" style={{ color: 'var(--warning)' }}>
            Aquí no se pueden usar: las passkeys necesitan HTTPS y un nombre de dominio (no una IP). Úsalas entrando por el puente
            (por ejemplo https://labnas.tudominio.cl) o por https://labnas.local con el certificado instalado.
          </p>
        )}
        {list.length > 0 && (
          <div className="space-y-2">
            {list.map(p => (
              <div key={p.id} className="flex items-center justify-between gap-3 text-sm">
                <div className="min-w-0">
                  <p style={{ color: 'var(--text-primary)' }}>{p.name}</p>
                  <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                    {p.rp_id} · creada {new Date(p.created_at).toLocaleDateString()}
                    {p.last_used ? ` · último uso ${new Date(p.last_used).toLocaleString()}` : ''}
                  </p>
                </div>
                <button title="Eliminar" onClick={() => { if (confirm(`Eliminar la passkey "${p.name}"?`)) deletePasskey(p.id).then(load).catch(e => setMsg({ ok: false, text: errorMessage(e) })) }}
                  className="p-2 rounded-lg" style={{ color: 'var(--danger)' }}><Trash2 size={15} /></button>
              </div>
            ))}
          </div>
        )}
        {supported && (
          <div className="flex items-center gap-2 flex-wrap">
            <input value={name} onChange={e => setName(e.target.value)} placeholder="Nombre (ej. Celular)" maxLength={60}
              className="flex-1 min-w-[12rem] px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
            <button onClick={add} disabled={busy} className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              {busy ? <Loader2 size={16} className="animate-spin" /> : <Plus size={16} />}Agregar passkey
            </button>
          </div>
        )}
        {msg && <p className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</p>}
      </div>
    </section>
  )
}
