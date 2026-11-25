import { useEffect, useState } from 'react'
import { Check, Copy, Download, MonitorSmartphone, Trash2 } from 'lucide-react'
import { fetchDeviceTokens, deleteDeviceToken, type DeviceToken } from '../api'
import { errorMessage } from '../lib/errors'

type Platform = 'windows-x86_64' | 'linux-x86_64' | 'linux-aarch64'

const PLATFORMS: { id: Platform; label: string }[] = [
  { id: 'windows-x86_64', label: 'Windows' },
  { id: 'linux-x86_64', label: 'Linux (x86_64)' },
  { id: 'linux-aarch64', label: 'Linux (ARM64)' },
]

/** Sistema de quien mira la pagina, para destacar su descarga */
function detectPlatform(): Platform | null {
  const ua = navigator.userAgent
  if (/Windows/i.test(ua)) return 'windows-x86_64'
  if (/Linux/i.test(ua) && !/Android/i.test(ua)) return /aarch64|arm64/i.test(ua) ? 'linux-aarch64' : 'linux-x86_64'
  return null
}

/** Dispositivos vinculados a la cuenta (labnas-tray en la bandeja del sistema) */
export default function DeviceTokensSection() {
  const [list, setList] = useState<DeviceToken[]>([])
  const [error, setError] = useState<string | null>(null)
  const mine = detectPlatform()
  const origin = window.location.origin
  const [copied, setCopied] = useState(false)

  async function copyOrigin() {
    try { await navigator.clipboard.writeText(origin); setCopied(true); setTimeout(() => setCopied(false), 2000) } catch { /* sin portapapeles */ }
  }

  async function load() {
    try { setList(await fetchDeviceTokens()) } catch { /* sin sesion */ }
  }
  useEffect(() => { void load() }, [])

  async function remove(d: DeviceToken) {
    if (!confirm(`Desvincular "${d.name}"? Su control de música dejará de funcionar.`)) return
    try { await deleteDeviceToken(d.id); await load() } catch (e) { setError(errorMessage(e)) }
  }

  return (
    <section id="bandeja" className="mt-8 scroll-mt-4">
      <div className="flex items-center gap-3 mb-4">
        <MonitorSmartphone size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Dispositivos vinculados</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Computadores con <strong>labnas-tray</strong>, el control de música en la bandeja del sistema (Windows y Linux).
          Se vinculan desde el propio icono, con «Vincular con mi cuenta», y solo pueden controlar el reproductor.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          {PLATFORMS.map(p => (
            <a key={p.id} href={`/api/music/tray/${p.id}?server=${encodeURIComponent(origin)}`} download
              className="flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-medium hover:opacity-80"
              style={p.id === mine
                ? { backgroundColor: 'var(--accent)', color: '#fff' }
                : { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
              <Download size={14} />Descargar para {p.label}
            </a>
          ))}
        </div>
        <ul className="text-xs list-disc pl-5 space-y-0.5" style={{ color: 'var(--text-secondary)' }}>
          <li><strong>Windows:</strong> abre el <code>.exe</code> descargado (conviene dejarlo en una carpeta fija, no en Descargas). Si Windows avisa que la app no es conocida, elige «Más información» › «Ejecutar de todas formas».</li>
          <li><strong>Linux:</strong> descomprime y ejecuta <code>./install.sh</code> dentro de la carpeta. En GNOME hace falta la extensión AppIndicator para ver el icono.</li>
          <li>La copia descargada ya sabe la dirección de este LabNAS: al abrirla te lleva directo a vincular tu cuenta.</li>
        </ul>
        <div className="flex items-center gap-2 text-xs flex-wrap" style={{ color: 'var(--text-secondary)' }}>
          Si te la pide, la dirección es
          <code className="px-2 py-1 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>{origin}</code>
          <button onClick={copyOrigin} className="p-1 rounded hover:opacity-80" title="Copiar" style={{ color: 'var(--accent)' }}>
            {copied ? <Check size={14} /> : <Copy size={14} />}
          </button>
        </div>
        {list.length === 0 ? (
          <p className="text-xs" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>Ningún dispositivo vinculado.</p>
        ) : (
          <div className="space-y-2">
            {list.map(d => (
              <div key={d.id} className="flex items-center justify-between gap-3 text-sm">
                <div className="min-w-0">
                  <p className="truncate" style={{ color: 'var(--text-primary)' }}>{d.name}</p>
                  <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                    vinculado {new Date(d.created_at).toLocaleDateString()}
                    {d.last_used ? ` · último uso ${new Date(d.last_used).toLocaleString()}` : ' · sin uso'}
                  </p>
                </div>
                <button title="Desvincular" onClick={() => remove(d)} className="p-2 rounded-lg" style={{ color: 'var(--danger)' }}>
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
          </div>
        )}
        {error && <p className="text-xs" style={{ color: 'var(--danger)' }}>{error}</p>}
      </div>
    </section>
  )
}
