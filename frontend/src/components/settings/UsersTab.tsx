import { useEffect, useState } from 'react'
import { Users, MessageCircle, X, Trash2, Link2, UserCheck } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { fetchWebUsers, fetchNotificationConfig, resetUserTwoFactor, deleteWebUser, setWebUserRole, adminLinkChat, deleteTelegramChat, setChatRole } from '../../api'
import type { NotificationConfig, UserRole, UserPermissions } from '../../types'
import { errorMessage } from '../../lib/errors'

export default function UsersTab() {
  const { user: authUser } = useAuth()
  const [webUsers, setWebUsers] = useState<{ username: string; role: UserRole; permissions: UserPermissions; totp_enabled?: boolean }[]>([])
  const [notifConfig, setNotifConfig] = useState<NotificationConfig | null>(null)

  useEffect(() => {
    fetchWebUsers().then(setWebUsers).catch(() => {})
    fetchNotificationConfig().then(setNotifConfig).catch(() => {})
  }, [])

  return (
    <>
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
            const roleColor = u.role === 'admin' ? 'var(--accent)' : u.role === 'operador' ? 'var(--success)' : u.role === 'observador' ? 'var(--text-secondary)' : 'var(--warning)'
            const linkedChat = notifConfig?.telegram_chats.find(c => c.linked_web_user === u.username)
            return (
              <div key={u.username} className="rounded-xl p-4 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                {/* User header */}
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div className="w-9 h-9 rounded-full flex items-center justify-center text-sm font-bold" style={{ backgroundColor: roleColor + '20', color: roleColor }}>
                      {u.username[0].toUpperCase()}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{u.username}</span>
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: roleColor + '20', color: roleColor }}>
                          {u.role === 'admin' ? 'Admin' : u.role === 'operador' ? 'Operador' : u.role === 'observador' ? 'Observador' : 'Pendiente'}
                        </span>
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
                      try { await setWebUserRole(u.username, role, u.permissions); setWebUsers(await fetchWebUsers()) }
                      catch (err) { alert(err instanceof Error ? err.message : String(err)) }
                    }} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                      <option value="pendiente">Pendiente</option>
                      <option value="observador">Observador</option>
                      <option value="operador">Operador</option>
                      <option value="admin">Admin</option>
                    </select>

                    {u.role === 'operador' && (
                      <div className="flex items-center gap-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
                        {(['terminal', 'impresion', 'archivos_escritura'] as const).map((perm) => (
                          <label key={perm} className="flex items-center gap-1 cursor-pointer">
                            <input type="checkbox" checked={u.permissions[perm]} onChange={async (e) => {
                              try { await setWebUserRole(u.username, u.role, { ...u.permissions, [perm]: e.target.checked }); setWebUsers(await fetchWebUsers()) } catch {}
                            }} />
                            {perm === 'terminal' ? 'Terminal' : perm === 'impresion' ? 'Impresion' : 'Archivos'}
                          </label>
                        ))}
                      </div>
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
              </div>
            )
          })}

          {/* Telegram chats sin vincular */}
          {notifConfig?.telegram_chats.filter(c => !c.linked_web_user).map(c => {
            const roleColor = c.role === 'admin' ? 'var(--accent)' : c.role === 'operador' ? 'var(--success)' : c.role === 'observador' ? 'var(--text-secondary)' : 'var(--warning)'
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
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: roleColor + '20', color: roleColor }}>
                          {c.role === 'admin' ? 'Admin' : c.role === 'operador' ? 'Operador' : c.role === 'observador' ? 'Observador' : 'Pendiente'}
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
                      try { await setChatRole(c.chat_id, e.target.value); setNotifConfig(await fetchNotificationConfig()) } catch {}
                    }} className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                      <option value="pendiente">Pendiente</option>
                      <option value="observador">Observador</option>
                      <option value="operador">Operador</option>
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
