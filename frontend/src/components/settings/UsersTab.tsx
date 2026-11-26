import { useEffect, useState } from 'react'
import { Users, MessageCircle, X, Trash2, Link2, UserCheck, SlidersHorizontal, Save } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { fetchWebUsers, fetchNotificationConfig, resetUserTwoFactor, deleteWebUser, setWebUserRole, adminLinkChat, deleteTelegramChat, setChatRole, fetchRoles } from '../../api'
import type { WebUserRow } from '../../api/auth'
import type { ModuleLevel, NotificationConfig, RoleInfo } from '../../types'
import { errorMessage } from '../../lib/errors'
import RolesSection from './RolesSection'
import { LEVELS, MODULE_LABELS } from '../../lib/roles'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function roleColor(role: string): string {
  if (role === 'admin') return 'var(--accent)'
  if (role === 'pendiente') return 'var(--warning)'
  return 'var(--success)'
}

/** unix -> 'aaaa-mm-dd' (input date) y vuelta (fin del dia local) */
function toDateInput(ts: number | null): string {
  if (!ts) return ''
  const d = new Date(ts * 1000)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}
function fromDateInput(v: string): number | null {
  if (!v) return null
  const [y, m, d] = v.split('-').map(Number)
  return Math.floor(new Date(y, m - 1, d, 23, 59, 59).getTime() / 1000)
}

/** Vencimiento y excepciones por modulo de una persona */
function AccessEditor({ u, role, modules, onSaved }: { u: WebUserRow; role?: RoleInfo; modules: string[]; onSaved: () => void }) {
  const [expires, setExpires] = useState(toDateInput(u.expires_at))
  const [overrides, setOverrides] = useState<Record<string, ModuleLevel>>(u.overrides)
  const [busy, setBusy] = useState(false)
  const dirty = expires !== toDateInput(u.expires_at) || JSON.stringify(overrides) !== JSON.stringify(u.overrides)

  return (
    <div className="rounded-lg p-3 space-y-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
      <label className="flex items-center gap-2 text-xs flex-wrap" style={{ color: 'var(--text-secondary)' }}>
        Fecha de termino de la cuenta
        <input type="date" value={expires} onChange={e => setExpires(e.target.value)} className="px-2 py-1 rounded-lg text-xs outline-none" style={inputStyle} />
        {expires && <button type="button" onClick={() => setExpires('')} className="underline">sin fecha</button>}
        <span>(despues de ese dia no puede entrar)</span>
      </label>
      <div>
        <div className="text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
          Excepciones a su rol{role ? ` (${role.name})` : ''}: solo para esta persona
        </div>
        <div className="grid gap-x-6 gap-y-1.5 md:grid-cols-2">
          {modules.map(m => {
            const own = overrides[m]
            const base = role?.levels[m] ?? 'ninguno'
            const baseLabel = LEVELS.find(l => l.id === base)?.label ?? base
            return (
              <div key={m} className="flex items-center justify-between gap-2">
                <span className="text-xs" style={{ color: own ? 'var(--warning)' : 'var(--text-primary)' }}>{MODULE_LABELS[m] ?? m}</span>
                <select value={own ?? ''} onChange={e => {
                  const next = { ...overrides }
                  if (e.target.value) next[m] = e.target.value as ModuleLevel
                  else delete next[m]
                  setOverrides(next)
                }} className="px-2 py-1 rounded-lg text-[11px] outline-none cursor-pointer" style={inputStyle}>
                  <option value="">Como su rol ({baseLabel})</option>
                  {LEVELS.map(l => <option key={l.id} value={l.id}>{l.label}</option>)}
                </select>
              </div>
            )
          })}
        </div>
      </div>
      <button type="button" disabled={!dirty || busy} onClick={async () => {
        setBusy(true)
        try {
          await setWebUserRole(u.username, u.role, { expires_at: fromDateInput(expires), overrides })
          onSaved()
        } catch (e) { alert(errorMessage(e)) } finally { setBusy(false) }
      }} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium disabled:opacity-40" style={{ backgroundColor: 'var(--accent)', color: 'white' }}>
        <Save size={12} /> Guardar
      </button>
    </div>
  )
}

export default function UsersTab() {
  const { user: authUser } = useAuth()
  const [webUsers, setWebUsers] = useState<WebUserRow[]>([])
  const [notifConfig, setNotifConfig] = useState<NotificationConfig | null>(null)
  const [roles, setRoles] = useState<RoleInfo[]>([])
  const [modules, setModules] = useState<string[]>([])
  const [editing, setEditing] = useState<string | null>(null)

  async function loadRoles() {
    const r = await fetchRoles().catch(() => null)
    if (r) { setRoles(r.roles); setModules(r.modules) }
  }

  useEffect(() => {
    fetchWebUsers().then(setWebUsers).catch(() => {})
    fetchNotificationConfig().then(setNotifConfig).catch(() => {})
    fetchRoles().then(r => { setRoles(r.roles); setModules(r.modules) }).catch(() => {})
  }, [])

  const roleName = (id: string) => id === 'pendiente' ? 'Pendiente' : roles.find(r => r.id === id)?.name ?? id
  const [now] = useState(() => Date.now() / 1000)

  return (
    <>
      <RolesSection onChanged={loadRoles} />

      {/* Usuarios y Telegram (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Users size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Usuarios
          </h2>
          <span className="text-xs px-2 py-0.5 rounded-full" style={{ backgroundColor: 'var(--accent)' + '20', color: 'var(--accent)' }}>
            {webUsers.length} web · {notifConfig?.telegram_chats.length ?? 0} telegram
          </span>
        </div>

        <div className="space-y-2">
          {webUsers.map((u) => {
            const color = roleColor(u.role)
            const expired = u.expires_at !== null && u.expires_at <= now
            const linkedChat = notifConfig?.telegram_chats.find(c => c.linked_web_user === u.username)
            return (
              <div key={u.username} className="rounded-xl p-4 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                {/* User header */}
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div className="w-9 h-9 rounded-full flex items-center justify-center text-sm font-bold" style={{ backgroundColor: color + '20', color }}>
                      {u.username[0].toUpperCase()}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{u.username}</span>
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: color + '20', color }}>
                          {roleName(u.role)}
                        </span>
                        {u.expires_at !== null && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: (expired ? 'var(--danger)' : 'var(--warning)') + '20', color: expired ? 'var(--danger)' : 'var(--warning)' }}>
                            {expired ? 'Vencida' : `Vence ${new Date(u.expires_at * 1000).toLocaleDateString()}`}
                          </span>
                        )}
                        {Object.keys(u.overrides).length > 0 && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}
                            title={Object.entries(u.overrides).map(([m, l]) => `${MODULE_LABELS[m] ?? m}: ${l}`).join(', ')}>
                            {Object.keys(u.overrides).length} {Object.keys(u.overrides).length === 1 ? 'excepcion' : 'excepciones'}
                          </span>
                        )}
                        {u.totp_enabled && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium flex items-center gap-1" style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)' }}>
                            2FA
                            {u.username !== authUser?.username && (
                              <button onClick={async () => {
                                if (!confirm(`Quitar el doble factor de ${u.username}? Usalo solo si perdio el telefono y los codigos de recuperacion.`)) return
                                try { await resetUserTwoFactor(u.username); setWebUsers(await fetchWebUsers()) } catch (e) { alert(errorMessage(e)) }
                              }} title="Quitar doble factor" className="hover:opacity-70"><X size={10} /></button>
                            )}
                          </span>
                        )}
                      </div>
                      {linkedChat && (
                        <div className="flex items-center gap-1 mt-0.5">
                          <MessageCircle size={10} style={{ color: 'var(--success)' }} />
                          <span className="text-[10px]" style={{ color: 'var(--success)' }}>
                            {linkedChat.name}{linkedChat.username && ` @${linkedChat.username}`}
                            {linkedChat.daily_enabled && ` · ${String(linkedChat.daily_hour).padStart(2, '0')}:${String(linkedChat.daily_minute).padStart(2, '0')}`}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>
                  {u.role !== 'admin' && u.username !== authUser?.username && (
                    <button onClick={async () => {
                      if (!confirm(`Eliminar usuario ${u.username}?`)) return
                      try { await deleteWebUser(u.username); setWebUsers(await fetchWebUsers()) } catch {}
                    }} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--danger)' }} title="Eliminar">
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>

                {/* Role + Permissions + Telegram link */}
                {u.username !== authUser?.username && (
                  <div className="flex items-center gap-3 flex-wrap">
                    <select value={u.role} onChange={async (e) => {
                      const role = e.target.value
                      if (role === 'admin' && !confirm(`¿Dar rol de administrador a ${u.username}? Tendrá control total del NAS (usuarios, respaldos, terminal, configuración).`)) return
                      // El admin no vence; un rol con vencimiento sugerido (auditor) lo propone si no tenia fecha
                      const days = roles.find(r => r.id === role)?.expiry_days
                      const expires_at = role === 'admin' ? null : u.expires_at ?? (days ? Math.floor(now) + days * 86400 : null)
                      try { await setWebUserRole(u.username, role, { expires_at, overrides: u.overrides }); setWebUsers(await fetchWebUsers()); loadRoles() }
                      catch (err) { alert(errorMessage(err)) }
                    }} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer" style={inputStyle}>
                      <option value="pendiente">Pendiente</option>
                      {roles.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
                      {!roles.some(r => r.id === u.role) && u.role !== 'pendiente' && <option value={u.role}>{u.role}</option>}
                    </select>

                    {u.role !== 'admin' && u.role !== 'pendiente' && (
                      <button type="button" onClick={() => setEditing(editing === u.username ? null : u.username)}
                        className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs"
                        style={{ color: editing === u.username ? 'var(--accent)' : 'var(--text-secondary)', border: '1px solid var(--input-border)' }}>
                        <SlidersHorizontal size={12} /> Vencimiento y excepciones
                      </button>
                    )}

                    {/* Telegram link selector */}
                    {notifConfig && notifConfig.telegram_chats.length > 0 && (
                      <div className="flex items-center gap-1">
                        <Link2 size={12} style={{ color: 'var(--text-secondary)' }} />
                        <select value={linkedChat?.chat_id ?? ''} onChange={async (e) => {
                          try {
                            if (e.target.value) await adminLinkChat(parseInt(e.target.value), u.username)
                            setNotifConfig(await fetchNotificationConfig())
                          } catch {}
                        }} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                          <option value="">Telegram: sin vincular</option>
                          {notifConfig.telegram_chats.map(c => (
                            <option key={c.chat_id} value={c.chat_id}>{c.name}{c.username ? ` @${c.username}` : ''}</option>
                          ))}
                        </select>
                      </div>
                    )}
                  </div>
                )}
                {editing === u.username && u.role !== 'admin' && u.role !== 'pendiente' && (
                  <AccessEditor key={`${u.username}-${u.role}-${u.expires_at}-${JSON.stringify(u.overrides)}`} u={u}
                    role={roles.find(r => r.id === u.role)} modules={modules}
                    onSaved={async () => { setWebUsers(await fetchWebUsers()); setEditing(null) }} />
                )}
              </div>
            )
          })}

          {/* Telegram chats sin vincular */}
          {notifConfig?.telegram_chats.filter(c => !c.linked_web_user).map(c => {
            const color = roleColor(c.role)
            return (
              <div key={c.chat_id} className="rounded-xl p-4 space-y-2" style={{ backgroundColor: 'var(--card-bg)', border: '1px dashed var(--border)' }}>
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div className="w-9 h-9 rounded-full flex items-center justify-center" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <MessageCircle size={16} style={{ color: 'var(--text-secondary)' }} />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{c.name}</span>
                        {c.username && <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>@{c.username}</span>}
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: color + '20', color }}>
                          {roleName(c.role)}
                        </span>
                        <span className="text-[10px] px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>
                          Solo Telegram
                        </span>
                      </div>
                    </div>
                  </div>
                  <button onClick={async () => {
                    try { await deleteTelegramChat(c.chat_id); setNotifConfig(await fetchNotificationConfig()) } catch {}
                  }} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--danger)' }}>
                    <Trash2 size={14} />
                  </button>
                </div>
                {c.role !== 'admin' && (
                  <div className="flex items-center gap-2 flex-wrap">
                    {c.role === 'pendiente' && (
                      <button onClick={async () => {
                        try { await setChatRole(c.chat_id, 'observador'); setNotifConfig(await fetchNotificationConfig()) } catch {}
                      }} className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium"
                        style={{ color: 'var(--success)', border: '1px solid var(--success)' }}>
                        <UserCheck size={12} /> Aprobar
                      </button>
                    )}
                    <select value={c.role} onChange={async (e) => {
                      try { await setChatRole(c.chat_id, e.target.value); setNotifConfig(await fetchNotificationConfig()) } catch (err) { alert(errorMessage(err)) }
                    }} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer" style={inputStyle}>
                      <option value="pendiente">Pendiente</option>
                      {roles.filter(r => !r.locked).map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
                    </select>
                    <select value="" onChange={async (e) => {
                      if (!e.target.value) return
                      try { await adminLinkChat(c.chat_id, e.target.value); setNotifConfig(await fetchNotificationConfig()) } catch {}
                    }} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                      <option value="">Vincular a usuario web...</option>
                      {webUsers.filter(u => !notifConfig?.telegram_chats.some(tc => tc.linked_web_user === u.username)).map(u => (
                        <option key={u.username} value={u.username}>{u.username}</option>
                      ))}
                    </select>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </section>
    </>
  )
}
