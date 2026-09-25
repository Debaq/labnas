import { useEffect, useState } from 'react'
import { Film, X, Loader2, AlertTriangle } from 'lucide-react'
import { fetchTimelapse, saveTimelapse, type TimelapseSettings } from '../../api'
import type { Printer3DConfig } from '../../types'
import { useAuth } from '../../auth/AuthContext'
import { errorMessage } from '../../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

/** Timelapse por impresora: fotos durante la impresion y video al terminar */
export default function TimelapseModal({ printers, onClose }: { printers: Printer3DConfig[]; onClose: () => void }) {
  const { user } = useAuth()
  const canEdit = user?.role === 'admin' || user?.role === 'operador'
  const [cfg, setCfg] = useState<TimelapseSettings | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  useEffect(() => { fetchTimelapse().then(setCfg).catch((e) => setError(errorMessage(e))) }, [])

  async function save() {
    if (!cfg) return
    setSaving(true)
    setError(null)
    setSaved(false)
    try {
      setCfg(await saveTimelapse({ printers: cfg.printers, interval_secs: cfg.interval_secs, dir: cfg.dir }))
      setSaved(true)
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setSaving(false)
    }
  }

  function toggle(id: string) {
    if (!cfg) return
    setCfg({ ...cfg, printers: cfg.printers.includes(id) ? cfg.printers.filter((p) => p !== id) : [...cfg.printers, id] })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.5)' }} onClick={onClose}>
      <div className="w-full max-w-lg rounded-xl" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between p-4" style={{ borderBottom: '1px solid var(--border)' }}>
          <div className="flex items-center gap-2">
            <Film size={18} style={{ color: 'var(--accent)' }} />
            <h2 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Timelapse</h2>
          </div>
          <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
        </div>

        {!cfg ? (
          <div className="p-6"><Loader2 size={18} className="animate-spin" /></div>
        ) : (
          <div className="p-4 space-y-4">
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              Mientras una impresora elegida imprime, se guarda una foto de su camara cada {cfg.interval_secs} s.
              Al terminar se arma <code>timelapse.mp4</code> en la carpeta de destino y se avisa.
            </p>
            {!cfg.ffmpeg && (
              <p className="flex items-center gap-2 text-xs" style={{ color: 'var(--warning)' }}>
                <AlertTriangle size={14} /> ffmpeg no esta instalado en el servidor: se guardan las fotos pero no el video.
              </p>
            )}

            <div className="space-y-1">
              {printers.length === 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin impresoras.</p>}
              {printers.map((p) => {
                const noCamera = p.printer_type === 'FlashForge' && !p.camera_url
                return (
                  <label key={p.id} className="flex items-center gap-2 text-sm" style={{ color: noCamera ? 'var(--text-secondary)' : 'var(--text-primary)' }}>
                    <input type="checkbox" checked={cfg.printers.includes(p.id)} disabled={!canEdit || noCamera} onChange={() => toggle(p.id)} />
                    {p.name}{noCamera && <span className="text-xs">(sin camara)</span>}
                  </label>
                )
              })}
            </div>

            <div className="grid grid-cols-3 gap-2 items-center">
              <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Segundos entre fotos</label>
              <input type="number" min={5} max={3600} disabled={!canEdit} value={cfg.interval_secs}
                onChange={(e) => setCfg({ ...cfg, interval_secs: parseInt(e.target.value) || 30 })}
                className="col-span-2 px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
              <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Carpeta</label>
              <input disabled={!canEdit} value={cfg.dir} placeholder={cfg.effective_dir}
                onChange={(e) => setCfg({ ...cfg, dir: e.target.value })}
                className="col-span-2 px-3 py-1.5 rounded-lg text-sm font-mono outline-none" style={inputStyle} />
            </div>

            {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
            {saved && !error && <p className="text-sm" style={{ color: 'var(--success)' }}>Guardado</p>}
            {canEdit && (
              <div className="flex justify-end">
                <button onClick={save} disabled={saving} className="flex items-center gap-1 px-4 py-2 rounded-lg text-sm disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                  {saving && <Loader2 size={14} className="animate-spin" />} Guardar
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
