import { useEffect, useState } from 'react'
import { MessageCircle, Bot, Link2, Key, Loader2, Send, Clock } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { fetchNotificationConfig, setBotToken, deleteBotToken, sendTestTelegram, setNotificationSchedule, generateLinkCode } from '../../api'
import type { NotificationConfig } from '../../types'
import { errorMessage } from '../../lib/errors'

export default function NotificationsTab() {
  const { user: authUser } = useAuth()
  const [linkCode, setLinkCode] = useState<string | null>(null)
  const [notifConfig, setNotifConfig] = useState<NotificationConfig | null>(null)
  const [tokenInput, setTokenInput] = useState('')
  const [tokenLoading, setTokenLoading] = useState(false)
  const [tokenError, setTokenError] = useState<string | null>(null)
  const [sendingTest, setSendingTest] = useState(false)
  const [testResult, setTestResult] = useState<string | null>(null)
  const [scheduleHour, setScheduleHour] = useState(8)
  const [scheduleMinute, setScheduleMinute] = useState(0)
  const [dailyEnabled, setDailyEnabled] = useState(false)

  useEffect(() => {
    fetchNotificationConfig().then((c) => {
      setNotifConfig(c)
      setDailyEnabled(c.daily_enabled)
      setScheduleHour(c.daily_hour)
      setScheduleMinute(c.daily_minute)
    }).catch(() => {})
  }, [])

  return (
    <>
      {/* Telegram Notifications (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <MessageCircle size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Notificaciones Telegram
          </h2>
        </div>

        {/* Setup Guide */}
        {!notifConfig?.bot_configured && (
          <div
            className="rounded-xl p-5 mb-4"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <div className="flex items-center gap-2 mb-4">
              <Bot size={18} style={{ color: 'var(--accent)' }} />
              <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                Como configurar el Bot de Telegram
              </p>
            </div>
            <div className="space-y-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
              <div className="flex gap-3">
                <span className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold" style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}>1</span>
                <p>
                  Abre Telegram y busca <strong style={{ color: 'var(--text-primary)' }}>@BotFather</strong>. Enviale el comando
                  <span className="font-mono mx-1 px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>/newbot</span>
                  y sigue las instrucciones para crear tu bot.
                </p>
              </div>
              <div className="flex gap-3">
                <span className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold" style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}>2</span>
                <p>
                  BotFather te dara un <strong style={{ color: 'var(--text-primary)' }}>Token</strong> (algo como <span className="font-mono" style={{ color: 'var(--accent)' }}>123456:ABC-xyz...</span>). Copialo y pegalo abajo.
                </p>
              </div>
              <div className="flex gap-3">
                <span className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold" style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}>3</span>
                <p>
                  Cada persona que quiera recibir notificaciones debe abrir el bot en Telegram y enviar <span className="font-mono px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>/start</span>. Se registraran automaticamente.
                </p>
              </div>
              <div className="flex gap-3">
                <span className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold" style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}>4</span>
                <p>
                  Los usuarios pueden enviar comandos como <strong style={{ color: 'var(--text-primary)' }}>/estado</strong>, <strong style={{ color: 'var(--text-primary)' }}>/discos</strong>, <strong style={{ color: 'var(--text-primary)' }}>/ram</strong> y el bot respondera con info del sistema en tiempo real.
                </p>
              </div>
            </div>
          </div>
        )}

        {/* Link my Telegram */}
        {notifConfig?.bot_configured && notifConfig?.bot_username && (
          <div
            className="rounded-xl p-5 mb-4"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Link2 size={16} style={{ color: 'var(--accent)' }} />
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Vincular mi Telegram</span>
              </div>
              {linkCode ? (
                <div className="flex items-center gap-2">
                  <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Envia al bot:</span>
                  <span className="font-mono text-sm font-bold px-3 py-1 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>
                    /vincular {linkCode}
                  </span>
                  <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Expira en 5 min</span>
                </div>
              ) : (
                <button
                  onClick={async () => {
                    if (!authUser) return
                    try {
                      const code = await generateLinkCode()
                      setLinkCode(code)
                    } catch {}
                  }}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >
                  Generar codigo
                </button>
              )}
            </div>
          </div>
        )}

        {/* Bot Token */}
        <div
          className="rounded-xl p-6 mb-4"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center gap-2 mb-3">
            <Key size={16} style={{ color: 'var(--accent)' }} />
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Bot Token</span>
            {notifConfig?.bot_configured && (
              <span className="text-xs px-2 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--success)' + '25', color: 'var(--success)' }}>
                Conectado
              </span>
            )}
          </div>

          {notifConfig?.bot_configured ? (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Bot: </span>
                  <span className="text-sm font-mono font-medium" style={{ color: 'var(--accent)' }}>
                    @{notifConfig.bot_username}
                  </span>
                </div>
                <button
                  onClick={async () => {
                    if (!confirm('Esto desconectara el bot y eliminara todos los chats registrados.')) return
                    try {
                      await deleteBotToken()
                      const cfg = await fetchNotificationConfig()
                      setNotifConfig(cfg)
                    } catch {}
                  }}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                  style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
                >
                  Desconectar
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="flex gap-2">
                <input
                  type="text"
                  value={tokenInput}
                  onChange={(e) => { setTokenInput(e.target.value); setTokenError(null) }}
                  placeholder="123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
                  className="flex-1 px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
                <button
                  onClick={async () => {
                    if (!tokenInput.trim()) return
                    setTokenLoading(true)
                    setTokenError(null)
                    try {
                      const cfg = await setBotToken(tokenInput.trim())
                      setNotifConfig(cfg)
                      setTokenInput('')
                    } catch (e) {
                      setTokenError(errorMessage(e))
                    } finally {
                      setTokenLoading(false)
                    }
                  }}
                  disabled={tokenLoading || !tokenInput.trim()}
                  className="px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >
                  {tokenLoading ? <Loader2 size={16} className="animate-spin" /> : 'Conectar'}
                </button>
              </div>
              {tokenError && (
                <div className="text-xs rounded-lg p-3" style={{ backgroundColor: 'var(--danger)' + '15', color: 'var(--danger)' }}>
                  {tokenError}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Test + Commands */}
        {notifConfig?.bot_configured && (
          <div className="rounded-xl p-6 space-y-4 mb-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {notifConfig.telegram_chats.length} chats registrados
              </span>
              <button onClick={async () => {
                setSendingTest(true); setTestResult(null)
                try { setTestResult(await sendTestTelegram()) } catch (e) { setTestResult(errorMessage(e)) } finally { setSendingTest(false) }
              }} disabled={sendingTest || !notifConfig.telegram_chats.length}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {sendingTest ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />}
                Enviar Test
              </button>
            </div>
            {testResult && (
              <div className="text-xs rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>{testResult}</div>
            )}
            <div className="pt-2" style={{ borderTop: '1px solid var(--border)' }}>
              <p className="text-xs font-medium mb-2" style={{ color: 'var(--text-secondary)' }}>Comandos del bot:</p>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
                {['/estado', '/discos', '/ram', '/cpu', '/uptime', '/red', '/musica', '/play', '/correos', '/ayuda'].map((cmd) => (
                  <span key={cmd} className="text-xs font-mono px-2 py-1 rounded text-center" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>{cmd}</span>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Schedule */}
        {notifConfig?.bot_configured && (
          <div
            className="rounded-xl p-6"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <Clock size={18} style={{ color: 'var(--accent)' }} />
                <div>
                  <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                    Mensaje diario automatico
                  </p>
                  <p className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                    Envia un resumen del sistema a todos los chats registrados
                  </p>
                </div>
              </div>
              <button
                onClick={async () => {
                  const newEnabled = !dailyEnabled
                  setDailyEnabled(newEnabled)
                  try {
                    await setNotificationSchedule({ daily_enabled: newEnabled, daily_hour: scheduleHour, daily_minute: scheduleMinute })
                  } catch { setDailyEnabled(!newEnabled) }
                }}
                className="relative w-14 h-7 rounded-full transition-all duration-300 focus:outline-none"
                style={{
                  backgroundColor: dailyEnabled ? 'var(--accent)' : 'var(--bg-tertiary)',
                  border: '1px solid var(--border)',
                }}
              >
                <span
                  className="absolute top-0.5 w-5 h-5 rounded-full transition-all duration-300"
                  style={{
                    backgroundColor: dailyEnabled ? '#ffffff' : 'var(--text-secondary)',
                    left: dailyEnabled ? '30px' : '4px',
                  }}
                />
              </button>
            </div>
            {dailyEnabled && (
              <div className="flex items-center gap-3 mt-4 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hora:</span>
                <input
                  type="number"
                  min={0}
                  max={23}
                  value={scheduleHour}
                  onChange={(e) => setScheduleHour(parseInt(e.target.value) || 0)}
                  className="w-16 px-2 py-1 rounded-lg text-sm text-center outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
                <span style={{ color: 'var(--text-secondary)' }}>:</span>
                <input
                  type="number"
                  min={0}
                  max={59}
                  value={scheduleMinute}
                  onChange={(e) => setScheduleMinute(parseInt(e.target.value) || 0)}
                  className="w-16 px-2 py-1 rounded-lg text-sm text-center outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
                <button
                  onClick={async () => {
                    try {
                      await setNotificationSchedule({ daily_enabled: dailyEnabled, daily_hour: scheduleHour, daily_minute: scheduleMinute })
                    } catch {}
                  }}
                  className="px-3 py-1 rounded-lg text-xs font-medium"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >
                  Guardar
                </button>
              </div>
            )}
          </div>
        )}
      </section>
    </>
  )
}
