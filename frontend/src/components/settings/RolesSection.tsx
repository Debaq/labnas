import { useEffect, useState } from 'react'
import { Shield, ChevronDown, ChevronRight, Plus, RotateCcw, Trash2, Save } from 'lucide-react'
import { fetchRoles, createRole, updateRole, resetRole, deleteRole } from '../../api'
import type { ModuleLevel, RoleInfo } from '../../types'
import { errorMessage } from '../../lib/errors'
import { LEVELS, MODULE_LABELS } from '../../lib/roles'

const LEVEL_COLOR: Record<ModuleLevel, string> = {
  ninguno: 'var(--text-primary)',
  ver: 'var(--accent)',
  usar: 'var(--success)',
  gestionar: 'var(--warning)',
}

/** Selector de nivel en botones (Sin acceso / Ver / Usar / Gestionar) */
export function LevelPicker({ value, onChange, disabled }: { value: ModuleLevel; onChange: (l: ModuleLevel) => void; disabled?: boolean }) {
  return (
    <div className="inline-flex rounded-lg overflow-hidden" style={{ border: '1px solid var(--input-border)' }} role="radiogroup">
      {LEVELS.map(l => {
        const active = l.id === value
        return (
          <button
            key={l.id}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={disabled}
            title={l.hint}
            onClick={() => onChange(l.id)}
            className="px-2 py-1 text-[11px] font-medium disabled:cursor-default"
            style={{
              backgroundColor: active ? 'var(--bg-tertiary)' : 'transparent',
              color: active ? LEVEL_COLOR[l.id] : 'var(--text-secondary)',
              fontWeight: active ? 700 : 500,
              opacity: active ? 1 : 0.7,
            }}
          >
            {l.label}
          </button>
        )
      })}
    </div>
  )
}

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function RoleCard({ role, modules, onChanged }: { role: RoleInfo; modules: string[]; onChanged: () => void }) {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState(role.name)
  const [description, setDescription] = useState(role.description)
  const [expiry, setExpiry] = useState(role.expiry_days ? String(role.expiry_days) : '')
  const [levels, setLevels] = useState(role.levels)
  const [busy, setBusy] = useState(false)

  const dirty =
    name !== role.name ||
    description !== role.description ||
    expiry !== (role.expiry_days ? String(role.expiry_days) : '') ||
    modules.some(m => levels[m] !== role.levels[m])

  function reload(r: RoleInfo) {
    setName(r.name)
    setDescription(r.description)
    setExpiry(r.expiry_days ? String(r.expiry_days) : '')
    setLevels(r.levels)
    onChanged()
  }

  async function run(fn: () => Promise<void>) {
    setBusy(true)
    try { await fn() } catch (e) { alert(errorMessage(e)) } finally { setBusy(false) }
  }

  const summary = modules.filter(m => role.levels[m] && role.levels[m] !== 'ninguno').length

  return (
    <div className="rounded-xl" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <button type="button" onClick={() => setOpen(!open)} className="w-full flex items-center gap-3 p-3 text-left">
        {open ? <ChevronDown size={14} style={{ color: 'var(--text-secondary)' }} /> : <ChevronRight size={14} style={{ color: 'var(--text-secondary)' }} />}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{role.name}</span>
            <span className="text-[10px] font-mono" style={{ color: 'var(--text-secondary)' }}>{role.id}</span>
            {role.expiry_days && (
              <span className="text-[10px] px-1.5 py-0.5 rounded-full" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>
                vence a los {role.expiry_days} dias
              </span>
            )}
          </div>
          {role.description && <div className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{role.description}</div>}
        </div>
        <span className="text-[11px] whitespace-nowrap" style={{ color: 'var(--text-secondary)' }}>
          {role.locked ? 'Todo' : `${summary}/${modules.length} modulos`} · {role.users} {role.users === 1 ? 'cuenta' : 'cuentas'}
        </span>
      </button>

      {open && (
        <div className="px-4 pb-4 space-y-3">
          {role.locked ? (
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              El administrador lo puede todo, incluida la configuracion del sistema. Este rol no se edita.
            </p>
          ) : (
            <>
              <div className="grid gap-2 sm:grid-cols-[1fr_2fr_auto]">
                <input value={name} onChange={e => setName(e.target.value)} maxLength={40} placeholder="Nombre"
                  className="px-2 py-1 rounded-lg text-xs outline-none" style={inputStyle} />
                <input value={description} onChange={e => setDescription(e.target.value)} maxLength={200} placeholder="Para que sirve este rol"
                  className="px-2 py-1 rounded-lg text-xs outline-none" style={inputStyle} />
                <label className="flex items-center gap-1 text-xs" style={{ color: 'var(--text-secondary)' }} title="Al asignar este rol, la cuenta vence en estos dias (se puede cambiar por persona)">
                  Vence en
                  <input value={expiry} onChange={e => setExpiry(e.target.value.replace(/\D/g, ''))} inputMode="numeric" placeholder="—"
                    className="w-14 px-2 py-1 rounded-lg text-xs outline-none" style={inputStyle} />
                  dias
                </label>
              </div>

              <div className="grid gap-x-6 gap-y-1.5 md:grid-cols-2">
                {modules.map(m => (
                  <div key={m} className="flex items-center justify-between gap-2">
                    <span className="text-xs" style={{ color: 'var(--text-primary)' }}>{MODULE_LABELS[m] ?? m}</span>
                    <LevelPicker value={levels[m] ?? 'ninguno'} onChange={l => setLevels({ ...levels, [m]: l })} disabled={busy} />
                  </div>
                ))}
              </div>

              <div className="flex items-center gap-2 flex-wrap pt-1">
                <button type="button" disabled={!dirty || busy || !name.trim()}
                  onClick={() => run(async () => reload(await updateRole(role.id, { name, description, expiry_days: expiry ? parseInt(expiry) : null, levels })))}
                  className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium disabled:opacity-40"
                  style={{ backgroundColor: 'var(--accent)', color: 'white' }}>
                  <Save size={12} /> Guardar
                </button>
                {dirty && (
                  <button type="button" onClick={() => reload(role)} className="px-3 py-1.5 rounded-lg text-xs" style={{ color: 'var(--text-secondary)' }}>
                    Descartar cambios
                  </button>
                )}
                <div className="flex-1" />
                {role.template && (
                  <button type="button" disabled={busy}
                    onClick={() => { if (confirm(`¿Volver ${role.name} a los niveles de su plantilla?`)) run(async () => reload(await resetRole(role.id))) }}
                    className="flex items-center gap-1 px-2 py-1.5 rounded-lg text-xs" style={{ color: 'var(--text-secondary)' }}>
                    <RotateCcw size={12} /> Restaurar plantilla
                  </button>
                )}
                <button type="button" disabled={busy || role.users > 0}
                  title={role.users > 0 ? 'Cambia de rol a sus cuentas antes de borrarlo' : 'Borrar rol'}
                  onClick={() => { if (confirm(`¿Borrar el rol ${role.name}?`)) run(async () => { await deleteRole(role.id); onChanged() }) }}
                  className="flex items-center gap-1 px-2 py-1.5 rounded-lg text-xs disabled:opacity-40" style={{ color: 'var(--danger)' }}>
                  <Trash2 size={12} /> Borrar
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  )
}

/** Roles editables: nivel de cada rol en cada modulo */
export default function RolesSection({ onChanged }: { onChanged?: () => void }) {
  const [data, setData] = useState<{ modules: string[]; roles: RoleInfo[] } | null>(null)
  const [creating, setCreating] = useState(false)
  const [newName, setNewName] = useState('')
  const [copyFrom, setCopyFrom] = useState('estudiante')

  async function load() {
    try {
      setData(await fetchRoles())
      onChanged?.()
    } catch (e) {
      alert(errorMessage(e))
    }
  }

  useEffect(() => {
    fetchRoles().then(setData).catch(() => {})
  }, [])

  return (
    <section>
      <div className="flex items-center gap-3 mb-2">
        <Shield size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Roles</h2>
        <div className="flex-1" />
        <button type="button" onClick={() => setCreating(!creating)}
          className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium"
          style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}>
          <Plus size={12} /> Nuevo rol
        </button>
      </div>
      <p className="text-xs mb-3" style={{ color: 'var(--text-secondary)' }}>
        Cada rol tiene un nivel en cada modulo: <b>Ver</b> consulta, <b>Usar</b> imprime, reserva o crea lo propio,
        y <b>Gestionar</b> edita lo de otros, aprueba y controla equipos. A una persona se le puede subir o bajar un
        modulo sin cambiarle el rol (en su ficha, mas abajo).
      </p>

      {creating && data && (
        <form className="rounded-xl p-3 mb-3 flex items-center gap-2 flex-wrap" style={{ backgroundColor: 'var(--card-bg)', border: '1px dashed var(--border)' }}
          onSubmit={async e => {
            e.preventDefault()
            if (!newName.trim()) return
            try {
              await createRole(newName.trim(), copyFrom || undefined)
              setNewName('')
              setCreating(false)
              await load()
            } catch (err) {
              alert(errorMessage(err))
            }
          }}>
          <input autoFocus value={newName} onChange={e => setNewName(e.target.value)} maxLength={40} placeholder="Nombre del rol (p. ej. Jefe de taller)"
            className="flex-1 min-w-48 px-2 py-1 rounded-lg text-xs outline-none" style={inputStyle} />
          <label className="flex items-center gap-1 text-xs" style={{ color: 'var(--text-secondary)' }}>
            Partir de
            <select value={copyFrom} onChange={e => setCopyFrom(e.target.value)} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer" style={inputStyle}>
              <option value="">Sin acceso a nada</option>
              {data.roles.filter(r => !r.locked).map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
            </select>
          </label>
          <button type="submit" disabled={!newName.trim()} className="px-3 py-1 rounded-lg text-xs font-medium disabled:opacity-40" style={{ backgroundColor: 'var(--accent)', color: 'white' }}>
            Crear
          </button>
        </form>
      )}

      <div className="space-y-2">
        {data?.roles.map(r => <RoleCard key={r.id} role={r} modules={data.modules} onChanged={load} />)}
      </div>
    </section>
  )
}
