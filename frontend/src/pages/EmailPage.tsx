import { useEffect, useState } from 'react'
import {
  Mail, Settings, Loader2, Trash2, RefreshCw, Brain, ClipboardList,
  Plus, X, Filter, AlertTriangle, Bell, BellOff, EyeOff, Key, Tag,
} from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import {
  configureEmailAccount, deleteEmailAccount, fetchInbox, checkEmailNow,
  classifyEmail, emailToTask, setGroqKey, fetchEmailFilters, addEmailFilter,
  deleteEmailFilter,
  type EmailMessage, type EmailFilter,
} from '../api'

const actionColors: Record<string, { bg: string; color: string; label: string; icon: typeof Bell }> = {
  prioritario: { bg: 'var(--danger)', color: '#fff', label: 'Prioritario', icon: AlertTriangle },
  normal: { bg: 'var(--accent)', color: '#fff', label: 'Normal', icon: Bell },
  silencioso: { bg: 'var(--text-secondary)', color: '#fff', label: 'Silencioso', icon: BellOff },
  ignorar: { bg: 'var(--border)', color: 'var(--text-secondary)', label: 'Ignorar', icon: EyeOff },
}

export default function EmailPage() {
  const { isAdmin } = useAuth()
  const [tab, setTab] = useState<'inbox' | 'config'>('inbox')

  // Config state
  const [imapHost, setImapHost] = useState('')
  const [imapPort, setImapPort] = useState(993)
  const [emailAddr, setEmailAddr] = useState('')
  const [emailPw, setEmailPw] = useState('')
  const [configuring, setConfiguring] = useState(false)
  const [configMsg, setConfigMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [hasAccount, setHasAccount] = useState(false)

  // Inbox
  const [emails, setEmails] = useState<EmailMessage[]>([])
  const [loadingInbox, setLoadingInbox] = useState(false)
  const [checking, setChecking] = useState(false)
  const [expandedUid, setExpandedUid] = useState<number | null>(null)

  // Filters
  const [filters, setFilters] = useState<EmailFilter[]>([])
  const [showAddFilter, setShowAddFilter] = useState(false)
  const [filterPattern, setFilterPattern] = useState('')
  const [filterLabel, setFilterLabel] = useState('')
  const [filterAction, setFilterAction] = useState<string>('normal')
  const [filterTag, setFilterTag] = useState('')

  // Groq key (admin)
  const [groqInput, setGroqInput] = useState('')
  const [groqMsg, setGroqMsg] = useState<string | null>(null)

  // Load inbox on mount
  useEffect(() => {
    loadInbox()
    loadFilters()
  }, [])

  async function loadInbox() {
    setLoadingInbox(true)
    try {
      const data = await fetchInbox()
      setEmails(data)
      setHasAccount(true)
    } catch {
      setHasAccount(false)
      setEmails([])
    } finally {
      setLoadingInbox(false)
    }
  }

  async function loadFilters() {
    try {
      const data = await fetchEmailFilters()
      setFilters(data)
    } catch {}
  }

  async function handleConfigure() {
    if (!imapHost || !emailAddr || !emailPw) return
    setConfiguring(true)
    setConfigMsg(null)
    try {
      const msg = await configureEmailAccount({ imap_host: imapHost, imap_port: imapPort, email: emailAddr, password: emailPw })
      setConfigMsg({ ok: true, text: msg })
      setHasAccount(true)
      setEmailPw('')
      await loadInbox()
    } catch (e: any) {
      setConfigMsg({ ok: false, text: e.message })
    } finally {
      setConfiguring(false)
    }
  }

  async function handleDeleteAccount() {
    if (!confirm('Eliminar tu cuenta de correo configurada?')) return
    try {
      await deleteEmailAccount()
      setHasAccount(false)
      setEmails([])
      setImapHost('')
      setEmailAddr('')
      setConfigMsg(null)
    } catch {}
  }

  async function handleCheck() {
    setChecking(true)
    try {
      await checkEmailNow()
      await loadInbox()
    } catch {} finally {
      setChecking(false)
    }
  }

  async function handleClassify(uid: number) {
    try {
      const updated = await classifyEmail(uid)
      setEmails(prev => prev.map(e => e.uid === uid ? updated : e))
    } catch {}
  }

  async function handleToTask(uid: number) {
    try {
      await emailToTask(uid)
      setEmails(prev => prev.map(e => e.uid === uid ? { ...e, task_created: true } : e))
    } catch (e: any) {
      alert(e.message)
    }
  }

  async function handleAddFilter() {
    if (!filterPattern || !filterLabel) return
    try {
      await addEmailFilter({ pattern: filterPattern, action: filterAction, label: filterLabel, auto_tag: filterTag || undefined })
      setFilterPattern('')
      setFilterLabel('')
      setFilterAction('normal')
      setFilterTag('')
      setShowAddFilter(false)
      await loadFilters()
    } catch (e: any) {
      alert(e.message)
    }
  }

  async function handleDeleteFilter(pattern: string) {
    try {
      await deleteEmailFilter(pattern)
      setFilters(prev => prev.filter(f => f.pattern !== pattern))
    } catch {}
  }

  return (
    <div className="space-y-6 max-w-4xl">
      {/* Tabs */}
      <div className="flex items-center gap-1 p-1 rounded-lg w-fit" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
        <button
          onClick={() => setTab('inbox')}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all"
          style={{
            backgroundColor: tab === 'inbox' ? 'var(--card-bg)' : 'transparent',
            color: tab === 'inbox' ? 'var(--text-primary)' : 'var(--text-secondary)',
          }}
        >
          <Mail size={16} />
          Bandeja
        </button>
        <button
          onClick={() => setTab('config')}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all"
          style={{
            backgroundColor: tab === 'config' ? 'var(--card-bg)' : 'transparent',
            color: tab === 'config' ? 'var(--text-primary)' : 'var(--text-secondary)',
          }}
        >
          <Settings size={16} />
          Configuracion
        </button>
      </div>

      {/* ========= INBOX TAB ========= */}
      {tab === 'inbox' && (
        <>
          {!hasAccount && !loadingInbox ? (
            <div className="rounded-xl p-8 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
              <Mail size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
              <p className="text-sm mb-2" style={{ color: 'var(--text-primary)' }}>No tienes cuenta de correo configurada</p>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                Ve a la pestana <strong>Configuracion</strong> para agregar tu cuenta IMAP
              </p>
            </div>
          ) : (
            <>
              {/* Toolbar */}
              <div className="flex items-center justify-between">
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                  {emails.length} correo{emails.length !== 1 ? 's' : ''} no leido{emails.length !== 1 ? 's' : ''}
                </span>
                <button
                  onClick={handleCheck}
                  disabled={checking}
                  className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-90"
                  style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                >
                  {checking ? <Loader2 size={16} className="animate-spin" /> : <RefreshCw size={16} />}
                  {checking ? 'Revisando...' : 'Revisar ahora'}
                </button>
              </div>

              {/* Email list */}
              {loadingInbox ? (
                <div className="flex items-center justify-center py-16">
                  <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
                </div>
              ) : emails.length === 0 ? (
                <div className="rounded-xl p-8 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                  <Mail size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
                  <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>No hay correos sin leer</p>
                </div>
              ) : (
                <div className="space-y-2">
                  {emails.map(email => {
                    const expanded = expandedUid === email.uid
                    const ac = email.filter_action ? actionColors[email.filter_action] : null
                    return (
                      <div
                        key={email.uid}
                        className="rounded-xl overflow-hidden transition-all"
                        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
                      >
                        {/* Header row */}
                        <div
                          className="flex items-center gap-3 px-5 py-3 cursor-pointer hover:opacity-90 transition-all"
                          onClick={() => setExpandedUid(expanded ? null : email.uid)}
                        >
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2 mb-1">
                              <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                                {email.from.split('<')[0].trim() || email.from}
                              </span>
                              {email.filter_label && ac && (
                                <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: ac.bg + '25', color: ac.bg }}>
                                  {email.filter_label}
                                </span>
                              )}
                              {email.ai_classification && (
                                <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--accent)' + '25', color: 'var(--accent)' }}>
                                  {email.ai_classification}
                                </span>
                              )}
                              {email.task_created && (
                                <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--success)' + '25', color: 'var(--success)' }}>
                                  Tarea creada
                                </span>
                              )}
                            </div>
                            <p className="text-sm truncate" style={{ color: 'var(--text-secondary)' }}>{email.subject}</p>
                          </div>
                          <span className="text-xs shrink-0" style={{ color: 'var(--text-secondary)' }}>
                            {email.date}
                          </span>
                        </div>

                        {/* Expanded content */}
                        {expanded && (
                          <div className="px-5 pb-4 space-y-3" style={{ borderTop: '1px solid var(--border)' }}>
                            <div className="pt-3">
                              <p className="text-xs font-mono mb-1" style={{ color: 'var(--text-secondary)' }}>{email.from}</p>
                              <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>
                                {email.body_preview}
                              </p>
                            </div>

                            {email.ai_summary && (
                              <div className="p-3 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                                <div className="flex items-center gap-1 mb-1">
                                  <Brain size={12} style={{ color: 'var(--accent)' }} />
                                  <span className="text-[10px] font-medium" style={{ color: 'var(--accent)' }}>Resumen IA</span>
                                </div>
                                <p className="text-xs" style={{ color: 'var(--text-primary)' }}>{email.ai_summary}</p>
                                {email.ai_action && (
                                  <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>Accion sugerida: {email.ai_action}</p>
                                )}
                              </div>
                            )}

                            <div className="flex items-center gap-2 flex-wrap">
                              {!email.processed && (
                                <button
                                  onClick={() => handleClassify(email.uid)}
                                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
                                  style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                                >
                                  <Brain size={14} />
                                  Clasificar con IA
                                </button>
                              )}
                              {!email.task_created && (
                                <button
                                  onClick={() => handleToTask(email.uid)}
                                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
                                  style={{ color: 'var(--success)', border: '1px solid var(--success)' }}
                                >
                                  <ClipboardList size={14} />
                                  Crear tarea
                                </button>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                    )
                  })}
                </div>
              )}
            </>
          )}
        </>
      )}

      {/* ========= CONFIG TAB ========= */}
      {tab === 'config' && (
        <div className="space-y-6">
          {/* IMAP Account */}
          <section>
            <div className="flex items-center gap-3 mb-4">
              <Mail size={22} style={{ color: 'var(--accent)' }} />
              <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Cuenta IMAP</h2>
            </div>
            <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Servidor IMAP</label>
                  <input value={imapHost} onChange={e => setImapHost(e.target.value)}
                    placeholder="imap.gmail.com" className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Puerto</label>
                  <input type="number" value={imapPort} onChange={e => setImapPort(parseInt(e.target.value) || 993)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Email</label>
                  <input type="email" value={emailAddr} onChange={e => setEmailAddr(e.target.value)}
                    placeholder="usuario@gmail.com" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Contrasena / App Password</label>
                  <input type="password" value={emailPw} onChange={e => setEmailPw(e.target.value)}
                    placeholder="••••••••" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                </div>
              </div>
              <div className="flex items-center gap-3">
                <button
                  onClick={handleConfigure}
                  disabled={configuring || !imapHost || !emailAddr || !emailPw}
                  className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-90"
                  style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                >
                  {configuring ? <Loader2 size={16} className="animate-spin" /> : null}
                  {configuring ? 'Conectando...' : hasAccount ? 'Actualizar cuenta' : 'Conectar'}
                </button>
                {hasAccount && (
                  <button
                    onClick={handleDeleteAccount}
                    className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-80"
                    style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
                  >
                    <Trash2 size={16} />
                    Eliminar cuenta
                  </button>
                )}
              </div>
              {configMsg && (
                <p className="text-xs" style={{ color: configMsg.ok ? 'var(--success)' : 'var(--danger)' }}>{configMsg.text}</p>
              )}
              <p className="text-xs" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>
                Para Gmail usa imap.gmail.com:993 con una App Password (no tu contrasena normal).
              </p>
            </div>
          </section>

          {/* Filters */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-3">
                <Filter size={22} style={{ color: 'var(--accent)' }} />
                <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Filtros</h2>
              </div>
              <button
                onClick={() => setShowAddFilter(true)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
              >
                <Plus size={14} />
                Agregar filtro
              </button>
            </div>

            {showAddFilter && (
              <div className="rounded-xl p-5 mb-4 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Patron (en remitente)</label>
                    <input value={filterPattern} onChange={e => setFilterPattern(e.target.value)}
                      placeholder="@universidad.cl" className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Etiqueta</label>
                    <input value={filterLabel} onChange={e => setFilterLabel(e.target.value)}
                      placeholder="Universidad" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Accion</label>
                    <select value={filterAction} onChange={e => setFilterAction(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                      <option value="prioritario">Prioritario (siempre notificar)</option>
                      <option value="normal">Normal (clasificar con IA)</option>
                      <option value="silencioso">Silencioso (sin notificar)</option>
                      <option value="ignorar">Ignorar (descartar)</option>
                    </select>
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Tag para tarea (opcional)</label>
                    <input value={filterTag} onChange={e => setFilterTag(e.target.value)}
                      placeholder="academico" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <button onClick={handleAddFilter} disabled={!filterPattern || !filterLabel}
                    className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                    Agregar
                  </button>
                  <button onClick={() => setShowAddFilter(false)}
                    className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                    Cancelar
                  </button>
                </div>
              </div>
            )}

            <div className="rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
              {filters.length === 0 ? (
                <div className="text-center py-8">
                  <Filter size={32} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
                  <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>No hay filtros configurados</p>
                </div>
              ) : (
                <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
                  {filters.map(f => {
                    const ac = actionColors[f.action] || actionColors.normal
                    const Icon = ac.icon
                    return (
                      <div key={f.pattern} className="flex items-center justify-between px-5 py-3">
                        <div className="flex items-center gap-3">
                          <Icon size={16} style={{ color: ac.bg }} />
                          <div>
                            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{f.label}</span>
                            <span className="text-xs font-mono ml-2" style={{ color: 'var(--text-secondary)' }}>{f.pattern}</span>
                          </div>
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: ac.bg + '25', color: ac.bg }}>
                            {ac.label}
                          </span>
                          {f.auto_tag && (
                            <span className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                              <Tag size={10} />{f.auto_tag}
                            </span>
                          )}
                        </div>
                        <button onClick={() => handleDeleteFilter(f.pattern)}
                          className="p-1.5 rounded-lg transition-all hover:opacity-80" style={{ color: 'var(--danger)' }}>
                          <Trash2 size={14} />
                        </button>
                      </div>
                    )
                  })}
                </div>
              )}
            </div>
          </section>

          {/* Groq API Key (admin only) */}
          {isAdmin && (
            <section>
              <div className="flex items-center gap-3 mb-4">
                <Key size={22} style={{ color: 'var(--accent)' }} />
                <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Clasificacion IA (Groq)</h2>
              </div>
              <div className="rounded-xl p-6 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  La API key de Groq permite clasificar correos automaticamente con IA. Se aplica a todos los usuarios.
                </p>
                <div className="flex items-center gap-2">
                  <input value={groqInput} onChange={e => { setGroqInput(e.target.value); setGroqMsg(null) }}
                    placeholder="gsk_..." className="flex-1 px-3 py-2 rounded-lg text-sm outline-none font-mono"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  <button
                    onClick={async () => {
                      if (!groqInput.trim()) return
                      try {
                        const msg = await setGroqKey(groqInput.trim())
                        setGroqMsg(msg)
                        setGroqInput('')
                      } catch (e: any) { setGroqMsg(e.message) }
                    }}
                    disabled={!groqInput.trim()}
                    className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                    Guardar
                  </button>
                </div>
                {groqMsg && <p className="text-xs" style={{ color: 'var(--success)' }}>{groqMsg}</p>}
              </div>
            </section>
          )}
        </div>
      )}
    </div>
  )
}
