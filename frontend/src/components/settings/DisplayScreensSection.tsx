import { useEffect, useState } from 'react'
import { Copy, ExternalLink, Monitor, Pencil, Plus, Trash2 } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import {
  createScreen, deleteScreen, displayUrl, DISPLAY_SECTIONS, fetchScreens, updateScreen,
  type DisplayScreen, type DisplaySection,
} from '../../api'
import { errorMessage } from '../../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const ALL = DISPLAY_SECTIONS.map(s => s.id)

function seen(ts: string | null): string {
  if (!ts) return 'nunca se ha abierto'
  return `vista por ultima vez ${ts.replace('T', ' ')}`
}

/** Pantallas del laboratorio (TV o tablet): enlace secreto de solo lectura por equipo */
export default function DisplayScreensSection() {
  const { isModuleEnabled } = useAuth()
  const [screens, setScreens] = useState<DisplayScreen[]>([])
  const [form, setForm] = useState<{ token: string | null; name: string; sections: DisplaySection[] } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState<string | null>(null)

  const load = () => fetchScreens().then(setScreens).catch(e => setError(errorMessage(e)))
  useEffect(() => { load() }, [])

  async function save() {
    if (!form) return
    try {
      if (form.token) await updateScreen(form.token, form.name, form.sections)
      else await createScreen(form.name, form.sections)
      setForm(null)
      setError(null)
      load()
    } catch (e) {
      setError(errorMessage(e))
    }
  }

  async function remove(s: DisplayScreen) {
    if (!confirm(`Revocar la pantalla «${s.name}»? Su enlace deja de funcionar.`)) return
    try { await deleteScreen(s.token); load() } catch (e) { setError(errorMessage(e)) }
  }

  function copy(s: DisplayScreen) {
    navigator.clipboard?.writeText(displayUrl(s.token))
    setCopied(s.token)
    setTimeout(() => setCopied(null), 1500)
  }

  const toggle = (id: DisplaySection) => form && setForm({ ...form, sections: form.sections.includes(id) ? form.sections.filter(x => x !== id) : [...form.sections, id] })

  return (
    <section>
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <Monitor size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Pantallas del laboratorio</h2>
        </div>
        {!form && (
          <button onClick={() => setForm({ token: null, name: '', sections: ALL })} className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={14} /> Nueva pantalla
          </button>
        )}
      </div>
      <div className="rounded-xl p-5 space-y-3" style={card}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Para una TV o tablet en el laboratorio: muestra las maquinas en uso y cuanto les queda, las impresoras 3D, las reservas del dia, los sensores, los plazos cercanos y los avisos. Se abre con su enlace, sin iniciar sesion, y se actualiza sola. El enlace solo sirve para mirar; si se pierde la tablet, revocalo.
        </p>
        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

        {form && (
          <div className="rounded-lg p-3 space-y-3" style={{ border: '1px solid var(--border)' }}>
            <input value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} placeholder="Nombre: TV de la entrada, tablet de la sala de impresion…" className="w-full rounded-lg px-3 py-2 text-sm" style={input} />
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-1">
              {DISPLAY_SECTIONS.map(s => (
                <label key={s.id} className="flex items-center gap-2 text-sm" style={{ color: isModuleEnabled(s.module) ? 'var(--text-primary)' : 'var(--text-secondary)' }}>
                  <input type="checkbox" checked={form.sections.includes(s.id)} onChange={() => toggle(s.id)} />
                  {s.label}{!isModuleEnabled(s.module) && ' (modulo apagado)'}
                </label>
              ))}
            </div>
            <div className="flex justify-end gap-2">
              <button onClick={() => setForm(null)} className="px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>Cancelar</button>
              <button onClick={save} className="px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
            </div>
          </div>
        )}

        {screens.length === 0 && !form && <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>No hay pantallas.</p>}
        {screens.map(s => (
          <div key={s.token} className="flex items-center justify-between gap-3 p-3 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)' }}>
            <div className="min-w-0">
              <div className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{s.name}</div>
              <div className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>
                {s.sections.map(id => DISPLAY_SECTIONS.find(x => x.id === id)?.label ?? id).join(', ')} · {seen(s.last_seen_at)}
              </div>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              <button onClick={() => copy(s)} className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
                <Copy size={13} /> {copied === s.token ? 'Copiado' : 'Copiar enlace'}
              </button>
              <a href={displayUrl(s.token)} target="_blank" rel="noreferrer" className="p-1.5" title="Abrir"><ExternalLink size={14} style={{ color: 'var(--accent)' }} /></a>
              <button onClick={() => setForm({ token: s.token, name: s.name, sections: s.sections })} className="p-1.5" title="Editar"><Pencil size={14} style={{ color: 'var(--text-secondary)' }} /></button>
              <button onClick={() => remove(s)} className="p-1.5" title="Revocar"><Trash2 size={14} style={{ color: 'var(--danger)' }} /></button>
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}
