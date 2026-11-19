import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Video, Camera, Circle, Square, Maximize2, X, Loader2, Plus, Pencil, Trash2, Download, Settings2, Search, AlertTriangle, ShieldCheck,
} from 'lucide-react'
import {
  fetchCameras, createCamera, updateCamera, deleteCamera, saveCameraSettings, detectCameraDevices,
  startCameraRecording, stopCameraRecording, takeCameraPhoto, fetchCameraRecordings, deleteCameraRecording,
  cameraStreamUrl, cameraFrameUrl, cameraRecordingUrl,
} from '../api'
import type { CamerasResponse, CameraView, CameraBody, CameraDetect, CameraRecording } from '../types'
import { useAuth } from '../auth/AuthContext'
import { useEvent } from '../events/useEvents'
import { useToast } from '../components/ToastContext'
import { errorMessage } from '../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
const btn = { color: 'var(--accent)', border: '1px solid var(--border)' }

function elapsed(since: string, now: number): string {
  const s = Math.max(0, Math.floor((now - new Date(since).getTime()) / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = s % 60
  const mm = String(m).padStart(2, '0')
  const ss = String(sec).padStart(2, '0')
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`
}

function fmtSize(n: number | null): string {
  if (!n) return '—'
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`
}

/** Miniatura que se refresca cada segundo (sin abrir un stream por camara) */
function Thumb({ id }: { id: string }) {
  const [t, setT] = useState(() => Date.now())
  const [failed, setFailed] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearTimeout(timer.current), [])
  const next = (ms: number) => {
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setT(Date.now()), ms)
  }
  return (
    <div className="relative aspect-video bg-black flex items-center justify-center overflow-hidden">
      <img
        src={cameraFrameUrl(id, t)}
        alt=""
        className="w-full h-full object-contain"
        onLoad={() => { setFailed(false); next(document.hidden ? 5000 : 1000) }}
        onError={() => { setFailed(true); next(5000) }}
      />
      {failed && (
        <span className="absolute text-xs" style={{ color: '#ccc' }}>Sin imagen</span>
      )}
    </div>
  )
}

const emptyForm: CameraBody = { name: '', kind: 'usb', device: '', url: '', resolution: '', audio: '' }

export default function CamerasPage() {
  const { isAdmin } = useAuth()
  const { addToast } = useToast()
  const [data, setData] = useState<CamerasResponse | null>(null)
  const [recordings, setRecordings] = useState<CameraRecording[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [live, setLive] = useState<CameraView | null>(null)
  const [now, setNow] = useState(() => Date.now())
  // Ajustes (admin)
  const [showSettings, setShowSettings] = useState(false)
  const [folder, setFolder] = useState('')
  const [maxMinutes, setMaxMinutes] = useState(60)
  // Formulario de camara (admin)
  const [editing, setEditing] = useState<string | null>(null)
  const [form, setForm] = useState<CameraBody | null>(null)
  const [detect, setDetect] = useState<CameraDetect | null>(null)

  const load = useCallback(() => {
    fetchCameras()
      .then((d) => {
        setData(d)
        setFolder(d.folder)
        setMaxMinutes(d.max_minutes)
      })
      .catch((e) => setError(errorMessage(e)))
    fetchCameraRecordings().then(setRecordings).catch(() => {})
  }, [])

  useEffect(() => { load() }, [load])
  useEvent('cameras.recording', () => load(), () => load())

  const anyRecording = data?.cameras.some((c) => c.recording) ?? false
  useEffect(() => {
    if (!anyRecording) return
    const i = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(i)
  }, [anyRecording])

  async function run(key: string, fn: () => Promise<void>) {
    setBusy(key)
    setError(null)
    try { await fn(); load() } catch (e) { setError(errorMessage(e)) } finally { setBusy(null) }
  }

  function record(c: CameraView) {
    const ok = confirm(
      `Grabar video${c.audio ? ' y audio' : ''} con "${c.name}".\n\n` +
      'Las personas presentes deben saber que se esta grabando. La grabacion queda registrada a tu nombre.',
    )
    if (ok) run(c.id, () => startCameraRecording(c.id))
  }

  function openForm(c?: CameraView) {
    setEditing(c?.id ?? null)
    setForm(c
      ? { name: c.name, kind: c.kind === 'ip' ? 'ip' : 'usb', device: c.device, url: '', resolution: c.resolution, audio: c.audio }
      : { ...emptyForm })
    if (!detect) detectCameraDevices().then(setDetect).catch(() => {})
  }

  async function submitForm() {
    if (!form) return
    await run('form', async () => {
      if (editing) await updateCamera(editing, form)
      else await createCamera(form)
      setForm(null)
      setEditing(null)
    })
  }

  if (!data) {
    return (
      <div className="flex justify-center py-16">
        {error ? <p style={{ color: 'var(--danger)' }}>{error}</p> : <Loader2 size={28} className="animate-spin" style={{ color: 'var(--accent)' }} />}
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-lg font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
            <Video size={20} style={{ color: 'var(--accent)' }} /> Camaras
          </h1>
          <p className="text-xs mt-1 flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}>
            <ShieldCheck size={13} /> Cada vista, foto y grabacion queda registrada. Mientras se graba, todos los usuarios ven el aviso.
          </p>
        </div>
        {isAdmin && (
          <div className="flex gap-2">
            <button onClick={() => setShowSettings(!showSettings)} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm" style={btn}>
              <Settings2 size={15} /> Ajustes
            </button>
            <button onClick={() => openForm()} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm" style={btn}>
              <Plus size={15} /> Agregar camara
            </button>
          </div>
        )}
      </div>

      {error && (
        <div className="flex items-start justify-between gap-2 text-sm rounded-lg p-3" style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>
          <span>{error}</span>
          <button onClick={() => setError(null)}><X size={14} /></button>
        </div>
      )}
      {!data.ffmpeg && (
        <p className="text-sm flex items-center gap-2" style={{ color: 'var(--warning)' }}>
          <AlertTriangle size={15} /> ffmpeg no esta instalado en el servidor: no hay vista ni grabacion (instalar con install.sh --deps).
        </p>
      )}
      {!data.folder && (
        <p className="text-sm flex items-center gap-2" style={{ color: 'var(--warning)' }}>
          <AlertTriangle size={15} /> Falta elegir la carpeta donde se guardan fotos y grabaciones{isAdmin ? ' (Ajustes).' : ': pidelo al administrador.'}
        </p>
      )}

      {isAdmin && showSettings && (
        <div className="rounded-xl p-4 grid grid-cols-1 md:grid-cols-3 gap-3 items-end" style={card}>
          <label className="text-xs space-y-1 md:col-span-2" style={{ color: 'var(--text-secondary)' }}>
            <span>Carpeta del NAS para fotos y grabaciones (una subcarpeta por camara)</span>
            <input value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="/home/lab/Camaras" className="w-full rounded-lg px-2 py-1.5 text-sm font-mono" style={input} />
          </label>
          <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
            <span>Duracion maxima por grabacion (minutos)</span>
            <input type="number" min={1} max={600} value={maxMinutes} onChange={(e) => setMaxMinutes(Number(e.target.value))} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
          </label>
          <div className="md:col-span-3 flex justify-end">
            <button
              disabled={busy === 'settings'}
              onClick={() => run('settings', async () => { await saveCameraSettings(folder.trim(), maxMinutes); addToast('Ajustes guardados', 'success') })}
              className="px-3 py-1.5 rounded-lg text-sm" style={btn}
            >
              Guardar
            </button>
          </div>
        </div>
      )}

      {isAdmin && form && (
        <div className="rounded-xl p-4 space-y-3" style={card}>
          <div className="flex items-center justify-between">
            <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{editing ? 'Editar camara' : 'Nueva camara'}</span>
            <button onClick={() => setForm(null)} style={{ color: 'var(--text-secondary)' }}><X size={16} /></button>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Nombre</span>
              <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Tipo</span>
              <select
                value={form.kind}
                disabled={!!editing}
                onChange={(e) => setForm({ ...form, kind: e.target.value as 'usb' | 'ip', audio: '' })}
                className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}
              >
                <option value="usb">USB conectada al servidor</option>
                <option value="ip">Camara IP de la red</option>
              </select>
            </label>
            {form.kind === 'usb' ? (
              <>
                <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                  <span className="flex items-center justify-between">
                    Dispositivo de video
                    <button type="button" onClick={() => detectCameraDevices().then(setDetect).catch(() => {})} className="flex items-center gap-1" style={{ color: 'var(--accent)' }}>
                      <Search size={12} /> detectar
                    </button>
                  </span>
                  <select value={form.device} onChange={(e) => setForm({ ...form, device: e.target.value })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                    <option value="">Elegir...</option>
                    {form.device && !detect?.video.some((v) => v.id === form.device) && <option value={form.device}>{form.device}</option>}
                    {detect?.video.map((v) => <option key={v.id} value={v.id}>{v.name}</option>)}
                  </select>
                </label>
                <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                  <span>Resolucion (vacio = la de la camara)</span>
                  <input value={form.resolution} onChange={(e) => setForm({ ...form, resolution: e.target.value })} placeholder="1280x720" className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
                </label>
                <label className="text-xs space-y-1 md:col-span-2" style={{ color: 'var(--text-secondary)' }}>
                  <span>Microfono para las grabaciones</span>
                  <select value={form.audio} onChange={(e) => setForm({ ...form, audio: e.target.value })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                    <option value="">Sin audio</option>
                    {form.audio && !detect?.audio.some((a) => a.id === form.audio) && <option value={form.audio}>{form.audio}</option>}
                    {detect?.audio.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
                  </select>
                </label>
              </>
            ) : (
              <>
                <label className="text-xs space-y-1 md:col-span-2" style={{ color: 'var(--text-secondary)' }}>
                  <span>URL del video (RTSP o MJPEG){editing ? ' — vacio para conservar la actual' : ''}</span>
                  <input value={form.url} onChange={(e) => setForm({ ...form, url: e.target.value })} placeholder="rtsp://usuario:clave@192.168.1.50:554/stream1" className="w-full rounded-lg px-2 py-1.5 text-sm font-mono" style={input} />
                </label>
                <label className="text-xs flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
                  <input type="checkbox" checked={form.audio === 'stream'} onChange={(e) => setForm({ ...form, audio: e.target.checked ? 'stream' : '' })} />
                  Grabar el audio de la camara (si tiene microfono)
                </label>
              </>
            )}
          </div>
          <div className="flex justify-end">
            <button disabled={busy === 'form'} onClick={submitForm} className="px-4 py-1.5 rounded-lg text-sm" style={btn}>
              {busy === 'form' ? <Loader2 size={15} className="animate-spin" /> : editing ? 'Guardar' : 'Agregar'}
            </button>
          </div>
        </div>
      )}

      {data.cameras.length === 0 ? (
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
          No hay camaras configuradas.{isAdmin ? ' Usa "Agregar camara".' : ''}
        </p>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {data.cameras.map((c) => (
            <div key={c.id} className="rounded-xl overflow-hidden" style={{ ...card, borderColor: c.recording ? 'var(--danger)' : undefined }}>
              <div className="relative">
                <Thumb id={c.id} />
                {c.recording && (
                  <span className="absolute top-2 left-2 flex items-center gap-1 px-2 py-0.5 rounded text-xs font-semibold" style={{ backgroundColor: 'var(--danger)', color: '#fff' }}>
                    <Circle size={9} fill="#fff" /> GRABANDO {elapsed(c.recording.since, now)} · {c.recording.by}
                  </span>
                )}
                <button onClick={() => setLive(c)} title="Ver en vivo" className="absolute top-2 right-2 p-1.5 rounded" style={{ backgroundColor: 'rgba(0,0,0,0.55)', color: '#fff' }}>
                  <Maximize2 size={15} />
                </button>
              </div>
              <div className="p-3 space-y-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{c.name}</p>
                    <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>
                      {c.kind === 'usb' ? 'USB' : c.kind === 'ip' ? 'IP' : 'Impresora 3D'}
                      {c.audio ? ' · con audio' : ''}
                      {c.viewers > 0 ? ` · ${c.viewers} mirando` : ''}
                    </p>
                  </div>
                  {isAdmin && c.editable && (
                    <div className="flex gap-1 shrink-0">
                      <button onClick={() => openForm(c)} title="Editar" className="p-1" style={{ color: 'var(--text-secondary)' }}><Pencil size={14} /></button>
                      <button
                        onClick={() => { if (confirm(`¿Eliminar la camara ${c.name}? Las grabaciones guardadas se conservan.`)) run(c.id, () => deleteCamera(c.id)) }}
                        title="Eliminar" className="p-1" style={{ color: 'var(--danger)' }}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  )}
                </div>
                {c.error && <p className="text-xs" style={{ color: 'var(--danger)' }}>{c.error}</p>}
                <div className="flex gap-2">
                  <button
                    disabled={busy === c.id || !data.folder}
                    onClick={() => run(c.id, async () => { const p = await takeCameraPhoto(c.id); addToast(`Foto guardada: ${p.split('/').pop()}`, 'success') })}
                    className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs disabled:opacity-50" style={btn}
                  >
                    <Camera size={14} /> Foto
                  </button>
                  {c.recording ? (
                    <button
                      disabled={busy === c.id}
                      onClick={() => run(c.id, () => stopCameraRecording(c.id))}
                      className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-semibold"
                      style={{ backgroundColor: 'var(--danger)', color: '#fff' }}
                    >
                      <Square size={12} fill="#fff" /> Detener
                    </button>
                  ) : (
                    <button
                      disabled={busy === c.id || !data.folder || !data.ffmpeg}
                      onClick={() => record(c)}
                      className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs disabled:opacity-50"
                      style={{ color: 'var(--danger)', border: '1px solid var(--border)' }}
                    >
                      {busy === c.id ? <Loader2 size={13} className="animate-spin" /> : <Circle size={12} />} Grabar
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Fotos y grabaciones */}
      <div>
        <h2 className="text-base font-semibold mb-3" style={{ color: 'var(--text-primary)' }}>Registro de fotos y grabaciones</h2>
        <div className="rounded-xl overflow-x-auto" style={card}>
          {recordings.length === 0 ? (
            <p className="text-sm p-4" style={{ color: 'var(--text-secondary)' }}>Todavia no hay fotos ni grabaciones.</p>
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr style={{ color: 'var(--text-secondary)' }}>
                  <th className="text-left p-2">Fecha</th>
                  <th className="text-left p-2">Camara</th>
                  <th className="text-left p-2">Tipo</th>
                  <th className="text-left p-2">Por</th>
                  <th className="text-left p-2">Duracion</th>
                  <th className="text-left p-2">Tamano</th>
                  <th className="text-left p-2">Estado</th>
                  <th className="p-2"></th>
                </tr>
              </thead>
              <tbody>
                {recordings.map((r) => (
                  <tr key={r.id} style={{ borderTop: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                    <td className="p-2 whitespace-nowrap">{new Date(r.started_at).toLocaleString()}</td>
                    <td className="p-2">{r.camera_name}</td>
                    <td className="p-2">{r.kind === 'foto' ? 'Foto' : 'Video'}</td>
                    <td className="p-2">{r.username}</td>
                    <td className="p-2">{r.kind === 'video' && r.ended_at ? elapsed(r.started_at, new Date(r.ended_at).getTime()) : '—'}</td>
                    <td className="p-2">{fmtSize(r.size)}</td>
                    <td className="p-2">
                      {r.status === 'borrado' ? `borrado por ${r.deleted_by}` : r.status}
                    </td>
                    <td className="p-2 whitespace-nowrap text-right">
                      {(r.status === 'ok' || r.status === 'interrumpida') && (
                        <a href={cameraRecordingUrl(r.id)} title="Descargar" className="inline-block p-1" style={{ color: 'var(--accent)' }}><Download size={14} /></a>
                      )}
                      {isAdmin && r.status !== 'borrado' && r.status !== 'grabando' && (
                        <button
                          onClick={() => { if (confirm('¿Borrar el archivo? El registro de que existio se conserva.')) run(r.id, () => deleteCameraRecording(r.id)) }}
                          title="Borrar archivo" className="p-1" style={{ color: 'var(--danger)' }}
                        >
                          <Trash2 size={14} />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>

      {/* Vista en vivo */}
      {live && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.85)' }} onClick={() => setLive(null)}>
          <div className="relative max-w-6xl w-full" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium flex items-center gap-2" style={{ color: '#fff' }}>
                {live.name}
                {data.cameras.find((c) => c.id === live.id)?.recording && (
                  <span className="flex items-center gap-1 px-2 py-0.5 rounded text-xs" style={{ backgroundColor: 'var(--danger)' }}>
                    <Circle size={9} fill="#fff" /> GRABANDO
                  </span>
                )}
              </span>
              <button onClick={() => setLive(null)} style={{ color: '#fff' }}><X size={20} /></button>
            </div>
            <img src={cameraStreamUrl(live.id)} alt={live.name} className="w-full max-h-[80vh] object-contain bg-black rounded-lg" />
          </div>
        </div>
      )}
    </div>
  )
}
