import { useEffect, useState } from 'react'
import { Monitor, Copy, Check, Loader2, KeyRound, ChevronDown, ChevronRight, AlertTriangle } from 'lucide-react'
import { fetchIppInfo, regenerateIppToken, revokeIppToken } from '../api'
import type { CupsPrinter, IppInfo } from '../types'
import { errorMessage } from '../lib/errors'
import { useViaBridge } from '../lib/bridge'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

/** Copia al portapapeles; sin HTTPS el navegador no expone clipboard y se usa execCommand */
async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch { /* se intenta el metodo antiguo */ }
  const ta = document.createElement('textarea')
  ta.value = text
  ta.style.position = 'fixed'
  ta.style.opacity = '0'
  document.body.appendChild(ta)
  ta.select()
  const ok = document.execCommand('copy')
  document.body.removeChild(ta)
  return ok
}

/** Direcciones personales para agregar las impresoras de LabNAS en Windows, Linux o macOS */
export default function NetworkPrinterPanel({ printers }: { printers: CupsPrinter[] }) {
  const [info, setInfo] = useState<IppInfo | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState<string | null>(null)
  const [showHelp, setShowHelp] = useState(false)
  const viaBridge = useViaBridge()

  useEffect(() => {
    fetchIppInfo().then(setInfo).catch((e) => setError(errorMessage(e)))
  }, [])

  async function run(fn: () => Promise<void>) {
    setBusy(true)
    setError(null)
    try { await fn() } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  if (!info) return null
  const host = window.location.hostname
  const urlFor = (scheme: 'http' | 'ipp', printer: string) =>
    `${scheme}://${host}:${info.port}/ipp/${info.token}/${encodeURIComponent(printer)}`

  async function copy(key: string, text: string) {
    if (await copyText(text)) {
      setCopied(key)
      setTimeout(() => setCopied((c) => (c === key ? null : c)), 2000)
    }
  }

  return (
    <div className="rounded-xl p-5 space-y-4" style={card}>
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div className="flex items-center gap-2">
          <Monitor size={18} style={{ color: 'var(--accent)' }} />
          <span className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>
            Imprimir desde Windows, Linux o Mac
          </span>
        </div>
        {info.can_print && info.token && (
          <div className="flex items-center gap-2">
            <button
              disabled={busy}
              onClick={() => {
                if (!confirm('Se creara una direccion nueva y la actual dejara de funcionar en los equipos donde la agregaste. ¿Continuar?')) return
                run(async () => setInfo(await regenerateIppToken()))
              }}
              className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs"
              style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}
            >
              <KeyRound size={13} /> Regenerar
            </button>
            <button
              disabled={busy}
              onClick={() => {
                if (!confirm('Tus impresoras agregadas en otros equipos dejaran de funcionar. ¿Revocar?')) return
                run(async () => { await revokeIppToken(); setInfo({ ...info, token: null }) })
              }}
              className="px-2 py-1 rounded-lg text-xs"
              style={{ color: 'var(--danger)', border: '1px solid var(--border)' }}
            >
              Revocar
            </button>
          </div>
        )}
      </div>

      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Agrega las impresoras de LabNAS en tu computador como impresoras de red. Lo que imprimas pasa por LabNAS
        y queda contado a tu nombre en las estadisticas y costos. La direccion es <strong>personal</strong>: no la compartas.
      </p>

      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      {info.listen_error && (
        <p className="text-xs flex items-center gap-1" style={{ color: 'var(--warning)' }}>
          <AlertTriangle size={13} /> {info.listen_error}
        </p>
      )}
      {viaBridge && (
        <p className="text-xs flex items-center gap-1" style={{ color: 'var(--warning)' }}>
          <AlertTriangle size={13} /> Estas entrando desde internet: estas direcciones solo funcionan dentro de la red del laboratorio.
        </p>
      )}

      {!info.can_print ? (
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
          Necesitas el permiso de impresion. Pidelo al administrador.
        </p>
      ) : !info.token ? (
        <button
          disabled={busy}
          onClick={() => run(async () => setInfo(await regenerateIppToken()))}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
          style={{ color: 'var(--accent)', border: '1px solid var(--accent-alpha)', backgroundColor: 'var(--accent-alpha)' }}
        >
          {busy ? <Loader2 size={15} className="animate-spin" /> : <KeyRound size={15} />}
          Generar mi direccion de impresora
        </button>
      ) : (
        <div className="space-y-2">
          {printers.length === 0 && (
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>No hay impresoras configuradas en CUPS.</p>
          )}
          {printers.map((p) => {
            const url = urlFor('http', p.name)
            return (
              <div key={p.name} className="px-3 py-2 rounded-lg space-y-1" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{p.description}</p>
                <div className="flex items-center gap-2">
                  <code className="flex-1 text-xs break-all font-mono select-all" style={{ color: 'var(--text-secondary)' }}>{url}</code>
                  <button title="Copiar direccion" onClick={() => copy(p.name, url)} style={{ color: 'var(--accent)' }}>
                    {copied === p.name ? <Check size={16} /> : <Copy size={16} />}
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}

      {info.can_print && info.token && (
        <div>
          <button
            onClick={() => setShowHelp(!showHelp)}
            className="flex items-center gap-1 text-xs font-medium"
            style={{ color: 'var(--accent)' }}
          >
            {showHelp ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            Como agregarla
          </button>
          {showHelp && (
            <div className="mt-2 space-y-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
              <div>
                <p className="font-semibold" style={{ color: 'var(--text-primary)' }}>Windows 10 / 11</p>
                <ol className="list-decimal ml-5 space-y-0.5">
                  <li>Configuracion &gt; Bluetooth y dispositivos &gt; Impresoras y escaneres &gt; Agregar dispositivo.</li>
                  <li>Clic en <em>Agregar manualmente</em> (o <em>La impresora que deseo no esta en la lista</em>).</li>
                  <li>Elige <em>Seleccionar una impresora compartida por nombre</em> y pega la direccion (empieza con http://).</li>
                  <li>Siguiente. Windows usa su driver generico (<em>Microsoft IPP Class Driver</em>); no hay que instalar nada.</li>
                </ol>
              </div>
              <div>
                <p className="font-semibold" style={{ color: 'var(--text-primary)' }}>Linux</p>
                <p>En la terminal (cambia la impresora al final de la direccion si quieres otra):</p>
                {printers[0] && (
                  <code className="block mt-1 p-2 rounded font-mono break-all select-all" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                    lpadmin -p LabNAS-{printers[0].name} -E -v "{urlFor('ipp', printers[0].name)}" -m everywhere
                  </code>
                )}
              </div>
              <div>
                <p className="font-semibold" style={{ color: 'var(--text-primary)' }}>macOS</p>
                <p>
                  Ajustes del Sistema &gt; Impresoras &gt; Agregar &gt; pestana IP. Direccion: <code>{host}:{info.port}</code>,
                  protocolo: IPP, cola: <code className="break-all">ipp/{info.token}/</code> seguido del nombre de la impresora.
                </p>
              </div>
              <p>
                Solo funciona dentro de la red del laboratorio (puerto {info.port}). Si cambia la IP o el nombre del servidor,
                vuelve a agregar la impresora.
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
