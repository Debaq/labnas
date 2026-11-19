import { useCallback, useEffect, useMemo, useState } from 'react'
import { ScanEye, Plus, Pencil, Trash2, Play, Pause, Loader2, X, Camera, Send, Video, AlertTriangle, Download } from 'lucide-react'
import {
  fetchDetectors, saveDetector, deleteDetector, testDetector, prepareDetectionModel, fetchDetections, cameraRecordingUrl,
  type DetectorsResponse, type DetectorInput, type DetectorRule, type DetectionRow,
} from '../api'
import type { CameraView } from '../types'
import { useEvent } from '../events/useEvents'
import { useToast } from './ToastContext'
import { errorMessage } from '../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
const btn = { color: 'var(--accent)', border: '1px solid var(--border)' }

const KIND_HELP: Record<string, string> = {
  personas: 'Avisa cuando aparece una persona.',
  objetos: 'Avisa cuando aparece alguno de los objetos elegidos.',
  movimiento: 'Avisa cuando cambia una parte de la imagen. No usa IA: es muy liviano, pero también reacciona a cambios de luz.',
}

function newRule(cameraId: string): DetectorInput {
  return {
    camera_id: cameraId, kind: 'personas', classes: [], min_score: 0.5, interval_s: 2,
    schedule: { days: [0, 1, 2, 3, 4], from: '20:00', to: '07:00' },
    actions: { photo: true, telegram: true, clip_s: 10, telegram_operators: false }, cooldown_s: 120, enabled: true,
  }
}

function scheduleText(r: DetectorInput, days: string[]): string {
  if (!r.schedule) return 'Siempre'
  const d = r.schedule.days.length === 7 ? 'todos los días' : r.schedule.days.map((i) => days[i]).join(' ')
  return `${d}, ${r.schedule.from}–${r.schedule.to}`
}

/** Deteccion con IA por camara: reglas (admin), estado, prueba e historial */
export default function DetectionSection({ cameras, isAdmin }: { cameras: CameraView[]; isAdmin: boolean }) {
  const { addToast } = useToast()
  const [data, setData] = useState<DetectorsResponse | null>(null)
  const [history, setHistory] = useState<DetectionRow[]>([])
  const [form, setForm] = useState<{ id?: string; rule: DetectorInput } | null>(null)
  const [classFilter, setClassFilter] = useState('')
  const [busy, setBusy] = useState(false)
  const [test, setTest] = useState<{ image: string; detections: { label: string; score: number }[] } | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    fetchDetectors().then(setData).catch((e) => setError(errorMessage(e)))
    fetchDetections().then(setHistory).catch(() => {})
  }, [])
  useEffect(() => { load() }, [load])
  useEvent('cameras.detection', load, load)
  useEvent('cameras.detecting', load)

  const classNames = useMemo(() => new Map((data?.classes ?? []).map(([en, es]) => [en, es])), [data])

  const run = async (f: () => Promise<unknown>, ok?: string) => {
    setBusy(true)
    try {
      await f()
      if (ok) addToast(ok, 'success')
      load()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  const doTest = async (rule: DetectorInput) => {
    setBusy(true)
    try {
      setTest(await testDetector(rule))
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  const set = (patch: Partial<DetectorInput>) => form && setForm({ ...form, rule: { ...form.rule, ...patch } })
  const ownCameras = cameras

  if (!data) return error ? <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p> : null
  const r = form?.rule
  const isMotion = r?.kind === 'movimiento'

  return (
    <div>
      <div className="flex items-center gap-2 mb-3 flex-wrap">
        <ScanEye size={18} style={{ color: 'var(--accent)' }} />
        <h2 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Detección con IA</h2>
        <span className="text-xs px-2 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: data.model.error ? 'var(--danger)' : 'var(--text-secondary)' }}>
          {data.model.downloading ? 'Descargando modelo…' : data.model.ready ? 'Modelo listo (YOLOX-nano, en el NAS)' : data.model.error ? `Modelo: ${data.model.error}` : 'Modelo sin descargar'}
        </span>
        {isAdmin && !data.model.ready && !data.model.downloading && (
          <button onClick={() => run(prepareDetectionModel, 'Modelo listo')} disabled={busy} className="text-xs px-2 py-1 rounded-lg disabled:opacity-50" style={btn}>
            Descargar modelo (~4 MB)
          </button>
        )}
        {isAdmin && ownCameras.length > 0 && !form && (
          <button onClick={() => setForm({ rule: newRule(ownCameras[0].id) })} className="ml-auto flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg" style={btn}>
            <Plus size={14} /> Nueva regla
          </button>
        )}
      </div>
      <p className="text-xs mb-3" style={{ color: 'var(--text-secondary)' }}>
        El análisis corre en el NAS: las imágenes no salen del laboratorio. Mientras una regla está activa, todos los usuarios ven el aviso «Detección activa».
      </p>

      {/* Formulario */}
      {form && r && (
        <div className="rounded-xl p-4 mb-4 space-y-3" style={card}>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Cámara</span>
              <select value={r.camera_id} onChange={(e) => set({ camera_id: e.target.value })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                {ownCameras.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </select>
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Detectar</span>
              <select
                value={r.kind}
                onChange={(e) => {
                  const kind = e.target.value as DetectorInput['kind']
                  set({ kind, min_score: kind === 'movimiento' ? 0.03 : 0.5 })
                }}
                className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}
              >
                {data.kinds.map(([k, l]) => <option key={k} value={k}>{l}</option>)}
              </select>
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>{isMotion ? `Sensibilidad: cambio ≥ ${(r.min_score * 100).toFixed(1)} % de la imagen` : `Confianza mínima: ${Math.round(r.min_score * 100)} %`}</span>
              <input
                type="range"
                min={isMotion ? 0.005 : 0.1} max={isMotion ? 0.3 : 0.95} step={isMotion ? 0.005 : 0.05}
                value={r.min_score}
                onChange={(e) => set({ min_score: Number(e.target.value) })}
                className="w-full"
              />
            </label>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>{KIND_HELP[r.kind]}</p>

          {r.kind === 'objetos' && (
            <div className="space-y-1">
              <input value={classFilter} onChange={(e) => setClassFilter(e.target.value)} placeholder="Buscar objeto (notebook, mochila, celular…)" className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
              <div className="max-h-40 overflow-auto rounded-lg p-2 grid grid-cols-2 sm:grid-cols-4 gap-1" style={input}>
                {data.classes
                  .filter(([en, es]) => en !== 'person' && (!classFilter || es.includes(classFilter.toLowerCase()) || en.includes(classFilter.toLowerCase())))
                  .map(([en, es]) => (
                    <label key={en} className="text-xs flex items-center gap-1 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={r.classes.includes(en)}
                        onChange={(e) => set({ classes: e.target.checked ? [...r.classes, en] : r.classes.filter((c) => c !== en) })}
                      />
                      {es}
                    </label>
                  ))}
              </div>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                {r.classes.length ? `Elegidos: ${r.classes.map((c) => classNames.get(c) ?? c).join(', ')}` : 'Elige al menos uno.'}
              </p>
            </div>
          )}

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Analizar cada (s)</span>
              <input type="number" min={1} max={60} value={r.interval_s} onChange={(e) => set({ interval_s: Number(e.target.value) })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Pausa entre avisos (s)</span>
              <input type="number" min={10} max={3600} value={r.cooldown_s} onChange={(e) => set({ cooldown_s: Number(e.target.value) })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Clip de video (s, 0 = no)</span>
              <input type="number" min={0} max={120} value={r.actions.clip_s} onChange={(e) => set({ actions: { ...r.actions, clip_s: Number(e.target.value) } })} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
            </label>
            <div className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Al detectar</span>
              <label className="flex items-center gap-1"><input type="checkbox" checked={r.actions.photo} onChange={(e) => set({ actions: { ...r.actions, photo: e.target.checked } })} /> Guardar foto</label>
              <label className="flex items-center gap-1"><input type="checkbox" checked={r.actions.telegram} onChange={(e) => set({ actions: { ...r.actions, telegram: e.target.checked } })} /> Telegram con foto</label>
              {r.actions.telegram && (
                <label className="flex items-center gap-1 pl-4" title="Si no, solo a los administradores">
                  <input type="checkbox" checked={r.actions.telegram_operators ?? false} onChange={(e) => set({ actions: { ...r.actions, telegram_operators: e.target.checked } })} /> también a operadores
                </label>
              )}
            </div>
          </div>

          <div className="space-y-2">
            <label className="text-xs flex items-center gap-2" style={{ color: 'var(--text-secondary)' }}>
              <input type="checkbox" checked={!!r.schedule} onChange={(e) => set({ schedule: e.target.checked ? { days: [0, 1, 2, 3, 4], from: '20:00', to: '07:00' } : null })} />
              Solo en un horario (si no, siempre)
            </label>
            {r.schedule && (
              <div className="flex flex-wrap items-center gap-2 text-xs" style={{ color: 'var(--text-secondary)' }}>
                {data.days.map((d, i) => (
                  <label key={d} className="flex items-center gap-1">
                    <input
                      type="checkbox"
                      checked={r.schedule!.days.includes(i)}
                      onChange={(e) => set({ schedule: { ...r.schedule!, days: e.target.checked ? [...r.schedule!.days, i].sort() : r.schedule!.days.filter((x) => x !== i) } })}
                    />
                    {d}
                  </label>
                ))}
                <span>de</span>
                <input type="time" value={r.schedule.from} onChange={(e) => set({ schedule: { ...r.schedule!, from: e.target.value } })} className="rounded px-1 py-0.5" style={input} />
                <span>a</span>
                <input type="time" value={r.schedule.to} onChange={(e) => set({ schedule: { ...r.schedule!, to: e.target.value } })} className="rounded px-1 py-0.5" style={input} />
                {r.schedule.to <= r.schedule.from && <span>(termina al día siguiente)</span>}
              </div>
            )}
          </div>

          <div className="flex gap-2 justify-end">
            <button onClick={() => setForm(null)} className="text-xs px-3 py-1.5 rounded-lg" style={btn}>Cancelar</button>
            <button onClick={() => doTest(r)} disabled={busy} className="flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg disabled:opacity-50" style={btn}>
              {busy ? <Loader2 size={13} className="animate-spin" /> : <ScanEye size={13} />} Probar ahora
            </button>
            <button
              onClick={() => run(() => saveDetector(r, form.id), 'Regla guardada').then(() => setForm(null))}
              disabled={busy}
              className="text-xs px-3 py-1.5 rounded-lg disabled:opacity-50"
              style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}
            >
              Guardar
            </button>
          </div>
        </div>
      )}

      {/* Reglas */}
      {data.rules.length > 0 && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-3 mb-4">
          {data.rules.map((rule: DetectorRule) => (
            <div key={rule.id} className="rounded-xl p-3 text-sm space-y-1" style={card}>
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{rule.camera}</span>
                <span className="text-xs px-2 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                  {rule.kind === 'objetos' ? rule.classes.map((c) => classNames.get(c) ?? c).join(', ') : rule.kind}
                </span>
                <span className="text-xs" style={{ color: !rule.enabled ? 'var(--text-secondary)' : rule.active_now ? 'var(--success)' : 'var(--text-secondary)' }}>
                  {!rule.enabled ? 'Pausada' : rule.active_now ? '● Vigilando' : 'Fuera de horario'}
                </span>
                {isAdmin && (
                  <span className="ml-auto flex gap-1">
                    <button onClick={() => doTest(rule)} disabled={busy} title="Probar ahora" className="p-1" style={{ color: 'var(--accent)' }}><ScanEye size={14} /></button>
                    <button onClick={() => run(() => saveDetector({ ...rule, enabled: !rule.enabled }, rule.id))} disabled={busy} title={rule.enabled ? 'Pausar' : 'Activar'} className="p-1" style={{ color: 'var(--accent)' }}>
                      {rule.enabled ? <Pause size={14} /> : <Play size={14} />}
                    </button>
                    <button onClick={() => setForm({ id: rule.id, rule: { ...rule } })} title="Editar" className="p-1" style={{ color: 'var(--accent)' }}><Pencil size={14} /></button>
                    <button onClick={() => { if (confirm('¿Borrar la regla? El historial de detecciones se conserva.')) run(() => deleteDetector(rule.id)) }} title="Borrar" className="p-1" style={{ color: 'var(--danger)' }}>
                      <Trash2 size={14} />
                    </button>
                  </span>
                )}
              </div>
              <div className="text-xs flex flex-wrap gap-x-3 gap-y-1" style={{ color: 'var(--text-secondary)' }}>
                <span>{scheduleText(rule, data.days)}</span>
                <span>cada {rule.interval_s} s · pausa {rule.cooldown_s} s</span>
                <span className="flex items-center gap-2">
                  {rule.actions.photo && <Camera size={12} />}
                  {rule.actions.telegram && <Send size={12} />}
                  {rule.actions.clip_s > 0 && <span className="flex items-center gap-0.5"><Video size={12} />{rule.actions.clip_s}s</span>}
                </span>
              </div>
              {rule.last_fire_at && (
                <p className="text-xs" style={{ color: 'var(--text-primary)' }}>Último: {rule.last_labels} — {new Date(rule.last_fire_at).toLocaleString()}</p>
              )}
              {rule.last_error && (
                <p className="text-xs flex items-center gap-1" style={{ color: 'var(--warning)' }}><AlertTriangle size={12} /> {rule.last_error}</p>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Historial */}
      <div className="rounded-xl overflow-x-auto" style={card}>
        {history.length === 0 ? (
          <p className="text-sm p-4" style={{ color: 'var(--text-secondary)' }}>Todavía no hay detecciones.</p>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr style={{ color: 'var(--text-secondary)' }}>
                <th className="text-left p-2">Fecha</th>
                <th className="text-left p-2">Cámara</th>
                <th className="text-left p-2">Tipo</th>
                <th className="text-left p-2">Detectado</th>
                <th className="p-2"></th>
              </tr>
            </thead>
            <tbody>
              {history.map((d) => (
                <tr key={d.id} style={{ borderTop: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                  <td className="p-2 whitespace-nowrap">{new Date(d.at).toLocaleString()}</td>
                  <td className="p-2">{d.camera_name}</td>
                  <td className="p-2">{d.kind}</td>
                  <td className="p-2">{d.labels}</td>
                  <td className="p-2 whitespace-nowrap text-right">
                    {d.photo_id && <a href={cameraRecordingUrl(d.photo_id)} title="Foto" className="inline-block p-1" style={{ color: 'var(--accent)' }}><Camera size={14} /></a>}
                    {d.clip_id && <a href={cameraRecordingUrl(d.clip_id)} title="Clip" className="inline-block p-1" style={{ color: 'var(--accent)' }}><Download size={14} /></a>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Resultado de la prueba */}
      {test && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.85)' }} onClick={() => setTest(null)}>
          <div className="relative max-w-4xl w-full" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between mb-2 text-sm" style={{ color: '#fff' }}>
              <span>{test.detections.length ? test.detections.map((d) => `${d.label} (${Math.round(d.score * 100)}%)`).join(', ') : 'No detectó nada en este cuadro'}</span>
              <button onClick={() => setTest(null)}><X size={20} /></button>
            </div>
            <img src={`data:image/jpeg;base64,${test.image}`} alt="Prueba de detección" className="w-full max-h-[80vh] object-contain bg-black rounded-lg" />
          </div>
        </div>
      )}
    </div>
  )
}
