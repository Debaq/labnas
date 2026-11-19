import { useEffect, useState } from 'react'
import { Globe2, Download, Loader2, Link2, Unlink } from 'lucide-react'
import { fetchBridge, linkBridge, setBridgeEnabled, unlinkBridge, downloadBridgePackage, type BridgeInfo } from '../api'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function fmtBytes(n?: number) {
  if (n === undefined) return '?'
  if (n < 0) return 'sin límite'
  return `${Math.round(n / 1024 / 1024)} MB`
}

/** Admin: acceso remoto por el conector PHP en un hosting compartido */
export default function BridgeSection() {
  const [info, setInfo] = useState<BridgeInfo | null>(null)
  const [url, setUrl] = useState('')
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)

  async function load() {
    try { setInfo(await fetchBridge()) } catch (e) { setMsg({ ok: false, text: errorMessage(e) }) }
  }

  useEffect(() => {
    void load()
    const id = setInterval(() => { void load() }, 15000)
    return () => clearInterval(id)
  }, [])

  async function run(action: () => Promise<unknown>, ok: string) {
    setBusy(true)
    setMsg(null)
    try {
      await action()
      setMsg({ ok: true, text: ok })
      await load()
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setBusy(false)
    }
  }

  const st = info?.status
  const remote = st?.remote

  return (
    <section className="mt-8">
      <div className="flex items-center gap-3 mb-4">
        <Globe2 size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Acceso remoto (puente PHP)</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Usa LabNAS desde internet a través de un hosting compartido con PHP (7.0 o superior), sin abrir puertos en el router.
          Por el puente no están disponibles la terminal ni la administración.
        </p>

        <button onClick={() => run(downloadBridgePackage, 'Paquete descargado')} disabled={busy}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
          style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
          <Download size={16} />Descargar conector PHP
        </button>

        {info?.linked ? (
          <div className="space-y-3">
            <div className="text-sm space-y-1" style={{ color: 'var(--text-primary)' }}>
              <p>Enlazado con <strong className="font-mono">{info.url}</strong></p>
              <p className="text-xs" style={{ color: st?.connected ? 'var(--success)' : 'var(--danger)' }}>
                {!info.enabled ? 'Desactivado' : st?.connected ? `Conectado · ${st.served} pedidos atendidos` : `Sin conexión${st?.last_error ? `: ${st.last_error}` : ''}`}
              </p>
              {remote && (
                <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  Conector v{remote.version} · PHP {remote.php} · envío máx. {fmtBytes(remote.limits?.post_max_size)} · archivo máx. {fmtBytes(remote.limits?.upload_max_filesize)}
                  {remote.limits && remote.limits.max_execution_time > 0 ? ` · ${remote.limits.max_execution_time} s por pedido` : ''}
                </p>
              )}
            </div>
            <div className="flex items-center gap-3 flex-wrap">
              <button onClick={() => run(() => setBridgeEnabled(!info.enabled), info.enabled ? 'Puente desactivado' : 'Puente activado')} disabled={busy}
                className="px-4 py-2 rounded-lg text-sm font-medium" style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
                {info.enabled ? 'Desactivar' : 'Activar'}
              </button>
              <button onClick={() => run(unlinkBridge, 'Puente desenlazado: su clave ya no vale')} disabled={busy}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium" style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>
                <Unlink size={16} />Desenlazar
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <ol className="text-xs list-decimal list-inside space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <li>Crea un subdominio con HTTPS en tu hosting (por ejemplo <code>labnas.tudominio.cl</code>).</li>
              <li>Descarga el conector (botón de arriba) y descomprímelo en la carpeta del subdominio.</li>
              <li>Abre <code>https://tu-subdominio/instalar.php</code>, elige una carpeta de datos fuera de <code>public_html</code> y copia el código.</li>
              <li>Pega aquí la URL y el código.</li>
            </ol>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <input value={url} onChange={e => setUrl(e.target.value)} placeholder="https://labnas.tudominio.cl"
                className="px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
              <input value={code} onChange={e => setCode(e.target.value.toUpperCase())} placeholder="XXXX-XXXX-XXXX-XXXX"
                className="px-3 py-2 rounded-lg text-sm outline-none font-mono tracking-wider" style={inputStyle} />
            </div>
            <button onClick={() => run(() => linkBridge(url.trim(), code.trim()), 'Puente enlazado')} disabled={busy || !url.trim() || code.replace(/[^A-Z0-9]/g, '').length !== 16}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              {busy ? <Loader2 size={16} className="animate-spin" /> : <Link2 size={16} />}Enlazar
            </button>
          </div>
        )}
        {msg && <p className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</p>}
      </div>
    </section>
  )
}
