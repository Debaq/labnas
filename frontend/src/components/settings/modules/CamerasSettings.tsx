import { useEffect, useState } from 'react'
import { fetchCameras, saveCameraSettings } from '../../../api'
import { errorMessage } from '../../../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

/** Carpeta de fotos y grabaciones, y duracion maxima de cada grabacion */
export default function CamerasSettings() {
  const [folder, setFolder] = useState('')
  const [maxMinutes, setMaxMinutes] = useState(60)
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)

  useEffect(() => {
    fetchCameras().then(d => { setFolder(d.folder); setMaxMinutes(d.max_minutes) }).catch(() => {})
  }, [])

  async function save() {
    setBusy(true)
    setMsg(null)
    try {
      await saveCameraSettings(folder.trim(), maxMinutes)
      setMsg({ ok: true, text: 'Ajustes guardados' })
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-5 grid grid-cols-1 md:grid-cols-3 gap-3 items-end" style={card}>
      <label className="text-xs space-y-1 md:col-span-2" style={{ color: 'var(--text-secondary)' }}>
        <span>Carpeta del NAS para fotos y grabaciones (una subcarpeta por camara)</span>
        <input value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="/home/lab/Camaras" className="w-full rounded-lg px-2 py-1.5 text-sm font-mono" style={input} />
      </label>
      <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
        <span>Duracion maxima por grabacion (minutos)</span>
        <input type="number" min={1} max={600} value={maxMinutes} onChange={(e) => setMaxMinutes(Number(e.target.value))} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
      </label>
      <div className="md:col-span-3 flex items-center justify-end gap-3">
        {msg && <span className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</span>}
        <button disabled={busy} onClick={save} className="px-4 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          Guardar
        </button>
      </div>
    </div>
  )
}
