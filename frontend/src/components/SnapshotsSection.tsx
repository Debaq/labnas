import { useEffect, useState } from 'react'
import { Camera, Loader2 } from 'lucide-react'
import { fetchSnapshots, saveSnapshotConfig, snapshotNow, type SnapshotsInfo } from '../api'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

/** Admin: snapshots btrfs/ZFS programados (versiones instantaneas de carpetas completas) */
export default function SnapshotsSection() {
  const [info, setInfo] = useState<SnapshotsInfo | null>(null)
  const [interval, setIntervalH] = useState(0)
  const [keep, setKeep] = useState(24)
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)

  async function load() {
    try {
      const i = await fetchSnapshots()
      setInfo(i)
      setIntervalH(i.config.interval_hours)
      setKeep(i.config.keep)
    } catch (e) { setMsg({ ok: false, text: errorMessage(e) }) }
  }
  useEffect(() => { void load() }, [])

  async function run(action: () => Promise<unknown>, ok: string) {
    setBusy(true)
    setMsg(null)
    try { await action(); setMsg({ ok: true, text: ok }); await load() }
    catch (e) { setMsg({ ok: false, text: errorMessage(e) }) }
    finally { setBusy(false) }
  }

  return (
    <section className="mt-8">
      <div className="flex items-center gap-3 mb-4">
        <Camera size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Snapshots (btrfs / ZFS)</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Versiones instantáneas de carpetas completas que casi no ocupan espacio: protegen contra borrados y ransomware.
          Se ven (solo lectura) en <code>&lt;carpeta&gt;/.snapshots/</code> (btrfs) o <code>&lt;carpeta&gt;/.zfs/snapshot/</code> (ZFS); para recuperar, copia desde ahí.
        </p>
        {info && !info.ready && (
          <p className="text-xs" style={{ color: 'var(--warning)' }}>
            {info.error ?? 'No configurado.'} Ejecuta en el servidor: <code>sudo bash setup-snapshots.sh &lt;usuario&gt; &lt;subvolumen o dataset&gt;</code>
          </p>
        )}
        {info?.ready && (
          <>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 items-end">
              <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                <span>Cada (horas, 0 = desactivado)</span>
                <input type="number" min={0} max={168} value={interval} onChange={e => setIntervalH(parseInt(e.target.value) || 0)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              </label>
              <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                <span>Conservar por carpeta</span>
                <input type="number" min={1} max={1000} value={keep} onChange={e => setKeep(parseInt(e.target.value) || 1)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              </label>
              <div className="flex gap-2">
                <button onClick={() => run(() => saveSnapshotConfig({ interval_hours: interval, keep }), 'Guardado')} disabled={busy}
                  className="px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
                <button onClick={() => run(snapshotNow, 'Snapshot creado')} disabled={busy}
                  className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium" style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
                  {busy && <Loader2 size={14} className="animate-spin" />}Ahora
                </button>
              </div>
            </div>
            {info.paths.map(p => (
              <div key={p.path} className="text-xs space-y-1">
                <p className="font-mono" style={{ color: 'var(--text-primary)' }}>{p.path}</p>
                {p.error
                  ? <p style={{ color: 'var(--danger)' }}>{p.error}</p>
                  : <p style={{ color: 'var(--text-secondary)' }}>{p.snapshots.length} snapshots{p.snapshots[0] ? ` · último ${p.snapshots[0]}` : ''}</p>}
              </div>
            ))}
          </>
        )}
        {msg && <p className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</p>}
      </div>
    </section>
  )
}
